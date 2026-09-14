use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::io::AsRawFd as _;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt as _;

use mio::unix::SourceFd;
use serial_test::serial;

use super::*;
use crate::terminal::local_tty::spawner::PtySpawner;
use crate::terminal::shell::ShellType;

// Fix for the read loop fabricating a signalled exit on a normal exit race:
// it used to treat every read error other than `WouldBlock`/`Interrupted` as
// fatal, including the `EIO` (-> `ErrorKind::Other`) a pty master read
// commonly returns on Linux/FreeBSD once the slave hangs up -- see
// `is_benign_pty_hangup_read_error`'s doc comment. This pins the
// classification directly rather than trying to race a real read against a
// real child's exit, which is inherently timing-dependent and would make
// this test flaky for exactly the race the fix is about.
//
// Breaks if: `is_benign_pty_hangup_read_error` stops treating
// `ErrorKind::Other` as benign, or starts treating some other kind as benign.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn benign_pty_hangup_read_error_classification() {
    assert!(is_benign_pty_hangup_read_error(&std::io::Error::from(
        std::io::ErrorKind::Other
    )));

    for kind in [
        std::io::ErrorKind::WouldBlock,
        std::io::ErrorKind::Interrupted,
        std::io::ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::BrokenPipe,
    ] {
        assert!(
            !is_benign_pty_hangup_read_error(&std::io::Error::from(kind)),
            "{kind:?} must not be treated as a benign hangup"
        );
    }
}

// Fix for the pty leak in `spawn_with_shell_starter`: `Pty::new` succeeding
// only guarantees a live child exists, not that this function will return
// `Ok` -- `Poll::new`, `Waker::new`, `pty.register`, and
// `thread::Builder::spawn` can each still fail afterwards, and neither `Pty`
// nor `DirectPtyHandle` reaps a child on drop. `KillPtyOnDrop` is the guard
// that closes that gap; this test exercises the guard directly rather than
// trying to force one of those four calls to fail. `Poll::new`/`Waker::new`
// need fd-exhaustion, and `thread::Builder::spawn` needs thread/process-limit
// exhaustion, to fail at all -- none of which can be induced hermetically
// without also perturbing every other test running in this process, so a
// direct test of the guard's own cleanup semantics is what is actually
// achievable here.
//
// Breaks if: `KillPtyOnDrop`'s `Drop` impl stops calling `EventedPty::kill`
// (or is removed outright), which is exactly what "just call `pty.kill()` on
// each error branch instead of a guard" would silently regress the next time
// a fallible step is added between `Pty::new` and the guard being disarmed.
#[test]
fn dropping_an_undisarmed_guard_kills_and_reaps_the_child() {
    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(|_ctx| PtySpawner::new_for_test());

        let shell_starter = ShellStarter::Direct(DirectShellStarter::new_for_test(
            ShellType::Bash,
            PathBuf::from("/bin/cat"),
            Vec::new(),
        ));
        let options = PtyOptions {
            size: SizeInfo::new_without_font_metrics(24, 80),
            window_id: None,
            shell_starter,
            start_dir: Some(std::env::temp_dir()),
            env_vars: std::collections::HashMap::new(),
            enable_ssh_wrapper: false,
            reuse_ssh_control_master: false,
            shell_debug_mode: false,
            honor_ps1: false,
            node_version_chip_enabled: false,
            close_fds: true,
        };

        let pty = app
            .update(|ctx| Pty::new(options, false, ctx))
            .expect("spawning /bin/cat in a real pty should succeed");
        let pid = pty.get_pid() as libc::pid_t;

        // Stands in for any of `spawn_with_shell_starter`'s failure paths
        // between a successful `Pty::new` and the guard being disarmed:
        // drop it without ever calling `disarm`.
        drop(KillPtyOnDrop::new(pty));

        // `KillPtyOnDrop::drop` calls `EventedPty::kill`, which blocks on
        // `Child::wait` -- so by the time the `drop` above has returned, the
        // child has already been reaped and its pid is free for reuse.
        // `kill(pid, 0)` delivers no signal (0 is the standard
        // existence/permission probe), so it must now report ESRCH ("no
        // such process").
        let probe = unsafe { libc::kill(pid, 0) };
        assert_eq!(probe, -1, "the child must no longer exist");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH),
            "the child must already be reaped, not merely killed"
        );
    });
}

