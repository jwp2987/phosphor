//! The single decision for "does closing this window quit the app?" (#1154).
//!
//! Linux and Windows always quit when the last window goes away. macOS follows the
//! `general.quit_on_last_window_closed` setting, which now defaults to on, so the
//! app can no longer be left running with no window (and no way to reach it) unless
//! the user opts into Mac-style behaviour.
//!
//! Two call sites consult it: `on_should_close_window` (before a window closes, so
//! the quit goes through the normal terminate path and its unsaved-state prompt) and
//! `on_window_will_close` (a backstop for closes that skip the first, such as the
//! unsaved-changes dialog's force-close).

/// Why the policy is being asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    /// A window is about to close (close button, Cmd-W / close of the last tab, the
    /// last tab's shell exiting, programmatic cancellable close). It is still counted
    /// in `windows_open`.
    Requested,
    /// The window's content was moved into another window; the app never quits.
    ContentTransferred,
    /// A window has already closed and is no longer counted in `windows_open`.
    AlreadyClosed,
}

/// Whether the platform quits on last-window close regardless of the setting.
pub const PLATFORM_ALWAYS_QUITS: bool =
    cfg!(any(target_os = "linux", target_os = "freebsd", windows));

/// Returns true if the app should terminate instead of leaving zero windows.
///
/// Quake-mode windows are ordinary windows here: a hidden quake window still counts
/// as open, and closing the only remaining window of any kind quits.
pub fn should_quit_app(
    platform_always_quits: bool,
    quit_on_last_window_closed: bool,
    windows_open: usize,
    reason: CloseReason,
) -> bool {
    if !(platform_always_quits || quit_on_last_window_closed) {
        return false;
    }
    match reason {
        CloseReason::Requested => windows_open == 1,
        CloseReason::AlreadyClosed => windows_open == 0,
        CloseReason::ContentTransferred => false,
    }
}

#[cfg(test)]
#[path = "window_close_policy_tests.rs"]
mod tests;
