//! Tests for shutting down the PTY reader thread (#1135).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::*;

/// Closing a tab must not wait out a reader that is busy: the join moves to a background thread
/// after the timeout, and still collects the reader once it finishes.
#[test]
fn a_busy_reader_does_not_block_the_caller_past_the_timeout() {
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

    let started = Instant::now();
    join_event_loop(handle, Some(Duration::from_millis(50)));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "join_event_loop waited for a busy reader"
    );
    assert!(!finished.load(Ordering::SeqCst));

    // The background join still lets the reader finish.
    release.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(finished.load(Ordering::SeqCst));
}

#[test]
fn a_reader_that_exits_promptly_is_joined_in_place() {
    let handle = std::thread::spawn(|| {});
    let started = Instant::now();
    join_event_loop(handle, Some(Duration::from_secs(10)));
    assert!(started.elapsed() < Duration::from_secs(5));
}