/// Guards the process-wide `SHELL` env var for the duration of a test,
/// restoring its original value (or absence) on drop -- including on
/// panic/unwind, since `#[serial]` alone only keeps tests from *racing* on
/// the var, not from leaving it mutated for whichever test runs next in this
/// process.
struct ShellEnvGuard(Option<std::ffi::OsString>);

impl ShellEnvGuard {
    fn set(value: &std::ffi::OsStr) -> Self {
        let guard = Self(std::env::var_os("SHELL"));
        // SAFETY: serialized via #[serial(remote_pty_thread_shell_env)] --
        // no other test in this process observes SHELL while this guard is
        // alive.
        unsafe { std::env::set_var("SHELL", value) };
        guard
    }
}

impl Drop for ShellEnvGuard {
    fn drop(&mut self) {
        // SAFETY: see `set`.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("SHELL", value),
                None => std::env::remove_var("SHELL"),
            }
        }
    }
}

/// Writes a trivial, executable file at `dir/name` and returns its path.
/// That is enough for `supported_shell_path_and_type` to resolve it as a
/// supported shell binary -- it only checks that the path exists, is
/// executable, and is *named* one of bash/zsh/fish; it never runs it.
fn fake_shell_binary(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"#!/bin/sh\n").expect("write fake shell binary");
    let mut perms = std::fs::metadata(&path)
        .expect("stat fake shell binary")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod fake shell binary");
    path
}

// Fix for `resolve_shell_starter`'s fallback chain: it used to try `$SHELL`
// before the passwd-entry resolution `ShellStarter::compute_fallback_shell`
// uses for a local session, a materially weaker signal since a daemon
// started by a service manager has no inherited login shell. Planting a
// fake, resolvable "bash" at a path nothing else on the host could ever
// produce pins this down: if `resolve_shell_starter` ever reads `$SHELL`
// again, it resolves to *this* path, which `compute_fallback_shell` could
// never independently produce.
//
// Breaks if: `resolve_shell_starter`'s `None` branch reads `$SHELL` again
// before falling back to `ShellStarter::compute_fallback_shell`.
#[test]
#[serial(remote_pty_thread_shell_env)]
fn resolve_shell_starter_fallback_ignores_shell_env_var() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let fake_bash = fake_shell_binary(dir.path(), "bash");
    let _guard = ShellEnvGuard::set(fake_bash.as_os_str());

    let resolved =
        resolve_shell_starter(None, false, None).expect("a supported shell should still resolve");
    let expected = ShellStarter::from(ShellStarter::compute_fallback_shell().expect(
        "this host must have at least one fallback shell for the rest of the suite to run at all",
    ));

    let (ShellStarter::Direct(resolved), ShellStarter::Direct(expected)) = (resolved, expected)
    else {
        panic!(
            "resolve_shell_starter's fallback and compute_fallback_shell's result always \
             convert to ShellStarter::Direct"
        );
    };

    assert_ne!(
        resolved.shell_path(),
        fake_bash.as_path(),
        "resolve_shell_starter must not resolve to the fake $SHELL"
    );
    assert_eq!(resolved.shell_path(), expected.shell_path());
    assert_eq!(resolved.shell_type(), expected.shell_type());
}

