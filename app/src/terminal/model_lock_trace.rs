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

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Holds at least this long are logged.
pub const SLOW_HOLD: Duration = Duration::from_millis(2);

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
            if held >= SLOW_HOLD {
                log::debug!("model lock held {held:?} by {}", self.label);
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
    fn timer_reports_elapsed() {
        let t = HoldTimer::start("test");
        std::thread::sleep(Duration::from_millis(3));
        assert!(t.finish() >= Duration::from_millis(3));
    }
}
