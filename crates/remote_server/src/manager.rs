use std::collections::{HashMap, HashSet};
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(target_family = "wasm"))]
use std::time::Duration;

use crate::HostId;
use crate::auth::RemoteServerAuthContext;
#[cfg(not(target_family = "wasm"))]
use crate::client::ClientEvent;
use crate::client::RemoteServerClient;
use crate::setup::PreinstallCheckResult;
#[cfg(not(target_family = "wasm"))]
use crate::setup::RemoteOs;
use crate::setup::RemotePlatform;
use crate::setup::RemoteServerSetupState;
use crate::setup::UnsupportedReason;
#[cfg(not(target_family = "wasm"))]
use crate::transport::Connection;
use crate::transport::RemoteTransport;
use futures::channel::oneshot;
use repo_metadata::RepoMetadataUpdate;
use serde::Serialize;
use warp_core::SessionId;
#[cfg(not(target_family = "wasm"))]
use warp_core::channel::{Channel, ChannelState};
#[cfg(not(target_family = "wasm"))]
use warpui::r#async::FutureExt as _;
use warpui::{Entity, ModelContext, ModelSpawner, SingletonEntity};

/// Maximum number of reconnection attempts after a spontaneous disconnect.
#[cfg(not(target_family = "wasm"))]
const MAX_RECONNECT_ATTEMPTS: u32 = 2;
/// Delay between reconnection attempts.
#[cfg(not(target_family = "wasm"))]
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Parameters that travel together through the reconnection flow.
#[cfg(not(target_family = "wasm"))]
struct ReconnectParams {
    attempt: u32,
    host_id: HostId,
    exit_status: Option<RemoteServerExitStatus>,
    transport: Arc<dyn RemoteTransport>,
    auth_context: Arc<RemoteServerAuthContext>,
    control_path: Option<PathBuf>,
    identity_key: String,
}

/// Error from [`RemoteServerManager::run_connect_and_handshake`] that
/// preserves which phase failed so callers can report accurate telemetry.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, thiserror::Error)]
enum ConnectAndHandshakeError {
    /// `transport.connect()` failed, or the session was deregistered
    /// before the connect phase could complete.
    #[error("connect: {0:#}")]
    Connect(anyhow::Error),
    /// `client.initialize()` handshake failed.
    #[error("initialize: {0:#}")]
    Initialize(anyhow::Error),
}

#[cfg(not(target_family = "wasm"))]
impl ConnectAndHandshakeError {
    fn phase(&self) -> RemoteServerInitPhase {
        match self {
            Self::Connect(_) => RemoteServerInitPhase::Connect,
            Self::Initialize(_) => RemoteServerInitPhase::Initialize,
        }
    }
}

/// Which phase of the remote server connection flow failed.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteServerInitPhase {
    /// `transport.connect()` failed (SSH/process spawn level).
    Connect,
    /// `client.initialize()` failed (protocol handshake level).
    Initialize,
}

/// The remote server client operation that failed.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteServerOperation {
    NavigateToDirectory,
    LoadRepoMetadataDirectory,
}

/// Classification of a remote server client error for telemetry.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteServerErrorKind {
    Timeout,
    Disconnected,
    ServerError,
    Other,
}

/// Exit status information captured from the remote server subprocess
/// when the connection drops. Used for diagnostics and telemetry.
#[derive(Clone, Debug, Serialize)]
pub struct RemoteServerExitStatus {
    /// Process exit code, if the process exited normally.
    pub code: Option<i32>,
    /// True if the process was killed by a signal (Unix only).
    pub signal_killed: bool,
}

impl RemoteServerErrorKind {
    /// Classify a [`ClientError`] into a telemetry error kind.
    pub fn from_client_error(error: &crate::client::ClientError) -> Self {
        use crate::client::ClientError;
        match error {
            ClientError::Timeout(_) => Self::Timeout,
            ClientError::Disconnected | ClientError::ResponseChannelClosed => Self::Disconnected,
            ClientError::ServerError { .. } => Self::ServerError,
            ClientError::Protocol(_)
            | ClientError::UnexpectedResponse
            | ClientError::FileOperationFailed(_) => Self::Other,
        }
    }
}

/// Returns `true` if the client and server are on compatible versions for
/// the initialize handshake.
///
/// Semantics:
/// - Both sides carry a non-empty release tag (`Some(_)` client, non-empty
///   `server` string): the tags must match exactly. Mismatched releases
///   cause the manager to tear the session down and delete the stale
///   binary so the next reconnect reinstalls.
/// - Both sides are unknown (client `None` and server reports an empty
///   string): treat as compatible. This preserves the `cargo run` +
///   `script/deploy_remote_server` dev loop, where neither side reports a
///   release tag.
#[cfg(not(target_family = "wasm"))]
fn version_is_compatible(client: Option<&str>, server: &str) -> bool {
    match (client, server.is_empty()) {
        (Some(c), false) => c == server,
        (None, true) => true,
        (Some(_), true) | (None, false) => false,
    }
}

/// Whether to enforce strict tag matching against the remote `server_version`.
///
/// For [`Channel::Oss`](Zap), a local source build has no `GIT_RELEASE_TAG`,
/// but the SSH Extension may install a remote-server from the latest release.
/// If strict checking were enforced, the client's `None` vs the server's
/// non-empty tag would trigger a delete-reinstall-mismatch-again loop. Release
/// builds avoid stale binaries via the versioned install path; local builds
/// continue to skip strict version checking.
#[cfg(not(target_family = "wasm"))]
fn should_enforce_remote_version_check(channel: Channel) -> bool {
    !matches!(channel, Channel::Oss)
}

/// Per-session connection state. Encodes which data is available at each
/// lifecycle stage so the compiler prevents invalid combinations.
///
/// For subprocess-backed transports (SSH), the `Initializing` and
/// `Connected` variants also own the transport's `Child`. Dropping or
/// replacing the state sends SIGKILL to the subprocess via
/// `kill_on_drop`, which is the authoritative teardown path -- it fires
/// on both explicit deregistration and spontaneous disconnect, and is
/// unaffected by lingering `Arc<RemoteServerClient>` clones held
/// elsewhere (e.g. the per-session command executor).
///
/// They also optionally carry a `control_path` pointing at the SSH
/// `ControlMaster` socket for this session. On explicit teardown
/// (after the user's shell exits), `deregister_session` uses this to
/// run `ssh -O exit`, forcing the master to terminate without waiting
/// for half-closed multiplexed channels to finish cleanup on the
/// remote side.
#[derive(Debug)]
pub enum RemoteSessionState {
    /// `connect_session` has been called; background task is starting the
    /// server process over SSH.
    Connecting,
    /// Server process spawned, client exists, initialize handshake in progress.
    Initializing {
        client: Arc<RemoteServerClient>,
        /// The transport's owning `Child`. Dropped when the state is
        /// replaced or removed, killing the subprocess via
        /// `kill_on_drop`.
        #[cfg(not(target_family = "wasm"))]
        _child: async_process::Child,
        /// See type-level doc.
        #[cfg(not(target_family = "wasm"))]
        control_path: Option<PathBuf>,
    },
    /// Initialize handshake succeeded. Client is ready for requests.
    Connected {
        client: Arc<RemoteServerClient>,
        host_id: HostId,
        /// Identity key that was active when this session was established.
        /// Used by `rotate_auth_token` to ensure token rotation notifications
        /// are only delivered to sessions that belong to the current user
        /// identity, preventing a stale session for a previous identity from
        /// receiving a different user's bearer token.
        identity_key: String,
        /// The transport's owning `Child`. See `Initializing::_child`.
        #[cfg(not(target_family = "wasm"))]
        _child: async_process::Child,
        /// See type-level doc.
        #[cfg(not(target_family = "wasm"))]
        control_path: Option<PathBuf>,
        /// Transport stored for reconnection after spontaneous disconnect.
        #[cfg(not(target_family = "wasm"))]
        transport: Arc<dyn RemoteTransport>,
    },
    /// A reconnection attempt is in progress after a spontaneous disconnect.
    #[cfg(not(target_family = "wasm"))]
    Reconnecting {
        attempt: u32,
        host_id: HostId,
        control_path: Option<PathBuf>,
    },
    /// Connection dropped (EOF/error from the reader task).
    Disconnected,
}

/// Events emitted by [`RemoteServerManager`].
#[derive(Clone, Debug)]
pub enum RemoteServerManagerEvent {
    // --- Session-scoped events ---
    /// A connection flow has started for this session.
    SessionConnecting { session_id: SessionId },
    /// This session's server is connected and ready. Includes the `HostId`
    /// received from the initialize handshake, for model deduplication.
    SessionConnected {
        session_id: SessionId,
        host_id: HostId,
    },
    /// The remote server launch or handshake failed.
    SessionConnectionFailed {
        session_id: SessionId,
        /// Which phase of the connection flow failed.
        phase: RemoteServerInitPhase,
        /// The error message from the failed phase.
        error: String,
    },
    /// This session's connection dropped. Carries `host_id` so consumers
    /// don't need to look it up from the already-transitioned state.
    /// This session's underlying connection is no longer usable: the
    /// stream closed (EOF/error), the initialize handshake failed, or the
    /// session was explicitly deregistered while `Connected`. Signals to
    /// subscribers that they should drop any `Arc<RemoteServerClient>` they
    /// hold for this session. Carries `host_id` so consumers don't need to
    /// look it up from the already-transitioned state.
    ///
    /// Note this is about *transport* state, not manager tracking: after
    /// this event fires the session may still be present in the manager
    /// in the `Disconnected` state (e.g. when the stream dropped on its
    /// own). Use `SessionDeregistered` to observe removal from the manager.
    SessionDisconnected {
        session_id: SessionId,
        host_id: HostId,
        /// Exit status of the remote server subprocess, if available.
        /// `None` when the session was explicitly deregistered or when
        /// the exit status could not be determined.
        exit_status: Option<RemoteServerExitStatus>,
    },
    /// A reconnection attempt succeeded. Downstream owners (e.g.
    /// `RemoteServerCommandExecutor`) should swap their client reference
    /// to the new one carried in `client`.
    SessionReconnected {
        session_id: SessionId,
        host_id: HostId,
        attempt: u32,
        client: Arc<RemoteServerClient>,
    },
    /// The manager is no longer tracking this session -- it has been
    /// removed from the `sessions` map via `deregister_session`. Fires
    /// exactly once per session, and only on explicit teardown (never as
    /// a result of a spontaneous connection drop).
    ///
    /// If the session was `Connected` at the point of deregistration, a
    /// `SessionDisconnected` event is emitted first so transport-level
    /// subscribers can release their client references.
    SessionDeregistered { session_id: SessionId },

