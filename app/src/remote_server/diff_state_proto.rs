//! Conversions between the diff-state proto wire messages and the fork's
//! domain types in `code_review::diff_state`, `code_review::diff_size_limits`,
//! and `util::git`.
//!
//! The proto schema was ported faithfully from Warp's `diff_state.proto`, which
//! carries a few fields the fork's domain types do not have yet (per-file
//! `files` on `Commit` / `DiffMetadataAgainstBase`, `FileDiff.content_at_base`,
//! and the `DIFF_SIZE_UNRENDERABLE_FILE_TOO_LARGE` size). Those are handled lossily here: dropped on decode, defaulted on
//! encode. `content_at_base` is threaded explicitly rather than stored on the
//! fork's plain `FileDiff` (the fork keeps base content in the separate
//! `FileDiffAndContent`).
//!
//! These live in `app` rather than the `remote_server` crate because the
//! domain types are defined here and `remote_server` cannot depend on `app`.

use std::path::PathBuf;
use std::sync::Arc;

use crate::code_review::diff_size_limits::{DiffSize, MAX_DIFF_SIZE};
use crate::code_review::diff_state::{
    DiffHunk, DiffLine, DiffLineType, DiffMetadata, DiffMetadataAgainstBase, DiffMode, DiffState,
    DiffStats, FileDiff, FileDiffAndContent, FileStatusInfo, GitDiffData, GitDiffWithBaseContent,
    GitFileStatus,
};
use crate::util::git::{Commit, FileChangeEntry, PrInfo};

use super::proto;

// ── DiffLineType ─────────────────────────────────────────────────

pub(super) fn diff_line_type_to_proto(value: &DiffLineType) -> proto::DiffLineType {
    match value {
        DiffLineType::Context => proto::DiffLineType::Context,
        DiffLineType::Add => proto::DiffLineType::Add,
        DiffLineType::Delete => proto::DiffLineType::Delete,
        DiffLineType::HunkHeader => proto::DiffLineType::HunkHeader,
    }
}

pub(super) fn proto_to_diff_line_type(value: i32) -> DiffLineType {
    match proto::DiffLineType::try_from(value).unwrap_or(proto::DiffLineType::Unspecified) {
        proto::DiffLineType::Add => DiffLineType::Add,
        proto::DiffLineType::Delete => DiffLineType::Delete,
        proto::DiffLineType::HunkHeader => DiffLineType::HunkHeader,
        // The fork's domain has no Unspecified; treat it (and Context) as Context.
        proto::DiffLineType::Context | proto::DiffLineType::Unspecified => DiffLineType::Context,
    }
}

// ── DiffLine ─────────────────────────────────────────────────────

pub(super) fn diff_line_to_proto(line: &DiffLine) -> proto::DiffLine {
    proto::DiffLine {
        line_type: diff_line_type_to_proto(&line.line_type) as i32,
        old_line_number: line.old_line_number.map(|n| n as u64),
        new_line_number: line.new_line_number.map(|n| n as u64),
        text: line.text.clone(),
        no_trailing_newline: line.no_trailing_newline,
    }
}

pub(super) fn proto_to_diff_line(line: &proto::DiffLine) -> DiffLine {
    DiffLine {
        line_type: proto_to_diff_line_type(line.line_type),
        old_line_number: line.old_line_number.map(|n| n as usize),
        new_line_number: line.new_line_number.map(|n| n as usize),
        text: line.text.clone(),
        no_trailing_newline: line.no_trailing_newline,
    }
}

// ── DiffHunk ─────────────────────────────────────────────────────

pub(super) fn diff_hunk_to_proto(hunk: &DiffHunk) -> proto::DiffHunk {
    proto::DiffHunk {
        old_start_line: hunk.old_start_line as u64,
        old_line_count: hunk.old_line_count as u64,
        new_start_line: hunk.new_start_line as u64,
        new_line_count: hunk.new_line_count as u64,
        lines: hunk.lines.iter().map(diff_line_to_proto).collect(),
        unified_diff_start: hunk.unified_diff_start as u64,
        unified_diff_end: hunk.unified_diff_end as u64,
    }
}

