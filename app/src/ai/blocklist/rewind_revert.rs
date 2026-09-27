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

/// One revert queued or in flight, and every lane it belongs to.
///
/// A plain edit belongs to one lane (its file). A rename belongs to two — the
/// source and the destination — because it is, at once, the newest thing to
/// happen to the source and (from that point on) part of the destination's
/// history too; see the module docs' "Rename identity" section. Removed (set
/// to `None` in [`RevertSequence::jobs`]) the moment it is dispatched-and-not-
/// reverted, abandoned, or settled — never left `Some` past that point, which
/// is what lets [`RevertSequence::is_settled`] and [`RevertSequence::holds`]
/// just check for `None`/`Some` rather than tracking state twice.
#[derive(Debug)]
struct Job<U> {
    payload: U,
    /// Indices into [`RevertSequence::lanes`]. Never empty: a revert with no
    /// file to write gets one dedicated lane of its own (see
    /// [`RevertSequence::add`]).
    lanes: Vec<usize>,
}

/// The reverts touching one file, in the order they must run — as job ids
/// into [`RevertSequence::jobs`], since a job spanning more than one lane
/// (a rename) has to be found and cleared in all of them together.
#[derive(Debug, Default)]
struct Lane {
    /// Not yet dispatched, newest first.
    waiting: VecDeque<usize>,
    /// Dispatched, outcome not yet known.
    in_flight: Option<usize>,
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
///
/// # Rename identity (#686 follow-up)
///
/// A rename card is the newest event of its *source* lane and, from that
/// point in time on, part of its *destination* lane too: a later edit to the
/// destination has to revert before the rename is undone, and the rename has
/// to be undone before anything older on the source side is. A job that
/// belongs to only one of those two lanes can be scheduled behind an edit to
/// the *other* lane's file while the sequence still thinks it is free to run
/// — two reverts touching the same physical file (post-rename) racing each
/// other exactly the way this type exists to prevent for a single lane.
///
/// [`Job::lanes`] is what fixes that: a job is only dispatched once it is the
/// newest undispatched entry in *every* lane it belongs to, simultaneously
/// (see [`Self::advance`]), and while in flight it occupies (and so blocks)
/// every one of those lanes. Settling — landed or not — releases all of them
/// at once. A plain edit's `lanes` has exactly one entry, so this collapses
/// to the original single-lane behaviour for every non-rename revert.
#[derive(Debug)]
pub struct RevertSequence<U, K> {
    lanes: Vec<Lane>,
    lane_of_file: HashMap<K, usize>,
    /// Every job ever added and not yet fully settled or abandoned, indexed
    /// by the id [`Lane::waiting`]/[`Lane::in_flight`] refer to it by. `None`
    /// is a tombstone: the job has been dealt with through *one* of its
    /// lanes (dispatched-and-refused, abandoned, or settled) but may still
    /// have a stale id sitting in one of its *other* lanes' queues —
    /// [`Self::peek_lane_front`] is what skips those rather than
    /// re-processing or re-reporting them.
    jobs: Vec<Option<Job<U>>>,
}

impl<U, K> Default for RevertSequence<U, K> {
    fn default() -> Self {
        Self {
            lanes: Vec::new(),
            lane_of_file: HashMap::new(),
            jobs: Vec::new(),
        }
    }
}

impl<U, K: Eq + Hash> RevertSequence<U, K> {
    /// A sequence holding `reverts`; see [`Self::add`].
    pub fn new<L: IntoIterator<Item = K>>(reverts: impl IntoIterator<Item = (L, U)>) -> Self {
        let mut sequence = Self::default();
        sequence.add(reverts);
        sequence
    }

