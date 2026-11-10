//! #1967: "no pending action without a card", as a runtime invariant that heals itself.
//!
//! A turn hangs when the action model and the conversation disagree about a tool call and no
//! event is left that would settle it. The agent fuzzer's oracle states the rule as invariant I1
//! (`agent_fuzz/oracle.rs`); this module is the part of I1 the running app can decide EXACTLY,
//! from its own state, with no clock:
//!
//! | fault | what it is | heal |
//! |---|---|---|
//! | [`TurnFault::UnsettledCalls`] | the last reply finished cleanly, no stream is in flight, and one of its tool calls has no state in the action model and no result in the conversation: nothing will ever queue it, so its card can never settle (#1946) | each such call gets a cancelled result, through the ordinary cancel path: the results go back to the model when another result of the step finished, else the turn ends and "Agent stopped: ..." says why |
//! | [`TurnFault::ParkedActions`] | actions finished preprocessing with no batch ahead of them, and never became pending (#1859) | they are queued: their cards surface |
//! | [`TurnFault::BlockedQuestionWithoutCard`] | the front action is a question waiting for the user and no question card exists for it (the "Preparing question..." hang, #1797 family) | the question is cancelled as a stop, with a toast that says why |
//!
//! WHEN. Never on a timer. A check is scheduled by the events that end a step (a reply's stream
//! ended, an action finished, a stop) and runs as a main-loop hop after that event's own
//! handlers. A fault must be seen unchanged on [`CONFIRM_ROUNDS`] consecutive hops before it is
//! healed: every state the lifecycle passes through on its way (a reply's calls between the end
//! of its stream and their queueing, a batch between its preprocess future and the queue) is
//! held by a future or an event already posted, and is gone a hop later. What is still there
//! after the hops has nothing left to move it.
//!
//! KILL SWITCH. `PHOSPHOR_AGENT_TURN_SELF_HEAL=0` (also `off`, `false`, `no`): faults are still
//! logged (`hop=agent_turn_stuck ... healed=0`), nothing is healed.
//!
//! NOT COVERED, on purpose: "the turn is in progress and nothing is held". That shape is also a
//! new, empty conversation, a turn waiting for the network, a monitored command and a parked
//! send, and telling those apart by reading was not possible; the user's Stop is the way out of
//! it (`cancel_conversation_progress` ends such a turn).

use std::hash::{Hash, Hasher};

use crate::ai::agent::conversation::AIConversation;
use crate::ai::agent::{
    AIAgentAction, AIAgentActionId, AIAgentOutputStatus, FinishedAIAgentOutput,
};

use super::action_model::BlocklistAIActionModel;

/// How many consecutive main-loop hops a fault must be seen unchanged on before it is healed.
pub const CONFIRM_ROUNDS: u8 = 3;

/// The environment variable of the kill switch.
pub const SELF_HEAL_ENV: &str = "PHOSPHOR_AGENT_TURN_SELF_HEAL";

/// A turn that nothing will move again. `ids` is a hash of the action ids involved, so a fault
/// about other calls is a different fault and starts its own count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnFault {
    UnsettledCalls { count: usize, ids: u64 },
    ParkedActions { count: usize },
    BlockedQuestionWithoutCard { ids: u64 },
}

impl TurnFault {
    /// The name in the `hop=agent_turn_stuck fault=` log line.
    pub fn name(&self) -> &'static str {
        match self {
            TurnFault::UnsettledCalls { .. } => "unsettled_calls",
            TurnFault::ParkedActions { .. } => "parked_actions",
            TurnFault::BlockedQuestionWithoutCard { .. } => "blocked_question_without_card",
        }
    }
}

/// A stable hash of a set of action ids (order-independent).
pub fn ids_hash<'a>(ids: impl IntoIterator<Item = &'a AIAgentActionId>) -> u64 {
    let mut names: Vec<String> = ids.into_iter().map(ToString::to_string).collect();
    names.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    names.hash(&mut hasher);
    hasher.finish()
}

/// The facts the controller's check decides on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnFacts {
    /// A response stream of the conversation is in flight: its calls are not queued yet, or the
    /// calls of the reply before it are being answered.
    pub stream_in_flight: bool,
    /// The pane is closing: its close discards results on purpose.
    pub suspended_for_close: bool,
    /// Actions that finished preprocessing with no batch ahead and are not pending.
    pub parked: usize,
    /// Tool calls of the last cleanly finished reply with no action-model state and no result.
    pub unsettled: usize,
    /// [`ids_hash`] of those calls.
    pub unsettled_ids: u64,
}

