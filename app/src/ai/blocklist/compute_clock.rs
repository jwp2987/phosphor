//! A clock for the compute a background job is actually spending (#1351).
//!
//! An Enter that waits for a result (the autodetect verdict, a Tab completion) must give up on a
//! job that is *slow*, never on one that merely had to wait for the main thread: a frame, a
//! reflow or a big paste can keep the main thread busy for far longer than the job takes, and the
//! finished job's result then sits in the event queue behind the Enter's own timer. So the budget
//! is measured on this clock, which only runs while a [`ComputeGuard`] is alive on the job's
//! thread, and never against the age of the Enter.
//!
//! The clock also tells the waiting Enter when a stage starts running ([`ComputeClock::notify_on_stage_start`]),
//! so the give-up deadline is armed from that event instead of being polled.

use std::sync::Arc;
use std::time::Duration;

use instant::Instant;
use parking_lot::Mutex;

#[derive(Default)]
struct ClockState {
    /// Guards currently alive (stages of the job running right now).
    active: u32,
    /// When the first of the active guards began.
    running_since: Option<Instant>,
    /// Compute of the stages that have finished.
    spent: Duration,
    /// Told each time a stage begins running.
    stage_started: Option<async_channel::Sender<()>>,
}

/// Shared between the job (any thread) and the waiting Enter (main thread).
#[derive(Clone, Default)]
pub struct ComputeClock(Arc<Mutex<ClockState>>);

impl std::fmt::Debug for ComputeClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComputeClock")
            .field("spent", &self.spent())
            .field("running", &self.is_running())
            .finish()
    }
}

impl ComputeClock {
    /// Starts the clock for the lifetime of the returned guard. Call it as the first thing the
    /// job's future does, so time spent queued behind other work is not counted.
    pub fn begin(&self) -> ComputeGuard {
        let mut state = self.0.lock();
        state.active += 1;
        if state.active == 1 {
            state.running_since = Some(Instant::now());
        }
        if let Some(tx) = &state.stage_started {
            // Unbounded: never blocks, and a closed receiver just means nobody waits any more.
            let _ = tx.try_send(());
        }
        ComputeGuard(self.clone())
    }

    /// Compute spent so far, including the stage running now.
    pub fn spent(&self) -> Duration {
        let state = self.0.lock();
        state.spent
            + state
                .running_since
                .map(|since| since.elapsed())
                .unwrap_or_default()
    }

    /// Whether a stage is running right now.
    pub fn is_running(&self) -> bool {
        self.0.lock().active > 0
    }

    /// Sends a unit on `tx` each time a stage begins running, replacing any earlier receiver.
    pub fn notify_on_stage_start(&self, tx: async_channel::Sender<()>) {
        self.0.lock().stage_started = Some(tx);
    }

    /// Stops the stage-start notifications (closes the receiver's stream).
    pub fn stop_notifying(&self) {
        self.0.lock().stage_started = None;
    }

    /// A clock that already spent `spent` and is idle, for tests.
    #[cfg(test)]
    pub fn with_spent_for_test(spent: Duration) -> Self {
        let clock = Self::default();
        clock.0.lock().spent = spent;
        clock
    }

    /// A clock whose current stage began `running_for` ago, for tests.
    #[cfg(test)]
    pub fn running_for_test(running_for: Duration) -> (Self, ComputeGuard) {
        let clock = Self::default();
        let guard = clock.begin();
        clock.0.lock().running_since = Some(Instant::now() - running_for);
        (clock, guard)
    }
}

/// Keeps the clock running; dropping it books the elapsed time as spent.
pub struct ComputeGuard(ComputeClock);

impl Drop for ComputeGuard {
    fn drop(&mut self) {
        let mut state = (self.0).0.lock();
        state.active = state.active.saturating_sub(1);
        if state.active == 0
            && let Some(since) = state.running_since.take()
        {
            state.spent += since.elapsed();
        }
    }
}

#[cfg(test)]
#[path = "compute_clock_tests.rs"]
mod tests;
