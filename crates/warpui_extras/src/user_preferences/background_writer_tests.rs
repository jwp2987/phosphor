use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{BackgroundWriter, FailureFn, WriteFn};

type Writes = Arc<Mutex<Vec<String>>>;

/// A writer over an in-memory "disk" that records every write it is asked to make.
fn recording_writer(debounce: Duration, write_delay: Duration) -> (BackgroundWriter, Writes) {
    let writes: Writes = Arc::default();
    let sink = writes.clone();
    let write: WriteFn = Arc::new(move |_: &Path, data: &str| {
        std::thread::sleep(write_delay);
        sink.lock().unwrap().push(data.to_owned());
        Ok(())
    });
    let on_failure: FailureFn = Arc::new(|_| {});
    let writer = BackgroundWriter::new("settings.toml".into(), debounce, write, on_failure)
        .expect("writer thread should start");
    (writer, writes)
}

#[test]
fn a_burst_of_changes_is_coalesced_into_one_write_of_the_latest() {
    let (writer, writes) = recording_writer(Duration::from_millis(100), Duration::ZERO);
    for i in 0..200 {
        writer.queue(format!("value = {i}")).unwrap();
    }
    writer.flush().unwrap();
    // The old code wrote on every call (200 writes, all on the calling thread).
    let writes = writes.lock().unwrap();
    assert!(
        writes.len() <= 2,
        "200 queued changes must collapse to a write or two, got {}",
        writes.len()
    );
    assert_eq!(writes.last().map(String::as_str), Some("value = 199"));
}

#[test]
fn queue_returns_immediately_even_when_the_write_is_slow() {
    let (writer, writes) = recording_writer(Duration::from_millis(10), Duration::from_millis(300));
    let start = Instant::now();
    for i in 0..20 {
        writer.queue(format!("v = {i}")).unwrap();
    }
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "queueing must not wait for the disk, took {:?}",
        start.elapsed()
    );
    writer.flush().unwrap();
    assert_eq!(
        writes.lock().unwrap().last().map(String::as_str),
        Some("v = 19")
    );
}

#[test]
fn flush_writes_immediately_without_waiting_for_the_debounce() {
    let (writer, writes) = recording_writer(Duration::from_secs(60), Duration::ZERO);
    writer.queue("last = 1".to_owned()).unwrap();
    let start = Instant::now();
    writer.flush().unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(*writes.lock().unwrap(), vec!["last = 1".to_owned()]);
}

#[test]
fn dropping_the_writer_flushes_the_last_change() {
    let (writer, writes) = recording_writer(Duration::from_secs(60), Duration::ZERO);
    writer.queue("quit = true".to_owned()).unwrap();
    drop(writer);
    assert_eq!(*writes.lock().unwrap(), vec!["quit = true".to_owned()]);
}

#[test]
fn pending_write_lands_on_its_own_after_the_debounce() {
    let (writer, writes) = recording_writer(Duration::from_millis(30), Duration::ZERO);
    writer.queue("a = 1".to_owned()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while writes.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(*writes.lock().unwrap(), vec!["a = 1".to_owned()]);
}

#[test]
fn own_writes_are_recognised_and_foreign_content_is_not() {
    let (writer, _writes) = recording_writer(Duration::from_millis(10), Duration::ZERO);
    writer.queue("mine = 1".to_owned()).unwrap();
    writer.queue("mine = 2".to_owned()).unwrap();
    writer.flush().unwrap();
    assert!(writer.is_recent_write(b"mine = 2"));
    assert!(!writer.is_recent_write(b"edited_by_hand = true"));
}

#[test]
fn discarded_pending_write_is_never_written() {
    let (writer, writes) = recording_writer(Duration::from_secs(60), Duration::ZERO);
    writer.queue("stale = 1".to_owned()).unwrap();
    writer.discard_pending();
    writer.flush().unwrap();
    assert!(writes.lock().unwrap().is_empty());
}

#[test]
fn failure_is_reported_once_and_surfaced_by_flush() {
    let failures = Arc::new(AtomicUsize::new(0));
    let counter = failures.clone();
    let write: WriteFn = Arc::new(|_: &Path, _: &str| Err(std::io::Error::other("disk full")));
    let on_failure: FailureFn = Arc::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let writer = BackgroundWriter::new(
        "settings.toml".into(),
        Duration::from_millis(5),
        write,
        on_failure,
    )
    .unwrap();
    writer.queue("a = 1".to_owned()).unwrap();
    assert!(writer.flush().unwrap_err().contains("disk full"));
    writer.queue("a = 2".to_owned()).unwrap();
    assert!(writer.flush().is_err());
    assert_eq!(
        failures.load(Ordering::SeqCst),
        1,
        "reported on the transition to failing, not on every retry"
    );
}

/// A write that failed is kept pending and tried again (#1248): the stores skip a value equal to
/// the one in memory, so a dropped write would never have been made again until some other
/// setting changed. The failure is still reported once, on the transition.
#[test]
fn a_failed_write_is_retried_until_the_disk_takes_it() {
    let writes: Writes = Arc::default();
    let sink = writes.clone();
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempt_counter = attempts.clone();
    let write: WriteFn = Arc::new(move |_: &Path, data: &str| {
        if attempt_counter.fetch_add(1, Ordering::SeqCst) < 2 {
            return Err(std::io::Error::other("disk full"));
        }
        sink.lock().unwrap().push(data.to_owned());
        Ok(())
    });
    let failures = Arc::new(AtomicUsize::new(0));
    let failure_counter = failures.clone();
    let on_failure: FailureFn = Arc::new(move |_| {
        failure_counter.fetch_add(1, Ordering::SeqCst);
    });
    let writer = BackgroundWriter::new_with_retry_delay(
        "settings.toml".into(),
        Duration::from_millis(5),
        Duration::from_millis(10),
        write,
        on_failure,
    )
    .unwrap();
    writer.queue("a = 1".to_owned()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while writes.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        *writes.lock().unwrap(),
        vec!["a = 1".to_owned()],
        "the write that failed twice must land on the third attempt"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert!(writer.flush().is_ok(), "the last write succeeded");
}

/// An older own write found on disk while nothing is pending was put there by someone else (a
/// `git checkout` of the settings file) and is reloaded; only while a newer write is pending or
/// in flight is the older content the echo of our own write (#1249). The newest write is always
/// recognised.
#[test]
fn an_older_own_write_is_an_echo_only_while_a_newer_one_is_pending() {
    let (writer, _writes) = recording_writer(Duration::from_secs(60), Duration::ZERO);
    for version in 1..=3 {
        writer.queue(format!("v = {version}")).unwrap();
        writer.flush().unwrap();
    }
    assert!(writer.is_recent_write(b"v = 3"));
    assert!(
        !writer.is_recent_write(b"v = 1"),
        "with nothing pending, an older own write on disk is a hand restore to reload"
    );
    assert!(!writer.is_recent_write(b"v = 2"));

    // v = 4 waits out the 60 s debounce: the file legitimately still holds v = 3.
    writer.queue("v = 4".to_owned()).unwrap();
    assert!(writer.is_recent_write(b"v = 3"));
    // Only the newest two writes can be on disk; v = 1 and v = 2 are a user's hand edit even
    // while a write is pending (#1253).
    assert!(!writer.is_recent_write(b"v = 1"));
    assert!(!writer.is_recent_write(b"v = 2"));
    writer.discard_pending();
    assert!(!writer.is_recent_write(b"v = 1"));
    assert!(writer.is_recent_write(b"v = 3"));
}
