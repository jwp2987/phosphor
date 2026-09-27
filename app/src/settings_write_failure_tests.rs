use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{MIN_TOAST_INTERVAL, should_show_toast, take_toast_slot};

#[test]
fn first_failure_always_shows_a_toast() {
    assert!(should_show_toast(None, Instant::now()));
}

#[test]
fn a_second_failure_within_the_window_is_suppressed() {
    let last = Instant::now();
    let now = last + Duration::from_millis(1);
    assert!(!should_show_toast(Some(last), now));
}

#[test]
fn a_failure_right_at_the_boundary_is_shown() {
    let last = Instant::now();
    let now = last + MIN_TOAST_INTERVAL;
    assert!(should_show_toast(Some(last), now));
}

#[test]
fn a_failure_just_short_of_the_boundary_is_suppressed() {
    let last = Instant::now();
    let now = last + MIN_TOAST_INTERVAL - Duration::from_millis(1);
    assert!(!should_show_toast(Some(last), now));
}

#[test]
fn a_failure_well_after_the_window_is_shown() {
    let last = Instant::now();
    let now = last + MIN_TOAST_INTERVAL * 10;
    assert!(should_show_toast(Some(last), now));
}

#[test]
fn take_toast_slot_records_the_first_success_so_the_next_call_is_gated() {
    let slot: Mutex<Option<Instant>> = Mutex::new(None);
    let t0 = Instant::now();
    assert!(
        take_toast_slot(&slot, t0),
        "first call on an empty slot must pass"
    );
    assert!(
        !take_toast_slot(&slot, t0 + Duration::from_millis(1)),
        "a call immediately after must be throttled"
    );
    assert!(
        take_toast_slot(&slot, t0 + MIN_TOAST_INTERVAL),
        "a call at the boundary must pass again"
    );
}

#[test]
fn take_toast_slot_is_independent_per_slot() {
    // Two distinct slots (as two independent code paths would have if this
    // were ever split per-setting) must not throttle each other -- this is
    // what makes the pure logic testable without touching the real
    // process-global `LAST_TOAST_AT`.
    let slot_a: Mutex<Option<Instant>> = Mutex::new(None);
    let slot_b: Mutex<Option<Instant>> = Mutex::new(None);
    let now = Instant::now();
    assert!(take_toast_slot(&slot_a, now));
    assert!(take_toast_slot(&slot_b, now));
}
