use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

use log::{Level, Log, Metadata, Record};

use super::{report_error, take_once, ReportErrorLogMode};

// A capture logger, modelled on `warp_core/src/errors_tests.rs`, which tests the real
// macro the same way. Throttling is only observable through what actually reaches a
// logger: the per-invocation `static` lives inside the macro expansion, so nothing
// short of driving two distinct call sites and counting the resulting log lines can
// tell a per-callsite flag from a single shared global one.

struct TestLogger;

static LOGGER: TestLogger = TestLogger;
static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

impl Log for TestLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if record.level() == Level::Error {
            logs().lock().unwrap().push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

fn logs() -> &'static Mutex<Vec<String>> {
    LOGS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Counts captured lines equal to `message`. Matching on the exact message keeps this
/// immune to whatever other tests in this binary happen to log in parallel.
fn logged(message: &str) -> usize {
    logs()
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.as_str() == message)
        .count()
}

// Each of these is a SEPARATE `report_error!` invocation, so each expands its own
// `static` flag. That separation is the whole property under test and it cannot be
// reproduced by constructing `AtomicBool`s by hand.
fn report_sanity_callsite() {
    report_error!(anyhow::anyhow!("sanity"));
}

fn report_first_callsite() {
    report_error!(
        anyhow::anyhow!("first callsite"),
        ReportErrorLogMode::OncePerRun
    );
}

fn report_second_callsite() {
    report_error!(
        anyhow::anyhow!("second callsite"),
        ReportErrorLogMode::OncePerRun
    );
}

fn report_every_time_callsite() {
    report_error!(
        anyhow::anyhow!("every time"),
        ReportErrorLogMode::EveryTime
    );
}

// One test, not several: `OncePerRun` flags latch for the lifetime of the process, so
// each call site can only be observed once per binary, and splitting these across
// parallel `#[test]`s would race on the shared capture buffer.
#[test]
fn report_error_throttles_per_callsite_not_globally() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Trace);

    // If another test in this binary won `set_logger` first, nothing below can observe
    // anything and every assertion would trivially read zero -- a passing-looking test
    // that checks nothing. Fail loudly and specifically instead.
    report_sanity_callsite();
    assert_eq!(
        logged("sanity"),
        1,
        "log capture is not working; another test in this binary installed a global \
         logger first, so the assertions below would be vacuous"
    );

    for _ in 0..3 {
        report_first_callsite();
        report_second_callsite();
    }

    // Each call site fires exactly once despite three passes.
    assert_eq!(
        logged("first callsite"),
        1,
        "OncePerRun did not throttle a repeated call site"
    );
    // The load-bearing assertion. The second call site must still fire after the first
    // has already latched. If the macro declared one shared global flag instead of a
    // `static` per invocation, this would be 0 -- the first per-frame error to fire
    // would permanently silence every other call site in the crate.
    assert_eq!(
        logged("second callsite"),
        1,
        "a second call site was silenced by the first one latching; the OncePerRun \
         flag is shared instead of per-invocation"
    );

    // EveryTime must not throttle at all.
    for _ in 0..3 {
        report_every_time_callsite();
    }
    assert_eq!(logged("every time"), 3);
}

#[test]
fn take_once_fires_exactly_once() {
    let flag = AtomicBool::new(false);
    assert!(take_once(&flag), "the first call must be allowed through");
    for _ in 0..100 {
        assert!(!take_once(&flag), "every later call must be suppressed");
    }
}