/// The fault in `facts`, if any. Parked actions come first: queueing them is the lighter heal,
/// and a parked action has an action-model state, so it is never also unsettled.
pub fn judge(facts: &TurnFacts) -> Option<TurnFault> {
    if facts.suspended_for_close {
        return None;
    }
    if facts.parked > 0 {
        return Some(TurnFault::ParkedActions {
            count: facts.parked,
        });
    }
    if facts.unsettled > 0 && !facts.stream_in_flight {
        return Some(TurnFault::UnsettledCalls {
            count: facts.unsettled,
            ids: facts.unsettled_ids,
        });
    }
    None
}

/// What one check of a [`StuckWatch`] asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStep {
    /// No fault: nothing to do, nothing scheduled.
    Clear,
    /// A fault, not yet seen often enough: check again on the next hop.
    Recheck,
    /// The same fault on [`CONFIRM_ROUNDS`] consecutive hops: heal it.
    Confirmed(TurnFault),
}

/// Counts how many consecutive hops have shown the same fault, per conversation. Bounded: one
/// hop is pending at a time, and a fault is confirmed (and the count reset) after
/// [`CONFIRM_ROUNDS`] of them.
#[derive(Debug, Default)]
pub struct StuckWatch {
    fault: Option<TurnFault>,
    rounds: u8,
    hop_pending: bool,
    /// The last fault healed. A fault that is still there after its heal is logged once and
    /// left alone: healing it again would do the same nothing.
    healed: Option<TurnFault>,
}

impl StuckWatch {
    /// An event that may have ended a step. Returns whether a hop must be scheduled (`false`
    /// when one is already pending: it will see this event's outcome too).
    pub fn schedule(&mut self) -> bool {
        if self.hop_pending {
            return false;
        }
        self.hop_pending = true;
        true
    }

    /// A hop ran and found `fault`. On [`WatchStep::Recheck`] the caller schedules the next hop
    /// (the watch already counts it as pending).
    pub fn on_hop(&mut self, fault: Option<TurnFault>) -> WatchStep {
        self.hop_pending = false;
        let Some(fault) = fault else {
            self.fault = None;
            self.rounds = 0;
            self.healed = None;
            return WatchStep::Clear;
        };
        if self.healed == Some(fault) {
            // Healed once and unchanged: not counted again until something else is seen.
            return WatchStep::Clear;
        }
        if self.fault == Some(fault) {
            self.rounds = self.rounds.saturating_add(1);
        } else {
            self.fault = Some(fault);
            self.rounds = 1;
        }
        if self.rounds >= CONFIRM_ROUNDS {
            self.fault = None;
            self.rounds = 0;
            self.healed = Some(fault);
            WatchStep::Confirmed(fault)
        } else {
            self.hop_pending = true;
            WatchStep::Recheck
        }
    }
}

/// The kill switch's reading of its environment value: on unless it says off.
pub fn self_heal_enabled_from(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// Whether faults are healed (see [`SELF_HEAL_ENV`]). Read once.
pub fn self_heal_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| self_heal_enabled_from(std::env::var(SELF_HEAL_ENV).ok().as_deref()))
}

/// The tool calls of the conversation's last reply, when that reply finished cleanly, that have
/// neither an action-model state (preprocessing, queued, blocked, running, finished) nor a
/// result message in the conversation. The same reading as the fuzzer's `Rig::unsettled_calls`,
/// kept separate from the action model's own bookkeeping on purpose: a call the stream delivered
/// but nothing queued or answered shows up only here.
pub fn unsettled_actions(
    conversation: &AIConversation,
    action_model: &BlocklistAIActionModel,
) -> Vec<AIAgentAction> {
    let exchanges = conversation.all_exchanges();
    let Some(last) = exchanges.last() else {
        return Vec::new();
    };
    let AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Success { output },
    } = &last.output_status
    else {
        return Vec::new();
    };
    let calls: Vec<AIAgentAction> = {
        let output = output.get();
        output.actions().cloned().collect()
    };
    calls
        .into_iter()
        .filter(|action| action_model.get_action_status(&action.id).is_none())
        .filter(|action| {
            let wanted = action.id.to_string();
            !conversation.all_tasks().any(|task| {
                task.messages().any(|message| {
                    matches!(
                        &message.message,
                        Some(agent_proto::message::Message::ToolCallResult(result))
                            if result.tool_call_id == wanted
                    )
                })
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "turn_invariant_tests.rs"]
mod tests;
