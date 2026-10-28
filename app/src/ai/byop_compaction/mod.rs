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
    /// The turn ended over the threshold *asking for tool calls* (#1059): don't summarize
    /// now (that cancels the calls, #959), but when this step's tool results are ready,
    /// send them with an automatic summary instead of as a plain follow-up. See
    /// [`TurnCompactionState::on_tool_results_ready`].
    pub compact_after_tool_results: bool,
}

/// The auto-compaction policy for a cleanly finished BYOP stream (#959), as a pure function
/// so the controller's behaviour matrix is testable without a provider:
///
/// - an automatic summary that committed → resume the task with the continue message
///   (opencode `compaction.ts`: `auto` and nothing to replay → synthetic "Continue ...");
/// - a manual `/compact` summary → nothing more (the user asked only for the summary);
/// - a turn that requested tool calls → never compact now: its calls are queued only after
///   the finish handler returns, and a summary sent first cancels them and halts the task.
///   Over the threshold with the allowance unspent, compact once its tool results are ready
///   instead (`compact_after_tool_results`, #1059). With the allowance spent (the summary
///   itself did not get the context under the threshold) do nothing: compacting again would
///   loop, and giving up would cancel the calls; the loop carries on uncompacted, and its
///   first tool-free turn end still over the threshold is [`AutoCompactionDecision::GiveUp`];
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
            compact_after_tool_results: false,
        },
        FinishedStream::Turn {
            requested_tool_calls,
            overflowing,
        } => {
            let restore_turn_allowance =
                already_auto_compacted_this_turn && overflowing == Some(false);
            let overflow_decision = auto_compaction_decision(
                overflowing == Some(true),
                already_auto_compacted_this_turn && !restore_turn_allowance,
            );
            let (decision, compact_after_tool_results) = if requested_tool_calls {
                (
                    AutoCompactionDecision::None,
                    overflow_decision == AutoCompactionDecision::Compact,
                )
            } else {
                (overflow_decision, false)
            };
            FinishedStreamPlan {
                decision,
                resume_with_continue_message: false,
                restore_turn_allowance,
                compact_after_tool_results,
            }
        }
    }
}

/// What to send once a tool-call step's results are ready (the point where the agent loop
/// sends its follow-up). See [`TurnCompactionState::on_tool_results_ready`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultsRequest {
    /// The ordinary follow-up: the results go back to the model.
    FollowUp,
    /// One request carrying the results *and* an automatic `SummarizeConversation
    /// { overflow: true }` (#1059). The BYOP preflight records the results in history
    /// before the summary request is built (the same commit a plain follow-up gets), so the
    /// summarizer sees every call with its real result and nothing is cancelled or
    /// synthesized. The summary then resumes the task with the continue message
    /// ([`FinishedStreamPlan::resume_with_continue_message`]) — even if it found nothing to
    /// commit, because this summary interrupted a task mid-loop. The controller reports the
    /// send with [`TurnCompactionState::on_deferred_summary_sent`].
    SummarizeWithResults,
}

/// The auto-compaction bookkeeping the controller keeps per conversation between streams,
/// driven only through these methods so the whole sequence is testable without a provider.
///
/// Invariants:
/// - at most one automatic summary per turn unless a later turn came in under the threshold
///   (`auto_compacted`, #959 / cc5b6a422);
/// - `compact_after_tool_results` is set only by a cleanly finished tool-call step that was
///   over the threshold with the allowance unspent, and lives only until that step's results
///   are ready: any later finished stream recomputes it, and a new user turn, a stop, a
///   close, or a step whose results trigger no follow-up drops it;
/// - the deferral never cancels, drops or replaces a tool result: it only changes the request
///   the results travel in;
/// - the allowance is spent only once the deferred summary request was actually sent (or
///   stashed to be sent), not when it was merely decided;
/// - a deferred summary always resumes the task when it finishes cleanly, committed or not:
///   it interrupted a task mid-loop, which must never stop silently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnCompactionState {
    auto_compacted: bool,
    compact_after_tool_results: bool,
    /// A deferred summary (with a tool-loop step's results) is in flight; the next finished
    /// stream is that summary.
    awaiting_deferred_summary: bool,
}

