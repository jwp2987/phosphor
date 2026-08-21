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
//! every write goes through the guarded `FileModel` API — all three limbs, on
//! both the *accept* and the `/rewind` path. The two paths guard against
//! different pre-images; [`accept_pre_image`] and [`revert_plan`] each say
//! which and why.
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ai::agent::action_result::RequestFileEditsResult;
use ai::diff_validation::{DiffDelta, DiffType};
use futures::FutureExt;
use futures::channel::oneshot;
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
/// `expected` is the pre-image the write is guarded against. There is
/// deliberately no way to ask this function for an unguarded write: nothing in
/// this module ever holds a live editor buffer — the TUI has none — so every
/// write it makes is derived from a snapshot and every one of them has a
/// pre-image it can state. The `/rewind` path used to pass `None` here; see
/// [`revert_plan`] for the pre-image that replaced it.
fn dispatch_write(
    file_model: &mut FileModel,
    session_type: &DiffSessionType,
    action: &PersistAction,
    path: &str,
    final_content: String,
    expected: ExpectedDiskState,
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
    // All three limbs are guarded, not just the `Write` one. `delete` removes a
    // file the snapshot reasoned about, and `rename_and_save` ends in
    // `async_fs::rename`, which replaces whatever is at the destination and
    // *succeeds* — no error path, no trace, no toast.
    let dispatch = match action {
        PersistAction::Delete => file_model.delete_if_unchanged(file_id, expected, version, ctx),
        PersistAction::Rename(to) => {
            // The destination's pre-image is always `Absent`.
            // `PersistAction::resolve` only produces a `Rename` for a target
            // that differs from the source, and `diff_application` only emits
            // such a rename when the target did not exist at proposal time —
            // a rename onto an existing file is rewritten there into a deletion
            // plus an update, which never reaches this arm.
            //
            // `revert_plan` never produces a `Rename`: undoing an applied
            // rename is a write at the original path plus a delete of the
            // renamed file, two separately guarded steps, because the two
            // endpoints have different pre-images by then.
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
        PersistAction::Write => {
            file_model.save_if_unchanged(file_id, final_content, expected, version, ctx)
        }
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
///
/// Every write is guarded, including the delete limbs — see [`revert_plan`] for
/// the pre-image each one asserts — and every write's outcome is observed
/// rather than dropped, because a guard whose refusal nobody reads is a guard
/// that is not there. What is *not* fixed here: `/rewind`'s caller
/// (`terminal_session_view`) still reports "Rewound conversation and reverted
/// file edits" unconditionally, because the outcomes arrive after it has
/// returned and it owns the footer hint this function cannot reach. A refusal
/// therefore lands in the log, not on screen.
pub(crate) fn revert_file_diffs(diffs: &[FileDiff], app: &mut AppContext) {
    for diff in diffs {
        let path = diff.file_path();
        match revert_plan(diff, &path) {
            Ok(steps) => {
                for step in steps {
                    dispatch_revert(step, app);
                }
            }
            // The accept derives its content the same way, so a derivation that
            // fails now failed then: `start_saving` returned a failed future and
            // never wrote anything, and there is correspondingly nothing to
            // undo. Reverting anyway would write the base over a file this edit
            // never touched.
            Err(error) => {
                log::warn!("Not reverting the file edit at {path}: {error}");
            }
        }
    }
}

/// Completion of the most recently dispatched revert write, so that the next one
/// waits for it instead of racing it.
///
/// Reverts are dispatched newest-first, and the caller depends on that order:
/// repeated edits to one file unwind through each intermediate state back to the
/// original. `FileModel`'s guarded writes run on spawned tasks, though, so
/// dispatch order is not execution order — two reverts of the same file
/// otherwise both probe the disk before either writes, and the older one finds
/// the newer edit's content instead of the state it expects. Unguarded that was
/// a silent coin-flip between the original content and an intermediate one;
/// guarded it would be a near-certain spurious refusal. Chaining each dispatch
/// onto its predecessor's completion makes the sequence the caller already
/// assumes actually hold.
///
/// The chain is a plain `Receiver` rather than a shared future because it is
/// linear: each tail is awaited by exactly one successor. A cancelled sender
/// resolves it immediately, which is the right answer — a write whose task went
/// away has had its turn.
thread_local! {
    static REVERT_CHAIN_TAIL: RefCell<Option<oneshot::Receiver<()>>> =
        const { RefCell::new(None) };
}

/// Dispatches one guarded revert write behind the [`REVERT_CHAIN_TAIL`] chain,
/// logging whatever it refuses or fails to do.
fn dispatch_revert(step: RevertStep, app: &mut AppContext) {
    let (finished, wait_for_this) = oneshot::channel::<()>();
    let wait_for_previous = REVERT_CHAIN_TAIL.with(|tail| tail.borrow_mut().replace(wait_for_this));

    FileModel::handle(app).update(app, |_file_model, ctx| {
        ctx.spawn(
            async move {
                if let Some(previous) = wait_for_previous {
                    let _ = previous.await;
                }
            },
            move |file_model, _, ctx| {
                let RevertStep {
                    path,
                    action,
                    content,
                    expected,
                } = step;
                // The TUI surface is always local; `revert_file_diffs`'
                // registry only ever holds diffs applied by this session.
                let session_type = DiffSessionType::Local;
                match dispatch_write(
                    file_model,
                    &session_type,
                    &action,
                    &path,
                    content,
                    expected,
                    ctx,
                ) {
                    Ok(completion) => {
                        ctx.spawn(completion, move |_file_model, outcome, _ctx| {
                            if let Err(error) = outcome {
                                log::warn!("Did not revert the file edit at {path}: {error}");
                            }
                            let _ = finished.send(());
                        });
                    }
                    Err(error) => {
                        log::warn!("Failed to revert the file edit at {path}: {error}");
                        let _ = finished.send(());
                    }
                }
            },
        );
    });
}

/// One write that undoes part of an applied diff, with the disk state it is
/// guarded against.
struct RevertStep {
    path: String,
    action: PersistAction,
    content: String,
    expected: ExpectedDiskState,
}

/// The write(s) that undo `diff`, in the order they must be applied.
///
/// # The pre-image a revert asserts
///
/// A revert's pre-image is not the diff base — that is what it is putting
/// *back*. It is what the accept left on disk, and the accept's own bytes are
/// not lost or unrecorded: [`TuiDiffStorage::start_saving`] wrote exactly
/// `final_content_from_op(&diff.base.content, &diff.diff_type)`, a pure function
/// of the diff this function is already holding. Nothing needs retaining at
/// accept time; the earlier note claiming otherwise was wrong about where the
/// accepted content lives, and it is why the delete limb below went unguarded.
///
/// So each step asserts the file is still exactly as the accept left it:
///
/// * `Create` — the accept wrote the insertion, so the delete that undoes it
///   requires the insertion to still be there. This is the limb that matters
///   most: unguarded, `/rewind` removed an agent-created file the user had since
///   built on, and a delete is the one outcome no later step can undo.
/// * `Delete` — the accept removed the file, so re-creating it requires the path
///   to still be free. `Absent` is checked with `symlink_metadata`, so a file
///   somebody re-created — or a symlink somebody dropped there — refuses instead
///   of being overwritten (or followed).
/// * `Update` with a rename — the accept moved the file, so the original path
///   must still be free and the renamed file must still hold the accepted
///   content. Two steps, two different pre-images, which is why this does not go
///   through [`PersistAction::Rename`].
/// * `Update` in place — the accept overwrote the file, so the revert requires
///   the accepted content to still be there.
///
/// A formatter, a build step or the user having touched the file since the
/// accept therefore refuses the revert rather than silently discarding their
/// work. That is the intended trade: the common case (nothing touched the file
/// between accepting and rewinding) still passes, and `ExpectedDiskState`'s
/// comparison is line-ending-normalised, so a CRLF checkout is not a refusal.
fn revert_plan(diff: &FileDiff, path: &str) -> Result<Vec<RevertStep>, String> {
    let accepted = final_content_from_op(&diff.base.content, &diff.diff_type)?;
    let steps = match &diff.diff_type {
        // The edit created the file; delete it to revert.
        DiffType::Create { .. } => vec![RevertStep {
            path: path.to_owned(),
            action: PersistAction::Delete,
            content: String::new(),
            expected: ExpectedDiskState::Content(accepted),
        }],
        // The edit deleted the file; re-create it with the base content.
        DiffType::Delete { .. } => vec![RevertStep {
            path: path.to_owned(),
            action: PersistAction::Write,
            content: diff.base.content.clone(),
            expected: ExpectedDiskState::Absent,
        }],
        // The edit renamed `path` → `to`; restore the base content at the
        // original path and remove the renamed file.
        DiffType::Update {
            rename: Some(to), ..
        } if to.to_string_lossy() != path => vec![
            RevertStep {
                path: path.to_owned(),
                action: PersistAction::Write,
                content: diff.base.content.clone(),
                expected: ExpectedDiskState::Absent,
            },
            RevertStep {
                path: to.to_string_lossy().to_string(),
                action: PersistAction::Delete,
                content: String::new(),
                expected: ExpectedDiskState::Content(accepted),
            },
        ],
        // In-place update; write the base content back.
        DiffType::Update { .. } => vec![RevertStep {
            path: path.to_owned(),
            action: PersistAction::Write,
            content: diff.base.content.clone(),
            expected: ExpectedDiskState::Content(accepted),
        }],
    };
    Ok(steps)
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
                            expected,
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
