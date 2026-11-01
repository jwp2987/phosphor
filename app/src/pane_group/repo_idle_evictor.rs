//! Idle eviction of auto-indexed repository file trees (#869).
//!
//! `DetectedRepositories` makes `LocalRepoMetadataModel` index every repo a terminal ever
//! enters, and nothing ever removed one, so a process that visits many repos keeps every file
//! tree (up to 200k entries each) for its whole life.
//!
//! This singleton watches the pane groups' working directories (the single place that feeds the
//! file tree, code review and global search views) and, once a repo has had no pane group
//! inside it for [`IDLE_GRACE`], calls `remove_repository`. It is deliberately conservative:
//!
//! * only repos indexed *solely* by terminal navigation are candidates
//!   (`LocalRepoMetadataModel::idle_evictable_repositories`); anything a code-review view,
//!   user-added workspace or other caller asked for stays resident;
//! * a repo is "referenced" if any pane group directory or repo root equals it, lies inside it,
//!   or contains it;
//! * when a pane group's directories later point into an evicted repo, detection is re-run so
//!   it is re-indexed exactly as on first entry (this also covers restoring a closed tab).

use instant::Instant;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use repo_metadata::repositories::{DetectedRepositories, RepoDetectionSource};
use repo_metadata::{RepoMetadataModel, RepositoryIdentifier};
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::standardized_path::StandardizedPath;
use warpui::{Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

use super::working_directories::{WorkingDirectoriesEvent, WorkingDirectoriesModel};

/// How long a repo must be unreferenced before it is evicted.
pub(crate) const IDLE_GRACE: Duration = Duration::from_secs(5 * 60);

/// Whether a repo at `repo` is referenced by any of `paths` (equal, inside, or containing).
pub(crate) fn is_referenced<'a>(repo: &Path, mut paths: impl Iterator<Item = &'a PathBuf>) -> bool {
    paths.any(|path| path.starts_with(repo) || repo.starts_with(path))
}

/// Pure bookkeeping of how long each candidate repo has been unreferenced.
#[derive(Default)]
pub(crate) struct IdleTracker {
    idle_since: HashMap<PathBuf, Instant>,
}

impl IdleTracker {
    /// Feeds one observation. `candidates` are the evictable repos; `referenced` says whether a
    /// pane group is still in one. Returns the repos idle for at least `grace`.
    pub(crate) fn observe(
        &mut self,
        now: Instant,
        grace: Duration,
        candidates: &[PathBuf],
        referenced: impl Fn(&Path) -> bool,
    ) -> Vec<PathBuf> {
        let live: HashSet<&PathBuf> = candidates.iter().collect();
        self.idle_since.retain(|path, _| live.contains(path));

        let mut due = Vec::new();
        for path in candidates {
            if referenced(path) {
                self.idle_since.remove(path);
                continue;
            }
            let since = *self.idle_since.entry(path.clone()).or_insert(now);
            if now.duration_since(since) >= grace {
                due.push(path.clone());
            }
        }
        for path in &due {
            self.idle_since.remove(path);
        }
        due
    }

    /// How long from `now` until the earliest tracked repo reaches `grace`, or `None` when no
    /// repo is currently idle (so nothing needs a timer).
    pub(crate) fn next_due_in(&self, now: Instant, grace: Duration) -> Option<Duration> {
        self.idle_since
            .values()
            .map(|since| grace.saturating_sub(now.saturating_duration_since(*since)))
            .min()
    }
}

#[derive(Default)]
pub struct RepoIdleEvictor {
    directories: HashMap<EntityId, Vec<PathBuf>>,
    repositories: HashMap<EntityId, Vec<PathBuf>>,
    tracker: IdleTracker,
    /// Repo roots this model evicted, so a later visit can re-trigger detection.
    evicted: HashSet<PathBuf>,
    /// Identifies the one eviction timer that is armed; a firing from a replaced timer is stale.
    timer_generation: u64,
    /// When the armed timer is due, if one is armed.
    timer_due: Option<Instant>,
}

impl Entity for RepoIdleEvictor {
    type Event = ();
}

impl SingletonEntity for RepoIdleEvictor {}

fn local_paths(paths: impl IntoIterator<Item = LocalOrRemotePath>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter_map(|path| match path {
            LocalOrRemotePath::Local(path) => Some(path),
            LocalOrRemotePath::Remote(_) => None,
        })
        .collect()
}

