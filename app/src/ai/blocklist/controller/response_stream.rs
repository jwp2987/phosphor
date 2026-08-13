use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::ai::api_error::AIApiError;
use anyhow::anyhow;
use chrono::{DateTime, Local, TimeDelta};
use futures::channel::oneshot;
use futures_util::StreamExt;
use uuid::Uuid;
use warp_multi_agent_api::response_event;
use warpui::{Entity, ModelContext};

use crate::{
    ai::agent::{
        api::{self, ConvertToAPITypeError},
        conversation::AIConversationId,
        AIAgentInput, AIIdentifiers, CancellationReason,
    },
    ai::blocklist::BlocklistAIHistoryModel,
    ai::byop_readiness::BlockedByopReadinessError,
    network::NetworkStatus,
    report_error, send_telemetry_from_ctx,
};
use warpui::SingletonEntity;

/// What to do about a failed or truncated MAA response attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryAction {
    /// Re-send the same request immediately.
    RetryNow,
    /// Re-send the same request once connectivity returns.
    RetryWhenOnline,
    /// Resume the conversation with a fresh request after the stream completes.
    Resume,
    /// Surface the error; the conversation ends in error.
    Fail,
}

/// Decides how to recover from a failed response-stream attempt.
///
/// Before any client actions have been received, the request can be re-sent
/// verbatim (immediately, or once connectivity returns). After actions have
/// streamed, re-sending is unsafe, so recovery uses a fresh resume request.
fn recovery_action(
    has_received_client_actions: bool,
    is_recoverable: bool,
    has_retry_budget: bool,
    can_attempt_resume_on_error: bool,
    is_online: bool,
) -> RecoveryAction {
    if !has_received_client_actions && is_recoverable && has_retry_budget {
        if is_online {
            RecoveryAction::RetryNow
        } else {
            RecoveryAction::RetryWhenOnline
        }
    } else if has_received_client_actions && is_recoverable && can_attempt_resume_on_error {
        RecoveryAction::Resume
    } else {
        RecoveryAction::Fail
    }
}

/// Request-routing parameters for the BYOP path. Extracted from LLMId, settings, and
/// conversation, then handed to the spawn closure in one shot (ctx can't cross an
/// await boundary).
pub(super) struct PendingTitleGeneration {
    pub(super) input: crate::ai::agent_providers::chat_stream::TitleGenInput,
    pub(super) user_query: String,
    pub(super) task_id: String,
}

struct ByopDispatch {
    base_url: String,
    api_key: String,
    model_id: String,
    /// Explicitly-specified API protocol type; chat_stream maps this to a genai AdapterKind.
    api_type: crate::settings::AgentProviderApiType,
    /// Provider-level reasoning effort preference. When `Auto`, no effort is passed
    /// to genai and the adapter infers it from the model name suffix itself;
    /// non-Auto is injected after the client capability gate.
    reasoning_effort: crate::settings::ReasoningEffortSetting,
    extra_headers: Vec<(String, String)>,
    /// The conversation's root task id — must use the locally-registered id,
    /// otherwise the downstream `Action::AddMessagesToTask` will get `TaskNotFound`
    /// when it can't find it in task_store.
    root_task_id: String,
    /// The task id this round's model output should be written to. Equal to the
    /// root task for a normal conversation; a subtask for subsequent rounds of a CLI
    /// subagent.
    target_task_id: String,
    /// Whether `CreateTask` needs to be emitted to upgrade the Optimistic root to a
    /// Server task. Only needed on the first round (before the root task has a
    /// source); sending it again triggers `UnexpectedUpgrade`.
    needs_create_task: bool,
    /// Title-generation model parameters. Only filled in on the first round
    /// (needs_create_task) when the active title_model decodes to a valid BYOP id;
    /// otherwise background title generation isn't started.
    title_gen: Option<TitleGenParams>,
    /// The `command_id` bound to an LRC scenario (= the LRC block id string).
    lrc_command_id: Option<String>,
    /// Whether a subagent CreateTask needs to be synthesized in chat_stream to
    /// upgrade an optimistic CLI subtask.
    lrc_should_spawn_subagent: bool,
    /// The selected model's context window (tokens). 0/None ⇒ neither the user nor
    /// the catalog filled it in, so chat_stream skips the context_window_usage
    /// calculation and the UI stays at a 100% placeholder.
    context_window: Option<u32>,
    /// The selected model's per-request output cap (tokens). `0` means unspecified —
    /// `build_chat_options` skips `with_max_tokens` entirely in that case rather than
    /// sending a literal zero, which every provider would reject. Kept as a plain `u32`
    /// (not `Option`) to match that sentinel, which is the same convention the setting
    /// itself uses (`is_zero_u32` / `skip_serializing_if` on `AgentProviderModel`).
    max_output_tokens: u32,
    /// Attachment caps that already reflect the user's settings (the three-way
    /// image/pdf/audio Override). Computed by `resolve_for_model`. The UI display
    /// and runtime behavior reference the same caps.
    attachment_caps: crate::ai::agent_providers::attachment_caps::AttachmentCaps,
}

