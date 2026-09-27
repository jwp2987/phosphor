//! The file reverts a rewind performs, as one batch (#686).
//!
//! Rewinding a conversation undoes every accepted agent edit after the rewind
//! point. Each revert is a guarded write (#672): it only lands if the file
//! still holds exactly what that edit's accept wrote. Two edits to the same
//! file therefore have to be undone strictly newest first — the older edit's
//! accepted text is only back on disk once the newer edit's revert has
//! restored *its* base. Dispatched together, the older write races the newer
//! one, probes the file while it still holds the newer edit, and is refused.
//!
//! [`RevertSequence`] is that ordering, and nothing else: one lane per file,
//! newest revert first, the next one dispatched only once the previous one has
//! settled. Different files are different lanes and run concurrently. A lane
//! stops at the first revert that does not land; the older ones behind it are
//! abandoned rather than attempted, because the file no longer holds what
//! they would assert (and if it somehow did, restoring an older base over a
//! newer edit the user chose to keep would be wrong).
//!
//! [`RewindRevertBatch`] is the rewind's use of it: it also owns the diff
//! views for as long as any of their writes is outstanding — the rewind
//! removes their blocks at once, and the views' write outcomes, refusal
//! toasts and late "reverted" marks would otherwise be dropped with them —
//! and knows the pre-rewind backup, which records the reverts that landed.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use warpui::{ViewAsRef, ViewHandle};

use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::inline_action::code_diff_view::{CodeDiffState, CodeDiffView};

/// What dispatching one revert did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevertStart {
    /// A guarded write is in flight; its outcome arrives later.
    InFlight,
    /// Settled already, not reverted: refused before anything was written, or
    /// nothing to write to. No outcome will follow.
    NotReverted,
}

/// The reverts touching one file, in the order they must run.
#[derive(Debug)]
struct Lane<U> {
    /// Not yet dispatched, newest first.
    waiting: VecDeque<U>,
    /// Dispatched, outcome not yet known.
    in_flight: Option<U>,
}

impl<U> Default for Lane<U> {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
            in_flight: None,
        }
    }
}

/// Runs each file's reverts strictly newest → oldest, one at a time, and
/// different files' reverts concurrently. See the module docs.
///
/// Generic over the revert `U` so the ordering can be tested without views.
#[derive(Debug)]
pub struct RevertSequence<U> {
    lanes: Vec<Lane<U>>,
}

impl<U> RevertSequence<U> {
    /// `reverts` newest first, each with the file it writes. Reverts with no
    /// file (nothing they could write to) are each their own lane.
    pub fn new<K: Eq + Hash>(reverts: impl IntoIterator<Item = (Option<K>, U)>) -> Self {
        let mut lanes: Vec<Lane<U>> = Vec::new();
        let mut lane_of_file: HashMap<K, usize> = HashMap::new();
        for (file, revert) in reverts {
            let lane = match file {
                Some(file) => *lane_of_file.entry(file).or_insert_with(|| {
                    lanes.push(Lane::default());
                    lanes.len() - 1
                }),
                None => {
                    lanes.push(Lane::default());
                    lanes.len() - 1
                }
            };
            lanes[lane].waiting.push_back(revert);
        }
        Self { lanes }
    }

    /// Dispatches the newest revert of every file.
    ///
    /// Returns the reverts abandoned because a newer revert of the same file
    /// settled as not reverted during dispatch; the caller must tell each one
    /// it will not run.
    pub fn start(&mut self, mut dispatch: impl FnMut(&U) -> RevertStart) -> Vec<U> {
        let mut abandoned = Vec::new();
        for lane in 0..self.lanes.len() {
            self.advance(lane, &mut dispatch, &mut abandoned);
        }
        abandoned
    }

