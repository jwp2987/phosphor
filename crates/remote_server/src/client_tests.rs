use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::proto::{
    ClientMessage, CodebaseIndexStatus, CodebaseIndexStatusState, CodebaseIndexStatusUpdated,
    CodebaseIndexStatusesSnapshot, ErrorCode, FileSystemEntryKind, GetDiffState,
    GetDiffStateResponse, GitCommitChainMode, GitCommitChainRequest, GitCommitChainResponse,
    GitCommitChainSuccess, GitCreatePrRequest, GitCreatePrResponse, GitOpDelta, GitOpError,
    GitPullRequest, GitPullResponse, GitPushRequest, GitPushResponse, HostScopedRequest, InitializeResponse, Notification,
    OpenBufferResponse, PrInfo, ReadFileChunkResponse, ReadFileChunkSuccess,
    ResolvePathResponse, ResolvePathSuccess, RunCommandResponse, RunCommandSuccess, ServerMessage,
    SessionScopedRequest, WriteFileChunkResponse, WriteFileChunkSuccess, client_message,
    git_commit_chain_response, git_create_pr_response, git_pull_response, git_push_response, host_scoped_request,
    notification, read_file_chunk_response, resolve_path_response, run_command_response,
    server_message, session_scoped_request, write_file_chunk_response,
};
use crate::protocol;
use warp_core::SessionId;
use warpui::r#async::executor;

use super::*;

/// Generic mock server: loops reading ClientMessages and responds using the
/// provided closure. Exits cleanly on EOF.
async fn mock_server_with<F>(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
    responder: F,
) where
    F: Fn(&ClientMessage) -> server_message::Message,
{
    loop {
        match protocol::read_client_message(&mut reader).await {
            Ok(msg) => {
                let response = ServerMessage {
                    request_id: msg.request_id.clone(),
                    message: Some(responder(&msg)),
                };
                protocol::write_server_message(&mut writer, &response)
                    .await
                    .unwrap();
            }
            Err(protocol::ProtocolError::UnexpectedEof) => break,
            Err(e) => panic!("mock server error: {e}"),
        }
    }
}

fn not_enabled_codebase_status(repo_path: &str) -> CodebaseIndexStatus {
    CodebaseIndexStatus {
        repo_path: repo_path.to_string(),
        state: CodebaseIndexStatusState::NotEnabled.into(),
        last_updated_epoch_millis: Some(123),
        progress_completed: None,
        progress_total: None,
        failure_message: None,
        root_hash: None,
    }
}

/// Unwraps a `ClientMessage` sent as `SessionScoped`, panicking with a useful
/// message otherwise.
fn unwrap_session_scoped(msg: &ClientMessage) -> &session_scoped_request::Message {
    match &msg.message {
        Some(client_message::Message::SessionScoped(SessionScopedRequest { message: Some(m) })) => {
            m
        }
        other => panic!("Expected SessionScoped, got {other:?}"),
    }
}

/// Sets up a duplex stream, spawns `mock_server_with` with the given responder,
/// and returns a connected `RemoteServerClient`, its event receiver, and the
/// background executor (which must be kept alive for the test duration).
fn setup_mock_client<F>(
    responder: F,
) -> (
    RemoteServerClient,
    async_channel::Receiver<ClientEvent>,
    executor::Background,
)
where
    F: Fn(&ClientMessage) -> server_message::Message + Send + 'static,
{
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);

    tokio::spawn(mock_server_with(
        server_read.compat(),
        server_write.compat_write(),
        responder,
    ));

    let executor = executor::Background::default();
    let (client, event_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);
    (client, event_rx, executor)
}

#[tokio::test]
async fn initialize_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|_| {
        server_message::Message::InitializeResponse(InitializeResponse {
            server_version: "test-0.1.0".to_string(),
            host_id: "test-host-id".to_string(),
        })
    });

    let resp = client
        .initialize(None, ClientPreferences::default())
        .await
        .unwrap();
    assert_eq!(resp.server_version, "test-0.1.0");
    assert_eq!(resp.host_id, "test-host-id");
}

