//! The `BlocklistAIActionModel` is responsible for managing state related to `AIAgentAction`s
//! received in AI responses.
//!
//! Notably, this model manages the "action queue", which is used to support receiving multiple
//! actions in a single AI response.
//!
//! Actions are executed, one by one, either initiated by the user or auto-executed, if the user's
//! AI permissions permit. Action execution is handled by `BlocklistAIActionExecutor`, which
//! consumes the action to be executed and emits an event when execution is complete.
//!
//! Action state also has indirect implications for various parts of the terminal UI -- for
//! example, the input should be hidden if there is a pending AI requested command that requires
//! action from the user.

mod execute;
mod preprocess;

use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::agent::{
    AIAgentActionResultType, AIAgentActionType, AIAgentExchange, CancellationReason,
    CreateDocumentsResult, EditDocumentsResult, RequestCommandOutputResult,
};
use crate::ai::{
    agent::AIAgentInput,
    blocklist::action_model::execute::suggest_new_conversation::SuggestNewConversationExecutor,
};
use chrono::Local;
pub(crate) use execute::FileReadResult;
pub(crate) use execute::MalformedFinalLineProxyEvent;
pub(crate) use execute::apply_edits;
pub(crate) use execute::coerce_integer_args;
pub(crate) use execute::pty_write_for_target_command;
pub use execute::{
    EditAcceptAndContinueClickedEvent, EditAcceptClickedEvent, EditResolvedEvent, EditStats,
    NewConversationDecision, PromptSuggestionExecutor, PromptSuggestionExecutorEvent,
    ReadFileContextResult, RequestFileEditsExecutor, RequestFileEditsFormatKind,
    RequestFileEditsTelemetryEvent, ShellCommandExecutor, ShellCommandExecutorEvent,
    StartAgentExecutor, StartAgentExecutorEvent, StartAgentOutcome, StartAgentRequest,
    StartAgentRequestId, read_local_file_context,
};

use futures::future::{BoxFuture, join_all};
use preprocess::{PendingPreprocessedActions, PreprocessId};

use std::{
    cell::Cell,
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    rc::Weak,
    sync::Arc,
};

use crate::ai::agent::conversation::AIConversationId;
use itertools::Itertools;
use parking_lot::FairMutex;
use warpui::{AppContext, Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

use crate::{
    TelemetryEvent,
    ai::agent::{AIAgentAction, AIAgentActionId, AIAgentActionResult},
    send_telemetry_from_ctx,
    terminal::{
        TerminalModel, model::session::active_session::ActiveSession,
        model_events::ModelEventDispatcher,
    },
};

use self::execute::{
    ActionInitiator, BlocklistAIActionExecutor, BlocklistAIActionExecutorEvent, NotExecutedReason,
    RunningActionPhase, TryExecuteResult, ask_user_question::AskUserQuestionExecutor,
};

use super::BlocklistAIHistoryModel;
use super::inline_action::command_approval_preview::approval_requires_full_view;
use crate::ai::ai_document_view::DEFAULT_PLANNING_DOCUMENT_TITLE;
use crate::ai::document::ai_document_model::AIDocumentModel;

/// The status of an action from an AI output.
#[derive(Clone, Debug)]
pub enum AIActionStatus {
    /// The action is preprocessing and has yet to be started.
    Preprocessing,

    /// The action is queued, but isn't yet actionable by the user (there is another action that
    /// was queued prior that the user must act on first).
    Queued,

    // The action is next up for execution, but is blocked by the completion of another action
    // and/or user confirmation.
    Blocked,

    /// The action is running asynchronously.
    ///
    /// This is never the status for actions that are executed synchronously.
    RunningAsync,

    /// The action has either been cancelled or completed.
    Finished(Arc<AIAgentActionResult>),
}

impl AIActionStatus {
    /// Returns whether the action is currently preprocessing.
    pub fn is_preprocessing(&self) -> bool {
        matches!(self, AIActionStatus::Preprocessing)
    }

    pub fn is_queued(&self) -> bool {
        matches!(self, AIActionStatus::Queued)
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, AIActionStatus::Blocked)
    }

    pub fn is_done(&self) -> bool {
        matches!(self, AIActionStatus::Finished(..))
    }

    pub fn is_running(&self) -> bool {
        matches!(self, AIActionStatus::RunningAsync)
    }

    pub fn is_success(&self) -> bool {
        let AIActionStatus::Finished(result) = self else {
            return false;
        };
        result.result.is_successful()
    }

    pub fn is_failed(&self) -> bool {
        let AIActionStatus::Finished(result) = self else {
            return false;
        };
        result.result.is_failed()
    }

    pub fn is_cancelled(&self) -> bool {
        let AIActionStatus::Finished(result) = self else {
            return false;
        };
        result.result.is_cancelled()
    }

    pub fn is_cancelled_during_requested_command_execution(&self) -> bool {
        let AIActionStatus::Finished(result) = self else {
            return false;
        };
        result
            .result
            .is_cancelled_during_requested_command_execution()
    }

    pub fn finished_result(&self) -> Option<&AIAgentActionResult> {
        let AIActionStatus::Finished(result) = self else {
            return None;
        };
        Some(result.as_ref())
    }
}

#[derive(Debug, Clone)]
struct RunningActions {
    /// The execution phase for this batch of actions.
    phase: RunningActionPhase,

    /// The specific action IDs still running within the phase.
    /// If the phase is serial, there is only at most one action in here.
    /// For parallel phases, there can be several action IDs present at once,
    /// or there can be 0 or 1 actions; actions are added and removed as
    /// they are produced and completed, respectively.
    action_ids: Vec<AIAgentActionId>,
}

impl RunningActions {
    fn new(phase: RunningActionPhase, action_id: AIAgentActionId) -> Self {
        Self {
            phase,
            action_ids: vec![action_id],
        }
    }

    fn add_action(&mut self, action_id: AIAgentActionId) {
        self.action_ids.push(action_id);
    }

    fn remove_action(&mut self, action_id: &AIAgentActionId) {
        self.action_ids.retain(|id| id != action_id);
    }

    fn contains(&self, action_id: &AIAgentActionId) -> bool {
        self.action_ids.iter().any(|id| id == action_id)
    }

    fn first_action_id(&self) -> Option<&AIAgentActionId> {
        self.action_ids.first()
    }

