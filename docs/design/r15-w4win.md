# Round 15 wave 4, Windows harness: pointer (T31) and window size (T33)

Issue #2060 (gap G4 / H1 / H3 of `docs/design/r14-test-gaps.md`). Owner 2026-11-11: every test gap of that review is in the harness or
fixed. Nothing was compiled or run; the Windows VM has not seen this code. The per-case description is in `docs/testing.md`
("Round 15 wave 4"). No app code changed.

## Cause, with evidence (integ/r14 75847d84c)

| missing | evidence |
|---|---|
| pointer on Windows | `backend.py` `WindowsBackend._send` fills only the keyboard member of `INPUT`; `cases_ui.mouse_or_skip` skipped every backend but x11 / wayland; `winmap.NOT_WINDOWS` gave "no pointer synthesis" as the reason of 10 cases |
| positions without OCR | `cases_ui._hook_word` returned None unless the backend was wayland or x11, although `app.py` `WindowsApp.env` has set `PHOSPHOR_APPCI_FIND_FILE` and `warpui::appci_find=info` since #1789; `cases_ui.words` runs tesseract, which the VM does not have |
| window resize | `cases_ui.resize_to` ends in `return "unsupported backend"`; `cases_window_use.need_window` skipped on `ctx.win` |
| settings rewrite | `cases_ui.settings_path` fell through to `<home>/.config/phosphor/settings.toml` on Windows: the profile's source file. The app reads `%LOCALAPPDATA%\phosphor\Phosphor\config\settings.toml` (`WindowsApp.config`, installed by `start()`), so a live rewrite changed nothing; `cases_wrap_vtabs.vt_need` skipped instead |
| tab keys | `cases_wrap_vtabs.KX` spells next / previous tab `ctrl+Next` / `ctrl+Prior`; `WindowsBackend.NAMED` had `pagedown` / `pageup` only, so `_chord` would raise "no US-layout key" |

## Chosen design

- **`winapi.py`** holds the Win32 part, with every call a parameter so it is tested on Linux: fixed-width `INPUT` structures (40 bytes on
  Win64), `mouse_norm` (ceil(p * 65536 / size): the smallest absolute value Windows maps back to pixel p, by truncation and by MulDiv
  rounding), `Pointer` (move, button, wheel), `client_rect_screen`, `root_window_at`, `set_window_rect`, `resize_verdict`.
- **Delivery is proven, not assumed.** `SendInput` reports events as accepted on a desktop that then drops them. `Pointer.move` reads the
  cursor back (1 px, waited for at most 1 s as a give-up net) and raises `PointerError` with the `NEEDS:` text. `mouse_or_skip` hovers the
  middle of the app window first; a failure there makes the case NOT JUDGED (`cases.HostMissing` -> `noref`), never a skip, never a pass.
- **The pointer only touches Phosphor.** A press needs the foreground guard and the top-level window under the pointer to belong to the
  app's pid; it is refused while `guard_override` (the reference console) is set. A release is never refused; a drag releases in `finally`.
- **Positions: the find hook only.** Device pixels of the client area plus `ClientToScreen`. The harness is DPI aware
  (`SetProcessDPIAware`), so cursor, window and hook pixels are one unit. No hook line: UNVERIFIABLE (`wl_check` on Windows), as
  `context_menu_actions` already did on Linux.
- **Resize: `SetWindowPos` posted, then read back.** `SWP_ASYNCWINDOWPOS` so a busy app cannot block the harness; the proof is
  `GetWindowRect`. Windows clamps to the desktop, so "changed" passes and "unchanged" is the error. `resize_reflow_output` first checks that
  the desktop holds its sizes (else `NEEDS: a desktop of at least WxH`).
- **Panel rows (`ui_vertical_tabs_panel_click`).** The hook logs a token once per position; a tab's token is drawn in the panel and, while
  that tab is shown, in its pane. `hook_panel_rows` takes the newest sighting in the token's leftmost column. The oracle stays each
  clicked tab's own `pwd -W`, so a wrong pick fails, it cannot pass.
- **`winmap.PROVING_WINDOWS`.** `ui_mouse_select_copy` and `ui_resize_typing` are full-tier cases; their Windows arm is new. The tuple puts
  them in the Windows fix tier only.

## Rejected

- A helper binary or PowerShell `mouse_event` script: one more thing on the VM; ctypes already does the keys.
- Relative moves, or `SetCursorPos`: relative moves are bent by pointer acceleration; `SetCursorPos` moves the cursor without the input
  event a real mouse makes.
- OCR twins or a fixed-fraction click sweep: guesses. A missing hook line is reported as not judged instead.
- Adding the two full-tier cases to `cases.PROVING`: it would add `ui_mouse_select_copy` (unclassified on Wayland, F14) to the Linux fix tier.
- Blocking `SetWindowPos`: the harness would hang with the app.
- Porting `tab_pointer_use`, `ui_settings_live`, `minimize_fullscreen`: not in T31 / T33; their `winmap` reasons now say what is missing.

## Risk

- Unknown until a run: whether the app reloads the settings file live on Windows; whether msys bash under ConPTY gets SIGWINCH; whether a
  ConPTY reflow keeps copied output byte-identical; the VM's desktop size; whether the VM profile's panel shows a tab's command or directory.
- `settings_path` now names the app's file on Windows for every caller. The other callers there only read it (`meta_and_word_keys`), or are
  `NOT_WINDOWS`. `cases_r14_state.panel_on` still refuses to switch the panel on live on Windows; it could use `VTabs` now (not changed here).
- Windows fix tier: +8 cases per Git Bash pass, about 15 minutes for both modes (estimate).

## Found on the way

- #2086 (TEST): six `selftest.py` check functions had lost `@check` and never ran. Five registered again; a guard added.
