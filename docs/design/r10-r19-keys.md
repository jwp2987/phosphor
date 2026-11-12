# Round 19: the key cases of the round 14 candidate

Branch `fix/r19-keys`, from `integ/r15` 4da375d0f. Candidate under test: build/r14a `42ef67839`, pipeline 3052606915.
Nothing was compiled and no app case was run when this was written: the code is worked, unproven. None of these cases ran in
rounds 10 to 13 (fix tier; `test-results/` has no row for them).

Evidence used: the job traces, `results.json` / `results.jsonl` of the three app jobs, and the macOS job's per-case app logs
and shots. The Wayland job uploaded no shots and no app logs (its `after_script` timed out, `run_slim` was never made) and
the Windows artifact holds none either, so a Wayland-only or Windows-only failure could be judged only from its result row
and the code.

## 1. scroll_keys_foreground (Wayland both modes, macOS, Git Bash both modes)

Three different things under one FAIL.

**1a. "Shift+Home with text in the input: the shell received `echo sfgold...echo sfgnew...`" is a TEST fault.**
`cases_r15_defects.py` `_sfg_prompt_half` called `_sfg_prompt_press(ctx, off, key("home", True), 0)`. The fourth argument is
the press count (`ctx.be.keys([spec] * n)`), so the step sent no key at all, and the next typed line was appended. macOS app
log, `case_scroll_keys_foreground.log`: the last character of the first line at `t=...837544`, the first character of the
second at `t=...838302`, and no `os_key_event` / `keydown` between them. Not an input bug: nothing was glued, two lines were
typed one after the other into the same input with no key between.
Fix: `sfg_press_plan(spec, n, want_hops)`; a press count below 1 raises; the step is one press that must log no hop. The
oracle is unchanged and can fail: an app that does not select leaves both lines in the input.

**1b. Shift+PageUp with a full-screen program in front wrote nothing: APP, #2105.**
`crates/warp_terminal/src/model/escape_sequences.rs` `to_escape_sequence` encodes modified arrows / Home / End
(`cursor_movement_keystroke_to_escape_sequence`) and modified Delete, but had nothing for Page Up / Page Down with a
modifier; the plain keys are fixed bindings (`terminal/view/init.rs`) that a shifted chord does not match. macOS log:
`keydown key=pageup shift=true`, `kh_pane_key class=Control target=Terminal`, then nothing. The pin is the same.
Fix: `page_keystroke_to_escape_sequence`: `CSI 5;<mods>~` / `CSI 6;<mods>~`, placed after the Meta rule so a Meta chord
keeps its ESC-prefixed form. Bindings are asked before the encoder (the run shows it: Shift+Home under a running command
logged `hop=scrollback_key key=top` and wrote no byte although Shift+Home has an encoding), so the scrollback keys and the
tab keys are unchanged. `DECLINED.md` IMPROVED.
Rejected: also encoding Shift+Insert (`CSI 2;2~`): Shift+Insert is paste on Linux and nothing in the run asks for it.

**1c. macOS only: Shift+Home with a full-screen program in front wrote nothing: APP, #2106.**
`platform/mac/event.rs` passes `NSEvent.characters` as the key down's text; for a function key that is a private-use code
point (Home U+F729). `core/app.rs` `offer_pane_key` called any non-control text printable, so `will_type` was true, and
`terminal/view.rs` `terminal_key_down` returns early for such a key (kitty protocol off, no Ctrl / Alt / Cmd / Meta) to let
the typed event be the key. No typed event follows a function key; the elements skip a key the sink has seen. Wayland wrote
`1b5b313b3248` for the same key.
Fix: `pane_key::key_down_will_type` leaves U+F700..U+F8FF out. Linux and Windows send no text with these keys.
Risk: low; the only reader of `will_type` is that early return. With the flag false the key goes to the encoder, and a key
the encoder has nothing for is returned to the elements as before.

## 2. scroll_keys_position

**macOS terminal: TEST fault.** "numbered lines on screen 362..906 (before 466..500)". 500 lines exist. `shots/terminal_skp_1_pageup.jpg`
shows 362..397: the view moved. 906 is `nwpenr`, line 394's word `nwpcnr` with `c` read as `e`. The verdict used the highest
number read. Fix: `skp_visible(text, tag, hi)` drops numbers outside 1..hi. A view that did not move still reads 466..500.

**Wayland agent: not judged.** "block-up then Home did not move the view above where PageUp left it (462..500, before
317..363)" and "after one typed key none of the job's last 8 lines is on screen". Terminal mode passed in the same run. No
shot and no app log were uploaded. The numbers say the view sat at the end of the `cat` block with no stream line on screen
after block-up and Home, and was not at the bottom after a typed key. Whether block-up selects another block in the agent
view, or the typed key does not end a block selection's scroll there, needs the two shots. No change.

## 3. meta_and_word_keys (Wayland terminal, Git Bash both modes): TEST fault

"Ctrl-Left with a program in front: the pty received 1b62, expected 1b5b313b3544". `terminal/view/init.rs` has two editable
bindings, in the initial upstream import: `terminal:executing_command_move_cursor_word_left` / `_right`, `ctrl-left` /
`ctrl-right` (macOS `alt-left` / `alt-right`), context `Terminal & !IMEOpen & LongRunningCommand`, action
`ControlSequence(EscCodes::WORD_LEFT / WORD_RIGHT)` = `ESC b` / `ESC f`. The case's table was written from xterm's encoding.
The app does what its binding says; the 13 other raw keys of the row matched.
Fix: the table expects the binding's bytes; the self-test pins the constants and the bindings.
Not decided here: whether Phosphor should send xterm's `CSI 1;5D` instead (a divergence from upstream; owner).
The row's "lost-before-app 1" is one key of the raw half the app's key trace does not count; every raw key's bytes arrived,
so no key was lost. Not looked into further.

