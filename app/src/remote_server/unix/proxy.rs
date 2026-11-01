//! Remote server proxy — runs over SSH stdio and bridges to the long-lived
//! daemon process via a Unix domain socket.
//!
//! Responsibilities:
//! 1. Acquire an exclusive `flock` on the PID file to serialise concurrent
//!    proxy starts (e.g. two tabs SSH-ing to the same host at the same time).
//! 2. Check whether the daemon is already running (`kill -0`).
//! 3. If not: spawn the daemon subcommand in a new session and wait for its
//!    socket to appear.
//! 4. Connect to the daemon socket and bridge stdin/stdout to it using the
//!    existing 4-byte length-prefixed frame format.

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use super::super::setup;

/// Path to the daemon's Unix domain socket.
///
/// Named via [`setup::daemon_socket_name`], which bakes in a short hash of
/// the app version when one is available (release builds), so an upgraded
/// client can never attach to a daemon started by a different build — the
/// wire protocol between proxy and daemon has no version negotiation of its
/// own, so a mismatched pair would otherwise talk past each other silently.
/// Both `run` (below, the client side) and `run_daemon` (`unix::run_daemon`,
/// which binds the socket this path names) call this, so the two sides can
/// never disagree.
pub(super) fn socket_path(identity_key: &str) -> PathBuf {
    let dir = setup::remote_server_daemon_dir(identity_key);
    let expanded = shellexpand::tilde(&dir).into_owned();
    PathBuf::from(expanded).join(setup::daemon_socket_name())
}

/// Path to the daemon's PID file (also used as the flock target). Versioned
/// the same way as [`socket_path`], for the same reason.
pub(super) fn pid_path(identity_key: &str) -> PathBuf {
    let dir = setup::remote_server_daemon_dir(identity_key);
    let expanded = shellexpand::tilde(&dir).into_owned();
    PathBuf::from(expanded).join(setup::daemon_pid_name())
}

/// Ensures the daemon directory exists with owner-only permissions.
pub(super) fn ensure_private_daemon_dir(path: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, Permissions::from_mode(0o700))?;
    Ok(())
}

/// The unversioned socket/PID filenames every build used before daemon files
/// were version-partitioned (and what [`setup::daemon_socket_name`] /
/// [`setup::daemon_pid_name`] still return for an unversioned build, e.g.
/// plain `cargo run` with no `GIT_RELEASE_TAG`).
const UNVERSIONED_SOCKET_NAME: &str = "server.sock";
const UNVERSIONED_PID_NAME: &str = "server.pid";

/// Best-effort cleanup of a daemon left behind by a build from before daemon
/// files were version-partitioned.
///
/// Runs only when the *current* build is versioned (`setup::version_hash()`
/// is `Some`), so it never touches another unversioned dev build's daemon,
/// which legitimately still uses these names on purpose. A dead PID means the
/// old daemon's process is gone, so its socket/PID files are litter that
/// nothing else will ever clean up: the daemon that created them computed the
/// same unversioned path it always did, has no code that would notice this
/// build moved on, and — being dead — cannot remove them itself the way
/// [`super::run_daemon`]'s own exit path does for its own files. A live PID
/// is left alone rather than guessed at; it will exit on its own idle-grace
/// timeout like any other daemon.
fn cleanup_stale_unversioned_daemon(dir: &std::path::Path) {
    if setup::version_hash().is_none() {
        return;
    }
    let old_pid_path = dir.join(UNVERSIONED_PID_NAME);
    let old_socket_path = dir.join(UNVERSIONED_SOCKET_NAME);
    if !old_pid_path.exists() && !old_socket_path.exists() {
        return;
    }
    if check_daemon_running(&old_pid_path) {
        return;
    }
    log::info!(
        "Proxy: removing stale unversioned daemon files in {}",
        dir.display()
    );
    let _ = std::fs::remove_file(&old_pid_path);
    let _ = std::fs::remove_file(&old_socket_path);
}

