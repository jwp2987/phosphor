//! A clock for the compute a background job is actually spending (#1351, #1651).
//!
//! An Enter that waits for a result (the autodetect verdict, a Tab completion) must give up on a
//! job that is *slow*, never on one that merely had to wait for the main thread: a frame, a
//! reflow or a big paste can keep the main thread busy for far longer than the job takes, and the
//! finished job's result then sits in the event queue behind the Enter's own timer. So the budget
//! is measured on this clock, which only runs while a stage of the job runs, and never against
//! the age of the Enter.
//!
//! Two ways of measuring a stage:
//!
//! * **Wall** ([`ComputeClock::default`], fed by [`ComputeClock::begin`]): the wall time from
//!   when the stage began running until it ended. Used by the Tab completer, whose stage may wait
//!   on a completion process, which its cap is meant to count.
//! * **Thread CPU** ([`ComputeClock::thread_cpu`], fed by [`ComputeClock::measure`]): only the
//!   CPU time the stage's polls ran on their thread (#1651), read from the thread's own CPU clock
//!   ([`super::thread_cpu`]). Time the job spends runnable but waiting for a CPU (a loaded
//!   machine), or queued between polls behind other work, does not count. Used by the
//!   input-type detection request, whose stages are CPU-bound (the classification parse runs no
//!   generators). Where the platform has no thread CPU clock a poll is measured on wall time
//!   instead (still only while it runs), and that is logged once.
//!
//! A job that never gets a CPU never reaches its cap on the thread CPU clock: the waiting Enter's
//! wall-clock backstop is the safety net for it, and it only hands the text back to the editor
//! (#1577).
//!
//! The clock also tells the waiting Enter when a stage starts running ([`ComputeClock::notify_on_stage_start`]),
//! so the give-up deadline is armed from that event instead of being polled.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use instant::Instant;
use parking_lot::Mutex;

use super::thread_cpu::{self, ThreadCpuProbe};

/// How a stage's compute is measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Measure {
    /// Wall time while a stage runs.
    #[default]
    Wall,
    /// The CPU time of the stage's polls, on their thread's CPU clock (#1651).
    ThreadCpu,
}

/// A poll of a measured stage in progress on some thread (thread CPU clocks only).
struct PollInProgress {
    id: u64,
    /// Reads the polling thread's CPU clock from another thread, with its reading when the poll
    /// began, for [`ComputeClock::spent`] while the poll runs. `None` where the platform cannot.
    probe: Option<(ThreadCpuProbe, Duration)>,
    /// When the poll began, for a platform without a cross-thread probe.
    wall_start: Instant,
}

#[derive(Default)]
struct ClockState {
    measure: Measure,
    /// Guards currently alive (stages of the job running right now).
    active: u32,
    /// When the first of the active guards began (wall clocks only).
    running_since: Option<Instant>,
    /// Compute of the stages (wall) or polls (thread CPU) that have finished.
    spent: Duration,
    /// Polls running right now (thread CPU clocks only).
    polls: Vec<PollInProgress>,
    next_poll_id: u64,
    /// Told each time a stage begins running.
    stage_started: Option<async_channel::Sender<()>>,
}

/// Shared between the job (any thread) and the waiting Enter (main thread).
#[derive(Clone, Default)]
pub struct ComputeClock(Arc<Mutex<ClockState>>);

impl std::fmt::Debug for ComputeClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let measure = self.0.lock().measure;
        f.debug_struct("ComputeClock")
            .field("measure", &measure)
            .field("spent", &self.spent())
            .field("running", &self.is_running())
            .finish()
    }
}

/// Logs once per process that the thread CPU clock is unavailable, so polls are measured on wall
/// time instead (#1651).
fn log_thread_cpu_unavailable() {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "thread CPU clock unavailable: the Enter's classifier budget is measured on wall time \
             while the classifier runs (#1651)"
        );
    }
}

impl ComputeClock {
    /// A clock that counts only the CPU time its stages' polls run (#1651). Feed it with
    /// [`Self::measure`].
    pub fn thread_cpu() -> Self {
        let clock = Self::default();
        clock.0.lock().measure = Measure::ThreadCpu;
        clock
    }

    /// Starts the clock for the lifetime of the returned guard. Call it as the first thing the
    /// job's future does, so time spent queued behind other work is not counted. On a wall clock
    /// the stage's wall time is counted; on a thread CPU clock only the polls [`Self::measure`]
    /// times are, so a stage there is wrapped in [`Self::measure`] instead.
    pub fn begin(&self) -> ComputeGuard {
        let mut state = self.0.lock();
        state.active += 1;
        if state.active == 1 && state.measure == Measure::Wall {
            state.running_since = Some(Instant::now());
        }
        if let Some(tx) = &state.stage_started {
            // Unbounded: never blocks, and a closed receiver just means nobody waits any more.
            let _ = tx.try_send(());
        }
        ComputeGuard(self.clone())
    }

