use std::rc::Rc;

#[cfg(not(target_family = "wasm"))]
use crate::ai::blocklist::inline_action::code_diff_view::DiffSessionType;
use ai::diff_validation::DiffType;
#[cfg(not(target_family = "wasm"))]
use warp_files::{ExpectedDiskState, FileModel, FileModelEvent};
use warp_util::file::FileId;
#[cfg(not(target_family = "wasm"))]
use warp_util::file::FileSaveError;
use warp_util::standardized_path::StandardizedPath;
#[cfg(not(target_family = "wasm"))]
use warpui::SingletonEntity;
use warpui::elements::ChildView;
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};

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
    /// - Accept, save, and revert are no-ops.
    backing_file_id: Option<FileId>,
    /// Whether the diff is a new file creation (for revert: delete instead of restore).
    #[cfg(not(target_family = "wasm"))]
    is_new_file: bool,
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
        ctx.subscribe_to_model(&file_model, move |_me, _file_model, event, ctx| {
            if file_id == event.file_id() {
                match event {
                    FileModelEvent::FileSaved { .. } => {
                        ctx.emit(InlineDiffViewEvent::FileSaved);
                    }
                    FileModelEvent::FailedToSave { error, .. } => {
                        ctx.emit(InlineDiffViewEvent::FailedToSave {
                            error: error.clone(),
                        });
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

    fn restore_diff_base(&mut self, _ctx: &mut ViewContext<Self>) -> Result<(), String> {
        // No-op when no file is registered (WASM / restored conversations).
        if self.backing_file_id.is_none() {
            return Ok(());
        }

        #[cfg(not(target_family = "wasm"))]
        {
            let file_id = self
                .backing_file_id
                .expect("backing_file_id must be Some — checked by early return above");

            if self.is_new_file {
                // For newly created files, delete instead of restoring.
                //
                // Unguarded, for the same reason as the restore below and with
                // the same cost: `FileModel::delete_if_unchanged` exists and
                // would take a pre-image, but the only honest pre-image here is
                // what the accept wrote, which this view does not retain. A file
                // the accept created and something else then rewrote is removed
                // by this call with no check and no message. Filed with the
                // revert follow-up rather than guarded against the wrong thing.
                let version = self.editor.as_ref(_ctx).version(_ctx);
                FileModel::handle(_ctx)
                    .update(_ctx, |file_model, ctx| {
                        file_model.delete(file_id, version, ctx)
                    })
                    .map_err(|e| format!("Failed to delete file: {e:?}"))?;
                return Ok(());
            }

            // For existing files, restore the base content from the editor's DiffModel.
            //
            // This write is deliberately left unguarded, unlike `save_content`.
            // Revert is only reachable from `CodeDiffState::Accepted(None)`, so
            // its pre-image is not the diff base but whatever the accept just
            // wrote — bytes this view does not durably record. Guarding it
            // against the base instead would refuse every revert that follows a
            // format-on-save, which is the common case rather than the
            // dangerous one. Closing this properly needs the accepted content
            // retained at accept time; filed rather than guessed at here.
            let base_content = self
                .editor
                .as_ref(_ctx)
                .model
                .as_ref(_ctx)
                .diff()
                .as_ref(_ctx)
                .base()
                .ok_or_else(|| "Missing base content".to_string())?
                .to_string();

            let version = self.editor.as_ref(_ctx).version(_ctx);
            FileModel::handle(_ctx)
                .update(_ctx, |file_model, ctx| {
                    file_model.save(file_id, base_content, version, ctx)
                })
                .map_err(|e| format!("Failed to save file: {e:?}"))?;
        }

        Ok(())
    }
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
}