/// Entry point for `remote-server-proxy`.
///
/// Ensures the daemon is running, then bridges stdin/stdout to the daemon's
/// Unix socket for the lifetime of this SSH session.
pub fn run(identity_key: &str) -> anyhow::Result<()> {
    let socket_path = socket_path(identity_key);
    let pid_path = pid_path(identity_key);

    // Ensure the parent directory exists.
    if let Some(parent) = socket_path.parent() {
        ensure_private_daemon_dir(parent)?;
        cleanup_stale_unversioned_daemon(parent);
    }

    // ---- Acquire exclusive flock on the PID file --------------------------------
    //
    // This serialises concurrent proxy starts.  If two tabs SSH in at the
    // same time and both see "no daemon running", only one will succeed in
    // forking a daemon; the other will block here, then connect to the one
    // the first proxy started.
    //
    // The lock is released automatically when the File is dropped.
    let pid_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&pid_path)?;
    let pid_fd = pid_file.as_raw_fd();
    flock_wait(pid_fd, libc::LOCK_EX)?;

    // ---- Check whether daemon is already running --------------------------------
    let daemon_running = check_daemon_running(&pid_path);
    if daemon_running {
        log::info!("Proxy: reusing existing daemon");
    } else {
        log::info!("Proxy: no daemon running, will start one");
    }

    if !daemon_running {
        // Remove any stale socket from a previous crash.
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        // Spawn the daemon in a new Unix session so it is detached from
        // the SSH session.  When SSH exits the OS sends SIGHUP to every
        // process in the session's foreground process group.  `setsid()`
        // creates a new session for the child, so the daemon is not in
        // SSH's process group and will not receive that signal.
        let exe = std::env::current_exe()?;
        let mut cmd = command::blocking::Command::new(&exe);
        cmd.arg("remote-server-daemon")
            .arg("--identity-key")
            .arg(identity_key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid(2) is async-signal-safe and has no side effects
        // other than creating a new session.  pre_exec closures run between
        // fork and exec in the child process.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        // Readiness pipe: the daemon writes one byte once its socket is bound and listening.
        // `None` (pipe creation failed) falls back to polling for the socket file.
        let ready = ReadyPipe::new()
            .map_err(|e| log::warn!("Proxy: readiness pipe unavailable, will poll: {e}"))
            .ok();
        if let Some(ready) = &ready {
            cmd.env(READY_FD_ENV, ready.write_fd().to_string());
        }
        cmd.spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn daemon: {e}"))?;

        // Wait for the daemon's socket to be ready before releasing the flock.
        // Holding the lock here prevents a concurrent proxy from acquiring it,
        // reading a stale PID file, and racing to spawn a second daemon.
        match ready {
            Some(ready) => ready.wait_for_daemon(DAEMON_READY_TIMEOUT)?,
            None => wait_for_socket(&socket_path)?,
        }

        flock_wait(pid_fd, libc::LOCK_UN)?;
        drop(pid_file);
    } else {
        // Daemon already running — release the flock and connect.
        flock_wait(pid_fd, libc::LOCK_UN)?;
        drop(pid_file);
    }

    // ---- Bridge stdin/stdout to the daemon socket --------------------------------
    bridge_stdio_to_socket(&socket_path)
}

