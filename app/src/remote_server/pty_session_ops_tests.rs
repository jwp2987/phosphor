use super::session_exit_status_from_process;
use remote_server::session_store::SessionExitStatus;

#[cfg(feature = "local_tty")]
use super::remote_pty_thread::LiveSession;
#[cfg(feature = "local_tty")]
use super::{PtySessionEvent, PtySpawnSpec};
#[cfg(feature = "local_tty")]
use crate::terminal::local_tty::shell::{DirectShellStarter, ShellStarter};
#[cfg(feature = "local_tty")]
use crate::terminal::local_tty::spawner::PtySpawner;
#[cfg(feature = "local_tty")]
use crate::terminal::shell::ShellType;
#[cfg(feature = "local_tty")]
use remote_server::RemotePtySessionId;
#[cfg(feature = "local_tty")]
use std::path::PathBuf;

// `std::process::ExitStatus::from_raw` decodes the POSIX `wait(2)` status
// word without spawning anything: low byte 0 means "exited normally, code in
// the high byte"; a low byte that is a signal number (and not 0x7f, which
// means stopped) means "terminated by that signal". Building status values
// this way lets these tests exercise the exited-vs-signalled mapping without
// spawning a process, matching this branch's rule that a test needing a real
// process is a test that will not run.

#[test]
fn exit_status_with_code_maps_to_exited() {
    use std::os::unix::process::ExitStatusExt;

    let status = std::process::ExitStatus::from_raw(42 << 8);

    assert_eq!(
        session_exit_status_from_process(status),
        SessionExitStatus::exited(42)
    );
}

#[test]
fn exit_status_from_signal_maps_to_signalled_with_no_code() {
    use std::os::unix::process::ExitStatusExt;

    // Killed by SIGKILL (9), no core dump.
    let status = std::process::ExitStatus::from_raw(9);

    let mapped = session_exit_status_from_process(status);
    assert_eq!(mapped, SessionExitStatus::signalled());
    assert_eq!(mapped.code, None);
}

