use std::cell::Cell;
use std::time::Duration;

use instant::Instant;

use super::*;

const DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn pump_returns_completed_when_work_finishes_before_the_deadline() {
    let spent = Cell::new(Duration::ZERO);
    let pumps = Cell::new(0);
    let outcome = pump_until(
        DEADLINE,
        || spent.get(),
        || pumps.get() >= 3,
        |_| {
            pumps.set(pumps.get() + 1);
            spent.set(spent.get() + Duration::from_millis(10));
        },
    );
    assert_eq!(outcome, PumpOutcome::Completed);
    assert_eq!(pumps.get(), 3);
}

#[test]
fn pump_times_out_at_the_deadline_and_hands_pump_the_time_left() {
    let spent = Cell::new(Duration::ZERO);
    let remaining_seen = Cell::new(Vec::new());
    let outcome = pump_until(
        DEADLINE,
        || spent.get(),
        || false,
        |remaining| {
            let mut seen = remaining_seen.take();
            seen.push(remaining);
            remaining_seen.set(seen);
            spent.set(spent.get() + Duration::from_secs(2));
        },
    );
    assert_eq!(outcome, PumpOutcome::TimedOut);
    assert_eq!(
        remaining_seen.take(),
        vec![
            Duration::from_secs(5),
            Duration::from_secs(3),
            Duration::from_secs(1)
        ]
    );
}

#[test]
fn pump_prefers_completed_over_timed_out_at_the_deadline() {
    let outcome = pump_until(DEADLINE, || DEADLINE, || true, |_| unreachable!());
    assert_eq!(outcome, PumpOutcome::Completed);
}

#[test]
fn pump_with_an_already_elapsed_deadline_never_pumps() {
    let outcome = pump_until(DEADLINE, || DEADLINE, || false, |_| unreachable!());
    assert_eq!(outcome, PumpOutcome::TimedOut);
}

#[test]
fn gate_is_open_when_no_session_end_was_reported() {
    let gate = SessionEndGate::new();
    assert!(!gate.should_keep_exited_pane(Instant::now()));
}

#[test]
fn gate_holds_panes_open_while_the_query_is_pending() {
    // #1103: Windows kills the console shells when the session end is only being queried; the
    // panes must survive until the user decides.
    let t0 = Instant::now();
    let mut gate = SessionEndGate::new();
    gate.note_query(t0);
    assert!(gate.should_keep_exited_pane(t0));
    assert!(gate.should_keep_exited_pane(t0 + Duration::from_secs(60)));
}

#[test]
fn gate_pending_query_expires() {
    let t0 = Instant::now();
    let mut gate = SessionEndGate::new();
    gate.note_query(t0);
    assert!(!gate.should_keep_exited_pane(t0 + QUERY_PENDING_MAX));
}

#[test]
fn gate_stays_closed_to_closing_panes_for_a_session_that_is_ending() {
    let t0 = Instant::now();
    let mut gate = SessionEndGate::new();
    gate.note_query(t0);
    gate.note_end_session(true, t0);
    assert!(gate.should_keep_exited_pane(t0 + Duration::from_secs(3600)));
}

#[test]
fn gate_releases_after_a_cancelled_session_end_once_the_grace_passes() {
    let t0 = Instant::now();
    let mut gate = SessionEndGate::new();
    gate.note_query(t0);
    gate.note_end_session(false, t0);
    // Late-delivered exits from the killed shells still belong to the cancelled session end.
    assert!(gate.should_keep_exited_pane(t0 + CANCEL_GRACE - Duration::from_millis(1)));
    // After that, an ordinary shell exit closes its pane again.
    assert!(!gate.should_keep_exited_pane(t0 + CANCEL_GRACE));
}

#[test]
fn gate_a_new_query_clears_an_earlier_cancel() {
    let t0 = Instant::now();
    let mut gate = SessionEndGate::new();
    gate.note_end_session(false, t0);
    let t1 = t0 + Duration::from_secs(10);
    gate.note_query(t1);
    gate.note_end_session(false, t1 + Duration::from_secs(1));
    assert!(!gate.should_keep_exited_pane(t1 + Duration::from_secs(1) + CANCEL_GRACE));
}
