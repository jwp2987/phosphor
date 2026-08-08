use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures::channel::oneshot;
use futures::io::{AsyncRead, AsyncWrite};
use warpui::r#async::{executor, FutureExt as _};

use crate::proto::{
    discard_files_response, host_scoped_request, notification, read_file_chunk_response,
    server_message, session_scoped_request, Abort, Authenticate, BufferEdit, ClientMessage,
    CloseBuffer, CreateDirectory, CreateDirectoryResponse, DeleteFile, DiffStateFileDelta,
    DiffStateMetadataUpdate, DiffStateSnapshot, DiscardFilesRequest, ErrorCode, GetBranches,
    GetBranchesResponse, GetCommittedBranchFilesRequest, GetCommittedBranchFilesResponse,
    GetDiffState, GetDiffStateResponse, GitCommitChainRequest, GitCommitChainResponse,
    GitCreatePrRequest, GitCreatePrResponse, GitPushRequest, GitPushResponse, Initialize,
    InitializeResponse, ListDirectory, ListDirectoryResponse, LoadRepoMetadataDirectoryResponse,
    NavigatedToDirectoryResponse, OpenBuffer, OpenBufferResponse, ReadFileChunk,
    ReadFileChunkResponse, ReadFileContextRequest, ReadFileContextResponse, ResolveConflict,
    ResolveConflictResponse, ResolvePath, ResolvePathResponse, RipgrepSearchRequest,
    RipgrepSearchResponse, RunCommandRequest, RunCommandResponse, SaveBuffer, SaveBufferResponse,
    ServerMessage, SessionBootstrapped, TextEdit, UnsubscribeDiffState, WriteFile, WriteFileChunk,
    WriteFileChunkResponse,
};

use crate::protocol::{self, ProtocolError, RequestId};

use warp_core::SessionId;
use warpui::r#async::TransportStream;

/// Default request timeout (2 minutes).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Errors from the `RemoteServerClient`.
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("Connection was dropped")]
    Disconnected,

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Response channel closed before receiving a reply")]
    ResponseChannelClosed,

    #[error("Unexpected response from server")]
    UnexpectedResponse,

    #[error("Server error ({code:?}): {message}")]
    ServerError { code: ErrorCode, message: String },

    #[error("Request timed out after {0:?}")]
    Timeout(Duration),

    #[error("File operation failed: {0}")]
    FileOperationFailed(String),
}

/// Events received from the remote server, delivered through the event
/// channel returned by [`RemoteServerClient::new`].
///
/// The consumer (typically `RemoteServerManager`) drains this channel to
/// react to connection lifecycle changes and server-pushed data.
#[derive(Clone, Debug)]
pub enum ClientEvent {
    /// The reader task detected EOF or a fatal error. The connection is gone.
    /// This is always the last event sent on the channel.
    Disconnected,
    /// A full or lazy-loaded repo metadata snapshot was pushed by the server.
    RepoMetadataSnapshotReceived {
        update: repo_metadata::RepoMetadataUpdate,
    },
    /// An incremental repo metadata update was pushed by the server.
    RepoMetadataUpdated {
        update: repo_metadata::RepoMetadataUpdate,
    },
    /// A buffer was updated on the server (file changed on disk).
    BufferUpdated {
        path: String,
        new_server_version: u64,
        expected_client_version: u64,
        edits: Vec<TextEdit>,
    },
    /// The file changed on disk while the client had unsaved edits. The
    /// server did NOT apply the disk change; the client should show a
    /// conflict resolution banner and discard any in-flight edit batch.
    BufferConflictDetected { path: String },
    /// A full diff-state snapshot was pushed by the server for a subscribed
    /// (repo, mode) pair. Carries the raw proto message; the consumer
    /// (`DiffStateModel` remote backend, in `app`) converts it to domain types.
    DiffStateSnapshotReceived { snapshot: DiffStateSnapshot },
    /// A metadata-only diff-state update was pushed by the server.
    DiffStateMetadataUpdateReceived { update: DiffStateMetadataUpdate },
    /// A single-file diff-state delta was pushed by the server.
    DiffStateFileDeltaReceived { delta: DiffStateFileDelta },
    /// A server message could not be decoded and had no parseable request_id.
    MessageDecodingError,
}
/// Client for communicating with a `remote_server` process over the remote server protocol.
///
/// Exposes async request/response APIs over generic I/O streams (child-process pipes,
/// SSH channels, or in-memory streams for testing).
///
/// Designed to be wrapped in `Arc` for sharing across threads. Construction
/// returns an event receiver that delivers push events and a final
/// `Disconnected` event when the connection drops.
///
/// This type does **not** own the child subprocess whose stdio backs it.
/// For transports that spawn a subprocess (e.g. SSH), the caller is
/// responsible for holding the `Child` for the lifetime of the session
/// so that `kill_on_drop` fires when teardown occurs. In Zap this is
/// the `RemoteServerManager`, which stores the child in
/// `RemoteSessionState` alongside the `Arc<RemoteServerClient>`. That
/// way the child's lifetime is gated by the manager's session map
/// rather than by `Arc` refcount -- cloning `Arc<RemoteServerClient>`
/// into other owners (e.g. the command executor) no longer keeps the
/// child alive.
pub struct RemoteServerClient {
    /// Channel for queuing ClientMessages to send to the remote server.
    outbound_tx: async_channel::Sender<ClientMessage>,

