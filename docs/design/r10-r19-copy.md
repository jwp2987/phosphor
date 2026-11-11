# r19-copy: round 14 candidate, the copy / window / pointer rows

Round 14 candidate: build/r14a `42ef67839`, pipeline 3052606915. Branch `fix/r19-copy` from integ/r15 `4da375d0f`.
Nothing here was compiled or run. Evidence is the job traces, the job artifacts (results, the macOS screenshots and app logs)
and the code. The Wayland job uploaded no screenshots and no app logs (`run_slim/shots`, `run_slim/logs`: "no matching files"),
the Windows job no app log, so those rows rest on the result rows and the code.

All rows but `ui_copy_wrapped_output` / `ui_copy_wrapped_drag` are new in round 14 (no row in `test-results/` up to r13a). Those
two passed on Wayland agent in r13a.

## Verdicts

| case | platform | verdict | root |
|---|---|---|---|
| `resize_reflow_output` (copy after reflow, stream copy) | Wayland both, macOS | TEST | R1 |
| `resize_reflow_output` (vim size) | Wayland terminal + agent, macOS | TEST | R2 |
| `ui_copy_wrapped_output` | Wayland agent | TEST | R1 |
| `minimize_fullscreen` (`mf_after_return` 0/3) | Wayland agent | TEST | R1 |
| `ui_copy_wrapped_drag` | Wayland agent | not decided (no log); expected to follow R1 | R1? |
| `pane_resize_maximize` | Wayland both | APP, #2119 | R3 |
| `selection_modes` | Windows Git Bash both | APP #2117 and TEST | R4 |
| `job_control` | macOS | TEST | R5 |
| `flood_scrollback_find` | macOS | TEST | R6 |
| six pointer cases, `mptr exit 4` | macOS | TEST (tool) | R7 |
| `minimize_fullscreen` (fullscreen off) | macOS | not decided; made decidable | R8 |

## R1. A selected block stays selected; the copy helper assumed it does not

`cases_window_use.copy_last_block_output` = block-up, copy-output, then `back_to_input` (the focus-input key), on the stated
assumption that focus-input "also clears the selection" (`cases_wrap_vtabs.KX` docstring).

- `app/src/terminal/view.rs` `focus_input_box`: selected blocks are cleared only when `!FeatureFlag::AgentView.is_enabled()`.
  With the agent view feature on a selected block is context for the next agent query and is kept. By design (upstream comment).
- So the second block-up moves one block up from the block still selected. macOS `shots/terminal_rr_copy_base.jpg`: the `cat`
  block is selected. `shots/terminal_rr_copy_narrow.jpg`: `stty size > vw_size_rr.txt` is selected, one above; its output is
  empty: "0 chars on the clipboard". The stream copy two blocks later: "0 of 700 lines".
- A selected block is a pending context block; `BlocklistAIInputModel::is_autodetection_enabled_for_current_context` returns
  false while `has_locking_attachment()` (pending context blocks). In the agent view the input type then stays what it is, AI
  after a submit: `enter_route` of Wayland agent `ui_copy_wrapped_output` is `[Shell,Shell,Shell,AI,AI,AI]` and of
  `minimize_fullscreen` `[..., AI,AI,AI, ...]`, exactly the burst lines typed after a block copy. By design (upstream
  QUALITY-544: "Lock NLD for block/image/file-attached agent-view entry").
- Why r13a passed the same case in agent mode is not known (no log from either run).

Fix (test): `cases_wrap_vtabs.leave_block_selection`: the user's way out of a block selection, the block-down key
(`select_more_recent_block` with the newest block selected and visible: `clear_selected_blocks` + focus input;
`navigate_agent_transcript` Next past the newest item: the same). `copy_last_block_output` and `ui_copy_wrapped_output` press it twice after
their copy, before focus-input (the first press only scrolls when the newest block is not fully in view; a press with nothing
selected does nothing, both code paths return), and three times before their block-up, for a block another case left selected.
On macOS the chord is a menu key equivalent (`hop=keydown_menu` in the logs), like block-up. `back_to_input` itself is not
changed (many cases call it with a program in front). The oracle is unchanged and can fail as before: the copies are compared
byte for byte.

