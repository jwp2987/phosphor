//! Per-repository GitHub metadata models (`gh pr view` / `gh repo view`).
//!
//! Ported from the pinned oracle
//! (`02b53fcd8:app/src/code_review/github_repo_model/{mod,local,local_tests}.rs`).
//!
//! Only the **local** backend is ported. The pin's `remote.rs` is a pure push
//! receiver over `RemoteServerManagerEvent::{GitHubPrInfoPushReceived,
//! GitHubRepositoryInfoPushReceived}` plus the matching daemon-side model and
//! `UpdateGitHubPrInfo` / `UpdateGitHubRepoInfo` notifications, none of which
//! exist in this fork's `remote_server` yet. The unified enum shape is kept so a
//! `Remote` variant can be added later without touching any consumer.
//!
//! Everything here is local: `gh` is a locally-authenticated CLI, so no cloud
//! backend is involved.

use warpui::{AppContext, Entity, ModelContext, ModelHandle};

mod local;
pub use local::LocalGitHubRepoModel;

#[cfg(test)]
use crate::code_review::git_status_update::GitRepoStatusModel;
use crate::util::git::{PrInfo, RepositoryInfo};

#[derive(Debug)]
pub enum GitHubRepoEvent {
    /// Emitted when `pr_info` changes value (fetch result differs from
    /// cached, branch change cleared the cache, etc.).
    PrInfoChanged,
    /// Emitted when `repository_info` changes value.
    RepositoryInfoChanged,
}

// ── Unified GitHubRepoModel (local backend only, for now) ───────────────────

/// Unified per-repo GitHub-info model that dispatches to a backend, mirroring
/// [`crate::code_review::git_status_update::GitRepoStatusModel`].
///
/// Consumers (prompt chips, code review, agent context) hold a
/// `ModelHandle<GitHubRepoModel>` and subscribe to its [`GitHubRepoEvent`]s
/// without caring which backend serves them.
pub enum GitHubRepoModel {
    Local(ModelHandle<LocalGitHubRepoModel>),
}

impl Entity for GitHubRepoModel {
    type Event = GitHubRepoEvent;
}

impl GitHubRepoModel {
    /// Re-emit a sub-model event so subscribers of the unified model observe
    /// the same `GitHubRepoEvent`s regardless of backend.
    pub(crate) fn forward_event(&mut self, event: &GitHubRepoEvent, ctx: &mut ModelContext<Self>) {
        match event {
            GitHubRepoEvent::PrInfoChanged => ctx.emit(GitHubRepoEvent::PrInfoChanged),
            GitHubRepoEvent::RepositoryInfoChanged => {
                ctx.emit(GitHubRepoEvent::RepositoryInfoChanged)
            }
        }
    }

    /// PR info for the current branch.
    pub fn pr_info<'a>(&self, ctx: &'a AppContext) -> Option<&'a PrInfo> {
        match self {
            Self::Local(m) => m.as_ref(ctx).pr_info(),
        }
    }

    /// Repository info (name/owner) returned by `gh repo view`.
    pub fn repository_info<'a>(&self, ctx: &'a AppContext) -> Option<&'a RepositoryInfo> {
        match self {
            Self::Local(m) => m.as_ref(ctx).repository_info(),
        }
    }

    /// Whether a `gh pr view` fetch is currently in flight.
    #[allow(dead_code)]
    pub fn is_refreshing_pr_info(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Local(m) => m.as_ref(ctx).is_refreshing_pr_info(),
        }
    }

    /// Force a PR info refresh (e.g. after a `gh`/`gt` command completes).
    pub fn refresh_pr_info(&self, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |m, ctx| m.refresh_pr_info(ctx)),
        }
    }

    /// Force a repository-info refresh.
    #[allow(dead_code)]
    pub fn refresh_repository_info(&self, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |m, ctx| m.refresh_repository_info(ctx)),
        }
    }
}

#[cfg(test)]
impl GitHubRepoModel {
    /// Wraps an inert local-backend test model in the unified enum.
    pub(crate) fn new_local_for_test(
        git_status: ModelHandle<GitRepoStatusModel>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let inner = ctx.add_model(move |_| LocalGitHubRepoModel::new_for_test(git_status));
        ctx.subscribe_to_model(&inner, |me, event, ctx| me.forward_event(event, ctx));
        Self::Local(inner)
    }

    pub(crate) fn set_pr_info_for_test(
        &mut self,
        pr_info: Option<PrInfo>,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => m.update(ctx, |m, ctx| m.set_pr_info_for_test(pr_info, ctx)),
        }
    }

    pub(crate) fn set_repository_info_for_test(
        &mut self,
        repository_info: Option<RepositoryInfo>,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => m.update(ctx, |m, ctx| {
                m.set_repository_info_for_test(repository_info, ctx)
            }),
        }
    }
}