    /// Queues `reverts`, newest first, each with the lane(s) it belongs to —
    /// [`Option<K>`] for the common case of one file (or none), an owned
    /// `Vec<K>` (or any other `IntoIterator<Item = K>`) for a job spanning
    /// more than one, such as a rename's source and destination. Two lane
    /// keys that are `==` share a lane no matter which reverts they arrive
    /// through, so this call and an earlier one queue behind each other
    /// correctly. An empty set of lanes gets one dedicated lane of its own,
    /// so reverts with nothing to write never block each other. Nothing is
    /// dispatched until [`Self::start`].
    pub fn add<L: IntoIterator<Item = K>>(&mut self, reverts: impl IntoIterator<Item = (L, U)>) {
        if self.is_settled() {
            // Nothing outstanding: drop the idle lanes and jobs so a long
            // session does not accumulate one per file ever reverted.
            self.lanes.clear();
            self.lane_of_file.clear();
            self.jobs.clear();
        }
        let lanes = &mut self.lanes;
        let lane_of_file = &mut self.lane_of_file;
        let jobs = &mut self.jobs;
        for (lane_keys, payload) in reverts {
            let mut lane_indices = Vec::new();
            for key in lane_keys {
                let index = *lane_of_file.entry(key).or_insert_with(|| {
                    lanes.push(Lane::default());
                    lanes.len() - 1
                });
                if !lane_indices.contains(&index) {
                    lane_indices.push(index);
                }
            }
            if lane_indices.is_empty() {
                lanes.push(Lane::default());
                lane_indices.push(lanes.len() - 1);
            }

            let id = jobs.len();
            jobs.push(Some(Job {
                payload,
                lanes: lane_indices.clone(),
            }));
            for lane in lane_indices {
                lanes[lane].waiting.push_back(id);
            }
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
    /// dispatches the next-older revert of every lane it occupied if it
    /// landed.
    ///
    /// `None`, changing nothing, if no in-flight revert matches — an outcome
    /// this sequence is not waiting on. Otherwise the reverts abandoned:
    /// every remaining one of every lane the settled job touched, if it did
    /// not land (or if a later one dispatched here settled as not reverted).
    pub fn settled(
        &mut self,
        is_it: impl Fn(&U) -> bool,
        reverted: bool,
        mut dispatch: impl FnMut(&U) -> RevertStart,
    ) -> Option<Vec<U>> {
        let mut found = None;
        for lane in &self.lanes {
            if let Some(id) = lane.in_flight {
                if self.jobs[id]
                    .as_ref()
                    .is_some_and(|job| is_it(&job.payload))
                {
                    found = Some(id);
                    break;
                }
            }
        }
        let id = found?;
        let job_lanes = self.jobs[id]
            .as_ref()
            .expect("just confirmed present above")
            .lanes
            .clone();

        // The settled job's own payload is consumed either way: a landed
        // revert has already been reported through the caller's own
        // side-channel (the event that led here), and a refused one is
        // reported by whoever dispatched it, not by ending up in `abandoned`.
        self.jobs[id] = None;
        for &lane in &job_lanes {
            self.lanes[lane].in_flight = None;
        }

        let mut abandoned = Vec::new();
        if reverted {
            for lane in job_lanes {
                self.advance(lane, &mut dispatch, &mut abandoned);
            }
        } else {
            for lane in job_lanes {
                self.drain_lane(lane, &mut abandoned);
            }
        }
        Some(abandoned)
    }

    /// Whether every revert has either settled or been abandoned.
    pub fn is_settled(&self) -> bool {
        self.jobs.iter().all(Option::is_none)
    }

    /// Whether any revert `matches` picks out is still queued or in flight.
    pub fn holds(&self, matches: impl Fn(&U) -> bool) -> bool {
        self.jobs
            .iter()
            .flatten()
            .any(|job| matches(&job.payload))
    }

    /// Dispatches the next waiting job of `lane_idx`, but only once it is
    /// also the newest undispatched job of *every other* lane it belongs to
    /// (a plain edit has no other lane, so this is immediate for it) — see
    /// the type's "Rename identity" doc. A job that settles as not-reverted
    /// during its own dispatch abandons the rest of every lane it touched;
    /// the loop then re-checks `lane_idx`'s (now shorter) queue.
    fn advance(
        &mut self,
        lane_idx: usize,
        dispatch: &mut impl FnMut(&U) -> RevertStart,
        abandoned: &mut Vec<U>,
    ) {
        loop {
            let Some(id) = self.peek_lane_front(lane_idx) else {
                return;
            };
            let job_lanes = self.jobs[id]
                .as_ref()
                .expect("peek_lane_front only returns ids of live jobs")
                .lanes
                .clone();

            let mut ready = true;
            for &lane in &job_lanes {
                if self.lanes[lane].in_flight.is_some() || self.peek_lane_front(lane) != Some(id) {
                    ready = false;
                    break;
                }
            }
            if !ready {
                // Waiting on another of this job's lanes to reach it too.
                return;
            }

            for &lane in &job_lanes {
                self.lanes[lane].waiting.pop_front();
                self.lanes[lane].in_flight = Some(id);
            }
            let start = dispatch(
                &self.jobs[id]
                    .as_ref()
                    .expect("just marked in flight, still present")
                    .payload,
            );
            match start {
                RevertStart::InFlight => return,
                RevertStart::NotReverted => {
                    for &lane in &job_lanes {
                        self.lanes[lane].in_flight = None;
                    }
                    self.jobs[id] = None;
                    for &lane in &job_lanes {
                        self.drain_lane(lane, abandoned);
                    }
                    // `lane_idx` is one of `job_lanes`, so it was just
                    // drained; loop back to see what (if anything) is left.
                }
            }
        }
    }

    /// The front of `lane_idx`'s waiting queue, skipping (and discarding) any
    /// leading ids whose job has already been tombstoned by
    /// [`Self::abandon`] acting through a *different* lane the same job
    /// belonged to.
    fn peek_lane_front(&mut self, lane_idx: usize) -> Option<usize> {
        loop {
            let front = *self.lanes[lane_idx].waiting.front()?;
            if self.jobs[front].is_some() {
                return Some(front);
            }
            self.lanes[lane_idx].waiting.pop_front();
        }
    }

    /// Abandons every job still waiting in `lane_idx`, cascading into any
    /// other lane a multi-lane job among them also belongs to.
    fn drain_lane(&mut self, lane_idx: usize, abandoned: &mut Vec<U>) {
        while let Some(id) = self.lanes[lane_idx].waiting.pop_front() {
            self.abandon(id, abandoned);
        }
    }

    /// Abandons job `id`: removes it (and, transitively, everything queued
    /// behind it) from every lane it belongs to, and records its payload in
    /// `abandoned`. A no-op if `id` was already tombstoned — by this same
    /// call's own recursion, or by a sibling lane's [`Self::drain_lane`] —
    /// so cascading into a job's other lanes can never double-report or
    /// double-remove it.
    fn abandon(&mut self, id: usize, abandoned: &mut Vec<U>) {
        let Some(job) = self.jobs[id].take() else {
            return;
        };
        for lane in job.lanes {
            if self.lanes[lane].in_flight == Some(id) {
                self.lanes[lane].in_flight = None;
            }
            let Some(pos) = self.lanes[lane].waiting.iter().position(|&x| x == id) else {
                continue;
            };
            // `id` itself, plus everything older than it in this lane: all
            // of it is unreachable now that `id` will never run.
            let rest = self.lanes[lane].waiting.split_off(pos);
            for other in rest {
                if other != id {
                    self.abandon(other, abandoned);
                }
            }
        }
        abandoned.push(job.payload);
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

    /// The key for a *local* path given directly, rather than through a
    /// [`StandardizedPath`] — for a rename's destination, which
    /// [`InlineDiffView::write_action`] hands back as a raw [`PathBuf`] (it is
    /// never remote: a remote rename has already resolved to an in-place
    /// write by the time `write_action` returns, so this constructor is only
    /// ever reached for a real, local move).
    ///
    /// [`InlineDiffView::write_action`]: crate::code::inline_diff::InlineDiffView::write_action
    pub fn for_local_path(path: &Path) -> Self {
        Self {
            host: None,
            path: fold_case(resolve_local_path(path)),
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
    /// [`CodeDiffView::begin_revert`] returned for it — one entry per file,
    /// paired with every lane key that file's write touches (two, for a
    /// rename: source and destination; see [`RevertSequence`]'s "Rename
    /// identity" doc). Call `sequence.start` to dispatch what can run.
    pub fn add_rewind(
        &mut self,
        cards: Vec<(ViewHandle<CodeDiffView>, Vec<(usize, Vec<RevertLaneKey>)>)>,
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
