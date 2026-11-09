//! Test seam for the BYOP send path: scripted model turns instead of an HTTP request.
//!
//! `chat_stream::generate_byop_output` calls [`intercept`] right before
//! `client.exec_chat_stream(...)`. If a test has registered a script for that request's model id
//! ([`install`]), the stream comes from the script as genai [`ChatStreamEvent`]s and nothing
//! touches the network; otherwise `intercept` returns `None` and the real client runs. The
//! module is compiled only under `cfg(any(test, feature = "test-util"))`: a release build has
//! neither the module nor the hook.
//!
//! It is the in-process half of the agent fuzzer (L1, docs/testing.md "Agent fuzzer and fake
//! model"). Everything above the seam is the real app: request building, the stream loop,
//! tool-call parsing and dispatch, the action model, the queue. Everything below it is replaced.
//!
//! What a script sees and the test reads back is a [`RecordedRequest`]: the tool messages and
//! tool-call ids of each request (invariant I4: exactly one result per emitted call id), the last
//! user text (so queued prompts can be matched, I3) and the tool names offered.
//!
//! The registry is keyed by model id so parallel tests with their own ids never share a script.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::Stream;
use genai::chat::{
    ChatRequest, ChatRole, ChatStreamEvent, ContentPart, MessageContent, StopReason, StreamChunk,
    StreamEnd, ToolCall, ToolChunk,
};
use genai::{ModelIden, adapter::AdapterKind};
use serde_json::Value;

/// The stream type the send loop reads: genai's own `ChatStream` is a concrete struct that cannot
/// be built from events outside the crate, so under the seam the loop reads a boxed stream.
pub(crate) type SeamStream =
    Pin<Box<dyn Stream<Item = genai::Result<ChatStreamEvent>> + Send + 'static>>;

/// One step of a scripted reply stream.
#[derive(Debug, Clone, PartialEq)]
pub enum SeamEvent {
    Text(String),
    Reasoning(String),
    /// One tool call as a single `ToolCallChunk` carrying the whole raw argument text. An empty
    /// `id` is a provider that sent none; the same id twice is a duplicate.
    ToolCall {
        id: String,
        name: String,
        args: String,
    },
    /// Pause before the next event (a slow provider; keeps the stream "in flight").
    Wait(Duration),
    /// Yield this error from the stream (a connection dropped or a status after the open).
    Fail(SeamError),
    /// Never yield again and never end (a stalled provider).
    Stall,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SeamError {
    /// A status the provider returned, as genai delivers it: boxed inside `WebStream`.
    Http {
        status: u16,
        body: String,
    },
    /// A transport failure (reset, EOF) boxed inside `WebStream`.
    Transport(String),
    Other(String),
}

/// A scripted reply.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeamTurn {
    /// Fail the open itself (`exec_chat_stream(...).await` returns this).
    pub open_error: Option<SeamError>,
    pub events: Vec<SeamEvent>,
    /// Finish with an `End` event. False = the stream just stops (a clean close before `[DONE]`).
    pub end: bool,
}

impl SeamTurn {
    pub fn text(text: &str) -> Self {
        Self {
            open_error: None,
            events: vec![SeamEvent::Text(text.to_owned())],
            end: true,
        }
    }
}

