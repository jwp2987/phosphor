//! Unit tests for `manager.rs`.
//!
//! Most of this file covers pure-function helpers. The host-scoped request
//! tracking tests (#438 dependent features 1/4/5) exercise
//! `RemoteServerManager` itself via `warpui_core::App::test` + `add_model`,
//! matching the pinned oracle's `manager_tests.rs` — `warpui`'s `test-util`
//! feature (enabled in `Cargo.toml`'s `[dev-dependencies]`) provides a
//! lightweight headless `App` for exactly this.

use std::sync::Mutex;

use futures::channel::oneshot;
use warp_core::SessionId;
use warp_util::standardized_path::StandardizedPath;
#[cfg(unix)]
use warpui::r#async::executor;
use warpui::{App, ModelHandle};

#[cfg(unix)]
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::RemotePtySessionId;

use super::*;

// ---------------------------------------------------------------------------
// version_is_compatible
// ---------------------------------------------------------------------------

#[test]
fn version_compat_both_tagged_and_equal() {
    assert!(version_is_compatible(
        Some("v0.2026.05.10.stable"),
        "v0.2026.05.10.stable",
    ));
}

#[test]
fn version_compat_both_tagged_and_different() {
    assert!(!version_is_compatible(
        Some("v0.2026.05.10.stable"),
        "v0.2026.05.10.preview",
    ));
}

#[test]
fn version_compat_both_untagged() {
    // Client has no GIT_RELEASE_TAG (cargo run), and the server also reports
    // an empty string (`script/deploy_remote_server` dev deployment): treated
    // as compatible, keeping the local dev loop unaffected.
    assert!(version_is_compatible(None, ""));
}

#[test]
fn version_compat_client_tagged_server_untagged() {
    // Client is a release build, server is a dev deployment → treated as
    // incompatible, normally triggering the reinstall flow.
    assert!(!version_is_compatible(Some("v0.2026.05.10.stable"), ""));
}

#[test]
fn version_compat_client_untagged_server_tagged() {
    // **Key scenario**: the Zap client has no tag (cargo build), and the
    // server is a release downloaded from the official CDN (with a tag). The
    // original helper judges this incompatible, triggering
    // `remove_remote_server_binary` → an infinite loop. This test only
    // documents that `version_is_compatible`'s own behavior is unchanged; the
    // actual "skip checking" logic is handled by
    // [`should_enforce_remote_version_check`].
    assert!(!version_is_compatible(None, "v0.2026.05.10.stable"));
}

// ---------------------------------------------------------------------------
// should_enforce_remote_version_check
// ---------------------------------------------------------------------------

#[test]
fn enforce_version_check_skipped_on_oss() {
    // When Zap temporarily reuses the official release binary, the client and
    // server versions never match, so strict checking must be skipped.
    assert!(!should_enforce_remote_version_check(Channel::Oss));
}

#[test]
fn enforce_version_check_kept_on_official_channels() {
    // On official channels, the client and server either both come from the
    // same release CI run, or both come from a local
    // `script/deploy_remote_server` deployment, so strict checking is still
    // necessary — preserving the original stale-binary self-healing path.
    for channel in [
        Channel::Stable,
        Channel::Preview,
        Channel::Dev,
        Channel::Local,
        Channel::Integration,
    ] {
        assert!(
            should_enforce_remote_version_check(channel),
            "channel {channel:?} should still enforce version check"
        );
    }
}

// ---------------------------------------------------------------------------
// Host-scoped request tracking (#438 dependent features 1/4/5)
// ---------------------------------------------------------------------------

#[test]
fn abort_host_request_removes_pending_request_and_resolves_caller() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("test-host".to_string());
        let request_id = crate::protocol::RequestId::new();
        let (result_tx, result_rx) = oneshot::channel();

        manager.update(&mut app, |manager, _ctx| {
            manager.pending_host_requests.insert(
                request_id.clone(),
                PendingHostRequest {
                    host_id,
                    dispatched_session_id: SessionId::from(1u64),
                    result_tx,
                },
            );
            manager.abort_host_request(&request_id);
            assert!(!manager.pending_host_requests.contains_key(&request_id));
        });

        assert!(matches!(
            result_rx.await.expect("manager should resolve caller"),
            Err(HostRequestError::Aborted)
        ));
    });
}

