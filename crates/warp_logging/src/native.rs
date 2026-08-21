use std::env;
use std::fs::{self, File};
use std::io::{copy, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use chrono::Local;
use log::LevelFilter;
use warp_core::channel::ChannelState;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{LogConfig, LogDestination, LogFrontend};

const MAX_FILES_IN_GUI_ROTATION: usize = 5;
const MAX_FILES_IN_CLI_ROTATION: usize = 10;
const CLI_LOG_SUBDIRECTORY: &str = "oz";
const TUI_LOG_SUBDIRECTORY: &str = "warp-cli";
const TEMP_LOG_FILE_SUFFIX: &str = "old.temp";

/// Runtime logging state, computed from `LogConfig` during initialization.
#[derive(Debug)]
struct LogState {
    /// Whether or not logs should be written to a file.
    use_logfile: bool,

    /// The directory that logs should be written to. This is set even if `use_logfile` is false,
    /// as we sometimes generate other log files.
    log_directory: PathBuf,

    /// The resolved base filename for the active frontend and channel.
    logfile_name: String,

    /// The maximum number of backup log files to keep during rotation.
    max_rotation: usize,
}

impl LogFrontend {
    fn log_directory(self, base_directory: PathBuf) -> PathBuf {
        match self {
            LogFrontend::Gui => base_directory,
            LogFrontend::Tui => base_directory.join(TUI_LOG_SUBDIRECTORY),
            LogFrontend::Cli => base_directory.join(CLI_LOG_SUBDIRECTORY),
        }
    }

    fn max_rotation(self) -> usize {
        match self {
            LogFrontend::Gui => MAX_FILES_IN_GUI_ROTATION,
            LogFrontend::Tui | LogFrontend::Cli => MAX_FILES_IN_CLI_ROTATION,
        }
    }
}

impl LogState {
    fn new(
        use_logfile: bool,
        base_log_directory: PathBuf,
        logfile_name: String,
        frontend: LogFrontend,
    ) -> Self {
        Self {
            use_logfile,
            log_directory: frontend.log_directory(base_log_directory),
            logfile_name,
            max_rotation: frontend.max_rotation(),
        }
    }

    fn log_file_path(&self) -> PathBuf {
        main_process_log_file_path(&self.log_directory, &self.logfile_name)
    }

    /// Bundles the current, in-session, and rotated logs for this state into a
    /// timestamped zip in the state's log directory. No extras — this is the
    /// entry point exercised directly by the unit tests. The public
    /// `create_log_bundle_zip` free function layers caller-provided extras on
    /// top via [`Self::create_log_bundle_zip_with_extras`].
    #[cfg_attr(not(test), allow(dead_code))]
    fn create_log_bundle_zip(&self) -> Result<PathBuf> {
        self.create_log_bundle_zip_with_extras(&LogBundleExtras::default())
    }

    /// Bundles this state's logs plus caller-provided `extras` into a
    /// timestamped zip in the state's log directory.
    fn create_log_bundle_zip_with_extras(&self, extras: &LogBundleExtras) -> Result<PathBuf> {
        let logfile_stem = self
            .logfile_name
            .strip_suffix(".log")
            .unwrap_or(&self.logfile_name);

        let zip_path = self.log_directory.join(format!(
            "{logfile_stem}-{}.zip",
            Local::now().format("%Y%m%d-%H%M%S")
        ));
        if zip_path.exists() {
            let error_message = format!(
                "New log zip path conflicts with an existing zip: {}",
                zip_path.display()
            );
            return Err(anyhow::anyhow!("{error_message}"));
        }

        let log_files = collect_log_paths_in(&self.log_directory, &self.logfile_name)?;
        write_log_bundle_zip_inner(&zip_path, extras, &log_files)?;
        Ok(zip_path)
    }
}

static LOG_STATE: OnceLock<LogState> = OnceLock::new();

/// Formats a log record to be output to the terminal.
fn format_for_terminal_output(
    buf: &mut env_logger::fmt::Formatter,
    record: &log::Record,
) -> std::io::Result<()> {
    let level = record.level();
    let mut level_style = buf.default_level_style(record.level());
    // Adjust colors to match what we're used to from simplelog.
    match &level {
        log::Level::Info => {
            level_style.set_color(env_logger::fmt::Color::Blue);
        }
        log::Level::Debug => {
            level_style.set_color(env_logger::fmt::Color::Green);
        }
        _ => {}
    }
    let level = level_style.value(format!("[{level}]"));

    let mut target_style = buf.style();
    let target = if cfg!(debug_assertions) {
        target_style.set_dimmed(true);
        target_style.value(format!("[{}] ", record.target()))
    } else {
        target_style.value(String::default())
    };

    let time = chrono::Local::now();
    writeln!(
        buf,
        "{} {level} {target}{}",
        time.format("%H:%M:%S%.3f"),
        record.args()
    )
}

/// Formats a log record to be output to a file.
fn format_for_file_output(
    buf: &mut env_logger::fmt::Formatter,
    record: &log::Record,
) -> std::io::Result<()> {
    let target = if cfg!(debug_assertions) {
        format!("[{}] ", record.target())
    } else {
        String::default()
    };

    writeln!(
        buf,
        "{} [{}] {}{}",
        buf.timestamp(),
        record.level(),
        target,
        record.args()
    )
}

/// Handles the crash recovery process being killed by removing the crash recovery process log file
/// (which is stored in a temp directory and only persisted if the crash recovery process actually
/// handled a crash in the parent process).
pub fn on_crash_recovery_process_killed() {
    let config = LOG_STATE.get().expect("Logging not initialized");
    if !config.use_logfile {
        return;
    }

    let _ = fs::remove_file(crash_recovery_process_log_file_path(
        &config.log_directory,
        &config.logfile_name,
    ));
}

/// Handles the crash recovery process "recovering" from a parent crash by:
/// 1) Renaming the log file from the main process (which just panicked) to `warp.log.old.temp`.
/// 2) Moving the crash recovery process log (which is located at `warp.log.recovery`) to the usual
///    path warp logs are located (log_directory/warp.log).
///    The temp log file (`warp.log.old.temp`) will ultimately be rotated to `warp.log.old.0` the next
///    time [`rotate_log_files`] is called (which will get called when the event loop starts and we
///    have access to the `AppContext`)
pub fn on_parent_process_crash() {
    let config = LOG_STATE.get().expect("Logging not initialized");
    if !config.use_logfile {
        return;
    }

    let main_log_path = main_process_log_file_path(&config.log_directory, &config.logfile_name);
    let temp_path = temp_log_file_path(&config.log_directory, &config.logfile_name);

    let _ = fs::rename(&main_log_path, temp_path);

    let _ = fs::rename(
        crash_recovery_process_log_file_path(&config.log_directory, &config.logfile_name),
        main_log_path,
    );
}

/// Rotates the log and telemetry files, such that:
/// - Each file stores the logs of a single execution.
/// - The .old files store the previous executions, with larger suffixes indicating older executions.
pub async fn rotate_log_files() {
    let config = LOG_STATE.get().expect("Logging not initialized");
    if !config.use_logfile {
        return;
    }

    let max_rotation = config.max_rotation;

    if let Err(err) = rotate_files(&config.logfile_name, max_rotation).await {
        log::error!("Failed to rotate log files: {err:?}");
    }
}

pub async fn rotate_files(channel_file_name: &str, max_rotation: usize) -> Result<()> {
    let log_directory = match log_directory() {
        Ok(log_directory) => log_directory,
        Err(err) => {
            return Err(anyhow::anyhow!("Could not get log directory {err:?}"));
        }
    };

    // Delete the oldest log file (and any nested .in_session.M chunks that
    // belonged to that oldest startup-rotation slot).
    let largest_log_file_suffix = max_rotation.saturating_sub(1);
    let _ = fs::remove_file(
        log_directory.join(format!("{channel_file_name}.old.{largest_log_file_suffix}")),
    );
    remove_old_session_in_session_chunks(
        &log_directory,
        channel_file_name,
        largest_log_file_suffix,
    );

    // Rotate the .old.N startup-rotation slots, and along with each one any
    // nested `<name>.log.old.{N}.in_session.M` chunks left by the session
    // that produced the .old.N slot. Nested chunks shift with their parent
    // so they stay associated with the same logical session.
    for file_no in (0..largest_log_file_suffix).rev() {
        let old_file_path = log_directory.join(format!("{channel_file_name}.old.{file_no}"));
        let new_file_path = log_directory.join(format!("{channel_file_name}.old.{}", file_no + 1));
        let _ = fs::rename(old_file_path, new_file_path);

        shift_old_session_in_session_chunks(&log_directory, channel_file_name, file_no);
    }

    // Migrate the previous session's `<name>.log.in_session.M` files into
    // the `<name>.log.old.0.in_session.M` namespace, so the next session
    // opens with a clean `.in_session.*` window. The active log it produced
    // is renamed below from `.log.old.temp` to `.log.old.0`, so this naming
    // co-locates each old session's final state with its mid-session chunks.
    migrate_previous_session_in_session_chunks(&log_directory, channel_file_name);

    // Rename `warp.log.old.temp` (the temporary file) to `warp.log.old.0`.
    let temp_file_path = temp_log_file_path(&log_directory, channel_file_name);

    let _ = fs::rename(
        temp_file_path,
        log_directory.join(format!("{channel_file_name}.old.0")),
    );

    Ok(())
}

/// Remove every `<channel_file_name>.old.{slot_index}.in_session.M` file.
/// Called when an entire `.old.{slot_index}` slot is being discarded so its
/// nested mid-session chunks are discarded alongside it.
fn remove_old_session_in_session_chunks(
    log_directory: &Path,
    channel_file_name: &str,
    slot_index: usize,
) {
    let prefix = format!("{channel_file_name}.old.{slot_index}.in_session.");
    let Ok(read_dir) = fs::read_dir(log_directory) else {
        return;
    };
    for entry in read_dir.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(rest) = name.strip_prefix(&prefix)
            && rest.parse::<usize>().is_ok()
        {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Rename `<channel_file_name>.old.{from}.in_session.M` files to
/// `<channel_file_name>.old.{from+1}.in_session.M`, shifting a previous
/// session's nested chunks one slot older alongside their parent `.old.N`.
fn shift_old_session_in_session_chunks(log_directory: &Path, channel_file_name: &str, from: usize) {
    let prefix = format!("{channel_file_name}.old.{from}.in_session.");
    let Ok(read_dir) = fs::read_dir(log_directory) else {
        return;
    };
    for entry in read_dir.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(rest) = name.strip_prefix(&prefix)
            && let Ok(chunk_index) = rest.parse::<usize>()
        {
            let new_path = log_directory.join(format!(
                "{channel_file_name}.old.{}.in_session.{chunk_index}",
                from + 1
            ));
            let _ = fs::rename(path, new_path);
        }
    }
}

/// Rename the previous session's `<channel_file_name>.in_session.M` files
/// into `<channel_file_name>.old.0.in_session.M`. Co-locates each old
/// session's mid-session chunks with the `.old.0` slot that holds its
/// final-state log, and frees the `.in_session.*` namespace for the new
/// session that just started.
fn migrate_previous_session_in_session_chunks(log_directory: &Path, channel_file_name: &str) {
    let prefix = format!("{channel_file_name}.in_session.");
    let Ok(read_dir) = fs::read_dir(log_directory) else {
        return;
    };
    for entry in read_dir.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(rest) = name.strip_prefix(&prefix)
            && let Ok(chunk_index) = rest.parse::<usize>()
        {
            let new_path =
                log_directory.join(format!("{channel_file_name}.old.0.in_session.{chunk_index}"));
            let _ = fs::rename(path, new_path);
        }
    }
}

/// Initializes the logger for the crash recovery process.
pub fn init_for_crash_recovery_process() -> Result<()> {
    init_internal(
        true, /* is_from_crash_recovery_process */
        LogFrontend::Gui,
        None, /* log_destination */
        None, /* max_file_size_bytes — crash recovery uses its own short-lived log */
    )
}

/// Initializes the global logger for the application.
/// If `config.log_destination` is `Some`, always use the specified destination regardless of
/// environment. The frontend selects the log subdirectory and rotation policy.
pub fn init(config: LogConfig) -> Result<()> {
    init_internal(
        false, /* is_from_crash_recovery_process */
        config.frontend,
        config.log_destination,
        config.max_file_size_bytes,
    )
}

/// Return the path to the log file that is used within the crash recovery process.
/// We use a separate log file for the crash recovery process. If the crash
/// recovery process handles a crash, we'll move the crash recovery process log file to its usual
/// location at `log_directory/warp.log`.
fn crash_recovery_process_log_file_path(
    log_directory: impl AsRef<Path>,
    logfile_name: &str,
) -> PathBuf {
    log_directory
        .as_ref()
        .join(format!("{logfile_name}.recovery"))
}

/// Returns the path to the main process's log file.
fn main_process_log_file_path(log_directory: impl AsRef<Path>, logfile_name: &str) -> PathBuf {
    log_directory.as_ref().join(logfile_name)
}

/// Returns the path to the current execution's main log file.
///
/// Note: logging must be initialized before calling this function, otherwise this will
/// return an error.
pub fn log_file_path() -> Result<PathBuf> {
    let state = LOG_STATE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Logging not initialized"))?;
    Ok(state.log_file_path())
}

/// Collects paths to the current warp instance's log file and any older
/// log files (up to 6 retained, all potentially useful for debugging).
///
/// Returned ordering is newest-first, grouped by session:
///
/// - The active `<name>.log` (current session's most recent writes).
/// - `<name>.log.in_session.N` files produced by mid-session size rotation
///   of the current session, sorted by index (`.in_session.0` is the most
///   recent rotation).
/// - For each previous-startup slot `K = 0..max_rotation`, in order:
///   `<name>.log.old.K` (that session's final-state log) immediately
///   followed by its `<name>.log.old.K.in_session.N` chunks, sorted by N.
///
/// Scans the resolved log directory for files belonging to `logfile_name`.
fn collect_log_paths_in(log_directory: &Path, logfile_name: &str) -> Result<Vec<PathBuf>> {
    let current_log_path = log_directory.join(logfile_name);
    let in_session_prefix = format!("{logfile_name}.in_session.");
    let old_prefix = format!("{logfile_name}.old.");

    // Current session's mid-session rotation slots: <name>.log.in_session.N.
    let mut current_in_session: Vec<(usize, PathBuf)> = Vec::new();
    // Previous-startup final logs: <name>.log.old.K.
    let mut old_logs: Vec<(usize, PathBuf)> = Vec::new();
    // Previous sessions' nested mid-session chunks: <name>.log.old.K.in_session.M.
    // Keyed by (K, M) so each K's chunks group together with their .old.K parent.
    let mut old_nested: Vec<(usize, usize, PathBuf)> = Vec::new();

    for entry in fs::read_dir(log_directory)?.filter_map(Result::ok) {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(suffix) = file_name.strip_prefix(&in_session_prefix) {
            if let Ok(index) = suffix.parse::<usize>() {
                current_in_session.push((index, path));
            }
        } else if let Some(suffix) = file_name.strip_prefix(&old_prefix) {
            // suffix can be either `K` (an old log) or `K.in_session.M`
            // (a previous session's nested mid-rotation chunk).
            if let Ok(index) = suffix.parse::<usize>() {
                old_logs.push((index, path));
            } else if let Some((slot_str, chunk_str)) = suffix.split_once(".in_session.")
                && let (Ok(slot), Ok(chunk)) =
                    (slot_str.parse::<usize>(), chunk_str.parse::<usize>())
            {
                old_nested.push((slot, chunk, path));
            }
        }
    }
    current_in_session.sort_by_key(|(index, _)| *index);
    old_logs.sort_by_key(|(index, _)| *index);
    old_nested.sort_by_key(|(slot, chunk, _)| (*slot, *chunk));

    let mut files = Vec::new();
    if current_log_path.is_file() {
        files.push(current_log_path);
    }
    files.extend(
        current_in_session
            .into_iter()
            .map(|(_, path)| path)
            .filter(|path| path.is_file()),
    );

    // Interleave each .old.K with its nested .old.K.in_session.M chunks so
    // a session's final state is immediately followed by that session's
    // mid-session chunks before the next-older session begins.
    let mut nested_iter = old_nested.into_iter().peekable();
    for (slot, old_path) in old_logs {
        if old_path.is_file() {
            files.push(old_path);
        }
        while let Some((nslot, _, _)) = nested_iter.peek() {
            if *nslot != slot {
                break;
            }
            let (_, _, npath) = nested_iter.next().expect("peek matched");
            if npath.is_file() {
                files.push(npath);
            }
        }
    }
    // Any nested chunks whose parent .old.K is missing on disk still get
    // included after their slot has been skipped above — they show up here
    // grouped by (slot, chunk) ordering since they were never paired.
    for (_, _, npath) in nested_iter {
        if npath.is_file() {
            files.push(npath);
        }
    }

    if files.is_empty() {
        return Err(anyhow::anyhow!("No warp logs were found for {logfile_name}"));
    }

    Ok(files)
}

/// Extra content to bundle into the log zip, collected and passed in by the caller.
///
/// `warp_logging` itself only knows about the main log file; diagnostic
/// summaries, other subsystems' (e.g. MCP) log paths, autoupdate logs, etc.
/// are all collected and passed in via this struct by the `app` layer, avoiding
/// a reverse dependency of this crate on upper-layer modules.
#[derive(Debug, Default)]
pub struct LogBundleExtras {
    /// Extra on-disk files to bundle into the zip as-is; missing files are silently skipped.
    pub extra_files: Vec<ExtraFile>,
    /// Virtual files written into the zip directly from an in-memory string (e.g. `manifest.txt`).
    pub inline_files: Vec<InlineFile>,
}

/// Description of an extra on-disk file to bundle.
#[derive(Debug)]
pub struct ExtraFile {
    /// The real on-disk path.
    pub source_path: PathBuf,
    /// The relative path to save it as inside the zip (supports subdirectories, e.g. `mcp/<uuid>.log`).
    pub entry_name: String,
}

/// A virtual file written into the zip from in-memory content.
#[derive(Debug)]
pub struct InlineFile {
    /// The relative path to save it as inside the zip.
    pub entry_name: String,
    /// The file content (UTF-8).
    pub contents: String,
}

/// Default zip filename (used for the "export to log directory" flow, and as
/// the default filename for the save-file picker). Looks like `zap-20260518-093000.zip`.
pub fn default_log_bundle_filename() -> String {
    let logfile_name = ChannelState::logfile_name();
    let logfile_stem = logfile_name.strip_suffix(".log").unwrap_or(&logfile_name);
    format!(
        "{logfile_stem}-{}.zip",
        Local::now().format("%Y%m%d-%H%M%S")
    )
}

/// Normalizes the caller-provided entry name into a safe relative path inside
/// the zip, defending against path traversal:
/// - Backslashes are unified to `/`;
/// - Absolute paths / Windows drive letters are rejected;
/// - `..` parent components and consecutive `/` / `.` are stripped;
/// - An empty string is treated as invalid.
///
/// Returns `None` to mean this entry should be skipped (the caller will `log::warn!` and continue).
fn sanitize_zip_entry_name(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    // Unify separators.
    let normalized = name.replace('\\', "/");

    // Check for a Windows drive letter, e.g. `C:/foo`.
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let mut parts: Vec<&str> = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => continue, // Consecutive `/` or `./`, discard.
            ".." => return None,  // Not allowed to break out.
            other => parts.push(other),
        }
    }

    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// The core implementation that actually writes `log_files` + `extras` to the
