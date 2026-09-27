use std::collections::{HashMap, HashSet};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use instant::Instant;
use uuid::Uuid;

use super::{ChildKillHandle, ChildProcessSlot, McpAppExitOutcome, McpAppExitShutdown};

const GRACE: Duration = Duration::from_millis(200);
/// Generous upper slack for a loaded CI box; the point is "bounded", not "exact".
const SLACK: Duration = Duration::from_secs(2);

fn shutdown_with(servers: &[Uuid]) -> (McpAppExitShutdown, mpsc::Sender<Uuid>) {
    let (done_tx, done_rx) = mpsc::channel();
    let pending = servers
        .iter()
        .map(|uuid| (*uuid, Arc::new(ChildProcessSlot::default())))
        .collect::<HashMap<_, _>>();
    (McpAppExitShutdown::new(done_rx, pending), done_tx)
}

/// A fake kill step: servers in `with_process` have a live child to kill; the rest
/// have none (HTTP/SSE, or a child whose handle was released).
fn fake_kill<'a>(
    with_process: &'a HashSet<Uuid>,
    killed: &'a mut Vec<Uuid>,
) -> impl FnMut(Uuid, &ChildProcessSlot) -> Option<bool> + 'a {
    move |uuid, _| {
        if with_process.contains(&uuid) {
            killed.push(uuid);
            Some(true)
        } else {
            None
        }
    }
}

#[test]
fn returns_early_once_every_server_has_stopped_and_kills_nothing() {
    let stdio = Uuid::new_v4();
    let http = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[stdio, http]);
    assert_eq!(shutdown.pending(), 2);
    done.send(stdio).unwrap();
    done.send(http).unwrap();

    let with_process = HashSet::from([stdio]);
    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(
        start + Duration::from_secs(30),
        fake_kill(&with_process, &mut killed),
    );

    assert!(
        start.elapsed() < SLACK,
        "must not wait out the deadline once every server has stopped"
    );
    assert_eq!(
        outcome,
        McpAppExitOutcome {
            stopped: 2,
            killed: 0,
            abandoned: 0
        }
    );
    assert!(killed.is_empty(), "stopped servers must not be killed");
}

#[test]
fn a_server_that_ignores_shutdown_is_killed_at_the_deadline() {
    let polite = Uuid::new_v4();
    let stubborn = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[polite, stubborn]);
    // Only the polite server's transport close completes; `done` stays alive, as the
    // stubborn server's waiter task would be.
    done.send(polite).unwrap();

    let with_process = HashSet::from([polite, stubborn]);
    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(start + GRACE, fake_kill(&with_process, &mut killed));
    let elapsed = start.elapsed();

    assert!(elapsed >= GRACE, "must give the server the whole grace");
    assert!(elapsed < GRACE + SLACK, "must not wait past the deadline");
    assert_eq!(killed, vec![stubborn]);
    assert_eq!(
        outcome,
        McpAppExitOutcome {
            stopped: 1,
            killed: 1,
            abandoned: 0
        }
    );
    drop(done);
}

#[test]
fn a_server_with_no_process_is_abandoned_not_killed() {
    // HTTP/SSE, or a stdio server whose child already exited and was released.
    let http = Uuid::new_v4();
    let (shutdown, _done) = shutdown_with(&[http]);

    let with_process = HashSet::new();
    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(start + GRACE, fake_kill(&with_process, &mut killed));

    assert!(killed.is_empty());
    assert_eq!(outcome.abandoned, 1);
}

#[test]
fn completions_already_received_count_even_after_the_deadline() {
    // The language-server shutdown shares the deadline and may use all of it; a
    // server whose close already completed must not then be killed.
    let stdio = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[stdio]);
    done.send(stdio).unwrap();

    let with_process = HashSet::from([stdio]);
    let mut killed = Vec::new();
    let outcome = shutdown.finish_with(
        Instant::now() - Duration::from_millis(1),
        fake_kill(&with_process, &mut killed),
    );

    assert!(killed.is_empty());
    assert_eq!(outcome.stopped, 1);
}