pub(super) fn proto_to_diff_hunk(hunk: &proto::DiffHunk) -> DiffHunk {
    DiffHunk {
        old_start_line: hunk.old_start_line as usize,
        old_line_count: hunk.old_line_count as usize,
        new_start_line: hunk.new_start_line as usize,
        new_line_count: hunk.new_line_count as usize,
        lines: hunk.lines.iter().map(proto_to_diff_line).collect(),
        unified_diff_start: hunk.unified_diff_start as usize,
        unified_diff_end: hunk.unified_diff_end as usize,
    }
}

// ── GitFileStatus ────────────────────────────────────────────────

pub(super) fn git_file_status_to_proto(status: &GitFileStatus) -> proto::GitFileStatus {
    use proto::git_file_status::Status;
    let status = match status {
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
    };
    proto::GitFileStatus {
        status: Some(status),
    }
}

pub(super) fn proto_to_git_file_status(status: &proto::GitFileStatus) -> GitFileStatus {
    use proto::git_file_status::Status;
    match &status.status {
        Some(Status::NewFile(_)) => GitFileStatus::New,
        Some(Status::Modified(_)) => GitFileStatus::Modified,
        Some(Status::Deleted(_)) => GitFileStatus::Deleted,
        Some(Status::Renamed(r)) => GitFileStatus::Renamed {
            old_path: r.old_path.clone(),
        },
        Some(Status::Copied(c)) => GitFileStatus::Copied {
            old_path: c.old_path.clone(),
        },
        Some(Status::Untracked(_)) => GitFileStatus::Untracked,
        Some(Status::Conflicted(_)) => GitFileStatus::Conflicted,
        // Missing oneof: fall back to Modified (the domain's TryFrom default).
        None => GitFileStatus::Modified,
    }
}

/// Fallible companion to [`proto_to_git_file_status`]: same mapping, but a
/// missing status oneof is an error rather than a silent `Modified` default.
/// Used by `FileStatusInfo`'s `TryFrom` (issue #326), where a malformed
/// discard-op message from a mismatched client build must be rejected, not
/// reinterpreted. The other decode paths in this file (`FileDiff`,
/// `GitDiffData`, ...) are out of scope for #326 and keep the lossy default.
fn try_proto_to_git_file_status(status: &proto::GitFileStatus) -> Result<GitFileStatus, String> {
    use proto::git_file_status::Status;
    match &status.status {
        Some(Status::NewFile(_)) => Ok(GitFileStatus::New),
        Some(Status::Modified(_)) => Ok(GitFileStatus::Modified),
        Some(Status::Deleted(_)) => Ok(GitFileStatus::Deleted),
        Some(Status::Renamed(r)) => Ok(GitFileStatus::Renamed {
            old_path: r.old_path.clone(),
        }),
        Some(Status::Copied(c)) => Ok(GitFileStatus::Copied {
            old_path: c.old_path.clone(),
        }),
        Some(Status::Untracked(_)) => Ok(GitFileStatus::Untracked),
        Some(Status::Conflicted(_)) => Ok(GitFileStatus::Conflicted),
        None => Err("missing status variant in GitFileStatus".to_string()),
    }
}

// ── DiffSize ─────────────────────────────────────────────────────

pub(super) fn diff_size_to_proto(size: &DiffSize) -> proto::DiffSize {
    match size {
        DiffSize::Normal => proto::DiffSize::Normal,
        DiffSize::Large => proto::DiffSize::Large,
        DiffSize::Unrenderable => proto::DiffSize::UnrenderableDiffTooLarge,
    }
}

