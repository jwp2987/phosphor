//! The TUI surface's diff storage for `RequestFileEdits` actions.
//!
//! [`TuiDiffStorage`] implements the app's surface-agnostic [`DiffStorage`]
//! contract: it holds the resolved diffs and persists them on accept by
//! writing through [`FileModel`]. The TUI has no review UI or editor
//! buffers, so final content is derived by applying each diff's deltas to its
//! base. The file-edits view registers one per action with the shared executor
//! and renders a compact summary over it.
//!
//! Content derived from a base read at proposal time is snapshot-derived, so
//! every write on the *accept* path goes through the guarded `FileModel` API —
//! all three limbs, not just the plain write. `/rewind` is deliberately
//! unguarded; [`revert_file_diffs`] says why.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ai::agent::action_result::RequestFileEditsResult;
use ai::diff_validation::{DiffDelta, DiffType};
use futures::FutureExt;
use futures::future::BoxFuture;
use warp::tui_export::{
    DiffSessionType, DiffStorage, DiffStorageHelper, FileDiff, FileSnapshot, RegisteredDiffStorage,
    SaveFuture, UpdatedFileState, changed_lines_from_op,
};
use warp_files::{ExpectedDiskState, FileModel};
use warp_util::content_version::ContentVersion;
use warp_util::file::{FileId, FileSaveError};
use warp_util::standardized_path::StandardizedPath;
use warpui::{AppContext, Entity, ModelContext, ModelHandle, SingletonEntity};

/// Derives the final file content for a diff from its base content and deltas.
///
/// Used because the TUI has no editor buffers; the GUI reads final content
/// from its buffers instead.
fn final_content_from_op(base_content: &str, op: &DiffType) -> Result<String, String> {
    match op {
        DiffType::Create { delta } => Ok(delta.insertion.clone()),
        DiffType::Update { deltas, .. } => apply_deltas_to_content(base_content, deltas),
        DiffType::Delete { .. } => Ok(String::new()),
    }
}

/// Applies line-range replacement deltas to `content`, producing the new content.
fn apply_deltas_to_content(content: &str, deltas: &[DiffDelta]) -> Result<String, String> {
    let mut lines = split_lines_preserving_newlines(content);
    let mut deltas = deltas.to_vec();
    deltas.sort_by_key(|delta| delta.replacement_line_range.start);

    for delta in deltas.into_iter().rev() {
        let start = delta.replacement_line_range.start.saturating_sub(1);
        let end = delta.replacement_line_range.end.saturating_sub(1);
        if start > lines.len() || end > lines.len() || start > end {
            return Err(format!(
                "Diff range {:?} is out of bounds for file with {} lines",
                delta.replacement_line_range,
                lines.len()
            ));
        }
        let mut insertion = delta.insertion;
        // Insertions often lack a trailing newline; without one the splice
        // would run into the next line (mirrors `EditorModel::apply_diffs`).
        if !insertion.is_empty() && !insertion.ends_with('\n') && !content.is_empty() {
            insertion.push('\n');
        }
        let replacement = split_lines_preserving_newlines(&insertion);
        lines.splice(start..end, replacement);
    }

    Ok(lines.concat())
}

/// Splits content into lines while keeping trailing newlines, so reassembly via
/// `concat` reproduces the original byte-for-byte.
fn split_lines_preserving_newlines(content: &str) -> Vec<String> {
    if content.is_empty() {
        Vec::new()
    } else {
        content.split_inclusive('\n').map(str::to_string).collect()
    }
}

/// The actual write operation performed for a file.
enum PersistAction {
    /// Write the final content at the file's original path.
    Write,
    /// Move the file to the new path and write the final content there.
    Rename(PathBuf),
    /// Delete the file.
    Delete,
}

impl PersistAction {
    /// Decides the write operation once from the diff op and backend. Remote
    /// sessions have no rename primitive, so remote renames fall back to an
    /// in-place write at the original path (and are reported as such).
    fn resolve(op: &DiffType, session_type: &DiffSessionType, path: &str) -> Self {
        match (op, session_type) {
            (DiffType::Delete { .. }, _) => PersistAction::Delete,
            (
                DiffType::Update {
                    rename: Some(to), ..
                },
                DiffSessionType::Local,
            ) if to.to_string_lossy() != path => PersistAction::Rename(to.clone()),
            _ => PersistAction::Write,
        }
    }
}

