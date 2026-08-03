//! Surface-agnostic storage + persistence for pending file-edit diffs.
//!
//! Ported from warp/master `app/src/ai/blocklist/diff_storage.rs`. Zap keeps its
//! diff types (`FileDiff`/`DiffSessionType`) in
//! [`inline_action::code_diff_view`] rather than a `diff_types` module, and
//! `changed_lines_from_op` (which warp keeps in that same `diff_types` module) is
//! co-located here over Zap's [`ai::diff_validation::DiffType`]. The traits let a
//! single accept flow drive both the GUI review surface and the headless TUI
//! surface (which has no editor buffers — see `warp_tui`'s `TuiDiffStorage`).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use ai::diff_validation::{DiffDelta, DiffType};
use futures::future::{join_all, BoxFuture};
use futures::FutureExt;
use itertools::Itertools;
use warp_editor::multiline::AnyMultilineString;
use warp_util::file::FileSaveError;
use warpui::AppContext;

use crate::ai::agent::{
    AnyFileContent, FileContext, FileLocations, RequestFileEditsResult, UpdatedFileContext,
};
use crate::ai::blocklist::inline_action::code_diff_view::{DiffSessionType, FileDiff};
use crate::code::editor::compute_unified_diff;
use crate::code::DiffResult;

const APPLY_DIFF_RESULT_CONTEXT_LINES: usize = 10;

/// Resolves with the outcome of one file's dispatched save.
pub type SaveFuture = BoxFuture<'static, Result<(), Arc<FileSaveError>>>;

/// A surface that stores pending file-edit diffs and persists them on accept.
///
/// This trait is only ever implemented, never imported for its methods: every
/// method is required — the accept-time snapshot (the fields live on each
/// impl, since traits cannot hold state) plus the surface-specific write
/// kickoff ([`Self::start_saving`]). The shared save-completion flow lives on
/// [`DiffStorageHelper`]; callers drive an accept solely through
/// [`DiffStorageHelper::accept_and_save`].
pub trait DiffStorage {
    /// Snapshot of per-file state for result assembly, captured as the accept
    /// kicks off: reported paths, changed lines, contents, and user-edit
    /// flags. Snapshotting at kickoff means the result reports exactly the
    /// content handed to [`Self::start_saving`].
    fn snapshot_pending_files(&self, app: &AppContext) -> Vec<FileSnapshot>;

    /// Kicks off persistence for every pending file, returning each
    /// dispatched save's completion future.
    ///
    /// The surface-specific hook invoked by
    /// [`DiffStorageHelper::accept_and_save`] — never called directly by
    /// callers. The GUI saves through its editor buffers; surfaces without
    /// editor buffers dispatch writes to `FileModel`.
    fn start_saving(&mut self, app: &mut AppContext) -> Vec<SaveFuture>;
}

/// The shared save-completion flow over an impl of [`DiffStorage`].
///
/// Defined within a separate trait rather than a default implementation of
/// `DiffStorage` so implementations cannot errantly override it (the same
/// convention as `AIBlockModelHelper`).
pub trait DiffStorageHelper {
    /// The entry point for accepting a surface's diffs: snapshots per-file
    /// state, persists every file, and resolves with the assembled result
    /// once every save completes.
    fn accept_and_save(
        &mut self,
        app: &mut AppContext,
    ) -> BoxFuture<'static, RequestFileEditsResult>;
}

impl<T: DiffStorage> DiffStorageHelper for T {
    fn accept_and_save(
        &mut self,
        app: &mut AppContext,
    ) -> BoxFuture<'static, RequestFileEditsResult> {
        let files = self.snapshot_pending_files(app);
        let saves = self.start_saving(app);
        async move {
            let save_errors = join_all(saves)
                .await
                .into_iter()
                .filter_map(Result::err)
                .collect_vec();
            if !save_errors.is_empty() {
                return save_failure_result(&save_errors);
            }

            let mut combined = DiffResult::default();
            for file in &files {
                let base = AnyMultilineString::infer(file.diff_base.clone());
                let new = AnyMultilineString::infer(file.diff_new.clone());
                combined += &compute_unified_diff(
                    base.to_format().as_ref(),
                    new.to_format().as_ref(),
                    &file.diff_name,
                )
                .await;
            }
            assemble_result(combined, files)
        }
        .boxed()
    }
}

/// The executor-facing handle over a registered [`DiffStorage`] surface.
///
/// A separate trait from [`DiffStorage`] because the executor holds
/// surfaces by handle, and GUI view handles and model handles share no common
/// type. Each surface's handle type (e.g. `WeakViewHandle<CodeDiffView>`)
/// implements this directly, delegating each call to its entity's
/// [`DiffStorageHelper`] flow.
pub trait RegisteredDiffStorage {
    /// Pushes resolved diffs into the surface (called when preprocess resolves).
    fn set_candidate_diffs(
        &self,
        diffs: Vec<FileDiff>,
        session_type: DiffSessionType,
        app: &mut AppContext,
    );

    /// Persists all diffs, resolving with the result reported to the LLM.
    fn accept_and_save(&self, app: &mut AppContext) -> BoxFuture<'static, RequestFileEditsResult>;
}

/// One file's contribution to the assembled result, snapshotted at accept
/// time.
#[derive(Clone)]
pub struct FileSnapshot {
    /// The updated file, reported at its final path; `None` for deletions.
    pub updated: Option<UpdatedFileState>,
    /// Paths reported as deleted (the deleted file, or a rename's source path).
    pub deleted_paths: Vec<String>,
    /// Base content the result diff is computed against.
    pub diff_base: String,
    /// Final content the result diff is computed from.
    pub diff_new: String,
    /// Path used for the result diff's header.
    pub diff_name: String,
}

