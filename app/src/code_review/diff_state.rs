//! Code review diff state management.
//!
//! Some of the code in this module is adapted from GitHub Desktop, which is licensed under the MIT license,
//! Copyright (c) GitHub, Inc.  See GITHUB-DESKTOP-LICENSE in this directory.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Read, Seek},
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        use futures::future::Either;
        use std::fs;
        use std::future;
        use std::future::Future;
    }
}
#[cfg(not(target_arch = "wasm32"))]
use warpui::AppContext;
use warpui::{r#async::SpawnedFutureHandle, ModelContext};

use crate::code_review::diff_size_limits::DiffSize;
use crate::features::FeatureFlag;
#[cfg(feature = "local_fs")]
use crate::util::git::get_pr_for_branch;
use crate::util::git::{
    detect_current_branch, detect_main_branch, get_unpushed_commits, run_git_command, Commit,
    PrInfo,
};

use super::diff_size_limits::compute_diff_size;

use crate::code_review::CodeReviewTelemetryEvent;
#[cfg(not(target_family = "wasm"))]
use warp_core::channel::ChannelState;
use warp_core::{safe_warn, send_telemetry_from_ctx};

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        use repo_metadata::repositories::{DetectedRepositories, RepoDetectionSource};
        use warpui::SingletonEntity;
        use repo_metadata::{
            repository::{RepositorySubscriber, SubscriberId},
            RepoMetadataError, Repository, RepositoryUpdate, RepositoryWatchMode,
        };
        use async_channel::Sender;
        use warpui::ModelHandle;
    }
}
#[cfg(all(feature = "local_fs", feature = "local_tty"))]
use crate::terminal::local_shell::LocalShellState;

const UNCOMMITTED_CHANGES: &str = "Uncommitted changes";

/// Represents a parsed unified diff header
/// Format: @@ -old_start,old_count +new_start,new_count @@ [optional context]
#[derive(Clone, Debug, PartialEq)]
pub struct UnifiedDiffHeader {
    pub old_start_line: usize,
    pub old_line_count: usize,
    pub new_start_line: usize,
    pub new_line_count: usize,
}

// Unicode bidirectional characters that should be flagged
const BIDI_CHARS: [char; 9] = [
    '\u{202A}', // LEFT-TO-RIGHT EMBEDDING
    '\u{202B}', // RIGHT-TO-LEFT EMBEDDING
    '\u{202C}', // POP DIRECTIONAL FORMATTING
    '\u{202D}', // LEFT-TO-RIGHT OVERRIDE
    '\u{202E}', // RIGHT-TO-LEFT OVERRIDE
    '\u{2066}', // LEFT-TO-RIGHT ISOLATE
    '\u{2067}', // RIGHT-TO-LEFT ISOLATE
    '\u{2068}', // FIRST STRONG ISOLATE
    '\u{2069}', // POP DIRECTIONAL ISOLATE
];

/// Represents the status of a file in the git working directory
/// This matches Git Desktop's AppFileStatusKind enum
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

/// Where a file's changes sit relative to the index (Zap #329).
///
/// Derived from the `XY` column of `git status --porcelain=v2`: `X` is the
/// index-vs-HEAD state and `Y` the worktree-vs-index state, so `X != '.'`
/// means "something is staged" and `Y != '.'` means "something is not".
///
/// Deliberately *not* stored on [`GitFileStatus`]: that enum answers "what
/// kind of change is this" and is shared with the base-branch diff modes,
/// which have no index at all. This answers a different question, and is
/// carried as an `Option` on [`FileDiff`] so those modes can say "not
/// applicable" instead of lying with `Unstaged`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStagedState {
    /// The whole diff for this path is in the index (`XY` = `X.`).
    Staged,
    /// Nothing for this path is in the index (`XY` = `.Y`, or untracked).
    Unstaged,
    /// Both columns are dirty (`XY` = `XY`): some of this file is staged and
    /// some is not, so neither a plain "stage" nor "unstage" label is honest.
    PartiallyStaged,
}

impl FileStagedState {
    /// Whether *any* part of this file is in the index.
    pub fn is_staged(&self) -> bool {
        matches!(self, Self::Staged | Self::PartiallyStaged)
    }

    /// Whether the file's whole diff is in the index — the predicate the UI
    /// uses to pick between the stage and unstage affordance. A partially
    /// staged file gets the *stage* action, because `git add` on it stages
    /// the remainder rather than discarding what is already staged.
    pub fn is_fully_staged(&self) -> bool {
        matches!(self, Self::Staged)
    }

    /// Reads the `XY` status field of a `git status --porcelain=v2` changed
    /// or renamed entry. Anything that is not a two-column code (an untracked
    /// or unmerged entry, or malformed input) is reported as unstaged, which
    /// is what both of those are: an untracked path has no index entry, and an
    /// unmerged one cannot be staged without resolving the conflict first.
    fn from_status_code(status_code: &str) -> Self {
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
    /// Whole files, by **repo-relative** path. Absolute paths would work for
    /// the local backend and silently mean the wrong thing on the daemon,
    /// whose repo lives at a different absolute prefix.
    Paths(Vec<String>),
    /// One reconstructed hunk patch from [`hunk_to_patch`], applied with
    /// `git apply --cached`.
    ///
    /// The patch's line numbers are those of the diff it was built from, so a
    /// caller must re-diff between operations rather than queueing several
    /// hunks from one snapshot — see [`hunk_to_patch`]'s note. `git apply`
    /// verifies context and rejects a stale patch, so getting this wrong
    /// fails loudly instead of corrupting the index.
    Hunk(String),
}

/// One stage/unstage operation. Shared by the local backend, the remote
/// backend, and the daemon handler so all three express the same vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRequest {
    pub target: StageTarget,
    /// Un-stage rather than stage. For [`StageTarget::Paths`] this is
    /// `git restore --staged` (index only — the working tree is untouched,
    /// unlike the Discard Files path, which also passes `--worktree`); for
    /// [`StageTarget::Hunk`] it is `git apply --cached --reverse`.
    pub unstage: bool,
}

#[derive(Clone, Debug)]
pub struct FileStatusInfo {
    pub path: PathBuf,
    pub status: GitFileStatus,
}

/// One parsed `git status --porcelain=v2` entry, carrying the index state
/// alongside the change kind.
///
/// [`LocalDiffStateModel::parse_git_status`] keeps returning bare
/// `(path, status)` pairs because its five callers only ever wanted those;
/// only the HEAD-mode diff builder needs the staged column, and it calls
/// [`LocalDiffStateModel::parse_git_status_entries`] directly.
#[derive(Clone, Debug, PartialEq)]
pub struct GitStatusEntry {
    pub path: PathBuf,
    pub status: GitFileStatus,
    pub staged: FileStagedState,
}

impl TryFrom<&str> for GitFileStatus {
    type Error = anyhow::Error;

    fn try_from(status_code: &str) -> Result<Self> {
        match status_code {
            ".M" | "M." | "MM" => Ok(GitFileStatus::Modified),
            ".A" | "A." | "AM" => Ok(GitFileStatus::New),
            ".D" | "D." | "AD" => Ok(GitFileStatus::Deleted),
            _ => Ok(GitFileStatus::Modified), // Default fallback
        }
    }
}

/// Represents a single line in a diff hunk, as rendered by `git diff`.
/// This matches Git Desktop's DiffLine structure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub old_line_number: Option<usize>,
    pub new_line_number: Option<usize>,
    pub text: String,
    pub no_trailing_newline: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DiffLineType {
    Context,
    Add,
    Delete,
    HunkHeader,
}

/// Represents a hunk of changes in a file diff, as rendered by `git diff`,
/// including the header and context lines before/after an insertion or
/// deletion.
/// This matches Git Desktop's DiffHunk structure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start_line: usize,
    pub old_line_count: usize,
    pub new_start_line: usize,
    pub new_line_count: usize,
    pub lines: Vec<DiffLine>,
    pub unified_diff_start: usize,
    pub unified_diff_end: usize,
}

/// Reconstructs a minimal, self-contained unified diff for a single [`DiffHunk`],
/// suitable for `git apply --cached` — the primitive behind hunk-level staging.
///
/// Whole-file staging elsewhere in this module uses
/// `git restore --staged --worktree`, which cannot express "part of this file".
/// Reconstructing a one-hunk patch is how git itself implements `add -p`.
///
/// **Line numbers are those of the diff this hunk came from.** Staging hunk N
/// after already staging hunk N-1 shifts the file, so the caller must re-diff
/// between operations rather than applying a batch of hunks from one snapshot.
/// `run_apply_patch_cached` deliberately does not pass `--unidiff-zero`, so git
/// verifies context and rejects a stale patch instead of corrupting the index.
///
/// `no_trailing_newline` emits git's `\ No newline at end of file` marker;
/// omitting it makes git apply a patch that silently appends a newline.
pub fn hunk_to_patch(file_path: &Path, hunk: &DiffHunk) -> String {
    let path = file_path.to_string_lossy();
    let mut out = String::new();
    out.push_str(&format!("diff --git a/{path} b/{path}\n"));
    out.push_str(&format!("--- a/{path}\n"));
    out.push_str(&format!("+++ b/{path}\n"));
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start_line, hunk.old_line_count, hunk.new_start_line, hunk.new_line_count
    ));
    for line in &hunk.lines {
        let prefix = match line.line_type {
            DiffLineType::Context => " ",
            DiffLineType::Add => "+",
            DiffLineType::Delete => "-",
            // The `@@` header is rebuilt above from the hunk's own counts; a
            // parsed header line is not part of the body.
            DiffLineType::HunkHeader => continue,
        };
        out.push_str(prefix);
        out.push_str(&line.text);
        out.push('\n');
        if line.no_trailing_newline {
            out.push_str("\\ No newline at end of file\n");
        }
    }
    out
}

/// The half-open range of **new-file** lines a [`DiffHunk`] occupies, using
/// git's 1-indexed line numbers.
///
/// # End convention
///
/// `[start, end)` — `end` is *one past* the last line the hunk covers. Two
/// hunks that abut in the new file therefore produce `[10, 21)` and `[21, 22)`
/// and share no line, so callers must test overlap with `<` / `>` and never
/// `<=` / `>=`. Both predicates in `code_review_view.rs` used to compare
/// `requested_start <= hunk_end` against a one-past-the-end bound, which let
/// every hunk claim the first line of its successor; `Iterator::find` then
/// returned the earlier hunk and the click staged the wrong change.
///
/// # Why `new_line_count` and not the materialised lines
///
/// `new_line_count` is read straight off the `@@ -a,b +c,d @@` header, where
/// git computes it from the file rather than from the rendered body. Deriving
/// the extent from `lines` instead is wrong in both directions:
///
/// * `lines` includes `Delete` entries, which occupy no new-file line at all,
///   so `lines.len()` *overstates* the extent by the deletion count — a hunk
///   removing fifty lines and adding one claims fifty-one.
/// * Filtering to `Add | Context` *understates* it, because the parser drops
///   empty content lines, and an empty content line is exactly what git emits
///   for a blank context line under `diff.suppressBlankEmpty=true`. The
///   recount then loses every blank context line in the hunk.
///
/// The header count is immune to both.
///
/// # Pure deletions
///
/// A hunk that only deletes has `new_line_count == 0` and genuinely occupies
/// no new-file line: `@@ -3,2 +2,0 @@` describes the *gap* after new line 2.
/// Padding it to one line at `new_start_line` would make it overlap the hunk
/// that really owns that line, so it is anchored on `new_start_line + 1` — the
/// line immediately *after* the gap. That is the same line the editor gutter
/// hangs its deletion marker on (`DiffStatus::deletion_mapping` is keyed by
/// the 0-indexed new-file line following the removed block), so the button
/// drawn for a deletion resolves to the deletion.
///
/// # Divergence from the pinned oracle
///
/// This is **not** a parity port. The pinned oracle `42effe840` has no
/// equivalent of this function, and its `extract_diff_hunk_data` computes the
/// extent as `hunk.new_start_line + hunk.lines.len()` with an inclusive `<=`
/// — character-identical to what this fork carried before. The oracle shares
/// the defect; we are fixing it ahead of the oracle deliberately, because the
/// fork put a *staging* button on the same predicate (Zap #329) and resolving
/// to the wrong hunk there writes the index rather than merely quoting the
/// wrong text. A re-pin must not "restore parity" by reverting this.
pub fn hunk_new_line_span(hunk: &DiffHunk) -> Range<usize> {
    if hunk.new_line_count == 0 {
        let anchor = hunk.new_start_line + 1;
        anchor..anchor + 1
    } else {
        hunk.new_start_line..hunk.new_start_line + hunk.new_line_count
    }
}

/// Whether `hunk` covers any part of `requested`, a 1-indexed, half-open range
/// of new-file lines.
///
/// This is the single predicate behind both gutter buttons — staging a hunk
/// and quoting one as agent context. They are handed the identical
/// `line_range` by the editor (see `render_stage_button` / `render_plus_button`
/// in `code/editor/element.rs`, both passing `line.line_range()`), so if they
/// disagree the plus and the plus-minus resolve to different hunks. Sharing
/// one function is what stops that recurring.
///
/// An empty `requested` is read as a point query on `requested.start`: the
/// editor reports a pure-deletion gutter hunk as the empty range `N..N`
/// (0-indexed), and a click on that marker must still resolve to a hunk.
pub fn hunk_covers_new_lines(hunk: &DiffHunk, requested: &Range<usize>) -> bool {
    let span = hunk_new_line_span(hunk);
    let requested_end = requested.end.max(requested.start + 1);
    requested.start < span.end && requested_end > span.start
}

/// Represents the diff for a single file, as rendered by `git diff`.
/// This matches Git Desktop's FileDiff structure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub file_path: PathBuf,
    pub status: GitFileStatus,
    pub hunks: Arc<Vec<DiffHunk>>,
    pub is_binary: bool,
    pub is_autogenerated: bool,
    pub max_line_number: usize,
    pub has_hidden_bidi_chars: bool,
    pub size: DiffSize,
    /// Index state for this path, or `None` when the question does not apply
    /// (Zap #329). `None` in the base-branch diff modes, whose file list comes
    /// from `git diff --name-status <merge-base>` and carries no index column;
    /// `None` too for a remote snapshot from a daemon predating this field,
    /// since the wire default decodes to "not reported" rather than
    /// "unstaged". Both cases render without a staging affordance.
    pub staged: Option<FileStagedState>,
}

impl FileDiff {
    pub fn is_empty(&self) -> bool {
        self.additions() == 0 && self.deletions() == 0
    }

    /// Returns the number of added lines in this file diff
    pub fn additions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.line_type == DiffLineType::Add)
            .count()
    }

    /// Returns the number of deleted lines in this file diff
    pub fn deletions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.line_type == DiffLineType::Delete)
            .count()
    }
}

/// IMPORTANT: This struct contains expensive data like the full content of diff files
/// at base. This should not be cloned at any time.
#[derive(Debug)]
pub struct FileDiffAndContent {
    pub file_diff: FileDiff,
    pub content_at_head: Option<String>,
}

/// IMPORTANT: This struct contains expensive data like the full content of diff files
/// at base. This should not be cloned at any time.
#[derive(Debug)]
pub struct GitDiffWithBaseContent {
    pub files: Vec<FileDiffAndContent>,
    pub total_additions: usize,
    pub total_deletions: usize,
    pub files_changed: usize,
}

impl From<GitDiffWithBaseContent> for GitDiffData {
    fn from(value: GitDiffWithBaseContent) -> Self {
        Self {
            files: value.files.into_iter().map(|file| file.file_diff).collect(),
            total_additions: value.total_additions,
            total_deletions: value.total_deletions,
            files_changed: value.files_changed,
        }
    }
}

impl From<&GitDiffWithBaseContent> for GitDiffData {
    fn from(value: &GitDiffWithBaseContent) -> Self {
        Self {
            files: value
                .files
                .iter()
                .map(|file| file.file_diff.clone())
                .collect(),
            total_additions: value.total_additions,
            total_deletions: value.total_deletions,
            files_changed: value.files_changed,
        }
    }
}

/// Represents the complete git diff information for a repository
#[derive(Clone, Debug, PartialEq)]
pub struct GitDiffData {
    pub files: Vec<FileDiff>,
    pub total_additions: usize,
    pub total_deletions: usize,
    pub files_changed: usize,
}

impl GitDiffData {
    pub fn is_dirty(&self) -> bool {
        self.total_additions + self.total_deletions + self.files_changed > 0
    }
}

/// Some actions should only apply when a [`GitDiffData`] is dirty, i.e. not empty. This enum allows
/// callers to express this preference.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GitDeltaPreference {
    Always,
    OnlyDirty,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize)]