/// Returns true if the PID stored in `pid_path` belongs to a live process.
fn check_daemon_running(pid_path: &std::path::Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(pid_path) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<libc::pid_t>() else {
        return false;
    };
    // kill(pid, 0) succeeds (returns 0) if the process exists and we can
    // signal it; it fails with ESRCH if the process does not exist.
    // SAFETY: sending signal 0 is always safe — it performs a permission
    // check only and does not deliver an actual signal.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Environment variable carrying the write end of the readiness pipe to the daemon.
pub(super) const READY_FD_ENV: &str = "PHOSPHOR_DAEMON_READY_FD";

/// Give-up safety net for a daemon that neither signals readiness nor exits.
const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// A pipe the daemon uses to tell the proxy "my socket is bound and listening".
///
/// This replaces polling for the socket file every 20 ms: the proxy blocks in `poll(2)` on the
/// read end and wakes exactly when the daemon signals, or when the daemon exits without
/// signalling (EOF, which reports a startup failure at once instead of after a 10 s timeout).
pub(super) struct ReadyPipe {
    read: std::os::unix::io::OwnedFd,
    write: Option<std::os::unix::io::OwnedFd>,
}

impl ReadyPipe {
    pub(super) fn new() -> std::io::Result<Self> {
        use std::os::unix::io::FromRawFd as _;
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: `fds` has room for the two descriptors pipe(2) writes.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: both descriptors were just created and are owned by nobody else.
        let (read, write) = unsafe {
            (
                std::os::unix::io::OwnedFd::from_raw_fd(fds[0]),
                std::os::unix::io::OwnedFd::from_raw_fd(fds[1]),
            )
        };
        // The read end must not leak into the daemon; the write end must (it is inherited).
        // SAFETY: fcntl on a descriptor we own.
        if unsafe { libc::fcntl(read.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            read,
            write: Some(write),
        })
    }

    /// The descriptor number the daemon inherits.
    pub(super) fn write_fd(&self) -> std::os::unix::io::RawFd {
        self.write.as_ref().map_or(-1, |fd| fd.as_raw_fd())
    }

    /// Takes the write end (so a test, or the parent after spawn, can close/use it).
    #[cfg(test)]
    pub(super) fn take_write(&mut self) -> Option<std::os::unix::io::OwnedFd> {
        self.write.take()
    }

    /// Blocks until the daemon signals readiness.  Errors if the daemon exits first (EOF) or
    /// `timeout` elapses (logged: the signal never arrived).
    pub(super) fn wait_for_daemon(mut self, timeout: Duration) -> anyhow::Result<()> {
        // Close our copy of the write end so the daemon's exit is observable as EOF.
        self.write = None;
        let start = instant::Instant::now();
        loop {
            let remaining = timeout.saturating_sub(start.elapsed());
            let mut pfd = libc::pollfd {
                fd: self.read.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ms = remaining.as_millis().min(libc::c_int::MAX as u128) as libc::c_int;
            // SAFETY: `pfd` is a valid pollfd and nfds is 1.
            let r = unsafe { libc::poll(&mut pfd, 1, ms) };
            if r < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                anyhow::bail!("waiting for daemon readiness failed: {err}");
            }
            if r == 0 {
                log::error!("Proxy: daemon gave no readiness signal within {timeout:?}");
                anyhow::bail!("timed out waiting for daemon to become ready");
            }
            let mut byte = [0u8; 1];
            // SAFETY: reading one byte into a one-byte buffer from a descriptor we own.
            let n = unsafe { libc::read(self.read.as_raw_fd(), byte.as_mut_ptr().cast(), 1) };
            return match n {
                1 => {
                    log::info!("Proxy: daemon ready after {:?}", start.elapsed());
                    Ok(())
                }
                0 => anyhow::bail!("daemon exited before it was ready"),
                _ => {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    anyhow::bail!("reading daemon readiness failed: {err}")
                }
            };
        }
    }
}

/// Daemon side: if the proxy passed a readiness pipe, signal that the socket is bound and
/// listening, then close it.  A no-op when the daemon was started some other way.
pub(super) fn signal_ready_from_env() {
    use std::io::Write as _;
    use std::os::unix::io::FromRawFd as _;
    let Some(fd) = std::env::var(READY_FD_ENV)
        .ok()
        .and_then(|v| v.parse::<libc::c_int>().ok())
    else {
        return;
    };
    // The descriptor is closed below, so a stale copy of the variable in a child's environment
    // names nothing.  (`remove_var` is unsafe in edition 2024 and not worth it here.)
    // SAFETY: the proxy created this descriptor for us and passes it nowhere else.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    if let Err(e) = file.write_all(&[1]) {
        log::warn!("Daemon: could not signal readiness: {e}");
    }
    // Dropping `file` closes the descriptor.
}

