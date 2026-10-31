//! Unit tests for `manager.rs`.
//!
//! Most of this file covers pure-function helpers. The host-scoped request
//! tracking tests (#438 dependent features 1/4/5) exercise
//! `RemoteServerManager` itself via `warpui_core::App::test` + `add_model`,
//! matching the pinned oracle's `manager_tests.rs` — `warpui`'s `test-util`
//! feature (enabled in `Cargo.toml`'s `[dev-dependencies]`) provides a
//! lightweight headless `App` for exactly this.

use futures::channel::oneshot;
use warp_core::SessionId;
use warp_util::standardized_path::StandardizedPath;
use warpui::App;

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
// #1083: one owner for `ssh -O exit`
// ---------------------------------------------------------------------------

/// The `ExitShell` path never stops the master (the ssh wrapper does, right after its session
/// client returns); only a discarded pane does.
#[test]
fn only_a_discarded_pane_stops_the_master() {
    let path = PathBuf::from("/home/u/.ssh/42");
    assert_eq!(
        MasterTeardown::Stop.path_to_stop(Some(path.clone())),
        Some(path.clone())
    );
    assert_eq!(
        MasterTeardown::LeaveToWrapper.path_to_stop(Some(path)),
        None
    );
    assert_eq!(MasterTeardown::Stop.path_to_stop(None), None);
}

/// #1083 end to end on the manager: deregistering after `ExitShell` drops the session but does
/// not run `ssh -O exit` (whose teardown would also remove the socket file), so the wrapper's
/// own `-O exit` is the only one and cannot race a second one from the app.
#[cfg(unix)]
#[test]
fn deregister_after_shell_exit_leaves_the_master_to_the_wrapper() {
    App::test((), |mut app| async move {
        let manager = app.add_model(RemoteServerManager::new);
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("42");
        // A dead socket: a stop would fail fast and then remove it.
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
        let session_id = SessionId::from(42u64);
        manager.update(&mut app, |manager, ctx| {
            manager.sessions.insert(
                session_id,
                RemoteSessionState::AwaitingExitStatus {
                    control_path: Some(socket.clone()),
                    host_id: HostId::new("h".to_string()),
                },
            );
            manager.deregister_session_after_shell_exit(session_id, ctx);
            assert!(!manager.sessions.contains_key(&session_id));
        });
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(
            socket.exists(),
            "the ExitShell path must not stop (or clean up after) the wrapper's master"
        );
    });
}

// ---------------------------------------------------------------------------
// run_binary_check_probes: setup probes overlap (#1136)
// ---------------------------------------------------------------------------

mod binary_check_probes {
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use futures::channel::oneshot;

    use crate::setup::{PreinstallStatus, RemoteArch, RemoteLibc};

    use super::*;

    type Fut<T> = Pin<Box<dyn std::future::Future<Output = T> + Send>>;

    /// A transport whose probes resolve immediately, except that `detect_platform` can be held
    /// back until `run_preinstall_check` has started.
    #[derive(Debug)]
    struct FakeTransport {
        platform: Result<RemotePlatform, String>,
        preinstall: Result<PreinstallCheckResult, String>,
        preinstall_started_tx: Mutex<Option<oneshot::Sender<()>>>,
        detect_gate_rx: Mutex<Option<oneshot::Receiver<()>>>,
        preinstall_calls: AtomicUsize,
    }

    impl FakeTransport {
        fn new(
            platform: Result<RemotePlatform, String>,
            preinstall: Result<PreinstallCheckResult, String>,
        ) -> Self {
            Self {
                platform,
                preinstall,
                preinstall_started_tx: Mutex::new(None),
                detect_gate_rx: Mutex::new(None),
                preinstall_calls: AtomicUsize::new(0),
            }
        }

        /// `detect_platform` will not resolve until `run_preinstall_check` has been polled.
        fn gate_detect_on_preinstall(self) -> Self {
            let (tx, rx) = oneshot::channel();
            *self.preinstall_started_tx.lock().unwrap() = Some(tx);
            *self.detect_gate_rx.lock().unwrap() = Some(rx);
            self
        }
    }