/// Registers `path` with [`FileModel`] using the session's local/remote backend.
fn register_file(
    file_model: &mut FileModel,
    session_type: &DiffSessionType,
    path: &str,
    ctx: &mut ModelContext<FileModel>,
) -> Result<FileId, FileSaveError> {
    match session_type {
        DiffSessionType::Local => Ok(file_model.register_file_path(Path::new(path), false, ctx)),
        DiffSessionType::Remote(host_id) => {
            let standardized = StandardizedPath::try_new(path)
                .map_err(|_| FileSaveError::RemoteError(format!("Invalid remote path: {path}")))?;
            Ok(file_model.register_remote_file(host_id.clone(), standardized))
        }
    }
}

/// The state each write mode requires the file to still be in for an *accept*.
///
/// The TUI has no editor buffers, so its content is derived from `diff.base`:
/// a snapshot of the file taken when the edit was proposed, possibly minutes
/// before the user accepted it. That is precisely the caller
/// [`FileModel::save_if_unchanged`] exists for. A creation asserts the file is
/// still absent (the diff was only offered because it was); everything else
/// asserts the file still holds the base the deltas were computed against.
fn accept_pre_image(diff: &FileDiff) -> ExpectedDiskState {
    match &diff.diff_type {
        DiffType::Create { .. } => ExpectedDiskState::Absent,
        DiffType::Update { .. } | DiffType::Delete { .. } => {
            ExpectedDiskState::Content(diff.base.content.clone())
        }
    }
}

/// Registers a file with [`FileModel`] and dispatches its write, returning the
/// write's completion future.
///
/// `expected` is the pre-image the write is guarded against, or `None` to write
/// unguarded. Only [`revert_file_diffs`] passes `None`, and it says there why.
fn dispatch_write(
    file_model: &mut FileModel,
    session_type: &DiffSessionType,
    action: &PersistAction,
    path: &str,
    final_content: String,
    expected: Option<ExpectedDiskState>,
    ctx: &mut ModelContext<FileModel>,
) -> Result<SaveFuture, FileSaveError> {
    let file_id = register_file(file_model, session_type, path, ctx)?;
    let version = ContentVersion::new();
    // Zap's FileModel is EVENT-based, not future-based: save/delete/
    // rename_and_save dispatch the write on a spawned task and return
    // `Result<(), FileSaveError>` for the SYNCHRONOUS setup errors only
    // (the async write completes later via FileModelEvent). warp's contract
    // wants a SaveFuture that resolves on the ACTUAL write outcome, so we
    // register a `save_completion` waiter and return it: it resolves `Ok` on
    // FileSaved and `Err` on FailedToSave. A synchronous dispatch error still
    // short-circuits as an immediately-failed future.
    //
    // All three limbs are guarded on the accept path, not just the `Write` one.
    // `delete` removes a file the snapshot reasoned about, and `rename_and_save`
    // ends in `async_fs::rename`, which replaces whatever is at the destination
    // and *succeeds* — no error path, no trace, no toast.
    let dispatch = match (action, expected) {
        (PersistAction::Delete, Some(expected)) => {
            file_model.delete_if_unchanged(file_id, expected, version, ctx)
        }
        (PersistAction::Delete, None) => file_model.delete(file_id, version, ctx),
        (PersistAction::Rename(to), Some(expected)) => {
            // The destination's pre-image is always `Absent`.
            // `PersistAction::resolve` only produces a `Rename` for a target
            // that differs from the source, and `diff_application` only emits
            // such a rename when the target did not exist at proposal time —
            // a rename onto an existing file is rewritten there into a deletion
            // plus an update, which never reaches this arm.
            file_model.rename_and_save_if_unchanged(
                file_id,
                to.clone(),
                final_content,
                expected,
                ExpectedDiskState::Absent,
                version,
                ctx,
            )
        }
        (PersistAction::Rename(to), None) => {
            file_model.rename_and_save(file_id, to.clone(), final_content, version, ctx)
        }
        (PersistAction::Write, Some(expected)) => {
            file_model.save_if_unchanged(file_id, final_content, expected, version, ctx)
        }
        (PersistAction::Write, None) => file_model.save(file_id, final_content, version, ctx),
    };
    // Register the completion waiter before the spawned write can resolve (the
    // callback only runs after this synchronous frame yields). Only for a
    // successful dispatch — a synchronous error never spawns a write, so it
    // would leave an unfired waiter.
    let completion = dispatch.map(|()| file_model.save_completion(file_id));
    // Release the temporary registration either way so FileModel state doesn't
    // grow unboundedly. This does not cancel the spawned write or the waiter.
    file_model.unsubscribe(file_id, ctx);
    completion
}

