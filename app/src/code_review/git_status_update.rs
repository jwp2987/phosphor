use warpui::{Entity, SingletonEntity};

#[cfg(feature = "local_fs")]
use std::path::{Path, PathBuf};
#[cfg(feature = "local_fs")]
use warpui::{AppContext, ModelContext};

#[cfg(feature = "local_fs")]
use {
    crate::throttle::throttle,
    crate::util::git::{detect_current_branch_display, detect_main_branch, run_git_command},
    async_channel::Sender,
    repo_metadata::{
        repositories::DetectedRepositories,
        repository::{RepositorySubscriber, SubscriberId},
        Repository, RepositoryUpdate, RepositoryWatchMode,
    },
    std::{collections::HashMap, time::Duration},
    warp_util::local_or_remote_path::LocalOrRemotePath,
    warpui::{r#async::SpawnedFutureHandle, ModelHandle, WeakModelHandle},
};

#[cfg(feature = "local_fs")]
use super::diff_state::DiffStats;
#[cfg(feature = "local_fs")]
use super::git_status_update_remote::RemoteGitRepoStatusModel;
#[cfg(feature = "local_fs")]
use super::github_repo_model::{GitHubRepoModel, LocalGitHubRepoModel, RemoteGitHubRepoModel};
#[cfg(feature = "local_fs")]
use crate::context_chips::display_chip::GitBranchTrackingStatus;

/// Public metadata exposed to consumers — the subset of diff metadata
/// that the git chip (prompt display, agent view footer) needs.
///
/// Still `local_fs`-gated, unlike the pin (which gates only the *local*
/// backend and marks this type `allow(dead_code)` instead). In the `app` crate
/// `local_fs` is set by `build.rs` for exactly `target_family != "wasm"`
/// (`app/build.rs:245-249`), and the remote backend is wasm-gated for the same
/// reason `diff_state_remote` is — it drives `RemoteServerManager`. The two
/// predicates therefore select the same builds, so keeping the single existing
/// gate is equivalent to the pin's split one and avoids an enum with zero
/// live variants on wasm.
#[cfg(feature = "local_fs")]
#[derive(Debug, Clone)]
pub struct GitStatusMetadata {
    pub current_branch_name: String,
    pub main_branch_name: String,
    pub stats_against_head: DiffStats,
    pub branch_tracking_status: GitBranchTrackingStatus,
}

// ── GitStatusUpdateModel (singleton cache) ──────────────────────────────────

/// Singleton model that acts as a cache / factory for per-repository
/// [`GitRepoStatusModel`] instances.
///
/// Multiple terminals in the same repo share a single sub-model.  When the last
/// strong handle to a sub-model is dropped, the watcher is torn down
/// automatically.
/// The TUI's git-status singleton. Warp OSS names this `GitRepoModels` (a cache/factory of
/// per-repo status models); Zap's equivalent is [`GitStatusUpdateModel`], so this is an alias.
pub type GitRepoModels = GitStatusUpdateModel;

pub struct GitStatusUpdateModel {
    /// Per-repo status models, keyed by [`LocalOrRemotePath`] so one cache
    /// covers both local (watcher-backed) and remote (push receiver) repos.
    /// Ported from the pin's `GitRepoModels::git_status_models`; before this
    /// port the key was a bare `PathBuf` and only local repos could be cached.
    #[cfg(feature = "local_fs")]
    repos: HashMap<LocalOrRemotePath, WeakModelHandle<GitRepoStatusModel>>,
    /// Per-repo GitHub-info models (`gh pr view` / `gh repo view`), cached the
    /// same way as `repos` so every consumer in one repo shares a single model.
    #[cfg(feature = "local_fs")]
    github_repos: HashMap<LocalOrRemotePath, WeakModelHandle<GitHubRepoModel>>,
}

// ── Non-local_fs stub ───────────────────────────────────────────────────────

#[cfg(not(feature = "local_fs"))]
#[allow(dead_code)]
impl GitStatusUpdateModel {
    pub fn new() -> Self {
        Self {}
    }
}

// ── local_fs implementation ─────────────────────────────────────────────────

#[cfg(feature = "local_fs")]
impl GitStatusUpdateModel {
    pub fn new() -> Self {
        Self {
            repos: HashMap::new(),
            github_repos: HashMap::new(),
        }
    }

    /// Get or create the per-repo status model for `repo`, returning a unified
    /// [`GitRepoStatusModel`] handle that dispatches to a local watcher-backed
    /// model or a remote push receiver based on the location.
    ///
    /// If a live model already exists for this location, returns a new strong
    /// handle to it.  Otherwise, creates a new sub-model (with an active
    /// filesystem watcher for local repos, or an `UpdateGitStatus` subscription
    /// for remote ones) and returns a handle to it.
    ///
    /// Callers hold the returned `ModelHandle` for as long as they need updates.
    /// When all handles are dropped, the model (and its watcher) is torn down.
    pub fn subscribe(
        &mut self,
        repo: &LocalOrRemotePath,
        ctx: &mut ModelContext<Self>,
    ) -> anyhow::Result<ModelHandle<GitRepoStatusModel>> {
        // Check the cache for an existing live model.
        if let Some(handle) = self.repos.get(repo).and_then(|weak| weak.upgrade(ctx)) {
            return Ok(handle);
        }

        let handle = match repo {
            LocalOrRemotePath::Local(repo_path) => {
                let Some(repository_model) =
                    DetectedRepositories::as_ref(ctx).get_watched_repo_for_path(repo_path, ctx)
                else {
                    anyhow::bail!(
                        "No watched repository found for path: {}",
                        repo_path.display()
                    );
                };
                let repo_path = repo_path.clone();
                let inner = ctx.add_model(|ctx| {
                    LocalGitRepoStatusModel::new(repo_path, repository_model, ctx)
                });
                ctx.add_model(|ctx| {
                    ctx.subscribe_to_model(&inner, |me, event, ctx| {
                        GitRepoStatusModel::forward_event(me, event, ctx)
                    });
                    GitRepoStatusModel::Local(inner)
                })
            }
            LocalOrRemotePath::Remote(remote_path) => {
                let remote_path = remote_path.clone();
                let inner =
                    ctx.add_model(|ctx| RemoteGitRepoStatusModel::new(remote_path, ctx));
                ctx.add_model(|ctx| {
                    ctx.subscribe_to_model(&inner, |me, event, ctx| {
                        GitRepoStatusModel::forward_event(me, event, ctx)
                    });
                    GitRepoStatusModel::Remote(inner)
                })
            }
        };

        self.repos.insert(repo.clone(), handle.downgrade());
        Ok(handle)
    }

    /// Get or create the per-repo GitHub-info model for `repo`, returning a
    /// unified [`GitHubRepoModel`] handle that dispatches to a local
    /// `gh`-driven model or a remote push receiver based on the location.
    ///
    /// The local backend subscribes to the sibling git status model to track
    /// the current branch and fetches PR / repository info on creation, on
    /// branch change, and on a periodic timer. The remote backend asks the
    /// daemon to do the same and receives the results as pushes. Multiple
    /// callers in the same repo share one model; it is torn down when the last
    /// strong handle is dropped.
    ///
    /// Callers hold the returned `ModelHandle` for as long as they need updates.
    ///
    /// Ported from the pin's `GitRepoModels::subscribe_github_repo`
    /// (`42effe840:app/src/code_review/git_repo_models.rs`).
    pub fn subscribe_github_repo(
        &mut self,
        repo: &LocalOrRemotePath,
        ctx: &mut ModelContext<Self>,
    ) -> anyhow::Result<ModelHandle<GitHubRepoModel>> {
        if let Some(handle) = self.github_repos.get(repo).and_then(|weak| weak.upgrade(ctx)) {
            return Ok(handle);
        }

        let handle = match repo {
            LocalOrRemotePath::Local(repo_path) => {
                // `LocalGitHubRepoModel` needs a sibling `GitRepoStatusModel`
                // for branch info.
                let git_status = self.subscribe(repo, ctx)?;
                let repo_path = repo_path.clone();
                let inner = ctx
                    .add_model(|ctx| LocalGitHubRepoModel::new(repo_path, git_status, ctx));
                ctx.add_model(|ctx| {
                    ctx.subscribe_to_model(&inner, |me, event, ctx| {
                        GitHubRepoModel::forward_event(me, event, ctx)
                    });
                    GitHubRepoModel::Local(inner)
                })
            }
            LocalOrRemotePath::Remote(remote_path) => {
                // The remote backend needs no sibling status model: the
                // daemon's own `GitHubRepoModel` tracks the branch, and results
                // arrive as pushes.
                let remote_path = remote_path.clone();
                let inner = ctx.add_model(|ctx| RemoteGitHubRepoModel::new(remote_path, ctx));
                ctx.add_model(|ctx| {
                    ctx.subscribe_to_model(&inner, |me, event, ctx| {
                        GitHubRepoModel::forward_event(me, event, ctx)
                    });
                    GitHubRepoModel::Remote(inner)
                })
            }
        };

        self.github_repos.insert(repo.clone(), handle.downgrade());
        Ok(handle)
    }
}

impl Entity for GitStatusUpdateModel {
    type Event = ();
}

impl SingletonEntity for GitStatusUpdateModel {}

// ── GitRepoStatusModel (unified: local or remote backend) ───────────────────

#[cfg(feature = "local_fs")]
#[derive(Debug)]
pub enum GitRepoStatusEvent {
    /// Emitted whenever the metadata changes (branch name, diff stats, etc.).
    MetadataChanged,
}

/// Unified per-repo git status model that dispatches to a local or remote
/// backend, mirroring [`crate::code_review::diff_state::DiffStateModel`].
///
/// Consumers (prompt chips, tabs, code review, agent context) hold a
/// `ModelHandle<GitRepoStatusModel>` and subscribe to its [`GitRepoStatusEvent`]s
/// without caring whether the repository is local or on an SSH host. Only one
/// variant is populated at a time.
///
/// Ported from `42effe840:app/src/code_review/git_repo_model/mod.rs`. Before
/// this port the fork had a plain struct here — the local backend inlined —
/// which made the remote push receivers unreachable.
#[cfg(feature = "local_fs")]
pub enum GitRepoStatusModel {
    Local(ModelHandle<LocalGitRepoStatusModel>),
    Remote(ModelHandle<RemoteGitRepoStatusModel>),
}

#[cfg(feature = "local_fs")]
impl Entity for GitRepoStatusModel {
    type Event = GitRepoStatusEvent;
}

#[cfg(feature = "local_fs")]
impl GitRepoStatusModel {
    /// Re-emit a sub-model event so subscribers of the unified model observe
    /// the same `GitRepoStatusEvent`s regardless of backend.
    fn forward_event(&mut self, event: &GitRepoStatusEvent, ctx: &mut ModelContext<Self>) {
        match event {
            GitRepoStatusEvent::MetadataChanged => ctx.emit(GitRepoStatusEvent::MetadataChanged),
        }
    }

    /// Mode-independent status metadata (branch names + HEAD diff stats).
    pub fn metadata<'a>(&self, ctx: &'a AppContext) -> Option<&'a GitStatusMetadata> {
        match self {
            Self::Local(m) => m.as_ref(ctx).metadata(),
            Self::Remote(m) => m.as_ref(ctx).metadata(),
        }
    }

    /// Force a metadata refresh (branch names, diff stats). For the remote
    /// backend this asks the daemon to push a fresh snapshot.
    pub fn refresh_metadata(&self, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |m, ctx| m.refresh_metadata(ctx)),
            Self::Remote(m) => m.update(ctx, |m, ctx| m.request_snapshot(ctx)),
        }
    }
}

