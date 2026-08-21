mod anyhow;
mod registration;
mod reqwest;
#[cfg(not(target_family = "wasm"))]
mod tokio;
#[cfg(not(target_family = "wasm"))]
mod websocket;

// Re-export for macro use.
#[doc(hidden)]
pub use inventory::submit;
// Re-exported under a name that doesn't collide with the local `anyhow` submodule (which holds
// `AnyhowErrorExt`), so the macro can build an `anyhow::Error` without requiring every calling
// crate to depend on the `anyhow` crate directly.
#[doc(hidden)]
pub use ::anyhow as __anyhow;

pub use self::anyhow::AnyhowErrorExt;
pub use registration::{ErrorRegistration, RegisteredError};

pub use registration::register_error;

/// The `target` that is set by log entries from this module.
pub const LOG_TARGET: &str = "errors::report_error";

/// Controls how often a [`report_error!`] invocation logs errors.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReportErrorLogMode {
    /// Log every time the error is reported.
    #[default]
    EveryTime,
    /// Log only the first time this macro invocation is reached during the current app run.
    OncePerRun,
}

/// Formats `report_error!` `extra: { .. }` fields as a suffix appended to the log line, e.g.
/// `" [key=value, key2=value2]"`. Returns an empty string when there are no fields.
#[doc(hidden)]
pub fn format_context_suffix(fields: &[(&'static str, String)]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let mut suffix = String::from(" [");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            suffix.push_str(", ");
        }
        suffix.push_str(key);
        suffix.push('=');
        suffix.push_str(value);
    }
    suffix.push(']');
    suffix
}

