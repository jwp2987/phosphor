use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use super::*;
use crate::ai::agent::{AIAgentActionResultType, task::TaskId};

fn make_action_result(id: &str) -> Arc<AIAgentActionResult> {
    Arc::new(AIAgentActionResult {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: TaskId::new("task".to_owned()),
        result: AIAgentActionResultType::InitProject,
    })
}

fn count_startable_actions_for_pass(phases: &[(RunningActionPhase, bool)]) -> usize {
    let mut current_phase = None;
    let mut count = 0;

    for (phase, can_autoexecute) in phases {
        if let Some(current_phase) = current_phase {
            if !can_start_action_with_current_phase(current_phase, *phase, *can_autoexecute) {
                break;
            }
        }

        count += 1;
        current_phase = Some(*phase);

        if matches!(*phase, RunningActionPhase::Serial) {
            break;
        }
    }

    count
}

#[test]
fn parallel_phase_only_admits_matching_autoexecutable_actions() {
    let phase =
        RunningActionPhase::Parallel(execute::ParallelExecutionPolicy::ReadOnlyLocalContext);

    assert!(can_start_action_with_current_phase(phase, phase, true));
    assert!(!can_start_action_with_current_phase(phase, phase, false));
    assert!(!can_start_action_with_current_phase(
        phase,
        RunningActionPhase::Serial,
        true
    ));
    assert!(!can_start_action_with_current_phase(
        RunningActionPhase::Serial,
        phase,
        true
    ));
}

#[test]
fn phased_scheduling_stops_at_serial_barrier_and_resumes_afterward() {
    let read_only_phase =
        RunningActionPhase::Parallel(execute::ParallelExecutionPolicy::ReadOnlyLocalContext);
    let actions = vec![
        (read_only_phase, true),
        (read_only_phase, true),
        (RunningActionPhase::Serial, true),
        (read_only_phase, true),
        (read_only_phase, true),
    ];

    assert_eq!(count_startable_actions_for_pass(&actions), 2);
    assert_eq!(count_startable_actions_for_pass(&actions[2..]), 1);
    assert_eq!(count_startable_actions_for_pass(&actions[3..]), 2);
}

#[test]
fn finished_results_stay_in_original_action_order() {
    let action_order = HashMap::from([
        (AIAgentActionId::from("first".to_owned()), 0),
        (AIAgentActionId::from("second".to_owned()), 1),
        (AIAgentActionId::from("third".to_owned()), 2),
    ]);
    let mut finished_results = [
        make_action_result("third"),
        make_action_result("first"),
        make_action_result("second"),
    ];

    finished_results
        .sort_by_key(|result| action_order.get(&result.id).copied().unwrap_or(usize::MAX));

    assert_eq!(
        finished_results[0].id,
        AIAgentActionId::from("first".to_owned())
    );
    assert_eq!(
        finished_results[1].id,
        AIAgentActionId::from("second".to_owned())
    );
    assert_eq!(
        finished_results[2].id,
        AIAgentActionId::from("third".to_owned())
    );
}

fn pending_action(id: &str, action: AIAgentActionType) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: TaskId::new("task".to_owned()),
        action,
        requires_result: false,
    }
}

fn shell_command() -> AIAgentActionType {
    AIAgentActionType::RequestCommandOutput {
        command: "free -h".to_owned(),
        is_read_only: None,
        is_risky: None,
        rationale: None,
        uses_pager: None,
        wait_until_completion: true,
        citations: vec![],
    }
}

/// The AI block's Enter key and the CLI subagent view's accept both reach
/// `execute_next_action_for_user`, which runs the queue front as `ActionInitiator::User`. A
/// blocked `ask_user_question` at the front must not be picked: running it with no answer on
/// the executor's channel hangs the turn. Nor may the path skip past it to the action behind:
/// nothing queued after a question may run before it is answered.
///
/// Red if the `can_be_accepted_without_its_own_ui` filter is removed from
/// `next_action_acceptable_by_plain_accept` (the front question's id is returned), which is
/// exactly the unguarded front-of-queue lookup `execute_next_action_for_user` did before.
#[test]
fn plain_accept_refuses_a_front_question_and_does_not_skip_it() {
    let queue: VecDeque<_> = [
        pending_action(
            "question",
            AIAgentActionType::AskUserQuestion { questions: vec![] },
        ),
        pending_action("shell", shell_command()),
    ]
    .into_iter()
    .collect();

    assert_eq!(next_action_acceptable_by_plain_accept(Some(&queue)), None);
}