pub enum DiffMode {
    /// Show changes in working directory against latest commit (git diff)
    #[default]
    Head,
    /// Show changes in working directory against main branch (git diff $(git merge-base HEAD origin/master))
    MainBranch,
    /// Show changes in working directory against an arbitrary branch (git diff $(git merge-base HEAD <branch>))
    OtherBranch(#[serde(skip_serializing)] String),
}

impl DiffMode {
    /// Creates a DiffMode from a branch name.
    /// If the branch matches the repository's main branch, returns `MainBranch`;
    /// otherwise returns `OtherBranch(branch)`.
    pub fn from_branch(branch: &str, main_branch_name: Option<&str>) -> Self {
        if main_branch_name == Some(branch) {
            DiffMode::MainBranch
        } else {
            DiffMode::OtherBranch(branch.to_string())
        }
    }
}

/// Internal representation of the diffs we've loaded against all bases.
/// This could include changes against both the latest commit/HEAD
/// and changes against the main branch.
#[derive(Clone, Default)]
enum InternalDiffState {
    #[default]
    NotInRepository,
    Loading,
    Loaded(Diffs),
}

/// User-visible representation of the diffs we've loaded,
/// which only includes changes against the specific base the user has selected.
pub enum DiffState {
    NotInRepository,
    Loading,
    Error(String),
    Loaded(GitDiffData),
}

#[derive(Clone)]
struct Diffs {
    changes: Result<GitDiffData, String>,
}

impl From<DiffsWithBaseContent> for Diffs {
    fn from(value: DiffsWithBaseContent) -> Self {
        Self {
            changes: value.changes.map(|diff| diff.into()),
        }
    }
}

impl From<&DiffsWithBaseContent> for Diffs {
    fn from(value: &DiffsWithBaseContent) -> Self {
        Self {
            changes: match value.changes.as_ref() {
                Ok(changes) => Ok(changes.into()),
                Err(e) => Err(e.clone()),
            },
        }
    }
}

/// IMPORTANT: This struct contains expensive data like the full content of diff files
/// at base. This should not be cloned at any time.
struct DiffsWithBaseContent {
    changes: Result<GitDiffWithBaseContent, String>,
    repository_path: PathBuf,
}

// `pub` (with public fields) so the remote-server daemon can build a diff-state
// snapshot from a headless `load_metadata_for_repo` and convert it to the wire
// proto (see app/src/remote_server/diff_state_proto.rs). Warp treats this
// metadata as a first-class serialized type; the fork mirrors that.
#[derive(Default, Debug, PartialEq)]
pub struct DiffMetadata {
    pub main_branch_name: String,
    pub current_branch_name: String,
    pub against_head: DiffMetadataAgainstBase,
    pub against_base_branch: Option<DiffMetadataAgainstBase>,
    pub has_head_commit: bool,
    pub unpushed_commits: Vec<Commit>,
    pub upstream_ref: Option<String>,
    pub pr_info: Option<PrInfo>,
}

#[derive(Default, Debug, PartialEq)]
pub struct DiffMetadataAgainstBase {
    pub aggregate_stats: DiffStats,
}

impl DiffMetadataAgainstBase {
    pub fn is_dirty(&self) -> bool {
        !self.aggregate_stats.has_no_changes()
    }
}

/// Model that contains all state related to the current pane's open git repository.
pub struct LocalDiffStateModel {
    #[cfg(feature = "local_fs")]
    repository: Option<ModelHandle<Repository>>,
    #[cfg(feature = "local_fs")]
    subscriber_id: Option<SubscriberId>,
    state: InternalDiffState,
    mode: DiffMode,
    metadata: Option<DiffMetadata>,
    computing_diffs_abort_handle: Option<SpawnedFutureHandle>,
    computing_metadata_abort_handle: Option<SpawnedFutureHandle>,
    /// Tracks an in-flight `refresh_pr_info` lookup so the UI can avoid
    /// treating a pending/unknown PR state as "no PR" (flicker bug).
    refreshing_pr_info_handle: Option<SpawnedFutureHandle>,
    /// Branch name for which `refresh_pr_info` has been called at least once.
    /// Gates the metadata-refresh fallback so it only retries `gh pr view` once
    /// per branch when `pr_info` is `None` (e.g. branch has no PR, lookup
    /// failed). Without this, every filesystem event on the repo would re-fire
    /// `gh pr view` for branches that legitimately have no PR.
    pr_info_attempted_for_branch: Option<String>,
    /// Controls whether periodic throttled metadata refresh is active.
    /// Refresh is suppressed when the code review pane is not open.
    metadata_refresh_enabled: bool,
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct DiffStats {
    pub files_changed: usize,
    pub total_additions: usize,
    pub total_deletions: usize,
}

impl DiffStats {
    pub(crate) fn has_no_changes(&self) -> bool {
        self.files_changed == 0
    }
}

/// Comprehensive git diff information including uncommitted changes, main branch comparison, and main branch name
#[derive(Debug, Clone)]
pub struct GitDiffInfo {
    pub uncommitted_stats: Option<DiffStats>,
    pub main_branch_stats: Option<DiffStats>,
    pub main_branch_name: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct GitNumStatMetadata {
    lines_added: usize,
    lines_removed: usize,
    is_binary_file: bool,
}

impl LocalDiffStateModel {
    #[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
    pub fn new(repo_path: Option<String>, ctx: &mut ModelContext<Self>) -> Self {
        let model = Self {
            #[cfg(feature = "local_fs")]
            repository: None,
            state: InternalDiffState::default(),
            #[cfg(feature = "local_fs")]
            subscriber_id: None,
            mode: DiffMode::default(),
            metadata: None,
            computing_diffs_abort_handle: None,
            computing_metadata_abort_handle: None,
            refreshing_pr_info_handle: None,
            pr_info_attempted_for_branch: None,
            metadata_refresh_enabled: false,
        };

        #[cfg(feature = "local_fs")]
        {
            if let Some(repo_path) = &repo_path {
                let fut = DetectedRepositories::handle(ctx).update(ctx, |model, ctx| {
                    model.detect_possible_git_repo(
                        repo_path,
                        RepoDetectionSource::CodeReviewInitialization,
                        ctx,
                    )
                });

                ctx.spawn(fut, move |me, repo_path_opt, ctx| {
                    me.maybe_set_new_active_repository(repo_path_opt.as_deref(), ctx);
                });
            }
        }
        model
    }

    /// Given a repository path, sets the new active repository to track diffs for.
    #[cfg(feature = "local_fs")]
    pub fn maybe_set_new_active_repository(
        &mut self,
        repo_path: Option<&Path>,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(repo_path) = repo_path else {
            return;
        };

        // Check if the repository is new.
        if self.repository.as_ref().is_some_and(|repository| {
            repository.as_ref(ctx).root_dir().to_local_path().as_deref() == Some(repo_path)
        }) {
            return;
        }

        let Some(repository_model) =
            DetectedRepositories::as_ref(ctx).get_watched_repo_for_path(repo_path, ctx)
        else {
            return;
        };

        self.set_active_repository(repository_model, ctx);
    }

    pub fn get(&self) -> DiffState {
        match &self.state {
            InternalDiffState::NotInRepository => DiffState::NotInRepository,
            InternalDiffState::Loading => DiffState::Loading,
            InternalDiffState::Loaded(diffs) => match &diffs.changes {
                Ok(git_diff_data) => DiffState::Loaded(git_diff_data.clone()),
                Err(err) => DiffState::Error(err.clone()),
            },
        }
    }

    pub fn get_metadata(&self) -> Option<&DiffMetadataAgainstBase> {
        self.metadata
            .as_ref()
            .and_then(|metadata| match &self.mode {
                DiffMode::Head => Some(&metadata.against_head),
                DiffMode::MainBranch => metadata.against_base_branch.as_ref(),
                DiffMode::OtherBranch(_) => None, // TODO: implement caching for arbitrary branches
            })
    }

    pub fn diff_mode(&self) -> DiffMode {
        self.mode.clone()
    }

    pub fn get_uncommitted_stats(&self) -> Option<DiffStats> {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.against_head.aggregate_stats)
    }

    pub fn get_main_branch_stats(&self) -> Option<DiffStats> {
        self.metadata.as_ref().and_then(|metadata| {
            metadata
                .against_base_branch
                .as_ref()
                .map(|base| base.aggregate_stats)
        })
    }

    /// Get the name of the main branch being used for comparison
    pub fn get_main_branch_name(&self) -> Option<String> {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.main_branch_name.clone())
            .filter(|name| !name.is_empty())
    }

    /// Converts an optional base branch name into a `DiffMode`.
    /// Falls back to `DiffMode::MainBranch` when no branch is specified.
    pub fn diff_mode_for_base_branch(&self, base_branch: Option<&str>) -> DiffMode {
        match base_branch {
            Some(branch) => {
                let main_branch = self
                    .get_main_branch_name()
                    .and_then(|main_branch| main_branch.strip_prefix("origin/").map(String::from));
                DiffMode::from_branch(branch, main_branch.as_deref())
            }
            None => DiffMode::MainBranch,
        }
    }

    /// Get the name of the current checked-out branch or commit.
    pub fn get_current_branch_name(&self) -> Option<String> {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.current_branch_name.clone())
            .filter(|name| !name.is_empty())
    }

    /// Returns true if the current branch is the main/trunk branch.
    pub fn is_on_main_branch(&self) -> bool {
        match (self.get_current_branch_name(), self.get_main_branch_name()) {
            (Some(current), Some(main)) => {
                let main_short = main.strip_prefix("origin/").unwrap_or(&main);
                current == main || current == main_short
            }
            _ => false,
        }
    }