pub(super) fn proto_to_diff_size(size: i32) -> DiffSize {
    match proto::DiffSize::try_from(size).unwrap_or(proto::DiffSize::Unspecified) {
        proto::DiffSize::Large => DiffSize::Large,
        proto::DiffSize::UnrenderableDiffTooLarge | proto::DiffSize::UnrenderableFileTooLarge => {
            DiffSize::Unrenderable
        }
        // The fork has no Unspecified; treat it (and Normal) as Normal.
        proto::DiffSize::Normal | proto::DiffSize::Unspecified => DiffSize::Normal,
    }
}

// ── FileDiff ─────────────────────────────────────────────────────
// The fork's `FileDiff` has no base-content field (that lives in the separate
// `FileDiffAndContent`), so `content_at_base` is passed in / returned out.

pub(super) fn file_diff_to_proto(
    diff: &FileDiff,
    content_at_base: Option<String>,
) -> proto::FileDiff {
    // Decide what base content (if any) to send over the wire, adjusting the
    // rendered size accordingly. This gating is remote-only: it runs when the
    // daemon serializes a diff for a subscriber, never on the local in-memory
    // path, so local rendering keeps full content regardless of size.
    let (size, content_at_base) = if diff.is_binary {
        // Binary base content is never rendered by the client; never ship it.
        // Size is untouched for binary files; the client renders the binary
        // placeholder via `is_binary`, not via size.
        (diff_size_to_proto(&diff.size), None)
    } else if content_at_base
        .as_deref()
        .is_some_and(|c| c.len() > MAX_DIFF_SIZE)
    {
        // Base blob too large for the wire and won't be rendered by the
        // client. The fork's `DiffSize` domain type has no separate
        // file-too-large reason (see the module doc comment above), so this
        // is encoded directly as the wire's file-too-large variant rather
        // than round-tripped through `diff_size_to_proto`.
        (proto::DiffSize::UnrenderableFileTooLarge, None)
    } else {
        (diff_size_to_proto(&diff.size), content_at_base)
    };

    proto::FileDiff {
        file_path: diff.file_path.to_string_lossy().into_owned(),
        status: Some(git_file_status_to_proto(&diff.status)),
        hunks: diff.hunks.iter().map(diff_hunk_to_proto).collect(),
        is_binary: diff.is_binary,
        is_autogenerated: diff.is_autogenerated,
        max_line_number: diff.max_line_number as u64,
        has_hidden_bidi_chars: diff.has_hidden_bidi_chars,
        size: size as i32,
        content_at_base,
    }
}

/// Decodes a proto `FileDiff` into the fork's `FileDiff` plus its detached
/// base content (`None` when the wire omitted it).
pub(super) fn proto_to_file_diff(diff: &proto::FileDiff) -> (FileDiff, Option<String>) {
    let file_diff = FileDiff {
        file_path: PathBuf::from(&diff.file_path),
        status: diff
            .status
            .as_ref()
            .map(proto_to_git_file_status)
            .unwrap_or(GitFileStatus::Modified),
        hunks: Arc::new(diff.hunks.iter().map(proto_to_diff_hunk).collect()),
        is_binary: diff.is_binary,
        is_autogenerated: diff.is_autogenerated,
        max_line_number: diff.max_line_number as usize,
        has_hidden_bidi_chars: diff.has_hidden_bidi_chars,
        size: proto_to_diff_size(diff.size),
    };
    (file_diff, diff.content_at_base.clone())
}

// ── GitDiffData ──────────────────────────────────────────────────

pub(super) fn git_diff_data_to_proto(data: &GitDiffData) -> proto::GitDiffData {
    proto::GitDiffData {
        files: data
            .files
            .iter()
            .map(|f| file_diff_to_proto(f, None))
            .collect(),
        total_additions: data.total_additions as u64,
        total_deletions: data.total_deletions as u64,
        files_changed: data.files_changed as u64,
    }
}