    fn is_empty(&self) -> bool {
        self.action_ids.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartedAction {
    Sync,
    Async { phase: RunningActionPhase },
}

/// Whether a user-initiated accept of `action` must be refused (issue #847). This applies to a
/// `RequestCommandOutput` whose approval-card preview elides part of the command (a long run,
/// blank-line padding, too many lines), unless that exact text was reviewed in full. Every
/// other action, and any command shown whole, is unaffected. The GUI card records a review
/// (`mark_full_command_shown`) only when the user accepts after the full view has been on screen
/// for a moment and scrolled to its end. So a command that needs one can only be accepted
/// through the card. The AI block's Enter and the auto-execute toggle are routed through it; the
/// CLI subagent's accept is refused.
///
/// Applied in `start_pending_action_by_id` to `ActionInitiator::User` only. That one choke point
/// is reached by every user accept. The agent's own drain (`ActionInitiator::Agent`) is profile
/// policy, not approval of text on screen, so it is out of scope here.
fn hidden_command_blocks_user_accept(
    conversation_id: AIConversationId,
    action: &AIAgentAction,
    full_command_shown: &HashMap<(AIConversationId, AIAgentActionId), String>,
) -> bool {
    match &action.action {
        AIAgentActionType::RequestCommandOutput { command, .. } => approval_requires_full_view(
            command,
            is_shown_text(full_command_shown, conversation_id, &action.id, command),
        ),
        _ => false,
    }
}

/// Whether `command` is exactly the text recorded as shown for this action.
fn is_shown_text(
    full_command_shown: &HashMap<(AIConversationId, AIAgentActionId), String>,
    conversation_id: AIConversationId,
    action_id: &AIAgentActionId,
    command: &str,
) -> bool {
    full_command_shown
        .get(&(conversation_id, action_id.clone()))
        .is_some_and(|shown| shown == command)
}

/// The id of the action `BlocklistAIActionModel::execute_next_action_for_user` may run: the
/// front of the conversation's pending queue, or `None` if the queue is empty or its front
/// cannot be accepted without its own UI (`AIAgentActionType::can_be_accepted_without_its_own_ui`).
///
/// This is the choke point for every accept that does not know what it is accepting (the AI
/// block's Enter key, the CLI subagent view's accept and auto-approve). Running a blocked
/// `ask_user_question` from there executes it as the user with no answer written to
/// `AskUserQuestionExecutor`'s channel: its bare `recv()` waits forever, and the orphaned
/// receiver, which shares the channel with every later question in the view, can swallow the
/// next properly answered one. The question UIs write the answer first and then call
/// `execute_action` by id, which does not come through here.
///
/// Returns `None` rather than skipping to a later action: the ordinary drain never runs past a
/// blocked front action, and nothing queued after the question may run before it is answered.
fn next_action_acceptable_by_plain_accept(
    queue: Option<&VecDeque<AIAgentAction>>,
) -> Option<&AIAgentActionId> {
    queue
        .and_then(|queue| queue.front())
        .filter(|action| action.action.can_be_accepted_without_its_own_ui())
        .map(|action| &action.id)
}

/// Whether a user accept of `action` must be refused because it is an `ask_user_question` whose
/// answer has not been staged (#1797). `has_staged_answer`: the question card staged its answers
/// for this action (`AskUserQuestionExecutor::stage_answers`).
///
/// A question's confirmation IS its answer. The cards stage the answer and then execute the
/// action by id; anything else that executes it by id -- the generic Run / Enter accept
/// (`AIBlockAction::ExecuteRequestedAction`), a TUI generic tool view -- carries no answer, and
/// the executor would wait on a decision channel only a card's submit ever writes: the turn
/// hung on "Preparing question..." with the card hidden (it renders only while the action is
/// blocked, not while it runs). `execute_action` is the one place every by-id accept passes, so
/// the refusal lives there rather than at each caller. The question stays blocked and its card
/// stays up. A question with no answerable question never waits for an answer
/// (`AskUserQuestionExecutor::has_no_answerable_questions`): its execution is an error result,
/// so it is not refused.
fn user_accept_lacks_question_answer(action: &AIAgentActionType, has_staged_answer: bool) -> bool {
    matches!(action, AIAgentActionType::AskUserQuestion { .. })
        && !AskUserQuestionExecutor::has_no_answerable_questions(action)
        && !has_staged_answer
}

/// What [`BlocklistAIActionModel::cancel_all_pending_actions`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CancelledPendingActions {
    /// At least one action got its cancelled result in the call. Each one emits `FinishedAction`,
    /// so the controller's subscriber decides the turn's end.
    pub cancelled_any: bool,
    /// At least one batch was still preprocessing and was dropped.
    ///
    /// Since #2104 its calls got their cancelled result in the call, from the batch's kept
    /// copies, like every other cancelled action (so `cancelled_any` is true as well, and each
    /// emitted `FinishedAction`); what the batch's future brings back later is dropped.
    ///
    /// With the kill switch off ([`stop_settles_preprocessing_enabled`]) it is as before: the
    /// actions are held by the preprocess future, so they have no result yet
    /// (`record_stale_cancelled` records one, silently, if the future ever resolves) and no
    /// `FinishedAction` is emitted for them.
    pub dropped_preprocessing: bool,
}

/// Whether rejecting one pending action cancels every later pending action of the same turn
/// with it (#1799). True for the user's own reject (`Rejected`: every Reject button, the TUI
/// reject) and the user's stop on one action (`ManuallyCancelled`: the status bar's Ctrl-C /
/// Stop).
///
/// Why every later action, not only the ones that visibly depend on the rejected one:
/// * A turn's tool calls are planned together, on the assumption that the earlier ones run. The
///   only dependency the client can see is a command id (`transfer_shell_command_control_to_user`
///   / `write_to_long_running_shell_command` / `read_shell_command_output` on the command just
///   requested), but most are invisible: a rejected `cd`, checkout or file edit changes what
///   every later call means.
/// * A reject is the user saying "not this". Carrying on runs, or asks approval for, the rest of
///   a plan the user just turned down, and an action the GUI has no card for (a hand-over of a
///   command that never ran) sat pending, unseen, with follow-ups queued behind it forever.
/// * Nothing is lost: every cancelled call gets its cancelled result, and the batch goes to the
///   model -- at once after a Reject when an earlier call of the step had finished (#1942,
///   owner decision 2026-11-09), otherwise with the user's next message -- so the model sees
///   which calls did not run and can issue them again.
///
/// The queue only ever holds one turn: a stream's actions are queued together when it ends,
/// and a new request cancels whatever is still pending first.
/// #1967: whether `action_id` is a question waiting for the user in `queue`: the front of the
/// queue (only the front is ever blocked), with nothing of the conversation running (a running
/// action makes the front merely queued), in a pane that takes answers (a view-only pane never
/// blocks on its viewer).
fn front_is_waiting_question(
    queue: &VecDeque<AIAgentAction>,
    something_running: bool,
    is_view_only: bool,
    action_id: &AIAgentActionId,
) -> bool {
    !is_view_only
        && !something_running
        && queue.front().is_some_and(|front| {
            &front.id == action_id
                && matches!(front.action, AIAgentActionType::AskUserQuestion { .. })
        })
}

fn rejection_cancels_rest_of_turn(reason: CancellationReason) -> bool {
    matches!(
        reason,
        CancellationReason::ManuallyCancelled | CancellationReason::Rejected
    )
}

/// Kill switch for #2034: `PHOSPHOR_CTRL_C_INTERRUPTS_ACTION=0` restores what Ctrl-C and Stop did
/// with a running non-command action before: both cancelled that one action as a user stop and
/// left the later actions of the reply queued.
pub(crate) const CTRL_C_INTERRUPTS_ACTION_ENV: &str = "PHOSPHOR_CTRL_C_INTERRUPTS_ACTION";

/// Whether the #2034 rule is on: it is, unless the kill switch says `0`.
pub(crate) fn ctrl_c_interrupts_action_enabled() -> bool {
    ctrl_c_interrupts_action_enabled_from(
        std::env::var(CTRL_C_INTERRUPTS_ACTION_ENV).ok().as_deref(),
    )
}

/// [`ctrl_c_interrupts_action_enabled`] with the variable's value passed in (tests).
pub(crate) fn ctrl_c_interrupts_action_enabled_from(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

/// Kill switch for the #2103 retire: `PHOSPHOR_CLOSE_KEEPS_CALL_STATUS=0` (also `off`, `false`,
/// `no`) restores what a pane close did with the finished, unsent results of the reply it stops
/// before: it erased them, and their calls were left with no status at all.
pub(crate) const CLOSE_KEEPS_CALL_STATUS_ENV: &str = "PHOSPHOR_CLOSE_KEEPS_CALL_STATUS";

/// Whether a pane close keeps the status of the calls whose results it discards (#2103): it
/// does, unless the kill switch says off. Read at each close, not cached: a close is rare.
pub(crate) fn close_keeps_call_status_enabled() -> bool {
    close_keeps_call_status_enabled_from(std::env::var(CLOSE_KEEPS_CALL_STATUS_ENV).ok().as_deref())
}

/// [`close_keeps_call_status_enabled`] with the variable's value passed in (tests).
pub(crate) fn close_keeps_call_status_enabled_from(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// Kill switch for #2104: `PHOSPHOR_STOP_SETTLES_PREPROCESSING=0` (also `off`, `false`, `no`)
/// restores what a stop (and a new turn) did with a batch that was still preprocessing: its
/// calls got no result until the batch's future came back, if it ever did, and that late result
/// was recorded without a look at what the call already had.
pub(crate) const STOP_SETTLES_PREPROCESSING_ENV: &str = "PHOSPHOR_STOP_SETTLES_PREPROCESSING";

/// Whether the calls of a batch that is still preprocessing are settled when the batch is
/// dropped (#2104): they are, unless the kill switch says off. Read at each stop, each new turn
/// and each late preprocess result, not cached: all three are rare.
pub(crate) fn stop_settles_preprocessing_enabled() -> bool {
    stop_settles_preprocessing_enabled_from(
        std::env::var(STOP_SETTLES_PREPROCESSING_ENV)
            .ok()
            .as_deref(),
    )
}

/// [`stop_settles_preprocessing_enabled`] with the variable's value passed in (tests).
pub(crate) fn stop_settles_preprocessing_enabled_from(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// An action that is a command in the pane's shell, or drives one. A Ctrl-C for it belongs to
/// the pty (`TerminalView::interrupt_agent_command`, #850), never to
/// [`BlocklistAIActionModel::interrupt_running_actions`].
fn drives_a_shell_command(action: &AIAgentActionType) -> bool {
    matches!(
        action,
        AIAgentActionType::RequestCommandOutput { .. }
            | AIAgentActionType::WriteToLongRunningShellCommand { .. }
            | AIAgentActionType::ReadShellCommandOutput { .. }
            | AIAgentActionType::TransferShellCommandControlToUser { .. }
    )
}

/// Whether a Ctrl-C can interrupt the running actions as actions (#2034). `running`: the
/// executor's view of each action the model counts as running (`None`: the executor does not
/// hold it, so cancelling it there would do nothing). True only when something runs, the
/// executor holds all of it, and none of it is a shell command.
fn ctrl_c_can_interrupt_actions<'a>(
    running: impl IntoIterator<Item = Option<&'a AIAgentActionType>>,
) -> bool {
    let mut any = false;
    for action in running {
        match action {
            Some(action) if !drives_a_shell_command(action) => any = true,
            _ => return false,
        }
    }
    any
}

/// Returns whether another action may join the currently running phase.
///
/// Parallel phases only admit additional actions that classify into the same group and
/// can still be auto-executed. Serial phases always act as a barrier.
fn can_start_action_with_current_phase(
    current_phase: RunningActionPhase,
    next_phase: RunningActionPhase,
    can_autoexecute: bool,
) -> bool {
    match current_phase {
        RunningActionPhase::Serial => false,
        RunningActionPhase::Parallel(group) => {
            next_phase == RunningActionPhase::Parallel(group) && can_autoexecute
        }
    }
}

/// Where an action is, for the agent fuzzer ([`BlocklistAIActionModel::pending_snapshot_for_test`]).
#[cfg(any(test, feature = "test-util"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingStage {
    Preprocessing,
    /// Preprocessing is done but no Pending batch is ahead of it, yet it has not become
    /// pending or blocked: it should have in the same event cascade (#1859).
    Parked,
    /// Pending behind another action.
    Queued,
    /// The front of its conversation's queue with nothing running: waiting for the user (or the
    /// drain), so a card must be on screen if it needs the user.
    Blocked,
    Running,
}

#[cfg(any(test, feature = "test-util"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingActionSnapshot {
    pub conversation_id: AIConversationId,
    pub action_id: AIAgentActionId,
    /// `None` for actions already past the pending queue (their type is no longer held).
    pub kind: Option<&'static str>,
    pub stage: PendingStage,
}

/// The tool name of an action kind, or `None` for a kind not listed. One list for two users:
/// the `hop=` diagnostic lines (`hop_kind`, which says "other") and the agent fuzzer
/// ([`fuzz_action_kind`], which panics).
fn action_kind_name(action: &AIAgentActionType) -> Option<&'static str> {
    Some(match action {
        AIAgentActionType::RequestCommandOutput { .. } => "run_shell_command",
        AIAgentActionType::WriteToLongRunningShellCommand { .. } => {
            "write_to_long_running_shell_command"
        }
        AIAgentActionType::ReadShellCommandOutput { .. } => "read_shell_command_output",
        AIAgentActionType::ReadFiles(..) => "read_files",
        AIAgentActionType::Grep { .. } => "grep",
        AIAgentActionType::FileGlob { .. } | AIAgentActionType::FileGlobV2 { .. } => "file_glob",
        AIAgentActionType::AskUserQuestion { .. } => "ask_user_question",
        AIAgentActionType::TransferShellCommandControlToUser { .. } => {
            "transfer_shell_command_control_to_user"
        }
        _ => return None,
    })
}

/// The kind for the `hop=` lines: the tool's name, never any argument.
fn hop_kind(action: &AIAgentActionType) -> &'static str {
    action_kind_name(action).unwrap_or("other")
}

/// The fuzzer's name for an action kind. PANICS on a kind [`action_kind_name`] does not list, so
/// a new action kind fails the fuzz run loudly until it is classified, rather than being
/// silently treated as "needs no card".
#[cfg(any(test, feature = "test-util"))]
pub fn fuzz_action_kind(action: &AIAgentActionType) -> &'static str {
    action_kind_name(action).unwrap_or_else(|| {
        panic!("agent fuzzer: action kind {action:?} is not classified; add it to action_kind_name")
    })
}

pub struct BlocklistAIActionModel {
    executor: ModelHandle<BlocklistAIActionExecutor>,

    pending_preprocessed_actions: HashMap<AIConversationId, PendingPreprocessedActions>,

    /// Map from conversation ID to queue of pending [`AIAgentAction`]s.
    pending_actions: HashMap<AIConversationId, VecDeque<AIAgentAction>>,

    /// Map from conversation ID to the currently running action phase, if any.
    running_actions: HashMap<AIConversationId, RunningActions>,

    /// Map from conversation ID to actions received in the most recent AI output that are finished.
    finished_action_results: HashMap<AIConversationId, Vec<Arc<AIAgentActionResult>>>,

    /// Original order for the current batch of actions.
    ///
    /// We maintain this so that even though we might process actions in parallel,
    /// we can still order the results consistently.
    action_order: HashMap<AIConversationId, HashMap<AIAgentActionId, usize>>,

    /// Past actions and their corresponding statuses from previous AI exchanges.
    past_action_results: HashMap<AIAgentActionId, Arc<AIAgentActionResult>>,

    /// #2103: the finished results a pane close discarded (#848), kept so that each of their
    /// calls still has a status after the close: its card shows what the call did, and the
    /// turn watch (#1967) and the fuzz oracle (I1) do not read it as a call nothing holds.
    ///
    /// DISPLAY AND BOOKKEEPING ONLY. #848 discards these results so that a reply the user
    /// abandoned by closing its pane is never sent to the model later, and that still holds:
    /// nothing that builds a request reads this map. Requests take results from
    /// `finished_action_results` only (`drain_finished_action_results`,
    /// `finished_action_results_matching`); this map is read by [`Self::get_action_result`]
    /// (cards, status) and skipped by [`Self::get_action_result_for_export`] (the copy / export
    /// text, which can be attached to a request). It is kept apart from
    /// `past_action_results`, whose results did go to the model, for that reason. Never
    /// persisted: after a restart these calls have no status, as before.
    results_discarded_by_close: HashMap<AIAgentActionId, Arc<AIAgentActionResult>>,

    /// The actions whose own card the user rejected (`CancellationReason::Rejected`), as
    /// opposed to the later actions of the turn cancelled with them (#1799) or anything a Stop
    /// cancelled. `AIAgentActionResult` carries no reason, so this is how the controller tells
    /// the model "the user rejected this call" rather than a bare "cancelled" (#1942). One id
    /// per reject the user clicked; never cleared, the ids are unique for the process.
    rejected_by_user: HashSet<AIAgentActionId>,

    /// The ID of the terminal view this controller is associated with.
    terminal_view_id: EntityId,

    /// In view-only mode, we never block on user acceptance and avoid any interactive controls.
    /// This is used for agent session sharing to avoid any tools blocking on the viewer's acceptance.
    is_view_only: bool,

    /// The ID of the ambient agent task which owns this action model, if any.
    ambient_agent_task_id: Option<crate::ai::ambient_agents::AmbientAgentTaskId>,

    /// The exact command text the user chose to see in full on the approval card, per
    /// (conversation, action). A command whose card preview elides anything may not be accepted
    /// by the user unless the text that would run is the text recorded here -- see
    /// `hidden_command_blocks_user_accept` and issue #847.
    /// * Keyed by conversation as well, because action ids are only unique per conversation.
    /// * Holds the text, not a flag, so a mark for one text never covers another: an edit, or
    ///   a re-stream, after "Show full command" does not inherit it.
    ///
    /// This lives on the model, not the card, because the card is only one of the accept paths.
    /// The AI block's Enter key, the CLI subagent's accept and the auto-execute toggle all bypass
    /// the card.
    full_command_shown: HashMap<(AIConversationId, AIAgentActionId), String>,