#[test]
fn abort_host_request_is_a_no_op_for_unknown_request_id() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let request_id = crate::protocol::RequestId::new();

        // No pending entry was ever registered — must not panic.
        manager.update(&mut app, |manager, _ctx| {
            manager.abort_host_request(&request_id);
        });
    });
}

/// Pin: `remote_agent_context_snapshot_is_a_host_scoped_manager_event`.
#[test]
fn remote_agent_context_snapshot_is_a_host_scoped_manager_event() {
    let host_id = HostId::new("test-host".to_string());
    let event = RemoteServerManagerEvent::RemoteAgentContextSnapshot {
        host_id,
        snapshot: crate::proto::RemoteAgentContextSnapshot {
            revision: 1,
            home_dir: "/home/user".to_string(),
            skills: Vec::new(),
            global_rules: Vec::new(),
        },
    };
    assert!(event.session_id().is_none());
}

/// Builds a minimal `RemoteAgentContextSnapshot` with the given revision,
/// for exercising `accept_remote_agent_context_snapshot`'s dedup logic
/// without caring about skills/rules content.
fn test_snapshot(revision: u64) -> crate::proto::RemoteAgentContextSnapshot {
    crate::proto::RemoteAgentContextSnapshot {
        revision,
        home_dir: "/home/user".to_string(),
        skills: Vec::new(),
        global_rules: Vec::new(),
    }
}

#[test]
fn remote_agent_context_snapshot_is_queryable_after_being_accepted() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("test-host".to_string());

        manager.update(&mut app, |manager, _ctx| {
            assert!(manager.remote_agent_context_snapshot(&host_id).is_none());

            assert!(manager.accept_remote_agent_context_snapshot(&host_id, test_snapshot(2)));
            assert_eq!(
                manager
                    .remote_agent_context_snapshot(&host_id)
                    .expect("snapshot should be stored")
                    .revision,
                2
            );
        });
    });
}

#[test]
fn remote_agent_context_snapshot_revisions_are_deduplicated_per_host() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("test-host".to_string());
        let other_host_id = HostId::new("other-host".to_string());

        manager.update(&mut app, |manager, ctx| {
            assert!(manager.accept_remote_agent_context_snapshot(&host_id, test_snapshot(2)));
            // Same revision again — rejected, stored snapshot unchanged.
            assert!(!manager.accept_remote_agent_context_snapshot(&host_id, test_snapshot(2)));
            // Older revision — rejected.
            assert!(!manager.accept_remote_agent_context_snapshot(&host_id, test_snapshot(1)));
            // Newer revision — accepted.
            assert!(manager.accept_remote_agent_context_snapshot(&host_id, test_snapshot(3)));
            // A different host has independent dedup state.
            assert!(manager.accept_remote_agent_context_snapshot(&other_host_id, test_snapshot(1)));

            manager.handle_host_disconnected(host_id.clone(), ctx);
            // Disconnecting the host clears its stored snapshot, so a
            // previously-seen revision is accepted again.
            assert!(manager.remote_agent_context_snapshot(&host_id).is_none());
            assert!(manager.accept_remote_agent_context_snapshot(&host_id, test_snapshot(3)));
        });
    });
}

#[test]
fn start_ripgrep_search_without_connected_host_resolves_immediately() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("missing-host".to_string());
        let pending = manager.update(&mut app, |manager, _ctx| {
            manager.start_ripgrep_search(
                &host_id,
                RipgrepSearchParams {
                    pattern: "needle".to_string(),
                    roots: vec![StandardizedPath::try_new("/repo").unwrap()],
                    ignore_case: false,
                    multiline: false,
                    max_matches: 100,
                },
            )
        });

        assert!(matches!(
            pending.result().await,
            Err(HostRequestError::AllSessionsDisconnected)
        ));
    });
}