    /// The unpushed commits on the current branch (empty if none or no upstream).
    pub fn unpushed_commits(&self) -> &[Commit] {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.unpushed_commits.as_slice())
            .unwrap_or_default()
    }

    pub fn upstream_ref(&self) -> Option<&str> {
        self.metadata
            .as_ref()
            .and_then(|m| m.upstream_ref.as_deref())
    }

    /// Returns `true` if the branch's upstream tracking ref differs from the
    /// detected main branch. `false` when either is unknown, or when the
    /// upstream points directly at main (e.g. after
    /// `git checkout -b feature origin/master`).
    pub fn upstream_differs_from_main(&self) -> bool {
        match (self.upstream_ref(), self.get_main_branch_name().as_deref()) {
            (Some(upstream), Some(main)) => upstream != main,
            _ => false,
        }
    }

    /// The PR info for the current branch, if one exists.
    pub fn pr_info(&self) -> Option<&PrInfo> {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.pr_info.as_ref())
    }

    /// Whether PR info for the current branch is currently being refreshed.
    pub fn is_pr_info_refreshing(&self) -> bool {
        self.refreshing_pr_info_handle.is_some()
    }

    /// Checks if git operations like stash or reset would be blocked due to repository state.
    /// This performs quick file existence checks to detect if git is in the middle of an operation.
    #[cfg(feature = "local_fs")]
    pub fn is_git_operation_blocked(&self, app: &AppContext) -> bool {
        let Some(repo) = &self.repository else {
            return false;
        };
        let repo_path = repo.as_ref(app).root_dir().to_local_path_lossy();
        crate::util::git::git_operation_in_progress(&repo_path)
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn is_git_operation_blocked(&self, _app: &warpui::AppContext) -> bool {
        false
    }
    /// Get comprehensive git diff information including uncommitted stats, main branch stats, and main branch name
    /// This is a utility function that combines all diff-related information in a single call
    pub fn get_diff_stats(&self) -> GitDiffInfo {
        GitDiffInfo {
            uncommitted_stats: self.get_uncommitted_stats(),
            main_branch_stats: self.get_main_branch_stats(),
            main_branch_name: self.get_main_branch_name(),
        }
    }

    pub fn get_stats_for_current_mode(&self) -> Option<DiffStats> {
        self.get_stats_for_mode(self.mode.clone())
    }

    pub fn get_stats_for_mode(&self, mode: DiffMode) -> Option<DiffStats> {
        let metadata = self.metadata.as_ref()?;
        match mode {
            DiffMode::Head => Some(metadata.against_head.aggregate_stats),
            DiffMode::MainBranch => metadata
                .against_base_branch
                .as_ref()
                .map(|base| base.aggregate_stats),
            DiffMode::OtherBranch(_) => None, // TODO: implement caching for arbitrary branches
        }
    }

    // you'll have a HEAD after the first commit
    pub fn has_head(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(feature = "local_fs")] {
                self
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.has_head_commit)
            } else {
                false
            }
        }
    }

    #[cfg(feature = "local_fs")]
    pub fn active_repository_path(&self, app: &AppContext) -> Option<PathBuf> {
        self.repository
            .as_ref()?
            .as_ref(app)
            .root_dir()
            .to_local_path()
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn active_repository_path(&self, _app: &warpui::AppContext) -> Option<PathBuf> {
        None
    }

    pub fn is_inside_repository(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(feature = "local_fs")] {
                self.repository.is_some()
            } else {
                false
            }
        }
    }

    pub fn set_diff_mode(
        &mut self,
        mode: DiffMode,
        should_fetch_base: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.mode != mode {
            self.mode = mode;
            ctx.emit(DiffStateModelEvent::DiffModeChanged { should_fetch_base });
        }
    }

    /// Like [`Self::set_diff_mode`], but also arranges for the next diff load
    /// to fetch the base branch from origin if it is not available locally.
    /// This is intended for the `insert_code_review_comments` flow where the
    /// requested base branch may never have been checked out.
    pub fn set_diff_mode_and_fetch_base(&mut self, mode: DiffMode, ctx: &mut ModelContext<Self>) {
        self.set_diff_mode(mode, true, ctx);
    }

    /// Loads the actual diffs for the current repo.
    /// See [`DiffStateModel::refresh_diff_metadata_for_current_repo`] for a version that only refreshes diff _metadata.
    #[cfg(feature = "local_fs")]
    pub fn load_diffs_for_current_repo(
        &mut self,
        should_fetch_base: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        // Abort any previous diff loading operations before spawning a new one.
        if let Some(handle) = self.computing_diffs_abort_handle.take() {
            handle.abort();
        }

        let Some(current_repository) = &self.repository else {
            return;
        };
        let current_repository_path = current_repository
            .as_ref(ctx)
            .root_dir()
            .to_local_path_lossy();
        let mode = self.mode.clone();
        self.state = InternalDiffState::Loading;
        self.computing_diffs_abort_handle = Some(ctx.spawn(
            async move {
                Self::load_diffs_for_repo(current_repository_path, mode, should_fetch_base).await
            },
            Self::handle_updated_state_for_repo,
        ));
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn load_diffs_for_current_repo(
        &mut self,
        _should_fetch_base: bool,
        _ctx: &mut ModelContext<Self>,
    ) {
        // Noop on WASM builds.
    }

    /// Stashes uncommitted changes for specific files
    #[cfg(feature = "local_fs")]
    async fn stash_uncommitted_changes(repo_path: &Path, relative_paths: &[String]) -> Result<()> {
        let app_id = ChannelState::app_id();
        let app_name = app_id.application_name();
        let msg = if relative_paths.len() == 1 {
            format!("{app_name}: stash {}", relative_paths[0])
        } else {
            format!("{app_name}: stash {} files", relative_paths.len())
        };

        let mut stash_args = vec!["stash", "push", "-u", "-m", msg.as_str(), "--"];
        for path in relative_paths {
            stash_args.push(path.as_str());
        }

        log::debug!(
            "[GIT OPERATION] diff_state.rs stash_uncommitted_changes git {}",
            stash_args.join(" ")
        );
        let stash_res = run_git_command(repo_path, &stash_args).await;

        match stash_res {
            Ok(_) => Ok(()),
            Err(err) => {
                let err_msg = err.to_string();
                // If there are no local changes to stash, git stash will fail
                // In this case, we can safely ignore the error
                if err_msg.contains("No local changes to save") {
                    Ok(())
                } else {
                    let context = if relative_paths.len() == 1 {
                        relative_paths[0].clone()
                    } else {
                        format!("{} files", relative_paths.len())
                    };
                    Err(anyhow!(
                        "Failed to stash changes for {}: {}",
                        context,
                        err_msg
                    ))
                }
            }
        }
    }
    /// Runs git restore and git clean for one or more files
    #[cfg(feature = "local_fs")]
    async fn git_restore_and_clean(
        repo_path: &Path,
        relative_paths: &[String],
        branch: &str,
    ) -> Result<()> {
        let source_arg = format!("--source={branch}");
        let mut restore_args = vec![
            "restore",
            "--staged",
            "--worktree",
            source_arg.as_str(),
            "--",
        ];
        for path in relative_paths {
            restore_args.push(path.as_str());
        }

        log::debug!(
            "[GIT OPERATION] diff_state.rs git_restore_and_clean git {}",
            restore_args.join(" ")
        );
        let restore_res = run_git_command(repo_path, &restore_args).await;

        match restore_res {
            Ok(_) => {
                // Clean untracked files for these specific paths
                let mut clean_args = vec!["clean", "-fd"];
                for path in relative_paths {
                    clean_args.push(path.as_str());
                }
                log::debug!(
                    "[GIT OPERATION] diff_state.rs git_restore_and_clean git {}",
                    clean_args.join(" ")
                );
                let clean_res = run_git_command(repo_path, &clean_args).await;

                match clean_res {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        log::warn!("Failed to clean untracked files: {err}");
                        Ok(())
                    }
                }
            }
            Err(err) => {
                let err_msg = err.to_string();
                if branch == "HEAD" && err_msg.contains("could not resolve HEAD") {
                    let mut clean_args = vec!["clean", "-fd"];
                    for path in relative_paths {
                        clean_args.push(path.as_str());
                    }
                    log::debug!(
                        "[GIT OPERATION] diff_state.rs git_restore_and_clean git {}",
                        clean_args.join(" ")
                    );
                    let clean_res = run_git_command(repo_path, &clean_args).await;
                    if let Err(err) = clean_res {
                        log::warn!("Failed to clean untracked files: {err}");
                    }
                    Ok(())
                } else if err_msg.contains("did not match any file(s) known to git") {
                    // If some files don't exist in the branch, we need to remove them
                    for file_path in relative_paths {
                        log::debug!(
                            "[GIT OPERATION] diff_state.rs git_restore_and_clean git rm -f -- {file_path}"
                        );
                        let rm_res =
                            run_git_command(repo_path, &["rm", "-f", "--", file_path.as_str()])
                                .await;

                        if let Err(rm_err) = rm_res {
                            let rm_err_msg = rm_err.to_string();
                            if rm_err_msg.contains("did not match any files") {
                                // if the file was staged but it isn't in the working directory,
                                // e.g. it was locally deleted
                                log::debug!(
                                    "[GIT OPERATION] diff_state.rs git_restore_and_clean git reset -- {file_path}"
                                );
                                if let Err(e) =
                                    run_git_command(repo_path, &["reset", "--", file_path.as_str()])
                                        .await
                                {
                                    log::warn!("Failed to unstage file '{file_path}': {e}");
                                }
                            } else {
                                log::warn!("Failed to remove file '{file_path}': {rm_err_msg}");
                            }
                        }

                        if let Err(e) = fs::remove_file(repo_path.join(file_path)) {
                            if e.kind() != std::io::ErrorKind::NotFound {
                                log::warn!(
                                    "Failed to remove file '{file_path}' from filesystem: {e}"
                                );
                            }
                        }
                    }
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Removes files based on the operation type
    ///
    /// `pub(crate)` rather than private: the remote-server daemon's
    /// `DiscardFiles` handler (`app/src/remote_server/server_model.rs`, #437)
    /// calls this directly to run the same git restore/stash/rm logic
    /// against its own filesystem, mirroring how `load_metadata_for_repo` /
    /// `load_diff_data_for_mode` are already reused for `GetDiffState`.
    #[cfg(feature = "local_fs")]
    pub(crate) async fn discard_files_impl(
        repo_path: &Path,
        file_infos: Vec<FileStatusInfo>,
        should_stash: bool,
        branch: &str,
    ) -> Result<()> {
        let mut renamed_file_infos = Vec::new();
        let mut other_file_infos = Vec::new();

        for info in file_infos {
            if matches!(info.status, GitFileStatus::Renamed { .. }) {
                renamed_file_infos.push(info);
            } else {
                other_file_infos.push(info);
            }
        }

        // Handle renamed files specially
        if !renamed_file_infos.is_empty() {
            if branch == "HEAD" && should_stash {
                let renamed_paths: Vec<String> = renamed_file_infos
                    .iter()
                    .map(|info| match info.path.strip_prefix(repo_path) {
                        Ok(rel_path) => rel_path.to_string_lossy().to_string(),
                        Err(_) => info.path.to_string_lossy().to_string(),
                    })
                    .collect();
                Self::stash_uncommitted_changes(repo_path, &renamed_paths).await?;

                for info in &renamed_file_infos {
                    if let GitFileStatus::Renamed { old_path } = &info.status {
                        log::debug!(
                            "[GIT OPERATION] diff_state.rs discard_files_impl git restore --staged --worktree -- {old_path}"
                        );
                        let _ = run_git_command(
                            repo_path,
                            &["restore", "--staged", "--worktree", "--", old_path],
                        )
                        .await;
                    }
                }
            } else {
                for info in renamed_file_infos {
                    if let GitFileStatus::Renamed { old_path } = &info.status {
                        let relative_new_path = match info.path.strip_prefix(repo_path) {
                            Ok(rel) => rel.to_string_lossy().to_string(),
                            Err(_) => info.path.to_string_lossy().to_string(),
                        };

                        // Remove the new file
                        log::debug!(
                            "[GIT OPERATION] diff_state.rs discard_files_impl git rm -f -- {relative_new_path}"
                        );
                        if let Err(e) =
                            run_git_command(repo_path, &["rm", "-f", "--", &relative_new_path])
                                .await
                        {
                            log::warn!("Failed to remove renamed file '{relative_new_path}': {e}");
                        }

                        // Restore the old file from the branch using git checkout
                        // We use checkout instead of restore because the old path doesn't exist in the
                        // working directory yet, and git restore requires the path to exist
                        log::debug!(
                            "[GIT OPERATION] diff_state.rs discard_files_impl git checkout {branch} -- {old_path}"
                        );
                        if let Err(e) =
                            run_git_command(repo_path, &["checkout", branch, "--", old_path]).await
                        {
                            log::error!(
                                "Failed to restore old file '{old_path}' from branch '{branch}': {e}"
                            );
                        }
                    }
                }
            }
        }

        // Handle other files normally
        if !other_file_infos.is_empty() {
            let relative_paths: Vec<String> = other_file_infos
                .iter()
                .map(|info| match info.path.strip_prefix(repo_path) {
                    Ok(rel_path) => rel_path.to_string_lossy().to_string(),
                    Err(_) => info.path.to_string_lossy().to_string(),
                })
                .collect();

            if branch == "HEAD" && should_stash {
                Self::stash_uncommitted_changes(repo_path, &relative_paths).await?;
            } else {
                Self::git_restore_and_clean(repo_path, &relative_paths, branch).await?;
            }
        }
        Ok(())
    }

    /// Discard changes for one or more files
    #[cfg(feature = "local_fs")]
    pub fn discard_files(
        &mut self,
        file_infos: Vec<FileStatusInfo>,
        should_stash: bool,
        branch_name: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(current_repository) = &self.repository else {
            return;
        };
        let current_repository_path = current_repository
            .as_ref(ctx)
            .root_dir()
            .to_local_path_lossy();

        let branch = branch_name.unwrap_or_else(|| "HEAD".to_string());
        ctx.spawn(
            async move {
                Self::discard_files_impl(
                    &current_repository_path,
                    file_infos,
                    should_stash,
                    &branch,
                )
                .await
            },
            |me, result, ctx| match result {
                Ok(_) => {
                    me.load_diffs_for_current_repo(false, ctx);
                    me.refresh_diff_metadata_for_current_repo(
                        InvalidationBehavior::PromptRefresh,
                        ctx,
                    );
                }
                Err(err) => {
                    log::error!("Failed to restore files: {err}");
                    ctx.emit(DiffStateModelEvent::GitWriteFailed {
                        operation: GitWriteOperation::Discard,
                        message: err.to_string(),
                    });
                }
            },
        );
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn discard_files(
        &mut self,
        _file_infos: Vec<FileStatusInfo>,
        _should_stash: bool,
        _branch_name: Option<String>,
        _ctx: &mut ModelContext<Self>,
    ) {
        // Noop on WASM builds.
    }

    /// Stages or un-stages part of the working tree (Zap #329).
    ///
    /// Unlike [`Self::discard_files`] this never touches the working tree, so
    /// there is nothing to stash and no confirmation dialog upstream of it.
    /// It reloads the diff afterwards for the same reason discard does: the
    /// index is not a path the file watcher reports, so nothing else would
    /// tell the view that a file's staged column moved.
    #[cfg(feature = "local_fs")]
    pub fn stage_changes(&mut self, request: StageRequest, ctx: &mut ModelContext<Self>) {
        let Some(current_repository) = &self.repository else {
            return;
        };
        let repo_path = current_repository
            .as_ref(ctx)
            .root_dir()
            .to_local_path_lossy();

        // Read before `request` moves into the future: the failure toast has to
        // name the direction the user actually asked for.
        let operation = if request.unstage {
            GitWriteOperation::Unstage
        } else {
            GitWriteOperation::Stage
        };

        ctx.spawn(
            async move { Self::stage_changes_impl(&repo_path, &request).await },
            move |me, result, ctx| match result {
                Ok(_) => {
                    me.load_diffs_for_current_repo(false, ctx);
                    me.refresh_diff_metadata_for_current_repo(
                        InvalidationBehavior::PromptRefresh,
                        ctx,
                    );
                }
                Err(err) => {
                    log::error!("Failed to stage changes: {err}");
                    // Nothing reloads on this path, so without this the view is
                    // unchanged and the click looks like it did nothing.
                    ctx.emit(DiffStateModelEvent::GitWriteFailed {
                        operation,
                        message: err.to_string(),
                    });
                }
            },
        );
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn stage_changes(&mut self, _request: StageRequest, _ctx: &mut ModelContext<Self>) {
        // Noop on WASM builds.
    }

    /// Runs one staging operation against `repo_path`.
    ///
    /// `pub(crate)` for the same reason as [`Self::discard_files_impl`]: the
    /// remote-server daemon's `GitStage` handler
    /// (`app/src/remote_server/server_model.rs`) calls it directly so the SSH
    /// path runs byte-identical git invocations to the local one.
    #[cfg(feature = "local_fs")]
    pub(crate) async fn stage_changes_impl(
        repo_path: &Path,
        request: &StageRequest,
    ) -> Result<()> {
        match &request.target {
            StageTarget::Paths(paths) => {
                log::debug!(
                    "[GIT OPERATION] diff_state.rs stage_changes_impl {} {}",
                    if request.unstage { "unstage" } else { "stage" },
                    paths.join(" ")
                );
                crate::util::git::run_stage_paths(repo_path, paths, request.unstage).await?;
            }
            StageTarget::Hunk(patch) => {
                log::debug!(
                    "[GIT OPERATION] diff_state.rs stage_changes_impl git apply --cached{} (hunk patch, {} bytes)",
                    if request.unstage { " --reverse" } else { "" },
                    patch.len()
                );
                crate::util::git::run_apply_patch_cached(repo_path, patch, request.unstage).await?;
            }
        }
        Ok(())
    }

    /// Sets whether the code review pane needs diff metadata.
    /// When transitioning from disabled to enabled, triggers an
    /// immediate refresh to catch up on changes that occurred while disabled.
    pub fn set_code_review_metadata_refresh_enabled(
        &mut self,
        enabled: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let was_enabled = self.metadata_refresh_enabled;
        self.metadata_refresh_enabled = enabled;
        if !was_enabled && enabled {
            self.refresh_diff_metadata_for_current_repo(InvalidationBehavior::PromptRefresh, ctx);
        }
    }

    /// Loads diff metadata (aggregate lines added, removed, etc.) for the current repo.
    #[cfg(feature = "local_fs")]
    pub fn refresh_diff_metadata_for_current_repo(
        &mut self,
        invalidation_behavior: InvalidationBehavior,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self.metadata_refresh_enabled {
            return;
        }
        let Some(current_repository) = &self.repository else {
            return;
        };
        let current_repository_path = current_repository
            .as_ref(ctx)
            .root_dir()
            .to_local_path_lossy();
        if let Some(handle) = self.computing_metadata_abort_handle.take() {
            handle.abort();
        }

        // Always include base branch metadata since only code review uses this model now.
        let include_base_branch = true;
        let abort_handle = ctx.spawn(
            async move {
                Self::load_metadata_for_repo(current_repository_path, include_base_branch).await
            },
            move |me, res, ctx| {
                me.handle_updated_metadata_for_repo(res, invalidation_behavior, ctx)
            },
        );
        self.computing_metadata_abort_handle = Some(abort_handle);
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn refresh_diff_metadata_for_current_repo(
        &mut self,
        _invalidation_behavior: InvalidationBehavior,
        _ctx: &mut ModelContext<Self>,
    ) {
        // Noop on WASM builds.
    }

    #[cfg(feature = "local_fs")]
    pub fn set_active_repository(
        &mut self,
        new_repository: ModelHandle<Repository>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Unsubscribe from the old repository.

        use std::time::Duration;

        use crate::throttle::throttle;
        if let Some(old_repository) = &self.repository {
            if let Some(subscriber_id) = self.subscriber_id {
                old_repository.update(ctx, |old_repository, ctx| {
                    old_repository.stop_watching(subscriber_id, ctx);
                })
            }
        }

        ctx.emit(DiffStateModelEvent::RepositoryChanged);

        if let Some(handle) = self.computing_metadata_abort_handle.take() {
            handle.abort();
        }

        // Only kick off the expensive metadata + diff loading when the code
        // review pane is actually open. When the pane opens later, `on_open`
        // calls `set_code_review_metadata_refresh_enabled(true)` (which
        // triggers metadata) and `load_diffs_for_current_repo` (which triggers
        // diffs), so nothing is lost — just deferred.
        if self.metadata_refresh_enabled {
            let new_repository_root = new_repository.as_ref(ctx).root_dir().to_local_path_lossy();
            // Always include base branch metadata since only code review uses this model now.
            let include_base_branch = true;
            let abort_handle = ctx.spawn(
                async move {
                    Self::load_metadata_for_repo(new_repository_root, include_base_branch).await
                },
                move |me, res, ctx| {
                    me.handle_updated_metadata_for_repo(
                        res,
                        InvalidationBehavior::All(InvalidationSource::MetadataChange),
                        ctx,
                    )
                },
            );

            self.computing_metadata_abort_handle = Some(abort_handle);
            self.state = InternalDiffState::Loading;
        }

        // Assign the repository handle before spawning the registration future.
        // `registration_future` may resolve immediately (e.g. watcher already active), and
        // `handle_repository_updated` relies on `self.repository` being available for cleanup.
        self.repository = Some(new_repository.clone());

        let (repository_update_tx, repository_update_rx) = async_channel::unbounded();
        let (throttled_repository_update_tx, throttled_repository_update_rx) =
            async_channel::unbounded();
        let start = new_repository.update(ctx, |new_repository, ctx| {
            new_repository.start_watching(
                RepositoryWatchMode::GitRepository,
                Box::new(DiffStateModelRepositorySubscriber {
                    repository_update_tx,
                }),
                ctx,
            )
        });
        self.subscriber_id = Some(start.subscriber_id);
        ctx.spawn(start.registration_future, Self::handle_repository_updated);

        ctx.spawn_stream_local(
            repository_update_rx.clone(),
            move |me, item, ctx| match item {
                DiffStateRepositoryUpdate::Invalidation(update) => {
                    if me.handle_file_update(update, ctx) {
                        log::debug!("[GIT OPERATION] handle_file_update found changes");
                        let throttled_repository_update_tx_clone =
                            throttled_repository_update_tx.clone();
                        ctx.background_executor()
                            .spawn(async move {
                                let _ = throttled_repository_update_tx_clone.send(()).await;
                            })
                            .detach();
                    } else {
                        log::debug!("[GIT OPERATION] No changes found no metadata update.");
                    }
                }
                DiffStateRepositoryUpdate::InvalidationWithLockedIndex => {
                    ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
                        InvalidationBehavior::AllLockedIndex,
                    ));
                }
            },
            |_, _| {},
        );

        // Only update metadata at most once every 5 seconds. The code review pane
        // will refresh diffs at a faster rate if it's open (from the stream above).
        ctx.spawn_stream_local(
            throttle(Duration::from_secs(5), throttled_repository_update_rx),
            |me, _, ctx| {
                me.refresh_diff_metadata_for_current_repo(InvalidationBehavior::PromptRefresh, ctx);
            },
            |_, _| {},
        );
    }

    #[cfg(feature = "local_fs")]
    /// Processes a repository file-system update, emitting a `DiffMetadataChanged` event
    /// when relevant (non-ignored) files are affected.
    ///
    /// Returns `true` if a metadata refresh should be triggered.
    fn handle_file_update(
        &mut self,
        update: RepositoryUpdate,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(invalidation_behavior) = classify_repository_update(update) else {
            return false;
        };
        ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
            invalidation_behavior,
        ));
        true
    }

    #[cfg(feature = "local_fs")]
    fn handle_repository_updated(
        &mut self,
        result: Result<(), RepoMetadataError>,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Err(err) = result {
            log::warn!("Could not update repository: {err}");

            let Some(repository) = self.repository.as_ref() else {
                return;
            };
            let Some(subscriber_id) = self.subscriber_id.take() else {
                return;
            };

            repository.update(ctx, |repo, ctx| {
                repo.stop_watching(subscriber_id, ctx);
            });
        }
    }

    /// Stops watching the active repository without clearing other state.
    /// Called when this model is about to be dropped to ensure the underlying
    /// `DirectoryWatcher` can be cleaned up.
    #[cfg(feature = "local_fs")]
    pub fn stop_active_watcher(&mut self, ctx: &mut ModelContext<Self>) {
        if let Some(repository) = &self.repository {
            if let Some(subscriber_id) = self.subscriber_id.take() {
                repository.update(ctx, |repo, ctx| {
                    repo.stop_watching(subscriber_id, ctx);
                });
            }
        }
    }

    #[cfg(feature = "local_fs")]
    pub fn remove_active_repo(&mut self, ctx: &mut ModelContext<Self>) {
        let Some(repository) = &self.repository else {
            return;
        };

        // Unsubscribe from the repository watcher before releasing the handle.
        if let Some(subscriber_id) = self.subscriber_id.take() {
            repository.update(ctx, |repo, ctx| {
                repo.stop_watching(subscriber_id, ctx);
            });
        }

        let repository = self.repository.take().unwrap();
        ctx.unsubscribe_from_model(&repository);
        self.state = InternalDiffState::NotInRepository;
        ctx.emit(DiffStateModelEvent::RepositoryChanged);
        ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
            InvalidationBehavior::All(InvalidationSource::MetadataChange),
        ));
    }

    /// Gets the merge base between HEAD and the specified branch
    async fn get_merge_base(repo_path: &Path, branch: &str) -> Result<String> {
        log::debug!("[GIT OPERATION] diff_state.rs get_merge_base git merge-base HEAD {branch}");
        let output = run_git_command(repo_path, &["merge-base", "HEAD", branch]).await?;
        Ok(output.trim().to_string())
    }

    /// Computes the merge base for the given diff mode.
    ///
    /// For [`DiffMode::MainBranch`] the main branch is detected automatically;
    /// for [`DiffMode::OtherBranch`] the provided branch name is used directly.
    /// [`DiffMode::Head`] does not have a merge base and returns an error.
    pub(crate) async fn compute_merge_base(repo_path: &Path, mode: &DiffMode) -> Result<String> {
        let branch = match mode {
            DiffMode::MainBranch => detect_main_branch(repo_path).await?,
            DiffMode::OtherBranch(branch) => branch.clone(),
            DiffMode::Head => {
                anyhow::bail!("merge base is not applicable for Head mode")
            }
        };
        Self::get_merge_base(repo_path, &branch).await
    }

    /// Like [`Self::get_merge_base`], but if the branch ref doesn't exist
    /// locally, attempts to fetch it from the `origin` remote so that
    /// `git merge-base` can succeed even when the base branch was never
    /// checked out in this working copy.
    async fn get_or_fetch_merge_base(repo_path: &Path, branch: &str) -> Result<String> {
        // Fast path: the ref already exists locally (local branch or remote-tracking ref).
        if let Ok(merge_base) = Self::get_merge_base(repo_path, branch).await {
            return Ok(merge_base);
        }

        // The ref may not be present locally. Try the remote-tracking ref
        // before hitting the network.
        let origin_branch = format!("origin/{branch}");
        if let Ok(merge_base) = Self::get_merge_base(repo_path, &origin_branch).await {
            return Ok(merge_base);
        }

        // Fetch the branch from origin. This creates / updates the remote-tracking
        // ref `origin/<branch>` without altering the working tree.
        log::warn!("Base branch '{branch}' not found locally, fetching from origin");
        run_git_command(repo_path, &["fetch", "origin", branch]).await?;

        // Retry with the now-available remote-tracking ref.
        Self::get_merge_base(repo_path, &origin_branch).await
    }

    pub async fn load_metadata_for_repo(
        repo_path: PathBuf,
        include_base_branch: bool,
    ) -> Result<DiffMetadata> {
        // Detect the main branch name first
        let main_branch_name = detect_main_branch(&repo_path).await?;
        let current_branch_name = detect_current_branch(&repo_path).await?;

        log::debug!("[GIT OPERATION] diff_state.rs load_metadata_for_repo git rev-parse HEAD");
        let has_head_commit = run_git_command(&repo_path, &["rev-parse", "HEAD"])
            .await
            .is_ok();

        let diff_against_head = Self::diff_metadata_against_head(&repo_path).await?;
        let diff_against_base_branch = if include_base_branch {
            Some(Self::diff_metadata_against_specific_branch(&repo_path, &main_branch_name).await?)
        } else {
            None
        };

        let (unpushed_commits, upstream_ref) =
            if FeatureFlag::GitOperationsInCodeReview.is_enabled() {
                let upstream_branch = run_git_command(
                    &repo_path,
                    &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
                )
                .await
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
                let unpushed = get_unpushed_commits(
                    &repo_path,
                    Some(current_branch_name.as_str()),
                    upstream_branch.as_deref(),
                )
                .await
                .unwrap_or_default();
                (unpushed, upstream_branch)
            } else {
                (Vec::new(), None)
            };

        Ok(DiffMetadata {
            main_branch_name,
            current_branch_name,
            against_head: diff_against_head,
            against_base_branch: diff_against_base_branch,
            has_head_commit,
            unpushed_commits,
            upstream_ref,
            pr_info: None,
        })
    }

    /// Gets diff data for the specified mode, checking the model's internal state first.
    ///
    /// This should rarely be used, because this does not cache the result.
    /// Normally, you should use DiffStateModel.get() to get the current diff state.
    /// This is useful to read a specific mode that might not be the current mode.
    #[cfg(feature = "local_fs")]
    pub fn get_diff_data_for_mode(
        &self,
        mode: DiffMode,
        repo_path: PathBuf,
    ) -> impl Future<Output = Option<GitDiffData>> + use<> {
        // Check if we have the data already loaded for this mode
        if let InternalDiffState::Loaded(diffs) = &self.state {
            if self.mode == mode {
                if let Ok(data) = &diffs.changes {
                    return Either::Left(future::ready(Some(data.clone())));
                }
            }
        }

        // Data not cached - need to load it
        Either::Right(async {
            let diffs = Self::load_diffs_for_repo(repo_path, mode, false).await;
            diffs.changes.ok().map(|diff| diff.into())
        })
    }

    /// Load diff data for a given mode without requiring an existing model instance.
    /// Unlike `get_diff_data_for_mode`, this always loads fresh data from disk.
    #[cfg(feature = "local_fs")]
    pub async fn load_diff_data_for_mode(
        mode: DiffMode,
        repo_path: PathBuf,
    ) -> Option<GitDiffData> {
        let diffs = Self::load_diffs_for_repo(repo_path, mode, false).await;
        diffs.changes.ok().map(|diff| diff.into())
    }

    /// Like [`Self::load_diff_data_for_mode`], but keeps each file's base content
    /// instead of dropping it via the `GitDiffWithBaseContent -> GitDiffData`
    /// conversion.
    ///
    /// Used by the remote-server daemon's `GetDiffState` reply and live-push paths
    /// (`app/src/remote_server/server_model.rs`), which need the base content to
    /// serialize `content_at_base` onto the wire for the client's inline diff
    /// decorations (issue #388 item 4) — `load_diff_data_for_mode` computes the same
    /// `GitDiffWithBaseContent` internally and then throws the content away before
    /// the daemon ever gets to send it.
    #[cfg(feature = "local_fs")]
    pub async fn load_diff_with_base_content_for_mode(
        mode: DiffMode,
        repo_path: PathBuf,
    ) -> Option<GitDiffWithBaseContent> {
        let diffs = Self::load_diffs_for_repo(repo_path, mode, false).await;
        diffs.changes.ok()
    }

    async fn load_diffs_for_repo(
        repo_path: PathBuf,
        mode: DiffMode,
        should_fetch_base: bool,
    ) -> DiffsWithBaseContent {
        let diffs = match mode {
            DiffMode::Head => Self::diff_state_against_head(&repo_path).await,
            DiffMode::MainBranch => {
                Self::diff_state_against_base_branch(&repo_path, should_fetch_base).await
            }
            DiffMode::OtherBranch(branch) => {
                Self::diff_state_against_specific_branch(&repo_path, branch, should_fetch_base)
                    .await
            }
        };

        DiffsWithBaseContent {
            changes: diffs.map_err(|err| err.to_string()),
            repository_path: repo_path,
        }
    }

    fn handle_updated_metadata_for_repo(
        &mut self,
        metadata: Result<DiffMetadata>,
        invalidation_behavior: InvalidationBehavior,
        ctx: &mut ModelContext<Self>,
    ) {
        let previous_branch = self
            .metadata
            .as_ref()
            .map(|metadata| metadata.current_branch_name.clone());
        let previous_pr_info = self
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pr_info.clone());

        match metadata {
            Ok(mut metadata) => {
                // Carry forward cached PR info while refresh_pr_info is pending
                // so the header doesn't flash to Commit/Create PR between
                // branch switch and PR lookup completion.
                metadata.pr_info = previous_pr_info;
                self.metadata = Some(metadata);
            }
            Err(e) => {
                send_telemetry_from_ctx!(
                    CodeReviewTelemetryEvent::CalculateDiffMetadataFailed {
                        error: e.to_string()
                    },
                    ctx
                );
                self.metadata = None;
            }
        }

        let current_branch = self
            .metadata
            .as_ref()
            .map(|metadata| metadata.current_branch_name.clone());

        if previous_branch != current_branch {
            ctx.emit(DiffStateModelEvent::CurrentBranchChanged);

            // Refresh PR info on branch change (network call, not on every tick).
            if FeatureFlag::GitOperationsInCodeReview.is_enabled() {
                self.refresh_pr_info(ctx);
            }
        } else if FeatureFlag::GitOperationsInCodeReview.is_enabled()
            && self.pr_info().is_none()
            && !self.is_pr_info_refreshing()
            && self.pr_info_attempted_for_branch != current_branch
        {
            // Initial-load fallback: if metadata arrived without a successful
            // PR lookup yet on this branch, try once. Gated by
            // `pr_info_attempted_for_branch` so subsequent metadata refreshes
            // (every fs event on the repo) don't re-fire `gh pr view` when the
            // branch has no PR or the lookup failed.
            self.refresh_pr_info(ctx);
        }

        ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
            invalidation_behavior,
        ));
    }

    #[cfg(feature = "local_fs")]
    fn handle_updated_state_for_repo(
        &mut self,
        diffs: DiffsWithBaseContent,
        ctx: &mut ModelContext<Self>,
    ) {
        if Some(diffs.repository_path.as_path()) != self.active_repository_path(ctx).as_deref() {
            // The current repository is different from the one we fetched state for.
            // Refetch metadata for this new repository (which will trigger any downstream consumers to
            // recompute diffs if they so please.
            self.refresh_diff_metadata_for_current_repo(
                InvalidationBehavior::All(InvalidationSource::MetadataChange),
                ctx,
            );
            return;
        }

        if let Err(e) = &diffs.changes {
            send_telemetry_from_ctx!(
                CodeReviewTelemetryEvent::LoadDiffFailed {
                    error: e.to_string(),
                },
                ctx
            );
        }

        self.state = InternalDiffState::Loaded((&diffs).into());
        ctx.emit(DiffStateModelEvent::NewDiffsComputed(
            diffs.changes.ok().map(Arc::new),
        ));
    }

    /// Returns the number of lines in a given file. Returns `None` if the file is a binary file
    /// or if it's larger than 20MB.
    async fn num_lines_in_file_if_non_binary(file_path: &Path) -> Result<Option<usize>> {
        const MAX_FILE_SIZE_TO_READ_LINES: u64 = 20 * 1000 * 1000; // 20MB

        // Size of the chunk of file to read to determine whether it's binary.
        const CHUNK_SIZE: usize = 1024;

        let mut file = File::open(file_path)?;
        let mut buffer = vec![0; CHUNK_SIZE];

        let n = file.read(&mut buffer)?;
        buffer.truncate(n);

        if warp_util::file_type::is_buffer_binary(&buffer) {
            return Ok(None);
        }
        file.rewind()?;

        if file.metadata()?.len() > MAX_FILE_SIZE_TO_READ_LINES {
            return Ok(None);
        }

        Self::num_lines_in_file(file).await.map(Some)
    }

    /// Returns the number of lines in a file. This is optimized to avoid loading the entire file into memory.
    async fn num_lines_in_file(file: File) -> Result<usize> {
        // Read in chunks of 64 KBs.
        const CHUNK_SIZE: usize = 1024 * 64;

        let mut reader = BufReader::with_capacity(CHUNK_SIZE, file);
        let mut count = 0;
        loop {
            let len = {
                let buf = reader.fill_buf()?;
                if buf.is_empty() {
                    break;
                }
                count += bytecount::count(buf, b'\n');
                buf.len()
            };
            reader.consume(len);
            // Yield so that an attempt to abort this operation is handled.
            futures_lite::future::yield_now().await;
        }
        Ok(count)
    }

    pub async fn diff_metadata_against_head(repo_path: &Path) -> Result<DiffMetadataAgainstBase> {
        // First, get the list of changed files with their status
        log::debug!(
            "[GIT OPERATION] diff_state.rs diff_metadata_against_head git --no-optional-locks status --untracked-files=all --branch --porcelain=2 -z"
        );
        let status_output = run_git_command(
            repo_path,
            &[
                "--no-optional-locks", // Avoid taking locks that might interfere with other git operations
                "status",
                "--untracked-files=all", // Get all untracked files
                "--branch",              // Get branch information
                "--porcelain=2",         // Use porcelain=2 to match git desktop implementation
                "-z",                    // Split output by null characters
            ],
        )
        .await?;

        let changed_files = Self::parse_git_status(&status_output)?;
        let num_stat_metadata = Self::get_diff_metadata_using_numstat(repo_path, "HEAD").await?;

        let mut total_additions = 0;
        let mut total_deletions = 0;

        for (file_path, status) in &changed_files {
            if let Some(metadata) = num_stat_metadata.get(file_path) {
                total_additions += metadata.lines_added;
                total_deletions += metadata.lines_removed;
            } else if matches!(status, GitFileStatus::Untracked) {
                // Degrade per entry: an uncountable untracked path (e.g. a nested
                // repo/worktree directory, or an unreadable file) contributes 0
                // lines instead of failing the whole metadata computation.
                let num_lines = Self::num_lines_in_file_if_non_binary(&repo_path.join(file_path))
                    .await
                    .unwrap_or_else(|err| {
                        log::debug!(
                            "Could not count lines for untracked entry {}: {err}",
                            file_path.display()
                        );
                        None
                    });
                total_additions += num_lines.unwrap_or(0);
            }
        }

        Ok(DiffMetadataAgainstBase {
            aggregate_stats: DiffStats {
                files_changed: changed_files.len(),
                total_additions,
                total_deletions,
            },
        })
    }

    async fn file_statuses_against_head(repo_path: &Path) -> Result<Vec<GitStatusEntry>> {
        // First, get the list of changed files with their status
        log::debug!(
            "[GIT OPERATION] diff_state.rs file_statuses_against_head git --no-optional-locks status --untracked-files=all --branch --porcelain=2 -z"
        );
        let status_output = run_git_command(
            repo_path,
            &[
                "--no-optional-locks", // Avoid taking locks that might interfere with other git operations
                "status",
                "--untracked-files=all", // Get all untracked files
                "--branch",              // Get branch information
                "--porcelain=2",         // Use porcelain=2 to match git desktop implementation
                "-z",                    // Split output by null characters
            ],
        )
        .await?;

        Self::parse_git_status_entries(&status_output)
    }

    async fn diff_state_against_head(repo_path: &Path) -> Result<GitDiffWithBaseContent> {
        let changed_files = Self::file_statuses_against_head(repo_path).await?;

        // Get binary file information using git diff --numstat
        let binary_files = Self::get_binary_files(repo_path).await?;

        // Then get the diff for each file
        let mut files = Vec::new();
        let mut total_additions = 0;
        let mut total_deletions = 0;

        for GitStatusEntry {
            path: file_path,
            status,
            staged,
        } in changed_files
        {
            let is_binary = binary_files.contains(&file_path);
            let mut file_diff =
                Self::get_file_diff(repo_path, &file_path, &status, Some(staged), is_binary, None)
                    .await?;
            // Never read or ship base content for binary files: it can't be
            // inline-rendered and, after lossy UTF-8 decoding, can balloon ~3x.
            let content_at_head = if is_binary {
                None
            } else {
                Self::get_file_content_at_head(repo_path, &file_path, &status).await
            };

            file_diff.is_autogenerated =
                super::is_file_autogenerated(&file_path, content_at_head.as_deref());

            total_additions += file_diff.additions();
            total_deletions += file_diff.deletions();

            files.push(FileDiffAndContent {
                file_diff,
                content_at_head,
            });
        }

        Ok(GitDiffWithBaseContent {
            files_changed: files.len(),
            files,
            total_additions,
            total_deletions,
        })
    }

    async fn diff_state_against_base_branch(
        repo_path: &Path,
        should_fetch_base: bool,
    ) -> Result<GitDiffWithBaseContent> {
        // First detect the main branch
        let main_branch = detect_main_branch(repo_path).await?;

        Self::diff_state_against_specific_branch(repo_path, main_branch, should_fetch_base).await
    }

    /// Returns the per-file status by running a scoped `git status` (Head mode)
    /// or `git diff --name-status` (base-branch mode) limited to a single path.
    ///
    /// The third tuple element is the index state (Zap #329). It is `Some`
    /// only in the Head arm: `git diff --name-status` against a merge base has
    /// no index column, and the untracked fallback below is reached precisely
    /// when the path is *not* in the index, so neither base-branch answer
    /// would mean anything.
    async fn file_status_for_path(
        repo_path: &Path,
        file: &Path,
        mode: &DiffMode,
        merge_base: Option<&str>,
    ) -> Result<Option<(PathBuf, GitFileStatus, Option<FileStagedState>)>> {
        let relative = file
            .strip_prefix(repo_path)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| file.to_path_buf());
        let rel_str = relative.to_str().ok_or_else(|| anyhow!("non-UTF-8 path"))?;

        match (mode, merge_base) {
            (DiffMode::Head, _) => {
                log::debug!(
                    "[GIT OPERATION] diff_state.rs file_status_for_path git status -- {rel_str}"
                );
                let output = run_git_command(
                    repo_path,
                    &[
                        "--no-optional-locks",
                        "status",
                        "--porcelain=2",
                        "-z",
                        "--",
                        rel_str,
                    ],
                )
                .await?;
                let statuses = Self::parse_git_status_entries(&output)?;
                Ok(statuses
                    .into_iter()
                    .find(|entry| entry.path == relative)
                    .map(|entry| (entry.path, entry.status, Some(entry.staged))))
            }
            (_, Some(base)) => {
                log::debug!(
                    "[GIT OPERATION] diff_state.rs file_status_for_path git diff --name-status -z {base} -- {rel_str}"
                );
                let diff_output = run_git_command(
                    repo_path,
                    &["diff", "--name-status", "-z", base, "--", rel_str],
                )
                .await?;

                if let Some((path, status)) = Self::parse_git_diff_name_status(&diff_output)?
                    .into_iter()
                    .find(|(p, _)| *p == relative)
                {
                    return Ok(Some((path, status, None)));
                }

                // The file may be untracked (not in the base diff). Fall back to
                // `git status` scoped to this path to detect untracked files.
                log::debug!(
                    "[GIT OPERATION] diff_state.rs file_status_for_path git status -- {rel_str} (untracked fallback)"
                );
                let status_output =
                    run_git_command(repo_path, &["status", "--porcelain=2", "-z", "--", rel_str])
                        .await?;
                let status_files = Self::parse_git_status(&status_output)?;
                Ok(status_files
                    .into_iter()
                    .find(|(p, s)| *p == relative && matches!(s, GitFileStatus::Untracked))
                    .map(|(path, status)| (path, status, None)))
            }
            _ => Ok(None),
        }
    }

    /// Checks whether a single file is binary by running a scoped
    /// `git diff --numstat <commit> -- <file>`.
    async fn is_file_binary(repo_path: &Path, file: &Path, commit: &str) -> Result<bool> {
        let relative = file
            .strip_prefix(repo_path)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| file.to_path_buf());
        let rel_str = relative.to_str().ok_or_else(|| anyhow!("non-UTF-8 path"))?;

        log::debug!(
            "[GIT OPERATION] diff_state.rs is_file_binary git diff --numstat {commit} -- {rel_str}"
        );
        let output =
            match run_git_command(repo_path, &["diff", "--numstat", commit, "--", rel_str]).await {
                Ok(o) => o,
                Err(_) => return Ok(false),
            };

        // numstat output: "<add>\t<del>\t<file>" — binary files use "-\t-".
        Ok(output
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("-\t-")))
    }

    /// Retrieves the diff state for a single invalidated file using scoped
    /// per-file git commands instead of full-repo operations.
    ///
    /// Returns `(relative_path, Option<FileDiffAndContent>)` — `None` when the
    /// file is no longer part of the diff (e.g. reverted).
    pub async fn retrieve_diff_state(
        repo_path: &Path,
        file: &Path,
        mode: &DiffMode,
        merge_base: Option<&str>,
    ) -> Result<(PathBuf, Option<FileDiffAndContent>)> {
        let relative = file
            .strip_prefix(repo_path)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| file.to_path_buf());

        let Some((file_path, status, staged)) =
            Self::file_status_for_path(repo_path, file, mode, merge_base).await?
        else {
            // File is no longer part of the diff.
            return Ok((relative, None));
        };

        let commit = match mode {
            DiffMode::Head => "HEAD",
            _ => merge_base.unwrap_or("HEAD"),
        };
        let is_binary = Self::is_file_binary(repo_path, file, commit).await?;

        let diff = Self::file_diff_for_path(
            is_binary,
            repo_path,
            &file_path,
            &status,
            staged,
            merge_base,
        )
        .await?;

        Ok((relative, diff))
    }

    async fn file_diff_for_path(
        is_binary: bool,
        repo_path: &Path,
        file_path: &PathBuf,
        status: &GitFileStatus,
        staged: Option<FileStagedState>,
        merge_base: Option<&str>,
    ) -> Result<Option<FileDiffAndContent>> {
        let mut file_diff =
            Self::get_file_diff(repo_path, file_path, status, staged, is_binary, merge_base)
                .await?;

        // Skip files that have no actual changes (empty hunks and not binary)
        // Also skip files with no additions or deletions (no real changes) except for renamed or new files.
        if !is_binary
            && (file_diff.hunks.is_empty() || file_diff.is_empty())
            && !status.is_renamed()
            && !status.is_new_file()
        {
            return Ok(None);
        }

        // Never read or ship base content for binary files: it can't be
        // inline-rendered and, after lossy UTF-8 decoding, can balloon ~3x.
        let content_at_head = if is_binary {
            None
        } else {
            match &merge_base {
                Some(base) => {
                    match status {
                        GitFileStatus::New | GitFileStatus::Untracked => {
                            // For new and untracked files that don't exist in the merge base,
                            // provide empty baseline content. The diff hunks will show
                            // all content as additions, which is the correct representation.
                            // Non-file entries (e.g. nested repo/worktree directories) get
                            // no baseline so no editor is constructed for them.
                            repo_path.join(file_path).is_file().then(String::new)
                        }
                        GitFileStatus::Renamed { old_path } => {
                            // The original content is in the old path of the given base commit.
                            Self::get_file_content_at_commit(repo_path, old_path, base).await
                        }
                        _ => {
                            Self::get_file_content_at_commit(
                                repo_path,
                                &file_path.to_string_lossy(),
                                base,
                            )
                            .await
                        }
                    }
                }
                None => Self::get_file_content_at_head(repo_path, file_path, status).await,
            }
        };

        file_diff.is_autogenerated =
            super::is_file_autogenerated(file_path, content_at_head.as_deref());

        Ok(Some(FileDiffAndContent {
            file_diff,
            content_at_head,
        }))
    }

    async fn file_statuses_against_base(
        repo_path: &Path,
        merge_base: &str,
    ) -> Result<Vec<(PathBuf, GitFileStatus)>> {
        log::debug!(
            "[GIT OPERATION] diff_state.rs file_statuses_against_base git diff --name-status -z {merge_base}"
        );
        let diff_output =
            run_git_command(repo_path, &["diff", "--name-status", "-z", merge_base]).await?;

        let mut changed_files = if diff_output.trim().is_empty() {
            // No tracked changes, but we might have untracked files
            Vec::new()
        } else {
            Self::parse_git_diff_name_status(&diff_output)?
        };

        // Also get untracked files, as they should be included in branch comparisons
        log::debug!(
            "[GIT OPERATION] diff_state.rs file_statuses_against_base git status --untracked-files=all --porcelain=2 -z"
        );
        let status_output = run_git_command(
            repo_path,
            &["status", "--untracked-files=all", "--porcelain=2", "-z"],
        )
        .await?;

        let status_files = Self::parse_git_status(&status_output)?;

        // Add untracked files to the changed files list
        for (file_path, status) in status_files {
            if matches!(status, GitFileStatus::Untracked) {
                changed_files.push((file_path, status));
            }
        }

        Ok(changed_files)
    }

    /// Diff against a specific branch (similar to main branch but with custom branch name)
    async fn diff_state_against_specific_branch(
        repo_path: &Path,
        branch: String,
        should_fetch_base: bool,
    ) -> Result<GitDiffWithBaseContent> {
        // Get the merge base between HEAD and the specified branch.
        let merge_base_result = if should_fetch_base {
            Self::get_or_fetch_merge_base(repo_path, &branch).await
        } else {
            Self::get_merge_base(repo_path, &branch).await
        };
        let merge_base = match merge_base_result {
            Ok(merge_base) => merge_base,
            Err(err) => {
                log::warn!("Could not determine merge base against branch {branch}: {err:?}");
                return Ok(GitDiffWithBaseContent {
                    files_changed: 0,
                    files: Vec::new(),
                    total_additions: 0,
                    total_deletions: 0,
                });
            }
        };

        let changed_files = Self::file_statuses_against_base(repo_path, &merge_base).await?;

        // If we have no changes at all (tracked or untracked), return empty result
        if changed_files.is_empty() {
            return Ok(GitDiffWithBaseContent {
                files_changed: 0,
                files: Vec::new(),
                total_additions: 0,
                total_deletions: 0,
            });
        }

        // Get binary file information using git diff --numstat against merge base
        let binary_files = Self::get_binary_files_vs_commit(repo_path, &merge_base).await?;

        // Get the diff for each file
        let mut files = Vec::new();
        let mut total_additions = 0;
        let mut total_deletions = 0;

        for (file_path, status) in changed_files {
            let is_binary = binary_files.contains(&file_path);
            let file_diff = Self::file_diff_for_path(
                is_binary,
                repo_path,
                &file_path,
                &status,
                // Base-branch mode has no index to be staged against.
                None,
                Some(&merge_base),
            )
            .await?;

            if let Some(file_diff) = file_diff {
                total_additions += file_diff.file_diff.additions();
                total_deletions += file_diff.file_diff.deletions();

                files.push(file_diff);
            }
        }

        Ok(GitDiffWithBaseContent {
            files_changed: files.len(),
            files,
            total_additions,
            total_deletions,
        })
    }

    /// Diff against a specific branch (similar to main branch but with custom branch name)
    async fn diff_metadata_against_specific_branch(
        repo_path: &Path,
        branch: &str,
    ) -> Result<DiffMetadataAgainstBase> {
        // Get the merge base between HEAD and the main branch
        let Ok(merge_base) = Self::get_merge_base(repo_path, branch).await else {
            // If we can't get the merge base, return empty metadata
            log::warn!("Could not determine merge base against branch {branch}",);
            return Ok(DiffMetadataAgainstBase::default());
        };

        // Get the diff between working directory and merge base with full patch output
        log::debug!(
            "[GIT OPERATION] diff_state.rs diff_metadata_against_specific_branch git diff --name-status -z {merge_base}"
        );
        let diff_output =
            run_git_command(repo_path, &["diff", "--name-status", "-z", &merge_base]).await?;

        let mut changed_files = if diff_output.trim().is_empty() {
            // No tracked changes, but we might have untracked files
            Vec::new()
        } else {
            Self::parse_git_diff_name_status(&diff_output)?
        };

        // Also get untracked files, as they should be included in branch comparisons
        log::debug!(
            "[GIT OPERATION] diff_state.rs diff_metadata_against_specific_branch git status --untracked-files=all --porcelain=2 -z"
        );
        let status_output = run_git_command(
            repo_path,
            &["status", "--untracked-files=all", "--porcelain=2", "-z"],
        )
        .await?;

        let status_files = Self::parse_git_status(&status_output)?;

        // Add untracked files to the changed files list
        for (file_path, status) in status_files {
            if matches!(status, GitFileStatus::Untracked) {
                changed_files.push((file_path, status));
            }
        }

        // If we have no changes at all (tracked or untracked), return empty result
        if changed_files.is_empty() {
            return Ok(DiffMetadataAgainstBase::default());
        }

        let num_stat_metadata =
            Self::get_diff_metadata_using_numstat(repo_path, &merge_base).await?;

        let mut total_additions = 0;
        let mut total_deletions = 0;

        for (file_path, status) in &changed_files {
            if let Some(metadata) = num_stat_metadata.get(file_path) {
                total_additions += metadata.lines_added;
                total_deletions += metadata.lines_removed;
            } else if matches!(status, GitFileStatus::Untracked) {
                // Degrade per entry: an uncountable untracked path (e.g. a nested
                // repo/worktree directory, or an unreadable file) contributes 0
                // lines instead of failing the whole metadata computation.
                let num_lines = Self::num_lines_in_file_if_non_binary(&repo_path.join(file_path))
                    .await
                    .unwrap_or_else(|err| {
                        log::debug!(
                            "Could not count lines for untracked entry {}: {err}",
                            file_path.display()
                        );
                        None
                    });
                total_additions += num_lines.unwrap_or(0);
            }
        }

        Ok(DiffMetadataAgainstBase {
            aggregate_stats: DiffStats {
                files_changed: changed_files.len(),
                total_additions,
                total_deletions,
            },
        })
    }

    /// Gets git branches, sorted by commit date (most recent first)
    /// Returns a list of (branch_name, is_main_branch) tuples
    /// Defaults to the most recent 100 branches for performance
    pub async fn get_all_branches(
        repo_path: &Path,
        max_branch_count: Option<usize>,
        include_remotes: bool,
    ) -> Result<Vec<(String, bool)>> {
        let main_branch = match detect_main_branch(repo_path).await {
            Ok(branch) => branch,
            Err(err) => {
                log::warn!("Failed to detect main branch: {err}");
                "origin/main".to_string()
            }
        };
        Self::fetch_branch_list_with_main(
            repo_path,
            &main_branch,
            max_branch_count,
            include_remotes,
        )
        .await
    }

    /// Like [`Self::get_all_branches`] but with a pre-known main branch, skipping
    /// [`detect_main_branch`].
    ///
    /// Use this when the main branch is already cached from a previous call to avoid
    /// the up-to-6 sequential subprocess calls that detection may require.
    pub async fn get_all_branches_with_known_main(
        repo_path: &Path,
        main_branch: &str,
        max_branch_count: Option<usize>,
        include_remotes: bool,
    ) -> Result<Vec<(String, bool)>> {
        Self::fetch_branch_list_with_main(repo_path, main_branch, max_branch_count, include_remotes)
            .await
    }

    /// Returns the committed-only changed files for the current branch against
    /// its merge base with the main branch, as `(path, additions, deletions)`.
    ///
    /// This is the `main...HEAD` committed diff (`git diff --numstat
    /// <merge_base> HEAD`): no working-tree edits, no untracked files. Binary
    /// files render as `-\t-\t<path>`; unparsable counts fall back to 0.
    /// Reused by the remote-server `GetCommittedBranchFiles` handler to back
    /// code review over SSH.
    pub async fn get_committed_branch_file_entries(
        repo_path: &Path,
    ) -> Result<Vec<(String, u64, u64)>> {
        let main_branch = detect_main_branch(repo_path).await?;
        let merge_base =
            match run_git_command(repo_path, &["merge-base", "HEAD", main_branch.trim()]).await {
                Ok(output) => output.trim().to_string(),
                Err(err) => {
                    log::warn!(
                        "Could not determine merge base against branch {main_branch}: {err:?}"
                    );
                    return Ok(Vec::new());
                }
            };

        let output = run_git_command(repo_path, &["diff", "--numstat", &merge_base, "HEAD"])
            .await
            .unwrap_or_default();
        let mut entries = Vec::new();
        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                entries.push((
                    parts[2].to_string(),
                    parts[0].parse().unwrap_or(0),
                    parts[1].parse().unwrap_or(0),
                ));
            }
        }
        Ok(entries)
    }

    /// Shared implementation for [`Self::get_all_branches`] and
    /// [`Self::get_all_branches_with_known_main`]. Runs `git for-each-ref` and
    /// marks each branch as main or not based on the supplied `main_branch` string.
    async fn fetch_branch_list_with_main(
        repo_path: &Path,
        main_branch: &str,
        max_branch_count: Option<usize>,
        include_remotes: bool,
    ) -> Result<Vec<(String, bool)>> {
        let count_arg = format!("--count={}", max_branch_count.unwrap_or(100));

        let mut args = vec![
            "for-each-ref",
            count_arg.as_str(),
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ];

        if include_remotes {
            args.push("refs/remotes");
        }
        // Get branches sorted by commit date, limited to 100 most recent for performance
        // Using git's --count option to limit at the git command level for efficiency
        log::debug!(
            "[GIT OPERATION] diff_state.rs get_all_branches git {}",
            args.join(" ")
        );
        let output = run_git_command(repo_path, args.as_slice()).await?;

        let mut branches = Vec::new();

        for branch in output.lines() {
            let branch = branch.trim();
            if branch.is_empty() {
                continue;
            }

            // Skip HEAD pointer and detached HEAD states
            if branch.contains("HEAD") || branch.starts_with("(") {
                continue;
            }

            let is_main =
                branch == main_branch || branch == main_branch.trim_start_matches("origin/");
            branches.push((branch.to_string(), is_main));
        }

        // Remove duplicates while preserving order (most recent first)
        let mut seen = std::collections::HashSet::new();
        branches.retain(|(name, _)| seen.insert(name.clone()));

        if branches.is_empty() {
            safe_warn!(
                safe: ("Code Review: get_all_branches returned empty list"),
                full: ("Code Review: get_all_branches returned empty list for repo: {:?}", repo_path)
            );
        }

        Ok(branches)
    }

    /// Returns an iterator over `branches` with main branches (`is_main == true`) first,
    /// then the rest in their existing order.
    ///
    /// `get_all_branches` already returns branches sorted by recency; this helper
    /// promotes the main branch to the front while preserving that recency order
    /// for remaining entries. Callers that need a different ordering (e.g. the
    /// code-review panel which also prepends "Uncommitted changes") should compose
    /// this with their own logic or filter directly.
    pub fn sort_branches_main_first(
        branches: &[(String, bool)],
    ) -> impl Iterator<Item = &(String, bool)> {
        branches
            .iter()
            .filter(|(_, is_main)| *is_main)
            .chain(branches.iter().filter(|(_, is_main)| !is_main))
    }

    /// Back-compatible view of [`Self::parse_git_status_entries`] for the
    /// callers that only need the change kind.
    pub(crate) fn parse_git_status(status_output: &str) -> Result<Vec<(PathBuf, GitFileStatus)>> {
        Ok(Self::parse_git_status_entries(status_output)?
            .into_iter()
            .map(|entry| (entry.path, entry.status))
            .collect())
    }

    /// Parses git status output to get changed files and their status
    /// This handles porcelain=2 format to match git desktop implementation
    ///
    /// Also keeps the index (`X`) column, which the tuple-returning
    /// [`Self::parse_git_status`] drops — see [`FileStagedState`] (Zap #329).
    pub(crate) fn parse_git_status_entries(status_output: &str) -> Result<Vec<GitStatusEntry>> {
        if status_output.is_empty() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        let tokens: Vec<&str> = status_output.split('\0').collect();

        let mut i = 0;
        while i < tokens.len() {
            let token = tokens[i];

            if token.is_empty() {
                i += 1;
                continue;
            }

            // Skip header lines that start with '#'
            if token.starts_with("# ") {
                i += 1;
                continue;
            }

            let entry_kind = token.chars().next().unwrap_or('?');

            match entry_kind {
                '1' => {
                    // Changed entry: 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
                    // Use splitn to preserve spaces in filenames (the path is the last field).
                    let parts: Vec<&str> = token.splitn(9, ' ').collect();
                    if parts.len() >= 9 {
                        let status_code = parts[1];
                        let path = parts[8];

                        let status = Self::map_status_code(status_code).map_err(|e| {
                            anyhow!(
                                "Invalid status code '{}' for path '{}': {}",
                                status_code,
                                path,
                                e
                            )
                        })?;
                        files.push(GitStatusEntry {
                            path: PathBuf::from(path),
                            status,
                            staged: FileStagedState::from_status_code(status_code),
                        });
                    } else {
                        log::warn!("Invalid format for changed entry: '{token}' - expected at least 9 parts, got {}", parts.len());
                    }
                }
                '2' => {
                    // Renamed/copied entry: 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path><sep><origPath>
                    // Use splitn to preserve spaces in filenames (the path is the last field).
                    let parts: Vec<&str> = token.splitn(10, ' ').collect();
                    if parts.len() >= 10 {
                        let status_code = parts[1];
                        let path = parts[9];

                        // Get the old path from the next token
                        let old_path = if i + 1 < tokens.len() {
                            tokens[i + 1].to_string()
                        } else {
                            log::warn!("Missing old path for renamed/copied entry: '{token}'");
                            String::new()
                        };

                        let status = if status_code.starts_with('R') {
                            GitFileStatus::Renamed { old_path }
                        } else if status_code.starts_with('C') {
                            GitFileStatus::Copied { old_path }
                        } else {
                            Self::map_status_code(status_code).map_err(|e| {
                                anyhow!(
                                    "Invalid status code '{}' for renamed/copied path '{}': {}",
                                    status_code,
                                    path,
                                    e
                                )
                            })?
                        };

                        files.push(GitStatusEntry {
                            path: PathBuf::from(path),
                            status,
                            staged: FileStagedState::from_status_code(status_code),
                        });
                        i += 1; // Skip the old path token
                    } else {
                        log::warn!("Invalid format for renamed/copied entry: '{token}' - expected at least 10 parts, got {}", parts.len());
                    }
                }
                'u' => {
                    // Unmerged entry: u <xy> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
                    // Use splitn to preserve spaces in filenames (the path is the last field).
                    let parts: Vec<&str> = token.splitn(11, ' ').collect();
                    if parts.len() >= 11 {
                        let path = parts[10];
                        // An unmerged path cannot be staged until the conflict
                        // is resolved, so it is never reported as staged.
                        files.push(GitStatusEntry {
                            path: PathBuf::from(path),
                            status: GitFileStatus::Conflicted,
                            staged: FileStagedState::Unstaged,
                        });
                    } else {
                        log::warn!("Invalid format for unmerged entry: '{}' - expected at least 11 parts, got {}", token, parts.len());
                    }
                }
                '?' => {
                    // Untracked entry: ? <path>
                    if token.len() > 2 {
                        let path = &token[2..]; // Skip "? "
                        files.push(GitStatusEntry {
                            path: PathBuf::from(path),
                            status: GitFileStatus::Untracked,
                            // Untracked means "no index entry at all".
                            staged: FileStagedState::Unstaged,
                        });
                    } else {
                        log::warn!("Invalid format for untracked entry: '{token}' - expected path after '? '");
                    }
                }
                '!' => {
                    // Ignored entry: ! <path>
                    // We skip ignored files
                }
                _ => {
                    // Unknown entry type, skip
                    log::debug!("Unknown git status entry type '{entry_kind}' in token: '{token}'");
                }
            }

            i += 1;
        }

        Ok(files)
    }

    /// Get binary files using git diff --numstat
    async fn get_binary_files(repo_path: &Path) -> Result<std::collections::HashSet<PathBuf>> {
        Self::get_binary_files_vs_commit(repo_path, "HEAD").await
    }

    /// Gets the file content at HEAD commit for diff comparison
    async fn get_file_content_at_head(
        repo_path: &Path,
        file_path: &Path,
        status: &GitFileStatus,
    ) -> Option<String> {
        match status {
            GitFileStatus::Untracked | GitFileStatus::New => {
                // For new and untracked files, provide empty baseline content
                // since they don't exist in HEAD. The diff hunks will show
                // all content as additions, which is the correct representation.
                // Non-file entries (e.g. nested repo/worktree directories) get
                // no baseline so no editor is constructed for them.
                repo_path.join(file_path).is_file().then(String::new)
            }
            GitFileStatus::Renamed { old_path } => {
                // The file only exists under its new path in the working tree; at HEAD it still
                // lives at the old path. Mirror the merge-base path (see the `Renamed` arm of
                // `get_file_diff`) and read the baseline content from `old_path`.
                log::debug!(
                    "[GIT OPERATION] diff_state.rs get_file_content_at_head git show HEAD:{old_path}"
                );
                run_git_command(repo_path, &["show", &format!("HEAD:{old_path}")])
                    .await
                    .ok()
            }
            _ => {
                log::debug!(
                    "[GIT OPERATION] diff_state.rs get_file_content_at_head git show HEAD:{}",
                    file_path.display()
                );
                (run_git_command(
                    repo_path,
                    &["show", &format!("HEAD:{}", file_path.to_str()?)],
                )
                .await)
                    .ok()
            }
        }
    }

    /// Gets the diff for a specific file
    /// This matches Git Desktop's getWorkingDirectoryDiff implementation
    /// If commit is provided, diffs against that commit; otherwise handles different statuses appropriately
    async fn get_file_diff(
        repo_path: &Path,
        file_path: &PathBuf,
        status: &GitFileStatus,
        staged: Option<FileStagedState>,
        is_binary: bool,
        commit: Option<&str>,
    ) -> Result<FileDiff> {
        let mut hunks = Vec::new();
        let mut max_line_number = 0;

        // If it's a binary file, don't fetch or parse the diff content
        if is_binary {
            return Ok(FileDiff {
                file_path: file_path.clone(),
                status: status.clone(),
                hunks: Arc::new(Vec::new()),
                is_binary: true,
                is_autogenerated: false,
                max_line_number: 0,
                has_hidden_bidi_chars: false,
                size: DiffSize::Normal,
                staged,
            });
        }

        // `git status` can report a non-file untracked entry (e.g. a nested
        // repo/worktree directory checked out inside this repo). There is no
        // content to diff — `git diff --no-index` against a directory fails —
        // so return an empty non-binary diff up front.
        if matches!(status, GitFileStatus::Untracked) && repo_path.join(file_path).is_dir() {
            return Ok(FileDiff {
                file_path: file_path.clone(),
                status: status.clone(),
                hunks: Arc::new(Vec::new()),
                is_binary: false,
                is_autogenerated: false,
                max_line_number: 0,
                has_hidden_bidi_chars: false,
                size: DiffSize::Normal,
                staged,
            });
        }

        let file_path_str = file_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid file path: contains invalid UTF-8"))?;

        // Get the actual diff content for text files only
        // Use the same diff arguments as Git Desktop or compare against specific commit
        let diff_args = if let Some(commit) = commit {
            // When commit is specified, handle special cases that need different treatment
            match status {
                GitFileStatus::Untracked => {
                    // Untracked files don't exist in commit history, compare against empty
                    vec![
                        "diff",
                        "--no-ext-diff",
                        "--patch-with-raw",
                        "-z",
                        "--no-color",
                        "--no-index",
                        "--",
                        "/dev/null",
                        file_path_str,
                    ]
                }
                // For renamed files, we compare the old file path to the new file path.
                GitFileStatus::Renamed { old_path } => {
                    vec![
                        "diff",
                        "--no-ext-diff",
                        "--patch-with-raw",
                        "-z",
                        "--no-color",
                        commit,
                        "--",
                        old_path,
                        file_path_str,
                    ]
                }
                _ => {
                    // For all other statuses, diff against the specified commit
                    vec![
                        "diff",
                        "--no-ext-diff",
                        "--patch-with-raw",
                        "-z",
                        "--no-color",
                        commit,
                        "--",
                        file_path_str,
                    ]
                }
            }
        } else {
            // Handle different statuses when no specific commit is provided
            match status {
                GitFileStatus::New | GitFileStatus::Untracked => {
                    // For new/untracked files - compare against empty file
                    vec![
                        "diff",
                        "--no-ext-diff",
                        "--patch-with-raw",
                        "-z",
                        "--no-color",
                        "--no-index",
                        "--",
                        "/dev/null",
                        file_path_str,
                    ]
                }
                GitFileStatus::Renamed { old_path } => {
                    // Compare the working tree directly against HEAD, passing both the
                    // old and new paths (mirroring the merge-base branch above). This
                    // covers every staged/unstaged combination of the rename and any
                    // content edit: comparing against the index alone (as before) produced
                    // an empty diff whenever the rename and edit were both staged, since
                    // the working tree then matched the index exactly.
                    vec![
                        "diff",
                        "--no-ext-diff",
                        "--patch-with-raw",
                        "-z",
                        "--no-color",
                        "HEAD",
                        "--",
                        old_path,
                        file_path_str,
                    ]
                }
                _ => {
                    // For existing files - compare against HEAD
                    vec![
                        "diff",
                        "--no-ext-diff",
                        "--patch-with-raw",
                        "-z",
                        "--no-color",
                        "HEAD",
                        "--",
                        file_path_str,
                    ]
                }
            }
        };

        log::debug!(
            "[GIT OPERATION] diff_state.rs get_file_diff git {}",
            diff_args.join(" ")
        );
        let diff_output = match run_git_command(repo_path, &diff_args).await {
            Ok(output) => output,
            Err(error) => {
                log::info!(
                    "Failed to get file diff for {file_path:?}{}: {error}",
                    commit.map(|c| format!(" vs {c}")).unwrap_or_default()
                );
                // If diff fails, treat as binary or empty
                return Ok(FileDiff {
                    file_path: file_path.clone(),
                    status: status.clone(),
                    hunks: Arc::new(hunks),
                    is_binary: true,
                    is_autogenerated: false,
                    max_line_number: 0,
                    has_hidden_bidi_chars: false,
                    size: DiffSize::Normal,
                    staged,
                });
            }
        };

        // Double-check for binary files using the old method as fallback
        // Git outputs "Binary files a/file and b/file differ" for binary files
        if diff_output
            .lines()
            .any(|line| line.starts_with("Binary files ") && line.contains(" differ"))
        {
            return Ok(FileDiff {
                file_path: file_path.clone(),
                status: status.clone(),
                hunks: Arc::new(Vec::new()),
                is_binary: true,
                is_autogenerated: false,
                max_line_number: 0,
                has_hidden_bidi_chars: false,
                size: DiffSize::Normal,
                staged,
            });
        }

        // Parse the diff output for text files
        let parsed_hunks = Self::parse_diff_hunks(&diff_output)?;
        for hunk in &parsed_hunks {
            for line in &hunk.lines {
                if let Some(line_num) = line.old_line_number {
                    max_line_number = max_line_number.max(line_num);
                }
                if let Some(line_num) = line.new_line_number {
                    max_line_number = max_line_number.max(line_num);
                }
            }
        }
        hunks = parsed_hunks;

        // Check for hidden bidi characters
        let has_hidden_bidi_chars = Self::check_for_hidden_bidi_chars(&diff_output);

        let size = compute_diff_size(&hunks, diff_output.len());

        Ok(FileDiff {
            file_path: file_path.clone(),
            status: status.clone(),
            hunks: Arc::new(hunks),
            is_binary,
            is_autogenerated: false,
            max_line_number,
            has_hidden_bidi_chars,
            size,
            staged,
        })
    }

    /// Parses diff hunks from git diff output
    pub(crate) fn parse_diff_hunks(diff_output: &str) -> Result<Vec<DiffHunk>> {
        let mut hunks = Vec::new();
        let lines: Vec<&str> = diff_output.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Look for hunk headers (@@)
            if line.starts_with("@@") {
                // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
                let header = Self::parse_unified_diff_header(line)?;

                let old_start = header.old_start_line;
                let old_count = header.old_line_count;
                let new_start = header.new_start_line;
                let new_count = header.new_line_count;

                // Collect hunk lines
                let mut hunk_lines = Vec::new();
                i += 1;

                let mut old_line = old_start;
                let mut new_line = new_start;

                while i < lines.len()
                    && !lines[i].starts_with("@@")
                    && !lines[i].starts_with("diff ")
                {
                    let content_line = lines[i];

                    if content_line.is_empty() {
                        i += 1;
                        continue;
                    }

                    let (line_type, old_num, new_num) = if content_line.starts_with('+') {
                        let num = new_line;
                        new_line += 1;
                        (DiffLineType::Add, None, Some(num))
                    } else if content_line.starts_with('-') {
                        let num = old_line;
                        old_line += 1;
                        (DiffLineType::Delete, Some(num), None)
                    } else if content_line.starts_with(' ') {
                        let old_num = old_line;
                        let new_num = new_line;
                        old_line += 1;
                        new_line += 1;
                        (DiffLineType::Context, Some(old_num), Some(new_num))
                    } else if content_line.starts_with('\\') {
                        // `\ No newline at end of file` is its OWN line in git's
                        // output -- backslash, space, text -- emitted directly
                        // after the line it describes, on whichever side that
                        // line was. So the flag belongs to the entry already
                        // pushed, not to this one, and a hunk that rewrites the
                        // final line carries two markers, one per side.
                        //
                        // Testing the content line itself for the text (which is
                        // what this parser used to do) can never match: such a
                        // line starts with `\`, so it lands here in the skip
                        // branch, and `no_trailing_newline` stayed false for
                        // every line ever parsed. That was harmless until
                        // `hunk_to_patch` started replaying the flag into a
                        // patch for `git apply --cached`, where a missing marker
                        // makes git either append a newline the file never had
                        // or reject the hunk outright.
                        if let Some(last) = hunk_lines.last_mut() {
                            last.no_trailing_newline = true;
                        }
                        i += 1;
                        continue;
                    } else {
                        // Skip lines that don't start with +, -, or space
                        i += 1;
                        continue;
                    };

                    let text = if content_line.len() > 1 {
                        content_line[1..].to_string()
                    } else {
                        String::new()
                    };

                    hunk_lines.push(DiffLine {
                        line_type,
                        old_line_number: old_num,
                        new_line_number: new_num,
                        text,
                        // Set retroactively by the `\` branch above when the
                        // marker line that follows this one says so.
                        no_trailing_newline: false,
                    });

                    i += 1;
                }

                hunks.push(DiffHunk {
                    old_start_line: old_start,
                    old_line_count: old_count,
                    new_start_line: new_start,
                    new_line_count: new_count,
                    lines: hunk_lines,
                    unified_diff_start: 0, // Will be calculated later
                    unified_diff_end: 0,   // Will be calculated later
                });

                continue;
            }

            i += 1;
        }

        Ok(hunks)
    }

    /// Parses a range string like "1,5" or "1" into (start, count)
    pub(crate) fn parse_range(range_str: &str) -> Result<(usize, usize)> {
        if let Some(comma_pos) = range_str.find(',') {
            let start: usize = range_str[..comma_pos]
                .parse()
                .map_err(|_| anyhow!("Invalid range start: {}", range_str))?;
            let count: usize = range_str[comma_pos + 1..]
                .parse()
                .map_err(|_| anyhow!("Invalid range count: {}", range_str))?;
            Ok((start, count))
        } else {
            let start: usize = range_str
                .parse()
                .map_err(|_| anyhow!("Invalid range: {}", range_str))?;
            Ok((start, 1))
        }
    }

    /// Parses a unified diff header line
    /// Format: @@ -old_start,old_count +new_start,new_count @@ [optional context]
    pub(crate) fn parse_unified_diff_header(header_line: &str) -> Result<UnifiedDiffHeader> {
        if !header_line.starts_with("@@") {
            return Err(anyhow!("Invalid unified diff header: {}", header_line));
        }

        // Split by whitespace and take only the first 3 tokens to ignore optional context
        let header_parts: Vec<&str> = header_line.split_whitespace().take(3).collect();
        if header_parts.len() < 3 {
            return Err(anyhow!(
                "Invalid unified diff header format: {}",
                header_line
            ));
        }

        let old_range = &header_parts[1][1..]; // Remove the '-'
        let new_range = &header_parts[2][1..]; // Remove the '+'

        let (old_start_line, old_line_count) = Self::parse_range(old_range)?;
        let (new_start_line, new_line_count) = Self::parse_range(new_range)?;

        Ok(UnifiedDiffHeader {
            old_start_line,
            old_line_count,
            new_start_line,
            new_line_count,
        })
    }

    /// Check for hidden bidirectional characters in diff output
    fn check_for_hidden_bidi_chars(diff_output: &str) -> bool {
        diff_output.chars().any(|c| BIDI_CHARS.contains(&c))
    }

    /// Parses git diff --name-status output with -z flag (null-separated)
    /// Format: status<null>filename<null>status<null>filename<null>...
    /// For renamed/copied files: status<null>old_path<null>new_path<null>
    fn parse_git_diff_name_status(diff_output: &str) -> Result<Vec<(PathBuf, GitFileStatus)>> {
        if diff_output.is_empty() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        let tokens: Vec<&str> = diff_output.split('\0').collect();

        let mut i = 0;
        while i < tokens.len() {
            let token = tokens[i].trim();
            if token.is_empty() {
                i += 1;
                continue;
            }

            // First token should be the status (M, A, D, R, C, etc.)
            let status_char = token.chars().next().unwrap_or('M');

            // Next token should be the file path
            if i + 1 >= tokens.len() {
                // No filename following the status, skip
                i += 1;
                continue;
            }

            let mut file_path = tokens[i + 1].trim();
            if file_path.is_empty() {
                i += 2;
                continue;
            }

            let status = match status_char {
                'A' => GitFileStatus::New,
                'M' => GitFileStatus::Modified,
                'D' => GitFileStatus::Deleted,
                'R' => {
                    // For renamed files: R<score> <null> old_path <null> new_path <null>
                    // We need to get the old path from the next token
                    if i + 2 < tokens.len() {
                        let old_path = file_path;
                        file_path = tokens[i + 2].trim();
                        i += 1; // Skip the new path token (we'll increment again at the end)
                        GitFileStatus::Renamed {
                            old_path: old_path.to_string(),
                        }
                    } else {
                        GitFileStatus::Modified
                    }
                }
                'C' => {
                    // For copied files: C<score> <null> new_path <null> old_path <null>
                    // We need to get the old path from the next token
                    if i + 2 < tokens.len() {
                        let old_path = tokens[i + 2].trim().to_string();
                        i += 1; // Skip the old path token (we'll increment again at the end)
                        GitFileStatus::Copied { old_path }
                    } else {
                        GitFileStatus::Modified
                    }
                }
                _ => GitFileStatus::Modified,
            };

            files.push((PathBuf::from(file_path), status));

            // Move to next status token (skip status + filename, or status + old_path + new_path for R/C)
            i += 2;
        }

        Ok(files)
    }

    /// Get binary files using git diff --numstat against a specific commit
    async fn get_diff_metadata_using_numstat(
        repo_path: &Path,
        commit: &str,
    ) -> Result<HashMap<PathBuf, GitNumStatMetadata>> {
        log::debug!(
            "[GIT OPERATION] diff_state.rs get_diff_metadata_using_numstat git diff --numstat {commit}"
        );
        let numstat_output = match run_git_command(repo_path, &["diff", "--numstat", commit]).await
        {
            Ok(output) => output,
            Err(_) => {
                // If numstat fails, return empty set
                return Ok(HashMap::new());
            }
        };

        let mut diff_metadata = std::collections::HashMap::new();

        for line in numstat_output.lines() {
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let additions = parts[0];
                let deletions = parts[1];
                let filename = parts[2];

                let metadata = GitNumStatMetadata {
                    lines_added: additions.parse().unwrap_or(0),
                    lines_removed: deletions.parse().unwrap_or(0),
                    is_binary_file: additions == "-" && deletions == "-",
                };

                diff_metadata.insert(PathBuf::from(filename), metadata);
            }
        }

        Ok(diff_metadata)
    }

    /// Get binary files using git diff --numstat against a specific commit
    async fn get_binary_files_vs_commit(
        repo_path: &Path,
        commit: &str,
    ) -> Result<std::collections::HashSet<PathBuf>> {
        let diff_metadata = Self::get_diff_metadata_using_numstat(repo_path, commit).await?;

        let binary_files = diff_metadata
            .into_iter()
            .filter_map(|(path, metadata)| metadata.is_binary_file.then_some(path))
            .collect();

        Ok(binary_files)
    }

    /// Gets the file content at a specific commit
    async fn get_file_content_at_commit(
        repo_path: &Path,
        file_path: &str,
        commit: &str,
    ) -> Option<String> {
        log::debug!(
            "[GIT OPERATION] diff_state.rs get_file_content_at_commit git show {commit}:{file_path}"
        );
        run_git_command(repo_path, &["show", &format!("{commit}:{file_path}")])
            .await
            .ok()
    }

    /// Maps git status codes to GitFileStatus
    fn map_status_code(status_code: &str) -> Result<GitFileStatus> {
        GitFileStatus::try_from(status_code)
    }

    fn changes_vs_main_branch_label(&self) -> String {
        let main_branch_name = self.get_main_branch_name().unwrap_or("main".to_string());
        format!("Changes vs. {main_branch_name}")
    }

    fn changes_vs_head_label(&self) -> String {
        UNCOMMITTED_CHANGES.to_string()
    }

    pub fn label_text(&self, mode: DiffMode) -> String {
        match mode {
            DiffMode::Head => self.changes_vs_head_label(),
            DiffMode::MainBranch => self.changes_vs_main_branch_label(),
            DiffMode::OtherBranch(branch) => format!("Changes vs. {branch}"),
        }
    }

    /// Fetches PR info for the current branch via `gh pr view` (network call).
    /// Call this on branch change or after push — not on every metadata refresh.
    #[cfg(feature = "local_fs")]
    pub fn refresh_pr_info(&mut self, ctx: &mut ModelContext<Self>) {
        if let Some(handle) = self.refreshing_pr_info_handle.take() {
            handle.abort();
        }
        let Some(repo_path) = self.active_repository_path(ctx) else {
            return;
        };
        // Mark this branch as attempted before spawning so the fallback in
        // `handle_updated_metadata_for_repo` won't re-fire while the lookup
        // is in flight or after it completes with no PR.
        self.pr_info_attempted_for_branch = self.get_current_branch_name();
        #[cfg(feature = "local_tty")]
        let path_future = LocalShellState::handle(ctx).update(ctx, |shell_state, ctx| {
            shell_state.get_interactive_path_env_var(ctx)
        });
        #[cfg(not(feature = "local_tty"))]
        let path_future: futures::future::BoxFuture<'static, Option<String>> = {
            use futures::FutureExt;
            futures::future::ready(None).boxed()
        };
        let handle = ctx.spawn(
            async move {
                let path_env = path_future.await;
                get_pr_for_branch(&repo_path, path_env.as_deref())
                    .await
                    .unwrap_or(None)
            },
            |me, pr_info, ctx| {
                me.refreshing_pr_info_handle = None;
                if let Some(metadata) = &mut me.metadata {
                    metadata.pr_info = pr_info;
                    ctx.emit(DiffStateModelEvent::DiffMetadataChanged(
                        InvalidationBehavior::PromptRefresh,
                    ));
                }
            },
        );
        self.refreshing_pr_info_handle = Some(handle);
    }

    #[cfg(not(feature = "local_fs"))]
    pub fn refresh_pr_info(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Fetches the branches of the active repository off-thread and emits
    /// [`DiffStateModelEvent::BranchesReceived`] with the result. On failure an
    /// empty list is emitted so the branch dropdown falls back to its defaults
    /// (uncommitted changes + main branch) rather than keeping stale entries.
    #[cfg(feature = "local_fs")]
    pub fn fetch_branches(&self, ctx: &mut ModelContext<Self>) {
        let Some(repo_path) = self.active_repository_path(ctx) else {
            return;
        };
        let fetched_repo_path = repo_path.clone();
        ctx.spawn(
            async move {
                Self::get_all_branches(&repo_path, None, false /* include_remotes */).await
            },
            move |me, branches_result, ctx| {
                // If the active repo changed while branches were being fetched,
                // discard the stale result.
                if me.active_repository_path(ctx) != Some(fetched_repo_path) {
                    return;
                }
                let branches = match branches_result {
                    Ok(branches) => branches,
                    Err(err) => {
                        log::warn!("LocalDiffStateModel: failed to fetch branches: {err}");
                        vec![]
                    }
                };
                ctx.emit(DiffStateModelEvent::BranchesReceived(branches));
            },
        );
    }

    /// No local filesystem to run `git` against on WASM builds.
    #[cfg(not(feature = "local_fs"))]
    pub fn fetch_branches(&self, _ctx: &mut ModelContext<Self>) {}
}

