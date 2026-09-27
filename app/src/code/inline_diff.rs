#[cfg(not(target_family = "wasm"))]
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;

#[cfg(not(target_family = "wasm"))]
use crate::ai::blocklist::inline_action::code_diff_view::DiffSessionType;
use ai::diff_validation::DiffType;
#[cfg(not(target_family = "wasm"))]
use warp_files::{ExpectedDiskState, FileModel, FileModelEvent};
#[cfg(not(target_family = "wasm"))]
use warp_util::content_version::ContentVersion;
use warp_util::file::FileId;
#[cfg(not(target_family = "wasm"))]
use warp_util::file::FileSaveError;
use warp_util::standardized_path::StandardizedPath;
use warpui::elements::ChildView;
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};
#[cfg(not(target_family = "wasm"))]
use warpui::{ModelContext, SingletonEntity};

use super::DiffResult;
use super::diff_viewer::DiffViewer;
use super::diff_viewer::DisplayMode;
use super::editor::NavBarBehavior;
use super::editor::scroll::{ScrollPosition, ScrollTrigger};
use super::editor::view::{CodeEditorEvent, CodeEditorView};
use crate::editor::InteractionState;

pub enum InlineDiffViewEvent {
    DiffStatusUpdated,
    #[cfg(not(target_family = "wasm"))]
    FileLoaded,
    #[cfg(not(target_family = "wasm"))]
    FileSaved,
    #[cfg(not(target_family = "wasm"))]
    FailedToSave {
        error: Rc<FileSaveError>,
    },
    DiffAccepted {
        diff: Rc<DiffResult>,
    },
    UserEdited,
}

/// An inline diff viewer with optional file-backed save support.
///
/// When a backing file is registered (via [`Self::register_file`]), this view supports the full
/// accept/save/revert lifecycle through `FileModel`. Without a registered file, it behaves
/// as a read-only diff viewer (e.g. for WASM or restored conversations).
pub struct InlineDiffView {
    editor: ViewHandle<CodeEditorView>,
    diff_type: Option<DiffType>,
    file_path: Option<StandardizedPath>,
    /// Whether the user has edited the diff content.
    was_edited: bool,
    /// `FileModel` file ID for the backing file. Set via [`Self::register_file`].
    ///
    /// When `Some`:
    /// - The editor is editable (interaction state follows the `DisplayMode` rules).
    /// - Accept, save, and revert operations write through `FileModel`.
    ///
    /// When `None` (WASM, restored conversations, or before registration):
    /// - The editor is selection-only (never editable).
    /// - Accept and save are no-ops; revert writes nothing and reports
    ///   [`RevertDispatch::NoBackingFile`].
    backing_file_id: Option<FileId>,
    /// The session backend the file was registered against. Set via
    /// [`Self::register_file`]. `None` (WASM, or before registration) is
    /// treated the same as `Local` by [`Self::write_action`]: there is no
    /// backing file either way, so nothing is ever dispatched.
    #[cfg(not(target_family = "wasm"))]
    session_type: Option<DiffSessionType>,
    /// Whether the diff is a new file creation (for revert: delete instead of restore).
    #[cfg(not(target_family = "wasm"))]
    is_new_file: bool,
    /// What the accept actually did to the file, recorded at accept time.
    /// This is the pre-image a revert asserts: a revert undoes the accept, so
    /// it may only run against a file that still holds what the accept left
    /// there — or, for a delete or a rename, only against the disk state the
    /// accept left behind. `None` until an accept has dispatched a write.
    ///
    /// A `RefCell` because the accept path (`DiffViewer::accept_and_save_diff`)
    /// takes `&self`.
    #[cfg(not(target_family = "wasm"))]
    accepted_action: RefCell<Option<AcceptedAction>>,
    /// Set once a revert has dispatched its write, so that the write's
    /// asynchronous refusal is reported as a failed *revert* rather than as a
    /// failed save of the accept.
    #[cfg(not(target_family = "wasm"))]
    revert_dispatched: bool,
    /// Swallows the *next* `FileSaved`/`FailedToSave` event for
    /// [`Self::backing_file_id`] instead of forwarding it as
    /// [`InlineDiffViewEvent::FileSaved`]/[`InlineDiffViewEvent::FailedToSave`].
    ///
    /// Undoing an accepted rename is two guarded writes — restore the
    /// original path, then remove the file left at the destination — and the
    /// ordinary per-file subscription set up in [`Self::finish_file_registration`]
    /// would report the *first* write's outcome as if the whole revert were
    /// decided. Set immediately after dispatching that first write; consumed
    /// (and left `false`) by the very next event for this file, whichever it
    /// is. `finish_rename_revert` reports the real, combined outcome once both
    /// writes have resolved.
    #[cfg(not(target_family = "wasm"))]
    suppress_next_backing_file_event: Cell<bool>,
    /// The file's text exactly as the edit was proposed against — raw, with
    /// its own line endings — which is what a revert puts back. Set with
    /// [`Self::set_original_content`]; `None` refuses the revert.
    ///
    /// Not the editor's diff base: `CodeEditorModel::set_base` normalises
    /// that to LF, so writing it back turned a CRLF file into an LF one, and
    /// the guard (rightly) refused the write as converting the file's line
    /// endings — every revert of a CRLF file was refused (#672). For a file
    /// with mixed endings it would also have rewritten the minority lines.
    /// The TUI's `revert_plan` restores `diff.base.content` the same way.
    #[cfg(not(target_family = "wasm"))]
    original_content: Option<String>,
}

/// What an accept actually did to the file, recorded so a later revert knows
/// both the pre-image to assert and the inverse write(s) to make. See
/// [`InlineDiffView::accepted_action`].
#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Debug)]
enum AcceptedAction {
    /// The accept wrote `content` at the registered path: a creation, or an
    /// in-place update.
    Wrote { content: String },
    /// The accept deleted the file at the registered path. A revert re-creates
    /// it with [`InlineDiffView::original_content`].
    Deleted,
    /// The accept moved the file from the registered path to `to`, writing
    /// `content` there. A revert restores
    /// [`InlineDiffView::original_content`] at the registered path and
    /// removes `to` if it still holds `content`.
    Renamed { to: PathBuf, content: String },
    /// A rename revert's first step landed — the registered path holds the
    /// original text again — but its second step (removing whatever the
    /// accept left at `to`) has not: it either has not been attempted yet, or
    /// was refused because `to` no longer holds `content`. Recorded so a
    /// retry does not attempt the first step again, whose `Absent` guard
    /// would now refuse against the very state that step just established
    /// (see [`InlineDiffView::finish_rename_revert`]); only `to` remains to
    /// be dealt with.
    PartiallyRevertedRename { to: PathBuf, content: String },
}

impl InlineDiffView {
    pub fn new(
        editor: ViewHandle<CodeEditorView>,
        diff_type: Option<DiffType>,
        display_mode: Option<DisplayMode>,
        file_path: Option<StandardizedPath>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        #[cfg(not(target_family = "wasm"))]
        let is_new_file = matches!(diff_type, Some(DiffType::Create { .. }));

        ctx.subscribe_to_view(&editor, |me, _view, event, ctx| match event {
            CodeEditorEvent::DiffUpdated => {
                ctx.emit(InlineDiffViewEvent::DiffStatusUpdated);
            }
            CodeEditorEvent::UnifiedDiffComputed(diff) => {
                ctx.emit(InlineDiffViewEvent::DiffAccepted { diff: diff.clone() });
            }
            CodeEditorEvent::ContentChanged { origin } => {
                if origin.from_user() && !me.was_edited {
                    me.was_edited = true;
                    ctx.emit(InlineDiffViewEvent::UserEdited);
                }
            }
            _ => {}
        });

        let model = Self {
            editor,
            diff_type,
            file_path,
            was_edited: false,
            backing_file_id: None,
            #[cfg(not(target_family = "wasm"))]
            session_type: None,
            #[cfg(not(target_family = "wasm"))]
            is_new_file,
            #[cfg(not(target_family = "wasm"))]
            accepted_action: RefCell::new(None),
            #[cfg(not(target_family = "wasm"))]
            revert_dispatched: false,
            #[cfg(not(target_family = "wasm"))]
            suppress_next_backing_file_event: Cell::new(false),
            #[cfg(not(target_family = "wasm"))]
            original_content: None,
        };

        model.apply_diffs_if_any(ctx);
        if let Some(display_mode) = display_mode {
            model.set_display_mode(display_mode, ctx);
        }

        model
    }

    /// Register a file with `FileModel` for save support.
    ///
    /// The `session_type` determines whether the file is local or remote.
    /// For `Local`, the file is registered by path on the local filesystem.
    /// For `Remote`, the file is registered against the remote backend so
    /// that `save()` / `delete()` dispatch over the wire via
    /// `RemoteServerClient`.
    ///
    /// This must be called after construction for non-WASM environments.
    #[cfg(not(target_family = "wasm"))]
    pub fn register_file(&mut self, session_type: &DiffSessionType, ctx: &mut ViewContext<Self>) {
        let Some(file_path) = &self.file_path else {
            return;
        };

        let file_model = FileModel::handle(ctx);
        let file_id = match session_type {
            DiffSessionType::Local => {
                let Some(local_path) = file_path.to_local_path() else {
                    log::error!(
                        "Failed to convert StandardizedPath to local path: {file_path}; \
                         diff will be read-only",
                    );
                    return;
                };
                // `subscribe_to_updates` stays `false`, matching the pin. Turning it
                // on would register a watcher (a whole repository subscription, per
                // diff view) and start emitting `FileModelEvent::FileUpdated`, which
                // the subscription in `finish_file_registration` does not handle and
                // which nothing in this read-only-until-accepted view could usefully
                // act on — it would change the editor's live behaviour to no benefit.
                //
                // It is also not what makes the accept path safe. A watcher is
                // advisory and debounced (200ms in `BulkFilesystemWatcher`), so a
                // change landing between the last event and the write would still be
                // missed. `save_content` instead checks the file's actual contents
                // against the pre-image the diff was computed from, at write time, in
                // the same task as the write. That check is authoritative and does
                // not depend on any subscription being live.
                file_model.update(ctx, |file_model, ctx| {
                    file_model.register_file_path(&local_path, false, ctx)
                })
            }
            DiffSessionType::Remote(host_id) => {
                let host_id = host_id.clone();
                let remote_path = file_path.clone();
                file_model.update(ctx, |file_model, _ctx| {
                    file_model.register_remote_file(host_id, remote_path)
                })
            }
        };

        self.session_type = Some(session_type.clone());
        self.finish_file_registration(file_id, ctx);
    }