pub(super) fn proto_to_git_diff_data(data: &proto::GitDiffData) -> GitDiffData {
    GitDiffData {
        files: data
            .files
            .iter()
            .map(|f| proto_to_file_diff(f).0)
            .collect(),
        total_additions: data.total_additions as usize,
        total_deletions: data.total_deletions as usize,
        files_changed: data.files_changed as usize,
    }
}

// ── DiffMode ─────────────────────────────────────────────────────

pub(super) fn diff_mode_to_proto(mode: &DiffMode) -> proto::DiffMode {
    use proto::diff_mode::Mode;
    let mode = match mode {
        DiffMode::Head => Mode::Head(proto::DiffModeHead {}),
        DiffMode::MainBranch => Mode::MainBranch(proto::DiffModeMainBranch {}),
        DiffMode::OtherBranch(branch_name) => Mode::OtherBranch(proto::DiffModeOtherBranch {
            branch_name: branch_name.clone(),
        }),
    };
    proto::DiffMode { mode: Some(mode) }
}

pub(super) fn proto_to_diff_mode(mode: &proto::DiffMode) -> DiffMode {
    use proto::diff_mode::Mode;
    match &mode.mode {
        Some(Mode::MainBranch(_)) => DiffMode::MainBranch,
        Some(Mode::OtherBranch(b)) => DiffMode::OtherBranch(b.branch_name.clone()),
        // Missing oneof or Head → Head (the domain default).
        Some(Mode::Head(_)) | None => DiffMode::Head,
    }
}

// ── DiffState ────────────────────────────────────────────────────
// The proto `DiffState` carries only the variant tag; the Loaded diff payload
// travels separately (in `DiffStateSnapshot.diffs`), matching Warp's split.

pub(super) fn diff_state_to_proto(state: &DiffState) -> proto::DiffState {
    use proto::diff_state::State;
    let state = match state {
        DiffState::NotInRepository => State::NotInRepository(proto::DiffStateNotInRepository {}),
        DiffState::Loading => State::Loading(proto::DiffStateLoading {}),
        DiffState::Error(message) => State::Error(proto::DiffStateErrorValue {
            message: message.clone(),
        }),
        DiffState::Loaded(_) => State::Loaded(proto::DiffStateLoaded {}),
    };
    proto::DiffState { state: Some(state) }
}

/// Rebuilds the domain `DiffState`, folding the separately-carried `diffs`
/// payload back into the `Loaded` variant.
pub(super) fn proto_to_diff_state(state: &proto::DiffState, diffs: Option<GitDiffData>) -> DiffState {
    use proto::diff_state::State;
    match &state.state {
        Some(State::NotInRepository(_)) | None => DiffState::NotInRepository,
        Some(State::Loading(_)) => DiffState::Loading,
        Some(State::Error(e)) => DiffState::Error(e.message.clone()),
        Some(State::Loaded(_)) => {
            DiffState::Loaded(diffs.unwrap_or_else(|| GitDiffData {
                files: Vec::new(),
                total_additions: 0,
                total_deletions: 0,
                files_changed: 0,
            }))
        }
    }
}

// ── FileStatusInfo ───────────────────────────────────────────────

/// `pub(crate)` rather than `pub(super)`: `RemoteDiffStateModel::discard_files`
/// (`code_review::diff_state_remote`, #437) encodes the outgoing
/// `DiscardFilesRequest.files` list with this from outside `remote_server`.
pub(crate) fn file_status_info_to_proto(info: &FileStatusInfo) -> proto::FileStatusInfo {
    proto::FileStatusInfo {
        path: info.path.to_string_lossy().into_owned(),
        status: Some(git_file_status_to_proto(&info.status)),
    }
}