/// given zip output path. The public `create_log_bundle_zip` and
/// `write_log_bundle_zip_to` both delegate here.
fn write_log_bundle_zip_inner(
    zip_path: &Path,
    extras: &LogBundleExtras,
    log_files: &[PathBuf],
) -> Result<()> {
    let zip_file = File::create(zip_path)?;
    let mut zip_writer = ZipWriter::new(zip_file);
    let zip_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // Main log + in-session + rotated old logs, flattened into the zip's root directory.
    for log_file in log_files {
        let entry_name = log_file
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid log file name: {}", log_file.display()))?;
        zip_writer.start_file(entry_name, zip_options)?;

        let mut source = File::open(log_file)?;
        copy(&mut source, &mut zip_writer)?;
    }

    // Extra on-disk files: bundled if present; missing/read failures only print a warning without affecting the main flow.
    for extra in &extras.extra_files {
        if !extra.source_path.is_file() {
            continue;
        }
        let source_display = extra.source_path.display();
        let Some(safe_entry) = sanitize_zip_entry_name(&extra.entry_name) else {
            let raw = &extra.entry_name;
            log::warn!("Skipping extra log file {source_display}: invalid zip entry name {raw:?}");
            continue;
        };
        match File::open(&extra.source_path) {
            Ok(mut source) => {
                if let Err(err) = zip_writer.start_file(&safe_entry, zip_options) {
                    log::warn!("Skipping extra log file {source_display} in bundle: {err}");
                    continue;
                }
                if let Err(err) = copy(&mut source, &mut zip_writer) {
                    log::warn!(
                        "Failed to write extra log file {source_display} into bundle: {err}"
                    );
                }
            }
            Err(err) => {
                log::warn!("Failed to open extra log file {source_display} for bundle: {err}");
            }
        }
    }

    // In-memory content (`manifest.txt`, etc.): always attempted.
    for inline in &extras.inline_files {
        let raw_name = &inline.entry_name;
        let Some(safe_entry) = sanitize_zip_entry_name(raw_name) else {
            log::warn!("Skipping inline entry: invalid zip entry name {raw_name:?}");
            continue;
        };
        if let Err(err) = zip_writer.start_file(&safe_entry, zip_options) {
            log::warn!("Failed to start inline entry {safe_entry} in bundle: {err}");
            continue;
        }
        if let Err(err) = zip_writer.write_all(inline.contents.as_bytes()) {
            log::warn!("Failed to write inline entry {safe_entry} into bundle: {err}");
        }
    }

    zip_writer.finish()?;
    Ok(())
}

