# Round 21, macOS: the four FAIL rows of the round 15 targeted run (#2137, #2138)

Run: `app-test-macos` job 17936839263, build/r15a `b06800e98`, pipeline 3054716634. 29 PASS, 4 FAIL, 3 NO-REFERENCE.
Evidence: the job's `run_slim` logs and screenshots (kept since #2123). Nothing here was compiled or run on a Mac.

All four rows are TEST faults or bookkeeping. No app code changed.

## 1. `mac_pointer_chord_proof`: TEST

**Row.** "a click on the menu bar at (756, 6) points was NOT refused by mptr ('pointer injector failed: mptr exit 4 ( Window Server (pid 547,
layer 24), not to pid ...".

**Cause.** `mptr` refused the click, as it must. `logs/injector-stderr.log`:
`mptr exit=4: refused a click at (756.0, 6.0): the topmost window there belongs to Window Server (pid 547, layer 24) ...`.
`MacBackend.ptr` (`script/realapp/appci/backend.py`) built its error from `_tail(stderr)`, the last 240 characters. Since r19 (#2118) a
refusal also lists the windows at the point and the area clear of the bars, about 330 characters. `cases_macptr._pointer_half` looked for
the word `refused` in that message; the word was in the part cut off.

The brief's guess was that the Dock rule changed what `mptr` looks through. It did not: the window list in the refusal shows the menu
bar's own window (layer 24) in front of the Dock's backing window, and `ownerAt` returned it. `mptr.swift` is unchanged.

**Change.** `backend.PtrError(RuntimeError)` carries `rc`, the whole `stderr` and `posted` (the out file's press / drag / wheel rows).
`cases_macptr.menu_bar_refusal` passes only for exit 4, a stderr that starts with `refused ` and nothing posted. The message keeps the
first 200 and last 240 characters (`backend.head_tail`). The check is stricter than before: it now also fails when the tool says it
refused after posting something.

**Rejected.** Raising `_tail`'s limit: the next longer refusal breaks it again, and a message is not an interface.

## 2. `link_and_path_click`: TEST

**Row.** "Cmd+click on `<file>.rs:140`: nothing opened ..." and "after_links: file has 0/4 lines".

**Cause.** The click landed on a prompt-suggestions banner, not on the path. App log of the case:

| time | line |
|---|---|
| 15:56:16 | `appci_find token=<path word> ... y0=1276.0 ... y1=1308.0` (the positions the case kept) |
| 15:56:31.26 | `notify_cause view=Input cause=set_prompt_suggestions_banner_state`; same frame: `appci_find token=<path word> ... y0=1198.0 ... y1=1230.0`, and a second sighting of the URL's word at another x and height (the banner's text) |
| 15:56:33.64 | `hop=view_mode mode=agent`, then `build_chat_request UserQuery: query_len=32` |

The block moved up 78 px and the banner took its place. `links_play` clicked the place found at 15:56:16. `shots/terminal_lc_path_open.jpg`
shows the agent conversation "Open https://.../<token>". The four lines of `after_links` were typed into the agent view, hence 0/4. The URL
and OSC 8 clicks happened before the banner and passed.

**Change** (`cases_pointer.py`, `cases_ui.py`).
- `hook_find(..., all_hits=True)` returns every sighting of a token since an offset.
- `current_row(row, hits)`: the newest sighting with the same left edge, right edge and height as the row first found, i.e. the same
  text moved with its block. A sighting of another shape is another copy of the word and is never taken.
- `aim()`: hover where the text is now, read the place again, hover again if it moved; four tries, then the click is not sent and the
  step is UNVERIFIABLE.
- After a click that opened nothing: if the text moved between the hover and the check, the click is UNVERIFIABLE, not a failure of the
  app. If a terminal-mode pane entered the agent view, the failure says so (`click_went_elsewhere`).

The expectations are unchanged: exactly one opener call with exactly the URL, the path opened by the system or drawn at its line.

**Rejected.** Switching prompt suggestions off for the case (the owner's settings are the test's settings). A fixed wait for the banner
(its delay is the model's). Taking the hook's last line (`hook_find`'s default): the last line for the URL's word was the banner.

**Filed, not decided: #2138.** A user's click meets the same moving layout. Whether the banner may move the output, or take a click right
after it appears, is the owner's question.

## 3. `minimize_fullscreen`: TEST (the app was not shown to be wrong)

**Row.** "fullscreen off: the window says it left fullscreen, but `stty size` is (56, 153), it was (45, 124) before fullscreen (the grid
did not follow)".

**Cause.** The window never left fullscreen, so the grid had nothing to follow.
- `shots/terminal_mf_fullscreen_off.jpg` and `shots/terminal_minimize_fullscreen_FAIL.jpg`: the window fills the display, no menu bar, no
  Dock, no desktop.
- App log, fullscreen ON (16:02:22 to 16:02:23): `hop=grid_resize rows=54 cols=153`, `received
  NSApplicationDidChangeScreenParametersNotification`, `hop=grid_resize rows=56 cols=153`.
- App log, the off request (between 16:02:30.75 and 16:02:34.45): `paint_only` frames only. No resize, no screen change, no layout. The
  case's log has two `grid_resize` lines in all.
- Metric `fullscreen_off`: `request_error: None, window_says_fullscreen: False`.

The harness set `AXFullScreen` of `window 1` of the process to false and read `window 1` back. The request did not reach the fullscreen
window. Which window `window 1` was cannot be read from this run. The app has no code that refuses a native exit:
`crates/warpui/src/platform/mac/objc/window.m` `windowWillExitFullScreen:` only restores the title bar, and nothing overrides
`toggleFullScreen:`.

**Change** (`cases_window_use.py`).
- `ax_fs_script`: one System Events script lists every Accessibility window of the app as `subrole=state`. Off: every window that reads
  true is set to false. On: the first window that reads false is set to true. No request is addressed to `window 1`.
- A request that reached no window, or a write System Events refused, is the request's error: NO-REFERENCE with the window list
  (`ax_request_verdict`). The harness did not change the window, so nothing about the app follows.
- `Window.fullscreen_state()`: true when any window says so. `Window.covers_display()`: the window server's answer (`winrect` against
  `mptr info`), a second opinion that does not ask the window.
- `wait_fullscreen_left()`: waits for the window to stop covering the display (give-up net 10 s), in place of a fixed 2.5 s.
- `judge_fullscreen_off(..., covers)`: accepted, no window says fullscreen, the window still covers the display: FAIL "the window did not
  leave fullscreen". The window left and the grid is not back: FAIL "the grid did not follow" (the app defect the old text claimed).
- Metric `fullscreen_off` gains `covers_display`, `windows_fullscreen`, `windows_at_request`, `windows_after`.
- `Window.restore()` uses the same request. A window still covering the display at the end of the case is noted (the cases after it run
  in a fullscreen window until the next relaunch, as they did in this run).

**Risk.** The AppleScript was not run. If it errors, `fullscreen(True)` returns the error and the case fails at "fullscreen on" with the
osascript text; nothing passes by mistake. If no window reads true while fullscreen, the row becomes NO-REFERENCE with the list, and the
harness needs another way to ask (the View menu item through Accessibility is the candidate; not built without evidence it is needed).

**What the next run decides.** PASS: the request had gone to the wrong window. FAIL "the grid did not follow" with `covers_display: false`:
an app defect, to be filed. NO-REFERENCE: the window list says what to target.

## 4. `needs_owner`: bookkeeping

The Local Network dialog was seen after `keybinding_rebind` (which relaunches the app; its own row passed). The 45 terminal-mode rows before
it were judged without the dialog. It cost:
- the rest of the terminal pass: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `cb_ui_action_latency`,
  `desktop_notification_agent_question`, `meta_and_word_keys`, `upgrade_over_previous` (in the job's `APPCI_CASES`, no row in any mode);
- the whole agent pass: no case of the list has an agent-mode row.

`APPCI_MAC_AUTO_ALLOW_LOCAL_NETWORK=1` (7eaf9b7ea) answers it from the next run on. No change here.
