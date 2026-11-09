//! The code-review diff state, computed over a [`GitRunner`] and produced as the wire messages
//! (`DiffStateSnapshot`, `DiffStateFileDelta`) directly. The helper and the shell backend call the
//! same functions here, so a remote repository reads the same through either: same git argv, same
//! parsing, same size limits, same `content_at_base` budget.
//!
//! This is the runner-driven counterpart of the app's `LocalDiffStateModel` computation
//! (`app/src/code_review/diff_state.rs`) and of the old full daemon's `GetDiffState` encoding
//! (`app/src/remote_server/diff_state_proto.rs`). Where the old code touched the filesystem
//! (`is_dir`, `is_file`, counting an untracked file's lines) this asks git instead, because the
//! shell backend has no filesystem:
//!
//! * a directory entry in `git status` (a nested repository) is the one that ends in `/`;
//! * an untracked file's added-line count is `git diff --no-index --numstat`.
//!
//! The size constants mirror `app/src/code_review/diff_size_limits.rs` and must change together.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use futures::StreamExt as _;
use futures::stream;

use super::ops::{compute_unpushed_state, detect_current_branch, detect_main_branch};
use super::parse::{STATUS_FULL_ARGS, parse_name_status, parse_numstat_map, parse_status_entries};
use super::runner::{GitRunner, git_text};
use super::types::{FileStagedState, GitFileStatus, GitStatusEntry};
use crate::proto;

/// A diff larger than this is not rendered, and base content larger than this is not sent.
pub const MAX_DIFF_SIZE: usize = 4_375_000;
const MAX_REASONABLE_DIFF_SIZE: usize = 2_187_500;
const MAX_CHARACTERS_PER_LINE: usize = 5000;
const DIFF_LINE_RENDER_LIMIT: usize = 10_000;
const DELETION_LINE_RENDER_LIMIT: usize = 8000;

/// Git calls run at once per snapshot. Each is a round trip on the shell arm (the out-of-band
/// channel allows four in flight), and in-process on the helper, where it changes nothing.
const PARALLEL: usize = 4;

const BIDI_CHARS: [char; 9] = [
    '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}',
    '\u{2069}',
];

/// What a diff is taken against.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// The working tree against `HEAD` (uncommitted changes).
    Head,
    /// Against the merge base with the detected main branch.
    MainBranch,
    /// Against the merge base with the named branch.
    OtherBranch(String),
}

impl Mode {
    /// A missing or empty `DiffMode` is `Head`, like the old server.
    pub fn from_proto(mode: Option<&proto::DiffMode>) -> Mode {
        use proto::diff_mode::Mode as M;
        match mode.and_then(|m| m.mode.as_ref()) {
            Some(M::MainBranch(_)) => Mode::MainBranch,
            Some(M::OtherBranch(b)) => Mode::OtherBranch(b.branch_name.clone()),
            Some(M::Head(_)) | None => Mode::Head,
        }
    }

    pub fn to_proto(&self) -> proto::DiffMode {
        use proto::diff_mode::Mode as M;
        proto::DiffMode {
            mode: Some(match self {
                Mode::Head => M::Head(proto::DiffModeHead {}),
                Mode::MainBranch => M::MainBranch(proto::DiffModeMainBranch {}),
                Mode::OtherBranch(branch_name) => M::OtherBranch(proto::DiffModeOtherBranch {
                    branch_name: branch_name.clone(),
                }),
            }),
        }
    }

    /// Only the branch modes compare against a base branch.
    pub fn includes_base_branch(&self) -> bool {
        !matches!(self, Mode::Head)
    }
}

// ── Snapshot ─────────────────────────────────────────────────────────────────

/// The whole diff state of `repo` for `mode`, as the `GetDiffStateResponse` the old server sent.
/// `wire_repo_path` is echoed verbatim: the client keys its subscription on the string it sent.
///
/// Metadata that cannot be read is `NotInRepository`; metadata that reads but a diff that fails
/// is `Error("Failed to compute diff")`, exactly the old server's decision.
pub async fn compute_snapshot(
    runner: &dyn GitRunner,
    repo: &Path,
    wire_repo_path: &str,
    mode: &Mode,
) -> proto::DiffStateSnapshot {
    let metadata = load_metadata(runner, repo, mode.includes_base_branch()).await;
    let diffs = load_diff(runner, repo, mode).await;
    snapshot_from_parts(wire_repo_path, mode, metadata.ok(), diffs.ok())
}

