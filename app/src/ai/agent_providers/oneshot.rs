//! BYOP one-shot (non-streaming) completion adapter.
//!
//! Used by the "active AI" sub-flows (prompt suggestions / NLD predict / relevant
//! files / conversation-title generation, etc.): send one short request to get back
//! a piece of text — **no tool calling, no streaming, no persistence to
//! task.messages**.
//!
//! Differences from `chat_stream::generate_byop_output` (the main conversation flow):
//! - This goes through `Client::exec_chat` (non-streaming) and takes
//!   `ChatResponse::first_text()` in one shot.
//! - It doesn't touch `RequestParams` / `ResponseEvent` / `task_store` — pure
//!   string in, string out.
//! - Reasoning is off by default (active AI shouldn't trigger a chain of thought —
//!   wastes tokens and is slow); it's only injected, subject to the capability gate,
//!   when `OneshotOptions.allow_reasoning = true`.
//!
//! Model selection is up to the caller: `resolve_active_ai_oneshot()` decodes
//! `active_ai_model` (falling back to base_model at the profile level) into a BYOP
//! `OneshotConfig`. If decoding fails (BYOP not configured / model not in the BYOP
//! encoding space) it returns `None` and the caller silently no-ops.

use anyhow::Context as _;
use futures::StreamExt;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent};
use warpui::{AppContext, EntityId, SingletonEntity as _};

use super::chat_stream;
use super::wire_log;
use crate::ai::llms::LLMPreferences;
use crate::settings::{AgentProviderApiType, ReasoningEffortSetting};

/// Provider/model info needed for a BYOP one-shot request.
#[derive(Debug, Clone)]
pub struct OneshotConfig {
    pub base_url: String,
    pub api_key: String,
    pub model_id: String,
    pub api_type: AgentProviderApiType,
    pub reasoning_effort: ReasoningEffortSetting,
    /// The provider's configured custom headers (`AgentProvider::extra_headers`), forwarded
    /// verbatim -- some self-hosted gateways reject a request that lacks a custom auth
    /// header, and one-shot calls (active AI / next-command / commit-message / title gen)
    /// used to drop these on the floor even though the main streaming path
    /// (`ByopDispatch::extra_headers`) already carried them. Values may contain secrets:
    /// never logged.
    pub extra_headers: Vec<(String, String)>,
    /// Set when this config was resolved while the keyring read was still in flight (#1368):
    /// `api_key` is then not trustworthy, and the request waits for the read -- see
    /// [`PendingKey`] -- instead of going out with an empty key.
    pub pending_key: Option<PendingKey>,
}

/// The API key of a [`OneshotConfig`] that has not been read from the keyring yet.
#[derive(Debug, Clone)]
pub struct PendingKey {
    pub provider_id: String,
    pub load: super::secrets::LoadSignal,
}

/// What a one-shot does about an API key that is still loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhileKeysLoading {
    /// User-initiated work: resolve now, and have the request await the load.
    Wait,
    /// Fire-and-forget suggestions (next command, prompt suggestions, NLD predict): resolve to
    /// `None`, i.e. skip the request, rather than delay it or send an empty key.
    Skip,
}

impl OneshotConfig {
    /// The key to send: the resolved one, or -- when the keyring read was in flight at
    /// resolution -- the one that read returns. Fails (no request) if the keyring could not be
    /// read.
    async fn effective_api_key(&self) -> anyhow::Result<String> {
        let Some(pending) = &self.pending_key else {
            return Ok(self.api_key.clone());
        };
        match pending.load.keys().await {
            Some(keys) => Ok(keys.get(&pending.provider_id).cloned().unwrap_or_default()),
            None => {
                log::warn!(
                    "[byop] one-shot not sent: the API key could not be read from the keyring"
                );
                Err(anyhow::Error::new(super::KeyringReadFailed))
            }
        }
    }
}