#[test]
fn host_request_handle_without_connected_host_resolves_immediately() {
    // Mirrors `start_ripgrep_search_without_connected_host_resolves_immediately`:
    // `HostRequestHandle` bounces through `send_host_request` the same way, so
    // with no connected session for the host it must fail fast rather than
    // hang or panic. Exercises `host_request_handle` → `HostRequestHandle::send`
    // → `HostRequestHandle::read_file_context` end to end.
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("missing-host".to_string());
        let handle = manager.update(&mut app, |manager, _ctx| {
            manager.host_request_handle(&host_id)
        });

        let result = handle
            .read_file_context(crate::proto::ReadFileContextRequest {
                files: vec![crate::proto::ReadFileContextFile {
                    path: "/tmp/does-not-matter".to_string(),
                    line_ranges: vec![],
                }],
                max_file_bytes: None,
                max_batch_bytes: None,
            })
            .await;

        assert!(matches!(
            result,
            Err(HostRequestError::AllSessionsDisconnected)
        ));
    });
}

#[test]
fn search_remote_codebase_without_connected_host_resolves_immediately() {
    // Same shape as `host_request_handle_without_connected_host_resolves_immediately`,
    // for the codebase-search RPC (TODO.md "UNWIRED-CODE AUDIT 2026-08-10" finding #5):
    // exercises `host_request_handle` → `HostRequestHandle::send` →
    // `HostRequestHandle::search_remote_codebase` end to end, with no daemon on the
    // other end to answer. This is the transport-failure path
    // `app::ai::codebase_retrieval::RetrievalFailure::HostUnreachable` is built on: a
    // caller must get a fast, typed error here, never a hang.
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("missing-host".to_string());
        let handle = manager.update(&mut app, |manager, _ctx| {
            manager.host_request_handle(&host_id)
        });

        let result = handle
            .search_remote_codebase(crate::proto::SearchRemoteCodebase {
                repo_path: "/repo".to_string(),
                query: "where is the parser".to_string(),
            })
            .await;

        assert!(matches!(
            result,
            Err(HostRequestError::AllSessionsDisconnected)
        ));
    });
}

#[test]
fn handle_host_disconnected_fails_pending_host_requests_for_that_host_only() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let host_id = HostId::new("host-a".to_string());
        let other_host_id = HostId::new("host-b".to_string());
        let request_id = crate::protocol::RequestId::new();
        let other_request_id = crate::protocol::RequestId::new();
        let (result_tx, result_rx) = oneshot::channel();
        let (other_result_tx, other_result_rx) = oneshot::channel();

        manager.update(&mut app, |manager, ctx| {
            manager.pending_host_requests.insert(
                request_id.clone(),
                PendingHostRequest {
                    host_id: host_id.clone(),
                    dispatched_session_id: SessionId::from(1u64),
                    result_tx,
                },
            );
            manager.pending_host_requests.insert(
                other_request_id.clone(),
                PendingHostRequest {
                    host_id: other_host_id.clone(),
                    dispatched_session_id: SessionId::from(2u64),
                    result_tx: other_result_tx,
                },
            );

            manager.handle_host_disconnected(host_id, ctx);

            assert!(!manager.pending_host_requests.contains_key(&request_id));
            assert!(
                manager
                    .pending_host_requests
                    .contains_key(&other_request_id),
                "a different host's pending request must not be touched"
            );
        });

        assert!(matches!(
            result_rx.await.expect("manager should resolve caller"),
            Err(HostRequestError::AllSessionsDisconnected)
        ));
        // The other host's request is still pending — dropping its sender
        // (end of test) resolves it with a channel-closed error, which we
        // don't care about here; we only assert it wasn't pre-empted above.
        drop(other_result_rx);
    });
}

// ---------------------------------------------------------------------------
// Remote pty session pushes (groundwork; see docs/design/moth-parliament.md,
// "Scoping session ownership")
// ---------------------------------------------------------------------------

/// `RemoteTransport` double for constructing a `Connected` session in tests.
/// None of its methods are exercised: the routing tests below only need a
/// value of the right type to occupy `RemoteSessionState::Connected`'s
/// `transport` field.
#[cfg(unix)]
#[derive(Debug)]
struct UnusedTransport;