/// Creates a timestamped zip archive containing the current log file
/// and any older logs for the active instance, written into the active
/// log directory. Returns the resulting zip path.
///
/// Used as the entry point for "bundle then show in file manager" (Help menu → View Zap Logs).
///
/// `extras` lets the caller append other diagnostic artifacts (MCP logs,
/// autoupdate logs, diagnostic summaries, etc.); any missing or unreadable
/// extra file is skipped and recorded via `log::warn!`, without failing the
/// whole export.
pub fn create_log_bundle_zip(extras: LogBundleExtras) -> Result<PathBuf> {
    let state = LOG_STATE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Logging not initialized"))?;
    state.create_log_bundle_zip_with_extras(&extras)
}

/// Writes a log bundle zip directly to `output_path` (overwriting if it
/// already exists, mirroring the save-file picker contract).
///
/// Used as the entry point for "user chooses a path in the save dialog" (Settings → About → Export Logs).
/// Shares the same bundled content and failure-tolerance policy as `create_log_bundle_zip`.
pub fn write_log_bundle_zip_to(
    output_path: impl AsRef<Path>,
    extras: LogBundleExtras,
) -> Result<()> {
    let state = LOG_STATE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Logging not initialized"))?;
    let log_files = collect_log_paths_in(&state.log_directory, &state.logfile_name)?;
    write_log_bundle_zip_inner(output_path.as_ref(), &extras, &log_files)
}

