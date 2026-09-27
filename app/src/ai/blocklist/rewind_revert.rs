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
//! Lanes are keyed by [`RevertLaneKey`] — the resolved file, not its
//! spelling — and outlive a single rewind, so neither two spellings of one
//! file nor two overlapping rewinds can race on it.
//!
//! [`RewindReverts`] is the terminal view's use of it: it also owns the diff
//! views for as long as any of their writes is outstanding — the rewind
//! removes their blocks at once, and the views' write outcomes, refusal
//! toasts and late "reverted" marks would otherwise be dropped with them —
//! and knows each rewind's pre-rewind backup, which records the reverts that
//! landed.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::path::Path;

use warp_core::HostId;
use warp_util::standardized_path::StandardizedPath;
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
/// Long-lived: reverts added later (a second rewind while the first one's
/// writes are still out) queue behind the ones already in their file's lane,
/// so two rewinds never race on one file either.
///
/// Generic over the revert `U` and the file key `K` so the ordering can be
/// tested without views or a filesystem.
#[derive(Debug)]
pub struct RevertSequence<U, K> {
    lanes: Vec<Lane<U>>,
    lane_of_file: HashMap<K, usize>,
}

impl<U, K> Default for RevertSequence<U, K> {
    fn default() -> Self {
        Self {
            lanes: Vec::new(),
            lane_of_file: HashMap::new(),
        }
    }
}

impl<U, K: Eq + Hash> RevertSequence<U, K> {
    /// A sequence holding `reverts`; see [`Self::add`].
    pub fn new(reverts: impl IntoIterator<Item = (Option<K>, U)>) -> Self {
        let mut sequence = Self::default();
        sequence.add(reverts);
        sequence
    }

