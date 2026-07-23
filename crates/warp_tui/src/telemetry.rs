//! Telemetry event types for the `warp-tui` front-end.
//!
//! Zap has physically removed telemetry sending (see the compatibility shims in
//! `warp_core::telemetry`), so these types no longer implement the upstream
//! `TelemetryEvent`/`TelemetryEventDesc` framework. They are retained only as
//! the payloads named by the no-op `send_telemetry_*!` macros at their call
//! sites, keeping those call sites (and their type inference) unchanged while no
//! event is ever emitted.

/// Marks that the headless TUI has launched. Named by the startup send-telemetry
/// call in `session::init`; emits nothing.
#[derive(Debug)]
pub(crate) struct TuiStartupTelemetryEvent;

/// Health signals for the TUI auto-updater, named by the auto-updater's
/// send-telemetry call. Constructed to describe a check outcome; emits nothing.
///
/// The fields are retained so the construction sites keep their shape, even
/// though Zap sends no telemetry.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum TuiAutoupdateTelemetryEvent {
    /// A background update check completed.
    CheckCompleted {
        /// `"up_to_date"`, `"installed"`, `"pending_restart"`, or `"locked"`.
        outcome: &'static str,
        /// The relevant version: the running version when up to date, or the
        /// newly installed / staged version.
        version: Option<String>,
    },
    /// A background update check failed (e.g. network or install errors).
    CheckFailed { error: String },
}