Rejected: Escape (in the agent view it leaves the view); a click in the input (pointer on three platforms); an app change so
that focus-input clears block selections (reverses an upstream design decision: an owner question, listed in the report).

## R2. vim's size was compared with the size after vim had exited

macOS app log, `case_resize_reflow_output.log`: `alt_screen_enter`, `grid_resize rows=40 cols=116`, after the window resize
`rows=37 cols=101`; after `:q!` `rows=36 cols=97`. vim wrote `37 101`. The case compared it with `stty size` typed after vim
quit (36, 97). A full-screen program's grid is larger than the block list's (less padding): vim was told the right size.

Fix (test): vim's `&lines &columns` must equal the app's grid from the `hop=grid_resize` line the resize produced while vim
was open (`apply_size` already returns it). No hop: UNVERIFIABLE, as elsewhere in the case. It fails when vim is not told of
the resize (it would report the 40x116 it started with).

## R3. APP #2119: a release over another pane moves the keyboard there

`pane_resize_maximize` finds the divider by dragging 120 px left from 1, 2, ... px left of the right pane's text. Ten drags
began in the right pane's padding and ended over the left pane; the eleventh was on the divider (`divider_px_left_of_text: 11`).
After it the keyboard was in the left pane (the probe answered with the left shell's pid; Ctrl-C did not end the logger in the
right pane).

`BlockListElement::mouse_up` runs for every block list the release is over and dispatches `ClickOnGrid` and
`BlockSelectAction::MouseUp { should_redetermine_focus: true }`; `TerminalView::block_select` ends in
`redetermine_global_focus`. `mouse_down` already ignores a press in an unfocused pane; the release had no such rule.

Fix (app): the element records, for every left press it is shown (handled or not), whether it began inside the block list
(`left_press_began_in_block_list`, an `Arc<AtomicBool>` shared with the view like `rich_content_drag_selecting`); a non-synthetic
mouse move (no button held) drops it; `mouse_up` takes it. `left_mouse_up_is_foreign(press_began_here, pane_is_focused)`:
a release with no press of its own does nothing more to an UNFOCUSED pane. `BlockTextSelectAction::End` is still dispatched
first, so a selection always ends.

Rejected: ignoring every release in an unfocused pane (a fast click on an unfocused pane is released before the pane's focus
state is redrawn; that release must still count); gating in `block_select` on `mouse_down_block_index` (not set for every
press: `MouseDown(None)`, cards, the snackbar); gating the focused pane too (a press taken by a menu or a card and released over
the block list would change behaviour I cannot test here).

Risk: a stale "began here" when a press in the pane is never followed by a release the element sees; then one later foreign
release behaves as before the fix. No key path is touched.

The test is not changed: its check "after the divider drag the probe ran in the right pane's shell" is the proof.

## R4. APP #2117 and TEST: Ctrl-C with a text selection on Windows

`selection_modes` drags over a streaming program's line, copies, then presses Ctrl-C to end the program, with the selection
still up. On Windows that Ctrl-C is a copy by design (`ctrl_c_internal`, Windows arm). The program ran on
(`stream_ticks_around_drag: [3, 12, 15]`, no prompt in 8 s, the four lines typed next went into it).

- TEST: the case never pressed the Ctrl-C that interrupts.
- APP: after that copy `clear_selections_when_shell_mode_without_focusing_input` does nothing while the agent view feature is
  on, so the selection stays and every later Ctrl-C copies again. The alt-screen arm clears unconditionally.

Fix (app): `ctrl_c_copy_drops_text_selection(command_in_front)`: with a command in front the copy drops the text selection.
Idle prompt: unchanged (owner question in #2117). Fix (test): on Windows the case plays both: first Ctrl-C copies (clipboard
compared, the program still ticks), second ends the program. Other platforms: one Ctrl-C, as before.

