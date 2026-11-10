//! Frame-latency bookkeeping for the macOS draw path (#1148).
//!
//! The ObjC side (`objc/host_view.m`) timestamps key presses and installs a
//! presented-handler on every drawable; it reports back through
//! [`warp_frame_presented`]. This module decides which frames are slow enough
//! to be worth a WARN and rate limits that WARN so a slow stretch produces one
//! summary line per [`WARN_INTERVAL_MS`] rather than one line per frame.

use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::Mutex;

/// A frame whose key press -> on-screen latency exceeds this is "slow".
pub(super) const SLOW_KEY_TO_PRESENTED_MS: u64 = 50;
/// A frame whose `nextDrawable` wait exceeds this is "slow".
pub(super) const SLOW_DRAWABLE_WAIT_MS: u64 = 8;
/// A `nextDrawable` wait at least this long is logged as `input_order hop=drawable_wait` (#1606).
pub(super) const DRAWABLE_WAIT_LOG_NS: u64 = 2_000_000;
/// Minimum spacing between WARN lines.
pub(super) const WARN_INTERVAL_MS: u64 = 10_000;

/// Whether a frame with these timings should count towards the WARN summary.
pub(super) fn is_slow_frame(key_to_presented_ms: Option<u64>, drawable_wait_ms: u64) -> bool {
    drawable_wait_ms > SLOW_DRAWABLE_WAIT_MS
        || key_to_presented_ms.is_some_and(|ms| ms > SLOW_KEY_TO_PRESENTED_MS)
}

/// Summary of the slow frames seen since the last WARN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlowFrameSummary {
    pub count: u32,
    pub max_key_to_presented_ms: u64,
    pub max_drawable_wait_ms: u64,
}

/// Accumulates slow frames and releases a summary at most once per
/// [`WARN_INTERVAL_MS`]. Time is passed in so the logic is testable.
#[derive(Debug, Default)]
pub(super) struct SlowFrameLimiter {
    last_emit_ms: Option<u64>,
    pending: Option<SlowFrameSummary>,
}

impl SlowFrameLimiter {
    pub(super) const fn new() -> Self {
        Self {
            last_emit_ms: None,
            pending: None,
        }
    }

    /// Records one frame. Returns a summary to log when this frame is slow and
    /// the rate-limit window has elapsed; slow frames inside the window are
    /// folded into the next summary.
    pub(super) fn record(
        &mut self,
        now_ms: u64,
        key_to_presented_ms: Option<u64>,
        drawable_wait_ms: u64,
    ) -> Option<SlowFrameSummary> {
        if !is_slow_frame(key_to_presented_ms, drawable_wait_ms) {
            return None;
        }
        let summary = self.pending.get_or_insert(SlowFrameSummary {
            count: 0,
            max_key_to_presented_ms: 0,
            max_drawable_wait_ms: 0,
        });
        summary.count += 1;
        summary.max_key_to_presented_ms = summary
            .max_key_to_presented_ms
            .max(key_to_presented_ms.unwrap_or(0));
        summary.max_drawable_wait_ms = summary.max_drawable_wait_ms.max(drawable_wait_ms);

        let window_elapsed = self
            .last_emit_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= WARN_INTERVAL_MS);
        if window_elapsed {
            self.last_emit_ms = Some(now_ms);
            self.pending.take()
        } else {
            None
        }
    }
}

/// Per-window "draw owed" bookkeeping (#1358).
///
/// When `warp_update_layer` finds the app mutably borrowed it cannot draw. The draw is not
/// forgotten: the window records that one is owed and a single flush is queued on the
/// foreground executor, which runs from the main queue once the outer borrow has been released
/// (the same release event `dispatch_window_resized` relies on). There is no per-run-loop-turn
/// re-arm and no retry cap: the release of the borrow is the signal.
#[derive(Debug, Default)]
pub(super) struct DrawOwed {
    owed: Cell<bool>,
    flush_queued: Cell<bool>,
}

impl DrawOwed {
    pub(super) const fn new() -> Self {
        Self {
            owed: Cell::new(false),
            flush_queued: Cell::new(false),
        }
    }

    /// Records a dropped draw. Returns true if the caller must queue a flush (none is queued yet);
    /// further drops while a flush is queued only keep the flag set.
    pub(super) fn note_deferred(&self) -> bool {
        self.owed.set(true);
        !self.flush_queued.replace(true)
    }