#[tokio::test]
async fn initialize_sends_empty_auth_token_when_none() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match &msg.message {
            Some(client_message::Message::SessionScoped(SessionScopedRequest {
                message: Some(session_scoped_request::Message::Initialize(init)),
            })) => {
                assert!(init.auth_token.is_empty());
            }
            other => panic!("Expected Initialize, got {other:?}"),
        }
        server_message::Message::InitializeResponse(InitializeResponse {
            server_version: "test-0.1.0".to_string(),
            host_id: "test-host-id".to_string(),
        })
    });

    client
        .initialize(None, ClientPreferences::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn initialize_sends_auth_token_when_provided() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match &msg.message {
            Some(client_message::Message::SessionScoped(SessionScopedRequest {
                message: Some(session_scoped_request::Message::Initialize(init)),
            })) => {
                assert_eq!(init.auth_token, "secret-token");
            }
            other => panic!("Expected Initialize, got {other:?}"),
        }
        server_message::Message::InitializeResponse(InitializeResponse {
            server_version: "test-0.1.0".to_string(),
            host_id: "test-host-id".to_string(),
        })
    });

    client
        .initialize(Some("secret-token"), ClientPreferences::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn authenticate_sends_fire_and_forget_message() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, _server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let executor = executor::Background::default();
    let (client, _event_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);

    client.authenticate("rotated-secret");

    let msg = protocol::read_client_message(&mut server_read.compat())
        .await
        .unwrap();
    match msg.message {
        Some(client_message::Message::Notification(Notification {
            message: Some(notification::Message::Authenticate(auth)),
        })) => {
            assert_eq!(auth.auth_token, "rotated-secret");
        }
        other => panic!("Expected Authenticate, got {other:?}"),
    }
}

#[tokio::test]
async fn disconnected_on_closed_stream() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    // Drop the server side immediately.
    drop(server_stream);

    let (client_read, client_write) = tokio::io::split(client_stream);
    let executor = executor::Background::default();
    let (client, disconnect_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);

    // An initialize call on a dead stream must complete with an error rather than hang.
    let result = client.initialize(None, ClientPreferences::default()).await;
    assert!(result.is_err());

    // The reader task should detect EOF and emit a Disconnected event.
    let event = disconnect_rx.recv().await.unwrap();
    assert!(matches!(event, ClientEvent::Disconnected));

    // After the Disconnected event has been observed, the reader task has
    // already stored `true` into the atomic flag (it does the store before
    // sending the event), so callers can rely on `is_disconnected()` to
    // short-circuit further requests. #438 dependent feature 2.
    assert!(client.is_disconnected());
}

#[tokio::test]
async fn is_disconnected_starts_false() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|_| {
        server_message::Message::InitializeResponse(InitializeResponse {
            server_version: "test-0.1.0".to_string(),
            host_id: "test-host-id".to_string(),
        })
    });

    assert!(!client.is_disconnected());
}

