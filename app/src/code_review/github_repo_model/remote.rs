//! Remote GitHub-info model.
//!
//! Client-side per-repo GitHub info for a repository on an SSH host.
//!
//! Presents the same read surface as [`super::LocalGitHubRepoModel`] and emits
//! the same [`GitHubRepoEvent`]s so the unified [`super::GitHubRepoModel`] can
//! substitute it transparently (mirrors
//! [`RemoteGitRepoStatusModel`](crate::code_review::git_status_update_remote::RemoteGitRepoStatusModel)).
//!
//! Holds the latest PR / repository info for its `(host_id, repo_path)`. On
//! construction (and again on reconnect) it asks the host's backend. With a helper
//! those are `UpdateGitHubPrInfo` / `UpdateGitHubRepoInfo` notifications and the
//! results arrive as server-broadcast pushes filtered by `(host_id, repo_path)`
//! (the helper also looks again by itself when the branch changes). Without one
//! (shell mode) the backend runs the user's `gh` on the host now and the answer is
//! applied here; PR info is asked for again only on an explicit refresh or when the
//! branch moved since the last lookup, on `ShellRefresh` events that can have moved
//! git state. `gh` missing or not signed in reads as no PR / no repository. A
//! disconnect preserves stale data.
//!
//! Ported from `42effe840:app/src/code_review/github_repo_model/remote.rs`.
//!
//! De-clouding note: the forge integration on the other end is `gh`, a
//! locally-authenticated CLI run on the SSH host — the same binary the local
//! backend runs on the local machine. No Warp backend, GraphQL API or HTTP
//! client is involved.

use remote_server::backend::refresh::{PrecmdPolicy, RefreshCoordinator, RemoteRefreshEvent};
use remote_server::backend::{BackendKind, Family, PrLookup, Requested};
use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use warpui::{Entity, ModelContext, SingletonEntity};

use super::GitHubRepoEvent;
use crate::code::buffer_location::{RemotePath, util_remote_path_to_buffer};
use crate::remote_server::diff_state_proto::proto_to_pr_info;
use crate::remote_server::git_status_proto::proto_to_repository_info;
use crate::remote_server::proto;
use crate::util::git::{PrInfo, RepositoryInfo};

pub struct RemoteGitHubRepoModel {
    /// Manager-facing remote path (`warp_core::HostId`); see
    /// [`RemoteGitRepoStatusModel`](crate::code_review::git_status_update_remote::RemoteGitRepoStatusModel)
    /// for why the conversion happens here.
    remote_path: RemotePath,
    pr_info: Option<PrInfo>,
    repository_info: Option<RepositoryInfo>,
    /// The branch the last shell-mode PR lookup was for (`None`: no lookup yet).
    pr_branch: Option<Option<String>>,
    /// Git-relevant events only (shell mode): one lookup in flight, one queued.
    refresh: RefreshCoordinator,
}

impl Entity for RemoteGitHubRepoModel {
    type Event = GitHubRepoEvent;
}

