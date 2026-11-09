//! Remote git-status model.
//!
//! Client-side per-repo git status for a repository on an SSH host. Presents
//! the same read surface as
//! [`LocalGitRepoStatusModel`](super::git_status_update::LocalGitRepoStatusModel)
//! and emits the same [`GitRepoStatusEvent`]s, so the unified
//! [`GitRepoStatusModel`](super::git_status_update::GitRepoStatusModel) can
//! dispatch to either backend transparently.
//!
//! Holds the latest [`GitStatusMetadata`] for its `(host_id, repo_path)`. On
//! construction (and again on reconnect) it asks the host's backend for the
//! current status. With a helper that is an `UpdateGitStatus` notification and
//! live watcher updates then arrive as `GitStatusPush` messages filtered by
//! `(host_id, repo_path)`; without one (shell mode) the backend computes the
//! status now and the model asks again on `ShellRefresh` events that can have
//! moved git state (a git command finished, the cwd moved, the app wrote),
//! one request in flight at a time, never on a timer. A disconnect preserves
//! stale data rather than clearing the chip.
//!
//! Ported from `42effe840:app/src/code_review/git_repo_model/remote.rs`. The
//! fork keeps `<subsystem>_remote.rs` as a sibling file (the shape
//! `diff_state.rs` / `diff_state_remote.rs` already uses) instead of the pin's
//! `git_repo_model/{mod,local,remote}.rs` directory.
//!
//! De-clouding note: nothing here talks to a Warp backend. The daemon computes
//! this from `git` subprocesses on the SSH host, exactly as the local backend
//! does on the local filesystem.

use remote_server::backend::refresh::{
    PrecmdPolicy, RefreshCoordinator, RefreshDecision, RemoteRefreshEvent,
};
use remote_server::backend::{BackendKind, Family, Requested};
use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use warpui::{Entity, ModelContext, SingletonEntity};

use super::git_status_update::{GitRepoStatusEvent, GitStatusMetadata};
use crate::code::buffer_location::{RemotePath, util_remote_path_to_buffer};
use crate::remote_server::git_status_proto::proto_to_git_status_metadata;
use crate::remote_server::proto;

pub struct RemoteGitRepoStatusModel {
    /// Manager-facing remote path (`warp_core::HostId`), converted once at
    /// construction from the `warp_util` flavour the `LocalOrRemotePath` cache
    /// key carries. See `code::buffer_location` for why the fork has two.
    remote_path: RemotePath,
    metadata: Option<GitStatusMetadata>,
    /// See `LocalGitRepoStatusModel::has_emitted_metadata`.
    has_emitted_metadata: bool,
    /// One shell-mode refresh in flight, one more queued (no timer).
    refresh: RefreshCoordinator,
}

impl Entity for RemoteGitRepoStatusModel {
    type Event = GitRepoStatusEvent;
}

