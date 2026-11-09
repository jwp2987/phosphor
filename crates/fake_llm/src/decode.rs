//! A real genai client pointed at a base URL, and a collector for what it decodes: the L0 wire
//! tests drive the same crate (`lib/rust-genai`) and the same options the app's BYOP send path
//! uses (`chat_stream::build_client_uncached`, `ChatOptions` with every capture flag on), against
//! llmposter, the scripted wire server and the chaos proxy.

use std::time::Duration;

use futures::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{ModelIden, ServiceTarget};
use serde_json::Value;

pub const MODEL: &str = "fake-model";

/// Built like `build_client_uncached` for the OpenAI api type: the adapter, endpoint and a static
/// key are pinned, so no model-name guessing is involved.
pub fn client_for(base_url: &str) -> genai::Client {
    let endpoint_url = format!("{}/", base_url.trim_end_matches('/'));
    let resolver = ServiceTargetResolver::from_resolver_fn(
        move |target: ServiceTarget| -> Result<ServiceTarget, genai::resolver::Error> {
            let ServiceTarget { model, .. } = target;
            Ok(ServiceTarget {
                endpoint: Endpoint::from_owned(endpoint_url.clone()),
                auth: AuthData::from_single("fake-key-not-a-secret".to_owned()),
                model: ModelIden::new(AdapterKind::OpenAI, model.model_name),
            })
        },
    );
    genai::Client::builder()
        .with_service_target_resolver(resolver)
        .build()
}

/// The options `chat_stream.rs` sends (capture flags only; tool choice is irrelevant here).
pub fn app_chat_options() -> ChatOptions {
    ChatOptions::default()
        .with_capture_content(true)
        .with_capture_tool_calls(true)
        .with_capture_reasoning_content(true)
        .with_capture_usage(true)
}

/// A tool call as the app would see it at the end of the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedCall {
    pub call_id: String,
    pub name: String,
    /// The arguments exactly as accumulated from the deltas (`Value::String`), or the parsed
    /// object when the whole text was valid JSON.
    pub args: Value,
}

#[derive(Debug, Default)]
pub struct Decoded {
    pub text: String,
    pub reasoning: String,
    /// The last `ToolCallChunk` seen per call index, in first-seen order. The app accumulates
    /// by call id and relies on this being the merged call (genai merges when
    /// `capture_tool_calls` is on).
    pub chunk_calls: Vec<DecodedCall>,
    /// `End.captured_tool_calls`, if an End arrived.
    pub end_calls: Option<Vec<DecodedCall>>,
    pub saw_end: bool,
    /// An error returned by `exec_chat_stream(...).await` itself.
    pub open_error: Option<String>,
    /// The first error yielded by the stream.
    pub stream_error: Option<String>,
    /// The stream hit `deadline` without ending (a stall).
    pub timed_out: bool,
    /// The stream ended (`None`) with no End event and no error: genai reports a connection that
    /// closed cleanly before `[DONE]` as a plain end of stream. The app's only defence is the
    /// "ended without a terminal End event" guard in `chat_stream.rs`.
    pub ended_without_end: bool,
}

impl Decoded {
    /// "Complete": an End arrived and nothing went wrong.
    pub fn is_clean(&self) -> bool {
        self.saw_end && self.open_error.is_none() && self.stream_error.is_none() && !self.timed_out
    }

    /// A broken wire must end as one of: a complete End, an error, a stall the caller can time
    /// out, or a visibly missing End ([`Self::ended_without_end`], which the app turns into an
    /// error). Always true by construction of [`decode`]; kept so a test names its expectation.
    pub fn is_honest(&self) -> bool {
        self.saw_end
            || self.ended_without_end
            || self.open_error.is_some()
            || self.stream_error.is_some()
            || self.timed_out
    }
}

/// Opens a streaming chat against `base_url` and drains it, giving up after `deadline` of total
/// silence per event.
pub async fn decode(base_url: &str, deadline: Duration) -> Decoded {
    let client = client_for(base_url);
    let request = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
    let mut out = Decoded::default();
    let response = match client
        .exec_chat_stream(MODEL, request, Some(&app_chat_options()))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            out.open_error = Some(format!("{e:#}"));
            return out;
        }
    };
    let mut stream = response.stream;
    loop {
        let next = tokio::time::timeout(deadline, stream.next()).await;
        let Ok(item) = next else {
            out.timed_out = true;
            return out;
        };
        match item {
            None => {
                out.ended_without_end = !out.saw_end;
                return out;
            }
            Some(Err(e)) => {
                out.stream_error = Some(format!("{e:#}"));
                return out;
            }
            Some(Ok(event)) => match event {
                ChatStreamEvent::Chunk(c) => out.text.push_str(&c.content),
                ChatStreamEvent::ReasoningChunk(c) => out.reasoning.push_str(&c.content),
                ChatStreamEvent::ToolCallChunk(c) => {
                    let call = DecodedCall {
                        call_id: c.tool_call.call_id,
                        name: c.tool_call.fn_name,
                        args: c.tool_call.fn_arguments,
                    };
                    match out
                        .chunk_calls
                        .iter_mut()
                        .find(|k| k.call_id == call.call_id)
                    {
                        Some(slot) => *slot = call,
                        None => out.chunk_calls.push(call),
                    }
                }
                ChatStreamEvent::End(end) => {
                    out.saw_end = true;
                    out.end_calls = end.captured_content.map(|content| {
                        content
                            .into_tool_calls()
                            .into_iter()
                            .map(|t| DecodedCall {
                                call_id: t.call_id,
                                name: t.fn_name,
                                args: t.fn_arguments,
                            })
                            .collect()
                    });
                }
                _ => {}
            },
        }
    }
}