fn temp_log_file_path(log_directory: impl AsRef<Path>, channel_logfile_name: &str) -> PathBuf {
    log_directory
        .as_ref()
        .join(format!("{channel_logfile_name}.{TEMP_LOG_FILE_SUFFIX}"))
}

fn init_internal(
    is_from_crash_recovery_process: bool,
    frontend: LogFrontend,
    log_destination: Option<LogDestination>,
    max_file_size_bytes: Option<u64>,
) -> Result<()> {
    /// Returns an empty file named `warp.log` to log the current execution, and
    /// renames the previous execution's log to a temporary name.
    fn setup_log_files_for_current_execution(
        log_directory: &Path,
        logfile_name: &str,
        is_from_crash_recovery_process: bool,
    ) -> Result<File> {
        fs::create_dir_all(log_directory)?;

        let main_log_path = if is_from_crash_recovery_process {
            // Use a temporary file for logs within the crash recovery process. We intentionally do
            // not rename the old main log file to `warp.log.temp` like we do below because this
            // would result in us moving the log file of the parent process.
            crash_recovery_process_log_file_path(log_directory, logfile_name)
        } else {
            let main_log_path = main_process_log_file_path(log_directory, logfile_name);

            // Rename the old main log file to `warp.log.temp`.
            // We rotate the log files later in the background to make fewer blocking calls.
            let _ = fs::rename(
                main_log_path.clone(),
                temp_log_file_path(log_directory, logfile_name),
            );
            main_log_path
        };

        let main_log_file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(main_log_path)?;
        Ok(main_log_file)
    }

    let mut base_logger = env_logger::builder();

    base_logger.filter_level(LevelFilter::Info);

    // Only include `WARN` or higher logs for wgpu. By default, wgpu outputs logs at the `INFO`
    // level multiple times _per_ frame. See https://github.com/gfx-rs/wgpu/issues/3206.
    // Naga is overly noisy at `DEBUG`, so increase to `INFO`.
    base_logger
        .filter(Some("naga"), LevelFilter::Info)
        .filter(Some("wgpu_core"), LevelFilter::Warn)
        // Since we always pair an insertion with a deletion to avoid duplicate,
        // tantivy will log a lot of warnings for deleting a non-existing doc.
        .filter(Some("tantivy"), LevelFilter::Error)
        .filter(
            Some("wgpu_hal"),
            // On Windows with the DX12 backend, wgpu_hal outputs a ton of WARN-level logs.
            if cfg!(windows) {
                LevelFilter::Error
            } else {
                LevelFilter::Warn
            },
        );
    base_logger.parse_default_env();

    let stdout_is_a_tty = std::io::stdout().is_terminal();
    let in_ci = env::var("CI").is_ok();
    let integration_test = env::var("WARP_INTEGRATION").is_ok();
    let use_logfile = match log_destination {
        Some(LogDestination::File) => true,
        Some(LogDestination::Stderr) => false,
        None => !stdout_is_a_tty && !in_ci && !integration_test,
    };

    let state = LogState::new(
        use_logfile,
        init_log_directory()?,
        ChannelState::logfile_name().into_owned(),
        frontend,
    );
    if use_logfile {
        let file = setup_log_files_for_current_execution(
            &state.log_directory,
            &state.logfile_name,
            is_from_crash_recovery_process,
        )?;
        // Crash-recovery logs are short-lived (the file is renamed into place
        // by the parent on crash, and otherwise deleted on clean exit), so
        // skip in-session rotation for them — `max_file_size_bytes` only
        // applies to the main process's `warp.log`.
        let target: Box<dyn std::io::Write + Send + 'static> = if is_from_crash_recovery_process {
            Box::new(file)
        } else {
            crate::rotation::wrap_for_rotation(
                file,
                &state.log_directory,
                &state.logfile_name,
                max_file_size_bytes,
                state.max_rotation,
            )?
        };
        base_logger.target(env_logger::Target::Pipe(target));
        base_logger.format(format_for_file_output);
    } else {
        // Agent mode eval outputs are written to stdout but redirected to a file, so we don't want terminal styling.
        if cfg!(feature = "agent_mode_evals") {
            base_logger.write_style(env_logger::WriteStyle::Never);
        } else {
            base_logger.write_style(env_logger::WriteStyle::Always);
        }
        base_logger.format(format_for_terminal_output);
    }

    base_logger.init();

    // If we're logging to a file, initialize the `log_panics` crate, which
    // will install a panic hook that writes out panics using `log::error`.
    if use_logfile {
        log_panics::init();
    }

    LOG_STATE.set(state).expect("Logging already initialized");
    // We can .expect here because .init would have already panicked if we initialized logging twice.

    Ok(())
}