/// BYOP config dedicated to title generation (may share the same provider as the main base model, or may not).
pub(crate) struct TitleGenParams {
    pub base_url: String,
    pub api_key: String,
    pub model_id: String,
    pub api_type: crate::settings::AgentProviderApiType,
    pub reasoning_effort: crate::settings::ReasoningEffortSetting,
    /// UI language name substituted for the `{{ language }}` placeholder in
    /// title_system.md (the title-language fallback when the input language is
    /// ambiguous; aligns with the #277 prompt-language injection).
    pub ui_language: &'static str,
    /// Per-prompt override for the title system prompt, resolved from the active
    /// profile. `None` = Auto (built-in / hot-reloaded template).
    pub prompt_override: Option<crate::ai::execution_profiles::PromptSource>,
}

fn byop_dispatch_info(
    params: &api::RequestParams,
    ai_identifiers: &AIIdentifiers,
    ctx: &warpui::AppContext,
) -> Option<ByopDispatch> {
    let (provider, api_key, model_id) =
        crate::ai::agent_providers::lookup_byop(ctx, &params.model)?;
    let extra_headers = provider.extra_headers.clone();
    // Finds the current model entry in provider.models and takes its context_window
    // (tokens). 0 is treated as unfilled, falling through to the None branch ⇒
    // chat_stream skips the usage-rate calculation.
    let context_window = provider
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.context_window)
        .filter(|n| *n > 0);
    // Same lookup for the output cap. Unlike context_window this keeps 0 rather than
    // mapping it to None: 0 IS the "unspecified" sentinel downstream, and
    // `build_chat_options` tests it directly.
    let max_output_tokens = provider
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.max_output_tokens)
        .unwrap_or(0);
    let conversation_id = ai_identifiers.client_conversation_id.as_ref()?;
    let history = BlocklistAIHistoryModel::as_ref(ctx);
    let conversation = history.conversation(conversation_id)?;
    let root_task_id = conversation.get_root_task_id().to_string();
    let target_task_id = params
        .byop_target_task_id
        .clone()
        .unwrap_or_else(|| root_task_id.clone());
    // compute_active_tasks only returns tasks where `task.source().is_some()` —
    // so non-empty ⇒ the root has already been upgraded to the Server state, so
    // don't emit CreateTask again.
    let needs_create_task = conversation.compute_active_tasks().is_empty();

    // Title generation: only triggered on the first round (avoiding re-titling
    // every round). Resolves the active title_model: it may be the base_model
    // itself, or another BYOP model the user selected independently. If either
    // model isn't BYOP-coded (e.g. falling back to a non-BYOP default), skip it —
    // Zap's main path is always BYOP, so when it actually falls back to base, base
    // itself is BYOP.
    let llm_prefs = crate::ai::llms::LLMPreferences::as_ref(ctx);
    let title_gen = if needs_create_task {
        let title_id = llm_prefs.get_active_title_model(ctx, None).id.clone();
        // The profile's per-prompt override for the title system prompt.
        let title_prompt_override = {
            use warpui::SingletonEntity;
            crate::ai::execution_profiles::profiles::AIExecutionProfilesModel::as_ref(ctx)
                .active_profile(None, ctx)
                .data()
                .prompt_overrides
                .title
                .clone()
        };
        crate::ai::agent_providers::lookup_byop(ctx, &title_id).map(
            |(t_provider, t_api_key, t_model_id)| {
                let t_effort =
                    llm_prefs.get_reasoning_effort(None, t_provider.api_type, &t_model_id);
                TitleGenParams {
                    base_url: t_provider.resolved_base_url(),
                    api_key: t_api_key,
                    model_id: t_model_id,
                    api_type: t_provider.api_type,
                    reasoning_effort: t_effort,
                    ui_language: (*crate::settings::language::LanguageSettings::as_ref(ctx)
                        .language)
                        .prompt_language_name(),
                    prompt_override: title_prompt_override.clone(),
                }
            },
        )
    } else {
        None
    };

    let reasoning_effort = llm_prefs.get_reasoning_effort(None, provider.api_type, &model_id);
    let attachment_caps = provider
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| {
            crate::ai::agent_providers::attachment_caps::resolve_for_model(
                &provider.id,
                provider.api_type,
                m,
            )
        })
        .unwrap_or_else(|| {
            log::warn!(
                "[byop] model '{}' not found in provider.models — falling back to caps_for (user overrides ignored)",
                model_id
            );
            crate::ai::agent_providers::attachment_caps::caps_for(provider.api_type, &model_id)
        });
    Some(ByopDispatch {
        base_url: provider.resolved_base_url(),
        api_key,
        model_id,
        api_type: provider.api_type,
        reasoning_effort,
        extra_headers,
        root_task_id,
        target_task_id,
        needs_create_task,
        title_gen,
        lrc_command_id: params.lrc_command_id.clone(),
        lrc_should_spawn_subagent: params.lrc_should_spawn_subagent,
        context_window,
        max_output_tokens,
        attachment_caps,
    })
}

