//! Per-repository git watch that lets the daemon push **granular** diff-state
//! deltas instead of a full repo snapshot on every change (#577).
//!
//! # Why this exists
//!
//! The daemon's live-update path used to hang off `RepoMetadataEvent::RepositoryUpdated`,
//! which carries only a repository id — the per-file granularity is already gone
//! by the time that event is emitted. So the only thing the daemon could do was
//! recompute and re-serialize the entire repository's diff for every subscriber
//! on every keystroke-sized change, even though both ends of the wire have
//! supported per-file deltas the whole time: the proto defines
//! `DiffStateFileDelta`, the client decodes and folds it into its cached
//! `GitDiffData` (`code_review::diff_state_remote::apply_file_delta`), and the
//! builder [`super::diff_state_proto::build_diff_state_file_delta`] existed with
//! no non-test caller.
//!
//! This subscribes to the same git watcher the GUI's `LocalDiffStateModel` uses,
//! so the daemon sees the raw [`RepositoryUpdate`] and can classify it with
//! [`classify_repository_update`] — the *same* function the GUI classifies with,
//! deliberately shared rather than reimplemented so the two cannot drift on the
//! subtle cases (tracked-remote-ref moves, locked index, touched `.gitignore`).
//!
//! # Fallback
//!
//! Establishing a watch requires the daemon to have a detected, watched
//! `Repository` for the path. When that is unavailable the caller keeps the old
//! whole-snapshot push for that repo rather than going silent — losing live
//! updates entirely would be a far worse regression than sending them coarsely.
//! See `ServerModel::ensure_diff_state_watch`.

use std::path::PathBuf;

use async_channel::Sender;
use repo_metadata::repository::{Repository, RepositorySubscriber, SubscriberId};
use warpui::{ModelContext, ModelHandle};

use crate::code_review::diff_state::InvalidationBehavior;

/// What the watcher observed, already classified.
///
/// `Files` is the case that motivates all of this; the rest fall back to a full
/// snapshot exactly as before.
pub(super) enum DiffStateWatchUpdate {
    /// Specific non-ignored files changed — push a per-file delta for each.
    Files(Vec<PathBuf>),
    /// Something repo-wide changed (commit, remote ref, `.gitignore`) — push a
    /// full snapshot.
    All,
    /// The git index is locked (mid pull/merge). Recomputing now would read
    /// half-written state, so push nothing; the lock release triggers a fresh
    /// commit update that takes the `All` path.
    LockedIndex,
}

/// A live git watch on one repository, shared by every `(repo, mode)` diff-state
/// subscription for that repo — one watcher per repository, not per key, since
/// the filesystem events are identical across modes.
pub(super) struct DiffStateWatch {
    pub(super) repository: ModelHandle<Repository>,
    pub(super) subscriber_id: SubscriberId,
}

impl DiffStateWatch {
    pub(super) fn stop(&self, ctx: &mut warpui::AppContext) {
        let subscriber_id = self.subscriber_id;
        self.repository.update(ctx, |repository, ctx| {
            repository.stop_watching(subscriber_id, ctx);
        });
    }
}

/// Forwards classified watcher updates onto a channel the `ServerModel` streams.
///
/// Mirrors `code_review::diff_state::DiffStateModelRepositorySubscriber`, which
/// is private to that module. The locked-index detection is duplicated here
/// deliberately: it depends on `repository.git_dir()`, which is only reachable
/// from inside the subscriber callback.
pub(super) struct DiffStateTrackerSubscriber {
    pub(super) tx: Sender<(PathBuf, DiffStateWatchUpdate)>,
    pub(super) repo_root: PathBuf,
}

impl RepositorySubscriber for DiffStateTrackerSubscriber {
    fn on_scan(
        &mut self,
        _repository: &Repository,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        // The initial scan carries no delta; subscriptions are established
        // alongside a full snapshot reply, which already reflects it.
        Box::pin(async {})
    }

    fn on_files_updated(
        &mut self,
        repository: &Repository,
        update: &repo_metadata::RepositoryUpdate,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let tx = self.tx.clone();
        let repo_root = self.repo_root.clone();
        let update = update.clone();
        let index_lock_path = repository.git_dir().join("index.lock");
        Box::pin(async move {
            if update.commit_updated && async_fs::metadata(&index_lock_path).await.is_ok() {
                let _ = tx
                    .send((repo_root, DiffStateWatchUpdate::LockedIndex))
                    .await;
                return;
            }
            let Some(behavior) =
                crate::code_review::diff_state::classify_repository_update(update)
            else {
                return;
            };
            let classified = match behavior {
                InvalidationBehavior::Files(files) => DiffStateWatchUpdate::Files(files),
                InvalidationBehavior::AllLockedIndex => DiffStateWatchUpdate::LockedIndex,
                InvalidationBehavior::All(_) | InvalidationBehavior::PromptRefresh => {
                    DiffStateWatchUpdate::All
                }
            };
            let _ = tx.send((repo_root, classified)).await;
        })
    }
}