/// Assembles a snapshot from its computed parts (see [`compute_snapshot`] for the rule).
pub fn snapshot_from_parts(
    wire_repo_path: &str,
    mode: &Mode,
    metadata: Option<proto::DiffMetadata>,
    diffs: Option<proto::GitDiffData>,
) -> proto::DiffStateSnapshot {
    use proto::diff_state::State;
    let (metadata, state, diffs) = match (metadata, diffs) {
        (None, _) => (
            proto::DiffMetadata::default(),
            State::NotInRepository(proto::DiffStateNotInRepository {}),
            None,
        ),
        (Some(metadata), None) => (
            metadata,
            State::Error(proto::DiffStateErrorValue {
                message: "Failed to compute diff".to_string(),
            }),
            None,
        ),
        (Some(metadata), Some(diffs)) => (
            metadata,
            State::Loaded(proto::DiffStateLoaded {}),
            Some(diffs),
        ),
    };
    proto::DiffStateSnapshot {
        repo_path: wire_repo_path.to_string(),
        mode: Some(mode.to_proto()),
        metadata: Some(metadata),
        state: Some(proto::DiffState { state: Some(state) }),
        diffs,
    }
}

/// `GetDiffStateResponse` carrying `snapshot`.
pub fn snapshot_response(snapshot: proto::DiffStateSnapshot) -> proto::GetDiffStateResponse {
    proto::GetDiffStateResponse {
        result: Some(proto::get_diff_state_response::Result::Snapshot(snapshot)),
    }
}

/// `GetDiffStateResponse` carrying an error message.
pub fn error_response(message: String) -> proto::GetDiffStateResponse {
    proto::GetDiffStateResponse {
        result: Some(proto::get_diff_state_response::Result::Error(
            proto::DiffStateError { message },
        )),
    }
}

// ── Metadata ─────────────────────────────────────────────────────────────────

/// The branch names, change counts, unpushed commits and upstream of `repo`.
///
/// `include_base_branch` adds the counts against the main branch's merge base. Unpushed commits
/// are always computed (the app gates only their display on a feature flag); they cost one
/// `git log`.
pub async fn load_metadata(
    runner: &dyn GitRunner,
    repo: &Path,
    include_base_branch: bool,
) -> Result<proto::DiffMetadata> {
    let main_branch_name = detect_main_branch(runner, repo).await?;
    let current_branch_name = detect_current_branch(runner, repo).await?;
    let has_head_commit = git_text(runner, repo, &["rev-parse", "HEAD"]).await.is_ok();
    let against_head = metadata_against_head(runner, repo).await?;
    let against_base_branch = if include_base_branch {
        Some(metadata_against_branch(runner, repo, &main_branch_name).await?)
    } else {
        None
    };
    let (unpushed, upstream_ref) = compute_unpushed_state(runner, repo).await;
    Ok(proto::DiffMetadata {
        main_branch_name,
        current_branch_name,
        against_head: Some(against_head),
        against_base_branch,
        has_head_commit,
        unpushed_commits: unpushed
            .iter()
            .map(|c| proto::Commit {
                hash: c.hash.clone(),
                subject: c.subject.clone(),
                files_changed: c.files_changed as u64,
                additions: c.additions as u64,
                deletions: c.deletions as u64,
                files: Vec::new(),
            })
            .collect(),
        upstream_ref,
        pr_info: None,
    })
}

fn base(stats: proto::DiffStats) -> proto::DiffMetadataAgainstBase {
    proto::DiffMetadataAgainstBase {
        aggregate_stats: Some(stats),
        files: Vec::new(),
    }
}

async fn metadata_against_head(
    runner: &dyn GitRunner,
    repo: &Path,
) -> Result<proto::DiffMetadataAgainstBase> {
    let status_output = git_text(runner, repo, &STATUS_FULL_ARGS).await?;
    let changed = parse_status_entries(&status_output)?;
    let numstat = numstat_against(runner, repo, "HEAD").await;
    Ok(base(stats_for(runner, repo, &changed, &numstat).await))
}

