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

/// Repo metadata: `NavigatedToDirectory`, `LoadRepoMetadataDirectory` and the file-tree pushes
/// (`RepoMetadataSnapshot`, `RepoMetadataUpdatePush`). The helper implements it (Wave H, P3).
pub const REPO_METADATA: &str = "repo_metadata";
/// Buffer sync: `OpenBuffer`, `BufferEdit`, `SaveBuffer`, `ResolveConflict`, `CloseBuffer`.
pub const BUFFERS: &str = "buffers";
/// Diff-state subscriptions and the git status and GitHub info pushes.
pub const DIFF_STATE: &str = "diff_state";
/// Git write operations: commit chain, push, pull, create PR, discard, stage.
pub const GIT_WRITE: &str = "git_write";
/// Batch `ReadFileContext` (the agent file reader).
pub const FILE_CONTEXT: &str = "file_context";
/// Remote codebase indexing and search.
pub const CODEBASE_INDEX: &str = "codebase_index";

/// Aggregate git status pushes (`UpdateGitStatus` and the `GitStatusPush` the daemon sends back).
pub const GIT_STATUS: &str = "git_status";
/// GitHub PR and repository info (`UpdateGitHubPrInfo`, `UpdateGitHubRepoInfo` and their pushes).
pub const GITHUB: &str = "github";
/// The agent-context snapshot push: the host's skills and global rule files. This is the one
/// name for what the earlier `SKILLS` constant (never referenced outside this module) meant;
/// bundled skills are never part of it, the app supplies its own.
pub const AGENT_CONTEXT: &str = "agent_context";
/// Codebase chunking for indexing: the host scans, chunks and hashes, the app embeds. Distinct
/// from the legacy [`CODEBASE_INDEX`] (host-side embedding), which the helper never implements.
pub const CODEBASE_CHUNKS: &str = "codebase_chunks";

/// The families above that only the legacy full daemon implements, for documentation and for
/// tests that pin the helper's list against them.
pub const LEGACY_ONLY_CAPABILITIES: &[&str] = &[
    BUFFERS,
    DIFF_STATE,
    GIT_WRITE,
    CODEBASE_INDEX,
    GIT_STATUS,
    GITHUB,
    AGENT_CONTEXT,
    CODEBASE_CHUNKS,
];

/// Every capability the helper in this build implements.
pub const HELPER_CAPABILITIES: &[&str] = &[
    FILE_WRITE,
    FS_BASIC,
    RUN_COMMAND,
    GIT_READ,
    RIPGREP_SEARCH,
    REPO_METADATA,
    FILE_CONTEXT,
];

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
    fn the_helper_does_not_claim_what_only_the_legacy_daemon_has() {
        for legacy_only in LEGACY_ONLY_CAPABILITIES {
            assert!(
                !HELPER_CAPABILITIES.contains(legacy_only),
                "{legacy_only} is not implemented by the helper"
            );
        }
    }

    #[test]
    fn the_new_family_capabilities_have_stable_wire_names_and_are_reserved() {
        assert_eq!(AGENT_CONTEXT, "agent_context");
        assert_eq!(CODEBASE_CHUNKS, "codebase_chunks");
        assert!(LEGACY_ONLY_CAPABILITIES.contains(&AGENT_CONTEXT));
        assert!(LEGACY_ONLY_CAPABILITIES.contains(&CODEBASE_CHUNKS));
    }

    #[test]
    fn the_helper_advertises_a_non_empty_list_so_it_is_never_mistaken_for_a_legacy_daemon() {
        assert!(!HELPER_CAPABILITIES.is_empty());
    }
}
