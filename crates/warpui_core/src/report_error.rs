//! Local `report_error!` shim for `warpui_core`.
//!
//! The fork's real macro is `warp_core::errors::report_error!`, but `warpui_core`
//! cannot reach it: `warp_core` depends on `warpui`, and `warpui` depends on
//! `warpui_core`, so adding `warp_core` here would close a Cargo dependency cycle.
//! This mirrors the part of that macro's surface `warpui_core` actually needs --
//! `ReportErrorLogMode::OncePerRun` -- so upstream call sites port across with only
//! their `use` line rewritten. `warp_tui` carries an equivalent shim for a different
//! reason (it does not vendor `warp_errors` at all); see
//! `crates/warp_tui/src/report_error.rs`.
//!
//! Differences from `warp_core::errors::report_error!`, all deliberate:
//!
//! - Everything logs at `Error` level. The `is_actionable()` split that picks
//!   `Error` vs `Warn` rides on the `ErrorExt`/`RegisteredError` traits, which live
//!   in `warp_core` and are unreachable from here for the same cycle reason.
//! - There is no out-of-band sink. `report_error!` in this fork is local logging in
//!   every crate -- the Sentry sink went with the dropped cloud integrations -- so
//!   nothing is lost by logging directly.
//! - `extra: { .. }` is not implemented. Upstream splits variable context out of the
//!   message so Sentry groups the fixed part; with no grouping sink that split has no
//!   consumer, and callers can interpolate the context into the message instead.

use std::sync::atomic::{AtomicBool, Ordering};

/// Controls how often a [`report_error!`] invocation logs.
///
/// Kept name-compatible with `warp_core::errors::ReportErrorLogMode` so ported call
/// sites read identically.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum ReportErrorLogMode {
    /// Log every time the error is reported.
    #[default]
    EveryTime,
    /// Log only the first time this macro invocation is reached during the current
    /// app run.
    OncePerRun,
}

/// Returns `true` the first time it is called for `flag`, and `false` every time
/// after. Each `report_error!` invocation site owns its own flag, so throttling one
/// call site never silences another.
///
/// Split out of the macro so the throttle is unit-testable without installing a
/// global logger.
pub(crate) fn take_once(flag: &AtomicBool) -> bool {
    !flag.swap(true, Ordering::Relaxed)
}

/// Reports an error encountered during execution.
///
/// Accepted forms:
/// - `report_error!(err)` -- log every time.
/// - `report_error!(err, ReportErrorLogMode::OncePerRun)` -- log only the first time
///   this callsite is reached during the run.
macro_rules! report_error {
    ($err:expr, $log_mode:expr) => {{
        // One flag per macro invocation, matching `warp_core`'s semantics.
        static HAS_LOGGED_REPORT_ERROR: ::std::sync::atomic::AtomicBool =
            ::std::sync::atomic::AtomicBool::new(false);
        match $log_mode {
            $crate::report_error::ReportErrorLogMode::EveryTime => {
                log::error!("{:#}", $err);
            }
            $crate::report_error::ReportErrorLogMode::OncePerRun => {
                if $crate::report_error::take_once(&HAS_LOGGED_REPORT_ERROR) {
                    log::error!("{:#}", $err);
                }
            }
        }
    }};
    ($err:expr $(,)?) => {{
        log::error!("{:#}", $err);
    }};
}

pub(crate) use report_error;

#[cfg(test)]
#[path = "report_error_tests.rs"]
mod tests;