/// Optional parameters for a one-shot call.
#[derive(Debug, Clone, Default)]
pub struct OneshotOptions {
    /// Character truncation cap for the user message (by char, to protect CJK).
    /// `None` = default 8000.
    pub max_chars: Option<usize>,
    /// Temperature (genai `ChatOptions::temperature`); `None` = provider default.
    pub temperature: Option<f32>,
    /// Whether to require JSON output (OpenAI-compatible providers use
    /// response_format). Note: adapters that don't support it ignore this, so the
    /// system prompt must ask for JSON itself.
    pub response_format_json: bool,
    /// Whether reasoning may be triggered. Default `false` (active AI is all
    /// low-latency lightweight calls).
    pub allow_reasoning: bool,
    /// Wire-inspector category for this call. `None` -> `Kind::Oneshot`; title
    /// generation sets `Kind::TitleGen` so it is filterable in the inspector.
    pub wire_kind: Option<super::wire_log::Kind>,
}

const DEFAULT_MAX_CHARS: usize = 8000;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max).collect()
}

fn build_oneshot_request(
    cfg: &OneshotConfig,
    system: &str,
    user: &str,
    opts: &OneshotOptions,
) -> (ChatRequest, ChatOptions) {
    let mut chat_opts = ChatOptions::default()
        .with_capture_content(true)
        .with_capture_usage(true);
    if let Some(t) = opts.temperature {
        chat_opts = chat_opts.with_temperature(t.into());
    }
    if opts.response_format_json {
        chat_opts = chat_opts.with_response_format(genai::chat::ChatResponseFormat::JsonMode);
    }
    if opts.allow_reasoning {
        if let Some(effort) = cfg.reasoning_effort.to_genai() {
            if super::reasoning::model_supports_reasoning(cfg.api_type, &cfg.model_id) {
                chat_opts = chat_opts.with_reasoning_effort(effort);
            }
        }
    }
    // Filtered (#1091): see `usable_extra_headers`.
    let extra_headers = super::usable_extra_headers(&cfg.extra_headers);
    if !extra_headers.is_empty() {
        chat_opts = chat_opts.with_extra_headers(extra_headers);
    }

    let max_chars = opts.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
    let user_truncated = truncate_chars(user, max_chars);

    let chat_req = ChatRequest::from_messages(vec![ChatMessage::user(user_truncated)])
        .with_system(system.to_owned());

    (chat_req, chat_opts)
}

/// Send one BYOP non-streaming chat completion and return the model reply's plain text.
///
/// Whether to swallow errors is up to the caller — this only propagates
/// `anyhow::Error` and does no logging.
pub async fn byop_oneshot_completion(
    cfg: &OneshotConfig,
    system: &str,
    user: &str,
    opts: &OneshotOptions,
) -> anyhow::Result<String> {
    let api_key = cfg.effective_api_key().await?;
    let client = chat_stream::build_client(cfg.api_type, &cfg.base_url, api_key);
    let (chat_req, chat_opts) = build_oneshot_request(cfg, system, user, opts);

    wire_capture_out(cfg, system, &chat_req, opts);

    let resp = client
        .exec_chat(&cfg.model_id, chat_req, Some(&chat_opts))
        .await
        .with_context(|| format!("byop oneshot exec_chat failed (model={})", cfg.model_id))?;

    let text = resp.first_text().unwrap_or("").to_owned();
    wire_capture_in(cfg, &text, opts);
    Ok(text)
}

/// Endpoint label matching the main path's `wire_adapter_label`.
fn wire_adapter(cfg: &OneshotConfig) -> String {
    format!("{:?} @ {}", cfg.api_type, cfg.base_url)
}

