# Round 15: the defects the round 14 cases found (#2079, #2080, #2081, #2082)

Branch `fix/r15-defects`, from `integ/r14` 75847d84c. Nothing was compiled and no app case was run
when this was written: the code is worked, unproven. Not touched, by instruction:
`terminal/input.rs`, `terminal/input/enter_queue.rs`, `terminal/view/give_up_to_pty.rs`.

All three app behaviours are byte-identical at the upstream pin (`42effe840`): `meta-f` / `meta-b`
only (`editor/view/mod.rs`), no shifted scroll key (`terminal/view/init.rs`), the PRIMARY fallback
(`warpui/.../linux/clipboard.rs`). They are recorded in `DECLINED.md` as IMPROVED.

## 1. #2079: Alt-B / Alt-F / Alt-Backspace at the prompt, Linux and Windows

**Cause.** The input editor bound word motion to `meta-f` / `meta-b` (two fixed bindings and the
editable `editor_view:move_forward_one_word` / `editor_view:move_backward_one_word`,
`editor/view/mod.rs`) and word-cut to `meta-backspace`. Off macOS a key event never carries
`meta` (`warpui/.../winit/event_loop/key_events.rs` builds every keystroke with `meta: false`;
only `apply_extra_meta_keys`, default off, sets it), and `alt-b` does not match `meta-b`. So the
readline word keys did nothing at the prompt with default settings, while a program in front
already got `ESC b` / `ESC f` (`meta_keystroke_to_escape_sequence`). Alt-D was fine (`alt-d` on
every platform). Same class as #1989 (Alt-.).

**Fix.** The #1989 approach, in the same place (`util/bindings.rs`):

- the two editable word-motion bindings default to `alt-f` / `alt-b` on Linux and Windows and stay
  `meta-f` / `meta-b` on macOS;
- the fixed `meta-f` / `meta-b` / `meta-backspace` bindings stay on every platform, so an Alt key
  set to act as Meta loses nothing;
- a fixed `alt-backspace` cuts the word before the cursor off macOS (the action `meta-backspace`
  has, readline's `backward-kill-word`; it uses the editor's own kill buffer, not the system
  clipboard). On macOS `alt-backspace` already is the default of `editor:delete_word_left`;
- the code editor and the notebook editor register bindings of the SAME two names with `meta-f` /
  `meta-b`. They get the same defaults, plus fixed `meta-` fallbacks they did not have, so one
  binding name does not carry two different defaults.

Kill switch `PHOSPHOR_ALT_WORD_KEYS=0`, read when the bindings are registered: the defaults are
the Meta forms again and `alt-backspace` is not registered.

**Rejected.** (a) Mapping Alt to Meta in the key event off macOS: it would change every `alt-`
binding of the app and the pty encoding. (b) Changing Ctrl-A / Ctrl-E / Ctrl-Y to readline off
macOS: those defaults are deliberate in the source and an owner question (the issue says so).

**Risk.** Low. `alt-f` / `alt-b` / `alt-backspace` were unclaimed off macOS (grep over `app/src`;
the unit test asserts nobody else claims them). A layout that types a character with a plain Alt
is not affected on Linux or Windows (AltGr is `ctrl-alt` on Windows and a different modifier on
Linux). A user who had bound `alt-f` to something else keeps their binding: a custom trigger wins.

## 2. #2080: a key scrolls the scrollback while a foreground command runs

**Cause.** `TerminalView::page_up` / `page_down` / `move_home` / `move_end` (`terminal/view.rs`)
write the key to the program while a command runs (`nav_key_goes_to_pty(is_long_running() && ..)`)
and no other default key scrolls the block list: `terminal:scroll_up_one_line` has no key, there
was no Shift+PageUp. Only the wheel and the scrollbar read back through a build.

**Fix.** Four fixed bindings of the terminal view (`terminal/view/scrollback_keys.rs`,
registered in `init.rs`): `shift-pageup`, `shift-pagedown`, `shift-home`, `shift-end` dispatch
`TerminalAction::ScrollbackKey`, whose handler makes the scroll update the plain key makes at a
prompt (`AfterPageUp` / `AfterPageDown` / `AfterHome` / `AfterEnd`) through
`update_scroll_position_locking`, the key-path-safe entry (a busy model defers the update; the
key never waits for the pty reader). Nothing is written to the pty.

Where they are live (one predicate, unit-tested):
`Terminal & !IMEOpen & LongRunningCommand & !AltScreen & !EditorFocused`.

- `LongRunningCommand` is not set in the alternate screen (`keymap_context`), and `!AltScreen` is
  stated anyway: a full-screen program keeps the shifted keys exactly as before.