    /// Records the outcome of the in-flight revert `is_it` picks out, and
    /// dispatches the next-older revert of its file if it landed.
    ///
    /// `None`, changing nothing, if no in-flight revert matches — an outcome
    /// this batch is not waiting on. Otherwise the reverts abandoned: every
    /// remaining one of the file if this one did not land (or if a later one
    /// dispatched here settled as not reverted).
    pub fn settled(
        &mut self,
        is_it: impl Fn(&U) -> bool,
        reverted: bool,
        mut dispatch: impl FnMut(&U) -> RevertStart,
    ) -> Option<Vec<U>> {
        let lane = self
            .lanes
            .iter()
            .position(|lane| lane.in_flight.as_ref().is_some_and(&is_it))?;
        self.lanes[lane].in_flight = None;

        let mut abandoned = Vec::new();
        if reverted {
            self.advance(lane, &mut dispatch, &mut abandoned);
        } else {
            abandoned.extend(self.lanes[lane].waiting.drain(..));
        }
        Some(abandoned)
    }

    /// Whether every revert has either settled or been abandoned.
    pub fn is_settled(&self) -> bool {
        self.lanes
            .iter()
            .all(|lane| lane.in_flight.is_none() && lane.waiting.is_empty())
    }

    /// Dispatches the next waiting revert of `lane`. A revert that settles
    /// during dispatch is one that did *not* land, so the rest of the lane is
    /// abandoned.
    fn advance(
        &mut self,
        lane: usize,
        dispatch: &mut impl FnMut(&U) -> RevertStart,
        abandoned: &mut Vec<U>,
    ) {
        let lane = &mut self.lanes[lane];
        debug_assert!(lane.in_flight.is_none());
        let Some(next) = lane.waiting.pop_front() else {
            return;
        };
        match dispatch(&next) {
            RevertStart::InFlight => lane.in_flight = Some(next),
            RevertStart::NotReverted => abandoned.extend(lane.waiting.drain(..)),
        }
    }
}

/// One file of one diff card, as a rewind reverts it.
pub struct FileRevert {
    /// A strong handle: see [`RewindRevertBatch`].
    pub view: ViewHandle<CodeDiffView>,
    /// Index into the card's pending diffs.
    pub file_idx: usize,
}

/// The reverts one rewind performs, from dispatch until every one has settled.
///
/// Owned by the `TerminalView` that rewound, not by the diff views: the rewind
/// removes the reverted blocks immediately, which drops the views — and their
/// `FileModel` subscriptions — before any write comes back. The write itself
/// still runs, but its refusal toast and the card's late "reverted" mark were
/// lost with the view. Holding strong handles here keeps every view alive
/// until the batch settles.
pub struct RewindRevertBatch {
    pub sequence: RevertSequence<FileRevert>,
    /// Every card this rewind started reverting.
    views: Vec<ViewHandle<CodeDiffView>>,
    /// The pre-rewind backup, forked before the reverts ran. The reverts that
    /// land are recorded into it once the batch settles, so it shows what the
    /// rewind actually undid.
    backup_conversation_id: Option<AIConversationId>,
}

impl RewindRevertBatch {
    /// `cards` newest first, each with the files [`CodeDiffView::begin_revert`]
    /// returned for it (newest first within a card, too).
    pub fn new<K: Eq + Hash>(
        cards: Vec<(ViewHandle<CodeDiffView>, Vec<(usize, Option<K>)>)>,
        backup_conversation_id: Option<AIConversationId>,
    ) -> Self {
        let views = cards.iter().map(|(view, _)| view.clone()).collect();
        let sequence = RevertSequence::new(cards.into_iter().flat_map(|(view, files)| {
            files.into_iter().map(move |(file_idx, file)| {
                (
                    file,
                    FileRevert {
                        view: view.clone(),
                        file_idx,
                    },
                )
            })
        }));
        Self {
            sequence,
            views,
            backup_conversation_id,
        }
    }

    pub fn is_settled(&self) -> bool {
        self.sequence.is_settled()
    }

    pub fn backup_conversation_id(&self) -> Option<AIConversationId> {
        self.backup_conversation_id
    }

    /// The actions of the cards this batch reverted — every file's write
    /// landed. Meaningful once [`Self::is_settled`].
    pub fn reverted_actions(&self, app: &impl ViewAsRef) -> Vec<AIAgentActionId> {
        self.views
            .iter()
            .map(|view| view.as_ref(app))
            .filter(|view| matches!(view.state(), CodeDiffState::Reverted))
            .map(|view| view.action_id().clone())
            .collect()
    }
}

#[cfg(test)]
#[path = "rewind_revert_tests.rs"]
mod tests;
