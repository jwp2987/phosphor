use super::*;
use crate::ai::agent::{AIAgentAction, AIAgentActionId, AIAgentActionType, task::TaskId};
use std::collections::HashSet;

fn create_test_action(id: AIAgentActionId) -> AIAgentAction {
    AIAgentAction {
        id,
        task_id: TaskId::new("fake-task".to_owned()),
        action: AIAgentActionType::RequestCommandOutput {
            command: "test".to_string(),
            is_read_only: None,
            is_risky: None,
            rationale: None,
            uses_pager: None,
            wait_until_completion: true,
            citations: vec![],
        },
        requires_result: false,
    }
}

fn generate_new_action_id() -> AIAgentActionId {
    AIAgentActionId::from(uuid::Uuid::new_v4().to_string())
}

#[test]
fn test_single_batch_done() {
    let mut actions = PendingPreprocessedActions::default();
    let action_id = generate_new_action_id();

    let mut action_ids = HashSet::new();
    action_ids.insert(action_id.clone());

    let preprocess_id = actions.insert_preprocess_action_batch(action_ids);

    let test_action = create_test_action(action_id.clone());
    let result_actions = vec![test_action.clone()];

    let queued_actions = actions.handle_preprocess_actions_result(preprocess_id, result_actions);

    // Verify the action is returned
    assert_eq!(queued_actions.len(), 1);
    assert_eq!(queued_actions[0].id, action_id);

    // Verify the batch is removed
    assert_eq!(actions.0.len(), 0);
}

#[test]
fn test_multiple_batches() {
    let mut actions = PendingPreprocessedActions::default();

    // Insert three batches.
    let action_id1 = generate_new_action_id();
    let mut action_ids1 = HashSet::new();
    action_ids1.insert(action_id1.clone());
    let preprocess_id1 = actions.insert_preprocess_action_batch(action_ids1);

    let action_id2 = generate_new_action_id();
    let mut action_ids2 = HashSet::new();
    action_ids2.insert(action_id2.clone());
    let preprocess_id2 = actions.insert_preprocess_action_batch(action_ids2);

    let action_id3 = generate_new_action_id();
    let mut action_ids3 = HashSet::new();
    action_ids3.insert(action_id3.clone());
    let preprocess_id3 = actions.insert_preprocess_action_batch(action_ids3);

    // Process the last batch. Should return nothing since the batch is not done.
    let test_action3 = create_test_action(action_id3.clone());
    let result_actions3 = vec![test_action3.clone()];
    let queued_actions3 = actions.handle_preprocess_actions_result(preprocess_id3, result_actions3);

    // Nothing should be returned: the first two batches are not done.
    assert_eq!(queued_actions3.len(), 0);
    assert_eq!(actions.0.len(), 3);

    // Process the second-to-last-batch.
    let test_action2 = create_test_action(action_id2.clone());
    let result_actions2 = vec![test_action2.clone()];

    let queued_actions = actions.handle_preprocess_actions_result(preprocess_id2, result_actions2);

    // Nothing should be returned: the first batch is not done.
    assert_eq!(queued_actions.len(), 0);
    assert_eq!(actions.0.len(), 3);

    // Process the first batch.
    let test_action1 = create_test_action(action_id1.clone());
    let result_actions1 = vec![test_action1.clone()];
    let queued_actions = actions.handle_preprocess_actions_result(preprocess_id1, result_actions1);

    // All of the batches are done--they should all be returned and the internal queue should be empty.
    assert_eq!(queued_actions.len(), 3);
    assert_eq!(actions.0.len(), 0);
    assert_eq!(queued_actions[0].id, action_id1);
    assert_eq!(queued_actions[1].id, action_id2);
    assert_eq!(queued_actions[2].id, action_id3);
}

fn batch_of(actions: &mut PendingPreprocessedActions) -> (AIAgentActionId, PreprocessId) {
    let id = generate_new_action_id();
    let mut ids = HashSet::new();
    ids.insert(id.clone());
    (id, actions.insert_preprocess_action_batch(ids))
}

/// #1859: a batch that never resolves parks every later batch; abandoning it releases them.
#[test]
fn abandon_pending_unparks_later_batches() {
    let mut actions = PendingPreprocessedActions::default();
    let (_hung_id, _hung) = batch_of(&mut actions);
    let (ask_id, ask_batch) = batch_of(&mut actions);

    // The ask's batch completes first and is parked behind the hung one.
    let parked = actions
        .handle_preprocess_actions_result(ask_batch, vec![create_test_action(ask_id.clone())]);
    assert!(parked.is_empty());

    let abandoned = actions.abandon_pending();
    assert_eq!(abandoned.len(), 1);

    // The next turn's batch drains the abandoned marker and the parked ask.
    let (next_id, next_batch) = batch_of(&mut actions);
    let out = actions
        .handle_preprocess_actions_result(next_batch, vec![create_test_action(next_id.clone())]);
    let ids: Vec<_> = out.iter().map(|a| a.id.clone()).collect();
    assert_eq!(ids, vec![ask_id, next_id]);
    assert_eq!(actions.0.len(), 0);
}

