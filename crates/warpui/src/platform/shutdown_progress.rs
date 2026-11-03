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

/// Set by the first [`arm_quit_give_up_before_will_terminate`]: one pre-terminate give-up per
/// process, counting from the earliest point a quit was decided.
static PRE_TERMINATE_ARMED: AtomicBool = AtomicBool::new(false);
/// Set by the first [`arm_quit_give_up`] (`on_will_terminate`), which has its own full budget.
static WILL_TERMINATE_ARMED: AtomicBool = AtomicBool::new(false);

/// Claims one give-up slot. True only for the first caller.
fn claim_quit_give_up(armed: &AtomicBool) -> bool {
    !armed.swap(true, Ordering::AcqRel)
}

/// Arms the give-up for the quit steps (`on_will_terminate`): if the process is still alive
/// [`QUIT_GIVE_UP`] after the first call, logs the step that was running (see [`current_step`])
/// and ends the process. It gets the whole budget for the steps (the persistence writer's drain
/// is worth waiting for), whatever happened before. The thread dies with the process on a
/// normal exit. Only the first call starts it; a failed spawn lets a later call retry.
pub fn arm_quit_give_up() {
    use super::termination_signals::{hard_exit, spawn_deadline_watchdog};
    if !claim_quit_give_up(&WILL_TERMINATE_ARMED) {
        return;
    }
    if let Err(error) = spawn_deadline_watchdog(QUIT_GIVE_UP, 0, hard_exit) {
        WILL_TERMINATE_ARMED.store(false, Ordering::Release);
        log::warn!("Could not start the quit give-up thread: {error}");
    }
}

/// Arms a give-up for a quit that was decided (the quit dialog confirmed, the event loop approved
/// it) but may stall before `on_will_terminate` runs (#1410): the terminate request never reaching
/// the loop, the loop never reaching its exit. After [`QUIT_GIVE_UP`] it ends the process, naming
/// the step, unless `on_will_terminate` has armed its own give-up meanwhile, in which case it
/// stands down so the quit steps keep their full budget. Only the first call starts the thread;
/// a failed spawn lets a later call retry.
pub fn arm_quit_give_up_before_will_terminate() {
    use super::termination_signals::hard_exit;
    if !claim_quit_give_up(&PRE_TERMINATE_ARMED) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("quit-give-up-pre".to_string())
        .spawn(|| {
            run_pre_terminate_give_up(&WILL_TERMINATE_ARMED, std::thread::sleep, hard_exit);
        });
    if let Err(error) = spawned {
        PRE_TERMINATE_ARMED.store(false, Ordering::Release);
        log::warn!("Could not start the pre-terminate quit give-up thread: {error}");
    }
}

/// The pre-terminate give-up's body: waits [`QUIT_GIVE_UP`], then stands down if
/// `on_will_terminate` armed its own give-up (`will_terminate_armed`), else ends the process with
/// the step logged. Tests inject `sleep` and `exit`.
fn run_pre_terminate_give_up(
    will_terminate_armed: &AtomicBool,
    sleep: impl FnOnce(Duration),
    exit: impl FnOnce(i32),
) {
    sleep(QUIT_GIVE_UP);
    if will_terminate_armed.load(Ordering::Acquire) {
        return;
    }
    super::termination_signals::run_deadline_watchdog(QUIT_GIVE_UP, 0, |_| {}, exit);
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
    fn the_pre_terminate_give_up_stands_down_once_will_terminate_armed() {
        // #1410: the quit steps keep their own full budget; the earlier give-up does not cut it.
        let armed = AtomicBool::new(true);
        let mut exited = None;
        run_pre_terminate_give_up(&armed, |_| {}, |code| exited = Some(code));
        assert_eq!(exited, None);
    }

    #[test]
    fn the_pre_terminate_give_up_ends_a_quit_that_never_reached_will_terminate() {
        let armed = AtomicBool::new(false);
        let mut slept = None;
        let mut exited = None;
        run_pre_terminate_give_up(&armed, |d| slept = Some(d), |code| exited = Some(code));
        assert_eq!(slept, Some(QUIT_GIVE_UP));
        assert_eq!(exited, Some(0));
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