    // --- Host-scoped events ---
    /// The first session for this host reached `Connected`. Downstream
    /// features should create per-host models (e.g. `RepoMetadataModel`).
    HostConnected { host_id: HostId },
    /// The last session for this host was disconnected or deregistered.
    /// Downstream features should tear down per-host models.
    HostDisconnected { host_id: HostId },
    /// A newer full Agent Mode context snapshot published by the daemon host,
    /// accepted by `accept_remote_agent_context_snapshot` (dedup by `revision`).
    /// #438 dependent feature 1 / #353 producer / #487 SSH-arm reversal.
    /// Consumed by `app::ai::remote_agent_context::RemoteAgentContext`.
    RemoteAgentContextSnapshot {
        host_id: HostId,
        snapshot: crate::proto::RemoteAgentContextSnapshot,
    },

    // --- Repo metadata events (forwarded from ClientEvent push channel) ---
    /// Response to a `navigate_to_directory` request.
    NavigatedToDirectory {
        session_id: SessionId,
        host_id: HostId,
        indexed_path: String,
        is_git: bool,
    },
    /// A full or lazy-loaded repo metadata snapshot was pushed by the server.
    RepoMetadataSnapshot {
        host_id: HostId,
        update: RepoMetadataUpdate,
    },
    /// An incremental repo metadata update was pushed by the server.
    RepoMetadataUpdated {
        host_id: HostId,
        update: RepoMetadataUpdate,
    },
    /// A `LoadRepoMetadataDirectory` response was received from the server.
    RepoMetadataDirectoryLoaded {
        host_id: HostId,
        update: RepoMetadataUpdate,
    },
    /// A remote buffer was updated on the server (file changed on disk).
    /// Forwarded from the client's `ClientEvent::BufferUpdated` push channel
    /// so `GlobalBufferModel` can apply the incremental edits.
    BufferUpdated {
        host_id: HostId,
        path: String,
        new_server_version: u64,
        expected_client_version: u64,
        edits: Vec<crate::proto::TextEdit>,
    },
    /// A remote buffer conflict was detected on the server (the file changed
    /// on disk while the client had unsaved edits). Forwarded from the
    /// client's `ClientEvent::BufferConflictDetected` push channel so
    /// `GlobalBufferModel` can discard its pending edit batch and surface
    /// the conflict resolution banner.
    BufferConflictDetected { host_id: HostId, path: String },

    // --- Diff-state events (forwarded from ClientEvent push channel) ---
    /// A full diff-state snapshot was pushed by the server for a subscribed
    /// (repo, mode) pair. Carries the raw proto message; the `DiffStateModel`
    /// remote backend (in `app`) converts it to domain types.
    DiffStateSnapshotReceived {
        host_id: HostId,
        snapshot: crate::proto::DiffStateSnapshot,
    },
    /// A metadata-only diff-state update was pushed by the server.
    DiffStateMetadataUpdateReceived {
        host_id: HostId,
        update: crate::proto::DiffStateMetadataUpdate,
    },
    /// A single-file diff-state delta was pushed by the server.
    DiffStateFileDeltaReceived {
        host_id: HostId,
        delta: crate::proto::DiffStateFileDelta,
    },

    // --- Setup events ---
    /// Intermediate state change during the binary check/install flow.
    SetupStateChanged {
        session_id: SessionId,
        state: RemoteServerSetupState,
    },
    /// Result of [`RemoteServerManager::check_binary`]. Returns a result where:
    /// - `Ok(true)` means the binary is installed and executable,
    /// - `Ok(false)` means it is definitively not installed, and
    /// - `Err(_)` means the check itself failed (e.g. SSH error or timeout).
    BinaryCheckComplete {
        session_id: SessionId,
        result: Result<bool, String>,
        /// The detected remote platform (OS + arch) from `uname -sm`.
        /// `None` if detection failed or was not attempted.
        remote_platform: Option<RemotePlatform>,
        /// Outcome of the preinstall check script. Populated when the
        /// script ran successfully against a Linux host. `None` when the
        /// host is not Linux (the script is skipped) or when the SSH-level
        /// invocation failed (the controller treats that as inconclusive
        /// and falls open).
        preinstall_check: Option<PreinstallCheckResult>,
        /// `true` if the remote already has an existing install of the
        /// remote-server binary, detected by probing whether the install
        /// directory exists (see `RemoteTransport::check_has_old_binary`).
        /// Combined with `result == Ok(false)`, this tells the controller
        /// it should auto-install as an update instead of prompting the
        /// user. `false` when no prior install was detected, or when the
        /// detection itself failed.
        has_old_binary: bool,
    },
    /// Result of [`RemoteServerManager::install_binary`]. Returns a result where:
    /// - `Ok(())` means the install succeeded, and
    /// - `Err(_)` means the install failed and carries the failure reason (SSH error, timeout, script error, etc.).
    BinaryInstallComplete {
        session_id: SessionId,
        result: Result<(), String>,
    },

    // --- Telemetry events ---
    /// A client request to the remote server failed.
    ClientRequestFailed {
        session_id: SessionId,
        operation: RemoteServerOperation,
        error_kind: RemoteServerErrorKind,
    },
    /// A server message could not be decoded (no parseable request_id).
    ServerMessageDecodingError { session_id: SessionId },
}

impl RemoteServerManagerEvent {
    /// Returns the [`SessionId`] this event pertains to, or `None` for
    /// host-scoped variants.
    pub fn session_id(&self) -> Option<SessionId> {
        match self {
            RemoteServerManagerEvent::SessionConnecting { session_id }
            | RemoteServerManagerEvent::SessionConnected { session_id, .. }
            | RemoteServerManagerEvent::SessionConnectionFailed { session_id, .. }
            | RemoteServerManagerEvent::SessionDisconnected { session_id, .. }
            | RemoteServerManagerEvent::SessionReconnected { session_id, .. }
            | RemoteServerManagerEvent::SessionDeregistered { session_id }
            | RemoteServerManagerEvent::NavigatedToDirectory { session_id, .. }
            | RemoteServerManagerEvent::SetupStateChanged { session_id, .. }
            | RemoteServerManagerEvent::BinaryCheckComplete { session_id, .. }
            | RemoteServerManagerEvent::BinaryInstallComplete { session_id, .. }
            | RemoteServerManagerEvent::ClientRequestFailed { session_id, .. }
            | RemoteServerManagerEvent::ServerMessageDecodingError { session_id } => {
                Some(*session_id)
            }
            RemoteServerManagerEvent::HostConnected { .. }
            | RemoteServerManagerEvent::HostDisconnected { .. }
            | RemoteServerManagerEvent::RepoMetadataSnapshot { .. }
            | RemoteServerManagerEvent::RepoMetadataUpdated { .. }
            | RemoteServerManagerEvent::RepoMetadataDirectoryLoaded { .. }
            | RemoteServerManagerEvent::BufferUpdated { .. }
            | RemoteServerManagerEvent::BufferConflictDetected { .. }
            | RemoteServerManagerEvent::DiffStateSnapshotReceived { .. }
            | RemoteServerManagerEvent::DiffStateMetadataUpdateReceived { .. }
            | RemoteServerManagerEvent::DiffStateFileDeltaReceived { .. }
            // Host-scoped: the agent-context snapshot is keyed by `HostId`, not by
            // any one session on that host.
            | RemoteServerManagerEvent::RemoteAgentContextSnapshot { .. } => None,
        }
    }
}

/// Errors from [`RemoteServerManager::send_host_request`] and its typed
/// wrappers (e.g. [`RemoteServerManager::start_ripgrep_search`]).
///
/// #438 dependent features 4/5. Deviation from the pin: no `Timeout` variant
/// — this port doesn't arm a per-request timeout (the pin's
/// `schedule_host_request_timeout`/`timeout_host_request`), since nothing in
/// this fork's scope requires it yet; a request either dispatches
/// successfully and eventually resolves via a matched response, or fails
/// immediately because no session for the host is connected.
#[derive(Clone, Debug, thiserror::Error)]
pub enum HostRequestError {
    /// All sessions for the host disconnected before the request completed
    /// (including: none were ever connected).
    #[error("all sessions disconnected")]
    AllSessionsDisconnected,
    /// The server returned an error response.
    #[error("server error ({code:?}): {message}")]
    ServerError {
        code: crate::proto::ErrorCode,
        message: String,
    },
    /// The server replied with a variant that didn't match the request, or
    /// an empty result where one was required.
    #[error("unexpected response")]
    UnexpectedResponse,
    /// The request was delivered and answered, but the server reported a
    /// domain-level failure (e.g. the search itself errored). Carries the
    /// server-provided message verbatim so callers can surface it directly.
    #[error("{0}")]
    OperationFailed(String),
    /// The request was cancelled client-side via
    /// [`RemoteServerManager::abort_host_request`].
    #[error("host request aborted")]
    Aborted,
}

/// A host-scoped request registered with
/// [`RemoteServerManager::send_host_request`], awaiting a response matched
/// by `request_id` on the host-response channel drained from every
/// session's [`Connection`]. #438 dependent features 4/5.
pub struct PendingHostRequest {
    host_id: HostId,
    /// The session this request was dispatched on. Logging-only in this
    /// port: unlike the pin, a writer failure on this session doesn't retry
    /// through a sibling session (that cross-connection failover is out of
    /// #438's own scope — see the envelope's proto doc comment).
    #[allow(dead_code)]
    dispatched_session_id: SessionId,
    /// Oneshot sender to resolve the caller's future with the raw
    /// `ServerMessage` (or a `HostRequestError`). The caller parses the
    /// response variant it expects.
    result_tx: oneshot::Sender<Result<crate::proto::ServerMessage, HostRequestError>>,
}

