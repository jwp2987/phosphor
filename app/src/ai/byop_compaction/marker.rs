//! How a BYOP compaction summary is shown in the transcript (#1060).
//!
//! The summary exchange (auto-compaction or a manual `/compact`) streams the
//! `## Goal / ## Progress / ...` template back as an ordinary agent text message. Shown as
//! is, it reads as the agent's answer. The blocklist and the TUI instead show a collapsed
//! marker ("Conversation compacted"), with the summary text available by expanding it. The
//! text itself is not changed, because later requests still read it from history.
//!
//! Which message is a summary comes from the data, never from matching the template:
//! - [`CompactionState::completed_for_summary`]: the committed record of the compaction.
//!   It is persisted with the conversation (`compaction_state_json`), so it also covers
//!   restored conversations.
//! - Otherwise, the exchange's own `AIAgentInput::SummarizeConversation { overflow }`. This
//!   covers the summary while it is still streaming (it is committed only once the stream
//!   finishes) and a summary that was stopped before it committed.

use super::state::CompactionState;

/// How a compaction summary message is labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionMarker {
    /// The summary is still streaming.
    InProgress,
    /// The summary finished. `auto` is true for the automatic (context overflow) trigger
    /// and false for a manual `/compact`.
    Compacted { auto: bool },
    /// The summary stream was cancelled or failed before the compaction was recorded.
    Interrupted,
}

/// The state of the response stream that holds the summary message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryStreamStatus {
    Streaming,
    Complete,
    /// Cancelled or failed.
    Stopped,
}

/// Decides whether the output message `message_id` is a compaction summary, and how to
/// label it.
///
/// - `state`: the conversation's compaction state (`None` when the conversation cannot be
///   resolved).
/// - `summarize_request_overflow`: `Some(overflow)` when the exchange holding the message
///   was a `SummarizeConversation` request, `None` for any other exchange.
/// - `stream`: the status of that exchange's response stream.
///
/// A committed compaction wins over the exchange input, so a restored conversation (where
/// the summary may not sit in its own `SummarizeConversation` exchange) is still labelled,
/// with the trigger that was recorded.
pub fn compaction_marker(
    state: Option<&CompactionState>,
    message_id: &str,
    summarize_request_overflow: Option<bool>,
    stream: SummaryStreamStatus,
) -> Option<CompactionMarker> {
    if let Some(completed) = state.and_then(|state| state.completed_for_summary(message_id)) {
        return Some(CompactionMarker::Compacted {
            auto: completed.auto,
        });
    }
    let overflow = summarize_request_overflow?;
    Some(match stream {
        SummaryStreamStatus::Streaming => CompactionMarker::InProgress,
        SummaryStreamStatus::Complete => CompactionMarker::Compacted { auto: overflow },
        SummaryStreamStatus::Stopped => CompactionMarker::Interrupted,
    })
}

impl CompactionMarker {
    /// The marker's user-visible label.
    pub fn label(self) -> String {
        match self {
            CompactionMarker::InProgress => crate::t!("ai-block-compaction-in-progress"),
            CompactionMarker::Compacted { auto: true } => {
                crate::t!("ai-block-compaction-done-auto")
            }
            CompactionMarker::Compacted { auto: false } => {
                crate::t!("ai-block-compaction-done-manual")
            }
            CompactionMarker::Interrupted => crate::t!("ai-block-compaction-interrupted"),
        }
    }
}

#[cfg(test)]
#[path = "marker_tests.rs"]
mod tests;