    /// A draw ran, so nothing is owed any more.
    pub(super) fn note_draw_ran(&self) {
        self.owed.set(false);
    }

    /// Runs when the queued flush executes (borrow released). Returns true if a redraw should be
    /// requested, i.e. a draw is still owed.
    pub(super) fn take_for_flush(&self) -> bool {
        self.flush_queued.set(false);
        self.owed.replace(false)
    }

    #[cfg(test)]
    pub(super) fn is_owed(&self) -> bool {
        self.owed.get()
    }
}

/// How long frames may be skipped in a row before one is drawn anyway (and may block in
/// `nextDrawable`): the net for a presented handler that never fires (#1838).
pub(super) const GATE_WATCHDOG_MS: u64 = 100;

/// Kill switch for the blocked-frame gate (#1838): `PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES=0` restores
/// the old behaviour (the main thread blocks in `nextDrawable`). On by default.
pub(super) fn gate_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES").map_or(true, |value| value != "0")
    })
}

/// What to do with a frame the main thread is about to draw (#1838).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GateDecision {
    /// Ask for the drawable now.
    Draw,
    /// `nextDrawable` would block: leave the main thread free for input and let the presented
    /// handler of the in-flight frame request this draw again.
    Skip,
}

/// The inputs of [`gate_decision`].
#[derive(Debug, Clone, Copy)]
pub(super) struct GateInputs {
    pub enabled: bool,
    /// A frame capture or a presents-with-transaction resize is in progress: always draw.
    pub must_draw: bool,
    /// At least one frame has reached the screen, so a drawable is held by the compositor.
    pub presented_once: bool,
    /// Drawables handed out whose presented handler has not run yet.
    pub unpresented: i32,
    /// How long the current run of skips has lasted, if one is running.
    pub skipping_for_ms: Option<u64>,
}

/// With `maximumDrawableCount = 2` one drawable is on screen and at most one more can be queued,
/// so with a drawable already queued (`unpresented >= 1`) the next `nextDrawable` blocks until
/// that one is presented, which is exactly when its presented handler runs. Skipping until then
/// costs no frame and frees the main thread to dequeue input in the meantime.
pub(super) fn gate_decision(inputs: GateInputs) -> GateDecision {
    let blocked = inputs.presented_once && inputs.unpresented >= 1;
    let watchdog_hit = inputs
        .skipping_for_ms
        .is_some_and(|ms| ms >= GATE_WATCHDOG_MS);
    if inputs.enabled && !inputs.must_draw && blocked && !watchdog_hit {
        GateDecision::Skip
    } else {
        GateDecision::Draw
    }
}

/// One run of consecutive skipped frames on a window (#1838).
#[derive(Debug, Default)]
pub(super) struct SkipRun {
    started_ms: Cell<Option<u64>>,
    skipped: Cell<u32>,
    /// Skipped frames that a key press had invalidated (render-latency step A, measurement only).
    key_skipped: Cell<u32>,
}

impl SkipRun {
    /// Milliseconds the current run has lasted.
    pub(super) fn running_for_ms(&self, now_ms: u64) -> Option<u64> {
        self.started_ms.get().map(|s| now_ms.saturating_sub(s))
    }

    pub(super) fn note_skip(&self, now_ms: u64) {
        if self.started_ms.get().is_none() {
            self.started_ms.set(Some(now_ms));
        }
        self.skipped.set(self.skipped.get().saturating_add(1));
    }

    /// Notes that the frame skipped by the last [`Self::note_skip`] was key-driven.
    pub(super) fn note_key_skip(&self) {
        self.key_skipped
            .set(self.key_skipped.get().saturating_add(1));
    }

    /// Key-driven frames skipped in the run that [`Self::finish`] is about to end. Call before it.
    pub(super) fn take_key_skipped(&self) -> u32 {
        self.key_skipped.take()
    }

    /// Ends the run when a frame is drawn: `(skipped frames, ms since the first skip)`.
    pub(super) fn finish(&self, now_ms: u64) -> Option<(u32, u64)> {
        let started = self.started_ms.take()?;
        Some((self.skipped.take(), now_ms.saturating_sub(started)))
    }
}

pub(super) fn gate_now_ms() -> u64 {
    now_ms()
}

/// Monotonic nanoseconds (non-zero) on this module's clock, for the key-frame schedule stamp.
fn mono_ns() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

/// When `request_redraw` first asked for a key-driven frame that has not started yet (0 = none).
static KEY_FRAME_REQUESTED_NS: AtomicU64 = AtomicU64::new(0);

