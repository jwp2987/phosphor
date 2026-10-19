/// Destination for log output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogDestination {
    /// Write logs to a file.
    File,
    /// Write logs to stderr.
    Stderr,
}

/// Frontend that owns a logging session.
///
/// Selects the log subdirectory and rotation policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogFrontend {
    /// The desktop GUI frontend.
    #[default]
    Gui,
    /// The headless terminal frontend.
    Tui,
    /// CLI and remote server processes.
    Cli,
}

/// Configuration for initializing the logger.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogConfig {
    /// Frontend-specific directory and rotation policy. Filenames continue to come from the
    /// active channel configuration.
    pub frontend: LogFrontend,
    /// The destination for log output. If `None`, the destination is inferred from the environment.
    pub log_destination: Option<LogDestination>,
    /// Optional in-session size threshold for `warp.log`. When `Some(n)` and the active
    /// file accumulates more than `n` bytes during a single execution, it is rotated to
    /// `warp.log.in_session.0` and a fresh active file is opened. Older `.in_session.N`
    /// files shift up and the oldest is discarded, matching the per-startup
    /// `rotate_log_files` behavior. `None` preserves the existing unbounded-within-session
    /// growth (warpdotdev/warp#10879).
    pub max_file_size_bytes: Option<u64>,
}

#[cfg_attr(not(target_family = "wasm"), path = "native.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

#[cfg(not(target_family = "wasm"))]
mod rotation;

pub use imp::init;
#[cfg(not(target_family = "wasm"))]
pub use imp::{
    ExtraFile, InlineFile, LogBundleExtras, create_log_bundle_zip, default_log_bundle_filename,
    log_directory, log_file_path, rotate_log_files, write_log_bundle_zip_to,
};

#[cfg(not(target_family = "wasm"))]
pub use imp::{
    init_for_crash_recovery_process, init_logging_for_unit_tests, on_crash_recovery_process_killed,
    on_parent_process_crash,
};

/// Tap on the unit-test logger, for tests that must assert on what a production
/// call site actually emitted. See [`set_unit_test_log_tap`].
#[cfg(not(target_family = "wasm"))]
pub use imp::{UnitTestLogTap, set_unit_test_log_tap, unit_test_log_tap_is_available};

/// Why log-file setup failed and logging fell back to the terminal (#861).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFallbackNotice {
    /// The directory the log file could not be created in.
    pub directory: std::path::PathBuf,
    /// The OS error, as text.
    pub reason: String,
}

static LOG_FALLBACK_NOTICE: std::sync::Mutex<Option<LogFallbackNotice>> =
    std::sync::Mutex::new(None);

/// Records that logging fell back to the terminal. Called by the logger
/// before any UI exists; the UI takes it with [`take_log_fallback_notice`].
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub(crate) fn record_log_fallback(directory: std::path::PathBuf, reason: String) {
    if let Ok(mut slot) = LOG_FALLBACK_NOTICE.lock() {
        *slot = Some(LogFallbackNotice { directory, reason });
    }
}

/// Returns (once) the notice left when the log file could not be set up, so
/// the UI can tell the user instead of the fallback living only in a terminal.
pub fn take_log_fallback_notice() -> Option<LogFallbackNotice> {
    LOG_FALLBACK_NOTICE
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
}

#[cfg(test)]
mod fallback_notice_tests {
    use super::*;

    #[test]
    fn fallback_notice_is_taken_once() {
        let _ = take_log_fallback_notice();
        record_log_fallback("/state".into(), "Permission denied (os error 13)".into());
        let notice = take_log_fallback_notice().expect("recorded");
        assert_eq!(notice.directory, std::path::PathBuf::from("/state"));
        assert!(notice.reason.contains("Permission denied"));
        assert!(take_log_fallback_notice().is_none());
    }
}
