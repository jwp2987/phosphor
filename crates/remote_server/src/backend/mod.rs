//! One interface for every remote feature family, two ways to serve it, one place that picks.
//!
//! * [`RemoteFeatures`]: async methods per family over the EXISTING proto request/response types.
//! * [`HelperBackend`]: a pass-through over a connected [`RemoteServerClient`].
//! * [`ShellBackend`]: the same features by running ordinary commands on the host over an
//!   OUT-OF-BAND channel ([`OutOfBandExecutor`]). It never types into the user's pty (ruling R1):
//!   a session with no out-of-band channel reports the feature unavailable instead.
//! * [`FeatureRouter`]: the ONE decision point. Helper connected and it supports the family's
//!   capability (an empty list means a legacy full daemon: everything) -> helper; otherwise
//!   shell. A request the helper answers `UNSUPPORTED` is served by the shell from then on, as is
//!   everything after the connection is lost. Call sites never retry or fall back themselves.
//! * [`RefreshCoordinator`](refresh::RefreshCoordinator): when a shell-backed view re-reads state.
//!   Event driven, no timer.
//!
//! Wave H parity, package P0. The families' real shell implementations land in P1..P9; until
//! then [`ShellBackend`] answers every call `Unsupported` (or `Unavailable` without a channel).

mod executor;
mod features;
mod helper;
pub mod refresh;
mod router;
mod shell;
mod shell_git_runner;
mod shell_git_write;

#[cfg(test)]
mod contract_tests;

pub use executor::{
    ChannelKind, ExecError, ExecOutput, ExecRequest, MAX_EXEC_OUTPUT_BYTES, OutOfBandExecutor,
};
pub use features::{METHODS, RemoteFeatures};
pub use helper::HelperBackend;
pub use router::{AlwaysShell, BackendKind, FeatureRouter, HelperProbe, RoutedBackend, decide};
pub use shell::ShellBackend;
pub use shell_git_runner::ShellGitRunner;

use crate::capability;
use crate::client::ClientError;
use crate::proto::ErrorCode;
use crate::shellcmd::frame::FrameError;

/// A feature family: the unit the router decides for, tied to one capability name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// `WriteFile`, `DeleteFile`.
    FileWrite,
    /// `ListDirectory`, `ResolvePath`, `CreateDirectory`, chunked reads and writes.
    FsBasic,
    /// `SessionBootstrapped`, `RunCommand`.
    RunCommand,
    /// `GetBranches`, `GetCommittedBranchFiles`.
    GitRead,
    /// `RipgrepSearch`.
    RipgrepSearch,
    /// Batch `ReadFileContext`.
    FileContext,
    /// Repo metadata and the lazy file tree.
    RepoMetadata,
    /// Buffer sync.
    Buffers,
    /// Diff-state subscriptions.
    DiffState,
    /// Aggregate git status.
    GitStatus,
    /// GitHub PR and repo info.
    Github,
    /// Commit chain, push, pull, create PR, discard, stage.
    GitWrite,
    /// The host's skills and rule files.
    AgentContext,
    /// Scan/chunk/hash for indexing.
    CodebaseChunks,
    /// Legacy host-side indexing (full daemon only).
    CodebaseIndex,
}

impl Family {
    pub const ALL: &'static [Family] = &[
        Family::FileWrite,
        Family::FsBasic,
        Family::RunCommand,
        Family::GitRead,
        Family::RipgrepSearch,
        Family::FileContext,
        Family::RepoMetadata,
        Family::Buffers,
        Family::DiffState,
        Family::GitStatus,
        Family::Github,
        Family::GitWrite,
        Family::AgentContext,
        Family::CodebaseChunks,
        Family::CodebaseIndex,
    ];

    /// The capability a server must advertise to serve this family.
    pub const fn capability(self) -> &'static str {
        match self {
            Family::FileWrite => capability::FILE_WRITE,
            Family::FsBasic => capability::FS_BASIC,
            Family::RunCommand => capability::RUN_COMMAND,
            Family::GitRead => capability::GIT_READ,
            Family::RipgrepSearch => capability::RIPGREP_SEARCH,
            Family::FileContext => capability::FILE_CONTEXT,
            Family::RepoMetadata => capability::REPO_METADATA,
            Family::Buffers => capability::BUFFERS,
            Family::DiffState => capability::DIFF_STATE,
            Family::GitStatus => capability::GIT_STATUS,
            Family::Github => capability::GITHUB,
            Family::GitWrite => capability::GIT_WRITE,
            Family::AgentContext => capability::AGENT_CONTEXT,
            Family::CodebaseChunks => capability::CODEBASE_CHUNKS,
            Family::CodebaseIndex => capability::CODEBASE_INDEX,
        }
    }

    /// The family for a capability name (the form the always-shell override is written in).
    pub fn from_capability(name: &str) -> Option<Family> {
        Self::ALL.iter().copied().find(|f| f.capability() == name)
    }
}

/// Why a [`RemoteFeatures`] call failed.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The backend does not implement this request (the helper answered `UNSUPPORTED`, or the
    /// shell implementation has not landed yet).
    #[error("{what} is not supported in this mode")]
    Unsupported { family: Family, what: &'static str },
    /// The shell backend has no out-of-band channel for this session, or the channel is gone.
    /// The feature is unavailable on this session; nothing is ever typed into the user's pty.
    #[error("not available on this session: {0}")]
    Unavailable(String),
    /// The helper connection returned an error.
    #[error(transparent)]
    Client(#[from] ClientError),
    /// The out-of-band command could not be run, or ran over its output cap.
    #[error("remote command failed: {0}")]
    Exec(#[from] ExecError),
    /// The command ran but its framed output was unusable.
    #[error("unreadable remote output: {0}")]
    Frame(#[from] FrameError),
}

impl BackendError {
    /// The server (or the shell stub) said it does not do this. The request was NOT executed.
    pub fn is_unsupported(&self) -> bool {
        match self {
            BackendError::Unsupported { .. } => true,
            BackendError::Client(ClientError::ServerError { code, .. }) => {
                *code == ErrorCode::Unsupported
            }
            _ => false,
        }
    }

    /// The helper connection is gone. Whether the request ran is unknown.
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self,
            BackendError::Client(ClientError::Disconnected | ClientError::ResponseChannelClosed)
        )
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
