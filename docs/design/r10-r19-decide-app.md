# Round 19, task `r19-decide-app`: three owner decisions of 2026-11-11, made in the app

Issues: #2130 (word keys), #2131 (Windows idle Ctrl-C copy), #2132 (editor tab close question by keyboard). Nothing here
was compiled or run. The decisions answer the owner questions the round 14 verdicts left open (r19-keys, r19-copy,
r19-tabs). All three are identical at the pin `42effe840` (`git show 42effe840:app/src/terminal/view/init.rs`,
`...:app/src/workspace/native_modal.rs`), so each has an `IMPROVED` entry in `DECLINED.md`.

## 1. Ctrl+Left / Ctrl+Right with a program in front (#2130)

Today. `app/src/terminal/view/init.rs`, bindings `terminal:executing_command_move_cursor_word_left` / `_right`
(context `Terminal & !IMEOpen & LongRunningCommand`; `ctrl-left` / `ctrl-right` off macOS, `alt-left` / `alt-right` on
macOS) carried `EscCodes::WORD_LEFT` / `WORD_RIGHT` = `ESC b` / `ESC f`
(`crates/warp_terminal/src/model/escape_sequences.rs`).

Decision. Linux and Windows: `CSI 1;5D` / `CSI 1;5C`. macOS: unchanged.

Fix. One pure function, `escape_sequences::word_key_sequence_for(left, mac, xterm_form)`, and
`word_key_sequence(left)` that reads the platform and the kill switch; the two bindings call it. The bytes are the
ones `cursor_movement_keystroke_to_escape_sequence` gives `ctrl-left` / `ctrl-right` when no binding claims the key (a
unit test holds the two together, also in application cursor mode, where only the plain arrows change). Kill switch
`PHOSPHOR_WORD_KEYS_XTERM=0` (read once per run).

The other modifier word keys of the same table (what I found; none changed, the decision covers the two above only):

| binding | key | bytes | xterm for the same key |
|---|---|---|---|
| `..._delete_word_left` | `ctrl-backspace` (Linux, Windows), `alt-backspace` (macOS) | `0x17` (Ctrl-W) | Ctrl+Backspace is `0x08` in xterm's default setup; `0x17` is readline's unix-word-rubout, so this one is also a readline form, not the xterm one |
| `..._move_cursor_home` / `_end` | `cmd-left` / `cmd-right`, macOS only | `0x01` / `0x05` | no such key off macOS; Home / End go through the encoder |
| `..._delete_line_start` / `_end` | `cmd-backspace` / `cmd-delete`, macOS only | `0x15` / `0x0b` | macOS only |
| no binding: `alt-left` / `alt-right` off macOS | | encoder: `CSI 1;3D` / `CSI 1;3C` | same |
| no binding: `ctrl-shift-left` / `-right`, `ctrl-up` / `-down`, `ctrl-delete` | | encoder: `CSI 1;6D/C`, `CSI 1;5A/B`, `CSI 3;5~` | same |

So after this change Ctrl+Backspace is the one Linux / Windows key of the table that still writes a readline byte in
place of what xterm sends. It deletes a word in every readline-style editor and in most full-screen programs that read
Ctrl-W, so it does less harm than `ESC b` did; whether to move it is an owner question (in the report).

Rejected. Removing the two bindings off macOS so the encoder answers: the result is the same bytes today, but the
bindings are editable (a user may have moved them to another key) and the kill switch needs a place. Changing the
macOS bytes: not decided.

Risk. A line editor with no binding for `CSI 1;5D/C` no longer moves by a word (bash's readline binds them by default
on current systems; a bare zsh does not). That is how xterm, Konsole and Windows Terminal behave there. No key is
lost, reordered or split: the same binding writes one sequence in place of another.

## 2. Windows: Ctrl-C copy at an idle prompt drops the selection (#2131)

Today. `TerminalView::ctrl_c_internal`, Windows arm (`app/src/terminal/view.rs`): with a block-list text selection the
key copies and returns. Since #2117 `ctrl_c_copy_drops_text_selection(command_in_front)` drops the selection with a
command in front; at an idle prompt `clear_selections_when_shell_mode_without_focusing_input` ran, which does nothing
while `FeatureFlag::AgentView` is on.