// ── LocalGitRepoStatusModel ─────────────────────────────────────────────────

/// Per-repository model that owns the filesystem watcher and exposes git status
/// metadata for a repo on the local filesystem.  Consumers do not hold this
/// directly; they hold the unified [`GitRepoStatusModel`], which forwards its
/// events.
///
/// When all strong handles are dropped the model (and its watcher) is
/// automatically torn down.
#[cfg(feature = "local_fs")]
pub struct LocalGitRepoStatusModel {
    repo_path: PathBuf,
    repository: ModelHandle<Repository>,
    subscriber_id: Option<SubscriberId>,
    metadata: Option<GitStatusMetadata>,
    computing_metadata_abort_handle: Option<SpawnedFutureHandle>,
}

#[cfg(feature = "local_fs")]
impl Entity for LocalGitRepoStatusModel {
    type Event = GitRepoStatusEvent;
}

#[cfg(feature = "local_fs")]
impl LocalGitRepoStatusModel {
    /// Create a new per-repo status model, set up the filesystem watcher, and
    /// kick off the initial metadata computation.
    fn new(
        repo_path: PathBuf,
        repository_model: ModelHandle<Repository>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let mut model = Self {
            repo_path: repo_path.clone(),
            repository: repository_model.clone(),
            subscriber_id: None,
            metadata: None,
            computing_metadata_abort_handle: None,
        };

        // Kick off initial metadata computation.
        model.refresh_metadata(ctx);

        // Start watching for filesystem changes.
        let (repository_update_tx, repository_update_rx) = async_channel::unbounded();
        let (throttled_tx, throttled_rx) = async_channel::unbounded();
        let start = repository_model.update(ctx, |repo, ctx| {
            repo.start_watching(
                RepositoryWatchMode::GitRepository,
                Box::new(GitStatusRepositorySubscriber {
                    repository_update_tx,
                }),
                ctx,
            )
        });
        model.subscriber_id = Some(start.subscriber_id);

        // Handle watcher registration.
        ctx.spawn(start.registration_future, |me, result, ctx| {
            if let Err(err) = result {
                log::warn!("LocalGitRepoStatusModel: watcher registration failed: {err}");
                if let Some(subscriber_id) = me.subscriber_id.take() {
                    me.repository.update(ctx, |repo, ctx| {
                        repo.stop_watching(subscriber_id, ctx);
                    });
                }
            }
        });

        // Stream raw updates; determine whether a throttled metadata refresh is warranted.
        {
            let throttled_tx_clone = throttled_tx;
            ctx.spawn_stream_local(
                repository_update_rx,
                move |_me, update: RepositoryUpdate, _ctx| {
                    if Self::should_refresh_metadata(&update) {
                        let _ = throttled_tx_clone.try_send(());
                    }
                },
                |_, _| {},
            );
        }

        // Throttled metadata refresh (at most once every 5 seconds).
        ctx.spawn_stream_local(
            throttle(Duration::from_secs(5), throttled_rx),
            |me, _, ctx| {
                me.refresh_metadata(ctx);
            },
            |_, _| {},
        );

        model
    }

