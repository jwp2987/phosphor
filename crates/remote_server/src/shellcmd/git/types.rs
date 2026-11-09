//! Plain data shared by the git builders, parsers and algorithms.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Represents the status of a file in the git working directory.
/// This matches Git Desktop's AppFileStatusKind enum.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GitFileStatus {
    New,
    Modified,
    Deleted,
    Renamed { old_path: String },
    Copied { old_path: String },
    Untracked,
    Conflicted,
}

impl GitFileStatus {
    pub fn is_renamed(&self) -> bool {
        matches!(self, Self::Renamed { .. })
    }

    pub fn is_new_file(&self) -> bool {
        matches!(self, Self::New | Self::Untracked)
    }
}

impl TryFrom<&str> for GitFileStatus {
    type Error = anyhow::Error;

    fn try_from(status_code: &str) -> Result<Self, Self::Error> {
        match status_code {
            ".M" | "M." | "MM" => Ok(GitFileStatus::Modified),
            ".A" | "A." | "AM" => Ok(GitFileStatus::New),
            ".D" | "D." | "AD" => Ok(GitFileStatus::Deleted),
            _ => Ok(GitFileStatus::Modified), // Default fallback
        }
    }
}

/// Where a file's changes sit relative to the index (Zap #329).
///
/// Derived from the `XY` column of `git status --porcelain=v2`: `X` is the
/// index-vs-HEAD state and `Y` the worktree-vs-index state, so `X != '.'`
/// means "something is staged" and `Y != '.'` means "something is not".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStagedState {
    /// The whole diff for this path is in the index (`XY` = `X.`).
    Staged,
    /// Nothing for this path is in the index (`XY` = `.Y`, or untracked).
    Unstaged,
    /// Both columns are dirty: some of this file is staged and some is not.
    PartiallyStaged,
}

impl FileStagedState {
    /// Whether *any* part of this file is in the index.
    pub fn is_staged(&self) -> bool {
        matches!(self, Self::Staged | Self::PartiallyStaged)
    }

    /// Whether the file's whole diff is in the index. A partially staged file gets the *stage*
    /// action, because `git add` on it stages the remainder.
    pub fn is_fully_staged(&self) -> bool {
        matches!(self, Self::Staged)
    }

    /// Reads the `XY` status field of a `git status --porcelain=v2` changed or renamed entry.
    /// Anything that is not a two-column code (untracked, unmerged, malformed) is `Unstaged`.
    pub fn from_status_code(status_code: &str) -> Self {
        let mut chars = status_code.chars();
        let (Some(index), Some(worktree)) = (chars.next(), chars.next()) else {
            return Self::Unstaged;
        };
        match (index != '.', worktree != '.') {
            (true, true) => Self::PartiallyStaged,
            (true, false) => Self::Staged,
            (false, _) => Self::Unstaged,
        }
    }
}

/// What a single staging operation applies to (Zap #329).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageTarget {
    /// Whole files, by **repo-relative** path.
    Paths(Vec<String>),
    /// One reconstructed hunk patch, applied with `git apply --cached`.
    Hunk(String),
}

/// One stage/unstage operation, shared by the local backend, the remote backend and the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRequest {
    pub target: StageTarget,
    /// Un-stage rather than stage (`git restore --staged` / `git apply --cached --reverse`).
    pub unstage: bool,
}

/// One parsed `git status --porcelain=v2` entry, carrying the index state alongside the change
/// kind.
#[derive(Clone, Debug, PartialEq)]
pub struct GitStatusEntry {
    pub path: PathBuf,
    pub status: GitFileStatus,
    pub staged: FileStagedState,
}

/// Short summary of a commit: hash and subject line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub files_changed: usize,
    pub additions: usize,
    pub deletions: usize,
}

/// A single changed file with per-file addition/deletion counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeEntry {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
}

/// PR information returned by `gh pr view`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub draft: bool,
    pub base_branch: String,
}

/// Repository information returned by `gh repo view`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInfo {
    pub name: String,
    pub owner: Option<String>,
    /// The repository host (e.g. "github.com"), parsed from the repo URL.
    pub host: Option<String>,
}

/// What to run after the commit succeeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitChainMode {
    CommitOnly,
    CommitAndPush,
    CommitAndCreatePr,
}

/// `git diff --numstat` per-file counts keyed by path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NumStat {
    pub lines_added: usize,
    pub lines_removed: usize,
    pub is_binary_file: bool,
}

/// The `# branch.*` header lines of `git status --branch --porcelain=2`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BranchHeaders {
    /// `# branch.oid`: `None` for an unborn branch (`(initial)`).
    pub oid: Option<String>,
    /// `# branch.head`: `None` when detached (`(detached)`).
    pub head: Option<String>,
    /// `# branch.upstream`.
    pub upstream: Option<String>,
    /// `# branch.ab +A -B`: `None` when there is no upstream or it is gone.
    pub ahead_behind: Option<(u32, u32)>,
}

impl BranchHeaders {
    pub fn is_unborn(&self) -> bool {
        self.oid.is_none()
    }

    pub fn is_detached(&self) -> bool {
        self.head.is_none()
    }
}

/// One path to discard: the repo-relative path, and the old path when it is a rename.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardTarget {
    pub rel_path: String,
    pub renamed_from: Option<String>,
}
