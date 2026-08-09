use crate::terminal::shell::ShellType;
use repo_metadata::repositories::{DetectedRepositories, RepoDetectionSource};
use repo_metadata::{RepoMetadataEvent, RepoMetadataModel, RepositoryIdentifier};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use warp_core::channel::ChannelState;
use warp_core::SessionId;
use warp_util::standardized_path::StandardizedPath;
use warpui::platform::TerminationMode;
use warpui::r#async::{Spawnable, SpawnableOutput, SpawnedFutureHandle};
use warpui::{Entity, ModelContext, SingletonEntity};

use warp_files::{FileModel, FileModelEvent};
use warp_util::content_version::ContentVersion;
use warp_util::file::FileId;

use super::proto::{
    client_message, delete_file_response, discard_files_response, git_commit_chain_response,
    git_create_pr_response, git_push_response, host_scoped_request, notification,
    run_command_response, server_message, session_scoped_request, write_file_response, Abort,
    Authenticate, ClientMessage, DeleteFile, DeleteFileResponse, DeleteFileSuccess,
    DiscardFilesError, DiscardFilesRequest, DiscardFilesResponse, DiscardFilesSuccess, ErrorCode,
    ErrorResponse, FailedFileRead, FileContextProto, FileOperationError, GetBranches,
    GetCommittedBranchFilesRequest, GetDiffState, GitCommitChainMode, GitCommitChainRequest,
    GitCommitChainResponse, GitCommitChainSuccess, GitCreatePrRequest, GitCreatePrResponse,
    GitOpDelta, GitOpError, GitPushRequest, GitPushResponse, HostScopedRequest, Initialize,
    InitializeResponse, NavigatedToDirectory, NavigatedToDirectoryResponse, Notification,
    ReadFileContextResponse, RipgrepSearchRequest, RunCommandError, RunCommandErrorCode,
    RunCommandRequest, RunCommandResponse, RunCommandSuccess, ServerMessage, SessionBootstrapped,
    SessionScopedRequest, UnsubscribeDiffState, WriteFile, WriteFileResponse, WriteFileSuccess,
};

// Remote Agent Mode context snapshot (#438 dependent feature 1, #353 producer): depends
// on `SkillManager`'s real (non-dummy) API, gated `local_fs` like the buffer-sync imports
// below.
#[cfg(feature = "local_fs")]
use super::proto::{remote_skill_proto, HomeSkillMetadata, RemoteAgentContextSnapshot, RemoteSkillProto};
#[cfg(feature = "local_fs")]
use crate::ai::skills::{bundled_skill_snapshot_protos, BundledSkill, SkillManager, SkillManagerEvent};

// Buffer-sync related: depends on GlobalBufferModel, whose server-local
// operations are only available under `local_fs`, so the entire server-side
// buffer handling is gated on `local_fs`.
#[cfg(feature = "local_fs")]
use super::proto::{
    create_directory_response, list_directory_response, read_file_chunk_response,
    resolve_conflict_response, resolve_path_response, save_buffer_response,
    write_file_chunk_response, BufferEdit, BufferUpdatedPush, CloseBuffer, CreateDirectory,
    CreateDirectoryResponse, CreateDirectorySuccess, DirEntry, FileSystemEntryKind, ListDirectory,
    ListDirectoryResponse, ListDirectorySuccess, OpenBuffer, OpenBufferResponse, ReadFileChunk,
    ReadFileChunkResponse, ReadFileChunkSuccess, ResolveConflict, ResolveConflictResponse,
    ResolveConflictSuccess, ResolvePath, ResolvePathResponse, ResolvePathSuccess, SaveBuffer,
    SaveBufferResponse, SaveBufferSuccess, TextEdit, WriteFileChunk, WriteFileChunkResponse,
    WriteFileChunkSuccess,
};
#[cfg(feature = "local_fs")]
use super::server_buffer_tracker::{PendingBufferRequestKind, ServerBufferTracker};
#[cfg(feature = "local_fs")]
use crate::code::global_buffer_model::{GlobalBufferModel, GlobalBufferModelEvent};
#[cfg(feature = "local_fs")]
use crate::code_review::git_status_update::GitRepoStatusModel;
#[cfg(feature = "local_fs")]
use crate::code_review::github_repo_model::GitHubRepoModel;
#[cfg(feature = "local_fs")]
use warpui::ModelHandle;

/// How long the daemon waits with no connections before exiting.
pub const GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Unique identifier for a connected proxy session in daemon mode.
pub type ConnectionId = uuid::Uuid;
use super::protocol::RequestId;
use crate::ai::agent::FileLocations;
use crate::ai::blocklist::{read_local_file_context, ReadFileContextResult};
use crate::terminal::model::session::command_executor::{
    ExecuteCommandOptions, LocalCommandExecutor,
};

/// Outcome of dispatching a request-style `ClientMessage`.
///
/// Notifications (fire-and-forget messages like `SessionBootstrapped` and
/// `Abort`) do not produce a `HandlerOutcome`; they are dispatched inline in
/// `handle_message` and return early.
enum HandlerOutcome {
    /// The response is ready synchronously — the caller sends it immediately.
    Sync(server_message::Message),
    /// The handler initiated async work whose response will be sent later.
    ///
    /// When the handle is `Some`, the caller inserts it into `in_progress`
    /// so the request can be cancelled via `Abort`. Removal on
    /// completion/abort is arranged by [`ServerModel::spawn_request_handler`].
    ///
    /// `None` is used for async work whose completion is delivered through
    /// a separate event subscription and is not currently cancellable via
    /// `Abort` (e.g. `FileModel` events for file writes and deletes, which
    /// are tracked by `FileId` in `pending_file_ops` rather than by
    /// `RequestId` in `in_progress`).
    Async(Option<SpawnedFutureHandle>),
}

#[cfg(test)]
impl HandlerOutcome {
    fn into_message(self) -> server_message::Message {
        match self {
            HandlerOutcome::Sync(message) => message,
            HandlerOutcome::Async(_) => panic!("expected synchronous handler outcome"),
        }
    }
}

/// Tracks an in-flight file write or delete so the async completion
/// event can be correlated back to the originating client request.
enum FileOpKind {
    Write,
    Delete,
}

struct PendingFileOp {
    request_id: RequestId,
    conn_id: ConnectionId,
    kind: FileOpKind,
}

/// Manages pending file operations and ensures that the corresponding
/// `FileModel` entry is always cleaned up when an operation completes
/// or fails, preventing `FileState` leaks.
struct PendingFileOps {
    ops: HashMap<FileId, PendingFileOp>,
}

impl PendingFileOps {
    fn new() -> Self {
        Self {
            ops: HashMap::new(),
        }
    }

    /// Registers a file path with `FileModel`, sets the initial version,
    /// and tracks the pending operation. Returns the `FileId` and
    /// `ContentVersion` for the caller to initiate the actual I/O.
    fn insert(
        &mut self,
        path: &Path,
        request_id: RequestId,
        conn_id: ConnectionId,
        kind: FileOpKind,
        ctx: &mut ModelContext<ServerModel>,
    ) -> (FileId, ContentVersion) {
        let file_model = FileModel::handle(ctx);
        let file_id = file_model.update(ctx, |m, ctx| m.register_file_path(path, false, ctx));
        let version = ContentVersion::new();
        file_model.update(ctx, |m, _| m.set_version(file_id, version));
        self.ops.insert(
            file_id,
            PendingFileOp {
                request_id,
                conn_id,
                kind,
            },
        );
        (file_id, version)
    }

    fn get(&self, file_id: &FileId) -> Option<&PendingFileOp> {
        self.ops.get(file_id)
    }

    /// Removes a pending operation and unsubscribes the file from `FileModel`,
    /// preventing the `FileState` entry from leaking.
    fn remove(
        &mut self,
        file_id: FileId,
        ctx: &mut ModelContext<ServerModel>,
    ) -> Option<PendingFileOp> {
        let op = self.ops.remove(&file_id)?;
        FileModel::handle(ctx).update(ctx, |m, ctx| m.unsubscribe(file_id, ctx));
        Some(op)
    }
}

/// The top-level server-side orchestrator model.
/// Server-side cap on the number of branches returned by `GetBranches`,
/// bounding response size regardless of the client's requested count.
const MAX_BRANCH_COUNT_CAP: usize = 500;

///
/// Receives `ClientMessage`s from connected proxy sessions and routes
/// `ServerMessage` responses and push notifications back through each
/// connection's dedicated sender channel.
/// A single active diff-state subscription. `canonical_path` (canonicalized on
/// this host) is matched against repository-change events; `wire_repo_path` is
/// the exact string the client sent, echoed back in pushed snapshots so the
/// client's `RemoteDiffStateModel` can key on it.
#[cfg(feature = "local_fs")]
#[derive(Clone)]
struct DiffStateSubscription {
    canonical_path: StandardizedPath,
    wire_repo_path: String,
    mode: crate::code_review::diff_state::DiffMode,
}

/// Resolves the global bundled resources directory populated by the install script.
///
/// **Always returns `None` in this fork.** The pin resolves this via
/// `remote_server::setup::remote_server_bundled_resources_dir()`, which does not exist
/// here — see issue #440 (reopened): the fork's install script/setup module has no
/// `BUNDLED_RESOURCES_DIR_NAME` / `remote_server_bundled_resources_dir()` /
/// `remote_server_removal_command()`, and the release artifact doesn't ship a
/// `resources/` tree at all. That is separate, larger work (touches the release
/// pipeline, not just Rust) and is deliberately not attempted here. Until #440 lands,
/// `bundled_skills` below stays empty and the daemon logs why — the rest of the
/// `RemoteAgentContextSnapshot` plumbing (`home_dir`, home skills) is unaffected.
#[cfg(feature = "local_fs")]
fn daemon_bundled_resources_dir() -> Option<PathBuf> {
    None
}

/// Builds a `RemoteAgentContextSnapshot` for this host at `revision`, combining the
/// daemon's own bundled-skill catalog (`bundled_skills`, produced once at startup —
/// see `daemon_bundled_resources_dir`) with its currently-cached home skills.
///
/// `global_rules` is left empty: unlike the pin, this fork's `ProjectContextModel`
/// (`crates/ai/src/project_context/model.rs`) has no per-host rule storage or
/// `global_rules()` accessor — building that (plus the client-side consumer in
/// `app/src/ai/remote_agent_context.rs`) is a separate, comparably-sized feature and is
/// out of scope here. See that file's module doc comment for the client-side half of
/// the same decision.
#[cfg(feature = "local_fs")]
fn remote_agent_context_snapshot(
    revision: u64,
    bundled_skills: &[RemoteSkillProto],
    ctx: &warpui::AppContext,
) -> RemoteAgentContextSnapshot {
    let home_dir = dirs::home_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut skills = bundled_skills.to_vec();
    skills.extend(
        SkillManager::as_ref(ctx)
            .home_skills()
            .map(|skill| RemoteSkillProto {
                path: skill.path.display_path(),
                content: skill.content.clone(),
                source: Some(remote_skill_proto::Source::Home(HomeSkillMetadata {})),
            }),
    );
    skills.sort_by(|a, b| a.path.cmp(&b.path));
    RemoteAgentContextSnapshot {
        revision,
        home_dir,
        skills,
        global_rules: Vec::new(),
    }
}