    /// Whether [`Self::restore_diff_base`] could write anything: a file is
    /// registered and the accept recorded what it did there.
    pub fn can_revert(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        {
            self.backing_file_id.is_some() && self.accepted_action.borrow().is_some()
        }
        #[cfg(target_family = "wasm")]
        {
            false
        }
    }

    /// Records the file's raw text as the edit was proposed against it (the
    /// diff base before LF normalisation): what a revert restores.
    #[cfg(not(target_family = "wasm"))]
    pub fn set_original_content(&mut self, content: String) {
        self.original_content = Some(content);
    }

    /// Common registration logic: subscribes to events and sets the
    /// backing file ID after a file has been registered with `FileModel`.
    #[cfg(not(target_family = "wasm"))]
    fn finish_file_registration(&mut self, file_id: FileId, ctx: &mut ViewContext<Self>) {
        let file_model = FileModel::handle(ctx);

        let version = self.editor.as_ref(ctx).version(ctx);
        file_model.update(ctx, |file_model, _ctx| {
            file_model.set_version(file_id, version);
        });

        self.backing_file_id = Some(file_id);

        // Subscribe to FileModel events for this file.
        ctx.subscribe_to_model(&file_model, move |me, _file_model, event, ctx| {
            if file_id == event.file_id() {
                match event {
                    // A rename-revert dispatches its first write (the restore)
                    // at this same `file_id` and awaits its outcome itself, via
                    // `finish_rename_revert` — see
                    // `suppress_next_backing_file_event`. That first outcome is
                    // not the revert's outcome, so it must not be forwarded
                    // here.
                    FileModelEvent::FileSaved { .. }
                        if me.suppress_next_backing_file_event.take() => {}
                    FileModelEvent::FailedToSave { .. }
                        if me.suppress_next_backing_file_event.take() => {}
                    FileModelEvent::FileSaved { .. } => {
                        ctx.emit(InlineDiffViewEvent::FileSaved);
                    }
                    FileModelEvent::FailedToSave { error, .. } => {
                        let error = if me.revert_dispatched {
                            Rc::new(revert_failure(error))
                        } else {
                            error.clone()
                        };
                        ctx.emit(InlineDiffViewEvent::FailedToSave { error });
                    }
                    _ => {}
                }
            }
        });

        ctx.emit(InlineDiffViewEvent::FileLoaded);
    }

    fn apply_diffs_if_any(&self, ctx: &mut ViewContext<Self>) {
        let Some(diff) = self.diff_type.clone() else {
            return;
        };

        let deltas = match diff {
            DiffType::Create { delta } => vec![delta],
            DiffType::Update { mut deltas, .. } => {
                deltas.sort_by_key(|delta| delta.replacement_line_range.start);
                deltas
            }
            DiffType::Delete { delta } => vec![delta],
        };

        if deltas.is_empty() {
            return;
        }

        self.editor.update(ctx, |editor, ctx| {
            editor.apply_diffs(deltas, ctx);
            editor.toggle_diff_nav(None, ctx);
            editor.set_pending_scroll(ScrollTrigger::new(
                ScrollPosition::FocusedDiffHunk,
                editor.buffer_version(ctx),
            ));
        });
    }

    /// The state this view requires the file to still be in before it will
    /// overwrite it — the pre-image the proposed diff was computed against.
    ///
    /// Reads the diff base out of the editor and hands the actual decision to
    /// [`pre_image_for_diff`], which needs no `AppContext` and is therefore the
    /// part that can be tested. What is left here is the plumbing: two handle
    /// dereferences and a clone.
    #[cfg(not(target_family = "wasm"))]
    fn expected_disk_state(&self, ctx: &AppContext) -> Result<ExpectedDiskState, String> {
        // The diff base is the file's text as it was read when the edit was
        // proposed — LF-normalised by `CodeEditorModel::set_base`, which is why
        // `FileModel` compares normalised on both sides. Not read at all for a
        // creation, which has no base and asserts absence instead.
        let base = if self.is_new_file {
            None
        } else {
            self.editor
                .as_ref(ctx)
                .model
                .as_ref(ctx)
                .diff()
                .as_ref(ctx)
                .base()
                .map(|base| base.to_string())
        };

        pre_image_for_diff(self.is_new_file, base, self.file_path.as_ref())
    }

    /// The write [`Self::save_content`] actually performs, given this diff's
    /// `DiffType` and the backend the file was registered against.
    ///
    /// The GUI counterpart of `warp_tui::tui_diff_storage::PersistAction::resolve`,
    /// with the same fallback: a remote session has no rename primitive, so a
    /// remote rename resolves to an in-place [`FileWriteAction::Write`] rather
    /// than a move — and callers that report what happened (not just what the
    /// diff describes) must ask this, not [`DiffViewer::diff`], to get the
    /// remote case right.
    #[cfg(not(target_family = "wasm"))]
    pub fn write_action(&self) -> FileWriteAction {
        let is_remote = matches!(self.session_type, Some(DiffSessionType::Remote(_)));
        resolve_write_action(self.diff_type.as_ref(), is_remote, self.file_path.as_ref())
    }

    /// WASM never registers a backing file, so nothing is ever written; kept
    /// only so callers outside this module do not need to `cfg`-gate the call.
    #[cfg(target_family = "wasm")]
    pub fn write_action(&self) -> FileWriteAction {
        resolve_write_action(self.diff_type.as_ref(), false, self.file_path.as_ref())
    }

    #[cfg(not(target_family = "wasm"))]
    fn save_content(&self, ctx: &mut ViewContext<Self>) {
        let Some(file_id) = self.backing_file_id else {
            return;
        };
        let version = self.editor.as_ref(ctx).version(ctx);

        // The buffer being written is a snapshot the agent produced, possibly
        // minutes ago, plus whatever the user typed into this view. Anything
        // that touched the file in between — another editor, a formatter, a
        // rebase — is not in it. Write only if the file still holds the text the
        // diff was computed from; otherwise report and write nothing.
        //
        // # Divergence from the pinned oracle
        //
        // This is **not** a parity port. Pinned Warp `4111d08f9` writes here
        // unconditionally (`4111d08f9:app/src/code/inline_diff.rs:219-236` calls
        // `FileModel::save` with the whole buffer regardless of `DiffType`, so a
        // V4A rename is written to the ORIGINAL path and a delete truncates the
        // file instead of removing it), so accepting an edit silently discards
        // every concurrent external change AND ignores what the diff actually
        // asked for. The oracle shares both defects and we are fixing them ahead
        // of the oracle deliberately, because the loss is unrecoverable and the
        // user (and the model, via `try_emit_diffs_saved`) is never told. A
        // re-pin must not "restore parity" by reverting this to `FileModel::save`
        // or by collapsing delete/rename back into a plain write.
        match self.write_action() {
            FileWriteAction::Delete => self.dispatch_accept_delete(file_id, version, ctx),
            FileWriteAction::Rename(to) => self.dispatch_accept_rename(file_id, to, version, ctx),
            FileWriteAction::Write => self.dispatch_accept_write(file_id, version, ctx),
        }
    }

    /// The ordinary case: overwrite the registered path in place (a creation
    /// or an in-place update).
    #[cfg(not(target_family = "wasm"))]
    fn dispatch_accept_write(
        &self,
        file_id: FileId,
        version: ContentVersion,
        ctx: &mut ViewContext<Self>,
    ) {
        let content = self.editor.as_ref(ctx).text(ctx).into_string();
        let expected = match self.expected_disk_state(ctx) {
            Ok(expected) => expected,
            Err(message) => {
                ctx.emit(InlineDiffViewEvent::FailedToSave {
                    error: Rc::new(FileSaveError::Other(message)),
                });
                return;
            }
        };

        // Recorded before the write is dispatched, and whether or not it lands:
        // it is what a later revert requires the file to still hold. If this
        // accept is refused, the file never held it, and a revert guarded by it
        // is refused in turn — which is right, because there is nothing of the
        // accept's to undo, and the unguarded revert this replaced would have
        // written the base over (or deleted) whatever *was* there.
        *self.accepted_action.borrow_mut() = Some(AcceptedAction::Wrote {
            content: content.clone(),
        });

        if let Err(err) = FileModel::handle(ctx).update(ctx, |file_model, ctx| {
            file_model.save_if_unchanged(file_id, content, expected, version, ctx)
        }) {
            ctx.emit(InlineDiffViewEvent::FailedToSave {
                error: Rc::new(err),
            });
        }
    }

    /// A `DiffType::Delete`: remove the file, guarded against the content the
    /// diff was based on (the same pre-image an in-place write would assert).
    #[cfg(not(target_family = "wasm"))]
    fn dispatch_accept_delete(
        &self,
        file_id: FileId,
        version: ContentVersion,
        ctx: &mut ViewContext<Self>,
    ) {
        let expected = match self.expected_disk_state(ctx) {
            Ok(expected) => expected,
            Err(message) => {
                ctx.emit(InlineDiffViewEvent::FailedToSave {
                    error: Rc::new(FileSaveError::Other(message)),
                });
                return;
            }
        };

        *self.accepted_action.borrow_mut() = Some(AcceptedAction::Deleted);

        if let Err(err) = FileModel::handle(ctx).update(ctx, |file_model, ctx| {
            file_model.delete_if_unchanged(file_id, expected, version, ctx)
        }) {
            ctx.emit(InlineDiffViewEvent::FailedToSave {
                error: Rc::new(err),
            });
        }
    }

