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
    if !cfg.extra_headers.is_empty() {
        chat_opts = chat_opts.with_extra_headers(cfg.extra_headers.clone());
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
    let client = chat_stream::build_client(cfg.api_type, &cfg.base_url, cfg.api_key.clone());
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
    let client = chat_stream::build_client(cfg.api_type, &cfg.base_url, cfg.api_key.clone());
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

/// Resolve the active profile's `active_ai_model` (falling back to `base_model`);
/// if it decodes to a valid BYOP encoding, return an `OneshotConfig`, otherwise
/// `None` (the caller silently no-ops).
pub fn resolve_active_ai_oneshot(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
) -> Option<OneshotConfig> {
    let llm_prefs = LLMPreferences::as_ref(app);
    let id = llm_prefs
        .get_active_ai_model(app, terminal_view_id)
        .id
        .clone();
    let (provider, api_key, model_id) = super::lookup_byop(app, &id)?;
    let reasoning_effort =
        llm_prefs.get_reasoning_effort(terminal_view_id, provider.api_type, &model_id);
    Some(OneshotConfig {
        base_url: provider.resolved_base_url(),
        api_key,
        model_id,
        api_type: provider.api_type,
        reasoning_effort,
        extra_headers: provider.extra_headers.clone(),
    })
}

/// Resolve the active profile's `next_command_model` (falling back to `base_model`);
/// if it decodes to a valid BYOP encoding, return an `OneshotConfig`, otherwise `None`.
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
    Some(OneshotConfig {
        base_url: provider.resolved_base_url(),
        api_key,
        model_id,
        api_type: provider.api_type,
        reasoning_effort,
        extra_headers: provider.extra_headers.clone(),
    })
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
}