pub struct ServerModel {
    /// Per-connection outbound channels, keyed by `ConnectionId`.
    ///
    /// The daemon can serve multiple proxy connections simultaneously — one
    /// per SSH session / Zap tab connecting to this host.  Each entry maps
    /// a connection's `Uuid` to the channel the connection task drains to
    /// write `ServerMessage`s back to its proxy.
    connection_senders: HashMap<ConnectionId, async_channel::Sender<ServerMessage>>,
    /// Per-connection set of repo roots for which we've already sent a
    /// snapshot in this connection's lifetime.
    ///
    /// Used to avoid sending duplicate snapshots on repeated
    /// `NavigatedToDirectory` calls while the user `cd`s within the same repo.
    snapshot_sent_roots_by_connection: HashMap<ConnectionId, HashSet<StandardizedPath>>,
    /// Active diff-state subscriptions, keyed by connection. Each entry is a
    /// `(repo, mode)` the client subscribed to via `GetDiffState`; on a
    /// repository change the daemon recomputes and pushes a fresh snapshot to
    /// the subscribing connection. Cleared per-entry on `UnsubscribeDiffState`
    /// and wholesale on connection teardown.
    #[cfg(feature = "local_fs")]
    diff_state_subscriptions: HashMap<ConnectionId, Vec<DiffStateSubscription>>,
    /// Per-repo local git-status models tracked on the daemon, keyed by repo
    /// path. Ported from the pin's `git_status_models` field. Only the
    /// subscription bookkeeping is ported here (issue #330); the daemon-side
    /// wiring that would actually populate this map — subscribing on
    /// navigation and broadcasting `GitStatusPush` — is a separate, larger
    /// feature gap and is not part of this change, so this map stays empty.
    /// It exists now so `drop_subscription` evicts the right entries once
    /// that wiring lands.
    #[cfg(feature = "local_fs")]
    git_status_models: HashMap<StandardizedPath, ModelHandle<GitRepoStatusModel>>,
    /// Per-repo local GitHub-info models tracked on the daemon, keyed by repo
    /// path. Ported from the pin's `github_repo_models` field. Same caveat as
    /// `git_status_models`: stays empty until the daemon-side push wiring is
    /// ported separately.
    #[cfg(feature = "local_fs")]
    github_repo_models: HashMap<StandardizedPath, ModelHandle<GitHubRepoModel>>,
    /// Connections subscribed (via navigation) to each repo's git status,
    /// keyed by repo path. A repo's git-status *and* GitHub-info models live
    /// while this set is non-empty and are evicted once the last connection
    /// unsubscribes (navigates away or disconnects). Mirrors
    /// `diff_state_subscriptions`'s per-connection tracking, but keyed by
    /// repo since git-status subscription is exclusive (one repo per
    /// connection) rather than a list of `(repo, mode)` pairs.
    #[cfg(feature = "local_fs")]
    git_status_subscribers: HashMap<StandardizedPath, HashSet<ConnectionId>>,
    /// Each connection's current git repo (a connection is in at most one
    /// repo at a time), so a navigation can move its subscription and a
    /// disconnect can drop it.
    #[cfg(feature = "local_fs")]
    git_status_repo_by_conn: HashMap<ConnectionId, StandardizedPath>,
    /// Abort handle for the active grace timer, if any.
    /// Calling `.abort()` cancels the timer before it fires.
    grace_timer_cancel: Option<SpawnedFutureHandle>,
    /// Tracks in-progress requests that can be cancelled via `Abort`.
    /// Calling `.abort()` on the handle cancels the background future and
    /// triggers its `on_abort` callback.
    in_progress: HashMap<RequestId, SpawnedFutureHandle>,
    /// In-flight requests that must reach *some* live connection to this
    /// host, not necessarily the exact proxy connection that issued them.
    /// Maps a tracked `RequestId` to the `ConnectionId` it was originally
    /// dispatched to. `send_server_message` consults this map when the
    /// original connection is gone or its channel is closed: if the
    /// request is tracked here, the response is failed over to another
    /// live connection instead of being silently dropped.
    ///
    /// The pin (`02b53fcd8`) populates this from a `HostScoped` request
    /// envelope (`client_message::Message::HostScoped`) that classifies
    /// every request kind as host- or session-scoped at dispatch time. The
    /// fork's wire protocol has no such envelope yet (tracked separately —
    /// see the parity issue on the host-scoped/session-scoped protocol
    /// envelope), so this map is not yet populated from `handle_message`.
    /// The delivery mechanism itself (this field plus the failover branch
    /// in `send_server_message`) is ported now so it is ready to use once
    /// that envelope lands.
    host_scoped_requests: HashMap<RequestId, ConnectionId>,
    /// Stable host identifier generated once at process startup.
    /// Returned in every `InitializeResponse` so clients can deduplicate
    /// host-scoped models.
    host_id: String,
    /// Per-session command executors created from `SessionBootstrapped` notifications.
    executors: HashMap<SessionId, Arc<LocalCommandExecutor>>,
    /// Tracks in-flight file write/delete operations and handles cleanup.
    pending_file_ops: PendingFileOps,
    /// Tracks open server-local buffers, their connections, and pending
    /// buffer requests (OpenBuffer, SaveBuffer, ResolveConflict).
    #[cfg(feature = "local_fs")]
    buffers: ServerBufferTracker,
    /// This daemon's own bundled-skill catalog, serialized once at startup by
    /// `daemon_bundled_resources_dir`/`bundled_skill_snapshot_protos` (#353). Always
    /// empty pending #440 — see `daemon_bundled_resources_dir`'s doc comment.
    #[cfg(feature = "local_fs")]
    bundled_skills: Vec<RemoteSkillProto>,
    /// Latest revisioned full replacement of this daemon host's Agent Mode context
    /// (#438 dependent feature 1 / #353), pushed to every connection and re-pushed on
    /// (re)connect via `send_remote_agent_context_snapshot_to_connection`.
    #[cfg(feature = "local_fs")]
    remote_agent_context_snapshot: RemoteAgentContextSnapshot,
    /// Connections that have already received the current
    /// `remote_agent_context_snapshot` revision — avoids re-sending an unchanged
    /// snapshot on every `register_connection`.
    #[cfg(feature = "local_fs")]
    remote_agent_context_snapshot_sent: HashSet<ConnectionId>,
    /// Daemon-wide bearer credential for the identity-scoped daemon.
    ///
    /// The token is written by Initialize when the client supplies a
    /// non-empty credential, or by Authenticate during token rotation. It is
    /// intentionally retained across proxy connection teardown and cleared
    /// only by daemon process exit.
    auth_token: Option<String>,
}

impl Entity for ServerModel {
    type Event = ();
}

impl SingletonEntity for ServerModel {}

