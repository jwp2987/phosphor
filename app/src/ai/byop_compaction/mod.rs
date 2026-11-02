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

pub use commit::summarizable_history_tokens;
pub use config::CompactionConfig;
pub use overflow::{
    OverflowVerdict, overflow_verdict, turn_has_tool_calls, turn_has_visible_output, usable,
};

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

/// The one task every id in `task_ids` names, if there is exactly one (and at least one). The
/// task a deferred summary rides in with a step's tool results (#1074): the root task or a
/// CLI/LRC subagent's own task. `None` for results spanning tasks.
pub fn single_task_id<'a, T: PartialEq>(
    task_ids: impl IntoIterator<Item = &'a T>,
) -> Option<&'a T> {
    let mut ids = task_ids.into_iter();
    let first = ids.next()?;
    ids.all(|id| id == first).then_some(first)
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
/// - an automatic summary → resume the task with the continue message (opencode
///   `compaction.ts`: `auto` and nothing to replay → synthetic "Continue ..."), whether or not
///   it committed: every auto-compaction is followed by the continue message (owner rule), so
///   a summary that produced nothing usable (blank, or narration before a dropped tool call)
///   never leaves the task stopped with no error. It cannot loop: the allowance is already
///   spent, so a context still over the threshold ends at the next tool-free turn with
///   [`AutoCompactionDecision::GiveUp`];
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
        FinishedStream::Summary { auto, committed: _ } => FinishedStreamPlan {
            decision: AutoCompactionDecision::None,
            // `committed` only decides what the continue request is built from.
            resume_with_continue_message: auto,
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
    /// ([`FinishedStreamPlan::resume_with_continue_message`]), like every automatic summary,
    /// even if it found nothing to commit. The controller reports the send with
    /// [`TurnCompactionState::on_deferred_summary_sent`].
    SummarizeWithResults,
}

/// What `send_request_input` did with a request, as far as a deferred summary cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSendOutcome {
    /// A response stream was opened for it.
    Opened,
    /// The BYOP readiness pass stashed it until pending tool results land; it is resent then.
    Stashed,
    /// The readiness pass blocked it: rendered as an error, no stream (reported as `Ok`).
    Blocked,
    /// Refused before anything was sent (closed pane, request already in flight, ...).
    Refused,
}

impl RequestSendOutcome {
    /// Whether a summary sent this way will actually run — the only case in which it may
    /// spend the turn's allowance ([`TurnCompactionState::on_deferred_summary_sent`]).
    pub fn summary_will_run(self) -> bool {
        matches!(self, Self::Opened | Self::Stashed)
    }
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
/// - every automatic summary that finishes cleanly resumes the task, committed or not
///   ([`plan_after_stream_finished`]); the deferred one too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnCompactionState {
    auto_compacted: bool,
    compact_after_tool_results: bool,
    /// The synthetic continue message was sent and its reply has not finished yet (#1102).
    continue_pending: bool,
    /// The continue message was already re-sent once after an empty reply (#1102).
    continue_retried: bool,
    /// The summary was already re-asked once for the current compaction after the model
    /// answered it with a tool call (#1079); cleared once a summary commits or the allowance
    /// is restored.
    summary_retried: bool,
    /// The deferred summary request went out and its stream has not finished yet (#1238).
    deferred_summary_in_flight: bool,
    /// A deferred summary that failed already got its allowance back once this turn, so a
    /// summary that fails because it is itself too large cannot loop compact -> fail -> compact.
    deferred_summary_restored: bool,
}

/// What to do when the reply to the post-compaction continue message finishes
/// ([`TurnCompactionState::on_continue_reply`], #1102).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueReplyAction {
    /// Nothing: not a continue reply, or it produced visible output or tool calls.
    None,
    /// The reply was empty (no text, no tool calls): send the continue message once more.
    Retry,
    /// The reply was empty again after the retry: end the turn with a visible error.
    Fail,
}

impl TurnCompactionState {
    /// Nothing to remember; the controller may drop the entry.
    pub fn is_idle(&self) -> bool {
        !self.auto_compacted
            && !self.compact_after_tool_results
            && !self.continue_pending
            && !self.continue_retried
            && !self.summary_retried
            && !self.deferred_summary_in_flight
            && !self.deferred_summary_restored
    }

    /// Whether this turn's one auto-compaction allowance is spent.
    #[cfg(test)]
    pub fn auto_compacted(&self) -> bool {
        self.auto_compacted
    }

    /// Whether the next ready tool results go out with an automatic summary.
    #[cfg(test)]
    pub fn compaction_deferred_to_tool_results(&self) -> bool {
        self.compact_after_tool_results
    }

