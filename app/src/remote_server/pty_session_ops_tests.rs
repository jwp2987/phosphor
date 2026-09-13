use super::session_exit_status_from_process;
use remote_server::session_store::SessionExitStatus;

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