/// Fallback when no readiness pipe could be created: poll until the daemon's socket file
/// appears or the timeout elapses.  There is no OS event for "a file appeared" short of a
/// watcher on the directory, which is not worth setting up for this rare path.
fn wait_for_socket(socket_path: &std::path::Path) -> anyhow::Result<()> {
    const TIMEOUT: Duration = Duration::from_secs(10);
    const POLL_INTERVAL: Duration = Duration::from_millis(20);
    let start = instant::Instant::now();
    while !socket_path.exists() {
        if start.elapsed() >= TIMEOUT {
            anyhow::bail!(
                "timed out waiting for daemon socket at {}",
                socket_path.display()
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    log::info!("Proxy: daemon socket ready after {:?}", start.elapsed());
    Ok(())
}

/// Calls `flock(2)` with the given operation, retrying on `EINTR`.
///
/// Blocking `flock(LOCK_EX)` can be interrupted by a signal before acquiring
/// the lock; ignoring the return value would cause the proxy to proceed
/// without actually holding the lock.
fn flock_wait(fd: std::os::unix::io::RawFd, operation: libc::c_int) -> anyhow::Result<()> {
    loop {
        // SAFETY: flock(2) is safe to call with a valid fd and a valid operation.
        let ret = unsafe { libc::flock(fd, operation) };
        if ret == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINTR) {
            continue; // Interrupted by signal — retry.
        }
        return Err(anyhow::anyhow!("flock failed: {err}"));
    }
}

/// Connect to the daemon's Unix socket and copy bytes bidirectionally between
/// stdin/stdout and the socket.
///
/// The proxy is protocol-agnostic — it forwards raw bytes without parsing the
/// length-prefixed framing.  The framing is handled at the endpoints (Zap
/// client and daemon).
///
/// **Important**: the stdout direction uses a manual read→write→flush loop
/// instead of `io::copy` because `std::io::stdout()` wraps the fd in a
/// `LineWriter` that only flushes up to the last `\n` byte in each write.
/// For a binary protocol the trailing bytes after the last `0x0a` get stuck
/// in the internal `BufWriter` and are never flushed, causing the client to
/// hang forever waiting for complete messages.
///
/// **Shutdown coordination**: each direction explicitly
/// [`shutdown(Both)`s][Shutdown] the Unix socket when its copy loop
/// returns, which unblocks the other thread's read/write on the same
/// underlying socket. Without this, when the client SIGKILLs the local
/// `ssh ... remote-server-proxy` slave (e.g. on `ExitShell`), sshd
/// closes our stdin but the daemon has no reason to close its end of
/// the Unix socket, so the stdout thread sits forever in a blocking
/// read. That keeps the proxy alive with stdout still open, which
/// keeps the SSH channel half-closed on the server side, which in
/// turn keeps the client's `ssh` ControlMaster from exiting until
/// sshd's session cleanup eventually fires. Shutting the Unix socket
/// here makes teardown deterministic and independent of whatever the
/// daemon is doing.
///
/// [Shutdown]: std::net::Shutdown
fn bridge_stdio_to_socket(socket_path: &std::path::Path) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    use std::net::Shutdown;

    log::info!(
        "Proxy: connecting to daemon socket at {}",
        socket_path.display()
    );
    let stream = std::os::unix::net::UnixStream::connect(socket_path)?;
    log::info!("Proxy: connected, bridging stdio");

    // Each thread holds two clones: one it actively reads/writes, and
    // one used solely to `shutdown(Both)` on exit so the peer thread's
    // blocking call returns. `UnixStream::try_clone` shares the
    // underlying socket, so `shutdown` on any clone tears down both
    // directions for every clone.
    let stream_for_t1 = stream.try_clone()?;
    let stream_shutdown_for_t1 = stream.try_clone()?;
    let stream_for_t2 = stream.try_clone()?;
    let stream_shutdown_for_t2 = stream.try_clone()?;
    drop(stream);

    let t1 = std::thread::Builder::new()
        .name("proxy-stdin-fwd".into())
        .spawn(move || {
            let result = std::io::copy(&mut std::io::stdin(), &mut &stream_for_t1);
            match &result {
                Ok(total) => log::info!(
                    "Proxy: stdin->socket copy ended ({total} bytes); \
                     shutting down socket to unblock peer"
                ),
                Err(e) => log::info!(
                    "Proxy: stdin->socket copy errored ({e}); \
                     shutting down socket to unblock peer"
                ),
            }
            let _ = stream_shutdown_for_t1.shutdown(Shutdown::Both);
            result
        })?;

    // Socket → stdout: flush after every write so that complete protocol
    // frames reach the SSH tunnel without waiting for the `LineWriter`
    // buffer to fill.
    let t2 = std::thread::Builder::new()
        .name("proxy-stdout-fwd".into())
        .spawn(move || -> std::io::Result<u64> {
            let mut stdout = std::io::stdout().lock();
            let mut buf = [0u8; 8192];
            let mut total = 0u64;
            let result = loop {
                let n = match (&stream_for_t2).read(&mut buf) {
                    Ok(0) => break Ok(total),
                    Ok(n) => n,
                    Err(e) => break Err(e),
                };
                if let Err(e) = stdout.write_all(&buf[..n]) {
                    break Err(e);
                }
                if let Err(e) = stdout.flush() {
                    break Err(e);
                }
                total += n as u64;
            };
            match &result {
                Ok(total) => log::info!(
                    "Proxy: socket->stdout copy ended ({total} bytes); \
                     shutting down socket to unblock peer"
                ),
                Err(e) => log::info!(
                    "Proxy: socket->stdout copy errored ({e}); \
                     shutting down socket to unblock peer"
                ),
            }
            let _ = stream_shutdown_for_t2.shutdown(Shutdown::Both);
            result
        })?;

    let _ = t1.join();
    let _ = t2.join();

    log::info!("Proxy: bridge closed, exiting");
    Ok(())
}

#[cfg(test)]
#[path = "proxy_tests.rs"]
mod tests;