/// Builds a file's report state and result-diff inputs to mirror the write
/// that `action` will actually perform.
fn persist_outcome(
    action: &PersistAction,
    diff: &FileDiff,
    path: &str,
    final_content: &str,
) -> FileSnapshot {
    let changed_lines = changed_lines_from_op(&diff.diff_type);
    match action {
        PersistAction::Delete => FileSnapshot {
            updated: None,
            deleted_paths: vec![path.to_owned()],
            diff_base: diff.base.content.clone(),
            diff_new: String::new(),
            diff_name: path.to_owned(),
        },
        PersistAction::Rename(to) => {
            let target = to.to_string_lossy().to_string();
            FileSnapshot {
                updated: Some(UpdatedFileState {
                    path: target.clone(),
                    changed_lines,
                    final_content: final_content.to_owned(),
                    was_edited: false,
                }),
                deleted_paths: vec![path.to_owned()],
                diff_base: diff.base.content.clone(),
                diff_new: final_content.to_owned(),
                diff_name: target,
            }
        }
        PersistAction::Write => FileSnapshot {
            updated: Some(UpdatedFileState {
                path: path.to_owned(),
                changed_lines,
                final_content: final_content.to_owned(),
                was_edited: false,
            }),
            deleted_paths: Vec::new(),
            diff_base: diff.base.content.clone(),
            diff_new: final_content.to_owned(),
            diff_name: path.to_owned(),
        },
    }
}

/// Restores the pre-edit state of each applied diff, undoing a file edit — the
/// inverse of [`TuiDiffStorage::start_saving`]. For an `Update`/`Delete` it
/// writes the diff's base (pre-edit) content back; for a `Create` it removes the
/// file the edit added; for a rename it restores the original path and removes
/// the renamed file. Best-effort: per-file failures are logged, not fatal.
///
/// Used by `/rewind` (via [`crate::tui_revert_registry`]). The TUI surface is
/// always local, so writes go through the local [`FileModel`] backend.
pub(crate) fn revert_file_diffs(diffs: &[FileDiff], app: &mut AppContext) {
    let session_type = DiffSessionType::Local;
    let file_model = FileModel::handle(app);
    for diff in diffs {
        let path = diff.file_path();
        for (write_path, action, content) in revert_plan(diff, &path) {
            // Deliberately unguarded, unlike the accept path above. A revert's
            // pre-image is not the diff base but whatever the accept wrote, and
            // whatever has happened to the file since — a formatter, a build
            // step, the user. Guarding against the base would refuse the common
            // case rather than the dangerous one, and guarding against the
            // accepted content needs it retained at accept time. Filed as a
            // follow-up rather than guessed at here; the GUI's
            // `InlineDiffView::restore_diff_base` carries the same note.
            let result = file_model.update(app, |file_model, ctx| {
                dispatch_write(
                    file_model,
                    &session_type,
                    &action,
                    &write_path,
                    content,
                    None,
                    ctx,
                )
            });
            if let Err(error) = result {
                log::warn!("Failed to revert file edit at {write_path}: {error}");
            }
        }
    }
}

/// The write(s) that undo `diff`, as `(path, action, content)` tuples applied in
/// order.
fn revert_plan(diff: &FileDiff, path: &str) -> Vec<(String, PersistAction, String)> {
    match &diff.diff_type {
        // The edit created the file; delete it to revert.
        DiffType::Create { .. } => vec![(path.to_owned(), PersistAction::Delete, String::new())],
        // The edit deleted the file; re-create it with the base content.
        DiffType::Delete { .. } => {
            vec![(path.to_owned(), PersistAction::Write, diff.base.content.clone())]
        }
        // The edit renamed `path` → `to`; restore the base content at the
        // original path and remove the renamed file.
        DiffType::Update {
            rename: Some(to), ..
        } if to.to_string_lossy() != path => vec![
            (path.to_owned(), PersistAction::Write, diff.base.content.clone()),
            (
                to.to_string_lossy().to_string(),
                PersistAction::Delete,
                String::new(),
            ),
        ],
        // In-place update; write the base content back.
        DiffType::Update { .. } => {
            vec![(path.to_owned(), PersistAction::Write, diff.base.content.clone())]
        }
    }
}