    /// A local `DiffType::Update { rename: Some(to), .. }`: write the final
    /// content at `to` and remove it from the registered path, both in one
    /// guarded operation. The destination's pre-image is always `Absent`: a
    /// proposed rename is only ever offered when the destination did not
    /// exist (a rename onto an existing file is rewritten upstream into a
    /// deletion + update — see `diff_application::apply_v4a_update` — which
    /// never reaches this arm). Mirrors `PersistAction::Rename` in
    /// `tui_diff_storage`.
    #[cfg(not(target_family = "wasm"))]
    fn dispatch_accept_rename(
        &self,
        file_id: FileId,
        to: PathBuf,
        version: ContentVersion,
        ctx: &mut ViewContext<Self>,
    ) {
        let content = self.editor.as_ref(ctx).text(ctx).into_string();
        let expected = match self.expected_disk_state(ctx) {
            Ok(expected) => expected,
            Err(message) => {
                ctx.emit(InlineDiffViewEvent::FailedToSave {
                    error: Rc::new(FileSaveError::Other(message)),
                });
                return;
            }
        };

        *self.accepted_action.borrow_mut() = Some(AcceptedAction::Renamed {
            to: to.clone(),
            content: content.clone(),
        });

        if let Err(err) = FileModel::handle(ctx).update(ctx, |file_model, ctx| {
            file_model.rename_and_save_if_unchanged(
                file_id,
                to,
                content,
                expected,
                ExpectedDiskState::Absent,
                version,
                ctx,
            )
        }) {
            ctx.emit(InlineDiffViewEvent::FailedToSave {
                error: Rc::new(err),
            });
        }
    }
}

/// The write [`InlineDiffView::save_content`] actually performs for a diff —
/// not necessarily what its raw `DiffType` alone would suggest, since a
/// remote session has no rename primitive. See [`InlineDiffView::write_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileWriteAction {
    /// Write the final content at the file's registered path.
    Write,
    /// Move the file to the new path and write the final content there.
    Rename(PathBuf),
    /// Delete the file.
    Delete,
}

/// Decides the write [`InlineDiffView::write_action`] reports, given the
/// diff's `DiffType`, whether the session is remote, and the file's
/// registered path. Needs no `AppContext`, so it is the part that can be
/// tested directly; `write_action` is the plumbing around it.
fn resolve_write_action(
    diff_type: Option<&DiffType>,
    is_remote: bool,
    file_path: Option<&StandardizedPath>,
) -> FileWriteAction {
    match diff_type {
        Some(DiffType::Delete { .. }) => FileWriteAction::Delete,
        Some(DiffType::Update {
            rename: Some(to), ..
        }) if !is_remote => {
            let current = file_path.map(ToString::to_string);
            let target = to.to_string_lossy();
            if current.as_deref() == Some(target.as_ref()) {
                FileWriteAction::Write
            } else {
                FileWriteAction::Rename(to.clone())
            }
        }
        _ => FileWriteAction::Write,
    }
}

/// Decides the pre-image a guarded accept asserts, given what the view could
/// find.
///
/// `Err` means the pre-image could not be established. That is *not* the same as
/// "nothing to compare, go ahead": a buffer with no diff base is a buffer whose
/// relationship to the file on disk is unknown, and writing it would be the very
/// overwrite this guard exists to prevent. The caller surfaces the message
/// instead of writing.
#[cfg(not(target_family = "wasm"))]
fn pre_image_for_diff(
    is_new_file: bool,
    base: Option<String>,
    file_path: Option<&StandardizedPath>,
) -> Result<ExpectedDiskState, String> {
    if is_new_file {
        // A creation diff is only offered after the file was found absent
        // (`apply_create_file` rejects the edit outright if it already exists),
        // so "still absent" is the pre-image being asserted. The base is not
        // consulted: a creation has none, and demanding one would refuse every
        // file creation.
        return Ok(ExpectedDiskState::Absent);
    }

    let base = base.ok_or_else(|| {
        let path = file_path
            .map(ToString::to_string)
            .unwrap_or_else(|| "file".to_owned());
        format!(
            "{path} was not written: the original contents this edit was based on \
             are no longer available, so there is no way to tell whether the file \
             changed in the meantime. Nothing was changed."
        )
    })?;

    Ok(ExpectedDiskState::Content(base))
}

impl InlineDiffView {
    pub fn file_path(&self) -> Option<&StandardizedPath> {
        self.file_path.as_ref()
    }

    pub fn file_name(&self) -> Option<String> {
        self.file_path()
            .map(|p| p.file_name().unwrap_or_default().to_owned())
    }
}

impl DiffViewer for InlineDiffView {
    fn editor(&self) -> &ViewHandle<CodeEditorView> {
        &self.editor
    }

    fn diff(&self) -> Option<&DiffType> {
        self.diff_type.as_ref()
    }

    fn was_edited(&self) -> bool {
        self.was_edited
    }

    fn set_display_mode(&self, mode: DisplayMode, ctx: &mut ViewContext<Self>) {
        let is_delete = matches!(self.diff(), Some(DiffType::Delete { .. }));
        let interaction_state = if self.backing_file_id.is_some() {
            mode.interaction_state(is_delete)
        } else {
            // No file registered (e.g. WASM or restored conversations): always read-only.
            InteractionState::Selectable
        };
        self.editor().update(ctx, |editor, ctx| {
            editor.set_scroll_wheel_behavior(mode.scroll_wheel_behavior());
            editor.set_vertical_expansion_behavior(mode.vertical_expansion_behavior(), ctx);
            editor.set_vertical_scrollbar_appearance(mode.scrollbar_appearance());
            editor.set_horizontal_scrollbar_appearance(mode.scrollbar_appearance());
            editor.set_interaction_state(interaction_state, ctx);
            editor.set_show_nav_bar(mode.show_nav_bar());
            editor.set_nav_bar_behavior(NavBarBehavior::NotClosable, ctx);
        });
    }

    fn accept_and_save_diff(&self, ctx: &mut ViewContext<Self>) {
        // No-op when no file is registered (WASM / restored conversations).
        if self.backing_file_id.is_none() {
            return;
        }

        // Compute the unified diff (result arrives via CodeEditorEvent::UnifiedDiffComputed).
        if let Some(file_path) = &self.file_path {
            let file_name = file_path.to_string();
            self.editor.update(ctx, |editor, ctx| {
                editor.retrieve_unified_diff(file_name, ctx)
            });
        }
        // Save the current editor content to disk.
        #[cfg(not(target_family = "wasm"))]
        self.save_content(ctx);
    }
}

/// What [`InlineDiffView::restore_diff_base`] did. A revert is not done when
/// this returns: callers must not record one until its outcome arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub enum RevertDispatch {
    /// A guarded write is in flight. Its outcome arrives later, exactly once, as
    /// [`InlineDiffViewEvent::FileSaved`] (the file was reverted) or
    /// [`InlineDiffViewEvent::FailedToSave`] (the write was refused or failed,
    /// and the file was left alone).
    WriteInFlight,
    /// No file is registered (WASM, restored conversations): nothing was
    /// written, no event will follow, and the file was **not** reverted.
    NoBackingFile,
}

impl InlineDiffView {
    /// Undoes this view's accepted diff on disk: restores the diff base, or
    /// deletes a file the accept created — guarded, so a file that changed after
    /// the accept is left alone.
    ///
    /// An inherent method rather than a `DiffViewer` member: this is the only
    /// revert there is. The trait used to declare one with an `Ok(())` default,
    /// and `LocalCodeEditorView` implemented it with an unguarded
    /// `std::fs::remove_file` / `GlobalBufferModel::save`; neither had a caller
    /// (nor at pin `4111d08f9`), and both are gone so that no unguarded or
    /// silently-succeeding revert can be reached by a future caller (#684).
    ///
    /// `Err` is a refusal decided before anything was dispatched, with a
    /// user-facing message; nothing was written and no event will follow.
    pub fn restore_diff_base(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) -> Result<RevertDispatch, String> {
        let Some(file_id) = self.backing_file_id else {
            return Ok(RevertDispatch::NoBackingFile);
        };
        self.dispatch_guarded_revert(file_id, ctx)
    }

    /// Nothing is ever registered on WASM, so `restore_diff_base` never gets
    /// here; this only keeps the signature total.
    #[cfg(target_family = "wasm")]
    fn dispatch_guarded_revert(
        &mut self,
        _file_id: FileId,
        _ctx: &mut ViewContext<Self>,
    ) -> Result<RevertDispatch, String> {
        Ok(RevertDispatch::NoBackingFile)
    }

    #[cfg(not(target_family = "wasm"))]
    fn dispatch_guarded_revert(
        &mut self,
        file_id: FileId,
        ctx: &mut ViewContext<Self>,
    ) -> Result<RevertDispatch, String> {
        // Guarded, like the accept (`save_content`), and against the right
        // pre-image: not the diff base — that is what the revert puts back —
        // but what the accept actually did, recorded in `accepted_action`.
        // See `revert_plan` for what each case asserts.
        //
        // # Divergence from the pinned oracle
        //
        // Not a parity port. Pinned Warp `4111d08f9` reverts with the
        // unconditional `FileModel::save` / `FileModel::delete`, so a revert
        // destroys every edit made to the file after the accept, and deletes
        // an agent-created file the user has since built on, with no check
        // and no message. A re-pin must not "restore parity" here.
        //
        // The base written back is the raw original text, not the editor's
        // LF-normalised diff base; see `original_content`.
        let base = if self.is_new_file {
            None
        } else {
            self.original_content.clone()
        };
        let plan = revert_plan(
            self.is_new_file,
            self.accepted_action.borrow().clone(),
            base,
            self.file_path.as_ref(),
        )?;

        let version = self.editor.as_ref(ctx).version(ctx);
        match plan {
            RevertPlan::Single(write) => {
                FileModel::handle(ctx)
                    .update(ctx, |file_model, ctx| {
                        dispatch_revert_write(file_model, file_id, write, version, ctx)
                    })
                    .map_err(|error| revert_failure(&error).to_string())?;
                // Only once a write is actually in flight: its refusal, if
                // any, arrives later as `FileModelEvent::FailedToSave` and is
                // reported as a failed revert from there.
                self.revert_dispatched = true;
                Ok(RevertDispatch::WriteInFlight)
            }
            RevertPlan::UndoRename {
                restore,
                at,
                remove_expected,
            } => {
                // Undoing a rename is two guarded writes: restore the
                // original path, then — only if that lands — remove
                // whatever the accept left at the destination. Registering
                // the completion waiter before dispatch, exactly as
                // `tui_diff_storage::dispatch_write` does, so the write
                // cannot resolve before something is listening for it.
                let restore_completion = FileModel::handle(ctx).update(ctx, |file_model, ctx| {
                    let completion = file_model.save_completion(file_id);
                    dispatch_revert_write(file_model, file_id, restore, version, ctx)?;
                    Ok::<_, FileSaveError>(completion)
                });
                let completion =
                    restore_completion.map_err(|error| revert_failure(&error).to_string())?;
                // The ordinary per-file subscription (`finish_file_registration`)
                // would otherwise report this first write's outcome as the
                // whole revert; suppress that one occurrence and let
                // `finish_rename_revert` emit the real, combined outcome.
                self.suppress_next_backing_file_event.set(true);
                self.revert_dispatched = true;
                ctx.spawn(completion, move |me, restore_result, ctx| {
                    me.finish_rename_revert(restore_result, at, remove_expected, ctx);
                });
                Ok(RevertDispatch::WriteInFlight)
            }
            RevertPlan::FinishRename { at, remove_expected } => {
                // A previous attempt's first step already landed -- the
                // registered path (`file_id`) already holds the original
                // text -- so nothing is dispatched against it here, and
                // `suppress_next_backing_file_event` does not apply: no event
                // for `file_id` is coming. Reuse `finish_rename_revert`'s own
                // second-step dispatch by handing it the already-known-good
                // outcome of a step it does not need to repeat.
                self.revert_dispatched = true;
                self.finish_rename_revert(Ok(()), at, remove_expected, ctx);
                Ok(RevertDispatch::WriteInFlight)
            }
        }
    }