## R5. `job_control` on macOS: BSD cat exits on `fg`

App log: `fg` submitted, `preexec` and `CommandFinished` 2 ms apart, no `pty_write` in between: the reader ended by itself.
On macOS a terminal `read` returns EINTR after stop + continue and `cat` exits ("Interrupted system call") in any terminal.
The second line and Ctrl-D then went to the shell prompt; Ctrl-D ended the shell, the last window closed, the app quit
("No windows left, terminating app"). Not a crash.

Fix (test): the reader is `sh jc_reader_<t>.sh <file>` (a `read` loop; sh retries EINTR), on Linux too (one path). After `fg`
the case checks the OS says the reader is running before it types the second line; if not it fails there and types nothing.

## R6. `flood_scrollback_find`: the tail rule matched the command header

`shots/terminal_find_after.jpg`: Find bar 1/1, the marker line highlighted on screen, no flood output line; the flood block's
header (`... do seq 1 500000; ...`) is clipped at the bottom. App: `match_line=34.61` of `viewport_lines=39.61`, `moved=true`.
The tail rule (a line of just 499999 or 500000) matched a fragment of the header.

Fix (test): the tail is a line of just 499999 or 499998. The command text holds neither. It fails as before when the view
stays on the flood's tail (those lines are then on screen).

## R7. `mptr exit 4`: the Dock's display-sized window

All six macOS pointer cases: "refused ... the topmost window there belongs to Dock (pid 90039, layer 20)", from the first
case, at points from x 408 to 830 and y 515 to 697 on a 1512x982 display, the same pid for 100 minutes. The screenshots show
nothing over the app. The Dock process keeps a see-through window the size of the display at the Dock window level; a press
there reaches the window below. `mptr`'s fence treated it as an obstacle. Not machine state.

Fix (tool): `ownerAt` skips a window only when all of this holds: owner `Dock`, layer = the Dock window level
(`CGWindowLevelForKey(.dockWindow)`), bounds covering the whole main display, and the point inside the main screen's
`NSScreen.visibleFrame` (the display minus what the Dock bar and the menu bar take), converted to the top-left origin. A point
on the Dock bar or the menu bar is still refused, and so is any other Dock-owned window. A refusal now lists every window at
the point, front to back, with bounds, so a wrong guess about the Dock's window is readable from the next trace.
`APPCI_MAC_PTR_IGNORE_OWNERS` is unchanged.

That the Dock's window is display-sized is inferred (the refusals, the screenshots, one pid for the whole run); the run kept
no window list. If it is not, the rule does not match, the clicks are refused as before and the new message shows the stack.

Rejected: `APPCI_MAC_PTR_IGNORE_OWNERS=Dock` in the job (drops the fence for the Dock bar, Launchpad and Mission Control).
Risk: written blind; `import AppKit` in a command-line tool. A compile error costs the pointer cases only (non-fatal loop in
`app_test.sh`), as before. An auto-hidden Dock: the visible frame is the whole display below the menu bar, and the bar is not
on screen.

Separate: the run ended on NEEDS_OWNER, a `UserNotificationCenter` dialog ("Allow Phosphor to find devices on local
networks?"), which appeared at `depth_check_off_before_editor_memory_7mb`, after every case here. Owner: click Allow once in
the gitrunner GUI session.

## R8. `minimize_fullscreen` on macOS: fullscreen off

`stty size` (56, 153) after the off request and at the end; (45, 124) before. `shots/terminal_minimize_fullscreen_FAIL.jpg`:
still fullscreen. App log: `grid_resize rows=54/56 cols=153` at fullscreen on, no grid change after. `Window.fullscreen(False)`
returns the osascript error and the case drops it. Refused request or app not leaving fullscreen: not decidable from this run.

Fix (test): the error text is kept and the window's `AXFullScreen` is read back. Request refused or attribute unreadable:
UNVERIFIABLE with the text. Attribute false and `stty size` not back: FAIL (app). Attribute still true after an accepted
request: FAIL naming that.