impl ServerModel {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let host_id = uuid::Uuid::new_v4().to_string();
        log::info!(
            "Daemon started: PID={}, host_id={}",
            std::process::id(),
            host_id
        );
        #[cfg(feature = "local_fs")]
        let bundled_skills: Vec<RemoteSkillProto> = Vec::new();
        #[cfg(feature = "local_fs")]
        let initial_remote_agent_context_snapshot =
            remote_agent_context_snapshot(1, &bundled_skills, ctx);
        let mut model = Self {
            connection_senders: HashMap::new(),
            snapshot_sent_roots_by_connection: HashMap::new(),
            #[cfg(feature = "local_fs")]
            diff_state_subscriptions: HashMap::new(),
            #[cfg(feature = "local_fs")]
            git_status_models: HashMap::new(),
            #[cfg(feature = "local_fs")]
            github_repo_models: HashMap::new(),
            #[cfg(feature = "local_fs")]
            git_status_subscribers: HashMap::new(),
            #[cfg(feature = "local_fs")]
            git_status_repo_by_conn: HashMap::new(),
            grace_timer_cancel: None,
            in_progress: HashMap::new(),
            host_scoped_requests: HashMap::new(),
            host_id,
            executors: HashMap::new(),
            pending_file_ops: PendingFileOps::new(),
            #[cfg(feature = "local_fs")]
            buffers: ServerBufferTracker::new(),
            #[cfg(feature = "local_fs")]
            bundled_skills,
            #[cfg(feature = "local_fs")]
            remote_agent_context_snapshot: initial_remote_agent_context_snapshot,
            #[cfg(feature = "local_fs")]
            remote_agent_context_snapshot_sent: HashSet::new(),
            auth_token: None,
        };
        // Subscribe to FileModel and RepoMetadataModel events
        // file operation results and repo metadata pushes are forwarded to all
        // connected proxy sessions.
        {
            let file_model = FileModel::handle(ctx);
            ctx.subscribe_to_model(&file_model, |me, event, ctx| {
                let file_id = event.file_id();
                let Some(pending_kind) = me.pending_file_ops.get(&file_id).map(|op| &op.kind)
                else {
                    return; // Not a file op we're tracking.
                };
                let response_message = match (event, pending_kind) {
                    (FileModelEvent::FileSaved { .. }, FileOpKind::Write) => {
                        server_message::Message::WriteFileResponse(WriteFileResponse {
                            result: Some(write_file_response::Result::Success(WriteFileSuccess {})),
                        })
                    }
                    (FileModelEvent::FileSaved { .. }, FileOpKind::Delete) => {
                        server_message::Message::DeleteFileResponse(DeleteFileResponse {
                            result: Some(delete_file_response::Result::Success(
                                DeleteFileSuccess {},
                            )),
                        })
                    }
                    (FileModelEvent::FailedToSave { error, .. }, FileOpKind::Write) => {
                        server_message::Message::WriteFileResponse(WriteFileResponse {
                            result: Some(write_file_response::Result::Error(FileOperationError {
                                message: format!("{error}"),
                            })),
                        })
                    }
                    (FileModelEvent::FailedToSave { error, .. }, FileOpKind::Delete) => {
                        server_message::Message::DeleteFileResponse(DeleteFileResponse {
                            result: Some(delete_file_response::Result::Error(FileOperationError {
                                message: format!("{error}"),
                            })),
                        })
                    }
                    (FileModelEvent::FileLoaded { .. }, _)
                    | (FileModelEvent::FailedToLoad { .. }, _)
                    | (FileModelEvent::FileUpdated { .. }, _) => return,
                };
                // Remove the pending op and unsubscribe from FileModel.
                let pending = me
                    .pending_file_ops
                    .remove(file_id, ctx)
                    .expect("pending op was confirmed present");
                me.send_server_message(
                    Some(pending.conn_id),
                    Some(&pending.request_id),
                    response_message,
                );
            });
        }
        {
            let repo_model = RepoMetadataModel::handle(ctx);
            ctx.subscribe_to_model(&repo_model, |me, event, ctx| match event {
                RepoMetadataEvent::IncrementalUpdateReady { update } => {
                    me.send_server_message(
                        None,
                        None,
                        server_message::Message::RepoMetadataUpdate(update.into()),
                    );
                }
                RepoMetadataEvent::RepositoryUpdated {
                    id: RepositoryIdentifier::Local(path),
                } => {
                    // A repo finished indexing — push the full tree as a snapshot.
                    let id = RepositoryIdentifier::local(path.clone());
                    let repo_model = RepoMetadataModel::handle(ctx);
                    if let Some(state) = repo_model.as_ref(ctx).get_repository(&id, ctx) {
                        let entries = super::repo_metadata_proto::file_tree_entry_to_snapshot_proto(
                            &state.entry,
                        );
                        let standing_results: Option<super::proto::StandingQueryResultsDelta> =
                            repo_model
                                .as_ref(ctx)
                                .standing_query_results(path, ctx)
                                .map(|results| (&results.as_snapshot_delta()).into());
                        me.send_server_message(
                            None,
                            None,
                            server_message::Message::RepoMetadataSnapshot(
                                super::proto::RepoMetadataSnapshot {
                                    repo_path: path.to_string(),
                                    entries,
                                    sync_complete: true,
                                    standing_results,
                                },
                            ),
                        );
                        // Mark this root as snapshot-sent for all active connections
                        // so subsequent NavigatedToDirectory calls skip re-sending.
                        for sent_roots in me.snapshot_sent_roots_by_connection.values_mut() {
                            sent_roots.insert(path.clone());
                        }
                    }
                    // The repository's contents changed — push a fresh diff-state
                    // snapshot to any connection subscribed to it.
                    #[cfg(feature = "local_fs")]
                    me.push_diff_state_for_repo(path, ctx);
                }
                RepoMetadataEvent::RepositoryRemoved { .. }
                | RepoMetadataEvent::FileTreeUpdated { .. }
                | RepoMetadataEvent::FileTreeEntryUpdated { .. }
                | RepoMetadataEvent::UpdatingRepositoryFailed { .. }
                | RepoMetadataEvent::StandingQueryResultsUpdated { .. }
                | RepoMetadataEvent::RepositoryUpdated {
                    id: RepositoryIdentifier::Remote(_),
                } => {}
            });
        }
        // Subscribe to GlobalBufferModel events for server-local buffers.
        #[cfg(feature = "local_fs")]
        {
            let gbm = GlobalBufferModel::handle(ctx);
            ctx.subscribe_to_model(&gbm, |me, event, ctx| match event {
                GlobalBufferModelEvent::BufferLoaded { file_id, .. } => {
                    // Complete all pending OpenBuffer requests for this file.
                    let pending = me
                        .buffers
                        .take_pending_by_kind(file_id, PendingBufferRequestKind::OpenBuffer);
                    if !pending.is_empty() {
                        let gbm = GlobalBufferModel::handle(ctx);
                        let content = gbm.as_ref(ctx).content_for_file(*file_id, ctx);
                        let server_version = gbm
                            .as_ref(ctx)
                            .sync_clock_for_server_local(*file_id)
                            .map(|c| c.server_version.as_u64());

                        for (request_id, conn_id) in pending {
                            let message = match (&content, server_version) {
                                (Some(content), Some(sv)) => {
                                    server_message::Message::OpenBufferResponse(
                                        OpenBufferResponse {
                                            content: content.clone(),
                                            server_version: sv,
                                        },
                                    )
                                }
                                _ => server_message::Message::Error(ErrorResponse {
                                    code: ErrorCode::Internal.into(),
                                    message: format!(
                                        "Buffer loaded but content or sync clock unavailable for file {file_id:?}"
                                    ),
                                }),
                            };
                            me.send_server_message(Some(conn_id), Some(&request_id), message);
                        }
                    }
                }
                GlobalBufferModelEvent::ServerLocalBufferUpdated {
                    file_id,
                    edits,
                    new_server_version,
                    expected_client_version,
                } => {
                    // Push incremental edits to all connections that have this buffer open.
                    let Some(conns) = me.buffers.connections_for_buffer(file_id) else {
                        return;
                    };
                    // Find the path for this file_id; abort the push if tracker
                    // state is inconsistent (a missing path would break the
                    // path↔buffer contract).
                    let Some(path) = me.buffers.path_for_file_id(*file_id) else {
                        log::error!(
                            "Missing path mapping for server-local buffer file_id={file_id:?}"
                        );
                        return;
                    };

                    let proto_edits: Vec<TextEdit> = edits
                        .iter()
                        .map(|edit| TextEdit {
                            start_offset: edit.start.as_usize() as u64,
                            end_offset: edit.end.as_usize() as u64,
                            text: edit.text.clone(),
                        })
                        .collect();

                    let conns: Vec<_> = conns.iter().copied().collect();
                    for conn_id in conns {
                        me.send_server_message(
                            Some(conn_id),
                            None,
                            server_message::Message::BufferUpdated(BufferUpdatedPush {
                                path: path.clone(),
                                new_server_version: new_server_version.as_u64(),
                                expected_client_version: expected_client_version.as_u64(),
                                edits: proto_edits.clone(),
                            }),
                        );
                    }
                }
                GlobalBufferModelEvent::FileSaved { file_id } => {
                    for (request_id, conn_id) in me
                        .buffers
                        .take_pending_by_kind(file_id, PendingBufferRequestKind::SaveBuffer)
                    {
                        me.send_server_message(
                            Some(conn_id),
                            Some(&request_id),
                            server_message::Message::SaveBufferResponse(SaveBufferResponse {
                                result: Some(save_buffer_response::Result::Success(
                                    SaveBufferSuccess {},
                                )),
                            }),
                        );
                    }
                    for (request_id, conn_id) in me
                        .buffers
                        .take_pending_by_kind(file_id, PendingBufferRequestKind::ResolveConflict)
                    {
                        me.send_server_message(
                            Some(conn_id),
                            Some(&request_id),
                            server_message::Message::ResolveConflictResponse(
                                ResolveConflictResponse {
                                    result: Some(resolve_conflict_response::Result::Success(
                                        ResolveConflictSuccess {},
                                    )),
                                },
                            ),
                        );
                    }
                }
                GlobalBufferModelEvent::FailedToSave { file_id, error } => {
                    for (request_id, conn_id) in me
                        .buffers
                        .take_pending_by_kind(file_id, PendingBufferRequestKind::SaveBuffer)
                    {
                        me.send_server_message(
                            Some(conn_id),
                            Some(&request_id),
                            server_message::Message::SaveBufferResponse(SaveBufferResponse {
                                result: Some(save_buffer_response::Result::Error(
                                    FileOperationError {
                                        message: format!("{error}"),
                                    },
                                )),
                            }),
                        );
                    }
                    for (request_id, conn_id) in me
                        .buffers
                        .take_pending_by_kind(file_id, PendingBufferRequestKind::ResolveConflict)
                    {
                        me.send_server_message(
                            Some(conn_id),
                            Some(&request_id),
                            server_message::Message::ResolveConflictResponse(
                                ResolveConflictResponse {
                                    result: Some(resolve_conflict_response::Result::Error(
                                        FileOperationError {
                                            message: format!("{error}"),
                                        },
                                    )),
                                },
                            ),
                        );
                    }
                }
                GlobalBufferModelEvent::FailedToLoad { file_id, error } => {
                    for (request_id, conn_id) in me
                        .buffers
                        .take_pending_by_kind(file_id, PendingBufferRequestKind::OpenBuffer)
                    {
                        me.send_server_message(
                            Some(conn_id),
                            Some(&request_id),
                            server_message::Message::Error(ErrorResponse {
                                code: ErrorCode::Internal.into(),
                                message: format!("Failed to load buffer: {error}"),
                            }),
                        );
                    }
                }
                GlobalBufferModelEvent::BufferUpdatedFromFileEvent { .. }
                | GlobalBufferModelEvent::RemoteBufferConflict { .. } => {
                    // Not relevant for server-local buffers.
                }
            });
        }
        // Refresh the pushed `RemoteAgentContextSnapshot` whenever the daemon's own
        // home skills change. The fork's `SkillManagerEvent` (unlike the pin's
        // `SkillsChanged { home_skills_changed }`) doesn't distinguish home from
        // project skill changes, so this refreshes on every change — a superset of
        // the pin's trigger, safe but occasionally redundant.
        #[cfg(feature = "local_fs")]
        {
            let skill_manager = SkillManager::handle(ctx);
            ctx.subscribe_to_model(&skill_manager, |me, event, ctx| match event {
                SkillManagerEvent::InventoryChanged => {
                    me.refresh_remote_agent_context_snapshot(ctx);
                }
            });
        }
        // Parse the bundled skill catalog from the global install location.
        // Parsing never blocks the initialize handshake: connections that
        // initialize before parsing completes receive the catalog via the
        // completion broadcast instead. Deliberately not feature-flag gated:
        // the flag controls exposure on the client (catalog storage and
        // skill selection), where the connecting user's flag state actually
        // lives — a headless daemon only sees its own channel defaults.
        #[cfg(feature = "local_fs")]
        if let Some(resources_dir) = daemon_bundled_resources_dir() {
            ctx.spawn(
                BundledSkill::detect_in_resources_dir(resources_dir),
                |me, catalog, ctx| {
                    let skills = bundled_skill_snapshot_protos(&catalog);
                    log::info!("Daemon parsed {} bundled skills", skills.len());
                    me.bundled_skills = skills;
                    me.refresh_remote_agent_context_snapshot(ctx);
                },
            );
        } else {
            log::info!(
                "Daemon found no global bundled resources directory; \
                 bundled skills unavailable on this host"
            );
        }
        // Start the grace timer immediately so the daemon exits if no proxy
        // connects within GRACE_PERIOD. In practice the spawning proxy connects
        // within milliseconds, so the risk of premature shutdown is negligible;
        // register_connection will cancel the timer the moment the first proxy
        // arrives.
        model.start_grace_timer(ctx);
        model
    }

    /// Recomputes `remote_agent_context_snapshot` at the next revision and
    /// broadcasts it to every connection.
    #[cfg(feature = "local_fs")]
    fn refresh_remote_agent_context_snapshot(&mut self, ctx: &warpui::AppContext) {
        let revision = self.remote_agent_context_snapshot.revision.saturating_add(1);
        self.remote_agent_context_snapshot =
            remote_agent_context_snapshot(revision, &self.bundled_skills, ctx);
        self.broadcast_remote_agent_context_snapshot();
    }

    /// Pushes the current `remote_agent_context_snapshot` to every connection and
    /// marks them all as having received it.
    #[cfg(feature = "local_fs")]
    fn broadcast_remote_agent_context_snapshot(&mut self) {
        self.send_server_message(
            None,
            None,
            server_message::Message::RemoteAgentContextSnapshot(
                self.remote_agent_context_snapshot.clone(),
            ),
        );
        self.remote_agent_context_snapshot_sent
            .extend(self.connection_senders.keys().copied());
    }

    /// Sends the current `remote_agent_context_snapshot` to `conn_id` if it hasn't
    /// already received this revision. Called on (re)connect.
    #[cfg(feature = "local_fs")]
    fn send_remote_agent_context_snapshot_to_connection(&mut self, conn_id: ConnectionId) {
        if self.remote_agent_context_snapshot_sent.contains(&conn_id) {
            return;
        }
        self.send_server_message(
            Some(conn_id),
            None,
            server_message::Message::RemoteAgentContextSnapshot(
                self.remote_agent_context_snapshot.clone(),
            ),
        );
        self.remote_agent_context_snapshot_sent.insert(conn_id);
    }

    /// Called when a proxy connects.  Inserts `conn_tx` into the connection
    /// map so `send_server_message` can route responses to this proxy, and
    /// cancels the grace timer if it was running.
    pub fn register_connection(
        &mut self,
        conn_id: ConnectionId,
        conn_tx: async_channel::Sender<ServerMessage>,
        ctx: &mut ModelContext<Self>,
    ) {
        log::info!(
            "Daemon: connection {conn_id} registered — {} active, host_id={}",
            self.connection_senders.len() + 1,
            self.host_id
        );
        if let Some(handle) = self.grace_timer_cancel.take() {
            handle.abort();
        }
        self.connection_senders.insert(conn_id, conn_tx);
        self.snapshot_sent_roots_by_connection
            .insert(conn_id, HashSet::new());
        #[cfg(feature = "local_fs")]
        self.send_remote_agent_context_snapshot_to_connection(conn_id);
        ctx.notify();
    }

    /// Called when a proxy disconnects.  Removes it from the connection map
    /// and starts the grace timer if no connections remain.
    pub fn deregister_connection(&mut self, conn_id: ConnectionId, ctx: &mut ModelContext<Self>) {
        self.snapshot_sent_roots_by_connection.remove(&conn_id);
        #[cfg(feature = "local_fs")]
        self.diff_state_subscriptions.remove(&conn_id);
        #[cfg(feature = "local_fs")]
        self.remote_agent_context_snapshot_sent.remove(&conn_id);
        // A connection is in at most one git-status repo, so `unsubscribe_git_status`
        // also serves as the disconnect sweep (mirrors the pin's comment on the
        // same method).
        #[cfg(feature = "local_fs")]
        self.unsubscribe_git_status(conn_id);
        // Guard against double-deregister (reader and writer tasks both call
        // this on connection close; the second call must be a safe no-op).
        if self.connection_senders.remove(&conn_id).is_none() {
            return;
        }
        // Drop this connection from all open server-local buffers; orphaned
        // buffers (no remaining connections) are deallocated by the tracker.
        #[cfg(feature = "local_fs")]
        self.buffers.remove_connection(conn_id, ctx);
        let remaining = self.connection_senders.len();
        log::info!("Daemon: connection {conn_id} deregistered — {remaining} active remaining");
        if remaining == 0 {
            log::info!("Daemon: grace timer started ({GRACE_PERIOD:?})");
            self.start_grace_timer(ctx);
        }
        ctx.notify();
    }

    /// Starts (or restarts) a timer that shuts the daemon down after
    /// [`GRACE_PERIOD`] with no connected proxies.  If a timer is already
    /// running its abort handle is cancelled before the new one is stored.
    /// When a proxy connects, `register_connection` aborts the handle,
    /// preventing the shutdown.
    fn start_grace_timer(&mut self, ctx: &mut ModelContext<Self>) {
        if let Some(handle) = self.grace_timer_cancel.take() {
            handle.abort();
        }
        let handle = ctx.spawn_abortable(
            async_io::Timer::after(GRACE_PERIOD),
            |_, _, ctx| {
                log::info!("Daemon: grace period expired, shutting down");
                ctx.terminate_app(TerminationMode::ForceTerminate, None);
            },
            |_, _| {
                log::debug!("Daemon: grace timer cancelled");
            },
        );
        self.grace_timer_cancel = Some(handle);
    }

    /// Called by the background stdin reader task via `ModelSpawner`.
    ///
    /// Dispatches on the `oneof message` variant. Notifications are handled
    /// inline; request-style messages return a `HandlerOutcome` that is
    /// centrally acted on here: `Sync` responses are sent immediately and
    /// `Async` handles are tracked in `in_progress` so they can be aborted.
    pub fn handle_message(
        &mut self,
        conn_id: ConnectionId,
        msg: ClientMessage,
        ctx: &mut ModelContext<Self>,
    ) {
        let request_id = RequestId::from(msg.request_id);

        // Dispatches through the host-scoped / session-scoped / notification
        // envelope (#438). Deviation from the pin: the daemon still answers a
        // host-scoped request only on its originating connection — this port
        // carries the wire shape but not the pin's cross-connection failover
        // (see the envelope's proto doc comment for why that's out of scope
        // here).
        let outcome = match msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest { message })) => {
                match message {
                    Some(host_scoped_request::Message::WriteFile(msg)) => {
                        self.handle_write_file(msg, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::DeleteFile(msg)) => {
                        self.handle_delete_file(msg, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::ReadFileContext(msg)) => {
                        self.handle_read_file_context(msg, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::SaveBuffer(msg)) => {
                        self.handle_save_buffer(msg, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::ResolveConflict(msg)) => {
                        self.handle_resolve_conflict(msg, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::GetBranches(req)) => {
                        self.handle_get_branches(req, &request_id, conn_id, ctx)
                    }
                    // Git write-ops over SSH (#116): the daemon runs the git / gh
                    // subprocesses host-local against the daemon's filesystem,
                    // mirroring the local code-review dialog so local and remote
                    // behave identically.
                    Some(host_scoped_request::Message::GitCommitChain(req)) => {
                        self.handle_git_commit_chain(req, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::GitPush(req)) => {
                        self.handle_git_push(req, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::GitCreatePr(req)) => {
                        self.handle_create_pr(req, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::GetCommittedBranchFiles(req)) => {
                        self.handle_get_committed_branch_files(req, &request_id, conn_id, ctx)
                    }
                    // Discard changes over SSH (#437): the SSH-remote equivalent of
                    // the local code-review "discard changes" action. Runs
                    // git restore/stash/rm on the daemon's filesystem.
                    Some(host_scoped_request::Message::DiscardFiles(req)) => {
                        self.handle_discard_files(req, &request_id, conn_id, ctx)
                    }
                    Some(host_scoped_request::Message::RipgrepSearch(req)) => {
                        self.handle_ripgrep_search(req, &request_id, conn_id, ctx)
                    }
                    // Zap: directory listing for remote terminal file links (used
                    // to validate path shape).
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::ListDirectory(msg)) => {
                        self.handle_list_directory(msg)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::ResolvePath(msg)) => {
                        self.handle_resolve_path(msg)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::CreateDirectory(msg)) => {
                        self.handle_create_directory(msg)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::ReadFileChunk(msg)) => {
                        self.handle_read_file_chunk(msg)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::WriteFileChunk(msg)) => {
                        self.handle_write_file_chunk(msg)
                    }
                    #[cfg(not(feature = "local_fs"))]
                    Some(
                        host_scoped_request::Message::SaveBuffer(_)
                        | host_scoped_request::Message::ResolveConflict(_)
                        | host_scoped_request::Message::ListDirectory(_)
                        | host_scoped_request::Message::ResolvePath(_)
                        | host_scoped_request::Message::CreateDirectory(_)
                        | host_scoped_request::Message::ReadFileChunk(_)
                        | host_scoped_request::Message::WriteFileChunk(_),
                    ) => HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                        code: ErrorCode::InvalidRequest.into(),
                        message: "Buffer syncing requires the local_fs feature".to_string(),
                    })),
                    None => {
                        log::warn!(
                            "Received HostScopedRequest with no message variant \
                             (request_id={request_id})"
                        );
                        HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                            code: ErrorCode::InvalidRequest.into(),
                            message: "HostScopedRequest had no message variant set".to_string(),
                        }))
                    }
                }
            }
            Some(client_message::Message::SessionScoped(SessionScopedRequest { message })) => {
                match message {
                    Some(session_scoped_request::Message::Initialize(msg)) => {
                        self.handle_initialize(msg, &request_id)
                    }
                    Some(session_scoped_request::Message::NavigatedToDirectory(msg)) => {
                        self.handle_navigated_to_directory(msg, &request_id, conn_id, ctx)
                    }
                    Some(session_scoped_request::Message::LoadRepoMetadataDirectory(msg)) => {
                        self.handle_load_repo_metadata_directory(msg, &request_id, ctx)
                    }
                    Some(session_scoped_request::Message::RunCommand(req)) => {
                        self.handle_run_command(req, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(session_scoped_request::Message::OpenBuffer(msg)) => {
                        self.handle_open_buffer(msg, &request_id, conn_id, ctx)
                    }
                    Some(session_scoped_request::Message::GetDiffState(req)) => {
                        self.handle_get_diff_state(req, &request_id, conn_id, ctx)
                    }
                    #[cfg(not(feature = "local_fs"))]
                    Some(session_scoped_request::Message::OpenBuffer(_)) => {
                        HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                            code: ErrorCode::InvalidRequest.into(),
                            message: "Buffer syncing requires the local_fs feature".to_string(),
                        }))
                    }
                    None => {
                        log::warn!(
                            "Received SessionScopedRequest with no message variant \
                             (request_id={request_id})"
                        );
                        HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                            code: ErrorCode::InvalidRequest.into(),
                            message: "SessionScopedRequest had no message variant set".to_string(),
                        }))
                    }
                }
            }
            Some(client_message::Message::Notification(Notification { message })) => {
                match message {
                    Some(notification::Message::Abort(abort)) => {
                        self.handle_abort(abort, &request_id);
                        return;
                    }
                    Some(notification::Message::Authenticate(msg)) => {
                        self.handle_authenticate(msg);
                        return;
                    }
                    Some(notification::Message::SessionBootstrapped(msg)) => {
                        self.handle_session_bootstrapped(msg);
                        return;
                    }
                    #[cfg(feature = "local_fs")]
                    Some(notification::Message::BufferEdit(msg)) => {
                        self.handle_buffer_edit(msg, ctx);
                        return; // fire-and-forget notification
                    }
                    #[cfg(feature = "local_fs")]
                    Some(notification::Message::CloseBuffer(msg)) => {
                        self.handle_close_buffer(msg, conn_id, ctx);
                        return; // fire-and-forget notification
                    }
                    Some(notification::Message::UnsubscribeDiffState(msg)) => {
                        self.handle_unsubscribe_diff_state(msg, conn_id, ctx);
                        return; // fire-and-forget notification
                    }
                    // Notifications carry no response contract, but buffer
                    // syncing is unavailable without `local_fs` — mirror the
                    // host/session-scoped fallback and answer with an explicit
                    // error rather than silently dropping the message, so the
                    // client doesn't wait on state that will never change.
                    #[cfg(not(feature = "local_fs"))]
                    Some(
                        notification::Message::BufferEdit(_)
                        | notification::Message::CloseBuffer(_),
                    ) => {
                        self.send_server_message(
                            Some(conn_id),
                            Some(&request_id),
                            server_message::Message::Error(ErrorResponse {
                                code: ErrorCode::InvalidRequest.into(),
                                message: "Buffer syncing requires the local_fs feature".to_string(),
                            }),
                        );
                        return;
                    }
                    None => {
                        log::warn!(
                            "Received Notification with no message variant \
                             (request_id={request_id})"
                        );
                        return;
                    }
                }
            }
            None => {
                log::warn!(
                    "Received ClientMessage with no message variant (request_id={request_id})"
                );
                HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                    code: ErrorCode::InvalidRequest.into(),
                    message: "ClientMessage had no message variant set".to_string(),
                }))
            }
        };

        match outcome {
            HandlerOutcome::Sync(message) => {
                self.send_server_message(Some(conn_id), Some(&request_id), message);
            }
            HandlerOutcome::Async(Some(handle)) => {
                self.in_progress.insert(request_id, handle);
            }
            HandlerOutcome::Async(None) => {
                // Async work tracked elsewhere (e.g. `pending_file_ops`);
                // the response will be sent via an event subscription.
            }
        }
    }

    /// Routes a server message to its destination.
    ///
    /// - `conn_id = Some(id)` — sends only to the connection that originated
    ///   the request (used for all request/response pairs).
    /// - `conn_id = None` — broadcasts to every connected proxy (used for
    ///   server-initiated push notifications such as repo metadata updates).
    ///
    /// For host-scoped requests (tracked in `host_scoped_requests`): if the
    /// target connection is gone, or its channel rejects the send, the
    /// response is delivered through any other open connection instead of
    /// being dropped. This covers a session disconnecting (e.g. closing a
    /// tab) while a host-scoped request it issued is still in flight —
    /// another tab connected to the same host still gets the response.
    /// Non-host-scoped responses are never failed over; their target
    /// connection owns state (e.g. a subscription) that a sibling
    /// connection does not have.
    fn send_server_message(
        &mut self,
        conn_id: Option<ConnectionId>,
        request_id: Option<&RequestId>,
        message: server_message::Message,
    ) {
        // Sending a response is the terminal step of a host-scoped request,
        // so its failover-tracking entry is dropped here regardless of
        // which path below actually delivers it. Whether the request was
        // tracked is snapshotted *before* removal, since that's what decides
        // failover eligibility below. Push notifications (no request_id)
        // are never tracked, so this is a no-op for them.
        let is_host_scoped_response = request_id
            .is_some_and(|rid| !rid.is_empty() && self.host_scoped_requests.contains_key(rid));
        if let Some(rid) = request_id {
            self.host_scoped_requests.remove(rid);
        }

        let msg = ServerMessage {
            request_id: request_id.map(|id| id.clone().into()).unwrap_or_default(),
            message: Some(message),
        };
        if let Some(target) = conn_id {
            if let Some(conn_tx) = self.connection_senders.get(&target) {
                if let Err(e) = conn_tx.try_send(msg.clone()) {
                    log::warn!("Daemon: failed to send to conn {target}: {e}");
                    if is_host_scoped_response {
                        self.send_host_scoped_response_via_alternate_connection(target, msg);
                    }
                }
            } else if is_host_scoped_response {
                // Target connection is gone. Deliver the host-scoped
                // response through any other open connection.
                self.send_host_scoped_response_via_alternate_connection(target, msg);
            } else {
                log::debug!("Daemon: no sender for conn {target} (already disconnected)");
            }
        } else {
            // Push notification — broadcast to all connections.
            for (id, conn_tx) in &self.connection_senders {
                if let Err(e) = conn_tx.try_send(msg.clone()) {
                    log::warn!("Daemon: failed to send to conn {id}: {e}");
                }
            }
        }
    }

    /// Delivers a host-scoped response through a connected proxy other than
    /// `target`. Used when the original connection has disappeared or its
    /// outbound channel rejected the response.
    fn send_host_scoped_response_via_alternate_connection(
        &self,
        target: ConnectionId,
        msg: ServerMessage,
    ) {
        for (&alt_id, alt_tx) in &self.connection_senders {
            if alt_id == target {
                continue;
            }
            log::info!(
                "Daemon: failover delivery for request_id={} from conn {target} to conn {alt_id}",
                msg.request_id
            );
            match alt_tx.try_send(msg.clone()) {
                Ok(()) => return,
                Err(e) => {
                    log::warn!("Daemon: failover delivery failed to conn {alt_id}: {e}");
                }
            }
        }
        log::warn!(
            "Daemon: cannot deliver host-scoped response for request_id={}, \
             no alternate connections available",
            msg.request_id
        );
    }

    /// Spawns an abortable future tied to `request_id` and wires up automatic
    /// removal from `in_progress` on completion or abort.
    ///
    /// The returned handle is intended to be returned from a handler as
    /// `HandlerOutcome::Async(Some(handle))`; the caller (`handle_message`)
    /// inserts it into `in_progress`.
    fn spawn_request_handler<S, F>(
        &mut self,
        request_id: RequestId,
        future: S,
        on_resolve: F,
        ctx: &mut ModelContext<Self>,
    ) -> SpawnedFutureHandle
    where
        S: Spawnable,
        <S as Future>::Output: SpawnableOutput,
        F: 'static + FnOnce(&mut Self, <S as Future>::Output, &mut ModelContext<Self>),
    {
        let resolve_id = request_id.clone();
        let abort_id = request_id;
        ctx.spawn_abortable(
            future,
            move |me, output, ctx| {
                me.in_progress.remove(&resolve_id);
                on_resolve(me, output, ctx);
            },
            move |me, _ctx| {
                log::info!("Request cancelled (request_id={abort_id})");
                me.in_progress.remove(&abort_id);
            },
        )
    }

    /// Handles `Initialize` by returning the server version and host id.
    ///
    /// `server_version` is the release tag the daemon was built from
    /// (`GIT_RELEASE_TAG`) or the empty string for `cargo run` / locally
    /// deployed builds. The client treats an empty version as "unknown" and
    /// skips strict version enforcement, which keeps the
    /// `script/deploy_remote_server` developer workflow functional.
    fn handle_initialize(&mut self, msg: Initialize, request_id: &RequestId) -> HandlerOutcome {
        log::info!("Handling Initialize (request_id={request_id})");
        if !msg.auth_token.is_empty() {
            self.auth_token = Some(msg.auth_token);
        }
        let server_version = ChannelState::app_version().unwrap_or("").to_string();
        HandlerOutcome::Sync(server_message::Message::InitializeResponse(
            InitializeResponse {
                server_version,
                host_id: self.host_id.clone(),
            },
        ))
    }

    /// Handles `Authenticate` by replacing the daemon-wide credential.
    /// This is a notification — no response is sent.
    fn handle_authenticate(&mut self, msg: Authenticate) {
        if msg.auth_token.is_empty() {
            log::warn!("Received Authenticate notification with empty auth token; ignoring");
            return;
        }
        self.auth_token = Some(msg.auth_token);
    }

    pub fn auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    /// Handles `Abort` by cancelling the in-progress request it targets.
    /// This is a notification — no response is sent.
    fn handle_abort(&mut self, abort: Abort, request_id: &RequestId) {
        let target_id = RequestId::from(abort.request_id_to_abort);
        if let Some(handle) = self.in_progress.remove(&target_id) {
            log::info!(
                "Aborting in-progress request (request_id={target_id}, \
                 abort_request_id={request_id})"
            );
            handle.abort();
        } else {
            log::info!(
                "Abort for unknown/completed request (request_id={target_id}, \
                 abort_request_id={request_id})"
            );
        }
    }

    /// Handles `SessionBootstrapped` by creating a `LocalCommandExecutor` for
    /// the session. This is a notification — no response is sent.
    fn handle_session_bootstrapped(&mut self, msg: SessionBootstrapped) {
        let session_id = SessionId::from(msg.session_id);
        log::info!(
            "Handling SessionBootstrapped: session_id={session_id:?}, \
             shell_type={:?}, shell_path={:?}",
            msg.shell_type,
            msg.shell_path,
        );

        let Some(shell_type) = ShellType::from_name(&msg.shell_type) else {
            log::error!(
                "Unknown shell_type {:?} in SessionBootstrapped for session {session_id:?}",
                msg.shell_type,
            );
            return;
        };

        let shell_path = msg.shell_path.map(PathBuf::from);
        if shell_path.is_none() {
            log::warn!(
                "SessionBootstrapped for session {session_id:?} had no shell_path; \
                 LocalCommandExecutor will fall back to bare shell name",
            );
        }
        let executor = Arc::new(LocalCommandExecutor::new(shell_path, shell_type));
        if self.executors.insert(session_id, executor).is_some() {
            log::warn!(
                "Overwriting existing executor for session {session_id:?} \
                 (re-SessionBootstrapped with shell_type={:?})",
                msg.shell_type,
            );
        }
    }

    /// Handles `RunCommand` by delegating to the session's `LocalCommandExecutor`.
    ///
    /// On success, returns a `HandlerOutcome::Async` whose task resolves the
    /// request with a `RunCommandResponse`. On validation failure (missing
    /// executor), returns a `HandlerOutcome::Sync` error response.
    fn handle_ripgrep_search(
        &mut self,
        msg: RipgrepSearchRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling RipgrepSearch ({} roots, request_id={request_id})",
            msg.roots.len()
        );

        let params = match super::ripgrep_search::validate_request(msg) {
            Ok(params) => params,
            Err(message) => {
                return HandlerOutcome::Sync(server_message::Message::RipgrepSearchResponse(
                    super::ripgrep_search::error_response(message),
                ));
            }
        };

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move { super::ripgrep_search::run_search(params).await },
            move |me, result, _ctx| {
                let response = super::ripgrep_search::search_result_to_response(result);
                me.send_server_message(
                    Some(conn_id),
                    Some(&request_id_for_response),
                    server_message::Message::RipgrepSearchResponse(response),
                );
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `GetBranches` — lists the git branches of a repo on the remote
    /// host, backing the code-review branch picker over SSH. Reuses the same
    /// `git for-each-ref` listing used by local code review
    /// ([`LocalDiffStateModel::get_all_branches`]).
    fn handle_get_branches(
        &mut self,
        msg: GetBranches,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => p.to_local_path_lossy(),
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::GetBranchesResponse(
                    super::get_branches::error_response(format!("Invalid repo_path: {e}")),
                ));
            }
        };

        let max_branch_count = msg
            .max_branch_count
            .map(|c| (c as usize).min(MAX_BRANCH_COUNT_CAP));
        let include_remotes = msg.include_remotes;

        log::info!(
            "Handling GetBranches repo={} (request_id={request_id})",
            msg.repo_path,
        );

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                crate::code_review::diff_state::LocalDiffStateModel::get_all_branches(
                    &repo_path,
                    max_branch_count,
                    include_remotes,
                )
                .await
            },
            move |me, branches_result, _ctx| {
                let response = super::get_branches::branches_result_to_response(branches_result);
                me.send_server_message(
                    Some(conn_id),
                    Some(&request_id_for_response),
                    server_message::Message::GetBranchesResponse(response),
                );
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `GetCommittedBranchFiles` — lists the committed-only changed
    /// files of the current branch (`main...HEAD`) on the remote host, backing
    /// the code-review file list over SSH. Reuses the same git listing as local
    /// code review ([`LocalDiffStateModel::get_committed_branch_file_entries`]).
    fn handle_get_committed_branch_files(
        &mut self,
        msg: GetCommittedBranchFilesRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => p.to_local_path_lossy(),
            Err(e) => {
                return HandlerOutcome::Sync(
                    server_message::Message::GetCommittedBranchFilesResponse(
                        super::get_committed_branch_files::error_response(format!(
                            "Invalid repo_path: {e}"
                        )),
                    ),
                );
            }
        };

        log::info!(
            "Handling GetCommittedBranchFiles repo={} (request_id={request_id})",
            msg.repo_path,
        );

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                crate::code_review::diff_state::LocalDiffStateModel::get_committed_branch_file_entries(
                    &repo_path,
                )
                .await
            },
            move |me, files_result, _ctx| {
                let response =
                    super::get_committed_branch_files::files_result_to_response(files_result);
                me.send_server_message(
                    Some(conn_id),
                    Some(&request_id_for_response),
                    server_message::Message::GetCommittedBranchFilesResponse(response),
                );
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Maps the wire commit-chain mode to the `util::git` orchestration mode.
    fn commit_chain_mode_from_proto(mode: GitCommitChainMode) -> crate::util::git::CommitChainMode {
        use crate::util::git::CommitChainMode;
        match mode {
            GitCommitChainMode::CommitOnly => CommitChainMode::CommitOnly,
            GitCommitChainMode::CommitAndPush => CommitChainMode::CommitAndPush,
            GitCommitChainMode::CommitAndCreatePr => CommitChainMode::CommitAndCreatePr,
        }
    }

    /// Handles `GitCommitChainRequest` — runs the commit chain (commit, then
    /// optionally push, then optionally create-PR) on the daemon's filesystem
    /// in a single round trip, returning the post-chain delta (refreshed
    /// unpushed commits + upstream) and any created PR.
    ///
    /// `path_env` is `None`: the remote host's daemon runs from a login/sshd
    /// context with a normal `PATH`, so — unlike Warp's macOS GUI, which must
    /// capture an interactive-shell `PATH` for launchd-spawned processes — it
    /// finds `git` / `gh` directly.
    ///
    /// BYOP divergence (#116): `autogenerate_pr_content` is accepted for
    /// protocol parity but ignored — the fork drops Warp's cloud AIClient and
    /// the daemon has no BYOP provider reachable, so create-PR falls back to
    /// `gh pr create --fill` (see `util::git::run_commit_chain`).
    fn handle_git_commit_chain(
        &mut self,
        msg: GitCommitChainRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => PathBuf::from(p.to_local_path_lossy()),
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::GitCommitChainResponse(
                    GitCommitChainResponse {
                        result: Some(git_commit_chain_response::Result::Error(GitOpError {
                            message: format!("Invalid repo_path: {e}"),
                        })),
                    },
                ));
            }
        };
        let mode = Self::commit_chain_mode_from_proto(msg.mode());
        let message = msg.message;
        let include_unstaged = msg.include_unstaged;
        let branch = msg.branch;
        let _ = msg.autogenerate_pr_content; // BYOP: ignored, see doc comment.

        log::info!(
            "Handling GitCommitChain repo={} mode={mode:?} (request_id={request_id})",
            msg.repo_path,
        );

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                guard_git_operation_in_progress(&repo_path)?;
                crate::util::git::run_commit_chain(
                    &repo_path,
                    mode,
                    &message,
                    include_unstaged,
                    &branch,
                    None,
                )
                .await
            },
            move |me, result, _ctx| {
                let message = match result {
                    Ok((commits, upstream_ref, pr_info)) => {
                        server_message::Message::GitCommitChainResponse(GitCommitChainResponse {
                            result: Some(git_commit_chain_response::Result::Success(
                                GitCommitChainSuccess {
                                    delta: Some(GitOpDelta {
                                        unpushed_commits: commits
                                            .iter()
                                            .map(super::diff_state_proto::commit_to_proto)
                                            .collect(),
                                        upstream_ref,
                                    }),
                                    pr_info: pr_info
                                        .as_ref()
                                        .map(super::diff_state_proto::pr_info_to_proto),
                                },
                            )),
                        })
                    }
                    Err(e) => {
                        server_message::Message::GitCommitChainResponse(GitCommitChainResponse {
                            result: Some(git_commit_chain_response::Result::Error(GitOpError {
                                message: format!("{e:#}"),
                            })),
                        })
                    }
                };
                me.send_server_message(Some(conn_id), Some(&request_id_for_response), message);
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `GitPushRequest` — runs `git push --set-upstream` on the
    /// daemon's filesystem, then returns the refreshed unpushed/upstream delta.
    fn handle_git_push(
        &mut self,
        msg: GitPushRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => PathBuf::from(p.to_local_path_lossy()),
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::GitPushResponse(
                    GitPushResponse {
                        result: Some(git_push_response::Result::Error(GitOpError {
                            message: format!("Invalid repo_path: {e}"),
                        })),
                    },
                ));
            }
        };
        let branch = msg.branch;

        log::info!(
            "Handling GitPush repo={} branch={branch} (request_id={request_id})",
            msg.repo_path,
        );

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                guard_git_operation_in_progress(&repo_path)?;
                crate::util::git::run_push(&repo_path, &branch, None).await?;
                anyhow::Ok(crate::util::git::compute_unpushed_state(&repo_path).await)
            },
            move |me, result, _ctx| {
                let message = match result {
                    Ok((commits, upstream_ref)) => {
                        server_message::Message::GitPushResponse(GitPushResponse {
                            result: Some(git_push_response::Result::Success(GitOpDelta {
                                unpushed_commits: commits
                                    .iter()
                                    .map(super::diff_state_proto::commit_to_proto)
                                    .collect(),
                                upstream_ref,
                            })),
                        })
                    }
                    Err(e) => server_message::Message::GitPushResponse(GitPushResponse {
                        result: Some(git_push_response::Result::Error(GitOpError {
                            message: format!("{e:#}"),
                        })),
                    }),
                };
                me.send_server_message(Some(conn_id), Some(&request_id_for_response), message);
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `GitCreatePrRequest` — runs `gh pr create` on the daemon's
    /// filesystem and returns the created PR info.
    ///
    /// BYOP divergence (#116): `autogenerate_content` is accepted for protocol
    /// parity but ignored — no BYOP provider is reachable on the daemon, so the
    /// PR is created with `gh pr create --fill` (see `util::git::create_pr`).
    fn handle_create_pr(
        &mut self,
        msg: GitCreatePrRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => PathBuf::from(p.to_local_path_lossy()),
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::GitCreatePrResponse(
                    GitCreatePrResponse {
                        result: Some(git_create_pr_response::Result::Error(GitOpError {
                            message: format!("Invalid repo_path: {e}"),
                        })),
                    },
                ));
            }
        };
        let _ = msg.branch; // Used by Warp's AI generation; unused in the --fill path.
        let _ = msg.autogenerate_content; // BYOP: ignored, see doc comment.

        log::info!(
            "Handling GitCreatePr repo={} (request_id={request_id})",
            msg.repo_path,
        );

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                guard_git_operation_in_progress(&repo_path)?;
                crate::util::git::create_pr(&repo_path, None, None, None).await
            },
            move |me, result, _ctx| {
                let message = match result {
                    Ok(pr) => server_message::Message::GitCreatePrResponse(GitCreatePrResponse {
                        result: Some(git_create_pr_response::Result::Success(
                            super::diff_state_proto::pr_info_to_proto(&pr),
                        )),
                    }),
                    Err(e) => server_message::Message::GitCreatePrResponse(GitCreatePrResponse {
                        result: Some(git_create_pr_response::Result::Error(GitOpError {
                            message: format!("{e:#}"),
                        })),
                    }),
                };
                me.send_server_message(Some(conn_id), Some(&request_id_for_response), message);
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `GetDiffState` — computes the current diff-state snapshot for a
    /// (repo, mode) pair on the remote filesystem and replies with it, then
    /// registers a subscription so subsequent repository changes push a fresh
    /// snapshot to this connection (see `push_diff_state_for_repo`).
    #[cfg(feature = "local_fs")]
    fn handle_get_diff_state(
        &mut self,
        msg: GetDiffState,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        use crate::code_review::diff_state::{DiffMode, LocalDiffStateModel};

        let canonical_path =
            match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path)) {
                Ok(p) => p,
                Err(e) => {
                    return HandlerOutcome::Sync(server_message::Message::GetDiffStateResponse(
                        super::diff_state_proto::error_response(format!("Invalid repo_path: {e}")),
                    ));
                }
            };

        let mode = super::diff_state_proto::proto_to_diff_mode(&msg.mode.unwrap_or_default());
        // Only compare against a base branch when the mode requires it.
        let include_base_branch = !matches!(mode, DiffMode::Head);
        // Echoed verbatim in the snapshot so the client's RemoteDiffStateModel
        // (which keys on the string it sent) matches pushes.
        let wire_repo_path = msg.repo_path.clone();
        let repo_pathbuf = std::path::PathBuf::from(canonical_path.to_local_path_lossy());

        log::info!(
            "Handling GetDiffState repo={} mode={mode:?} (request_id={request_id})",
            msg.repo_path,
        );

        // Register the subscription for live pushes on repository change.
        self.register_diff_state_subscription(
            conn_id,
            canonical_path,
            wire_repo_path.clone(),
            mode.clone(),
        );

        let request_id_for_response = request_id.clone();
        let mode_for_compute = mode.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                let metadata = LocalDiffStateModel::load_metadata_for_repo(
                    repo_pathbuf.clone(),
                    include_base_branch,
                )
                .await;
                // Loads the content-carrying `GitDiffWithBaseContent` (rather than
                // `load_diff_data_for_mode`'s content-less `GitDiffData`) so
                // `content_at_base` can reach the wire — see issue #388 item 4.
                let diff_data = LocalDiffStateModel::load_diff_with_base_content_for_mode(
                    mode_for_compute,
                    repo_pathbuf,
                )
                .await;
                (metadata, diff_data)
            },
            move |me, (metadata_result, diff_data), _ctx| {
                let snapshot = super::diff_state_proto::snapshot_from_parts_with_base_content(
                    wire_repo_path,
                    &mode,
                    metadata_result.ok(),
                    diff_data,
                );
                me.send_server_message(
                    Some(conn_id),
                    Some(&request_id_for_response),
                    server_message::Message::GetDiffStateResponse(
                        super::diff_state_proto::snapshot_response(snapshot),
                    ),
                );
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Records a `(repo, mode)` diff-state subscription for `conn_id`,
    /// deduplicating against an existing identical entry.
    #[cfg(feature = "local_fs")]
    fn register_diff_state_subscription(
        &mut self,
        conn_id: ConnectionId,
        canonical_path: StandardizedPath,
        wire_repo_path: String,
        mode: crate::code_review::diff_state::DiffMode,
    ) {
        let subs = self.diff_state_subscriptions.entry(conn_id).or_default();
        if !subs
            .iter()
            .any(|s| s.canonical_path == canonical_path && s.mode == mode)
        {
            subs.push(DiffStateSubscription {
                canonical_path,
                wire_repo_path,
                mode,
            });
        }
    }

    /// Pushes a fresh diff-state snapshot to every connection subscribed to
    /// `changed_repo`. Called when a repository's contents change.
    #[cfg(feature = "local_fs")]
    fn push_diff_state_for_repo(
        &mut self,
        changed_repo: &StandardizedPath,
        ctx: &mut ModelContext<Self>,
    ) {
        // Snapshot the matching subscriptions first so the async spawns don't
        // hold a borrow of `self.diff_state_subscriptions`.
        let matches: Vec<(ConnectionId, DiffStateSubscription)> = self
            .diff_state_subscriptions
            .iter()
            .flat_map(|(conn_id, subs)| {
                let conn_id = *conn_id;
                subs.iter()
                    .filter(|s| &s.canonical_path == changed_repo)
                    .cloned()
                    .map(move |s| (conn_id, s))
            })
            .collect();
        for (conn_id, sub) in matches {
            self.spawn_push_diff_state_snapshot(conn_id, sub, ctx);
        }
    }

    /// Recomputes the snapshot for one subscription and pushes it (unsolicited,
    /// no request_id) to `conn_id`.
    #[cfg(feature = "local_fs")]
    fn spawn_push_diff_state_snapshot(
        &mut self,
        conn_id: ConnectionId,
        sub: DiffStateSubscription,
        ctx: &mut ModelContext<Self>,
    ) {
        use crate::code_review::diff_state::{DiffMode, LocalDiffStateModel};

        let include_base_branch = !matches!(sub.mode, DiffMode::Head);
        let repo_pathbuf = std::path::PathBuf::from(sub.canonical_path.to_local_path_lossy());
        let mode = sub.mode.clone();
        let mode_for_compute = sub.mode;
        let wire_repo_path = sub.wire_repo_path;
        ctx.spawn_abortable(
            async move {
                let metadata = LocalDiffStateModel::load_metadata_for_repo(
                    repo_pathbuf.clone(),
                    include_base_branch,
                )
                .await;
                // See the matching comment in `handle_get_diff_state`: this loads
                // the content-carrying variant so live pushes also carry
                // `content_at_base` (issue #388 item 4).
                let diff_data = LocalDiffStateModel::load_diff_with_base_content_for_mode(
                    mode_for_compute,
                    repo_pathbuf,
                )
                .await;
                (metadata, diff_data)
            },
            move |me, (metadata_result, diff_data), _ctx| {
                let snapshot = super::diff_state_proto::snapshot_from_parts_with_base_content(
                    wire_repo_path,
                    &mode,
                    metadata_result.ok(),
                    diff_data,
                );
                me.send_server_message(
                    Some(conn_id),
                    None,
                    server_message::Message::DiffStateSnapshot(snapshot),
                );
            },
            |_me, _ctx| {},
        );
    }

    #[cfg(not(feature = "local_fs"))]
    fn handle_get_diff_state(
        &mut self,
        _msg: GetDiffState,
        _request_id: &RequestId,
        _conn_id: ConnectionId,
        _ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        HandlerOutcome::Sync(server_message::Message::GetDiffStateResponse(
            super::diff_state_proto::error_response(
                "Diff state requires the local_fs feature".to_string(),
            ),
        ))
    }

    /// Handles `DiscardFilesRequest` — runs `git restore`/`git stash`/
    /// `git rm` on the daemon's filesystem for the specified files, reusing
    /// the same `LocalDiffStateModel::discard_files_impl` logic the local
    /// code-review dialog uses (#437). On success, pushes a fresh diff-state
    /// snapshot to every connection subscribed to this repo so the
    /// code-review UI updates without waiting for the next file-watcher
    /// event.
    ///
    /// `msg.mode` is accepted but not used to select a cached model — see
    /// the field's doc comment in the proto for why (a discard invalidates
    /// every mode's diff for this repo, not just the requesting one).
    #[cfg(feature = "local_fs")]
    fn handle_discard_files(
        &mut self,
        msg: DiscardFilesRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        use crate::code_review::diff_state::{FileStatusInfo, LocalDiffStateModel};

        let canonical_path =
            match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path)) {
                Ok(p) => p,
                Err(e) => {
                    return HandlerOutcome::Sync(server_message::Message::DiscardFilesResponse(
                        DiscardFilesResponse {
                            result: Some(discard_files_response::Result::Error(
                                DiscardFilesError {
                                    message: format!("Invalid repo_path: {e}"),
                                },
                            )),
                        },
                    ));
                }
            };

        if msg.files.is_empty() {
            return HandlerOutcome::Sync(server_message::Message::DiscardFilesResponse(
                DiscardFilesResponse {
                    result: Some(discard_files_response::Result::Error(DiscardFilesError {
                        message: "No files specified in DiscardFilesRequest".to_string(),
                    })),
                },
            ));
        }

        // Decoding is fallible: #326 replaced the infallible
        // `proto_to_file_status_info` with a validating `TryFrom` that rejects
        // non-absolute paths and missing status variants. Surface a malformed
        // entry as a DiscardFilesError rather than acting on a half-decoded
        // request -- discarding files against a bad path is destructive.
        let file_infos: Vec<FileStatusInfo> = match msg
            .files
            .iter()
            .map(FileStatusInfo::try_from)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(infos) => infos,
            Err(err) => {
                return HandlerOutcome::Sync(server_message::Message::DiscardFilesResponse(
                    DiscardFilesResponse {
                        result: Some(discard_files_response::Result::Error(DiscardFilesError {
                            message: format!("Invalid file entry in DiscardFilesRequest: {err}"),
                        })),
                    },
                ));
            }
        };
        let should_stash = msg.should_stash;
        let branch = msg.branch_name.unwrap_or_else(|| "HEAD".to_string());
        let repo_path = PathBuf::from(canonical_path.to_local_path_lossy());

        log::info!(
            "Handling DiscardFiles repo={} files={} (request_id={request_id})",
            msg.repo_path,
            file_infos.len(),
        );

        let request_id_for_response = request_id.clone();
        let repo_path_for_push = canonical_path.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                guard_git_operation_in_progress(&repo_path)?;
                LocalDiffStateModel::discard_files_impl(&repo_path, file_infos, should_stash, &branch)
                    .await
            },
            move |me, result, ctx| {
                let message = match result {
                    Ok(_) => {
                        me.push_diff_state_for_repo(&repo_path_for_push, ctx);
                        server_message::Message::DiscardFilesResponse(DiscardFilesResponse {
                            result: Some(discard_files_response::Result::Success(
                                DiscardFilesSuccess {},
                            )),
                        })
                    }
                    Err(e) => server_message::Message::DiscardFilesResponse(DiscardFilesResponse {
                        result: Some(discard_files_response::Result::Error(DiscardFilesError {
                            message: format!("{e:#}"),
                        })),
                    }),
                };
                me.send_server_message(Some(conn_id), Some(&request_id_for_response), message);
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    #[cfg(not(feature = "local_fs"))]
    fn handle_discard_files(
        &mut self,
        _msg: DiscardFilesRequest,
        _request_id: &RequestId,
        _conn_id: ConnectionId,
        _ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        HandlerOutcome::Sync(server_message::Message::DiscardFilesResponse(
            DiscardFilesResponse {
                result: Some(discard_files_response::Result::Error(DiscardFilesError {
                    message: "Discard files requires the local_fs feature".to_string(),
                })),
            },
        ))
    }

    /// Handles `UnsubscribeDiffState` — fire-and-forget removal of a diff-state
    /// subscription for a `(repo, mode)` pair on this connection.
    #[cfg(feature = "local_fs")]
    fn handle_unsubscribe_diff_state(
        &mut self,
        msg: UnsubscribeDiffState,
        conn_id: ConnectionId,
        _ctx: &mut ModelContext<Self>,
    ) {
        let Ok(canonical_path) =
            StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        else {
            return;
        };
        let mode = super::diff_state_proto::proto_to_diff_mode(&msg.mode.unwrap_or_default());
        if let Some(subs) = self.diff_state_subscriptions.get_mut(&conn_id) {
            subs.retain(|s| !(s.canonical_path == canonical_path && s.mode == mode));
            if subs.is_empty() {
                self.diff_state_subscriptions.remove(&conn_id);
            }
        }
    }

    #[cfg(not(feature = "local_fs"))]
    fn handle_unsubscribe_diff_state(
        &mut self,
        _msg: UnsubscribeDiffState,
        _conn_id: ConnectionId,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    // ── Git-status / GitHub: per-connection subscription tracking ───────
    //
    // Ported from the pinned oracle's `subscribe_git_status` /
    // `unsubscribe_git_status` (issue #330). Pure bookkeeping: which
    // connection currently watches which repo's git status, and eviction of
    // the (currently always-empty; see the `git_status_models` field doc)
    // per-repo model caches once a repo has no subscribers left. The pin
    // also drives these from `NavigatedToDirectory` / `UpdateGitStatus`
    // handlers and pushes `GitStatusPush` messages on model events; that
    // wiring is a separate, larger feature gap and is not part of this
    // change.

    /// Subscribe `conn` to `repo`'s git status (navigation in), moving it off
    /// any repo it was previously in. A no-op if `conn` is already the repo's
    /// subscriber.
    ///
    /// Not yet called from a handler — the `NavigatedToDirectory` wiring is
    /// the separate feature gap noted above — so this is currently exercised
    /// only by tests.
    #[cfg(feature = "local_fs")]
    #[allow(dead_code)]
    fn subscribe_git_status(&mut self, conn: ConnectionId, repo: &StandardizedPath) {
        match self.git_status_repo_by_conn.get(&conn) {
            Some(prev) if prev == repo => return,
            Some(prev) => {
                let prev = prev.clone();
                self.drop_git_status_subscription(&prev, conn);
            }
            None => {}
        }
        self.git_status_repo_by_conn.insert(conn, repo.clone());
        self.git_status_subscribers
            .entry(repo.clone())
            .or_default()
            .insert(conn);
    }

    /// Unsubscribe `conn` from its current repo (navigation out of git, or
    /// disconnect). A connection is in at most one repo, so this single
    /// method also serves as the disconnect sweep — see `deregister_connection`.
    #[cfg(feature = "local_fs")]
    fn unsubscribe_git_status(&mut self, conn: ConnectionId) {
        if let Some(repo) = self.git_status_repo_by_conn.remove(&conn) {
            self.drop_git_status_subscription(&repo, conn);
        }
    }

    /// Removes one `(repo, conn)` subscription, evicting the per-repo
    /// git-status and GitHub-info model caches once the repo has no
    /// subscribers left.
    #[cfg(feature = "local_fs")]
    fn drop_git_status_subscription(&mut self, repo: &StandardizedPath, conn: ConnectionId) {
        let Some(subscribers) = self.git_status_subscribers.get_mut(repo) else {
            return;
        };
        subscribers.remove(&conn);
        if subscribers.is_empty() {
            self.git_status_subscribers.remove(repo);
            self.github_repo_models.remove(repo);
            self.git_status_models.remove(repo);
        }
    }

    fn handle_run_command(
        &mut self,
        req: RunCommandRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let session_id = SessionId::from(req.session_id);
        log::info!(
            "Handling RunCommand (request_id={request_id}, session_id={session_id:?}): \
             command={:?}, cwd={:?}",
            req.command,
            req.working_directory,
        );

        let command = req.command;
        let cwd = req.working_directory;
        let env_vars = if req.environment_variables.is_empty() {
            None
        } else {
            Some(req.environment_variables)
        };

        let Some(executor) = self.executors.get(&session_id).cloned() else {
            log::error!("No executor for session {session_id:?}, session was never initialized");
            return HandlerOutcome::Sync(server_message::Message::RunCommandResponse(
                RunCommandResponse {
                    result: Some(run_command_response::Result::Error(RunCommandError {
                        code: RunCommandErrorCode::SessionNotFound.into(),
                        message: format!("No executor for session {session_id:?}"),
                    })),
                },
            ));
        };

        // Call `execute_local_command` directly because the
        // `CommandExecutor::execute_command` trait method requires
        // a `&Shell` (version, options, plugins from bootstrap).
        let request_id_for_response = request_id.clone();
        let conn_id_for_response = conn_id;
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                executor
                    .execute_local_command(
                        &command,
                        cwd.as_deref(),
                        env_vars,
                        ExecuteCommandOptions::default(),
                    )
                    .await
            },
            move |me, result, _ctx| {
                let result_oneof = match result {
                    Ok(output) => {
                        log::info!(
                            "RunCommand completed (request_id={request_id_for_response}): \
                             exit_code={:?}, stdout_len={}, stderr_len={}",
                            output.exit_code,
                            output.stdout.len(),
                            output.stderr.len(),
                        );
                        run_command_response::Result::Success(RunCommandSuccess {
                            stdout: output.stdout.clone(),
                            stderr: output.stderr.clone(),
                            exit_code: output.exit_code.map(|c| c.value()),
                        })
                    }
                    Err(e) => {
                        log::warn!("RunCommand failed (request_id={request_id_for_response}): {e}");
                        run_command_response::Result::Error(RunCommandError {
                            code: RunCommandErrorCode::ExecutionFailed.into(),
                            message: format!("Failed to execute command: {e}"),
                        })
                    }
                };
                me.send_server_message(
                    Some(conn_id_for_response),
                    Some(&request_id_for_response),
                    server_message::Message::RunCommandResponse(RunCommandResponse {
                        result: Some(result_oneof),
                    }),
                );
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `NavigatedToDirectory` by running git detection first, then
    /// responding. On validation failure returns a `HandlerOutcome::Sync` error;
    /// otherwise spawns a task and returns a `HandlerOutcome::Async(Some(_))`
    /// handle.
    fn handle_navigated_to_directory(
        &mut self,
        msg: NavigatedToDirectory,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling NavigatedToDirectory path={} (request_id={request_id})",
            msg.path
        );

        let std_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.path)) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Invalid path for NavigatedToDirectory: {e}");
                return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                    code: ErrorCode::InvalidRequest.into(),
                    message: format!("Invalid path: {e}"),
                }));
            }
        };

        // Kick off git detection. The returned future resolves with the git
        // root path (Some) or None if no git repo was found.
        let path_str = msg.path.clone();
        let git_future = DetectedRepositories::handle(ctx).update(ctx, |repos, ctx| {
            repos.detect_possible_git_repo(&path_str, RepoDetectionSource::TerminalNavigation, ctx)
        });

        let request_id_for_response = request_id.clone();
        let conn_id_for_response = conn_id;
        let handle = self.spawn_request_handler(
            request_id.clone(),
            git_future,
            move |me, git_root, ctx| {
                let (indexed_path, is_git) = if let Some(root) = git_root {
                    // Git repo found. Full indexing was already triggered by
                    // DetectedGitRepo → LocalRepoMetadataModel. The client
                    // waits for RepositoryIndexedPush before FetchFileTree.
                    let root_str = root.to_string_lossy().to_string();
                    log::info!("Git repo detected at {root_str} for path {}", std_path);
                    (root_str, true)
                } else {
                    // No git repo. Lazy-load the directory for first-level data,
                    // then push the snapshot immediately.
                    RepoMetadataModel::handle(ctx).update(ctx, |repo_model, ctx| {
                        if let Err(e) = repo_model.index_lazy_loaded_path(&std_path, ctx) {
                            log::warn!("Failed to lazy-load directory {std_path}: {e}");
                        }
                    });
                    (std_path.to_string(), false)
                };

                me.send_server_message(
                    Some(conn_id_for_response),
                    Some(&request_id_for_response),
                    server_message::Message::NavigatedToDirectoryResponse(
                        NavigatedToDirectoryResponse {
                            indexed_path: indexed_path.clone(),
                            is_git,
                        },
                    ),
                );

                // After responding, push a snapshot if metadata is available.
                //
                // For git repos this is an opportunistic push for the case
                // where the repo was already indexed and RepositoryUpdated
                // won't fire again (which would otherwise leave the client
                // with only a placeholder root). We skip if a snapshot was
                // already sent for this connection+root.
                //
                // For non-git directories the lazy-loaded tree is always
                // broadcast to all connections.
                if let Ok(root_path) =
                    StandardizedPath::from_local_canonicalized(Path::new(&indexed_path))
                {
                    if is_git {
                        let already_sent = me
                            .snapshot_sent_roots_by_connection
                            .get(&conn_id_for_response)
                            .is_some_and(|roots| roots.contains(&root_path));
                        if already_sent {
                            log::debug!(
                                "Snapshot already sent for repo {indexed_path} \
                                 to conn {conn_id_for_response}, skipping"
                            );
                            return;
                        }
                    }

                    let id = RepositoryIdentifier::local(root_path.clone());
                    let repo_model = RepoMetadataModel::handle(ctx);
                    if let Some(state) = repo_model.as_ref(ctx).get_repository(&id, ctx) {
                        let entries = super::repo_metadata_proto::file_tree_entry_to_snapshot_proto(
                            &state.entry,
                        );
                        let standing_results: Option<super::proto::StandingQueryResultsDelta> =
                            repo_model
                                .as_ref(ctx)
                                .standing_query_results(&root_path, ctx)
                                .map(|results| (&results.as_snapshot_delta()).into());
                        // Git snapshots target the requesting connection;
                        // non-git snapshots broadcast to all.
                        let target = if is_git {
                            Some(conn_id_for_response)
                        } else {
                            None
                        };
                        me.send_server_message(
                            target,
                            None,
                            server_message::Message::RepoMetadataSnapshot(
                                super::proto::RepoMetadataSnapshot {
                                    repo_path: indexed_path,
                                    entries,
                                    sync_complete: true,
                                    standing_results,
                                },
                            ),
                        );
                        if is_git {
                            if let Some(sent_roots) = me
                                .snapshot_sent_roots_by_connection
                                .get_mut(&conn_id_for_response)
                            {
                                sent_roots.insert(root_path);
                            }
                        }
                    }
                }
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `LoadRepoMetadataDirectory` by loading a subdirectory on the
    /// server's local model and returning the children synchronously.
    fn handle_load_repo_metadata_directory(
        &mut self,
        msg: super::proto::LoadRepoMetadataDirectory,
        request_id: &RequestId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling LoadRepoMetadataDirectory repo_path={} dir_path={} (request_id={request_id})",
            msg.repo_path,
            msg.dir_path
        );

        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => p,
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                    code: ErrorCode::InvalidRequest.into(),
                    message: format!("Invalid repo_path: {e}"),
                }));
            }
        };

        let dir_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.dir_path)) {
            Ok(p) => p,
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                    code: ErrorCode::InvalidRequest.into(),
                    message: format!("Invalid dir_path: {e}"),
                }));
            }
        };

        // Validate that the directory is a descendant of the repo.
        if !dir_path.starts_with(&repo_path) {
            return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                code: ErrorCode::InvalidRequest.into(),
                message: format!(
                    "dir_path {dir_path} is not a descendant of repo_path {repo_path}"
                ),
            }));
        }

        // Load the directory on the server's local model.
        let load_result = RepoMetadataModel::handle(ctx).update(ctx, |model, ctx| {
            model.load_directory(&repo_path, &dir_path, ctx)
        });

        if let Err(e) = load_result {
            log::warn!("LoadRepoMetadataDirectory failed: {e}");
            return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                code: ErrorCode::Internal.into(),
                message: format!("Failed to load directory: {e}"),
            }));
        }

        // Read back the loaded children and serialize them.
        let id = RepositoryIdentifier::local(repo_path.clone());
        let entries = RepoMetadataModel::handle(ctx)
            .as_ref(ctx)
            .get_repository(&id, ctx)
            .map(|state| {
                super::repo_metadata_proto::file_tree_children_to_proto_entries(
                    &state.entry,
                    &dir_path,
                )
            })
            .unwrap_or_default();

        HandlerOutcome::Sync(server_message::Message::LoadRepoMetadataDirectoryResponse(
            super::proto::LoadRepoMetadataDirectoryResponse {
                repo_path: msg.repo_path,
                dir_path: msg.dir_path,
                entries,
            },
        ))
    }

    /// Handles `WriteFile` by registering the path and triggering an async
    /// write via `FileModel`. On a successful dispatch, returns
    /// `HandlerOutcome::Async(None)` — the response is sent later by the
    /// `FileModel` event subscription, and the op is not cancellable via
    /// `Abort`. On failure to dispatch, returns a `HandlerOutcome::Sync`
    /// error response.
    fn handle_write_file(
        &mut self,
        msg: WriteFile,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling WriteFile path={} (request_id={request_id})",
            msg.path
        );
        let path = Path::new(&msg.path);

        let (file_id, version) =
            self.pending_file_ops
                .insert(path, request_id.clone(), conn_id, FileOpKind::Write, ctx);

        let file_model = FileModel::handle(ctx);
        if let Err(err) =
            file_model.update(ctx, |m, ctx| m.save(file_id, msg.content, version, ctx))
        {
            self.pending_file_ops.remove(file_id, ctx);
            return HandlerOutcome::Sync(server_message::Message::WriteFileResponse(
                WriteFileResponse {
                    result: Some(write_file_response::Result::Error(FileOperationError {
                        message: format!("Failed to initiate write: {err}"),
                    })),
                },
            ));
        }

        // Response sent asynchronously via the event subscription.
        HandlerOutcome::Async(None)
    }

    /// Handles `DeleteFile` by registering the path and triggering an async
    /// delete via `FileModel`. On a successful dispatch, returns
    /// `HandlerOutcome::Async(None)` — the response is sent later by the
    /// `FileModel` event subscription, and the op is not cancellable via
    /// `Abort`. On failure to dispatch, returns a `HandlerOutcome::Sync`
    /// error response.
    fn handle_delete_file(
        &mut self,
        msg: DeleteFile,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling DeleteFile path={} (request_id={request_id})",
            msg.path
        );
        let path = Path::new(&msg.path);

        let (file_id, version) = self.pending_file_ops.insert(
            path,
            request_id.clone(),
            conn_id,
            FileOpKind::Delete,
            ctx,
        );

        let file_model = FileModel::handle(ctx);
        if let Err(err) = file_model.update(ctx, |m, ctx| m.delete(file_id, version, ctx)) {
            self.pending_file_ops.remove(file_id, ctx);
            return HandlerOutcome::Sync(server_message::Message::DeleteFileResponse(
                DeleteFileResponse {
                    result: Some(delete_file_response::Result::Error(FileOperationError {
                        message: format!("Failed to initiate delete: {err}"),
                    })),
                },
            ));
        }

        // Response sent asynchronously via the event subscription.
        HandlerOutcome::Async(None)
    }

    /// Handles `ReadFileContext` by spawning an async batch file read on the
    /// background executor. Returns `HandlerOutcome::Async` with the spawned
    /// handle so the request can be cancelled via `Abort`.
    fn handle_read_file_context(
        &mut self,
        msg: super::proto::ReadFileContextRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling ReadFileContext ({} files, request_id={request_id})",
            msg.files.len()
        );

        let max_file_bytes = msg.max_file_bytes.map(|b| b as usize);
        let max_batch_bytes = msg.max_batch_bytes.map(|b| b as usize);
        let file_locations: Vec<FileLocations> = msg
            .files
            .into_iter()
            .map(|f| FileLocations {
                name: f.path,
                lines: f
                    .line_ranges
                    .into_iter()
                    .map(|r| r.start as usize..r.end as usize)
                    .collect(),
            })
            .collect();
        let request_id_for_response = request_id.clone();

        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                read_local_file_context(
                    &file_locations,
                    None,
                    None,
                    max_file_bytes,
                    max_batch_bytes,
                )
                .await
            },
            move |me, result: anyhow::Result<ReadFileContextResult>, _ctx| {
                let response = match result {
                    Ok(result) => file_context_result_to_proto(result),
                    Err(err) => ReadFileContextResponse {
                        file_contexts: vec![],
                        failed_files: vec![FailedFileRead {
                            path: String::new(),
                            error: Some(FileOperationError {
                                message: format!("{err:#}"),
                            }),
                        }],
                    },
                };
                me.send_server_message(
                    Some(conn_id),
                    Some(&request_id_for_response),
                    server_message::Message::ReadFileContextResponse(response),
                );
            },
            ctx,
        );

        HandlerOutcome::Async(Some(handle))
    }

    /// Handles `OpenBuffer` by opening the file via `GlobalBufferModel`.
    /// The response is sent asynchronously when `BufferLoaded` fires.
    #[cfg(feature = "local_fs")]
    fn handle_open_buffer(
        &mut self,
        msg: OpenBuffer,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling OpenBuffer path={path} (request_id={request_id})",
            path = msg.path
        );

        let path = PathBuf::from(&msg.path);
        let gbm = GlobalBufferModel::handle(ctx);
        let buffer_state = gbm.update(ctx, |gbm, ctx| gbm.open_server_local(path, ctx));
        let file_id = buffer_state.file_id;

        // Track path → FileId mapping and connection. track_open_buffer also
        // holds a strong reference to the buffer — the daemon has no editor
        // view, so without holding it the buffer would be reclaimed before
        // FileModel's async load completes (see
        // ServerBufferTracker::buffer_handles).
        self.buffers
            .track_open_buffer(msg.path.clone(), file_id, buffer_state.buffer);
        self.buffers.add_connection(file_id, conn_id);

        // If already loaded, respond immediately.
        if gbm.as_ref(ctx).buffer_loaded(file_id) {
            let content = gbm
                .as_ref(ctx)
                .content_for_file(file_id, ctx)
                .unwrap_or_default();
            let server_version = gbm
                .as_ref(ctx)
                .sync_clock_for_server_local(file_id)
                .map(|c| c.server_version.as_u64())
                .unwrap_or(1);
            return HandlerOutcome::Sync(server_message::Message::OpenBufferResponse(
                OpenBufferResponse {
                    content,
                    server_version,
                },
            ));
        }

        // Not yet loaded — stash request info so the GlobalBufferModelEvent
        // subscription can send the response when content arrives.
        self.buffers.insert_pending(
            file_id,
            request_id.clone(),
            conn_id,
            PendingBufferRequestKind::OpenBuffer,
        );
        HandlerOutcome::Async(None)
    }

    /// Handles `BufferEdit` notification (fire-and-forget).
    /// Delegates to `GlobalBufferModel::apply_client_edit`. On rejection
    /// (stale server version), the edit is silently dropped.
    #[cfg(feature = "local_fs")]
    fn handle_buffer_edit(&mut self, msg: BufferEdit, ctx: &mut ModelContext<Self>) {
        let Some(file_id) = self.buffers.file_id_for_path(&msg.path) else {
            log::warn!("BufferEdit for unknown buffer: {path}", path = msg.path);
            return;
        };

        let expected_sv = ContentVersion::from_wire_u64(msg.expected_server_version);
        let new_cv = ContentVersion::from_wire_u64(msg.new_client_version);

        // Per spec: if the edit is rejected (stale server version),
        // the server silently drops it.
        GlobalBufferModel::handle(ctx).update(ctx, |gbm, ctx| {
            gbm.apply_client_edit(file_id, &msg.edits, expected_sv, new_cv, ctx);
        });
    }

    /// Handles `SaveBuffer` by persisting the buffer to disk.
    #[cfg(feature = "local_fs")]
    fn handle_save_buffer(
        &mut self,
        msg: SaveBuffer,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling SaveBuffer path={path} (request_id={request_id})",
            path = msg.path
        );

        let Some(file_id) = self.buffers.file_id_for_path(&msg.path) else {
            return HandlerOutcome::Sync(server_message::Message::SaveBufferResponse(
                SaveBufferResponse {
                    result: Some(save_buffer_response::Result::Error(FileOperationError {
                        message: format!("Buffer not open: {path}", path = msg.path),
                    })),
                },
            ));
        };

        let result = GlobalBufferModel::handle(ctx)
            .update(ctx, |gbm, ctx| gbm.save_server_local(file_id, ctx));

        match result {
            Ok(()) => {
                // Response will come via the FileSaved event subscription.
                // Track the file_id → (request_id, conn_id) so the event
                // handler can correlate.
                self.buffers.insert_pending(
                    file_id,
                    request_id.clone(),
                    conn_id,
                    PendingBufferRequestKind::SaveBuffer,
                );
                HandlerOutcome::Async(None)
            }
            Err(err) => HandlerOutcome::Sync(server_message::Message::SaveBufferResponse(
                SaveBufferResponse {
                    result: Some(save_buffer_response::Result::Error(FileOperationError {
                        message: format!("Failed to save: {err}"),
                    })),
                },
            )),
        }
    }

    /// Handles `ResolveConflict` by replacing the server buffer with the
    /// client's content and persisting to disk. Returns an async
    /// `HandlerOutcome` — the response is sent when `FileSaved` or
    /// `FailedToSave` fires.
    #[cfg(feature = "local_fs")]
    fn handle_resolve_conflict(
        &mut self,
        msg: ResolveConflict,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling ResolveConflict path={path} (request_id={request_id})",
            path = msg.path
        );

        let Some(file_id) = self.buffers.file_id_for_path(&msg.path) else {
            return HandlerOutcome::Sync(server_message::Message::ResolveConflictResponse(
                ResolveConflictResponse {
                    result: Some(resolve_conflict_response::Result::Error(
                        FileOperationError {
                            message: format!("Buffer not open: {path}", path = msg.path),
                        },
                    )),
                },
            ));
        };

        let ack_sv = ContentVersion::from_wire_u64(msg.acknowledged_server_version);
        let current_cv = ContentVersion::from_wire_u64(msg.current_client_version);
        let result = GlobalBufferModel::handle(ctx).update(ctx, |gbm, ctx| {
            gbm.resolve_conflict(file_id, ack_sv, current_cv, &msg.client_content, ctx)
        });

        match result {
            Ok(()) => {
                self.buffers.insert_pending(
                    file_id,
                    request_id.clone(),
                    conn_id,
                    PendingBufferRequestKind::ResolveConflict,
                );
                HandlerOutcome::Async(None)
            }
            Err(err) => HandlerOutcome::Sync(server_message::Message::ResolveConflictResponse(
                ResolveConflictResponse {
                    result: Some(resolve_conflict_response::Result::Error(
                        FileOperationError {
                            message: format!("Failed to resolve conflict: {err}"),
                        },
                    )),
                },
            )),
        }
    }

    /// Zap: handles `ListDirectory` — synchronously lists the immediate
    /// children of a directory.
    ///
    /// Used for precise validation by remote terminal file-link detection:
    /// the client caches the real directory entries for a given cwd, and the
    /// link detector uses them to cut out the correct filename from a full
    /// `ls -l` line. `std::fs::read_dir` is a cheap synchronous call on the
    /// daemon side, so this returns `HandlerOutcome::Sync` directly instead
    /// of an async spawn.
    #[cfg(feature = "local_fs")]
    fn handle_list_directory(&self, msg: ListDirectory) -> HandlerOutcome {
        log::info!("Handling ListDirectory path={}", msg.path);

        let path = expand_user_path(&msg.path);
        let result = match std::fs::read_dir(&path) {
            Ok(read_dir) => {
                let mut entries = Vec::new();
                for entry in read_dir.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    // Prefer `file_type()` (doesn't follow symlinks, no extra
                    // stat needed); fall back to `metadata()` (which does
                    // follow symlinks) on failure.
                    let file_type = entry.file_type().ok();
                    let metadata = entry.metadata().ok();
                    let kind = entry_kind(file_type.as_ref(), metadata.as_ref());
                    let is_dir = kind == FileSystemEntryKind::Directory as i32;
                    let size_bytes = metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len());
                    let modified_epoch_millis = metadata
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(system_time_to_epoch_millis);
                    entries.push(DirEntry {
                        name,
                        is_dir,
                        kind,
                        size_bytes,
                        modified_epoch_millis,
                    });
                }
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                let canonical_path = path
                    .canonicalize()
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();
                list_directory_response::Result::Success(ListDirectorySuccess {
                    entries,
                    canonical_path,
                })
            }
            Err(err) => list_directory_response::Result::Error(FileOperationError {
                message: format!("Failed to list directory {}: {err}", msg.path),
            }),
        };

        HandlerOutcome::Sync(server_message::Message::ListDirectoryResponse(
            ListDirectoryResponse {
                result: Some(result),
            },
        ))
    }

    #[cfg(feature = "local_fs")]
    fn handle_resolve_path(&self, msg: ResolvePath) -> HandlerOutcome {
        let path = expand_user_path(&msg.path);
        let result = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                let kind = entry_kind(Some(&file_type), Some(&metadata));
                let canonical_path = path
                    .canonicalize()
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();
                resolve_path_response::Result::Success(ResolvePathSuccess {
                    canonical_path,
                    kind,
                    size_bytes: metadata.is_file().then_some(metadata.len()),
                })
            }
            Err(err) => resolve_path_response::Result::Error(FileOperationError {
                message: format!("Failed to resolve path {}: {err}", msg.path),
            }),
        };

        HandlerOutcome::Sync(server_message::Message::ResolvePathResponse(
            ResolvePathResponse {
                result: Some(result),
            },
        ))
    }

    #[cfg(feature = "local_fs")]
    fn handle_create_directory(&self, msg: CreateDirectory) -> HandlerOutcome {
        let path = expand_user_path(&msg.path);
        let result = match std::fs::create_dir_all(&path) {
            Ok(()) => {
                let canonical_path = path
                    .canonicalize()
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();
                create_directory_response::Result::Success(CreateDirectorySuccess {
                    canonical_path,
                })
            }
            Err(err) => create_directory_response::Result::Error(FileOperationError {
                message: format!("Failed to create directory {}: {err}", msg.path),
            }),
        };

        HandlerOutcome::Sync(server_message::Message::CreateDirectoryResponse(
            CreateDirectoryResponse {
                result: Some(result),
            },
        ))
    }

    #[cfg(feature = "local_fs")]
    fn handle_read_file_chunk(&self, msg: ReadFileChunk) -> HandlerOutcome {
        use std::io::{Read, Seek, SeekFrom};

        let path = expand_user_path(&msg.path);
        let result = (|| -> std::io::Result<ReadFileChunkSuccess> {
            let mut file = std::fs::File::open(&path)?;
            let total_size = file.metadata().ok().map(|m| m.len());
            file.seek(SeekFrom::Start(msg.offset))?;
            let max_bytes = msg.max_bytes.min(8 * 1024 * 1024) as usize;
            let mut bytes = vec![0; max_bytes];
            let read = file.read(&mut bytes)?;
            bytes.truncate(read);
            let next_offset = msg.offset + read as u64;
            let eof = total_size.is_some_and(|size| next_offset >= size) || read == 0;
            Ok(ReadFileChunkSuccess {
                bytes,
                next_offset,
                total_size,
                eof,
            })
        })();

        let result = match result {
            Ok(success) => read_file_chunk_response::Result::Success(success),
            Err(err) => read_file_chunk_response::Result::Error(FileOperationError {
                message: format!("Failed to read file chunk {}: {err}", msg.path),
            }),
        };

        HandlerOutcome::Sync(server_message::Message::ReadFileChunkResponse(
            ReadFileChunkResponse {
                result: Some(result),
            },
        ))
    }

    #[cfg(feature = "local_fs")]
    fn handle_write_file_chunk(&self, msg: WriteFileChunk) -> HandlerOutcome {
        use std::io::{Seek, SeekFrom, Write};

        let path = expand_user_path(&msg.path);
        let result = (|| -> std::io::Result<WriteFileChunkSuccess> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut options = std::fs::OpenOptions::new();
            options.create(true).write(true);
            if msg.truncate {
                options.truncate(true);
            }
            let mut file = options.open(&path)?;
            file.seek(SeekFrom::Start(msg.offset))?;
            file.write_all(&msg.bytes)?;
            #[cfg(unix)]
            if let Some(executable) = msg.executable {
                use std::os::unix::fs::PermissionsExt;

                let mode = if executable { 0o755 } else { 0o644 };
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
            }
            Ok(WriteFileChunkSuccess {
                next_offset: msg.offset + msg.bytes.len() as u64,
            })
        })();

        let result = match result {
            Ok(success) => write_file_chunk_response::Result::Success(success),
            Err(err) => write_file_chunk_response::Result::Error(FileOperationError {
                message: format!("Failed to write file chunk {}: {err}", msg.path),
            }),
        };

        HandlerOutcome::Sync(server_message::Message::WriteFileChunkResponse(
            WriteFileChunkResponse {
                result: Some(result),
            },
        ))
    }

    /// Handles `CloseBuffer` notification (fire-and-forget).
    /// Removes the connection from the buffer's connection set.
    /// Deallocates the buffer if no connections remain.
    #[cfg(feature = "local_fs")]
    fn handle_close_buffer(
        &mut self,
        msg: CloseBuffer,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) {
        log::info!(
            "Handling CloseBuffer path={path} conn={conn_id}",
            path = msg.path
        );
        self.buffers.close_buffer(&msg.path, conn_id, ctx);
    }
}