    /// Maps `request_id` → oneshot sender for the correlated response from the remote server.
    pending_requests: Arc<DashMap<RequestId, oneshot::Sender<Result<ServerMessage, ClientError>>>>,

    /// Set to `true` by the reader task when the connection is lost. Checked by
    /// `send_request` after inserting into `pending_requests` to avoid hanging
    /// on a dead connection.
    disconnected: Arc<AtomicBool>,
}

impl fmt::Debug for RemoteServerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteServerClient").finish_non_exhaustive()
    }
}

#[cfg(not(target_family = "wasm"))]
impl RemoteServerClient {
    /// Creates a client from a child process's stdin, stdout, and stderr.
    ///
    /// The caller retains ownership of the `Child` itself. Typically the
    /// caller spawns the `Command` with `kill_on_drop(true)` and stashes
    /// the returned `Child` somewhere whose lifetime matches the
    /// session's (in Zap, on the `RemoteServerManager`'s
    /// `RemoteSessionState`). Dropping the `Child` there triggers
    /// SIGKILL on the subprocess, regardless of how many
    /// `Arc<RemoteServerClient>` clones are still alive.
    ///
    /// Internally forwards stderr lines to local logging via
    /// [`spawn_stderr_forwarder`], then delegates to [`Self::new`] for the
    /// protocol reader/writer setup.
    ///
    /// Returns the client and an event receiver that delivers push events
    /// and a final `Disconnected` event when the connection drops.
    pub fn from_child_streams(
        stdin: async_process::ChildStdin,
        stdout: async_process::ChildStdout,
        stderr: async_process::ChildStderr,
        executor: &executor::Background,
    ) -> (Self, async_channel::Receiver<ClientEvent>) {
        spawn_stderr_forwarder(stderr, executor);
        Self::new(stdout, stdin, executor)
    }
}

