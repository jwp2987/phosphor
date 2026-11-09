//! Recovery for a BYOP request that fails before the model has sent anything.
//!
//! Two failure rules, both applied around `open_sdk_stream` in `chat_stream::generate_byop_output`:
//!
//! 1. **Retry only before the first streamed byte.** A rate limit (429), a request timeout (408),
//!    a server error (5xx) or a transport failure that arrives before any real event is retried
//!    after 2, 4 and 8 seconds. A `Retry-After` the provider sent replaces that step's wait. Once
//!    one real event has been read (text, reasoning, a tool-call chunk, the end), nothing is ever
//!    retried here: a re-sent request would repeat output the user already saw and could repeat a
//!    tool call. The error goes to the user as it always did. Anything that is not transient (a
//!    rejected key, an unknown model, a decode failure) is never retried.
//! 2. **Context overflow is answered by trimming, once.** If the request is already over the
//!    model's usable window before it is sent, or the provider answers that it is too long, the
//!    oldest conversation turns are dropped from *this request* (not from the stored history) and
//!    the request is sent again once. What stays: every system message, the latest user message and
//!    everything after it, and every tool call together with its results (a call and its results
//!    are dropped together, so no tool result is ever left without its call). A marker line tells
//!    the model that older turns were left out.
//!
//! The decisions are pure functions ([`classify`], [`compact_request`], [`retry_wait`]) and the
//! driver ([`open_with_recovery`]) takes its clock and its opener as arguments, so the tests run
//! it against `fake_stream_seam` with no sleeping.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use futures::{Stream, StreamExt};
use genai::chat::{ChatMessage, ChatRequest, ChatRole, ChatStreamEvent, ContentPart};

/// The stream the send loop reads (genai's `ChatStream`, boxed so events read ahead of the loop
/// can be put back in front of it).
pub(crate) type SdkStream =
    Pin<Box<dyn Stream<Item = genai::Result<ChatStreamEvent>> + Send + 'static>>;

/// The wait before retry number `n` (0-based) when the provider named none.
pub(crate) const RETRY_SCHEDULE: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];

/// Text put in front of the first kept message when older turns were left out of a request.
pub(crate) const COMPACTION_MARKER: &str = "[Earlier turns of this conversation were left out to fit the model's context window. Continue from the messages below.]";

/// What to do about a failure that happened before the first streamed byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FailureClass {
    /// Transient: try again after the schedule's wait, or `retry_after` when the provider sent one.
    Retryable { retry_after: Option<Duration> },
    /// The provider says the request is longer than the model's window.
    ContextOverflow,
    /// Not worth repeating.
    Fatal,
}

/// Phrases providers use for "your request is too long" (lowercase). OpenAI-compatible servers say
/// `context_length_exceeded` / "maximum context length", Anthropic "prompt is too long", others
/// "too many tokens" or ask to "reduce the length".
const OVERFLOW_PHRASES: [&str; 9] = [
    "context_length_exceeded",
    "maximum context length",
    "context length",
    "context window",
    "prompt is too long",
    "too many tokens",
    "reduce the length",
    "input is too long",
    "exceeds the maximum",
];

pub(crate) fn is_context_overflow(status: u16, body: &str) -> bool {
    if !matches!(status, 400 | 413 | 422) {
        return false;
    }
    let body = body.to_ascii_lowercase();
    OVERFLOW_PHRASES.iter().any(|p| body.contains(p))
}

fn classify_status(
    status: http::StatusCode,
    body: &str,
    retry_after: Option<&str>,
) -> FailureClass {
    if status.is_success() {
        // A 2xx that genai reports as an error is an HTML page: not transient.
        return FailureClass::Fatal;
    }
    if is_context_overflow(status.as_u16(), body) {
        return FailureClass::ContextOverflow;
    }
    let transient = status == http::StatusCode::REQUEST_TIMEOUT
        || status == http::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error();
    if transient {
        FailureClass::Retryable {
            retry_after: retry_after
                .and_then(|raw| crate::ai::api_error::parse_retry_after(raw, chrono::Utc::now())),
        }
    } else {
        FailureClass::Fatal
    }
}