- `!EditorFocused`: at a prompt, or with the rich input of a CLI agent open, Shift+Home /
  Shift+End select text in the editor.
- A program that enabled the kitty keyboard protocol without leaving the block list still has a
  scrollback behind it; the terminal's binding wins there, as in other terminals.

The plain keys are unchanged in every state. The next key written to the program snaps the view
back to the bottom, as after a wheel scroll (`AfterWriteUserBytesToPty`, existing).

One trace-only log line: `hop=scrollback_key key=<page_up|page_down|top|bottom>` (input-order
log, debug; a fixed word). The app case reads it; a unit test pins the format.

Kill switch `PHOSPHOR_SCROLLBACK_KEYS=0`: the bindings are not registered, so the keys take
exactly the path they took before.

**Rejected.** (a) Changing what the plain PageUp does under a running command: every program that
reads it would lose it. (b) Editable bindings: an editable binding shows in the keybinding
settings and can be rebound to a key that must reach the pty; fixed is the smaller change, and
the settings entry can follow if the owner wants it. (c) The shifted keys in every state (the
issue's wording): at a prompt the editor has Shift+Home / Shift+End, and PageUp / PageDown
already scroll there; the task is the foreground command. Left for an owner decision.

**Risk.** Low to medium. A program outside the alternate screen that read Shift+PageUp /
Shift+Home (rare: these are the keys other terminals keep for themselves) no longer gets them
unless the kill switch is set. Whether the terminal view, not the editor, has the key focus under
a running command in every pane state (agent view included) is read from the code, not run: if
the editor holds focus the keys do nothing new. The new `TerminalAction` variant is added to the
four exhaustive matches (enum, `Debug`, accessibility, `handle_action`).

## 3. #2081: a selection written to CLIPBOARD with copy on select off

**Cause.** `SelectionSettings::maybe_copy_on_select` (`settings/select.rs`) asks for a PRIMARY
write on every selection and a CLIPBOARD write only when `terminal.copy_on_select` is on. Both
PRIMARY writers in `warpui/.../linux/clipboard.rs` (`write_to_primary_clipboard`, and the
`ClipboardTarget::Primary` arm of the background writer `write_for_target`) wrote the normal
clipboard instead when arboard answered `ClipboardNotSupported` (Wayland without the
primary-selection protocol, `lib/arboard/.../wayland.rs`). So on such a display every selection
replaced what the user had copied.

**Fix.** One decision, `primary_write_outcome(result, legacy_fallback)`: `ClipboardNotSupported`
is `Dropped` (one info line per process), other errors are failures as before. The only caller of
a PRIMARY write is a text selection, and with copy on select ON the same caller writes CLIPBOARD
itself, so the fallback never had anything to add. The background writer reports a dropped write
as done (not `Failed`: there is nothing to warn about per selection).

Kill switch `PHOSPHOR_PRIMARY_FALLS_BACK_TO_CLIPBOARD=1` restores the fallback.

**Not changed.** The read side (`read_from_primary_clipboard` falls back to CLIPBOARD, so a
middle click pastes the clipboard there): it destroys nothing; the issue calls it a separate
question.

**Risk.** Low. On X11 and on compositors with the protocol nothing changes. Not checked: which
compositors in real use lack the protocol (sway, GNOME and KDE have it).

## 4. #2082: the find hook in Linux and macOS runs

**Verdict: TEST fault, half already worked.** Linux: `e9018de13` (round 14, #2069) exports
`PHOSPHOR_APPCI_FIND_FILE` in `app.INNER` and enables `warpui::appci_find=info`; confirmed by
reading and pinned by the self-test. macOS: `MacApp.start` enabled the log target but never set
the variable, so the hook was off. It now passes `find_hook_env(home)` through `open --env`.
`APPCI_NO_FIND_HOOK=1` leaves it unset (the same switch puts Linux positions back on OCR).

No macOS case reads the hook yet: the macOS harness has no pointer tool and
`cases_ui._hook_word` answers on Wayland and X11 only (a macOS reader would need the window
origin and the Retina scale). The change makes the app log positions there; nothing consumes
them, so no passing macOS case can change.

## 5. Tests

Unit: `util/bindings_tests.rs`, `terminal/view/scrollback_keys_tests.rs`,
`terminal/view/scrollback_keys_view_tests.rs`, `clipboard_tests.rs` (`primary_write_tests`).
App cases and self-tests: `docs/testing.md`, the round 15 section. `meta_and_word_keys` expected
Alt-B / Alt-F to fail with "known: #2079"; it now expects them to pass (the expected line is the
same) and has an Alt-Backspace step. `scroll_keys_foreground` and `primary_absent_clipboard_kept`
are new, in `cases.PROVING` only.
