# Round 14: tests that can never fail (false coverage)

Owner 2026-11-10: a test that can never fail is false coverage. Source: `docs/design/r14-test-gaps.md` section 6 (F1 to F20) and 7.0 g.
Issues #2065 to #2070. Nothing here was compiled or run in an app; the only thing run is the harness self-test. All of it is
"worked, unproven" until a fix-tier run. No app code changes: every item below is a harness or test fault, or needs an owner decision.

## 1. `agent_mcp_faults` never judged (#2065, F1, T40)

Cause. The case wrote the servers into `<scratch home>/.warp/.mcp.json`. The app under test never reads that path:

- `app/src/ai/mcp/file_mcp_watcher.rs:176` takes the Phosphor config from `warp_managed_mcp_config_path()`, and skips
  `MCPProvider::Zap` in the per-provider home scan (`:186`).
- That path is `warp_core::paths::warp_home_mcp_config_file_path()` = `dirs::home_dir()/<config dir>/.mcp.json`
  (`crates/warp_core/src/paths.rs:67-90`), and `<config dir>` for `Channel::Oss` is `.phosphor` (`paths.rs:38-47`). Every build the
  harness tests is `app/src/bin/phosphor_oss.rs` (`Channel::Oss`). `.warp` is the upstream Stable name.
- Evidence in the rows: r12a Wayland and macOS, "servers seen: []; starts (0, 0)": no server was ever spawned.
- Windows: `dirs::home_dir()` is `SHGetKnownFolderPath(FOLDERID_Profile)` (dirs 6.0.0 `win.rs:5`); `HOME` is not read, so the Git Bash
  pane's scratch `HOME` does not move it. The case skipped there by `winmap.NOT_WINDOWS`.

Fix (harness only). The config goes to `<app home>/.phosphor/.mcp.json`: the scratch home on Linux and macOS, the profile known folder
on Windows (`winapi.profile_dir`). The self-test pins `.phosphor` and the file name against `paths.rs`. On Windows the file is in the
runner user's real profile, so the case keeps and restores what was there, and removes a file a killed run left (all server names ours).
Windows runs it in the Git Bash pane's fake pass only (the MCP path does not depend on the pane shell).

