//! Ordering state between typed queries and the results shown for them (#1365).
//!
//! The menu's query reaches the mixer through a debounce channel, so for a moment after a key
//! the displayed results (and the main-menu category filter) answer an OLDER query. An Enter in
//! that window must never act on them. The gate records whether a newer query is outstanding
//! and whether an Enter is waiting for it; completion is signalled by the query being applied and
//! the mixer reporting results for it, never by elapsed time. The debounce stays as pacing only.

#[derive(Debug, Default)]
pub(super) struct QueryGate {
    /// The newest query handed to the debounce channel that has not been applied yet.
    requested: Option<String>,
    /// The applied query's results have not been reported by the mixer yet.
    awaiting_results: bool,
    /// An Enter arrived while the displayed results were stale and is waiting for fresh ones.
    select_waiting: bool,
    /// Bumped whenever a wait begins or is cancelled, so a give-up timer only cancels its own wait.
    wait_id: u64,
    /// The newest query requested (outstanding or applied since); `None` after a reset.
    latest: Option<String>,
    /// The query the waiting Enter was typed after (#1645): the editor's edit event for it had not
    /// reached the menu yet when the Enter did. Its arrival is that Enter's own query, not the
    /// user typing on.
    expected: Option<String>,
}

impl QueryGate {
    /// A new query was typed. Any Enter waiting on an older query is cancelled: the user kept
    /// typing, so that Enter no longer refers to what is on screen.
    ///
    /// The same query again is not typing: the editor re-reports the filter on every edit event,
    /// and an edit's event can be handled after an Enter that followed it (a replayed key, #1633).
    /// That request changes nothing, so it neither cancels the waiting Enter (which would lose it)
    /// nor marks the shown results stale.
    pub(super) fn request(&mut self, query: String) {
        if self.latest.as_deref() == Some(query.as_str()) {
            return;
        }
        self.latest = Some(query.clone());
        // The waiting Enter followed the edit that produced exactly this query: keep it, it now
        // waits for this query's results (the query is outstanding, so the gate is stale).
        let keep_select = self.select_waiting && self.expected.as_deref() == Some(query.as_str());
        self.requested = Some(query);
        if keep_select {
            self.expected = None;
        } else {
            self.cancel_select();
        }
    }

    /// An Enter arrived with the editor's buffer already holding `typed` as the filter. When that
    /// is not the newest query the gate has seen, the edit event for it is still on its way
    /// (#1645): the Enter must wait for that query rather than act on the old results, and that
    /// query's arrival must not cancel it. `None` (no editor text known) or a fresh gate: no wait.
    pub(super) fn edit_pending_for(&self, typed: Option<&str>) -> bool {
        match (typed, self.latest.as_deref()) {
            (Some(typed), Some(latest)) => typed != latest,
            _ => false,
        }
    }

    /// The debounce delivered `query` to the search bar. `results_will_change` is false when the
    /// search bar already held that exact query, so the mixer will not report anything new.
    pub(super) fn applied(&mut self, query: &str, results_will_change: bool) {
        if self.requested.as_deref() == Some(query) {
            self.requested = None;
        }
        self.awaiting_results = results_will_change;
    }

    /// The mixer reported results (for the most recently applied query).
    pub(super) fn results_reported(&mut self) {
        self.awaiting_results = false;
    }

    /// Whether what is displayed answers an older query than the one the user typed.
    pub(super) fn is_stale(&self) -> bool {
        self.requested.is_some() || self.awaiting_results
    }

    /// Records an Enter that must wait for fresh results; returns the id for its give-up timer.
    pub(super) fn defer_select(&mut self) -> u64 {
        self.defer_select_after(None)
    }

    /// As [`Self::defer_select`] for an Enter typed after the edit that makes `expected` the
    /// query (its event not yet seen, #1645).
    pub(super) fn defer_select_after(&mut self, expected: Option<String>) -> u64 {
        self.expected = expected;
        self.select_waiting = true;
        self.wait_id = self.wait_id.wrapping_add(1);
        self.wait_id
    }

    pub(super) fn select_waiting(&self) -> bool {
        self.select_waiting
    }

    /// Takes the waiting Enter if the results are now current. `still_loading` is the mixer's
    /// own state for sources that have not finished (the search bar ignores Enter then).
    pub(super) fn take_ready_select(&mut self, still_loading: bool) -> bool {
        if self.select_waiting && !self.is_stale() && !still_loading {
            self.select_waiting = false;
            self.expected = None;
            return true;
        }
        false
    }

    pub(super) fn cancel_select(&mut self) {
        self.expected = None;
        if self.select_waiting {
            self.select_waiting = false;
            self.wait_id = self.wait_id.wrapping_add(1);
        }
    }

    /// Give-up safety net: cancels the wait `id` if it is still the current one. Returns whether
    /// it cancelled anything (the caller logs).
    pub(super) fn give_up(&mut self, id: u64) -> bool {
        if self.select_waiting && self.wait_id == id {
            self.select_waiting = false;
            self.expected = None;
            // The signal never came; stop treating the display as stale so later Enters work.
            self.awaiting_results = false;
            return true;
        }
        false
    }

    /// The id of the current wait, as its give-up timer holds it.
    #[cfg(test)]
    pub(super) fn wait_id_for_test(&self) -> u64 {
        self.wait_id
    }

    /// Forget everything (menu closed or reset).
    pub(super) fn reset(&mut self) {
        self.requested = None;
        self.latest = None;
        self.awaiting_results = false;
        self.cancel_select();
    }
}

#[cfg(test)]
#[path = "query_gate_tests.rs"]
mod tests;