// `no_bootstrap` is honoured by discarding the shell starter's arguments,
// because the arguments *are* the bootstrap: every branch of
// `arguments_for_session_spawning_command` builds a `-c` wrapper that
// re-execs the shell with an injected rcfile or init-command carrying the
// InitShell OSC handshake. The two assertions are paired deliberately --
// proving the args are empty is only meaningful next to proof that the same
// resolution *does* produce args when the flag is off, since an empty vec is
// also what a broken resolution would return.
//
// Both calls take the explicit-shell branch, so this is hermetic: it never
// depends on what shell this host's passwd entry names.
//
// Breaks if: `resolve_shell_starter` stops calling `without_bootstrap`, or
// `without_bootstrap` starts substituting a hand-written "plain" argument
// list instead of leaving the shell to its own defaults.
#[test]
fn no_bootstrap_strips_the_shell_starter_arguments() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let fake_bash = fake_shell_binary(dir.path(), "bash");
    let shell = fake_bash.to_str().expect("temp path is utf-8");

    // `"bash"` matters now: bash carries its init in argv, so no session id is
    // needed and nothing is stripped. A fake named `"zsh"` would be stripped by
    // the no-id rule and this assertion would invert -- which is what
    // `a_shell_needing_an_init_script_is_spawned_plainly_without_a_session_id`
    // below pins deliberately.
    let ShellStarter::Direct(bootstrapped) = resolve_shell_starter(Some(shell), false, None)
        .expect("an explicitly named, resolvable shell must resolve")
    else {
        panic!("an explicitly named shell always resolves to ShellStarter::Direct");
    };
    assert!(
        !bootstrapped.args().is_empty(),
        "the default path must still inject Warp's bootstrap"
    );

    let ShellStarter::Direct(plain) = resolve_shell_starter(Some(shell), true, None)
        .expect("an explicitly named, resolvable shell must resolve")
    else {
        panic!("an explicitly named shell always resolves to ShellStarter::Direct");
    };
    assert!(
        plain.args().is_empty(),
        "no_bootstrap must leave the shell binary to start on its own terms, got {:?}",
        plain.args()
    );
    assert_eq!(
        plain.shell_path(),
        bootstrapped.shell_path(),
        "stripping the bootstrap must not change which shell is spawned"
    );
    assert_eq!(plain.shell_type(), bootstrapped.shell_type());
}

/// Token the [`FakePty`] reports for its stand-in pty fd, and the one it
/// reports for its stand-in `SIGCHLD` fd. Deliberately the same numbers a
/// real `Pty` uses (`PTY_TOKEN`(1) / `SIGNALS_TOKEN`(2) in
/// `local_tty::unix::Pty::new`), so a fake session exercises the same token
/// dispatch a real one does, and so both stay clear of `COMMAND_TOKEN`(3).
const FAKE_PTY_TOKEN: Token = Token(1);
const FAKE_CHILD_TOKEN: Token = Token(2);

/// A stand-in for `local_tty::Pty` that [`run_session_loop`] can drive
/// without spawning a process: one socketpair carries the bytes a real pty
/// master would, and a second one stands in for the `SIGCHLD` fd a real `Pty`
/// reports child exits through.
///
/// Why a fake rather than a real pty. The behaviour under test is an
/// *ordering* property -- what the loop does when a single `poll()` wakeup
/// carries both the child's exit and its last readable output. With a real
/// shell that is a race between the child's final write and its own exit;
/// `pty_session_ops_tests.rs`'s real-pty test says so itself and deliberately
/// steers around the race rather than trying to win it. A test that only
/// sometimes observes the regression is not a regression test. Arming both
/// fds before the loop's first `poll()` makes that one wakeup carry both
/// events on every run.
struct FakePty {
    /// The loop's end of the "pty master": whatever is written to
    /// [`FakeSession::child_output`] arrives here.
    stream: UnixStream,
    /// Readable exactly once the test has exited the fake child.
    child_signal: UnixStream,
    /// Yielded to the loop once, mirroring the take-once contract of
    /// `Pty::take_exit_status` (which drains the record `Child::try_wait`
    /// produced).
    exit_status: Option<std::process::ExitStatus>,
    child_event_reported: bool,
}

impl EventedReadWrite for FakePty {
    type Reader = UnixStream;
    type Writer = UnixStream;

    fn register(&mut self, poll: &mio::Poll, interest: mio::Interest) -> io::Result<()> {
        poll.registry().register(
            &mut SourceFd(&self.stream.as_raw_fd()),
            FAKE_PTY_TOKEN,
            interest,
        )?;
        poll.registry().register(
            &mut SourceFd(&self.child_signal.as_raw_fd()),
            FAKE_CHILD_TOKEN,
            Interest::READABLE,
        )
    }