/// Outbound capture for a one-shot call. Gated on the inspector being armed only
/// (not a context window): these auxiliary calls — title generation, next-command,
/// etc. — have no usage meter, but the user still wants to see them go out.
fn wire_capture_out(
    cfg: &OneshotConfig,
    system: &str,
    chat_req: &ChatRequest,
    opts: &OneshotOptions,
) {
    if !wire_log::is_enabled() {
        return;
    }
    let kind = opts.wire_kind.unwrap_or(wire_log::Kind::Oneshot);
    let payload = match serde_json::to_string_pretty(&chat_req.messages) {
        Ok(s) => wire_log::Payload::Json(s),
        Err(e) => wire_log::Payload::Flagged(format!("serialize failed: {e}")),
    };
    wire_log::capture_out(
        kind,
        cfg.model_id.clone(),
        wire_adapter(cfg),
        payload,
        Some(wire_log::ContextSnapshot {
            system: (!system.is_empty()).then(|| system.to_owned()),
            ..Default::default()
        }),
    );
}

/// Inbound capture for a one-shot call: the model's reply text. No token usage —
/// one-shots do not report context occupancy.
fn wire_capture_in(cfg: &OneshotConfig, text: &str, opts: &OneshotOptions) {
    if !wire_log::is_enabled() {
        return;
    }
    let kind = opts.wire_kind.unwrap_or(wire_log::Kind::Oneshot);
    wire_log::capture_in(
        kind,
        cfg.model_id.clone(),
        wire_adapter(cfg),
        wire_log::Payload::Json(text.to_owned()),
        None,
    );
}

/// Send one BYOP streaming chat completion, aggregate all text chunks, and return them.
///
/// For OpenAI Responses-compatible proxies that only accept `stream=true`. The caller
/// still gets the full string, so it can keep reusing the one-shot title-cleaning /
/// JSON-parsing logic.
pub async fn byop_oneshot_streaming_completion(
    cfg: &OneshotConfig,
    system: &str,
    user: &str,
    opts: &OneshotOptions,
) -> anyhow::Result<String> {
    let api_key = cfg.effective_api_key().await?;
    let client = chat_stream::build_client(cfg.api_type, &cfg.base_url, api_key);
    let (chat_req, chat_opts) = build_oneshot_request(cfg, system, user, opts);
    wire_capture_out(cfg, system, &chat_req, opts);
    let mut resp = client
        .exec_chat_stream(&cfg.model_id, chat_req, Some(&chat_opts))
        .await
        .with_context(|| {
            format!(
                "byop oneshot exec_chat_stream failed (model={})",
                cfg.model_id
            )
        })?
        .stream;

    let mut text = String::new();
    while let Some(event) = resp.next().await {
        match event.with_context(|| {
            format!(
                "byop oneshot exec_chat_stream event failed (model={})",
                cfg.model_id
            )
        })? {
            ChatStreamEvent::Chunk(chunk) => {
                text.push_str(&chunk.content);
            }
            // `Heartbeat` is new in genai 0.7 — Anthropic emits it to keep a
            // long stream alive. It carries no content, so a oneshot capture
            // ignores it exactly like the other non-chunk events.
            ChatStreamEvent::Start
            | ChatStreamEvent::Heartbeat
            | ChatStreamEvent::ReasoningChunk(_)
            | ChatStreamEvent::ThoughtSignatureChunk(_)
            | ChatStreamEvent::ToolCallChunk(_)
            | ChatStreamEvent::End(_) => {}
        }
    }

    wire_capture_in(cfg, &text, opts);
    Ok(text)
}

/// Builds the config for a resolved BYOP model, applying `policy` if the provider's key is
/// still being read from the keyring. `None` means skip (policy `Skip` while loading).
fn config_for(
    app: &AppContext,
    provider: crate::settings::AgentProvider,
    api_key: String,
    model_id: String,
    reasoning_effort: ReasoningEffortSetting,
    policy: WhileKeysLoading,
) -> Option<OneshotConfig> {
    let pending_key = if super::must_wait_for_keys(app, &provider) {
        if policy == WhileKeysLoading::Skip {
            log::debug!("[byop] skipping a one-shot: API keys are still loading");
            return None;
        }
        super::AgentProviderSecrets::as_ref(app)
            .load_signal()
            .map(|load| PendingKey {
                provider_id: provider.id.clone(),
                load,
            })
    } else {
        None
    };
    Some(OneshotConfig {
        base_url: provider.resolved_base_url(),
        api_key,
        model_id,
        api_type: provider.api_type,
        reasoning_effort,
        extra_headers: provider.extra_headers.clone(),
        pending_key,
    })
}