/// The guard is narrow: any other front action is still accepted by Enter, and an empty or
/// missing queue still yields nothing.
#[test]
fn plain_accept_still_picks_an_ordinary_front_action() {
    let queue: VecDeque<_> = [
        pending_action("shell", shell_command()),
        pending_action(
            "question",
            AIAgentActionType::AskUserQuestion { questions: vec![] },
        ),
    ]
    .into_iter()
    .collect();

    assert_eq!(
        next_action_acceptable_by_plain_accept(Some(&queue)),
        Some(&AIAgentActionId::from("shell".to_owned()))
    );
    assert_eq!(
        next_action_acceptable_by_plain_accept(Some(&VecDeque::new())),
        None
    );
    assert_eq!(next_action_acceptable_by_plain_accept(None), None);
}

fn shell_command_text(command: &str) -> AIAgentActionType {
    AIAgentActionType::RequestCommandOutput {
        command: command.to_owned(),
        is_read_only: None,
        is_risky: None,
        rationale: None,
        uses_pager: None,
        wait_until_completion: true,
        citations: vec![],
    }
}

/// #847: `start_pending_action_by_id` refuses an `ActionInitiator::User` start of a command whose
/// approval card elides part of it, unless the exact text that would run was shown in full in
/// the same conversation. That covers long tokens and blank-line padding. Other action types and
/// short commands are never affected.
#[test]
fn hidden_command_blocks_user_accept_until_that_exact_text_is_shown() {
    let long_text = format!("echo {}; touch ~/x", "A".repeat(100_000));
    let long_token = pending_action("long", shell_command_text(&long_text));
    let blank_padding = pending_action(
        "padded",
        shell_command_text(&format!("echo hi{}rm -rf ~", "\n".repeat(3_000))),
    );
    let short = pending_action("short", shell_command());
    let question = pending_action(
        "question",
        AIAgentActionType::AskUserQuestion { questions: vec![] },
    );
    let conversation = AIConversationId::new();
    let other_conversation = AIConversationId::new();

    let none_shown = HashMap::new();
    assert!(hidden_command_blocks_user_accept(
        conversation,
        &long_token,
        &none_shown
    ));
    assert!(hidden_command_blocks_user_accept(
        conversation,
        &blank_padding,
        &none_shown
    ));
    assert!(!hidden_command_blocks_user_accept(
        conversation,
        &short,
        &none_shown
    ));
    assert!(!hidden_command_blocks_user_accept(
        conversation,
        &question,
        &none_shown
    ));

    let mut shown = HashMap::new();
    shown.insert((conversation, long_token.id.clone()), long_text.clone());
    assert!(!hidden_command_blocks_user_accept(
        conversation,
        &long_token,
        &shown
    ));
    // A mark is per action, per conversation: the same id elsewhere is not covered.
    assert!(hidden_command_blocks_user_accept(
        other_conversation,
        &long_token,
        &shown
    ));
    assert!(hidden_command_blocks_user_accept(
        conversation,
        &blank_padding,
        &shown
    ));

    // Review B1: a mark for one text does not cover another. The user edits the command,
    // shows the edit in full, and then an accept path runs a different (original) text.
    let mut shown_edit = HashMap::new();
    shown_edit.insert(
        (conversation, long_token.id.clone()),
        format!("{long_text} # edited"),
    );
    assert!(hidden_command_blocks_user_accept(
        conversation,
        &long_token,
        &shown_edit
    ));
}

fn one_question() -> AIAgentActionType {
    use ai::agent::action::{AskUserQuestionItem, AskUserQuestionOption, AskUserQuestionType};
    AIAgentActionType::AskUserQuestion {
        questions: vec![AskUserQuestionItem {
            question_id: "q1".to_owned(),
            question: "Which branch?".to_owned(),
            question_type: AskUserQuestionType::MultipleChoice {
                is_multiselect: false,
                options: vec![AskUserQuestionOption {
                    label: "main".to_owned(),
                    recommended: true,
                }],
                supports_other: true,
            },
        }],
    }
}

/// #1797: `execute_action` (every by-id accept: the generic Run / Enter, the cards, the TUI
/// views) refuses a question whose answer the card has not staged. Red if the guard is removed:
/// the generic Run then ran the question with no answer and the turn hung on "Preparing
/// question...".
#[test]
fn user_accept_of_a_question_needs_its_staged_answer() {
    assert!(user_accept_lacks_question_answer(&one_question(), false));
    assert!(!user_accept_lacks_question_answer(&one_question(), true));
}

/// The guard is narrow: other actions are unaffected, and a question with nothing to show is
/// not refused -- it never waits for an answer (its execution is an error result).
#[test]
fn question_answer_guard_leaves_other_actions_alone() {
    assert!(!user_accept_lacks_question_answer(&shell_command(), false));
    assert!(!user_accept_lacks_question_answer(
        &AIAgentActionType::TransferShellCommandControlToUser {
            reason: "needs a password".to_owned(),
        },
        false
    ));
    assert!(!user_accept_lacks_question_answer(
        &AIAgentActionType::AskUserQuestion { questions: vec![] },
        false
    ));
}

