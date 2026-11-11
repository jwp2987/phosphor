# Round 15 wave 4: the macOS pointer, close key and quit key (#2061; r14-test-gaps T32, G5 / H2)

Nothing here was compiled or run: no cargo on the laptop, and the Swift tools compile only on the Mac runner. The read-only check of the runner
(2026-11-11, `ssh gitrunner@...`: `sw_vers`, `which`, one `python3 -c 'import Quartz'`) is the only thing executed there.

## Cause (why the cases SKIP on macOS today)

- No pointer. `cases_ui.mouse_or_skip` skips on `mac` (`cases_ui.py:390` before this change: "htype2/kl3 are keyboard-only"). The Mac tools
  are `htype2` (keys), `kl3`, `ocr` (lines, no boxes), `winrect`, `tis`, `ktap` (`script/realapp/mac/tools/`).
- No close key, no quit key. `backend.MacBackend._kspec` refuses every Cmd chord on `NEVER_CMD = (q, w, tab, space, h, m)`
  (`backend.py:796, 1012` before this change), by owner rule (docs/testing.md 3.3: a Cmd-W or Cmd-Q that reaches the wrong app closes or quits
  it). `mac/ci/app_test.sh` therefore exports `APPCI_KEY_CLOSE=""`, `cases_robust.need_linux_app` skips the quit cases, and
  `tab_lifecycle_keys` skipped with `NO_UNDO_MAC`.
- Two statements in the harness were wrong and are corrected: the macOS close shortcut is **Cmd-W**, not Cmd-Shift-W
  (`app/src/util/bindings.rs:457`: `CloseCurrentSession => cmd_or_ctrl_shift("w")`, and `cmd_or_ctrl_shift` is `cmd-{key}` on macOS, `:913`;
  Cmd-Shift-W is `CloseWindow`, `:456`). And the close question is not "a native system dialog the harness cannot answer": it is an `NSAlert` of the
  app's own process (`app/src/quit_warning/mod.rs:452` `show_native_platform_modal`; `crates/warpui/src/platform/mac/app.rs:29`), whose first
  button added is the confirm one ("Yes, close" / "Yes, quit": Return) and whose "Cancel" button takes Escape, the same two answers the Linux
  flow sends. The frontmost-app guard passes on it (same PID; not one of `DIALOG_OWNERS`).