impl RemoteGitRepoStatusModel {
    pub fn new(
        remote_path: warp_util::remote_path::RemotePath,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let mgr = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&mgr, |me, event, ctx| me.handle_manager_event(event, ctx));
        let model = Self {
            remote_path: util_remote_path_to_buffer(&remote_path),
            metadata: None,
            has_emitted_metadata: false,
            refresh: RefreshCoordinator::new(PrecmdPolicy::GitRelevant),
        };
        model.request_snapshot(ctx);
        model
    }

    fn handle_manager_event(
        &mut self,
        event: &RemoteServerManagerEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            RemoteServerManagerEvent::GitStatusPushReceived { host_id, push }
                if host_id == &self.remote_path.host_id
                    && push.repo_path == self.repo_path_string() =>
            {
                self.apply_push(push.metadata.as_ref(), ctx);
            }
            RemoteServerManagerEvent::ShellRefresh { host_id, event }
                if host_id == &self.remote_path.host_id =>
            {
                self.handle_shell_refresh(event, ctx);
            }
            RemoteServerManagerEvent::HostConnected { host_id }
                if host_id == &self.remote_path.host_id =>
            {
                // A fresh daemon has no per-repo model for us yet; ask again so
                // the chip repopulates without waiting for a watcher tick.
                self.request_snapshot(ctx);
            }
            _ => {}
        }
    }

    /// The remote repository path as the string the daemon keys pushes by.
    fn repo_path_string(&self) -> String {
        self.remote_path.path.to_string()
    }

    /// Asks for the current status. With a helper this is the fire-and-forget `UpdateGitStatus`
    /// (the daemon creates the per-repo status and pushes it); in shell mode the backend computes
    /// it now and the answer is applied here. No-op when nothing can serve it for the host;
    /// `HostConnected` retries.
    pub(super) fn request_snapshot(&self, ctx: &mut ModelContext<Self>) {
        self.request(ctx, false);
    }

    fn request(&self, ctx: &mut ModelContext<Self>, use_cached_main: bool) {
        let backend = RemoteServerManager::as_ref(ctx).backend_for_host(&self.remote_path.host_id);
        if !backend.is_available(Family::GitStatus) {
            return;
        }
        let path = self.remote_path.path.clone();
        let cached_main = use_cached_main
            .then(|| self.metadata.as_ref().map(|m| m.main_branch_name.clone()))
            .flatten();
        ctx.spawn(
            async move { backend.request_git_status(&path, cached_main).await },
            |me, result, ctx| {
                match result {
                    Ok(Requested::Computed(Some(push))) => {
                        me.apply_push(push.metadata.as_ref(), ctx)
                    }
                    Ok(_) => {}
                    Err(e) => log::debug!("RemoteGitRepoStatusModel: status request failed: {e}"),
                }
                // A refresh queued while this one ran starts now.
                if me.refresh.finished() {
                    me.request(ctx, true);
                }
            },
        );
    }

    /// A refresh event for the host: in shell mode, re-read when the event can have moved git
    /// state; with a helper only an explicit user refresh asks again (the daemon pushes the rest).
    fn handle_shell_refresh(&mut self, event: &RemoteRefreshEvent, ctx: &mut ModelContext<Self>) {
        let shell_backed = RemoteServerManager::as_ref(ctx)
            .backend_for_host(&self.remote_path.host_id)
            .kind(Family::GitStatus)
            == BackendKind::Shell;
        let explicit = matches!(event, RemoteRefreshEvent::UserAction);
        if !shell_backed && !explicit {
            return;
        }
        if self.refresh.on_event(event) == RefreshDecision::Start {
            self.request(ctx, !explicit);
        }
    }

    /// Decode a pushed `GitStatusMetadata` (branch + stats) and replace the
    /// stored value, emitting `MetadataChanged`.
    fn apply_push(
        &mut self,
        metadata: Option<&proto::GitStatusMetadata>,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(metadata) = metadata else {
            log::warn!(
                "RemoteGitRepoStatusModel: GitStatusPush without metadata for {}",
                self.repo_path_string()
            );
            return;
        };
        match proto_to_git_status_metadata(metadata) {
            Ok(status) => {
                let source = match RemoteServerManager::as_ref(ctx)
                    .backend_for_host(&self.remote_path.host_id)
                    .kind(Family::GitStatus)
                {
                    BackendKind::Helper => "helper",
                    BackendKind::Shell => "shell",
                };
                // Counts only, never branch names or paths.
                log::info!(
                    "hop=git_chip source={source} files={} added={} removed={} ahead={} behind={} upstream={}",
                    status.stats_against_head.files_changed,
                    status.stats_against_head.total_additions,
                    status.stats_against_head.total_deletions,
                    status.branch_tracking_status.ahead,
                    status.branch_tracking_status.behind,
                    u8::from(status.branch_tracking_status.upstream.is_some()),
                );
                let new_metadata = Some(status);
                let should_emit = super::git_status_update::should_emit_metadata_changed(
                    &self.metadata,
                    &new_metadata,
                    self.has_emitted_metadata,
                );
                self.metadata = new_metadata;
                if should_emit {
                    self.has_emitted_metadata = true;
                    ctx.emit(GitRepoStatusEvent::MetadataChanged);
                }
            }
            Err(error) => {
                // Keep the previous value: a malformed push is a daemon bug,
                // not evidence that the repo left git.
                log::warn!("RemoteGitRepoStatusModel: failed to decode git status push: {error}");
            }
        }
    }

    pub fn metadata(&self) -> Option<&GitStatusMetadata> {
        self.metadata.as_ref()
    }
}
