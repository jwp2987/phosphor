use std::collections::{HashSet, VecDeque};

use uuid::Uuid;

use crate::ai::agent::{AIAgentAction, AIAgentActionId};

/// A unique ID for a batch of preprocessed actions.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreprocessId(String);

impl PreprocessId {
    fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

/// A list of pending preprocessed actions.
/// Each action goes through a preprocessing step where executors
/// can asynchronously do arbitrary work and store any state as needed.
/// Upon completing the preprocessing step for a batch of actions, consumers can
/// call `handle_process_actions_result` to get the actions that are ready to be queued.
#[derive(Default, Debug)]
pub(super) struct PendingPreprocessedActions(VecDeque<PreprocessActionBatch>);

impl PendingPreprocessedActions {
    pub fn contains(&self, action_id: &AIAgentActionId) -> bool {
        self.0.iter().any(|action| action.contains(action_id))
    }

    /// Test-only (agent fuzzer): every action id still in a preprocessing batch, with whether
    /// it is parked with nothing ahead to wait for. Abandoned batches are not listed.
    #[cfg(any(test, feature = "test-util"))]
    pub fn action_ids_for_test(&self) -> Vec<(AIAgentActionId, bool)> {
        let mut pending_ahead = false;
        let mut out = Vec::new();
        for batch in self.0.iter() {
            match batch.status {
                PreprocessActionStatus::Pending => {
                    pending_ahead = true;
                    out.extend(batch.action_ids.iter().cloned().map(|id| (id, false)));
                }
                PreprocessActionStatus::Abandoned => {}
                PreprocessActionStatus::Done { .. } => {
                    out.extend(
                        batch
                            .action_ids
                            .iter()
                            .cloned()
                            .map(|id| (id, !pending_ahead)),
                    );
                }
            }
        }
        out
    }

    /// Returns the actions that are ready to be queued now that the group of actions identified by [`PreprocessId`] have completed.
    /// NOTE this may return actions that have been completed earlier to maintain the invariant that actions are returned in the
    /// order they are added.
    pub fn handle_preprocess_actions_result(
        &mut self,
        preprocess_id: PreprocessId,
        actions: Vec<AIAgentAction>,
    ) -> Vec<AIAgentAction> {
        self.handle_preprocess_actions_result_with_stale(preprocess_id, actions)
            .0
    }

    /// [`Self::handle_preprocess_actions_result`] that also hands back the actions IN HAND whose
    /// batch is missing or abandoned (#1894): they will never be queued, so the caller must give
    /// each a cancelled result (a turn always ends in an answerable card). Returns
    /// `(actions to queue, stale actions to cancel)`.
    pub fn handle_preprocess_actions_result_with_stale(
        &mut self,
        preprocess_id: PreprocessId,
        actions: Vec<AIAgentAction>,
    ) -> (Vec<AIAgentAction>, Vec<AIAgentAction>) {
        let mut stale = Vec::new();
        let ready = self.handle_preprocess_actions_result_inner(preprocess_id, actions, &mut stale);
        (ready, stale)
    }

    fn handle_preprocess_actions_result_inner(
        &mut self,
        preprocess_id: PreprocessId,
        actions: Vec<AIAgentAction>,
        stale: &mut Vec<AIAgentAction>,
    ) -> Vec<AIAgentAction> {
        let mut actions_to_queue = Vec::with_capacity(actions.len());

        // Find the index of the action with the given preprocess_id
        let Some(current_index) = self.0.iter().position(|batch| batch.id == preprocess_id) else {
            log::warn!("Action not found in list of preprocessed actions");
            *stale = actions;
            return vec![];
        };

        // A batch abandoned by `abandon_pending` (cancel / new turn) must not resurrect its
        // actions when its future finally resolves; its marker is removed here; while it
        // waits it is an empty marker so later batches still drain in order.
        if matches!(
            self.0[current_index].status,
            PreprocessActionStatus::Abandoned
        ) {
            log::warn!(
                "hop=agent_action status=parked late preprocess result for an abandoned batch dropped"
            );
            self.0.remove(current_index);
            *stale = actions;
            return vec![];
        }

        // Check if there are any pending actions before the current one
        let has_pending_before = self
            .0
            .iter()
            .take(current_index)
            .any(|action| matches!(action.status, PreprocessActionStatus::Pending));

        if has_pending_before {
            // If there are pending actions before this one, just mark this one as done
            // and don't return any actions yet.
            log::warn!(
                "hop=agent_action status=parked ids={:?} waiting on an earlier preprocess batch",
                self.0[current_index].action_ids
            );
            self.0[current_index].status = PreprocessActionStatus::Done { actions };
            self.0[current_index].held = Vec::new();
            vec![]
        } else {
            // All actions before this one are done: process them all.

            // First, collect actions from all completed batches before this one
            for action in self.0.drain(..current_index) {
                match action.status {
                    PreprocessActionStatus::Pending => {
                        #[cfg(debug_assertions)]
                        panic!("Preprocess action batch should be completed but was pending")
                    }
                    PreprocessActionStatus::Abandoned => {}
                    PreprocessActionStatus::Done { actions } => {
                        actions_to_queue.extend(actions);
                    }
                }
            }

            // Then add the current batch's actions.
            actions_to_queue.extend(actions);

            // Remove the current batch.
            self.0.pop_front();

            // Process any subsequent completed batches.
            while let Some(action) = self.0.pop_front() {
                match action.status {
                    PreprocessActionStatus::Pending => {
                        self.0.push_front(action);
                        break;
                    }
                    PreprocessActionStatus::Abandoned => {}
                    PreprocessActionStatus::Done { actions } => {
                        actions_to_queue.extend(actions);
                    }
                }
            }

            actions_to_queue
        }
    }