impl TurnCompactionState {
    /// Nothing to remember; the controller may drop the entry.
    pub fn is_idle(&self) -> bool {
        !self.auto_compacted && !self.compact_after_tool_results && !self.awaiting_deferred_summary
    }

    /// Whether this turn's one auto-compaction allowance is spent.
    pub fn auto_compacted(&self) -> bool {
        self.auto_compacted
    }

    /// Whether the next ready tool results go out with an automatic summary.
    pub fn compaction_deferred_to_tool_results(&self) -> bool {
        self.compact_after_tool_results
    }

    /// A BYOP stream of this conversation finished cleanly: plan per
    /// [`plan_after_stream_finished`] and record what the plan implies.
    ///
    /// One addition to the pure plan: a deferred summary (see
    /// [`Self::on_deferred_summary_sent`]) that committed nothing still resumes the task. It
    /// interrupted a loop mid-task; without the continue message the task would just stop.
    /// The resumed request carries the uncompacted history, so a context still over the
    /// threshold ends at the next tool-free turn with the visible GiveUp error, not silently.
    pub fn on_stream_finished(&mut self, stream: FinishedStream) -> FinishedStreamPlan {
        let deferred_summary = std::mem::take(&mut self.awaiting_deferred_summary);
        let mut plan = plan_after_stream_finished(stream, self.auto_compacted);
        if deferred_summary && matches!(stream, FinishedStream::Summary { auto: true, .. }) {
            plan.resume_with_continue_message = true;
        }
        if plan.restore_turn_allowance {
            self.auto_compacted = false;
        }
        if plan.decision == AutoCompactionDecision::Compact {
            self.auto_compacted = true;
        }
        // Whatever this stream is, it is the latest step: an earlier deferral belonged to a
        // step whose results have been sent (or dropped) since.
        self.compact_after_tool_results = plan.compact_after_tool_results;
        plan
    }

    /// The finished step's tool results are ready to go back to the model. Consumes a
    /// deferral. `results_all_in_root_task`: every result answers a call of the
    /// conversation's root task; results of a subagent task (a CLI/LRC subagent's own loop)
    /// are sent as a plain follow-up and the deferral is dropped, so a summary never lands in
    /// the middle of a subagent's work (subagent loops are not compacted: #1074). The next
    /// finished stream re-checks the usage.
    ///
    /// Does not spend the allowance: the controller calls [`Self::on_deferred_summary_sent`]
    /// once the request really went out.
    pub fn on_tool_results_ready(&mut self, results_all_in_root_task: bool) -> ToolResultsRequest {
        if !std::mem::take(&mut self.compact_after_tool_results) || !results_all_in_root_task {
            return ToolResultsRequest::FollowUp;
        }
        ToolResultsRequest::SummarizeWithResults
    }

    /// The [`ToolResultsRequest::SummarizeWithResults`] request was sent (or stashed by the
    /// BYOP readiness pass, to be sent once pending results land): the turn's allowance is
    /// spent, and the next finished stream is the deferred summary.
    pub fn on_deferred_summary_sent(&mut self) {
        self.auto_compacted = true;
        self.awaiting_deferred_summary = true;
    }

    /// The step's results will not be sent as their own follow-up (the user rejected the
    /// only call, or a stashed request took them): drop a deferral so it can't fire on an
    /// unrelated later follow-up. The allowance is unchanged.
    pub fn on_tool_results_abandoned(&mut self) {
        self.compact_after_tool_results = false;
    }

    /// The user stopped the conversation (Stop / Ctrl-C / a superseding submission) or
    /// closed its pane: drop the deferral, and forget an in-flight deferred summary (a
    /// stopped stream never finishes cleanly, so nothing must resume on its behalf). The
    /// allowance is unchanged.
    pub fn on_conversation_stopped(&mut self) {
        self.compact_after_tool_results = false;
        self.awaiting_deferred_summary = false;
    }

    /// The user started a new turn (anything but the agent's own continue message): a fresh
    /// allowance and no deferral.
    pub fn on_new_user_turn(&mut self) {
        *self = Self::default();
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