impl RemoteServerClient {
    /// Creates a new client, spawning background reader and writer tasks on the
    /// provided executor.
    ///
    /// Returns the client and an event receiver that delivers push events
    /// and a final `Disconnected` event when the connection drops.
    pub fn new(
        reader: impl AsyncRead + TransportStream,
        writer: impl AsyncWrite + TransportStream,
        executor: &executor::Background,
    ) -> (Self, async_channel::Receiver<ClientEvent>) {
        let pending_requests: Arc<
            DashMap<RequestId, oneshot::Sender<Result<ServerMessage, ClientError>>>,
        > = Arc::new(DashMap::new());
        let (outbound_tx, outbound_rx) = async_channel::unbounded::<ClientMessage>();
        let (event_tx, event_rx) = async_channel::unbounded::<ClientEvent>();
        let disconnected = Arc::new(AtomicBool::new(false));

        executor
            .spawn(Self::writer_task(
                writer,
                outbound_rx,
                Arc::clone(&pending_requests),
            ))
            .detach();
        executor
            .spawn(Self::reader_task(
                reader,
                Arc::clone(&pending_requests),
                event_tx,
                Arc::clone(&disconnected),
            ))
            .detach();

        (
            Self {
                outbound_tx,
                pending_requests,
                disconnected,
            },
            event_rx,
        )
    }

    /// Sends an `Initialize` request and awaits the `InitializeResponse`.
    pub async fn initialize(
        &self,
        auth_token: Option<&str>,
    ) -> Result<InitializeResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::session_scoped(
            request_id.to_string(),
            session_scoped_request::Message::Initialize(Initialize {
                auth_token: auth_token.unwrap_or_default().to_owned(),
            }),
        );

        let response = self.send_request(request_id, msg).await?;