    fn reregister(&mut self, poll: &mio::Poll, interest: mio::Interest) -> io::Result<()> {
        poll.registry().reregister(
            &mut SourceFd(&self.stream.as_raw_fd()),
            FAKE_PTY_TOKEN,
            interest,
        )?;
        poll.registry().reregister(
            &mut SourceFd(&self.child_signal.as_raw_fd()),
            FAKE_CHILD_TOKEN,
            Interest::READABLE,
        )
    }

    fn deregister(&mut self, poll: &mio::Poll) -> io::Result<()> {
        poll.registry()
            .deregister(&mut SourceFd(&self.stream.as_raw_fd()))?;
        poll.registry()
            .deregister(&mut SourceFd(&self.child_signal.as_raw_fd()))
    }

    fn reader(&mut self) -> &mut UnixStream {
        &mut self.stream
    }

    fn read_token(&self) -> Token {
        FAKE_PTY_TOKEN
    }

    fn writer(&mut self) -> &mut UnixStream {
        &mut self.stream
    }

    fn write_token(&self) -> Token {
        FAKE_PTY_TOKEN
    }
}

impl EventedPty for FakePty {
    fn child_event_token(&self) -> Token {
        FAKE_CHILD_TOKEN
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        // Reported at most once, like the real `Pty`'s: its `next_child_event`
        // consumes a pending signal and then observes an already-reaped child.
        if self.child_event_reported {
            return None;
        }
        let mut byte = [0u8; 1];
        match self.child_signal.read(&mut byte) {
            Ok(1) => {
                self.child_event_reported = true;
                Some(ChildEvent::Exited)
            }
            _ => None,
        }
    }

    fn on_resize(&mut self, _size: &SizeInfo) {}

    fn kill(self) -> anyhow::Result<()> {
        // Reached only from `force_kill_and_report`, i.e. a shutdown that was
        // not itself a child exit. Neither test below takes that path; if one
        // ever does, it will be because the loop regressed into force-killing
        // a child that exited on its own, so this records that rather than
        // failing silently.
        panic!("a fake session that exited on its own must never be force-killed");
    }
}

impl SessionPty for FakePty {
    fn take_exit_status(&mut self) -> Option<std::process::ExitStatus> {
        self.exit_status.take()
    }
}

/// A [`FakePty`] plus the ends of its socketpairs a test drives it from, and
/// the `mio` machinery `run_session_loop` expects to be handed.
struct FakeSession {
    pty: FakePty,
    poll: Poll,
    commands: Arc<CommandChannel>,
    /// Writing here is the fake child writing to its terminal.
    child_output: UnixStream,
    /// Writing one byte here is the fake child exiting.
    child_exit: UnixStream,
}

impl FakeSession {
    fn new(exit_status: std::process::ExitStatus) -> Self {
        let (child_output, stream) = UnixStream::pair().expect("socketpair for fake pty output");
        let (child_exit, child_signal) =
            UnixStream::pair().expect("socketpair for fake child exit");
        // Non-blocking on the loop's side only: `run_session_loop` drains
        // until `WouldBlock`, which a blocking fd would never return -- it
        // would park the test thread inside the loop instead.
        stream
            .set_nonblocking(true)
            .expect("the loop's pty end must be non-blocking");
        child_signal
            .set_nonblocking(true)
            .expect("the loop's child-signal end must be non-blocking");

        let mut pty = FakePty {
            stream,
            child_signal,
            exit_status: Some(exit_status),
            child_event_reported: false,
        };

        let poll = Poll::new().expect("create mio poll");
        let waker = Waker::new(poll.registry(), COMMAND_TOKEN).expect("create mio waker");
        let commands = Arc::new(CommandChannel {
            queue: Mutex::new(VecDeque::new()),
            waker,
        });
        pty.register(&poll, Interest::READABLE | Interest::WRITABLE)
            .expect("register the fake pty");

        Self {
            pty,
            poll,
            commands,
            child_output,
            child_exit,
        }
    }