- The find hook was never armed on macOS (`MacApp.start` did not set `PHOSPHOR_APPCI_FIND_FILE`; #2082 notes the same for Linux), and
  `context_menu_actions` / `tab_pointer_use` place the pointer from the hook only.

## Which tool: Swift, compiled by the job

The runner has no PyObjC (`import Quartz` fails for `/usr/bin/python3`, the only python3 on its PATH) and has `swiftc` 6.3 (Xcode CLT), which
the job already requires for `htype2`. So the pointer is a Swift source compiled at job start, cached by source hash, exactly like the existing
tools. No install, no binary in git.

## Chosen design

1. **`mptr.swift`** (pointer). Ops in points: move, click (button 1 / 2 / 3, count 1 to 3 with the click state set), an 8-step left drag, wheel
   lines, modifiers held around the ops (posted as flagsChanged and set on every mouse event), pause. Fence in the tool itself: a press, a drag
   end or a wheel step is refused (exit 4) unless the topmost on-screen window at that point (`CGWindowListCopyWindowInfo`, front to back,
   alpha > 0) belongs to the PID of the app under test. The cursor is read back after the last op and written to the out file.
   `MacBackend.ptr()` runs the frontmost-app guard first and refuses while a reference terminal is the target.
2. **Retina mapping.** The cases keep one unit on all platforms, screenshot pixels. `macptr.MacMouse` (same methods as `cases_ui.Mouse`)
   divides by a scale measured once: a real screenshot's width over the main display's width in points. Not a whole number within 2 % means
   "this screenshot is not of this display" and is an error, never a guess.
3. **Positions.** The find hook first: now armed on macOS; its device-pixel window coordinates are added to the window's corner in pixels
   (`winrect` points times the scale). That rests on one assumption, that the window's corner is the corner of what the app draws
   (`hide_title_bar: true`, `crates/warpui_core/src/core/app.rs:2996`, gives the window a full-size content view,
   `crates/warpui/src/platform/mac/objc/window.m:648`). The proof case checks it against Apple Vision on the live machine. Fallback: Vision
   word boxes from a new tool `ocrw.swift` (`ocr.swift` is left alone: it is proven and a change would recompile it).
4. **`appkey.swift`** (Cmd-W, Cmd-Shift-W, Cmd-Q; nothing else). `MacBackend.chord_kind` makes these three a third kind, `app`, carved out of
   the refused set; everything else that was refused stays refused (Cmd-Shift-Q, Cmd-Alt-W, Cmd-H, ...). htype2 still never carries them
   (`_kspec` raises). `app_chord()` refuses for a reference-terminal target, an unknown PID, a bundle path that is not an absolute `.app`;
   then the guard; then the tool, which checks in its own process that the PID runs from inside the job's bundle and is frontmost, and posts with
   `CGEvent.postToPid` (default) so the event cannot reach another process even if the focus changes after the check.
5. **Blast radius kept small.** `APPCI_KEY_CLOSE` stays empty on macOS, and `stop_for_restart` keeps stopping by pid there, so every macOS
   case that passes today runs the same code as before. Only the cases this task names ask `close_key` / `quit_key` / `mac_quit=True`.
6. **Proof first** (`mac_pointer_chord_proof`, order 2). It proves the tools on the machine with a control for each claim, and a failed chord
   half switches the chord path off for the rest of the run, so one undelivered Cmd-Q cannot make every later case type Return into a pane.
7. **`link_and_path_click`.** The app calls `-[NSWorkspace openURL:]` on macOS (`crates/warpui/src/platform/mac/objc/window.m:979`), so the
   Linux recorder (a PATH stand-in) cannot exist. The macOS arm is built (Cmd+click, the recorder's log, the same verdict helpers) and SKIPs with
   `NEEDS:` until the owner makes the recorder the default http / https handler (`mac/ci/open_recorder.sh install`, one dialog).

## Rejected

- **Python + PyObjC.** Not on the runner; installing it is a machine change the task rules out.
- **cliclick.** Not part of the CI toolset; a process per event; no ownership fence.
- **Cmd-W / Cmd-Q through htype2 with only the existing guard.** The guard and the post are two processes; a focus change between them sends
  the chord to another app. That is the incident the rule exists for.
- **`osascript 'tell application to quit'`.** Not the user's key path (no key event, no menu key equivalent), and Apple Events to the app need an
  Automation grant.
- **Setting `APPCI_KEY_CLOSE=cmd+w` globally now.** About a dozen macOS cases would change path at once on an unproven mechanism. One line later.
- **An env-gated "record instead of open" seam in the app** for the link case. It changes app behaviour under an environment variable, which is
  more than a trace-only log line: an owner decision, not taken here. Listed in the report.
- **Opening the test links for real** (Safari on a TEST-NET address): leaves a browser running on the runner and records nothing.
- **Editing `ocr.swift` to add boxes.** A compile error there would stop every macOS run (`fail3`); the new tools' compile errors do not.

## Risk

- **Nothing is compiled.** Four Swift files written blind. A compile error costs those tools only (non-fatal loop in `app_test.sh`); the proof
  case then fails and names the log.
- **`postToPid` may not deliver a menu key equivalent to this app.** Unproven. The proof case's Cmd-W says so within the first two minutes of a
  run, and `APPCI_MAC_CHORD_VIA=hid` is the switch (the HID tap, the path every other key already takes; the in-tool frontmost check is then
  the fence).
- **The ownership fence may refuse a legitimate click** if some other process keeps a visible window over the app (an overlay). The error names
  the owner; `APPCI_MAC_PTR_IGNORE_OWNERS=<owner name>` lets the tool look through it. Not set by default.
- **Hook origin assumption** (point 3): checked by the proof case; `APPCI_NO_FIND_HOOK=1` falls back to OCR for the cases that have a fallback
  (`context_menu_actions` and `tab_pointer_use` are hook-first and would then be UNVERIFIABLE or OCR-located).
- **Accessibility.** The new binaries post events under the runner's existing grant (the responsible process is the runner, as for htype2). If
  macOS wants a per-binary grant the cursor read-back fails and the proof says "the events were not accepted".
- **Keys are not lost, reordered or split by this change**: `keys()` flushes the htype2 run before an app chord and starts a new one after it; no
  timer was added to any input path; no file of the input queue was touched.