/// A key-driven frame was requested (`window.rs` `request_redraw` / `render_scene`). Keeps the
/// earliest request: several invalidations before one frame all wait for that one frame.
pub(super) fn note_key_frame_requested() {
    let _ =
        KEY_FRAME_REQUESTED_NS.compare_exchange(0, mono_ns(), Ordering::Relaxed, Ordering::Relaxed);
}

/// The frame is starting (`warp_update_layer`, past the re-entrancy check): logs
/// `hop=key_frame_sched` for a key-driven request that was waiting. Measurement only.
pub(super) fn log_key_frame_sched() {
    let requested = KEY_FRAME_REQUESTED_NS.swap(0, Ordering::Relaxed);
    if requested == 0 {
        return;
    }
    let elapsed_ns = mono_ns().saturating_sub(requested);
    warpui_core::input_order::log_hop("key_frame_sched", || key_frame_sched_detail(elapsed_ns));
}

/// Detail of `hop=key_frame_sched`: request_redraw -> displayLayer entry.
pub(super) fn key_frame_sched_detail(elapsed_ns: u64) -> String {
    format!("ms={:.2}", elapsed_ns as f64 / 1_000_000.0)
}

/// Detail of `hop=key_presented`: the stage after submit that the key frame spent in the
/// compositor, the whole key -> presented time, the layer's `displaySyncEnabled` at submit and the
/// `nextDrawable` wait.
pub(super) fn key_presented_detail(
    submit_to_presented_ns: u64,
    key_to_presented_ns: u64,
    sync: bool,
    drawable_wait_ns: u64,
) -> String {
    format!(
        "submit_to_presented_ms={:.2} key_to_presented_ms={:.2} sync={} wait_ms={:.2}",
        submit_to_presented_ns as f64 / 1_000_000.0,
        key_to_presented_ns as f64 / 1_000_000.0,
        u8::from(sync),
        drawable_wait_ns as f64 / 1_000_000.0
    )
}

/// Kill switch for drawing a key's frame at the end of its key press (#1848):
/// `PHOSPHOR_MAC_KEY_FRAME_NOW=0` restores the old behaviour (every frame waits for AppKit's
/// display pass). On by default.
pub(super) fn key_frame_now_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        key_frame_now_enabled_from_env(std::env::var("PHOSPHOR_MAC_KEY_FRAME_NOW").ok().as_deref())
    })
}