/// Resolve the active profile's `active_ai_model` (falling back to `base_model`);
/// if it decodes to a valid BYOP encoding, return an `OneshotConfig`, otherwise
/// `None` (the caller silently no-ops). Waits for the keyring read if it is in flight.
pub fn resolve_active_ai_oneshot(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
) -> Option<OneshotConfig> {
    resolve_active_ai_oneshot_with(app, terminal_view_id, WhileKeysLoading::Wait)
}

/// [`resolve_active_ai_oneshot`] with an explicit policy for keys that are still loading.
pub fn resolve_active_ai_oneshot_with(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
    policy: WhileKeysLoading,
) -> Option<OneshotConfig> {
    let llm_prefs = LLMPreferences::as_ref(app);
    let id = llm_prefs
        .get_active_ai_model(app, terminal_view_id)
        .id
        .clone();
    let (provider, api_key, model_id) = super::lookup_byop(app, &id)?;
    let reasoning_effort =
        llm_prefs.get_reasoning_effort(terminal_view_id, provider.api_type, &model_id);
    config_for(app, provider, api_key, model_id, reasoning_effort, policy)
}

/// Resolve the model that explains a requested command on the approval card: the base model
/// the agent conversation in `terminal_view_id` is currently using, falling back to
/// [`resolve_active_ai_oneshot`] when that model is not BYOP-coded. `None` means no usable
/// BYOP provider is configured for either. Waits for the keyring read if it is in flight.
///
/// The base model comes first because it is the model the user picked for the agent that
/// asked to run the command, so it already has whatever capability the user trusts for
/// shell work; the active-AI model is often a small, cheap one.
pub fn resolve_explain_command_oneshot(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
) -> Option<OneshotConfig> {
    let llm_prefs = LLMPreferences::as_ref(app);
    let id = llm_prefs
        .get_active_base_model(app, terminal_view_id)
        .id
        .clone();
    let Some((provider, api_key, model_id)) = super::lookup_byop(app, &id) else {
        return resolve_active_ai_oneshot(app, terminal_view_id);
    };
    let reasoning_effort =
        llm_prefs.get_reasoning_effort(terminal_view_id, provider.api_type, &model_id);
    config_for(
        app,
        provider,
        api_key,
        model_id,
        reasoning_effort,
        WhileKeysLoading::Wait,
    )
}

