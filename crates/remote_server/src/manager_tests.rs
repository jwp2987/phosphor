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