#[derive(Debug, Clone)]
pub enum InvalidationBehavior {
    All(InvalidationSource),
    /// Like `All`, but the git index was locked when the update was detected.
    /// Signals the view to cancel in-flight work without starting a new diff
    /// reload (the data would be stale while the lock is held).
    AllLockedIndex,
    Files(Vec<PathBuf>),
    PromptRefresh,
}

#[derive(Debug, Clone)]
pub enum InvalidationSource {
    /// This is from an actual underlying metadata change.
    MetadataChange,
    /// Index is unlocked. We will attempt to flush the invalidation if there is
    /// an inflight pending invalidation.
    IndexLockChange,
}

#[derive(Debug)]
pub enum DiffStateModelEvent {
    /// The repository the diff state is for has changed.
    RepositoryChanged,
    /// Event dispatched when the current branch changes.
    CurrentBranchChanged,
    /// Event dispatched whenever the diff metadata changes in any way.
    DiffMetadataChanged(InvalidationBehavior),
    /// Event dispatched when new diffs are computed.
    ///
    /// The payload is wrapped in `Arc` so the `DiffStateModel` enum wrapper can
    /// forward the event from its inner backend model without cloning the
    /// expensive base-content (the whole reason `GitDiffWithBaseContent` is
    /// itself non-Clone); the Arc shares it instead of copying.
    NewDiffsComputed(Option<Arc<GitDiffWithBaseContent>>),
    /// Event dispatched when new diff mode is set.
    /// The boolean indicates whether the next diff load should attempt to
    /// fetch the base branch from origin if it is not available locally.
    DiffModeChanged { should_fetch_base: bool },
    /// Branch list received from the backend (local git or the remote server's
    /// `GetBranches` RPC). The payload is `(branch_name, is_main_branch)` pairs,
    /// as returned by [`LocalDiffStateModel::get_all_branches`].
    BranchesReceived(Vec<(String, bool)>),
    /// The remote-server session for this repository's host was lost
    /// (`RemoteServerManagerEvent::HostDisconnected`). Only ever emitted by
    /// [`super::diff_state_remote::RemoteDiffStateModel`] — the local backend has
    /// no transport to lose. Carries no payload; `RemoteDiffStateModel::get()`
    /// keeps returning the last-known `Loaded` diffs (mapped from its internal
    /// `Disconnected` state) so the view can keep showing stale data while
    /// signaling the loss separately, e.g. a "reconnecting…" indicator.
    ConnectionLost,
    /// A git write the user explicitly asked for failed (Zap #329).
    ///
    /// Staging and discarding both reload the diff only on success, so a
    /// rejected `git apply --cached` — which is not rare, since the hunk path
    /// guesses its direction from the *file's* staged column — leaves the view
    /// byte-identical to before the click. Logging alone made that
    /// indistinguishable from the click never having registered. The model has
    /// no window, so it reports the failure here and the view, which does, turns
    /// it into a toast.
    GitWriteFailed {
        operation: GitWriteOperation,
        message: String,
    },
}