/// Reports an error encountered during execution.
///
/// This checks whether or not the error is actionable, and logs an error or
/// warning accordingly.  (Logs at the Error level get reported back to us, so
/// we don't want to log anything at Error level that we aren't able to act
/// upon.)
///
/// Beyond the plain `report_error!(err)` form, this also accepts:
/// - `report_error!("a static message")` — wraps the literal in an `anyhow::Error`. Does not
///   accept trailing format arguments, to discourage interpolating variable data into the
///   (grouped) message — put variable data in `extra: { .. }` instead.
/// - `report_error!(err, extra: { "key" => value, .. })` — appends structured fields to the log
///   line without perturbing the message. `%value` forces `Display`, `?value` forces `Debug`, a
///   bare `value` defaults to `Display`.
/// - `report_error!(err, ReportErrorLogMode::OncePerRun)` — logs only the first time this
///   callsite is reached during the run (each callsite tracks its own `AtomicBool`).
/// - Any combination of an `extra:` block followed by a log mode.
#[macro_export]
macro_rules! report_error {
    (@log $err:expr_2021) => {{
        #[allow(unused_imports)]
        use $crate::errors::{AnyhowErrorExt as _, ErrorExt as _, LOG_TARGET};
        let err = $err;
        let log_level = if err.is_actionable() {
            err.report_error();
            log::Level::Error
        } else {
            log::Level::Warn
        };
        log::log!(target: LOG_TARGET, log_level, "{:#}", err);
    }};
    (@once_per_run $err:expr_2021) => {{
        static HAS_LOGGED_REPORT_ERROR: ::std::sync::atomic::AtomicBool =
            ::std::sync::atomic::AtomicBool::new(false);
        if !HAS_LOGGED_REPORT_ERROR.swap(true, ::std::sync::atomic::Ordering::Relaxed) {
            $crate::report_error!(@log $err);
        }
    }};
    (@once_per_run_extra $err:expr_2021, { $($fields:tt)* }) => {{
        static HAS_LOGGED_REPORT_ERROR: ::std::sync::atomic::AtomicBool =
            ::std::sync::atomic::AtomicBool::new(false);
        if !HAS_LOGGED_REPORT_ERROR.swap(true, ::std::sync::atomic::Ordering::Relaxed) {
            $crate::report_error!(@log_extra $err, { $($fields)* });
        }
    }};
    // Reports `err` and appends the given fields to the local log line, keeping per-instance
    // specifics out of the (grouped) message.
    (@log_extra $err:expr_2021, { $($fields:tt)* }) => {{
        #[allow(unused_imports)]
        use $crate::errors::{AnyhowErrorExt as _, ErrorExt as _, LOG_TARGET};
        let err = $err;
        let mut __fields: ::std::vec::Vec<(&'static str, ::std::string::String)> =
            ::std::vec::Vec::new();
        $crate::report_error!(@fields __fields $($fields)*);
        let __suffix = $crate::errors::format_context_suffix(&__fields);
        let log_level = if err.is_actionable() {
            err.report_error();
            log::Level::Error
        } else {
            log::Level::Warn
        };
        log::log!(target: LOG_TARGET, log_level, "{:#}{}", err, __suffix);
    }};
    // Field muncher for `extra: { .. }`. `%expr` forces Display, `?expr` forces Debug, a bare
    // expr defaults to Display.
    (@fields $vec:ident $key:literal => ? $value:expr_2021 $(, $($rest:tt)*)?) => {{
        $vec.push(($key, format!("{:?}", $value)));
        $crate::report_error!(@fields $vec $($($rest)*)?);
    }};
    (@fields $vec:ident $key:literal => % $value:expr_2021 $(, $($rest:tt)*)?) => {{
        $vec.push(($key, format!("{}", $value)));
        $crate::report_error!(@fields $vec $($($rest)*)?);
    }};
    (@fields $vec:ident $key:literal => $value:expr_2021 $(, $($rest:tt)*)?) => {{
        $vec.push(($key, format!("{}", $value)));
        $crate::report_error!(@fields $vec $($($rest)*)?);
    }};
    (@fields $vec:ident $(,)?) => {};
    // Static-message form: a bare string literal, wrapped in an `anyhow::Error`.
    ($fmt:literal, extra: { $($fields:tt)* }) => {{
        $crate::report_error!(
            @log_extra $crate::errors::__anyhow::anyhow!($fmt), { $($fields)* }
        );
    }};
    ($fmt:literal, extra: { $($fields:tt)* }, $log_mode:expr_2021) => {{
        match $log_mode {
            $crate::errors::ReportErrorLogMode::EveryTime => {
                $crate::report_error!(
                    @log_extra $crate::errors::__anyhow::anyhow!($fmt), { $($fields)* }
                );
            }
            $crate::errors::ReportErrorLogMode::OncePerRun => {
                $crate::report_error!(
                    @once_per_run_extra $crate::errors::__anyhow::anyhow!($fmt), { $($fields)* }
                );
            }
        }
    }};
    ($fmt:literal) => {{
        $crate::report_error!(@log $crate::errors::__anyhow::anyhow!($fmt));
    }};
    ($err:expr_2021, extra: { $($fields:tt)* }) => {{
        $crate::report_error!(@log_extra $err, { $($fields)* });
    }};
    ($err:expr_2021, extra: { $($fields:tt)* }, $log_mode:expr_2021) => {{
        match $log_mode {
            $crate::errors::ReportErrorLogMode::EveryTime => {
                $crate::report_error!(@log_extra $err, { $($fields)* });
            }
            $crate::errors::ReportErrorLogMode::OncePerRun => {
                $crate::report_error!(@once_per_run_extra $err, { $($fields)* });
            }
        }
    }};
    ($err:expr_2021) => {{
        $crate::report_error!(@log $err);
    }};
    ($err:expr_2021, $log_mode:expr_2021) => {{
        match $log_mode {
            $crate::errors::ReportErrorLogMode::EveryTime => {
                $crate::report_error!(@log $err);
            }
            $crate::errors::ReportErrorLogMode::OncePerRun => {
                $crate::report_error!(@once_per_run $err);
            }
        }
    }};
}
pub use report_error;

/// Reports an error if the provided [`Result`] is [`Err`].
///
/// This checks whether or not the error is actionable, and logs an error or
/// warning accordingly.  (Logs at the Error level get reported back to us, so
/// we don't want to log anything at Error level that we aren't able to act
/// upon.)
#[macro_export]
macro_rules! report_if_error {
    ($result:expr_2021) => {{
        if let Err(error) = &$result {
            $crate::report_error!(error);
        }
    }};
    ($result:expr_2021, extra: { $($fields:tt)* }) => {{
        if let Err(error) = &$result {
            $crate::report_error!(error, extra: { $($fields)* });
        }
    }};
    ($result:expr_2021, $log_mode:expr_2021) => {{
        if let Err(error) = &$result {
            $crate::report_error!(error, $log_mode);
        }
    }};
}
pub use report_if_error;

pub trait ErrorExt: RegisteredError + std::error::Error {
    /// Returns whether or not an error is something that is actionable by our
    /// engineering team.
    fn is_actionable(&self) -> bool;

    /// Hook for out-of-band error reporting.
    ///
    /// Upstream this captured the error to Sentry; that sink is gone with the
    /// cloud integrations, so there is nothing left to do here. It must stay a
    /// no-op: `report_error!(@log ..)` already emits the log line (at
    /// `LOG_TARGET`, with the `extra:` fields attached), so logging here would
    /// emit every actionable error twice — the second copy carrying the
    /// module-path target, which `RUST_LOG` filters on `LOG_TARGET` cannot
    /// suppress.
    fn report_error(&self) {}
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
