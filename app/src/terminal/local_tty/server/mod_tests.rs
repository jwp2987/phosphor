use super::*;

// `std::process::ExitStatus::from_raw` decodes the POSIX `wait(2)` status word without
// spawning anything, mirroring the technique `pty_session_ops_tests.rs` already uses for the
// same reason: a test that needs a real process to observe a real exit is a test that this
// branch's discipline says should not exist if it can be avoided.  Low byte 0 means "exited
// normally, code in the high byte"; a low byte that is a signal number means "terminated by
// that signal".
use std::os::unix::process::ExitStatusExt;

/// An `OwnedFd` that is never read from or written to -- these tests exercise the
/// `TerminalServerClient`'s in-memory exit-status bookkeeping, not its socket I/O, so any
/// valid file descriptor will do.
fn unused_owned_fd() -> std::os::fd::OwnedFd {
    std::fs::File::open("/dev/null")
        .expect("/dev/null should always be openable in a test environment")
        .into()
}

/// Builds a `TerminalServerClient` whose `terminated_children` record already contains
/// `entries`, as if `spawn_message_receiver_thread` had just processed a
/// `ChildrenTerminatedRequest` reporting them.
fn client_with_terminated(
    entries: impl IntoIterator<Item = (u32, std::process::ExitStatus)>,
) -> Arc<TerminalServerClient> {
    let terminated_children = Arc::new(Mutex::new(HashMap::from_iter(entries)));
    Arc::new(TerminalServerClient::new(
        unused_owned_fd(),
        terminated_children,
    ))
}

fn handle_for(pid: u32, client: Arc<TerminalServerClient>) -> ServerOwnedPtyHandle {
    ServerOwnedPtyHandle {
        pid,
        client,
        exit_status: None,
    }
}

// The following two tests cover `api::ChildExitStatus`'s reason for existing: carrying a real
// `std::process::ExitStatus` across the terminal-server socket. They go through
// `bincode::serialize`/`deserialize` directly on `api::Message`, the same (de)serialization
// `protocol::send_message`/`receive_message` perform, rather than opening a socket, since the
// framing around that call (the length-prefixed header, the `Message::initialized` sentinel)
// is exercised elsewhere and isn't what these tests are about.

#[test]
fn children_terminated_request_round_trip_preserves_exit_code() {
    let exited = std::process::ExitStatus::from_raw(42 << 8);
    let message = api::Message::ChildrenTerminatedRequest {
        children: vec![(123, api::ChildExitStatus::from_std(exited))],
    };

    let bytes = bincode::serialize(&message).expect("Message should serialize");
    let decoded: api::Message = bincode::deserialize(&bytes).expect("Message should deserialize");

    match decoded {
        api::Message::ChildrenTerminatedRequest { children } => {
            assert_eq!(children.len(), 1);
            let (pid, status) = children[0];
            assert_eq!(pid, 123);
            assert_eq!(status.into_std().code(), Some(42));
        }
        other => panic!("expected ChildrenTerminatedRequest, got {other:?}"),
    }
}

#[test]
fn children_terminated_request_round_trip_preserves_signalled_with_no_code() {
    // Killed by SIGTERM (15), no core dump.
    let signalled = std::process::ExitStatus::from_raw(15);
    let message = api::Message::ChildrenTerminatedRequest {
        children: vec![(456, api::ChildExitStatus::from_std(signalled))],
    };

    let bytes = bincode::serialize(&message).expect("Message should serialize");
    let decoded: api::Message = bincode::deserialize(&bytes).expect("Message should deserialize");

    match decoded {
        api::Message::ChildrenTerminatedRequest { children } => {
            assert_eq!(children.len(), 1);
            let (pid, status) = children[0];
            assert_eq!(pid, 456);
            let status = status.into_std();
            assert_eq!(status.code(), None);
            assert_eq!(status.signal(), Some(15));
        }
        other => panic!("expected ChildrenTerminatedRequest, got {other:?}"),
    }
}

// The following tests cover `ServerOwnedPtyHandle`, the actual production defect: on the
// default (server-hosted) pty path, this is the `PtyHandle` impl a real session holds, and
// before this fix it never overrode `take_exit_status`, so it always returned the trait
// default `None` regardless of how the child really exited.

#[test]
fn server_owned_pty_handle_recovers_a_real_exit_code() {
    let exited = std::process::ExitStatus::from_raw(7 << 8);
    let client = client_with_terminated([(42, exited)]);
    let mut handle = handle_for(42, client);

    assert!(handle.has_process_terminated().unwrap());
    assert_eq!(handle.take_exit_status().unwrap().code(), Some(7));
}

#[test]
fn server_owned_pty_handle_reports_no_code_when_signalled() {
    // Killed by SIGKILL (9).
    let signalled = std::process::ExitStatus::from_raw(9);
    let client = client_with_terminated([(42, signalled)]);
    let mut handle = handle_for(42, client);

    assert!(handle.has_process_terminated().unwrap());
    let status = handle.take_exit_status().expect("child has terminated");
    assert_eq!(status.code(), None);
    assert_eq!(status.signal(), Some(9));
}

/// `TerminalServerClient::take_child_exit_status` -- like `Child::try_wait`, which is its own
/// ultimate source -- yields the real status only once. This is the regression this fix must
/// not reintroduce: without `ServerOwnedPtyHandle` caching what it observes, a second query
/// would see an already-drained record and silently fall back to "no status", exactly the bug
/// `take_exit_status`'s trait-default `None` caused in the first place.
#[test]
fn server_owned_pty_handle_caches_the_status_after_first_observation() {
    let exited = std::process::ExitStatus::from_raw(3 << 8);
    let client = client_with_terminated([(42, exited)]);
    let mut handle = handle_for(42, client);

    assert!(handle.has_process_terminated().unwrap());
    // The client's own record is now empty; a naive re-query would find nothing.
    assert!(handle.has_process_terminated().unwrap());
    assert_eq!(handle.take_exit_status().unwrap().code(), Some(3));
    assert_eq!(handle.take_exit_status().unwrap().code(), Some(3));
}

/// Guards `TerminalServerClient::has_child_terminated`'s pre-existing contract -- used by the
/// local app's own terminals, not just remote sessions -- now that its backing storage changed
/// from a `HashSet<u32>` to a `HashMap<u32, ExitStatus>`.
#[test]
fn has_child_terminated_still_reports_true_exactly_once() {
    let status = std::process::ExitStatus::from_raw(0);
    let client = client_with_terminated([(7, status)]);

    assert!(client.has_child_terminated(7));
    assert!(!client.has_child_terminated(7));
}
