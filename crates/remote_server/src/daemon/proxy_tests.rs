use super::*;

/// `socket_path`/`pid_path` must be built from the shared naming helpers and live in the
/// identity directory under the base directory, rather than hardcoding a filename: hardcoding
/// `"server.sock"` (bypassing the version-partitioned name) was the live-path defect the
/// versioned names exist to fix.
#[test]
fn socket_and_pid_paths_use_the_versioned_names() {
    let identity_key = "test-identity";
    let dir = daemon_dir(identity_key);
    let hash = naming::helper_version_hash();

    assert_eq!(
        socket_path(identity_key),
        dir.join(format!("server-{hash}.sock"))
    );
    assert_eq!(
        pid_path(identity_key),
        dir.join(format!("server-{hash}.pid"))
    );
    assert_eq!(dir.file_name().unwrap().len(), 8);
}

/// An unversioned build must never sweep `server.sock`/`server.pid`: those are its OWN current
/// names in that case, not a predecessor's, and a peer unversioned dev-build daemon using them
/// legitimately must be left alone. (The helper is always versioned; the rule is kept testable.)
#[test]
fn cleanup_is_a_no_op_for_an_unversioned_build() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_PID_NAME), "1").unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_SOCKET_NAME), "").unwrap();

    cleanup_stale_unversioned_daemon_if(false, dir.path());

    assert!(dir.path().join(UNVERSIONED_PID_NAME).exists());
    assert!(dir.path().join(UNVERSIONED_SOCKET_NAME).exists());
}

/// Once this build has a version, cleanup must remove a dead old-format daemon's leftover files.
#[test]
fn cleanup_removes_a_dead_unversioned_daemons_files_once_versioned() {
    let dir = tempfile::tempdir().unwrap();
    // A PID no live process can plausibly hold, so `check_daemon_running`
    // reads it as dead without depending on timing or process cleanup.
    std::fs::write(dir.path().join(UNVERSIONED_PID_NAME), "999999999").unwrap();
    std::fs::write(dir.path().join(UNVERSIONED_SOCKET_NAME), "").unwrap();

    cleanup_stale_unversioned_daemon(dir.path());

    assert!(!dir.path().join(UNVERSIONED_PID_NAME).exists());
    assert!(!dir.path().join(UNVERSIONED_SOCKET_NAME).exists());
}

/// A live daemon's files (real PID: this test process's own) must survive cleanup even when
/// versioned -- cleanup only removes what it can positively confirm is dead.
#[test]
fn cleanup_leaves_a_live_unversioned_daemons_files_alone() {
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
}

/// The daemon's readiness byte wakes the proxy; no polling and no timing assumption.
#[test]
fn ready_pipe_wakes_on_the_daemon_signal() {
    let mut pipe = ReadyPipe::new().expect("pipe");
    let write = pipe.take_write().expect("write end");
    let mut file = std::fs::File::from(write);
    std::io::Write::write_all(&mut file, &[1]).expect("signal");
    // Hold the write end open past the wait: readiness must come from the byte, not from EOF.
    pipe.wait_for_daemon(Duration::from_secs(30))
        .expect("ready signal");
    drop(file);
}

/// A daemon that exits before binding closes the pipe: the proxy learns of the failure
/// immediately (EOF) rather than after the give-up timeout.
#[test]
fn ready_pipe_reports_daemon_exit_as_eof() {
    let mut pipe = ReadyPipe::new().expect("pipe");
    drop(pipe.take_write());
    let err = pipe
        .wait_for_daemon(Duration::from_secs(30))
        .expect_err("EOF is a failure");
    assert!(err.to_string().contains("exited before it was ready"));
}

/// The give-up safety net: a daemon that neither signals nor exits still ends the wait.
#[test]
fn ready_pipe_gives_up_when_no_signal_arrives() {
    let mut pipe = ReadyPipe::new().expect("pipe");
    let _keep_open = pipe.take_write();
    // `wait_for_daemon` closes its own copy; `_keep_open` is the "daemon" holding it.
    let err = pipe
        .wait_for_daemon(Duration::from_millis(50))
        .expect_err("no signal");
    assert!(err.to_string().contains("timed out"));
}
