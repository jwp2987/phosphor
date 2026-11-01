//! Ordering for "open this file in the code editor" requests (#1370 F5).
//!
//! `Workspace::open_code` stats the file off the UI thread before it opens it, so the results of
//! two quick requests can come back in either order. Every request therefore gets an id and the
//! tab that was active when the user asked, and results are applied strictly in request order:
//! a finished request waits behind an earlier one that is still being checked. Order is by
//! request (FIFO), not latest-wins, because every open is something the user asked for and each
//! must happen (opening B and silently dropping A would lose a file), and "A then B" must leave
//! B focused like the old synchronous open did.
//!
//! The only timer is the caller's give-up for a stat that never returns (a hung mount): it calls
//! [`CodeOpenQueue::give_up`], which logs, releases the requests queued behind it, and makes a
//! late result for it a no-op. Nothing here depends on elapsed time for correctness.

use std::{collections::VecDeque, path::PathBuf};

use warpui::EntityId;

/// What became of the size check for one request.
#[derive(Debug, PartialEq, Eq)]
pub enum CodeOpenOutcome {
    /// The check finished; these paths are over the size limit (usually empty).
    Checked(Vec<(PathBuf, u64)>),
    /// The check never returned and the caller gave up on it.
    GaveUp,
}

enum State {
    Pending,
    Done(CodeOpenOutcome),
}

struct Entry<T> {
    id: u64,
    target: EntityId,
    payload: T,
    state: State,
}

/// A request whose turn has come, with the tab (pane group id) it was made from.
pub struct ReadyCodeOpen<T> {
    pub target: EntityId,
    pub payload: T,
    pub outcome: CodeOpenOutcome,
}

pub struct CodeOpenQueue<T> {
    next_id: u64,
    entries: VecDeque<Entry<T>>,
}

impl<T> Default for CodeOpenQueue<T> {
    fn default() -> Self {
        Self {
            next_id: 0,
            entries: VecDeque::new(),
        }
    }
}

impl<T> CodeOpenQueue<T> {
    /// Registers a request whose size check is running; returns its id.
    pub fn push_pending(&mut self, target: EntityId, payload: T) -> u64 {
        self.push(target, payload, State::Pending)
    }

    /// Registers a request that needs no check (it still waits behind earlier pending ones).
    pub fn push_checked(&mut self, target: EntityId, payload: T) -> u64 {
        self.push(
            target,
            payload,
            State::Done(CodeOpenOutcome::Checked(Vec::new())),
        )
    }

    fn push(&mut self, target: EntityId, payload: T, state: State) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push_back(Entry {
            id,
            target,
            payload,
            state,
        });
        id
    }

    /// Records the check result for `id` and returns every request now free to open, in request
    /// order. Unknown ids (already given up on) are ignored.
    pub fn resolve(&mut self, id: u64, oversized: Vec<(PathBuf, u64)>) -> Vec<ReadyCodeOpen<T>> {
        self.finish(id, CodeOpenOutcome::Checked(oversized))
    }

    /// Gives up on `id` (its check never returned). Only acts on a request still pending.
    pub fn give_up(&mut self, id: u64) -> Vec<ReadyCodeOpen<T>> {
        let still_pending = self
            .entries
            .iter()
            .any(|e| e.id == id && matches!(e.state, State::Pending));
        if !still_pending {
            return Vec::new();
        }
        self.finish(id, CodeOpenOutcome::GaveUp)
    }

    /// Requests at the front that are free to open (used right after `push_checked`).
    pub fn drain_ready(&mut self) -> Vec<ReadyCodeOpen<T>> {
        let mut ready = Vec::new();
        while matches!(
            self.entries.front(),
            Some(Entry {
                state: State::Done(_),
                ..
            })
        ) {
            let Some(Entry {
                target,
                payload,
                state: State::Done(outcome),
                ..
            }) = self.entries.pop_front()
            else {
                break;
            };
            ready.push(ReadyCodeOpen {
                target,
                payload,
                outcome,
            });
        }
        ready
    }

    fn finish(&mut self, id: u64, outcome: CodeOpenOutcome) -> Vec<ReadyCodeOpen<T>> {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.state = State::Done(outcome);
        }
        self.drain_ready()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "code_open_queue_tests.rs"]
mod tests;