/// Direct coverage for #438 dependent feature 1 (`RemoteAgentContextSnapshot`):
/// a snapshot pushed by the server (empty request_id) is converted to a
/// `ClientEvent`. No daemon in this fork sends this push yet (see the proto
/// doc comment / issue #353) — this only proves the client-side plumbing.
#[tokio::test]
async fn remote_agent_context_snapshot_push_becomes_client_event() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    drop(server_read);

    let executor = executor::Background::default();
    let (_client, event_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);
    let mut writer = server_write.compat_write();

    protocol::write_server_message(
        &mut writer,
        &ServerMessage {
            request_id: String::new(),
            message: Some(server_message::Message::RemoteAgentContextSnapshot(
                crate::proto::RemoteAgentContextSnapshot {
                    revision: 7,
                    home_dir: "/home/user".to_string(),
                    skills: vec![crate::proto::RemoteSkillProto {
                        path: "/home/user/.agents/skills/test/SKILL.md".to_string(),
                        content: "skill content".to_string(),
                        source: Some(crate::proto::remote_skill_proto::Source::Home(
                            crate::proto::HomeSkillMetadata {},
                        )),
                    }],
                    global_rules: vec![crate::proto::RemoteContextFileProto {
                        path: "/home/user/.agents/AGENTS.md".to_string(),
                        content: "rule content".to_string(),
                    }],
                },
            )),
        },
    )
    .await
    .unwrap();
    writer.flush().await.unwrap();

    match event_rx.recv().await.unwrap() {
        ClientEvent::RemoteAgentContextSnapshotReceived { snapshot } => {
            assert_eq!(snapshot.revision, 7);
            assert_eq!(snapshot.skills[0].content, "skill content");
            assert_eq!(snapshot.global_rules[0].content, "rule content");
        }
        other => panic!("Expected RemoteAgentContextSnapshotReceived, got {other:?}"),
    }
}

/// Direct coverage for #438 dependent feature 5 (manager-layer host-scoped
/// dispatch): `send_host_scoped` queues the message without registering a
/// `pending_requests` entry, and it reaches the wire with the host-scoped
/// envelope intact.
#[tokio::test]
async fn send_host_scoped_returns_ok_when_connected() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, _server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let executor = executor::Background::default();
    let (client, _event_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);

    let msg = ClientMessage::host_scoped(
        "req-host-1".to_string(),
        host_scoped_request::Message::WriteFile(WriteFile {
            path: "/tmp/foo.txt".to_string(),
            content: "hello".to_string(),
        }),
    );

    // On a healthy connection, dispatch succeeds (the message is queued).
    assert!(client.send_host_scoped(msg).is_ok());

    // The queued message is written to the server with the host-scoped envelope.
    let received = protocol::read_client_message(&mut server_read.compat())
        .await
        .unwrap();
    assert_eq!(received.request_id, "req-host-1");
    match &received.message {
        Some(client_message::Message::HostScoped(HostScopedRequest {
            message: Some(host_scoped_request::Message::WriteFile(write)),
        })) => {
            assert_eq!(write.path, "/tmp/foo.txt");
            assert_eq!(write.content, "hello");
        }
        other => panic!("Expected WriteFile host-scoped request, got {other:?}"),
    }
}

/// Direct coverage for #438 dependent feature 3: a malformed response whose
/// `request_id` doesn't match any pending session-scoped request (i.e. it's
/// the response to a host-scoped request) emits `HostScopedDecodeFailed`
/// instead of only being logged, so the manager can fail that pending
/// request promptly instead of letting it hang until the request timeout.
#[tokio::test]
async fn malformed_host_scoped_response_emits_decode_failed_event() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    drop(server_read);

    let executor = executor::Background::default();
    let (_client, event_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);
    let mut server_write = server_write.compat_write();

    // Field 1 (string): tag=0x0a, length=15, "host-req-decode", then invalid
    // trailing bytes (field 1, reserved wire type 7) so prost decode fails
    // while `try_extract_request_id` still recovers the request_id.
    let mut payload = Vec::new();
    payload.push(0x0a);
    payload.push(15);
    payload.extend_from_slice(b"host-req-decode");
    payload.extend_from_slice(&[0x0F, 0x01]);

    let len = payload.len() as u32;
    server_write.write_all(&len.to_le_bytes()).await.unwrap();
    server_write.write_all(&payload).await.unwrap();
    server_write.flush().await.unwrap();

    match event_rx.recv().await.unwrap() {
        ClientEvent::HostScopedDecodeFailed { request_id } => {
            assert_eq!(request_id.to_string(), "host-req-decode");
        }
        other => panic!("Expected HostScopedDecodeFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn run_command_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        let command = match &msg.message {
            Some(client_message::Message::SessionScoped(SessionScopedRequest {
                message: Some(session_scoped_request::Message::RunCommand(req)),
            })) => req.command.clone(),
            other => panic!("Expected RunCommand, got {other:?}"),
        };
        server_message::Message::RunCommandResponse(RunCommandResponse {
            result: Some(run_command_response::Result::Success(RunCommandSuccess {
                stdout: format!("output of: {command}").into_bytes(),
                stderr: Vec::new(),
                exit_code: Some(0),
            })),
        })
    });

    let resp = client
        .run_command(
            SessionId::from(42u64),
            "echo hello".to_string(),
            None,
            Default::default(),
        )
        .await
        .unwrap();
    let success = match resp.result {
        Some(run_command_response::Result::Success(s)) => s,
        other => panic!("Expected RunCommandSuccess, got {other:?}"),
    };
    assert_eq!(success.stdout, b"output of: echo hello");
    assert!(success.stderr.is_empty());
    assert_eq!(success.exit_code, Some(0));
}