        match response.message {
            Some(server_message::Message::InitializeResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for Initialize: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Sends an `Authenticate` notification to rotate the daemon-wide
    /// credential after initialization.
    pub fn authenticate(&self, auth_token: &str) {
        let msg = ClientMessage::notification(notification::Message::Authenticate(Authenticate {
            auth_token: auth_token.to_owned(),
        }));
        self.send_notification(msg);
    }

    /// Sends a `SessionBootstrapped` notification (fire-and-forget) so the
    /// server can create a `LocalCommandExecutor` for the session.
    pub fn notify_session_bootstrapped(
        &self,
        session_id: SessionId,
        shell_type: &str,
        shell_path: Option<&str>,
    ) {
        let msg = ClientMessage::notification(notification::Message::SessionBootstrapped(
            SessionBootstrapped {
                session_id: session_id.as_u64(),
                shell_type: shell_type.to_owned(),
                shell_path: shell_path.map(ToOwned::to_owned),
            },
        ));
        self.send_notification(msg);
    }

    /// Sends a `NavigatedToDirectory` request and awaits the response.
    pub async fn navigate_to_directory(
        &self,
        path: String,
    ) -> Result<NavigatedToDirectoryResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::session_scoped(
            request_id.to_string(),
            session_scoped_request::Message::NavigatedToDirectory(
                crate::proto::NavigatedToDirectory { path },
            ),
        );

        let response = self.send_request(request_id, msg).await?;

        match response.message {
            Some(server_message::Message::NavigatedToDirectoryResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for NavigatedToDirectory: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Sends a `LoadRepoMetadataDirectory` request and awaits the response.
    pub async fn load_repo_metadata_directory(
        &self,
        repo_path: String,
        dir_path: String,
    ) -> Result<LoadRepoMetadataDirectoryResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::session_scoped(
            request_id.to_string(),
            session_scoped_request::Message::LoadRepoMetadataDirectory(
                crate::proto::LoadRepoMetadataDirectory {
                    repo_path,
                    dir_path,
                },
            ),
        );

        let response = self.send_request(request_id, msg).await?;

        match response.message {
            Some(server_message::Message::LoadRepoMetadataDirectoryResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for LoadRepoMetadataDirectory: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Writes content to a file on the remote host.
    /// Creates parent directories if they don't exist.
    pub async fn write_file(&self, path: String, content: String) -> Result<(), ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::WriteFile(WriteFile { path, content }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::WriteFileResponse(resp)) => match resp.result {
                Some(crate::proto::write_file_response::Result::Success(_)) | None => Ok(()),
                Some(crate::proto::write_file_response::Result::Error(e)) => {
                    Err(ClientError::FileOperationFailed(e.message))
                }
            },
            other => {
                log::error!("Unexpected response variant for WriteFile: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Batch-reads one or more files from the remote host with full context
    /// (line ranges, binary/image support, metadata, size limits).
    ///
    /// Per-file failures are reported in `ReadFileContextResponse::failed_files`
    /// rather than as a top-level error. The method only returns `Err` for
    /// transport-level failures (disconnect, timeout, etc.).
    pub async fn read_file_context(
        &self,
        request: ReadFileContextRequest,
    ) -> Result<ReadFileContextResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::ReadFileContext(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::ReadFileContextResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for ReadFileContext: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Runs a ripgrep search over the given root directories on the remote host.
    /// Used by global search for SSH-remote sessions.
    pub async fn ripgrep_search(
        &self,
        request: RipgrepSearchRequest,
    ) -> Result<RipgrepSearchResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::RipgrepSearch(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::RipgrepSearchResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for RipgrepSearch: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Lists the git branches of a repo on the remote host — backs the
    /// code-review branch picker over SSH.
    pub async fn get_branches(
        &self,
        request: GetBranches,
    ) -> Result<GetBranchesResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::GetBranches(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::GetBranchesResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for GetBranches: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Lists the committed-only changed files of the current branch on the
    /// remote host (`main...HEAD`) — backs the code-review file list over SSH.
    pub async fn get_committed_branch_files(
        &self,
        request: GetCommittedBranchFilesRequest,
    ) -> Result<GetCommittedBranchFilesResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::GetCommittedBranchFiles(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::GetCommittedBranchFilesResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for GetCommittedBranchFiles: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Runs the commit chain (commit, then optionally push, then optionally
    /// create-PR) on the remote host in a single round trip. Backs code-review
    /// commit / commit-and-push / commit-and-create-PR over SSH.
    pub async fn git_commit_chain(
        &self,
        request: GitCommitChainRequest,
    ) -> Result<GitCommitChainResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::GitCommitChain(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::GitCommitChainResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for GitCommitChain: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Pushes `branch` to origin on the remote host, setting upstream tracking.
    /// Backs the code-review push action over SSH.
    pub async fn git_push(&self, request: GitPushRequest) -> Result<GitPushResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::GitPush(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::GitPushResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for GitPush: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Creates a PR for the current branch on the remote host (branch must be
    /// pushed). Backs the code-review create-PR action over SSH.
    pub async fn git_create_pr(
        &self,
        request: GitCreatePrRequest,
    ) -> Result<GitCreatePrResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::GitCreatePr(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::GitCreatePrResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for GitCreatePr: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Subscribes to diff state for a (repo, mode) pair. Returns the server's
    /// initial `GetDiffStateResponse` (snapshot or error); subsequent changes
    /// arrive asynchronously as `ClientEvent::DiffState*Received` push events.
    /// Call [`unsubscribe_diff_state`](Self::unsubscribe_diff_state) to stop
    /// the updates.
    pub async fn get_diff_state(
        &self,
        request: GetDiffState,
    ) -> Result<GetDiffStateResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::session_scoped(
            request_id.to_string(),
            session_scoped_request::Message::GetDiffState(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::GetDiffStateResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for GetDiffState: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Unsubscribes from diff-state updates for a (repo, mode) pair.
    /// Fire-and-forget: the server sends no response.
    pub fn unsubscribe_diff_state(&self, request: UnsubscribeDiffState) {
        let msg = ClientMessage::notification(notification::Message::UnsubscribeDiffState(request));
        self.send_notification(msg);
    }

    /// Deletes a file on the remote host.
    pub async fn delete_file(&self, path: String) -> Result<(), ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::DeleteFile(DeleteFile { path }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::DeleteFileResponse(resp)) => match resp.result {
                Some(crate::proto::delete_file_response::Result::Success(_)) | None => Ok(()),
                Some(crate::proto::delete_file_response::Result::Error(e)) => {
                    Err(ClientError::FileOperationFailed(e.message))
                }
            },
            other => {
                log::error!("Unexpected response variant for DeleteFile: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Discards changes for one or more files on the remote host (git
    /// restore/stash/rm), mirroring the local code-review "discard changes"
    /// action. Backs `RemoteDiffStateModel::discard_files` over SSH (#437).
    pub async fn discard_files(&self, request: DiscardFilesRequest) -> Result<(), ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::DiscardFiles(request),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::DiscardFilesResponse(resp)) => match resp.result {
                Some(discard_files_response::Result::Success(_)) | None => Ok(()),
                Some(discard_files_response::Result::Error(e)) => {
                    Err(ClientError::FileOperationFailed(e.message))
                }
            },
            other => {
                log::error!("Unexpected response variant for DiscardFiles: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Zap: lists the immediate children of a directory on the remote host.
    ///
    /// Used by terminal file link detection to precisely validate the shape of
    /// remote paths (local sessions do this via `fs::metadata`, but remote
    /// files aren't on the local disk).
    pub async fn list_directory(&self, path: String) -> Result<ListDirectoryResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::ListDirectory(ListDirectory { path }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::ListDirectoryResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for ListDirectory: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Resolves a path on the remote host for the server file browser.
    pub async fn resolve_path(&self, path: String) -> Result<ResolvePathResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::ResolvePath(ResolvePath { path }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::ResolvePathResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for ResolvePath: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Creates a directory on the remote host, including missing parents.
    pub async fn create_directory(
        &self,
        path: String,
    ) -> Result<CreateDirectoryResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::CreateDirectory(CreateDirectory { path }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::CreateDirectoryResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for CreateDirectory: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Reads a byte range from a remote file.
    pub async fn read_file_chunk(
        &self,
        path: String,
        offset: u64,
        max_bytes: u64,
    ) -> Result<ReadFileChunkResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::ReadFileChunk(ReadFileChunk {
                path,
                offset,
                max_bytes,
            }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::ReadFileChunkResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for ReadFileChunk: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Reads an entire remote file by looping [`Self::read_file_chunk`] until EOF.
    ///
    /// Accumulates each chunk into a single buffer, advancing `offset` by the
    /// server-reported `next_offset`, until a chunk signals `eof`. Used by the
    /// in-app image viewer to fetch raw image bytes for `AssetSource::Raw`.
    pub async fn read_file_bytes(&self, path: String) -> Result<Vec<u8>, ClientError> {
        // The server's per-chunk cap is 8 MiB (`handle_read_file_chunk`). The
        // client requests in 4 MiB chunks, well below the 64 MiB message limit,
        // leaving plenty of headroom for framing.
        const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

        let mut bytes = Vec::new();
        let mut offset = 0u64;
        loop {
            let response = self
                .read_file_chunk(path.clone(), offset, CHUNK_SIZE)
                .await?;
            let success = match response.result {
                Some(read_file_chunk_response::Result::Success(success)) => success,
                Some(read_file_chunk_response::Result::Error(err)) => {
                    return Err(ClientError::FileOperationFailed(err.message));
                }
                None => return Err(ClientError::UnexpectedResponse),
            };
            bytes.extend_from_slice(&success.bytes);
            offset = success.next_offset;
            if success.eof {
                break;
            }
        }
        Ok(bytes)
    }

    /// Writes a byte range to a remote file.
    pub async fn write_file_chunk(
        &self,
        path: String,
        offset: u64,
        bytes: Vec<u8>,
        truncate: bool,
        executable: Option<bool>,
    ) -> Result<WriteFileChunkResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::WriteFileChunk(WriteFileChunk {
                path,
                offset,
                bytes,
                truncate,
                executable,
            }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::WriteFileChunkResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for WriteFileChunk: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Opens a buffer on the remote host for bidirectional syncing.
    pub async fn open_buffer(&self, path: String) -> Result<OpenBufferResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::session_scoped(
            request_id.to_string(),
            session_scoped_request::Message::OpenBuffer(OpenBuffer { path }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::OpenBufferResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for OpenBuffer: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Sends a buffer edit notification to the remote host.
    ///
    /// Zap: unlike other fire-and-forget notifications, a failed buffer-edit
    /// delivery must be reported. If silently swallowed when `outbound_tx` is
    /// closed (connection dead), the local buffer would keep advancing while
    /// the daemon never receives the edit, causing an invisible desync. Returns
    /// `Err` on failure so the caller can handle it.
    pub fn send_buffer_edit(
        &self,
        path: String,
        expected_server_version: u64,
        new_client_version: u64,
        edits: Vec<TextEdit>,
    ) -> Result<(), ClientError> {
        let msg = ClientMessage::notification(notification::Message::BufferEdit(BufferEdit {
            path,
            expected_server_version,
            new_client_version,
            edits,
        }));
        self.outbound_tx.try_send(msg).map_err(|e| {
            log::error!("Failed to enqueue buffer edit: {e}");
            ClientError::Disconnected
        })
    }

    /// Tells the remote host to close a buffer (stop watching).
    pub fn close_buffer(&self, path: String) {
        let msg =
            ClientMessage::notification(notification::Message::CloseBuffer(CloseBuffer { path }));
        self.send_notification(msg);
    }

    /// Persists the current in-memory buffer to disk on the remote host.
    pub async fn save_buffer(&self, path: String) -> Result<SaveBufferResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::SaveBuffer(SaveBuffer { path }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::SaveBufferResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for SaveBuffer: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Resolves a buffer conflict by accepting the client's content.
    pub async fn resolve_conflict(
        &self,
        path: String,
        acknowledged_server_version: u64,
        client_content: String,
        current_client_version: u64,
    ) -> Result<ResolveConflictResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::host_scoped(
            request_id.to_string(),
            host_scoped_request::Message::ResolveConflict(ResolveConflict {
                path,
                acknowledged_server_version,
                client_content,
                current_client_version,
            }),
        );
        let response = self.send_request(request_id, msg).await?;
        match response.message {
            Some(server_message::Message::ResolveConflictResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for ResolveConflict: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Converts a server push message (empty request_id) into a domain event.
    fn push_message_to_event(msg: ServerMessage) -> Option<ClientEvent> {
        match msg.message? {
            server_message::Message::RepoMetadataSnapshot(snapshot) => {
                let update = crate::repo_metadata_proto::proto_snapshot_to_update(&snapshot)?;
                Some(ClientEvent::RepoMetadataSnapshotReceived { update })
            }
            server_message::Message::RepoMetadataUpdate(push) => {
                let update = crate::repo_metadata_proto::proto_to_repo_metadata_update(&push)?;
                Some(ClientEvent::RepoMetadataUpdated { update })
            }
            server_message::Message::BufferUpdated(push) => Some(ClientEvent::BufferUpdated {
                path: push.path,
                new_server_version: push.new_server_version,
                expected_client_version: push.expected_client_version,
                edits: push.edits,
            }),
            server_message::Message::BufferConflictDetected(push) => {
                Some(ClientEvent::BufferConflictDetected { path: push.path })
            }
            server_message::Message::DiffStateSnapshot(snapshot) => {
                Some(ClientEvent::DiffStateSnapshotReceived { snapshot })
            }
            server_message::Message::DiffStateMetadataUpdate(update) => {
                Some(ClientEvent::DiffStateMetadataUpdateReceived { update })
            }
            server_message::Message::DiffStateFileDelta(delta) => {
                Some(ClientEvent::DiffStateFileDeltaReceived { delta })
            }
            other => {
                log::warn!("Unhandled push message variant: {other:?}");
                None
            }
        }
    }

    /// Sends a `RunCommand` request
    pub async fn run_command(
        &self,
        session_id: SessionId,
        command: String,
        working_directory: Option<String>,
        environment_variables: HashMap<String, String>,
    ) -> Result<RunCommandResponse, ClientError> {
        let request_id = RequestId::new();
        let msg = ClientMessage::session_scoped(
            request_id.to_string(),
            session_scoped_request::Message::RunCommand(RunCommandRequest {
                command,
                working_directory,
                environment_variables,
                session_id: session_id.as_u64(),
            }),
        );

        let response = self.send_request(request_id, msg).await?;

        match response.message {
            Some(server_message::Message::RunCommandResponse(resp)) => Ok(resp),
            other => {
                log::error!("Unexpected response variant for RunCommand: {other:?}");
                Err(ClientError::UnexpectedResponse)
            }
        }
    }

    /// Generic request/response correlation.
    ///
    /// Registers a oneshot channel keyed by `request_id`, sends the message
    /// through the outbound channel, and awaits the correlated response.
    /// Times out after `REQUEST_TIMEOUT` and sends an `Abort` to the server.
    async fn send_request(
        &self,
        request_id: RequestId,
        msg: ClientMessage,
    ) -> Result<ServerMessage, ClientError> {
        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(request_id.clone(), tx);

        // Check if the reader task has already marked the connection as dead.
        // The DashMap lock from `insert` above synchronizes with the lock from
        // `clear` in `reader_task`, so if `clear` ran before our insert the
        // flag is guaranteed to be visible here.
        if self.disconnected.load(Ordering::Acquire) {
            self.pending_requests.clear();
            return Err(ClientError::Disconnected);
        }

        if self.outbound_tx.send(msg).await.is_err() {
            self.pending_requests.remove(&request_id);
            return Err(ClientError::Disconnected);
        }

        let result = match rx.with_timeout(REQUEST_TIMEOUT).await {
            Ok(Ok(inner)) => inner,
            Ok(Err(_)) => return Err(ClientError::ResponseChannelClosed),
            Err(_) => {
                // Timed out — clean up and send abort.
                self.pending_requests.remove(&request_id);
                self.send_abort(&request_id);
                return Err(ClientError::Timeout(REQUEST_TIMEOUT));
            }
        };

        // Unwrap the inner Result (reader task may send Err for decode failures).
        let response = result?;

        // Convert server-reported ErrorResponse into ClientError so callers
        // only need to match on success variants.
        if let Some(server_message::Message::Error(ref e)) = response.message {
            return Err(ClientError::ServerError {
                code: e.code(),
                message: e.message.clone(),
            });
        }

        Ok(response)
    }

    /// Sends an `Abort` notification for the given request ID.
    fn send_abort(&self, request_id_to_abort: &RequestId) {
        let msg = ClientMessage::notification(notification::Message::Abort(Abort {
            request_id_to_abort: request_id_to_abort.to_string(),
        }));
        self.send_notification(msg);
    }

    /// Sends a message without registering a pending request (fire-and-forget).
    fn send_notification(&self, msg: ClientMessage) {
        // Use try_send to avoid blocking; if the channel is full or closed,
        // the notification is best-effort.
        if let Err(e) = self.outbound_tx.try_send(msg) {
            log::debug!("Failed to send notification (best-effort): {e}");
        }
    }

    /// Background task that writes `ClientMessage`s to the underlying stream.
    async fn writer_task(
        writer: impl AsyncWrite + TransportStream,
        outbound_rx: async_channel::Receiver<ClientMessage>,
        pending_requests: Arc<
            DashMap<RequestId, oneshot::Sender<Result<ServerMessage, ClientError>>>,
        >,
    ) {
        let mut writer = futures::io::BufWriter::new(writer);
        while let Ok(msg) = outbound_rx.recv().await {
            if let Err(e) = protocol::write_client_message(&mut writer, &msg).await {
                let request_id = RequestId::from(msg.request_id);
                if !e.is_write_recoverable() {
                    log::error!("Writer task fatal error: request_id={request_id}: {e}");
                    pending_requests.clear();
                    break;
                }
                log::error!("Writer task: request_id={request_id}: {e}");
                // Drop the sender so the caller receives ResponseChannelClosed.
                pending_requests.remove(&request_id);
            }
        }
    }

    /// Background task that reads `ServerMessage`s and resolves pending
    /// requests by `request_id`, or converts push messages to events.
    ///
    /// Sends `ClientEvent::Disconnected` as the final event when the
    /// connection is lost.
    async fn reader_task(
        reader: impl AsyncRead + TransportStream,
        pending_requests: Arc<
            DashMap<RequestId, oneshot::Sender<Result<ServerMessage, ClientError>>>,
        >,
        event_tx: async_channel::Sender<ClientEvent>,
        disconnected: Arc<AtomicBool>,
    ) {
        let mut reader = futures::io::BufReader::new(reader);
        loop {
            match protocol::read_server_message(&mut reader).await {
                Ok(msg) => {
                    let request_id = RequestId::from(msg.request_id.clone());
                    if request_id.is_empty() {
                        // Push message — convert to a domain event and forward.
                        if let Some(event) = Self::push_message_to_event(msg) {
                            if event_tx.send(event).await.is_err() {
                                log::warn!("Event channel closed, dropping push message");
                            }
                        }
                    } else {
                        match pending_requests.remove(&request_id) {
                            Some((_, tx)) => {
                                // Ignore send failure — the caller may have dropped the receiver.
                                let _ = tx.send(Ok(msg));
                            }
                            _ => {
                                log::warn!(
                                    "Received unexpected response with request_id={request_id}"
                                );
                            }
                        }
                    }
                }
                Err(ProtocolError::Decode(ref err, Some(ref request_id))) => {
                    match pending_requests.remove(request_id) {
                        Some((_, tx)) => {
                            log::warn!(
                                "Reader task: malformed response \
                             (request_id={request_id}): {err}"
                            );
                            let _ = tx.send(Err(ClientError::Protocol(ProtocolError::Decode(
                                err.clone(),
                                Some(request_id.clone()),
                            ))));
                        }
                        _ => {
                            log::warn!(
                                "Reader task: malformed response for \
                             unknown request (request_id={request_id}): {err}"
                            );
                        }
                    }
                }
                Err(ProtocolError::Decode(ref err, None)) => {
                    log::warn!(
                        "Reader task: skipping malformed response \
                         (no parseable request_id): {err}"
                    );
                    let _ = event_tx.send(ClientEvent::MessageDecodingError).await;
                }
                Err(e) if e.is_read_recoverable() => {
                    log::warn!("Reader task: skipping message: {e}");
                }
                Err(e) => {
                    match e {
                        ProtocolError::UnexpectedEof => {
                            log::info!("Reader task: server disconnected (EOF)");
                        }
                        _ => log::error!("Reader task fatal error: {e}"),
                    }
                    break;
                }
            }
        }

        // Mark the connection as dead so that any new `send_request` calls
        // fail immediately rather than hanging forever. This prevents a race
        // where `pending_requests.clear()` runs before `send_request` has
        // inserted its oneshot entry.
        disconnected.store(true, Ordering::Release);

        // Notify all pending requests that the connection is gone.
        pending_requests.clear();

        // Signal disconnection as the final event.
        let _ = event_tx.send(ClientEvent::Disconnected).await;
    }
}

/// Spawns a background task that reads lines from the server's stderr and
/// forwards them to the client's logging.
#[cfg(not(target_family = "wasm"))]
pub fn spawn_stderr_forwarder(
    stderr: impl AsyncRead + TransportStream,
    executor: &executor::Background,
) {
    use futures::io::AsyncBufReadExt;
    use futures::StreamExt;

    executor
        .spawn(async move {
            let reader = futures::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Some(Ok(line)) = lines.next().await {
                log::info!("[remote_server] {line}");
            }
        })
        .detach();
}

#[cfg(test)]
#[path = "../client_tests.rs"]
mod tests;
