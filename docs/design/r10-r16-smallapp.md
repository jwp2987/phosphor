# Round 16, small app changes: Enter cancels the overwrite question; passive code diffs follow the ssh rule (#2092, #2097)

Base: integ/r14 `37ad6ceb5`. Owner decisions of 2026-11-11. Nothing here was compiled or run on an app; `rustfmt --check`
and the harness self-test are the only things that ran.

## 1. Enter on the editor's overwrite question is Cancel, never Overwrite (#2092)

**Today.** A save while the disk-conflict banner shows asks Overwrite / Reload / Cancel (`docs/design/r16-editor.md`
section 2, `app/src/code/local_code_editor.rs` `show_overwrite_ask`). The question is the in-app dialog
(`app/src/workspace/native_modal.rs`), which has two keys for every dialog: Enter presses the first button
(`NativeModalAction::Confirm` -> `trigger_button_callback(0)`), Escape the last. The buttons are
`OverwriteAnswer::BUTTONS` = Overwrite, Reload, Cancel, so Enter overwrites. A user who presses Ctrl-S and then Enter
(typing on, or answering a question they did not read) replaces the other program's change.

**Chosen.** The dialog can be told which button Enter presses.

- `AlertDialogWithCallbacks::enter_button: Option<usize>` (`with_enter_button(i)`). `None`, the default for every
  existing dialog: exactly as before. `Some(i)`: the dialog comes up with button `i` selected.
- `NativeModal` keeps `selected_button`. Enter presses the selected button; Tab / Shift-Tab move the selection and
  wrap; the accent colour is on the selected button (it was always on the first), so the screen shows what Enter does.
  Escape is still the last button. Three pure functions carry the rule: `initial_selection`, `moved_selection`,
  `enter_button_index`.
- The overwrite question asks for Cancel: `OverwriteAnswer::enter_button(true)` is Cancel's position. The button
  order is unchanged (Overwrite, Reload, Cancel), so Escape is still Cancel and a click works as before. From Cancel
  one Tab wraps to Overwrite: "Tab, Enter" overwrites; Shift-Tab reaches Reload, which had no key at all before.
- Kill switch `PHOSPHOR_EDITOR_OVERWRITE_ENTER_CANCELS=0` (read once per run): the question is built without a
  selection, so Enter is Overwrite and Tab does nothing, as before the decision.

**Rejected.**
- *Reorder the buttons to put Cancel first.* Escape is the last button; with Cancel first the last would be Overwrite
  or Reload, and Escape would lose data.
- *A letter key for Overwrite (`o`).* A bare letter in a dialog is easy to hit by typing on, which is the accident the
  decision is about; it also depends on the keyboard layout and on an input method being off.
- *Change Enter for every in-app dialog.* The quit and close questions are answered with Enter today (by users and by
  `editor_daily`, `tab_lifecycle_keys`); nobody decided to change them.
- *An index past the end falls back to the first button.* A dialog asks for this so that Enter is NOT its first
  button; past the end is taken as the last button, the one Escape presses.

**Risk.** Low. Other dialogs: the only change is that Tab and Shift-Tab are now bound while one is up and do nothing
(before, they were not bound in the dialog and went wherever an unbound key goes). The platform-native macOS alert
does not look at `enter_button`; the overwrite question never uses it. The dialog text does not say "Tab"; the accent
on Cancel is the only hint (the text is English only already, `r16-editor.md` "Left open").

## 2. Passive code diffs are gated in ssh sessions like #2094 (#2097)