    /// Runs `future` as one stage of this clock: the stage begins at its first poll (so time
    /// queued before it runs is not counted) and ends when it completes or is dropped. On a
    /// thread CPU clock each poll is timed on the polling thread's own CPU clock, so time between
    /// polls, and time the thread waits for a CPU, is not counted (#1651).
    pub fn measure<F: Future>(&self, future: F) -> Measured<F> {
        Measured {
            clock: self.clone(),
            inner: Box::pin(future),
            stage: None,
        }
    }

    /// Compute spent so far, including the stage (or polls) running now.
    pub fn spent(&self) -> Duration {
        let state = self.0.lock();
        let running = match state.measure {
            Measure::Wall => state
                .running_since
                .map(|since| since.elapsed())
                .unwrap_or_default(),
            // Read under the lock: a poll's end takes it before the poll is unlisted, so the
            // probed thread is still inside the poll (and alive) here.
            Measure::ThreadCpu => state
                .polls
                .iter()
                .map(|poll| {
                    poll.probe
                        .and_then(|(probe, start)| {
                            probe.read().map(|now| now.saturating_sub(start))
                        })
                        .unwrap_or_else(|| poll.wall_start.elapsed())
                })
                .sum::<Duration>(),
        };
        state.spent + running
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

    /// Times one poll of a measured stage, on the polling thread. `None` on a wall clock, whose
    /// stage guard already counts the poll.
    fn begin_poll(&self) -> Option<PollGuard> {
        if self.0.lock().measure != Measure::ThreadCpu {
            return None;
        }
        let cpu_start = thread_cpu::current_thread_cpu();
        if cpu_start.is_none() {
            log_thread_cpu_unavailable();
        }
        let probe =
            ThreadCpuProbe::current().and_then(|probe| probe.read().map(|start| (probe, start)));
        let wall_start = Instant::now();
        let mut state = self.0.lock();
        let id = state.next_poll_id;
        state.next_poll_id = state.next_poll_id.wrapping_add(1);
        state.polls.push(PollInProgress {
            id,
            probe,
            wall_start,
        });
        Some(PollGuard {
            clock: self.clone(),
            id,
            cpu_start,
            wall_start,
        })
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

/// Keeps the clock running; dropping it books the elapsed time as spent (wall clocks).
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

/// One poll of a measured stage on a thread CPU clock. Dropped on the polling thread when the
/// poll returns (or unwinds), it books the poll's CPU time.
struct PollGuard {
    clock: ComputeClock,
    id: u64,
    /// The polling thread's CPU clock when the poll began; `None` without a thread CPU clock,
    /// when the poll's wall time is booked instead.
    cpu_start: Option<Duration>,
    wall_start: Instant,
}

impl Drop for PollGuard {
    fn drop(&mut self) {
        // Sampled on the thread that ran the poll, before the lock is taken.
        let spent = self
            .cpu_start
            .and_then(|start| thread_cpu::current_thread_cpu().map(|end| end.saturating_sub(start)))
            .unwrap_or_else(|| self.wall_start.elapsed());
        let mut state = self.clock.0.lock();
        let id = self.id;
        state.polls.retain(|poll| poll.id != id);
        state.spent += spent;
    }
}

/// A future run as one stage of a [`ComputeClock`]; see [`ComputeClock::measure`].
pub struct Measured<F> {
    clock: ComputeClock,
    inner: Pin<Box<F>>,
    /// The stage, from the first poll until the future completes (or this is dropped).
    stage: Option<ComputeGuard>,
}

impl<F: Future> Future for Measured<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        // `Measured` is `Unpin`: the inner future is pinned in its own box.
        let this = self.get_mut();
        if this.stage.is_none() {
            // The stage starts when the job first runs, not when it was queued (#1351).
            this.stage = Some(this.clock.begin());
        }
        let poll = this.clock.begin_poll();
        let output = this.inner.as_mut().poll(cx);
        // The poll's compute is booked before the stage ends, so a waiter never sees the stage
        // finished without its last poll.
        drop(poll);
        if output.is_ready() {
            this.stage = None;
        }
        output
    }
}

#[cfg(test)]
#[path = "compute_clock_tests.rs"]
mod tests;