/// Resolve the active profile's `next_command_model` (falling back to `base_model`);
/// if it decodes to a valid BYOP encoding, return an `OneshotConfig`, otherwise `None`.
/// Next-command suggestions are fire-and-forget and latency-sensitive: while the keyring
/// read is in flight this returns `None` (no suggestion yet) rather than an empty key.
pub fn resolve_next_command_oneshot(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
) -> Option<OneshotConfig> {
    let llm_prefs = LLMPreferences::as_ref(app);
    let id = llm_prefs
        .get_active_next_command_model(app, terminal_view_id)
        .id
        .clone();
    let (provider, api_key, model_id) = super::lookup_byop(app, &id)?;
    let reasoning_effort =
        llm_prefs.get_reasoning_effort(terminal_view_id, provider.api_type, &model_id);
    config_for(
        app,
        provider,
        api_key,
        model_id,
        reasoning_effort,
        WhileKeysLoading::Skip,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_config(extra_headers: Vec<(String, String)>) -> OneshotConfig {
        OneshotConfig {
            base_url: "http://127.0.0.1:1/v1".to_owned(),
            api_key: "test-key".to_owned(),
            model_id: "test-model".to_owned(),
            api_type: AgentProviderApiType::OpenAi,
            reasoning_effort: ReasoningEffortSetting::Auto,
            extra_headers,
            pending_key: None,
        }
    }

    // The bug this closes: `TitleGenInput`/`OneshotConfig` used to have no
    // `extra_headers` field at all, so a gateway that needs a custom auth header
    // (checked in the main streaming path via `ByopDispatch::extra_headers`) rejected
    // one-shot calls -- title generation, active AI, next-command, commit-message
    // generation -- even when the same provider's streaming requests worked fine.
    #[test]
    fn extra_headers_are_forwarded_into_chat_options() {
        let cfg = a_config(vec![(
            "X-Gateway-Auth".to_owned(),
            "secret-token".to_owned(),
        )]);
        let opts = OneshotOptions::default();
        let (_req, chat_opts) = build_oneshot_request(&cfg, "system", "user", &opts);

        let headers = chat_opts
            .extra_headers
            .as_ref()
            .expect("extra_headers must be set on ChatOptions when the config carries any");
        let found = headers
            .iter()
            .any(|(k, v)| k == "X-Gateway-Auth" && v == "secret-token");
        assert!(
            found,
            "the provider's configured header must reach ChatOptions unchanged"
        );
    }

    #[test]
    fn no_extra_headers_means_chat_options_carries_none() {
        let cfg = a_config(Vec::new());
        let opts = OneshotOptions::default();
        let (_req, chat_opts) = build_oneshot_request(&cfg, "system", "user", &opts);

        assert!(
            chat_opts.extra_headers.is_none(),
            "an empty extra_headers must not synthesize an empty header set on the wire"
        );
    }

    /// #1091: the `["", ""]` row the settings page's "+ Add Header" used to persist must
    /// not reach genai, where reqwest turned it into a `builder error` that failed the call.
    #[test]
    fn an_empty_header_row_never_reaches_chat_options() {
        let cfg = a_config(vec![(String::new(), String::new())]);
        let opts = OneshotOptions::default();
        let (_req, chat_opts) = build_oneshot_request(&cfg, "system", "user", &opts);

        assert!(
            chat_opts.extra_headers.is_none(),
            "an empty header row must be dropped, not sent as an empty header name"
        );
    }

    // ── a one-shot never goes out with an empty key while the keyring read is in flight (#1368) ──

    mod key_loading {
        use std::sync::Arc;

        use warpui::App;
        use warpui_extras::secure_storage::{self, Error, SecureStorage};

        use super::*;
        use crate::ai::agent_providers::{AgentProviderSecrets, KeyringReadFailed};
        use crate::settings::AgentProvider;

        struct Keyring {
            fail: bool,
        }

        impl SecureStorage for Keyring {
            fn write_value(&self, _key: &str, _value: &str) -> Result<(), Error> {
                Ok(())
            }
            fn read_value(&self, _key: &str) -> Result<String, Error> {
                // Slow, like a keyring that has to be unlocked.
                std::thread::sleep(std::time::Duration::from_millis(100));
                if self.fail {
                    Err(Error::Unknown(anyhow::anyhow!("keyring is locked")))
                } else {
                    Ok(r#"{"p":"sk-real"}"#.to_owned())
                }
            }
            fn remove_value(&self, _key: &str) -> Result<(), Error> {
                Ok(())
            }
        }

        fn install(app: &mut App, fail: bool) {
            app.update(move |ctx| {
                ctx.add_singleton_model(move |_| -> secure_storage::Model {
                    Box::new(Keyring { fail })
                });
            });
            secure_storage::background::set_factory_for_test(Some(Arc::new(move || {
                Box::new(Keyring { fail }) as secure_storage::Model
            })));
        }

        fn provider(api_type: AgentProviderApiType, base_url: &str) -> AgentProvider {
            AgentProvider {
                id: "p".to_owned(),
                name: "P".to_owned(),
                kind: Default::default(),
                api_type,
                base_url: base_url.to_owned(),
                models: Vec::new(),
                extra_headers: Vec::new(),
                vertex_project: String::new(),
                vertex_location: String::new(),
                disabled: false,
                token_price: None,
            }
        }

        fn config(
            app: &App,
            provider: AgentProvider,
            policy: WhileKeysLoading,
        ) -> Option<OneshotConfig> {
            app.read(|ctx| {
                config_for(
                    ctx,
                    provider,
                    String::new(),
                    "m".to_owned(),
                    ReasoningEffortSetting::Auto,
                    policy,
                )
            })
        }

        const HOSTED: &str = "https://api.example.com/v1";

        #[test]
        fn user_initiated_one_shots_wait_and_then_use_the_real_key() {
            App::test((), |mut app| async move {
                install(&mut app, false);
                app.add_singleton_model(AgentProviderSecrets::new);
                // Explain-command, workflow metadata, relevant files and commit messages
                // resolve with `Wait`: a config comes back at once, carrying the pending key.
                let cfg = config(
                    &app,
                    provider(AgentProviderApiType::OpenAi, HOSTED),
                    WhileKeysLoading::Wait,
                )
                .expect("a user-initiated one-shot is not dropped");
                assert!(cfg.pending_key.is_some());
                assert_eq!(cfg.effective_api_key().await.unwrap(), "sk-real");
                secure_storage::background::set_factory_for_test(None);
            });
        }

        #[test]
        fn fire_and_forget_suggestions_are_skipped_while_keys_load_then_resume() {
            App::test((), |mut app| async move {
                install(&mut app, false);
                let secrets = app.add_singleton_model(AgentProviderSecrets::new);
                // Next-command, prompt suggestions and NLD predict resolve with `Skip`.
                assert!(
                    config(
                        &app,
                        provider(AgentProviderApiType::OpenAi, HOSTED),
                        WhileKeysLoading::Skip
                    )
                    .is_none(),
                    "no request, in particular none with an empty key"
                );
                // Once the read lands the same call resolves, with the real key already in.
                let rx = secrets
                    .update(&mut app, |s, ctx| s.wait_for_load(ctx))
                    .expect("still loading");
                let _ = rx.await;
                let cfg = config(
                    &app,
                    provider(AgentProviderApiType::OpenAi, HOSTED),
                    WhileKeysLoading::Skip,
                );
                assert!(cfg.is_some_and(|c| c.pending_key.is_none()));
                secure_storage::background::set_factory_for_test(None);
            });
        }

        #[test]
        fn a_failed_keyring_read_fails_the_one_shot_without_a_request() {
            App::test((), |mut app| async move {
                install(&mut app, true);
                app.add_singleton_model(AgentProviderSecrets::new);
                let cfg = config(
                    &app,
                    provider(
                        AgentProviderApiType::Anthropic,
                        "https://api.anthropic.com/v1",
                    ),
                    WhileKeysLoading::Wait,
                )
                .unwrap();
                let err = cfg.effective_api_key().await.unwrap_err();
                assert!(err.downcast_ref::<KeyringReadFailed>().is_some());
                // The completion entry points propagate it before building any client.
                let sent =
                    byop_oneshot_completion(&cfg, "s", "u", &OneshotOptions::default()).await;
                assert!(
                    sent.unwrap_err()
                        .downcast_ref::<KeyringReadFailed>()
                        .is_some()
                );
                secure_storage::background::set_factory_for_test(None);
            });
        }

        #[test]
        fn keyless_providers_resolve_immediately_under_either_policy() {
            App::test((), |mut app| async move {
                install(&mut app, false);
                app.add_singleton_model(AgentProviderSecrets::new);
                for policy in [WhileKeysLoading::Wait, WhileKeysLoading::Skip] {
                    let cfg = config(
                        &app,
                        provider(AgentProviderApiType::Ollama, "http://198.51.100.7:11434"),
                        policy,
                    )
                    .expect("Ollama does not wait for the keyring");
                    assert!(cfg.pending_key.is_none());
                    assert!(cfg.api_key.is_empty());
                }
                secure_storage::background::set_factory_for_test(None);
            });
        }
    }
}