    /// Read the current metadata.  Returns `None` if metadata hasn't been
    /// computed yet.
    pub fn metadata(&self) -> Option<&GitStatusMetadata> {
        self.metadata.as_ref()
    }

    /// The path to the repository root.
    #[allow(dead_code)]
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Manually trigger a metadata refresh.  Called by the terminal view after
    /// events that may have changed git state (block completed, agent file
    /// edits, etc.).
    pub fn refresh_metadata(&mut self, ctx: &mut ModelContext<Self>) {
        if let Some(handle) = self.computing_metadata_abort_handle.take() {
            handle.abort();
        }
        let repo_path_buf = self.repo_path.clone();
        self.computing_metadata_abort_handle = Some(ctx.spawn(
            async move { Self::load_metadata(repo_path_buf).await },
            |me, result, ctx| {
                me.handle_metadata_result(result, ctx);
            },
        ));
    }

    // ── internal helpers ────────────────────────────────────────────────

    fn handle_metadata_result(
        &mut self,
        result: anyhow::Result<GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) {
        match result {
            Ok(metadata) => self.metadata = Some(metadata),
            Err(e) => {
                log::warn!("LocalGitRepoStatusModel: metadata load failed: {e}");
                self.metadata = None;
            }
        }
        ctx.emit(GitRepoStatusEvent::MetadataChanged);
    }

