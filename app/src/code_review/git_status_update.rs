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
    std::{
        collections::HashMap,
        time::{Duration, Instant},
    },
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
#[derive(Debug, Clone, PartialEq)]
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

    /// Refresh after a block completed with `command`; the local backend may skip or coalesce it
    /// (see [`LocalGitRepoStatusModel::refresh_metadata_after_command`]). Remote asks as usual.
    pub fn refresh_metadata_after_command(&self, command: &str, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => {
                m.update(ctx, |m, ctx| m.refresh_metadata_after_command(command, ctx))
            }
            Self::Remote(m) => m.update(ctx, |m, ctx| m.request_snapshot(ctx)),
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
    /// Whether `MetadataChanged` has fired at least once. The first event is
    /// always delivered (consumers such as `deferred_code_review_open` wait
    /// for it, even when the first load fails); later ones only on change.
    has_emitted_metadata: bool,
    computing_metadata_abort_handle: Option<SpawnedFutureHandle>,
    /// The main branch the last full detection found, and when. `detect_main_branch` is up to
    /// six git processes and its answer only moves when refs do, so refreshes inside
    /// [`MAIN_BRANCH_CACHE_TTL`] reuse it (#1334). A HEAD/ref/remote-ref update clears it.
    main_branch_cache: Option<(String, Instant)>,
    /// Decides whether a command-completion refresh can be skipped or coalesced (#1334 follow-up).
    refresh_gate: RefreshGate,
}

/// How long a detected main branch is reused by metadata refreshes (#1334).
#[cfg(feature = "local_fs")]
const MAIN_BRANCH_CACHE_TTL: Duration = Duration::from_secs(30);

/// The cached main branch when it is still within [`MAIN_BRANCH_CACHE_TTL`] at `now`.
#[cfg(feature = "local_fs")]
pub(super) fn fresh_cached_main_branch(
    cache: &Option<(String, Instant)>,
    now: Instant,
) -> Option<String> {
    let (name, detected_at) = cache.as_ref()?;
    (now.saturating_duration_since(*detected_at) < MAIN_BRANCH_CACHE_TTL).then(|| name.clone())
}

/// What a command-completion refresh request should do.
#[cfg(feature = "local_fs")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RefreshDecision {
    /// Nothing could have changed (an empty Enter with a quiet watcher); no git processes.
    Skip,
    /// Request a refresh; [`RefreshGate::request_refresh`] coalesces it with one in flight.
    Refresh,
}

/// Kill switch: `PHOSPHOR_GIT_REFRESH_ALWAYS=1` restores a refresh after every block.
#[cfg(feature = "local_fs")]
fn refresh_always_env() -> bool {
    std::env::var_os("PHOSPHOR_GIT_REFRESH_ALWAYS").is_some_and(|v| v != "0" && !v.is_empty())
}

/// Whether the command names a VCS tool, i.e. could change git state in a way that must not wait
/// for (or depend on) the file watcher. Plain file edits and build output are the watcher's job.
#[cfg(feature = "local_fs")]
pub(super) fn command_may_change_git_state(command: &str) -> bool {
    const VCS: &[&str] = &[
        "git", "gh", "gt", "hub", "jj", "hg", "sl", "svn", "stg", "lazygit", "pijul",
    ];
    command
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .any(|tok| {
            let tok = tok.trim_end_matches(".exe");
            VCS.contains(&tok) || tok.starts_with("git-")
        })
}

/// Pure bookkeeping behind metadata refreshes, kept free of the app context so it can be unit-tested.
///
/// A refresh is never skipped because of watcher timing: the watcher flushes its events after a
/// fixed window, so "the watcher has not reported anything" does not mean "nothing changed".
/// Requests instead coalesce: at most one refresh is in flight and one more is remembered as
/// pending, which re-runs as soon as the in-flight one finishes, so the result always reflects
/// the latest request (no timer involved).
#[cfg(feature = "local_fs")]
#[derive(Debug, Default)]
pub(super) struct RefreshGate {
    /// The watcher reported a relevant change since the last refresh started.
    dirty: bool,
    in_flight: bool,
    rerun_pending: bool,
}

#[cfg(feature = "local_fs")]
impl RefreshGate {
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Ask for a refresh. Returns true when the caller must start one now; false when one is
    /// already in flight, in which case it re-runs once that one finishes
    /// ([`Self::refresh_finished`]).
    pub(super) fn request_refresh(&mut self) -> bool {
        if self.in_flight {
            self.rerun_pending = true;
            return false;
        }
        self.in_flight = true;
        self.dirty = false;
        true
    }

    /// The in-flight refresh completed (successfully or not). Returns true when a request arrived
    /// meanwhile and the caller must start another refresh (via [`Self::request_refresh`]).
    pub(super) fn refresh_finished(&mut self) -> bool {
        self.in_flight = false;
        std::mem::take(&mut self.rerun_pending)
    }

    /// `trusted` is true when metadata exists and the repo watcher is live; without it we cannot
    /// know nothing changed. Only a command that never ran (empty Enter) with a quiet watcher is
    /// skipped; everything else refreshes (coalesced), whatever the watcher has delivered so far.
    pub(super) fn decide(&self, command: &str, trusted: bool, always: bool) -> RefreshDecision {
        if trusted && !always && !self.dirty && command.trim().is_empty() {
            RefreshDecision::Skip
        } else {
            RefreshDecision::Refresh
        }
    }
}

