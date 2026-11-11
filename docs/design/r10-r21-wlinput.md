# Round 21, Wayland input: five cases of the round 15 targeted run (#2139, #2140, #2141, #2142)

Run: `app-test-linux-wayland` job 17936839261, build/r15a `b06800e98`, pipeline 3054716634 (58 PASS, 16 FAIL, 20 NO-REFERENCE).
Evidence: the job's `run_slim` logs and screenshots (kept since #2123) and `run/results.jsonl`. Nothing here was compiled or run.

| case | mode | verdict | issue |
|---|---|---|---|
| `primary_and_middle_click`, `[copy_on_select off]` middle click | both | APP (upstream's defect, same line at the pin) | #2139 |
| `primary_and_middle_click`, `[copy_on_select on]` rows | both | TEST | #2141 |
| `link_and_path_click` | terminal FAIL, agent NOREF | TEST (both rows) | #2141 |
| `ime_compose` | both | TEST | #2141 |
| `desktop_notification_agent_question` | agent@fake | APP (upstream's defect, same call sites at the pin) | #2140 |
| `subshell_ready_relay_shape` | both | TEST; an owner question filed | #2141, #2142 |

## 1. A middle click pastes CLIPBOARD's text (#2139): APP

**Row.** `[copy_on_select off]`: PRIMARY held the double-clicked word (`wl-paste --primary`), CLIPBOARD held `SENTINEL_NOT_THE_PASTE`;
`echo ` + middle click in the input + ` >> file`: the shell received `echo SENTINEL_NOT_THE_PASTE >> file`.

**Cause.** `crates/warpui/src/windowing/winit/linux/clipboard.rs`, `read_from_specific_clipboard_inner(clipboard_kind, ..)`: the text
came from `read_text_retrying()`, which called `self.inner.get().text()` with no `.clipboard(kind)`: always CLIPBOARD. Only the HTML
probe named the selection (the log line of the click is its failure: `Unable to read HTML from clipboard: ContentNotAvailable`). The pin
(`4111d08f9`) has `let text_result = self.inner.get().text();` beside `.clipboard(clipboard_kind).html()`. X11 and Wayland alike; every
middle-click paste (input editor, terminal, plain editors) reads through it.

**Change.** `read_text_retrying_from(kind)` reads the selection asked for and that selection's pending write
(`text_selection_for`, `pending_target_for`). A PRIMARY read no longer runs the image probe, which reads CLIPBOARD only
(`probes_images`). An empty PRIMARY (`ContentNotAvailable`) pastes nothing and is not a warning. Kill switch
`PHOSPHOR_PRIMARY_READS_CLIPBOARD_TEXT=1` (the text from CLIPBOARD again).

**Not changed.** On a display with no PRIMARY the READ still falls back to CLIPBOARD (`ClipboardNotSupported` arm); #2081 changed the
write side only and left this as a separate question.

**Rejected.** Fixing it in `SelectionSettings` (a text-only read kind for the middle click): the fault is in the platform read, and the
terminal's and the plain editors' middle clicks share it.

**Risk.** Low: one selection argument. A person who relied on a middle click pasting the last COPY gets the last SELECTION, which is
what the setting says. Not compiled.

## 2. `[copy_on_select on]` rows of the same case (#2141): TEST

The `off` arm's own `echo ... >> file` adds a block; the fixture line moves up. The `on` arm dragged and double-clicked at the places
found before both arms: PRIMARY held a piece of the new block's command line (`TINEL_NOT_THE_P`) and its file name. `relocate()` reads
the places again at the start of each arm (find hook: `current_row`; OCR-found words: one new screen read).

## 3. `link_and_path_click` (#2141): TEST, both rows

**Terminal FAIL.** App log: the three words at y 774 / 806 / 838 at 16:18:16; at 16:18:28 the same words at y 735 / 767 / 799 and a
second copy of the path's word at y 848 (a prompt suggestion quoting it); 16:18:31 `hop=view_mode mode=agent` and a model request. The
macOS fault of #2137. Its fix (`aim()` follows the find hook's newest sighting) cannot work in this case on Linux: the case relaunches the
app as `exec strace -f ... <app>`, `app.pid` holds strace's PID, and `_hook_word` asks sway / xdotool for the window of that PID, gets
none and returns nothing. The run's `link_positions` are OCR positions (x 430 for a word the hook drew at 442 to 514).

**Agent NOREF.** "the words [...] were not found on screen": the app had logged the word (`appci_find token=... x0=306 y0=828`); the
harness fell back to OCR for the same reason and OCR missed it. The NO-REFERENCE hid a judgeable row.

**Change.** `cases_ui.app_window_origin`: the launcher PID's window, else the first descendant's (`descendant_pids`, `/proc/<pid>/stat`).
#2138 (the suggestion is a chip that moves no row) removes the cause of the move in the app; the harness follows the text either way.

**Rejected.** Writing the app's own PID to `app.pid` under a wrapper: `stop()`, the focus guard and the strace readers all use the
launcher's PID and group.

## 4. `ime_compose` (#2141): TEST

**Row.** "marked text leaked 6e 69 68 61 6f"; then `pty received 20`; in the editor `echo nihao  >> file` (two spaces).

**Cause.** `ime_focused_contexts` = `dev.phosphor.Phosphor/wayland_v2`: the daemon had the app's text input. Every `wtype` call of the
Wayland backend starts with a lone `Shift_L` tap (#1414). A lone Left Shift is fcitx5's default `AltTriggerKeys` ("temporarily toggle
input method"): the tap before the letters switched Pinyin off (allowed whenever the method is active); the tap before the commit Space
switched it back (allowed because the last change was this key), so the Space was typed in Pinyin with nothing to commit: a plain space.
That is the output, byte for byte, in both arms. Composition was never tested. Inferred from fcitx5's defaults and the bytes; the
daemon's log is not in the artifacts, so the next run's `ime_after_prime_tap` is the proof.

**Change.** The case writes `~/.config/fcitx5/config` with `AltTriggerKeys=` (empty) before the daemon starts (`fcitx_config`). Control
before anything is typed through the method: with Pinyin selected, `keys([])` (the tap alone), then the daemon's current method;
anything but Pinyin is UNVERIFIABLE with that reason (`prime_tap_verdict`). Metrics `ime_after_prime_tap`, `ime_after_keys`.

**Rejected.** Unprimed calls for this case: #1414's first-key loss would come back in the one case that counts bytes. Another priming
key: every backend call and the key accounting depend on it.

## 5. Two notifications for one question (#2140): APP

**Row.** One `ask_user_question` card with another window in front: 2 identical notifications; two `hop=desktop_notification` lines 56
microseconds apart after `hop=turn ... state=blocked`.

**Cause.** `TerminalView::maybe_send_agent_mode_desktop_notification` is called from `UpdatedConversationStatus` (the status became
Blocked) and from `AIBlockEvent::ActionBlockedOnUserConfirmation`. `handle_not_executed_action` emits the action event and then sets the
status, so both arrive every time; `AIBlock` re-emits the action model's event without checking the action is its own. A finished turn
has the same pair (status change, `AIBlockEvent::Finished`). The pin has the same three call sites.

**Change.** `terminal/view/agent_notify_once.rs`: per conversation, the episode last notified for (status name + the blocked action's id,
or the last exchange's id). The same episode is not notified for twice; a status that does not notify ends it. Kill switch
`PHOSPHOR_AGENT_NOTIFY_EVERY_EVENT=1`.

**Rejected.** Dropping one call site: each is the only one in some path (a second blocked action with no status change between; a
status change with no AI block event), and which is which cannot be proven by reading. A time window: a timer deciding what a person is
told.

**Risk.** A wanted second notification is lost only if the same action (or exchange) blocks (or ends) twice with no other status
between. Not compiled; the wiring in `view.rs` has no unit test (the module has).

## 6. `subshell_ready_relay_shape` (#2141): TEST; question #2142

**Row.** "the setup was written behind sudo: not after Ctrl-I and not within 10 s of the first typed line's prompt".

**Cause.** App log: `subshell_setup_armed`; `decision=Wait(RelayNoPrompt)` (the prompt was drawn before Ctrl-I); 79 `Wait(LineDirty)`
while the line was typed; at its Enter `subshell_ready_line_end non_empty=true fact=Relay ... tail=ShellPrompt verdict=Cancel` and
`subshell_setup_cancelled why=UserDrove`. That is rule R2 (`on_user_line_ended`): a non-empty line typed at a shell prompt before the
setup is written is the user driving the shell. The round 19 form of the case (#2109 item 9, written without the judge's lines)
required the write with that line's prompt.

**Change.** No write within 3 s of Ctrl-I: one Enter on an empty line (`ReArm`; the shell draws its prompt again), the write is
required within 10 s, an Enter that cancels fails, and the case's line is typed after the write. Metric `relay_write`: `at_arm` /
`after_empty_enter` / `never`. The failure prints the judge's looks and the line-end and cancel hops.

**Owner question (#2142, not decided here).** For a person: accepting the banner at an idle `sudo -s` prompt does nothing visible, and
the first command typed cancels the setup silently.