/// Which user-initiated git write failed, for
/// [`DiffStateModelEvent::GitWriteFailed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitWriteOperation {
    Stage,
    Unstage,
    Discard,
}

impl warpui::Entity for LocalDiffStateModel {
    type Event = DiffStateModelEvent;
}

/// Unified diff-state model: a thin wrapper that dispatches to a backend and
/// forwards the backend's [`DiffStateModelEvent`]s. All consumers hold a
/// `DiffStateModel` rather than a backend directly.
///
/// Currently only the local backend ([`LocalDiffStateModel`]) exists; the
/// remote backend (code-review diff-state over SSH) is added as a second
/// `Remote` variant in a following increment. Keeping the wrapper now lets the
/// call sites settle on the dispatching API before the remote backend lands.
pub enum DiffStateModel {
    Local(ModelHandle<LocalDiffStateModel>),
    /// Remote (SSH) backend. Native-only — it talks to the RemoteServerManager.
    #[cfg(not(target_family = "wasm"))]
    Remote(ModelHandle<crate::code_review::diff_state_remote::RemoteDiffStateModel>),
}

impl warpui::Entity for DiffStateModel {
    type Event = DiffStateModelEvent;
}

impl DiffStateModel {
    /// Creates a local-backed `DiffStateModel`. The wrapper subscribes to the
    /// inner model so it can forward its events to the wrapper's own
    /// subscribers.
    pub fn new(repo_path: Option<String>, ctx: &mut ModelContext<Self>) -> Self {
        let local = ctx.add_model(|ctx| LocalDiffStateModel::new(repo_path, ctx));
        ctx.subscribe_to_model(&local, |me, event, ctx| me.forward_event(event, ctx));
        Self::Local(local)
    }