    /// Queues `reverts`, newest first, each with the file it writes, behind
    /// whatever is already queued for that file. Reverts with no file
    /// (nothing they could write to) are each their own lane. Nothing is
    /// dispatched until [`Self::start`].
    pub fn add(&mut self, reverts: impl IntoIterator<Item = (Option<K>, U)>) {
        if self.is_settled() {
            // Nothing outstanding: drop the idle lanes so a long session does
            // not accumulate one per file ever reverted.
            self.lanes.clear();
            self.lane_of_file.clear();
        }
        let lanes = &mut self.lanes;
        for (file, revert) in reverts {
            let lane = match file {
                Some(file) => *self.lane_of_file.entry(file).or_insert_with(|| {
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
    }

    /// Dispatches the next revert of every file that has none in flight.
    ///
    /// Returns the reverts abandoned because a newer revert of the same file
    /// settled as not reverted during dispatch; the caller must tell each one
    /// it will not run.
    pub fn start(&mut self, mut dispatch: impl FnMut(&U) -> RevertStart) -> Vec<U> {
        let mut abandoned = Vec::new();
        for lane in 0..self.lanes.len() {
            if self.lanes[lane].in_flight.is_none() {
                self.advance(lane, &mut dispatch, &mut abandoned);
            }
        }
        abandoned
    }

    /// Records the outcome of the in-flight revert `is_it` picks out, and
    /// dispatches the next-older revert of its file if it landed.
    ///
    /// `None`, changing nothing, if no in-flight revert matches — an outcome
    /// this sequence is not waiting on. Otherwise the reverts abandoned: every
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

    /// Whether any revert `matches` picks out is still queued or in flight.
    pub fn holds(&self, matches: impl Fn(&U) -> bool) -> bool {
        self.lanes
            .iter()
            .any(|lane| lane.in_flight.iter().chain(&lane.waiting).any(&matches))
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

/// Which file a revert writes, for ordering: two reverts with the same key
/// never run at once.
///
/// Not the path as the agent spelled it. Two spellings of one file — through
/// a symlink, or differing only in case on a case-insensitive filesystem —
/// would get two lanes and race, which is the bug lanes exist to prevent. A
/// local path is resolved (symlinks, `..`) and, where the platform's usual
/// filesystem ignores case, case-folded; over-merging two files that differ
/// only in case would merely serialise them. A remote path is kept as spelled,
/// qualified by its host: it cannot be resolved from here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RevertLaneKey {
    host: Option<HostId>,
    path: String,
}

impl RevertLaneKey {
    /// The key for `path` on `host` (`None`: the local filesystem). Touches the
    /// filesystem for a local path.
    pub fn new(host: Option<&HostId>, path: &StandardizedPath) -> Self {
        match host {
            Some(host) => Self {
                host: Some(host.clone()),
                path: path.as_str().to_owned(),
            },
            None => Self {
                host: None,
                path: fold_case(
                    path.to_local_path()
                        .map(|local| resolve_local_path(&local))
                        .unwrap_or_else(|| path.as_str().to_owned()),
                ),
            },
        }
    }
}

/// `path` with symlinks and relative components resolved: the file itself if
/// it exists, else its parent directory (a revert may target a file that is
/// not there right now), else `path` unchanged.
fn resolve_local_path(path: &Path) -> String {
    if let Ok(resolved) = dunce::canonicalize(path) {
        return resolved.to_string_lossy().into_owned();
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(parent) = dunce::canonicalize(parent) {
            return parent.join(name).to_string_lossy().into_owned();
        }
    }
    path.to_string_lossy().into_owned()
}

/// Case-folds where the platform's default filesystem is case-insensitive
/// (macOS, Windows), so `Foo.rs` and `foo.rs` share a lane there.
fn fold_case(path: String) -> String {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        path.to_lowercase()
    } else {
        path
    }
}

/// One file of one diff card, as a rewind reverts it.
pub struct FileRevert {
    /// The rewind this revert belongs to.
    pub batch: u64,
    /// A strong handle: see [`RewindReverts`].
    pub view: ViewHandle<CodeDiffView>,
    /// Index into the card's pending diffs.
    pub file_idx: usize,
}

/// One rewind's share of [`RewindReverts`].
struct RewindBatch {
    id: u64,
    /// Every card this rewind started reverting.
    views: Vec<ViewHandle<CodeDiffView>>,
    /// The pre-rewind backup, forked before the reverts ran. The reverts that
    /// land are recorded into it once the batch settles, so it shows what the
    /// rewind actually undid.
    backup_conversation_id: Option<AIConversationId>,
}

/// A rewind whose reverts have all settled or been abandoned.
pub struct SettledRewind {
    views: Vec<ViewHandle<CodeDiffView>>,
    pub backup_conversation_id: Option<AIConversationId>,
}

impl SettledRewind {
    /// The actions of the cards this rewind reverted — every file's write
    /// landed.
    pub fn reverted_actions(&self, app: &impl ViewAsRef) -> Vec<AIAgentActionId> {
        self.views
            .iter()
            .map(|view| view.as_ref(app))
            .filter(|view| matches!(view.state(), CodeDiffState::Reverted))
            .map(|view| view.action_id().clone())
            .collect()
    }
}

/// Every rewind's file reverts in one terminal view, from dispatch until each
/// has settled.
///
/// One [`RevertSequence`] for all of them, so a second rewind's revert of a
/// file queues behind the first rewind's instead of racing it.
///
/// Owned by the `TerminalView`, not by the diff views: a rewind removes the
/// reverted blocks immediately, which drops the views — and their `FileModel`
/// subscriptions — before any write comes back. The write itself still ran,
/// but its refusal toast and the card's late "reverted" mark were lost with
/// the view. Holding strong handles here keeps every view alive until its
/// rewind settles.
#[derive(Default)]
pub struct RewindReverts {
    pub sequence: RevertSequence<FileRevert, RevertLaneKey>,
    batches: Vec<RewindBatch>,
    next_batch: u64,
}

impl RewindReverts {
    /// Queues one rewind's reverts: `cards` newest first, each with the files
    /// [`CodeDiffView::begin_revert`] returned for it. Call
    /// `sequence.start` to dispatch what can run.
    pub fn add_rewind(
        &mut self,
        cards: Vec<(
            ViewHandle<CodeDiffView>,
            Vec<(usize, Option<RevertLaneKey>)>,
        )>,
        backup_conversation_id: Option<AIConversationId>,
    ) {
        let batch = self.next_batch;
        self.next_batch += 1;
        let views = cards.iter().map(|(view, _)| view.clone()).collect();
        self.sequence
            .add(cards.into_iter().flat_map(|(view, files)| {
                files.into_iter().map(move |(file_idx, file)| {
                    (
                        file,
                        FileRevert {
                            batch,
                            view: view.clone(),
                            file_idx,
                        },
                    )
                })
            }));
        self.batches.push(RewindBatch {
            id: batch,
            views,
            backup_conversation_id,
        });
    }

    /// Removes and returns every rewind none of whose reverts is still queued
    /// or in flight, releasing the diff views it kept alive once the caller
    /// drops it.
    pub fn take_settled(&mut self) -> Vec<SettledRewind> {
        let (settled, outstanding): (Vec<_>, Vec<_>) = std::mem::take(&mut self.batches)
            .into_iter()
            .partition(|batch| !self.sequence.holds(|revert| revert.batch == batch.id));
        self.batches = outstanding;
        settled
            .into_iter()
            .map(|batch| SettledRewind {
                views: batch.views,
                backup_conversation_id: batch.backup_conversation_id,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "rewind_revert_tests.rs"]
mod tests;
