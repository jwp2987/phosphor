use std::time::Duration;

use super::ComputeClock;

#[test]
fn only_running_stages_count_as_compute() {
    let clock = ComputeClock::default();
    assert_eq!(clock.spent(), Duration::ZERO);
    assert!(!clock.is_running());

    // Time passing with no stage running (queued behind a stalled main thread) is not compute.
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(clock.spent(), Duration::ZERO);

    let (clock, guard) = ComputeClock::running_for_test(Duration::from_millis(30));
    assert!(clock.is_running());
    assert!(clock.spent() >= Duration::from_millis(30));
    drop(guard);
    assert!(!clock.is_running());
    let booked = clock.spent();
    assert!(booked >= Duration::from_millis(30));
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(clock.spent(), booked, "an idle clock does not advance");
}

#[test]
fn stage_start_is_announced() {
    let clock = ComputeClock::default();
    let (tx, rx) = async_channel::unbounded();
    clock.notify_on_stage_start(tx);
    let first = clock.begin();
    assert!(
        rx.try_recv().is_ok(),
        "beginning a stage notifies the waiter"
    );
    drop(first);
    clock.stop_notifying();
    let _second = clock.begin();
    assert!(rx.try_recv().is_err(), "a stopped watch is not notified");
}