    /// The edit-mode flag of each requested command's approval card, shared with the card. In
    /// edit mode the card shows the editor's uncommitted text, not the pending action's, so no
    /// user accept may run the pending text then (issue #847). The card's own accept leaves edit
    /// mode first. The flag is read live through a `Weak`. It is not a view lookup, which fails
    /// while the card is mid-update. A dropped card's flag no longer upgrades, so it cannot
    /// leave a command stuck.
    commands_being_edited: HashMap<(AIConversationId, AIAgentActionId), Weak<Cell<bool>>>,

    /// The (kind, last logged status) of each action not yet finished, per conversation (action
    /// ids are only unique within one), so the `hop=agent_action`
    /// line is written when an action's state CHANGES and never per frame.
    hop_state: HashMap<(AIConversationId, AIAgentActionId), (&'static str, &'static str)>,

    /// #2104: the calls whose batch was dropped while it was still preprocessing (a stop, a
    /// pane close's stop, a new turn) and that were settled THEN, from the batch's kept copies.
    /// The batch's future still holds the original actions; when it comes back, they are
    /// dropped here instead of getting a second result (`drop_late_preprocess_results`), and
    /// the entry goes. An entry whose future never comes back (a hung apply, #1894) stays: one
    /// id per such call. Keyed by conversation as well, because action ids are only unique
    /// within one.
    settled_while_preprocessing: HashSet<(AIConversationId, AIAgentActionId)>,

    /// Test-only: while `Some`, every batch queued waits on one more preprocess future, which
    /// resolves when its sender here is dropped ([`Self::release_held_preprocessing_for_test`]).
    /// This is a batch that is "still preprocessing" for as long as a test wants, and a hung
    /// apply (#1894) when the test never releases it.
    #[cfg(any(test, feature = "test-util"))]
    preprocess_hold_for_test: Option<Vec<futures::channel::oneshot::Sender<()>>>,
}

impl BlocklistAIActionModel {
    pub fn new(
        terminal_model: Arc<FairMutex<TerminalModel>>,
        active_session: ModelHandle<ActiveSession>,
        model_event_dispatcher: &ModelHandle<ModelEventDispatcher>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let executor = ctx.add_model(|ctx| {
            BlocklistAIActionExecutor::new(
                terminal_model,
                active_session.clone(),
                model_event_dispatcher,
                terminal_view_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&executor, move |me, event, ctx| match event {
            BlocklistAIActionExecutorEvent::ExecutingAction { action_id } => {
                ctx.emit(BlocklistAIActionEvent::ExecutingAction(action_id.clone()));
            }
            BlocklistAIActionExecutorEvent::FinishedAction {
                result,
                conversation_id,
                cancellation_reason,
            } => {
                me.handle_action_result(*conversation_id, result.clone(), *cancellation_reason, ctx)
            }
            BlocklistAIActionExecutorEvent::OpenCodeReview(id) => {
                ctx.emit(BlocklistAIActionEvent::ToggleCodeReview(id.clone()))
            }
            BlocklistAIActionExecutorEvent::InsertCodeReviewComments {
                action_id,
                repo_path,
                comments,
                base_branch,
            } => {
                ctx.emit(BlocklistAIActionEvent::InsertCodeReviewComments {
                    action_id: action_id.clone(),
                    repo_path: repo_path.clone(),
                    comments: comments.clone(),
                    base_branch: base_branch.clone(),
                });
            }
        });

        Self {
            pending_actions: Default::default(),
            finished_action_results: Default::default(),
            executor,
            past_action_results: HashMap::new(),
            results_discarded_by_close: HashMap::new(),
            rejected_by_user: HashSet::new(),
            running_actions: Default::default(),
            action_order: Default::default(),
            terminal_view_id,
            pending_preprocessed_actions: Default::default(),
            is_view_only: false,
            ambient_agent_task_id: None,
            full_command_shown: HashMap::new(),
            commands_being_edited: HashMap::new(),
            hop_state: HashMap::new(),
            settled_while_preprocessing: HashSet::new(),
            #[cfg(any(test, feature = "test-util"))]
            preprocess_hold_for_test: None,
        }
    }

    /// Records that the user saw exactly `command` in full on the approval card of requested
    /// command `action_id` in `conversation_id`. That lets a user accept it while it is still
    /// exactly that text (issue #847).
    pub fn mark_full_command_shown(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
        command: &str,
    ) {
        self.full_command_shown
            .insert((conversation_id, action_id.clone()), command.to_owned());
    }

    /// Withdraws [`Self::mark_full_command_shown`].
    pub fn clear_full_command_shown(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
    ) {
        self.full_command_shown
            .remove(&(conversation_id, action_id.clone()));
    }

    /// Whether the user saw exactly `command` in full for this action.
    pub fn is_full_command_shown(
        &self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
        command: &str,
    ) -> bool {
        is_shown_text(
            &self.full_command_shown,
            conversation_id,
            action_id,
            command,
        )
    }

    /// Whether a user's accept of `action` in `conversation_id` must be refused because its
    /// approval card elides part of the command and the user has not seen that exact text in
    /// full. See `hidden_command_blocks_user_accept`.
    pub fn user_accept_blocked_by_hidden_command(
        &self,
        conversation_id: AIConversationId,
        action: &AIAgentAction,
    ) -> bool {
        hidden_command_blocks_user_accept(conversation_id, action, &self.full_command_shown)
    }

    /// Shares the edit-mode flag of the approval card of `action_id` with the model. See
    /// `commands_being_edited`.
    pub fn register_command_edit_flag(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
        flag: Weak<Cell<bool>>,
    ) {
        self.commands_being_edited
            .insert((conversation_id, action_id.clone()), flag);
    }

    /// Whether a user accept of `action_id` must wait because its approval card is in edit mode.
    pub fn is_command_being_edited(
        &self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
    ) -> bool {
        self.commands_being_edited
            .get(&(conversation_id, action_id.clone()))
            .and_then(Weak::upgrade)
            .is_some_and(|editing| editing.get())
    }

    /// Replaces the text of pending requested command `action_id` with the user's edit. The card
    /// calls this whenever it commits edited text, so every accept path runs exactly what the
    /// card shows, not only the card's own Run (issue #847). No-op if the action is not pending.
    pub fn set_pending_command_text(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
        new_command: String,
    ) {
        if let Some(AIAgentActionType::RequestCommandOutput { command, .. }) = self
            .pending_actions
            .get_mut(&conversation_id)
            .and_then(|queue| queue.iter_mut().find(|action| &action.id == action_id))
            .map(|action| &mut action.action)
        {
            *command = new_command;
        }
    }

    /// Enable or disable view-only mode (for use in agent session sharing).
    pub fn set_view_only(&mut self, is_view_only: bool) {
        self.is_view_only = is_view_only;
    }

    /// Returns true if the action model is operating in view-only mode (used for shared-session viewers).
    pub fn is_view_only(&self) -> bool {
        self.is_view_only
    }

    pub fn shell_command_executor(&self, app: &AppContext) -> ModelHandle<ShellCommandExecutor> {
        self.executor.as_ref(app).shell_command_executor().clone()
    }

    pub fn suggest_new_conversation_executor(
        &self,
        app: &AppContext,
    ) -> ModelHandle<SuggestNewConversationExecutor> {
        self.executor
            .as_ref(app)
            .suggest_new_conversation_executor()
            .clone()
    }

    pub fn request_file_edits_executor(
        &self,
        app: &AppContext,
    ) -> ModelHandle<RequestFileEditsExecutor> {
        self.executor
            .as_ref(app)
            .request_file_edits_executor()
            .clone()
    }

    pub fn suggest_prompt_executor(
        &self,
        app: &AppContext,
    ) -> ModelHandle<PromptSuggestionExecutor> {
        self.executor.as_ref(app).suggest_prompt_executor().clone()
    }

    pub fn ask_user_question_executor(
        &self,
        app: &AppContext,
    ) -> ModelHandle<AskUserQuestionExecutor> {
        self.executor
            .as_ref(app)
            .ask_user_question_executor()
            .clone()
    }

    pub fn set_ambient_agent_task_id(
        &mut self,
        id: Option<crate::ai::ambient_agents::AmbientAgentTaskId>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.ambient_agent_task_id = id;
        self.executor.update(ctx, |executor, ctx| {
            executor.set_ambient_agent_task_id(id, ctx);
        });
    }

    /// Whether the shell-command action `action_id` is currently running for `conversation_id`.
    /// Used by prompt queueing to detect an in-flight agent-requested command whose snapshot has
    /// not yet fired, so a submitted prompt queues as `PendingLrcAutoQueue` instead of racing it.
    pub fn is_shell_command_action_pending(
        &self,
        action_id: &AIAgentActionId,
        conversation_id: AIConversationId,
    ) -> bool {
        self.running_actions
            .get(&conversation_id)
            .is_some_and(|r| r.contains(action_id))
    }

    fn blocked_action_for_conversation(
        &self,
        conversation_id: &AIConversationId,
    ) -> Option<&AIAgentAction> {
        if self.running_actions.contains_key(conversation_id) {
            return None;
        }

        self.pending_actions
            .get(conversation_id)
            .and_then(|queue| queue.front())
    }

    fn action_execution_phase(
        &self,
        conversation_id: AIConversationId,
    ) -> Option<RunningActionPhase> {
        self.running_actions
            .get(&conversation_id)
            .map(|running| running.phase)
    }

    fn add_running_action(
        &mut self,
        conversation_id: AIConversationId,
        action_id: AIAgentActionId,
        phase: RunningActionPhase,
    ) {
        match self.running_actions.entry(conversation_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                debug_assert_eq!(entry.get().phase, phase);
                entry.get_mut().add_action(action_id);
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(RunningActions::new(phase, action_id));
            }
        }
    }

    /// Writes `hop=agent_action id= kind= status=` when `action_id`'s state changed (queued,
    /// blocked, running, done, failed, cancelled). Ids, kinds and states only, never content.
    fn hop(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
        status: &'static str,
    ) {
        let key = (conversation_id, action_id.clone());
        let entry = self.hop_state.entry(key.clone()).or_insert(("other", ""));
        if entry.1 == status {
            return;
        }
        entry.1 = status;
        log::info!(
            "hop=agent_action id={action_id} kind={} status={status}",
            entry.0
        );
        if matches!(status, "done" | "failed" | "cancelled") {
            self.hop_state.remove(&key);
        }
    }

    fn try_to_execute_available_actions(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        loop {
            let Some(front_action) = self
                .pending_actions
                .get(&conversation_id)
                .and_then(|queue| queue.front())
                .cloned()
            else {
                return;
            };

            if let Some(current_phase) = self.action_execution_phase(conversation_id) {
                if !self.can_start_action_in_current_phase(
                    &front_action,
                    conversation_id,
                    current_phase,
                    ctx,
                ) {
                    return;
                }
            }

            let Some(result) = self.start_pending_action_by_id(
                &front_action.id,
                conversation_id,
                ActionInitiator::Agent,
                ctx,
            ) else {
                return;
            };

            if matches!(
                result,
                StartedAction::Async {
                    phase: RunningActionPhase::Serial
                }
            ) {
                return;
            }
        }
    }

    fn sort_finished_results(&mut self, conversation_id: AIConversationId) {
        if let Some(action_order) = self.action_order.get(&conversation_id) {
            if let Some(finished_results) = self.finished_action_results.get_mut(&conversation_id) {
                finished_results.sort_by_key(|result| {
                    action_order.get(&result.id).copied().unwrap_or(usize::MAX)
                });
            }
        }
    }

    /// Returns all pending actions for all conversations.
    pub fn get_pending_actions(&self) -> Vec<&AIAgentAction> {
        self.pending_actions
            .values()
            .flat_map(|queue| queue.iter())
            .collect()
    }

    /// Returns all pending actions for a specific conversation.
    pub fn get_pending_actions_for_conversation(
        &self,
        conversation_id: &AIConversationId,
    ) -> impl Iterator<Item = &AIAgentAction> + use<'_> {
        self.pending_actions
            .get(conversation_id)
            .into_iter()
            .flat_map(|queue| queue.iter())
    }

    /// Returns the next pending action
    pub fn get_pending_action(&self, app: &AppContext) -> Option<&AIAgentAction> {
        let conversation_id = self.active_conversation_id(app)?;
        self.blocked_action_for_conversation(&conversation_id)
    }