/// Parameters for [`RemoteServerManager::start_ripgrep_search`].
pub struct RipgrepSearchParams {
    /// Effective ripgrep pattern after applying the caller's regex settings.
    pub pattern: String,
    /// Absolute roots to search on the remote host.
    pub roots: Vec<warp_util::standardized_path::StandardizedPath>,
    /// Whether matching should ignore case.
    pub ignore_case: bool,
    /// Whether matching should span multiple lines.
    pub multiline: bool,
    /// Maximum number of matches requested from the remote host.
    pub max_matches: u32,
}

/// A host-scoped remote ripgrep search registered synchronously with
/// [`RemoteServerManager`], whose typed result can be awaited off-thread.
/// #438 dependent feature 5.
///
/// Synchronous registration (the request is dispatched and, on success,
/// inserted into `pending_host_requests` before `start_ripgrep_search`
/// returns) means [`RemoteServerManager::abort_host_request`] can always
/// find the request by the time a caller has this value in hand, instead of
/// racing an asynchronously queued registration.
#[must_use = "pending remote searches must be awaited or aborted by request id"]
pub struct PendingRipgrepSearch {
    request_id: crate::protocol::RequestId,
    response: oneshot::Receiver<Result<crate::proto::ServerMessage, HostRequestError>>,
}

impl PendingRipgrepSearch {
    pub fn request_id(&self) -> &crate::protocol::RequestId {
        &self.request_id
    }

    pub async fn result(self) -> Result<crate::proto::RipgrepSearchSuccess, HostRequestError> {
        let msg = self
            .response
            .await
            .map_err(|_| HostRequestError::AllSessionsDisconnected)??;
        ripgrep_search_result(msg)
    }
}

fn ripgrep_search_result(
    msg: crate::proto::ServerMessage,
) -> Result<crate::proto::RipgrepSearchSuccess, HostRequestError> {
    match msg.message {
        Some(crate::proto::server_message::Message::RipgrepSearchResponse(resp)) => {
            match resp.result {
                Some(crate::proto::ripgrep_search_response::Result::Success(success)) => {
                    Ok(success)
                }
                Some(crate::proto::ripgrep_search_response::Result::Error(error)) => {
                    Err(HostRequestError::OperationFailed(error.message))
                }
                None => Err(HostRequestError::UnexpectedResponse),
            }
        }
        other => {
            log::error!("Unexpected response variant for RipgrepSearch: {other:?}");
            Err(HostRequestError::UnexpectedResponse)
        }
    }
}

/// Shell info recorded by [`RemoteServerManager::notify_session_bootstrapped`].
///
/// Persists for the lifetime of the session (removed only in
/// `deregister_session`) so that `mark_session_connected` can re-send
/// the notification after a reconnect.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
struct SessionBootstrapInfo {
    shell_type: String,
    shell_path: Option<String>,
}

/// Singleton model that manages connections to `remote_server` processes on
/// remote hosts.
///
/// Each SSH session gets its own `RemoteServerClient` and SSH connection.
/// Deduplication of the underlying long-lived server process happens on the
/// remote host. The `HostId` returned by the server's `InitializeResponse`
/// is used on the client to deduplicate host-scoped models (e.g.
/// `RepoMetadataModel`), not connections.
pub struct RemoteServerManager {
    /// Per-session connection state. Each SSH session gets its own dedicated
    /// connection to the remote server.
    sessions: HashMap<SessionId, RemoteSessionState>,
    /// Reverse index: host → sessions for O(1) lookup by `HostId`.
    host_to_sessions: HashMap<HostId, HashSet<SessionId>>,
    /// Spawner for running closures back on the main thread.
    spawner: ModelSpawner<Self>,
    /// Last path requested per session for dedup. Avoids redundant
    /// `navigate_to_directory` calls when `update_active_session` fires
    /// repeatedly for the same CWD.
    last_navigated_path: HashMap<SessionId, String>,
    /// Per-session shell info recorded at bootstrap time and re-sent to the
    /// remote server daemon on every (re)connect. Persists until
    /// `deregister_session`.
    session_bootstrap_info: HashMap<SessionId, SessionBootstrapInfo>,
    /// App auth context used for connection-time `Initialize` and future
    /// reconnect handshakes.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    auth_context: Option<Arc<RemoteServerAuthContext>>,
    /// Detected remote platform per session, populated during the binary check
    /// phase via `detect_platform()`. Used for telemetry.
    session_platforms: HashMap<SessionId, RemotePlatform>,
    /// User-facing connection label (e.g. `user@host`) recorded at
    /// `connect_session` time and cleared on `deregister_session`. Exposed via
    /// `host_label` for callers that want to display a friendlier name than
    /// the raw `HostId`.
    session_labels: HashMap<SessionId, String>,
    /// In-flight host-scoped requests dispatched via `send_host_request`,
    /// keyed by request id, awaiting a response drained off
    /// `host_response_rx`. #438 dependent features 4/5.
    pending_host_requests: HashMap<crate::protocol::RequestId, PendingHostRequest>,
    /// Last accepted `RemoteAgentContextSnapshot` per host, used by
    /// `accept_remote_agent_context_snapshot_revision` to drop stale/
    /// duplicate pushes and queried via `remote_agent_context_snapshot()`.
    /// #438 dependent feature 1.
    remote_agent_context_snapshots: HashMap<HostId, crate::proto::RemoteAgentContextSnapshot>,
}

impl Entity for RemoteServerManager {
    type Event = RemoteServerManagerEvent;
}

impl SingletonEntity for RemoteServerManager {}