/// Validates a decoded `FileStatusInfo` (issue #326): the path and any
/// Renamed/Copied `old_path` must be absolute, and the status oneof must be
/// present, rather than silently defaulting on malformed wire data.
///
/// Deviation from the pinned oracle (02b53fcd8, §5.10): the pin parses
/// `path`/`old_path` as `warp_util::standardized_path::StandardizedPath` and
/// stores that type on the domain `FileStatusInfo`. The fork's
/// `FileStatusInfo::path` is still a plain `PathBuf` (see
/// `code_review::diff_state`), actively used by the local discard-files flow
/// in `code_review_view.rs`. Migrating that field to `StandardizedPath` would
/// ripple into that unrelated, already-working call site, so this checks
/// `Path::is_absolute()` directly instead — same rejection behavior, no
/// domain-type change.
impl TryFrom<&proto::FileStatusInfo> for FileStatusInfo {
    type Error = String;

    fn try_from(info: &proto::FileStatusInfo) -> Result<Self, Self::Error> {
        let path = PathBuf::from(&info.path);
        if !path.is_absolute() {
            return Err(format!(
                "FileStatusInfo path is not absolute: {}",
                info.path
            ));
        }

        let status = info
            .status
            .as_ref()
            .ok_or_else(|| "missing status in FileStatusInfo".to_string())
            .and_then(try_proto_to_git_file_status)?;

        // Renamed/Copied old_path also flows into git restore/checkout
        // commands during discard, so it must be absolute too.
        if let GitFileStatus::Renamed { old_path } | GitFileStatus::Copied { old_path } = &status
        {
            if !PathBuf::from(old_path).is_absolute() {
                return Err(format!("old_path is not absolute: {old_path}"));
            }
        }

        Ok(FileStatusInfo { path, status })
    }
}

// ── FileChangeEntry (util/git.rs) ────────────────────────────────

pub(super) fn file_change_entry_to_proto(entry: &FileChangeEntry) -> proto::FileChangeEntry {
    proto::FileChangeEntry {
        path: entry.path.clone(),
        additions: entry.additions as u64,
        deletions: entry.deletions as u64,
    }
}

/// `pub(crate)` rather than `pub(super)`: the code-review Create-PR dialog
/// decodes `GetCommittedBranchFiles` responses with this too.
pub(crate) fn proto_to_file_change_entry(entry: &proto::FileChangeEntry) -> FileChangeEntry {
    FileChangeEntry {
        path: entry.path.clone(),
        additions: entry.additions as usize,
        deletions: entry.deletions as usize,
    }
}

// ── Commit (util/git.rs) ─────────────────────────────────────────
// The fork's `Commit` has no per-file `files`; encoded as empty, dropped on
// decode.

pub(super) fn commit_to_proto(commit: &Commit) -> proto::Commit {
    proto::Commit {
        hash: commit.hash.clone(),
        subject: commit.subject.clone(),
        files_changed: commit.files_changed as u64,
        additions: commit.additions as u64,
        deletions: commit.deletions as u64,
        files: Vec::new(),
    }
}

/// `pub(crate)` rather than `pub(super)`: the code-review git dialog decodes
/// `GitOpDelta` commits with this when folding a write-op result into the model.
pub(crate) fn proto_to_commit(commit: &proto::Commit) -> Commit {
    Commit {
        hash: commit.hash.clone(),
        subject: commit.subject.clone(),
        files_changed: commit.files_changed as usize,
        additions: commit.additions as usize,
        deletions: commit.deletions as usize,
    }
}

// ── PrInfo (util/git.rs) ─────────────────────────────────────────

pub(super) fn pr_info_to_proto(pr: &PrInfo) -> proto::PrInfo {
    proto::PrInfo {
        number: pr.number,
        url: pr.url.clone(),
        state: pr.state.clone(),
        draft: pr.draft,
        base_branch: pr.base_branch.clone(),
    }
}

pub(crate) fn proto_to_pr_info(pr: &proto::PrInfo) -> PrInfo {
    PrInfo {
        number: pr.number,
        url: pr.url.clone(),
        state: pr.state.clone(),
        draft: pr.draft,
        base_branch: pr.base_branch.clone(),
    }
}