    /// Marks every batch that is still `Pending` as `Abandoned` so it can no longer park the
    /// batches behind it (#1859). Returns the ids of the abandoned batches' actions (for
    /// logging). Batches already `Done` keep their actions and drain with the next completing
    /// batch.
    pub fn abandon_pending(&mut self) -> Vec<AIAgentActionId> {
        let mut abandoned = Vec::new();
        for batch in self.0.iter_mut() {
            if matches!(batch.status, PreprocessActionStatus::Pending) {
                batch.status = PreprocessActionStatus::Abandoned;
                abandoned.extend(batch.action_ids.iter().cloned());
            }
        }
        abandoned
    }

    /// Pops the leading batches that are no longer `Pending` and returns the actions of the
    /// finished ones. Used after `abandon_pending`: the caller CANCELS these stale actions
    /// (owner decision 2026-11-09), it never runs them.
    ///
    /// An `Abandoned` batch popped here is gone with its kept copies (#2136): whoever settles
    /// its calls takes them first ([`Self::take_actions_of_abandoned`]). Its late result is
    /// then that of a missing batch, which is stale all the same.
    pub fn release_ready(&mut self) -> Vec<AIAgentAction> {
        let mut ready = Vec::new();
        while let Some(batch) = self.0.pop_front() {
            match batch.status {
                PreprocessActionStatus::Pending => {
                    self.0.push_front(batch);
                    break;
                }
                PreprocessActionStatus::Abandoned => {}
                PreprocessActionStatus::Done { actions } => ready.extend(actions),
            }
        }
        ready
    }

    /// #1967: how many actions [`Self::release_ready`] would return: actions that finished
    /// preprocessing with no `Pending` batch ahead of them. Every path that completes or
    /// abandons a batch drains these in the same call, so a non-zero count that persists is a
    /// lost hand-off (#1859), not a wait.
    pub fn parked_count(&self) -> usize {
        let mut count = 0;
        for batch in self.0.iter() {
            match &batch.status {
                PreprocessActionStatus::Pending => break,
                PreprocessActionStatus::Abandoned => {}
                PreprocessActionStatus::Done { actions } => count += actions.len(),
            }
        }
        count
    }

    /// Whether any batch is still preprocessing (its actions are in the hands of its future, not
    /// here, so a cancel cannot give them results yet).
    pub fn has_pending(&self) -> bool {
        self.0
            .iter()
            .any(|batch| matches!(batch.status, PreprocessActionStatus::Pending))
    }

    /// Drops every batch (cancel). Returns the actions of batches that had finished
    /// preprocessing but were still parked, so the caller can give them cancel results.
    ///
    /// The calls of a batch that is still preprocessing are NOT returned: this is what a stop
    /// did before #2104, kept for the kill switch. See [`Self::abandon_all_settling`].
    pub fn abandon_all(&mut self) -> Vec<AIAgentAction> {
        self.abandon_all_settling().parked
    }

