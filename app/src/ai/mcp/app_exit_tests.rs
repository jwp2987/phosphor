use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use instant::Instant;
use uuid::Uuid;

use super::{McpAppExitOutcome, McpAppExitShutdown, kill_child_process};

const GRACE: Duration = Duration::from_millis(200);
/// Generous upper slack for a loaded CI box; the point is "bounded", not "exact".
const SLACK: Duration = Duration::from_secs(2);

fn shutdown_with(servers: &[(Uuid, Option<u32>)]) -> (McpAppExitShutdown, mpsc::Sender<Uuid>) {
    let (done_tx, done_rx) = mpsc::channel();
    let pending = servers.iter().copied().collect::<HashMap<_, _>>();
    (McpAppExitShutdown::new(done_rx, pending), done_tx)
}

#[test]
fn returns_early_once_every_server_has_stopped_and_kills_nothing() {
    let stdio = Uuid::new_v4();
    let http = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[(stdio, Some(4242)), (http, None)]);
    assert_eq!(shutdown.pending(), 2);
    done.send(stdio).unwrap();
    done.send(http).unwrap();

    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(start + Duration::from_secs(30), |pid| {
        killed.push(pid);
        true
    });

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
    let (shutdown, done) = shutdown_with(&[(polite, Some(1111)), (stubborn, Some(2222))]);
    // Only the polite server's transport close completes; `done` stays alive, as the
    // stubborn server's waiter task would be.
    done.send(polite).unwrap();

    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(start + GRACE, |pid| {
        killed.push(pid);
        true
    });
    let elapsed = start.elapsed();

    assert!(elapsed >= GRACE, "must give the server the whole grace");
    assert!(elapsed < GRACE + SLACK, "must not wait past the deadline");
    assert_eq!(killed, vec![2222]);
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
fn an_unfinished_http_server_is_abandoned_not_killed() {
    let http = Uuid::new_v4();
    let (shutdown, _done) = shutdown_with(&[(http, None)]);

    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(start + GRACE, |pid| {
        killed.push(pid);
        true
    });

    assert!(
        killed.is_empty(),
        "an HTTP/SSE server has no process of ours"
    );
    assert_eq!(outcome.abandoned, 1);
}

#[test]
fn completions_already_received_count_even_after_the_deadline() {
    // The language-server shutdown shares the deadline and may use all of it; a
    // server whose close already completed (so rmcp reaped its child) must not
    // then be signalled by a pid that may have been reused.
    let stdio = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[(stdio, Some(3333))]);
    done.send(stdio).unwrap();

    let mut killed = Vec::new();
    let outcome = shutdown.finish_with(Instant::now() - Duration::from_millis(1), |pid| {
        killed.push(pid);
        true
    });

    assert!(killed.is_empty());
    assert_eq!(outcome.stopped, 1);
}

#[test]
fn stops_waiting_when_every_waiter_is_gone_and_kills_the_rest() {
    // A waiter dropped without reporting (its executor went away) leaves the server
    // unconfirmed; don't wait out the deadline for it, kill it.
    let stdio = Uuid::new_v4();
    let (shutdown, done) = shutdown_with(&[(stdio, Some(4444))]);
    drop(done);

    let mut killed = Vec::new();
    let start = Instant::now();
    let outcome = shutdown.finish_with(start + Duration::from_secs(30), |pid| {
        killed.push(pid);
        true
    });

    assert!(start.elapsed() < SLACK);
    assert_eq!(killed, vec![4444]);
    assert_eq!(outcome.killed, 1);
}

#[test]
fn a_failed_kill_is_counted_as_abandoned() {
    let stdio = Uuid::new_v4();
    let (shutdown, _done) = shutdown_with(&[(stdio, Some(5555))]);

    let outcome = shutdown.finish_with(Instant::now() + GRACE, |_| false);

    assert_eq!(outcome.killed, 0);
    assert_eq!(outcome.abandoned, 1);
}

#[test]
fn no_servers_returns_immediately() {
    let (shutdown, _done) = shutdown_with(&[]);
    let start = Instant::now();

    let outcome = shutdown.finish_with(start + Duration::from_secs(30), |_| true);

    assert!(start.elapsed() < SLACK);
    assert_eq!(outcome, McpAppExitOutcome::default());
}

#[test]
fn pid_zero_is_never_signalled() {
    // kill(0, SIGKILL) would take down our own process group.
    assert!(!kill_child_process(0));
}

/// The real kill path, against a short-lived `sleep` child (the same pattern as
/// `local_control`'s discovery tests); the child is reaped here either way.
#[cfg(unix)]
#[test]
fn kill_child_process_kills_a_running_child() {
    use std::os::unix::process::ExitStatusExt as _;

    let mut child = command::blocking::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("sleep starts");

    let delivered = kill_child_process(child.id());
    let status = child.wait().expect("sleep is reaped");

    assert!(delivered);
    assert_eq!(status.signal(), Some(libc::SIGKILL));
}
