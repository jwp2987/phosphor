//! Policy for frames that failed to draw: which failures are transient (skip the frame and
//! try again shortly), which should rebuild the renderer, and which count toward the
//! crash-recovery exit (`app/src/crash_recovery.rs`: three consecutive counted errors call
//! `process::exit(1)`).
//!
//! A skipped acquire (`Timeout` / `Occluded`) is not a render fault: the compositor was busy,
//! or the window is hidden. Counting those toward the exit turned a 3 s stall into a process
//! exit, and not re-arming a redraw left the window showing a stale frame until the next
//! unrelated event.

use instant::{Duration, Instant};

use crate::rendering::wgpu::renderer::{Error, GetSurfaceTextureError};

/// First retry delay after a skipped frame. Each further consecutive skip doubles it.
const BASE_RETRY_DELAY: Duration = Duration::from_millis(16);
/// Upper bound on the retry delay, so a window that stays occluded wakes at most ~4 times a
/// second.
const MAX_RETRY_DELAY: Duration = Duration::from_millis(250);
/// `Lost` / `Outdated` mean the swapchain needs reconfiguring, which has already been tried
/// once inside `Resources::get_surface_texture`. If more than this many frames in a row still
/// fail that way, something is really wrong, so start counting toward the crash-recovery exit.
const MAX_CONSECUTIVE_RECONFIGURE_FAILURES: u32 = 5;

/// What `redraw_window` should do about a failed frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrameErrorDecision {
    /// Report the failure to the crash-recovery counter.
    pub counts_toward_exit: bool,
    /// Drop and recreate the renderer.
    pub recreate_renderer: bool,
    /// Request another redraw after this delay. `None` waits for the next event, as before.
    pub retry_after: Option<Duration>,
}

#[derive(Default)]
pub(super) struct DrawRetryState {
    /// Consecutive frames skipped for a transient reason (any kind).
    consecutive_skips: u32,
    /// Consecutive frames that failed with `Lost` / `Outdated`.
    consecutive_reconfigure_failures: u32,
}

impl DrawRetryState {
    /// A frame was drawn: forget the failure history.
    pub fn on_frame_drawn(&mut self) {
        self.consecutive_skips = 0;
        self.consecutive_reconfigure_failures = 0;
    }

    pub fn on_frame_error(&mut self, error: &Error) -> FrameErrorDecision {
        match error {
            // Not render faults: skip the frame, never count, try again soon.
            Error::SurfaceError(
                GetSurfaceTextureError::Timeout | GetSurfaceTextureError::Occluded,
            ) => {
                self.consecutive_reconfigure_failures = 0;
                self.skip(false, false)
            }
            // The swapchain was already reconfigured once for this frame and the retry failed
            // too. Rebuild the renderer for `Lost`, and keep trying; only a persistent
            // failure counts toward the exit.
            Error::SurfaceError(
                error @ (GetSurfaceTextureError::Lost | GetSurfaceTextureError::Outdated),
            ) => {
                self.consecutive_reconfigure_failures += 1;
                let counts =
                    self.consecutive_reconfigure_failures > MAX_CONSECUTIVE_RECONFIGURE_FAILURES;
                self.skip(counts, matches!(error, GetSurfaceTextureError::Lost))
            }
            // Fatal for this renderer: rebuild it and count, as before.
            Error::SurfaceConfigureError { .. } | Error::DeviceLost | Error::Unknown(_) => {
                self.consecutive_skips = 0;
                FrameErrorDecision {
                    counts_toward_exit: true,
                    recreate_renderer: true,
                    retry_after: None,
                }
            }
            // Validation / configuration errors: counted, nothing to retry blindly.
            Error::SurfaceError(_) => {
                self.consecutive_skips = 0;
                FrameErrorDecision {
                    counts_toward_exit: true,
                    recreate_renderer: false,
                    retry_after: None,
                }
            }
        }
    }

    fn skip(&mut self, counts_toward_exit: bool, recreate_renderer: bool) -> FrameErrorDecision {
        self.consecutive_skips = self.consecutive_skips.saturating_add(1);
        FrameErrorDecision {
            counts_toward_exit,
            recreate_renderer,
            retry_after: Some(retry_delay(self.consecutive_skips)),
        }
    }
}

/// Delay before the `n`th consecutive retry (`n >= 1`): 16, 32, 64, 128, then 250 ms.
fn retry_delay(consecutive_skips: u32) -> Duration {
    let shift = consecutive_skips.saturating_sub(1).min(8);
    (BASE_RETRY_DELAY * (1u32 << shift)).min(MAX_RETRY_DELAY)
}

/// Skips redraws of a minimized window and remembers that it did, so the window is redrawn the
/// first time anything happens to it after it comes back.
///
/// A minimized window cannot show a frame, yet every repaint timer (clock chip, spinners, output
/// of a running command) would still build a scene, ask the compositor for a texture, fail, and
/// schedule a retry. Skipping the redraw costs nothing and loses nothing: there is no stale frame
/// to see, and the redraw is requested again on the next event once the window is not minimized.
#[derive(Default)]
pub(super) struct MinimizedRedrawGate {
    skipped: bool,
}

impl MinimizedRedrawGate {
    /// A redraw was requested. Returns `true` when it should be skipped.
    pub fn should_skip_redraw(&mut self, minimized: bool) -> bool {
        if minimized {
            self.skipped = true;
        }
        minimized
    }

