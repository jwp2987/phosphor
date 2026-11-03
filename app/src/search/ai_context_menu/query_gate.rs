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
}

impl QueryGate {
    /// A new query was typed. Any Enter waiting on an older query is cancelled: the user kept
    /// typing, so that Enter no longer refers to what is on screen.
    pub(super) fn request(&mut self, query: String) {
        self.requested = Some(query);
        self.cancel_select();
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
            return true;
        }
        false
    }

    pub(super) fn cancel_select(&mut self) {
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
            // The signal never came; stop treating the display as stale so later Enters work.
            self.awaiting_results = false;
            return true;
        }
        false
    }

    /// Forget everything (menu closed or reset).
    pub(super) fn reset(&mut self) {
        self.requested = None;
        self.awaiting_results = false;
        self.cancel_select();
    }
}

#[cfg(test)]
#[path = "query_gate_tests.rs"]
mod tests;
