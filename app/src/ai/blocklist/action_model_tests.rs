use std::{
    collections::{HashMap, HashSet, VecDeque},
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

/// Tag-in batch `[A, B, C]`: A is sync, and its `Agent` drain runs B and leaves C blocked on a
/// confirmation only the override can give. The step for B then finds it already taken. The
/// batch must go on to C and auto-accept it; stopping at B stranded C in the alt screen.
#[test]
fn auto_accept_batch_continues_past_action_taken_by_earlier_drain() {
    let mut calls = Vec::new();
    let attempted = run_auto_accept_batch(&["a", "b", "c"], |id| {
        calls.push(*id);
        match *id {
            "a" => AutoAcceptOutcome::Started,
            "b" => AutoAcceptOutcome::AlreadyTaken,
            _ => AutoAcceptOutcome::Started,
        }
    });

    assert_eq!(calls, vec!["a", "b", "c"]);
    assert_eq!(attempted, 3);
}

/// A blocked action (for example an `ask_user_question`) stops the batch: nothing queued after
/// it may run before the user has answered.
#[test]
fn auto_accept_batch_stops_at_action_left_pending() {
    let mut calls = Vec::new();
    let attempted = run_auto_accept_batch(&["a", "q", "c"], |id| {
        calls.push(*id);
        match *id {
            "q" => AutoAcceptOutcome::LeftPending,
            _ => AutoAcceptOutcome::Started,
        }
    });

    assert_eq!(calls, vec!["a", "q"]);
    assert_eq!(attempted, 2);
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
    assert!(hidden_command_blocks_user_accept(conversation, &long_token, &none_shown));
    assert!(hidden_command_blocks_user_accept(conversation, &blank_padding, &none_shown));
    assert!(!hidden_command_blocks_user_accept(conversation, &short, &none_shown));
    assert!(!hidden_command_blocks_user_accept(conversation, &question, &none_shown));

    let mut shown = HashMap::new();
    shown.insert((conversation, long_token.id.clone()), long_text.clone());
    assert!(!hidden_command_blocks_user_accept(conversation, &long_token, &shown));
    // A mark is per action, per conversation: the same id elsewhere is not covered.
    assert!(hidden_command_blocks_user_accept(other_conversation, &long_token, &shown));
    assert!(hidden_command_blocks_user_accept(conversation, &blank_padding, &shown));

    // Review B1: a mark for one text does not cover another. The user edits the command,
    // shows the edit in full, and then an accept path runs a different (original) text.
    let mut shown_edit = HashMap::new();
    shown_edit.insert(
        (conversation, long_token.id.clone()),
        format!("{long_text} # edited"),
    );
    assert!(hidden_command_blocks_user_accept(conversation, &long_token, &shown_edit));
}
