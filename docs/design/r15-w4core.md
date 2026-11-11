# Round 15, wave 4 core: the grid snapshot hook (T34) and the freeze step (T36)

Issue #2075. Source: `docs/design/r14-test-gaps.md` 5.3 (H4, H6) and 7.1 (rows 34, 36). T40 (`mcp_basic`) was done in round 14.
Nothing was compiled or run when this was written. Branch `fix/r15-w4core`, from `integ/r14` 75847d84c.

## T34: what is missing

No case can say what the terminal painted except through OCR of a screenshot, which reads text only: not colour, not attributes, not cell width,
not the cursor's shape. Rendering correctness is therefore unjudged on every platform (H4), and Windows has no OCR at all (H19).

### Where the grid is painted

Every terminal grid goes through one function, `render_grid` (`app/src/terminal/grid_renderer.rs:295`): block grids
(`blockgrid_renderer.rs:101`), the deferred snapshot paint of finished blocks (`blockgrid_renderer.rs:427`) and the alternate screen
(`alt_screen/alt_screen_element.rs:814`). It gets the grid as a `GridSource` (rows of `Cell`: `c`, zero-width characters, `fg`, `bg`, `flags`),
the row window it paints, and `visible_cursor_shape`, which is `Some` exactly when the caller also draws that grid's cursor
(`block_list_element.rs:3533-3543`, `alt_screen_element.rs:838`). So one call site sees everything the spec asks for.

### Chosen design

A test-only hook, `app/src/terminal/appci_grid.rs`, built like the find hook (#1789, `crates/warpui_core/src/appci_find.rs`):

- Off unless `PHOSPHOR_APPCI_GRID_DIR` names a directory. Off costs one cached `OnceLock` read per painted grid.
- The harness lists tokens in `<dir>/request`. The app re-reads it on a size or mtime change, at most every 200 ms.
- When `render_grid` paints a grid whose visible rows show a listed token, the app writes `<dir>/<token>.grid`: one JSON object with the
  painted rows cell by cell and the cursor. Temporary file, then rename. Rewritten only when the content changed.
- Colours are written as the escape sequence gave them (`named:red`, `idx:196`, `rgb:ff8800`), not resolved through the theme, so the
  expected grid does not depend on the profile's theme.
- A grid whose secrets are being redacted is never written. Nothing goes to the log.

The case prints a fixed script that begins with its token, waits for the snapshot to show the script's last line, and compares cells. The
token selects the grid: the block list paints many grids per frame and only the one showing the token is the subject.

### The cursor

A block's output grid draws a cursor only while the block is latched as interactive (`Block::is_output_cursor_visible`,
`model/block.rs:1921`; the latch is set by events such as the alternate screen or the tty-read probe, `block.rs:1848`). ConPTY has no such
probe. The alternate screen draws the cursor whenever the program shows it, on every platform. So the cursor is judged on the alternate
screen, in two states with different places and shapes (DECSCUSR 6 at row 6 column 9, then DECSCUSR 4 at row 1 column 2): a hook that
reported one fixed cursor fails one of them. On the normal screen the cursor is reported as a metric only.

### Rejected

- **A debug action bound to a key or a palette entry.** It needs a key chord per platform and a focus, and a user could trigger it. The hook
  needs neither and cannot be reached without the variable.
- **A log hop with the grid in it.** The grid is screen content; the log is the one place that must never hold it (`appci_find` logs only
  the listed token for the same reason). A file in the scratch home of the test profile is removed with the profile.
- **Reading the model instead of the paint call.** The model holds more than is shown (scrollback, filtered rows). The spec says the
  visible grid, and `render_grid`'s row window is exactly that.
- **Resolved RGB colours.** They would tie the expected grid to the theme and to minimum-contrast adjustment.

### Risk

The hook adds one branch to `render_grid` and runs nothing else when the variable is unset. With it set, every painted grid builds its row
texts (bounded: 400 rows, 1000 columns) while a token is listed: test runs only. Nothing in the input path is touched.

## T36: what is missing

