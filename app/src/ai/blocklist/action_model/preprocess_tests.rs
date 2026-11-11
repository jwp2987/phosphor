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

/// #1967: `parked_count` is exactly what `release_ready` would hand out: the actions of the
/// leading batches that are done, up to the first batch still preprocessing. A batch that
/// waits behind a live one is a wait, not a lost hand-off, and is not counted.
#[test]
fn parked_count_counts_only_done_batches_with_nothing_pending_ahead() {
    let mut actions = PendingPreprocessedActions::default();
    assert_eq!(actions.parked_count(), 0);

    // first: still preprocessing; second: done, waiting behind it.
    let (_first_action, first) = batch_of(&mut actions);
    let (second_action, second) = batch_of(&mut actions);
    let queued = actions
        .handle_preprocess_actions_result(second, vec![create_test_action(second_action.clone())]);
    assert!(queued.is_empty());
    assert_eq!(
        actions.parked_count(),
        0,
        "done behind a batch that is still preprocessing: a wait"
    );

    // The first batch is given up on and nothing drains the second: now it is parked with
    // nothing ahead (the state every caller of `abandon_pending` drains at once).
    actions.abandon_pending();
    assert_eq!(actions.parked_count(), 1);

    // `release_ready` hands out exactly those, and nothing is parked afterwards.
    let released = actions.release_ready();
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].id, second_action);
    assert_eq!(actions.parked_count(), 0);
    assert!(!actions.has_pending());
    // The abandoned batch's late result is still dropped, not resurrected.
    let (queued, stale) = actions.handle_preprocess_actions_result_with_stale(first, Vec::new());
    assert!(queued.is_empty() && stale.is_empty());
}

/// #2104: a batch inserted with its actions keeps a copy of each, and a cancel hands back the
/// copies of every batch that is still preprocessing, next to the parked actions, in queue
/// order. `abandon_all` (the kill switch's arm) still returns the parked ones only.
///
/// Fails on the old code, where a batch knew ids only and a cancel could return nothing for it.
#[test]
fn abandon_all_settling_returns_the_calls_of_batches_still_preprocessing() {
    let (a, b, parked_id, c) = (
        generate_new_action_id(),
        generate_new_action_id(),
        generate_new_action_id(),
        generate_new_action_id(),
    );
    let build = || {
        let mut actions = PendingPreprocessedActions::default();
        let _first = actions.insert_preprocess_actions(&[
            create_test_action(a.clone()),
            create_test_action(b.clone()),
        ]);
        let parked_batch =
            actions.insert_preprocess_actions(&[create_test_action(parked_id.clone())]);
        let _last = actions.insert_preprocess_actions(&[create_test_action(c.clone())]);
        let queued = actions.handle_preprocess_actions_result(
            parked_batch,
            vec![create_test_action(parked_id.clone())],
        );
        assert!(queued.is_empty(), "parked behind the first batch");
        actions
    };

    let mut actions = build();
    assert!(actions.contains(&a) && actions.contains(&c));
    let abandoned = actions.abandon_all_settling();
    let ids = |list: &[AIAgentAction]| {
        list.iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&abandoned.parked), vec![parked_id.clone()]);
    assert_eq!(
        ids(&abandoned.preprocessing),
        vec![a.clone(), b.clone(), c.clone()],
        "every call of every batch still preprocessing, in queue order"
    );
    assert_eq!(actions.0.len(), 0);
    assert!(!actions.has_pending());

    let mut actions = build();
    let parked = actions.abandon_all();
    assert_eq!(ids(&parked), vec![parked_id.clone()]);
    assert_eq!(actions.0.len(), 0);

    // A batch inserted by ids alone has no copies to hand back (the older entry point).
    let mut actions = PendingPreprocessedActions::default();
    let _ = batch_of(&mut actions);
    let abandoned = actions.abandon_all_settling();
    assert!(abandoned.parked.is_empty() && abandoned.preprocessing.is_empty());
}

/// #2104: the copies are of batches that are still out. A batch that came back (queued, or
/// parked as `Done`) hands back nothing as "preprocessing", and its late-result bookkeeping is
/// unchanged: a result for a batch a cancel dropped is still handed back as stale (#1894); the
/// action model decides what to do with it.
#[test]
fn a_batch_that_came_back_is_not_handed_back_as_preprocessing() {
    let mut actions = PendingPreprocessedActions::default();
    let done_id = generate_new_action_id();
    let done = actions.insert_preprocess_actions(&[create_test_action(done_id.clone())]);
    let queued =
        actions.handle_preprocess_actions_result(done, vec![create_test_action(done_id.clone())]);
    assert_eq!(queued.len(), 1);
    assert!(actions.abandon_all_settling().preprocessing.is_empty());

    let late_id = generate_new_action_id();
    let late = actions.insert_preprocess_actions(&[create_test_action(late_id.clone())]);
    assert_eq!(actions.abandon_all_settling().preprocessing.len(), 1);
    let (queue, stale) = actions.handle_preprocess_actions_result_with_stale(
        late,
        vec![create_test_action(late_id.clone())],
    );
    assert!(queue.is_empty());
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id, late_id);
}

