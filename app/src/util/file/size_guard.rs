//! Refuses to load a huge file into the built-in code editor (#1329). The editor loads the whole
//! file into its rope and lays it out; a 40 MB text file took 30+ s and 5.4 GB of RSS (~130x the
//! file), with an empty editor shown meanwhile and the memory not returned afterwards.

use std::path::{Path, PathBuf};

/// Files larger than this are not opened in the built-in code editor.
pub const CODE_EDITOR_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Kill switch / override: `PHOSPHOR_EDITOR_MAX_FILE_MB=0` (or `off`) turns the guard off, any
/// other number replaces the 8 MB default.
const MAX_FILE_MB_ENV: &str = "PHOSPHOR_EDITOR_MAX_FILE_MB";

/// The effective limit in bytes for a `PHOSPHOR_EDITOR_MAX_FILE_MB` value; `None` disables it.
pub(crate) fn limit_from_env_value(value: Option<&str>) -> Option<u64> {
    match value.map(str::trim) {
        None | Some("") => Some(CODE_EDITOR_MAX_FILE_BYTES),
        Some(value) if value.eq_ignore_ascii_case("off") => None,
        Some(value) => match value.parse::<u64>() {
            Ok(0) => None,
            Ok(mb) => Some(mb.saturating_mul(1024 * 1024)),
            Err(_) => Some(CODE_EDITOR_MAX_FILE_BYTES),
        },
    }
}

fn limit() -> Option<u64> {
    static LIMIT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| limit_from_env_value(std::env::var(MAX_FILE_MB_ENV).ok().as_deref()))
}

/// Whether a file of `len` bytes is too large for the code editor under `limit`.
pub(crate) fn exceeds_limit(len: u64, limit: Option<u64>) -> bool {
    limit.is_some_and(|limit| len > limit)
}

/// The size in bytes of `path` if it is too large to open in the code editor.
pub fn too_large_for_code_editor(path: &Path) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    exceeds_limit(len, limit()).then_some(len)
}

/// Whether the guard is active (`PHOSPHOR_EDITOR_MAX_FILE_MB=0` turns it off).
pub fn enabled() -> bool {
    limit().is_some()
}

/// The paths among `paths` that are too large for the code editor, with their sizes. Does
/// blocking filesystem metadata calls: run it off the UI thread.
pub fn oversized_paths(paths: &[PathBuf]) -> Vec<(PathBuf, u64)> {
    paths
        .iter()
        .filter_map(|path| Some((path.clone(), too_large_for_code_editor(path)?)))
        .collect()
}

/// `extras` without the entries that appear in `oversized`.
pub(crate) fn without_oversized(extras: &[PathBuf], oversized: &[(PathBuf, u64)]) -> Vec<PathBuf> {
    extras
        .iter()
        .filter(|extra| !oversized.iter().any(|(path, _)| path == *extra))
        .cloned()
        .collect()
}

/// The message shown instead of opening a file of `len` bytes.
pub fn too_large_message(path: &Path, len: u64) -> String {
    format!(
        "{} is {:.1} MB, too large for the built-in editor. Open it with an external editor or a pager instead (set PHOSPHOR_EDITOR_MAX_FILE_MB to raise the limit).",
        path.display(),
        len as f64 / (1024.0 * 1024.0)
    )
}

#[cfg(test)]
#[path = "size_guard_tests.rs"]
mod tests;
