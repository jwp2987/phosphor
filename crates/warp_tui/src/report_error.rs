//! Local shim for warp's `warp_errors::report_error!` (absent in Zap).
//!
//! Zap does not vendor the `warp_errors` crate (it pulls Sentry/reqwest/tokio for
//! crash reporting we do not want here). This maps the macro's call forms to
//! `log`, preserving the log output without the crash-reporting side effects.

/// Log-frequency selector accepted by [`report_error!`]. Kept for source
/// compatibility with warp's macro; `OncePerRun` is honored, the rest log every
/// time.
#[allow(dead_code)]
pub enum ReportErrorLogMode {
    EveryTime,
    OncePerRun,
}

/// Drop-in for `warp_errors::report_error!`, logging via `log::error!` instead of
/// reporting to Sentry. Handles the call forms used in this crate:
/// `report_error!(err)`, `report_error!(err, ReportErrorLogMode::OncePerRun)`,
/// and `report_error!(err, extra: { "k" => v, ... })`.
macro_rules! report_error {
    ($err:expr, $crate_mode:path) => {{
        // OncePerRun: log at most once per process for this call site.
        static HAS_LOGGED: ::std::sync::atomic::AtomicBool =
            ::std::sync::atomic::AtomicBool::new(false);
        let _ = &$crate_mode;
        if !HAS_LOGGED.swap(true, ::std::sync::atomic::Ordering::Relaxed) {
            log::error!("{:#}", $err);
        }
    }};
    // structured `extra` fields, tracing-style sigils: `k => %v` (Display),
    // `k => ?v` (Debug), or plain `k => v`.
    ($err:expr, extra: { $($k:tt => $(%)? $(?)? $v:expr),* $(,)? }) => {{
        log::error!("{:#}{}", $err,
            format_args!(concat!($(" ", $k, "={:?}"),*) $(, $v)*));
    }};
    ($err:expr $(,)?) => {{
        log::error!("{:#}", $err);
    }};
}

pub(crate) use report_error;