    /// Decide whether a `RepositoryUpdate` warrants a metadata refresh.
    fn should_refresh_metadata(update: &RepositoryUpdate) -> bool {
        if update.is_empty() {
            return false;
        }
        if update.commit_updated || update.index_lock_detected || update.remote_ref_updated {
            return true;
        }
        // Check if any non-ignored file was touched.
        let changed_count = update
            .added
            .iter()
            .chain(&update.modified)
            .chain(&update.deleted)
            .chain(update.moved.keys())
            .chain(update.moved.values())
            .filter(|f| !f.is_ignored)
            .count();
        changed_count > 0
    }

    /// Compute metadata for a repo — branch names and diff stats against HEAD.
    ///
    /// This reuses logic extracted from `LocalDiffStateModel::load_metadata_for_repo`
    /// but only computes the HEAD (uncommitted) stats since that's all the git
    /// chip needs.
    async fn load_metadata(repo_path: PathBuf) -> anyhow::Result<GitStatusMetadata> {
        // Detect main branch.
        let main_branch_name = detect_main_branch(&repo_path).await?;
        // Detect current branch (using the display variant so detached HEAD
        // shows the short SHA instead of the literal "HEAD").
        let current_branch_name = detect_current_branch_display(&repo_path).await?;
        // Diff stats against HEAD.
        let stats_against_head =
            super::diff_state::LocalDiffStateModel::diff_metadata_against_head(&repo_path).await?;
        let branch_tracking_status =
            Self::branch_tracking_status(&repo_path, &current_branch_name).await;

        Ok(GitStatusMetadata {
            current_branch_name,
            main_branch_name,
            stats_against_head: stats_against_head.aggregate_stats,
            branch_tracking_status,
        })
    }