#[test]
fn stops_waiting_when_every_waiter_is_gone() {
    // A waiter dropped without reporting (its executor went away) leaves the server
    // unconfirmed: don't wait out the deadline for it. Whether it is then killed is
    // up to its handle, which a reaped child no longer holds.
    let stdio = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[stdio]);
    drop(done);

    let with_process = HashSet::from([stdio]);
    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(
        start + Duration::from_secs(30),
        fake_kill(&with_process, &mut killed),
    );

    assert!(start.elapsed() < SLACK);
    assert_eq!(outcome.killed, 1);
}

#[test]
fn a_failed_kill_is_counted_as_abandoned() {
    let stdio = Uuid::new_v4();
    let (shutdown, _done) = shutdown_with(&[stdio]);

    let outcome = shutdown.finish_with(Instant::now() + GRACE, |_, _| Some(false));

    assert_eq!(outcome.killed, 0);
    assert_eq!(outcome.abandoned, 1);
}

#[test]
fn no_servers_returns_immediately() {
    let (shutdown, _done) = shutdown_with(&[]);
    let start = Instant::now();

    let outcome = shutdown.finish_with(start + Duration::from_secs(30), |_, _| Some(true));

    assert!(start.elapsed() < SLACK);
    assert_eq!(outcome, McpAppExitOutcome::default());
}

#[test]
fn the_real_finish_leaves_an_empty_slot_alone() {
    let stdio = Uuid::new_v4();
    let (shutdown, _done) = shutdown_with(&[stdio]);

    let outcome = shutdown.finish(Instant::now());

    assert_eq!(outcome.abandoned, 1);
    assert_eq!(outcome.killed, 0);
}

#[test]
fn pid_zero_gets_no_handle() {
    // kill(0, ...) would signal our own process group.
    assert!(ChildKillHandle::open(0).is_none());
}

/// The real handle-based kill, against short-lived `sleep` children (the same
/// pattern as `local_control`'s discovery tests); every child is reaped here.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod real_process {
    use std::os::unix::process::ExitStatusExt as _;
    use std::sync::Arc;

    use super::super::{ChildKillHandle, ChildProcessSlot};

    fn sleep_child() -> std::process::Child {
        command::blocking::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep starts")
    }

    #[test]
    fn a_handle_kills_its_running_child() {
        let mut child = sleep_child();
        let handle = ChildKillHandle::open(child.id()).expect("handle opens");

        let delivered = handle.kill();
        let status = child.wait().expect("sleep is reaped");

        assert!(delivered);
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }

    #[test]
    fn a_handle_never_signals_a_reaped_child() {
        // The crash case: the server died and rmcp reaped it long before quit, so its
        // pid is free for reuse. The handle must not signal whatever holds it now.
        let mut child = sleep_child();
        let handle = ChildKillHandle::open(child.id()).expect("handle opens");
        child.kill().expect("sleep is killed");
        child.wait().expect("sleep is reaped");

        assert!(!handle.kill(), "a reaped child must not be signalled");
    }

    #[test]
    fn a_released_slot_kills_nothing() {
        let mut child = sleep_child();
        let slot = Arc::new(ChildProcessSlot::default());
        slot.fill(ChildKillHandle::open(child.id()).expect("handle opens"));

        slot.release();

        assert_eq!(slot.kill(), None);
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "a released child must be left running"
        );
        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }

    #[test]
    fn a_filled_slot_kills_once() {
        let mut child = sleep_child();
        let slot = ChildProcessSlot::default();
        slot.fill(ChildKillHandle::open(child.id()).expect("handle opens"));

        assert_eq!(slot.kill(), Some(true));
        assert_eq!(slot.kill(), None, "the handle is consumed by the kill");
        assert!(!slot.is_filled());
        let status = child.wait().expect("sleep is reaped");
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }
}
