//! Hold-time instrumentation and the kill switch for the model-lock reductions (round 18 R1).
//!
//! `TerminalView::render`, `BlockListElement::layout` and `handle_terminal_wakeup` take the
//! terminal model's `FairMutex`; while the UI holds it the pty reader's `try_lock` fails and
//! batches. A [`HoldTimer`] records how long a named scope held the lock and logs at `debug` when
//! the hold reached [`SLOW_HOLD`], so a build can measure before/after (run with
//! `RUST_LOG=warp::terminal::model_lock_trace=debug`) at the cost of two `Instant::now` calls.
//!
//! `PHOSPHOR_RENDER_LOCK_LEGACY=1` restores the previous lock scoping everywhere this change
//! touched (see [`legacy_lock_scoping`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Holds at least this long are logged.
pub const SLOW_HOLD: Duration = Duration::from_millis(2);

/// Holds longer than this are logged at `warn` (rate limited) so they show in ordinary logs, not
/// only when `model_lock_trace=debug` is switched on (#1377).
pub const WARN_HOLD: Duration = Duration::from_millis(8);

/// Minimum spacing between warn lines; holds in between are counted and reported on the next.
const WARN_INTERVAL_MS: u64 = 1000;

/// Which level a hold of `held` is logged at (`None` = not logged).
pub fn level_for_hold(held: Duration) -> Option<log::Level> {
    if held > WARN_HOLD {
        Some(log::Level::Warn)
    } else if held >= SLOW_HOLD {
        Some(log::Level::Debug)
    } else {
        None
    }
}

/// Rate limiter state: `last_ms` is the time of the last warn (+1 so 0 = never).
pub struct WarnLimiter {
    last_ms: AtomicU64,
    suppressed: AtomicU64,
}

impl WarnLimiter {
    pub const fn new() -> Self {
        Self {
            last_ms: AtomicU64::new(0),
            suppressed: AtomicU64::new(0),
        }
    }

    /// `now_ms` is a monotonic millisecond clock. Returns `Some(suppressed_since_last)` when a
    /// warn should be emitted now, `None` when it is rate limited.
    pub fn admit(&self, now_ms: u64) -> Option<u64> {
        let now = now_ms + 1;
        let last = self.last_ms.load(Ordering::Relaxed);
        if last != 0 && now.saturating_sub(last) < WARN_INTERVAL_MS {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        if self
            .last_ms
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        Some(self.suppressed.swap(0, Ordering::Relaxed))
    }
}

static WARN_LIMITER: WarnLimiter = WarnLimiter::new();

fn process_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// True when `PHOSPHOR_RENDER_LOCK_LEGACY` is set to anything but `0`/empty. Read once.
pub fn legacy_lock_scoping() -> bool {
    static LEGACY: OnceLock<bool> = OnceLock::new();
    *LEGACY.get_or_init(|| legacy_from_value(std::env::var("PHOSPHOR_RENDER_LOCK_LEGACY").ok()))
}

fn legacy_from_value(value: Option<String>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

/// Measures one lock-holding scope. Dropping (or [`HoldTimer::finish`]) logs a slow hold.
pub struct HoldTimer {
    label: &'static str,
    start: Instant,
    done: bool,
}

impl HoldTimer {
    /// Start timing right after the lock was acquired.
    pub fn start(label: &'static str) -> Self {
        Self {
            label,
            start: Instant::now(),
            done: false,
        }
    }

    /// Call right after the guard is released; also runs on drop.
    pub fn finish(mut self) -> Duration {
        self.report()
    }

    fn report(&mut self) -> Duration {
        let held = self.start.elapsed();
        if !self.done {
            self.done = true;
            match level_for_hold(held) {
                Some(log::Level::Warn) => {
                    if let Some(suppressed) = WARN_LIMITER.admit(process_ms()) {
                        log::warn!(
                            "model lock held {held:?} by {} (> {WARN_HOLD:?}; {suppressed} similar holds suppressed)",
                            self.label
                        );
                    } else {
                        log::debug!("model lock held {held:?} by {}", self.label);
                    }
                }
                Some(_) => log::debug!("model lock held {held:?} by {}", self.label),
                None => {}
            }
        }
        held
    }
}

impl Drop for HoldTimer {
    fn drop(&mut self) {
        self.report();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_switch_values() {
        assert!(!legacy_from_value(None));
        assert!(!legacy_from_value(Some(String::new())));
        assert!(!legacy_from_value(Some("0".into())));
        assert!(legacy_from_value(Some("1".into())));
    }

    #[test]
    fn long_holds_log_at_warn_short_at_debug() {
        assert_eq!(level_for_hold(Duration::from_millis(1)), None);
        assert_eq!(
            level_for_hold(Duration::from_millis(2)),
            Some(log::Level::Debug)
        );
        assert_eq!(
            level_for_hold(Duration::from_millis(8)),
            Some(log::Level::Debug)
        );
        assert_eq!(
            level_for_hold(Duration::from_millis(9)),
            Some(log::Level::Warn)
        );
    }

    #[test]
    fn warn_is_rate_limited_and_counts_suppressed() {
        let l = WarnLimiter::new();
        assert_eq!(l.admit(0), Some(0));
        assert_eq!(l.admit(10), None);
        assert_eq!(l.admit(500), None);
        assert_eq!(l.admit(1001), Some(2));
    }

    #[test]
    fn timer_reports_elapsed() {
        let t = HoldTimer::start("test");
        std::thread::sleep(Duration::from_millis(3));
        assert!(t.finish() >= Duration::from_millis(3));
    }
}