#[tokio::test]
async fn resolve_path_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::ResolvePath(req)),
            })) => {
                assert_eq!(req.path, "~/project");
            }
            other => panic!("Expected ResolvePath, got {other:?}"),
        }
        server_message::Message::ResolvePathResponse(ResolvePathResponse {
            result: Some(resolve_path_response::Result::Success(ResolvePathSuccess {
                canonical_path: "/home/me/project".to_string(),
                kind: FileSystemEntryKind::Directory as i32,
                size_bytes: None,
            })),
        })
    });

    let resp = client.resolve_path("~/project".to_string()).await.unwrap();
    let Some(resolve_path_response::Result::Success(success)) = resp.result else {
        panic!("expected resolve path success");
    };
    assert_eq!(success.canonical_path, "/home/me/project");
    assert_eq!(success.kind, FileSystemEntryKind::Directory as i32);
}

#[tokio::test]
async fn read_file_chunk_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::ReadFileChunk(req)),
            })) => {
                assert_eq!(req.path, "/tmp/blob.bin");
                assert_eq!(req.offset, 4);
                assert_eq!(req.max_bytes, 2);
            }
            other => panic!("Expected ReadFileChunk, got {other:?}"),
        }
        server_message::Message::ReadFileChunkResponse(ReadFileChunkResponse {
            result: Some(read_file_chunk_response::Result::Success(
                ReadFileChunkSuccess {
                    bytes: vec![5, 6],
                    next_offset: 6,
                    total_size: Some(8),
                    eof: false,
                },
            )),
        })
    });

    let resp = client
        .read_file_chunk("/tmp/blob.bin".to_string(), 4, 2)
        .await
        .unwrap();
    let Some(read_file_chunk_response::Result::Success(success)) = resp.result else {
        panic!("expected read chunk success");
    };
    assert_eq!(success.bytes, vec![5, 6]);
    assert_eq!(success.next_offset, 6);
}

#[tokio::test]
async fn write_file_chunk_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::WriteFileChunk(req)),
            })) => {
                assert_eq!(req.path, "/tmp/blob.bin");
                assert_eq!(req.offset, 0);
                assert_eq!(req.bytes, vec![1, 2, 3]);
                assert!(req.truncate);
            }
            other => panic!("Expected WriteFileChunk, got {other:?}"),
        }
        server_message::Message::WriteFileChunkResponse(WriteFileChunkResponse {
            result: Some(write_file_chunk_response::Result::Success(
                WriteFileChunkSuccess { next_offset: 3 },
            )),
        })
    });

    let resp = client
        .write_file_chunk("/tmp/blob.bin".to_string(), 0, vec![1, 2, 3], true, None)
        .await
        .unwrap();
    let Some(write_file_chunk_response::Result::Success(success)) = resp.result else {
        panic!("expected write chunk success");
    };
    assert_eq!(success.next_offset, 3);
}

