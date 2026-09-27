//! Central toast notification for a settings write that failed to persist.
//!
//! `report_if_error!`/`report_error!` only log -- `report_error()` has been a
//! documented no-op since the Sentry sink was removed
//! (`crates/warp_core/src/errors.rs:212-223`) -- so a settings write that fails
//! (e.g. `settings.toml` has become unparseable, or the disk is full) is
//! invisible at the moment it happens. The toggle the user clicked either
//! stays in memory only or never changes at all, and either way it reverts (or
//! was never applied) the next time the app starts, with no indication of why.
//!
//! This module adds a rate-limited toast on top of the existing log line. Use
//! [`report_settings_write_error!`] at a `set_value`/`toggle_and_save_value`
//! call site in place of a bare `let _ = ...` (which drops the error with no
//! log line at all) or a log-only `report_if_error!`.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use warpui::{
    AppContext, Entity, GetSingletonModelHandle, ModelContext, SingletonEntity as _, UpdateModel,
    ViewContext, WindowId,
};

use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// Minimum time between two settings-write-failure toasts, app-wide.
///
/// A `settings.toml` that has stopped parsing fails *every* subsequent write
/// for as long as it stays broken, so without a floor here one bad file (or
/// one bad disk) would show a toast per keystroke. The log line is
/// unthrottled -- only the toast is rate-limited -- so nothing is lost from
/// the log for diagnosing the underlying cause.
const MIN_TOAST_INTERVAL: Duration = Duration::from_secs(15);

/// Last time a settings-write-failure toast was shown, process-wide.
static LAST_TOAST_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Pure gate: given the instant of the last toast (if any) and the current
/// instant, should a new toast fire?
///
/// Split out so the throttle is unit-testable without touching the
/// process-global `LAST_TOAST_AT` -- mirrors the reasoning in
/// `crates/warpui_core/src/report_error.rs`'s `take_once`, which tests its
/// throttle the same way.
fn should_show_toast(last: Option<Instant>, now: Instant) -> bool {
    match last {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= MIN_TOAST_INTERVAL,
    }
}

/// Evaluates [`should_show_toast`] against `slot`'s current value and, if it
/// passes, records `now` into `slot` so the next call is throttled from this
/// one -- an atomic check-and-set under `slot`'s lock, not a separate read
/// and write, so two failures racing on different threads cannot both pass
/// the gate.
fn take_toast_slot(slot: &Mutex<Option<Instant>>, now: Instant) -> bool {
    let mut last = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if should_show_toast(*last, now) {
        *last = Some(now);
        true
    } else {
        false
    }
}

/// Resolves the window a settings-write-failure toast should appear in.
///
/// A [`ViewContext`] has a precise answer: the window of the view that
/// triggered the write. Anything else -- an `AppContext`-only global-action
/// handler, or a settings model's own [`ModelContext`] reached through
/// `SomeSettings::handle(ctx).update(ctx, ...)` -- has no view to ask, so it
/// falls back to whichever window is currently active. That fallback is what
/// unblocks `workspace/global_actions.rs`'s handlers, which have no
/// `window_id` of their own.
trait SettingsWriteFailureWindow {
    fn settings_write_failure_window_id(&self) -> Option<WindowId>;
}

impl SettingsWriteFailureWindow for AppContext {
    fn settings_write_failure_window_id(&self) -> Option<WindowId> {
        self.windows().active_window()
    }
}

impl<T: Entity> SettingsWriteFailureWindow for ViewContext<'_, T> {
    fn settings_write_failure_window_id(&self) -> Option<WindowId> {
        Some(self.window_id())
    }
}

impl<M> SettingsWriteFailureWindow for ModelContext<'_, M> {
    fn settings_write_failure_window_id(&self) -> Option<WindowId> {
        self.windows().active_window()
    }
}

/// Reports a failed settings write: always logs (like `report_if_error!`), and
/// shows a rate-limited toast in the active window so the user learns the
/// write did not stick instead of finding out only when it silently reverts
/// at the next launch.
///
/// Called by [`report_settings_write_error!`]; use the macro at call sites
/// rather than this function directly.
pub(crate) fn notify_settings_write_failed<C>(err: anyhow::Error, ctx: &mut C)
where
    C: GetSingletonModelHandle + UpdateModel + SettingsWriteFailureWindow,
{
    log::error!("{err:#}");
    if !take_toast_slot(&LAST_TOAST_AT, Instant::now()) {
        return;
    }
    let Some(window_id) = ctx.settings_write_failure_window_id() else {
        // No window is active (e.g. headless/TUI, or every window closed) --
        // nothing to show the toast in. The log line above is still the
        // record of the failure.
        return;
    };
    ToastStack::handle(ctx).update(ctx, move |toast_stack, ctx| {
        let toast = DismissibleToast::error(crate::t!("common-settings-write-failed"));
        toast_stack.add_ephemeral_toast(toast, window_id, ctx);
    });
}

/// Routes a settings-write `Result` through [`notify_settings_write_failed`]
/// in place of `let _ = ...` (discards the error with no log line at all) or
/// `report_if_error!` (logs only, never shown to the user).
///
/// Usage mirrors the call it replaces: wrap the `set_value`/
/// `toggle_and_save_value` call and pass the same context used inside it,
/// e.g. `report_settings_write_error!(settings.field.set_value(value, ctx), ctx)`.
macro_rules! report_settings_write_error {
    ($result:expr, $ctx:expr) => {{
        if let ::std::result::Result::Err(err) = $result {
            $crate::settings_write_failure::notify_settings_write_failed(
                ::anyhow::Error::from(err),
                $ctx,
            );
        }
    }};
}

pub(crate) use report_settings_write_error;

#[cfg(test)]
#[path = "settings_write_failure_tests.rs"]
mod tests;
