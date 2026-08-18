//! Remote GitHub-info model.
//!
//! Client-side per-repo GitHub info for a repository on an SSH host.
//!
//! Presents the same read surface as [`super::LocalGitHubRepoModel`] and emits
//! the same [`GitHubRepoEvent`]s so the unified [`super::GitHubRepoModel`] can
//! substitute it transparently (mirrors
//! [`RemoteGitRepoStatusModel`](crate::code_review::git_status_update_remote::RemoteGitRepoStatusModel)).
//!
//! Pure push receiver: holds the latest PR / repository info for its
//! `(host_id, repo_path)`. On construction (and again on reconnect) it sends
//! `UpdateGitHubPrInfo` / `UpdateGitHubRepoInfo` notifications asking the
//! daemon to create the per-repo model if needed and refresh; results then
//! arrive as server-broadcast push messages filtered by `(host_id, repo_path)`.
//! The daemon's `GitHubRepoModel` is the single source of truth, so there is no
//! request/response and no client-side refresh state. A disconnect preserves
//! stale data.
//!
//! Ported from `42effe840:app/src/code_review/github_repo_model/remote.rs`.
//!
//! De-clouding note: the forge integration on the other end is `gh`, a
//! locally-authenticated CLI run on the SSH host — the same binary the local
//! backend runs on the local machine. No Warp backend, GraphQL API or HTTP
//! client is involved.

use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use warpui::{Entity, ModelContext, SingletonEntity};

use super::GitHubRepoEvent;
use crate::code::buffer_location::{util_remote_path_to_buffer, RemotePath};
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
        let Some(client) = RemoteServerManager::as_ref(ctx)
            .client_for_host(&self.remote_path.host_id)
            .cloned()
        else {
            return;
        };
        client.update_github_pr_info(&self.remote_path.path);
    }

    fn request_repository_info(&self, ctx: &mut ModelContext<Self>) {
        let Some(client) = RemoteServerManager::as_ref(ctx)
            .client_for_host(&self.remote_path.host_id)
            .cloned()
        else {
            return;
        };
        client.update_github_repo_info(&self.remote_path.path);
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