    /// Returns a pending action by its ID, searching across all conversations.
    pub fn get_pending_action_by_id(&self, action_id: &AIAgentActionId) -> Option<&AIAgentAction> {
        self.pending_actions
            .values()
            .flat_map(|queue| queue.iter())
            .find(|action| &action.id == action_id)
    }

    /// Returns the next pending or running action ID, for the active conversation, if any.
    pub fn get_pending_or_running_action_id<'a>(
        &'a self,
        app: &'a AppContext,
    ) -> Option<&'a AIAgentActionId> {
        let conversation_id = self.active_conversation_id(app)?;
        self.blocked_action_for_conversation(&conversation_id)
            .map(|action| &action.id)
            .or_else(|| {
                self.running_actions
                    .get(&conversation_id)
                    .and_then(RunningActions::first_action_id)
            })
    }

    /// Returns one of the currently asynchronously-executing actions, if any.
    ///
    /// When multiple actions run in parallel, only the first is returned. This is
    /// sufficient for callers that need a single status indicator (e.g., "Searching
    /// codebase...") or just need to know whether *something* is running.
    pub fn get_async_running_action<'a>(
        &'a self,
        app: &'a AppContext,
    ) -> Option<&'a AIAgentAction> {
        let conversation_id = self.active_conversation_id(app)?;
        self.running_actions
            .get(&conversation_id)
            .and_then(RunningActions::first_action_id)
            .and_then(|action_id| self.executor.as_ref(app).async_executing_action(action_id))
    }

    /// Returns whether there is a pending or running action for the active conversation.
    pub fn has_unfinished_actions(&self, app: &AppContext) -> bool {
        let Some(conversation_id) = self.active_conversation_id(app) else {
            return false;
        };
        self.has_unfinished_actions_for_conversation(conversation_id)
    }

    pub fn has_unfinished_actions_for_conversation(
        &self,
        conversation_id: AIConversationId,
    ) -> bool {
        let has_pending = self
            .pending_actions
            .get(&conversation_id)
            .is_some_and(|queue| !queue.is_empty());
        let has_running = self
            .running_actions
            .get(&conversation_id)
            .is_some_and(|running| !running.is_empty());
        has_pending || has_running
    }

    pub(super) fn has_unfinished_action_for_tool_call(
        &self,
        conversation_id: AIConversationId,
        task_id: &str,
        tool_call_id: &str,
        app: &AppContext,
    ) -> bool {
        // In the action model, `AIAgentActionId` is just tool_call_id; this only
        // explicitly renames the parameter so the call site (controller's byop
        // preflight) reads more intuitively.
        let action_id = crate::ai::agent::AIAgentActionId::from(tool_call_id.to_owned());
        let has_pending = self
            .pending_actions
            .get(&conversation_id)
            .is_some_and(|queue| {
                queue
                    .iter()
                    .any(|action| action.id == action_id && action.task_id.to_string() == task_id)
            });
        let has_running = self
            .running_actions
            .get(&conversation_id)
            .is_some_and(|running| running.contains(&action_id))
            && self
                .executor
                .as_ref(app)
                .async_executing_action(&action_id)
                .is_some_and(|action| {
                    action.id == action_id && action.task_id.to_string() == task_id
                });

        has_pending || has_running
    }

    /// Returns finished action results received from the most recent AI output for the active conversation.
    pub fn get_finished_action_results(
        &self,
        conversation_id: AIConversationId,
    ) -> Option<&Vec<Arc<AIAgentActionResult>>> {
        self.finished_action_results.get(&conversation_id)
    }

    /// Returns the `AIActionStatus` for the action corresponding to the given `id`, if any.
    pub fn get_action_status(&self, id: &AIAgentActionId) -> Option<AIActionStatus> {
        for (conversation_id, pending_actions_for_conversation) in &self.pending_actions {
            for (index, action) in pending_actions_for_conversation.iter().enumerate() {
                if &action.id != id {
                    continue;
                }

                if index == 0
                    && !self.is_view_only
                    && !self.running_actions.contains_key(conversation_id)
                {
                    return Some(AIActionStatus::Blocked);
                }

                return Some(AIActionStatus::Queued);
            }
        }

        self.running_actions
            .values()
            .find(|running| running.contains(id))
            .map(|_| AIActionStatus::RunningAsync)
            .or_else(|| {
                self.get_action_result(id)
                    .map(|result| AIActionStatus::Finished(result.clone()))
            })
            .or_else(|| {
                self.pending_preprocessed_actions
                    .values()
                    .any(|preprocessing| preprocessing.contains(id))
                    .then_some(AIActionStatus::Preprocessing)
            })
    }

    /// The result of the action `id`, for its card and its status. Includes a result a pane
    /// close discarded (#2103, `results_discarded_by_close`), which the model never received:
    /// anything that may end up in a request uses [`Self::get_action_result_for_export`].
    pub fn get_action_result(&self, id: &AIAgentActionId) -> Option<&Arc<AIAgentActionResult>> {
        self.get_action_result_for_export(id)
            .or_else(|| self.results_discarded_by_close.get(id))
    }

    /// [`Self::get_action_result`] without the results a pane close discarded (#848, #2103):
    /// for text that leaves the pane (copy, export, a conversation attached to a prompt). Such a
    /// text can be sent to the model, and a discarded result must never be.
    pub fn get_action_result_for_export(
        &self,
        id: &AIAgentActionId,
    ) -> Option<&Arc<AIAgentActionResult>> {
        // Search through all conversations' finished action results
        self.finished_action_results
            .values()
            .flat_map(|results| results.iter())
            .find(|result| &result.id == id)
            .or_else(|| self.past_action_results.get(id))
    }

    /// Bulk restore action results from a list of exchanges (used when loading conversations from tasks)
    pub fn restore_action_results_from_exchanges(&mut self, exchanges: Vec<&AIAgentExchange>) {
        for exchange in exchanges.iter() {
            for input in &exchange.input {
                if let AIAgentInput::ActionResult { result, .. } = input {
                    let result_id = result.id.clone();
                    let mut result_to_insert = result.clone();
                    if let AIAgentActionResultType::RequestCommandOutput(
                        RequestCommandOutputResult::LongRunningCommandSnapshot { .. },
                    ) = &result.result
                    {
                        // On restoration we set long running command snapshot results to cancelled,
                        // since this means the command was incomplete when the app was closed.
                        result_to_insert.result = AIAgentActionResultType::RequestCommandOutput(
                            RequestCommandOutputResult::CancelledBeforeExecution,
                        );
                    }
                    self.past_action_results
                        .insert(result_id, Arc::new(result_to_insert));
                }
            }
        }
    }

    /// Attempts to execute the next pending action for the active conversation.
    ///
    /// This is the "accept whatever is blocked" path: the AI block's Enter key
    /// (`AIBlockAction::ExecuteNextPendingAction`, bound whenever any action is pending) and the
    /// CLI subagent view's accept / auto-approve. None of them knows what it is accepting, so
    /// this refuses a front action that a plain accept cannot confirm -- see
    /// `next_action_acceptable_by_plain_accept`. The action stays pending; it can still be
    /// answered through its own UI (which calls `execute_action` by id) or rejected.
    pub fn execute_next_action_for_user(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(pending_action_id) =
            next_action_acceptable_by_plain_accept(self.pending_actions.get(&conversation_id))
                .cloned()
        else {
            return;
        };

        if self
            .start_pending_action_by_id(
                &pending_action_id,
                conversation_id,
                ActionInitiator::User,
                ctx,
            )
            .is_some_and(|result| matches!(result, StartedAction::Sync))
        {
            self.try_to_execute_available_actions(conversation_id, ctx);
        }
    }

    /// Attempts to execute the pending action with the given `action_id` for the given conversation.
    ///
    /// This is the **user clicked Accept** entry point, and every caller must be one: the
    /// action runs with `ActionInitiator::User`, which stands in for the confirmation prompt
    /// on any action, computer use included. Callers are the confirmation UIs in `block.rs`
    /// and `inline_action/ask_user_question_view.rs`.
    ///
    /// An `ask_user_question` is accepted only with its answer staged by its question card;
    /// any other accept of one is refused here (`user_accept_lacks_question_answer`, #1797).
    ///
    /// A caller that is *not* a click does not belong here. The fork once had one: the LRC
    /// tag-in auto-accept, which ran the agent's first round as a forged Accept and so
    /// overrode an `Always ask` profile (#617, then removed entirely in #1070).
    pub fn execute_action(
        &mut self,
        action_id: &AIAgentActionId,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        if self
            .start_pending_action_by_id(action_id, conversation_id, ActionInitiator::User, ctx)
            .is_some_and(|result| matches!(result, StartedAction::Sync))
        {
            self.try_to_execute_available_actions(conversation_id, ctx);
        }
    }

    /// Gets the active conversation ID for this terminal view.
    fn active_conversation_id(&self, app: &AppContext) -> Option<AIConversationId> {
        BlocklistAIHistoryModel::as_ref(app).active_conversation_id(self.terminal_view_id)
    }

    fn update_conversation_in_progress_status(
        &self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
            history_model.update_conversation_status(
                self.terminal_view_id,
                conversation_id,
                ConversationStatus::InProgress,
                ctx,
            );
        });
    }

    fn handle_not_executed_action(
        &self,
        action: &AIAgentAction,
        reason: NotExecutedReason,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        if reason.needs_confirmation() {
            ctx.emit(BlocklistAIActionEvent::ActionBlockedOnUserConfirmation(
                action.id.clone(),
            ));
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                let blocked_action_user_friendly_str = action.action.user_friendly_name();
                history_model.update_conversation_status(
                    self.terminal_view_id,
                    conversation_id,
                    ConversationStatus::Blocked {
                        blocked_action: format!("{blocked_action_user_friendly_str:?}"),
                    },
                    ctx,
                );
            });
        }
    }

    fn action_phase_for_action(
        &self,
        action: &AIAgentAction,
        ctx: &ModelContext<Self>,
    ) -> RunningActionPhase {
        self.executor.as_ref(ctx).action_phase(action, ctx)
    }

    fn can_start_action_in_current_phase(
        &self,
        action: &AIAgentAction,
        conversation_id: AIConversationId,
        current_phase: RunningActionPhase,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        // Recompute the candidate action's phase on demand so executor-side capability checks
        // (for example, whether the active session can run shell commands in parallel) are applied
        // using the latest runtime state.
        let next_phase = self.action_phase_for_action(action, ctx);
        let can_autoexecute = self.executor.update(ctx, |executor, ctx| {
            executor.can_autoexecute_action(action, conversation_id, ctx)
        });
        can_start_action_with_current_phase(current_phase, next_phase, can_autoexecute)
    }

    fn start_pending_action_by_id(
        &mut self,
        action_id: &AIAgentActionId,
        conversation_id: AIConversationId,
        initiator: ActionInitiator,
        ctx: &mut ModelContext<Self>,
    ) -> Option<StartedAction> {
        if initiator.is_out_of_band() && self.running_actions.contains_key(&conversation_id) {
            // User-driven approvals still execute one action at a time so that interactive
            // confirmations do not overlap in the UI.
            return None;
        }

        let idx = self
            .pending_actions
            .get(&conversation_id)
            .and_then(|queue| queue.iter().position(|action| &action.id == action_id))?;

        if initiator == ActionInitiator::User
            && self.is_command_being_edited(conversation_id, action_id)
        {
            // The card shows uncommitted edits; the pending text is not what is on screen.
            log::info!(
                "Refusing user accept of requested command {action_id:?}: its approval card is \
                 in edit mode"
            );
            return None;
        }

        let has_staged_answer = self
            .executor
            .as_ref(ctx)
            .ask_user_question_executor()
            .as_ref(ctx)
            .has_staged_answers(action_id);
        if initiator == ActionInitiator::User
            && self
                .pending_actions
                .get(&conversation_id)
                .and_then(|queue| queue.get(idx))
                .is_some_and(|action| {
                    user_accept_lacks_question_answer(&action.action, has_staged_answer)
                })
        {
            // An accept with no answer: the question stays blocked and its card stays up.
            log::info!(
                "Refusing user accept of ask_user_question {action_id:?}: no answer was \
                 submitted on its question card (#1797)"
            );
            return None;
        }

        if initiator == ActionInitiator::User
            && self
                .pending_actions
                .get(&conversation_id)
                .and_then(|queue| queue.get(idx))
                .is_some_and(|action| {
                    hidden_command_blocks_user_accept(
                        conversation_id,
                        action,
                        &self.full_command_shown,
                    )
                })
        {
            // Part of this command is elided on its approval card and the user has not asked to
            // see all of it, so no accept can stand for approval of that text (issue #847). It
            // stays pending; the card offers "Show full command".
            log::info!(
                "Refusing user accept of requested command {action_id:?}: part of it is hidden \
                 on its approval card"
            );
            return None;
        }

        let action = self
            .pending_actions
            .get_mut(&conversation_id)?
            .remove(idx)?;

        let action_id = action.id.clone();
        let phase = self.action_phase_for_action(&action, ctx);
        log::info!(
            "[byop-diag] try_to_execute_action: enter action_id={action_id:?} \
             initiator={initiator:?} phase={phase:?}"
        );
        let execute_result = self.executor.update(ctx, |executor, ctx| {
            executor.try_to_execute_action(action, conversation_id, initiator, ctx)
        });

        match execute_result {
            TryExecuteResult::ExecutedAsync => {
                log::info!(
                    "[byop-diag] try_to_execute_action: ExecutedAsync action_id={action_id:?}"
                );
                self.update_conversation_in_progress_status(conversation_id, ctx);
                self.hop(conversation_id, &action_id, "running");
                self.add_running_action(conversation_id, action_id, phase);
                Some(StartedAction::Async { phase })
            }
            TryExecuteResult::ExecutedSync => {
                log::info!(
                    "[byop-diag] try_to_execute_action: ExecutedSync action_id={action_id:?}"
                );
                self.update_conversation_in_progress_status(conversation_id, ctx);
                Some(StartedAction::Sync)
            }
            TryExecuteResult::NotExecuted { reason, action } => {
                log::info!(
                    "[byop-diag] try_to_execute_action: NotExecuted action_id={:?} reason={:?} \
                     → entering pending_actions[{:?}]",
                    action.id,
                    reason,
                    conversation_id
                );
                self.pending_actions
                    .entry(conversation_id)
                    .or_default()
                    .insert(idx, (*action).clone());
                self.hop(conversation_id, &action.id, "blocked");
                self.handle_not_executed_action(action.as_ref(), reason, conversation_id, ctx);
                None
            }
        }
    }

    fn preprocess_action(
        &mut self,
        action: &AIAgentAction,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        self.executor.update(ctx, |executor, ctx| {
            executor.preprocess_action(action, conversation_id, ctx)
        })
    }

    /// Test-only: records `result` as a finished action result of `conversation_id`, as if an
    /// earlier action of the current batch had already completed.
    #[cfg(test)]
    pub(crate) fn push_finished_action_result_for_test(
        &mut self,
        conversation_id: AIConversationId,
        result: AIAgentActionResult,
    ) {
        self.finished_action_results
            .entry(conversation_id)
            .or_default()
            .push(Arc::new(result));
    }

    /// Test-only (agent fuzzer, invariant I1): every action this model is holding, with the stage
    /// it is in. Nothing that is pending, preprocessing or running is left out, so the oracle can
    /// check that each one is runnable, running, or blocked behind a card.
    ///
    /// Panics on an action kind [`fuzz_action_kind`] does not classify: a new kind must be given a
    /// card verdict before the fuzzer can say anything about it.
    #[cfg(any(test, feature = "test-util"))]
    pub fn pending_snapshot_for_test(&self) -> Vec<PendingActionSnapshot> {
        let mut out = Vec::new();
        for (conversation_id, queue) in &self.pending_actions {
            for (index, action) in queue.iter().enumerate() {
                let front_and_idle = index == 0
                    && !self.is_view_only
                    && !self.running_actions.contains_key(conversation_id);
                out.push(PendingActionSnapshot {
                    conversation_id: *conversation_id,
                    action_id: action.id.clone(),
                    kind: Some(fuzz_action_kind(&action.action)),
                    stage: if front_and_idle {
                        PendingStage::Blocked
                    } else {
                        PendingStage::Queued
                    },
                });
            }
        }
        for (conversation_id, running) in &self.running_actions {
            for action_id in &running.action_ids {
                out.push(PendingActionSnapshot {
                    conversation_id: *conversation_id,
                    action_id: action_id.clone(),
                    kind: None,
                    stage: PendingStage::Running,
                });
            }
        }
        for (conversation_id, preprocessing) in &self.pending_preprocessed_actions {
            for (action_id, parked_with_nothing_ahead) in preprocessing.action_ids_for_test() {
                out.push(PendingActionSnapshot {
                    conversation_id: *conversation_id,
                    action_id,
                    kind: None,
                    stage: if parked_with_nothing_ahead {
                        PendingStage::Parked
                    } else {
                        PendingStage::Preprocessing
                    },
                });
            }
        }
        out
    }

    /// Test-only: queues a single action through the normal dispatch pipeline so
    /// confirmation-requiring actions surface as a pending permission prompt.
    /// Exposed for the `warp_tui` test suite via `tui_test_support`.
    #[cfg(any(test, feature = "test-util"))]
    #[allow(dead_code)] // used by warp_tui tests via tui_test_support
    pub(crate) fn queue_action_for_test(
        &mut self,
        action: AIAgentAction,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.queue_actions_without_abandoning(vec![action], conversation_id, ctx);
    }

    /// Test-only: queues `actions` as ONE turn's batch through the normal dispatch pipeline, as
    /// the controller does when a stream ends with several tool calls.
    #[cfg(test)]
    pub(crate) fn queue_actions_for_test(
        &mut self,
        actions: Vec<AIAgentAction>,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.queue_actions_without_abandoning(actions, conversation_id, ctx);
    }

    /// Test-only: moves `action_id` from pending to running for `conversation_id`, so
    /// `get_action_status` reports `RunningAsync` without spawning anything.
    #[cfg(test)]
    pub(crate) fn mark_action_running_for_test(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
    ) {
        if let Some(pending) = self.pending_actions.get_mut(&conversation_id) {
            pending.retain(|action| &action.id != action_id);
        }
        self.running_actions.insert(
            conversation_id,
            RunningActions::new(RunningActionPhase::Serial, action_id.clone()),
        );
    }

    /// Test-only: registers `action` as a really running async action of `conversation_id` --
    /// in the executor's `async_executing_actions` (so cancelling it goes through
    /// `cancel_running_async_action` and the shell executor's `cancel_execution`) as well as in
    /// `running_actions` -- without spawning its execution.
    #[cfg(test)]
    pub(crate) fn start_running_async_action_for_test(
        &mut self,
        action: AIAgentAction,
        conversation_id: AIConversationId,
        execution: Option<warpui::r#async::SpawnedFutureHandle>,
        ctx: &mut ModelContext<Self>,
    ) {
        let action_id = action.id.clone();
        self.executor.update(ctx, |executor, _| {
            executor.insert_async_executing_action_for_test(action, conversation_id, execution);
        });
        self.mark_action_running_for_test(conversation_id, &action_id);
    }

    /// Test-only (#2104; the agent fuzzer's `env.preprocess`): from now on every batch queued
    /// stays in preprocessing until [`Self::release_held_preprocessing_for_test`] is called
    /// (`hold = true`), or batches preprocess as usual again (`hold = false`, which also
    /// releases what is held).
    #[cfg(any(test, feature = "test-util"))]
    pub fn set_preprocess_hold_for_test(&mut self, hold: bool) {
        self.preprocess_hold_for_test = hold.then(Vec::new);
    }

    /// Test-only: lets every batch held by [`Self::set_preprocess_hold_for_test`] finish
    /// preprocessing; each batch's result arrives on a later turn of the main loop, as a real
    /// preprocess future's does. Batches queued afterwards are held again. Returns how many
    /// batches were released.
    #[cfg(any(test, feature = "test-util"))]
    pub fn release_held_preprocessing_for_test(&mut self) -> usize {
        self.preprocess_hold_for_test
            .as_mut()
            .map_or(0, |held| held.drain(..).count())
    }

    /// Test-only: whether `action_id` of `conversation_id` was settled when its batch was
    /// dropped while preprocessing and the batch's future has not come back yet (#2104).
    #[cfg(test)]
    pub(crate) fn awaits_late_preprocess_result_for_test(
        &self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
    ) -> bool {
        self.settled_while_preprocessing
            .contains(&(conversation_id, action_id.clone()))
    }

    /// Queues the `actions` in the given iterator for the given conversation,
    /// to be dispatched in the order in which they appear in the iterator.
    pub(super) fn queue_actions(
        &mut self,
        actions: Vec<AIAgentAction>,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        // A new turn: earlier turns' unresolved preprocess batches can no longer legitimately
        // produce cards, and would park this turn's batch behind them forever (#1859).
        if let Some(batches) = self.pending_preprocessed_actions.get_mut(&conversation_id) {
            let abandoned = batches.abandon_pending();
            if !abandoned.is_empty() {
                log::warn!(
                    "hop=agent_action status=parked abandoned unresolved preprocess batches ids={abandoned:?} at new turn"
                );
                // Owner decision 2026-11-09: leftovers of an earlier turn that had finished
                // preprocessing but were parked behind an abandoned batch are CANCELLED, never
                // executed. Nothing from an old turn runs on its own.
                let stale = batches.release_ready();
                // #2104: the calls of the batches just given up on are settled now as well,
                // from the batches' kept copies, not when (or if) their futures come back:
                // one rule for every dropped batch, and no result is recorded later behind
                // the back of whatever the pane is doing then (a close, a later turn).
                let still_preprocessing = if stop_settles_preprocessing_enabled() {
                    batches.take_actions_of_abandoned()
                } else {
                    Vec::new()
                };
                self.record_stale_cancelled(conversation_id, stale);
                let still_preprocessing =
                    self.mark_settled_while_preprocessing(conversation_id, still_preprocessing);
                self.record_stale_cancelled(conversation_id, still_preprocessing);
                // #1894: a hung apply of an abandoned batch is cancelled too, or it stays hung
                // and its cancel sender leaks until a conversation cancel.
                self.executor.update(ctx, |executor, ctx| {
                    executor.cancel_preprocessing(conversation_id, ctx)
                });
            }
        }
        self.queue_actions_without_abandoning(actions, conversation_id, ctx);
    }

    /// `queue_actions` minus the new-turn abandonment: tests that queue several single-action
    /// batches back to back model batches of ONE turn.
    fn queue_actions_without_abandoning(
        &mut self,
        actions: Vec<AIAgentAction>,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.action_order.insert(
            conversation_id,
            actions
                .iter()
                .enumerate()
                .map(|(index, action)| (action.id.clone(), index))
                .collect(),
        );
        let mut preprocess_future: Vec<BoxFuture<'static, ()>> =
            Vec::with_capacity(actions.len() + 1);

        for action in actions.iter() {
            preprocess_future.push(self.preprocess_action(action, conversation_id, ctx));
        }
        #[cfg(any(test, feature = "test-util"))]
        {
            if let Some(held) = self.preprocess_hold_for_test.as_mut() {
                let (release, released) = futures::channel::oneshot::channel::<()>();
                held.push(release);
                preprocess_future.push(Box::pin(async move {
                    // Resolves when the sender is dropped: a release, or the model going away.
                    let _ = released.await;
                }));
            }
        }

        // #2104: the batch keeps a copy of each action. The originals go with the future
        // below; the copies are what a stop cancels while that future is still out.
        let preprocess_id = self
            .pending_preprocessed_actions
            .entry(conversation_id)
            .or_default()
            .insert_preprocess_actions(&actions);

        ctx.spawn(join_all(preprocess_future), move |me, _, ctx| {
            me.handle_preprocess_actions_results(conversation_id, preprocess_id, actions, ctx);
        });
    }

    /// Installs a front-of-queue confirmation action without preprocessing.
    ///
    /// Ported from the pin (`02b53fcd8`). The fork had never ported this, so
    /// `queue_tui_permission_action` fell back to `queue_action_for_test`, which
    /// routes through `ctx.spawn`'d preprocessing -- the action's status is still
    /// `None` when a test asserts immediately after queueing. That silently broke
    /// four ported `tui_ask_question_view` tests: with no blocked status,
    /// `is_awaiting_answers` is false, so `on_focus` never delegates to the option
    /// selector and every focus/keystroke assertion fails.
    #[cfg(all(feature = "tui", any(test, feature = "test-util")))]
    pub fn queue_confirmation_action(
        &mut self,
        action: AIAgentAction,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let action_id = action.id.clone();
        self.pending_actions
            .entry(conversation_id)
            .or_default()
            .push_back(action);
        ctx.emit(BlocklistAIActionEvent::QueuedAction(action_id.clone()));
        ctx.emit(BlocklistAIActionEvent::ActionBlockedOnUserConfirmation(
            action_id,
        ));
    }

    /// #1967: how many of `conversation_id`'s actions finished preprocessing with nothing ahead
    /// of them and still are not pending (`PendingPreprocessedActions::parked_count`).
    pub(super) fn parked_action_count(&self, conversation_id: AIConversationId) -> usize {
        self.pending_preprocessed_actions
            .get(&conversation_id)
            .map_or(0, |batches| batches.parked_count())
    }

    /// #1967 self-heal: queues the actions [`Self::parked_action_count`] counts, as the
    /// completing batch's own handler would have, so their cards surface and the turn goes on.
    /// Returns how many were released.
    pub(super) fn release_parked_actions(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> usize {
        let ready = self
            .pending_preprocessed_actions
            .get_mut(&conversation_id)
            .map(|batches| batches.release_ready())
            .unwrap_or_default();
        let released = ready.len();
        if released == 0 {
            return 0;
        }
        log::warn!(
            "hop=agent_action status=parked released {released} action(s) that finished \
             preprocessing with nothing ahead and were never queued (#1967)"
        );
        self.enqueue_preprocessed_actions(conversation_id, ready, ctx);
        self.try_to_execute_available_actions(conversation_id, ctx);
        released
    }

    /// #1967: the conversation in which `action_id` is a question waiting for the user: the
    /// front of its queue with nothing running, in a pane that takes answers. `None` for any
    /// other action or state.
    pub fn blocked_question_conversation(
        &self,
        action_id: &AIAgentActionId,
    ) -> Option<AIConversationId> {
        self.pending_actions
            .iter()
            .find(|(conversation_id, queue)| {
                front_is_waiting_question(
                    queue,
                    self.running_actions.contains_key(*conversation_id),
                    self.is_view_only,
                    action_id,
                )
            })
            .map(|(conversation_id, _)| *conversation_id)
    }

    /// #2104: whether `action_id` already has a finished, unsent result in `conversation_id`.
    fn has_finished_result(
        &self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
    ) -> bool {
        self.finished_action_results
            .get(&conversation_id)
            .is_some_and(|results| results.iter().any(|result| &result.id == action_id))
    }

    /// #2104: `actions` are the kept copies of calls whose batch was just dropped while it was
    /// still preprocessing. Notes every one of them, so that what the batch's future brings
    /// back later is dropped ([`Self::drop_late_preprocess_results`]), and returns the ones
    /// that still owe the conversation a result: all but a call that already has one (in a
    /// shared session the sharer's result can arrive while the viewer still preprocesses).
    fn mark_settled_while_preprocessing(
        &mut self,
        conversation_id: AIConversationId,
        actions: Vec<AIAgentAction>,
    ) -> Vec<AIAgentAction> {
        let mut owed = Vec::with_capacity(actions.len());
        for action in actions {
            self.settled_while_preprocessing
                .insert((conversation_id, action.id.clone()));
            if !self.has_finished_result(conversation_id, &action.id) {
                owed.push(action);
            }
        }
        owed
    }

    /// #2104: `stale` are the actions a preprocess future brought back for a batch that is
    /// gone or was given up on. Takes out the ones that must NOT get a result now and returns
    /// the rest:
    /// * a call settled when its batch was dropped (`settled_while_preprocessing`): it has had
    ///   its one cancelled result, which may have been sent, or discarded by a pane close,
    ///   since. A second result here would be a duplicate in the next request, and, arriving
    ///   while the pane is closed, a result the close never saw and never discarded (#848);
    /// * a call that already has a finished, unsent result for any other reason.
    ///
    /// Whatever per-action state the late preprocess left in the executor (a diff that failed
    /// to apply because the stop cancelled it) is released, as it is for every finished
    /// action. Logged once per late batch. With the kill switch off nothing is taken out.
    fn drop_late_preprocess_results(
        &mut self,
        conversation_id: AIConversationId,
        stale: Vec<AIAgentAction>,
        ctx: &mut ModelContext<Self>,
    ) -> Vec<AIAgentAction> {
        if stale.is_empty() {
            return stale;
        }
        let guard_results = stop_settles_preprocessing_enabled();
        let mut owed = Vec::with_capacity(stale.len());
        let mut dropped = Vec::new();
        for action in stale {
            let settled = self
                .settled_while_preprocessing
                .remove(&(conversation_id, action.id.clone()));
            if settled || (guard_results && self.has_finished_result(conversation_id, &action.id)) {
                dropped.push(action.id);
            } else {
                owed.push(action);
            }
        }
        if !dropped.is_empty() {
            // A count only, never an action's arguments.
            log::info!(
                "hop=agent_late_preprocess_result dropped={} recorded={} \
                 conversation={conversation_id:?}: the batch was dropped while it was still \
                 preprocessing and its calls were settled then (#2104)",
                dropped.len(),
                owed.len()
            );
            self.executor.update(ctx, |executor, ctx| {
                for action_id in &dropped {
                    executor.discard_action_state(action_id, ctx);
                }
            });
        }
        owed
    }

    /// Gives stale earlier-turn actions a `cancelled` result without executing them. The result
    /// is recorded directly (no `FinishedAction` event, no status write): the new turn is
    /// starting, and these results ride on the next request like any other cancelled call.
    ///
    /// Callers: a new turn (`queue_actions`), for the earlier turn's parked actions and, since
    /// #2104, the calls of its batches that were still preprocessing; and a late preprocess
    /// result (`handle_preprocess_actions_results`) for what
    /// [`Self::drop_late_preprocess_results`] left, which since #2104 is nothing unless the
    /// kill switch is off.
    fn record_stale_cancelled(
        &mut self,
        conversation_id: AIConversationId,
        stale: Vec<AIAgentAction>,
    ) {
        for action in stale {
            log::warn!(
                "hop=agent_action id={} status=cancelled earlier-turn action parked at a new turn",
                action.id
            );
            let result = Arc::new(AIAgentActionResult {
                id: action.id.clone(),
                task_id: action.task_id.clone(),
                result: action.action.cancelled_result(),
            });
            self.hop_state.remove(&(conversation_id, action.id.clone()));
            self.finished_action_results
                .entry(conversation_id)
                .or_default()
                .push(result);
        }
    }

    /// Moves preprocessed actions into the pending queue (skipping ones that already have
    /// results) and emits `QueuedAction` for each. Does not start execution.
    fn enqueue_preprocessed_actions(
        &mut self,
        conversation_id: AIConversationId,
        actions_to_enqueue: Vec<AIAgentAction>,
        ctx: &mut ModelContext<Self>,
    ) {
        for action in actions_to_enqueue {
            let action_id = action.id.clone();
            // Some actions may already have results. This can happen in session sharing when
            // the sharer finishes and sends a result while preprocessing is still running on the viewer.
            // This is an edge case that only happens with fast tool calls, but we still need to guard against it,
            // as otherwise tools get stuck in a pending state on the viewer's side of things. This check
            // must be scoped to the current conversation as some providers generate tool call IDs that
            // only unique within a conversation.
            if self
                .finished_action_results
                .get(&conversation_id)
                .is_some_and(|results| results.iter().any(|r| r.id == action_id))
            {
                continue;
            }

            // In view-only mode, if an action is already marked as running
            // (which can happen if we receive a CommandExecutionStarted event
            // before the action is queued), don't add it to the pending queue to avoid an inconsistent state.
            if self.is_view_only
                && self
                    .running_actions
                    .get(&conversation_id)
                    .is_some_and(|running| running.contains(&action_id))
            {
                continue;
            }

            self.hop_state.insert(
                (conversation_id, action_id.clone()),
                (hop_kind(&action.action), ""),
            );
            self.pending_actions
                .entry(conversation_id)
                .or_default()
                .push_back(action);
            self.hop(conversation_id, &action_id, "queued");
            ctx.emit(BlocklistAIActionEvent::QueuedAction(action_id));
        }
    }

    fn handle_preprocess_actions_results(
        &mut self,
        conversation_id: AIConversationId,
        preprocess_id: PreprocessId,
        actions: Vec<AIAgentAction>,
        ctx: &mut ModelContext<Self>,
    ) {
        let (actions_to_enqueue, stale) = self
            .pending_preprocessed_actions
            .entry(conversation_id)
            .or_default()
            .handle_preprocess_actions_result_with_stale(preprocess_id, actions);
        // #1894: actions in hand whose batch was abandoned (cancel / new turn) or is gone never
        // execute, and still owe the conversation a cancelled result.
        // #2104: unless they were settled when the batch was dropped. Then this late result
        // is dropped: the call has had its one result.
        let stale = self.drop_late_preprocess_results(conversation_id, stale, ctx);
        self.record_stale_cancelled(conversation_id, stale);

        self.enqueue_preprocessed_actions(conversation_id, actions_to_enqueue, ctx);
        self.try_to_execute_available_actions(conversation_id, ctx);
    }

    /// Apply a finished action result to the conversation.
    /// This is used in agent session sharing to apply finished action results
    /// received from the action stream.
    pub fn apply_finished_action_result(
        &mut self,
        conversation_id: AIConversationId,
        mut action_result: AIAgentActionResult,
        ctx: &mut ModelContext<Self>,
    ) {
        let action_id = action_result.id.clone();
        if let Some(queue) = self.pending_actions.get_mut(&conversation_id) {
            if let Some(idx) = queue.iter().position(|a| a.id == action_id) {
                queue.remove(idx);
            }
        }

        // For shared session viewers, take in any document action results
        // and apply the associated actions to the local document version
        // (or create a new document if the given doc does not exist).
        self.maybe_sync_view_only_documents_with_local_model(
            conversation_id,
            &mut action_result,
            ctx,
        );

        self.handle_action_result(conversation_id, Arc::new(action_result), None, ctx);
    }

    /// Whether the user rejected this action's own card; see `rejected_by_user`.
    pub fn was_rejected_by_user(&self, action_id: &AIAgentActionId) -> bool {
        self.rejected_by_user.contains(action_id)
    }

    /// A Ctrl-C typed while actions of `conversation_id` run that are not shell commands (a read,
    /// a search, an MCP call): the key interrupts what runs and the turn goes on, as it does for
    /// a running command (#850, owner decision 2026-11-10; #2034). Every running action of the
    /// conversation is cancelled (its work aborted, its result `cancelled`) with NO cancellation
    /// reason: nobody stopped the turn. The later actions of the reply stay queued and come up
    /// next, and the step ends by the ordinary end-of-step rule (`plan_finished_batch`, no
    /// reason): its results go back to the model when any of them finished, and a step whose
    /// only result is the interrupted action ends `Cancelled` with the reason shown (#1178).
    /// This is exactly what the interrupted command's exit 130 does.
    ///
    /// Returns false, having done nothing, when this is not such a Ctrl-C: nothing runs, a
    /// running action is a shell command (its ^C is the pty's), the executor does not hold a
    /// running action, or the kill switch is off. The caller then does what it did before.
    pub fn interrupt_running_actions(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if !ctrl_c_interrupts_action_enabled() {
            return false;
        }
        let running: Vec<AIAgentActionId> = self
            .running_actions
            .get(&conversation_id)
            .map(|running| running.action_ids.clone())
            .unwrap_or_default();
        let interruptible = {
            let executor = self.executor.as_ref(ctx);
            ctrl_c_can_interrupt_actions(running.iter().map(|action_id| {
                executor
                    .async_executing_action(action_id)
                    .map(|action| &action.action)
            }))
        };
        if !interruptible {
            return false;
        }
        log::info!(
            "Ctrl-C: interrupting {} running action(s) of conversation {conversation_id:?}; \
             the turn goes on (#2034)",
            running.len()
        );
        // For the real-app cases (`agent_ctrl_c_running_tool`): the key took this path. A count
        // only, never an action's arguments.
        let count = running.len();
        warpui::input_order::log_hop("agent_action_interrupt", || format!("count={count}"));
        self.executor.update(ctx, |executor, ctx| {
            for action_id in &running {
                executor.cancel_running_async_action(action_id, None, ctx);
            }
        });
        true
    }

    /// Cancels one action by id: a card's Reject (`CancellationReason::Rejected`) or the status
    /// bar's Stop / Ctrl-C on the shown action (`ManuallyCancelled`).
    pub fn cancel_action_with_id(
        &mut self,
        conversation_id: AIConversationId,
        action_id: &AIAgentActionId,
        reason: CancellationReason,
        ctx: &mut ModelContext<Self>,
    ) {
        if self
            .running_actions
            .get(&conversation_id)
            .is_some_and(|running| running.contains(action_id))
        {
            // A reject is an answer to a card that still waits for one. Once the action runs
            // there is nothing left to reject: cancelling it is the user stopping it, with
            // everything a manual stop does (the running command is interrupted, #850; no
            // request is sent afterwards, #1942).
            let reason = if reason.is_rejected() {
                CancellationReason::ManuallyCancelled
            } else {
                reason
            };
            if reason.is_user_stop() && ctrl_c_interrupts_action_enabled() {
                // #2034: a stop stops the turn. Cancelling only the running action left the
                // later actions of the reply queued, so their cards came up after the user had
                // stopped the agent; a stop on a card that still waits has always cancelled
                // them (#1799). Everything the conversation holds goes with the stop's reason:
                // what runs (a running command is interrupted, #850), what is queued behind
                // it, and what is still preprocessing.
                log::info!(
                    "Action {action_id:?} was stopped while it ran: cancelling the rest of \
                     the turn with it (#2034)"
                );
                self.cancel_all_pending_actions(conversation_id, Some(reason), ctx);
                return;
            }
            self.executor.update(ctx, |executor, ctx| {
                executor.cancel_running_async_action(action_id, Some(reason), ctx)
            });
        } else {
            let Some(pending_actions_for_conversation) =
                self.pending_actions.get_mut(&conversation_id)
            else {
                return;
            };
            if let Some((idx, _)) = pending_actions_for_conversation
                .iter()
                .find_position(|action| action.id == *action_id)
            {
                if let Some(action) = pending_actions_for_conversation.remove(idx) {
                    // Taken out of the queue BEFORE the rejected action's result is handled:
                    // that handling drains the queue (`try_to_execute_available_actions`), and
                    // would otherwise start, or block on, the next action of the turn.
                    // Only the actions after it: an earlier one still pending is not part of
                    // what the user turned down.
                    let rest_of_turn = if rejection_cancels_rest_of_turn(reason) {
                        pending_actions_for_conversation.drain(idx..).collect_vec()
                    } else {
                        Vec::new()
                    };
                    if !rest_of_turn.is_empty() {
                        log::info!(
                            "Action {action_id:?} was rejected: cancelling the {} later \
                             action(s) of the same turn with it (#1799)",
                            rest_of_turn.len()
                        );
                    }
                    if reason.is_rejected() {
                        self.rejected_by_user.insert(action.id.clone());
                    }
                    self.cancel_pending_action(conversation_id, action, Some(reason), ctx);
                    for later in rest_of_turn {
                        self.cancel_pending_action(conversation_id, later, Some(reason), ctx);
                    }
                }
            }
        }
    }

    /// Gives actions that were never queued (the turn's stream was cancelled by a follow-up
    /// before `queue_actions`) a cancelled result, so the follow-up request carries a result
    /// for every tool call and is never blocked on an unanswered one (#1861). Also used when the
    /// stream was cancelled by a Stop / Ctrl-C / Delete in that same window (#1946): every call
    /// of a reply gets exactly one result, at once, whoever cancelled it.
    pub(super) fn cancel_discarded_actions(
        &mut self,
        actions: Vec<AIAgentAction>,
        conversation_id: AIConversationId,
        reason: Option<CancellationReason>,
        ctx: &mut ModelContext<Self>,
    ) {
        for action in actions {
            let already_finished = self
                .finished_action_results
                .get(&conversation_id)
                .is_some_and(|results| results.iter().any(|r| r.id == action.id));
            if already_finished {
                continue;
            }
            log::info!(
                "hop=agent_action id={} status=discarded stream cancelled before the action was queued",
                action.id
            );
            self.cancel_pending_action(conversation_id, action, reason, ctx);
        }
    }

    /// Returns what it did, for the caller that has to know whether anything will still end the
    /// turn (#1944): see [`CancelledPendingActions`].
    pub(super) fn cancel_all_pending_actions(
        &mut self,
        conversation_id: AIConversationId,
        reason: Option<CancellationReason>,
        ctx: &mut ModelContext<Self>,
    ) -> CancelledPendingActions {
        let settle_preprocessing = stop_settles_preprocessing_enabled();
        self.cancel_all_pending_actions_with(conversation_id, reason, settle_preprocessing, ctx)
    }

    /// [`Self::cancel_all_pending_actions`] with the #2104 kill switch's reading passed in
    /// (tests drive both arms without touching the process environment).
    ///
    /// `settle_preprocessing`: the calls of a batch that is still preprocessing get their
    /// cancelled result HERE, like the parked, queued and blocked ones, through the same path
    /// (`cancel_pending_action`: a result, a `FinishedAction`, the executor's per-action state
    /// released). Before #2104 they were left with no status and no result until the batch's
    /// future came back, which a hung apply never does (#1894); in between the turn watch
    /// (#1967) read them as calls nothing holds, and a pane close could not discard a result
    /// that did not exist yet (#848).
    pub(super) fn cancel_all_pending_actions_with(
        &mut self,
        conversation_id: AIConversationId,
        reason: Option<CancellationReason>,
        settle_preprocessing: bool,
        ctx: &mut ModelContext<Self>,
    ) -> CancelledPendingActions {
        self.executor.update(ctx, |executor, ctx| {
            executor.cancel_all_running_async_actions_for_conversation(conversation_id, reason, ctx)
        });

        // Preprocess batches of a cancelled turn can no longer produce cards, and one that never
        // resolves would park every later batch (#1859). Give parked, already-preprocessed
        // actions their cancel results.
        let dropped_preprocessing = self
            .pending_preprocessed_actions
            .get(&conversation_id)
            .is_some_and(|batches| batches.has_pending());
        let (parked_actions, preprocessing_actions) =
            match self.pending_preprocessed_actions.get_mut(&conversation_id) {
                Some(batches) if settle_preprocessing => {
                    let abandoned = batches.abandon_all_settling();
                    (abandoned.parked, abandoned.preprocessing)
                }
                Some(batches) => (batches.abandon_all(), Vec::new()),
                None => (Vec::new(), Vec::new()),
            };
        let mut cancelled_any = !parked_actions.is_empty();
        for action in parked_actions {
            log::warn!(
                "hop=agent_action id={} status=parked cancelled while parked behind a preprocess batch",
                action.id
            );
            self.cancel_pending_action(conversation_id, action, reason, ctx);
        }

        // The conversation's pending actions are going away; so do their approval marks.
        self.full_command_shown
            .retain(|(conversation, _), _| *conversation != conversation_id);
        self.commands_being_edited
            .retain(|(conversation, _), _| *conversation != conversation_id);

        if let Some(actions_to_cancel) = self.pending_actions.get_mut(&conversation_id) {
            for action in actions_to_cancel.drain(..).collect_vec() {
                cancelled_any = true;
                self.cancel_pending_action(conversation_id, action, reason, ctx);
            }
        }

        // #2104: the calls of the batches that were still preprocessing, last: the queue is
        // empty by now, so settling one cannot start (or block on) a queued action of the
        // turn being stopped. The results are sorted back into the reply's order as always.
        let preprocessing_actions =
            self.mark_settled_while_preprocessing(conversation_id, preprocessing_actions);
        for action in preprocessing_actions {
            log::info!(
                "hop=agent_action id={} status=preprocessing cancelled with its batch still \
                 preprocessing; what the batch's future brings back is dropped (#2104)",
                action.id
            );
            // Its `hop=agent_action ... status=cancelled` line names the tool, as for a
            // queued action (an action that never left preprocessing has no kind on record).
            self.hop_state.insert(
                (conversation_id, action.id.clone()),
                (hop_kind(&action.action), ""),
            );
            cancelled_any = true;
            self.cancel_pending_action(conversation_id, action, reason, ctx);
        }
        CancelledPendingActions {
            cancelled_any,
            dropped_preprocessing,
        }
    }

    /// Removes and returns all pending RequestCommandOutput actions for a conversation.
    fn drain_pending_request_command_actions(
        &mut self,
        conversation_id: AIConversationId,
    ) -> Vec<AIAgentAction> {
        let Some(pending_actions) = self.pending_actions.get_mut(&conversation_id) else {
            return Vec::new();
        };

        let mut to_drain = Vec::new();
        let mut i = 0;
        while i < pending_actions.len() {
            if matches!(
                pending_actions[i].action,
                AIAgentActionType::RequestCommandOutput { .. }
            ) {
                to_drain.push(
                    pending_actions
                        .remove(i)
                        .expect("index is valid because i < pending_actions.len()"),
                );
            } else {
                i += 1;
            }
        }
        to_drain
    }

    fn cancel_pending_action(
        &mut self,
        conversation_id: AIConversationId,
        pending_action: AIAgentAction,
        reason: Option<CancellationReason>,
        ctx: &mut ModelContext<Self>,
    ) {
        if matches!(
            pending_action.action,
            AIAgentActionType::RequestComputerUse(_)
        ) {
            send_telemetry_from_ctx!(
                TelemetryEvent::ComputerUseCancelled {
                    conversation_id,
                    ambient_agent_task_id: self.ambient_agent_task_id,
                },
                ctx
            );
        }

        let result = Arc::new(AIAgentActionResult {
            id: pending_action.id,
            task_id: pending_action.task_id,
            result: pending_action.action.cancelled_result(),
        });
        self.handle_action_result(conversation_id, result, reason, ctx);
    }

    /// Returns all finished action results from the given conversation, moving them to the
    /// Clears the results restored from a previous session's persisted conversation.
    pub fn clear_restored_action_results(&mut self) {
        self.past_action_results.clear();
    }

    /// `past_action_results` in the process.
    pub(super) fn drain_finished_action_results(
        &mut self,
        conversation_id: AIConversationId,
    ) -> Vec<AIAgentActionResult> {
        self.action_order.remove(&conversation_id);
        let finished_action_results = self
            .finished_action_results
            .remove(&conversation_id)
            .unwrap_or_default();

        for result in finished_action_results.iter() {
            self.past_action_results
                .insert(result.id.clone(), result.clone());
        }
        finished_action_results
            .into_iter()
            .map(|result| (*result).clone())
            .collect_vec()
    }

    pub(super) fn finished_action_results_matching(
        &self,
        conversation_id: AIConversationId,
        keys: &HashSet<(String, String)>,
    ) -> Vec<AIAgentActionResult> {
        self.finished_action_results
            .get(&conversation_id)
            .into_iter()
            .flat_map(|results| results.iter())
            .filter(|result| keys.contains(&(result.task_id.to_string(), result.id.to_string())))
            .map(|result| (**result).clone())
            .collect_vec()
    }

    pub(super) fn remove_finished_action_results_matching(
        &mut self,
        conversation_id: AIConversationId,
        keys: &HashSet<(String, String)>,
    ) -> usize {
        let mut should_remove_bucket = false;
        let mut removed = 0;
        if let Some(results) = self.finished_action_results.get_mut(&conversation_id) {
            let mut index = 0;
            while index < results.len() {
                let key = (
                    results[index].task_id.to_string(),
                    results[index].id.to_string(),
                );
                if keys.contains(&key) {
                    let result = results.remove(index);
                    self.past_action_results.insert(result.id.clone(), result);
                    removed += 1;
                } else {
                    index += 1;
                }
            }
            should_remove_bucket = results.is_empty();
        }
        if should_remove_bucket {
            self.action_order.remove(&conversation_id);
            self.finished_action_results.remove(&conversation_id);
        }
        removed
    }

    /// Clears finished action results for a conversation. Used when reverting.
    pub(super) fn clear_finished_action_results(&mut self, conversation_id: AIConversationId) {
        self.action_order.remove(&conversation_id);
        self.finished_action_results.remove(&conversation_id);
    }

    /// #2103: what a pane close does with the conversation's finished, unsent results (#848).
    /// They leave `finished_action_results`, so none of them goes out with a later request,
    /// exactly as [`Self::clear_finished_action_results`] has it; but each is kept in
    /// `results_discarded_by_close`, so its call still has a status (see that field for what
    /// may read it). Returns how many results were discarded.
    ///
    /// With the kill switch off ([`close_keeps_call_status_enabled`]) the results are erased
    /// as before.
    pub(super) fn discard_finished_action_results_for_close(
        &mut self,
        conversation_id: AIConversationId,
        keep_status: bool,
    ) -> usize {
        self.action_order.remove(&conversation_id);
        let discarded = self
            .finished_action_results
            .remove(&conversation_id)
            .unwrap_or_default();
        let count = discarded.len();
        if keep_status {
            for result in discarded {
                self.results_discarded_by_close
                    .insert(result.id.clone(), result);
            }
        }
        count
    }

    /// Whether `id`'s result is one a pane close discarded and kept for its status only
    /// (#2103).
    #[cfg(test)]
    pub(crate) fn is_result_discarded_by_close(&self, id: &AIAgentActionId) -> bool {
        self.results_discarded_by_close.contains_key(id)
    }

    /// The control flow for initiating cancellations across suggested plans, requested commands,
    /// and code diff views are identical, and thus should be handled directly by the [`AIBlock`]'s
    /// respective functions.
    pub fn handle_requested_command_accepted(
        &mut self,
        action_id: &AIAgentActionId,
        command: String,
        ctx: &mut ModelContext<Self>,
    ) {
        // Search through all pending conversations to find the action and conversation ID
        let mut found_conversation_id = None;
        for (conversation_id, pending_actions_for_conversation) in self.pending_actions.iter_mut() {
            if let Some(action) = pending_actions_for_conversation
                .iter_mut()
                .find(|action| action.id == *action_id)
            {
                if let AIAgentActionType::RequestCommandOutput {
                    command: original_command,
                    ..
                } = &mut action.action
                {
                    *original_command = command;
                    found_conversation_id = Some(*conversation_id);
                    break;
                }
            }
        }

        let Some(conversation_id) = found_conversation_id else {
            log::error!(
                "[byop-diag] handle_requested_command_accepted: action_id={action_id:?} NOT FOUND \
                 in pending_actions. pending_conversations=[{}] (the action didn't enter \
                 pending_actions correctly; the chain broke somewhere between \
                 controller.queue_actions → action_model.try_to_execute_action)",
                self.pending_actions
                    .iter()
                    .map(|(c, q)| format!("{c:?}:{}", q.len()))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            debug_assert!(false, "Expected action to be requested command.");
            return;
        };

        log::info!(
            "[byop-diag] handle_requested_command_accepted: action_id={action_id:?} found in \
             conversation_id={conversation_id:?}, calling execute_action"
        );
        self.execute_action(action_id, conversation_id, ctx);
    }

    fn handle_action_result(
        &mut self,
        conversation_id: AIConversationId,
        action_result: Arc<AIAgentActionResult>,
        cancellation_reason: Option<CancellationReason>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.full_command_shown
            .remove(&(conversation_id, action_result.id.clone()));
        self.commands_being_edited
            .remove(&(conversation_id, action_result.id.clone()));
        let should_remove_entry =
            self.running_actions
                .get_mut(&conversation_id)
                .is_some_and(|running| {
                    running.remove_action(&action_result.id);
                    running.is_empty()
                });

        if should_remove_entry {
            self.running_actions.remove(&conversation_id);
        }

        let action_id = action_result.id.clone();
        let hop_status = if action_result.result.is_cancelled() {
            "cancelled"
        } else if action_result.result.is_failed() {
            "failed"
        } else {
            "done"
        };
        self.hop(conversation_id, &action_id, hop_status);

        // Every terminal outcome (success, failure, cancellation — from any
        // path) funnels through here, so this is the one place executor-held
        // per-action state is released.
        self.executor.update(ctx, |executor, ctx| {
            executor.discard_action_state(&action_id, ctx);
        });

        // If a command action entered long-running mode (returned a snapshot), cancel all other
        // pending RequestCommandOutput actions. Only one command can be active at a time, and the
        // server can only spawn one CLI subagent. We don't cancel other actions because those
        // actions will complete before we send any response to the server. NOTE: this does allow
        // the long-running command to execute in parallel with the other actions.
        if matches!(
            &action_result.result,
            AIAgentActionResultType::RequestCommandOutput(
                RequestCommandOutputResult::LongRunningCommandSnapshot { .. }
            )
        ) {
            // Deliberately `None` rather than `cancellation_reason`. These actions are
            // collateral -- they are cancelled because the *other* action went long-running,
            // not because anything about them succeeded. Propagating the originating reason
            // stranded the whole conversation: a long-running command completes with
            // `OptimisticCLISubagentCompletion`, whose `conversation_outcome()` is
            // `Succeeded`, and `BlocklistAIController`'s `FinishedAction` subscriber reads the
            // reason off the *last* `FinishedAction` to compute `treat_as_success`. With the
            // reason inherited, that last event was a collateral cancellation claiming success,
            // so `should_trigger_follow_up_request` was false, no follow-up request was sent,
            // and the block below left the conversation `InProgress` with nothing pending,
            // nothing running and nothing in flight -- permanently. Observed live as 2m36s of
            // dead air with two prompts stuck in the queued-prompts panel, which drains only on
            // `FinishReason::Complete` and so never fired; the user had to send both by hand.
            //
            // `None` maps to `treat_as_success == false`, handing the follow-up decision back
            // to its real question: did any non-cancelled result finish? It does not force a
            // follow-up, so a turn in which everything was cancelled still ends quietly.
            //
            // LOAD-BEARING FOR THE UI, so do not "improve" this into a real reason without
            // reading the other end first. `RequestedCommandView` (inline_action/
            // requested_command.rs) distinguishes a drain cancellation from every other kind
            // by exactly this absence -- `cancellation_reason.is_none()` plus the action never
            // having reached a block -- because `AIAgentActionResult` carries no reason of its
            // own: every cancel path stores the same `CancelledBeforeExecution`. That is what
            // lets the row say "Cancelled -- another command was already running" instead of
            // looking identical to a user-pressed Cancel. Giving this call site a reason would
            // silently revert the row to an unexplained cancel, and no test would fail. If a
            // reason is ever wanted here, thread it through `AIAgentActionResult` and switch
            // the view to read it, in that order.
            for action in self.drain_pending_request_command_actions(conversation_id) {
                self.cancel_pending_action(conversation_id, action, None, ctx);
            }
        }

        self.finished_action_results
            .entry(conversation_id)
            .or_default()
            .push(action_result);

        ctx.emit(BlocklistAIActionEvent::FinishedAction {
            action_id,
            conversation_id,
            cancellation_reason,
        });
        if self
            .running_actions
            .get(&conversation_id)
            .is_some_and(|running| !running.is_empty())
        {
            // Wait until the entire phase drains before scheduling subsequent actions or deciding
            // whether to send a follow-up request.
            return;
        }

        // The phase is fully drained — sort results back into original tool-call order.
        self.sort_finished_results(conversation_id);

        if self
            .pending_actions
            .get(&conversation_id)
            .is_none_or(|actions| actions.is_empty())
        {
            if !cancellation_reason.is_some_and(|r| r.is_follow_up_for_same_conversation()) {
                BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                    let status = if self
                        .finished_action_results
                        .get(&conversation_id)
                        .is_some_and(|finished_results| {
                            finished_results
                                .iter()
                                .all(|result| result.result.is_cancelled())
                        }) {
                        ConversationStatus::Cancelled
                    } else {
                        ConversationStatus::InProgress
                    };
                    history_model.update_conversation_status(
                        self.terminal_view_id,
                        conversation_id,
                        status,
                        ctx,
                    );
                });
            }
        } else {
            self.try_to_execute_available_actions(conversation_id, ctx);
        }
    }

    /// In shared-session viewer (view-only) mode, ensure document-related action results
    /// are backed by documents in the local `AIDocumentModel` and that their
    /// `DocumentContext` versions match. For CreateDocuments, restore missing documents
    /// (using titles from the original action); for EditDocuments, apply edits to local
    /// documents and align versions, so headers and "View" buttons stay accurate.
    fn maybe_sync_view_only_documents_with_local_model(
        &self,
        conversation_id: AIConversationId,
        result: &mut AIAgentActionResult,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self.is_view_only {
            return;
        }

        match &mut result.result {
            AIAgentActionResultType::CreateDocuments(CreateDocumentsResult::Success {
                created_documents,
            }) => {
                let history = BlocklistAIHistoryModel::handle(ctx);
                let Some(conversation) = history.as_ref(ctx).conversation(&conversation_id) else {
                    return;
                };
                let titles = conversation.get_document_titles_for_action(&result.id);

                let doc_model = AIDocumentModel::handle(ctx);
                doc_model.update(ctx, |doc_model, doc_ctx| {
                    for (index, doc_context) in created_documents.iter_mut().enumerate() {
                        // If a user is re-opening a shared session that they previously closed in the current warp session,
                        // we should delete the previously created document so that the verseion history doesn't get messed up.
                        doc_model.delete_document(&doc_context.document_id);

                        let title = titles
                            .as_ref()
                            .and_then(|t| t.get(index))
                            .cloned()
                            .unwrap_or_else(|| DEFAULT_PLANNING_DOCUMENT_TITLE.to_string());

                        doc_model.restore_document(
                            doc_context.document_id,
                            conversation_id,
                            &title,
                            doc_context.content.clone(),
                            Local::now(),
                            doc_ctx,
                        );
                    }
                });
            }
            AIAgentActionResultType::EditDocuments(EditDocumentsResult::Success {
                updated_documents,
            }) => {
                let doc_model = AIDocumentModel::handle(ctx);
                doc_model.update(ctx, |doc_model, doc_ctx| {
                    for doc_context in updated_documents.iter_mut() {
                        if doc_model
                            .get_current_document(&doc_context.document_id)
                            .is_none()
                        {
                            // You can't make edits to a doc that does not exist.
                            continue;
                        }

                        if let Some(new_version) = doc_model.restore_document_edit(
                            &doc_context.document_id,
                            doc_context.content.clone(),
                            Local::now(),
                            doc_ctx,
                        ) {
                            // Align the header's version with the locally restored doc
                            // so the viewer sees the correct bumped version.
                            doc_context.document_version = new_version;
                        }
                    }
                });
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum BlocklistAIActionEvent {
    /// Emitted when the action with the given ID is enqueued for execution.
    QueuedAction(AIAgentActionId),
    /// Emitted when the action with the given ID requires user confirmation to execute.
    ActionBlockedOnUserConfirmation(AIAgentActionId),
    /// Emitted when the action with the given ID begins execution.
    ExecutingAction(AIAgentActionId),
    /// Emitted when the action with the given ID has finished.
    FinishedAction {
        action_id: AIAgentActionId,
        conversation_id: AIConversationId,
        cancellation_reason: Option<CancellationReason>,
    },
    ToggleCodeReview(AIAgentActionId),
    InsertCodeReviewComments {
        action_id: AIAgentActionId,
        repo_path: PathBuf,
        comments: Vec<ai::agent::action::InsertReviewComment>,
        base_branch: Option<String>,
    },
}

impl BlocklistAIActionEvent {
    pub fn action_id(&self) -> &AIAgentActionId {
        match self {
            BlocklistAIActionEvent::QueuedAction(action_id) => action_id,
            BlocklistAIActionEvent::ActionBlockedOnUserConfirmation(action_id) => action_id,
            BlocklistAIActionEvent::ExecutingAction(action_id) => action_id,
            BlocklistAIActionEvent::FinishedAction { action_id, .. } => action_id,
            BlocklistAIActionEvent::ToggleCodeReview(action_id) => action_id,
            BlocklistAIActionEvent::InsertCodeReviewComments { action_id, .. } => action_id,
        }
    }
}

impl Entity for BlocklistAIActionModel {
    type Event = BlocklistAIActionEvent;
}

#[cfg(test)]
#[path = "action_model_tests.rs"]
mod tests;