    impl RemoteTransport for FakeTransport {
        fn detect_platform(&self) -> Fut<Result<RemotePlatform, String>> {
            let gate = self.detect_gate_rx.lock().unwrap().take();
            let platform = self.platform.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    let _ = gate.await;
                }
                platform
            })
        }

        fn run_preinstall_check(&self) -> Fut<Result<PreinstallCheckResult, String>> {
            self.preinstall_calls.fetch_add(1, Ordering::SeqCst);
            let started = self.preinstall_started_tx.lock().unwrap().take();
            let result = self.preinstall.clone();
            Box::pin(async move {
                if let Some(started) = started {
                    let _ = started.send(());
                }
                result
            })
        }

        fn check_binary(&self) -> Fut<Result<bool, String>> {
            Box::pin(async { Ok(true) })
        }

        fn check_has_old_binary(&self) -> Fut<anyhow::Result<bool>> {
            Box::pin(async { Ok(false) })
        }

        fn install_binary(&self) -> Fut<Result<(), String>> {
            Box::pin(async { Err("unused".to_string()) })
        }

        fn connect(
            &self,
            _executor: Arc<warpui::r#async::executor::Background>,
        ) -> Fut<anyhow::Result<Connection>> {
            Box::pin(async { Err(anyhow::anyhow!("unused")) })
        }

        fn remove_remote_server_binary(&self) -> Fut<anyhow::Result<()>> {
            Box::pin(async { Ok(()) })
        }
    }

    fn platform(os: RemoteOs) -> RemotePlatform {
        RemotePlatform {
            os,
            arch: RemoteArch::X86_64,
        }
    }

    fn supported() -> PreinstallCheckResult {
        PreinstallCheckResult {
            status: PreinstallStatus::Supported,
            libc: RemoteLibc::Unknown,
            raw: "status=supported".to_string(),
        }
    }

    /// Runs the probes on a helper thread so that a regression to serial probing (which can
    /// never finish with a gated transport) fails the test instead of hanging it.
    fn run(transport: FakeTransport) -> (BinaryCheckProbes, Arc<FakeTransport>) {
        let transport = Arc::new(transport);
        let (tx, rx) = std::sync::mpsc::channel();
        let for_thread = transport.clone();
        std::thread::spawn(move || {
            let probes = futures::executor::block_on(run_binary_check_probes(
                &*for_thread,
                SessionId::from(1u64),
            ));
            let _ = tx.send(probes);
        });
        let probes = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("setup probes did not run concurrently");
        (probes, transport)
    }

    #[test]
    fn preinstall_check_starts_without_waiting_for_platform_detection() {
        // Fails on the old code: `run_preinstall_check` was only called after `detect_platform`
        // resolved, but detection here resolves only once preinstall has started.
        let transport = FakeTransport::new(Ok(platform(RemoteOs::Linux)), Ok(supported()))
            .gate_detect_on_preinstall();
        let (probes, _) = run(transport);
        assert_eq!(probes.platform, Some(platform(RemoteOs::Linux)));
        assert_eq!(probes.preinstall, Some(supported()));
        assert_eq!(probes.check_result, Ok(true));
        assert!(!probes.has_old_binary);
    }

    #[test]
    fn preinstall_result_is_dropped_for_non_linux_hosts() {
        let transport = FakeTransport::new(Ok(platform(RemoteOs::MacOs)), Ok(supported()));
        let (probes, transport) = run(transport);
        assert_eq!(probes.platform, Some(platform(RemoteOs::MacOs)));
        assert_eq!(probes.preinstall, None);
        // Started in parallel with everything else, but its answer is ignored.
        assert_eq!(transport.preinstall_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn preinstall_result_is_dropped_when_platform_detection_fails() {
        let transport = FakeTransport::new(Err("uname failed".to_string()), Ok(supported()));
        let (probes, _) = run(transport);
        assert_eq!(probes.platform, None);
        assert_eq!(probes.preinstall, None);
    }

    #[test]
    fn preinstall_failure_on_linux_is_inconclusive() {
        let transport = FakeTransport::new(
            Ok(platform(RemoteOs::Linux)),
            Err("ssh timed out".to_string()),
        );
        let (probes, _) = run(transport);
        assert_eq!(probes.platform, Some(platform(RemoteOs::Linux)));
        assert_eq!(probes.preinstall, None);
    }
}
