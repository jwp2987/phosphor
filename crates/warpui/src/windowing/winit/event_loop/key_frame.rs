//! A key press's frame is built at the end of the loop iteration that handled the key, even when
//! the compositor is still holding back `RedrawRequested` (#1843).
//!
//! On Wayland, winit withholds `RedrawRequested` while a frame callback is outstanding
//! (`pre_present_notify` asks for one with every frame; `lib/winit` wayland `event_loop/mod.rs`,
//! `FrameCallbackState::Requested`). While something animates (a streaming agent reply, a
//! spinner) a callback is always outstanding, so a key typed just after a frame waits for the
//! next callback before its own frame is even built: measured 10.3 ms median key -> frame against
//! 3.0 ms idle, 13 ms for a key that lands in the first 4 ms after a frame
//! (docs/design/1843-1848-latency.md).
//!
//! The fix keeps winit's throttle for every other frame and lets only a key bypass it: when the
//! iteration that dispatched a key ends (`AboutToWait`) with the window still invalidated, the
//! frame is drawn there. One frame per loop iteration at most, however many keys the iteration
//! handled, so a burst of keys still coalesces into one frame. The buffer is committed without
//! waiting (only when the surface's present mode never waits for vsync), and the compositor shows
//! the newest buffer at its next refresh.
//!
//! Kill switch: `PHOSPHOR_KEY_FRAME_NOW=0` restores the old behaviour exactly. A frame drawn
//! here that blocks in acquire or present turns the path off for that window for the rest of the
//! session (the driver throttles after all), so the main thread is never parked there repeatedly.

use std::sync::OnceLock;

use instant::Duration;

/// A key frame whose acquire + present took at least this long was blocked by the driver.
pub(super) const BLOCKED_FRAME: Duration = Duration::from_millis(4);
/// This many blocked key frames turn the path off for the window.
pub(super) const BLOCKED_FRAMES_TO_DISABLE: u8 = 2;

/// `PHOSPHOR_KEY_FRAME_NOW=0` turns the path off; anything else (or unset) leaves it on.
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| enabled_from_env(std::env::var("PHOSPHOR_KEY_FRAME_NOW").ok().as_deref()))
}

pub(super) fn enabled_from_env(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

/// What is true at the end of a loop iteration, for [`KeyFrameNow::take_decision`].
#[derive(Clone, Copy, Debug)]
pub(super) struct Facts {
    /// The kill switch is not set.
    pub enabled: bool,
    /// The windowing backend holds `RedrawRequested` back for frame callbacks (Wayland).
    pub redraw_is_throttled: bool,
    /// The window was invalidated and no frame has been built since (no scene).
    pub scene_invalid: bool,
    /// The surface's present mode never waits for vsync (Immediate or Mailbox).
    pub present_never_waits: bool,
    /// The app can be borrowed (not inside a nested native loop).
    pub can_borrow_app: bool,
}

/// Per-window state: whether a key was dispatched in this loop iteration and has had no frame.
#[derive(Debug, Default)]
pub(super) struct KeyFrameNow {
    key_pending: bool,
    blocked_frames: u8,
}

impl KeyFrameNow {
    /// A key press (or committed IME text) was dispatched to the window.
    pub(super) fn note_key(&mut self) {
        self.key_pending = true;
    }

    /// A frame was drawn for the window (by any path): the key, if any, has its frame.
    pub(super) fn note_frame_drawn(&mut self) {
        self.key_pending = false;
    }

    /// Whether the path has turned itself off for this window.
    pub(super) fn is_disabled(&self) -> bool {
        self.blocked_frames >= BLOCKED_FRAMES_TO_DISABLE
    }

    /// End of the loop iteration: whether to draw the key's frame now. Consumes the pending key
    /// either way, so a key is offered exactly once; a key whose frame is not drawn here keeps
    /// the ordinary `RedrawRequested` it already asked for.
    pub(super) fn take_decision(&mut self, facts: Facts) -> bool {
        let key_pending = std::mem::take(&mut self.key_pending);
        key_pending
            && facts.enabled
            && facts.redraw_is_throttled
            && facts.scene_invalid
            && facts.present_never_waits
            && facts.can_borrow_app
            && !self.is_disabled()
    }

    /// A frame was drawn by this path; `wait` is its acquire + present time. Returns true when
    /// this frame turned the path off for the window (log it once).
    pub(super) fn note_now_frame(&mut self, wait: Duration) -> bool {
        if self.is_disabled() {
            return false;
        }
        if wait >= BLOCKED_FRAME {
            self.blocked_frames += 1;
        } else {
            // Only consecutive blocked frames count: one slow acquire is not a throttling driver.
            self.blocked_frames = 0;
        }
        self.is_disabled()
    }
}

/// Detail of `input_order hop=key_frame_now`: the frame's acquire + present time.
pub(super) fn now_frame_detail(wait: Duration) -> String {
    format!("wait_ms={:.1}", wait.as_secs_f64() * 1e3)
}

#[cfg(test)]
#[path = "key_frame_tests.rs"]
mod tests;