    /// A BYOP stream of this conversation finished cleanly: plan per
    /// [`plan_after_stream_finished`] and record what the plan implies.
    pub fn on_stream_finished(&mut self, stream: FinishedStream) -> FinishedStreamPlan {
        // Whatever finished, a deferred summary is no longer waiting on a reply.
        self.deferred_summary_in_flight = false;
        let plan = plan_after_stream_finished(stream, self.auto_compacted);
        if plan.restore_turn_allowance {
            self.auto_compacted = false;
        }
        let summary_committed = matches!(
            stream,
            FinishedStream::Summary {
                committed: true,
                ..
            }
        );
        if plan.restore_turn_allowance || summary_committed {
            self.summary_retried = false;
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
    /// deferral. `results_in_one_task`: every result answers a call of the same task (the
    /// root task, or a CLI/LRC subagent's own task, #1074); the summary request is then sent in
    /// that task so a subagent loop is compacted like the main one and resumes inside the
    /// subagent. Results spanning several tasks have no single task to carry the summary (a
    /// request over two tasks appends one exchange per task, and only the last is tracked
    /// by `added_exchanges_by_response`): they are sent as a plain follow-up and the deferral
    /// is dropped. The next finished stream re-checks the usage.
    ///
    /// Does not spend the allowance: the controller calls [`Self::on_deferred_summary_sent`]
    /// once the request really went out.
    pub fn on_tool_results_ready(&mut self, results_in_one_task: bool) -> ToolResultsRequest {
        if !std::mem::take(&mut self.compact_after_tool_results) || !results_in_one_task {
            return ToolResultsRequest::FollowUp;
        }
        ToolResultsRequest::SummarizeWithResults
    }

    /// A summarize stream finished without a summary because the model answered it with a tool
    /// call (providers that ignore `tool_choice`, #1079). Returns whether the summary may be
    /// asked for once more with an explicit "no tools" instruction; the retry is part of the
    /// same compaction, so it does not touch the allowance, and a second failure returns false
    /// (the task then resumes uncompacted, as before) — it cannot loop.
    pub fn claim_summary_retry(&mut self) -> bool {
        !std::mem::replace(&mut self.summary_retried, true)
    }

    /// The [`ToolResultsRequest::SummarizeWithResults`] request was sent (or stashed by the
    /// BYOP readiness pass, to be sent once pending results land): the turn's allowance is
    /// spent.
    pub fn on_deferred_summary_sent(&mut self) {
        self.auto_compacted = true;
        self.deferred_summary_in_flight = true;
    }

    /// The step's results will not be sent as their own follow-up (the user rejected the
    /// only call, or a stashed request took them): drop a deferral so it can't fire on an
    /// unrelated later follow-up. The allowance is unchanged.
    pub fn on_tool_results_abandoned(&mut self) {
        self.compact_after_tool_results = false;
    }

    /// The user stopped the conversation (Stop / Ctrl-C / a superseding submission) or
    /// closed its pane: drop the deferral, and a continue message whose reply was stopped is
    /// no longer owed one. A stopped summary stream never finishes cleanly, so nothing resumes
    /// on its behalf. The allowance is unchanged.
    pub fn on_conversation_stopped(&mut self) {
        self.compact_after_tool_results = false;
        self.continue_pending = false;
        self.deferred_summary_in_flight = false;
    }

    /// A request of this conversation failed for good (no retry or resume is coming): nothing
    /// about it is still on its way, so neither a deferral nor a pending continue reply may be
    /// waited on. The allowance is unchanged: a summary that failed did not compact, and
    /// restoring it could re-run a summary that fails the same way every turn (#1238).
    ///
    /// The one exception is a *deferred* summary (the tool results' request carried a summary):
    /// when that request fails for good nothing was compacted and the tool loop goes on
    /// uncompacted, so its first over-threshold turn end would give up with "still over the
    /// window after automatic compaction". Its allowance is restored, once per turn
    /// (`deferred_summary_restored`), so a summary that fails because it is itself too large
    /// cannot repeat every step (#1238).
    pub fn on_request_failed(&mut self) {
        self.compact_after_tool_results = false;
        self.continue_pending = false;
        if std::mem::take(&mut self.deferred_summary_in_flight) && !self.deferred_summary_restored {
            self.auto_compacted = false;
            self.deferred_summary_restored = true;
        }
    }

    /// A request of this conversation failed with a transport error and a plain resume is
    /// scheduled for it. The resumed request does not carry the summary again, so a deferred
    /// summary that was in flight never compacts: its allowance is restored exactly as
    /// [`Self::on_request_failed`] does (once per turn, shared `deferred_summary_restored`), and
    /// nothing else changes since the resume continues the same turn (#1238).
    pub fn on_request_resumed(&mut self) {
        if std::mem::take(&mut self.deferred_summary_in_flight) && !self.deferred_summary_restored {
            self.auto_compacted = false;
            self.deferred_summary_restored = true;
        }
    }

    /// A deferred summary is owed to the next tool results, or the continue message sent after a
    /// summary has not been answered yet: a request about this conversation is still coming.
    pub fn is_mid_compaction(&self) -> bool {
        self.compact_after_tool_results || self.continue_pending
    }

    /// The synthetic continue message was just sent: its reply is judged by
    /// [`Self::on_continue_reply`].
    pub fn on_continue_sent(&mut self) {
        self.continue_pending = true;
    }

    /// An agent turn finished. If it answers the continue message and
    /// `produced_output` is false (a reasoning-only or blank reply: the turn would end
    /// silently, #1102), retry the continue once, then fail visibly. Bounded: the retry flag
    /// is cleared by any reply that produced output.
    pub fn on_continue_reply(&mut self, produced_output: bool) -> ContinueReplyAction {
        if !std::mem::take(&mut self.continue_pending) {
            return ContinueReplyAction::None;
        }
        if produced_output {
            self.continue_retried = false;
            return ContinueReplyAction::None;
        }
        if self.continue_retried {
            self.continue_retried = false;
            ContinueReplyAction::Fail
        } else {
            self.continue_retried = true;
            ContinueReplyAction::Retry
        }
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
