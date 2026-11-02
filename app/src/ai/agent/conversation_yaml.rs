use std::path::PathBuf;

const BASE_DIR_NAME: &str = "warp_conversation_search";

/// Returns the base directory for conversation search temp files.
///
/// Uses the platform temp directory so paths are fully qualified with
/// native separators on every OS (e.g. includes drive prefix on Windows).
pub(crate) fn base_dir() -> PathBuf {
    std::env::temp_dir().join(BASE_DIR_NAME)
}