    /// Creates a remote-backed `DiffStateModel` for a repository on an SSH host.
    /// Subscribes to the inner remote model to forward its events, mirroring
    /// `new`. The caller must ensure a session for the host is connected.
    #[cfg(not(target_family = "wasm"))]
    pub fn new_remote(
        remote_path: crate::code::buffer_location::RemotePath,
        mode: DiffMode,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let remote = ctx.add_model(|ctx| {
            crate::code_review::diff_state_remote::RemoteDiffStateModel::new(remote_path, mode, ctx)
        });
        ctx.subscribe_to_model(&remote, |me, event, ctx| me.forward_event(event, ctx));
        Self::Remote(remote)
    }

    /// Re-emits an inner backend event as the wrapper's own event so existing
    /// `subscribe_to_model(&diff_state_model, ...)` consumers are unaffected by
    /// the wrapper.
    fn forward_event(&mut self, event: &DiffStateModelEvent, ctx: &mut ModelContext<Self>) {
        match event {
            DiffStateModelEvent::RepositoryChanged => {
                ctx.emit(DiffStateModelEvent::RepositoryChanged);
            }
            DiffStateModelEvent::CurrentBranchChanged => {
                ctx.emit(DiffStateModelEvent::CurrentBranchChanged);
            }
            DiffStateModelEvent::DiffMetadataChanged(behavior) => {
                ctx.emit(DiffStateModelEvent::DiffMetadataChanged(behavior.clone()));
            }
            DiffStateModelEvent::NewDiffsComputed(diffs) => {
                ctx.emit(DiffStateModelEvent::NewDiffsComputed(diffs.clone()));
            }
            DiffStateModelEvent::DiffModeChanged { should_fetch_base } => {
                ctx.emit(DiffStateModelEvent::DiffModeChanged {
                    should_fetch_base: *should_fetch_base,
                });
            }
            DiffStateModelEvent::BranchesReceived(branches) => {
                ctx.emit(DiffStateModelEvent::BranchesReceived(branches.clone()));
            }
            // Added for #388's idempotent disconnect handling. Forwarded like every
            // other backend event so wrapper subscribers see it too.
            DiffStateModelEvent::ConnectionLost => {
                ctx.emit(DiffStateModelEvent::ConnectionLost);
            }
            DiffStateModelEvent::GitWriteFailed { operation, message } => {
                ctx.emit(DiffStateModelEvent::GitWriteFailed {
                    operation: *operation,
                    message: message.clone(),
                });
            }
        }
    }