/// Whether a freshly loaded/pushed snapshot warrants a `MetadataChanged`
/// event. Re-emitting identical metadata invalidates every subscribed terminal
/// view (a full-window layout) and, on the remote daemon, pushes a redundant
/// message over the shared ssh connection.
#[cfg(feature = "local_fs")]
pub(super) fn should_emit_metadata_changed(
    old: &Option<GitStatusMetadata>,
    new: &Option<GitStatusMetadata>,
    has_emitted_before: bool,
) -> bool {
    !has_emitted_before || old != new
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
            has_emitted_metadata: false,
            computing_metadata_abort_handle: None,
            main_branch_cache: None,
            refresh_gate: RefreshGate::default(),
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
                move |me, update: RepositoryUpdate, ctx| {
                    if update.commit_updated || update.remote_ref_updated {
                        me.main_branch_cache = None;
                    }
                    if update.commit_updated {
                        // HEAD / a local branch ref moved: cached branch lists are stale.
                        crate::util::git::note_refs_changed();
                    }
                    if Self::is_head_change(&update) || Self::should_refresh_metadata(&update) {
                        me.refresh_gate.mark_dirty();
                    }
                    if Self::is_head_change(&update) {
                        // A HEAD / branch-ref change must not wait out the 5s
                        // throttle: `git switch a; git switch b` in quick succession
                        // would otherwise leave the live branch on `a` while the
                        // shell already reports `b` (#892).
                        me.refresh_metadata(ctx);
                    } else if Self::should_refresh_metadata(&update) {
                        let _ = throttled_tx_clone.try_send(());
                    }
                },
                |_, _| {},
            );
        }

        // Throttled metadata refresh for plain file-change events (at most once every 5 seconds).
        // This is pacing for build-output churn, not correctness: command completions and HEAD
        // changes refresh through the coalescing gate immediately.
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
        // Coalesce instead of aborting/skipping: a request during a refresh re-runs after it.
        if !self.refresh_gate.request_refresh() {
            return;
        }
        let repo_path_buf = self.repo_path.clone();
        let cached_main_branch = fresh_cached_main_branch(&self.main_branch_cache, Instant::now());
        let reused_main_branch = cached_main_branch.is_some();
        self.computing_metadata_abort_handle = Some(ctx.spawn(
            async move { Self::load_metadata(repo_path_buf, cached_main_branch).await },
            move |me, result, ctx| {
                if let (false, Ok(metadata)) = (reused_main_branch, &result) {
                    me.main_branch_cache =
                        Some((metadata.main_branch_name.clone(), Instant::now()));
                }
                me.handle_metadata_result(result, ctx);
                if me.refresh_gate.refresh_finished() {
                    me.refresh_metadata(ctx);
                }
            },
        ));
    }

    /// Refresh after a block completed with `command`. Only an empty Enter with a quiet watcher
    /// reuses the last result; every other completion refreshes, coalesced with any refresh
    /// already in flight, so the chip reflects the latest edit even when the block finished
    /// before the file watcher flushed its event.
    pub fn refresh_metadata_after_command(&mut self, command: &str, ctx: &mut ModelContext<Self>) {
        let trusted = self.metadata.is_some() && self.subscriber_id.is_some();
        match self
            .refresh_gate
            .decide(command, trusted, refresh_always_env())
        {
            RefreshDecision::Skip => {}
            RefreshDecision::Refresh => self.refresh_metadata(ctx),
        }
    }

    // ── internal helpers ────────────────────────────────────────────────

    fn handle_metadata_result(
        &mut self,
        result: anyhow::Result<GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) {
        let new_metadata = match result {
            Ok(metadata) => Some(metadata),
            Err(e) => {
                log::warn!("LocalGitRepoStatusModel: metadata load failed: {e}");
                None
            }
        };
        let should_emit =
            should_emit_metadata_changed(&self.metadata, &new_metadata, self.has_emitted_metadata);
        self.metadata = new_metadata;
        if should_emit {
            self.has_emitted_metadata = true;
            ctx.emit(GitRepoStatusEvent::MetadataChanged);
        }
    }

    /// Whether the update moved HEAD or a local branch ref (`commit_updated`).
    /// These bypass the metadata throttle so the branch never lags a switch.
    fn is_head_change(update: &RepositoryUpdate) -> bool {
        update.commit_updated
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
    async fn load_metadata(
        repo_path: PathBuf,
        cached_main_branch: Option<String>,
    ) -> anyhow::Result<GitStatusMetadata> {
        // Detect main branch (or reuse the recent detection).
        let main_branch_name = match cached_main_branch {
            Some(name) => name,
            None => detect_main_branch(&repo_path).await?,
        };
        // Detect current branch (using the display variant so detached HEAD
        // shows the short SHA instead of the literal "HEAD").
        let current_branch_name = detect_current_branch_display(&repo_path).await?;
        // Diff stats against HEAD.
        // Chip-specific stats: tracked insertions/deletions like `git diff --shortstat
        // HEAD`, untracked files counted as files only (#1134).
        let stats_against_head =
            super::diff_state::LocalDiffStateModel::chip_stats_against_head(&repo_path).await?;
        let branch_tracking_status =
            Self::branch_tracking_status(&repo_path, &current_branch_name).await;

        Ok(GitStatusMetadata {
            current_branch_name,
            main_branch_name,
            stats_against_head,
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
            has_emitted_metadata: false,
            computing_metadata_abort_handle: None,
            main_branch_cache: None,
            refresh_gate: RefreshGate::default(),
        }
    }

    pub(crate) fn set_metadata_for_test(
        &mut self,
        metadata: Option<GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.metadata = metadata;
        self.has_emitted_metadata = true;
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