// ── DiffStats ────────────────────────────────────────────────────

pub(super) fn diff_stats_to_proto(stats: &DiffStats) -> proto::DiffStats {
    proto::DiffStats {
        files_changed: stats.files_changed as u64,
        total_additions: stats.total_additions as u64,
        total_deletions: stats.total_deletions as u64,
    }
}

pub(super) fn proto_to_diff_stats(stats: &proto::DiffStats) -> DiffStats {
    DiffStats {
        files_changed: stats.files_changed as usize,
        total_additions: stats.total_additions as usize,
        total_deletions: stats.total_deletions as usize,
    }
}

// ── DiffMetadataAgainstBase ──────────────────────────────────────
// The fork's `DiffMetadataAgainstBase` carries only aggregate stats; the
// proto's per-file `files` list is encoded empty and dropped on decode.

pub(super) fn diff_metadata_against_base_to_proto(
    base: &DiffMetadataAgainstBase,
) -> proto::DiffMetadataAgainstBase {
    proto::DiffMetadataAgainstBase {
        aggregate_stats: Some(diff_stats_to_proto(&base.aggregate_stats)),
        files: Vec::new(),
    }
}

/// Requires `aggregate_stats` (issue #326): a `DiffMetadataAgainstBase` with
/// no stats is malformed wire data, not a legitimate "no changes" state (that
/// is expressed by stats that are all-zero, not absent).
impl TryFrom<&proto::DiffMetadataAgainstBase> for DiffMetadataAgainstBase {
    type Error = String;

    fn try_from(base: &proto::DiffMetadataAgainstBase) -> Result<Self, Self::Error> {
        Ok(DiffMetadataAgainstBase {
            aggregate_stats: base
                .aggregate_stats
                .as_ref()
                .map(proto_to_diff_stats)
                .ok_or_else(|| {
                    "missing aggregate_stats in DiffMetadataAgainstBase".to_string()
                })?,
        })
    }
}

// ── DiffMetadata ─────────────────────────────────────────────────

pub(super) fn diff_metadata_to_proto(metadata: &DiffMetadata) -> proto::DiffMetadata {
    proto::DiffMetadata {
        main_branch_name: metadata.main_branch_name.clone(),
        current_branch_name: metadata.current_branch_name.clone(),
        against_head: Some(diff_metadata_against_base_to_proto(&metadata.against_head)),
        against_base_branch: metadata
            .against_base_branch
            .as_ref()
            .map(diff_metadata_against_base_to_proto),
        has_head_commit: metadata.has_head_commit,
        unpushed_commits: metadata.unpushed_commits.iter().map(commit_to_proto).collect(),
        upstream_ref: metadata.upstream_ref.clone(),
        pr_info: metadata.pr_info.as_ref().map(pr_info_to_proto),
    }
}

/// Requires `against_head` (issue #326): every real `DiffMetadata` the daemon
/// sends has a head comparison, so an absent one means the message was
/// truncated or built from a mismatched schema version, not a valid state.
impl TryFrom<&proto::DiffMetadata> for DiffMetadata {
    type Error = String;

    fn try_from(metadata: &proto::DiffMetadata) -> Result<Self, Self::Error> {
        Ok(DiffMetadata {
            main_branch_name: metadata.main_branch_name.clone(),
            current_branch_name: metadata.current_branch_name.clone(),
            against_head: metadata
                .against_head
                .as_ref()
                .ok_or_else(|| "missing against_head in DiffMetadata".to_string())
                .and_then(DiffMetadataAgainstBase::try_from)?,
            against_base_branch: metadata
                .against_base_branch
                .as_ref()
                .map(DiffMetadataAgainstBase::try_from)
                .transpose()?,
            has_head_commit: metadata.has_head_commit,
            unpushed_commits: metadata
                .unpushed_commits
                .iter()
                .map(proto_to_commit)
                .collect(),
            upstream_ref: metadata.upstream_ref.clone(),
            pr_info: metadata.pr_info.as_ref().map(proto_to_pr_info),
        })
    }
}

