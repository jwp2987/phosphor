//! The one place outbound secret redaction is applied for BYOP provider traffic (#1469).
//!
//! When the user turns secret redaction on (Settings > Privacy), every byte of user-derived
//! content that leaves for a configured provider is scrubbed with the same detector the UI uses
//! to mask output (`SECRETS_REGEX`: built-in patterns plus the user's custom regexes), so what
//! the provider receives matches what the user sees masked.
//!
//! ## Choke points (every wire send goes through exactly one of these)
//!
//! | path | call site | function |
//! |---|---|---|
//! | agent chat stream, summarize/compaction, title | `chat_stream::build_chat_request_for_model` | [`redact_chat_request`] |
//! | one-shot (active AI, commit msg, explain, title) | `oneshot::byop_oneshot_completion` / `_streaming_completion` | [`redact_chat_request`] |
//! | embeddings / rerank | `embeddings::HttpEmbeddingProvider::embed` / `HttpRerankProvider::rerank` | [`redact_texts`] / [`redact_str`] |
//! | Exa web search | `tools::web_runtime` | [`redact_json_strings`] |
//!
//! `script/check_outbound_redaction` fails CI if a new `exec_chat`, `exec_chat_stream` or
//! embeddings `.post(` appears outside those files.
//!
//! ## What is redacted
//!
//! System prompt, every text part of every message (user queries, history, attached block
//! output, file/project context, summaries), tool-call arguments (string values), and tool
//! responses (command output, file reads, MCP results). Not redacted: tool schemas, binary
//! parts (images/PDF), opaque reasoning content / thought signatures (provider-signed; the model
//! only ever saw redacted input), and the URL of a web fetch.
//!
//! The switch is a process-wide flag ([`set_enabled`]) kept in sync with the settings by
//! `lib.rs`, because the oneshot and embeddings paths have no `AppContext`. The chat path also
//! honors `RequestParams::should_redact_secrets`.

use std::sync::atomic::{AtomicBool, Ordering};

use genai::chat::{ChatRequest, ContentPart};
use serde_json::Value;

use crate::ai::agent::redaction::redact_secrets;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Mirror the user's secret-redaction setting. Called at startup and on every settings change.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Redacts `text` and returns it (owned).
pub fn redact_str(text: &str) -> String {
    let mut s = text.to_owned();
    redact_secrets(&mut s);
    s
}

pub fn redact_texts(texts: &mut [String]) {
    texts.iter_mut().for_each(redact_secrets);
}

/// Redacts every string value (not object keys) inside `value`.
pub fn redact_json_strings(value: &mut Value) {
    match value {
        Value::String(s) => redact_secrets(s),
        Value::Array(items) => items.iter_mut().for_each(redact_json_strings),
        Value::Object(map) => map.values_mut().for_each(redact_json_strings),
        _ => {}
    }
}

/// Redacts a whole chat request in place. See the module docs for what is covered.
pub fn redact_chat_request(req: &mut ChatRequest) {
    if let Some(system) = req.system.as_mut() {
        redact_secrets(system);
    }
    for message in req.messages.iter_mut() {
        for part in message.content.iter_mut() {
            match part {
                ContentPart::Text(text) => redact_secrets(text),
                ContentPart::ToolCall(call) => redact_json_strings(&mut call.fn_arguments),
                ContentPart::ToolResponse(resp) => redact_secrets(&mut resp.content),
                ContentPart::Binary(_)
                | ContentPart::ThoughtSignature(_)
                | ContentPart::ReasoningContent(_)
                | ContentPart::Custom(_) => {}
            }
        }
    }
}