    /// Runs the loop to completion on this thread and returns every event it
    /// forwarded, in order. Returns them rather than asserting so each test
    /// states its own expectations.
    ///
    /// This cannot hang: the loop's only blocking call is `poll()`, and every
    /// caller below arms the child-exit fd before calling this, so the first
    /// wakeup already carries the event that ends the session.
    fn run_to_completion(self) -> Vec<PtySessionEvent> {
        let Self {
            pty,
            poll,
            commands,
            child_output,
            child_exit,
        } = self;
        let id = RemotePtySessionId::from("fake-pty-session".to_string());
        let (events_tx, events_rx) = async_channel::unbounded();

        run_session_loop(id.clone(), pty, poll, commands, events_tx);

        // Held until the loop has returned: closing the far end of the pty
        // socketpair early would turn the drain's `WouldBlock` into an `Ok(0)`
        // EOF, so a loop that never drained at all would still end -- and the
        // test would stop distinguishing the two.
        drop((child_output, child_exit));

        let mut events = Vec::new();
        while let Ok((received_id, event)) = events_rx.try_recv() {
            assert_eq!(received_id, id, "every event carries its own session's id");
            events.push(event);
        }
        events
    }
}

// Fix for gap 2 of "Known gaps from refuting `3d638ad25`": a shell that writes
// and immediately exits could lose its last output. One `poll()` wakeup can
// deliver the child-exit event and a final readable event together, and the
// exit branch used to `break` before the read drain ran, discarding whatever
// was still buffered. Both of this fake session's sources are armed before the
// loop's first `poll()`, so that wakeup carries both events deterministically
// -- and it fails under the old code whichever order `events.iter()` yields
// them in, because the drain ran after the whole event loop either way.
//
// Breaks if: `run_session_loop` honours a child exit before draining readable
// output, or sends the `Exited` event ahead of output that preceded it (a
// client would render an exit above the last line of the command that caused
// it).
#[test]
fn output_pending_when_the_child_exits_is_forwarded_before_the_exit() {
    let mut session = FakeSession::new(std::process::ExitStatus::from_raw(7 << 8));

    session
        .child_output
        .write_all(b"last line\n")
        .expect("write the fake child's final output");
    session
        .child_exit
        .write_all(b"x")
        .expect("arm the fake child's exit");

    let events = session.run_to_completion();

    assert_eq!(
        events.len(),
        2,
        "expected the final output followed by the exit, got {} event(s)",
        events.len()
    );
    match &events[0] {
        PtySessionEvent::Output(bytes) => assert_eq!(
            bytes.as_slice(),
            b"last line\n".as_slice(),
            "the child's last output must arrive intact"
        ),
        PtySessionEvent::Exited(_) => {
            panic!("the exit was reported before the output that preceded it")
        }
    }
    match &events[1] {
        PtySessionEvent::Exited(status) => assert_eq!(
            *status,
            SessionExitStatus::exited(7),
            "the real exit code must survive the drain"
        ),
        PtySessionEvent::Output(_) => panic!("expected the exit event last"),
    }
}

// The other half of the same fix, and the reason it is a separate test: the
// drain now runs on the exit path unconditionally, including when there is
// nothing to drain. A drain that waits for readable output that will never
// come would strand the session as "running" forever -- a worse failure than
// the lost bytes the fix is about, and invisible to the test above, which
// always has something to read.
//
// Breaks if: a child exit with no pending output stops producing an `Exited`
// event, or stops producing it promptly (this test would hang rather than
// fail).
#[test]
fn a_child_exit_with_no_pending_output_still_reports_the_exit() {
    let mut session = FakeSession::new(std::process::ExitStatus::from_raw(0));

    session
        .child_exit
        .write_all(b"x")
        .expect("arm the fake child's exit");

    let events = session.run_to_completion();

    assert_eq!(
        events.len(),
        1,
        "expected exactly one exit event and no output"
    );
    match &events[0] {
        PtySessionEvent::Exited(status) => {
            assert_eq!(*status, SessionExitStatus::exited(0))
        }
        PtySessionEvent::Output(_) => panic!("the fake child wrote nothing to forward"),
    }
}