async fn metadata_against_branch(
    runner: &dyn GitRunner,
    repo: &Path,
    branch: &str,
) -> Result<proto::DiffMetadataAgainstBase> {
    let Ok(merge_base) = merge_base(runner, repo, branch).await else {
        log::warn!("Could not determine merge base against branch {branch}");
        return Ok(base(proto::DiffStats::default()));
    };
    let changed = files_against_base(runner, repo, &merge_base).await?;
    if changed.is_empty() {
        return Ok(base(proto::DiffStats::default()));
    }
    let entries: Vec<GitStatusEntry> = changed
        .into_iter()
        .map(|(path, status)| GitStatusEntry {
            path,
            status,
            staged: FileStagedState::Unstaged,
        })
        .collect();
    let numstat = numstat_against(runner, repo, &merge_base).await;
    Ok(base(stats_for(runner, repo, &entries, &numstat).await))
}

/// Sums the per-file counts. A file git's numstat does not list (untracked) is counted by
/// diffing it against nothing; one that cannot be counted (a nested repository, an unreadable
/// file) contributes 0 instead of failing the whole read.
async fn stats_for(
    runner: &dyn GitRunner,
    repo: &Path,
    changed: &[GitStatusEntry],
    numstat: &HashMap<PathBuf, super::types::NumStat>,
) -> proto::DiffStats {
    let (mut added, mut removed) = (0usize, 0usize);
    let mut untracked: Vec<&Path> = Vec::new();
    for entry in changed {
        if let Some(stat) = numstat.get(&entry.path) {
            added += stat.lines_added;
            removed += stat.lines_removed;
        } else if matches!(entry.status, GitFileStatus::Untracked) {
            untracked.push(&entry.path);
        }
    }
    // Collect the futures first: a closure kept inside the stream type makes the outer future
    // fail the higher-ranked Send check ("Send is not general enough").
    let futures: Vec<_> = untracked
        .into_iter()
        .map(|path| untracked_line_count(runner, repo, path))
        .collect();
    let counts: Vec<usize> = stream::iter(futures).buffered(PARALLEL).collect().await;
    added += counts.iter().sum::<usize>();
    proto::DiffStats {
        files_changed: changed.len() as u64,
        total_additions: added as u64,
        total_deletions: removed as u64,
    }
}

async fn untracked_line_count(runner: &dyn GitRunner, repo: &Path, path: &Path) -> usize {
    let Some(path) = path.to_str().filter(|p| !p.ends_with('/')) else {
        return 0;
    };
    let args = [
        "diff",
        "--no-ext-diff",
        "--numstat",
        "--no-index",
        "--",
        "/dev/null",
        path,
    ];
    match git_text(runner, repo, &args).await {
        Ok(out) => out
            .lines()
            .next()
            .and_then(|line| line.split('\t').next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0),
        Err(e) => {
            log::debug!("Could not count lines for untracked entry: {e}");
            0
        }
    }
}

async fn numstat_against(
    runner: &dyn GitRunner,
    repo: &Path,
    commit: &str,
) -> HashMap<PathBuf, super::types::NumStat> {
    match git_text(runner, repo, &["diff", "--numstat", commit]).await {
        Ok(out) => parse_numstat_map(&out),
        // If numstat fails there is nothing to count or to mark binary.
        Err(_) => HashMap::new(),
    }
}

// ── Files and diffs ──────────────────────────────────────────────────────────

async fn merge_base(runner: &dyn GitRunner, repo: &Path, branch: &str) -> Result<String> {
    let out = git_text(runner, repo, &["merge-base", "HEAD", branch]).await?;
    Ok(out.trim().to_string())
}

/// The merge base for a branch mode. `Head` has none.
pub async fn compute_merge_base(
    runner: &dyn GitRunner,
    repo: &Path,
    mode: &Mode,
) -> Result<String> {
    let branch = match mode {
        Mode::MainBranch => detect_main_branch(runner, repo).await?,
        Mode::OtherBranch(branch) => branch.clone(),
        Mode::Head => anyhow::bail!("merge base is not applicable for Head mode"),
    };
    merge_base(runner, repo, &branch).await
}

/// Tracked changes against `merge_base` plus untracked files.
async fn files_against_base(
    runner: &dyn GitRunner,
    repo: &Path,
    merge_base: &str,
) -> Result<Vec<(PathBuf, GitFileStatus)>> {
    let diff_output = git_text(runner, repo, &["diff", "--name-status", "-z", merge_base]).await?;
    let mut changed = if diff_output.trim().is_empty() {
        Vec::new()
    } else {
        parse_name_status(&diff_output)?
    };
    let status_output = git_text(
        runner,
        repo,
        &["status", "--untracked-files=all", "--porcelain=2", "-z"],
    )
    .await?;
    for entry in parse_status_entries(&status_output)? {
        if matches!(entry.status, GitFileStatus::Untracked) {
            changed.push((entry.path, entry.status));
        }
    }
    Ok(changed)
}