/// #1944: a Stop has to know whether it dropped a batch that was still preprocessing (its
/// actions get no result at the stop, so nothing else ends the turn). A batch that finished and
/// is merely parked, an abandoned one, and no batch at all are not that.
#[test]
fn has_pending_is_true_only_while_a_batch_is_still_preprocessing() {
    let mut actions = PendingPreprocessedActions::default();
    assert!(!actions.has_pending());

    let (first_id, first_batch) = batch_of(&mut actions);
    let (second_id, second_batch) = batch_of(&mut actions);
    assert!(actions.has_pending());

    // The second batch finishes and is parked behind the first: one is still preprocessing.
    let parked = actions.handle_preprocess_actions_result(
        second_batch,
        vec![create_test_action(second_id.clone())],
    );
    assert!(parked.is_empty());
    assert!(actions.has_pending());

    // The first finishes: both drain, nothing is preprocessing.
    let out = actions
        .handle_preprocess_actions_result(first_batch, vec![create_test_action(first_id.clone())]);
    assert_eq!(out.len(), 2);
    assert!(!actions.has_pending());

    // An abandoned batch is no longer preprocessing as far as the queue is concerned.
    let (_hung_id, _hung_batch) = batch_of(&mut actions);
    assert!(actions.has_pending());
    actions.abandon_pending();
    assert!(!actions.has_pending());
}

/// #1859: a late result from an abandoned batch must not resurrect its actions.
#[test]
fn abandoned_batch_late_result_is_dropped() {
    let mut actions = PendingPreprocessedActions::default();
    let (id, batch) = batch_of(&mut actions);
    actions.abandon_pending();
    let out = actions.handle_preprocess_actions_result(batch, vec![create_test_action(id)]);
    assert!(out.is_empty());
    assert_eq!(actions.0.len(), 0);
}

/// #1859: cancel drops everything and returns the already-preprocessed, parked actions.
#[test]
fn abandon_all_returns_parked_actions() {
    let mut actions = PendingPreprocessedActions::default();
    let (_hung_id, _hung) = batch_of(&mut actions);
    let (ask_id, ask_batch) = batch_of(&mut actions);
    actions.handle_preprocess_actions_result(ask_batch, vec![create_test_action(ask_id.clone())]);

    let parked = actions.abandon_all();
    assert_eq!(parked.len(), 1);
    assert_eq!(parked[0].id, ask_id);
    assert_eq!(actions.0.len(), 0);
}

/// Owner decision 2026-11-09: an earlier-turn command that finished preprocessing but was parked
/// behind a hung batch is handed back by `release_ready` after `abandon_pending` so the caller can
/// cancel it; it is never part of what a later batch returns to run.
#[test]
fn stale_parked_command_is_released_for_cancel_not_queued_with_new_turn() {
    let mut actions = PendingPreprocessedActions::default();
    let (_hung_id, _hung) = batch_of(&mut actions);
    let (stale_id, stale_batch) = batch_of(&mut actions);
    actions
        .handle_preprocess_actions_result(stale_batch, vec![create_test_action(stale_id.clone())]);

    actions.abandon_pending();
    let stale = actions.release_ready();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id, stale_id);

    let (new_id, new_batch) = batch_of(&mut actions);
    let out = actions
        .handle_preprocess_actions_result(new_batch, vec![create_test_action(new_id.clone())]);
    let ids: Vec<_> = out.iter().map(|a| a.id.clone()).collect();
    assert_eq!(ids, vec![new_id], "only the new turn's action runs");
}

/// #1894: the actions in hand when an ABANDONED batch's late result arrives are handed back to be
/// cancelled (they never execute, but still owe a result); same when the batch is missing.
#[test]
fn late_result_of_an_abandoned_or_missing_batch_hands_back_its_actions_to_cancel() {
    let mut actions = PendingPreprocessedActions::default();
    let (id, batch) = batch_of(&mut actions);
    actions.abandon_pending();
    let (queue, stale) = actions.handle_preprocess_actions_result_with_stale(
        batch.clone(),
        vec![create_test_action(id.clone())],
    );
    assert!(queue.is_empty());
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id, id);
    assert_eq!(actions.0.len(), 0);

    // The marker is gone now: the same id again is a missing batch.
    let (queue, stale) = actions
        .handle_preprocess_actions_result_with_stale(batch, vec![create_test_action(id.clone())]);
    assert!(queue.is_empty());
    assert_eq!(stale.len(), 1);
}

/// #1894: a live batch hands back nothing to cancel.
#[test]
fn live_batch_result_has_no_stale_actions() {
    let mut actions = PendingPreprocessedActions::default();
    let (id, batch) = batch_of(&mut actions);
    let (queue, stale) =
        actions.handle_preprocess_actions_result_with_stale(batch, vec![create_test_action(id)]);
    assert_eq!(queue.len(), 1);
    assert!(stale.is_empty());
}
