//! Which step of the app's quit sequence is running right now.
//!
//! The quit path (`on_will_terminate`) is ordered, not timed: each step finishes before the
//! next begins, and the persistence-writer step waits for the writer thread's own
//! flush-complete (its join), however long that takes. The only timer around it is the
//! signal / session-end watchdog's give-up deadline, which exists for a step that never
//! finishes. When it fires it must say *which* step was still running -- otherwise a
//! truncated SQLite flush looks like any other slow quit in the log.
//!
//! The quit path calls [`set_step`]; the watchdog reads [`current_step`].

use std::sync::Mutex;

static CURRENT_STEP: Mutex<&'static str> = Mutex::new("not started");

/// Records the quit step that is about to run.
pub fn set_step(step: &'static str) {
    *CURRENT_STEP
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = step;
}

/// The quit step most recently started (and so, at a watchdog expiry, still running).
pub fn current_step() -> &'static str {
    *CURRENT_STEP
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_step_reports_the_last_step_set() {
        set_step("terminate_persistence_writer");
        assert_eq!(current_step(), "terminate_persistence_writer");
        set_step("tear_down_diagnostics");
        assert_eq!(current_step(), "tear_down_diagnostics");
    }
}