    /// #2104: [`Self::abandon_all`] that also hands back the calls of the batches that were
    /// still preprocessing, from the copies the batch keeps
    /// ([`Self::insert_preprocess_actions`]). The caller gives every returned action its
    /// cancelled result now, at the stop: the batch's future may come back much later or
    /// never (a hung apply, #1894), and until then nothing else would hold these calls. Both
    /// lists are in queue order.
    pub fn abandon_all_settling(&mut self) -> AbandonedBatches {
        let mut abandoned = AbandonedBatches::default();
        for batch in self.0.drain(..) {
            match batch.status {
                PreprocessActionStatus::Pending => abandoned.preprocessing.extend(batch.held),
                PreprocessActionStatus::Abandoned => {}
                PreprocessActionStatus::Done { actions } => abandoned.parked.extend(actions),
            }
        }
        abandoned
    }

    /// #2104: the kept copies of the calls of every batch [`Self::abandon_pending`] gave up
    /// on, taken out (a second call returns nothing). This call leaves the markers in place;
    /// the caller settles the returned calls now instead of when, or if, the batch's future
    /// comes back.
    ///
    /// #2136: call it BEFORE [`Self::release_ready`], which pops the abandoned markers at the
    /// front of the queue and their copies with them.
    pub fn take_actions_of_abandoned(&mut self) -> Vec<AIAgentAction> {
        let mut actions = Vec::new();
        for batch in self.0.iter_mut() {
            if matches!(batch.status, PreprocessActionStatus::Abandoned) {
                actions.append(&mut batch.held);
            }
        }
        actions
    }

    /// Inserts a batch of actions that need to be preprocessed. Returns a [`PreprocessId`] that
    /// uniquely identifies the batch. The batch knows the ids only, so a cancel cannot settle
    /// its calls while it is preprocessing: the action model uses
    /// [`Self::insert_preprocess_actions`].
    pub fn insert_preprocess_action_batch(
        &mut self,
        action_ids: HashSet<AIAgentActionId>,
    ) -> PreprocessId {
        let preprocess_id = PreprocessId::new();
        self.0.push_back(PreprocessActionBatch::new(
            preprocess_id.clone(),
            action_ids,
            Vec::new(),
        ));
        preprocess_id
    }

    /// #2104: [`Self::insert_preprocess_action_batch`] for `actions`, keeping a copy of each.
    /// The originals travel with the batch's preprocess future; the copies are what a stop
    /// (or a new turn) cancels while that future is still out.
    pub fn insert_preprocess_actions(&mut self, actions: &[AIAgentAction]) -> PreprocessId {
        let preprocess_id = PreprocessId::new();
        self.0.push_back(PreprocessActionBatch::new(
            preprocess_id.clone(),
            actions.iter().map(|action| action.id.clone()).collect(),
            actions.to_vec(),
        ));
        preprocess_id
    }
}

/// What [`PendingPreprocessedActions::abandon_all_settling`] dropped.
#[derive(Debug, Default, PartialEq)]
pub(super) struct AbandonedBatches {
    /// Actions that had finished preprocessing and were parked behind another batch.
    pub parked: Vec<AIAgentAction>,
    /// Actions of batches that were still preprocessing (the batch's kept copies).
    pub preprocessing: Vec<AIAgentAction>,
}

#[derive(Clone, Debug, PartialEq)]
enum PreprocessActionStatus {
    Pending,
    /// Given up on (cancel or a newer turn); contributes no actions.
    Abandoned,
    Done {
        actions: Vec<AIAgentAction>,
    },
}

/// A batch of actions that need to be preprocessed.
#[derive(Clone, Debug, PartialEq)]
struct PreprocessActionBatch {
    /// A unique identifier for this batch.
    id: PreprocessId,
    /// The current status of this batch.
    status: PreprocessActionStatus,
    /// Action IDs associated with this batch.
    action_ids: HashSet<AIAgentActionId>,
    /// #2104: a copy of each action of the batch, in queue order, for as long as the batch is
    /// not done (the originals are in the hands of its preprocess future). Empty for a batch
    /// inserted by ids alone, and once taken.
    held: Vec<AIAgentAction>,
}

impl PreprocessActionBatch {
    fn contains(&self, action_id: &AIAgentActionId) -> bool {
        self.action_ids.contains(action_id)
    }

    fn new(
        preprocess_id: PreprocessId,
        action_ids: HashSet<AIAgentActionId>,
        held: Vec<AIAgentAction>,
    ) -> Self {
        Self {
            id: preprocess_id,
            status: PreprocessActionStatus::Pending,
            action_ids,
            held,
        }
    }
}

#[cfg(test)]
#[path = "preprocess_tests.rs"]
mod tests;
