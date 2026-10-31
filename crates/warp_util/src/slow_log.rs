//! Threshold-based, rate-limited "slow operation" logging (#1146).
//!
//! Hot paths time themselves with a bare `Instant::now()` and call
//! [`SlowLog::finish`] when done. The fast path is one clock read and a
//! `Duration` compare; nothing is formatted or locked unless the threshold is
//! exceeded. When it is, at most one line per `window` is emitted; the others
//! are folded into a summary on the next emitted line, so a sustained problem
//! logs a count and the worst duration instead of flooding the log.
//!
//! Callers must never put secrets in the context string. Use [`truncate_for_log`]
//! for anything that can hold a command line.

use std::time::Duration;

use instant::Instant;
use parking_lot::Mutex;

/// Default rate-limit window shared by the call sites.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(10);

struct WindowState {
    /// Time of the last emitted line; `None` until the first emit.
    emitted_at: Option<Instant>,
    suppressed: u32,
    worst: Duration,
}

/// What [`SlowLog::check`] decided for one observation.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Log it. `suppressed`/`worst` summarize the over-threshold observations
    /// swallowed since the previous emitted line (0 when none).
    Emit { suppressed: u32, worst: Duration },
    /// Under threshold, or rate-limited and folded into the next emitted line.
    Suppress,
}

pub struct SlowLog {
    what: &'static str,
    threshold: Duration,
    window: Duration,
    state: Mutex<WindowState>,
}

impl SlowLog {
    pub const fn new(what: &'static str, threshold: Duration, window: Duration) -> Self {
        Self {
            what,
            threshold,
            window,
            state: Mutex::new(WindowState {
                emitted_at: None,
                suppressed: 0,
                worst: Duration::ZERO,
            }),
        }
    }

    pub const fn threshold(&self) -> Duration {
        self.threshold
    }

    /// Pure rate-limit decision for an observation that took `elapsed` at `now`.
    /// Anything under the threshold returns `Suppress` without touching state.
    pub fn check(&self, elapsed: Duration, now: Instant) -> Verdict {
        if elapsed < self.threshold {
            return Verdict::Suppress;
        }
        let mut state = self.state.lock();
        let due = match state.emitted_at {
            None => true,
            Some(at) => now.saturating_duration_since(at) >= self.window,
        };
        if due {
            let verdict = Verdict::Emit {
                suppressed: state.suppressed,
                worst: state.worst.max(elapsed),
            };
            state.emitted_at = Some(now);
            state.suppressed = 0;
            state.worst = Duration::ZERO;
            verdict
        } else {
            state.suppressed = state.suppressed.saturating_add(1);
            state.worst = state.worst.max(elapsed);
            Verdict::Suppress
        }
    }

    /// Call when the timed operation completes. `context` is only evaluated if a
    /// line is actually emitted. Logs at WARN.
    pub fn finish(&self, start: Instant, context: impl FnOnce() -> String) {
        self.finish_at(start, log::Level::Warn, context);
    }

    pub fn finish_at(&self, start: Instant, level: log::Level, context: impl FnOnce() -> String) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(start);
        if elapsed < self.threshold {
            return;
        }
        self.report(elapsed, now, level, context);
    }

    /// Like [`finish_at`](Self::finish_at) for callers that already measured `elapsed`.
    pub fn report(
        &self,
        elapsed: Duration,
        now: Instant,
        level: log::Level,
        context: impl FnOnce() -> String,
    ) {
        if let Verdict::Emit { suppressed, worst } = self.check(elapsed, now) {
            let ctx = context();
            if suppressed == 0 {
                log::log!(
                    level,
                    "slow {}: {} ms ({ctx})",
                    self.what,
                    elapsed.as_millis()
                );
            } else {
                log::log!(
                    level,
                    "slow {}: {} ms ({ctx}); {suppressed} more over {} ms since last report, worst {} ms",
                    self.what,
                    elapsed.as_millis(),
                    self.threshold.as_millis(),
                    worst.as_millis(),
                );
            }
        }
    }
}

/// Truncate `text` to its first line, at most `max` chars, with an ellipsis if
/// anything was cut, so a command can be identified in a log without dumping it.
pub fn truncate_for_log(text: &str, max: usize) -> String {
    let first_line = text.lines().next().unwrap_or("");
    let mut out: String = first_line.chars().take(max).collect();
    if text.contains('\n') || first_line.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
#[path = "slow_log_tests.rs"]
mod tests;