/// What a script is called with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    /// 0-based index among the requests for this model id.
    pub index: usize,
    pub model_id: String,
    pub messages: Vec<RecordedMessage>,
    /// Names of the tools the request advertised.
    pub tools_offered: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedMessage {
    pub role: RecordedRole,
    /// All text parts, concatenated.
    pub text: String,
    /// Ids of the tool calls an assistant message made.
    pub tool_call_ids: Vec<String>,
    /// Ids of the tool calls a tool message answers.
    pub tool_response_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedRole {
    System,
    User,
    Assistant,
    Tool,
}

impl RecordedRequest {
    pub fn from_chat_request(index: usize, model_id: &str, req: &ChatRequest) -> Self {
        let messages = req
            .messages
            .iter()
            .map(|m| RecordedMessage {
                role: match m.role {
                    ChatRole::System => RecordedRole::System,
                    ChatRole::User => RecordedRole::User,
                    ChatRole::Assistant => RecordedRole::Assistant,
                    ChatRole::Tool => RecordedRole::Tool,
                },
                text: m.content.texts().concat(),
                tool_call_ids: m
                    .content
                    .tool_calls()
                    .into_iter()
                    .map(|c| c.call_id.clone())
                    .collect(),
                tool_response_ids: m
                    .content
                    .tool_responses()
                    .into_iter()
                    .map(|r| r.call_id.clone())
                    .collect(),
            })
            .collect();
        Self {
            index,
            model_id: model_id.to_owned(),
            messages,
            tools_offered: req
                .tools
                .iter()
                .flatten()
                .map(|t| t.name.as_str().to_owned())
                .collect(),
        }
    }

    /// The text of the last user message, if any.
    pub fn last_user_text(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == RecordedRole::User)
            .map(|m| m.text.as_str())
    }

    /// Every tool-response id in the request, in order.
    pub fn tool_response_ids(&self) -> Vec<&str> {
        self.messages
            .iter()
            .flat_map(|m| m.tool_response_ids.iter().map(String::as_str))
            .collect()
    }
}

type Script = Box<dyn FnMut(&RecordedRequest) -> SeamTurn + Send>;

struct SeamState {
    script: Mutex<Script>,
    requests: Mutex<Vec<RecordedRequest>>,
}

static REGISTRY: LazyLock<Mutex<HashMap<String, Arc<SeamState>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A registered script. Dropping it unregisters the model id.
pub struct SeamHandle {
    model_id: String,
    state: Arc<SeamState>,
}

impl SeamHandle {
    /// Every request the app sent to this model id so far, in order.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        lock(&self.state.requests).clone()
    }

    pub fn request_count(&self) -> usize {
        lock(&self.state.requests).len()
    }
}

impl Drop for SeamHandle {
    fn drop(&mut self) {
        lock(&REGISTRY).remove(&self.model_id);
    }
}

/// Registers `script` for `model_id`: each request to that model calls it once and streams the
/// turn it returns. Panics if the id is already registered (two tests sharing an id would
/// silently steal each other's turns).
pub fn install(
    model_id: &str,
    script: impl FnMut(&RecordedRequest) -> SeamTurn + Send + 'static,
) -> SeamHandle {
    let state = Arc::new(SeamState {
        script: Mutex::new(Box::new(script)),
        requests: Mutex::new(Vec::new()),
    });
    let previous = lock(&REGISTRY).insert(model_id.to_owned(), state.clone());
    assert!(
        previous.is_none(),
        "fake_stream_seam: model id {model_id:?} is already installed"
    );
    SeamHandle {
        model_id: model_id.to_owned(),
        state,
    }
}

/// The hook `generate_byop_output` calls. `None` = no script for this model id: use the network.
pub(crate) fn intercept(model_id: &str, req: &ChatRequest) -> Option<genai::Result<SeamStream>> {
    let state = lock(&REGISTRY).get(model_id).cloned()?;
    let recorded = {
        let mut requests = lock(&state.requests);
        let recorded = RecordedRequest::from_chat_request(requests.len(), model_id, req);
        requests.push(recorded.clone());
        recorded
    };
    let turn = (lock(&state.script))(&recorded);
    if let Some(error) = &turn.open_error {
        return Some(Err(into_genai_error(error, model_id)));
    }
    Some(Ok(into_stream(turn, model_id)))
}

fn model_iden(model_id: &str) -> ModelIden {
    ModelIden::new(AdapterKind::OpenAI, model_id.to_owned())
}

/// Builds the error genai would deliver. A status is `HttpError` boxed in `WebStream` (the shape
/// `map_genai_error` unwraps); a transport failure is a boxed plain error.
fn into_genai_error(error: &SeamError, model_id: &str) -> genai::Error {
    match error {
        SeamError::Http { status, body } => {
            let status = http::StatusCode::from_u16(*status)
                .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);
            let inner = genai::Error::HttpError {
                status,
                canonical_reason: status.canonical_reason().unwrap_or_default().to_owned(),
                body: body.clone(),
                retry_after: None,
            };
            genai::Error::WebStream {
                model_iden: model_iden(model_id),
                cause: inner.to_string(),
                error: Box::new(inner),
            }
        }
        SeamError::Transport(cause) => genai::Error::WebStream {
            model_iden: model_iden(model_id),
            cause: cause.clone(),
            error: cause.clone().into(),
        },
        SeamError::Other(message) => genai::Error::Internal(message.clone()),
    }
}