#[tokio::test]
async fn concurrent_in_flight_requests() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|_| {
        server_message::Message::InitializeResponse(InitializeResponse {
            server_version: "test-0.1.0".to_string(),
            host_id: "test-host-id".to_string(),
        })
    });
    let client = std::sync::Arc::new(client);

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = std::sync::Arc::clone(&client);
        handles.push(tokio::spawn(async move {
            c.initialize(None, ClientPreferences::default())
                .await
                .expect("concurrent initialize failed")
        }));
    }

    for h in handles {
        let resp = h.await.unwrap();
        assert_eq!(resp.server_version, "test-0.1.0");
        assert_eq!(resp.host_id, "test-host-id");
    }
}

/// Simulates a server that reads raw bytes, sends an error response for
/// malformed messages where the request_id is parseable, then continues
/// processing valid messages.
async fn mock_server_with_error_handling(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
) {
    loop {
        match protocol::read_client_message(&mut reader).await {
            Ok(msg) => {
                let response = ServerMessage {
                    request_id: msg.request_id,
                    message: Some(server_message::Message::InitializeResponse(
                        InitializeResponse {
                            server_version: "test-0.1.0".to_string(),
                            host_id: "test-host-id".to_string(),
                        },
                    )),
                };
                protocol::write_server_message(&mut writer, &response)
                    .await
                    .unwrap();
            }
            Err(protocol::ProtocolError::Decode(_, Some(ref id))) => {
                let error_response = ServerMessage {
                    request_id: id.to_string(),
                    message: Some(server_message::Message::Error(
                        crate::proto::ErrorResponse {
                            code: ErrorCode::InvalidRequest.into(),
                            message: "malformed message".to_string(),
                        },
                    )),
                };
                protocol::write_server_message(&mut writer, &error_response)
                    .await
                    .unwrap();
            }
            Err(protocol::ProtocolError::Decode(_, None)) => {}
            Err(protocol::ProtocolError::UnexpectedEof) => break,
            Err(e) => panic!("mock server error: {e}"),
        }
    }
}

/// Sends a corrupted protobuf with a valid request_id to the server,
/// verifying the server responds with an ErrorResponse for that request_id.
#[tokio::test]
async fn server_returns_error_for_malformed_message_with_parseable_id() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);

    tokio::spawn(mock_server_with_error_handling(
        server_read.compat(),
        server_write.compat_write(),
    ));

    // Manually construct a corrupted message with a valid request_id field
    // followed by bytes that cause a prost decode failure.
    let mut payload = Vec::new();
    // Field 1 (string): tag=0x0a, length=15, "malformed-req-1"
    payload.push(0x0a);
    payload.push(15);
    payload.extend_from_slice(b"malformed-req-1");
    // Invalid trailing bytes: field tag with reserved wire type 7 causes
    // prost to fail, but our try_extract_request_id stops after field 1.
    payload.extend_from_slice(&[0x0F, 0x01]); // field 1, wire type 7 (invalid)

    // Write the corrupted message with length prefix.
    let mut client_write = client_write.compat_write();
    let len = payload.len() as u32;
    client_write.write_all(&len.to_le_bytes()).await.unwrap();
    client_write.write_all(&payload).await.unwrap();
    client_write.flush().await.unwrap();

    // Read the error response from the server.
    let mut client_reader = futures::io::BufReader::new(client_read.compat());
    let response: ServerMessage = protocol::read_server_message(&mut client_reader)
        .await
        .unwrap();

    assert_eq!(response.request_id, "malformed-req-1");
    match response.message {
        Some(server_message::Message::Error(e)) => {
            assert_eq!(e.code(), ErrorCode::InvalidRequest);
        }
        other => panic!("expected ErrorResponse, got: {other:?}"),
    }
}

