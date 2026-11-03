use std::cell::Cell;

use super::PtyStreamBarrier;

/// The master holds no unread byte.
fn nothing_unread() -> Option<bool> {
    Some(false)
}

#[test]
fn a_fresh_barrier_with_an_empty_master_is_drained() {
    let barrier = PtyStreamBarrier::default();
    assert!(barrier.drained_with(nothing_unread));
}

#[test]
fn bytes_read_but_not_yet_applied_are_not_drained() {
    let barrier = PtyStreamBarrier::default();
    barrier.begin_read();
    barrier.end_read(120);
    assert_eq!(barrier.bytes_read(), 120);
    assert!(
        !barrier.drained_with(nothing_unread),
        "read 120, applied 0: a Precmd in those bytes may be unparsed"
    );
    barrier.publish_applied(100);
    assert!(
        !barrier.drained_with(nothing_unread),
        "read 120, applied 100"
    );
    barrier.publish_applied(120);
    assert!(barrier.drained_with(nothing_unread), "read == applied");
}

#[test]
fn unread_bytes_on_the_master_or_an_unknown_answer_are_not_drained() {
    let barrier = PtyStreamBarrier::default();
    barrier.begin_read();
    barrier.end_read(10);
    barrier.publish_applied(10);
    assert!(
        !barrier.drained_with(|| Some(true)),
        "FIONREAD / poll says the master holds bytes"
    );
    assert!(
        !barrier.drained_with(|| None),
        "the platform cannot tell: today's behaviour"
    );
    assert!(barrier.drained_with(nothing_unread));
}

#[test]
fn a_read_in_flight_is_not_drained_and_the_master_is_not_even_asked() {
    let barrier = PtyStreamBarrier::default();
    barrier.begin_read();
    let asked = Cell::new(false);
    assert!(!barrier.drained_with(|| {
        asked.set(true);
        Some(false)
    }));
    assert!(!asked.get());
    barrier.end_read(0);
    assert!(
        barrier.drained_with(nothing_unread),
        "WouldBlock: nothing read"
    );
}

#[test]
fn a_read_that_starts_or_finishes_during_the_check_is_not_drained() {
    // The bytes left the master (so it reads empty) but were not counted when the counts were
    // looked at: the epoch moved, so the answer is "not drained" even though the counts agree.
    let barrier = PtyStreamBarrier::default();
    assert!(!barrier.drained_with(|| {
        barrier.begin_read();
        Some(false)
    }));
    barrier.end_read(5);
    barrier.publish_applied(5);
    assert!(!barrier.drained_with(|| {
        barrier.begin_read();
        barrier.end_read(0);
        Some(false)
    }));
    assert!(barrier.drained_with(nothing_unread));
}

#[test]
fn counts_accumulate_across_reads() {
    let barrier = PtyStreamBarrier::default();
    for chunk in [3usize, 0, 4096, 1] {
        barrier.begin_read();
        barrier.end_read(chunk);
    }
    assert_eq!(barrier.bytes_read(), 4100);
    barrier.publish_applied(4100);
    assert_eq!(barrier.bytes_applied(), 4100);
    assert!(barrier.drained_with(nothing_unread));
}
