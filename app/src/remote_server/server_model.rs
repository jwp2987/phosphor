use crate::terminal::shell::ShellType;
use repo_metadata::repositories::{DetectedRepositories, RepoDetectionSource};
use repo_metadata::{
    RepoMetadataEvent, RepoMetadataModel, RepositoryIdentifier, RepositoryWatchMode,
};
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
    git_create_pr_response, git_pull_response, git_push_response, git_stage_response,
    host_scoped_request, notification, run_command_response, server_message,
    session_scoped_request, write_file_response, Abort, Authenticate, ClientMessage, DeleteFile,
    DeleteFileResponse,
    DeleteFileSuccess, DiscardFilesError, DiscardFilesRequest, DiscardFilesResponse,
    DiscardFilesSuccess, ErrorCode, ErrorResponse, FailedFileRead, FileContextProto,
    FileOperationError, GetBranches, GetCommittedBranchFilesRequest, GetDiffState,
    GitCommitChainMode, GitCommitChainRequest, GitCommitChainResponse, GitCommitChainSuccess,
    GitCreatePrRequest, GitCreatePrResponse, GitOpDelta, GitOpError, GitPullRequest,
    GitPullResponse, GitPushRequest, GitPushResponse, GitStageError, GitStageRequest,
    GitStageResponse, GitStageSuccess, HostScopedRequest, Initialize,
    InitializeResponse, NavigatedToDirectory, NavigatedToDirectoryResponse, Notification,
    ReadFileContextResponse, RipgrepSearchRequest, RunCommandError, RunCommandErrorCode,
    RunCommandRequest, RunCommandResponse, RunCommandSuccess, ServerMessage, SessionBootstrapped,
    SessionScopedRequest, UnsubscribeDiffState, WriteFile, WriteFileResponse, WriteFileSuccess,
};

// Remote codebase indexing (Delta D2, remote-daemon leg). Gated `local_fs`
// because the daemon's index manager is: it walks the host's filesystem and
// stores its snapshots and vectors there.
#[cfg(feature = "local_fs")]
use super::codebase_index_status::{
    codebase_index_status_to_proto, disabled_codebase_index_status,
    not_enabled_codebase_index_status, queued_codebase_index_status,
    unavailable_codebase_index_status,
};
#[cfg(feature = "local_fs")]
use super::codebase_index_store::daemon_store_client;
// Remote git-chip / PR-context (issue: remote git status + PR context). Gated
// `local_fs` because every producer behind them runs `git` / `gh` against the
// host's own filesystem.
#[cfg(feature = "local_fs")]
use super::git_status_proto::{
    git_status_metadata_to_proto, repository_info_to_proto,
};
#[cfg(feature = "local_fs")]
use super::proto::{
    GitHubPrInfoPush, GitHubRepositoryInfoPush, GitStatusPush, UpdateGitHubPrInfo,
    UpdateGitHubRepoInfo, UpdateGitStatus,
};
#[cfg(feature = "local_fs")]
use crate::code_review::github_repo_model::GitHubRepoEvent;
#[cfg(feature = "local_fs")]
use crate::remote_server::diff_state_proto::pr_info_to_proto;
#[cfg(feature = "local_fs")]
use warp_util::local_or_remote_path::LocalOrRemotePath;
#[cfg(feature = "local_fs")]
use super::proto::{
    CodebaseIndexLimits, CodebaseIndexStatus, CodebaseIndexStatusUpdated,
    CodebaseIndexStatusesSnapshot, CodebaseResyncMode, DropCodebaseIndex,
    FragmentMetadata as ProtoFragmentMetadata,
    FragmentMetadataLookupError as ProtoFragmentMetadataLookupError,
    FragmentMetadataLookupErrorCode, GetFragmentMetadataFromHash,
    GetFragmentMetadataFromHashResponse, GetFragmentMetadataFromHashSuccess, IndexCodebase,
    MissingFragmentMetadata, RemoteCodebaseSearchError, RemoteCodebaseSearchErrorCode,
    ResyncCodebase, SearchRemoteCodebase, SearchRemoteCodebaseResponse,
    SearchRemoteCodebaseSuccess, UpdatePreferences, get_fragment_metadata_from_hash_response,
    search_remote_codebase_response,
};
#[cfg(feature = "local_fs")]
use crate::ai::agent_providers::embeddings::EmbeddingEndpoint;
#[cfg(feature = "local_fs")]
use ::ai::index::full_source_code_embedding::manager::{
    CodebaseIndexManager, CodebaseIndexManagerEvent,
    FragmentMetadataLookupError as LocalFragmentMetadataLookupError, RetrieveFileError,
};
#[cfg(feature = "local_fs")]
use ::ai::index::full_source_code_embedding::{
    ContentHash, EmbeddingConfig, FragmentMetadata as LocalFragmentMetadata, NodeHash, RetrievalID,
};
#[cfg(feature = "local_fs")]
use warp_core::features::FeatureFlag;

// Remote Agent Mode context snapshot (#438 dependent feature 1, #353 producer): depends
// on `SkillManager`'s real (non-dummy) API, gated `local_fs` like the buffer-sync imports
// below.
#[cfg(feature = "local_fs")]
use super::proto::{
    remote_skill_proto, HomeSkillMetadata, RemoteAgentContextSnapshot, RemoteContextFileProto,
    RemoteSkillProto,
};
#[cfg(feature = "local_fs")]
use crate::ai::skills::{bundled_skill_snapshot_protos, BundledSkill, SkillManager, SkillManagerEvent};
// Daemon-side producer for `RemoteAgentContextSnapshot.global_rules` (#575):
// this host's own file-based global rules (e.g. `~/.agents/AGENTS.md`),
// indexed by the same `ProjectContextModel` singleton `app/src/remote_server/
// mod.rs::run_daemon_app` registers for exactly this purpose.
#[cfg(feature = "local_fs")]
use ai::project_context::model::{ProjectContextModel, ProjectContextModelEvent};

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
use crate::code_review::git_status_update::{GitRepoModels, GitRepoStatusModel};
#[cfg(feature = "local_fs")]
use crate::code_review::github_repo_model::GitHubRepoModel;
#[cfg(feature = "local_fs")]
use warpui::ModelHandle;

/// How long the daemon waits with no connections before exiting.
pub const GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Unique identifier for a connected proxy session in daemon mode.
pub type ConnectionId = uuid::Uuid;
use super::protocol::{MAX_MESSAGE_SIZE, RequestId};
use crate::ai::agent::FileLocations;
use crate::ai::blocklist::{read_local_file_context, ReadFileContextResult};
use crate::terminal::model::session::command_executor::{
    ExecuteCommandOptions, LocalCommandExecutor,
};

/// Outcome of dispatching a request-style `ClientMessage`.
/// Notifications (fire-and-forget messages like `SessionBootstrapped` and
/// `Abort`) do not produce a `HandlerOutcome`; they are dispatched inline in
/// `handle_message` and return early.
enum HandlerOutcome {
    /// The response is ready synchronously — the caller sends it immediately.
    Sync(server_message::Message),
    /// The handler initiated async work whose response will be sent later.
    /// When the handle is `Some`, the caller inserts it into `in_progress`
    /// so the request can be cancelled via `Abort`. Removal on
    /// completion/abort is arranged by [`ServerModel::spawn_request_handler`].
    /// `None` is used for async work whose completion is delivered through
    /// a separate event subscription and is not currently cancellable via
    /// `Abort` (e.g. `FileModel` events for file writes and deletes, which
    /// are tracked by `FileId` in `pending_file_ops` rather than by
    /// `RequestId` in `in_progress`). `handle_get_diff_state` (#324) is a
    /// second, cancellable case: a request that joins an already in-flight
    /// computation for the same `DiffModelKey` returns `None` here too (the
    /// caller must not double-insert the leading request's own handle,
    /// which `handle_get_diff_state` already tracks in `in_progress`
    /// itself), but it stays abortable via `diff_state_pending_responses`
    /// — see `handle_abort`'s `abort_diff_state_pending_response` fallback.
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

/// A `SearchRemoteCodebase` request awaiting its retrieval-completion event.
/// `CodebaseIndexManager::retrieve_relevant_files` only registers the
/// request and returns a `RetrievalID`; the answer arrives later as a
/// `CodebaseIndexManagerEvent::RetrievalRequestCompleted`/
/// `RetrievalRequestFailed`, which carries only the `RetrievalID` — so this
/// is the only place the originating request can be recovered. Mirrors
/// `PendingFileOp` above and `app::ai::codebase_retrieval::PendingRetrieval`,
/// which bridges the same lifecycle for the local (in-process) consumer.
#[cfg(feature = "local_fs")]
struct PendingCodebaseRetrieval {
    request_id: RequestId,
    conn_id: ConnectionId,
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

/// Receives `ClientMessage`s from connected proxy sessions and routes
/// `ServerMessage` responses and push notifications back through each
/// connection's dedicated sender channel.
/// Composite key identifying one server-side diff-state subscription target:
/// a `(repo, mode)` pair. `repo_path` is canonicalized on this host and
/// matched against repository-change events. Ported from the pin's
/// `DiffModelKey` (`app/src/remote_server/diff_state_tracker.rs`, issue #324).
#[cfg(feature = "local_fs")]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DiffModelKey {
    repo_path: StandardizedPath,
    mode: crate::code_review::diff_state::DiffMode,
}

/// A `GetDiffState` request queued because a computation for the same
/// [`DiffModelKey`] was already in flight when it arrived (#324): joins the
/// in-flight computation instead of triggering a redundant one. Resolved
/// together with it on completion, or failed together with it on abort of
/// the leading request — see `resolve_diff_state_pending_responses` and
/// `handle_get_diff_state`.
/// Carries its own `wire_repo_path` rather than the pin's plain
/// `(request_id, conn_id)` pair, because this fork echoes each connection's
/// exact `repo_path` string back in responses (see the field doc on
/// `diff_state_subscribers`) — capturing it here at request time means
/// resolution never has to re-derive it from subscriber state that may
/// already be gone (e.g. the connection unsubscribed while this request's
/// computation was in flight).
#[cfg(feature = "local_fs")]
struct PendingDiffStateResponse {
    request_id: RequestId,
    conn_id: ConnectionId,
    wire_repo_path: String,
}

/// Resolves the global bundled resources directory populated by the install
/// script (see `remote_server::setup::remote_server_bundled_resources_dir()`),
/// expanding the shell-form `~/` prefix against this process's home directory.
/// This deliberately does not use any macOS app-bundle resource resolution:
/// the global location is version-independent, and a headless remote daemon
/// has no app bundle to resolve against anyway.
/// #440: the Rust side is now wired up, but this fork's
/// `install_remote_server.sh` doesn't yet create or populate
/// `BUNDLED_RESOURCES_DIR_NAME` from the release artifact's `resources/`
/// tree — that is a packaging/release-pipeline change, not a Rust one. Until
/// it lands, this directory never exists on a freshly installed host, so
/// this still returns `None` in practice and the daemon logs why. The rest
/// of the `RemoteAgentContextSnapshot` plumbing (`home_dir`, home skills) is
/// unaffected either way.
#[cfg(feature = "local_fs")]
fn daemon_bundled_resources_dir() -> Option<PathBuf> {
    let dir = remote_server::setup::remote_server_bundled_resources_dir();
    let suffix = dir.strip_prefix("~/")?;
    let dir = dirs::home_dir()?.join(suffix);
    dir.is_dir().then_some(dir)
}

/// Builds a `RemoteAgentContextSnapshot` for this host at `revision`, combining the
/// daemon's own bundled-skill catalog (`bundled_skills`, produced once at startup —
/// see `daemon_bundled_resources_dir`) with its currently-cached home skills, plus
/// this host's file-based global rules (e.g. `~/.agents/AGENTS.md`).
/// `global_rules` is sourced from `ProjectContextModel::global_rules()` (#575):
/// unlike the previous state of this fork, `ProjectContextModel`
/// (`crates/ai/src/project_context/model.rs`) now indexes file-based global rules
/// itself (`crates/ai/src/project_context/global_rules.rs`) via the
/// `index_global_rules` call in `app/src/remote_server/mod.rs::run_daemon_app`. See
/// `app/src/ai/remote_agent_context.rs`'s module doc comment for the client-side
/// consumer that turns this into per-host `remote_global_rules` storage.
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
    let mut global_rules = ProjectContextModel::as_ref(ctx)
        .global_rules()
        .map(|rule| RemoteContextFileProto {
            path: rule.path.to_string_lossy().into_owned(),
            content: rule.content,
        })
        .collect::<Vec<_>>();
    global_rules.sort_by(|a, b| a.path.cmp(&b.path));
    RemoteAgentContextSnapshot {
        revision,
        home_dir,
        skills,
        global_rules,
    }
}

