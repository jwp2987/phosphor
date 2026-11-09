//! Capability names the `phosphor-remote` helper advertises in `InitializeResponse`, and the
//! rule for reading them.
//!
//! An EMPTY list means the full app daemon of an older version, which supports every request.
//! The helper always sends a non-empty list, and answers every request outside it with
//! `ErrorCode::UNSUPPORTED`; the client then uses its no-helper path for that feature.

use crate::proto::InitializeResponse;

/// Whole-file `WriteFile` and `DeleteFile`.
pub const FILE_WRITE: &str = "file_write";
/// `ListDirectory`, `ResolvePath`, `CreateDirectory`, `ReadFileChunk`, `WriteFileChunk`.
pub const FS_BASIC: &str = "fs_basic";
/// `SessionBootstrapped` and `RunCommand`.
pub const RUN_COMMAND: &str = "run_command";
/// `GetBranches` and `GetCommittedBranchFiles`.
pub const GIT_READ: &str = "git_read";
/// `RipgrepSearch`.
pub const RIPGREP_SEARCH: &str = "ripgrep_search";

/// Every capability the helper in this build implements.
pub const HELPER_CAPABILITIES: &[&str] =
    &[FILE_WRITE, FS_BASIC, RUN_COMMAND, GIT_READ, RIPGREP_SEARCH];

/// What the server on the other end of a session supports.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ServerCapabilities {
    /// `None`: a legacy full daemon (no list sent), everything is supported.
    names: Option<Vec<String>>,
}

impl ServerCapabilities {
    /// Reads the list out of the handshake response.
    pub fn from_initialize_response(response: &InitializeResponse) -> Self {
        if response.capabilities.is_empty() {
            Self { names: None }
        } else {
            Self {
                names: Some(response.capabilities.clone()),
            }
        }
    }

    /// Whether `capability` is supported.
    pub fn supports(&self, capability: &str) -> bool {
        match &self.names {
            None => true,
            Some(names) => names.iter().any(|n| n == capability),
        }
    }

    /// True for the helper (a non-empty list), false for a legacy full daemon.
    pub fn is_limited(&self) -> bool {
        self.names.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_list_is_a_legacy_daemon_that_supports_everything() {
        let caps = ServerCapabilities::from_initialize_response(&InitializeResponse::default());
        assert!(!caps.is_limited());
        assert!(caps.supports(GIT_READ));
        assert!(caps.supports("anything_else"));
    }

    #[test]
    fn a_listed_server_supports_only_what_it_lists() {
        let response = InitializeResponse {
            capabilities: vec![FS_BASIC.to_string(), RUN_COMMAND.to_string()],
            ..Default::default()
        };
        let caps = ServerCapabilities::from_initialize_response(&response);
        assert!(caps.is_limited());
        assert!(caps.supports(FS_BASIC));
        assert!(!caps.supports(RIPGREP_SEARCH));
    }

    #[test]
    fn the_helper_advertises_a_non_empty_list_so_it_is_never_mistaken_for_a_legacy_daemon() {
        assert!(!HELPER_CAPABILITIES.is_empty());
    }
}