/// Sorts a genai error: a status the provider returned (top level, or boxed in `WebStream` as
/// genai delivers it for a stream), a transport failure, or something else.
pub(crate) fn classify(err: &genai::Error) -> FailureClass {
    use genai::Error as G;
    match err {
        G::HttpError {
            status,
            body,
            retry_after,
            ..
        } => classify_status(*status, body, retry_after.as_deref()),
        G::WebStream { error, .. } => match error.downcast_ref::<genai::Error>() {
            Some(G::HttpError {
                status,
                body,
                retry_after,
                ..
            }) => classify_status(*status, body, retry_after.as_deref()),
            // Not a status: reset, EOF, DNS, TLS, timeout.
            _ => FailureClass::Retryable { retry_after: None },
        },
        _ => FailureClass::Fatal,
    }
}

/// How long to wait before retry number `retry_index` (0-based), or `None` when the schedule is
/// used up. A usable `Retry-After` replaces the scheduled wait.
pub(crate) fn retry_wait(retry_index: usize, retry_after: Option<Duration>) -> Option<Duration> {
    let scheduled = *RETRY_SCHEDULE.get(retry_index)?;
    Some(retry_after.unwrap_or(scheduled))
}

// ---- compaction ----------------------------------------------------------------------------

/// Bytes a message adds to the prompt, counting text, tool-call arguments and tool results but
/// not binaries (an image's base64 would otherwise make a short chat look over the window).
fn message_bytes(message: &ChatMessage) -> usize {
    let content = &message.content;
    let text: usize = content.texts().iter().map(|t| t.len()).sum();
    let calls: usize = content
        .tool_calls()
        .iter()
        .map(|c| c.fn_name.len() + c.fn_arguments.to_string().len())
        .sum();
    let results: usize = content
        .tool_responses()
        .iter()
        .map(|r| r.content.len())
        .sum();
    text + calls + results
}

/// Bytes of the prompt text, system included.
pub(crate) fn request_bytes(req: &ChatRequest) -> usize {
    req.system.as_deref().map_or(0, str::len)
        + req.messages.iter().map(message_bytes).sum::<usize>()
}

/// What [`compact_request`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Compaction {
    pub dropped_messages: usize,
    pub dropped_bytes: usize,
}

/// Drops the oldest turns from `req` until it is at most `target_bytes` (always at least one unit
/// when anything is droppable), then marks the gap. Returns `None`, leaving `req` unchanged, when
/// nothing may be dropped.
///
/// A *unit* is one message plus the messages carrying tool results that directly follow it, so an
/// assistant message with tool calls always goes together with its results. System messages and
/// everything from the latest user message on are never dropped.
pub(crate) fn compact_request(req: &mut ChatRequest, target_bytes: usize) -> Option<Compaction> {
    let tail_start = req
        .messages
        .iter()
        .rposition(|m| m.role == ChatRole::User && !m.content.contains_tool_response())?;

    // Units over the droppable head: lists of message indices, oldest first.
    let mut units: Vec<Vec<usize>> = Vec::new();
    for (i, m) in req.messages[..tail_start].iter().enumerate() {
        if m.role == ChatRole::System {
            continue;
        }
        let continues_previous = m.content.contains_tool_response()
            && units
                .last()
                .is_some_and(|u| u.last().is_some_and(|&j| j + 1 == i));
        match units.last_mut() {
            Some(unit) if continues_previous => unit.push(i),
            _ => units.push(vec![i]),
        }
    }
    if units.is_empty() {
        return None;
    }

    let mut total = request_bytes(req);
    let mut drop_indices: Vec<usize> = Vec::new();
    let mut dropped_bytes = 0usize;
    for unit in &units {
        if total <= target_bytes && !drop_indices.is_empty() {
            break;
        }
        let bytes: usize = unit.iter().map(|&i| message_bytes(&req.messages[i])).sum();
        total = total.saturating_sub(bytes);
        dropped_bytes += bytes;
        drop_indices.extend(unit.iter().copied());
    }

    let dropped_messages = drop_indices.len();
    let mut index = 0usize;
    req.messages.retain(|_| {
        let keep = !drop_indices.contains(&index);
        index += 1;
        keep
    });

    // Mark the gap on the first kept non-system message: fold into it when it is a user message
    // (keeps roles alternating), otherwise put a user message in front of it.
    let first = req
        .messages
        .iter()
        .position(|m| m.role != ChatRole::System)
        .unwrap_or(req.messages.len());
    match req.messages.get_mut(first) {
        Some(m) if m.role == ChatRole::User && !m.content.contains_tool_response() => {
            m.content
                .prepend(ContentPart::Text(format!("{COMPACTION_MARKER}\n\n")));
        }
        _ => req
            .messages
            .insert(first, ChatMessage::user(COMPACTION_MARKER)),
    }
    Some(Compaction {
        dropped_messages,
        dropped_bytes,
    })
}