No case stops the app's process. A laptop lid, a debugger, an overloaded machine: after any of them the app wakes with its sockets, its
timers and its key queue in a state no test has seen (H6).

### Chosen design

A freeze step in the harness: SIGSTOP of the exact app pid for 60 s, then SIGCONT, always continued in `finally`. No app change.
`freeze_and_wake` (the fake-model pass, agent mode; Linux and macOS) opens three tabs: the typing tab, a tab with a scripted reply that
streams across the freeze, and a tab in an ssh session on the harness's test host. `lab_freeze_and_wake.*` runs the same body against the
ssh lab with the link stalled for 90 s from just before the freeze (`lab.drop(user, "stall", 90)`).

What is judged, each with a control or a check a wrong result trips:

| claim | evidence | what trips it |
|---|---|---|
| the app was frozen | the OS state at both ends and the log's size | a state that is not `T`, or a log that grew |
| local typing exact at once | the effect file and `rec.log` after a line typed right after SIGCONT; the time to its effect | a partial, merged, repeated or late line |
| no burst of held keys misrouted | a line typed 5 s into the freeze: after the wake the file is `[held, after]` or `[after]`; the tokens are in no other tab's file, on no host, in no model request | any other content, or a token elsewhere |
| ssh alive or cleanly dropped | a line typed in the ssh tab lands on the host; or it runs locally and the app logged ssh leaving the foreground | neither file gains it; a local run with no such hop |
| the agent turn ends or errors | a follow-up reaches the model; the conversation it sends back has no chunk twice and no foreign text | no follow-up (hung turn); a duplicate; a foreign token |

Controls before the freeze: the typing tab takes a line, the host takes a line. Either failing ends the case as failed, not as a pass.

### Decisions inside the case

- **Keys typed while frozen may be dropped by the display system.** Whether X11, a Wayland compositor or the macOS window server holds keys
  for a stopped client is theirs to decide. `[after]` alone is therefore NO-REFERENCE for the held half, never a pass and never a failure.
  What can only be the app's doing fails: a partial line, a merged line, a line in the wrong tab.
- **An errored turn is allowed** (the spec: "ends or errors"). A hung turn is not: the follow-up is how the two are told apart.
- **"At once" is 5 s** from SIGCONT to the effect of a line that takes about 0.7 s to type (`APPCI_FW_WAKE_S`). No budget exists for a
  wake; the number is the measured metric's bound, chosen to catch a stall of seconds. The owner may want a tighter one after a first run.
- **Tabs are addressed without counting them**: the ssh tab is opened last (tab key 9), the agent tab just before it (previous-tab key from
  there), so tabs left open by earlier cases do not shift the keys.

### Rejected

- **A real suspend (`systemctl suspend`, `pmset sleepnow`).** It stops the harness and the runner too, and needs a wake source: a machine
  change. SIGSTOP of the app alone is the spec's own wording and keeps the ssh client and the model server running, which is the hard case
  for the app (the world moved on while it did not).
- **Stopping the whole process tree.** The shells and ssh clients would stop with it and the session could not drop: less is tested.
- **Windows.** No SIGSTOP. `NtSuspendProcess` through `winapi.py` would do it; not built, the case SKIPs there with `NEEDS:`.

### Risk

A compositor may disconnect a client that does not read for 60 s. The case would then report the app dead after the wake; whether that is
the test's doing or the app's is a verdict for the first run (the app log's last lines say which).

## Not app bugs, found on the way

- `cases.PROVING_LAB` is assigned twice in `cases.py` (a merge left both lines). Python uses the second; `script/ci/ci_seq.py`
  `proving_lab_globs` reads the first assignment as a literal. So the fix tier's lab job plays `lab_agent_nld_predict_ssh.*` and not
  `lab_raw_key_arrival.*`, and the harness's own list says the opposite. An augmented assignment (`PROVING_LAB += ...`, which this branch
  adds for `lab_freeze_and_wake.*`) is not read by `ci_seq.py` either. Filed as #2089; not changed here (both are shared statements).