/// Every file's diff for `mode`, with the base content the client's editor decorates from.
pub async fn load_diff(
    runner: &dyn GitRunner,
    repo: &Path,
    mode: &Mode,
) -> Result<proto::GitDiffData> {
    let files = match mode {
        Mode::Head => diff_against_head(runner, repo).await?,
        Mode::MainBranch => {
            let main = detect_main_branch(runner, repo).await?;
            diff_against_branch(runner, repo, &main).await?
        }
        Mode::OtherBranch(branch) => diff_against_branch(runner, repo, branch).await?,
    };
    let (mut added, mut removed) = (0u64, 0u64);
    for file in &files {
        added += additions(file) as u64;
        removed += deletions(file) as u64;
    }
    Ok(proto::GitDiffData {
        files_changed: files.len() as u64,
        files,
        total_additions: added,
        total_deletions: removed,
    })
}

async fn diff_against_head(runner: &dyn GitRunner, repo: &Path) -> Result<Vec<proto::FileDiff>> {
    let status_output = git_text(runner, repo, &STATUS_FULL_ARGS).await?;
    let changed = parse_status_entries(&status_output)?;
    let binary = binary_files(runner, repo, "HEAD").await;
    let futures: Vec<_> = changed
        .into_iter()
        .map(|entry| {
            let is_binary = binary.contains(&entry.path);
            async move {
                let mut file = file_diff(
                    runner,
                    repo,
                    &entry.path,
                    &entry.status,
                    Some(entry.staged),
                    is_binary,
                    None,
                )
                .await?;
                let content = if is_binary {
                    None
                } else {
                    content_at_head(runner, repo, &entry.path, &entry.status).await
                };
                file.is_autogenerated = is_file_autogenerated(&entry.path, content.as_deref());
                Ok(finish(file, content))
            }
        })
        .collect();
    let results: Vec<Result<proto::FileDiff>> =
        stream::iter(futures).buffered(PARALLEL).collect().await;
    results.into_iter().collect()
}

async fn diff_against_branch(
    runner: &dyn GitRunner,
    repo: &Path,
    branch: &str,
) -> Result<Vec<proto::FileDiff>> {
    let merge_base = match merge_base(runner, repo, branch).await {
        Ok(mb) => mb,
        Err(err) => {
            log::warn!("Could not determine merge base against branch {branch}: {err:?}");
            return Ok(Vec::new());
        }
    };
    let changed = files_against_base(runner, repo, &merge_base).await?;
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let binary = binary_files(runner, repo, &merge_base).await;
    let merge_base = merge_base.as_str();
    let futures: Vec<_> = changed
        .into_iter()
        .map(|(path, status)| {
            let is_binary = binary.contains(&path);
            async move {
                // Base-branch mode has no index to be staged against.
                file_diff_for_path(
                    runner,
                    repo,
                    is_binary,
                    &path,
                    &status,
                    None,
                    Some(merge_base),
                )
                .await
            }
        })
        .collect();
    let results: Vec<Result<Option<proto::FileDiff>>> =
        stream::iter(futures).buffered(PARALLEL).collect().await;
    let mut files = Vec::new();
    for result in results {
        files.extend(result?);
    }
    Ok(files)
}

async fn binary_files(runner: &dyn GitRunner, repo: &Path, commit: &str) -> HashSet<PathBuf> {
    numstat_against(runner, repo, commit)
        .await
        .into_iter()
        .filter_map(|(path, stat)| stat.is_binary_file.then_some(path))
        .collect()
}