New case `mcp_basic` (PROVING, fake pass): one healthy server; started by the app, tool offered, called (after the approval card where
one is shown: the server's call count is read before the card is answered), result at the model, exactly one call, follow-up turn. Each
step is a check with the evidence in the message (config present, server state, tools offered, app log lines naming MCP).
`agent_mcp_faults` runs the same control as checks: it no longer skips when the control fails, and a fault stage whose tool was not
offered is NOREF, not a note inside a pass.

Rejected: an environment override for the config path in the app (app change for a test fault); running in both Windows panes (twice
the time for the same code path).

Risk: the Windows known-folder call is unrun (falls back to `%USERPROFILE%`). If the app does not offer MCP tools to a BYOP provider at
all, both cases now FAIL with the evidence: that would be an app finding, to file then.

## 2. `ssh_connect_typing_wrapper`, `ssh_nested_typeahead_wrapper` always SKIP (#2066, F7)

Cause. Both need `APPCI_SSH_WRAPPER=1` (profile writes the two `warpify.ssh` switches explicitly); no job sets it. They also skipped
off Windows, although the wrapper announcement and verdict (`hop=ssh_wrapper`, `app/src/terminal/view.rs:12015-12140`) are
platform-neutral.

Fix. `.gitlab-ci.yml`: `APPCI_SSH_WRAPPER: "1"` on `app-test-linux-wayland` and `app-test-windows` (job variables; the reuse and lab
jobs inherit). The cases run in any bash pane with the pin: the Windows Git Bash pane (as written: #2051 managed ssh switched on
explicitly by the case, verdict `managed=true` required) and the Linux pane (ControlMaster wrapper: announced, a verdict accepted, none
ignored, verdict managed). macOS (zsh pane, job not pinned) and the PowerShell pane skip with the exact reason. A self-test reads the
YAML so the variable cannot disappear silently.

The pin writes `enable_ssh_warpification` and `enable_legacy_ssh_wrapper` as `true` for the whole job. Both default to true
(`app/src/terminal/warpify/settings.rs`), so this changes other cases only if the machine's owner seed sets one of them false.

Rejected: pinning in every job without a variable (overrides the owner seed on machines nobody chose); a relaunch inside the case.

Risk: on Linux `verdict managed=true` is expected from the wrapper source and from r12a's `handovers [["precmd", n]]`, not from a log
line (slim logs carry no `ssh_wrapper` hop). If the Linux verdict is plain the row fails with the verdicts seen.

## 3. `lab_fam_agent_context` "registered so the guard passes" (#2068, F9)

Cause. `cases_lab_helper._register_families` generated `lab_fam_<cap>` for every backend family and counted it for the coverage guard,
but only `run_command`, `git_read`, `fs_basic` have a driver. The other eleven reported UNVERIFIABLE after the connect, and
`PROVING_LAB`'s glob `lab_fam_agent_*` played 48 arms of `lab_fam_agent_context` as NOREF rows. `fam_file_write_expect` and
`fam_file_delete_expect` were registered as always-SKIP rows for the same purpose.

Fix. A generic family case is registered only when it has a driver. The guard is satisfied by a driver case or by a declared real case
(`EXTERNAL_COVERS`), and now also checks that every declared case is registered. `agent_context` is covered by `lab_skills_remote`
(already declared). `ripgrep_search` had no real declaration: `lab_agent_remote_flow` drives grep and glob on both arms and checks the
route, so it is declared. The two `*_expect` rows become declarations of `lab_fam_agent_edit_conflict` / `lab_fam_agent_delete_conflict`.

Not weakened: the removed arms judged nothing of their own; their connect and leave steps are in every other lab case.

## 4. Never-passed cases: TEST or APP (#2069, 7.0 g)

Evidence available: result rows only. r18u28 has no committed log or screenshot.

| case | verdict | evidence |
|---|---|---|
| `ui_right_click_paste` (Wayland, X11) | TEST | r18u28 both: "the probe typed into the input was not found on screen by OCR: cannot place the pointer". It never reached the right click. The app's position hook (#1789) was to replace OCR, but the Linux launcher never set `PHOSPHOR_APPCI_FIND_FILE` nor `warpui::appci_find=info` (`appci/app.py` INNER; only the Windows launcher did): r12a Wayland, every hook-asking case, "app find hook gave no position ... located by OCR instead". |
| `ui_mouse_select_copy` (Wayland) | TEST | r18u28 Wayland: clipboard kept the sentinel in both modes; X11, same build: PASS. Headless sway had no pointer device then (`swaymsg seat - cursor` reaches no client). The virtual pointer (`wlptr.py`, a0c5517ef) came after; with it r12a Wayland `ui_copy_wrapped_drag` (pointer drag and triple click, then copy) PASSES in both modes. Full tier only: no row since. Worked, unproven. |
| `alt_screen_mouse_reporting` (Wayland, X11) | TEST, likely; not decided | Program received `b'k'` only. The app forwards a click to a running block only when the click is ON that block (`block_list_element.rs:1733-1737`, `should_intercept_mouse`). The case clicked 60 px BELOW the banner (X11) or swept fixed window fractions (Wayland); the program's block is two rows tall. No log to prove where the click landed. |
| `agent_approval` (Wayland) | TEST | Only failure: "written to the shell 0 times (hop=pty_write_bytes ...)". That hop is `if cfg!(windows)` (`event_loop.rs:1474`); the effect-file check passed. Corrected in ec4a03b04 (after r18u28): Linux counts by the effect file. No row since. |
| `agent_approval` (Git Bash) | not decided | Effect file held the token once (that check passed), yet 0 `class=UserCommand` writes carried it, and the follow-up opened no request in 60 s. No log committed. One TEST weakness is certain: the count needed the token inside ONE write line, and a ConPTY short write splits it (`pty_write_partial`). |

Fixes (tests): the Linux launcher sets the find hook's two variables; `alt_screen_mouse_reporting` prints a block of rows carrying a
token and clicks ON the token (hook, then OCR, then the old sweep), and reports where it clicked; `agent_approval` counts the token
over the joined `UserCommand` writes and, on a miss, names every class that carried it. No app issue filed: no row proves an app defect.

## 5. Always-SKIP cases stay, and say what they need (#2066, #2067)

Each skip text now ends with `NEEDS:` and the exact machine change or missing capability. `results_trend.py` lists, per platform and
overall, the PROVING cases that have never been judged in any committed run, with the newest reason, under a heading that says they
are not coverage.

## 6. `latency_key_log` NOREF everywhere (#2070, F11)

Cause. The case calls `lat.evaluate(..., ref=None, kind="overhead")` by construction ("no same-method reference exists"), so
`budget_verdict` is always `no-reference` and `lat_checks` makes the row NOREF whatever its own gates said. On macOS the case has an
app-share gate that judges (#2037: the app's share of key to frame, hard bound 2 ms, and regressions on it): a harness fault there.

Fix. Where the app-share gate judged (usable figure, verdict pass or fail), the missing reference is a note and the row is judged by
that gate. Elsewhere it stays NOREF, and says why.

Owner decision needed (not taken): Linux and Windows log no display-wait split, so there is no app-share of key to frame. The
key to pty write share exists everywhere (r12a medians 0.61 to 0.69 ms, macOS 0.33) and has a 1 ms target, but the code records it as
"never a check, until the owner sets the verdict" (#1176). Making it the judging gate would end the NOREF on those platforms.
