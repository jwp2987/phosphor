# r10 ctrlbreak: Windows Ctrl+Break never reaches the pty (#1906, #1888)

Written before any change. Nothing here was compiled or run: the verdict is from reading the code
and the round 8 / round 9 Windows results.

## Verdict: BOTH (test fault first, plus an app bug the test happened to hit)

The key is lost at the hop **winit key event -> UI key event**
(`crates/warpui/src/windowing/winit/event_loop/key_events.rs`, `convert_key`), before the
`input_order keydown` line, any binding, or the pty write. It is lost only for the key event the
harness injects. That event is not what the Break key produces, but it is what a real
**Ctrl+ScrollLock** produces, and Windows consoles treat Ctrl+ScrollLock as Ctrl+Break. So:

- **TEST fault:** the injector does not make the Break key. A real Ctrl+Break reaches dispatch
  by reading (below); the case never tested it.
- **APP bug:** Ctrl+ScrollLock (VK_CANCEL with the ScrollLock scan code) is dropped. Real
  keyboards can send it. This is what failed 0/3 in rounds 8 and 9.
- **APP bug (second, found on the way):** in a focus gap (focus on the input view outside its
  editor, a settled AI block, #1552) a Ctrl+Break that did reach dispatch is written as Ctrl-C
  (`0x03`), not as the Break sequence.

## The path, hop by hop

1. **Injector.** `script/realapp/appci/backend.py:1098` maps `break` to VK_CANCEL (0x03).
   `_chord` (1279-1315) sends LCtrl down, key down, key up, LCtrl up (4 events, as the log said:
   `events=4 accepted=4`). `_vkev` (1173-1176) fills `wScan = MapVirtualKeyW(vk, 0)` and sets
   `KEYEVENTF_EXTENDEDKEY` only for the VKs in `EXTENDED` (1099); VK_CANCEL is not in it.
   `MapVirtualKeyW(VK_CANCEL, MAPVK_VK_TO_VSC)` gives 0x46 without the E0 prefix. The window
   therefore gets `WM_KEYDOWN wParam=VK_CANCEL, scan 0x46, extended=0`, Ctrl held.
   A real Break key sends scan **E0 46** (extended=1). Scan 0x46 without E0 is the ScrollLock
   key; with Ctrl held the system gives it VK_CANCEL too (Raymond Chen, "Why does
   Ctrl+ScrollLock cancel dialogs?", quoted in `lib/winit/.../keyboard.rs:537-542`).
   Neither ConPTY nor the OS takes the key away: Phosphor is a GUI process, the window gets
   WM_KEYDOWN, and CTRL_BREAK_EVENT only exists on the console side of the pty.
2. **winit** (`lib/winit/src/platform_impl/windows/keyboard.rs`, `PartialKeyEventInfo::from_message`,
   511-560). The physical key comes from the scan code (`scancode_to_physicalkey`, 1280-1282):
   `0xE046 -> Pause`, `0x0046 -> ScrollLock`.
   - Real Break (E0 46, Ctrl): `code_as_key` (543-551) forces `Key::Named(Pause)`.
   - Injected / Ctrl+ScrollLock (0x46, Ctrl): `code_as_key` is `None`, so the key is
     `layout.get_key(.., VK_CANCEL, ScrollLock)` (`keyboard_layout.rs:282-323`).
     `vkey_to_non_char_key(VK_CANCEL)` is `Unidentified` (840), so `get_key` falls through to the
     per-physical-key map (315-321), which `prepare_layout` filled with `ScrollLock ->
     Named(ScrollLock)` from VK_SCROLL (456-466). The event is `Key::Named(ScrollLock)`,
     **not** `Unidentified(Windows(3))`.
3. **App conversion** (`key_events.rs:80-185`). `Named(Pause)` -> `"pause"` (301): the real Break
   key becomes `KeyDown { ctrl, key: "pause" }`. `Named(ScrollLock)` has no arm in `convert_key`
   (-> `None`, 338), so the result is `Unconverted`; the text fallback refuses because Ctrl is
   held (`text_fallback_event_for_unconverted_key`, 198-204). **The key is dropped here, with no
   log line of any kind.**
   This is why 03fea1d2b changed nothing: it renames `Unidentified(Windows(0x03))`
   (`is_windows_vk_cancel`, 114-118, 273-276), a value winit only produces when the scan code
   maps to no key at all. The round-8 comment on the issue read `vkey_to_non_char_key` and
   missed the fall-through in `get_key`.
4. **Dispatch** (`crates/warpui_core/src/core/app.rs:4988-4998`) logs `input_order keydown
   key=pause ctrl=true`. The round-8 log had no such line (issue comment); consistent with 3.
5. **Pane key sink** (`app/src/terminal/view.rs`, `on_pane_key_event`, ~13405-13560). Windows
   sends WM_CHAR 0x03 for VK_CANCEL, so `chars` is `"\x03"`.
   - Terminal view focused: `terminal_key_down` -> `to_escape_sequence` ->
     `ctrl_break_win32_input_sequence` (`crates/warp_terminal/src/model/escape_sequences.rs:259-263,
     659-664`) -> `control_sequence_on_terminal` -> pty write `ESC[3;70;0;1;8;1_ ESC[3;70;0;0;8;1_`.
     Correct.
   - Focus gap: the arm `(KeyTarget::Terminal, KeyDown { chars, .. })` calls
     `keydown_on_terminal(chars)`, which writes `0x03`: the program gets Ctrl-C, not Break.
6. **pty write** is logged on Windows as `hop=pty_write_bytes ... bytes="\x1b[3;70;..."`
   (`app/src/terminal/local_tty/event_loop.rs:1474-1488`); the case greps `3;70`.
7. **ConPTY -> program.** conhost's input parser turns a win32-input-mode key record with
   VK_CANCEL into CTRL_BREAK_EVENT for the console's processes. Not reached in any run so far,
   so still unproven; it depends on the conhost the Windows 10 VM runs. The next run separates
   this hop from the others (below).

## Fix

- **App, conversion:** on Windows, `Named(ScrollLock)` with Ctrl held is named `Cancel` (then
  `"pause"`), next to the existing `Unidentified(Windows(3))` rule. Pure predicate
  `is_windows_ctrl_break(key, ctrl)` with a unit test. Plain ScrollLock stays unconverted.
- **App, focus gap:** in the gap arm, a Ctrl+Break keystroke during the user's long-running
  command (not the agent's, not an ssh connect hold) goes out as the Break sequence through
  `control_sequence_on_terminal`. Every other state keeps today's behaviour. The key is also
  classed as the sink's when its `chars` are empty, so it cannot fall out of the gap silently.
  `escape_sequences::ctrl_break_sequence` is the public form of the existing encoder.
- **App, evidence:** new hop `input_order hop=key_unconverted logical=.. physical=.. ctrl=..` in
  the event loop for a pressed key that became no event at all. Names only; a character key
  prints `char`. This is the line that was missing in rounds 8 and 9.
- **Test, injector:** `break` / `cancel` are sent as the real Break key: VK_CANCEL, scan 0x46,
  `KEYEVENTF_EXTENDEDKEY`. New key name `scrollbreak` sends what the injector sent before
  (scan 0x46, not extended) = Ctrl+ScrollLock.
- **Test, cases:** `ps_ctrl_break` keeps both requirements (the `3;70` write and SIGBREAK, 3/3)
  and now reports the hop where a failed trial stopped. New `ps_ctrl_scrolllock_break`
  (PROVING) is the same body with `ctrl+scrollbreak`.

## Rejected

- **Change the winit fork** to report VK_CANCEL as `Named(Cancel)`: the cleaner place, but the
  app layer already holds the VK_CANCEL rule and is where a unit test can reach; the fork has
  no test for `from_message`.
- **Only fix the injector:** the case would go green and Ctrl+ScrollLock would stay dropped.
- **`GenerateConsoleCtrlEvent` from the app:** needs a console attach in the GUI process
  (already rejected in the #1888 encoder comment).
- **Send the Break sequence from every gap state:** at a prompt the gap's Ctrl+Break acts as
  Ctrl-C today; changing that is not needed for this issue.

## Risk

- Uncompiled. The view.rs hunk is small but in a shared file.
- `MapVirtualKeyW(VK_CANCEL, 0) == 0x46` is from knowledge of the US layout tables, not measured.
  If it returned 0, winit would have derived E0 46 itself and rounds 8/9 would have logged a
  `keydown key=pause`; they did not. The injector now hard-codes 0x46 so it no longer depends on it.
- A tool that injects VK_SCROLL with Ctrl held is now read as Break. Hardware cannot send that
  (Ctrl+ScrollLock is always VK_CANCEL).
- Not changed: an agent-owned command drops the Break sequence (non-abort control sequence,
  `control_sequence_on_terminal_legacy`), and an ssh connect hold holds it with the typeahead.

## What the next Windows run must show (PowerShell pane, both modes)

Per trial of `ps_ctrl_break` and `ps_ctrl_scrolllock_break`, in the app log after the injector's
key:

1. `input_order keydown key=pause ctrl=true` (conversion and dispatch reached). If absent, look
   for `hop=key_unconverted`: its `logical=` / `physical=` say what winit delivered.
2. `hop=kh_pane_key ... class=Control target=Terminal` (the sink took it).
3. `hop=pty_write_bytes ... bytes="\x1b[3;70;0;1;8;1_..."` (the write).
4. `BREAK` in the probe's file (ConPTY made CTRL_BREAK_EVENT). `INT` instead means `0x03` was
   written (gap path); neither with line 3 present means the VM's conhost does not turn the
   sequence into the event, which is then the remaining app-side question (which conhost /
   OpenConsole the pty uses).

The case prints which of these it found, so the job log alone answers it.