    // ── Read API (dispatched to the backend) ─────────────────────────

    pub fn get(&self, ctx: &AppContext) -> DiffState {
        match self {
            Self::Local(m) => m.as_ref(ctx).get(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).get(),
        }
    }

    pub fn diff_mode(&self, ctx: &AppContext) -> DiffMode {
        match self {
            Self::Local(m) => m.as_ref(ctx).diff_mode(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).diff_mode(),
        }
    }

    pub fn get_uncommitted_stats(&self, ctx: &AppContext) -> Option<DiffStats> {
        match self {
            Self::Local(m) => m.as_ref(ctx).get_uncommitted_stats(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).get_uncommitted_stats(),
        }
    }

    pub fn get_stats_for_current_mode(&self, ctx: &AppContext) -> Option<DiffStats> {
        match self {
            Self::Local(m) => m.as_ref(ctx).get_stats_for_current_mode(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).get_stats_for_current_mode(),
        }
    }

    pub fn get_main_branch_name(&self, ctx: &AppContext) -> Option<String> {
        match self {
            Self::Local(m) => m.as_ref(ctx).get_main_branch_name(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).get_main_branch_name(),
        }
    }

    pub fn get_current_branch_name(&self, ctx: &AppContext) -> Option<String> {
        match self {
            Self::Local(m) => m.as_ref(ctx).get_current_branch_name(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).get_current_branch_name(),
        }
    }

