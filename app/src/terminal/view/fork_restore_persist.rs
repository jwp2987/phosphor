//! Bookkeeping for persisting the restored blocks of a forked conversation (#1265, #1363).
//!
//! A fork re-emits `BlockCompleted` for every restored block so it is written to the new
//! conversation. A block whose grids are still being parsed off the UI thread (#1254) would
//! serialize blank, so it has to wait. The wait is driven by the install event (the view
//! re-polls when a deferred restore lands), never by elapsed time: a slow parse (huge
//! scrollback, loaded machine) must not drop the block. The only timer is a stall watchdog,
//! the give-up for a parse that never completes at all, which is logged and reported.

use std::collections::HashSet;

use crate::terminal::model::block::BlockId;

/// A stalled wait (no deferred restore installed for this long) is given up on. A slow parse
/// keeps producing installs and so keeps re-arming this; only a parse that is gone (panicked,
/// executor torn down) trips it.
pub(super) const FORK_RESTORE_STALL_GIVE_UP: std::time::Duration =
    std::time::Duration::from_secs(60);

#[derive(Default)]
pub(super) struct ForkRestorePersist {
    /// Blocks already handed to the persistence path.
    done: HashSet<BlockId>,
    /// Some restored block of the fork was still pending at the last poll.
    waiting: bool,
    /// Bumped by every poll, so a watchdog armed for an older poll is stale.
    generation: u64,
    /// Blocks whose deferred restore failed (#1265), already reported to the user.
    failed: HashSet<BlockId>,
}

impl ForkRestorePersist {
    /// Starts persisting a new fork: forgets what an earlier one had done.
    pub(super) fn reset(&mut self) {
        *self = Self {
            generation: self.generation + 1,
            ..Self::default()
        };
    }

    /// Whether a fork is waiting for deferred restores to install.
    pub(super) fn is_waiting(&self) -> bool {
        self.waiting
    }

    /// The generation a watchdog armed now belongs to.
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// Takes the restored blocks of the fork as `(id, restore_pending)`. Returns the ids that
    /// are ready to persist now (each id is returned once, ever); the pending ones are left for
    /// a later poll, which the install event triggers.
    pub(super) fn poll(
        &mut self,
        candidates: impl IntoIterator<Item = (BlockId, bool)>,
    ) -> Vec<BlockId> {
        self.generation += 1;
        self.waiting = false;
        let mut ready = Vec::new();
        for (id, pending) in candidates {
            if self.done.contains(&id) {
                continue;
            }
            if pending {
                self.waiting = true;
            } else {
                self.done.insert(id.clone());
                ready.push(id);
            }
        }
        ready
    }

    /// Records blocks of the fork whose deferred restore failed: they are blank and are not
    /// saved (#1265). Returns true when any of them is newly reported, so the user is told once
    /// per block that the fork lost it.
    pub(super) fn note_failed(&mut self, ids: impl IntoIterator<Item = BlockId>) -> bool {
        let mut newly = false;
        for id in ids {
            newly |= self.failed.insert(id);
        }
        newly
    }

    /// The stall watchdog for `generation` fired. Returns true when it is the current one and
    /// the fork is still waiting, i.e. nothing installed since it was armed: the wait is
    /// abandoned.
    pub(super) fn give_up(&mut self, generation: u64) -> bool {
        if self.waiting && self.generation == generation {
            self.waiting = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[path = "fork_restore_persist_tests.rs"]
mod tests;
