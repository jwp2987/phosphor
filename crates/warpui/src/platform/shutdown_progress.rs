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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long a quit that no signal started (a quit dialog's "Yes, quit", a menu or key quit, the last
/// window closing) may run before the process gives up and exits. It is a give-up for a step that
/// never finishes (#1410: a quit with 24 busy tabs once left the app alive), not a pace: every
/// step still runs to completion in order, and a healthy quit is over in well under a second. It
/// is longer than the 5 s signal deadline because nothing outside is waiting on this quit, and
/// the persistence writer's drain is worth waiting for.
pub const QUIT_GIVE_UP: Duration = Duration::from_secs(15);

/// Set by the first [`arm_quit_give_up`]: the give-up counts from the earliest point a quit was
/// decided, and later arms on the same quit do not start more threads.
static QUIT_GIVE_UP_ARMED: AtomicBool = AtomicBool::new(false);

/// Claims the one quit give-up for this process. True only for the first caller.
fn claim_quit_give_up(armed: &AtomicBool) -> bool {
    !armed.swap(true, Ordering::AcqRel)
}

/// Arms the quit give-up: if the process is still alive [`QUIT_GIVE_UP`] after the first call,
/// logs the step that was running (see [`current_step`]) and ends the process. The thread dies
/// with the process on a normal exit.
///
/// Called at every point a quit is decided, not only once `on_will_terminate` runs (#1410): the
/// confirmed quit dialog, the event loop approving a quit, and `on_will_terminate`. A quit that
/// hangs before `on_will_terminate` (the terminate request never reaching the loop, the loop never
/// reaching its exit) is bounded too, and the step it names says where it stopped. Only the first
/// call starts the thread; later ones are no-ops.
pub fn arm_quit_give_up() {
    use super::termination_signals::{hard_exit, spawn_deadline_watchdog};
    if !claim_quit_give_up(&QUIT_GIVE_UP_ARMED) {
        return;
    }
    if let Err(error) = spawn_deadline_watchdog(QUIT_GIVE_UP, 0, hard_exit) {
        log::warn!("Could not start the quit give-up thread: {error}");
    }
}

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
    fn quit_give_up_is_longer_than_the_signal_deadline() {
        assert!(QUIT_GIVE_UP > crate::platform::termination_signals::SHUTDOWN_DEADLINE);
    }

    #[test]
    fn only_the_first_arm_claims_the_quit_give_up() {
        // #1410: the give-up is armed at several points of one quit; it must start one thread,
        // counting from the first.
        let armed = AtomicBool::new(false);
        assert!(claim_quit_give_up(&armed));
        assert!(!claim_quit_give_up(&armed));
        assert!(!claim_quit_give_up(&armed));
    }

    #[test]
    fn current_step_reports_the_last_step_set() {
        set_step("terminate_persistence_writer");
        assert_eq!(current_step(), "terminate_persistence_writer");
        set_step("tear_down_diagnostics");
        assert_eq!(current_step(), "tear_down_diagnostics");
    }
}