#[tokio::test]
async fn git_commit_chain_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        let req = match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::GitCommitChain(req)),
            })) => req.clone(),
            other => panic!("Expected GitCommitChain, got {other:?}"),
        };
        assert_eq!(req.repo_path, "/remote/repo");
        assert_eq!(req.message, "a commit");
        assert_eq!(req.mode(), GitCommitChainMode::CommitAndCreatePr);
        server_message::Message::GitCommitChainResponse(GitCommitChainResponse {
            result: Some(git_commit_chain_response::Result::Success(
                GitCommitChainSuccess {
                    delta: Some(GitOpDelta {
                        unpushed_commits: Vec::new(),
                        upstream_ref: Some("origin/feature".to_string()),
                    }),
                    pr_info: Some(PrInfo {
                        number: 7,
                        url: "https://example.test/pr/7".to_string(),
                        state: "OPEN".to_string(),
                        draft: false,
                        base_branch: "main".to_string(),
                    }),
                },
            )),
        })
    });

    let resp = client
        .git_commit_chain(GitCommitChainRequest {
            repo_path: "/remote/repo".to_string(),
            message: "a commit".to_string(),
            include_unstaged: true,
            branch: "feature".to_string(),
            mode: GitCommitChainMode::CommitAndCreatePr as i32,
            autogenerate_pr_content: false,
        })
        .await
        .unwrap();
    let success = match resp.result {
        Some(git_commit_chain_response::Result::Success(s)) => s,
        other => panic!("Expected GitCommitChainSuccess, got {other:?}"),
    };
    assert_eq!(
        success.delta.unwrap().upstream_ref.as_deref(),
        Some("origin/feature")
    );
    assert_eq!(success.pr_info.unwrap().number, 7);
}

#[tokio::test]
async fn git_push_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        let req = match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::GitPush(req)),
            })) => req.clone(),
            other => panic!("Expected GitPush, got {other:?}"),
        };
        assert_eq!(req.repo_path, "/remote/repo");
        assert_eq!(req.branch, "feature");
        server_message::Message::GitPushResponse(GitPushResponse {
            result: Some(git_push_response::Result::Success(GitOpDelta {
                unpushed_commits: Vec::new(),
                upstream_ref: Some("origin/feature".to_string()),
            })),
        })
    });

    let resp = client
        .git_push(GitPushRequest {
            repo_path: "/remote/repo".to_string(),
            branch: "feature".to_string(),
        })
        .await
        .unwrap();
    match resp.result {
        Some(git_push_response::Result::Success(delta)) => {
            assert_eq!(delta.upstream_ref.as_deref(), Some("origin/feature"));
        }
        other => panic!("Expected GitPush success, got {other:?}"),
    }
}

#[tokio::test]
async fn git_pull_round_trip() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        let req = match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::GitPull(req)),
            })) => req.clone(),
            other => panic!("Expected GitPull, got {other:?}"),
        };
        assert_eq!(req.repo_path, "/remote/repo");
        assert_eq!(req.branch, "feature");
        server_message::Message::GitPullResponse(GitPullResponse {
            result: Some(git_pull_response::Result::Success(GitOpDelta {
                unpushed_commits: Vec::new(),
                upstream_ref: Some("origin/feature".to_string()),
            })),
        })
    });

    let resp = client
        .git_pull(GitPullRequest {
            repo_path: "/remote/repo".to_string(),
            branch: "feature".to_string(),
        })
        .await
        .unwrap();
    match resp.result {
        Some(git_pull_response::Result::Success(delta)) => {
            assert_eq!(delta.upstream_ref.as_deref(), Some("origin/feature"));
        }
        other => panic!("Expected GitPull success, got {other:?}"),
    }
}