/// One file's diff with its base content, or `None` when it has no real change left. The
/// `merge_base` picks the base: `None` is `HEAD`.
async fn file_diff_for_path(
    runner: &dyn GitRunner,
    repo: &Path,
    is_binary: bool,
    path: &Path,
    status: &GitFileStatus,
    staged: Option<FileStagedState>,
    merge_base: Option<&str>,
) -> Result<Option<proto::FileDiff>> {
    let mut file = file_diff(runner, repo, path, status, staged, is_binary, merge_base).await?;
    // Files with no actual change (empty hunks, not binary) are skipped, except renames and new
    // files.
    if !is_binary
        && (file.hunks.is_empty() || (additions(&file) == 0 && deletions(&file) == 0))
        && !status.is_renamed()
        && !status.is_new_file()
    {
        return Ok(None);
    }
    let content = if is_binary {
        None
    } else {
        match merge_base {
            Some(base) => match status {
                // Not in the base: empty baseline, so the hunks read as all additions. A nested
                // repository directory gets none.
                GitFileStatus::New | GitFileStatus::Untracked => {
                    (!path.to_string_lossy().ends_with('/')).then(String::new)
                }
                GitFileStatus::Renamed { old_path } => {
                    content_at_commit(runner, repo, old_path, base).await
                }
                _ => content_at_commit(runner, repo, &path.to_string_lossy(), base).await,
            },
            None => content_at_head(runner, repo, path, status).await,
        }
    };
    file.is_autogenerated = is_file_autogenerated(path, content.as_deref());
    Ok(Some(finish(file, content)))
}

async fn content_at_head(
    runner: &dyn GitRunner,
    repo: &Path,
    path: &Path,
    status: &GitFileStatus,
) -> Option<String> {
    match status {
        GitFileStatus::Untracked | GitFileStatus::New => {
            (!path.to_string_lossy().ends_with('/')).then(String::new)
        }
        // At HEAD a renamed file still lives at its old path.
        GitFileStatus::Renamed { old_path } => {
            content_at_commit(runner, repo, old_path, "HEAD").await
        }
        _ => content_at_commit(runner, repo, path.to_str()?, "HEAD").await,
    }
}

async fn content_at_commit(
    runner: &dyn GitRunner,
    repo: &Path,
    path: &str,
    commit: &str,
) -> Option<String> {
    git_text(runner, repo, &["show", &format!("{commit}:{path}")])
        .await
        .ok()
}

/// Applies the wire rules to a computed file: binary files never carry base content, and base
/// content over the wire budget is withheld and the file marked "too large". Local rendering
/// keeps full content; this runs only where a diff is serialized for a remote client.
fn finish(mut file: proto::FileDiff, content: Option<String>) -> proto::FileDiff {
    if file.is_binary {
        file.content_at_base = None;
    } else if content.as_deref().is_some_and(|c| c.len() > MAX_DIFF_SIZE) {
        file.size = proto::DiffSize::UnrenderableFileTooLarge as i32;
        file.content_at_base = None;
    } else {
        file.content_at_base = content;
    }
    file
}

fn staged_to_proto(staged: Option<FileStagedState>) -> i32 {
    (match staged {
        None => proto::FileStagedState::Unspecified,
        Some(FileStagedState::Staged) => proto::FileStagedState::Staged,
        Some(FileStagedState::Unstaged) => proto::FileStagedState::Unstaged,
        Some(FileStagedState::PartiallyStaged) => proto::FileStagedState::PartiallyStaged,
    }) as i32
}

/// `GitFileStatus` on the wire.
pub fn status_to_proto(status: &GitFileStatus) -> proto::GitFileStatus {
    use proto::git_file_status::Status;
    proto::GitFileStatus {
        status: Some(match status {
            GitFileStatus::New => Status::NewFile(proto::GitFileStatusNew {}),
            GitFileStatus::Modified => Status::Modified(proto::GitFileStatusModified {}),
            GitFileStatus::Deleted => Status::Deleted(proto::GitFileStatusDeleted {}),
            GitFileStatus::Renamed { old_path } => Status::Renamed(proto::GitFileStatusRenamed {
                old_path: old_path.clone(),
            }),
            GitFileStatus::Copied { old_path } => Status::Copied(proto::GitFileStatusCopied {
                old_path: old_path.clone(),
            }),
            GitFileStatus::Untracked => Status::Untracked(proto::GitFileStatusUntracked {}),
            GitFileStatus::Conflicted => Status::Conflicted(proto::GitFileStatusConflicted {}),
        }),
    }
}

fn plain_file(
    path: &Path,
    status: &GitFileStatus,
    staged: Option<FileStagedState>,
    is_binary: bool,
    size: proto::DiffSize,
) -> proto::FileDiff {
    proto::FileDiff {
        file_path: path.to_string_lossy().into_owned(),
        status: Some(status_to_proto(status)),
        hunks: Vec::new(),
        is_binary,
        is_autogenerated: false,
        max_line_number: 0,
        has_hidden_bidi_chars: false,
        size: size as i32,
        content_at_base: None,
        staged: staged_to_proto(staged),
    }
}

