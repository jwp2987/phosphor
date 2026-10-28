//! BYOP local session compaction — replicates opencode
//! `packages/opencode/src/session/{compaction,overflow,summary}.ts` 1:1.
//!
//! Entry-point API:
//! - [`overflow::is_overflow`] — auto-trigger determination (based on LLM response usage)
//! - [`algorithm::select`] — splits into head (sent to the summarization LLM) + tail (kept as-is)
//! - [`algorithm::prune`] — only clears old tool output (doesn't delete messages)
//! - [`prompt::build_prompt`] — builds the summary request text
//!
//! Decoupled from warp's server-side protobuf `SummarizeConversation`; only takes effect on the BYOP path.
pub mod algorithm;
pub mod commit;
pub mod config;
pub mod marker;
pub mod message_view;
pub mod overflow;
pub mod prompt;
pub mod state;
pub mod token;

pub use config::CompactionConfig;
pub use overflow::{is_overflow, turn_has_tool_calls, usable};

/// What to do when a finished, tool-free turn is over the context threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCompactionDecision {
    /// Under the threshold, or no usage reported: do nothing.
    None,
    /// Summarize now; the interrupted task is resumed after the summary commits.
    Compact,
    /// This turn was already auto-compacted once and is still over the limit; another
    /// summary would loop (compact, continue, still over, compact, ...). Fail instead.
    GiveUp,
}

/// Policy for [`AutoCompactionDecision`]: at most one auto-compaction per turn.
pub fn auto_compaction_decision(
    overflowing: bool,
    already_auto_compacted_this_turn: bool,
) -> AutoCompactionDecision {
    match (overflowing, already_auto_compacted_this_turn) {
        (false, _) => AutoCompactionDecision::None,
        (true, false) => AutoCompactionDecision::Compact,
        (true, true) => AutoCompactionDecision::GiveUp,
    }
}

/// The kind of BYOP response stream that just finished cleanly, as the compaction policy sees
/// it. See [`plan_after_stream_finished`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishedStream {
    /// A `SummarizeConversation` stream. `auto` is the overflow flag it was sent with (true
    /// only for the automatic trigger; a manual `/compact` sends false). `committed` is whether
    /// `commit::commit_summarization` found a summary and recorded the compaction.
    Summary { auto: bool, committed: bool },
    /// An ordinary agent turn. `requested_tool_calls`: the turn ended asking for tool calls,
    /// so the agent loop continues on its own once they run. `overflowing`: whether the
    /// reported usage is over the threshold, `None` when the provider reported no usage.
    Turn {
        requested_tool_calls: bool,
        overflowing: Option<bool>,
    },
}

/// What the controller does about compaction once a BYOP stream finishes cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinishedStreamPlan {
    /// Whether to start an automatic summary now (or fail the turn instead of looping).
    pub decision: AutoCompactionDecision,
    /// Send the synthetic continue message ([`prompt::build_continue_message`]) after the
    /// stream is marked complete, so the task carries on from the compacted history instead
    /// of ending on the summary text.
    pub resume_with_continue_message: bool,
    /// Give the conversation back its one-auto-compaction allowance: a turn after the
    /// compaction came in under the threshold, so the compaction worked and a later overflow
    /// in the same long task is new growth, not a summary that is still too big.
    pub restore_turn_allowance: bool,
}

/// The auto-compaction policy for a cleanly finished BYOP stream (#959), as a pure function
/// so the controller's behaviour matrix is testable without a provider:
///
/// - an automatic summary that committed → resume the task with the continue message
///   (opencode `compaction.ts`: `auto` and nothing to replay → synthetic "Continue ...");
/// - a manual `/compact` summary → nothing more (the user asked only for the summary);
/// - a turn that requested tool calls → never compact now: its calls are queued only after
///   the finish handler returns, and a summary sent first cancels them and halts the task;
/// - a tool-free turn over the threshold → compact (at most once per turn; a second overflow
///   with the allowance spent is [`AutoCompactionDecision::GiveUp`]);
/// - any turn under the threshold after an auto-compaction → restore the allowance.
pub fn plan_after_stream_finished(
    stream: FinishedStream,
    already_auto_compacted_this_turn: bool,
) -> FinishedStreamPlan {
    match stream {
        FinishedStream::Summary { auto, committed } => FinishedStreamPlan {
            decision: AutoCompactionDecision::None,
            resume_with_continue_message: auto && committed,
            restore_turn_allowance: false,
        },
        FinishedStream::Turn {
            requested_tool_calls,
            overflowing,
        } => {
            let restore_turn_allowance =
                already_auto_compacted_this_turn && overflowing == Some(false);
            let decision = if requested_tool_calls {
                AutoCompactionDecision::None
            } else {
                auto_compaction_decision(
                    overflowing == Some(true),
                    already_auto_compacted_this_turn && !restore_turn_allowance,
                )
            };
            FinishedStreamPlan {
                decision,
                resume_with_continue_message: false,
                restore_turn_allowance,
            }
        }
    }
}

/// Byte-for-byte matches the constants at the top of opencode `compaction.ts` (lines 33-39, overflow.ts:6, util/token.ts:1).
pub mod consts {
    pub const PRUNE_MINIMUM: usize = 20_000;
    pub const PRUNE_PROTECT: usize = 40_000;
    pub const TOOL_OUTPUT_MAX_CHARS: usize = 2_000;
    pub const DEFAULT_TAIL_TURNS: usize = 2;
    pub const MIN_PRESERVE_RECENT_TOKENS: usize = 2_000;
    pub const MAX_PRESERVE_RECENT_TOKENS: usize = 8_000;
    pub const COMPACTION_BUFFER: usize = 20_000;
    pub const CHARS_PER_TOKEN: usize = 4;
    pub const PRUNE_PROTECTED_TOOLS: &[&str] = &["skill"];
}

#[cfg(test)]
mod tests;