    /// Some other event reached the window. Returns `true` when a redraw skipped earlier is now
    /// due because the window is no longer minimized.
    pub fn redraw_due_on_event(&mut self, minimized: bool) -> bool {
        if self.skipped && !minimized {
            self.skipped = false;
            return true;
        }
        false
    }
}

/// A frame whose redraw (scene build, acquire, encode, present) took longer than this is slow.
const SLOW_FRAME: Duration = Duration::from_millis(50);
/// Waiting longer than this for the swapchain to hand out an image delays every key behind it.
const SLOW_ACQUIRE: Duration = Duration::from_millis(8);

/// Whether a frame deserves a slow-frame diagnostic.
pub(super) fn is_slow_frame(total: Duration, acquire: Option<Duration>) -> bool {
    total > SLOW_FRAME || acquire.is_some_and(|acquire| acquire > SLOW_ACQUIRE)
}

/// Collapses a stream of events into at most one summary per `interval`, carrying the number of
/// events and the largest value seen since the previous summary. The first event reports
/// immediately.
pub(super) struct RateLimitedSummary {
    interval: Duration,
    last_emitted: Option<Instant>,
    count: u32,
    max: f64,
}

/// What a [`RateLimitedSummary`] reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Summary {
    pub count: u32,
    pub max: f64,
}

impl RateLimitedSummary {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_emitted: None,
            count: 0,
            max: 0.0,
        }
    }

    /// Records one event with `value` and returns a summary if one is due.
    pub fn record(&mut self, now: Instant, value: f64) -> Option<Summary> {
        self.count = self.count.saturating_add(1);
        self.max = self.max.max(value);

        let due = self
            .last_emitted
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if !due {
            return None;
        }

        let summary = Summary {
            count: self.count,
            max: self.max,
        };
        self.last_emitted = Some(now);
        self.count = 0;
        self.max = 0.0;
        Some(summary)
    }
}

/// Per-window "draw owed" bookkeeping (UI-R8): `redraw_window` found the app mutably borrowed
/// and could not draw. The draw is owed, not dropped; one flush is queued, and it runs when the
/// borrow has been released. No timer and no per-turn re-arm: the release is the signal.
#[derive(Debug, Default)]
pub(super) struct OwedRedraw {
    owed: bool,
    flush_queued: bool,
}

impl OwedRedraw {
    /// Records a draw that could not run. Returns true if the caller must queue a flush.
    pub fn note_deferred(&mut self) -> bool {
        self.owed = true;
        !std::mem::replace(&mut self.flush_queued, true)
    }

    /// A frame was drawn, so nothing is owed any more.
    pub fn note_draw_ran(&mut self) {
        self.owed = false;
    }

    /// The queued flush ran. `can_draw` is whether the app can be borrowed now. Returns true if
    /// a redraw should be requested. If the borrow is still held, the debt stays and the next
    /// event (see [`Self::pending`]) retries.
    pub fn take_for_flush(&mut self, can_draw: bool) -> bool {
        self.flush_queued = false;
        if can_draw {
            std::mem::take(&mut self.owed)
        } else {
            false
        }
    }

    /// A draw is owed and no flush is queued to pay it.
    pub fn pending(&self) -> bool {
        self.owed && !self.flush_queued
    }
}

/// The variable that arms [`TestDeviceLoss`].
pub(super) const TEST_DEVICE_LOSS_ENV: &str = "PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE";
/// The file name the trigger path must end in. The hook DELETES the file it is given, so a
/// variable that names anything else is ignored.
pub(super) const TEST_DEVICE_LOSS_FILE_NAME: &str = "phosphor-test-lose-gpu-device";

/// Test hook (#2091): lets a real-app test take the GPU device away without root.
///
/// Armed only when `PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE` names an absolute path whose file name
/// is `phosphor-test-lose-gpu-device`; no normal run sets it, and then a redraw pays one
/// `Option` test. When armed, each redraw tries to remove that file. The redraw that removes it
/// reports [`Error::DeviceLost`] instead of drawing, exactly once per file: creating the file is
/// the request, removing it is taking the request. From there the app's own lost-device path
/// runs unchanged (the decision of [`DrawRetryState`], the renderer dropped and rebuilt).
///
/// The loss is reported, not produced: `wgpu::Device::destroy` would make later calls on the
/// device raise validation errors instead of the lost-device error, which is another path.
#[derive(Debug, Default)]
pub(super) struct TestDeviceLoss {
    trigger: Option<std::path::PathBuf>,
}

impl TestDeviceLoss {
    pub fn from_env() -> Self {
        Self::from_path(std::env::var_os(TEST_DEVICE_LOSS_ENV).map(std::path::PathBuf::from))
    }

    pub fn from_path(path: Option<std::path::PathBuf>) -> Self {
        let trigger = path.filter(|path| {
            path.is_absolute()
                && path
                    .file_name()
                    .is_some_and(|name| name == TEST_DEVICE_LOSS_FILE_NAME)
        });
        if let Some(path) = &trigger {
            log::warn!(
                "TEST HOOK armed ({TEST_DEVICE_LOSS_ENV}): a redraw that finds {} reports the \
                 GPU device as lost",
                path.display()
            );
        }
        Self { trigger }
    }

    #[cfg(test)]
    pub fn armed(&self) -> bool {
        self.trigger.is_some()
    }

    /// Whether this redraw must report a lost device. True once per trigger file.
    pub fn take(&self) -> bool {
        match &self.trigger {
            None => false,
            Some(path) => std::fs::remove_file(path).is_ok(),
        }
    }
}

#[cfg(test)]
#[path = "draw_retry_tests.rs"]
mod tests;
