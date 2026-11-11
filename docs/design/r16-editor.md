# Round 16, editor: unsaved edits always ask; a save on the conflict banner asks (#2092, #2087)

Base: integ/r14 `fae4cecfd`. Owner decisions of 2026-11-11 (#2092 split in two).
Nothing here was compiled or run on an app. The harness self-test and `rustfmt --check` are the only things that ran.

## 1. Losing unsaved editor edits always asks

**Cause.** `app/src/quit_warning/mod.rs:274` `should_display_warning` was
`show_warning_before_quitting && (long-running commands || shared sessions || unsaved_code_changes)`. Every close path
takes its answer from that one function: quit (`app/src/lib.rs:2809`), close window (`lib.rs:2764`), close tabs
(`app/src/workspace/view.rs:11675`), close pane (`app/src/pane_group/mod.rs:4344`), close an editor tab
(`app/src/code/view.rs:1636`). With the setting off, all five closed at once and the edits were gone. The pin has the
same line (`git show 42effe840:app/src/quit_warning/mod.rs`, line 353).

**Chosen.** The rule is now a pure function, `should_display_warning_for`:
`(setting && (running || unsaved)) || (always_ask_unsaved && unsaved)`. The setting keeps covering running commands and
shared sessions only. No call site changed, so no close path can be missed. Kill switch
`PHOSPHOR_UNSAVED_EDITS_ALWAYS_ASK=0` (read once per run) puts back the one-setting rule.

**A second defect this would have opened.** The dialog has a "Don't show again" checkbox whose callback *toggles* the
setting (`quit_warning/mod.rs` `on_disable_warning_modal`). On Linux and Windows the checkbox state starts from the
setting (`app/src/workspace/native_modal.rs:53`), so with the setting off it starts ticked, and pressing any button ran
the callback: the setting would have flipped back to on behind the user's back. On macOS a tick on the system alert's
checkbox would have done the same. Fix: `AlertDialogWithCallbacks::can_disable` (default `true`,
`without_disable()` clears it). A dialog that asks about unsaved edits is built without it: the in-app dialog draws no
checkbox and never runs the callback (`should_run_disable_callback`); the macOS alert is built without its suppression
button (`AlertDialog::show_suppression_button`, set in `create_native_platform_modal`; `configureAndRunModal` no longer
forces it on) and the core replaces the callback with a no-op. The running-commands dialog keeps the checkbox.

**Rejected.** A second setting for the editor question (the decision is "always"); editing the five call sites (one
rule is harder to get wrong); leaving the checkbox and making the callback idempotent (a checkbox that promises "don't
show again" on a question that comes back anyway).

**Risk.** Low. With the setting on nothing changes except the missing checkbox on a dialog that names unsaved edits.
The macOS change touches how the system alert is configured and was not compiled.

## 2. A save while the conflict banner shows asks

**Cause.** The banner (#2087) shows when the file on disk is no longer what the buffer started from
(`local_code_editor.rs` `has_version_conflicts`). `save_local` did not look at that state, and the banner's own
"Overwrite" button dispatched the same `SaveFile` action as Ctrl-S. So Ctrl-S, "Save" on a close question, a
code-review save and an auto-accepted agent diff all wrote the buffer over the other program's change.

**Chosen.** `LocalCodeEditorView::save_local` is the funnel for every one of those. While the banner shows it does not
write: it schedules the question and returns `ImmediateSaveError::AwaitingOverwriteAnswer` (not a failure; no toast, no
error log). The question is the in-app dialog with three buttons in this order: Overwrite, Reload, Cancel. Enter is the
first button, Escape the last, so Escape always loses nothing. No "don't ask again".
(Changed the same day by a second owner decision: Enter is Cancel, never Overwrite; Overwrite is a click or Tab then
Enter. See `docs/design/r10-r16-smallapp.md` section 1.)

- Overwrite: the save that was asked for (format-on-save included).
- Reload: the banner's "Discard this version" (same function, local and remote).
- Cancel: nothing; the banner stays.
- The banner's "Overwrite" button is a new action, `OverwriteFile`: it is the explicit answer and does not ask again.

A caller that waits on the save (`CodeView::save_local` with a callback: "Save" on a tab's close question, close all,
accept-and-save, render markdown) is parked in the map that already parks remote saves and is finished by `FileSaved` /
`FailedToSave` after Overwrite or by the new `LocalCodeEditorEvent::SaveCanceled` after Reload or Cancel. Before this,
"Save" on the close question would have closed the tab the moment `save_local` returned; with a question pending that
would have dropped the editor, and the edits with it, before the user answered.

The question is shown one step later (`ctx.spawn(async {}, ..)`), because a save can be asked for from inside the same
in-app dialog ("Save" on the close question): that dialog is mid-update and clears itself when its handler returns.
If the question cannot be shown (no workspace in the window, the workspace is mid-update, or another dialog is up:
`Workspace::show_native_modal_if_free`), nothing is written, a warning is logged and the save is reported cancelled.

The in-app dialog is used on macOS too (not the system alert): it does not block the app, and the app tests can answer
it by key.

Kill switch `PHOSPHOR_EDITOR_SAVE_CONFLICT_ASK=0`. Not asked on the integration-test channel (nobody can answer), like
the close and quit questions. Trace for the app tests, target `editor_disk`, debug:
`editor_save_ask outcome=<asked|overwrite|reload|cancel|not_shown|banner_overwrite>`.

**Rejected.** A new dialog view inside the editor (more code than reusing the workspace dialog, and a second focus
path); asking inside `perform_save` (callers have already been told the save is under way); a sticky "question open"
flag on the editor (a dialog that is replaced drops its callbacks unrun, and the flag would then swallow every later
save; the workspace's own "dialog open" state is the truth instead).

**Left open.**
- Ctrl-S in the moment before the file watcher has reported the change still overwrites (unchanged from #2087; needs a
  disk check at save time).
- A conflict that arrives while format-on-save is waiting for the language server is not asked about (the save was
  started before the banner showed).
- Two saves from two callers in the same step share one question; only the later caller's callback is kept.
- The title and body of the question and the "Reload" label are English only (the other two labels are translated).

**Risk.** Medium for a change nobody compiled: one new error variant, one new event variant (both match sites handled),
a deferred dialog. A wrong turn fails safe (no write) except where noted above.

## Tests

- Unit: `app/src/quit_warning/mod_tests.rs` (the rule, the kill switch, the checkbox rule),
  `app/src/workspace/native_modal_tests.rs` (the disable callback rule), `app/src/code/save_conflict_tests.rs` (the ask
  rule, the kill switch, button order, trace format).
- App case `editor_daily` (`cases_r15_w5a.py`, PROVING): the close arm no longer excuses a profile with the quit
  warning off (stricter); the external-change arm now judges the save: asked and nothing written, Escape cancels with
  the disk untouched, a second save asks again, Enter overwrites with the editor's text. Reload is not played (the
  dialog has keys for the first and last button only).