pub(super) fn key_frame_now_enabled_from_env(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

/// A window's redraw requests, numbered, so that the frame a key press draws at once can stand in
/// for the display passes its own requests asked for (#1848).
///
/// Every request (`request_redraw`, `render_scene`, `schedule_redraw`, a frame capture) used to be
/// one `dispatch_async` + `setNeedsDisplay:`, which AppKit serves on its next display pass:
/// measured 0-1 ms for half of the key frames and 6-9 ms (one 120 Hz interval) for the other half
/// (`hop=key_frame_sched`, docs/design/1843-1848-latency.md). A key press now draws its frame
/// when its handling ends, without that wait. The requests it made are still posted, each with its
/// number; when the posted block runs it asks [`Self::already_drawn`], and a request the key frame
/// covered is dropped instead of drawing the same scene a second time.
///
/// Nothing here can lose a frame: a request is only ever dropped when a frame that started after
/// it was handed to the compositor ([`Self::note_frame_submitted`] between
/// [`Self::begin_key_frame`] and [`Self::finish_key_frame`]). A key frame that did not draw (the
/// #1838 gate skipped it, the app was borrowed, no drawable) covers nothing, and the posted
/// requests run as before. Main thread only.
#[derive(Debug, Default)]
pub(super) struct RedrawRequests {
    /// Number of the latest request (0 = none yet).
    latest: Cell<u64>,
    /// Requests numbered up to this are shown by a key frame that was drawn.
    drawn_through: Cell<u64>,
    /// Frames handed to the compositor (a drawable acquired, about to be presented).
    frames_submitted: Cell<u64>,
    /// The key frame being drawn now: (latest request when it began, frames submitted before it).
    key_frame: Cell<Option<(u64, u64)>>,
}

impl RedrawRequests {
    /// A redraw is being requested; returns its number (never 0).
    pub(super) fn note_request(&self) -> u64 {
        let number = self.latest.get() + 1;
        self.latest.set(number);
        number
    }

    /// The latest request's number: taken when a key press begins, for [`Self::begin_key_frame`].
    pub(super) fn latest(&self) -> u64 {
        self.latest.get()
    }

    /// The renderer acquired a drawable and is about to present it.
    pub(super) fn note_frame_submitted(&self) {
        self.frames_submitted
            .set(self.frames_submitted.get().wrapping_add(1));
    }

    /// The key press that began at `at_key_start` has been handled. Returns whether its frame is
    /// drawn now: only when the press requested a redraw of this window, and never from inside
    /// another key frame. On `true` the caller draws and then calls [`Self::finish_key_frame`].
    pub(super) fn begin_key_frame(&self, enabled: bool, at_key_start: u64) -> bool {
        if !enabled || self.key_frame.get().is_some() || self.latest.get() <= at_key_start {
            return false;
        }
        self.key_frame
            .set(Some((self.latest.get(), self.frames_submitted.get())));
        true
    }

    /// The draw started by [`Self::begin_key_frame`] returned. Returns whether a frame was handed
    /// to the compositor; if so the requests made before the draw began are covered by it.
    /// Requests made while it drew (a scene build that invalidates again) are not.
    pub(super) fn finish_key_frame(&self) -> bool {
        let Some((through, submitted_before)) = self.key_frame.take() else {
            return false;
        };
        let drawn = self.frames_submitted.get() != submitted_before;
        if drawn {
            self.drawn_through
                .set(self.drawn_through.get().max(through));
        }
        drawn
    }

    /// Whether the posted request `number` is already on screen through a key frame.
    pub(super) fn already_drawn(&self, number: u64) -> bool {
        number != 0 && number <= self.drawn_through.get()
    }
}

/// Detail of `hop=key_frame_now` (macOS): whether the frame was handed to the compositor and how
/// long the synchronous draw took (scene build + encode).
pub(super) fn key_frame_now_detail(drawn: bool, elapsed_ms: f64) -> String {
    format!("drawn={} ms={elapsed_ms:.2}", u8::from(drawn))
}

static LIMITER: Mutex<SlowFrameLimiter> = Mutex::new(SlowFrameLimiter::new());

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Called from `host_view.m`'s presented handler (any thread) once a frame has
/// reached the screen. `key_to_presented_ns` is 0 when no key press was
/// pending for this frame; `presented` is false if the drawable was dropped.
#[unsafe(no_mangle)]
extern "C-unwind" fn warp_frame_presented(
    key_to_presented_ns: u64,
    drawable_wait_ns: u64,
    presented: bool,
    submit_to_presented_ns: u64,
    sync: bool,
) {
    let key_ms = (presented && key_to_presented_ns > 0).then_some(key_to_presented_ns / 1_000_000);
    let wait_ms = drawable_wait_ns / 1_000_000;
    // #1606: a main thread blocked in `nextDrawable` cannot dequeue key events; a Ctrl-C under an
    // output flood waits behind these. Logged on the input-order target (which the real-app
    // harness captures) only when the wait is long enough to matter for the 2 ms key budgets.
    if drawable_wait_ns >= DRAWABLE_WAIT_LOG_NS {
        warpui_core::input_order::log_hop("drawable_wait", || {
            format!(
                "wait_ms={:.2} key_frame={}",
                drawable_wait_ns as f64 / 1_000_000.0,
                key_to_presented_ns > 0
            )
        });
    }
    // Render-latency step A: where a key frame's time goes after submit (compositor stage).
    if presented && key_to_presented_ns > 0 {
        warpui_core::input_order::log_hop("key_presented", || {
            key_presented_detail(
                submit_to_presented_ns,
                key_to_presented_ns,
                sync,
                drawable_wait_ns,
            )
        });
    }
    // #1838: follow the frames after a ^C so the wait for the prompt's frame is measured.
    warpui_core::input_order::log_ctrl_c_frame_presented(presented);
    log::debug!(
        "frame presented={presented} key_to_presented_ms={key_ms:?} drawable_wait_ms={wait_ms}"
    );
    if let Some(summary) = LIMITER.lock().record(now_ms(), key_ms, wait_ms) {
        log::warn!(
            "Slow frames: {} since the last report (max key->presented {} ms, max nextDrawable wait {} ms)",
            summary.count,
            summary.max_key_to_presented_ms,
            summary.max_drawable_wait_ms
        );
    }
}

#[cfg(test)]
#[path = "frame_latency_tests.rs"]
mod tests;