// The one test in this file that actually spawns a process: exercises
// `remote_pty_thread::LiveSession` end to end (a real `Pty`, its dedicated
// mio thread, and the `async_channel` those events travel over) rather than
// the dispatch logic `server_model_tests.rs` covers through
// `FakePtySessionOperations`. `#[cfg(feature = "local_tty")]` because the
// real backend (and `PtySpawner`, which this needs registered) only exist
// under that feature -- matching `LocalTtyPtySessionOperations`'s own gate.
//
// Kept hermetic and fast by spawning `/bin/cat` -- a trivial, single-purpose
// command -- via `DirectShellStarter::new_for_test`, which bypasses this
// crate's normal shell resolution (`resolve_shell_starter` only recognizes
// bash/zsh/fish) entirely, rather than spawning and bootstrapping an
// interactive shell. `cat`, not `/bin/echo`, specifically so this test drives
// the exit itself instead of racing it -- see the comment below.
//
// No sleep-based synchronization: every wait is a real `.await` on the
// `async_channel::Receiver` the pty thread's `try_send` calls wake, driven by
// `warpui::App::test`'s own executor, raced against a timer so a regression
// fails this test instead of hanging the suite.
//
// Breaks if: `remote_pty_thread::run_session_loop` stops forwarding pty
// output before draining it, stops sending a real exit code, or the pty
// thread never terminates once its child does.
#[cfg(feature = "local_tty")]
#[test]
fn real_pty_backend_runs_a_trivial_command_and_reports_output_and_exit() {
    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(|_ctx| PtySpawner::new_for_test());

        let id = RemotePtySessionId::from("real-backend-test".to_string());
        let spec = PtySpawnSpec {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            shell: None,
            environment_variables: std::collections::HashMap::new(),
            rows: 24,
            cols: 80,
        };
        let shell_starter = ShellStarter::Direct(DirectShellStarter::new_for_test(
            ShellType::Bash,
            PathBuf::from("/bin/cat"),
            Vec::new(),
        ));
        let (events_tx, events_rx) = async_channel::unbounded();

        let session = app
            .update(|ctx| {
                LiveSession::spawn_with_shell_starter(
                    id.clone(),
                    shell_starter,
                    &spec,
                    ctx,
                    events_tx,
                )
            })
            .expect("spawning /bin/cat in a real pty should succeed");

        // Queued back to back: canonical-mode line buffering delivers "ping\n"
        // to `cat` as a complete line regardless of exactly when the
        // trailing EOF byte (0x04) is queued behind it, so there is nothing
        // to wait for between the two writes.
        session.write_stdin(b"ping\n".to_vec());
        session.write_stdin(vec![0x04]);

        // The event-ordering race a bare `/bin/echo` would hit: if the
        // child's exit and its last bit of pty-readable output become ready
        // in the same `mio` wakeup, `run_session_loop` (mirroring
        // `local_tty::event_loop::EventLoop`'s own structure) can observe the
        // exit first and never drain that output. Driving the exit
        // ourselves, well after `cat` has already echoed "ping" back, keeps
        // this test out of that race rather than depending on winning it.
        //
        // Raced against a timer rather than awaited unconditionally: if the
        // real backend regresses into never sending an exit event, this
        // fails the test instead of hanging the whole suite.
        let (collected_output, exit_status) = futures_lite::future::or(
            async {
                let mut collected_output = Vec::new();
                loop {
                    let (received_id, event) = events_rx
                        .recv()
                        .await
                        .expect("the pty thread should keep sending events until it exits");
                    assert_eq!(received_id, id);
                    match event {
                        PtySessionEvent::Output(data) => collected_output.extend_from_slice(&data),
                        PtySessionEvent::Exited(status) => break (collected_output, status),
                    }
                }
            },
            async {
                async_io::Timer::after(std::time::Duration::from_secs(10)).await;
                panic!(
                    "timed out waiting for the real pty backend to finish /bin/cat -- see \
                     this test's doc comment"
                );
            },
        )
        .await;

        assert!(
            collected_output.windows(4).any(|window| window == b"ping"),
            "expected cat's echoed output to contain \"ping\", got {:?}",
            String::from_utf8_lossy(&collected_output)
        );
        assert_eq!(exit_status, SessionExitStatus::exited(0));
    });
}

// The pid guard in `send_signal_to_process_group`. Signals are delivered to
// `-pid` (the process group), and that negation makes a bad pid catastrophic
// rather than merely wrong: `kill(-0, sig)` targets every process in the
// DAEMON'S OWN process group, and `kill(-1, sig)` every process the user may
// signal. `SignalSession` is client-triggerable, so without this guard a stale
// or defaulted pid turns a remote request into a kill of the daemon itself.
//
// Breaks if: the `pid <= 1` check is removed. Note the test asserts on the
// refusal rather than on the effect -- deliberately, since a test that verified
// the effect would have to actually signal the test runner's process group.
#[cfg(all(feature = "local_tty", unix))]
#[test]
fn signalling_an_implausible_pid_is_refused_rather_than_sent_to_our_own_group() {
    use super::remote_pty_thread::send_signal_to_process_group;

    for pid in [0, 1] {
        let error = send_signal_to_process_group(pid, libc::SIGTERM)
            .expect_err("pid 0 and 1 must be refused, never negated and sent");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    // A plausible pid is not refused by the guard. Signal 0 performs no
    // delivery -- it is the standard existence/permission probe -- so this
    // reaches the syscall without touching any process. The guard is what is
    // under test, so either outcome from the syscall is acceptable; only a
    // guard rejection would be wrong.
    let our_own_group = std::process::id();
    let result = send_signal_to_process_group(our_own_group, 0);
    if let Err(error) = result {
        assert_ne!(
            error.kind(),
            std::io::ErrorKind::InvalidInput,
            "a real pid must not be rejected by the pid guard"
        );
    }
}
