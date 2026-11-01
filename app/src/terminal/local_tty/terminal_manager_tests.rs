//! Tests for shutting down the PTY reader thread (#1135, #1376).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::*;

/// Closing a tab (`wait == false`) never joins on the calling thread: it returns while the reader
/// is still running (it cannot finish until released, so a join here would deadlock the test),
/// and a background reaper still collects the reader once it finishes.
#[test]
fn closing_does_not_join_on_the_calling_thread() {
    let release = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let handle = {
        let (release, finished) = (release.clone(), finished.clone());
        std::thread::spawn(move || {
            while !release.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            finished.store(true, Ordering::SeqCst);
        })
    };

    // Would never return if this joined in place: the reader waits on `release`, set below.
    join_event_loop(handle, false);
    assert!(!finished.load(Ordering::SeqCst));

    // The background join still lets the reader finish.
    release.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(finished.load(Ordering::SeqCst));
}

/// Closing N tabs in a row hands N joins to reapers without waiting on any of them.
#[test]
fn closing_many_tabs_never_waits_on_a_reader() {
    let release = Arc::new(AtomicBool::new(false));
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let release = release.clone();
            std::thread::spawn(move || {
                while !release.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        })
        .collect();
    for handle in handles {
        join_event_loop(handle, false);
    }
    release.store(true, Ordering::SeqCst);
}

#[test]
fn waiting_join_blocks_until_the_reader_finished() {
    let finished = Arc::new(AtomicBool::new(false));
    let handle = {
        let finished = finished.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            finished.store(true, Ordering::SeqCst);
        })
    };
    join_event_loop(handle, true);
    assert!(finished.load(Ordering::SeqCst));
}
