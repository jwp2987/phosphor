//! Producer-side coalescing of streamed text/reasoning chunks (#1204).
//!
//! The BYOP stream yields one `AppendToMessageContent` event per provider chunk (20-200 per
//! second on a fast model). Each event is a separate main-thread task that fans out to ~8
//! subscribers, re-renders the streaming block and notifies the input editor, so the cost of a
//! streamed reply scales with the *chunk rate*, not with how fast a human can read. Measured in
//! round 18 with the owner's config: ~2.3 ms of main-thread CPU per token and key-to-paint p95
//! up by 5-15 ms while typing during a stream; a backlog (a chunk arriving before the previous
//! one is processed) queues keystrokes behind it.
//!
//! [`coalesce_append_events`] merges *consecutive* appends to the same message and field into
//! one event, flushing at most every [`APPEND_COALESCE_WINDOW`]. It adds no latency to a lone
//! chunk (the first append after a quiet period goes out immediately) and no reordering: any
//! other event, an error, or the end of the stream flushes the held chunk first, and a timer
//! flushes it even when the provider goes silent, so text is never stranded.
//!
//! The merged event carries exactly the concatenation of the chunks' text, so the conversation
//! (which already parses appended text incrementally, `IncrementalTextSections`) ends in the same
//! state as with per-chunk events.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::{Stream, StreamExt};
use instant::Instant;
use warp_multi_agent_api as api;

use crate::ai::api_error::AIApiError;

/// How long consecutive chunks are held and merged. Roughly two display frames at 60 Hz, so the
/// text still appears to stream smoothly while the main thread does at most ~30 updates/s.
pub const APPEND_COALESCE_WINDOW: Duration = Duration::from_millis(33);

type Event = Result<api::ResponseEvent, Arc<AIApiError>>;

/// The single text/reasoning append inside `event`, if that is all it contains.
fn as_text_append_mut(
    event: &mut api::ResponseEvent,
) -> Option<&mut api::client_action::AppendToMessageContent> {
    let api::response_event::Type::ClientActions(actions) = event.r#type.as_mut()? else {
        return None;
    };
    let [action] = actions.actions.as_mut_slice() else {
        return None;
    };
    let api::client_action::Action::AppendToMessageContent(append) = action.action.as_mut()? else {
        return None;
    };
    Some(append)
}

/// The text of a pure text/reasoning append (the only kinds that are merged).
fn text_of(message: &mut api::Message) -> Option<&mut String> {
    match message.message.as_mut()? {
        api::message::Message::AgentOutput(output) => Some(&mut output.text),
        api::message::Message::AgentReasoning(reasoning) => Some(&mut reasoning.reasoning),
        _ => None,
    }
}

fn mask_paths(append: &api::client_action::AppendToMessageContent) -> Option<&[String]> {
    append.mask.as_ref().map(|mask| mask.paths.as_slice())
}

/// Whether `event` is a single text/reasoning append (a candidate for merging).
fn is_text_append(event: &mut api::ResponseEvent) -> bool {
    as_text_append_mut(event)
        .and_then(|append| append.message.as_mut())
        .is_some_and(|message| text_of(message).is_some())
}

/// Appends `next`'s text to `pending` when both are text appends to the same task, message and
/// field. Returns false (leaving both untouched) otherwise.
fn try_merge(pending: &mut api::ResponseEvent, next: &mut api::ResponseEvent) -> bool {
    let Some(p) = as_text_append_mut(pending) else {
        return false;
    };
    let Some(n) = as_text_append_mut(next) else {
        return false;
    };
    if p.task_id != n.task_id || mask_paths(p) != mask_paths(n) {
        return false;
    }
    let (Some(pm), Some(nm)) = (p.message.as_mut(), n.message.as_mut()) else {
        return false;
    };
    if pm.id != nm.id || pm.task_id != nm.task_id {
        return false;
    }
    // Same variant: the two `text_of` results must come from the same message kind.
    let same_kind = matches!(
        (pm.message.as_ref(), nm.message.as_ref()),
        (
            Some(api::message::Message::AgentOutput(_)),
            Some(api::message::Message::AgentOutput(_))
        ) | (
            Some(api::message::Message::AgentReasoning(_)),
            Some(api::message::Message::AgentReasoning(_))
        )
    );
    if !same_kind {
        return false;
    }
    let (Some(pt), Some(nt)) = (text_of(pm), text_of(nm)) else {
        return false;
    };
    pt.push_str(nt);
    true
}

/// Wraps `inner`, merging consecutive text/reasoning appends that arrive within `window` of the
/// last emitted append. See the module docs for the guarantees.
pub fn coalesce_append_events<S>(
    inner: S,
    window: Duration,
) -> Pin<Box<dyn Stream<Item = Event> + Send + 'static>>
where
    S: Stream<Item = Event> + Send + 'static,
{
    Box::pin(async_stream::stream! {
        let mut inner = Box::pin(inner);
        // A merged append being held, and when it must go out at the latest.
        let mut pending: Option<(api::ResponseEvent, Instant)> = None;
        // When an append last went out; drives "first chunk after a quiet period is immediate".
        let mut last_emit: Option<Instant> = None;
        loop {
            let item = match &pending {
                None => inner.next().await,
                Some((_, deadline)) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let next = inner.next();
                    let timer = warpui::r#async::Timer::after(remaining);
                    futures::pin_mut!(next, timer);
                    match futures::future::select(next, timer).await {
                        futures::future::Either::Left((item, _)) => item,
                        futures::future::Either::Right(_) => {
                            if let Some((event, _)) = pending.take() {
                                last_emit = Some(Instant::now());
                                yield Ok(event);
                            }
                            continue;
                        }
                    }
                }
            };
            match item {
                None => {
                    if let Some((event, _)) = pending.take() {
                        yield Ok(event);
                    }
                    break;
                }
                Some(Err(error)) => {
                    if let Some((event, _)) = pending.take() {
                        yield Ok(event);
                    }
                    yield Err(error);
                }
                Some(Ok(mut event)) => {
                    if !is_text_append(&mut event) {
                        if let Some((held, _)) = pending.take() {
                            yield Ok(held);
                        }
                        yield Ok(event);
                        continue;
                    }
                    match pending.as_mut() {
                        Some((held, _)) => {
                            if !try_merge(held, &mut event) {
                                // A different message/field: flush the held one, then treat
                                // this append as a fresh candidate.
                                let (held, _) = pending.take().expect("pending was Some");
                                last_emit = Some(Instant::now());
                                yield Ok(held);
                                pending = Some((event, Instant::now() + window));
                            }
                        }
                        None => {
                            let quiet = last_emit
                                .is_none_or(|at| at.elapsed() >= window);
                            if quiet {
                                last_emit = Some(Instant::now());
                                yield Ok(event);
                            } else {
                                let deadline = last_emit.map_or_else(Instant::now, |at| at + window);
                                pending = Some((event, deadline));
                            }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
#[path = "append_coalesce_tests.rs"]
mod tests;
