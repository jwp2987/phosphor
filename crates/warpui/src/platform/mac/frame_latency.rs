//! Frame-latency bookkeeping for the macOS draw path (#1148).
//!
//! The ObjC side (`objc/host_view.m`) timestamps key presses and installs a
//! presented-handler on every drawable; it reports back through
//! [`warp_frame_presented`]. This module decides which frames are slow enough
//! to be worth a WARN and rate limits that WARN so a slow stretch produces one
//! summary line per [`WARN_INTERVAL_MS`] rather than one line per frame.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use parking_lot::Mutex;

/// A frame whose key press -> on-screen latency exceeds this is "slow".
pub(super) const SLOW_KEY_TO_PRESENTED_MS: u64 = 50;
/// A frame whose `nextDrawable` wait exceeds this is "slow".
pub(super) const SLOW_DRAWABLE_WAIT_MS: u64 = 8;
/// Minimum spacing between WARN lines.
pub(super) const WARN_INTERVAL_MS: u64 = 10_000;
/// Consecutive deferred (app-borrowed) draws after which we stop re-arming.
pub(super) const MAX_CONSECUTIVE_REARMS: u32 = 1000;

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

/// Whether a draw that was deferred because the app was mutably borrowed should
/// request another redraw, given how many consecutive deferrals preceded it.
/// The cap keeps a (buggy) persistently-held borrow from spinning forever.
pub(super) fn should_rearm_deferred_draw(consecutive_deferrals: u32) -> bool {
    consecutive_deferrals < MAX_CONSECUTIVE_REARMS
}

static LIMITER: Mutex<SlowFrameLimiter> = Mutex::new(SlowFrameLimiter::new());

static CONSECUTIVE_DEFERRALS: AtomicU32 = AtomicU32::new(0);

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Notes that a draw was deferred (app mutably borrowed). Returns whether the
/// caller should re-request a redraw.
pub(super) fn note_deferred_draw() -> bool {
    let previous = CONSECUTIVE_DEFERRALS.fetch_add(1, Ordering::Relaxed);
    if previous + 1 == MAX_CONSECUTIVE_REARMS {
        log::warn!(
            "Draw deferred {MAX_CONSECUTIVE_REARMS} times in a row because the app stayed mutably borrowed; no longer re-arming"
        );
    }
    should_rearm_deferred_draw(previous)
}

/// Notes that a draw ran, clearing the deferral streak.
pub(super) fn note_draw_ran() {
    CONSECUTIVE_DEFERRALS.store(0, Ordering::Relaxed);
}

/// Called from `host_view.m`'s presented handler (any thread) once a frame has
/// reached the screen. `key_to_presented_ns` is 0 when no key press was
/// pending for this frame; `presented` is false if the drawable was dropped.
#[unsafe(no_mangle)]
extern "C-unwind" fn warp_frame_presented(
    key_to_presented_ns: u64,
    drawable_wait_ns: u64,
    presented: bool,
) {
    let key_ms = (presented && key_to_presented_ns > 0).then_some(key_to_presented_ns / 1_000_000);
    let wait_ms = drawable_wait_ns / 1_000_000;
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