/// Test helper and invariant I4 for a request: every tool result answers a call made earlier in
/// the request, and no call id is answered twice.
pub(crate) fn tool_pairs_are_intact(req: &ChatRequest) -> bool {
    let mut open: Vec<String> = Vec::new();
    let mut answered: Vec<String> = Vec::new();
    for m in &req.messages {
        for call in m.content.tool_calls() {
            open.push(call.call_id.clone());
        }
        for result in m.content.tool_responses() {
            if !open.contains(&result.call_id) || answered.contains(&result.call_id) {
                return false;
            }
            answered.push(result.call_id.clone());
        }
    }
    true
}

// ---- the driver ----------------------------------------------------------------------------

/// A stream that survived the first read.
pub(crate) struct Opened {
    pub stream: SdkStream,
    /// How long the final attempt's read-ahead already waited for its first real event; the send
    /// loop subtracts it from the first-event window so the window is not spent twice.
    pub waited: Duration,
    pub retries: usize,
    pub compaction: Option<Compaction>,
}

/// The request could not be opened. `error` is the last genai error (the caller builds its usual
/// message from it and appends [`OpenFailure::user_note`]).
pub(crate) struct OpenFailure {
    pub error: genai::Error,
    pub waits: Vec<Duration>,
    pub compaction: Option<Compaction>,
    /// The overflow answer came back and nothing was left to drop (or it came back a second time).
    pub overflow_unresolved: bool,
}

impl OpenFailure {
    /// The retry schedule ran out, or the prompt stayed too long after trimming: the outer
    /// recovery layer must not repeat any of it.
    pub fn recovery_exhausted(&self) -> bool {
        self.overflow_unresolved || self.waits.len() >= RETRY_SCHEDULE.len()
    }

    /// A sentence for the user about what was tried, or `None` when nothing was.
    pub fn user_note(&self) -> Option<String> {
        if self.overflow_unresolved {
            return Some(match self.compaction {
                Some(c) => format!(
                    "The conversation is still too long for the model's context window after \
                     leaving out {} older message(s). Start a new conversation or choose a model \
                     with a larger context window.",
                    c.dropped_messages
                ),
                None => "The conversation is too long for the model's context window and \
                         there is nothing older to leave out. Shorten your message or choose a \
                         model with a larger context window."
                    .to_owned(),
            });
        }
        if self.waits.is_empty() {
            return None;
        }
        let waits = self
            .waits
            .iter()
            .map(|w| format!("{}s", w.as_secs()))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Phosphor retried {} time(s) (waiting {waits}) and the provider still sent \
             nothing. Nothing was received, so it is safe to send your message again.",
            self.waits.len()
        ))
    }
}