/// #2104: a new turn gives up on the batches still preprocessing (`abandon_pending`, #1859).
/// Their calls can be taken, once, to be settled then; the markers stay so that later batches
/// still drain in order and the late result is still recognised.
#[test]
fn take_actions_of_abandoned_hands_the_calls_back_once_and_keeps_the_markers() {
    let mut actions = PendingPreprocessedActions::default();
    let (old_a, old_b, new_id) = (
        generate_new_action_id(),
        generate_new_action_id(),
        generate_new_action_id(),
    );
    let old = actions.insert_preprocess_actions(&[
        create_test_action(old_a.clone()),
        create_test_action(old_b.clone()),
    ]);
    assert!(
        actions.take_actions_of_abandoned().is_empty(),
        "nothing is abandoned yet"
    );
    let abandoned_ids = actions.abandon_pending();
    assert_eq!(abandoned_ids.len(), 2);
    let new = actions.insert_preprocess_actions(&[create_test_action(new_id.clone())]);

    let taken = actions.take_actions_of_abandoned();
    assert_eq!(
        taken
            .iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>(),
        vec![old_a.clone(), old_b.clone()]
    );
    assert!(actions.take_actions_of_abandoned().is_empty(), "taken once");
    assert_eq!(actions.0.len(), 2, "the abandoned marker stays");

    // The new batch is not held up by the marker, and its own copies were not taken.
    let queued =
        actions.handle_preprocess_actions_result(new, vec![create_test_action(new_id.clone())]);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].id, new_id);
    // The old batch's late result is stale, as before (its marker went when the new batch
    // drained past it).
    let (queue, stale) = actions.handle_preprocess_actions_result_with_stale(
        old,
        vec![create_test_action(old_a), create_test_action(old_b)],
    );
    assert!(queue.is_empty());
    assert_eq!(stale.len(), 2);
    assert_eq!(actions.0.len(), 0);
}

/// #2136: the order a new turn uses (`queue_actions`): give up on what is still preprocessing,
/// take the kept copies of its calls, THEN release what was parked. `release_ready` pops the
/// abandoned markers, so the copies must be in hand before it; afterwards they are gone.
///
/// Fails on the old order (release, then take) at the first assertion: nothing was taken.
#[test]
fn a_new_turn_takes_the_abandoned_calls_before_the_parked_ones_are_released() {
    let mut actions = PendingPreprocessedActions::default();
    let (hung_a, hung_b, parked_id) = (
        generate_new_action_id(),
        generate_new_action_id(),
        generate_new_action_id(),
    );
    let hung = actions.insert_preprocess_actions(&[
        create_test_action(hung_a.clone()),
        create_test_action(hung_b.clone()),
    ]);
    let parked = actions.insert_preprocess_actions(&[create_test_action(parked_id.clone())]);
    let queued = actions
        .handle_preprocess_actions_result(parked, vec![create_test_action(parked_id.clone())]);
    assert!(queued.is_empty(), "parked behind the hung batch");

    assert_eq!(actions.abandon_pending().len(), 2);
    let still_preprocessing = actions.take_actions_of_abandoned();
    let stale = actions.release_ready();

    assert_eq!(
        still_preprocessing
            .iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>(),
        vec![hung_a.clone(), hung_b.clone()],
        "the calls of the batch given up on, in the reply's order"
    );
    assert_eq!(
        stale
            .iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>(),
        vec![parked_id]
    );
    assert_eq!(actions.0.len(), 0, "release_ready popped the markers too");
    assert!(
        actions.take_actions_of_abandoned().is_empty(),
        "which is why the copies are taken first"
    );

    // The hung batch's late result is that of a missing batch: handed back as stale, for the
    // caller to drop (its calls were settled at the new turn).
    let (queue, late) = actions.handle_preprocess_actions_result_with_stale(
        hung,
        vec![create_test_action(hung_a), create_test_action(hung_b)],
    );
    assert!(queue.is_empty());
    assert_eq!(late.len(), 2);
}