    /// The second half of undoing an accepted rename: called once the guarded
    /// restore at the registered path has resolved. If it landed, removes
    /// whatever the accept left at the rename destination — a fresh,
    /// transient `FileModel` registration, since the destination was never
    /// this view's `backing_file_id` — and reports the combined outcome. If
    /// the restore was refused or failed, nothing else is attempted: the file
    /// is left exactly where the accept put it, and the refusal is reported
    /// as-is.
    ///
    /// # The partial outcome (step 1 landed, step 2 does not)
    ///
    /// Once the restore above has landed there are, unconditionally, now two
    /// copies of the file: the original at the registered path and whatever
    /// the accept left at `at`. `accepted_action` is updated to
    /// [`AcceptedAction::PartiallyRevertedRename`] *before* step 2 is even
    /// dispatched, not just on its refusal, because that is the moment this
    /// stops being a plain, still-intact `Renamed` accept: a retry from here
    /// must never repeat step 1 (its `Absent` guard would refuse against the
    /// state step 1 itself just established — the exact "retrying is dead"
    /// failure mode this exists to avoid), only ever attempt step 2 again.
    /// If step 2 then lands, this recorded state never matters again — the
    /// card moves straight to `Reverted` and consults `accepted_action` no
    /// further.
    ///
    /// If step 2 is refused or fails, the toast must say so honestly: not
    /// step 2's guard message alone (which, read on its own, sounds like
    /// *nothing* happened, when in fact the restore already did) but that the
    /// original was restored *and* the destination could not be removed, so
    /// both files now exist.
    #[cfg(not(target_family = "wasm"))]
    fn finish_rename_revert(
        &mut self,
        restore_result: Result<(), Arc<FileSaveError>>,
        at: PathBuf,
        remove_expected: ExpectedDiskState,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Err(error) = restore_result {
            ctx.emit(InlineDiffViewEvent::FailedToSave {
                error: Rc::new(revert_failure(&error)),
            });
            return;
        }

        let content = match &remove_expected {
            ExpectedDiskState::Content(content) => content.clone(),
            // `revert_plan` never builds a rename revert whose `remove_expected`
            // is anything else; kept total (rather than `unreachable!`) so a
            // future change to `revert_plan` fails safe here -- no attempt to
            // remove `at` at all -- instead of panicking mid-revert.
            ExpectedDiskState::Absent => {
                log::error!("a rename revert's destination guard was not a content guard");
                String::new()
            }
        };
        *self.accepted_action.borrow_mut() = Some(AcceptedAction::PartiallyRevertedRename {
            to: at.clone(),
            content,
        });

        let original_path = self
            .file_path
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "the file".to_owned());

        let version = self.editor.as_ref(ctx).version(ctx);
        let dispatched = FileModel::handle(ctx).update(ctx, |file_model, ctx| {
            let dest_id = file_model.register_file_path(&at, false, ctx);
            let completion = file_model.save_completion(dest_id);
            let result = dispatch_revert_write(
                file_model,
                dest_id,
                RevertWrite::Delete {
                    expected: remove_expected,
                },
                version,
                ctx,
            );
            // Transient, like `tui_diff_storage::dispatch_write`'s
            // registration of the rename target: this view never needs a
            // standing subscription on the destination.
            file_model.unsubscribe(dest_id, ctx);
            result.map(|()| completion)
        });

        match dispatched {
            Ok(completion) => {
                ctx.spawn(completion, move |_me, delete_result, ctx| match delete_result {
                    Ok(()) => ctx.emit(InlineDiffViewEvent::FileSaved),
                    Err(error) => ctx.emit(InlineDiffViewEvent::FailedToSave {
                        error: Rc::new(partial_rename_revert_failure(
                            &original_path,
                            &at,
                            &error,
                        )),
                    }),
                });
            }
            Err(error) => {
                ctx.emit(InlineDiffViewEvent::FailedToSave {
                    error: Rc::new(partial_rename_revert_failure(&original_path, &at, &error)),
                });
            }
        }
    }
}

/// The single guarded write that undoes an accepted diff at one path.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, PartialEq, Eq)]
enum RevertWrite {
    /// The accept created the file (or moved a file away from here); remove
    /// it, but only if it still holds what the accept left.
    Delete { expected: ExpectedDiskState },
    /// The accept overwrote the file, or deleted it, or moved it away; put
    /// `content` back, but only if the pre-image still holds.
    Restore {
        content: String,
        expected: ExpectedDiskState,
    },
}

/// The write(s) that undo an accept, in the order they must be applied.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, PartialEq, Eq)]
enum RevertPlan {
    /// One guarded write at the registered path: undoes a creation, an
    /// in-place update, or re-creates a deleted file.
    Single(RevertWrite),
    /// Two guarded writes: `restore` at the registered path first, and —
    /// only if that lands — remove whatever the accept left at `at`.
    UndoRename {
        restore: RevertWrite,
        at: PathBuf,
        remove_expected: ExpectedDiskState,
    },
    /// A retry of a rename revert whose first step already landed on a
    /// previous attempt: the registered path already holds the original
    /// text, so only `at` — not the registered path — needs a guarded write.
    /// Unlike [`Self::Single`], this does not touch `file_id` at all; see
    /// [`InlineDiffView::dispatch_guarded_revert`].
    FinishRename {
        at: PathBuf,
        remove_expected: ExpectedDiskState,
    },
}

/// Decides the write(s) that undo an accept, and the pre-image(s) they assert.
///
/// The GUI counterpart of `warp_tui::tui_diff_storage::revert_plan`, with the
/// same semantics: a revert's pre-image is what the accept left on disk, so
///
/// * a **creation** is undone by a delete that requires the file to still hold
///   the accepted text — an agent-created file the user has since edited is
///   not deleted;
/// * an **in-place edit** is undone by writing the diff base back, which
///   requires the file to still hold the accepted text — edits made after the
///   accept are not overwritten;
/// * a **delete** is undone by re-creating the file with the original text,
///   requiring the path to still be free — a file or symlink someone has since
///   put there is left alone;
/// * a **rename** is undone by restoring the original text at the registered
///   path (requiring it to still be free) and then removing the file at the
///   destination (requiring it to still hold the accepted text) — two steps,
///   two different pre-images, applied in that order so that a refused restore
///   never triggers the destination's removal (see `dispatch_guarded_revert`);
/// * a **partially-reverted rename** — the first of those two steps landed on
///   an earlier attempt, the second did not — is undone by *only* the second
///   step: retrying the first would guard on the destination's absence, which
///   the first step's own success has since falsified (see
///   `AcceptedAction::PartiallyRevertedRename`).
///
/// Unlike the TUI, the accepted text cannot be re-derived from the diff: this
/// view is editable, and the user may have changed the agent's proposal before
/// accepting it. So the accept records what it did (`accepted_action`) and
/// that is what arrives here.
///
/// A formatter or anything else that touched the file after the accept
/// therefore refuses the revert. That is the intended trade, and the TUI's:
/// the common case (nothing touched the file) still passes. Restored text must
/// be the file's *raw* original text: `FileModel` compares the pre-image
/// line-ending-normalised, but refuses a write whose line endings differ from
/// the file's (`LineEndingsChanged`), so an LF-normalised base written over a
/// CRLF file is refused — which is what reverting with the editor's diff base
/// did to every CRLF file.
///
/// `Err` is a refusal with a user-facing message; nothing is written.
#[cfg(not(target_family = "wasm"))]
fn revert_plan(
    is_new_file: bool,
    accepted: Option<AcceptedAction>,
    original: Option<String>,
    file_path: Option<&StandardizedPath>,
) -> Result<RevertPlan, String> {
    let path = || {
        file_path
            .map(ToString::to_string)
            .unwrap_or_else(|| "file".to_owned())
    };

    // No record of an accept means no way to tell what the file should hold.
    // Reverting blind is exactly the overwrite this guard exists to prevent.
    let accepted = accepted.ok_or_else(|| {
        format!(
            "{} was not reverted: there is no record of what accepting this edit \
             did, so there is no way to tell whether the file changed since. \
             Nothing was changed.",
            path()
        )
    })?;

    let missing_original = || {
        format!(
            "{} was not reverted: the original contents this edit replaced are no \
             longer available. Nothing was changed.",
            path()
        )
    };

    match accepted {
        AcceptedAction::Renamed { to, content } => {
            let original = original.ok_or_else(missing_original)?;
            Ok(RevertPlan::UndoRename {
                restore: RevertWrite::Restore {
                    content: original,
                    expected: ExpectedDiskState::Absent,
                },
                at: to,
                remove_expected: ExpectedDiskState::Content(content),
            })
        }
        // The registered path already holds the original text -- a previous
        // attempt's first step landed, only its second (removing `to`) did
        // not. Retrying the first step here would guard on `Absent`, which no
        // longer holds now that the first step put the original text back;
        // only `to` remains to be dealt with. Does not consult `original`: a
        // retry from this state needs none, which is exactly why this must be
        // its own `AcceptedAction` rather than reusing `Renamed`.
        AcceptedAction::PartiallyRevertedRename { to, content } => Ok(RevertPlan::FinishRename {
            at: to,
            remove_expected: ExpectedDiskState::Content(content),
        }),
        AcceptedAction::Deleted => {
            let original = original.ok_or_else(missing_original)?;
            Ok(RevertPlan::Single(RevertWrite::Restore {
                content: original,
                expected: ExpectedDiskState::Absent,
            }))
        }
        AcceptedAction::Wrote { content } => {
            if is_new_file {
                return Ok(RevertPlan::Single(RevertWrite::Delete {
                    expected: ExpectedDiskState::Content(content),
                }));
            }
            let original = original.ok_or_else(missing_original)?;
            Ok(RevertPlan::Single(RevertWrite::Restore {
                content: original,
                expected: ExpectedDiskState::Content(content),
            }))
        }
    }
}