pub fn log_directory() -> Result<std::path::PathBuf> {
    LOG_STATE
        .get()
        .map(|config| config.log_directory.clone())
        .ok_or_else(|| anyhow::anyhow!("Logging not initialized"))
}

fn init_log_directory() -> Result<std::path::PathBuf> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            Ok(dirs::home_dir()
                .ok_or_else(|| {
                    anyhow::anyhow!("could not locate home directory in order to create a log file")
                })?
                .join("Library/Logs/"))
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            Ok(warp_core::paths::state_dir())
        } else if #[cfg(windows)] {
            Ok(warp_core::paths::state_dir().join(warp_core::paths::WARP_LOGS_DIR))
        } else {
            Err(anyhow::anyhow!("Have not configured file-based logging for the current platform!"))
        }
    }
}

/// Initializes the logger before running tests.
///
/// Additionally, we must not write anything to stdout in this function, as it
/// can interfere with test harnesses collecting the set of tests to run.  (This
/// is why we're not simply calling the init() function above.)
pub fn init_logging_for_unit_tests() {
    let inner = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Info)
        .write_style(env_logger::WriteStyle::Always)
        .parse_default_env()
        .format(format_for_terminal_output)
        .build();
    let max_level = inner.filter();
    // Installed as a tee rather than directly, so a test that needs to assert on
    // what a production call site actually emitted can tap it. Before this, the
    // `#[ctor]` in `app/src/lib.rs` claimed the global logger slot before `main`,
    // so `log::set_logger` in a test could only ever fail -- and a test that
    // reacted by skipping would pass while observing nothing.
    if log::set_boxed_logger(Box::new(TeeLogger { inner })).is_ok() {
        log::set_max_level(max_level);
        TEE_INSTALLED.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Receives every record the unit-test logger handles. Set once, for the life of
/// the test binary; see [`set_unit_test_log_tap`].
pub type UnitTestLogTap = fn(&log::Record<'_>);

static UNIT_TEST_LOG_TAP: OnceLock<UnitTestLogTap> = OnceLock::new();

/// Routes every record the unit-test logger handles to `tap` as well as to the
/// terminal.
///
/// Returns `false` if a tap was already set, which a caller asserting on log
/// output should treat as a hard error rather than a reason to skip: two taps
/// would each see a partial stream.
pub fn set_unit_test_log_tap(tap: UnitTestLogTap) -> bool {
    UNIT_TEST_LOG_TAP.set(tap).is_ok()
}

/// Whether the unit-test tee is the installed logger, i.e. whether
/// [`set_unit_test_log_tap`] can observe anything.
pub fn unit_test_log_tap_is_available() -> bool {
    TEE_INSTALLED.load(std::sync::atomic::Ordering::SeqCst)
}

static TEE_INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct TeeLogger {
    inner: env_logger::Logger,
}

impl log::Log for TeeLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.inner.enabled(metadata) || UNIT_TEST_LOG_TAP.get().is_some()
    }

    fn log(&self, record: &log::Record<'_>) {
        if let Some(tap) = UNIT_TEST_LOG_TAP.get() {
            tap(record);
        }
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