impl RepoIdleEvictor {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self::default()
    }

    /// Starts tracking one window's working-directories model.
    pub fn watch(
        &mut self,
        working_directories: &ModelHandle<WorkingDirectoriesModel>,
        ctx: &mut ModelContext<Self>,
    ) {
        ctx.subscribe_to_model(working_directories, |me, event, ctx| {
            me.handle_event(event, ctx);
        });
    }

    fn handle_event(&mut self, event: &WorkingDirectoriesEvent, ctx: &mut ModelContext<Self>) {
        match event {
            WorkingDirectoriesEvent::DirectoriesChanged {
                pane_group_id,
                directories,
            } => {
                let paths = local_paths(directories.iter().map(|dir| dir.path.clone()));
                self.revisit_evicted(&paths, ctx);
                if paths.is_empty() {
                    self.directories.remove(pane_group_id);
                } else {
                    self.directories.insert(*pane_group_id, paths);
                }
            }
            WorkingDirectoriesEvent::RepositoriesChanged {
                pane_group_id,
                repositories,
            } => {
                let paths = local_paths(repositories.iter().cloned());
                self.revisit_evicted(&paths, ctx);
                if paths.is_empty() {
                    self.repositories.remove(pane_group_id);
                } else {
                    self.repositories.insert(*pane_group_id, paths);
                }
            }
            WorkingDirectoriesEvent::FocusedRepoChanged { .. } => return,
        }
        // Whether a repo is idle only changes when a pane group's directories do, so this is the
        // moment to start (or restart) its grace clock; there is no periodic sweep.
        self.evaluate_and_arm(ctx);
    }

    /// Re-runs repo detection for paths that moved into a repo we evicted, which makes
    /// `LocalRepoMetadataModel` index it again.
    fn revisit_evicted(&mut self, paths: &[PathBuf], ctx: &mut ModelContext<Self>) {
        if self.evicted.is_empty() {
            return;
        }
        let mut to_detect: Vec<PathBuf> = Vec::new();
        self.evicted.retain(|root| {
            // Only a path inside the evicted root gets that root indexed again. A path that
            // is merely an ancestor of the root would detect some other (or no) repo and
            // leave the root unindexed, so the root must stay marked as evicted.
            match paths.iter().find(|path| path.starts_with(root)) {
                Some(path) => {
                    to_detect.push(path.clone());
                    false
                }
                None => true,
            }
        });
        for path in to_detect {
            let Some(path) = path.to_str() else {
                continue;
            };
            DetectedRepositories::handle(ctx).update(ctx, |detected, ctx| {
                // The returned future only reports the root; indexing is triggered by the
                // `DetectedGitRepo` event, so it is fine to drop it.
                drop(detected.detect_possible_git_repo(
                    path,
                    RepoDetectionSource::TerminalNavigation,
                    ctx,
                ));
            });
        }
    }

    /// Refreshes the idle clocks, evicts what is due, and arms ONE timer for the earliest repo
    /// still waiting out its grace period. Runs on a `WorkingDirectoriesEvent` (the only thing
    /// that makes a repo newly idle or referenced again) and when that timer fires; with no idle
    /// repo nothing is armed, so an idle app has no wakeups from here.
    ///
    /// The timer itself is genuine pacing: `IDLE_GRACE` is a retention policy ("keep a repo's
    /// tree five minutes after the last tab leaves"), so elapsed time is the requirement.
    fn evaluate_and_arm(&mut self, ctx: &mut ModelContext<Self>) {
        self.evict_due(ctx);
        let now = Instant::now();
        let Some(wait) = self.tracker.next_due_in(now, IDLE_GRACE) else {
            // Nothing idle: any armed timer is now pointless, so make it stale.
            self.timer_generation = self.timer_generation.wrapping_add(1);
            self.timer_due = None;
            return;
        };
        let due = now + wait;
        // Keep an armed timer that fires no later than needed; an early firing just re-arms.
        if self.timer_due.is_some_and(|armed| armed <= due) {
            return;
        }
        self.timer_generation = self.timer_generation.wrapping_add(1);
        self.timer_due = Some(due);
        let generation = self.timer_generation;
        let _ = ctx.spawn(
            async move {
                warpui::r#async::Timer::after(wait).await;
            },
            move |me, _, ctx| {
                if me.timer_generation != generation {
                    return;
                }
                me.timer_due = None;
                me.evaluate_and_arm(ctx);
            },
        );
    }

    fn evict_due(&mut self, ctx: &mut ModelContext<Self>) {
        let candidates: Vec<(StandardizedPath, PathBuf)> = RepoMetadataModel::as_ref(ctx)
            .idle_evictable_local_repositories(ctx)
            .into_iter()
            .filter_map(|path| path.to_local_path().map(|local| (path, local)))
            .collect();
        let local_candidates: Vec<PathBuf> =
            candidates.iter().map(|(_, local)| local.clone()).collect();

        let directories = &self.directories;
        let repositories = &self.repositories;
        let due = self
            .tracker
            .observe(Instant::now(), IDLE_GRACE, &local_candidates, |repo| {
                is_referenced(repo, directories.values().flatten())
                    || is_referenced(repo, repositories.values().flatten())
            });

        for (std_path, local) in candidates {
            if !due.contains(&local) {
                continue;
            }
            let id = RepositoryIdentifier::Local(std_path.clone());
            let removed = RepoMetadataModel::handle(ctx)
                .update(ctx, |model, ctx| model.remove_repository(&id, ctx));
            match removed {
                Ok(()) => {
                    log::info!("Evicted idle repo file tree: {std_path}");
                    self.evicted.insert(local);
                }
                Err(err) => log::debug!("Idle eviction of {std_path} skipped: {err}"),
            }
        }
    }
}

#[cfg(test)]
#[path = "repo_idle_evictor_tests.rs"]
mod tests;