    fn parse_branch_tracking_counts(output: &str) -> Option<(u32, u32, u32)> {
        let mut parts = output.split_whitespace();
        let ahead = parts.next()?.parse().ok()?;
        let behind = parts.next()?.parse().ok()?;
        let equivalent = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
        Some((ahead, behind, equivalent))
    }

    /// Compute the ahead/behind (or rebased) tracking status of `current_branch_name`
    /// against its configured upstream.
    async fn branch_tracking_status(
        repo_path: &Path,
        current_branch_name: &str,
    ) -> GitBranchTrackingStatus {
        let upstream = run_git_command(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )
        .await
        .ok()
        .and_then(|output| {
            output
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
        });

        let Some(upstream) = upstream else {
            return GitBranchTrackingStatus::new(current_branch_name.to_string(), None, 0, 0);
        };

        let counts = run_git_command(
            repo_path,
            &[
                "rev-list",
                "--left-right",
                "--cherry-mark",
                "--count",
                "HEAD...@{u}",
            ],
        )
        .await
        .ok()
        .and_then(|output| Self::parse_branch_tracking_counts(&output));

        let Some((ahead, behind, equivalent)) = counts else {
            return GitBranchTrackingStatus::without_counts(
                current_branch_name.to_string(),
                Some(upstream),
            );
        };

        if ahead == 0 && behind == 0 && equivalent > 0 {
            return GitBranchTrackingStatus::rebased(current_branch_name.to_string(), upstream);
        }

        GitBranchTrackingStatus::new(
            current_branch_name.to_string(),
            Some(upstream),
            ahead,
            behind,
        )
    }
}

#[cfg(all(test, feature = "local_fs"))]
impl LocalGitRepoStatusModel {
    pub(crate) fn new_for_test(
        repository: ModelHandle<Repository>,
        metadata: Option<GitStatusMetadata>,
    ) -> Self {
        Self {
            repo_path: PathBuf::from("/test"),
            repository,
            subscriber_id: None,
            metadata,
            computing_metadata_abort_handle: None,
        }
    }

    pub(crate) fn set_metadata_for_test(
        &mut self,
        metadata: Option<GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.metadata = metadata;
        ctx.emit(GitRepoStatusEvent::MetadataChanged);
    }
}

#[cfg(all(test, feature = "local_fs"))]
impl GitRepoStatusModel {
    /// Wraps a local-backend test model in the unified enum.
    pub(crate) fn new_local_for_test(
        repository: ModelHandle<Repository>,
        metadata: Option<GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let inner =
            ctx.add_model(move |_| LocalGitRepoStatusModel::new_for_test(repository, metadata));
        ctx.subscribe_to_model(&inner, |me, event, ctx| me.forward_event(event, ctx));
        Self::Local(inner)
    }

    #[allow(dead_code)]
    pub(crate) fn set_metadata_for_test(
        &mut self,
        metadata: Option<GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => m.update(ctx, |m, ctx| m.set_metadata_for_test(metadata, ctx)),
            Self::Remote(_) => unreachable!("remote test models are not used"),
        }
    }
}

#[cfg(feature = "local_fs")]
impl Drop for LocalGitRepoStatusModel {
    fn drop(&mut self) {
        // Note: we cannot call `repository.update()` here because `Drop` does
        // not have access to `ModelContext`.  The `Repository` model will clean
        // up the subscriber when it notices the channel has been dropped.
        if let Some(handle) = self.computing_metadata_abort_handle.take() {
            handle.abort();
        }
    }
}

// ── Repository subscriber adapter ───────────────────────────────────────────

#[cfg(feature = "local_fs")]
struct GitStatusRepositorySubscriber {
    repository_update_tx: Sender<RepositoryUpdate>,
}

#[cfg(feature = "local_fs")]
impl RepositorySubscriber for GitStatusRepositorySubscriber {
    fn on_scan(
        &mut self,
        _repository: &Repository,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async {})
    }

    fn on_files_updated(
        &mut self,
        repository: &Repository,
        update: &RepositoryUpdate,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let tx = self.repository_update_tx.clone();
        let update = update.clone();
        let index_lock_path = repository.git_dir().join("index.lock");
        Box::pin(async move {
            // Suppress commit_updated events while the git index is locked to
            // avoid reacting to stale intermediate state during git operations.
            if update.commit_updated && async_fs::metadata(&index_lock_path).await.is_ok() {
                return;
            }
            let _ = tx.send(update).await;
        })
    }
}

#[cfg(all(test, feature = "local_fs"))]
#[path = "git_status_update_tests.rs"]
mod tests;