#[cfg(unix)]
impl RemoteTransport for UnusedTransport {
    fn detect_platform(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RemotePlatform, String>> + Send>>
    {
        unreachable!("not exercised by this test")
    }

    fn run_preinstall_check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<PreinstallCheckResult, String>> + Send>,
    > {
        unreachable!("not exercised by this test")
    }

    fn check_binary(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, String>> + Send>> {
        unreachable!("not exercised by this test")
    }

    fn check_has_old_binary(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send>> {
        unreachable!("not exercised by this test")
    }

    fn install_binary(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        unreachable!("not exercised by this test")
    }

    fn connect(
        &self,
        _executor: Arc<executor::Background>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Connection>> + Send>>
    {
        unreachable!("not exercised by this test")
    }

    fn remove_remote_server_binary(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>> {
        unreachable!("not exercised by this test")
    }
}

/// Builds a `RemoteServerClient` backed by an already-severed in-memory pipe
/// (the peer end is dropped immediately). Sufficient for tests that only
/// need a value of the right type to sit in `RemoteSessionState::Connected`
/// -- nothing here sends or receives a message. The returned `Background`
/// must be kept alive for as long as the client is in use, matching
/// `client_tests.rs`'s `setup_mock_client`.
#[cfg(unix)]
fn disconnected_client() -> (Arc<RemoteServerClient>, executor::Background) {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    drop(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let executor = executor::Background::default();
    let (client, _event_rx, _host_response_rx) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);
    (Arc::new(client), executor)
}

/// Inserts a `Connected` session for `session_id`/`host_id` directly into
/// the manager's session map, bypassing the real connect flow (which needs a
/// live SSH subprocess). `forward_client_event` only reads this map via
/// `host_id_for_session`, so a stand-in client/transport/child is enough.
/// Returns the `Background` backing the stand-in client -- keep it alive for
/// the rest of the test.
#[cfg(unix)]
fn insert_connected_session(
    manager: &mut RemoteServerManager,
    session_id: SessionId,
    host_id: HostId,
) -> executor::Background {
    let (client, executor) = disconnected_client();
    let child = async_process::Command::new("true")
        .spawn()
        .expect("short-lived process starts");
    manager.sessions.insert(
        session_id,
        RemoteSessionState::Connected {
            client,
            host_id,
            identity_key: "test-identity".to_string(),
            _child: child,
            control_path: None,
            transport: Arc::new(UnusedTransport),
        },
    );
    executor
}

/// Collects every `RemoteServerManagerEvent` emitted by `handle`.
#[cfg(unix)]
fn subscribe_to_manager_events(
    app: &mut App,
    handle: &ModelHandle<RemoteServerManager>,
) -> Arc<Mutex<Vec<RemoteServerManagerEvent>>> {
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_for_subscription = received.clone();
    app.update(|ctx| {
        ctx.subscribe_to_model(handle, move |_, event, _| {
            received_for_subscription
                .lock()
                .expect("events mutex should not be poisoned")
                .push(event.clone());
        });
    });
    received
}

/// Pin-adjacent coverage: a `SessionOutputChunkPush` -- one of the two
/// session-shaped pushes added for session-ownership groundwork -- is
/// forwarded by `forward_client_event` into `RemoteServerManagerEvent::
/// SessionOutputChunk`, carrying both the session's `host_id` and its
/// `remote_pty_session_id`. Fails if `forward_client_event`'s match arm for
/// `ClientEvent::SessionOutputChunkReceived` is removed, or if it drops or
/// swaps either id.
#[cfg(unix)]
#[test]
fn session_output_chunk_push_is_routed_with_host_and_session_id() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let received = subscribe_to_manager_events(&mut app, &manager);

        let session_id = SessionId::from(7u64);
        let host_id = HostId::new("test-host".to_string());
        let remote_pty_session_id = RemotePtySessionId::from("session-abc".to_string());

        let _executor = manager.update(&mut app, |manager, ctx| {
            let executor = insert_connected_session(manager, session_id, host_id.clone());
            manager.forward_client_event(
                session_id,
                ClientEvent::SessionOutputChunkReceived {
                    remote_session_id: remote_pty_session_id.clone(),
                    data: b"hello".to_vec(),
                },
                ctx,
            );
            executor
        });

        let received = received
            .lock()
            .expect("events mutex should not be poisoned");
        assert_eq!(received.len(), 1);
        match &received[0] {
            RemoteServerManagerEvent::SessionOutputChunk {
                host_id: got_host_id,
                remote_pty_session_id: got_session_id,
                data,
            } => {
                assert_eq!(*got_host_id, host_id);
                assert_eq!(*got_session_id, remote_pty_session_id);
                assert_eq!(data, b"hello");
            }
            other => panic!("expected SessionOutputChunk, got {other:?}"),
        }
    });
}

/// Same shape as the output-chunk test above, for the session-exit push.
/// Fails if `forward_client_event`'s match arm for
/// `ClientEvent::SessionExitedReceived` is removed, or if it drops or swaps
/// either id.
#[cfg(unix)]
#[test]
fn session_exited_push_is_routed_with_host_and_session_id() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let received = subscribe_to_manager_events(&mut app, &manager);

        let session_id = SessionId::from(9u64);
        let host_id = HostId::new("test-host-2".to_string());
        let remote_pty_session_id = RemotePtySessionId::from("session-def".to_string());

        let _executor = manager.update(&mut app, |manager, ctx| {
            let executor = insert_connected_session(manager, session_id, host_id.clone());
            manager.forward_client_event(
                session_id,
                ClientEvent::SessionExitedReceived {
                    remote_session_id: remote_pty_session_id.clone(),
                    exit_code: Some(0),
                },
                ctx,
            );
            executor
        });

        let received = received
            .lock()
            .expect("events mutex should not be poisoned");
        assert_eq!(received.len(), 1);
        match &received[0] {
            RemoteServerManagerEvent::SessionExited {
                host_id: got_host_id,
                remote_pty_session_id: got_session_id,
                exit_code,
            } => {
                assert_eq!(*got_host_id, host_id);
                assert_eq!(*got_session_id, remote_pty_session_id);
                assert_eq!(*exit_code, Some(0));
            }
            other => panic!("expected SessionExited, got {other:?}"),
        }
    });
}