/// The diff of one file against `commit` (or `HEAD`, or nothing for a new file).
async fn file_diff(
    runner: &dyn GitRunner,
    repo: &Path,
    path: &Path,
    status: &GitFileStatus,
    staged: Option<FileStagedState>,
    is_binary: bool,
    commit: Option<&str>,
) -> Result<proto::FileDiff> {
    use proto::DiffSize;
    if is_binary {
        return Ok(plain_file(path, status, staged, true, DiffSize::Normal));
    }
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid file path: contains invalid UTF-8"))?;
    // A directory entry (a nested repository checked out inside this one): no content to diff,
    // and `git diff --no-index` against a directory fails.
    if matches!(status, GitFileStatus::Untracked) && path_str.ends_with('/') {
        return Ok(plain_file(path, status, staged, false, DiffSize::Normal));
    }

    let head = commit.unwrap_or("HEAD");
    let mut args: Vec<&str> = vec![
        "diff",
        "--no-ext-diff",
        "--patch-with-raw",
        "-z",
        "--no-color",
    ];
    match status {
        GitFileStatus::New | GitFileStatus::Untracked if commit.is_none() => {
            args.extend(["--no-index", "--", "/dev/null", path_str]);
        }
        // Untracked files do not exist in any commit: compare against nothing.
        GitFileStatus::Untracked => args.extend(["--no-index", "--", "/dev/null", path_str]),
        // Compare the old path to the new one against the base, covering every staged and
        // unstaged combination of the rename and any edit.
        GitFileStatus::Renamed { old_path } => {
            args.extend([head, "--", old_path.as_str(), path_str]);
        }
        _ => args.extend([head, "--", path_str]),
    }
    let diff_output = match git_text(runner, repo, &args).await {
        Ok(output) => output,
        Err(error) => {
            log::info!("Failed to get file diff for {path_str}: {error}");
            // A failed diff is treated as binary: nothing to render.
            return Ok(plain_file(path, status, staged, true, DiffSize::Normal));
        }
    };
    // Oversized diffs are not parsed: they would be marked unrenderable anyway.
    if diff_output.len() > MAX_DIFF_SIZE {
        return Ok(plain_file(
            path,
            status,
            staged,
            false,
            DiffSize::UnrenderableDiffTooLarge,
        ));
    }
    if diff_output
        .lines()
        .any(|line| line.starts_with("Binary files ") && line.contains(" differ"))
    {
        return Ok(plain_file(path, status, staged, true, DiffSize::Normal));
    }
    let hunks = parse_diff_hunks(&diff_output)?;
    let max_line_number = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .flat_map(|l| [l.old_line_number, l.new_line_number])
        .flatten()
        .max()
        .unwrap_or(0);
    let size = compute_diff_size(&hunks, diff_output.len());
    let mut file = plain_file(path, status, staged, false, size);
    file.has_hidden_bidi_chars = diff_output.chars().any(|c| BIDI_CHARS.contains(&c));
    file.max_line_number = max_line_number;
    file.hunks = hunks;
    Ok(file)
}

/// Added lines of `file`.
pub fn additions(file: &proto::FileDiff) -> usize {
    count_lines(file, proto::DiffLineType::Add)
}

/// Deleted lines of `file`.
pub fn deletions(file: &proto::FileDiff) -> usize {
    count_lines(file, proto::DiffLineType::Delete)
}

fn count_lines(file: &proto::FileDiff, kind: proto::DiffLineType) -> usize {
    file.hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.line_type == kind as i32)
        .count()
}

// ── Per-file deltas ──────────────────────────────────────────────────────────

/// Recomputes just `files` (repo-relative paths) for `mode`, one delta each. `diff` is absent for
/// a file that no longer has changes. This is the cheap path for a watcher that knows which
/// files changed; a delta carries the repo-relative path, which is what the client matches.
pub async fn compute_file_deltas(
    runner: &dyn GitRunner,
    repo: &Path,
    wire_repo_path: &str,
    mode: &Mode,
    files: &[String],
) -> Vec<proto::DiffStateFileDelta> {
    // Head has no merge base; the other modes need one, the same for every file.
    let merge_base = if matches!(mode, Mode::Head) {
        None
    } else {
        compute_merge_base(runner, repo, mode).await.ok()
    };
    let mut deltas = Vec::with_capacity(files.len());
    for file in files {
        match retrieve_file_diff(runner, repo, file, mode, merge_base.as_deref()).await {
            Ok(diff) => deltas.push(proto::DiffStateFileDelta {
                repo_path: wire_repo_path.to_string(),
                mode: Some(mode.to_proto()),
                file_path: file.clone(),
                diff,
                metadata: None,
            }),
            Err(err) => log::warn!("Per-file diff-state delta failed for {file}: {err}"),
        }
    }
    deltas
}

