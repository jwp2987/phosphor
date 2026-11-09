//! "Report a problem": the app-side collector for the local diagnostics bundle (Wave 1.5 #1).
//!
//! Gathers the facts (version, OS, build flags, enabled feature flags, the settings file) and hands
//! them to [`warp_logging::report_bundle`], which scrubs and zips them. The result is ONE local
//! file; **nothing in this module (or the builder) opens a network connection or a URL** and there
//! is no "send" action. The user decides what to do with the file.
//!
//! The output directory is `$PHOSPHOR_DIAG_BUNDLE_DIR` when set (the appci test knob), else the
//! user's Downloads folder, else the home directory.

use std::path::PathBuf;

use warp_core::channel::ChannelState;
use warp_core::features::FeatureFlag;
use warp_logging::report_bundle::{
    Identity, REPORT_WINDOW_MINUTES, ReportInputs, ReportSummary, build_report_bundle,
    default_report_filename,
};

/// Everything read from app state on the main thread; the file work runs on a blocking thread.
pub struct ProblemReportRequest {
    inputs: ReportInputs,
    out_path: PathBuf,
}

/// Where the bundle goes.
pub fn output_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PHOSPHOR_DIAG_BUNDLE_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::download_dir().or_else(dirs::home_dir)
}

fn manifest_text() -> String {
    let version = ChannelState::app_version().unwrap_or("Dev");
    let mut flags: Vec<String> = enum_iterator::all::<FeatureFlag>()
        .filter(|f| f.is_enabled())
        .map(|f| format!("{f:?}"))
        .collect();
    flags.sort();
    format!(
        "Phosphor problem report\n\
         Generated at: {now}\n\
         Version: {version}\n\
         Channel: {channel}\n\
         OS: {os}\n\
         Arch: {arch}\n\
         Family: {family}\n\
         Build profile: {profile}\n\
         Release bundle: {release}\n\
         Channel state: {state}\n\
         Enabled feature flags ({n}): {flags}\n",
        now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z"),
        channel = ChannelState::channel(),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        family = std::env::consts::FAMILY,
        profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        release = ChannelState::is_release_bundle(),
        state = ChannelState::debug_str(),
        n = flags.len(),
        flags = flags.join(", "),
    )
}

/// The terminal's secret detector as a second pass over every line.
fn secret_detector_pass(line: &mut String) {
    crate::ai::agent::redaction::redact_secrets(line);
}

/// Read the app state needed for a bundle. `None` when no output directory exists.
pub fn prepare() -> Option<ProblemReportRequest> {
    let dir = output_dir()?;
    let hostname = crate::terminal::model::session::get_local_hostname().ok();
    let settings_toml =
        std::fs::read_to_string(crate::settings::user_preferences_toml_file_path()).ok();
    let inputs = ReportInputs {
        log_files: warp_logging::log_files_for_report().unwrap_or_default(),
        manifest: manifest_text(),
        settings_toml,
        identity: Identity::from_env(hostname),
        now: chrono::Utc::now(),
        window: chrono::Duration::minutes(REPORT_WINDOW_MINUTES),
        extra_scrub: Some(secret_detector_pass),
    };
    Some(ProblemReportRequest {
        inputs,
        out_path: dir.join(default_report_filename()),
    })
}

/// Write the bundle (blocking file work). Returns the file and what went in.
pub fn write(request: ProblemReportRequest) -> anyhow::Result<(PathBuf, ReportSummary)> {
    if let Some(parent) = request.out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let summary = build_report_bundle(&request.out_path, &request.inputs)?;
    Ok((request.out_path, summary))
}

#[cfg(test)]
#[path = "problem_report_tests.rs"]
mod tests;