impl RemoteServerManager {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        Self {
            sessions: HashMap::new(),
            host_to_sessions: HashMap::new(),
            spawner: ctx.spawner(),
            last_navigated_path: HashMap::new(),
            session_bootstrap_info: HashMap::new(),
            auth_context: None,
            session_platforms: HashMap::new(),
            session_labels: HashMap::new(),
            pending_host_requests: HashMap::new(),
            remote_agent_context_snapshots: HashMap::new(),
        }
    }

    /// Returns the user-facing connection label for a connected host, if one
    /// has been recorded on any active session for that host.
    pub fn host_label(&self, host_id: &HostId) -> Option<&str> {
        self.host_to_sessions
            .get(host_id)?
            .iter()
            .find_map(|session_id| self.session_labels.get(session_id).map(String::as_str))
    }

    /// Returns a connected client for the given host by picking an arbitrary
    /// session from the host's session pool.
    pub fn client_for_host(&self, host_id: &HostId) -> Option<&Arc<RemoteServerClient>> {
        let sessions = self.host_to_sessions.get(host_id)?;
        sessions
            .iter()
            .find_map(|session_id| self.client_for_session(*session_id))
    }

    /// Checks if the remote server binary is installed and executable.
    /// Emits `BinaryCheckComplete { result }`.
    ///
    /// Returns Ok(true) if the binary is installed and executable,
    /// Ok(false) if it is definitively not installed, and
    /// Err(_) if the check failed (e.g. SSH timeout/unreachable).
    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    pub fn check_binary<T>(
        &mut self,
        session_id: SessionId,
        transport: T,
        ctx: &mut ModelContext<Self>,
    ) where
        T: RemoteTransport + 'static,
    {
        #[cfg(target_family = "wasm")]
        {
            log::warn!("check_binary is a no-op on WASM");
        }

        #[cfg(not(target_family = "wasm"))]
        {
            ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
                session_id,
                state: RemoteServerSetupState::Checking,
            });
            let spawner = self.spawner.clone();
            ctx.background_executor()
                .spawn(async move {
                    // Run platform detection, binary check, and old-binary
                    // check concurrently. The old-binary check lets the
                    // controller distinguish fresh install (no prior
                    // versioned binary) from update (prior versioned
                    // binary present), so it can skip the install prompt
                    // in the update case.
                    let (platform_result, check_result, old_binary_result) = futures::join!(
                        transport.detect_platform(),
                        transport.check_binary(),
                        transport.check_has_old_binary(),
                    );
                    let platform = match platform_result {
                        Ok(p) => Some(p),
                        Err(e) => {
                            log::warn!("Platform detection failed for session {session_id:?}: {e}");
                            None
                        }
                    };
                    let has_old_binary = match old_binary_result {
                        Ok(has) => has,
                        Err(e) => {
                            log::warn!(
                                "Old-binary detection failed for session {session_id:?}: {e}. \
                                 Treating as fresh install."
                            );
                            false
                        }
                    };
                    // Run the preinstall check after platform detection
                    // resolves, only on Linux. macOS hosts pay zero extra
                    // round-trips. SSH-level failures are logged and
                    // surfaced as `None`, which the controller treats as
                    // inconclusive (fail open).
                    let preinstall = match &platform {
                        Some(p) if matches!(p.os, RemoteOs::Linux) => {
                            match transport.run_preinstall_check().await {
                                Ok(r) => Some(r),
                                Err(e) => {
                                    log::warn!(
                                        "Preinstall check failed for session {session_id:?}: {e}"
                                    );
                                    None
                                }
                            }
                        }
                        _ => None,
                    };
                    let _ = spawner
                        .spawn(move |me, ctx| {
                            if let Some(p) = &platform {
                                me.session_platforms.insert(session_id, p.clone());
                            }
                            if let Err(error) = &check_result {
                                ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
                                    session_id,
                                    state: RemoteServerSetupState::Failed {
                                        error: error.clone(),
                                    },
                                });
                            }
                            ctx.emit(RemoteServerManagerEvent::BinaryCheckComplete {
                                session_id,
                                result: check_result,
                                remote_platform: platform,
                                preinstall_check: preinstall,
                                has_old_binary,
                            });
                        })
                        .await;
                })
                .detach();
        }
    }

    /// Marks a session as unsupported by the prebuilt remote-server
    /// binary, based on a positive classification from the preinstall
    /// check. The setup state transitions to `Unsupported`, which the
    /// downstream UI treats as a clean fall-back to the legacy SSH flow.
    ///
    /// No-op on WASM (remote server connections use a different transport).
    #[cfg(target_family = "wasm")]
    pub fn mark_setup_unsupported(
        &mut self,
        _session_id: SessionId,
        _reason: UnsupportedReason,
        _ctx: &mut ModelContext<Self>,
    ) {
        log::warn!("mark_setup_unsupported is a no-op on WASM");
    }

    /// Marks a session as unsupported by the prebuilt remote-server
    /// binary, based on a positive classification from the preinstall
    /// check. The setup state transitions to `Unsupported`, which the
    /// downstream UI treats as a clean fall-back to the legacy SSH flow.
    #[cfg(not(target_family = "wasm"))]
    pub fn mark_setup_unsupported(
        &mut self,
        session_id: SessionId,
        reason: UnsupportedReason,
        ctx: &mut ModelContext<Self>,
    ) {
        ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
            session_id,
            state: RemoteServerSetupState::Unsupported { reason },
        });
    }

    /// Installs the remote server binary.
    /// Emits `BinaryInstallComplete { result }`.
    ///
    /// Returns Ok(()) if the install succeeded, and
    /// Err(_) if the install failed (e.g. SSH timeout/unreachable).
    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    pub fn install_binary<T>(
        &mut self,
        session_id: SessionId,
        transport: T,
        is_update: bool,
        ctx: &mut ModelContext<Self>,
    ) where
        T: RemoteTransport + 'static,
    {
        #[cfg(target_family = "wasm")]
        {
            log::warn!("install_binary is a no-op on WASM");
        }

        #[cfg(not(target_family = "wasm"))]
        {
            let setup_state = if is_update {
                RemoteServerSetupState::Updating
            } else {
                RemoteServerSetupState::Installing {
                    progress_percent: None,
                }
            };
            ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
                session_id,
                state: setup_state,
            });
            let spawner = self.spawner.clone();
            ctx.background_executor()
                .spawn(async move {
                    let result = transport.install_binary().await;
                    let _ = spawner
                        .spawn(move |_me, ctx| {
                            if let Err(error) = &result {
                                ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
                                    session_id,
                                    state: RemoteServerSetupState::Failed {
                                        error: error.clone(),
                                    },
                                });
                            }
                            ctx.emit(RemoteServerManagerEvent::BinaryInstallComplete {
                                session_id,
                                result,
                            });
                        })
                        .await;
                })
                .detach();
        }
    }

    /// Entry point for establishing a remote server connection for a session.
    /// This assumes the binary is already installed and executable.
    /// Callers should first call `check_binary` and `install_binary` to ensure the binary is present.
    ///
    /// The full flow is:
    /// 1. **Connect** — `transport.connect()` establishes the I/O streams and
    ///    creates the `RemoteServerClient`.
    /// 2. **Handshake** — perform the initialize handshake (which returns the
    ///    `HostId`) and transition to `Connected`.
    ///
    /// No-op on WASM (remote server connections use a different transport).
    #[cfg_attr(target_family = "wasm", allow(unused_variables, unused_mut))]
    pub fn connect_session<T>(
        &mut self,
        session_id: SessionId,
        transport: T,
        auth_context: Arc<RemoteServerAuthContext>,
        connection_label: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) where
        T: RemoteTransport + 'static,
    {
        #[cfg(target_family = "wasm")]
        {
            log::warn!("connect_session is a no-op on WASM");
        }

        #[cfg(not(target_family = "wasm"))]
        {
            log::info!("Starting remote server connection for session {session_id:?}");

            // Advance the user-visible setup pipeline.
            ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
                session_id,
                state: RemoteServerSetupState::Initializing,
            });

            self.sessions
                .insert(session_id, RemoteSessionState::Connecting);
            if let Some(connection_label) = connection_label {
                self.session_labels.insert(session_id, connection_label);
            }
            self.auth_context = Some(Arc::clone(&auth_context));
            ctx.emit(RemoteServerManagerEvent::SessionConnecting { session_id });

            let spawner = self.spawner.clone();
            let executor = ctx.background_executor().clone();
            // Wrap the transport in an Arc so it can be stored on `Connected`
            // for reconnection after a spontaneous disconnect.
            let transport: Arc<dyn RemoteTransport> = Arc::new(transport);
            let auth_context_for_task = Arc::clone(&auth_context);
            // Capture the identity key synchronously so it travels with the
            // session and can be used to filter token-rotation notifications.
            let identity_key = auth_context.remote_server_identity_key();

            ctx.background_executor()
                .spawn(async move {
                    match Self::run_connect_and_handshake(
                        session_id,
                        &*transport,
                        &auth_context_for_task,
                        &spawner,
                        &executor,
                    )
                    .await
                    {
                        Ok(host_id) => {
                            let _ = spawner
                                .spawn(move |me, ctx| {
                                    me.mark_session_connected(
                                        session_id,
                                        host_id,
                                        identity_key,
                                        transport,
                                        ctx,
                                    );
                                })
                                .await;
                        }
                        Err(e) => {
                            log::error!("Connection failed for session {session_id:?}: {e}");
                            let phase = e.phase();
                            let error = format!("{e}");
                            let _ = spawner
                                .spawn(move |me, ctx| {
                                    ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
                                        session_id,
                                        state: RemoteServerSetupState::Failed {
                                            error: error.clone(),
                                        },
                                    });
                                    ctx.emit(RemoteServerManagerEvent::SessionConnectionFailed {
                                        session_id,
                                        phase,
                                        error,
                                    });
                                    me.mark_session_disconnected(session_id, ctx);
                                })
                                .await;
                        }
                    }
                })
                .detach();
        }
    }

    /// Shared connect + handshake logic used by both `connect_session` and
    /// `attempt_reconnect`.
    ///
    /// 1. Calls `transport.connect()` to establish streams.
    /// 2. Transitions the session to `Initializing` and starts draining the
    ///    event channel.
    /// 3. Runs the initialize handshake with the current auth token, if any.
    ///
    /// Returns `Ok(host_id)` on success, or a phase-tagged error.
    #[cfg(not(target_family = "wasm"))]
    async fn run_connect_and_handshake(
        session_id: SessionId,
        transport: &dyn RemoteTransport,
        auth_context: &RemoteServerAuthContext,
        spawner: &ModelSpawner<Self>,
        executor: &Arc<warpui::r#async::executor::Background>,
    ) -> Result<HostId, ConnectAndHandshakeError> {
        // Phase 1: Connect (establish streams, create client).
        let Connection {
            client,
            event_rx,
            host_response_rx,
            child,
            control_path,
        } = transport
            .connect(executor.clone())
            .await
            .map_err(ConnectAndHandshakeError::Connect)?;

        let client = Arc::new(client);
        let client_for_init = Arc::clone(&client);

        // Transition to Initializing and start draining the event channel.
        // Guard: if the session was deregistered during `transport.connect()`,
        // the entry will have been removed; don't re-insert it.
        let was_inserted = spawner
            .spawn(move |me, ctx| {
                if !me.sessions.contains_key(&session_id) {
                    return false;
                }
                me.sessions.insert(
                    session_id,
                    RemoteSessionState::Initializing {
                        client: client_for_init,
                        _child: child,
                        control_path,
                    },
                );

                ctx.spawn_stream_local(
                    event_rx,
                    move |me, event, ctx| {
                        me.forward_client_event(session_id, event, ctx);
                    },
                    move |me, ctx| {
                        me.mark_session_disconnected(session_id, ctx);
                    },
                );

                // Drain responses to host-scoped requests dispatched via
                // `send_host_scoped` (matched against `pending_host_requests`
                // by request_id). #438 dependent features 4/5.
                ctx.spawn_stream_local(
                    host_response_rx,
                    move |me, msg, _ctx| {
                        me.resolve_host_response(msg);
                    },
                    move |_me, _ctx| {},
                );
                true
            })
            .await
            .unwrap_or(false);

        if !was_inserted {
            return Err(ConnectAndHandshakeError::Connect(anyhow::anyhow!(
                "Session {session_id:?} was deregistered during connect"
            )));
        }

        // Phase 2: Initialize handshake.
        let auth_token = auth_context.get_auth_token().await;
        let resp = client
            .initialize(auth_token.as_deref())
            .await
            .map_err(|e| ConnectAndHandshakeError::Initialize(anyhow::anyhow!("{e:#}")))?;

        // Version compatibility check. If the server reports a different release
        // tag than the client expects, the binary on disk is stale. Remove it so
        // the next reconnect (or explicit reconnect by the user) will reinstall.
        //
        // Under `Channel::Oss` (Zap), the official release binary is
        // temporarily reused, but the client itself has no `GIT_RELEASE_TAG`
        // and would never match the server, so strict checking is skipped.
        // See the comment on [`should_enforce_remote_version_check`] for details.
        let client_version = ChannelState::app_version();
        let enforce_version_check = should_enforce_remote_version_check(ChannelState::channel());
        if enforce_version_check && !version_is_compatible(client_version, &resp.server_version) {
            log::warn!(
                "Remote server version mismatch for session {session_id:?}: \
                 client={client_version:?}, server={:?}. Removing stale binary.",
                resp.server_version
            );

            const REMOVAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

            if let Err(e) = transport
                .remove_remote_server_binary()
                .with_timeout(REMOVAL_TIMEOUT)
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("timed out after {REMOVAL_TIMEOUT:?}")))
            {
                log::warn!("Failed to remove stale remote binary for session {session_id:?}: {e}");
            }
            return Err(ConnectAndHandshakeError::Initialize(anyhow::anyhow!(
                "remote server version mismatch (client: {client_version:?}, \
                 server: {:?}); reconnect to reinstall",
                resp.server_version
            )));
        }

        Ok(HostId::new(resp.host_id))
    }

    /// Removes a session from the manager and tears down its connection.
    ///
    /// Assumes the caller has already observed that the user's shell
    /// has exited (in practice this is only invoked from the
    /// `ExitShell` teardown path). Under that assumption we also force
    /// the local SSH `ControlMaster` to exit immediately via
    /// `ssh -O exit`, which is required because the master is the
    /// user's interactive ssh process and, without the explicit
    /// `-O exit`, it hangs waiting for remote-side cleanup of
    /// multiplexed channels (see [`crate::ssh::stop_control_master`]).
    ///
    /// Mechanically:
    /// 1. Remove the session entry. Dropping the `RemoteSessionState`
    ///    drops the transport's owned `Child`, which SIGKILLs the
    ///    `ssh … remote-server-proxy` subprocess via `kill_on_drop`.
    /// 2. If the session had a ControlMaster `control_path`, spawn a
    ///    background task that runs `ssh -O exit` against it.
    ///
    /// The `Child` is owned by the manager's state, *not* by
    /// `Arc<RemoteServerClient>`. Lingering `Arc` clones held elsewhere
    /// (e.g. by the per-session command executor) do *not* keep the
    /// subprocess alive -- removing the state here always SIGKILLs the
    /// child, regardless of client refcount.
    ///
    /// Two separate events can fire here, and they mean different things:
    ///
    /// * `SessionDisconnected` -- the *transport* went away. Emitted only
    ///   when the session was `Connected` at the time of deregistration.
    ///   Subscribers can use this to drop their
    ///   `Arc<RemoteServerClient>` references and cancel in-flight
    ///   requests. The same event also fires independently from
    ///   `mark_session_disconnected` when the stream drops on its own.
    /// * `SessionDeregistered` -- the manager is no longer *tracking* this
    ///   session. Always emitted, regardless of which state the session
    ///   was in, because the entry is being removed from `sessions`
    ///   outright. Unlike `SessionDisconnected`, this one never fires for
    ///   spontaneous drops -- only for explicit teardown.
    pub fn deregister_session(&mut self, session_id: SessionId, ctx: &mut ModelContext<Self>) {
        self.last_navigated_path.remove(&session_id);
        self.session_bootstrap_info.remove(&session_id);
        self.session_platforms.remove(&session_id);
        self.session_labels.remove(&session_id);

        // Remove the session entry. Dropping the `RemoteSessionState`
        // here drops the transport's owned `Child` (if any), which
        // SIGKILLs the `ssh … remote-server-proxy` subprocess via
        // `kill_on_drop`.
        let prev = self.sessions.remove(&session_id);

        // Extract the ControlMaster socket path (if any) so we can
        // force the master to exit below. Safe to do under the
        // "caller already observed ExitShell" assumption documented
        // above.
        #[cfg(not(target_family = "wasm"))]
        let control_path = match &prev {
            Some(RemoteSessionState::Connected { control_path, .. })
            | Some(RemoteSessionState::Initializing { control_path, .. }) => control_path.clone(),
            Some(RemoteSessionState::Reconnecting { control_path, .. }) => control_path.clone(),
            _ => None,
        };

        // Extract `host_id` from states that track a host connection.
        let host_id = match &prev {
            Some(RemoteSessionState::Connected { host_id, .. }) => Some(host_id.clone()),
            #[cfg(not(target_family = "wasm"))]
            Some(RemoteSessionState::Reconnecting { host_id, .. }) => Some(host_id.clone()),
            _ => None,
        };
        if let Some(host_id) = host_id {
            self.remove_from_host_index(&host_id, session_id);
            ctx.emit(RemoteServerManagerEvent::SessionDisconnected {
                session_id,
                host_id: host_id.clone(),
                exit_status: None,
            });
            if !self.host_to_sessions.contains_key(&host_id) {
                self.handle_host_disconnected(host_id, ctx);
            }
        }
        ctx.emit(RemoteServerManagerEvent::SessionDeregistered { session_id });

        // Force the local SSH ControlMaster to exit after teardown.
        // Spawned detached because the ssh subcommand may take a moment
        // to complete and we don't want to block the main thread on it.
        #[cfg(not(target_family = "wasm"))]
        if let Some(control_path) = control_path {
            ctx.background_executor()
                .spawn(async move {
                    crate::ssh::stop_control_master(&control_path).await;
                })
                .detach();
        }
    }

    /// Returns the client for this session, if connected.
    pub fn client_for_session(&self, session_id: SessionId) -> Option<&Arc<RemoteServerClient>> {
        match self.sessions.get(&session_id) {
            Some(RemoteSessionState::Connected { client, .. }) => Some(client),
            _ => None,
        }
    }

    /// Rotates the daemon-wide auth credential on each connected remote host.
    ///
    /// Only sessions whose stored `identity_key` matches the current identity
    /// (from `auth_context`) receive the notification. This prevents a stale
    /// session established under a previous user identity from receiving a
    /// newly-rotated bearer token that belongs to a different user.
    ///
    /// Within the matching identity, a daemon may have multiple client
    /// connections. The credential is stored daemon-wide, so sending one
    /// notification per connected host is sufficient.
    pub fn rotate_auth_token(&self, token: String) {
        let Some(ref auth_context) = self.auth_context else {
            log::warn!("rotate_auth_token: no auth_context available, skipping");
            return;
        };
        let current_identity_key = auth_context.remote_server_identity_key();
        let mut authenticated_hosts = HashSet::new();
        for state in self.sessions.values() {
            let RemoteSessionState::Connected {
                client,
                host_id,
                identity_key,
                ..
            } = state
            else {
                continue;
            };
            if identity_key != &current_identity_key {
                continue;
            }
            if authenticated_hosts.insert(host_id.clone()) {
                client.authenticate(&token);
            }
        }
    }

    /// Returns the connection state for this session.
    pub fn session(&self, session_id: SessionId) -> Option<&RemoteSessionState> {
        self.sessions.get(&session_id)
    }

    /// Returns `true` when the session exists and is in a state where the
    /// remote server might still deliver data (`Connecting`, `Initializing`,
    /// `Connected`, or `Reconnecting`). Returns `false` for `Disconnected`
    /// sessions and sessions not tracked by the manager.
    pub fn is_session_potentially_active(&self, session_id: SessionId) -> bool {
        match self.sessions.get(&session_id) {
            Some(RemoteSessionState::Disconnected) | None => false,
            Some(
                RemoteSessionState::Connecting
                | RemoteSessionState::Initializing { .. }
                | RemoteSessionState::Connected { .. },
            ) => true,
            #[cfg(not(target_family = "wasm"))]
            Some(RemoteSessionState::Reconnecting { .. }) => true,
        }
    }

    /// Returns the detected remote platform for this session, if available.
    pub fn platform_for_session(&self, session_id: SessionId) -> Option<&RemotePlatform> {
        self.session_platforms.get(&session_id)
    }

    /// Returns the `HostId` for this session, if the initialize handshake
    /// has completed. Downstream features use this to deduplicate
    /// host-scoped models (e.g. `RepoMetadataModel`).
    pub fn host_id_for_session(&self, session_id: SessionId) -> Option<&HostId> {
        match self.sessions.get(&session_id) {
            Some(RemoteSessionState::Connected { host_id, .. }) => Some(host_id),
            _ => None,
        }
    }

    /// Returns all session IDs connected to a given host. O(1) via the
    /// reverse index.
    pub fn sessions_for_host(&self, host_id: &HostId) -> Option<&HashSet<SessionId>> {
        self.host_to_sessions.get(host_id)
    }

    /// Sends a `NavigatedToDirectory` request to the remote server for
    /// the given session and emits the response as a manager event.
    ///
    /// Deduplicates: if the same `(session_id, path)` was already requested,
    /// the call is a no-op.
    pub fn navigate_to_directory(
        &mut self,
        session_id: SessionId,
        path: String,
        ctx: &mut ModelContext<Self>,
    ) {
        // Dedup: skip if this session already navigated to the same path.
        if self.last_navigated_path.get(&session_id) == Some(&path) {
            return;
        }

        let Some(client) = self.client_for_session(session_id).cloned() else {
            log::warn!("navigate_to_directory: no connected client for session {session_id:?}");
            return;
        };
        let Some(host_id) = self.host_id_for_session(session_id).cloned() else {
            log::warn!("navigate_to_directory: no host_id for session {session_id:?}");
            return;
        };

        // Record only after confirming the client is connected, so that a
        // retry after SessionConnected is not incorrectly deduplicated.
        self.last_navigated_path.insert(session_id, path.clone());

        let spawner = self.spawner.clone();
        ctx.background_executor()
            .spawn(async move {
                match client.navigate_to_directory(path).await {
                    Ok(resp) => {
                        let _ = spawner
                            .spawn(move |_me, ctx| {
                                ctx.emit(RemoteServerManagerEvent::NavigatedToDirectory {
                                    session_id,
                                    host_id,
                                    indexed_path: resp.indexed_path,
                                    is_git: resp.is_git,
                                });
                            })
                            .await;
                    }
                    Err(e) => {
                        log::error!("navigate_to_directory failed for session {session_id:?}: {e}");
                        let error_kind = RemoteServerErrorKind::from_client_error(&e);
                        let _ = spawner
                            .spawn(move |_me, ctx| {
                                ctx.emit(RemoteServerManagerEvent::ClientRequestFailed {
                                    session_id,
                                    operation: RemoteServerOperation::NavigateToDirectory,
                                    error_kind,
                                });
                            })
                            .await;
                    }
                }
            })
            .detach();
    }

    /// Sends a `SessionBootstrapped` notification to the remote server.
    ///
    /// If the session is already in `Connected` state the notification is sent
    /// immediately. Otherwise it is stashed and automatically flushed when
    /// `mark_session_connected` transitions the session to `Connected`.
    pub fn notify_session_bootstrapped(
        &mut self,
        session_id: SessionId,
        shell_type: &str,
        shell_path: Option<&str>,
    ) {
        // Always persist so we can re-send after a reconnect.
        self.session_bootstrap_info.insert(
            session_id,
            SessionBootstrapInfo {
                shell_type: shell_type.to_owned(),
                shell_path: shell_path.map(ToOwned::to_owned),
            },
        );

        if let Some(client) = self.client_for_session(session_id) {
            client.notify_session_bootstrapped(session_id, shell_type, shell_path);
        } else {
            log::info!(
                "notify_session_bootstrapped: session {session_id:?} not yet connected, \
                 will send on connect"
            );
        }
    }

    /// Sends a `LoadRepoMetadataDirectory` request to the remote server for
    /// the given session and emits the response as a manager event.
    pub fn load_remote_repo_metadata_directory(
        &mut self,
        session_id: SessionId,
        repo_path: String,
        dir_path: String,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(client) = self.client_for_session(session_id).cloned() else {
            log::warn!(
                "load_remote_repo_metadata_directory: no connected client for session {session_id:?}"
            );
            return;
        };
        let Some(host_id) = self.host_id_for_session(session_id).cloned() else {
            log::warn!(
                "load_remote_repo_metadata_directory: no host_id for session {session_id:?}"
            );
            return;
        };

        let spawner = self.spawner.clone();
        ctx.background_executor()
            .spawn(async move {
                match client
                    .load_repo_metadata_directory(repo_path, dir_path)
                    .await
                {
                    Ok(resp) => {
                        if let Some(update) =
                            crate::repo_metadata_proto::proto_load_repo_metadata_directory_response_to_update(&resp)
                        {
                            let _ = spawner
                                .spawn(move |_me, ctx| {
                                    ctx.emit(
                                        RemoteServerManagerEvent::RepoMetadataDirectoryLoaded {
                                            host_id,
                                            update,
                                        },
                                    );
                                })
                                .await;
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "load_repo_metadata_directory failed for session {session_id:?}: {e}"
                        );
                        let error_kind = RemoteServerErrorKind::from_client_error(&e);
                        let _ = spawner
                            .spawn(move |_me, ctx| {
                                ctx.emit(RemoteServerManagerEvent::ClientRequestFailed {
                                    session_id,
                                    operation: RemoteServerOperation::LoadRepoMetadataDirectory,
                                    error_kind,
                                });
                            })
                            .await;
                    }
                }
            })
            .detach();
    }

    /// Forwards a push event from the client event channel as a manager event.
    /// No-ops if the session is not in `Connected` state (i.e. `host_id` not
    /// yet available).
    #[cfg(not(target_family = "wasm"))]
    fn forward_client_event(
        &mut self,
        session_id: SessionId,
        event: ClientEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(host_id) = self.host_id_for_session(session_id) else {
            log::debug!("Dropping push event for session {session_id:?}: not connected yet");
            return;
        };
        let host_id = host_id.clone();

        match event {
            ClientEvent::RepoMetadataSnapshotReceived { update } => {
                ctx.emit(RemoteServerManagerEvent::RepoMetadataSnapshot { host_id, update });
            }
            ClientEvent::RepoMetadataUpdated { update } => {
                ctx.emit(RemoteServerManagerEvent::RepoMetadataUpdated { host_id, update });
            }
            ClientEvent::BufferUpdated {
                path,
                new_server_version,
                expected_client_version,
                edits,
            } => {
                ctx.emit(RemoteServerManagerEvent::BufferUpdated {
                    host_id,
                    path,
                    new_server_version,
                    expected_client_version,
                    edits,
                });
            }
            ClientEvent::BufferConflictDetected { path } => {
                ctx.emit(RemoteServerManagerEvent::BufferConflictDetected { host_id, path });
            }
            ClientEvent::DiffStateSnapshotReceived { snapshot } => {
                ctx.emit(RemoteServerManagerEvent::DiffStateSnapshotReceived { host_id, snapshot });
            }
            ClientEvent::DiffStateMetadataUpdateReceived { update } => {
                ctx.emit(RemoteServerManagerEvent::DiffStateMetadataUpdateReceived {
                    host_id,
                    update,
                });
            }
            ClientEvent::DiffStateFileDeltaReceived { delta } => {
                ctx.emit(RemoteServerManagerEvent::DiffStateFileDeltaReceived { host_id, delta });
            }
            ClientEvent::RemoteAgentContextSnapshotReceived { snapshot } => {
                // #438 dependent feature 1 / #487: dedup by revision per host,
                // store for the `remote_agent_context_snapshot()` query method,
                // and surface the accepted snapshot so
                // `app::ai::remote_agent_context::RemoteAgentContext` can
                // reconcile it into SkillManager's per-host catalogs.
                if self.accept_remote_agent_context_snapshot(&host_id, snapshot.clone()) {
                    ctx.emit(RemoteServerManagerEvent::RemoteAgentContextSnapshot {
                        host_id,
                        snapshot,
                    });
                }
            }
            ClientEvent::HostScopedDecodeFailed { request_id } => {
                // #438 dependent feature 3/4: the response for a host-scoped
                // request the manager is tracking couldn't be decoded — fail
                // it now instead of leaving it pending indefinitely.
                self.fail_host_request_decode_error(&request_id);
            }
            ClientEvent::MessageDecodingError => {
                ctx.emit(RemoteServerManagerEvent::ServerMessageDecodingError { session_id });
            }
            ClientEvent::Disconnected => {
                // Handled by the drain loop's completion callback.
            }
        }
    }

    /// Transitions a session from `Initializing` to `Connected`. Stores the
    /// `transport` for reconnection support after a spontaneous disconnect.
    #[cfg(not(target_family = "wasm"))]
    fn mark_session_connected(
        &mut self,
        session_id: SessionId,
        host_id: HostId,
        identity_key: String,
        transport: Arc<dyn RemoteTransport>,
        ctx: &mut ModelContext<Self>,
    ) {
        log::info!("Remote server connected for session {session_id:?}, host {host_id}");

        // Only transition if the session is still in Initializing state.
        let Some(RemoteSessionState::Initializing {
            client,
            _child,
            control_path,
        }) = self.sessions.remove(&session_id)
        else {
            return;
        };

        let is_first_session = !self.host_to_sessions.contains_key(&host_id);
        self.sessions.insert(
            session_id,
            RemoteSessionState::Connected {
                client: client.clone(),
                host_id: host_id.clone(),
                identity_key,
                _child,
                control_path,
                transport,
            },
        );
        self.host_to_sessions
            .entry(host_id.clone())
            .or_default()
            .insert(session_id);
        if is_first_session {
            ctx.emit(RemoteServerManagerEvent::HostConnected {
                host_id: host_id.clone(),
            });
        }
        ctx.emit(RemoteServerManagerEvent::SetupStateChanged {
            session_id,
            state: RemoteServerSetupState::Ready,
        });
        ctx.emit(RemoteServerManagerEvent::SessionConnected {
            session_id,
            host_id,
        });

        // (Re-)send the SessionBootstrapped notification so the daemon
        // registers an executor for this session. This fires on both the
        // initial connect and every reconnect.
        if let Some(info) = self.session_bootstrap_info.get(&session_id) {
            if let Some(client) = self.client_for_session(session_id) {
                log::info!("Sending SessionBootstrapped notification for session {session_id:?}");
                client.notify_session_bootstrapped(
                    session_id,
                    &info.shell_type,
                    info.shell_path.as_deref(),
                );
            }
        }
    }

    /// Captures the exit status from a `Child` process, if available.
    #[cfg(not(target_family = "wasm"))]
    fn capture_exit_status(
        child: &mut async_process::Child,
        session_id: SessionId,
    ) -> Option<RemoteServerExitStatus> {
        match child.try_status() {
            Ok(Some(status)) => {
                let code = status.code();
                #[cfg(unix)]
                let signal_killed = {
                    use std::os::unix::process::ExitStatusExt;
                    status.signal().is_some()
                };
                #[cfg(not(unix))]
                let signal_killed = false;
                log::warn!(
                    "Remote server process exited for session {session_id:?}: \
                     code={code:?}, signal_killed={signal_killed}"
                );
                Some(RemoteServerExitStatus {
                    code,
                    signal_killed,
                })
            }
            Ok(None) => {
                log::warn!(
                    "Remote server process still running for session {session_id:?} \
                     despite EOF on reader task"
                );
                None
            }
            Err(e) => {
                log::warn!("Failed to read exit status for session {session_id:?}: {e}");
                None
            }
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn mark_session_disconnected(
        &mut self,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(prev) = self.sessions.remove(&session_id) else {
            return;
        };

        // Only attempt reconnect for sessions that were in Connected state
        // with a transport available, and not being explicitly deregistered.
        if let RemoteSessionState::Connected {
            host_id,
            identity_key,
            mut _child,
            control_path,
            transport,
            ..
        } = prev
        {
            let exit_status = Self::capture_exit_status(&mut _child, session_id);
            // Drop the old child process explicitly before reconnecting.
            drop(_child);
            // If the child process has already exited (`exit_status.is_some()`),
            // it means the remote daemon is really gone — e.g. the user typed
            // `exit` in the remote shell, and ssh ControlMaster tore down all
            // slave channels together, so `remote-server-proxy` exited too. In
            // this case, an immediate reconnect will most likely fail and would
            // instead make the terminal pane hang for 2-4 seconds before fully
            // ending; so we skip auto-reconnect and go straight to the
            // Disconnected path. Only `exit_status.is_none()` (child still
            // running but the reader got EOF) — a "true transient network
            // blip" — keeps the reconnect logic.
            let child_already_exited = exit_status.is_some();
            let Some(auth_context) = self.auth_context.clone().filter(|_| !child_already_exited)
            else {
                if child_already_exited {
                    log::info!(
                        "Spontaneous disconnect for session {session_id:?}: \
                         child already exited (exit_status={exit_status:?}), \
                         skipping reconnect"
                    );
                } else {
                    log::warn!(
                        "Spontaneous disconnect for session {session_id:?}, \
                         but no auth context is available for reconnect"
                    );
                }
                self.sessions
                    .insert(session_id, RemoteSessionState::Disconnected);
                self.remove_from_host_index(&host_id, session_id);
                ctx.emit(RemoteServerManagerEvent::SessionDisconnected {
                    session_id,
                    host_id: host_id.clone(),
                    exit_status,
                });
                if !self.host_to_sessions.contains_key(&host_id) {
                    self.handle_host_disconnected(host_id, ctx);
                }
                return;
            };
            log::info!(
                "Spontaneous disconnect for session {session_id:?}, \
                 will attempt reconnect (transport={transport:?})"
            );

            // Clear stale repo metadata and host index so downstream
            // models don't hold onto data from the dead server process.
            self.remove_from_host_index(&host_id, session_id);
            if !self.host_to_sessions.contains_key(&host_id) {
                self.handle_host_disconnected(host_id.clone(), ctx);
            }

            // Clear last navigated path so navigate_to_directory
            // re-fires after reconnect.
            // We need to do this on disconnect because the cached
            // navigated path is only deduping for the current _remote server session.
            self.last_navigated_path.remove(&session_id);

            self.attempt_reconnect(
                session_id,
                ReconnectParams {
                    attempt: 1,
                    host_id,
                    exit_status,
                    transport,
                    auth_context,
                    control_path,
                    identity_key,
                },
                ctx,
            );
        } else {
            // Non-Connected states (Initializing, Connecting, etc.) —
            // no reconnect, just mark disconnected.
            self.sessions
                .insert(session_id, RemoteSessionState::Disconnected);
        }
    }

    /// Attempt to re-establish the remote server connection.
    #[cfg(not(target_family = "wasm"))]
    fn attempt_reconnect(
        &mut self,
        session_id: SessionId,
        params: ReconnectParams,
        ctx: &mut ModelContext<Self>,
    ) {
        let ReconnectParams {
            attempt,
            host_id,
            exit_status,
            transport,
            auth_context,
            control_path,
            identity_key,
        } = params;

        log::info!(
            "Attempting reconnect for session {session_id:?} \
             (attempt {attempt}/{MAX_RECONNECT_ATTEMPTS})"
        );

        self.sessions.insert(
            session_id,
            RemoteSessionState::Reconnecting {
                attempt,
                host_id: host_id.clone(),
                control_path: control_path.clone(),
            },
        );

        let spawner = self.spawner.clone();
        let executor = ctx.background_executor().clone();
        let transport_clone = Arc::clone(&transport);
        let auth_context_for_task = Arc::clone(&auth_context);

        ctx.background_executor()
            .spawn(async move {
                async_io::Timer::after(RECONNECT_DELAY).await;

                // Check if the session was deregistered during the delay.
                // (Checked via spawner since sessions lives on the main thread.)
                let was_removed = spawner
                    .spawn(move |me, _ctx| !me.sessions.contains_key(&session_id))
                    .await
                    .unwrap_or(true);
                if was_removed {
                    log::info!("Session {session_id:?} removed during reconnect delay, aborting");
                    return;
                }

                match Self::run_connect_and_handshake(
                    session_id,
                    &*transport_clone,
                    &auth_context_for_task,
                    &spawner,
                    &executor,
                )
                .await
                {
                    Ok(new_host_id) => {
                        let _ = spawner
                            .spawn(move |me, ctx| {
                                // If the session was deregistered during the
                                // handshake, don't resurrect it.
                                if !me.sessions.contains_key(&session_id) {
                                    log::info!(
                                        "Session {session_id:?} deregistered during \
                                         reconnect handshake, aborting"
                                    );
                                    return;
                                }
                                me.mark_session_connected(
                                    session_id,
                                    new_host_id.clone(),
                                    identity_key,
                                    transport,
                                    ctx,
                                );
                                if let Some(client) = me.client_for_session(session_id).cloned() {
                                    ctx.emit(RemoteServerManagerEvent::SessionReconnected {
                                        session_id,
                                        host_id: new_host_id,
                                        attempt,
                                        client,
                                    });
                                }
                            })
                            .await;
                    }
                    Err(e) => {
                        log::error!(
                            "Reconnect failed for session {session_id:?} \
                             (attempt {attempt}): {e}"
                        );
                        let _ = spawner
                            .spawn(move |me, ctx| {
                                // If the session was deregistered during the
                                // handshake, don't retry or insert Disconnected.
                                if !me.sessions.contains_key(&session_id) {
                                    log::info!(
                                        "Session {session_id:?} deregistered during \
                                         reconnect handshake, aborting"
                                    );
                                    return;
                                }
                                me.handle_reconnect_failure(
                                    session_id,
                                    ReconnectParams {
                                        attempt,
                                        host_id,
                                        exit_status,
                                        transport,
                                        auth_context,
                                        control_path,
                                        identity_key,
                                    },
                                    ctx,
                                );
                            })
                            .await;
                    }
                }
            })
            .detach();
    }

    /// Handle a failed reconnection attempt: either retry or give up.
    #[cfg(not(target_family = "wasm"))]
    fn handle_reconnect_failure(
        &mut self,
        session_id: SessionId,
        params: ReconnectParams,
        ctx: &mut ModelContext<Self>,
    ) {
        if params.attempt < MAX_RECONNECT_ATTEMPTS {
            self.attempt_reconnect(
                session_id,
                ReconnectParams {
                    attempt: params.attempt + 1,
                    ..params
                },
                ctx,
            );
        } else {
            log::warn!(
                "Reconnect exhausted for session {session_id:?} after {} attempt(s)",
                params.attempt
            );
            self.sessions
                .insert(session_id, RemoteSessionState::Disconnected);
            ctx.emit(RemoteServerManagerEvent::SessionDisconnected {
                session_id,
                host_id: params.host_id,
                exit_status: params.exit_status,
            });
            // Note: HostDisconnected was already emitted by
            // mark_session_disconnected when entering the reconnect flow.
        }
    }

    /// Removes a session from the host → sessions reverse index.
    /// Cleans up the entry entirely if the set becomes empty.
    fn remove_from_host_index(&mut self, host_id: &HostId, session_id: SessionId) {
        if let Some(set) = self.host_to_sessions.get_mut(host_id) {
            set.remove(&session_id);
            if set.is_empty() {
                self.host_to_sessions.remove(host_id);
            }
        }
    }

    // --- Host-scoped request tracking (#438 dependent features 3/4/5) ------
    //
    // Deviation from the pin: this only carries request dispatch + response
    // matching, not the pin's cross-connection failover (retry on writer
    // failure through a sibling session, `HostRequestHandle`, per-request
    // timeout). #438's own issue body lists that failover machinery as
    // downstream of the envelope, not required for `abort_host_request` or
    // `start_ripgrep_search` themselves.

    /// Tries each connected session for `host_id` in turn, dispatching `msg`
    /// via `client.send_host_scoped`. Returns the session id that accepted
    /// it, or `None` if the host has no connected sessions, or every
    /// session's outbound channel was already closed (writer task exited
    /// after a dead connection).
    fn dispatch_host_scoped_request(
        &self,
        host_id: &HostId,
        msg: &crate::proto::ClientMessage,
    ) -> Option<SessionId> {
        let sessions = self.host_to_sessions.get(host_id)?;
        for &session_id in sessions {
            let Some(client) = self.client_for_session(session_id) else {
                continue;
            };
            match client.send_host_scoped(msg.clone()) {
                Ok(()) => return Some(session_id),
                Err(err) => {
                    log::warn!(
                        "Host-scoped request dispatch failed on session {session_id:?}: \
                         host={host_id} request_id={} error={err}",
                        msg.request_id
                    );
                }
            }
        }
        None
    }

    /// Sends a host-scoped request to any connected session for the given
    /// host.
    ///
    /// The caller constructs the `ClientMessage` (already wrapped in
    /// `HostScopedRequest`). The manager dispatches it via
    /// `client.send_host_scoped()` and, on success, registers the request in
    /// `pending_host_requests`. The response arrives asynchronously on the
    /// host-response channel drained in `run_connect_and_handshake` and is
    /// matched by `request_id` in `resolve_host_response`.
    ///
    /// Returns a `oneshot::Receiver` that resolves with the raw
    /// `ServerMessage` on success, or `HostRequestError` on failure (no
    /// connected session for the host, or a server-reported error).
    pub fn send_host_request(
        &mut self,
        host_id: &HostId,
        msg: crate::proto::ClientMessage,
    ) -> oneshot::Receiver<Result<crate::proto::ServerMessage, HostRequestError>> {
        let (result_tx, result_rx) = oneshot::channel();
        let request_id = crate::protocol::RequestId::from(msg.request_id.clone());

        // Dispatch before registering the pending entry. If every session's
        // outbound channel is already closed, fail the caller now instead of
        // registering a request that can never be matched, which would
        // otherwise leak in `pending_host_requests` forever.
        let Some(dispatched_session_id) = self.dispatch_host_scoped_request(host_id, &msg) else {
            log::warn!(
                "Host-scoped request dispatch failed (no connected session for host): \
                 host={host_id} request_id={request_id}"
            );
            let _ = result_tx.send(Err(HostRequestError::AllSessionsDisconnected));
            return result_rx;
        };

        self.pending_host_requests.insert(
            request_id,
            PendingHostRequest {
                host_id: host_id.clone(),
                dispatched_session_id,
                result_tx,
            },
        );

        result_rx
    }

    /// Creates a [`HostRequestHandle`] for dispatching host-scoped requests
    /// from async contexts that don't have `&mut self` access.
    pub fn host_request_handle(&self, host_id: &HostId) -> HostRequestHandle {
        HostRequestHandle {
            spawner: self.spawner.clone(),
            host_id: host_id.clone(),
        }
    }

    /// Registers a remote ripgrep request synchronously and returns its
    /// typed pending result. #438 dependent feature 5 — the client already
    /// exposes a session-scoped-style `ripgrep_search` (used directly by
    /// global search over an already-known session); this is the
    /// manager-layer, host-scoped dispatch the issue identified as missing,
    /// used when only a `HostId` (not a specific session) is known, or when
    /// the caller needs `abort_host_request` cancellation.
    pub fn start_ripgrep_search(
        &mut self,
        host_id: &HostId,
        params: RipgrepSearchParams,
    ) -> PendingRipgrepSearch {
        let request_id = crate::protocol::RequestId::new();
        let msg = crate::proto::ClientMessage::host_scoped(
            request_id.to_string(),
            crate::proto::host_scoped_request::Message::RipgrepSearch(
                crate::proto::RipgrepSearchRequest {
                    pattern: params.pattern,
                    roots: params
                        .roots
                        .into_iter()
                        .map(|path| path.to_string())
                        .collect(),
                    ignore_case: params.ignore_case,
                    multiline: params.multiline,
                    max_matches: params.max_matches,
                },
            ),
        );
        let response = self.send_host_request(host_id, msg);
        PendingRipgrepSearch {
            request_id,
            response,
        }
    }

    /// Aborts a pending host-scoped request: resolves the caller with
    /// [`HostRequestError::Aborted`] and sends a best-effort `Abort`
    /// notification so the daemon stops work. No-op if the request already
    /// resolved (response arrived, or all sessions disconnected). #438
    /// dependent feature 4.
    pub fn abort_host_request(&mut self, request_id: &crate::protocol::RequestId) {
        let Some(pending) = self.pending_host_requests.remove(request_id) else {
            return;
        };
        log::info!(
            "Aborting host-scoped request: host={} request_id={request_id}",
            pending.host_id
        );
        // Best-effort daemon-side cancellation. The connection may already
        // be gone, in which case this is a no-op.
        if let Some(client) = self.client_for_host(&pending.host_id) {
            client.abort_request(request_id);
        }
        let _ = pending.result_tx.send(Err(HostRequestError::Aborted));
    }

    /// Matches a response drained off a session's host-response channel
    /// against `pending_host_requests` by `request_id`, resolving the
    /// caller. A server-reported `ErrorResponse` is converted to
    /// `HostRequestError::ServerError` so callers only need to match on
    /// success variants, mirroring `RemoteServerClient::send_request`'s
    /// error conversion for session-scoped requests. No-ops (logs at debug)
    /// if the request isn't pending — already resolved, already aborted, or
    /// a stray/duplicate response.
    #[cfg(not(target_family = "wasm"))]
    fn resolve_host_response(&mut self, msg: crate::proto::ServerMessage) {
        let request_id = crate::protocol::RequestId::from(msg.request_id.clone());
        let Some(pending) = self.pending_host_requests.remove(&request_id) else {
            log::debug!(
                "Received host response for unknown/already-resolved request_id={request_id}"
            );
            return;
        };
        let result = match &msg.message {
            Some(crate::proto::server_message::Message::Error(e)) => {
                Err(HostRequestError::ServerError {
                    code: e.code(),
                    message: e.message.clone(),
                })
            }
            _ => Ok(msg),
        };
        let _ = pending.result_tx.send(result);
    }

    /// Fails a pending host-scoped request whose response arrived but could
    /// not be decoded (`ClientEvent::HostScopedDecodeFailed`). The daemon
    /// already produced a reply, so this is terminal — unlike a dispatch
    /// failure, retrying wouldn't help. No-op if the request isn't pending.
    #[cfg(not(target_family = "wasm"))]
    fn fail_host_request_decode_error(&mut self, request_id: &crate::protocol::RequestId) {
        if let Some(pending) = self.pending_host_requests.remove(request_id) {
            log::warn!(
                "Failing host request {request_id} — server response could not be \
                 decoded (host={})",
                pending.host_id
            );
            let _ = pending
                .result_tx
                .send(Err(HostRequestError::UnexpectedResponse));
        }
    }

    /// Accepts a `RemoteAgentContextSnapshot` push if its `revision` is
    /// newer than the last one stored for `host_id`, storing it and
    /// returning `true`. Returns `false` (leaving the stored snapshot
    /// unchanged) for a stale or duplicate revision. #438 dependent feature
    /// 1.
    fn accept_remote_agent_context_snapshot(
        &mut self,
        host_id: &HostId,
        snapshot: crate::proto::RemoteAgentContextSnapshot,
    ) -> bool {
        let is_newer = !matches!(
            self.remote_agent_context_snapshots.get(host_id),
            Some(existing) if existing.revision >= snapshot.revision
        );
        if is_newer {
            self.remote_agent_context_snapshots
                .insert(host_id.clone(), snapshot);
        }
        is_newer
    }

    /// Returns the most recently accepted `RemoteAgentContextSnapshot` for
    /// `host_id`, if the daemon has pushed one since the host last
    /// connected (`handle_host_disconnected` clears it on disconnect). #438
    /// dependent feature 1; the daemon-side producer is #353
    /// (`app/src/remote_server/server_model.rs`). Callers generally don't need
    /// this directly — `app::ai::remote_agent_context::RemoteAgentContext`
    /// subscribes to `RemoteServerManagerEvent::RemoteAgentContextSnapshot`
    /// (emitted right after a snapshot is accepted here) instead of polling.
    /// Note two known gaps before assuming this snapshot is fully wired: (1) as of
    /// #440 (reopened), the daemon has no `daemon_bundled_resources_dir()`, so
    /// `snapshot.skills`'s bundled entries are always empty in production —
    /// `home_dir` and home-scoped `snapshot.skills` entries are populated, though.
    /// (2) `snapshot.global_rules` is always empty too: this fork's
    /// `ProjectContextModel` has no per-host rule storage to source it from (see
    /// `app/src/ai/remote_agent_context.rs`'s module doc comment) — a separate,
    /// comparably-sized gap from #440, not fixed here either.
    pub fn remote_agent_context_snapshot(
        &self,
        host_id: &HostId,
    ) -> Option<&crate::proto::RemoteAgentContextSnapshot> {
        self.remote_agent_context_snapshots.get(host_id)
    }

    /// Fails every pending host-scoped request for `host_id` with
    /// `HostRequestError::AllSessionsDisconnected`. Called from
    /// `handle_host_disconnected`, so it only needs to filter by host —
    /// the caller already established that no session for this host remains.
    fn fail_pending_host_requests_for_disconnected_host(&mut self, host_id: &HostId) {
        let orphaned: Vec<crate::protocol::RequestId> = self
            .pending_host_requests
            .iter()
            .filter(|(_, pending)| &pending.host_id == host_id)
            .map(|(rid, _)| rid.clone())
            .collect();
        for rid in orphaned {
            if let Some(pending) = self.pending_host_requests.remove(&rid) {
                log::info!("Failing pending host request {rid} — host {host_id} disconnected");
                let _ = pending
                    .result_tx
                    .send(Err(HostRequestError::AllSessionsDisconnected));
            }
        }
    }

    /// Single call site for the "last session for this host disconnected"
    /// transition: clears per-host dedup/pending-request state and emits
    /// `HostDisconnected`. Replaces the three call sites that used to emit
    /// `HostDisconnected` directly, so no caller can update `host_to_sessions`
    /// and forget to fail pending host requests for the departed host.
    fn handle_host_disconnected(&mut self, host_id: HostId, ctx: &mut ModelContext<Self>) {
        self.remote_agent_context_snapshots.remove(&host_id);
        self.fail_pending_host_requests_for_disconnected_host(&host_id);
        ctx.emit(RemoteServerManagerEvent::HostDisconnected { host_id });
    }
}

/// Handle for dispatching host-scoped requests from async contexts.
///
/// Pure-async callers don't have direct access to `&mut RemoteServerManager`.
/// This handle wraps a `ModelSpawner` so the async function can bounce each
/// request to the main thread for registration in `pending_host_requests`,
/// then await the response on the caller's thread.
///
/// Obtain via [`RemoteServerManager::host_request_handle`].
///
/// Deviation from the pin: only the `send` dispatch and a `read_file_context`
/// convenience wrapper are ported (what `FileNotebookView::open_remote`
/// needs). The pin's other typed convenience methods (`write_file`,
/// `delete_file`, `save_buffer`, `open_buffer`, ...) aren't ported here —
/// add them the same way (wrap [`Self::send`], match the response variant)
/// when a caller needs them.
pub struct HostRequestHandle {
    spawner: ModelSpawner<RemoteServerManager>,
    host_id: HostId,
}

impl HostRequestHandle {
    /// Sends a host-scoped request and awaits the raw `ServerMessage`.
    ///
    /// Bounces to the main thread to call `send_host_request`, then awaits
    /// the response on the caller's thread.
    pub async fn send(
        &self,
        inner: crate::proto::host_scoped_request::Message,
    ) -> Result<crate::proto::ServerMessage, HostRequestError> {
        let host_id = self.host_id.clone();
        let request_id = crate::protocol::RequestId::new();
        let msg = crate::proto::ClientMessage::host_scoped(request_id.to_string(), inner);
        let rx = self
            .spawner
            .spawn(move |me, _ctx| me.send_host_request(&host_id, msg))
            .await
            .map_err(|_| HostRequestError::AllSessionsDisconnected)?;
        rx.await
            .map_err(|_| HostRequestError::AllSessionsDisconnected)?
    }

    /// Batch-reads one or more files from the remote host with full context
    /// (line ranges, binary/image support, metadata, size limits).
    ///
    /// Per-file failures are reported in `ReadFileContextResponse::failed_files`
    /// rather than as a top-level error; this only returns `Err` for transport
    /// or unexpected-response failures.
    pub async fn read_file_context(
        &self,
        request: crate::proto::ReadFileContextRequest,
    ) -> Result<crate::proto::ReadFileContextResponse, HostRequestError> {
        let msg = self
            .send(crate::proto::host_scoped_request::Message::ReadFileContext(
                request,
            ))
            .await?;
        match msg.message {
            Some(crate::proto::server_message::Message::ReadFileContextResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for ReadFileContext: {other:?}");
                Err(HostRequestError::UnexpectedResponse)
            }
        }
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
#[path = "manager_tests.rs"]
mod tests;