async fn retrieve_file_diff(
    runner: &dyn GitRunner,
    repo: &Path,
    rel: &str,
    mode: &Mode,
    merge_base: Option<&str>,
) -> Result<Option<proto::FileDiff>> {
    let Some((path, status, staged)) =
        file_status_for_path(runner, repo, rel, mode, merge_base).await?
    else {
        return Ok(None);
    };
    let commit = match mode {
        Mode::Head => "HEAD",
        _ => merge_base.unwrap_or("HEAD"),
    };
    let is_binary = is_file_binary(runner, repo, rel, commit).await;
    // For Head mode the base is HEAD (`None`); a branch mode diffs against its merge base.
    let base = if matches!(mode, Mode::Head) {
        None
    } else {
        merge_base
    };
    file_diff_for_path(runner, repo, is_binary, &path, &status, staged, base).await
}

async fn file_status_for_path(
    runner: &dyn GitRunner,
    repo: &Path,
    rel: &str,
    mode: &Mode,
    merge_base: Option<&str>,
) -> Result<Option<(PathBuf, GitFileStatus, Option<FileStagedState>)>> {
    let relative = PathBuf::from(rel);
    match (mode, merge_base) {
        (Mode::Head, _) => {
            let output = git_text(
                runner,
                repo,
                &[
                    "--no-optional-locks",
                    "status",
                    "--porcelain=2",
                    "-z",
                    "--",
                    rel,
                ],
            )
            .await?;
            Ok(parse_status_entries(&output)?
                .into_iter()
                .find(|e| e.path == relative)
                .map(|e| (e.path, e.status, Some(e.staged))))
        }
        (_, Some(base)) => {
            let diff_output = git_text(
                runner,
                repo,
                &["diff", "--name-status", "-z", base, "--", rel],
            )
            .await?;
            if let Some((path, status)) = parse_name_status(&diff_output)?
                .into_iter()
                .find(|(p, _)| *p == relative)
            {
                return Ok(Some((path, status, None)));
            }
            // Possibly untracked, which a diff against the base does not list.
            let status_output =
                git_text(runner, repo, &["status", "--porcelain=2", "-z", "--", rel]).await?;
            Ok(parse_status_entries(&status_output)?
                .into_iter()
                .find(|e| e.path == relative && matches!(e.status, GitFileStatus::Untracked))
                .map(|e| (e.path, e.status, None)))
        }
        _ => Ok(None),
    }
}

async fn is_file_binary(runner: &dyn GitRunner, repo: &Path, rel: &str, commit: &str) -> bool {
    match git_text(runner, repo, &["diff", "--numstat", commit, "--", rel]).await {
        Ok(out) => out.lines().next().is_some_and(|l| l.starts_with("-\t-")),
        Err(_) => false,
    }
}

// ── Pure parts ───────────────────────────────────────────────────────────────

/// Parses hunks from `git diff` output, with the per-line numbers and the "no newline at end of
/// file" markers attached to the line they follow.
pub fn parse_diff_hunks(diff_output: &str) -> Result<Vec<proto::DiffHunk>> {
    let lines: Vec<&str> = diff_output.lines().collect();
    let mut hunks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !line.starts_with("@@") {
            i += 1;
            continue;
        }
        let (old_start, old_count, new_start, new_count) = parse_unified_diff_header(line)?;
        let mut hunk_lines: Vec<proto::DiffLine> = Vec::new();
        i += 1;
        let (mut old_line, mut new_line) = (old_start, new_start);
        while i < lines.len() && !lines[i].starts_with("@@") && !lines[i].starts_with("diff ") {
            let content = lines[i];
            i += 1;
            if content.is_empty() {
                continue;
            }
            let (line_type, old_num, new_num) = if content.starts_with('+') {
                let n = new_line;
                new_line += 1;
                (proto::DiffLineType::Add, None, Some(n))
            } else if content.starts_with('-') {
                let n = old_line;
                old_line += 1;
                (proto::DiffLineType::Delete, Some(n), None)
            } else if content.starts_with(' ') {
                let (o, n) = (old_line, new_line);
                old_line += 1;
                new_line += 1;
                (proto::DiffLineType::Context, Some(o), Some(n))
            } else if content.starts_with('\\') {
                // `\ No newline at end of file` is its own line, emitted right after the line it
                // describes: the flag belongs to the entry already pushed.
                if let Some(last) = hunk_lines.last_mut() {
                    last.no_trailing_newline = true;
                }
                continue;
            } else {
                continue;
            };
            hunk_lines.push(proto::DiffLine {
                line_type: line_type as i32,
                old_line_number: old_num.map(|n| n as u64),
                new_line_number: new_num.map(|n| n as u64),
                text: content.get(1..).unwrap_or("").to_string(),
                no_trailing_newline: false,
            });
        }
        hunks.push(proto::DiffHunk {
            old_start_line: old_start as u64,
            old_line_count: old_count as u64,
            new_start_line: new_start as u64,
            new_line_count: new_count as u64,
            lines: hunk_lines,
            unified_diff_start: 0,
            unified_diff_end: 0,
        });
    }
    Ok(hunks)
}