#[derive(Clone, Copy)]
enum Next {
    Start,
    Event(usize),
    End,
    Done,
}

/// Plays `turn`: `Start`, then each event, then `End` (if `turn.end`) with the content genai
/// would capture (text, then tool calls, arguments parsed when they are valid JSON).
fn into_stream(turn: SeamTurn, model_id: &str) -> SeamStream {
    let model_id = model_id.to_owned();
    let events = Arc::new(turn.events);
    let end = turn.end;
    let captured = Arc::new(captured_content(&events));
    Box::pin(futures::stream::unfold(Next::Start, move |mut next| {
        let model_id = model_id.clone();
        let events = events.clone();
        let captured = captured.clone();
        async move {
            loop {
                match next {
                    Next::Start => return Some((Ok(ChatStreamEvent::Start), Next::Event(0))),
                    Next::Event(i) => {
                        let Some(event) = events.get(i).cloned() else {
                            next = if end { Next::End } else { Next::Done };
                            continue;
                        };
                        let after = Next::Event(i + 1);
                        match event {
                            SeamEvent::Text(content) => {
                                return Some((
                                    Ok(ChatStreamEvent::Chunk(StreamChunk { content })),
                                    after,
                                ));
                            }
                            SeamEvent::Reasoning(content) => {
                                return Some((
                                    Ok(ChatStreamEvent::ReasoningChunk(StreamChunk { content })),
                                    after,
                                ));
                            }
                            SeamEvent::ToolCall { id, name, args } => {
                                return Some((
                                    Ok(ChatStreamEvent::ToolCallChunk(ToolChunk {
                                        tool_call: ToolCall {
                                            call_id: id,
                                            fn_name: name,
                                            fn_arguments: Value::String(args),
                                            thought_signatures: None,
                                        },
                                    })),
                                    after,
                                ));
                            }
                            SeamEvent::Wait(pause) => {
                                warpui::r#async::Timer::after(pause).await;
                                next = after;
                            }
                            SeamEvent::Fail(error) => {
                                return Some((
                                    Err(into_genai_error(&error, &model_id)),
                                    Next::Done,
                                ));
                            }
                            SeamEvent::Stall => futures::future::pending::<()>().await,
                        }
                    }
                    Next::End => {
                        return Some((
                            Ok(ChatStreamEvent::End(StreamEnd {
                                captured_content: captured.as_ref().clone(),
                                captured_stop_reason: Some(StopReason::Completed(
                                    "stop".to_owned(),
                                )),
                                ..StreamEnd::default()
                            })),
                            Next::Done,
                        ));
                    }
                    Next::Done => return None,
                }
            }
        }
    }))
}

fn captured_content(events: &[SeamEvent]) -> Option<MessageContent> {
    let mut text = String::new();
    let mut parts: Vec<ContentPart> = Vec::new();
    let mut calls: Vec<ContentPart> = Vec::new();
    for event in events {
        match event {
            SeamEvent::Text(t) => text.push_str(t),
            SeamEvent::ToolCall { id, name, args } => {
                // genai parses the accumulated text when it is valid JSON and keeps the raw
                // string otherwise.
                let fn_arguments = serde_json::from_str::<Value>(args)
                    .unwrap_or_else(|_| Value::String(args.clone()));
                calls.push(ContentPart::ToolCall(ToolCall {
                    call_id: id.clone(),
                    fn_name: name.clone(),
                    fn_arguments,
                    thought_signatures: None,
                }));
            }
            _ => {}
        }
    }
    if !text.is_empty() {
        parts.push(ContentPart::Text(text));
    }
    parts.extend(calls);
    (!parts.is_empty()).then(|| MessageContent::from_parts(parts))
}

#[cfg(test)]
#[path = "fake_stream_seam_tests.rs"]
mod tests;