/// Dispatches `write` through `FileModel`'s guarded operations. Its outcome —
/// including a refusal because the file changed — arrives asynchronously as
/// `FileModelEvent::FileSaved` / `FileModelEvent::FailedToSave`.
#[cfg(not(target_family = "wasm"))]
fn dispatch_revert_write(
    file_model: &mut FileModel,
    file_id: FileId,
    write: RevertWrite,
    version: ContentVersion,
    ctx: &mut ModelContext<FileModel>,
) -> Result<(), FileSaveError> {
    match write {
        RevertWrite::Delete { expected } => {
            file_model.delete_if_unchanged(file_id, expected, version, ctx)
        }
        RevertWrite::Restore { content, expected } => {
            file_model.save_if_unchanged(file_id, content, expected, version, ctx)
        }
    }
}

/// The plain-English reason inside a write failure, with no wrapping sentence
/// of its own: [`FileSaveError::Other`]'s message as-is (already a full
/// sentence naming the file), `path: io_error` for an
/// [`FileSaveError::IOError`] (whose own `Display` is a constant that drops
/// both), or the error's own text otherwise. Shared by [`revert_failure`] and
/// [`partial_rename_revert_failure`], which each wrap it in a different
/// sentence.
#[cfg(not(target_family = "wasm"))]
fn failure_reason(error: &FileSaveError) -> String {
    match error {
        FileSaveError::Other(message) => message.clone(),
        // `IOError`'s own `Display` is a constant; the `io::Error` is the reason.
        FileSaveError::IOError { error, path } => format!("{}: {error}", path.display()),
        other => other.to_string(),
    }
}

/// Rewrites a write failure as a failed *revert*, so the toast does not read
/// as a failed accept.
///
/// A guarded refusal (`FileSaveError::Other`) is a full sentence that already
/// names the file and says it was left alone, so it is prefixed rather than
/// wrapped in a second path. Anything else keeps its reason.
#[cfg(not(target_family = "wasm"))]
fn revert_failure(error: &FileSaveError) -> FileSaveError {
    FileSaveError::Other(format!(
        "Did not revert the agent's edit. {}",
        failure_reason(error)
    ))
}

/// The honest outcome of a rename revert whose first step (restoring the
/// original at `original_path`) landed but whose second (removing whatever
/// the accept left at `destination`) did not: unlike [`revert_failure`], this
/// must not read as "nothing happened" -- something did, the file at
/// `original_path` is back, and `destination` is a second copy left behind
/// because it changed since the accept (or could not be removed for some
/// other reason `error` names). A retry only has `destination` left to deal
/// with; see [`AcceptedAction::PartiallyRevertedRename`].
#[cfg(not(target_family = "wasm"))]
fn partial_rename_revert_failure(
    original_path: &str,
    destination: &std::path::Path,
    error: &FileSaveError,
) -> FileSaveError {
    FileSaveError::Other(format!(
        "{original_path} was restored, but {dest} could not be removed and was left as-is: {}. \
         Both files now exist; revert again to retry removing {dest}.",
        failure_reason(error),
        dest = destination.display(),
    ))
}

impl Entity for InlineDiffView {
    type Event = InlineDiffViewEvent;
}

impl View for InlineDiffView {
    fn ui_name() -> &'static str {
        "InlineDiffView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        ChildView::new(&self.editor).finish()
    }
}

impl TypedActionView for InlineDiffView {
    type Action = ();
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    /// What is and is not covered here, stated rather than implied.
    ///
    /// [`pre_image_for_diff`], [`resolve_write_action`] and [`revert_plan`] are
    /// the whole of the accept and revert paths' decisions, and all three are
    /// covered below. What is *not* covered is the plumbing around them —
    /// `expected_disk_state`, `save_content`, `dispatch_guarded_revert` — which
    /// needs a live `CodeEditorView` inside an `App::test`, with the buffer
    /// reset, diffs applied and a base set: the fixture the app crate's view
    /// tests build, and more setup than the handful of handle dereferences it
    /// would be testing. The disk-level tests below instead push each decision
    /// through the *same* guarded `FileModel` calls that plumbing dispatches,
    /// against a real file — which is what actually matters: that the write
    /// lands, or refuses, exactly as the decision says.
    const BASE: &str = "fn main() {}\n";

    fn path() -> StandardizedPath {
        StandardizedPath::try_new("/tmp/example.rs").expect("standardized path")
    }

    // ── Accept pre-image: `pre_image_for_diff` ────────────────────────────

    /// A creation asserts absence and never consults the base. It has none, and
    /// demanding one would refuse every file creation.
    #[test]
    fn a_creation_asserts_the_file_is_still_absent() {
        assert_eq!(
            pre_image_for_diff(true, None, Some(&path())),
            Ok(ExpectedDiskState::Absent)
        );
        assert_eq!(
            pre_image_for_diff(true, Some(BASE.to_owned()), Some(&path())),
            Ok(ExpectedDiskState::Absent)
        );
    }

    /// An edit to an existing file asserts the file still holds the text the
    /// diff was computed from, verbatim.
    #[test]
    fn an_edit_asserts_the_diff_base() {
        assert_eq!(
            pre_image_for_diff(false, Some(BASE.to_owned()), Some(&path())),
            Ok(ExpectedDiskState::Content(BASE.to_owned()))
        );
    }

    /// The case that must not collapse into "nothing to compare, go ahead": no
    /// diff base means no way to tell whether the file changed, which is a
    /// refusal, not a licence to overwrite.
    #[test]
    fn a_missing_diff_base_refuses_rather_than_writing_blind() {
        let error = pre_image_for_diff(false, None, Some(&path()))
            .expect_err("a missing base must not produce a pre-image");
        assert!(
            error.contains("/tmp/example.rs"),
            "the message must name the file, got: {error}"
        );
        assert!(
            error.contains("Nothing was changed"),
            "the message must say the write did not happen, got: {error}"
        );
    }

    /// The message still reads when the view has no path either — a restored
    /// conversation, or a path that failed to standardize.
    #[test]
    fn a_missing_path_still_produces_a_readable_refusal() {
        let error = pre_image_for_diff(false, None, None)
            .expect_err("a missing base must not produce a pre-image");
        assert!(error.starts_with("file was not written"), "got: {error}");
    }

    // ── Accept dispatch: `resolve_write_action` (#688) ────────────────────
    //
    // The defect: the pin's `save_content` ignores `DiffType` entirely and
    // always saves the editor buffer at the registered path, so a V4A rename
    // never moves the file and a delete truncates it instead of removing it.
    // `resolve_write_action` is the decision that fixes that; it mirrors
    // `warp_tui::tui_diff_storage::PersistAction::resolve`.

    fn local_rename(to: &str) -> DiffType {
        DiffType::update(Vec::new(), Some(to.to_owned()))
    }

    #[test]
    fn a_delete_diff_always_resolves_to_delete() {
        let diff = DiffType::deletion(3);
        assert_eq!(
            resolve_write_action(Some(&diff), false, Some(&path())),
            FileWriteAction::Delete
        );
        // Session backend is irrelevant: delete has no remote-primitive gap.
        assert_eq!(
            resolve_write_action(Some(&diff), true, Some(&path())),
            FileWriteAction::Delete
        );
    }

    #[test]
    fn a_local_rename_resolves_to_rename() {
        let diff = local_rename("/tmp/new.rs");
        assert_eq!(
            resolve_write_action(Some(&diff), false, Some(&path())),
            FileWriteAction::Rename(PathBuf::from("/tmp/new.rs"))
        );
    }

    /// A remote session has no rename primitive: the write must fall back to
    /// an in-place write, exactly like `PersistAction::resolve` on the TUI
    /// side — otherwise the destination is asked for over a transport that
    /// cannot honor it.
    #[test]
    fn a_remote_rename_falls_back_to_an_in_place_write() {
        let diff = local_rename("/tmp/new.rs");
        assert_eq!(
            resolve_write_action(Some(&diff), true, Some(&path())),
            FileWriteAction::Write
        );
    }

    /// A "rename" to the same path the file is already registered at is just
    /// a write — there is nothing to move.
    #[test]
    fn a_rename_to_the_same_path_is_a_write() {
        let diff = local_rename("/tmp/example.rs");
        assert_eq!(
            resolve_write_action(Some(&diff), false, Some(&path())),
            FileWriteAction::Write
        );
    }

    #[test]
    fn an_in_place_update_and_a_creation_are_writes() {
        let update = DiffType::update(Vec::new(), None);
        let create = DiffType::creation(BASE.to_owned());
        assert_eq!(
            resolve_write_action(Some(&update), false, Some(&path())),
            FileWriteAction::Write
        );
        assert_eq!(
            resolve_write_action(Some(&create), false, Some(&path())),
            FileWriteAction::Write
        );
        assert_eq!(
            resolve_write_action(None, false, Some(&path())),
            FileWriteAction::Write
        );
    }

    // ── Accept dispatch, on a real file: delete and rename (#688) ─────────
    //
    // `dispatch_accept_delete`/`dispatch_accept_rename` are the two handle
    // dereferences and a clone around the exact `FileModel` calls exercised
    // here (`delete_if_unchanged` / `rename_and_save_if_unchanged`), so these
    // tests push the same guarded operations through a real file rather than
    // requiring the live `CodeEditorView` those methods also need.

