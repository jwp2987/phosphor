# Round 15, wave 4 desktop: scale, notifications, drag and drop, IME (T35, T37, T38, T39)

Branch `fix/r15-w4desk`, from integ/r14 `75847d84c`. Spec: `docs/design/r14-test-gaps.md` 7.1 rows 35, 37, 38, 39 (gaps G9, H5, H7, H8, H9;
false coverage F2, F6). Issues #2075 (harness capabilities, wave 4) and #2078 (every run is at scale 1 on one output). Nothing was compiled and
no app was run when this was written. Two harness tools were run on the laptop without the app (see "What was run").

## What exists today (evidence)

| item | state at `75847d84c` | evidence |
|---|---|---|
| scale | every run is scale 1, one output | `wayland_session.sh` writes `output HEADLESS-1 resolution ... position 0 0`, no `scale`; `APPCI_SCALE` is a count scale, not a display scale (`cases.py:877`) |
| notifications | no service on the app's bus, nothing looks | `app.py` launches under `dbus-run-session`; `crates/warpui/src/windowing/winit/notifications/linux.rs` calls `notify_rust::Notification::show_async` (one `Notify` call), which fails with ServiceUnknown there. No case names notifications |
| drag and drop | `ui_drag_drop_file` SKIPs on all four jobs | `cases_ui.py:749`: X11 only, needs `dragon-drop`, which no machine has |
| IME | `intl_ime_japanese` / `intl_ime_pinyin` SKIP everywhere | `cases_caseintl.py:398` `mac_only`; macOS waits for the owner (NEEDS_OWNER) |

App code read for the oracles:

- Long-running notification: `app/src/terminal/view.rs` near 25895. Sent when notifications mode is `enabled`, the block ran at least
  `long_running_threshold`, and `is_navigated_away_from_window` (the window is not the active window). Title is the command plus
  " finished after Ns" (`create_notification_content`, view.rs:1285).
- Agent notification: `maybe_send_agent_mode_desktop_notification` (view.rs near 25950): status Blocked, Success or Error while navigated away.
- Every desktop notification passes `AppContext::send_desktop_notification` (`crates/warpui_core/src/core/app.rs:2780`).
- Settings: `notifications.preferences` (`app/src/terminal/session_settings.rs:345`); the file format of the threshold is a plain integer of
  seconds (`app/src/settings/init_tests.rs:454`); default mode is `unset`, which sends nothing.
- Drop: winit `DroppedFile` -> debounce -> `Event::DragAndDropFiles { paths, location }` with
  `location = window_state.last_cursor_position` (`crates/warpui/src/windowing/winit/event_loop/mod.rs:2008`); the terminal takes it only when
  that position is inside it (`app/src/terminal/terminal_size_element.rs:93`); the text inserted is `escaped_paths_str`
  (`app/src/editor/view/mod.rs:8929`).
- winit emits `DroppedFile` on X11 (`lib/winit/src/platform_impl/linux/x11/event_processor.rs:547`) and Windows only. The Wayland backend has no
  data-device code at all. Filed as **#2085** (app defect, not changed here).
- X11 scale: winit re-reads `Xft.dpi` when `RESOURCE_MANAGER` changes (`lib/winit/src/platform_impl/linux/x11/event_processor.rs:856`).
- macOS asks for notification permission when the mode is enabled (`crates/warpui/src/platform/mac/delegate.rs:316`). The prompt belongs to
  UserNotificationCenter, which `backend.MacBackend.DIALOG_OWNERS` treats as a system dialog in front of the app.

## Chosen design

One case file, `script/realapp/appci/cases_desk.py`, five cases in `cases.PROVING` (the agent half of T37 also in `PROVING_FAKE`), pure parts in
`selftest_desk.py`. Additions to `PROVING`, `PROVING_FAKE`, `winmap` and `selftest.py` are statements of their own.

**One launcher hook.** `app.py` `INNER` sources the file named by `APPCI_SESSION_PRE` inside the private D-Bus session, before the app starts.
A case writes the lines, sets `App.extra_env`, relaunches, and removes the variable in its `finally`. T37 and T39 both need a service inside
the app's session; one generic line serves both and is inert for every other case.