// ---------------------------------------------------------------------------
// connected_host_ids (host registry dashboard refresh, "it re-probes on
// open" -- docs/design/moth-parliament.md)
// ---------------------------------------------------------------------------

/// Breaks if `connected_host_ids` stops reading `Connected` sessions (e.g. an
/// empty stub), or starts reporting a session that never reached `Connected`
/// (nothing populates `sessions` with any other state in this test, so a
/// false positive here could only come from `connected_host_ids` itself
/// misreading the map).
#[cfg(unix)]
#[test]
fn connected_host_ids_reports_only_connected_sessions() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);

        let host_a = HostId::new("host-a".to_string());
        let host_b = HostId::new("host-b".to_string());

        let _executors = manager.update(&mut app, |manager, _ctx| {
            let exec_a = insert_connected_session(manager, SessionId::from(1u64), host_a.clone());
            let exec_b = insert_connected_session(manager, SessionId::from(2u64), host_b.clone());
            (exec_a, exec_b)
        });

        manager.read(&app, |manager, _ctx| {
            let mut connected: Vec<HostId> = manager.connected_host_ids().cloned().collect();
            connected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            assert_eq!(connected, vec![host_a.clone(), host_b.clone()]);
        });
    });
}

/// A session that never reached `Connected` (still `Initializing`, or
/// removed entirely) must not appear. Breaks if `connected_host_ids` widens
/// its match beyond the `Connected` variant.
#[cfg(unix)]
#[test]
fn connected_host_ids_excludes_non_connected_session_states() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);

        manager.update(&mut app, |manager, _ctx| {
            manager
                .sessions
                .insert(SessionId::from(3u64), RemoteSessionState::Disconnected);
        });

        manager.read(&app, |manager, _ctx| {
            assert_eq!(manager.connected_host_ids().count(), 0);
        });
    });
}