// ── GetDiffState response builders ───────────────────────────────
// Assemble the daemon's reply to a GetDiffState subscription. The Loaded diff
// payload is carried in `DiffStateSnapshot.diffs`, separate from the state tag.

pub(super) fn build_snapshot(
    repo_path: String,
    mode: &DiffMode,
    state: &DiffState,
    metadata: &DiffMetadata,
) -> proto::DiffStateSnapshot {
    proto::DiffStateSnapshot {
        repo_path,
        mode: Some(diff_mode_to_proto(mode)),
        metadata: Some(diff_metadata_to_proto(metadata)),
        state: Some(diff_state_to_proto(state)),
        diffs: match state {
            DiffState::Loaded(data) => Some(git_diff_data_to_proto(data)),
            _ => None,
        },
    }
}

/// Builds a `DiffStateFileDelta` proto message for a single-file diff-state
/// push. Mirrors `build_snapshot` but carries only one file's diff plus
/// (optionally) refreshed metadata, so the daemon does not have to
/// re-serialize the whole repo on a single-file change.
///
/// Not yet called outside tests: the debounced per-file push path lives in
/// the daemon's diff-state tracker, which the fork has not ported yet
/// (tracked by #324).
#[allow(dead_code)] // no non-test caller until #324 lands the daemon push path
pub(crate) fn build_diff_state_file_delta(
    repo_path: &str,
    mode: &DiffMode,
    repo_relative_path: &str,
    diff: Option<&FileDiffAndContent>,
    metadata: Option<&DiffMetadata>,
) -> proto::DiffStateFileDelta {
    proto::DiffStateFileDelta {
        repo_path: repo_path.to_string(),
        mode: Some(diff_mode_to_proto(mode)),
        file_path: repo_relative_path.to_string(),
        diff: diff.map(|d| file_diff_to_proto(&d.file_diff, d.content_at_head.clone())),
        metadata: metadata.map(diff_metadata_to_proto),
    }
}

/// Builds a `DiffStateSnapshot` from a headless computation's parts. `metadata`
/// is `None` when the repo could not be read (→ NotInRepository); `Some` with
/// `diff_data` `None` means the diff computation failed (→ Error); both present
/// means Loaded. Shared by the daemon's subscribe reply and its live pushes so
/// the state decision is identical.
pub(crate) fn snapshot_from_parts(
    repo_path: String,
    mode: &DiffMode,
    metadata: Option<DiffMetadata>,
    diff_data: Option<GitDiffData>,
) -> proto::DiffStateSnapshot {
    match metadata {
        Some(metadata) => {
            let state = match diff_data {
                Some(data) => DiffState::Loaded(data),
                None => DiffState::Error("Failed to compute diff".to_string()),
            };
            build_snapshot(repo_path, mode, &state, &metadata)
        }
        None => build_snapshot(
            repo_path,
            mode,
            &DiffState::NotInRepository,
            &DiffMetadata::default(),
        ),
    }
}

pub(super) fn snapshot_response(snapshot: proto::DiffStateSnapshot) -> proto::GetDiffStateResponse {
    proto::GetDiffStateResponse {
        result: Some(proto::get_diff_state_response::Result::Snapshot(snapshot)),
    }
}

pub(super) fn error_response(message: String) -> proto::GetDiffStateResponse {
    proto::GetDiffStateResponse {
        result: Some(proto::get_diff_state_response::Result::Error(
            proto::DiffStateError { message },
        )),
    }
}

// ── Decode API for the remote diff-state consumer ────────────────
// The RemoteDiffStateModel lives in the `code_review` module, so it cannot use
// the `pub(super)` converters above; these `pub(crate)` entry points give it
// clean domain values decoded from the server's push messages.