// Fix for gap 4 of "Known gaps from refuting `3d638ad25`": `--no-rcs`, and
// then nothing. `arguments_for_session_spawning_command` suppresses zsh's own
// startup files on the understanding that Warp takes over through the pty's
// input stream -- which locally `TerminalManager::enqueue_init_script` does
// and on the daemon nobody did, so a daemon-spawned zsh got neither its own
// configuration nor Warp's.
//
// Hermetic, and deliberately about the decision rather than the pty: which
// shells get a queued script, and with which session id.
//
// Breaks if: the daemon stops queueing zsh's init script, stops following it
// with the bytes that run it, or uses a session id other than the starter's
// own (the InitShell handshake would then carry an id nothing else in this
// session knows).
#[test]
fn zsh_gets_its_init_script_queued_as_stdin() {
    let zsh =
        DirectShellStarter::new_for_test(ShellType::Zsh, PathBuf::from("/bin/zsh"), Vec::new());
    let session_id = zsh.session_id();

    let writes = session_init_script_writes(&ShellStarter::Direct(zsh), false);

    assert_eq!(writes.len(), 2, "the init script, then the bytes that run it");
    let script = String::from_utf8(writes[0].clone()).expect("the init script is utf-8");
    assert!(
        script.contains(&format!("WARP_SESSION_ID={}", session_id.as_u64())),
        "the queued script must carry this starter's own session id"
    );
    assert_eq!(
        writes[1],
        ShellType::Zsh.execute_command_bytes().to_vec(),
        "the script is inert text until something runs it"
    );
}

// The paired negative, without which the test above cannot distinguish "zsh is
// bootstrapped" from "everything is bootstrapped twice": bash carries this
// same init script inside its own arguments (`--rcfile <(echo ...)`), as fish
// and PowerShell carry theirs, so queueing it again here would run it twice.
//
// Breaks if: the queued-script rule widens to shells whose arguments already
// carry their bootstrap.
#[test]
fn a_shell_whose_arguments_carry_its_init_gets_nothing_queued() {
    let bash =
        DirectShellStarter::new_for_test(ShellType::Bash, PathBuf::from("/bin/bash"), Vec::new());

    assert!(
        session_init_script_writes(&ShellStarter::Direct(bash), false).is_empty(),
        "bash's bootstrap is already in its argv"
    );
}

// The interaction most likely to regress silently, because both paths still
// produce a running shell: a `no_bootstrap` session whose init script gets
// queued anyway looks fine from the daemon's side and is a screenful of raw
// escape noise from the client's.
//
// Breaks if: `no_bootstrap` stops suppressing the queued init script -- a
// client that asked for a bare shell would get the InitShell handshake written
// into its session, which is the exact failure the flag exists to prevent.
#[test]
fn no_bootstrap_suppresses_the_queued_init_script() {
    let zsh =
        DirectShellStarter::new_for_test(ShellType::Zsh, PathBuf::from("/bin/zsh"), Vec::new());

    assert!(
        session_init_script_writes(&ShellStarter::Direct(zsh), true).is_empty(),
        "a session that asked for no bootstrap must be given none"
    );
}

