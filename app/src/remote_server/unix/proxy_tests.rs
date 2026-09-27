use warp_core::channel::ChannelState;

use super::*;

/// `socket_path`/`pid_path` must delegate to `setup::daemon_socket_name`/
/// `daemon_pid_name` rather than hardcoding a filename -- hardcoding
/// `"server.sock"`/`"server.pid"` directly (bypassing the version-partitioned
/// names those `setup` functions already computed) was the live-path defect
/// this module exists to fix; see TODO.md's "Daemon sockets are not
/// version-partitioned in practice" entry.
#[test]
fn socket_and_pid_paths_use_the_versioned_names() {
    let identity_key = "test-identity";
    let dir = setup::remote_server_daemon_dir(identity_key);
    let expanded = shellexpand::tilde(&dir).into_owned();

    assert_eq!(
        socket_path(identity_key),
        PathBuf::from(&expanded).join(setup::daemon_socket_name())
    );
    assert_eq!(
        pid_path(identity_key),
        PathBuf::from(&expanded).join(setup::daemon_pid_name())
    );
}

/// An unversioned build (no `GIT_RELEASE_TAG`, the default for a test binary)
/// must never sweep `server.sock`/`server.pid`: those are its OWN current
/// names in that case (see `setup::daemon_socket_name`'s doc comment), not a
/// predecessor's, and a peer unversioned dev-build daemon using them
/// legitimately must be left alone.
#[test]
fn cleanup_is_a_no_op_for_an_unversioned_build() {
    assert!(
        setup::version_hash().is_none(),
        "test setup: no GIT_RELEASE_TAG should be baked into a test binary"
    );

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_PID_NAME), "1").unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_SOCKET_NAME), "").unwrap();

    cleanup_stale_unversioned_daemon(dir.path());

    assert!(dir.path().join(UNVERSIONED_PID_NAME).exists());
    assert!(dir.path().join(UNVERSIONED_SOCKET_NAME).exists());
}

/// Once this build has a version (matching a real release binary), cleanup
/// must remove a dead old-format daemon's leftover files -- the whole point
/// of the function.
///
/// `ChannelState::set_app_version` is process-global, gated on the
/// `test-util` feature this crate's dev-dependency on `warp_core` enables.
/// Safe under this project's nextest-per-test-process runs (see
/// `autoupdate::mod::fail_closed_tests`'s note on the same API); reset at the
/// end regardless so a plain `cargo test` run does not leak it into this
/// file's other tests.
#[test]
fn cleanup_removes_a_dead_unversioned_daemons_files_once_versioned() {
    ChannelState::set_app_version(Some("v0.2026.09.27.00.00.stable_01"));
    assert!(
        setup::version_hash().is_some(),
        "test setup: version_hash must be Some for this test to exercise anything"
    );

    let dir = tempfile::tempdir().unwrap();
    // A PID no live process can plausibly hold, so `check_daemon_running`
    // reads it as dead without depending on timing or process cleanup.
    std::fs::write(dir.path().join(UNVERSIONED_PID_NAME), "999999999").unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_SOCKET_NAME), "").unwrap();

    cleanup_stale_unversioned_daemon(dir.path());

    assert!(!dir.path().join(UNVERSIONED_PID_NAME).exists());
    assert!(!dir.path().join(UNVERSIONED_SOCKET_NAME).exists());

    ChannelState::set_app_version(None);
}

/// A live daemon's files (real PID: this test process's own) must survive
/// cleanup even when versioned -- cleanup only removes what it can positively
/// confirm is dead.
#[test]
fn cleanup_leaves_a_live_unversioned_daemons_files_alone() {
    ChannelState::set_app_version(Some("v0.2026.09.27.00.00.stable_01"));
    assert!(setup::version_hash().is_some());

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(UNVERSIONED_PID_NAME),
        std::process::id().to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_SOCKET_NAME), "").unwrap();

    cleanup_stale_unversioned_daemon(dir.path());

    assert!(dir.path().join(UNVERSIONED_PID_NAME).exists());
    assert!(dir.path().join(UNVERSIONED_SOCKET_NAME).exists());

    ChannelState::set_app_version(None);
}