**Today.** `app/src/ai/blocklist/passive_suggestions/legacy.rs` `maybe_generate_passive_code_diff` runs after every
prompt suggestion (the static one and the model's, both in `generate_prompt_suggestions`). When the suggestion names files
(`coding_query_context`) it reads them from the local disk and sends them, with the finished block, to the model
provider (`BlocklistAIController::send_passive_code_diff_request`, an `AutoCodeDiffQuery`). Its only session check is
`SessionType::WarpifiedRemote` (skip; unknown = skip). A plain `ssh` that was bootstrapped the legacy way
(`Session::is_legacy_ssh_session`) is not that type, so there the request is built: paths taken from remote output,
read on the local disk, sent without the user asking. #2094 left this for a decision (`r16-agent2.md` section 3).

Two facts that bound it: the static suggestions carry no files today (`static_prompt_suggestions.rs:228`,
`coding_query_context: None`), so in practice the diff follows a model-made suggestion, and #2094 already stops that
request in an ssh session unless `prompt_suggestions_in_ssh_enabled` is on. So after #2094 the hole is "the user turned
prompt suggestions on for ssh and thereby also got file contents sent". The gate below closes it whatever starts the
diff. The other passive path (`passive_suggestions/maa.rs`) only runs with the `prompt_suggestions_via_maa` cargo
feature, which no build of ours enables; it is not changed (see Left open).

**Chosen.** The same decision, `active_ai::allowed_for_session(in_ssh_session, allowed_in_ssh)`, with a third
setting in the same family: `agents.warp_agent.active_ai.code_suggestions_in_ssh_enabled` (default false, not synced;
a switch under Suggested Code Banners on the AI settings page, greyed while its parent is off; en, ja, zh-CN). The
session is the pane's active one (`self.active_session.session(ctx).map(is_ssh_session)`); no session is `None`,
which the rule treats as ssh. The check sits after "is there anything to send" and before the file read, so nothing
is read either. A refusal emits `PassiveCodeDiffFailed { SSHRemoteSession }`, the event the existing remote skip
emits, so the prompt suggestion's banner drops its "code is coming" state and stays usable. The setting is the kill
switch: on restores the behaviour before.

**Rejected.**
- *Reuse `prompt_suggestions_in_ssh_enabled`.* That switch says "send the command and a summary of its output"; file
  contents are a different thing to agree to.
- *No setting, always off in ssh.* The decision says "exactly like #2094", which has a switch.
- *Gate at `passive_code_diffs_enabled`.* It has no session, and is also the settings-page question "is the feature
  on"; the ssh rule is about one request.

**Left open.**
- `maa.rs` (compiled, never enabled) sends one request for prompt suggestion and code diff together and has neither
  the #2094 nor this gate. If the cargo feature is ever switched on, both gates have to go in first.
- No app case (see `docs/testing.md`): it needs a fake-model reply that names files, on a plain bootstrapped ssh pane.

**Risk.** Low. A local pane whose active session is not known yet gets no passive code diff until it is (the same
trade as #2094). Not compiled: the new settings macro entry and `is_ssh_session(&session)` on an `Arc<Session>` are
the compile doubts.

## Tests

- Unit: `app/src/workspace/native_modal_tests.rs` (no selection = Enter on the first button; a selection; Tab and
  Shift-Tab wrap; past the end), `app/src/code/save_conflict_tests.rs` (`enter_is_cancel_never_overwrite`,
  `one_tab_from_the_enter_button_is_overwrite`, `the_kill_switch_puts_enter_back_on_overwrite`; the button-order test
  keeps its assertions, its comment no longer says Enter is the first button),
  `app/src/ai/agent_providers/active_ai/mod_test.rs`
  (`a_passive_code_diff_is_not_requested_in_an_ssh_session_unless_allowed`), `app/src/settings/ai_tests.rs`
  (`passive_code_diffs_in_ssh_default_off`).
- App case `editor_daily` (`cases_r15_w5a.py`, PROVING), changed expectation because the owner decision changed the
  behaviour: it used to press Enter to overwrite. Now: save asks; Escape cancels; save asks; Enter ALONE must cancel
  with the disk untouched 1 s later (`enter_alone_verdict`, new assertion); save asks; Tab then Enter overwrites with
  the editor's text. `close_editor` (macOS) answers with Tab then Enter.