**T35 `scale_matrix`.** Knob per backend: sway `output <name> scale <s>` while the app runs; on X11 the `Xft.dpi` resource through `xrdb`
(saved text loaded back at the end). For 1.25, 1.5, 2: the window gets one logical size, first at scale 1 (control), then at the scale.
Oracle: `stty size` at the scale equals the app's own `hop=grid_resize` and the control within 8 % or two cells. A grid laid out in device
pixels differs by the factor s, one that ignored the scale by 1/s; both are outside the tolerance (self-test). Then a double click on the
fifth of six printed words, placed from the app's `appci_find` line (device pixels) through the output's scale, must copy exactly that word.
Wayland: `swaymsg create_output`, the second output gets scale 2, the window moves there, both oracles again. `LayoutMouse` reads the layout
box on every motion (the `cases_ui.Mouse` cache of the first output's size is wrong after a scale change).

**T37 `desktop_notification`, `desktop_notification_agent_question`.** `notify_recorder.py`: the D-Bus wire protocol in plain Python (unix
socket, EXTERNAL auth, `Hello`, `RequestName`), answering `Notify` and writing one JSON line per notification. It is proven per case by a
control notification the harness sends itself on the app's bus. The app also logs `hop=desktop_notification title_chars= body_chars=`
(trace only, counts, no text) at the one call every notification passes; on Linux the service and the hop must agree, elsewhere the hop
judges. Arms: long command ends focused (none), ends while the reference terminal has the focus (one, naming the command), short command ends
unfocused (none); question card arrives unfocused (one), answered focused (still one).

**T38 `drag_drop_file`.** `dragsrc.py`, a GTK 3 window whose area starts a `text/uri-list` drag (GTK does X11 and Wayland); `dragon-drop` is
the fallback; neither present = SKIP with the package names. The pointer is parked on the target before the drag, because the app drops at
its last pointer position and gets no motion while the source holds the grab. Oracle: the shell receives one line whose words are `ls -d`,
the path as one argument, `> file`, and the file holds the path. The source's own events separate "the drag never began" (UNVERIFIABLE) from
"the target never asked for the data" (failure). Wayland is expected to fail until #2085.

**T39 `ime_compose`.** Linux: fcitx5 (both display systems) or ibus (X11) started inside the app's session by the hook, selected and verified
with `fcitx5-remote` / `ibus engine`. Windows: Microsoft Pinyin selected for the Phosphor window only (`WM_INPUTLANGCHANGEREQUEST`, the method
of `intl_altgr_*`), letters as `SendInput` virtual keys. Both reuse `cases_caseintl.compose_raw` / `compose_editor`. macOS stays with
`intl_ime_pinyin` / `intl_ime_japanese`.

## Rejected

- **A notification daemon package (dunst, mako) or python3-dbus.** A machine change, and a real daemon's log is not an oracle. The wire
  protocol is about 200 lines and has no dependency.
- **`dbus-monitor` with no service.** The call would be seen, but the app would still get ServiceUnknown and take its error path.
- **Starting the recorder for every launch.** It changes what every other case's app sees. Scoped to the two cases.
- **A Wayland drag source written against the wire (like `wlptr.py`).** It needs a mapped surface with a buffer (fd passing) and the grab
  serial of a press on it; GTK already does this.
- **Fixing #2085 here.** A drop target is new protocol code in the vendored winit (data device manager, offers, a pipe read); it cannot be
  proven by reading and nothing can be compiled on this machine.
- **Running the macOS notification arm by default.** The permission prompt would stop the whole Mac pass. Gated on `APPCI_MAC_NOTIFY_OK=1`.
- **Removing `ui_drag_drop_file`, `intl_ime_*`.** Owner 2026-11-10: always-SKIP cases stay registered. They are untouched.

## Risk

- Harness only, except one trace line in `warpui_core` (counts, no text, free when input-order logging is off; unit test pins the format).
- Each case that relaunches the app or changes a display setting restores it in `finally`; a restore failure is a failed check, not silent.
- Unproven until a run: the sway `create_output` / floating / `[pid=]` commands, the X11 runtime `Xft.dpi` reload, the GTK source under
  sway with the virtual pointer, and all of T39 (no machine has an input method). Each reports UNVERIFIABLE with the reason when its
  precondition does not hold; none can pass on missing evidence.

## What was run (laptop, no app, no cargo)

- `notify_recorder.py serve` on a private `dbus-run-session`: `notify-send`, `gdbus call ... Notify` with hints, `GetServerInformation`, an
  unknown method (error reply), the `send` control, and `NotificationClosed` seen by `gdbus monitor`. Three log lines, as sent.
- `dragsrc.py` on a private Xvfb with a GTK drop target and `xdotool` press / move / release: events `begin`, `data`, `end`; the target
  received `file:///.../dnd%20file%20a.txt`.
- `run.py --self-test`.
