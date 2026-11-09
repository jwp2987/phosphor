//! A capturing tap on the unit-test logger (I6: no ERROR line outside the chaos allowlist).
//!
//! `warp_logging` lets one tap be registered per test binary (`set_unit_test_log_tap`), and
//! `chat_stream.rs`'s own capture tests register one too and assert that they got it. The suite
//! runs under nextest, one process per test, so the two never meet; under plain `cargo test` the
//! first test to ask wins and the other would fail its assertion. [`install`] therefore reports
//! whether capture is available instead of asserting, and the oracle then skips I6 loudly.
//!
//! Records are filed with the thread that logged them and queried by thread: `App::test` runs the
//! app on the test thread, so a case sees its own lines even when tests share a process.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};
use std::thread::ThreadId;

struct Record {
    thread: ThreadId,
    level: log::Level,
    message: String,
}

static RECORDS: Mutex<Vec<Record>> = Mutex::new(Vec::new());
static AVAILABLE: AtomicBool = AtomicBool::new(false);

fn records() -> MutexGuard<'static, Vec<Record>> {
    RECORDS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn tap(record: &log::Record<'_>) {
    if record.level() <= log::Level::Warn {
        records().push(Record {
            thread: std::thread::current().id(),
            level: record.level(),
            message: record.args().to_string(),
        });
    }
}

/// Registers the tap (once per process). `false` = no capture in this process.
pub fn install() -> bool {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let available = warp_logging::unit_test_log_tap_is_available()
            && warp_logging::set_unit_test_log_tap(tap);
        AVAILABLE.store(available, Ordering::SeqCst);
    });
    AVAILABLE.load(Ordering::SeqCst)
}

/// A position in the capture, for "everything since".
#[derive(Clone, Copy)]
pub struct Mark(usize);

pub fn mark() -> Mark {
    Mark(records().len())
}

/// ERROR lines this thread logged since `mark`.
pub fn errors_since(mark: Mark) -> Vec<String> {
    lines_since(mark, log::Level::Error)
}

/// WARN lines this thread logged since `mark` (diagnostic only; never a violation).
pub fn warnings_since(mark: Mark) -> Vec<String> {
    lines_since(mark, log::Level::Warn)
}

fn lines_since(mark: Mark, level: log::Level) -> Vec<String> {
    let me = std::thread::current().id();
    records()
        .iter()
        .skip(mark.0)
        .filter(|r| r.thread == me && r.level == level)
        .map(|r| r.message.clone())
        .collect()
}
