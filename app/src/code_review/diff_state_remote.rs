//! Remote diff-state model.
//!
//! Client-side backend for a single remote repository's code-review diff state,
//! received from the remote-server daemon over SSH. Presents the same read API
//! as [`LocalDiffStateModel`](super::diff_state::LocalDiffStateModel) and emits
//! the same [`DiffStateModelEvent`]s, so the [`DiffStateModel`] enum wrapper can
//! dispatch to either backend transparently.
//!
//! Lifecycle: on construction the model issues an initial `GetDiffState`
//! subscription RPC to the host's daemon and stores the returned snapshot.
//! Subsequent server-pushed changes arrive as
//! `RemoteServerManagerEvent::DiffState*Received` and are folded into the stored
//! state. Changing the active [`DiffMode`] unsubscribes the old `(repo, mode)`
//! pair and subscribes the new one.

use std::path::PathBuf;
use std::sync::Arc;

use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use remote_server::HostId;
use warpui::{AppContext, ModelContext, SingletonEntity};

use super::diff_state::{
    DiffMetadata, DiffMetadataAgainstBase, DiffMode, DiffState, DiffStats, DiffStateModelEvent,
    GitDiffData, InvalidationBehavior, InvalidationSource,
};
use crate::code::buffer_location::RemotePath;
use crate::remote_server::diff_state_proto::{
    decode_file_delta, decode_metadata_update, decode_snapshot, encode_diff_mode,
};
use crate::remote_server::proto;
use crate::util::git::{Commit, PrInfo};

/// Internal diff payload state for a remote subscription. Mirrors the local
/// model's states, plus `Disconnected` for a dropped transport (stale data is
/// preserved until the subscription is re-established).
#[derive(Default)]
enum InternalRemoteDiffState {
    #[default]
    Loading,
    NotInRepository,
    Loaded(GitDiffData),
    Error(String),
    Disconnected,
}

pub struct RemoteDiffStateModel {
    remote_path: RemotePath,
    mode: DiffMode,
    state: InternalRemoteDiffState,
    metadata: Option<DiffMetadata>,
}

impl warpui::Entity for RemoteDiffStateModel {
    type Event = DiffStateModelEvent;
}

impl RemoteDiffStateModel {
    /// Creates a remote diff-state model for `(host, repo, mode)` and issues the
    /// initial `GetDiffState` subscription. The model subscribes to the
    /// `RemoteServerManager` push channel so later server-pushed changes are
    /// applied.
    pub fn new(remote_path: RemotePath, mode: DiffMode, ctx: &mut ModelContext<Self>) -> Self {
        let mgr = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&mgr, |me, event, ctx| me.handle_manager_event(event, ctx));

