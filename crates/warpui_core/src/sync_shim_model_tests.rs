//! Interleaving tests of the modelled `RwLock` (see `crate::sync_shim`): it must grant what
//! parking_lot grants (a nested `read_recursive`, even with a writer waiting) and panic on what
//! deadlocks with parking_lot (a thread re-entering its own write, or writing while it reads).
//!
//! ```text
//! RUSTFLAGS="--cfg shuttle" cargo test -p warpui_core --lib --release -- model_tests::
//! RUSTFLAGS="--cfg loom"    cargo test -p warpui_core --lib --release -- model_tests::
//! ```

use super::*;

/// Two read guards alive on one thread while another thread waits to write: both guards see the
/// same value, the writer is not lost, and nothing deadlocks.
#[test]
fn model_rwlock_nested_recursive_reads_with_a_waiting_writer() {
    check(|| {
        let lock = Arc::new(RwLock::new(0u32));
        let writer = {
            let lock = lock.clone();
            thread::spawn(move || *lock.write() += 1)
        };
        {
            let outer = lock.read_recursive();
            let inner = lock.read_recursive();
            assert_eq!(*outer, *inner);
            assert!(*outer <= 1);
        }
        writer.join().unwrap();
        assert_eq!(*lock.read_recursive(), 1);
    });
}

/// Writers in [`model_rwlock_writers_serialize_and_exclude_readers`]: loom explores every
/// schedule, so it gets one writer (against the reader) to stay tractable; shuttle gets two.
const WRITERS: u32 = if cfg!(loom) { 1 } else { 2 };

/// Writers exclude each other and readers: every increment lands, and a reader never sees a
/// value no writer produced.
#[test]
fn model_rwlock_writers_serialize_and_exclude_readers() {
    check(|| {
        let lock = Arc::new(RwLock::new(0u32));
        let writers: Vec<_> = (0..WRITERS)
            .map(|_| {
                let lock = lock.clone();
                thread::spawn(move || {
                    let mut guard = lock.write();
                    let seen = *guard;
                    *guard = seen + 1;
                })
            })
            .collect();
        let seen = *lock.read_recursive();
        assert!(seen <= WRITERS);
        if let Some(guard) = lock.try_read_recursive() {
            assert!(*guard >= seen);
        }
        for writer in writers {
            writer.join().unwrap();
        }
        assert_eq!(*lock.read_recursive(), WRITERS);
    });
}

/// Writing while this thread reads deadlocks with parking_lot: the model must say so.
#[test]
#[should_panic(expected = "while holding")]
fn model_rwlock_write_while_reading_panics() {
    check(|| {
        let lock = RwLock::new(0u32);
        let _read = lock.read_recursive();
        let _write = lock.write();
    });
}

/// Reading while this thread writes deadlocks with parking_lot: the model must say so.
#[test]
#[should_panic(expected = "while holding")]
fn model_rwlock_read_while_writing_panics() {
    check(|| {
        let lock = RwLock::new(0u32);
        let _write = lock.write();
        let _read = lock.read_recursive();
    });
}

/// A nested plain `read` deadlocks with parking_lot once a writer queues between the two reads:
/// the model must say so, with or without a writer.
#[test]
#[should_panic(expected = "nested non-recursive read")]
fn model_rwlock_nested_plain_read_panics() {
    check(|| {
        let lock = RwLock::new(0u32);
        let _outer = lock.read();
        let _inner = lock.read();
    });
}

/// A cross-thread deadlock (this thread reads, then waits for a writer that waits for that read)
/// is reported by the checker; the read guard's `Drop` runs while that panic unwinds and must not
/// abort. Both checkers' messages say "deadlock".
#[test]
#[should_panic(expected = "deadlock")]
fn model_rwlock_cross_thread_deadlock_is_reported() {
    check(|| {
        let lock = Arc::new(RwLock::new(0u32));
        let _read = lock.read_recursive();
        let writer = {
            let lock = lock.clone();
            thread::spawn(move || *lock.write() += 1)
        };
        writer.join().unwrap();
    });
}
