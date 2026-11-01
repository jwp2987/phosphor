//! Interleaving tests of the clipboard's cross-thread primitives, run under shuttle (random
//! schedules) or loom (every schedule); see `crate::sync_shim`.
//!
//! ```text
//! RUSTFLAGS="--cfg shuttle" cargo test -p warpui_core --lib --release -- model_tests::
//! RUSTFLAGS="--cfg loom"    cargo test -p warpui_core --lib --release -- model_tests::
//! ```
//!
//! Every test builds its own instances; the module's `static`s are never touched (a model's
//! primitives cannot outlive one execution).

use super::*;
use crate::sync_shim::atomic::{AtomicUsize, Ordering};
use crate::sync_shim::{Arc, check, thread};

fn text(s: &str) -> Arc<ClipboardContent> {
    Arc::new(ClipboardContent::plain_text(s.to_owned()))
}

/// The quit path: `wait_until_idle` must wake when the last pending copy lands, whichever side
/// reaches the lock first (no lost wakeup between the emptiness check and the wait).
#[test]
fn model_pending_write_flush_wakes_the_waiter() {
    check(|| {
        let writes = Arc::new(PendingWrites::new());
        writes.add(1, ClipboardTarget::Clipboard, text("copied"));

        let writer = {
            let writes = writes.clone();
            thread::spawn(move || writes.finish(1))
        };
        // The deadline is ignored by the model builds: the wait ends only by a notification.
        assert!(writes.wait_until_idle(
            ClipboardTarget::Clipboard,
            std::time::Duration::from_secs(3600)
        ));
        assert!(writes.latest(ClipboardTarget::Clipboard).is_none());
        writer.join().unwrap();
    });
}

/// A read racing a copy sees the pending text until the write is forgotten, and never a stale
/// one: `latest` is the newest generation still pending.
#[test]
fn model_pending_write_latest_is_the_newest_unfinished() {
    check(|| {
        let writes = Arc::new(PendingWrites::new());
        writes.add(1, ClipboardTarget::Clipboard, text("old"));
        writes.add(2, ClipboardTarget::Clipboard, text("new"));

        let finisher = {
            let writes = writes.clone();
            thread::spawn(move || writes.finish(1))
        };
        let seen = writes
            .latest(ClipboardTarget::Clipboard)
            .expect("write 2 is pending");
        assert_eq!(seen.plain_text, "new");
        finisher.join().unwrap();
        writes.finish(2);
        assert!(writes.latest(ClipboardTarget::Clipboard).is_none());
    });
}

/// Writes that reach the target are applied in generation order, whatever order the threads run
/// in: a superseded write is dropped, never applied late.
#[test]
fn model_write_order_never_applies_an_older_generation_after_a_newer() {
    check(|| {
        let order = Arc::new(WriteOrder::new());
        let applied = Arc::new(Mutex::new(Vec::<u64>::new()));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let order = order.clone();
            let applied = applied.clone();
            handles.push(thread::spawn(move || {
                let generation = order.next_generation();
                order.run(generation, ClipboardTarget::Clipboard, || {
                    applied.lock().push(generation);
                    true
                });
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let applied = applied.lock();
        assert!(
            !applied.is_empty(),
            "the newest generation is always written"
        );
        assert!(
            applied.windows(2).all(|pair| pair[0] < pair[1]),
            "applied out of order: {applied:?}"
        );
        assert_eq!(*applied.last().unwrap(), 2, "the newest generation wins");
    });
}

/// At most one availability probe runs per clipboard at a time, and an owner change that lands
/// while one runs is never lost: the cache ends on the answer of the changed world.
#[test]
fn model_availability_probe_is_exclusive_and_loses_no_owner_change() {
    check(|| {
        let cache = Arc::new(AvailabilityCache::new());
        // 0 = the clipboard is empty, 1 = it has content.
        let world = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicUsize::new(0));

        let probe = {
            let world = world.clone();
            let running = running.clone();
            move |_: ClipboardTarget| {
                assert_eq!(
                    running.fetch_add(1, Ordering::SeqCst),
                    0,
                    "two probes at once"
                );
                let answer = if world.load(Ordering::SeqCst) == 1 {
                    Availability::HasContent
                } else {
                    Availability::Empty
                };
                running.fetch_sub(1, Ordering::SeqCst);
                answer
            }
        };

        let refresher = {
            let cache = cache.clone();
            let probe = probe.clone();
            thread::spawn(move || cache.refresh_blocking(ClipboardTarget::Clipboard, &probe))
        };
        let owner_change = {
            let cache = cache.clone();
            let world = world.clone();
            thread::spawn(move || {
                world.store(1, Ordering::SeqCst);
                if cache.invalidate(ClipboardTarget::Clipboard) {
                    cache.refresh_blocking(ClipboardTarget::Clipboard, &probe);
                }
            })
        };
        refresher.join().unwrap();
        owner_change.join().unwrap();

        assert_eq!(
            cache.get(ClipboardTarget::Clipboard),
            Availability::HasContent
        );
        // The slot is free again: nothing is stuck in flight.
        assert!(cache.begin(ClipboardTarget::Clipboard).is_some());
    });
}

/// The blocked-reader cap (#1375): with `max` readers allowed to be blocked, the platform call
/// of a reader never overlaps another's, and a reader turned away answers `TimedOut` without
/// calling it. Every interleaving of the count's increment, the check and the guard's decrement
/// is covered under loom.
#[test]
fn model_tracked_reader_cap_never_overlaps_blocking_calls() {
    check(|| {
        // The tracker must be `'static`; one is leaked per execution (a few bytes).
        let tracker: &'static ReaderTracker = Box::leak(Box::new(ReaderTracker::new()));
        let concurrent = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let concurrent = concurrent.clone();
            let reader = tracked_reader_with(
                tracker,
                1,
                SLOW_READ,
                ReadRequestId::next(),
                ReadKind::Full,
                Box::new(move || {
                    assert_eq!(
                        concurrent.fetch_add(1, Ordering::SeqCst),
                        0,
                        "two platform calls blocked at once past a cap of 1"
                    );
                    concurrent.fetch_sub(1, Ordering::SeqCst);
                    ReadOutcome::Content(ClipboardContent::default())
                }),
            );
            handles.push(thread::spawn(reader));
        }
        let outcomes: Vec<ReadOutcome> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert!(
            outcomes
                .iter()
                .any(|outcome| matches!(outcome, ReadOutcome::Content(_))),
            "the first reader always runs"
        );
        assert!(
            outcomes
                .iter()
                .all(|outcome| matches!(outcome, ReadOutcome::Content(_) | ReadOutcome::TimedOut))
        );
        assert_eq!(tracker.in_flight(), 0, "a reader leaked its in-flight slot");
    });
}