/// A save future that fails immediately with `error`.
fn ready_save_failure(error: FileSaveError) -> SaveFuture {
    futures::future::ready(Err(Arc::new(error))).boxed()
}

/// Events emitted by [`TuiDiffStorage`].
pub(crate) enum TuiDiffStorageEvent {
    /// The executor seeded the storage with resolved diffs.
    CandidateDiffsSet,
}

/// The TUI surface's diff storage: holds the resolved diffs and persists them
/// by writing straight through [`FileModel`], with no review UI or editor
/// buffers of its own.
pub(crate) struct TuiDiffStorage {
    diffs: Vec<FileDiff>,
    session_type: DiffSessionType,
}

impl TuiDiffStorage {
    /// Creates storage over resolved diffs.
    pub(crate) fn new(diffs: Vec<FileDiff>, session_type: DiffSessionType) -> Self {
        Self {
            diffs,
            session_type,
        }
    }

    /// The stored diffs (for views rendering a summary over this storage).
    pub(crate) fn diffs(&self) -> &[FileDiff] {
        &self.diffs
    }

    /// Replaces the stored diffs and session backend.
    fn set_candidate_diffs(&mut self, diffs: Vec<FileDiff>, session_type: DiffSessionType) {
        self.diffs = diffs;
        self.session_type = session_type;
    }
}

impl DiffStorage for TuiDiffStorage {
    fn snapshot_pending_files(&self, _app: &AppContext) -> Vec<FileSnapshot> {
        self.diffs
            .iter()
            .map(|diff| {
                let path = diff.file_path();
                let action = PersistAction::resolve(&diff.diff_type, &self.session_type, &path);
                // A derivation failure is surfaced by `start_saving`; the
                // snapshot is unused when the accept fails.
                let final_content =
                    final_content_from_op(&diff.base.content, &diff.diff_type).unwrap_or_default();
                persist_outcome(&action, diff, &path, &final_content)
            })
            .collect()
    }

    fn start_saving(&mut self, app: &mut AppContext) -> Vec<SaveFuture> {
        let file_model = FileModel::handle(app);
        let session_type = self.session_type.clone();
        self.diffs
            .iter()
            .map(|diff| {
                let path = diff.file_path();
                let final_content = match final_content_from_op(&diff.base.content, &diff.diff_type)
                {
                    Ok(content) => content,
                    Err(error) => return ready_save_failure(FileSaveError::Other(error)),
                };
                let action = PersistAction::resolve(&diff.diff_type, &session_type, &path);
                let expected = accept_pre_image(diff);
                file_model
                    .update(app, |file_model, ctx| {
                        dispatch_write(
                            file_model,
                            &session_type,
                            &action,
                            &path,
                            final_content,
                            Some(expected),
                            ctx,
                        )
                    })
                    .unwrap_or_else(ready_save_failure)
            })
            .collect()
    }
}

impl Entity for TuiDiffStorage {
    type Event = TuiDiffStorageEvent;
}

/// The handle the TUI registers as the executor's storage.
///
/// Wraps the model handle because [`RegisteredDiffStorage`] and
/// [`ModelHandle`] are both foreign to this crate, so the orphan rule forbids
/// implementing the trait on the handle directly.
pub(crate) struct TuiDiffStorageHandle {
    storage: ModelHandle<TuiDiffStorage>,
}

impl TuiDiffStorageHandle {
    /// Wraps a storage handle for registration with the executor.
    pub(crate) fn new(storage: ModelHandle<TuiDiffStorage>) -> Self {
        Self { storage }
    }
}

impl RegisteredDiffStorage for TuiDiffStorageHandle {
    fn set_candidate_diffs(
        &self,
        diffs: Vec<FileDiff>,
        session_type: DiffSessionType,
        app: &mut AppContext,
    ) {
        self.storage.update(app, |model, ctx| {
            model.set_candidate_diffs(diffs, session_type);
            ctx.emit(TuiDiffStorageEvent::CandidateDiffsSet);
        });
    }

    fn accept_and_save(&self, app: &mut AppContext) -> BoxFuture<'static, RequestFileEditsResult> {
        self.storage.update(app, |model, ctx| {
            DiffStorageHelper::accept_and_save(model, ctx)
        })
    }
}

#[cfg(test)]
#[path = "tui_diff_storage_tests.rs"]
mod tests;