fn pending_title_generation_from_byop(
    params: &api::RequestParams,
    byop: &ByopDispatch,
) -> Option<PendingTitleGeneration> {
    let title_gen = byop.title_gen.as_ref()?;
    let user_query = params.input.iter().find_map(|input| {
        if let AIAgentInput::UserQuery { query, .. } = input {
            Some(query.clone())
        } else {
            None
        }
    })?;

    Some(PendingTitleGeneration {
        input: crate::ai::agent_providers::chat_stream::TitleGenInput {
            base_url: title_gen.base_url.clone(),
            api_key: title_gen.api_key.clone(),
            model_id: title_gen.model_id.clone(),
            api_type: title_gen.api_type,
            reasoning_effort: title_gen.reasoning_effort,
            ui_language: title_gen.ui_language,
            prompt_override: title_gen.prompt_override.clone(),
        },
        user_query,
        task_id: byop.root_task_id.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResponseStreamId(String);

impl ResponseStreamId {
    pub fn new_local() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn for_shared_session(init_event: &response_event::StreamInit) -> Self {
        // Make the stream ID unique per viewing by appending a local UUID
        // This prevents collisions when replaying the same conversation multiple times
        // (either on close-and-reopen or when viewing the same shared session from multiple terminals)
        Self(format!("{}-{}", init_event.request_id, Uuid::new_v4()))
    }

    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new_local()
    }
}

/// Model wrapping an agent API response stream.
///
/// Emits events when the output corresponding to the stream is updated, typically after receiving
/// each response chunk.
///
/// Handles retries internally - retries are only attempted if no ClientActions events have been
/// received yet, ensuring we don't retry after the AI has started executing actions.
pub struct ResponseStream {
    id: ResponseStreamId,
    params: api::RequestParams,
    retry_count: usize,
    start_time: DateTime<Local>,
    time_to_latest_event: TimeDelta,
    cancellation_tx: Option<oneshot::Sender<()>>,
    /// Store the original error for telemetry when retries succeed
    original_error: Option<String>,
    /// Track whether we've received any client actions
    /// If true, we cannot retry on subsequent errors since actions may have been executed
    has_received_client_actions: bool,
    /// AI identifiers for telemetry emission
    ai_identifiers: AIIdentifiers,

    /// Whether this request can attempt to resume the conversation on error.
    /// This is true for all requests except those that are themselves the result of a resume
    /// triggered by a previous error.
    can_attempt_resume_on_error: bool,

    pending_title_generation: Option<PendingTitleGeneration>,

    /// Whether we should attempt to resume the conversation after the stream finishes.
    ///
    /// This is set when we receive a retryable error after client actions have been received
    /// and `can_attempt_resume_on_error` is true.
    should_resume_conversation_after_stream_finished: bool,

    /// Unique, internal id for the current request.
    ///
    /// This ensures that the model never emits events for a request that was already cancelled (or
    /// retried) and is still receiving lagging events.
    ///
    /// Note this is unique compared to `id`; this is unique across retry requests while the response
    /// stream id remains stable.
    current_request_id: Option<Uuid>,
}

impl ResponseStream {
    /// Constructs a `ResponseStream` for a mock in-flight request, without
    /// spawning a real request task. Used by view tests that need to attach a
    /// stream to a conversation so cancellation (`cancel_conversation_progress`)
    /// has something real to cancel (#418).
    #[cfg(test)]
    pub fn new_for_test(id: ResponseStreamId) -> Self {
        let (cancellation_tx, _rx) = oneshot::channel();
        Self {
            id,
            params: api::RequestParams::new_for_test(vec![], vec![]),
            retry_count: 0,
            start_time: Local::now(),
            time_to_latest_event: TimeDelta::seconds(0),
            cancellation_tx: Some(cancellation_tx),
            original_error: None,
            has_received_client_actions: false,
            ai_identifiers: AIIdentifiers::default(),
            can_attempt_resume_on_error: false,
            pending_title_generation: None,
            should_resume_conversation_after_stream_finished: false,
            current_request_id: Some(Uuid::new_v4()),
        }
    }

    pub fn new(
        params: api::RequestParams,
        ai_identifiers: AIIdentifiers,
        can_attempt_resume_on_error: bool,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let (cancellation_tx, cancellation_rx) = oneshot::channel();
        let start_time = Local::now();

        let request_id = Uuid::new_v4();
        let params_clone = params.clone();
        // BYOP path: if the selected base model is an LLMId coded for a
        // user-defined provider, then before spawning, (provider, api_key,
        // model_id, root_task_id) are taken out of ctx and it goes through custom
        // chat completions. Otherwise it goes through warp's own multi-agent
        // endpoint (the original path).
        let byop_dispatch = byop_dispatch_info(&params, &ai_identifiers, ctx);
        let pending_title_generation = byop_dispatch
            .as_ref()
            .and_then(|byop| pending_title_generation_from_byop(&params, byop));
        let _ = ctx.spawn(
            async move {
                if let Some(byop) = byop_dispatch {
                    crate::ai::agent_providers::chat_stream::generate_byop_output(
                        crate::ai::agent_providers::chat_stream::ByopOutputInput {
                            params: params_clone,
                            base_url: byop.base_url,
                            api_key: byop.api_key,
                            model_id: byop.model_id,
                            api_type: byop.api_type,
                            reasoning_effort: byop.reasoning_effort,
                            extra_headers: byop.extra_headers,
                            task_id: byop.root_task_id,
                            target_task_id: byop.target_task_id,
                            needs_create_task: byop.needs_create_task,
                            lrc_command_id: byop.lrc_command_id,
                            lrc_should_spawn_subagent: byop.lrc_should_spawn_subagent,
                            context_window: byop.context_window,
                            max_output_tokens: byop.max_output_tokens,
                            cancellation_rx,
                            attachment_caps: byop.attachment_caps,
                        },
                    )
                    .await
                } else {
                    byop_required_response_stream(cancellation_rx).await
                }
            },
            move |me, stream, ctx| {
                me.handle_response_stream_result(request_id, stream, ctx);
            },
        );
        Self {
            id: ResponseStreamId(Uuid::new_v4().to_string()),
            params: params.clone(),
            start_time,
            time_to_latest_event: TimeDelta::seconds(0),
            cancellation_tx: Some(cancellation_tx),
            retry_count: 0,
            original_error: None,
            has_received_client_actions: false,
            ai_identifiers,
            can_attempt_resume_on_error,
            pending_title_generation,
            should_resume_conversation_after_stream_finished: false,
            current_request_id: Some(request_id),
        }
    }

    pub(super) fn take_pending_title_generation(&mut self) -> Option<PendingTitleGeneration> {
        self.pending_title_generation.take()
    }

    pub fn id(&self) -> &ResponseStreamId {
        &self.id
    }

    pub fn is_lrc_tag_in_request(&self) -> bool {
        self.params.lrc_should_spawn_subagent
    }

    /// Zap BYOP local session compaction: returns whether this stream is running
    /// SummarizeConversation, along with the overflow marker. In the Done branch of
    /// handle_response_stream_finished, the controller uses this to call
    /// commit_summarization and land the summary in conversation.compaction_state.
    pub fn summarization_overflow(&self) -> Option<bool> {
        self.params.input.iter().find_map(|input| match input {
            crate::ai::agent::AIAgentInput::SummarizeConversation { overflow, .. } => {
                Some(*overflow)
            }
            _ => None,
        })
    }

    /// Returns true if we should attempt to resume the conversation after the stream finishes.
    pub fn should_resume_conversation_after_stream_finished(&self) -> bool {
        self.should_resume_conversation_after_stream_finished
    }

    /// Helper function to emit AgentModeError telemetry for error that is retryable (not user visible).
    fn emit_retryable_agent_mode_error_telemetry(
        &self,
        error: String,
        ctx: &mut ModelContext<Self>,
    ) {
        send_telemetry_from_ctx!(
            crate::TelemetryEvent::AgentModeError {
                identifiers: self.ai_identifiers.clone(),
                error,
                is_user_visible: false,
                will_attempt_to_resume: false,
            },
            ctx
        );
    }

    /// Waits for connectivity to return, then re-sends the request. Used for a
    /// pre-action failure encountered while offline: retrying immediately would
    /// just fail again, so the retry is parked until `NetworkStatus` reports
    /// back online.
    fn defer_retry_until_online(&mut self, ctx: &mut ModelContext<Self>) {
        let wait = NetworkStatus::as_ref(ctx).wait_until_online();
        let _ = ctx.spawn(
            async move {
                wait.await;
            },
            move |me, _, ctx| {
                me.retry(ctx);
            },
        );
    }

    fn retry(&mut self, ctx: &mut ModelContext<Self>) {
        self.retry_count += 1;
        self.has_received_client_actions = false; // Reset for the new attempt

        let (cancellation_tx, cancellation_rx) = oneshot::channel();
        if let Some(old_cancellation_tx) = self.cancellation_tx.take() {
            let _ = old_cancellation_tx.send(());
        }
        self.cancellation_tx = Some(cancellation_tx);

        let request_id = Uuid::new_v4();
        self.current_request_id = Some(request_id);
        let params = self.params.clone();
        let byop_dispatch = byop_dispatch_info(&params, &self.ai_identifiers, ctx);
        // Exponential backoff: 0.5s / 1s / 2s. Previously there was no interval at
        // all, so retrying the same payload three times in a row within seconds was
        // pure waste for deterministic failures (like exceeding a request limit),
        // and it also worsened queueing on a single-machine LLM.
        let backoff = std::time::Duration::from_millis(500u64 << (self.retry_count - 1).min(4));
        let _ = ctx.spawn(
            async move {
                warpui::r#async::Timer::after(backoff).await;
                if let Some(byop) = byop_dispatch {
                    crate::ai::agent_providers::chat_stream::generate_byop_output(
                        crate::ai::agent_providers::chat_stream::ByopOutputInput {
                            params,
                            base_url: byop.base_url,
                            api_key: byop.api_key,
                            model_id: byop.model_id,
                            api_type: byop.api_type,
                            reasoning_effort: byop.reasoning_effort,
                            extra_headers: byop.extra_headers,
                            task_id: byop.root_task_id,
                            target_task_id: byop.target_task_id,
                            needs_create_task: byop.needs_create_task,
                            lrc_command_id: byop.lrc_command_id,
                            lrc_should_spawn_subagent: byop.lrc_should_spawn_subagent,
                            context_window: byop.context_window,
                            max_output_tokens: byop.max_output_tokens,
                            cancellation_rx,
                            attachment_caps: byop.attachment_caps,
                        },
                    )
                    .await
                } else {
                    byop_required_response_stream(cancellation_rx).await
                }
            },
            move |me, stream, ctx| {
                me.handle_response_stream_result(request_id, stream, ctx);
            },
        );
    }

    /// Cancels the stream. The conversation_id is preserved in the emitted event for async handling.
    pub(super) fn cancel(
        &mut self,
        reason: CancellationReason,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.current_request_id = None;
        let Some(cancellation_tx) = self.cancellation_tx.take() else {
            return;
        };
        let _ = cancellation_tx.send(());
        ctx.emit(ResponseStreamEvent::AfterStreamFinished {
            cancellation: Some(StreamCancellation {
                reason,
                conversation_id,
            }),
        });
    }

    fn handle_response_stream_result(
        &mut self,
        request_id: Uuid,
        stream_result: Result<api::ResponseStream, ConvertToAPITypeError>,
        ctx: &mut ModelContext<Self>,
    ) {
        match stream_result {
            Ok(stream) => {
                ctx.spawn_stream_local(
                    stream,
                    move |me, event, ctx| {
                        me.handle_response_stream_event(request_id, event, ctx);
                    },
                    move |me, ctx| {
                        me.on_response_stream_complete(request_id, ctx);
                    },
                );
            }
            Err(e) => {
                log::error!("Failed to send request to multi-agent API: {e:?}");
                let api_error = convert_to_api_error(e);
                ctx.emit(ResponseStreamEvent::ReceivedEvent(Consumable::new(Err(
                    Arc::new(api_error),
                ))));
                self.on_response_stream_complete(request_id, ctx);
            }
        }
    }

    fn handle_response_stream_event(
        &mut self,
        request_id: Uuid,
        event: api::Event,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.current_request_id.is_none_or(|id| id != request_id) {
            return;
        }
        self.time_to_latest_event = Local::now().signed_duration_since(self.start_time);

        match &event {
            Ok(response_event) => {
                if let Some(event_type) = &response_event.r#type {
                    match event_type {
                        warp_multi_agent_api::response_event::Type::Init(init_event) => {
                            // Capture server_output_id from StreamInit event
                            self.ai_identifiers.server_output_id =
                                Some(crate::ai::agent::ServerOutputId::new(
                                    init_event.request_id.clone(),
                                ));
                        }
                        warp_multi_agent_api::response_event::Type::ClientActions(_) => {
                            // Mark that we've received client actions
                            self.has_received_client_actions = true;
                        }
                        warp_multi_agent_api::response_event::Type::Finished(finished_event) => {
                            // Emit retry success telemetry on successful completion
                            if matches!(
                                finished_event.reason,
                                Some(warp_multi_agent_api::response_event::stream_finished::Reason::Done(_)) | None
                            ) {
                                // Emit retry success telemetry if this was a successful completion after retries
                                if self.retry_count > 0 {
                                    if let Some(original_error) = &self.original_error {
                                        send_telemetry_from_ctx!(
                                            crate::TelemetryEvent::AgentModeRequestRetrySucceeded {
                                                identifiers: self.ai_identifiers.clone(),
                                                retry_count: self.retry_count,
                                                original_error: original_error.clone(),
                                            },
                                            ctx
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                ctx.emit(ResponseStreamEvent::ReceivedEvent(Consumable::new(event)));
            }
            Err(e) => {
                // Store original error if this is the first error
                if self.retry_count == 0 {
                    self.original_error = Some(format!("{e:?}"));
                }

                // Decide what to do about the failure: retry now, retry once
                // connectivity returns, resume the conversation (if actions have
                // already executed, a verbatim retry would be unsafe), or fail.
                const MAX_RETRIES: usize = 3;
                let network_status = NetworkStatus::as_ref(ctx);
                let is_online = network_status.is_online();
                let is_retryable = e.is_retryable();

                match recovery_action(
                    self.has_received_client_actions,
                    is_retryable,
                    self.retry_count < MAX_RETRIES,
                    self.can_attempt_resume_on_error,
                    is_online,
                ) {
                    RecoveryAction::RetryNow => {
                        log::warn!(
                            "MultiAgent request failed, retrying (attempt {}/{}) - Error: {e:?}",
                            self.retry_count + 1,
                            MAX_RETRIES
                        );
                        // Only emit error telemetry here if we're retrying.
                        // Final errors that aren't being retried are emitted elsewhere.
                        self.emit_retryable_agent_mode_error_telemetry(format!("{e:?}"), ctx);
                        self.retry(ctx);
                        // Don't emit the error event, we're retrying
                        // TODO: emit a separate event if controller needs to know about failures that are being retried
                        return;
                    }
                    RecoveryAction::RetryWhenOnline => {
                        log::warn!(
                            "MultiAgent request failed while offline; retrying (attempt {}/{}) once connectivity returns - Error: {e:?}",
                            self.retry_count + 1,
                            MAX_RETRIES
                        );
                        self.emit_retryable_agent_mode_error_telemetry(format!("{e:?}"), ctx);
                        self.defer_retry_until_online(ctx);
                        return;
                    }
                    RecoveryAction::Resume => {
                        // Recoverable failure after client actions: resume the
                        // conversation once the stream finishes rather than
                        // surface the error.
                        self.should_resume_conversation_after_stream_finished = true;
                    }
                    RecoveryAction::Fail => {}
                }

                log::warn!(
                    "MultiAgent request failed after {} retries: has_received_client_actions={}, is_retryable={}, is_online={is_online}",
                    self.retry_count,
                    self.has_received_client_actions,
                    is_retryable
                );
                report_error!(anyhow!(e.clone()).context(format!(
                    "MultiAgent request failed after {} retries",
                    self.retry_count
                )));

                ctx.emit(ResponseStreamEvent::ReceivedEvent(Consumable::new(event)));
            }
        }
    }

    fn on_response_stream_complete(&mut self, request_id: Uuid, ctx: &mut ModelContext<Self>) {
        if self.current_request_id.is_none_or(|id| id != request_id) {
            return;
        }
        ctx.emit(ResponseStreamEvent::AfterStreamFinished { cancellation: None });
        self.cancellation_tx = None;
    }
}

fn convert_to_api_error(error: ConvertToAPITypeError) -> AIApiError {
    match &error {
        ConvertToAPITypeError::Other(inner)
            if inner.downcast_ref::<BlockedByopReadinessError>().is_some() =>
        {
            let blocked = inner
                .downcast_ref::<BlockedByopReadinessError>()
                .expect("checked blocked readiness error");
            AIApiError::Other(BlockedByopReadinessError::new(blocked.category()).into())
        }
        ConvertToAPITypeError::Ignore
        | ConvertToAPITypeError::Unimplemented(_)
        | ConvertToAPITypeError::Other(_) => AIApiError::Other(anyhow!(error.to_string())),
    }
}

#[derive(Debug)]
pub struct Consumable<T> {
    value: Rc<RefCell<Option<T>>>,
}

impl<T> Consumable<T> {
    fn new(value: T) -> Self {
        Consumable {
            value: Rc::new(RefCell::new(Some(value))),
        }
    }

    pub(super) fn consume(&self) -> Option<T> {
        self.value.borrow_mut().take()
    }
}

impl<T> Clone for Consumable<T> {
    fn clone(&self) -> Self {
        Consumable {
            value: Rc::clone(&self.value),
        }
    }
}

/// Cancellation context preserved for async event handling.
/// Includes conversation_id because truncation can remove exchange mappings before the event is processed.
#[derive(Debug, Clone)]
pub struct StreamCancellation {
    pub reason: CancellationReason,
    pub conversation_id: AIConversationId,
}

#[derive(Debug, Clone)]
pub enum ResponseStreamEvent {
    ReceivedEvent(Consumable<api::Event>),
    AfterStreamFinished {
        /// Some for cancellation (with context), None for natural completion (uses dynamic lookup).
        cancellation: Option<StreamCancellation>,
    },
}

impl Entity for ResponseStream {
    type Event = ResponseStreamEvent;
}

async fn byop_required_response_stream(
    cancellation_rx: oneshot::Receiver<()>,
) -> Result<api::ResponseStream, ConvertToAPITypeError> {
    log::debug!("No BYOP provider selected for Zap agent request");
    let error_stream = futures::stream::once(async {
        Err(Arc::new(AIApiError::Other(anyhow!(
            "Zap requires a configured BYOP provider in Settings"
        ))))
    })
    .take_until(cancellation_rx);
    Ok(Box::pin(error_stream))
}

#[cfg(test)]
#[path = "response_stream_tests.rs"]
mod tests;
