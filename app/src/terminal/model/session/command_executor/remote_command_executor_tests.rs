use std::time::Duration;

use futures::FutureExt as _;

use super::{InflightLimit, command_timeout_from_env, max_inflight_from_env};

/// #1240: the in-flight limit is a counting semaphore whose waiters leave the queue with their
/// futures, so a superseded generator never holds a slot it will not use.
#[test]
fn inflight_limit_admits_up_to_the_limit_and_frees_a_slot_when_a_permit_drops() {
    let limit = InflightLimit::new(Some(2));
    let first = limit.acquire().now_or_never().flatten();
    let second = limit.acquire().now_or_never().flatten();
    assert!(first.is_some() && second.is_some());
    assert_eq!(limit.available(), Some(0));

    // The third request waits...
    let mut third = Box::pin(limit.acquire());
    assert!(third.as_mut().now_or_never().is_none());

    // ...and a waiter that gives up (its caller was superseded) leaves no trace.
    drop(third);
    assert_eq!(limit.available(), Some(0));

    // A finished request frees its slot for the next one.
    drop(first);
    assert!(limit.acquire().now_or_never().flatten().is_some());
}

#[test]
fn an_unlimited_executor_never_waits() {
    let limit = InflightLimit::new(None);
    assert_eq!(limit.available(), None);
    for _ in 0..64 {
        // No permit to hold, and never pending.
        assert!(limit.acquire().now_or_never().is_some());
    }
}

#[test]
fn max_inflight_env_parsing() {
    assert_eq!(max_inflight_from_env(None), Some(4));
    assert_eq!(max_inflight_from_env(Some("")), Some(4));
    assert_eq!(max_inflight_from_env(Some("lots")), Some(4));
    assert_eq!(max_inflight_from_env(Some(" 2 ")), Some(2));
    assert_eq!(max_inflight_from_env(Some("0")), None);
}

#[test]
fn command_timeout_env_parsing() {
    assert_eq!(
        command_timeout_from_env(None),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        command_timeout_from_env(Some("x")),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        command_timeout_from_env(Some("15")),
        Some(Duration::from_secs(15))
    );
    assert_eq!(command_timeout_from_env(Some("0")), None);
}