/// Daemon-side execution-time guard for the git write-op handlers, mirroring
/// Warp (`warp/master:app/src/remote_server/server_model.rs`). The local dialog
/// guards pre-emptively via its blocked-state check before opening; the remote
/// dialog cannot, because that check probes the *client's* filesystem and the
/// repository lives on the daemon's. This is therefore the only guard on the
/// remote path, and `RemoteDiffStateModel::is_git_operation_blocked` returns
/// `false` on the promise that the daemon owns it.
///
/// The shared `util::git` orchestration itself stays guard-free, so each
/// mutating handler applies this at the head of its spawned future.
#[cfg(feature = "local_fs")]
fn guard_git_operation_in_progress(repo_path: &Path) -> anyhow::Result<()> {
    if crate::util::git::git_operation_in_progress(repo_path) {
        anyhow::bail!(
            "another git operation is in progress (merge, rebase, cherry-pick, or a lock file is present)"
        );
    }
    Ok(())
}

#[cfg(feature = "local_fs")]
fn expand_user_path(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

#[cfg(feature = "local_fs")]
fn entry_kind(file_type: Option<&std::fs::FileType>, metadata: Option<&std::fs::Metadata>) -> i32 {
    if file_type.is_some_and(|ft| ft.is_symlink()) {
        return FileSystemEntryKind::Symlink as i32;
    }
    if metadata.is_some_and(|metadata| metadata.is_dir()) {
        return FileSystemEntryKind::Directory as i32;
    }
    if metadata.is_some_and(|metadata| metadata.is_file()) {
        return FileSystemEntryKind::File as i32;
    }
    FileSystemEntryKind::Other as i32
}

#[cfg(feature = "local_fs")]
fn system_time_to_epoch_millis(time: std::time::SystemTime) -> Option<u64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

/// Converts a [`ReadFileContextResult`] into its protobuf equivalent.
fn file_context_result_to_proto(result: ReadFileContextResult) -> ReadFileContextResponse {
    use crate::ai::agent::AnyFileContent;

    let file_contexts = result
        .file_contexts
        .into_iter()
        .map(|fc| {
            let content = match fc.content {
                AnyFileContent::StringContent(text) => {
                    super::proto::file_context_proto::Content::TextContent(text)
                }
                AnyFileContent::BinaryContent(bytes) => {
                    super::proto::file_context_proto::Content::BinaryContent(bytes)
                }
            };
            let last_modified_epoch_millis = fc
                .last_modified
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            FileContextProto {
                file_name: fc.file_name,
                content: Some(content),
                line_range_start: fc.line_range.as_ref().map(|r| r.start as u32),
                line_range_end: fc.line_range.as_ref().map(|r| r.end as u32),
                last_modified_epoch_millis,
                line_count: fc.line_count as u32,
            }
        })
        .collect();

    // Carry the per-file reason through instead of flattening every failure into
    // one generic string — that flattening is the defect #369 was filed for.
    let failed_files = result
        .failed_files
        .into_iter()
        .map(|failed_file| FailedFileRead {
            path: failed_file.path,
            error: Some(FileOperationError {
                message: failed_file.message,
            }),
        })
        .collect();

    ReadFileContextResponse {
        file_contexts,
        failed_files,
    }
}

#[cfg(test)]
#[path = "server_model_tests.rs"]
mod tests;