pub struct ServerModel {
    /// Per-connection outbound channels, keyed by `ConnectionId`.
    /// The daemon can serve multiple proxy connections simultaneously — one
    /// per SSH session / Zap tab connecting to this host.  Each entry maps
    /// a connection's `Uuid` to the channel the connection task drains to
    /// write `ServerMessage`s back to its proxy.
    connection_senders: HashMap<ConnectionId, async_channel::Sender<ServerMessage>>,
    /// Per-connection set of repo roots for which we've already sent a
    /// snapshot in this connection's lifetime.
    /// Used to avoid sending duplicate snapshots on repeated
    /// `NavigatedToDirectory` calls while the user `cd`s within the same repo.
    snapshot_sent_roots_by_connection: HashMap<ConnectionId, HashSet<StandardizedPath>>,
    /// Connections subscribed to each diff-state [`DiffModelKey`], each with
    /// the exact `repo_path` string it sent in `GetDiffState` — echoed back
    /// verbatim in pushed snapshots so the client's `RemoteDiffStateModel`
    /// (which matches pushes by that exact string) sees them.
    /// Key -> conns indexing (#324; mirrors `git_status_subscribers`, added
    /// by #330) replaces an earlier conn -> keys design that could not
    /// express "push to every connection watching this repo" without a full
    /// scan, and left every future subscription type free to disagree with
    /// git-status about which direction to index in.
    #[cfg(feature = "local_fs")]
    diff_state_subscribers: HashMap<DiffModelKey, HashMap<ConnectionId, String>>,
    /// Reverse index of `diff_state_subscribers`, so a connection's teardown
    /// clears its subscriptions in O(subscriptions for that connection)
    /// instead of scanning every key. The pin's own
    /// `RemoteDiffStateManager::remove_connection` scans all keys; this
    /// keeps the property the fork's previous conn -> keys design had
    /// instead of regressing it (see the field doc on `diff_state_subscribers`).
    #[cfg(feature = "local_fs")]
    diff_state_keys_by_conn: HashMap<ConnectionId, HashSet<DiffModelKey>>,
    /// Keys with a `GetDiffState` computation currently in flight. Gates
    /// `handle_get_diff_state`: a request for a key already in this set
    /// joins `diff_state_pending_responses` instead of spawning a redundant
    /// recomputation (#324's "pending-response queueing" gap — the fork
    /// previously recomputed once per *request*, even when several arrived
    /// concurrently for the same repo/mode).
    #[cfg(feature = "local_fs")]
    diff_state_in_flight: HashSet<DiffModelKey>,
    /// Queued `GetDiffState` responses waiting for the in-flight computation
    /// for their key to finish. Resolved together when it completes
    /// (`resolve_diff_state_pending_responses`); drained with an error if
    /// the leading request is aborted, since this fork (unlike the pin) has
    /// no persistent per-key model to fall back to for a last-known-good
    /// snapshot. Also the target of `handle_abort`'s
    /// `abort_diff_state_pending_response` fallback, so aborting a *queued*
    /// (not yet in-flight) request removes it without cancelling the shared
    /// computation other subscribers are still waiting on.
    #[cfg(feature = "local_fs")]
    diff_state_pending_responses: HashMap<DiffModelKey, Vec<PendingDiffStateResponse>>,
    /// Per-repo local git-status models tracked on the daemon, keyed by repo
    /// path. Ported from the pin's `git_status_models` field. Populated by
    /// `subscribe_to_git_status_updates` (from `NavigatedToDirectory` and from
    /// an explicit `UpdateGitStatus`), which also wires each model's
    /// `MetadataChanged` event to broadcast a `GitStatusPush`.
    #[cfg(feature = "local_fs")]
    git_status_models: HashMap<StandardizedPath, ModelHandle<GitRepoStatusModel>>,
    /// Per-repo local GitHub-info models tracked on the daemon, keyed by repo
    /// path. Ported from the pin's `github_repo_models` field. Populated by
    /// `subscribe_to_github_info_updates` on an `UpdateGitHubPrInfo` /
    /// `UpdateGitHubRepoInfo` notification; each model's events broadcast a
    /// `GitHubPrInfoPush` / `GitHubRepositoryInfoPush`.
    #[cfg(feature = "local_fs")]
    github_repo_models: HashMap<StandardizedPath, ModelHandle<GitHubRepoModel>>,
    /// Connections subscribed (via navigation) to each repo's git status,
    /// keyed by repo path. A repo's git-status *and* GitHub-info models live
    /// while this set is non-empty and are evicted once the last connection
    /// unsubscribes (navigates away or disconnects). Same key -> conns shape
    /// as `diff_state_subscribers`, but git-status subscription is exclusive
    /// (one repo per connection) rather than a set of `(repo, mode)` keys, so
    /// its reverse index (`git_status_repo_by_conn`) stores a single value
    /// instead of a set.
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
    /// `daemon_bundled_resources_dir`/`bundled_skill_snapshot_protos` (#353). Empty
    /// until the install script actually ships a `bundled_resources/` tree — see
    /// `daemon_bundled_resources_dir`'s doc comment (#440).
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
    /// The token is written by Initialize when the client supplies a
    /// non-empty credential, or by Authenticate during token rotation. It is
    /// intentionally retained across proxy connection teardown and cleared
    /// only by daemon process exit.
    auth_token: Option<String>,
    /// Whether a `CodebaseIndexManager` singleton exists in this process.
    /// `ServerModel` is constructed both by `run_daemon_app` (where the manager
    /// is registered first) and by tests and the integration harness (where it
    /// is not). `CodebaseIndexManager::handle(ctx)` panics on an unregistered
    /// singleton — this fork has already shipped that panic once, for
    /// `GlobalBufferModel` and once for `WarpManagedPathsWatcher`, see the
    /// registration comments in `mod.rs` — so every codebase-index path checks
    /// this flag first. Captured once in `new`, because registration order is
    /// fixed by then and cannot change underneath us.
    #[cfg(feature = "local_fs")]
    codebase_indexing_available: bool,
    /// `SearchRemoteCodebase` requests awaiting their retrieval-completion
    /// event, keyed by the `RetrievalID` the event carries. See
    /// `handle_search_remote_codebase` and
    /// `handle_codebase_index_manager_event`.
    #[cfg(feature = "local_fs")]
    pending_codebase_retrievals: HashMap<RetrievalID, PendingCodebaseRetrieval>,
    /// Live git watches backing granular per-file diff-state pushes (#577),
    /// keyed by repository root — one watch per repo, shared by every
    /// `(repo, mode)` subscription on it, since the filesystem events do not
    /// depend on the mode.
    /// A repo present here is delta-capable: its live updates come from this
    /// watch, and the coarse `RepositoryUpdated` whole-snapshot push is skipped
    /// for it. A repo absent here (no detected/watched `Repository`) keeps the
    /// old whole-snapshot path — coarse updates beat none.
    #[cfg(feature = "local_fs")]
    diff_state_watches: HashMap<StandardizedPath, super::diff_state_tracker::DiffStateWatch>,
    /// Sender cloned into every watch subscriber; the receiver is streamed in
    /// `new` and drives `handle_diff_state_watch_update`.
    #[cfg(feature = "local_fs")]
    diff_state_watch_tx: async_channel::Sender<(
        std::path::PathBuf,
        super::diff_state_tracker::DiffStateWatchUpdate,
    )>,
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
        #[cfg(feature = "local_fs")]
        let (diff_state_watch_tx, diff_state_watch_rx) = async_channel::unbounded();
        let mut model = Self {
            connection_senders: HashMap::new(),
            snapshot_sent_roots_by_connection: HashMap::new(),
            #[cfg(feature = "local_fs")]
            diff_state_subscribers: HashMap::new(),
            #[cfg(feature = "local_fs")]
            diff_state_keys_by_conn: HashMap::new(),
            #[cfg(feature = "local_fs")]
            diff_state_in_flight: HashSet::new(),
            #[cfg(feature = "local_fs")]
            diff_state_pending_responses: HashMap::new(),
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
            #[cfg(feature = "local_fs")]
            codebase_indexing_available: ctx.has_singleton_model::<CodebaseIndexManager>(),
            #[cfg(feature = "local_fs")]
            pending_codebase_retrievals: HashMap::new(),
            #[cfg(feature = "local_fs")]
            diff_state_watches: HashMap::new(),
            #[cfg(feature = "local_fs")]
            diff_state_watch_tx,
        };
        // Drives granular per-file diff-state pushes (#577). The watches that
        // feed this are established lazily, on the first subscription for a repo.
        #[cfg(feature = "local_fs")]
        ctx.spawn_stream_local(
            diff_state_watch_rx,
            |me, (repo_root, update), ctx| {
                me.handle_diff_state_watch_update(repo_root, update, ctx);
            },
            |_, _| {},
        );
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
                                .standing_query_results(&id, ctx)
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
                    // Only for repos with no git watch. A watched repo drives its
                    // own pushes from `handle_diff_state_watch_update`, which can
                    // tell a single-file edit from a repo-wide change and sends a
                    // delta for the former (#577); pushing here as well would mean
                    // recomputing the whole repo alongside every delta, i.e. doing
                    // strictly more work than before the deltas existed.
                    #[cfg(feature = "local_fs")]
                    if !me.diff_state_watches.contains_key(path) {
                        me.push_diff_state_for_repo(path, ctx);
                    }
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
                    // Push incremental edits to all connections that have this buffer
                    // open, except connections with a pending OpenBuffer request --
                    // they get the same content via OpenBufferResponse instead, and
                    // applying both would double-apply the edit.
                    let Some(conns) = me.buffers.connections_for_buffer(file_id) else {
                        return;
                    };
                    let excluded = me.buffers.pending_connections_for_open_buffer(file_id);
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
                        if excluded.contains(&conn_id) {
                            continue;
                        }
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
                GlobalBufferModelEvent::BufferUpdatedFromFileEvent {
                    file_id, success, ..
                } => {
                    // When a file-watcher update couldn't be applied because the
                    // buffer has unsaved client edits, forward the conflict to the
                    // connected clients so they can show a resolution banner.
                    if !success
                        && let Some(conns) = me.buffers.connections_for_buffer(file_id)
                    {
                        // Collect to break the immutable borrow on `me.buffers`
                        // before calling `me.send_server_message(&mut self)`.
                        let conns: Vec<_> = conns.iter().copied().collect();
                        let path = me.buffers.path_for_file_id(*file_id).unwrap_or_default();
                        for conn_id in conns {
                            me.send_server_message(
                                Some(conn_id),
                                None,
                                server_message::Message::BufferConflictDetected(
                                    super::proto::BufferConflictDetected { path: path.clone() },
                                ),
                            );
                        }
                    }
                }
                GlobalBufferModelEvent::RemoteBufferConflict { .. } => {
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
        // Refresh the pushed `RemoteAgentContextSnapshot` whenever the daemon's own
        // file-based global rules change (e.g. `~/.agents/AGENTS.md` is created,
        // edited, or removed on this host). #575.
        #[cfg(feature = "local_fs")]
        {
            let project_context = ProjectContextModel::handle(ctx);
            ctx.subscribe_to_model(&project_context, |me, event, ctx| {
                if matches!(event, ProjectContextModelEvent::GlobalRulesChanged(_)) {
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
        // Codebase-index status pushes (Delta D2). Guarded rather than assumed:
        // `ServerModel::new` also runs under tests and the integration harness,
        // where no `CodebaseIndexManager` is registered and
        // `CodebaseIndexManager::handle` would panic.
        #[cfg(feature = "local_fs")]
        if model.codebase_indexing_available {
            let index_manager = CodebaseIndexManager::handle(ctx);
            ctx.subscribe_to_model(&index_manager, |me, event, ctx| {
                me.handle_codebase_index_manager_event(event, ctx);
            });
        } else {
            log::info!(
                "Daemon has no CodebaseIndexManager singleton; remote codebase \
                 indexing is inert in this process"
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
        // Bootstrap the new proxy with what this host already has indexed, so a
        // reconnecting client does not have to poll for it.
        #[cfg(feature = "local_fs")]
        self.push_codebase_index_statuses_snapshot(conn_id, ctx);
        ctx.notify();
    }

    /// Called when a proxy disconnects.  Removes it from the connection map
    /// and starts the grace timer if no connections remain.
    pub fn deregister_connection(&mut self, conn_id: ConnectionId, ctx: &mut ModelContext<Self>) {
        self.snapshot_sent_roots_by_connection.remove(&conn_id);
        #[cfg(feature = "local_fs")]
        {
            let touched_repos = self.remove_diff_state_connection(conn_id);
            for repo in touched_repos {
                self.release_diff_state_watch_if_unused(&repo, ctx);
            }
        }
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
                    // Git pull over SSH, Stage 1 (fast-forward only): mirrors
                    // GitPush's shape and locking exactly. Unlike push, a pull
                    // changes the daemon's working tree, but that doesn't need
                    // any bespoke handling here — the daemon's per-repo
                    // `DiffStateWatch` (diff_state_tracker.rs) is a real
                    // filesystem watcher already relied on to catch working-tree
                    // changes from *any* source (discard, an out-of-band `git
                    // pull` in a terminal, etc.), so it picks up a daemon-run
                    // pull's changed files the same way and pushes
                    // DiffStateFileDelta/DiffStateSnapshot to subscribers
                    // without this handler doing anything extra.
                    Some(host_scoped_request::Message::GitPull(req)) => {
                        self.handle_git_pull(req, &request_id, conn_id, ctx)
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
                    // Per-file / per-hunk staging over SSH (Zap #329): the
                    // remote equivalent of the code-review stage buttons.
                    // Runs `git add` / `git restore --staged` /
                    // `git apply --cached` against the daemon's index.
                    Some(host_scoped_request::Message::GitStage(req)) => {
                        self.handle_git_stage(req, &request_id, conn_id, ctx)
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
                    // Remote codebase indexing (Delta D2). `local_fs` for the
                    // same reason as buffer syncing: the daemon's index manager
                    // walks and stores on the host's filesystem.
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::IndexCodebase(msg)) => {
                        self.handle_index_codebase(msg, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::ResyncCodebase(msg)) => {
                        self.handle_resync_codebase(msg, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::DropCodebaseIndex(msg)) => {
                        self.handle_drop_codebase_index(msg, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::GetFragmentMetadataFromHash(msg)) => {
                        self.handle_get_fragment_metadata_from_hash(msg, &request_id, conn_id, ctx)
                    }
                    #[cfg(feature = "local_fs")]
                    Some(host_scoped_request::Message::SearchRemoteCodebase(msg)) => {
                        self.handle_search_remote_codebase(msg, &request_id, conn_id, ctx)
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
                    #[cfg(not(feature = "local_fs"))]
                    Some(
                        host_scoped_request::Message::IndexCodebase(_)
                        | host_scoped_request::Message::ResyncCodebase(_)
                        | host_scoped_request::Message::DropCodebaseIndex(_)
                        | host_scoped_request::Message::GetFragmentMetadataFromHash(_)
                        | host_scoped_request::Message::SearchRemoteCodebase(_),
                    ) => HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                        code: ErrorCode::InvalidRequest.into(),
                        message: "Codebase indexing requires the local_fs feature".to_string(),
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
                        // Configure indexing before answering: the client may
                        // send an `IndexCodebase` immediately after the
                        // handshake, and it must not race an unconfigured
                        // store client.
                        #[cfg(feature = "local_fs")]
                        {
                            Self::apply_embedding_provider(msg.embedding_provider.as_ref());
                            self.apply_codebase_index_limits(
                                msg.codebase_index_limits.as_ref(),
                                ctx,
                            );
                        }
                        self.handle_initialize(msg, &request_id)
                    }
                    Some(session_scoped_request::Message::NavigatedToDirectory(msg)) => {
                        self.handle_navigated_to_directory(msg, &request_id, conn_id, ctx)
                    }
                    Some(session_scoped_request::Message::LoadRepoMetadataDirectory(msg)) => {
                        self.handle_load_repo_metadata_directory(msg, &request_id, conn_id, ctx)
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
                    #[cfg(feature = "local_fs")]
                    Some(notification::Message::UpdateGitStatus(msg)) => {
                        self.handle_update_git_status(msg, conn_id, ctx);
                        return; // fire-and-forget notification
                    }
                    #[cfg(feature = "local_fs")]
                    Some(notification::Message::UpdateGithubPrInfo(msg)) => {
                        self.handle_update_github_pr_info(msg, ctx);
                        return; // fire-and-forget notification
                    }
                    #[cfg(feature = "local_fs")]
                    Some(notification::Message::UpdateGithubRepoInfo(msg)) => {
                        self.handle_update_github_repo_info(msg, ctx);
                        return; // fire-and-forget notification
                    }
                    // Without `local_fs` there is no host filesystem to read
                    // git status or run `gh` against, so there is nothing to
                    // create and nothing to push. Dropping is correct for the
                    // same reason as `UpdatePreferences` below: the client
                    // waits on a broadcast, not on a response, and simply
                    // never sees one.
                    #[cfg(not(feature = "local_fs"))]
                    Some(
                        notification::Message::UpdateGitStatus(_)
                        | notification::Message::UpdateGithubPrInfo(_)
                        | notification::Message::UpdateGithubRepoInfo(_),
                    ) => {
                        return;
                    }
                    #[cfg(feature = "local_fs")]
                    Some(notification::Message::UpdatePreferences(msg)) => {
                        self.handle_update_preferences(msg, ctx);
                        return; // fire-and-forget notification
                    }
                    // Without `local_fs` there is nothing to configure: the
                    // daemon cannot index. Dropping it is correct here (unlike
                    // the buffer-sync case below) because the client expects no
                    // state change it could be left waiting on.
                    #[cfg(not(feature = "local_fs"))]
                    Some(notification::Message::UpdatePreferences(_)) => {
                        return;
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
    /// - `conn_id = Some(id)` — sends only to the connection that originated
    ///   the request (used for all request/response pairs).
    /// - `conn_id = None` — broadcasts to every connected proxy (used for
    ///   server-initiated push notifications such as repo metadata updates).
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
    /// `server_version` is the release tag the daemon was built from
    /// (`GIT_RELEASE_TAG`) or the empty string for `cargo run` / locally
    /// deployed builds. The client treats an empty version as "unknown" and
    /// skips strict version enforcement, which keeps the
    /// `script/deploy_remote_server` developer workflow functional.
    /// Handles `Initialize`.
    /// Deliberately does NOT take a `ModelContext`: `server_model_tests.rs`
    /// drives this directly against a hand-built `ServerModel` with no app
    /// behind it. The codebase-index half of the handshake needs a context, so
    /// it is applied by the caller in `handle_message`, which has one.
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

    // ── Remote codebase indexing (Delta D2, remote-daemon leg) ────────────
    // Ported from `02b53fcd8:app/src/remote_server/server_model.rs`. Two
    // differences run through the whole block:
    //  * Every guard also checks `self.codebase_indexing_available`, because
    //    this fork constructs `ServerModel` in processes with no
    //    `CodebaseIndexManager` singleton, where the pin's unconditional
    //    `CodebaseIndexManager::handle(ctx)` would panic.
    //  * The pin authenticated each request against a Warp bearer token
    //    (`validate_remote_codebase_index_auth`). There is no such credential
    //    here; the daemon is configured once with the user's own embedding
    //    endpoint, so the equivalent precondition is "an embedding provider has
    //    been configured", checked by `codebase_indexing_ready`.

    /// Whether this process can actually index: the manager exists, the feature
    /// flag is on, and a client has supplied an embedding endpoint.
    #[cfg(feature = "local_fs")]
    fn codebase_indexing_ready(&self) -> Result<(), String> {
        if !self.codebase_indexing_available {
            return Err("This host's daemon was built without codebase indexing".to_string());
        }
        if !FeatureFlag::RemoteCodebaseIndexing.is_enabled() {
            return Err("Remote codebase indexing is not enabled".to_string());
        }
        match daemon_store_client() {
            Some(client) if client.is_configured() => Ok(()),
            Some(_) => Err(
                "No embedding provider has been configured for this host; add one under \
                 Settings > AI on the client"
                    .to_string(),
            ),
            None => Err("This host's daemon has no codebase index store".to_string()),
        }
    }

    /// Applies an `EmbeddingProviderConfig` from `Initialize` /
    /// `UpdatePreferences` to the daemon's store client.
    /// An absent or unparseable config clears the endpoint rather than leaving
    /// a stale one: continuing to embed against a model the user has removed
    /// would write vectors under a storage key nothing will ever query.
    #[cfg(feature = "local_fs")]
    fn apply_embedding_provider(config: Option<&super::proto::EmbeddingProviderConfig>) {
        let Some(store_client) = daemon_store_client() else {
            return;
        };
        let Some(config) = config else {
            log::info!(
                "[Remote codebase indexing] Client supplied no embedding provider; \
                 clearing the daemon's endpoint"
            );
            store_client.configure(None, None);
            return;
        };
        let Some(embedding_config) =
            EmbeddingConfig::from_storage_key(&config.embedding_storage_key)
        else {
            log::warn!(
                "[Remote codebase indexing] Unrecognized embedding storage key {:?}; \
                 clearing the daemon's endpoint rather than guessing a model",
                config.embedding_storage_key
            );
            store_client.configure(None, None);
            return;
        };
        log::info!(
            "[Remote codebase indexing] Daemon configured for embedding model {}",
            embedding_config.storage_key()
        );
        store_client.configure(
            Some(EmbeddingEndpoint {
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone(),
            }),
            Some(embedding_config),
        );
    }

    #[cfg(feature = "local_fs")]
    fn handle_codebase_index_manager_event(
        &mut self,
        event: &CodebaseIndexManagerEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        if !FeatureFlag::RemoteCodebaseIndexing.is_enabled() {
            return;
        }

        match event {
            CodebaseIndexManagerEvent::SyncStateUpdated { root_path }
            | CodebaseIndexManagerEvent::NewIndexCreated { root_path } => {
                self.push_codebase_index_status(&root_path.clone(), ctx);
            }
            CodebaseIndexManagerEvent::RemoveExpiredIndexMetadata { expired_metadata } => {
                for repo_path in expired_metadata.iter() {
                    self.push_codebase_index_status_update(disabled_codebase_index_status(
                        repo_path.to_string_lossy().to_string(),
                    ));
                }
            }
            // TODO.md "UNWIRED-CODE AUDIT 2026-08-10" finding #5: these two
            // used to be dropped unconditionally, which is exactly why the
            // daemon had no way to answer a `SearchRemoteCodebase` request —
            // nothing ever resolved the retrieval it started.
            CodebaseIndexManagerEvent::RetrievalRequestCompleted {
                retrieval_id,
                ranked_paths,
                ..
            } => {
                self.resolve_pending_codebase_retrieval(retrieval_id, Ok(ranked_paths.as_slice()));
            }
            CodebaseIndexManagerEvent::RetrievalRequestFailed {
                retrieval_id,
                error_message,
            } => {
                self.resolve_pending_codebase_retrieval(retrieval_id, Err(error_message.as_str()));
            }
            CodebaseIndexManagerEvent::IndexMetadataUpdated { .. } => {}
        }
    }

    #[cfg(feature = "local_fs")]
    fn push_codebase_index_status(&mut self, repo_path: &Path, ctx: &mut ModelContext<Self>) {
        let Some(status) = self.codebase_index_status(repo_path, ctx) else {
            return;
        };
        self.push_codebase_index_status_update(status);
    }

    #[cfg(feature = "local_fs")]
    fn push_codebase_index_status_update(&mut self, status: CodebaseIndexStatus) {
        self.send_server_message(
            None,
            None,
            server_message::Message::CodebaseIndexStatusUpdated(CodebaseIndexStatusUpdated {
                status: Some(status),
            }),
        );
    }

    /// Pushes the daemon's whole status table to a freshly-connected proxy, so
    /// a reconnecting client learns what this host already has without polling.
    #[cfg(feature = "local_fs")]
    fn push_codebase_index_statuses_snapshot(
        &mut self,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self.codebase_indexing_available || !FeatureFlag::RemoteCodebaseIndexing.is_enabled() {
            log::info!(
                "[Remote codebase indexing] Daemon skipping bootstrap codebase index statuses \
                 snapshot because remote indexing is unavailable: conn_id={conn_id}"
            );
            return;
        }
        let snapshot = self.codebase_index_statuses_snapshot(ctx);
        let status_count = snapshot.statuses.len();
        log::debug!(
            "[Remote codebase indexing] Daemon pushing bootstrap codebase index statuses \
             snapshot: conn_id={conn_id} bootstrap_status_count={status_count}"
        );
        self.send_server_message(
            Some(conn_id),
            None,
            server_message::Message::CodebaseIndexStatusesSnapshot(snapshot),
        );
    }

    #[cfg(feature = "local_fs")]
    fn codebase_index_statuses_snapshot(
        &self,
        ctx: &mut ModelContext<Self>,
    ) -> CodebaseIndexStatusesSnapshot {
        let index_manager = CodebaseIndexManager::handle(ctx);
        let statuses = index_manager
            .as_ref(ctx)
            .get_codebase_index_statuses(ctx)
            .map(|(repo_path, status)| codebase_index_status_to_proto(repo_path.as_path(), &status))
            .collect();
        CodebaseIndexStatusesSnapshot { statuses }
    }

    #[cfg(feature = "local_fs")]
    fn codebase_index_status(
        &self,
        repo_path: &Path,
        ctx: &mut ModelContext<Self>,
    ) -> Option<CodebaseIndexStatus> {
        if !self.codebase_indexing_available {
            return None;
        }
        let index_manager = CodebaseIndexManager::handle(ctx);
        index_manager
            .as_ref(ctx)
            .get_codebase_index_status_for_path(repo_path, ctx)
            .map(|status| codebase_index_status_to_proto(repo_path, &status))
    }

    #[cfg(feature = "local_fs")]
    fn handle_index_codebase(
        &mut self,
        msg: IndexCodebase,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match self.prepare_codebase_index_request(
            "IndexCodebase",
            msg.repo_path,
            CodebaseIndexRequestPathKind::Canonicalized,
            request_id,
            conn_id,
        ) {
            Ok(repo_path) => repo_path,
            Err(outcome) => return *outcome,
        };
        let status = CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.with_indexed_codebase(
                &repo_path,
                |manager, indexed_repo_path, ctx| {
                    Self::current_codebase_index_status_or_queued(manager, indexed_repo_path, ctx)
                },
                |manager, repo_path, ctx| {
                    if !manager.is_indexing_enabled() {
                        log::info!(
                            "[Remote codebase indexing] Daemon cannot start IndexCodebase because \
                             indexing is disabled: repo_path={}",
                            repo_path.display()
                        );
                        not_enabled_codebase_index_status(repo_path.to_string_lossy().to_string())
                    } else if !manager.can_create_new_indices() {
                        let failure_message = "Cannot index remote codebase because the maximum \
                                               number of codebase indexes has been reached."
                            .to_string();
                        log::warn!(
                            "[Remote codebase indexing] Daemon cannot start IndexCodebase: \
                             repo_path={} reason={failure_message}",
                            repo_path.display()
                        );
                        unavailable_codebase_index_status(
                            repo_path.to_string_lossy().to_string(),
                            failure_message,
                        )
                    } else if manager.index_directory(repo_path.to_path_buf(), ctx) {
                        Self::current_codebase_index_status_or_queued(manager, repo_path, ctx)
                    } else {
                        let failure_message =
                            "Cannot index remote codebase because indexing did not start."
                                .to_string();
                        log::warn!(
                            "[Remote codebase indexing] Daemon cannot start IndexCodebase: \
                             repo_path={} reason={failure_message}",
                            repo_path.display()
                        );
                        unavailable_codebase_index_status(
                            repo_path.to_string_lossy().to_string(),
                            failure_message,
                        )
                    }
                },
                ctx,
            )
        });

        codebase_index_status_response(status)
    }

    #[cfg(feature = "local_fs")]
    fn handle_resync_codebase(
        &mut self,
        msg: ResyncCodebase,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let ResyncCodebase { repo_path, mode } = msg;
        let mode = match CodebaseResyncMode::try_from(mode) {
            Ok(mode) => mode,
            Err(_) => {
                return invalid_request_response(format!("Invalid ResyncCodebase mode: {mode}"));
            }
        };
        let repo_path = match self.prepare_codebase_index_request(
            "ResyncCodebase",
            repo_path,
            CodebaseIndexRequestPathKind::Canonicalized,
            request_id,
            conn_id,
        ) {
            Ok(repo_path) => repo_path,
            Err(outcome) => return *outcome,
        };
        let status = CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.with_indexed_codebase(
                &repo_path,
                |manager, indexed_repo_path, ctx| {
                    match mode {
                        CodebaseResyncMode::Full => {
                            manager.try_manual_resync_codebase(indexed_repo_path, ctx);
                        }
                        CodebaseResyncMode::Incremental => {
                            if let Err(error) =
                                manager.trigger_incremental_sync_for_path(indexed_repo_path, ctx)
                            {
                                log::warn!(
                                    "Failed to trigger remote codebase incremental sync: \
                                     repo_path={} error={error}",
                                    indexed_repo_path.display()
                                );
                            }
                        }
                    }
                    Self::current_codebase_index_status_or_queued(manager, indexed_repo_path, ctx)
                },
                |_, repo_path, _| {
                    unavailable_codebase_index_status(
                        repo_path.to_string_lossy().to_string(),
                        "Cannot resync remote codebase because it has not been indexed."
                            .to_string(),
                    )
                },
                ctx,
            )
        });

        codebase_index_status_response(status)
    }

    #[cfg(feature = "local_fs")]
    fn current_codebase_index_status_or_queued(
        manager: &CodebaseIndexManager,
        indexed_repo_path: &Path,
        ctx: &mut ModelContext<CodebaseIndexManager>,
    ) -> CodebaseIndexStatus {
        manager
            .get_codebase_index_status_for_path(indexed_repo_path, ctx)
            .map(|status| codebase_index_status_to_proto(indexed_repo_path, &status))
            .unwrap_or_else(|| {
                queued_codebase_index_status(indexed_repo_path.to_string_lossy().to_string())
            })
    }

    #[cfg(feature = "local_fs")]
    fn handle_drop_codebase_index(
        &mut self,
        msg: DropCodebaseIndex,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match self.prepare_codebase_index_request(
            "DropCodebaseIndex",
            msg.repo_path,
            // Dropping uses the path as asked for, not the canonicalized one:
            // a repository that has been deleted or unmounted can still be
            // dropped from the index.
            CodebaseIndexRequestPathKind::Requested,
            request_id,
            conn_id,
        ) {
            Ok(repo_path) => repo_path,
            Err(outcome) => return *outcome,
        };
        CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.drop_index(repo_path.clone(), ctx);
        });

        codebase_index_status_response(disabled_codebase_index_status(
            repo_path.to_string_lossy().to_string(),
        ))
    }

    #[cfg(feature = "local_fs")]
    fn handle_get_fragment_metadata_from_hash(
        &self,
        msg: GetFragmentMetadataFromHash,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "[Remote codebase indexing] Daemon handling GetFragmentMetadataFromHash: \
             request_id={request_id} conn_id={conn_id} repo_path={} root_hash={} hash_count={}",
            msg.repo_path,
            msg.root_hash,
            msg.content_hashes.len()
        );

        if let Err(message) = self.codebase_indexing_ready() {
            return fragment_metadata_lookup_error_response(
                FragmentMetadataLookupErrorCode::RemoteCodebaseIndexingNotEnabled,
                message,
                None,
            );
        }

        let repo_path = match canonicalize_index_repo_path(&msg.repo_path) {
            Ok(repo_path) => repo_path,
            Err(error) => {
                return fragment_metadata_lookup_error_response(
                    FragmentMetadataLookupErrorCode::InvalidRepoPath,
                    error,
                    None,
                );
            }
        };
        let root_hash = match msg.root_hash.parse::<NodeHash>() {
            Ok(root_hash) => root_hash,
            Err(error) => {
                return fragment_metadata_lookup_error_response(
                    FragmentMetadataLookupErrorCode::InvalidRootHash,
                    format!("Invalid root_hash: {error}"),
                    None,
                );
            }
        };
        if let Err(error) = self.validate_fragment_metadata_lookup(&repo_path, &root_hash, ctx) {
            return fragment_metadata_lookup_error_response_from_error(error);
        }

        let mut valid_hashes = Vec::new();
        let mut missing_hashes = Vec::new();
        for content_hash in msg.content_hashes {
            match content_hash.parse::<ContentHash>() {
                Ok(parsed_hash) => valid_hashes.push((content_hash, parsed_hash)),
                Err(error) => missing_hashes.push(missing_fragment_metadata(
                    content_hash,
                    format!("Invalid content hash: {error}"),
                )),
            }
        }

        let content_hashes = valid_hashes
            .iter()
            .map(|(_, hash)| hash.clone())
            .collect::<Vec<_>>();
        let metadata_by_hash = match CodebaseIndexManager::handle(ctx)
            .as_ref(ctx)
            .fragment_metadatas_from_hashes(&repo_path, &root_hash, &content_hashes, ctx)
        {
            Ok(metadata_by_hash) => metadata_by_hash,
            Err(error) => {
                return fragment_metadata_lookup_error_response_from_error(error);
            }
        };

        let mut fragments = Vec::new();
        for (content_hash_string, content_hash) in valid_hashes {
            match metadata_by_hash.get(&content_hash) {
                Some(metadata) => {
                    fragments.extend(
                        metadata
                            .iter()
                            .map(|metadata| fragment_metadata_to_proto(&content_hash, metadata)),
                    );
                }
                None => missing_hashes.push(missing_fragment_metadata(
                    content_hash_string,
                    "No fragment metadata found for content hash".to_string(),
                )),
            }
        }

        HandlerOutcome::Sync(
            server_message::Message::GetFragmentMetadataFromHashResponse(
                GetFragmentMetadataFromHashResponse {
                    result: Some(get_fragment_metadata_from_hash_response::Result::Success(
                        GetFragmentMetadataFromHashSuccess {
                            fragments,
                            missing_hashes,
                        },
                    )),
                },
            ),
        )
    }

    /// Handles `SearchRemoteCodebase`: asks this host's `CodebaseIndexManager`
    /// to answer `query` against its own private index for `repo_path`.
    /// Unlike `GetFragmentMetadataFromHash`, this cannot be answered
    /// synchronously. `CodebaseIndexManager::retrieve_relevant_files` only
    /// registers the request and returns a `RetrievalID`; the real answer
    /// arrives later as a `CodebaseIndexManagerEvent::
    /// RetrievalRequestCompleted`/`RetrievalRequestFailed` — the same
    /// lifecycle `app::ai::codebase_retrieval::CodebaseRetrievalController`
    /// bridges for the local (in-process) consumer.
    /// `pending_codebase_retrievals` plus
    /// `handle_codebase_index_manager_event` are this handler's half of that
    /// bridge, so the outcome here is `HandlerOutcome::Async(None)` — not
    /// abortable via `in_progress`/`Abort`, matching `pending_file_ops`'s
    /// justification for the same shape: the retrieval outlives any one
    /// request/response pair and is tracked by its own domain id
    /// (`RetrievalID`), not by `RequestId`.
    #[cfg(feature = "local_fs")]
    fn handle_search_remote_codebase(
        &mut self,
        msg: SearchRemoteCodebase,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "[Remote codebase indexing] Daemon handling SearchRemoteCodebase: \
             request_id={request_id} conn_id={conn_id} repo_path={} query_len={}",
            msg.repo_path,
            msg.query.len()
        );

        if let Err(message) = self.codebase_indexing_ready() {
            return search_remote_codebase_error_response(
                RemoteCodebaseSearchErrorCode::NotEnabled,
                message,
            );
        }

        let repo_path = match canonicalize_index_repo_path(&msg.repo_path) {
            Ok(repo_path) => repo_path,
            Err(error) => {
                return search_remote_codebase_error_response(
                    RemoteCodebaseSearchErrorCode::InvalidRepoPath,
                    error,
                );
            }
        };

        let retrieval_id = CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.retrieve_relevant_files(msg.query, &repo_path, ctx)
        });
        let retrieval_id = match retrieval_id {
            Ok(retrieval_id) => retrieval_id,
            Err(error) => return search_remote_codebase_error_response_from_error(error),
        };

        self.pending_codebase_retrievals.insert(
            retrieval_id,
            PendingCodebaseRetrieval {
                request_id: request_id.clone(),
                conn_id,
            },
        );
        HandlerOutcome::Async(None)
    }

    /// Resolves a pending `SearchRemoteCodebase` request from the
    /// `CodebaseIndexManagerEvent` that answers it, and sends the response.
    /// No-op if `retrieval_id` is not one of ours — e.g. a retrieval this
    /// same daemon process started for a different reason, if one is ever
    /// added.
    #[cfg(feature = "local_fs")]
    fn resolve_pending_codebase_retrieval(
        &mut self,
        retrieval_id: &RetrievalID,
        result: Result<&[PathBuf], &str>,
    ) {
        let Some(pending) = self.pending_codebase_retrievals.remove(retrieval_id) else {
            return;
        };
        let response_message = match result {
            Ok(ranked_paths) => search_remote_codebase_success_message(ranked_paths),
            Err(error_message) => search_remote_codebase_error_message(
                RemoteCodebaseSearchErrorCode::RetrievalFailed,
                error_message.to_string(),
            ),
        };
        self.send_server_message(
            Some(pending.conn_id),
            Some(&pending.request_id),
            response_message,
        );
    }

    #[cfg(feature = "local_fs")]
    fn validate_fragment_metadata_lookup(
        &self,
        repo_path: &Path,
        root_hash: &NodeHash,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), LocalFragmentMetadataLookupError> {
        let Some(status) = CodebaseIndexManager::handle(ctx)
            .as_ref(ctx)
            .get_codebase_index_status_for_path(repo_path, ctx)
        else {
            return Err(LocalFragmentMetadataLookupError::IndexNotFound);
        };
        if !status.has_synced_version() {
            return Err(LocalFragmentMetadataLookupError::IndexNotSynced);
        }
        let Some(current_root_hash) = status.root_hash() else {
            return Err(LocalFragmentMetadataLookupError::IndexNotSynced);
        };
        if current_root_hash != root_hash {
            return Err(LocalFragmentMetadataLookupError::RootHashMismatch {
                requested: root_hash.clone(),
                current: current_root_hash.clone(),
            });
        }

        Ok(())
    }

    /// Validates the shared preconditions of `IndexCodebase`, `ResyncCodebase`
    /// and `DropCodebaseIndex`, returning the resolved repository path.
    #[cfg(feature = "local_fs")]
    fn prepare_codebase_index_request(
        &self,
        operation_name: &str,
        repo_path: String,
        path_kind: CodebaseIndexRequestPathKind,
        request_id: &RequestId,
        conn_id: ConnectionId,
    ) -> Result<PathBuf, Box<HandlerOutcome>> {
        let repo_path_for_log = repo_path.clone();
        if let Err(message) = self.codebase_indexing_ready() {
            log::info!(
                "[Remote codebase indexing] Daemon rejecting {operation_name}: \
                 request_id={request_id} conn_id={conn_id} repo_path={repo_path_for_log} \
                 reason={message}"
            );
            return Err(Box::new(codebase_index_status_response(
                not_enabled_codebase_index_status(repo_path),
            )));
        }

        let repo_path = match path_kind {
            CodebaseIndexRequestPathKind::Canonicalized => canonicalize_index_repo_path(&repo_path),
            CodebaseIndexRequestPathKind::Requested => requested_repo_path(&repo_path),
        }
        .map_err(|error| Box::new(invalid_request_response(error)))?;

        log::info!(
            "[Remote codebase indexing] Daemon handling {operation_name}: \
             request_id={request_id} conn_id={conn_id} repo_path={repo_path_for_log}"
        );
        Ok(repo_path)
    }

    /// Applies client-resolved index limits, so a remote index obeys the same
    /// caps as a local one.
    #[cfg(feature = "local_fs")]
    fn apply_codebase_index_limits(
        &self,
        limits: Option<&CodebaseIndexLimits>,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self.codebase_indexing_available {
            return;
        }
        let Some(limits) = limits else {
            return;
        };
        let max_indices_allowed = limits.max_indices_allowed.map(|limit| limit as usize);
        let max_files_per_repo = usize::try_from(limits.max_files_per_repo).unwrap_or(usize::MAX);
        let embedding_generation_batch_size =
            usize::try_from(limits.embedding_generation_batch_size).unwrap_or(usize::MAX);

        log::info!(
            "[Remote codebase indexing] Daemon applying codebase index limits: \
             max_indices_allowed={max_indices_allowed:?} max_files_per_repo={max_files_per_repo} \
             embedding_generation_batch_size={embedding_generation_batch_size}"
        );
        CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.update_max_limits(
                max_indices_allowed,
                max_files_per_repo,
                embedding_generation_batch_size,
                ctx,
            );
        });
    }

    /// Handles `UpdatePreferences`. This is a notification — no response is
    /// sent.
    #[cfg(feature = "local_fs")]
    fn handle_update_preferences(&mut self, msg: UpdatePreferences, ctx: &mut ModelContext<Self>) {
        log::info!("Handling UpdatePreferences");
        Self::apply_embedding_provider(msg.embedding_provider.as_ref());
        self.apply_codebase_index_limits(msg.codebase_index_limits.as_ref(), ctx);
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

    /// Handles `Abort` by cancelling the in-progress request it targets, or
    /// (#324) removing a still-queued diff-state response that joined
    /// another request's in-flight computation and so was never given its
    /// own `in_progress` entry — see `abort_diff_state_pending_response`.
    /// This is a notification — no response is sent.
    fn handle_abort(&mut self, abort: Abort, request_id: &RequestId) {
        let target_id = RequestId::from(abort.request_id_to_abort);
        if let Some(handle) = self.in_progress.remove(&target_id) {
            log::info!(
                "Aborting in-progress request (request_id={target_id}, \
                 abort_request_id={request_id})"
            );
            handle.abort();
            return;
        }
        #[cfg(feature = "local_fs")]
        if self.abort_diff_state_pending_response(&target_id) {
            log::info!(
                "Aborting queued diff-state response (request_id={target_id}, \
                 abort_request_id={request_id})"
            );
            return;
        }
        log::info!(
            "Abort for unknown/completed request (request_id={target_id}, \
             abort_request_id={request_id})"
        );
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
    /// `path_env` is `None`: the remote host's daemon runs from a login/sshd
    /// context with a normal `PATH`, so — unlike Warp's macOS GUI, which must
    /// capture an interactive-shell `PATH` for launchd-spawned processes — it
    /// finds `git` / `gh` directly.
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

    /// Handles `GitPullRequest` — runs `git pull --ff-only` on the daemon's
    /// filesystem, then returns the refreshed unpushed/upstream delta. Stage 1
    /// (fast-forward only): a diverged history comes back as `GitOpError`
    /// rather than a merge, so there is no conflict payload to plumb through.
    fn handle_git_pull(
        &mut self,
        msg: GitPullRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        let repo_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        {
            Ok(p) => PathBuf::from(p.to_local_path_lossy()),
            Err(e) => {
                return HandlerOutcome::Sync(server_message::Message::GitPullResponse(
                    GitPullResponse {
                        result: Some(git_pull_response::Result::Error(GitOpError {
                            message: format!("Invalid repo_path: {e}"),
                        })),
                    },
                ));
            }
        };
        let branch = msg.branch;

        log::info!(
            "Handling GitPull repo={} branch={branch} (request_id={request_id})",
            msg.repo_path,
        );

        let request_id_for_response = request_id.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                guard_git_operation_in_progress(&repo_path)?;
                crate::util::git::run_pull(&repo_path, &branch, None).await?;
                anyhow::Ok(crate::util::git::compute_unpushed_state(&repo_path).await)
            },
            move |me, result, _ctx| {
                let message = match result {
                    Ok((commits, upstream_ref)) => {
                        server_message::Message::GitPullResponse(GitPullResponse {
                            result: Some(git_pull_response::Result::Success(GitOpDelta {
                                unpushed_commits: commits
                                    .iter()
                                    .map(super::diff_state_proto::commit_to_proto)
                                    .collect(),
                                upstream_ref,
                            })),
                        })
                    }
                    Err(e) => server_message::Message::GitPullResponse(GitPullResponse {
                        result: Some(git_pull_response::Result::Error(GitOpError {
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
    /// A request for a key that already has a computation in flight joins it
    /// instead of triggering a redundant one (#324) — see
    /// `diff_state_in_flight` / `diff_state_pending_responses` and
    /// `resolve_diff_state_pending_responses`.
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
        let key = DiffModelKey {
            repo_path: canonical_path,
            mode: mode.clone(),
        };

        log::info!(
            "Handling GetDiffState repo={} mode={mode:?} (request_id={request_id})",
            msg.repo_path,
        );

        // Register the subscription for live pushes on repository change.
        self.register_diff_state_subscription(conn_id, key.clone(), wire_repo_path.clone());
        // Try to make this repo delta-capable so its live updates are per-file
        // rather than whole-repo (#577). Failure is fine and silent: the repo
        // simply keeps the coarse `RepositoryUpdated` push path.
        self.ensure_diff_state_watch(&key.repo_path, ctx);

        // Queue this request; if a computation for `key` is already in
        // flight (another connection watching the same repo/mode, or a fast
        // retry), join it instead of spawning a redundant one (#324).
        self.diff_state_pending_responses
            .entry(key.clone())
            .or_default()
            .push(PendingDiffStateResponse {
                request_id: request_id.clone(),
                conn_id,
                wire_repo_path,
            });
        if !self.diff_state_in_flight.insert(key.clone()) {
            return HandlerOutcome::Async(None);
        }

        let repo_pathbuf = std::path::PathBuf::from(key.repo_path.to_local_path_lossy());
        let mode_for_compute = mode.clone();
        let key_for_resolve = key.clone();
        let key_for_abort = key;
        let resolve_id = request_id.clone();
        let abort_id = request_id.clone();
        let handle = ctx.spawn_abortable(
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
                me.in_progress.remove(&resolve_id);
                me.resolve_diff_state_pending_responses(
                    &key_for_resolve,
                    &mode,
                    metadata_result.ok(),
                    diff_data,
                );
            },
            move |me, _ctx| {
                log::info!("Request cancelled (request_id={abort_id})");
                me.in_progress.remove(&abort_id);
                // This fork keeps no persistent per-key model (unlike the
                // pin's `RemoteDiffStateManager`), so an aborted leading
                // computation has no last-known-good snapshot to hand the
                // requests that joined it — fail them together instead of
                // leaving them queued for a resolution that will never come.
                me.fail_diff_state_pending_responses(&key_for_abort);
            },
        );
        self.in_progress.insert(request_id.clone(), handle);
        HandlerOutcome::Async(None)
    }

    /// Establishes the git watch that makes `repo` delta-capable, if it isn't
    /// already (#577).
    /// Returns whether the repo is delta-capable afterwards. `false` means the
    /// daemon has no watched `Repository` for the path — the repo was never
    /// detected, or detection is still in flight — and the caller must keep the
    /// coarse whole-snapshot push for it. Being coarse is a performance problem;
    /// being silent would be a correctness one, so the fallback is never removed
    /// on a guess.
    #[cfg(feature = "local_fs")]
    fn ensure_diff_state_watch(&mut self, repo: &StandardizedPath, ctx: &mut ModelContext<Self>) -> bool {
        use repo_metadata::repositories::DetectedRepositories;

        if self.diff_state_watches.contains_key(repo) {
            return true;
        }
        let repo_local = repo.to_local_path_lossy();
        let Some(repository) =
            DetectedRepositories::as_ref(ctx).get_watched_repo_for_path(&repo_local, ctx)
        else {
            log::debug!(
                "No watched repository for {repo}; diff-state pushes stay whole-snapshot"
            );
            return false;
        };

        let subscriber = super::diff_state_tracker::DiffStateTrackerSubscriber {
            tx: self.diff_state_watch_tx.clone(),
            repo_root: repo_local,
        };
        let start = repository.update(ctx, |repository, ctx| {
            repository.start_watching(
                RepositoryWatchMode::GitRepository,
                Box::new(subscriber),
                ctx,
            )
        });
        // The registration future reports scan failure; a failed scan leaves the
        // subscription inert, so drop the watch and fall back rather than
        // silently never pushing again.
        let repo_for_failure = repo.clone();
        ctx.spawn(start.registration_future, move |me, result, ctx| {
            if let Err(err) = result {
                log::warn!("Diff-state watch registration failed for {repo_for_failure}: {err}");
                if let Some(watch) = me.diff_state_watches.remove(&repo_for_failure) {
                    watch.stop(ctx);
                }
            }
        });
        self.diff_state_watches.insert(
            repo.clone(),
            super::diff_state_tracker::DiffStateWatch {
                repository,
                subscriber_id: start.subscriber_id,
            },
        );
        log::info!("Diff-state watch established for {repo}; pushes are now per-file deltas");
        true
    }

    /// Routes one classified watcher update to the right push shape (#577).
    #[cfg(feature = "local_fs")]
    fn handle_diff_state_watch_update(
        &mut self,
        repo_root: std::path::PathBuf,
        update: super::diff_state_tracker::DiffStateWatchUpdate,
        ctx: &mut ModelContext<Self>,
    ) {
        use super::diff_state_tracker::DiffStateWatchUpdate;

        let Ok(repo) = StandardizedPath::from_local_canonicalized(&repo_root) else {
            return;
        };
        match update {
            DiffStateWatchUpdate::Files(files) => {
                self.push_diff_state_file_deltas(&repo, files, ctx);
            }
            DiffStateWatchUpdate::All => self.push_diff_state_for_repo(&repo, ctx),
            // Deliberately nothing: recomputing against a locked index reads
            // half-written state. The lock release produces a fresh commit
            // update, which arrives here as `All`.
            DiffStateWatchUpdate::LockedIndex => {}
        }
    }

    /// Pushes one `DiffStateFileDelta` per changed file to every connection
    /// subscribed to `repo`, for each mode they subscribed with (#577).
    /// This is the whole point of the watch: the previous path re-serialized
    /// every file in the repository for every subscriber on every change.
    /// Each connection is addressed with the exact `repo_path` string it sent,
    /// preserving the same echo constraint the snapshot path has — the client
    /// matches incoming pushes against the literal string it subscribed with.
    #[cfg(feature = "local_fs")]
    fn push_diff_state_file_deltas(
        &mut self,
        repo: &StandardizedPath,
        files: Vec<std::path::PathBuf>,
        ctx: &mut ModelContext<Self>,
    ) {
        use crate::code_review::diff_state::{DiffMode, LocalDiffStateModel};

        let matching_keys: Vec<DiffModelKey> = self
            .diff_state_subscribers
            .keys()
            .filter(|key| &key.repo_path == repo)
            .cloned()
            .collect();
        if matching_keys.is_empty() || files.is_empty() {
            return;
        }

        let repo_pathbuf = std::path::PathBuf::from(repo.to_local_path_lossy());
        for key in matching_keys {
            let Some(subs) = self.diff_state_subscribers.get(&key).cloned() else {
                continue;
            };
            let mode = key.mode.clone();
            let files = files.clone();
            let repo_pathbuf = repo_pathbuf.clone();
            ctx.spawn(
                async move {
                    // `Head` has no merge base; the other modes need one, and it
                    // is the same for every file in this batch, so resolve it
                    // once rather than per file.
                    let merge_base = if matches!(mode, DiffMode::Head) {
                        None
                    } else {
                        LocalDiffStateModel::compute_merge_base(&repo_pathbuf, &mode)
                            .await
                            .ok()
                    };
                    let mut deltas = Vec::with_capacity(files.len());
                    for file in files {
                        match LocalDiffStateModel::retrieve_diff_state(
                            &repo_pathbuf,
                            &file,
                            &mode,
                            merge_base.as_deref(),
                        )
                        .await
                        {
                            // `retrieve_diff_state` returns the repo-relative
                            // path, which is the form the client matches
                            // against its cached `GitDiffData` entries.
                            Ok((relative, diff)) => deltas.push((relative, diff)),
                            Err(err) => {
                                log::warn!(
                                    "Per-file diff-state delta failed for {}: {err}",
                                    file.display()
                                );
                            }
                        }
                    }
                    (mode, deltas)
                },
                move |me, (mode, deltas), _ctx| {
                    for (relative, diff) in deltas {
                        let delta = super::diff_state_proto::build_diff_state_file_delta(
                            "",
                            &mode,
                            &relative.to_string_lossy(),
                            diff.as_ref(),
                            None,
                        );
                        for (conn_id, wire_repo_path) in &subs {
                            let mut delta = delta.clone();
                            delta.repo_path = wire_repo_path.clone();
                            me.send_server_message(
                                Some(*conn_id),
                                None,
                                server_message::Message::DiffStateFileDelta(delta),
                            );
                        }
                    }
                },
            );
        }
    }

    /// Records a `(repo, mode)` diff-state subscription for `conn_id`,
    /// storing the exact `repo_path` string the client sent (overwriting any
    /// previous string for the same `(key, conn_id)` — see the field doc on
    /// `diff_state_subscribers`).
    #[cfg(feature = "local_fs")]
    fn register_diff_state_subscription(
        &mut self,
        conn_id: ConnectionId,
        key: DiffModelKey,
        wire_repo_path: String,
    ) {
        self.diff_state_subscribers
            .entry(key.clone())
            .or_default()
            .insert(conn_id, wire_repo_path);
        self.diff_state_keys_by_conn
            .entry(conn_id)
            .or_default()
            .insert(key);
    }

    /// Removes one `(key, conn_id)` diff-state subscription, evicting the
    /// key's entry from both `diff_state_subscribers` and
    /// `diff_state_keys_by_conn` once its subscriber set is empty.
    #[cfg(feature = "local_fs")]
    fn drop_diff_state_subscription(&mut self, key: &DiffModelKey, conn_id: ConnectionId) {
        if let Some(subs) = self.diff_state_subscribers.get_mut(key) {
            subs.remove(&conn_id);
            if subs.is_empty() {
                self.diff_state_subscribers.remove(key);
            }
        }
        if let Some(keys) = self.diff_state_keys_by_conn.get_mut(&conn_id) {
            keys.remove(key);
            if keys.is_empty() {
                self.diff_state_keys_by_conn.remove(&conn_id);
            }
        }
    }

    /// Removes all of `conn_id`'s diff-state subscriptions (disconnect
    /// sweep), in O(subscriptions for this connection) via
    /// `diff_state_keys_by_conn` rather than a scan over every key. Also
    /// drops any of its queued pending responses, since a disconnected
    /// connection can no longer receive one.
    #[cfg(feature = "local_fs")]
    /// Returns the repositories this connection had subscriptions on, so the
    /// caller can release any now-unused git watch (#577). Kept free of
    /// `ModelContext` so it stays a pure bookkeeping operation, unit-testable
    /// against a bare `ServerModel` with no `App`.
    fn remove_diff_state_connection(&mut self, conn_id: ConnectionId) -> Vec<StandardizedPath> {
        let mut touched_repos = Vec::new();
        if let Some(keys) = self.diff_state_keys_by_conn.remove(&conn_id) {
            for key in keys {
                if let Some(subs) = self.diff_state_subscribers.get_mut(&key) {
                    subs.remove(&conn_id);
                    if subs.is_empty() {
                        self.diff_state_subscribers.remove(&key);
                    }
                }
                touched_repos.push(key.repo_path);
            }
        }
        for pending in self.diff_state_pending_responses.values_mut() {
            pending.retain(|p| p.conn_id != conn_id);
        }
        touched_repos
    }

    /// Stops `repo`'s git watch once nothing is subscribed to it (#577).
    /// Without this the daemon keeps a filesystem watcher alive for every
    /// repository any client ever asked about, for the life of the process.
    #[cfg(feature = "local_fs")]
    fn release_diff_state_watch_if_unused(
        &mut self,
        repo: &StandardizedPath,
        ctx: &mut ModelContext<Self>,
    ) {
        if self
            .diff_state_subscribers
            .keys()
            .any(|key| &key.repo_path == repo)
        {
            return;
        }
        if let Some(watch) = self.diff_state_watches.remove(repo) {
            watch.stop(ctx);
            log::debug!("Released diff-state watch for {repo} (no subscribers left)");
        }
    }

    /// Resolves every response queued for `key` — the leading request plus
    /// any that joined it while its computation was in flight — with the
    /// same computed snapshot, each addressed with its own connection's
    /// exact `repo_path` string. The proto snapshot is built once (with a
    /// placeholder path) and cloned per response, since `DiffMetadata` /
    /// `GitDiffWithBaseContent` are not `Clone` but the generated proto type
    /// is.
    #[cfg(feature = "local_fs")]
    fn resolve_diff_state_pending_responses(
        &mut self,
        key: &DiffModelKey,
        mode: &crate::code_review::diff_state::DiffMode,
        metadata: Option<crate::code_review::diff_state::DiffMetadata>,
        diff_data: Option<crate::code_review::diff_state::GitDiffWithBaseContent>,
    ) {
        self.diff_state_in_flight.remove(key);
        let pending = self
            .diff_state_pending_responses
            .remove(key)
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }
        let snapshot = super::diff_state_proto::snapshot_from_parts_with_base_content(
            String::new(),
            mode,
            metadata,
            diff_data,
        );
        for pending_response in pending {
            let mut snap = snapshot.clone();
            snap.repo_path = pending_response.wire_repo_path;
            self.send_server_message(
                Some(pending_response.conn_id),
                Some(&pending_response.request_id),
                server_message::Message::GetDiffStateResponse(
                    super::diff_state_proto::snapshot_response(snap),
                ),
            );
        }
    }

    /// Fails every response queued for `key` when its leading computation is
    /// aborted — see `handle_get_diff_state`'s `on_abort` closure.
    #[cfg(feature = "local_fs")]
    fn fail_diff_state_pending_responses(&mut self, key: &DiffModelKey) {
        self.diff_state_in_flight.remove(key);
        let pending = self
            .diff_state_pending_responses
            .remove(key)
            .unwrap_or_default();
        for pending_response in pending {
            self.send_server_message(
                Some(pending_response.conn_id),
                Some(&pending_response.request_id),
                server_message::Message::GetDiffStateResponse(
                    super::diff_state_proto::error_response(
                        "GetDiffState request was aborted".to_string(),
                    ),
                ),
            );
        }
    }

    /// Removes a queued (not yet in-flight) diff-state response matching
    /// `request_id`, across every key. Fallback used by `handle_abort` once
    /// the generic `in_progress` lookup misses: a request that joined an
    /// already-in-flight computation (see `handle_get_diff_state`) was never
    /// given its own `in_progress` entry, since cancelling it must not
    /// cancel the shared computation the other joiners are still waiting on.
    /// Returns `true` if a pending response was found and removed.
    #[cfg(feature = "local_fs")]
    fn abort_diff_state_pending_response(&mut self, request_id: &RequestId) -> bool {
        for pending in self.diff_state_pending_responses.values_mut() {
            if let Some(pos) = pending.iter().position(|p| &p.request_id == request_id) {
                pending.remove(pos);
                return true;
            }
        }
        false
    }

    /// Pushes a fresh diff-state snapshot to every connection subscribed to
    /// `changed_repo`, across every mode it has active subscriptions for.
    /// Called when a repository's contents change.
    #[cfg(feature = "local_fs")]
    fn push_diff_state_for_repo(
        &mut self,
        changed_repo: &StandardizedPath,
        ctx: &mut ModelContext<Self>,
    ) {
        // Snapshot the matching keys first so the loop below doesn't hold a
        // borrow of `self.diff_state_subscribers` across
        // `spawn_push_diff_state_snapshot`.
        let matching_keys: Vec<DiffModelKey> = self
            .diff_state_subscribers
            .keys()
            .filter(|key| &key.repo_path == changed_repo)
            .cloned()
            .collect();
        for key in matching_keys {
            let Some(subs) = self.diff_state_subscribers.get(&key).cloned() else {
                continue;
            };
            self.spawn_push_diff_state_snapshot(key, subs, ctx);
        }
    }

    /// Recomputes the snapshot for `key` once and pushes it (unsolicited, no
    /// request_id) to every connection in `subs`, each addressed with its
    /// own wire `repo_path` string. Computed once per key rather than once
    /// per connection (#324) — several connections watching the same
    /// (repo, mode) previously triggered independent, redundant recomputes
    /// on every push.
    #[cfg(feature = "local_fs")]
    fn spawn_push_diff_state_snapshot(
        &mut self,
        key: DiffModelKey,
        subs: HashMap<ConnectionId, String>,
        ctx: &mut ModelContext<Self>,
    ) {
        use crate::code_review::diff_state::{DiffMode, LocalDiffStateModel};

        let include_base_branch = !matches!(key.mode, DiffMode::Head);
        let repo_pathbuf = std::path::PathBuf::from(key.repo_path.to_local_path_lossy());
        let mode = key.mode.clone();
        let mode_for_compute = key.mode;
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
                    String::new(),
                    &mode,
                    metadata_result.ok(),
                    diff_data,
                );
                for (conn_id, wire_repo_path) in subs {
                    let mut snap = snapshot.clone();
                    snap.repo_path = wire_repo_path;
                    me.send_server_message(
                        Some(conn_id),
                        None,
                        server_message::Message::DiffStateSnapshot(snap),
                    );
                }
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

    /// Handles `GitStageRequest` — stages or un-stages whole files
    /// (`git add` / `git restore --staged`) or a single hunk
    /// (`git apply --cached`) on the daemon's filesystem, reusing the same
    /// `LocalDiffStateModel::stage_changes_impl` the local code-review
    /// buttons drive (Zap #329). On success, pushes a fresh diff-state
    /// snapshot to every connection subscribed to this repo: staging moves the
    /// index only, and the filesystem watcher that catches working-tree
    /// changes never fires for that, so without this push the client would
    /// keep rendering the pre-stage column indefinitely.
    ///
    /// `msg.mode` is accepted but not used to select a cached model, for the
    /// same reason as `handle_discard_files`.
    #[cfg(feature = "local_fs")]
    fn handle_git_stage(
        &mut self,
        msg: GitStageRequest,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        use crate::code_review::diff_state::{LocalDiffStateModel, StageRequest, StageTarget};

        fn stage_error(message: String) -> HandlerOutcome {
            HandlerOutcome::Sync(server_message::Message::GitStageResponse(GitStageResponse {
                result: Some(git_stage_response::Result::Error(GitStageError { message })),
            }))
        }

        let canonical_path =
            match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path)) {
                Ok(p) => p,
                Err(e) => return stage_error(format!("Invalid repo_path: {e}")),
            };

        // Exactly one target. Both-set is rejected rather than silently
        // preferring one: the two express different intents, and guessing
        // would stage something the client did not ask for.
        let target = match (msg.paths.is_empty(), msg.patch) {
            (false, None) => StageTarget::Paths(msg.paths),
            (true, Some(patch)) => StageTarget::Hunk(patch),
            (true, None) => {
                return stage_error("GitStageRequest set neither paths nor patch".to_string());
            }
            (false, Some(_)) => {
                return stage_error("GitStageRequest set both paths and patch".to_string());
            }
        };

        // A repo-relative path is what the local backend sends and what
        // `git add --` expects; an absolute one would resolve against the
        // daemon's filesystem root and silently address the wrong tree.
        if let StageTarget::Paths(paths) = &target {
            if let Some(bad) = paths.iter().find(|p| Path::new(p.as_str()).is_absolute()) {
                return stage_error(format!(
                    "GitStageRequest path must be repo-relative, got '{bad}'"
                ));
            }
        }

        let request = StageRequest {
            target,
            unstage: msg.reverse,
        };
        let repo_path = PathBuf::from(canonical_path.to_local_path_lossy());

        log::info!(
            "Handling GitStage repo={} reverse={} (request_id={request_id})",
            msg.repo_path,
            msg.reverse,
        );

        let request_id_for_response = request_id.clone();
        let repo_path_for_push = canonical_path.clone();
        let handle = self.spawn_request_handler(
            request_id.clone(),
            async move {
                guard_git_operation_in_progress(&repo_path)?;
                LocalDiffStateModel::stage_changes_impl(&repo_path, &request).await
            },
            move |me, result, ctx| {
                let message = match result {
                    Ok(_) => {
                        me.push_diff_state_for_repo(&repo_path_for_push, ctx);
                        server_message::Message::GitStageResponse(GitStageResponse {
                            result: Some(git_stage_response::Result::Success(GitStageSuccess {})),
                        })
                    }
                    Err(e) => server_message::Message::GitStageResponse(GitStageResponse {
                        result: Some(git_stage_response::Result::Error(GitStageError {
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
    fn handle_git_stage(
        &mut self,
        _msg: GitStageRequest,
        _request_id: &RequestId,
        _conn_id: ConnectionId,
        _ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        HandlerOutcome::Sync(server_message::Message::GitStageResponse(GitStageResponse {
            result: Some(git_stage_response::Result::Error(GitStageError {
                message: "Staging requires the local_fs feature".to_string(),
            })),
        }))
    }

    /// Handles `UnsubscribeDiffState` — fire-and-forget removal of a diff-state
    /// subscription for a `(repo, mode)` pair on this connection.
    #[cfg(feature = "local_fs")]
    fn handle_unsubscribe_diff_state(
        &mut self,
        msg: UnsubscribeDiffState,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Ok(canonical_path) =
            StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path))
        else {
            return;
        };
        let mode = super::diff_state_proto::proto_to_diff_mode(&msg.mode.unwrap_or_default());
        let key = DiffModelKey {
            repo_path: canonical_path,
            mode,
        };
        self.drop_diff_state_subscription(&key, conn_id);
        self.release_diff_state_watch_if_unused(&key.repo_path, ctx);
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
    // Ported from the pinned oracle's `subscribe_git_status` /
    // `unsubscribe_git_status` (issue #330). Bookkeeping only: which
    // connection currently watches which repo's git status, and eviction of
    // the per-repo model caches once a repo has no subscribers left. The
    // models themselves are created by `subscribe_to_git_status_updates` /
    // `subscribe_to_github_info_updates` further down, which is where the
    // `GitStatusPush` / `GitHubPrInfoPush` / `GitHubRepositoryInfoPush`
    // broadcasts are wired.

    /// Subscribe `conn` to `repo`'s git status (navigation in), moving it off
    /// any repo it was previously in. A no-op if `conn` is already the repo's
    /// subscriber.
    /// Pure bookkeeping — the caller ensures the per-repo git-status model
    /// exists via `subscribe_to_git_status_updates`.
    #[cfg(feature = "local_fs")]
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

    // ── Git-status / GitHub: per-repo models and push broadcasts ────────
    // Ported from the pin's `subscribe_to_git_status_updates`,
    // `push_git_status`, `subscribe_to_github_info_updates`,
    // `push_github_pr_info` and `push_github_repository_info`
    // (`42effe840:app/src/remote_server/server_model.rs:3469-3715`).
    // De-clouding note: every value pushed from here is produced by a local
    // subprocess on the daemon host — `git` for status, the `gh` CLI for PR and
    // repository info. No Warp backend is contacted on this side either.

    /// Subscribes the daemon to per-repo local git status updates. On first
    /// creation it wires model events to broadcast a `GitStatusPush`. No-op if
    /// already subscribed, or when the repo is not yet a watched repository;
    /// the next navigation or explicit snapshot request will try again.
    #[cfg(feature = "local_fs")]
    fn subscribe_to_git_status_updates(
        &mut self,
        repo_path: &StandardizedPath,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.git_status_models.contains_key(repo_path) {
            return;
        }
        let repo = LocalOrRemotePath::Local(repo_path.to_local_path_lossy());
        let handle = match GitRepoModels::handle(ctx)
            .update(ctx, |factory, ctx| factory.subscribe(&repo, ctx))
        {
            Ok(handle) => handle,
            Err(e) => {
                log::warn!("Daemon: git status subscribe failed for {repo_path}: {e}");
                return;
            }
        };

        let path_for_sub = repo_path.clone();
        ctx.subscribe_to_model(&handle, move |me, _event, ctx| {
            me.push_git_status(&path_for_sub, ctx);
        });

        self.git_status_models.insert(repo_path.clone(), handle);
    }

    /// Broadcasts the repo's current git-status snapshot to every connection.
    /// Silent when the model does not exist yet or has not computed metadata:
    /// the client re-asks on `HostConnected`, and the watcher tick will push
    /// as soon as there is something to push.
    #[cfg(feature = "local_fs")]
    fn push_git_status(&mut self, repo_path: &StandardizedPath, ctx: &mut ModelContext<Self>) {
        let Some(handle) = self.git_status_models.get(repo_path) else {
            return;
        };
        let Some(metadata) = handle.as_ref(ctx).metadata(ctx) else {
            return;
        };
        let proto_metadata = git_status_metadata_to_proto(metadata);
        self.send_server_message(
            None,
            None,
            server_message::Message::GitStatusPush(GitStatusPush {
                repo_path: repo_path.to_string(),
                metadata: Some(proto_metadata),
            }),
        );
    }

    /// Handles the `UpdateGitStatus` notification (fire-and-forget).
    #[cfg(feature = "local_fs")]
    fn handle_update_git_status(
        &mut self,
        msg: UpdateGitStatus,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let std_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path)) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Invalid repo_path for UpdateGitStatus: {e}");
                return;
            }
        };

        // This notification rides an arbitrary connection for the host, so it
        // says nothing about which repo the connection's session is in.
        // Register only when the connection is untracked, which keeps the
        // requested repo's model alive across reconnect until
        // `NavigatedToDirectory` lands.
        if !self.git_status_repo_by_conn.contains_key(&conn_id) {
            self.subscribe_git_status(conn_id, &std_path);
            self.subscribe_to_git_status_updates(&std_path, ctx);
        }
        self.push_git_status(&std_path, ctx);
    }

    /// Subscribes the daemon to per-repo local GitHub info updates. On first
    /// creation it wires model events to broadcast separate PR-info and
    /// repository-info pushes. No-op if already subscribed, or when the repo is
    /// not yet a watched repository (the client requests another snapshot on
    /// `HostConnected`).
    #[cfg(feature = "local_fs")]
    fn subscribe_to_github_info_updates(
        &mut self,
        repo_path: &StandardizedPath,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.github_repo_models.contains_key(repo_path) {
            return;
        }
        let repo = LocalOrRemotePath::Local(repo_path.to_local_path_lossy());
        let handle = match GitRepoModels::handle(ctx)
            .update(ctx, |factory, ctx| factory.subscribe_github_repo(&repo, ctx))
        {
            Ok(handle) => handle,
            Err(e) => {
                log::warn!("Daemon: github repo subscribe failed for {repo_path}: {e}");
                return;
            }
        };

        let path_for_sub = repo_path.clone();
        ctx.subscribe_to_model(&handle, move |me, event, ctx| match event {
            GitHubRepoEvent::PrInfoChanged => me.push_github_pr_info(&path_for_sub, ctx),
            GitHubRepoEvent::RepositoryInfoChanged => {
                me.push_github_repository_info(&path_for_sub, ctx)
            }
        });

        self.github_repo_models.insert(repo_path.clone(), handle);
    }

    /// Handles the `UpdateGitHubPrInfo` notification (fire-and-forget).
    /// Ensures the per-repo `GitHubRepoModel` exists and refreshes PR info.
    #[cfg(feature = "local_fs")]
    fn handle_update_github_pr_info(
        &mut self,
        msg: UpdateGitHubPrInfo,
        ctx: &mut ModelContext<Self>,
    ) {
        let std_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path)) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Invalid repo_path for UpdateGitHubPrInfo: {e}");
                return;
            }
        };
        // A model created just now already fetches on construction, so only an
        // already-tracked repo needs an explicit refresh.
        let already_tracked = self.github_repo_models.contains_key(&std_path);
        self.subscribe_to_github_info_updates(&std_path, ctx);
        if already_tracked {
            if let Some(handle) = self.github_repo_models.get(&std_path).cloned() {
                handle.update(ctx, |model, ctx| model.refresh_pr_info(ctx));
            }
        }
    }

    /// Handles the `UpdateGitHubRepoInfo` notification (fire-and-forget).
    /// Ensures the per-repo `GitHubRepoModel` exists and refreshes repo info.
    #[cfg(feature = "local_fs")]
    fn handle_update_github_repo_info(
        &mut self,
        msg: UpdateGitHubRepoInfo,
        ctx: &mut ModelContext<Self>,
    ) {
        let std_path = match StandardizedPath::from_local_canonicalized(Path::new(&msg.repo_path)) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Invalid repo_path for UpdateGitHubRepoInfo: {e}");
                return;
            }
        };
        let already_tracked = self.github_repo_models.contains_key(&std_path);
        self.subscribe_to_github_info_updates(&std_path, ctx);
        if already_tracked {
            if let Some(handle) = self.github_repo_models.get(&std_path).cloned() {
                handle.update(ctx, |model, ctx| model.refresh_repository_info(ctx));
            }
        }
    }

    /// Broadcasts the repo's current PR info to every connection. `None` is a
    /// meaningful value here (branch has no PR), so it is pushed as an absent
    /// optional rather than suppressed.
    #[cfg(feature = "local_fs")]
    fn push_github_pr_info(&mut self, repo_path: &StandardizedPath, ctx: &mut ModelContext<Self>) {
        let Some(handle) = self.github_repo_models.get(repo_path) else {
            return;
        };
        let pr_info = handle.as_ref(ctx).pr_info(ctx).map(pr_info_to_proto);
        self.send_server_message(
            None,
            None,
            server_message::Message::GithubPrInfoPush(GitHubPrInfoPush {
                repo_path: repo_path.to_string(),
                pr_info,
            }),
        );
    }

    /// Broadcasts the repo's current repository name/owner to every connection.
    #[cfg(feature = "local_fs")]
    fn push_github_repository_info(
        &mut self,
        repo_path: &StandardizedPath,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(handle) = self.github_repo_models.get(repo_path) else {
            return;
        };
        let repository_info = handle
            .as_ref(ctx)
            .repository_info(ctx)
            .map(repository_info_to_proto);
        self.send_server_message(
            None,
            None,
            server_message::Message::GithubRepositoryInfoPush(GitHubRepositoryInfoPush {
                repo_path: repo_path.to_string(),
                repository_info,
            }),
        );
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
                        let mut stdout = output.stdout.clone();
                        let mut stderr = output.stderr.clone();

                        // Truncate to stay under the wire-level message size
                        // limit. Leave headroom for protobuf framing overhead.
                        // Without this, an oversized `RunCommand` response
                        // failed at the write layer with `MessageTooLarge`
                        // (see the writer loop in `unix/mod.rs`) instead of
                        // ever reaching the client in any form.
                        // Upstream: d9dee18e19e8c06e24b7b32a9619685e5dd3289c
                        // (#10681).
                        const MAX_OUTPUT_BYTES: usize = MAX_MESSAGE_SIZE - 1024;
                        let total = stdout.len() + stderr.len();
                        if total > MAX_OUTPUT_BYTES {
                            log::warn!(
                                "RunCommand output too large \
                                 (request_id={request_id_for_response}): \
                                 {total} bytes, truncating to {MAX_OUTPUT_BYTES}"
                            );
                            let ratio = MAX_OUTPUT_BYTES as f64 / total as f64;
                            stdout.truncate((stdout.len() as f64 * ratio) as usize);
                            stderr.truncate((stderr.len() as f64 * ratio) as usize);
                        }

                        log::info!(
                            "RunCommand completed (request_id={request_id_for_response}): \
                             exit_code={:?}, stdout_len={}, stderr_len={}",
                            output.exit_code,
                            stdout.len(),
                            stderr.len(),
                        );
                        run_command_response::Result::Success(RunCommandSuccess {
                            stdout,
                            stderr,
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

                // Move this connection's git-status subscription to the repo it
                // just navigated into (or drop it when it left git), create the
                // per-repo models if this is the first subscriber, and push an
                // opportunistic snapshot. Ported from the pin's
                // `42effe840:app/src/remote_server/server_model.rs:2090-2108`;
                // before this the bookkeeping existed but nothing called it, so
                // the maps stayed empty and no push was ever sent.
                #[cfg(feature = "local_fs")]
                {
                    if is_git {
                        if let Ok(root_path) =
                            StandardizedPath::from_local_canonicalized(Path::new(&indexed_path))
                        {
                            me.subscribe_git_status(conn_id_for_response, &root_path);
                            me.subscribe_to_git_status_updates(&root_path, ctx);
                            me.push_git_status(&root_path, ctx);
                        }
                    } else {
                        me.unsubscribe_git_status(conn_id_for_response);
                    }
                }

                // After responding, push a snapshot if metadata is available.
                // For git repos this is an opportunistic push for the case
                // where the repo was already indexed and RepositoryUpdated
                // won't fire again (which would otherwise leave the client
                // with only a placeholder root). We skip if a snapshot was
                // already sent for this connection+root.
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
                                .standing_query_results(&id, ctx)
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
    /// server's local model and returning the children after the async load completes.
    fn handle_load_repo_metadata_directory(
        &mut self,
        msg: super::proto::LoadRepoMetadataDirectory,
        request_id: &RequestId,
        conn_id: ConnectionId,
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

        // Load the directory on the server's local model. The returned future resolves after the
        // LocalRepoMetadataModel completion callback has applied or rejected the subtree.
        let load_future = RepoMetadataModel::handle(ctx).update(ctx, |model, ctx| {
            model.load_directory_with_completion(&repo_path, &dir_path, ctx)
        });

        let load_future = match load_future {
            Ok(load_future) => load_future,
            Err(e) => {
                log::warn!("LoadRepoMetadataDirectory failed: {e}");
                return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                    code: ErrorCode::Internal.into(),
                    message: format!("Failed to load directory: {e}"),
                }));
            }
        };

        let request_id_for_response = request_id.clone();
        let repo_path_for_response = msg.repo_path;
        let dir_path_for_response = msg.dir_path;
        let handle = self.spawn_request_handler(
            request_id.clone(),
            load_future,
            move |me, load_result, ctx| {
                if let Err(e) = load_result {
                    log::warn!("LoadRepoMetadataDirectory failed: {e}");
                    me.send_server_message(
                        Some(conn_id),
                        Some(&request_id_for_response),
                        server_message::Message::Error(ErrorResponse {
                            code: ErrorCode::Internal.into(),
                            message: format!("Failed to load directory: {e}"),
                        }),
                    );
                    return;
                }

                // Read back the loaded children and serialize them after the completion callback
                // has inserted the subtree.
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

                me.send_server_message(
                    Some(conn_id),
                    Some(&request_id_for_response),
                    server_message::Message::LoadRepoMetadataDirectoryResponse(
                        super::proto::LoadRepoMetadataDirectoryResponse {
                            repo_path: repo_path_for_response,
                            dir_path: dir_path_for_response,
                            entries,
                        },
                    ),
                );
            },
            ctx,
        );
        HandlerOutcome::Async(Some(handle))
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
    /// When `force_reload` is set, the server re-reads the file from disk even
    /// if the buffer is already loaded. This broadcasts a `BufferUpdatedPush` to
    /// the other connections and responds with the fresh content via
    /// `OpenBufferResponse`.
    #[cfg(feature = "local_fs")]
    fn handle_open_buffer(
        &mut self,
        msg: OpenBuffer,
        request_id: &RequestId,
        conn_id: ConnectionId,
        ctx: &mut ModelContext<Self>,
    ) -> HandlerOutcome {
        log::info!(
            "Handling OpenBuffer path={path} force_reload={force_reload} (request_id={request_id})",
            path = msg.path,
            force_reload = msg.force_reload,
        );

        // For force_reload on an already-tracked buffer, skip open_server_local
        // to avoid a spurious BufferLoaded event that would consume the pending
        // request before ServerLocalBufferUpdated can use it for exclusion.
        if msg.force_reload
            && let Some(file_id) = self.buffers.file_id_for_path(&msg.path)
        {
            self.buffers.add_connection(file_id, conn_id);
            let gbm = GlobalBufferModel::handle(ctx);
            self.buffers.insert_pending(
                file_id,
                request_id.clone(),
                conn_id,
                PendingBufferRequestKind::OpenBuffer,
            );
            if let Err(e) = gbm.update(ctx, |gbm, ctx| gbm.force_reload_server_local(file_id, ctx))
            {
                self.buffers
                    .take_pending_by_kind(&file_id, PendingBufferRequestKind::OpenBuffer);
                return HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
                    code: ErrorCode::Internal.into(),
                    message: e,
                }));
            }
            return HandlerOutcome::Async(None);
        }
        // Buffer not yet tracked -- fall through to open_server_local below.

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

// ── Remote codebase indexing helpers (Delta D2) ───────────────────────────
// Ported verbatim from `02b53fcd8:app/src/remote_server/server_model.rs`
// except where noted.

/// Whether a codebase-index request's path should be canonicalized against the
/// filesystem, or taken as the client asked for it.
#[cfg(feature = "local_fs")]
#[derive(Clone, Copy)]
enum CodebaseIndexRequestPathKind {
    Canonicalized,
    Requested,
}

/// A malformed request. Shares the shape every other handler in this file uses
/// for `ErrorCode::InvalidRequest`; extracted because the codebase-index
/// handlers need it from three places.
#[cfg(feature = "local_fs")]
fn invalid_request_response(message: String) -> HandlerOutcome {
    HandlerOutcome::Sync(server_message::Message::Error(ErrorResponse {
        code: ErrorCode::InvalidRequest.into(),
        message,
    }))
}

#[cfg(feature = "local_fs")]
fn codebase_index_status_response(status: CodebaseIndexStatus) -> HandlerOutcome {
    HandlerOutcome::Sync(server_message::Message::CodebaseIndexStatusUpdated(
        CodebaseIndexStatusUpdated {
            status: Some(status),
        },
    ))
}

#[cfg(feature = "local_fs")]
fn requested_repo_path(repo_path: &str) -> Result<PathBuf, String> {
    if repo_path.is_empty() {
        return Err("repo_path is required".to_string());
    }
    StandardizedPath::from_local_canonicalized(Path::new(repo_path))
        .map(|path| path.to_local_path_lossy())
        .map_err(|error| format!("Invalid repo_path {repo_path}: {error}"))
}

#[cfg(feature = "local_fs")]
fn canonicalize_index_repo_path(repo_path: &str) -> Result<PathBuf, String> {
    requested_repo_path(repo_path)?;
    let standardized_path = StandardizedPath::from_local_canonicalized(Path::new(repo_path))
        .map_err(|error| format!("Invalid repo_path {repo_path}: {error}"))?;
    Ok(standardized_path
        .to_local_path()
        .unwrap_or_else(|| standardized_path.to_local_path_lossy()))
}

#[cfg(feature = "local_fs")]
fn missing_fragment_metadata(content_hash: String, message: String) -> MissingFragmentMetadata {
    MissingFragmentMetadata {
        content_hash,
        error: Some(FileOperationError { message }),
    }
}

#[cfg(feature = "local_fs")]
fn fragment_metadata_lookup_error_response(
    code: FragmentMetadataLookupErrorCode,
    message: String,
    current_root_hash: Option<String>,
) -> HandlerOutcome {
    HandlerOutcome::Sync(
        server_message::Message::GetFragmentMetadataFromHashResponse(
            GetFragmentMetadataFromHashResponse {
                result: Some(get_fragment_metadata_from_hash_response::Result::Error(
                    ProtoFragmentMetadataLookupError {
                        code: code.into(),
                        message,
                        current_root_hash,
                    },
                )),
            },
        ),
    )
}

#[cfg(feature = "local_fs")]
fn fragment_metadata_lookup_error_response_from_error(
    error: LocalFragmentMetadataLookupError,
) -> HandlerOutcome {
    let (code, message, current_root_hash) = match error {
        LocalFragmentMetadataLookupError::IndexNotFound => (
            FragmentMetadataLookupErrorCode::IndexNotFound,
            "Codebase index not found".to_string(),
            None,
        ),
        LocalFragmentMetadataLookupError::IndexNotSynced => (
            FragmentMetadataLookupErrorCode::IndexNotSynced,
            "Codebase index has no synced root hash".to_string(),
            None,
        ),
        LocalFragmentMetadataLookupError::RootHashMismatch { requested, current } => (
            FragmentMetadataLookupErrorCode::RootHashMismatch,
            format!("Codebase index root hash mismatch: requested {requested}, current {current}"),
            Some(current.to_string()),
        ),
    };

    fragment_metadata_lookup_error_response(code, message, current_root_hash)
}

#[cfg(feature = "local_fs")]
fn fragment_metadata_to_proto(
    content_hash: &ContentHash,
    metadata: &LocalFragmentMetadata,
) -> ProtoFragmentMetadata {
    ProtoFragmentMetadata {
        content_hash: content_hash.to_string(),
        path: metadata.absolute_path.to_string_lossy().to_string(),
        start_line: metadata.location.start_line as u32,
        end_line: metadata.location.end_line as u32,
        byte_start: metadata.location.byte_range.start.as_usize() as u64,
        byte_end: metadata.location.byte_range.end.as_usize() as u64,
    }
}

// ── SearchRemoteCodebase helpers (TODO.md "UNWIRED-CODE AUDIT 2026-08-10"
// finding #5) ───────────────────────────────────────────────────────────
// Fork-original: the pin has no equivalent RPC. See the field comment on
// `HostScopedRequest.search_remote_codebase` in the proto file.

#[cfg(feature = "local_fs")]
fn search_remote_codebase_success_message(ranked_paths: &[PathBuf]) -> server_message::Message {
    server_message::Message::SearchRemoteCodebaseResponse(SearchRemoteCodebaseResponse {
        result: Some(search_remote_codebase_response::Result::Success(
            SearchRemoteCodebaseSuccess {
                ranked_paths: ranked_paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            },
        )),
    })
}

#[cfg(feature = "local_fs")]
fn search_remote_codebase_error_message(
    code: RemoteCodebaseSearchErrorCode,
    message: String,
) -> server_message::Message {
    server_message::Message::SearchRemoteCodebaseResponse(SearchRemoteCodebaseResponse {
        result: Some(search_remote_codebase_response::Result::Error(
            RemoteCodebaseSearchError {
                code: code.into(),
                message,
            },
        )),
    })
}

#[cfg(feature = "local_fs")]
fn search_remote_codebase_error_response(
    code: RemoteCodebaseSearchErrorCode,
    message: String,
) -> HandlerOutcome {
    HandlerOutcome::Sync(search_remote_codebase_error_message(code, message))
}

/// Maps `retrieve_relevant_files`'s synchronous rejection (the index isn't in
/// a state where a retrieval could even be started) onto the wire error
/// code. Mirrors `app::ai::codebase_retrieval`'s `From<RetrieveFileError> for
/// RetrievalFailure` — the two are meant to produce the same classification
/// for the same underlying condition, one leg local, one remote.
#[cfg(feature = "local_fs")]
fn search_remote_codebase_error_response_from_error(error: RetrieveFileError) -> HandlerOutcome {
    let (code, message) = match error {
        RetrieveFileError::IndexNotFound => (
            RemoteCodebaseSearchErrorCode::IndexNotFound,
            "Codebase index not found".to_string(),
        ),
        RetrieveFileError::IndexSyncing => (
            RemoteCodebaseSearchErrorCode::IndexSyncing,
            "Codebase index still syncing".to_string(),
        ),
        RetrieveFileError::IndexFailed(error) => (
            RemoteCodebaseSearchErrorCode::IndexFailed,
            format!("Codebase index failed: {error:#}"),
        ),
    };
    search_remote_codebase_error_response(code, message)
}

#[cfg(test)]
#[path = "server_model_tests.rs"]
mod tests;