Decision. The idle copy drops the selection too (Windows Terminal).

Fix. `ctrl_c_copy_drops_text_selection(command_in_front, idle_clears)` = `command_in_front || idle_clears`;
`idle_clears` is the kill switch `PHOSPHOR_WIN_CTRL_C_COPY_CLEARS_IDLE` (on unless `0`). The idle branch still calls
the old clear afterwards, so selected blocks are treated exactly as before (kept as agent context with the agent view
feature on, cleared otherwise). The #2117 test asserted "kept at an idle prompt"; that expectation changed with the
decision and the test was split (command in front: unchanged; idle: now dropped; switch off: kept).

Rejected. Clearing selected blocks too: Windows Terminal has no such thing, and blocks are agent context by design.

Risk. A text selection copied with Ctrl-C is no longer attached to the next agent query. Windows only (`cfg(windows)`
arm; only the Windows jobs compile it).

Seen while reading, not changed: in the agent view an idle Ctrl-C goes through `handle_ctrl_c_input_event`, which can
take the key as the view's exit confirmation before `ctrl_c` (and so before the copy) runs. Whether Ctrl-C with a
selection copies there on Windows was not run; the app case records it (a note, not a verdict).

## 3. Editor tab close question by keyboard (#2132)

Today. `code/view.rs` `remove_tab_with_confirmation` builds the question through `quit_warning`
(`QuitWarningDialog::build`: Save, Don't Save, Cancel). On Linux and Windows it is the in-app dialog
(`workspace/native_modal.rs`): Enter = first button, Escape = last. The keyboard selection (`enter_button`, Tab /
Shift+Tab) exists since the overwrite question but only a dialog that asks for it has one.

Decision. Tab / Shift+Tab move between the buttons, Enter / Space press the focused one, Escape stays Cancel, focus
ring.

Fix.
- `quit_warning::close_question_enter_button(is_editor_tab, save_index, enabled)`: the editor tab's question asks for a
  selection that starts on Save, so Enter alone is what it was. Kill switch `PHOSPHOR_CLOSE_QUESTION_KEYS=0`.
- `native_modal`: `space` is bound to `NativeModalAction::PressSelected`, which presses the selected button and does
  nothing in a dialog without a selection (`space_button_index`). The selected button gets a 2 px border in the
  theme's foreground colour through the button's own style (`has_focus_ring`), so it holds in the hover and pressed
  states too; the accent fill still marks it as before.
- Trace for the app tests, `hop=native_modal`: `event=selected button=<i>`, `event=pressed button=<i>
  by=<pointer|enter|space|escape>` (indexes only).

Space and the ring apply to both dialogs that have a selection (this one and the overwrite question, where Space on the
initial selection is Cancel, like Enter). Other questions (pane, tab, window, quit) are unchanged: no selection, no
ring, Space does nothing.

Rejected. A selection for every in-app question: the decision names this one, and "Yes, quit" under a Tab walk is a
different question. Starting on Cancel as the overwrite question does: Enter alone would stop being Save. A wrapping
container for the ring: it changes the dialog's layout for the selected row only.

Risk. Low. Enter and Escape answer as before; Tab was a no-op. The ring's colour and the 2 px border inside a 240x40
button were not looked at on screen (the case takes the screenshot `edd_close_dont_save`). No timer anywhere.

## Tests

Unit: `escape_sequences_test.rs` (3), `view_test.rs` (3, one of them the changed #2117 expectation),
`quit_warning/mod_tests.rs` (3), `workspace/native_modal_tests.rs` (4). App cases: `meta_and_word_keys` (raw half, per
platform), `selection_modes` (Windows: idle Ctrl-C arm), `editor_daily` (Linux, Windows: Don't Save by Tab, Tab,
Shift+Tab, Space; then Enter alone is Save). Self-tests: `selftest_r19_decide.py` and the pins in `selftest.py`.
`docs/testing.md`: "Round 19: owner decisions of 2026-11-11 in the app".