#[tokio::test]
async fn git_pull_round_trip_error() {
    // Mirrors how a diverged (non-fast-forward) history comes back: Stage 1
    // never merges, so it's a `GitOpError`, not a conflict payload.
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        let req = match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::GitPull(req)),
            })) => req.clone(),
            other => panic!("Expected GitPull, got {other:?}"),
        };
        assert_eq!(req.branch, "feature");
        server_message::Message::GitPullResponse(GitPullResponse {
            result: Some(git_pull_response::Result::Error(GitOpError {
                message: "Not possible to fast-forward, aborting.".to_string(),
            })),
        })
    });

    let resp = client
        .git_pull(GitPullRequest {
            repo_path: "/remote/repo".to_string(),
            branch: "feature".to_string(),
        })
        .await
        .unwrap();
    match resp.result {
        Some(git_pull_response::Result::Error(e)) => {
            assert_eq!(e.message, "Not possible to fast-forward, aborting.");
        }
        other => panic!("Expected GitPull error, got {other:?}"),
    }
}

#[tokio::test]
async fn git_create_pr_round_trip_error() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        let req = match &msg.message {
            Some(client_message::Message::HostScoped(HostScopedRequest {
                message: Some(host_scoped_request::Message::GitCreatePr(req)),
            })) => req.clone(),
            other => panic!("Expected GitCreatePr, got {other:?}"),
        };
        assert_eq!(req.repo_path, "/remote/repo");
        assert!(!req.autogenerate_content);
        server_message::Message::GitCreatePrResponse(GitCreatePrResponse {
            result: Some(git_create_pr_response::Result::Error(GitOpError {
                message: "branch has no upstream".to_string(),
            })),
        })
    });

    let resp = client
        .git_create_pr(GitCreatePrRequest {
            repo_path: "/remote/repo".to_string(),
            branch: "feature".to_string(),
            autogenerate_content: false,
        })
        .await
        .unwrap();
    match resp.result {
        Some(git_create_pr_response::Result::Error(e)) => {
            assert_eq!(e.message, "branch has no upstream");
        }
        other => panic!("Expected GitCreatePr error, got {other:?}"),
    }
}

// ── Ported from the pinned oracle (`02b53fcd8:crates/remote_server/src/client_tests.rs`) ──
// The `SessionScoped`/`HostScoped` envelope split (issue #509) landed after these were
// last measured absent, unblocking them. Adapted for two proto-shape differences from
// the pin: `get_diff_state`/`open_buffer` take the request struct / a single path arg
// respectively here rather than the pin's separate positional args, and `OpenBuffer`
// has no `force_reload` field on this fork's wire (so there is nothing to assert there).

#[tokio::test]
async fn get_diff_state_round_trips_as_session_scoped() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match unwrap_session_scoped(msg) {
            session_scoped_request::Message::GetDiffState(req) => {
                assert_eq!(req.repo_path, "/repo");
            }
            other => panic!("Expected GetDiffState, got {other:?}"),
        }
        server_message::Message::GetDiffStateResponse(GetDiffStateResponse { result: None })
    });

    let resp = client
        .get_diff_state(GetDiffState {
            repo_path: "/repo".to_string(),
            mode: Some(crate::proto::DiffMode {
                mode: Some(crate::proto::diff_mode::Mode::Head(
                    crate::proto::DiffModeHead {},
                )),
            }),
        })
        .await
        .expect("get_diff_state should succeed");
    assert!(resp.result.is_none());
}

#[tokio::test]
async fn open_buffer_round_trips_as_session_scoped() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match unwrap_session_scoped(msg) {
            session_scoped_request::Message::OpenBuffer(req) => {
                assert_eq!(req.path, "/tmp/f.txt");
                // The plain open path must not ask the server to discard its
                // in-memory buffer state; only conflict resolution does.
                assert!(!req.force_reload);
            }
            other => panic!("Expected OpenBuffer, got {other:?}"),
        }
        server_message::Message::OpenBufferResponse(OpenBufferResponse {
            content: String::new(),
            server_version: 0,
        })
    });

    let resp = client
        .open_buffer("/tmp/f.txt".to_string(), false)
        .await
        .expect("open_buffer should succeed");
    assert_eq!(resp.content, "");
}

