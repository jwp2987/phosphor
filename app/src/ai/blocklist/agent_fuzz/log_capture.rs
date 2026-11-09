//! A capturing tap on the unit-test logger (I6: no ERROR line outside the chaos allowlist).
//!
//! `warp_logging` lets one tap be registered per test binary (`set_unit_test_log_tap`), and
//! `chat_stream.rs`'s own capture tests register one too and assert that they got it. The suite
//! runs under nextest, one process per test, so the two never meet; under plain `cargo test` the
//! first test to ask wins and the other would fail its assertion. [`install`] therefore reports
//! whether capture is available instead of asserting, and the oracle then skips I6 loudly.
//!
//! Records are NOT filtered by thread: the app spawns futures onto other threads, and their
//! ERRORs belong to the case too. The cost is that under plain `cargo test` (several tests in
//! one process) a case also sees other tests' lines; nextest gives each test its own process,
//! which is how this suite runs.
//!
//! [`PanicWatch`] does the same for panics: a panic in a background future never reaches
//! `catch_unwind` around the case, so a hook records it and the oracle reports it as I6.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

struct Record {
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
    records()
        .iter()
        .skip(mark.0)
        .filter(|r| r.level == level)
        .map(|r| r.message.clone())
        .collect()
}

static PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Records every panic (on any thread) while alive; restores the previous hook on drop.
pub struct PanicWatch {
    previous: Option<Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>>,
}

impl PanicWatch {
    pub fn start() -> Self {
        PANICS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic>".to_owned());
            let at = info
                .location()
                .map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
            // The hook replaces the default one, so without this a panic that ends in an abort
            // (a panic in a destructor while unwinding) leaves no trace of what the first one was.
            eprintln!("agent_fuzz: panic: {message}{at}");
            if std::env::var_os("RUST_BACKTRACE").is_some() {
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
            }
            PANICS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(format!("{message}{at}"));
        }));
        Self {
            previous: Some(previous),
        }
    }

    /// The panics recorded since [`Self::start`].
    pub fn panics(&self) -> Vec<String> {
        PANICS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for PanicWatch {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::panic::set_hook(previous);
        }
    }
}