/// #1799: only the user's own reject / stop cancels the rest of the turn. Other reasons have
/// their own paths (a follow-up or a revert cancels everything pending already; an optimistic
/// completion is not a reject) and must not be widened by this.
#[test]
fn only_a_user_reject_cancels_the_rest_of_the_turn() {
    assert!(rejection_cancels_rest_of_turn(
        CancellationReason::ManuallyCancelled
    ));
    // A card's own Reject has its own reason since the #1942 follow-up decision; it cancels the
    // rest of the turn exactly as before.
    assert!(rejection_cancels_rest_of_turn(CancellationReason::Rejected));
    for reason in [
        CancellationReason::FollowUpSubmitted {
            is_for_same_conversation: true,
        },
        CancellationReason::UserCommandExecuted,
        CancellationReason::Reverted,
        CancellationReason::Deleted,
        CancellationReason::OptimisticCLISubagentCompletion,
        CancellationReason::AgentExitedShell,
    ] {
        assert!(!rejection_cancels_rest_of_turn(reason), "{reason:?}");
    }
}

/// #2034: a Ctrl-C interrupts running actions as actions only when something runs, the executor
/// holds every one of them, and none is a shell command (or drives one: its ^C is the pty's,
/// #850). Anything else is left to the path that handled it before.
///
/// Red if the command check is dropped: a Ctrl-C that reached this path with a command running
/// would cancel the tool call with no reason and never interrupt the process.
#[test]
fn ctrl_c_interrupts_running_tools_but_never_a_shell_command() {
    let grep = AIAgentActionType::Grep {
        queries: vec!["needle".to_owned()],
        path: ".".to_owned(),
    };
    let question = one_question();
    let command = shell_command();
    let hand_over = AIAgentActionType::TransferShellCommandControlToUser {
        reason: "needs a password".to_owned(),
    };

    assert!(ctrl_c_can_interrupt_actions([Some(&grep)]));
    assert!(
        ctrl_c_can_interrupt_actions([Some(&grep), Some(&question)]),
        "a parallel phase: everything that runs is interrupted"
    );
    assert!(
        !ctrl_c_can_interrupt_actions(std::iter::empty::<Option<&AIAgentActionType>>()),
        "nothing runs: nothing to interrupt"
    );
    assert!(!ctrl_c_can_interrupt_actions([Some(&command)]));
    assert!(!ctrl_c_can_interrupt_actions([Some(&hand_over)]));
    assert!(
        !ctrl_c_can_interrupt_actions([Some(&grep), Some(&command)]),
        "one command among them: not this path"
    );
    assert!(
        !ctrl_c_can_interrupt_actions([Some(&grep), None]),
        "an action the executor does not hold cannot be cancelled there"
    );
}

/// The #2034 kill switch reads `0` as off and everything else as on.
#[test]
fn ctrl_c_interrupts_action_kill_switch() {
    assert!(ctrl_c_interrupts_action_enabled_from(None));
    assert!(ctrl_c_interrupts_action_enabled_from(Some("1")));
    assert!(!ctrl_c_interrupts_action_enabled_from(Some("0")));
    assert!(!ctrl_c_interrupts_action_enabled_from(Some(" 0 ")));
}

/// #1967: the pane's "a waiting question must have a card" check asks about exactly one state:
/// the question at the front of its conversation's queue, nothing running, in a pane that takes
/// answers. Everything else (a question queued behind another action, a command, a running
/// step, a view-only pane) is not a question waiting for the user, so no card is owed for it.
#[test]
fn only_a_front_question_with_nothing_running_waits_for_the_user() {
    let question_id = AIAgentActionId::from("question".to_owned());
    let shell_id = AIAgentActionId::from("shell".to_owned());
    let question_first: VecDeque<_> = [
        pending_action(
            "question",
            AIAgentActionType::AskUserQuestion { questions: vec![] },
        ),
        pending_action("shell", shell_command()),
    ]
    .into_iter()
    .collect();
    assert!(front_is_waiting_question(
        &question_first,
        false,
        false,
        &question_id
    ));
    // A command at the front waits on its approval card, which is not this check's business.
    assert!(!front_is_waiting_question(
        &question_first,
        false,
        false,
        &shell_id
    ));
    // Something of the turn still runs: the front is queued behind it, not blocked.
    assert!(!front_is_waiting_question(
        &question_first,
        true,
        false,
        &question_id
    ));
    // A view-only pane never blocks on its viewer.
    assert!(!front_is_waiting_question(
        &question_first,
        false,
        true,
        &question_id
    ));

    let question_second: VecDeque<_> = [
        pending_action("shell", shell_command()),
        pending_action(
            "question",
            AIAgentActionType::AskUserQuestion { questions: vec![] },
        ),
    ]
    .into_iter()
    .collect();
    assert!(!front_is_waiting_question(
        &question_second,
        false,
        false,
        &question_id
    ));
    assert!(!front_is_waiting_question(
        &VecDeque::new(),
        false,
        false,
        &question_id
    ));
}