impl RemoteGitHubRepoModel {
    pub fn new(
        remote_path: warp_util::remote_path::RemotePath,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let mgr = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&mgr, |me, event, ctx| me.handle_manager_event(event, ctx));
        let model = Self {
            remote_path: util_remote_path_to_buffer(&remote_path),
            pr_info: None,
            repository_info: None,
            pr_branch: None,
            refresh: RefreshCoordinator::new(PrecmdPolicy::GitRelevant),
        };
        model.request_github_info(ctx);
        model
    }

    fn handle_manager_event(
        &mut self,
        event: &RemoteServerManagerEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            RemoteServerManagerEvent::GitHubPrInfoPushReceived { host_id, push }
                if host_id == &self.remote_path.host_id
                    && push.repo_path == self.repo_path_string() =>
            {
                self.apply_pr_info_push(push.pr_info.as_ref(), ctx);
            }
            RemoteServerManagerEvent::GitHubRepositoryInfoPushReceived { host_id, push }
                if host_id == &self.remote_path.host_id
                    && push.repo_path == self.repo_path_string() =>
            {
                self.apply_repository_info_push(push.repository_info.as_ref(), ctx);
            }
            RemoteServerManagerEvent::ShellRefresh { host_id, event }
                if host_id == &self.remote_path.host_id =>
            {
                self.handle_shell_refresh(event, ctx);
            }
            RemoteServerManagerEvent::HostConnected { host_id }
                if host_id == &self.remote_path.host_id =>
            {
                self.request_github_info(ctx);
            }
            _ => {}
        }
    }

    /// The remote repository path as the string the daemon keys pushes by.
    fn repo_path_string(&self) -> String {
        self.remote_path.path.to_string()
    }

    /// Asks the daemon to (create and) refresh both PR and repository info.
    /// Fire-and-forget; results arrive as push broadcasts.
    fn request_github_info(&self, ctx: &mut ModelContext<Self>) {
        self.request_pr_info(ctx);
        self.request_repository_info(ctx);
    }

    fn request_pr_info(&self, ctx: &mut ModelContext<Self>) {
        self.request_pr(ctx, true);
    }

    /// `force`: always ask `gh`; otherwise (shell mode) only when the branch moved.
    fn request_pr(&self, ctx: &mut ModelContext<Self>, force: bool) {
        let backend = RemoteServerManager::as_ref(ctx).backend_for_host(&self.remote_path.host_id);
        if !backend.is_available(Family::Github) {
            return;
        }
        let path = self.remote_path.path.clone();
        let known = self.pr_branch.clone();
        ctx.spawn(
            async move { backend.request_pr_info(&path, known, force).await },
            |me, result, ctx| {
                match result {
                    Ok(Requested::Computed(PrLookup::Fetched { branch, push })) => {
                        me.pr_branch = Some(branch);
                        me.apply_pr_info_push(push.pr_info.as_ref(), ctx);
                    }
                    Ok(_) => {}
                    Err(e) => log::debug!("RemoteGitHubRepoModel: PR request failed: {e}"),
                }
                if me.refresh.finished() {
                    me.request_pr(ctx, false);
                }
            },
        );
    }

    fn request_repository_info(&self, ctx: &mut ModelContext<Self>) {
        let backend = RemoteServerManager::as_ref(ctx).backend_for_host(&self.remote_path.host_id);
        if !backend.is_available(Family::Github) {
            return;
        }
        let path = self.remote_path.path.clone();
        ctx.spawn(
            async move { backend.request_repository_info(&path).await },
            |me, result, ctx| match result {
                Ok(Requested::Computed(push)) => {
                    me.apply_repository_info_push(push.repository_info.as_ref(), ctx)
                }
                Ok(Requested::PushedByHelper) => {}
                Err(e) => log::debug!("RemoteGitHubRepoModel: repository request failed: {e}"),
            },
        );
    }

    /// A refresh event for the host. In shell mode a git-relevant event looks the PR up again if
    /// the branch moved; an explicit user refresh forces both lookups (with a helper too).
    fn handle_shell_refresh(&mut self, event: &RemoteRefreshEvent, ctx: &mut ModelContext<Self>) {
        use remote_server::backend::refresh::RefreshDecision;
        let shell_backed = RemoteServerManager::as_ref(ctx)
            .backend_for_host(&self.remote_path.host_id)
            .kind(Family::Github)
            == BackendKind::Shell;
        let explicit = matches!(event, RemoteRefreshEvent::UserAction);
        if !shell_backed && !explicit {
            return;
        }
        if self.refresh.on_event(event) == RefreshDecision::Start {
            self.request_pr(ctx, explicit);
            if explicit {
                self.request_repository_info(ctx);
            }
        }
    }

    /// Replace the stored PR info from a push, emitting `PrInfoChanged` only
    /// when the value moved.
    fn apply_pr_info_push(
        &mut self,
        pr_info: Option<&proto::PrInfo>,
        ctx: &mut ModelContext<Self>,
    ) {
        let pr_info = pr_info.map(proto_to_pr_info);
        if self.pr_info != pr_info {
            self.pr_info = pr_info;
            ctx.emit(GitHubRepoEvent::PrInfoChanged);
        }
    }

    /// Replace the stored repository info from a push, emitting
    /// `RepositoryInfoChanged` only when the value moved.
    fn apply_repository_info_push(
        &mut self,
        repository_info: Option<&proto::RepositoryInfo>,
        ctx: &mut ModelContext<Self>,
    ) {
        let repository_info = repository_info.map(proto_to_repository_info);
        if self.repository_info != repository_info {
            self.repository_info = repository_info;
            ctx.emit(GitHubRepoEvent::RepositoryInfoChanged);
        }
    }

    pub fn pr_info(&self) -> Option<&PrInfo> {
        self.pr_info.as_ref()
    }

    pub fn repository_info(&self) -> Option<&RepositoryInfo> {
        self.repository_info.as_ref()
    }

    /// Always `false`: the remote backend does not track refresh state, since
    /// results arrive as broadcasts with no request correlation.
    pub fn is_refreshing_pr_info(&self) -> bool {
        false
    }

    pub fn refresh_pr_info(&self, ctx: &mut ModelContext<Self>) {
        self.request_pr_info(ctx);
    }

    pub fn refresh_repository_info(&self, ctx: &mut ModelContext<Self>) {
        self.request_repository_info(ctx);
    }
}