/// Decodes a proto `GitDiffData` into the fork's `GitDiffWithBaseContent`,
/// preserving the per-file base content (proto `content_at_base` →
/// `FileDiffAndContent::content_at_head`).
pub(crate) fn proto_to_git_diff_with_base_content(
    data: &proto::GitDiffData,
) -> GitDiffWithBaseContent {
    GitDiffWithBaseContent {
        files: data
            .files
            .iter()
            .map(|f| {
                let (file_diff, content_at_head) = proto_to_file_diff(f);
                FileDiffAndContent {
                    file_diff,
                    content_at_head,
                }
            })
            .collect(),
        total_additions: data.total_additions as usize,
        total_deletions: data.total_deletions as usize,
        files_changed: data.files_changed as usize,
    }
}

/// A diff-state snapshot decoded into the fork's domain types.
pub(crate) struct DecodedSnapshot {
    /// The user-visible diff state, with the `Loaded` payload folded back in.
    pub state: DiffState,
    pub metadata: DiffMetadata,
    /// The full diff-with-base-content, kept separately so the model can emit
    /// it in `NewDiffsComputed` (the editor needs the base content). Present
    /// only when the snapshot carried diffs.
    pub diffs: Option<GitDiffWithBaseContent>,
}

/// Decodes a `DiffStateSnapshot` push. Returns `Err` when the wire message
/// carries a `metadata` field that fails `DiffMetadata`'s validation (issue
/// #326) — an absent `metadata` field entirely still defaults, matching prior
/// behavior; only a present-but-malformed one is now rejected instead of
/// silently degrading to `DiffMetadata::default()`.
pub(crate) fn decode_snapshot(
    snapshot: &proto::DiffStateSnapshot,
) -> Result<DecodedSnapshot, String> {
    let diffs = snapshot
        .diffs
        .as_ref()
        .map(proto_to_git_diff_with_base_content);
    let git_diff_data = diffs.as_ref().map(GitDiffData::from);
    let state = snapshot
        .state
        .as_ref()
        .map(|st| proto_to_diff_state(st, git_diff_data))
        .unwrap_or(DiffState::NotInRepository);
    let metadata = snapshot
        .metadata
        .as_ref()
        .map(DiffMetadata::try_from)
        .transpose()?
        .unwrap_or_default();
    Ok(DecodedSnapshot {
        state,
        metadata,
        diffs,
    })
}

/// Decodes the metadata-only push. `Ok(None)` when the wire omitted
/// `metadata` entirely; `Err` when it was present but failed validation
/// (issue #326).
pub(crate) fn decode_metadata_update(
    update: &proto::DiffStateMetadataUpdate,
) -> Result<Option<DiffMetadata>, String> {
    update.metadata.as_ref().map(DiffMetadata::try_from).transpose()
}

/// A single-file diff-state delta decoded into domain types.
pub(crate) struct DecodedFileDelta {
    pub file_path: String,
    /// The updated file diff (with base content), or `None` when the file no
    /// longer has changes.
    pub diff: Option<FileDiffAndContent>,
    pub metadata: Option<DiffMetadata>,
}

pub(crate) fn decode_file_delta(
    delta: &proto::DiffStateFileDelta,
) -> Result<DecodedFileDelta, String> {
    let diff = delta.diff.as_ref().map(|f| {
        let (file_diff, content_at_head) = proto_to_file_diff(f);
        FileDiffAndContent {
            file_diff,
            content_at_head,
        }
    });
    let metadata = delta.metadata.as_ref().map(DiffMetadata::try_from).transpose()?;
    Ok(DecodedFileDelta {
        file_path: delta.file_path.clone(),
        diff,
        metadata,
    })
}

/// Encodes a domain `DiffMode` to proto (used by the client when subscribing).
pub(crate) fn encode_diff_mode(mode: &DiffMode) -> proto::DiffMode {
    diff_mode_to_proto(mode)
}

#[cfg(test)]
#[path = "diff_state_proto_tests.rs"]
mod tests;