// The wiring, which the three tests above deliberately do not cover: they only
// pin `session_init_script_writes`, and deleting the loop in
// `spawn_with_shell_starter` that actually queues its result would leave every
// one of them passing. This is the only test here that fails if the writes are
// computed and then dropped on the floor.
//
// Spawns `/bin/cat` *declared as zsh*: the bootstrap decision is made from the
// starter's `ShellType`, never from the binary, so this session receives the
// exact writes a real zsh would -- and `cat` echoes them straight back through
// the pty instead of executing them, which is what makes the assertion
// possible at all. Follows the real-pty pattern in `pty_session_ops_tests.rs`,
// including its timer race, so a regression into never sending an exit event
// fails this test rather than hanging the suite.
//
// Breaks if: `spawn_with_shell_starter` stops queueing the init-script writes,
// or queues them somewhere a client's own writes could get in front of them.
#[test]
fn a_daemon_spawned_zsh_receives_its_init_script_on_stdin() {
    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(|_ctx| PtySpawner::new_for_test());

        let id = RemotePtySessionId::from("zsh-init-script-wiring".to_string());
        let starter =
            DirectShellStarter::new_for_test(ShellType::Zsh, PathBuf::from("/bin/cat"), Vec::new());
        // Reused as the spec's id purely so the assertion below has one value to
        // look for. The id the script carries now comes from the *spec*, not the
        // starter -- the daemon spawns the shell on one machine while the id has
        // to be registered on another, so the caller supplies it.
        let session_id = starter.session_id();
        let spec = PtySpawnSpec {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            shell: None,
            environment_variables: std::collections::HashMap::new(),
            rows: 24,
            cols: 80,
            // The point of the test: an ordinary request, which must be
            // bootstrapped.
            no_bootstrap: false,
            // And a caller that *can* register the id, which is what entitles
            // this session to a bootstrap at all. Without it
            // `session_init_script_writes` correctly writes nothing.
            bootstrap_session_id: Some(session_id),
        };
        let (events_tx, events_rx) = async_channel::unbounded();

        let session = app
            .update(|ctx| {
                LiveSession::spawn_with_shell_starter(
                    id.clone(),
                    ShellStarter::Direct(starter),
                    &spec,
                    ctx,
                    events_tx,
                )
            })
            .expect("spawning /bin/cat in a real pty should succeed");

        // Queued *behind* the init-script writes `spawn_with_shell_starter`
        // has already put in this session's queue -- which is half of what is
        // under test, since a client's first write must not be able to
        // overtake the bootstrap. The script's own trailing newline
        // (`execute_command_bytes`) ends the line, so this lands at the start
        // of a fresh one, where canonical mode reads 0x04 as EOF and `cat`
        // exits.
        session.write_stdin(vec![0x04]);

        let collected = futures_lite::future::or(
            async {
                let mut collected = Vec::new();
                loop {
                    let (received_id, event) = events_rx
                        .recv()
                        .await
                        .expect("the pty thread should keep sending events until it exits");
                    assert_eq!(received_id, id);
                    match event {
                        PtySessionEvent::Output(data) => collected.extend_from_slice(&data),
                        PtySessionEvent::Exited(_) => break collected,
                    }
                }
            },
            async {
                async_io::Timer::after(std::time::Duration::from_secs(10)).await;
                panic!(
                    "timed out waiting for /bin/cat to echo the queued init script back -- see \
                     this test's doc comment"
                );
            },
        )
        .await;

        let echoed = String::from_utf8_lossy(&collected);
        let expected = format!("WARP_SESSION_ID={}", session_id.as_u64());
        assert!(
            echoed.contains(&expected),
            "the session's own init script, carrying its own session id, must reach the shell's \
             stdin; looked for {expected:?} in {} echoed bytes",
            collected.len()
        );
    });
}

