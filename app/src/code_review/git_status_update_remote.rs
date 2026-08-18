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
//! construction (and again on reconnect) it sends an `UpdateGitStatus`
//! notification asking the daemon to push the current snapshot; live watcher
//! updates then arrive as `GitStatusPush` messages filtered by
//! `(host_id, repo_path)`. A disconnect preserves stale data rather than
//! clearing the chip.
//!
//! Ported from `42effe840:app/src/code_review/git_repo_model/remote.rs`. The
//! fork keeps `<subsystem>_remote.rs` as a sibling file (the shape
//! `diff_state.rs` / `diff_state_remote.rs` already uses) instead of the pin's
//! `git_repo_model/{mod,local,remote}.rs` directory.
//!
//! De-clouding note: nothing here talks to a Warp backend. The daemon computes
//! this from `git` subprocesses on the SSH host, exactly as the local backend
//! does on the local filesystem.

use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use warpui::{Entity, ModelContext, SingletonEntity};

use super::git_status_update::{GitRepoStatusEvent, GitStatusMetadata};
use crate::code::buffer_location::{util_remote_path_to_buffer, RemotePath};
use crate::remote_server::git_status_proto::proto_to_git_status_metadata;
use crate::remote_server::proto;

pub struct RemoteGitRepoStatusModel {
    /// Manager-facing remote path (`warp_core::HostId`), converted once at
    /// construction from the `warp_util` flavour the `LocalOrRemotePath` cache
    /// key carries. See `code::buffer_location` for why the fork has two.
    remote_path: RemotePath,
    metadata: Option<GitStatusMetadata>,
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

    /// Fire-and-forget `UpdateGitStatus`: asks the daemon to create the per-repo
    /// status model if needed and broadcast the current snapshot. No-op when no
    /// session for the host is connected; `HostConnected` retries.
    pub(super) fn request_snapshot(&self, ctx: &mut ModelContext<Self>) {
        let Some(client) = RemoteServerManager::as_ref(ctx)
            .client_for_host(&self.remote_path.host_id)
            .cloned()
        else {
            return;
        };
        client.update_git_status(&self.remote_path.path);
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
                self.metadata = Some(status);
                ctx.emit(GitRepoStatusEvent::MetadataChanged);
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