/// Reads until the first event that is not `Start` (a local event sent before any I/O). Returns
/// the stream with the events read put back in front, or the error that came first. If
/// `timeout` fires first the stream is returned as is: the send loop applies its own timeout.
async fn peek_first_event<T, TFut>(
    mut stream: SdkStream,
    timeout: &mut T,
) -> genai::Result<(SdkStream, Duration)>
where
    T: FnMut() -> TFut,
    TFut: Future<Output = ()>,
{
    let started = instant::Instant::now();
    let mut read: Vec<ChatStreamEvent> = Vec::new();
    loop {
        let next = stream.next();
        let idle = timeout();
        futures::pin_mut!(next, idle);
        match futures::future::select(next, idle).await {
            futures::future::Either::Left((Some(Ok(event)), _)) => {
                let real = !matches!(event, ChatStreamEvent::Start);
                read.push(event);
                if real {
                    break;
                }
            }
            futures::future::Either::Left((Some(Err(e)), _)) => return Err(e),
            futures::future::Either::Left((None, _)) | futures::future::Either::Right(_) => break,
        }
    }
    Ok((
        Box::pin(futures::stream::iter(read.into_iter().map(Ok)).chain(stream)),
        started.elapsed(),
    ))
}

/// Opens the request, retrying and compacting per the module rules. `target_bytes` is the size the
/// prompt should be trimmed to before the first send (`None` = only trim after an overflow answer).
pub(crate) async fn open_with_recovery<Open, OpenFut, Sleep, SleepFut, Timeout, TimeoutFut>(
    mut request: ChatRequest,
    target_bytes: Option<usize>,
    mut open: Open,
    mut sleep: Sleep,
    mut timeout: Timeout,
) -> Result<Opened, OpenFailure>
where
    Open: FnMut(ChatRequest) -> OpenFut,
    OpenFut: Future<Output = genai::Result<SdkStream>>,
    Sleep: FnMut(Duration) -> SleepFut,
    SleepFut: Future<Output = ()>,
    Timeout: FnMut() -> TimeoutFut,
    TimeoutFut: Future<Output = ()>,
{
    let mut waits: Vec<Duration> = Vec::new();
    let mut compaction: Option<Compaction> = None;
    let mut overflow_answered = false;

    if let Some(target) = target_bytes
        && request_bytes(&request) > target
    {
        compaction = compact_request(&mut request, target);
    }

    loop {
        let attempt = match open(request.clone()).await {
            Ok(stream) => peek_first_event(stream, &mut timeout).await,
            Err(e) => Err(e),
        };
        let error = match attempt {
            Ok((stream, waited)) => {
                return Ok(Opened {
                    stream,
                    waited,
                    retries: waits.len(),
                    compaction,
                });
            }
            Err(e) => e,
        };
        match classify(&error) {
            FailureClass::Retryable { retry_after } => {
                if let Some(wait) = retry_wait(waits.len(), retry_after) {
                    log::warn!(
                        "[byop] request failed before any output; retry {}/{} in {:?}",
                        waits.len() + 1,
                        RETRY_SCHEDULE.len(),
                        wait
                    );
                    waits.push(wait);
                    sleep(wait).await;
                    continue;
                }
            }
            FailureClass::ContextOverflow => {
                let first_answer = !overflow_answered;
                overflow_answered = true;
                if first_answer {
                    let half = request_bytes(&request) / 2;
                    if let Some(c) = compact_request(&mut request, half) {
                        log::warn!(
                            "[byop] provider reported a context overflow; dropped {} older \
                             message(s) ({} bytes) and retrying once",
                            c.dropped_messages,
                            c.dropped_bytes
                        );
                        compaction = Some(match compaction {
                            Some(prev) => Compaction {
                                dropped_messages: prev.dropped_messages + c.dropped_messages,
                                dropped_bytes: prev.dropped_bytes + c.dropped_bytes,
                            },
                            None => c,
                        });
                        continue;
                    }
                }
                return Err(OpenFailure {
                    error,
                    waits,
                    compaction,
                    overflow_unresolved: true,
                });
            }
            FailureClass::Fatal => {}
        }
        return Err(OpenFailure {
            error,
            waits,
            compaction,
            overflow_unresolved: false,
        });
    }
}

#[cfg(test)]
#[path = "stream_recovery_tests.rs"]
mod tests;