fn parse_range(range: &str) -> Result<(usize, usize)> {
    match range.split_once(',') {
        Some((start, count)) => Ok((
            start
                .parse()
                .map_err(|_| anyhow!("Invalid range start: {range}"))?,
            count
                .parse()
                .map_err(|_| anyhow!("Invalid range count: {range}"))?,
        )),
        None => Ok((
            range
                .parse()
                .map_err(|_| anyhow!("Invalid range: {range}"))?,
            1,
        )),
    }
}

/// `@@ -old_start,old_count +new_start,new_count @@ [context]`.
fn parse_unified_diff_header(header: &str) -> Result<(usize, usize, usize, usize)> {
    let parts: Vec<&str> = header.split_whitespace().take(3).collect();
    if !header.starts_with("@@") || parts.len() < 3 {
        return Err(anyhow!("Invalid unified diff header: {header}"));
    }
    let old = parts[1].get(1..).unwrap_or("");
    let new = parts[2].get(1..).unwrap_or("");
    let (old_start, old_count) = parse_range(old)?;
    let (new_start, new_count) = parse_range(new)?;
    Ok((old_start, old_count, new_start, new_count))
}

/// Categorizes a diff by the size heuristics the app renders by.
pub fn compute_diff_size(hunks: &[proto::DiffHunk], diff_len: usize) -> proto::DiffSize {
    use proto::DiffSize;
    if diff_len > MAX_DIFF_SIZE {
        return DiffSize::UnrenderableDiffTooLarge;
    }
    let count = |kind: proto::DiffLineType| {
        hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.line_type == kind as i32)
            .count()
    };
    let adds = count(proto::DiffLineType::Add);
    let dels = count(proto::DiffLineType::Delete);
    if dels > DELETION_LINE_RENDER_LIMIT {
        return DiffSize::UnrenderableDiffTooLarge;
    }
    let long_line = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .any(|l| l.text.len() > MAX_CHARACTERS_PER_LINE);
    if diff_len >= MAX_REASONABLE_DIFF_SIZE
        || long_line
        || adds > DIFF_LINE_RENDER_LIMIT
        || dels > DIFF_LINE_RENDER_LIMIT
    {
        return DiffSize::Large;
    }
    DiffSize::Normal
}

/// Heuristic: lock files, logs, minified or generated paths, and files that open with a
/// "generated" header.
pub fn is_file_autogenerated(file_path: &Path, content: Option<&str>) -> bool {
    const AUTOGEN_HEADERS: [&str; 3] = [
        "Code generated by",
        "This file is automatically generated",
        "AUTO-GENERATED FILE",
    ];
    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    match file_name {
        "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "Gemfile.lock"
        | "composer.lock" | "Pipfile.lock" | "poetry.lock" | "go.sum" | "mix.lock" => return true,
        name if name.ends_with(".log") => return true,
        _ => {}
    }
    if file_name.contains(".generated.")
        || file_name.contains(".gen.")
        || file_name.ends_with(".min.js")
        || file_name.ends_with(".min.css")
        || file_name.contains(".bundle.")
    {
        return true;
    }
    let path_str = file_path.to_string_lossy();
    if path_str.contains("__generated__/")
        || path_str.contains(".auto/")
        || path_str.contains("codegen/")
    {
        return true;
    }
    content
        .and_then(|c| c.lines().next())
        .is_some_and(|first| AUTOGEN_HEADERS.iter().any(|h| first.contains(h)))
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