// A zsh the daemon cannot hand a session id must be spawned PLAINLY, not
// half-bootstrapped. This is the rule a refutation pass produced, and the
// failure it prevents is worse than the gap it replaced.
//
// `arguments_for_session_spawning_command` gives zsh `--no-rcs`, suppressing the
// user's own startup files on the promise that an init script will arrive and
// complete an InitShell handshake. That handshake is only honoured if the
// *client* registered the same id with its `TerminalModel`, and nothing on this
// wire carries one (`SpawnSessionSuccess` is empty). Queue the script anyway and
// zsh ends up with no user config, no Warp config, and `ZLE` disabled by the
// script's first line and re-enabled only by a body that never arrives.
//
// Stripping gives the user a plain interactive zsh reading their own `~/.zshrc`.
//
// Breaks if: the no-id case stops stripping -- most plausibly by someone reading
// `resolve_shell_starter`'s first condition as being only about `no_bootstrap`,
// which is how it was written before this rule existed.
#[test]
#[serial(remote_pty_thread_shell_env)]
fn a_shell_needing_an_init_script_is_spawned_plainly_without_a_session_id() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let fake_zsh = fake_shell_binary(dir.path(), "zsh");
    let shell = fake_zsh.to_str().expect("temp path is utf-8");

    let ShellStarter::Direct(no_id) = resolve_shell_starter(Some(shell), false, None)
        .expect("an explicitly named, resolvable shell must resolve")
    else {
        panic!("an explicitly named shell always resolves to ShellStarter::Direct");
    };
    assert!(
        no_id.args().is_empty(),
        "a zsh with no session id must be spawned plainly, not with --no-rcs and no script; got \
         {:?}",
        no_id.args()
    );

    // The paired half: given an id, the very same shell IS bootstrapped. Without
    // this, an implementation that stripped unconditionally would pass above.
    let ShellStarter::Direct(with_id) =
        resolve_shell_starter(Some(shell), false, Some(no_id.session_id()))
            .expect("an explicitly named, resolvable shell must resolve")
    else {
        panic!("an explicitly named shell always resolves to ShellStarter::Direct");
    };
    assert!(
        !with_id.args().is_empty(),
        "given an id to bind the handshake to, zsh must get its real bootstrap back"
    );
}

// The crux of the client-minted bootstrap id, and the half that is easy to get
// wrong: a supplied id must reach the shell's **argv**, not merely the starter's
// `session_id()` field.
//
// bash, fish and PowerShell embed the id directly in their arguments -- bash via
// `--rcfile <(echo <script>)`, and the script carries
// `WARP_SESSION_ID=<id>`. Only zsh and MSYS2 receive theirs out-of-band. So an
// implementation that stored the supplied id and left `args` alone would satisfy
// every accessor, pass any test that only reads `session_id()`, and still leave
// bash emitting every hook against the id its starter minted -- which
// `DProtoHook::requires_registered_session` rejects for all of them, silently,
// one warning per hook.
//
// Asserted on the id's digits appearing in argv, because that is the substitution
// `init_shell_script_for_shell` actually performs
// (`SESSION_ID_PLACEHOLDER` -> `session_id.as_u64().to_string()`).
//
// Breaks if: `with_bootstrap_session_id` stops rebuilding `args`, or
// `resolve_shell_starter` stops calling it for a shell that carries its init in
// argv.
#[test]
#[serial(remote_pty_thread_shell_env)]
fn a_supplied_bootstrap_id_reaches_the_shells_arguments() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let fake_bash = fake_shell_binary(dir.path(), "bash");
    let shell = fake_bash.to_str().expect("temp path is utf-8");
    let supplied = warp_core::SessionId::from(4_242_424_242_u64);

    let ShellStarter::Direct(bound) = resolve_shell_starter(Some(shell), false, Some(supplied))
        .expect("an explicitly named, resolvable shell must resolve")
    else {
        panic!("an explicitly named shell always resolves to ShellStarter::Direct");
    };

    assert_eq!(
        bound.session_id(),
        supplied,
        "the starter must report the supplied id, not one it minted"
    );

    let argv = bound
        .args()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        argv.contains(&supplied.as_u64().to_string()),
        "the supplied id must be baked into argv, where bash actually reads it; got {argv:?}"
    );

    // And the paired negative: without one, the starter keeps its own minted id,
    // so argv must NOT carry the supplied one. Without this, an implementation
    // that ignored the parameter entirely could still pass above by coincidence
    // only if it minted that exact u64 -- but more usefully, this pins that the
    // no-id path is genuinely different rather than silently defaulting.
    let ShellStarter::Direct(unbound) = resolve_shell_starter(Some(shell), false, None)
        .expect("an explicitly named, resolvable shell must resolve")
    else {
        panic!("an explicitly named shell always resolves to ShellStarter::Direct");
    };
    let unbound_argv = unbound
        .args()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        !unbound_argv.contains(&supplied.as_u64().to_string()),
        "with no id supplied the starter must use its own, not the one from another call"
    );
}