    pub fn is_on_main_branch(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Local(m) => m.as_ref(ctx).is_on_main_branch(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).is_on_main_branch(),
        }
    }

    pub fn unpushed_commits<'a>(&self, ctx: &'a AppContext) -> &'a [Commit] {
        match self {
            Self::Local(m) => m.as_ref(ctx).unpushed_commits(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).unpushed_commits(),
        }
    }

    pub fn upstream_ref<'a>(&self, ctx: &'a AppContext) -> Option<&'a str> {
        match self {
            Self::Local(m) => m.as_ref(ctx).upstream_ref(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).upstream_ref(),
        }
    }

    pub fn upstream_differs_from_main(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Local(m) => m.as_ref(ctx).upstream_differs_from_main(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).upstream_differs_from_main(),
        }
    }

    pub fn pr_info<'a>(&self, ctx: &'a AppContext) -> Option<&'a PrInfo> {
        match self {
            Self::Local(m) => m.as_ref(ctx).pr_info(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).pr_info(),
        }
    }

    /// Whether PR info for the current branch is currently being refreshed.
    pub fn is_pr_info_refreshing(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Local(m) => m.as_ref(ctx).is_pr_info_refreshing(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).is_pr_info_refreshing(),
        }
    }

    pub fn is_git_operation_blocked(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Local(m) => m.as_ref(ctx).is_git_operation_blocked(ctx),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).is_git_operation_blocked(ctx),
        }
    }

    pub fn has_head(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Local(m) => m.as_ref(ctx).has_head(),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).has_head(),
        }
    }

    pub fn active_repository_path(&self, ctx: &AppContext) -> Option<PathBuf> {
        match self {
            Self::Local(m) => m.as_ref(ctx).active_repository_path(ctx),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.as_ref(ctx).active_repository_path(ctx),
        }
    }

    /// The remote host + path this model is bound to, or `None` for a local
    /// repository. The code-review git dialog uses this to address git write-op
    /// RPCs (#116) when the repo lives on an SSH host, rather than running git
    /// against a (nonexistent) local path.
    pub fn remote_target(
        &self,
        #[cfg_attr(target_family = "wasm", allow(unused_variables))] ctx: &AppContext,
    ) -> Option<crate::code::buffer_location::RemotePath> {
        match self {
            Self::Local(_) => None,
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => Some(m.as_ref(ctx).remote_path().clone()),
        }
    }

    /// File-change entries for the currently loaded diff, with per-file counts
    /// derived from the loaded hunks. Used to populate the git dialog's Changes
    /// box for remote repos, which can't read the working tree directly and so
    /// source the list from the synced diff state instead of `get_file_change_entries`.
    pub fn loaded_file_entries(&self, ctx: &AppContext) -> Vec<crate::util::git::FileChangeEntry> {
        let DiffState::Loaded(data) = self.get(ctx) else {
            return Vec::new();
        };
        data.files
            .iter()
            .map(|file| {
                let mut additions = 0usize;
                let mut deletions = 0usize;
                for hunk in file.hunks.iter() {
                    for line in &hunk.lines {
                        match line.line_type {
                            DiffLineType::Add => additions += 1,
                            DiffLineType::Delete => deletions += 1,
                            DiffLineType::Context | DiffLineType::HunkHeader => {}
                        }
                    }
                }
                crate::util::git::FileChangeEntry {
                    path: file.file_path.to_string_lossy().into_owned(),
                    additions,
                    deletions,
                }
            })
            .collect()
    }

    // ── Mutation API (dispatched to the backend) ─────────────────────

    /// Applies a remote git write-op's post-operation delta (refreshed unpushed
    /// commits + upstream ref) returned by the daemon.
    ///
    /// Remote-only: the local backend recomputes this state from the working
    /// tree after the operation, so it has no delta to fold in.
    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    pub fn apply_git_op_delta(
        &mut self,
        unpushed_commits: Vec<Commit>,
        upstream_ref: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(_) => {}
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |remote, ctx| {
                remote.apply_git_op_delta(unpushed_commits, upstream_ref, ctx)
            }),
        }
    }

    pub fn set_diff_mode(
        &mut self,
        mode: DiffMode,
        should_fetch_base: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => {
                m.update(ctx, |local, ctx| local.set_diff_mode(mode, should_fetch_base, ctx))
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => {
                m.update(ctx, |local, ctx| local.set_diff_mode(mode, should_fetch_base, ctx))
            }
        }
    }

    pub fn set_diff_mode_and_fetch_base(&mut self, mode: DiffMode, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => {
                m.update(ctx, |local, ctx| local.set_diff_mode_and_fetch_base(mode, ctx))
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => {
                m.update(ctx, |local, ctx| local.set_diff_mode_and_fetch_base(mode, ctx))
            }
        }
    }

    pub fn load_diffs_for_current_repo(
        &mut self,
        should_fetch_base: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => {
                m.update(ctx, |local, ctx| local.load_diffs_for_current_repo(should_fetch_base, ctx))
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => {
                m.update(ctx, |local, ctx| local.load_diffs_for_current_repo(should_fetch_base, ctx))
            }
        }
    }

    pub fn discard_files(
        &mut self,
        file_infos: Vec<FileStatusInfo>,
        should_stash: bool,
        branch_name: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| {
                local.discard_files(file_infos, should_stash, branch_name, ctx)
            }),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |local, ctx| {
                local.discard_files(file_infos, should_stash, branch_name, ctx)
            }),
        }
    }

    /// Stages or un-stages part of the working tree (Zap #329). Both backends
    /// implement it, so per-file and per-hunk staging work over SSH exactly as
    /// they do locally.
    pub fn stage_changes(&mut self, request: StageRequest, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| local.stage_changes(request, ctx)),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |remote, ctx| remote.stage_changes(request, ctx)),
        }
    }

    pub fn set_code_review_metadata_refresh_enabled(
        &mut self,
        enabled: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| {
                local.set_code_review_metadata_refresh_enabled(enabled, ctx)
            }),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |local, ctx| {
                local.set_code_review_metadata_refresh_enabled(enabled, ctx)
            }),
        }
    }

    pub fn refresh_diff_metadata_for_current_repo(
        &mut self,
        invalidation_behavior: InvalidationBehavior,
        ctx: &mut ModelContext<Self>,
    ) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| {
                local.refresh_diff_metadata_for_current_repo(invalidation_behavior, ctx)
            }),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |local, ctx| {
                local.refresh_diff_metadata_for_current_repo(invalidation_behavior, ctx)
            }),
        }
    }

    pub fn stop_active_watcher(&mut self, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| local.stop_active_watcher(ctx)),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |local, ctx| local.stop_active_watcher(ctx)),
        }
    }

    pub fn refresh_pr_info(&mut self, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| local.refresh_pr_info(ctx)),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |local, ctx| local.refresh_pr_info(ctx)),
        }
    }

    /// Fetches the active repository's branch list from the backend. The local
    /// backend shells out to `git`; the remote backend issues the `GetBranches`
    /// RPC so the listing runs on the host that actually holds the repo. Either
    /// way the result arrives as [`DiffStateModelEvent::BranchesReceived`].
    pub fn fetch_branches(&mut self, ctx: &mut ModelContext<Self>) {
        match self {
            Self::Local(m) => m.update(ctx, |local, ctx| local.fetch_branches(ctx)),
            #[cfg(not(target_family = "wasm"))]
            Self::Remote(m) => m.update(ctx, |remote, ctx| remote.fetch_branches(ctx)),
        }
    }
}

#[cfg(feature = "local_fs")]
enum DiffStateRepositoryUpdate {
    /// Normal file-system update to process.
    Invalidation(RepositoryUpdate),
    /// A commit-level update was detected while `.git/index.lock` was held.
    /// The receiver should mark a full invalidation pending without triggering
    /// a diff reload (the data would be stale).
    InvalidationWithLockedIndex,
}

/// Decides what a raw watcher [`RepositoryUpdate`] means for diff state.
///
/// Returns `None` when the update carries nothing worth reacting to (empty, or
/// every changed file gitignored).
///
/// Extracted from [`LocalDiffStateModel::handle_file_update`] so the remote-server
/// daemon can classify updates with **exactly** these rules rather than a
/// second copy of them (#577). The daemon needs the `Files(..)` case in
/// particular: it is what lets it push a per-file `DiffStateFileDelta` instead
/// of re-serializing the whole repo on every change. The subtleties here are
/// the reason this is shared and not reimplemented — a tracked-remote-ref move
/// is metadata rather than content (#294), a locked index must not trigger a
/// reload against stale data, and a touched `.gitignore` can change which files
/// belong in the diff at all, so it forces a full reload.
#[cfg(feature = "local_fs")]
pub(crate) fn classify_repository_update(
    update: RepositoryUpdate,
) -> Option<InvalidationBehavior> {
    // Refresh if there are file changes or if commit state has been updated
    if update.is_empty() {
        return None;
    }

    let RepositoryUpdate {
        added,
        modified,
        deleted,
        moved,
        commit_updated,
        index_lock_detected,
        remote_ref_updated,
    } = update;

    if commit_updated || remote_ref_updated {
        // A changed tracked-remote ref (#294) is metadata, not file
        // content — e.g. it moves the ahead/behind counts shown in the
        // diff-state pane — so it's treated the same as commit_updated.
        return Some(InvalidationBehavior::All(
            InvalidationSource::MetadataChange,
        ));
    }
    if index_lock_detected {
        return Some(InvalidationBehavior::All(
            InvalidationSource::IndexLockChange,
        ));
    }

    // Filter out gitignored files and extract paths
    let changed_files = added
        .into_iter()
        .chain(modified)
        .chain(deleted)
        .chain(moved.clone().into_keys())
        .chain(moved.into_values())
        .filter(|target_file| !target_file.is_ignored)
        .map(|target_file| target_file.path)
        .collect::<Vec<PathBuf>>();

    if changed_files.is_empty() {
        return None;
    }

    // Check if .gitignore was modified - if so, do a full reload since
    // this can fundamentally change which files should appear in the diff
    let gitignore_modified = changed_files.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == ".gitignore")
    });

    if gitignore_modified {
        Some(InvalidationBehavior::All(
            InvalidationSource::MetadataChange,
        ))
    } else {
        Some(InvalidationBehavior::Files(changed_files))
    }
}

#[cfg(feature = "local_fs")]
struct DiffStateModelRepositorySubscriber {
    repository_update_tx: Sender<DiffStateRepositoryUpdate>,
}

#[cfg(feature = "local_fs")]
impl RepositorySubscriber for DiffStateModelRepositorySubscriber {
    fn on_scan(
        &mut self,
        _repository: &Repository,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::prelude::rust_2024::Future<Output = ()> + Send + 'static>> {
        // The model can safely ignore the initial scan because the model only subscribes
        // after the repository is already scanned.
        Box::pin(async {})
    }

    fn on_files_updated(
        &mut self,
        repository: &Repository,
        update: &repo_metadata::RepositoryUpdate,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::prelude::rust_2024::Future<Output = ()> + Send + 'static>> {
        let tx = self.repository_update_tx.clone();
        let update = update.clone();
        let index_lock_path = repository.git_dir().join("index.lock");
        Box::pin(async move {
            // If commit state changed while the git index is locked (e.g. during
            // a pull or merge), signal the locked state instead of forwarding stale
            // data. The lock release atomically renames index.lock → index, which
            // triggers a fresh commit_updated event that takes the normal path.
            let msg = if update.commit_updated && async_fs::metadata(&index_lock_path).await.is_ok()
            {
                DiffStateRepositoryUpdate::InvalidationWithLockedIndex
            } else {
                DiffStateRepositoryUpdate::Invalidation(update)
            };
            let _ = tx.send(msg).await;
        })
    }
}

#[cfg(test)]
#[path = "diff_state_tests.rs"]
mod tests;

/// Unit tests for the new-file span predicate shared by both gutter buttons.
///
/// Inline rather than in `diff_state_tests.rs` because the repair that
/// introduced them was scoped to this file and `code_review_view.rs`.
#[cfg(test)]
mod hunk_span_tests {
    use super::{hunk_covers_new_lines, hunk_new_line_span, DiffHunk, DiffLine, DiffLineType};

    fn line(line_type: DiffLineType) -> DiffLine {
        DiffLine {
            line_type,
            old_line_number: None,
            new_line_number: None,
            text: String::new(),
            no_trailing_newline: false,
        }
    }

    /// `new_start_line` / `new_line_count` are the `+c,d` half of the `@@`
    /// header; `lines` is the materialised body, which the tests below vary
    /// independently precisely because the two can disagree.
    fn hunk(new_start_line: usize, new_line_count: usize, lines: Vec<DiffLine>) -> DiffHunk {
        DiffHunk {
            old_start_line: 1,
            old_line_count: 1,
            new_start_line,
            new_line_count,
            lines,
            unified_diff_start: 0,
            unified_diff_end: 0,
        }
    }

    /// The original predicate measured a hunk with `lines.len()`, which counts
    /// `Delete` entries that occupy no new-file line. A hunk removing 49 lines
    /// while keeping 1 then claimed 50 new-file lines instead of 1 and
    /// swallowed every later hunk within that reach — `Iterator::find` returns
    /// the first match, so the click staged the wrong hunk.
    #[test]
    fn deletion_heavy_hunk_does_not_swallow_its_successor() {
        let mut body = vec![line(DiffLineType::Context)];
        body.extend((0..49).map(|_| line(DiffLineType::Delete)));
        // @@ -10,50 +10,1 @@
        let first = hunk(10, 1, body);
        // @@ -70,1 +21,1 @@
        let second = hunk(21, 1, vec![line(DiffLineType::Add)]);

        // The bound the old code compared against, kept explicit so the
        // regression this test guards is legible.
        assert_eq!(first.new_start_line + first.lines.len(), 60);

        assert_eq!(hunk_new_line_span(&first), 10..11);
        assert!(!hunk_covers_new_lines(&first, &(21..22)));
        assert!(hunk_covers_new_lines(&second, &(21..22)));
    }

    /// `diff.suppressBlankEmpty=true` makes git print a blank context line as
    /// an empty string instead of a single space, and the parser skips empty
    /// content lines outright. Re-deriving the extent by counting
    /// `Add | Context` entries therefore *undercuts* the real span and the
    /// button silently resolves to nothing. `new_line_count` comes from the
    /// header, which git computes from the file, so it is unaffected.
    #[test]
    fn dropped_blank_context_lines_do_not_shrink_the_span() {
        // @@ -1,5 +1,5 @@ over a hunk whose two blank context lines the parser
        // never materialised.
        let body = vec![
            line(DiffLineType::Context),
            line(DiffLineType::Add),
            line(DiffLineType::Context),
        ];
        let hunk = hunk(1, 5, body);

        let recount = hunk
            .lines
            .iter()
            .filter(|l| matches!(l.line_type, DiffLineType::Add | DiffLineType::Context))
            .count();
        assert_eq!(recount, 3, "the parser dropped two blank context lines");

        assert_eq!(hunk_new_line_span(&hunk), 1..6);
        // Lines 4 and 5 exist in the new file and the recount would have lost
        // both of them.
        assert!(hunk_covers_new_lines(&hunk, &(4..5)));
        assert!(hunk_covers_new_lines(&hunk, &(5..6)));
        assert!(!hunk_covers_new_lines(&hunk, &(6..7)));
    }

    /// A pure deletion occupies no new-file line at all. It is anchored on the
    /// line *after* the gap, which is where the editor draws its marker, and
    /// deliberately does not claim `new_start_line` — that line belongs to
    /// whatever hunk really contains it.
    #[test]
    fn pure_deletion_anchors_on_the_line_after_the_gap() {
        // @@ -3,2 +2,0 @@ -- the gap after new line 2.
        let deletion = hunk(
            2,
            0,
            vec![line(DiffLineType::Delete), line(DiffLineType::Delete)],
        );

        assert_eq!(hunk_new_line_span(&deletion), 3..4);
        // The editor reports a deletion gutter hunk as the empty 0-indexed
        // range `2..2`, which converts to the empty 1-indexed `3..3`; an empty
        // request is a point query, not a match against nothing.
        assert!(hunk_covers_new_lines(&deletion, &(3..3)));
        assert!(hunk_covers_new_lines(&deletion, &(3..4)));
        assert!(!hunk_covers_new_lines(&deletion, &(2..3)));
    }

    /// Under `diff.context=0` two pure deletions land one new-file line apart.
    /// With `.max(1)` padding and an inclusive `requested_start <= hunk_end`
    /// against a one-past-the-end bound they overlapped, and the click on the
    /// second staged the first.
    #[test]
    fn context_zero_adjacent_deletions_resolve_to_distinct_hunks() {
        // @@ -3,2 +2,0 @@
        let first = hunk(
            2,
            0,
            vec![line(DiffLineType::Delete), line(DiffLineType::Delete)],
        );
        // @@ -6,2 +3,0 @@
        let second = hunk(
            3,
            0,
            vec![line(DiffLineType::Delete), line(DiffLineType::Delete)],
        );

        let hunks = [first, second];

        let click_on_first = 3..3;
        assert!(hunk_covers_new_lines(&hunks[0], &click_on_first));
        assert!(!hunk_covers_new_lines(&hunks[1], &click_on_first));

        let click_on_second = 4..4;
        assert!(!hunk_covers_new_lines(&hunks[0], &click_on_second));
        assert!(hunk_covers_new_lines(&hunks[1], &click_on_second));

        // `find` must reach the second hunk, not stop at the first.
        let resolved = hunks
            .iter()
            .position(|h| hunk_covers_new_lines(h, &click_on_second));
        assert_eq!(resolved, Some(1));
    }

    /// The end convention is exclusive: hunks that abut in the new file share
    /// no line. This is the case the old `<=` comparison got wrong for every
    /// hunk, not just the degenerate ones.
    #[test]
    fn abutting_hunks_share_no_line() {
        let first = hunk(10, 11, vec![line(DiffLineType::Add)]);
        let second = hunk(21, 1, vec![line(DiffLineType::Add)]);

        assert_eq!(hunk_new_line_span(&first), 10..21);
        assert_eq!(hunk_new_line_span(&second), 21..22);

        assert!(hunk_covers_new_lines(&first, &(20..21)));
        assert!(!hunk_covers_new_lines(&first, &(21..22)));
        assert!(hunk_covers_new_lines(&second, &(21..22)));
        assert!(!hunk_covers_new_lines(&second, &(20..21)));
    }

    /// The span and the `@@` header `git apply` is handed must describe the
    /// same lines. Re-deriving one of them from `lines` is how they diverged.
    #[test]
    fn span_length_matches_the_header_count_hunk_to_patch_emits() {
        let hunk = hunk(
            7,
            3,
            vec![
                line(DiffLineType::Context),
                line(DiffLineType::Delete),
                line(DiffLineType::Add),
                line(DiffLineType::Context),
            ],
        );
        let span = hunk_new_line_span(&hunk);
        assert_eq!(span.end - span.start, hunk.new_line_count);

        let patch = super::hunk_to_patch(std::path::Path::new("a.txt"), &hunk);
        assert!(
            patch.contains("@@ -1,1 +7,3 @@"),
            "header disagrees with span: {patch}"
        );
    }
}