        let model = Self {
            remote_path,
            mode,
            state: InternalRemoteDiffState::Loading,
            metadata: None,
        };
        model.spawn_get_diff_state(ctx);
        model
    }

    /// The remote repository path as the string the daemon keys subscriptions
    /// by.
    fn repo_path_string(&self) -> String {
        self.remote_path.path.to_string()
    }

    /// Issues a `GetDiffState` subscription RPC for the current `(repo, mode)`
    /// over the host's connected session. The initial snapshot arrives as the
    /// RPC response; later changes arrive as manager push events.
    fn spawn_get_diff_state(&self, ctx: &mut ModelContext<Self>) {
        let Some(client) = RemoteServerManager::as_ref(ctx)
            .client_for_host(&self.remote_path.host_id)
            .cloned()
        else {
            // No connected session for the host; leave the model in its current
            // state. A reconnect (HostConnected) re-subscribes once 4b lands.
            return;
        };
        let request = proto::GetDiffState {
            repo_path: self.repo_path_string(),
            mode: Some(encode_diff_mode(&self.mode)),
        };
        ctx.spawn(
            async move { client.get_diff_state(request).await },
            |me, result, ctx| match result {
                Ok(response) => me.handle_get_diff_state_response(response, ctx),
                Err(e) => me.set_error(format!("{e}"), ctx),
            },
        );
    }

    /// Fire-and-forget `UnsubscribeDiffState` for a `(repo, mode)` pair.
    fn spawn_unsubscribe(&self, mode: &DiffMode, ctx: &mut ModelContext<Self>) {
        let Some(client) = RemoteServerManager::as_ref(ctx)
            .client_for_host(&self.remote_path.host_id)
            .cloned()
        else {
            return;
        };
        let request = proto::UnsubscribeDiffState {
            repo_path: self.repo_path_string(),
            mode: Some(encode_diff_mode(mode)),
        };
        client.unsubscribe_diff_state(request);
    }

    fn handle_get_diff_state_response(
        &mut self,
        response: proto::GetDiffStateResponse,
        ctx: &mut ModelContext<Self>,
    ) {
        match response.result {
            Some(proto::get_diff_state_response::Result::Snapshot(snapshot)) => {
                self.apply_snapshot(&snapshot, ctx);
            }
            Some(proto::get_diff_state_response::Result::Error(e)) => {
                self.set_error(e.message, ctx);
            }
            None => self.set_error("empty GetDiffState response".to_string(), ctx),
        }
    }

    fn set_error(&mut self, message: String, ctx: &mut ModelContext<Self>) {
        self.state = InternalRemoteDiffState::Error(message);
        ctx.emit(DiffStateModelEvent::NewDiffsComputed(None));
    }

    // ── Manager push events ──────────────────────────────────────

    fn handle_manager_event(
        &mut self,
        event: &RemoteServerManagerEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            RemoteServerManagerEvent::DiffStateSnapshotReceived { host_id, snapshot } => {
                if self.matches(host_id, &snapshot.repo_path, snapshot.mode.as_ref()) {
                    self.apply_snapshot(snapshot, ctx);
                }
            }
            RemoteServerManagerEvent::DiffStateMetadataUpdateReceived { host_id, update } => {
                if self.matches(host_id, &update.repo_path, update.mode.as_ref()) {
                    if let Some(metadata) = decode_metadata_update(update) {
                        self.apply_metadata(metadata, ctx);
                    }
                }
            }
            RemoteServerManagerEvent::DiffStateFileDeltaReceived { host_id, delta } => {
                if self.matches(host_id, &delta.repo_path, delta.mode.as_ref()) {
                    self.apply_file_delta(delta, ctx);
                }
            }
            RemoteServerManagerEvent::HostDisconnected { host_id } => {
                if host_id == &self.remote_path.host_id {
                    self.state = InternalRemoteDiffState::Disconnected;
                }
            }
            RemoteServerManagerEvent::HostConnected { host_id } => {
                if host_id == &self.remote_path.host_id {
                    // Re-establish the subscription after a reconnect.
                    self.spawn_get_diff_state(ctx);
                }
            }
            _ => {}
        }
    }

    /// Whether an incoming push targets this model's `(host, repo, mode)`.
    fn matches(
        &self,
        host_id: &HostId,
        repo_path: &str,
        mode: Option<&proto::DiffMode>,
    ) -> bool {
        host_id == &self.remote_path.host_id
            && repo_path == self.repo_path_string()
            && mode == Some(&encode_diff_mode(&self.mode))
    }

    fn apply_snapshot(&mut self, snapshot: &proto::DiffStateSnapshot, ctx: &mut ModelContext<Self>) {
        let decoded = decode_snapshot(snapshot);
        let previous_branch = self.get_current_branch_name();

        self.state = match decoded.state {
            DiffState::Loaded(data) => InternalRemoteDiffState::Loaded(data),
            DiffState::Error(e) => InternalRemoteDiffState::Error(e),
            DiffState::Loading => InternalRemoteDiffState::Loading,
            DiffState::NotInRepository => InternalRemoteDiffState::NotInRepository,
        };
        self.metadata = Some(decoded.metadata);

        if previous_branch != self.get_current_branch_name() {
            ctx.emit(DiffStateModelEvent::CurrentBranchChanged);
        }
        ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
            InvalidationBehavior::All(InvalidationSource::MetadataChange),
        ));
        ctx.emit(DiffStateModelEvent::NewDiffsComputed(
            decoded.diffs.map(Arc::new),
        ));
    }

    fn apply_metadata(&mut self, metadata: DiffMetadata, ctx: &mut ModelContext<Self>) {
        let previous_branch = self.get_current_branch_name();
        self.metadata = Some(metadata);
        if previous_branch != self.get_current_branch_name() {
            ctx.emit(DiffStateModelEvent::CurrentBranchChanged);
        }
        ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
            InvalidationBehavior::All(InvalidationSource::MetadataChange),
        ));
    }

    fn apply_file_delta(&mut self, delta: &proto::DiffStateFileDelta, ctx: &mut ModelContext<Self>) {
        let decoded = decode_file_delta(delta);
        if let Some(metadata) = decoded.metadata {
            self.metadata = Some(metadata);
        }
        // A per-file delta invalidates just that path; the view re-reads the
        // model for the fresh diff. We surface it as a Files invalidation.
        ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
            InvalidationBehavior::Files(vec![PathBuf::from(decoded.file_path)]),
        ));
    }

    // ── Read API (mirrors LocalDiffStateModel) ───────────────────

    pub fn get(&self) -> DiffState {
        match &self.state {
            InternalRemoteDiffState::Loading => DiffState::Loading,
            InternalRemoteDiffState::NotInRepository => DiffState::NotInRepository,
            InternalRemoteDiffState::Loaded(data) => DiffState::Loaded(data.clone()),
            InternalRemoteDiffState::Error(e) => DiffState::Error(e.clone()),
            // Preserve the last-known diffs while disconnected; fall back to a
            // loading state when there is nothing to show.
            InternalRemoteDiffState::Disconnected => DiffState::Loading,
        }
    }

    pub fn get_metadata(&self) -> Option<&DiffMetadataAgainstBase> {
        self.metadata
            .as_ref()
            .and_then(|metadata| match &self.mode {
                DiffMode::Head => Some(&metadata.against_head),
                DiffMode::MainBranch => metadata.against_base_branch.as_ref(),
                DiffMode::OtherBranch(_) => None,
            })
    }

    pub fn diff_mode(&self) -> DiffMode {
        self.mode.clone()
    }

    pub fn get_uncommitted_stats(&self) -> Option<DiffStats> {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.against_head.aggregate_stats)
    }

    pub fn get_main_branch_name(&self) -> Option<String> {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.main_branch_name.clone())
    }

    pub fn get_current_branch_name(&self) -> Option<String> {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.current_branch_name.clone())
    }

    pub fn is_on_main_branch(&self) -> bool {
        match (self.get_current_branch_name(), self.get_main_branch_name()) {
            (Some(current), Some(main)) => {
                let main_short = main.strip_prefix("origin/").unwrap_or(&main);
                current == main || current == main_short
            }
            _ => false,
        }
    }

    pub fn unpushed_commits(&self) -> &[Commit] {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.unpushed_commits.as_slice())
            .unwrap_or_default()
    }

    pub fn upstream_ref(&self) -> Option<&str> {
        self.metadata
            .as_ref()
            .and_then(|m| m.upstream_ref.as_deref())
    }

    pub fn upstream_differs_from_main(&self) -> bool {
        match (self.upstream_ref(), self.get_main_branch_name().as_deref()) {
            (Some(upstream), Some(main)) => upstream != main,
            _ => false,
        }
    }

    pub fn pr_info(&self) -> Option<&PrInfo> {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.pr_info.as_ref())
    }

    /// Git-operation blocking is a local working-tree concern; the remote
    /// daemon owns it, so the client never blocks.
    pub fn is_git_operation_blocked(&self, _app: &AppContext) -> bool {
        false
    }

    pub fn has_head(&self) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|metadata| metadata.has_head_commit)
    }

    pub fn active_repository_path(&self, _app: &AppContext) -> Option<PathBuf> {
        Some(PathBuf::from(self.repo_path_string()))
    }

    pub fn get_stats_for_current_mode(&self) -> Option<DiffStats> {
        self.get_stats_for_mode(self.mode.clone())
    }

    pub fn get_stats_for_mode(&self, mode: DiffMode) -> Option<DiffStats> {
        let metadata = self.metadata.as_ref()?;
        match mode {
            DiffMode::Head => Some(metadata.against_head.aggregate_stats),
            DiffMode::MainBranch => metadata
                .against_base_branch
                .as_ref()
                .map(|base| base.aggregate_stats),
            DiffMode::OtherBranch(_) => None,
        }
    }

    // ── Mutation API (mirrors LocalDiffStateModel) ───────────────

    pub fn set_diff_mode(
        &mut self,
        mode: DiffMode,
        should_fetch_base: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.mode != mode {
            let old_mode = std::mem::replace(&mut self.mode, mode);
            self.spawn_unsubscribe(&old_mode, ctx);
            self.state = InternalRemoteDiffState::Loading;
            self.spawn_get_diff_state(ctx);
        }
        ctx.emit(DiffStateModelEvent::DiffModeChanged { should_fetch_base });
    }

    pub fn set_diff_mode_and_fetch_base(&mut self, mode: DiffMode, ctx: &mut ModelContext<Self>) {
        self.set_diff_mode(mode, true, ctx);
    }

    /// Re-requests the current snapshot from the daemon.
    pub fn load_diffs_for_current_repo(
        &mut self,
        _should_fetch_base: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.spawn_get_diff_state(ctx);
    }

    /// Discarding files over SSH is not yet supported (the DiscardFiles RPC is
    /// a later increment); this is a no-op rather than silently mutating the
    /// wrong (local) tree.
    pub fn discard_files(
        &mut self,
        _file_infos: Vec<super::diff_state::FileStatusInfo>,
        _should_stash: bool,
        _branch_name: Option<String>,
        _ctx: &mut ModelContext<Self>,
    ) {
        log::warn!("discard_files is not yet supported for remote diff state");
    }

    /// The daemon owns metadata refresh cadence, so this is a no-op.
    pub fn set_code_review_metadata_refresh_enabled(
        &mut self,
        _enabled: bool,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// The daemon pushes metadata changes; nothing to refresh client-side.
    pub fn refresh_diff_metadata_for_current_repo(
        &mut self,
        _invalidation_behavior: InvalidationBehavior,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// Tears down the subscription for the current `(repo, mode)`.
    pub fn stop_active_watcher(&mut self, ctx: &mut ModelContext<Self>) {
        let mode = self.mode.clone();
        self.spawn_unsubscribe(&mode, ctx);
    }

    /// PR info rides along in the daemon-pushed metadata; nothing to refresh.
    pub fn refresh_pr_info(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Fetches the remote repository's branches via the `GetBranches` RPC. The
    /// daemon runs the same `git for-each-ref` listing the local backend uses,
    /// so the branch dropdown is populated identically over SSH instead of
    /// shelling out to `git` against a remote path that does not exist on the
    /// client. The result is emitted as
    /// [`DiffStateModelEvent::BranchesReceived`], matching the local backend.
    pub fn fetch_branches(&self, ctx: &mut ModelContext<Self>) {
        let Some(client) = RemoteServerManager::as_ref(ctx)
            .client_for_host(&self.remote_path.host_id)
            .cloned()
        else {
            // No connected session for the host; the dropdown keeps its
            // defaults until a reconnect triggers another fetch.
            return;
        };
        let request = self.get_branches_request();
        ctx.spawn(
            async move { client.get_branches(request).await },
            |me, result, ctx| match result {
                Ok(response) => me.handle_get_branches_response(&response, ctx),
                Err(e) => {
                    log::warn!("RemoteDiffStateModel: GetBranches request failed: {e}");
                    ctx.emit(DiffStateModelEvent::BranchesReceived(vec![]));
                }
            },
        );
    }

    /// Builds the `GetBranches` request for this model's remote repository.
    /// The parameters mirror the local backend's call
    /// (`get_all_branches(repo, None, false)`) so both backends list the same
    /// set of branches.
    fn get_branches_request(&self) -> proto::GetBranches {
        proto::GetBranches {
            repo_path: self.repo_path_string(),
            max_branch_count: None,
            include_remotes: false,
        }
    }

    /// Converts a `GetBranchesResponse` into `(branch_name, is_main)` pairs and
    /// emits them as [`DiffStateModelEvent::BranchesReceived`]. An error
    /// response emits an empty list so the dropdown falls back to its defaults,
    /// mirroring the local backend's error path.
    fn handle_get_branches_response(
        &self,
        response: &proto::GetBranchesResponse,
        ctx: &mut ModelContext<Self>,
    ) {
        let branches = match &response.result {
            Some(proto::get_branches_response::Result::Success(success)) => success
                .branches
                .iter()
                .map(|info| (info.name.clone(), info.is_main))
                .collect(),
            Some(proto::get_branches_response::Result::Error(err)) => {
                let message = &err.message;
                log::warn!("RemoteDiffStateModel: GetBranches failed: {message}");
                vec![]
            }
            None => {
                log::warn!("RemoteDiffStateModel: empty GetBranches response");
                vec![]
            }
        };
        ctx.emit(DiffStateModelEvent::BranchesReceived(branches));
    }
}

#[cfg(test)]
#[path = "diff_state_remote_tests.rs"]
mod tests;
