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
