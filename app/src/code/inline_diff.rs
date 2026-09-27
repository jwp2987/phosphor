#[cfg(not(target_family = "wasm"))]
use std::cell::RefCell;
use std::rc::Rc;

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
    /// Whether the diff is a new file creation (for revert: delete instead of restore).
    #[cfg(not(target_family = "wasm"))]
    is_new_file: bool,
    /// The exact text the accept asked `FileModel` to write, recorded at accept
    /// time. This is the pre-image a revert asserts: a revert undoes the accept,
    /// so it may only run against a file that still holds what the accept left
    /// there. `None` until an accept has dispatched a write.
    ///
    /// A `RefCell` because the accept path (`DiffViewer::accept_and_save_diff`)
    /// takes `&self`.
    #[cfg(not(target_family = "wasm"))]
    accepted_content: RefCell<Option<String>>,
    /// Set once a revert has dispatched its write, so that the write's
    /// asynchronous refusal is reported as a failed *revert* rather than as a
    /// failed save of the accept.
    #[cfg(not(target_family = "wasm"))]
    revert_dispatched: bool,
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
            is_new_file,
            #[cfg(not(target_family = "wasm"))]
            accepted_content: RefCell::new(None),
            #[cfg(not(target_family = "wasm"))]
            revert_dispatched: false,
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

        self.finish_file_registration(file_id, ctx);
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

    #[cfg(not(target_family = "wasm"))]
    fn save_content(&self, ctx: &mut ViewContext<Self>) {
        let Some(file_id) = self.backing_file_id else {
            return;
        };
        let content = self.editor.as_ref(ctx).text(ctx).into_string();
        let version = self.editor.as_ref(ctx).version(ctx);

        // The buffer being written is a snapshot the agent produced, possibly
        // minutes ago, plus whatever the user typed into this view. Anything
        // that touched the file in between — another editor, a formatter, a
        // rebase — is not in it. Write only if the file still holds the text the
        // diff was computed from; otherwise report and write nothing.
        //
        // # Divergence from the pinned oracle
        //
        // This is **not** a parity port. Pinned Warp `42effe840` writes here
        // unconditionally (`42effe840:app/src/code/inline_diff.rs:220-228` calls
        // `FileModel::save` with the whole buffer, and that `save` has no mtime
        // or version check either), so accepting an edit silently discards every
        // concurrent external change. The oracle shares the defect and we are
        // fixing it ahead of the oracle deliberately, because the loss is
        // unrecoverable and the user is never told. A re-pin must not "restore
        // parity" by reverting this to `FileModel::save`.
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
        *self.accepted_content.borrow_mut() = Some(content.clone());

        if let Err(err) = FileModel::handle(ctx).update(ctx, |file_model, ctx| {
            file_model.save_if_unchanged(file_id, content, expected, version, ctx)
        }) {
            ctx.emit(InlineDiffViewEvent::FailedToSave {
                error: Rc::new(err),
            });
        }
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
        // but the text the accept wrote, recorded in `accepted_content`.
        // See `revert_plan` for what each case asserts.
        //
        // # Divergence from the pinned oracle
        //
        // Not a parity port. Pinned Warp `4111d08f9` reverts with the
        // unconditional `FileModel::save` / `FileModel::delete`, so a revert
        // destroys every edit made to the file after the accept, and deletes
        // an agent-created file the user has since built on, with no check
        // and no message. A re-pin must not "restore parity" here.
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
        let write = revert_plan(
            self.is_new_file,
            self.accepted_content.borrow().clone(),
            base,
            self.file_path.as_ref(),
        )?;

        let version = self.editor.as_ref(ctx).version(ctx);
        FileModel::handle(ctx)
            .update(ctx, |file_model, ctx| {
                dispatch_revert_write(file_model, file_id, write, version, ctx)
            })
            .map_err(|error| revert_failure(&error).to_string())?;
        // Only once a write is actually in flight: its refusal, if any,
        // arrives later as `FileModelEvent::FailedToSave` and is reported as
        // a failed revert from there.
        self.revert_dispatched = true;
        Ok(RevertDispatch::WriteInFlight)
    }
}

/// The single guarded write that undoes an accepted diff.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, PartialEq, Eq)]
enum RevertWrite {
    /// The accept created the file; remove it, but only if it still holds what
    /// the accept wrote.
    Delete { expected: ExpectedDiskState },
    /// The accept overwrote the file; put `content` (the diff base) back, but
    /// only if the file still holds what the accept wrote.
    Restore {
        content: String,
        expected: ExpectedDiskState,
    },
}

