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

/// A reader that blocks until released: stands in for ssh's stdin while the client is idle.
struct BlockedInput(std::sync::mpsc::Receiver<()>);

impl std::io::Read for BlockedInput {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}

/// A writer that hands what it was given to the test.
struct SinkOutput(std::sync::mpsc::Sender<Vec<u8>>);

impl std::io::Write for SinkOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = self.0.send(buf.to_vec());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// #2195: the daemon dying (its socket end closing) ends the proxy at once, even though the
/// client has sent nothing and stdin is still open: the client's connection then closes and it
/// can fall back to the shell path without first failing a request. The bytes the daemon sent
/// before it died still reach the output.
#[test]
fn bridge_ends_when_the_daemon_dies_with_stdin_still_open() {
    let (proxy_end, mut daemon_end) = std::os::unix::net::UnixStream::pair().expect("pair");
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let (out_tx, out_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<anyhow::Result<()>>();
    std::thread::spawn(move || {
        let _ = done_tx.send(bridge_streams(
            proxy_end,
            BlockedInput(release_rx),
            SinkOutput(out_tx),
        ));
    });
    std::io::Write::write_all(&mut daemon_end, b"hello").expect("daemon sends");
    drop(daemon_end); // the daemon is killed
    let res = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the proxy must exit when the daemon closes, without waiting for stdin");
    assert!(res.is_ok());
    let got: Vec<u8> = out_rx.try_iter().flatten().collect();
    assert_eq!(got, b"hello");
    drop(release_tx); // let the parked input thread end
}

/// Client end first (stdin EOF): the bridge still ends and shuts the daemon socket.
#[test]
fn bridge_ends_when_stdin_closes() {
    let (proxy_end, mut daemon_end) = std::os::unix::net::UnixStream::pair().expect("pair");
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let (out_tx, _out_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<anyhow::Result<()>>();
    std::thread::spawn(move || {
        let _ = done_tx.send(bridge_streams(
            proxy_end,
            BlockedInput(release_rx),
            SinkOutput(out_tx),
        ));
    });
    drop(release_tx); // stdin reaches EOF
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the proxy must exit on stdin EOF")
        .expect("ok");
    let mut buf = [0u8; 1];
    assert_eq!(
        std::io::Read::read(&mut daemon_end, &mut buf).expect("read"),
        0,
        "the daemon end sees the socket closed"
    );
}