/// `force_reload` is what makes the client's "accept the server's copy"
/// conflict resolution work, so it must survive the trip over the wire.
#[tokio::test]
async fn open_buffer_forwards_force_reload() {
    let (client, _disconnect_rx, _executor) = setup_mock_client(|msg| {
        match unwrap_session_scoped(msg) {
            session_scoped_request::Message::OpenBuffer(req) => {
                assert_eq!(req.path, "/tmp/f.txt");
                assert!(req.force_reload);
            }
            other => panic!("Expected OpenBuffer, got {other:?}"),
        }
        server_message::Message::OpenBufferResponse(OpenBufferResponse {
            content: "from disk".to_string(),
            server_version: 7,
        })
    });

    let resp = client
        .open_buffer("/tmp/f.txt".to_string(), true)
        .await
        .expect("open_buffer should succeed");
    assert_eq!(resp.content, "from disk");
    assert_eq!(resp.server_version, 7);
}

/// A session-scoped request on a connection that has already dropped resolves
/// promptly with a transport error (no hang), because `pending_requests` is
/// cleared on disconnect.
#[tokio::test]
async fn get_diff_state_on_dead_connection_errors_promptly() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    drop(server_stream);

    let (client_read, client_write) = tokio::io::split(client_stream);
    let executor = executor::Background::default();
    let (client, disconnect_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);

    // Drain the Disconnected event so the reader-task teardown is observed.
    let _ = disconnect_rx.recv().await;

    let result = client
        .get_diff_state(GetDiffState {
            repo_path: "/repo".to_string(),
            mode: Some(crate::proto::DiffMode {
                mode: Some(crate::proto::diff_mode::Mode::Head(
                    crate::proto::DiffModeHead {},
                )),
            }),
        })
        .await;
    assert!(result.is_err());
}

/// Pin: `codebase_index_push_messages_become_client_events`. Unblocked by the
/// D2 local/BYOP codebase-indexing port (`crates/remote_server/src/codebase_index_proto.rs`),
/// which carries this proto and its `ClientEvent` variants unchanged from the
/// pin. Adapted only for `RemoteServerClient::new`'s 3-tuple return here (no
/// separate failure channel) and the domain-type conversion the fork's client
/// applies to pushed statuses (`RemoteCodebaseIndexStatus`, same `repo_path` field).
#[tokio::test]
async fn codebase_index_push_messages_become_client_events() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    drop(server_read);

    let executor = executor::Background::default();
    let (_client, event_rx, _host_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);
    let mut writer = server_write.compat_write();

    protocol::write_server_message(
        &mut writer,
        &ServerMessage {
            request_id: String::new(),
            message: Some(server_message::Message::CodebaseIndexStatusesSnapshot(
                CodebaseIndexStatusesSnapshot {
                    statuses: vec![not_enabled_codebase_status("/repo")],
                },
            )),
        },
    )
    .await
    .unwrap();
    protocol::write_server_message(
        &mut writer,
        &ServerMessage {
            request_id: String::new(),
            message: Some(server_message::Message::CodebaseIndexStatusUpdated(
                CodebaseIndexStatusUpdated {
                    status: Some(not_enabled_codebase_status("/repo")),
                },
            )),
        },
    )
    .await
    .unwrap();
    writer.flush().await.unwrap();

    match event_rx.recv().await.unwrap() {
        ClientEvent::CodebaseIndexStatusesSnapshotReceived { statuses } => {
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].repo_path, "/repo");
        }
        other => panic!("Expected CodebaseIndexStatusesSnapshotReceived, got {other:?}"),
    }
    match event_rx.recv().await.unwrap() {
        ClientEvent::CodebaseIndexStatusUpdated { status } => {
            assert_eq!(status.repo_path, "/repo");
        }
        other => panic!("Expected CodebaseIndexStatusUpdated, got {other:?}"),
    }
}