/// Report state for a created or updated file.
#[derive(Clone)]
pub struct UpdatedFileState {
    /// Path the update is reported at (the rename target when renamed).
    pub path: String,
    /// 1-indexed changed line ranges.
    pub changed_lines: Vec<Range<usize>>,
    /// Final file content.
    pub final_content: String,
    /// Whether the user hand-edited the content during review.
    pub was_edited: bool,
}

/// Fails the whole edit when any file failed to save.
fn save_failure_result(errors: &[Arc<FileSaveError>]) -> RequestFileEditsResult {
    let error = errors
        .iter()
        .map(|error| match error.as_ref() {
            FileSaveError::IOError { error, path } => {
                format!("Failed to save file {path:?}: {error}")
            }
            other => other.to_string(),
        })
        .join("\n");
    RequestFileEditsResult::DiffApplicationFailed { error }
}

/// Combines per-file report state and the combined result diff into one
/// [`RequestFileEditsResult`].
fn assemble_result(combined: DiffResult, files: Vec<FileSnapshot>) -> RequestFileEditsResult {
    let mut updated_files = Vec::new();
    let mut deleted_files = Vec::new();
    let mut content_map: HashMap<String, String> = HashMap::new();
    for file in files {
        if let Some(updated) = file.updated {
            content_map.insert(updated.path.clone(), updated.final_content);
            updated_files.push((
                FileLocations {
                    name: updated.path,
                    lines: updated.changed_lines,
                },
                updated.was_edited,
            ));
        }
        deleted_files.extend(file.deleted_paths);
    }

    RequestFileEditsResult::Success {
        diff: combined.unified_diff,
        updated_files: updated_file_contexts_from_content_map(&updated_files, &content_map),
        deleted_files,
        lines_added: combined.lines_added,
        lines_removed: combined.lines_removed,
    }
}

/// Expands each updated file's changed lines with surrounding context and
/// extracts the corresponding fragments from the final file content.
fn updated_file_contexts_from_content_map(
    updated_files: &[(FileLocations, bool)],
    content_map: &HashMap<String, String>,
) -> Vec<UpdatedFileContext> {
    updated_files
        .iter()
        .flat_map(|(file_location, was_edited)| {
            let content = content_map
                .get(&file_location.name)
                .cloned()
                .unwrap_or_default();
            let line_count = content.lines().count();

            let mut file_location = file_location.clone();
            file_location.expand_surrounding_context(APPLY_DIFF_RESULT_CONTEXT_LINES);
            clamp_to_file_context_range_start(&mut file_location);

            if file_location.lines.is_empty() {
                return vec![UpdatedFileContext {
                    was_edited_by_user: *was_edited,
                    file_context: FileContext {
                        file_name: file_location.name,
                        content: AnyFileContent::StringContent(content),
                        line_range: None,
                        last_modified: None,
                        line_count,
                    },
                }];
            }

            let lines = content.lines().collect_vec();
            file_location
                .lines
                .into_iter()
                .map(|range| {
                    let start = range.start.saturating_sub(1).min(lines.len());
                    let end = range.end.saturating_sub(1).min(lines.len());
                    let fragment = if start >= end {
                        String::new()
                    } else {
                        lines[start..end].join("\n")
                    };

                    UpdatedFileContext {
                        was_edited_by_user: *was_edited,
                        file_context: FileContext {
                            file_name: file_location.name.clone(),
                            content: AnyFileContent::StringContent(fragment),
                            line_range: Some(range),
                            last_modified: None,
                            line_count,
                        },
                    }
                })
                .collect_vec()
        })
        .collect()
}

/// Clamps line ranges to the 1-indexed space used by file contexts.
fn clamp_to_file_context_range_start(file_location: &mut FileLocations) {
    for range in &mut file_location.lines {
        range.start = range.start.max(1);
        range.end = range.end.max(range.start);
    }
}

/// The 1-indexed changed line ranges implied by a diff op, for result
/// reporting. Ported from warp/master's `diff_types::changed_lines_from_op`,
/// over Zap's [`ai::diff_validation::DiffType`].
pub fn changed_lines_from_op(diff_type: &DiffType) -> Vec<Range<usize>> {
    match diff_type {
        DiffType::Create { delta } => inserted_content_range(1, &delta.insertion)
            .into_iter()
            .collect(),
        DiffType::Update { deltas, .. } => deltas
            .iter()
            .filter_map(changed_line_range_for_delta)
            .collect(),
        DiffType::Delete { .. } => vec![],
    }
}

/// The changed line range for one delta: the replacement range when it spans
/// lines, else the range covered by inserted content at the insertion point.
fn changed_line_range_for_delta(delta: &DiffDelta) -> Option<Range<usize>> {
    let replacement_range = &delta.replacement_line_range;
    if replacement_range.start == replacement_range.end {
        return inserted_content_range(replacement_range.start.max(1), &delta.insertion);
    }
    Some(replacement_range.clone())
}

/// Returns the line range covered by inserted content starting at `start`.
fn inserted_content_range(start: usize, content: &str) -> Option<Range<usize>> {
    let line_count = content.lines().count();
    (line_count > 0).then_some(start..start + line_count)
}

#[cfg(test)]
#[path = "diff_storage_tests.rs"]
mod tests;