## 4. cb_ui_action_latency (Wayland terminal): TEST fault

"Tab completion: no changed frame within 6 s". The same stage's effect proof passed (the completed line ran, so the
completion happened). The detector wants 0.05 % of the screen's pixels changed; the completion drew `_dir/`, five glyphs.
Fix: the directory ends in 56 dense glyphs; threshold unchanged; the largest change seen is reported on a miss.
Rejected: lowering the threshold (a cursor blink would pass it).

## 5. history_up_recall (Windows agent, PowerShell pane): not decided, candidate APP #2107

Step (b), Up + 3 BackSpaces + `e9z` + Enter: the shell received the recalled line with `e9z` appended and nothing deleted;
step (c)'s Enter went to the agent. Passed in terminal mode, in the Git Bash pane and on Linux.
Candidate, read not run: `inline_history/data_source.rs` lists up to 5000 entries inline and builds a longer history in the
background (#874); `inline_history/view.rs` `KeyGate` holds only Up / Down / accept while the list is pending. BackSpace on
the still-empty input does nothing, the recalled line arrives, `e9z` is appended: the observed line. A PowerShell pane loads
the PSReadLine history file.
Not fixed here: the fix holds editor keys behind a pending recall, which is the input's key path (`terminal/input.rs`) and a
second patch on #874's path: design first. The case now records the history size.

**Round 27 (#2107): decided and done.** Cause as read above, confirmed: a history above `INLINE_BUILD_MAX_ENTRIES` is listed
on a background thread, so Up opens the menu with no rows and the recalled line enters the buffer only when the list lands and
its first row is selected (`SelectCommand`); `KeyGate` holds only Up / Down / accept, so BackSpace, text and Enter acted on the
empty buffer first and the recalled line then replaced them.
Fix (Option A, one queue, no second one): `GateKind::HistoryRecall` on the pane's input queue.
- The menu emits `InlineHistoryMenuEvent::ListPending` from `open_with_current_buffer` when the new query's list is pending
  (`is_pending()`), after the mode change has run the query; a small history answers inline and never emits it, so its Up stays
  instant. The input opens the gate then (`input/history_recall.rs`, same pattern as `open_enter_gate_with`).
- The queue holds every editor key behind the gate as `EditorKey` entries in the existing FIFO; keys with no replayable form
  (Escape, Vim Escape, the page keys ...) are queued behind it too, so none overtakes an earlier key. The H8 matrix has no
  exemption for this gate (`table::gate_holds`).
- Closing events: the first row's `Select*` event (released from the input's handler after the buffer is set; there is no separate
  list-ready event: one would reach the input BEFORE the row's select event, which travels two more event hops, and a "list ready"
  release would replay the keys onto the empty buffer again), `NoResults`, and the menu leaving its mode (`ModeChanged`: Close, Escape, an
  accept). The drain replays the held keys in order; Up / Down then pass through the ready `KeyGate`.
- Replay order: the effects of an Enter, Escape, Up, Down, Tab or ^C are events delivered after the update that applies the key,
  so the drain yields after such a key (`EditorKeyPayload::new_with_deferred_effects`, `Host::yield_after` on both hosts). Without
  that, a key typed after an Escape was applied before the Escape's buffer restore ran and was overwritten by it.
- ^C (coordinator decision): no Enter held ahead of it: the recall is cancelled (the queue's `DiscardThenSignal` drops the edits
  typed meanwhile, the menu closes, a list landing later is ignored), the ^C acts at once; an Enter held ahead: the ^C is queued in
  order behind the held keys and the Enter, whose line runs first. The hop line `history_recall_ctrl_c` names the choice. The pre-Up
  buffer is not restored: ^C clears the line, as it always did with the menu open.
- Give-up net: 125 s (the mixer waits at most 120 s), WARN, menu closed first (no restore), then the keys replay in order on the
  unchanged buffer. Kill switch `PHOSPHOR_HOLD_KEYS_BEHIND_RECALL=0` restores the old behaviour exactly.
- Evidence: Rust tests in `input_queue/tests.rs` and `terminal/input_test.rs`; `history_up_recall` seeds 6000 lines on Linux and
  macOS so step (b) runs on the background path (`docs/testing.md`, Round 27). Rejected: holding in `KeyGate` (a second queue, and
  the menu would need to know editor keys); a timer to wait for the list (a timer must not decide).

## 6. keybinding_rebind (Git Bash both modes): not decided

Arm B "ctrl+shift+F9 did not open command search" and arm C "Alt-R did not open the history search"; controls passed; the
tab arms passed; Wayland passed everything. Both failures are the first open of command search in a freshly launched app,
read 1.2 s after the key. No log, no shot. Change: wait for the view's frame (8 s give-up net) and report the key's fate.

## 7. ime_compose (Wayland both modes): UNVERIFIABLE as run

The case has no dead-key arm. `nihao` reached the pty letter by letter and the editor got `nihao `: the app saw plain key
presses (its key trace counted all 337), so the input method was not in the key path, although `fcitx5-remote -n` answered
`pinyin`. The app enables its text input unconditionally (`windowing/winit/window.rs` `set_ime_allowed(true)`). Whether
fcitx5 was the compositor's input method cannot be read from the artifact. Change: read fcitx5's focused input contexts
before typing; none is UNVERIFIABLE.

## 8. focus_cycle (Git Bash both modes): TEST fault

`ref_terminal_windows` starts the Git Bash reference with the pane's `~/.bashrc`; its hook appends to the pane's `rec.log`.
`app_keys_while_away: 0` in the same row. Fix: the reference's own rc for this case.
