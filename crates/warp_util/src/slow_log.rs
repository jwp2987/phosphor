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

/// Calling thread's CPU time and fault counters, sampled around an operation so a multi-second
/// stall can be told apart: burning CPU (a real algorithmic cost) versus waiting (a lock, I/O,
/// page faults, or the whole host starving the process) (#1293). Zeroed where unsupported.
#[derive(Clone, Copy, Default)]
pub struct ThreadUsage {
    cpu: Duration,
    major_faults: i64,
    involuntary_switches: i64,
}

impl ThreadUsage {
    pub fn now() -> Self {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let mut ts = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            // SAFETY: `ts` is a valid out-pointer for the duration of the call.
            let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } == 0;
            let cpu = if ok {
                Duration::new(
                    ts.tv_sec.max(0) as u64,
                    ts.tv_nsec.clamp(0, 999_999_999) as u32,
                )
            } else {
                Duration::ZERO
            };
            let mut major_faults = 0;
            let mut involuntary_switches = 0;
            #[cfg(target_os = "linux")]
            {
                // SAFETY: zeroed `rusage` is a valid value and is a valid out-pointer.
                let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
                if unsafe { libc::getrusage(libc::RUSAGE_THREAD, &mut ru) } == 0 {
                    major_faults = ru.ru_majflt as i64;
                    involuntary_switches = ru.ru_nivcsw as i64;
                }
            }
            Self {
                cpu,
                major_faults,
                involuntary_switches,
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Self::default()
        }
    }

    /// One-line summary of what this thread did since `start`, for a stall of `wall`.
    pub fn describe_since(&self, start: &ThreadUsage, wall: Duration) -> String {
        let cpu = self.cpu.saturating_sub(start.cpu);
        format!(
            "thread cpu {} ms of {} ms wall ({}), major faults {}, involuntary switches {}",
            cpu.as_millis(),
            wall.as_millis(),
            classify_stall(wall, cpu),
            self.major_faults - start.major_faults,
            self.involuntary_switches - start.involuntary_switches,
        )
    }
}

/// Whether a stall of `wall` that used `cpu` on the thread was computing or waiting.
pub fn classify_stall(wall: Duration, cpu: Duration) -> &'static str {
    if cpu.saturating_mul(2) >= wall {
        "cpu-bound"
    } else {
        "waiting: blocked or descheduled"
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