/// Decides the write that undoes an accept, and the pre-image it asserts.
///
/// The GUI counterpart of `warp_tui::tui_diff_storage::revert_plan`, with the
/// same semantics: a revert's pre-image is what the accept left on disk, so
///
/// * a **creation** is undone by a delete that requires the file to still hold
///   the accepted text — an agent-created file the user has since edited is
///   not deleted;
/// * an **edit** is undone by writing the diff base back, which requires the
///   file to still hold the accepted text — edits made after the accept are not
///   overwritten.
///
/// Unlike the TUI, the accepted text cannot be re-derived from the diff: this
/// view is editable, and the user may have changed the agent's proposal before
/// accepting it. So the accept records what it wrote (`accepted_content`) and
/// that is what arrives here.
///
/// A formatter or anything else that touched the file after the accept
/// therefore refuses the revert. That is the intended trade, and the TUI's:
/// the common case (nothing touched the file) still passes, and the comparison
/// in `FileModel` is line-ending-normalised, so a CRLF file is not a refusal.
///
/// `Err` is a refusal with a user-facing message; nothing is written.
#[cfg(not(target_family = "wasm"))]
fn revert_plan(
    is_new_file: bool,
    accepted: Option<String>,
    base: Option<String>,
    file_path: Option<&StandardizedPath>,
) -> Result<RevertWrite, String> {
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
             wrote, so there is no way to tell whether the file changed since. \
             Nothing was changed.",
            path()
        )
    })?;

    if is_new_file {
        return Ok(RevertWrite::Delete {
            expected: ExpectedDiskState::Content(accepted),
        });
    }

    let content = base.ok_or_else(|| {
        format!(
            "{} was not reverted: the original contents this edit replaced are no \
             longer available. Nothing was changed.",
            path()
        )
    })?;

    Ok(RevertWrite::Restore {
        content,
        expected: ExpectedDiskState::Content(accepted),
    })
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

/// Rewrites a write failure as a failed *revert*, so the toast does not read
/// as a failed accept.
///
/// A guarded refusal (`FileSaveError::Other`) is a full sentence that already
/// names the file and says it was left alone, so it is prefixed rather than
/// wrapped in a second path. Anything else keeps its reason.
#[cfg(not(target_family = "wasm"))]
fn revert_failure(error: &FileSaveError) -> FileSaveError {
    let reason = match error {
        FileSaveError::Other(message) => message.clone(),
        // `IOError`'s own `Display` is a constant; the `io::Error` is the reason.
        FileSaveError::IOError { error, path } => format!("{}: {error}", path.display()),
        other => other.to_string(),
    };
    FileSaveError::Other(format!("Did not revert the agent's edit. {reason}"))
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
    /// [`pre_image_for_diff`] is the whole of the accept path's `Absent` vs
    /// `Content` vs refuse decision, and it is covered below. What is *not*
    /// covered is the plumbing in [`InlineDiffView::expected_disk_state`] that
    /// feeds it: reading the diff base out of the editor's `DiffModel` needs a
    /// live `CodeEditorView` inside an `App::test`, with the buffer reset,
    /// diffs applied and a base set — the fixture the app crate's view tests
    /// build, and more setup than the two handle dereferences it would be
    /// testing. That plumbing has no branch of its own; the branch is here.
    const BASE: &str = "fn main() {}\n";

    fn path() -> StandardizedPath {
        StandardizedPath::try_new("/tmp/example.rs").expect("standardized path")
    }

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

    // ── Revert (`revert_plan` + `dispatch_revert_write`) ─────────────────
    //
    // `restore_diff_base` is two editor-handle dereferences around these two
    // functions; the same fixture limitation as above applies to it. The
    // decisions are here, and the disk tests below push each plan through the
    // real `FileModel` guarded writes against a real file.

    /// What the user accepted — the agent's proposal, possibly hand-edited.
    const ACCEPTED: &str = "fn main() {\n    println!(\"accepted\");\n}\n";
    /// What the user wrote to the file after accepting.
    const LATER_EDIT: &str = "fn main() {\n    println!(\"my own work\");\n}\n";

    /// The defect: a revert's pre-image is what the accept wrote, never the
    /// base it is about to put back.
    #[test]
    fn reverting_an_edit_restores_the_base_guarded_by_the_accepted_text() {
        assert_eq!(
            revert_plan(
                false,
                Some(ACCEPTED.to_owned()),
                Some(BASE.to_owned()),
                Some(&path())
            ),
            Ok(RevertWrite::Restore {
                content: BASE.to_owned(),
                expected: ExpectedDiskState::Content(ACCEPTED.to_owned()),
            })
        );
    }

    /// A creation is undone by a delete that is itself guarded; it does not
    /// need, and does not consult, a base.
    #[test]
    fn reverting_a_creation_is_a_delete_guarded_by_the_accepted_text() {
        assert_eq!(
            revert_plan(true, Some(ACCEPTED.to_owned()), None, Some(&path())),
            Ok(RevertWrite::Delete {
                expected: ExpectedDiskState::Content(ACCEPTED.to_owned()),
            })
        );
    }

    /// No record of the accept is a refusal, not "nothing to compare, go
    /// ahead" — for a creation as much as for an edit.
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
        let error = revert_plan(false, Some(ACCEPTED.to_owned()), None, None)
            .expect_err("there is nothing to restore");
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

            let write = revert_plan(
                false,
                Some(ACCEPTED.to_owned()),
                Some(BASE.to_owned()),
                None,
            )
            .expect("plan");
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

            let write = revert_plan(
                false,
                Some(ACCEPTED.to_owned()),
                Some(BASE.to_owned()),
                None,
            )
            .expect("plan");
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

            let write = revert_plan(true, Some(ACCEPTED.to_owned()), None, None).expect("plan");
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

            let write = revert_plan(true, Some(ACCEPTED.to_owned()), None, None).expect("plan");
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
        std::cell::RefCell::new(Some(
            revert_plan(
                false,
                Some(accepted.to_owned()),
                Some(base.to_owned()),
                None,
            )
            .expect("plan"),
        ))
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