    #[test]
    fn accepting_a_delete_removes_the_file() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("gone.rs");
            std::fs::write(&file, BASE).expect("write file");

            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&file, false, ctx)
            });
            let completion = files.update(&mut app, |files, _| files.save_completion(file_id));
            files
                .update(&mut app, |files, ctx| {
                    files.delete_if_unchanged(
                        file_id,
                        ExpectedDiskState::Content(BASE.to_owned()),
                        ContentVersion::new(),
                        ctx,
                    )
                })
                .expect("dispatch");
            completion.await.expect("the delete should land");

            assert!(!file.exists(), "the file must be removed, not truncated");
        });
    }

    #[test]
    fn accepting_a_delete_refuses_when_the_file_changed() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("edited.rs");
            std::fs::write(&file, "someone else's edit\n").expect("write file");

            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&file, false, ctx)
            });
            let completion = files.update(&mut app, |files, _| files.save_completion(file_id));
            files
                .update(&mut app, |files, ctx| {
                    files.delete_if_unchanged(
                        file_id,
                        ExpectedDiskState::Content(BASE.to_owned()),
                        ContentVersion::new(),
                        ctx,
                    )
                })
                .expect("dispatch");
            completion.await.expect_err("the delete must be refused");

            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                "someone else's edit\n",
                "a refused delete must leave the file exactly as it was"
            );
        });
    }

    #[test]
    fn accepting_a_rename_moves_and_writes_the_file() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let old_path = directory.path().join("old.rs");
            let new_path = directory.path().join("new.rs");
            std::fs::write(&old_path, BASE).expect("write file");

            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&old_path, false, ctx)
            });
            let completion = files.update(&mut app, |files, _| files.save_completion(file_id));
            files
                .update(&mut app, |files, ctx| {
                    files.rename_and_save_if_unchanged(
                        file_id,
                        new_path.clone(),
                        "fn main() { renamed(); }\n".to_owned(),
                        ExpectedDiskState::Content(BASE.to_owned()),
                        ExpectedDiskState::Absent,
                        ContentVersion::new(),
                        ctx,
                    )
                })
                .expect("dispatch");
            completion.await.expect("the rename should land");

            assert!(!old_path.exists(), "the source must no longer exist");
            assert_eq!(
                std::fs::read_to_string(&new_path).unwrap(),
                "fn main() { renamed(); }\n"
            );
        });
    }

    #[test]
    fn accepting_a_rename_refuses_when_the_destination_already_exists() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let old_path = directory.path().join("old.rs");
            let new_path = directory.path().join("new.rs");
            std::fs::write(&old_path, BASE).expect("write file");
            std::fs::write(&new_path, "already here\n").expect("write file");

            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&old_path, false, ctx)
            });
            let completion = files.update(&mut app, |files, _| files.save_completion(file_id));
            files
                .update(&mut app, |files, ctx| {
                    files.rename_and_save_if_unchanged(
                        file_id,
                        new_path.clone(),
                        "fn main() { renamed(); }\n".to_owned(),
                        ExpectedDiskState::Content(BASE.to_owned()),
                        ExpectedDiskState::Absent,
                        ContentVersion::new(),
                        ctx,
                    )
                })
                .expect("dispatch");
            completion.await.expect_err("the rename must be refused");

            assert!(old_path.exists(), "the source must be left in place");
            assert_eq!(std::fs::read_to_string(&old_path).unwrap(), BASE);
            assert_eq!(
                std::fs::read_to_string(&new_path).unwrap(),
                "already here\n",
                "the pre-existing destination must be untouched"
            );
        });
    }

    #[test]
    fn accepting_a_rename_refuses_when_the_source_changed() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let old_path = directory.path().join("old.rs");
            let new_path = directory.path().join("new.rs");
            std::fs::write(&old_path, "someone else's edit\n").expect("write file");

            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&old_path, false, ctx)
            });
            let completion = files.update(&mut app, |files, _| files.save_completion(file_id));
            files
                .update(&mut app, |files, ctx| {
                    files.rename_and_save_if_unchanged(
                        file_id,
                        new_path.clone(),
                        "fn main() { renamed(); }\n".to_owned(),
                        ExpectedDiskState::Content(BASE.to_owned()),
                        ExpectedDiskState::Absent,
                        ContentVersion::new(),
                        ctx,
                    )
                })
                .expect("dispatch");
            completion.await.expect_err("the rename must be refused");

            assert!(
                !new_path.exists(),
                "nothing must be written at the destination"
            );
            assert_eq!(
                std::fs::read_to_string(&old_path).unwrap(),
                "someone else's edit\n",
                "the user's edit must survive"
            );
        });
    }

    // ── Revert planning: `revert_plan` ─────────────────────────────────────
    //
    // `restore_diff_base` is two-or-three handle dereferences around
    // `revert_plan` and `dispatch_revert_write`/`finish_rename_revert`; the
    // same fixture limitation as above applies to it. The decisions are here,
    // and the disk tests below push each plan through the real `FileModel`
    // guarded writes against a real file.

    /// What the user accepted — the agent's proposal, possibly hand-edited.
    const ACCEPTED: &str = "fn main() {\n    println!(\"accepted\");\n}\n";
    /// What the user wrote to the file after accepting.
    const LATER_EDIT: &str = "fn main() {\n    println!(\"my own work\");\n}\n";

    /// Unwraps the single-write case, panicking with the plan otherwise — for
    /// tests that only care about the non-rename plans.
    fn single(plan: RevertPlan) -> RevertWrite {
        match plan {
            RevertPlan::Single(write) => write,
            other => panic!("expected a single-step plan, got {other:?}"),
        }
    }

    fn wrote(content: &str) -> Option<AcceptedAction> {
        Some(AcceptedAction::Wrote {
            content: content.to_owned(),
        })
    }

    /// The defect: a revert's pre-image is what the accept wrote, never the
    /// base it is about to put back.
    #[test]
    fn reverting_an_edit_restores_the_base_guarded_by_the_accepted_text() {
        assert_eq!(
            single(
                revert_plan(false, wrote(ACCEPTED), Some(BASE.to_owned()), Some(&path()))
                    .expect("plan")
            ),
            RevertWrite::Restore {
                content: BASE.to_owned(),
                expected: ExpectedDiskState::Content(ACCEPTED.to_owned()),
            }
        );
    }

    /// A creation is undone by a delete that is itself guarded; it does not
    /// need, and does not consult, a base.
    #[test]
    fn reverting_a_creation_is_a_delete_guarded_by_the_accepted_text() {
        assert_eq!(
            single(revert_plan(true, wrote(ACCEPTED), None, Some(&path())).expect("plan")),
            RevertWrite::Delete {
                expected: ExpectedDiskState::Content(ACCEPTED.to_owned()),
            }
        );
    }

    /// A delete is undone by re-creating the file with the original text,
    /// guarded on the path still being free — the accept removed it, so
    /// nothing should be there.
    #[test]
    fn reverting_a_delete_recreates_the_file_guarded_by_absence() {
        assert_eq!(
            single(
                revert_plan(
                    false,
                    Some(AcceptedAction::Deleted),
                    Some(BASE.to_owned()),
                    Some(&path())
                )
                .expect("plan")
            ),
            RevertWrite::Restore {
                content: BASE.to_owned(),
                expected: ExpectedDiskState::Absent,
            }
        );
    }

    /// A rename is undone by two steps: restore the original text at the
    /// registered path (guarded by absence — the accept moved the file away),
    /// and remove whatever the accept left at the destination (guarded by the
    /// accepted content).
    #[test]
    fn reverting_a_rename_restores_the_original_and_queues_the_destinations_removal() {
        let plan = revert_plan(
            false,
            Some(AcceptedAction::Renamed {
                to: PathBuf::from("/tmp/new.rs"),
                content: ACCEPTED.to_owned(),
            }),
            Some(BASE.to_owned()),
            Some(&path()),
        )
        .expect("plan");

        assert_eq!(
            plan,
            RevertPlan::UndoRename {
                restore: RevertWrite::Restore {
                    content: BASE.to_owned(),
                    expected: ExpectedDiskState::Absent,
                },
                at: PathBuf::from("/tmp/new.rs"),
                remove_expected: ExpectedDiskState::Content(ACCEPTED.to_owned()),
            }
        );
    }

    /// No record of the accept is a refusal, not "nothing to compare, go
    /// ahead" — for every accepted-action kind.
    #[test]
    fn a_revert_with_no_record_of_the_accept_refuses() {
        for is_new_file in [false, true] {
            let error = revert_plan(is_new_file, None, Some(BASE.to_owned()), Some(&path()))
                .expect_err("reverting blind must be refused");
            assert!(error.contains("/tmp/example.rs"), "got: {error}");
            assert!(error.contains("Nothing was changed"), "got: {error}");
        }
    }

    #[test]
    fn a_revert_with_no_base_to_restore_refuses() {
        let error = revert_plan(false, wrote(ACCEPTED), None, None)
            .expect_err("there is nothing to restore");
        assert!(error.starts_with("file was not reverted"), "got: {error}");
    }

    /// A deleted file's revert needs the original text back just as much as an
    /// edit's does.
    #[test]
    fn a_revert_of_a_delete_with_no_original_text_refuses() {
        let error = revert_plan(false, Some(AcceptedAction::Deleted), None, None)
            .expect_err("there is nothing to recreate the file with");
        assert!(error.starts_with("file was not reverted"), "got: {error}");
    }

    /// A rename's revert needs the original text back for the same reason.
    #[test]
    fn a_revert_of_a_rename_with_no_original_text_refuses() {
        let error = revert_plan(
            false,
            Some(AcceptedAction::Renamed {
                to: PathBuf::from("/tmp/new.rs"),
                content: ACCEPTED.to_owned(),
            }),
            None,
            None,
        )
        .expect_err("there is nothing to restore at the original path");
        assert!(error.starts_with("file was not reverted"), "got: {error}");
    }

    /// A refusal reaching the toast says it was the *revert* that did not
    /// happen, and keeps the guard's own explanation.
    #[test]
    fn a_revert_refusal_reads_as_a_failed_revert() {
        let refusal = FileSaveError::Other("x.rs changed on disk.".to_owned());
        assert_eq!(
            revert_failure(&refusal).to_string(),
            "Did not revert the agent's edit. x.rs changed on disk."
        );
    }

    /// Registers `path` the way `register_file` does, dispatches `write`
    /// through the same function `restore_diff_base` uses, and waits for the
    /// write's real outcome.
    async fn revert_on_disk(
        app: &mut warpui::App,
        path: &std::path::Path,
        write: RevertWrite,
    ) -> Result<(), std::sync::Arc<FileSaveError>> {
        let files = app.add_singleton_model(FileModel::new);
        let file_id = files.update(app, |files, ctx| files.register_file_path(path, false, ctx));
        let completion = files.update(app, |files, _| files.save_completion(file_id));
        files
            .update(app, |files, ctx| {
                dispatch_revert_write(files, file_id, write, ContentVersion::new(), ctx)
            })
            .expect("the revert write should dispatch");
        completion.await
    }

    /// The data-loss case: the user accepts, keeps working on the file, then
    /// clicks revert. Their work must survive, and they must be told why.
    #[test]
    fn revert_after_a_later_edit_is_refused_and_leaves_the_file_alone() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("edited.rs");
            std::fs::write(&file, LATER_EDIT).expect("write file");

            let write = single(
                revert_plan(false, wrote(ACCEPTED), Some(BASE.to_owned()), None).expect("plan"),
            );
            let error = revert_on_disk(&mut app, &file, write)
                .await
                .expect_err("the revert must be refused");

            assert!(
                revert_failure(&error)
                    .to_string()
                    .contains("changed on disk"),
                "the refusal must say why, got: {error}"
            );
            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                LATER_EDIT,
                "the user's later edit must survive the revert"
            );
        });
    }

    /// ...and the ordinary case still reverts.
    #[test]
    fn revert_with_no_later_edit_restores_the_base() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("untouched.rs");
            std::fs::write(&file, ACCEPTED).expect("write file");

            let write = single(
                revert_plan(false, wrote(ACCEPTED), Some(BASE.to_owned()), None).expect("plan"),
            );
            revert_on_disk(&mut app, &file, write)
                .await
                .expect("the revert should succeed");

            assert_eq!(std::fs::read_to_string(&file).unwrap(), BASE);
        });
    }

    #[test]
    fn reverting_an_untouched_created_file_deletes_it() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("created.rs");
            std::fs::write(&file, ACCEPTED).expect("write file");

            let write = single(revert_plan(true, wrote(ACCEPTED), None, None).expect("plan"));
            revert_on_disk(&mut app, &file, write)
                .await
                .expect("the revert should succeed");

            assert!(!file.exists(), "the created file must be gone");
        });
    }

    /// The limb that matters most: a delete is the one outcome nothing later
    /// can undo, and the user may have built on the file the agent created.
    #[test]
    fn reverting_a_modified_created_file_is_refused_and_keeps_it() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("created.rs");
            std::fs::write(&file, LATER_EDIT).expect("write file");

            let write = single(revert_plan(true, wrote(ACCEPTED), None, None).expect("plan"));
            revert_on_disk(&mut app, &file, write)
                .await
                .expect_err("the delete must be refused");

            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                LATER_EDIT,
                "the modified file must not be deleted"
            );
        });
    }

    // ── Revert of a delete, on a real file (#688) ─────────────────────────

    #[test]
    fn reverting_an_accepted_delete_recreates_the_file_byte_exact() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            // The accept removed the file: nothing is there to start with.
            let file = directory.path().join("deleted.rs");
            const ORIGINAL_CRLF: &str = "fn main() {\r\n    old();\r\n}\r\n";

            let write = single(
                revert_plan(
                    false,
                    Some(AcceptedAction::Deleted),
                    Some(ORIGINAL_CRLF.to_owned()),
                    None,
                )
                .expect("plan"),
            );
            revert_on_disk(&mut app, &file, write)
                .await
                .expect("recreating the deleted file should succeed");

            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                ORIGINAL_CRLF,
                "the raw original bytes, CRLF included, must come back exactly"
            );
        });
    }

    /// If something now occupies the path the accept freed — another process
    /// re-created it, or a symlink landed there — the revert must refuse
    /// rather than clobber it.
    #[test]
    fn reverting_an_accepted_delete_refuses_if_the_path_is_no_longer_free() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("deleted.rs");
            std::fs::write(&file, "something new lives here\n").expect("write file");

            let write = single(
                revert_plan(
                    false,
                    Some(AcceptedAction::Deleted),
                    Some(BASE.to_owned()),
                    None,
                )
                .expect("plan"),
            );
            revert_on_disk(&mut app, &file, write)
                .await
                .expect_err("recreating over an occupied path must be refused");

            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                "something new lives here\n",
                "whatever now occupies the path must be left alone"
            );
        });
    }

    // ── Revert of a rename, on real files (#688) ──────────────────────────
    //
    // `dispatch_guarded_revert`/`finish_rename_revert` sequence these same two
    // guarded `FileModel` calls behind a `ViewContext`, restoring first and
    // only then removing the destination; that sequencing itself needs the
    // live view this module's tests otherwise avoid (see the module doc), so
    // these tests drive the two guarded writes directly, in the same order,
    // proving each half lands (or refuses) exactly as `revert_plan` says.

    #[test]
    fn reverting_an_accepted_rename_restores_the_original_and_removes_the_destination() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let original_path = directory.path().join("old.rs");
            let destination_path = directory.path().join("new.rs");
            // The accept moved the file: nothing at the original path, the
            // accepted content at the destination.
            std::fs::write(&destination_path, ACCEPTED).expect("write file");

            let plan = revert_plan(
                false,
                Some(AcceptedAction::Renamed {
                    to: destination_path.clone(),
                    content: ACCEPTED.to_owned(),
                }),
                Some(BASE.to_owned()),
                None,
            )
            .expect("plan");
            let RevertPlan::UndoRename {
                restore,
                at,
                remove_expected,
            } = plan
            else {
                panic!("expected a rename revert plan");
            };
            assert_eq!(at, destination_path);

            // One `FileModel`, two registrations — exactly as
            // `finish_rename_revert` registers the destination fresh while
            // reusing the view's own `backing_file_id` for the original path.
            let files = app.add_singleton_model(FileModel::new);
            let original_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&original_path, false, ctx)
            });

            // Step 1: restore the original path. Guarded revert dispatches
            // this first and only proceeds to step 2 once it lands.
            write_and_wait(&mut app, &files, original_id, restore)
                .await
                .expect("restoring the original path should succeed");
            assert_eq!(std::fs::read_to_string(&original_path).unwrap(), BASE);

            // Step 2: remove whatever the accept left at the destination.
            let destination_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&destination_path, false, ctx)
            });
            write_and_wait(
                &mut app,
                &files,
                destination_id,
                RevertWrite::Delete {
                    expected: remove_expected,
                },
            )
            .await
            .expect("removing the destination should succeed");
            assert!(
                !destination_path.exists(),
                "the destination must be gone once both steps land"
            );
        });
    }

    /// The order matters: if step 1 (the restore) is refused, step 2 must
    /// never run — the accepted content is the only copy of the file that
    /// still exists, and removing it would lose it entirely.
    #[test]
    fn a_refused_restore_must_never_be_followed_by_removing_the_destination() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let original_path = directory.path().join("old.rs");
            let destination_path = directory.path().join("new.rs");
            // Something now occupies the original path (the guard's `Absent`
            // pre-image no longer holds), and the destination still has the
            // only real copy of the accepted content.
            std::fs::write(&original_path, "reused for something else\n").expect("write file");
            std::fs::write(&destination_path, ACCEPTED).expect("write file");

            let plan = revert_plan(
                false,
                Some(AcceptedAction::Renamed {
                    to: destination_path.clone(),
                    content: ACCEPTED.to_owned(),
                }),
                Some(BASE.to_owned()),
                None,
            )
            .expect("plan");
            let RevertPlan::UndoRename { restore, .. } = plan else {
                panic!("expected a rename revert plan");
            };

            let files = app.add_singleton_model(FileModel::new);
            let original_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&original_path, false, ctx)
            });
            write_and_wait(&mut app, &files, original_id, restore)
                .await
                .expect_err("the restore must be refused: the original path is occupied");

            // `finish_rename_revert` never dispatches the destination's
            // removal when the restore above comes back `Err`; asserting the
            // destination is untouched is the observable half of that
            // contract this disk-level test can check without a live view.
            assert_eq!(
                std::fs::read_to_string(&destination_path).unwrap(),
                ACCEPTED,
                "the only copy of the file must survive a refused restore"
            );
            assert_eq!(
                std::fs::read_to_string(&original_path).unwrap(),
                "reused for something else\n",
                "whatever now occupies the original path must be left alone"
            );
        });
    }

    // ── Partial rename revert: step 1 lands, step 2 does not (#688 review) ─
    //
    // `finish_rename_revert` records `AcceptedAction::PartiallyRevertedRename`
    // the moment the restore at the registered path lands, before step 2 is
    // even attempted. `revert_plan` is what a retry consults, so these tests
    // drive it directly rather than the live view (see the module doc).

    /// The plan for a retry: only the destination write remains. Passing
    /// `original: None` and still getting a plan back is the point — a retry
    /// from this state needs no original text at all, which is why this must
    /// be its own `AcceptedAction` rather than reusing `Renamed`.
    #[test]
    fn a_partially_reverted_rename_retries_only_the_destination() {
        let plan = revert_plan(
            false,
            Some(AcceptedAction::PartiallyRevertedRename {
                to: PathBuf::from("/tmp/new.rs"),
                content: ACCEPTED.to_owned(),
            }),
            None,
            Some(&path()),
        )
        .expect("a retry needs no original text");

        assert_eq!(
            plan,
            RevertPlan::FinishRename {
                at: PathBuf::from("/tmp/new.rs"),
                remove_expected: ExpectedDiskState::Content(ACCEPTED.to_owned()),
            }
        );
    }

    /// The honest message: naming both what landed (the restore) and what did
    /// not (the removal), not just the removal's bare guard refusal, which
    /// read alone sounds like nothing happened at all.
    #[test]
    fn partial_rename_revert_failure_names_both_files() {
        let error = FileSaveError::Other("new.rs changed on disk.".to_owned());
        let message =
            partial_rename_revert_failure("old.rs", std::path::Path::new("new.rs"), &error)
                .to_string();

        assert!(message.contains("old.rs was restored"), "got: {message}");
        assert!(
            message.contains("both files now exist") || message.contains("Both files now exist"),
            "got: {message}"
        );
        assert!(
            message.contains("new.rs changed on disk"),
            "must keep the underlying reason, got: {message}"
        );
    }

    /// End to end on real files: step 1 already landed (the original is back
    /// at its path, untouched by anything below), step 2 is retried and
    /// refused because the destination changed since the first attempt. The
    /// retry must touch only the destination — never re-attempt the restore,
    /// whose `Absent` guard would now refuse it — and the original must
    /// survive exactly as it already was.
    #[test]
    fn retrying_a_partially_reverted_rename_only_touches_the_destination() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let original_path = directory.path().join("old.rs");
            let destination_path = directory.path().join("new.rs");
            // Step 1 already landed on a previous attempt.
            std::fs::write(&original_path, BASE).expect("write file");
            // The user edited the destination before the retry.
            std::fs::write(&destination_path, "edited after the first attempt\n")
                .expect("write file");

            let plan = revert_plan(
                false,
                Some(AcceptedAction::PartiallyRevertedRename {
                    to: destination_path.clone(),
                    content: ACCEPTED.to_owned(),
                }),
                None,
                None,
            )
            .expect("plan");
            let RevertPlan::FinishRename { at, remove_expected } = plan else {
                panic!("expected a finish-rename plan, got {plan:?}");
            };
            assert_eq!(at, destination_path);

            let files = app.add_singleton_model(FileModel::new);
            let destination_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&destination_path, false, ctx)
            });
            write_and_wait(
                &mut app,
                &files,
                destination_id,
                RevertWrite::Delete {
                    expected: remove_expected,
                },
            )
            .await
            .expect_err("the retry must be refused: the destination changed again");

            assert_eq!(
                std::fs::read_to_string(&original_path).unwrap(),
                BASE,
                "the retry must never touch the registered path a second time"
            );
            assert_eq!(
                std::fs::read_to_string(&destination_path).unwrap(),
                "edited after the first attempt\n",
                "a refused retry must leave the destination exactly as it was"
            );
        });
    }

    /// ...and once the destination is back to what the first attempt left, a
    /// retry lands and removes it, leaving only the (already-restored)
    /// original.
    #[test]
    fn retrying_a_partially_reverted_rename_succeeds_once_the_destination_matches() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let original_path = directory.path().join("old.rs");
            let destination_path = directory.path().join("new.rs");
            std::fs::write(&original_path, BASE).expect("write file");
            std::fs::write(&destination_path, ACCEPTED).expect("write file");

            let plan = revert_plan(
                false,
                Some(AcceptedAction::PartiallyRevertedRename {
                    to: destination_path.clone(),
                    content: ACCEPTED.to_owned(),
                }),
                None,
                None,
            )
            .expect("plan");
            let RevertPlan::FinishRename { at: _, remove_expected } = plan else {
                panic!("expected a finish-rename plan, got {plan:?}");
            };

            let files = app.add_singleton_model(FileModel::new);
            let destination_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&destination_path, false, ctx)
            });
            write_and_wait(
                &mut app,
                &files,
                destination_id,
                RevertWrite::Delete {
                    expected: remove_expected,
                },
            )
            .await
            .expect("the retry should land");

            assert!(!destination_path.exists(), "the destination must be gone");
            assert_eq!(
                std::fs::read_to_string(&original_path).unwrap(),
                BASE,
                "the original, already restored, must be left alone"
            );
        });
    }

    // ── Line endings (#672) ──────────────────────────────────────────────
    //
    // A revert must write back the file's *raw* original text. The editor's
    // diff base is LF-normalised, and `FileModel` refuses a write whose line
    // endings differ from the file's — so reverting with the diff base refused
    // every revert of a CRLF file, with a toast blaming a line-ending
    // conversion that never happened.

    const CRLF_ORIGINAL: &str = "fn main() {\r\n    old();\r\n}\r\n";
    /// What the accept wrote: the buffer keeps the file's (CRLF) endings.
    const CRLF_ACCEPTED: &str = "fn main() {\r\n    new();\r\n}\r\n";

    #[test]
    fn reverting_a_crlf_file_restores_it_byte_for_byte() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("crlf.rs");
            std::fs::write(&file, CRLF_ACCEPTED).expect("write file");

            let write = single(
                revert_plan(
                    false,
                    wrote(CRLF_ACCEPTED),
                    Some(CRLF_ORIGINAL.to_owned()),
                    None,
                )
                .expect("plan"),
            );
            revert_on_disk(&mut app, &file, write)
                .await
                .expect("reverting an untouched CRLF file must land");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), CRLF_ORIGINAL);
        });
    }

    /// The defect: the LF-normalised diff base, written over the CRLF file the
    /// accept left, is refused as a line-ending conversion.
    #[test]
    fn reverting_a_crlf_file_with_the_lf_normalised_base_is_refused() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("crlf.rs");
            std::fs::write(&file, CRLF_ACCEPTED).expect("write file");

            let write = single(
                revert_plan(
                    false,
                    wrote(CRLF_ACCEPTED),
                    Some(CRLF_ORIGINAL.replace("\r\n", "\n")),
                    None,
                )
                .expect("plan"),
            );
            revert_on_disk(&mut app, &file, write)
                .await
                .expect_err("an LF base over a CRLF file is a line-ending conversion");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), CRLF_ACCEPTED);
        });
    }

    /// A file with mixed endings: the buffer normalises to the majority, so
    /// the accept wrote uniform LF. The revert restores the original exactly,
    /// minority CRLF lines included, rather than leaving them converted.
    #[test]
    fn reverting_a_mixed_ending_file_restores_its_minority_line_endings() {
        const MIXED_ORIGINAL: &str = "fn main() {\r\n    old();\n}\n";
        const MIXED_ACCEPTED: &str = "fn main() {\n    new();\n}\n";
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("mixed.rs");
            std::fs::write(&file, MIXED_ACCEPTED).expect("write file");

            let write = single(
                revert_plan(
                    false,
                    wrote(MIXED_ACCEPTED),
                    Some(MIXED_ORIGINAL.to_owned()),
                    None,
                )
                .expect("plan"),
            );
            revert_on_disk(&mut app, &file, write)
                .await
                .expect("reverting an untouched mixed-ending file must land");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), MIXED_ORIGINAL);
        });
    }

    // ── Rewind over several edits to one file (#686) ─────────────────────

    /// What the agent's second accepted edit left in the file. Its diff base is
    /// `ACCEPTED`, the first edit's accepted text.
    const SECOND_ACCEPTED: &str = "fn main() {\n    println!(\"second\");\n}\n";

    /// Dispatches `write` against the already-registered `file_id` and waits
    /// for its real outcome.
    async fn write_and_wait(
        app: &mut warpui::App,
        files: &warpui::ModelHandle<FileModel>,
        file_id: FileId,
        write: RevertWrite,
    ) -> Result<(), std::sync::Arc<FileSaveError>> {
        let completion = files.update(app, |files, _| files.save_completion(file_id));
        files
            .update(app, |files, ctx| {
                dispatch_revert_write(files, file_id, write, ContentVersion::new(), ctx)
            })
            .expect("the revert write should dispatch");
        completion.await
    }

    fn edit_revert(accepted: &str, base: &str) -> std::cell::RefCell<Option<RevertWrite>> {
        std::cell::RefCell::new(Some(single(
            revert_plan(false, wrote(accepted), Some(base.to_owned()), None).expect("plan"),
        )))
    }

    /// Why a rewind must order reverts: the older edit's revert asserts the
    /// older edit's accepted text, which is only on disk again once the newer
    /// edit's revert has landed. Run first (as the concurrent dispatch
    /// effectively did), it is refused and the file is left alone.
    #[test]
    fn the_older_revert_of_a_file_is_refused_before_the_newer_one_lands() {
        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("twice.rs");
            std::fs::write(&file, SECOND_ACCEPTED).expect("write file");
            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&file, false, ctx)
            });

            let older = edit_revert(ACCEPTED, BASE).take().unwrap();
            write_and_wait(&mut app, &files, file_id, older)
                .await
                .expect_err("the older revert must be refused while the newer edit stands");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), SECOND_ACCEPTED);
        });
    }

    /// The regression (#686), end to end on a real file through the real
    /// guarded writes: two accepted edits to one file, rewound through
    /// `RevertSequence`, which holds the older revert back until the newer one
    /// has landed. Both land, and the file is back to its original contents.
    #[test]
    fn two_edits_to_one_file_revert_newest_first_to_the_original() {
        use crate::ai::blocklist::rewind_revert::{RevertSequence, RevertStart};

        warpui::App::test((), |mut app| async move {
            let directory = tempfile::tempdir().expect("temp dir");
            let file = directory.path().join("twice.rs");
            std::fs::write(&file, SECOND_ACCEPTED).expect("write file");
            let files = app.add_singleton_model(FileModel::new);
            let file_id = files.update(&mut app, |files, ctx| {
                files.register_file_path(&file, false, ctx)
            });

            // Newest first, as the rewind collects them.
            let mut sequence = RevertSequence::new([
                (Some("twice.rs"), edit_revert(SECOND_ACCEPTED, ACCEPTED)),
                (Some("twice.rs"), edit_revert(ACCEPTED, BASE)),
            ]);
            let mut dispatched = Vec::new();

            assert!(
                sequence
                    .start(|write| {
                        dispatched.push(write.take().expect("dispatched once"));
                        RevertStart::InFlight
                    })
                    .is_empty()
            );
            assert_eq!(
                dispatched.len(),
                1,
                "only the newer revert may be in flight"
            );
            write_and_wait(&mut app, &files, file_id, dispatched.pop().unwrap())
                .await
                .expect("the newer revert should land");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), ACCEPTED);

            let abandoned = sequence
                .settled(
                    |_| true,
                    true,
                    |write| {
                        dispatched.push(write.take().expect("dispatched once"));
                        RevertStart::InFlight
                    },
                )
                .expect("the newer revert was in flight");
            assert!(abandoned.is_empty());
            assert_eq!(dispatched.len(), 1, "the older revert is dispatched now");
            write_and_wait(&mut app, &files, file_id, dispatched.pop().unwrap())
                .await
                .expect("the older revert should land");

            assert!(
                sequence
                    .settled(|_| true, true, |_| unreachable!("nothing left to dispatch"))
                    .is_some_and(|abandoned| abandoned.is_empty())
            );
            assert!(sequence.is_settled());
            assert_eq!(std::fs::read_to_string(&file).unwrap(), BASE);
        });
    }
}
