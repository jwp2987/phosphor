# Round 19, task `r19-tabs`: tabs, close questions, Find, viewer (round 14 candidate failures)

Source: round 14 candidate, build/r14a `42ef67839`, pipeline 3052606915. Nothing here was compiled or run. Evidence is the
jobs' `results.json` (the app log and the screenshots are not in the artifacts) and the code. Issues: #2108 (app), #2109
(test faults), #2110 (restore after quit, unconfirmed).

None of these cases ran before round 14, except `ui_vertical_tabs_panel_click` (round 13: Wayland terminal pass, agent
NO-REFERENCE, Windows skip) and `subshell_ready_relay_shape` (skip everywhere until the runner got a sudo target).
`left_agent_mode_after_soak` also failed in the 2026-11-05 run.

## 1. One app defect: Cancel on a close question leaves the keyboard in the dismissed dialog (#2108)

Cause. `Workspace::show_native_modal` (app/src/workspace/view.rs) focuses the `NativeModal` view.
`handle_native_modal_event` only cleared `is_native_quit_modal_open`. The view is no longer drawn but keeps the focus.
Confirm hides it (closing a pane or tab moves the focus); Cancel moves nothing. The other workspace dialogs call
`focus_active_tab` when they close, and the #2092 overwrite question calls `ctx.focus_self()` in its answer for this
reason. `refocus_root_if_focus_unreachable` (#842) runs only when the window becomes active again.

Evidence. `tab_lifecycle_keys`, Wayland agent and three Windows passes: first close key asked
(`tl_close_asked.asked = true`), Escape, second close key: no question in 4 s. macOS (system alert, focus never moves)
passes.

Fix. When the modal closes and its view still has the focus, the workspace focuses the active tab (or itself when no
tab is left). A handler that moved the focus keeps its choice (the palette of "Show running processes", the editor's
overwrite answer). Kill switch `PHOSPHOR_MODAL_FOCUS_BACK=0`. Also: `NativeModalAction::Close` computed
`len() - 1` on an empty button list when Escape arrived with no dialog up (the stuck-focus state): now it only emits
`Close`. Trace: `hop=native_modal event=shown` and `event=closed focus_back=<bool>`.

Rejected. Focusing in every dialog's own cancel callback (five call sites, the next one forgets it again). Focusing
unconditionally on close (steals the focus from the palette a button opened).

Risk. Small: one focus call on a path that had none, guarded by "the dead view still has the focus".

## 2. Test faults (#2109)

| case | fault | fix | how the oracle can still fail |
|---|---|---|---|
| `hostile_output_recovers`, control of `restore_shows_history` | opening Find re-runs the previous query and logs `find_scroll result=scroll` for it (`show_find_bar`); the case read every line since the Find key | one shared step (`cases_r14_w1a.find_enter`): open, empty the field, type, wait for the scan, then judge only lines logged after Enter | a build that focuses a match for an absent token logs `result=scroll` after Enter |
| `editor_daily` close question | the editor's close key closes the editor TAB (`RemoveTabAtIndex`); that question never logs the pane path's line | "asked" = that line or `hop=native_modal event=shown` | no question = neither line |
| `tab_pointer_use` middle click | the closed tab is in the undo-close stack; its shell lives 60 s. The clean-up counted live shells as tabs and closed the last tab (app exit 0), which is the terminal pass's `tab_lifecycle_keys` precondition failure and wedge | closed = the last tab answers from the tab before; the victim's shell must be gone after the undo window; clean-up counts the tabs the case knows | a middle click that closes nothing leaves the victim answering; a leaked shell is alive after 75 s |
| `ui_vertical_tabs_panel_click` (Windows hook) | in the agent view the rows do not show the token; the token's pane sightings were taken for rows and clicked | a row is a sighting left of the pane's own left edge (a probe typed into the input, located by the same hook); not located = not judged, no click | rows in the panel that are not stacked fail as before |
| `md_viewer_first_frame` control (Linux) | the control arm relaunches; the recorded root shell pid is the old instance's | the arm records its own root pid | a viewer left in front answers no pid probe |
| `md_viewer_first_frame` (Windows) | `WINDOWS_RUST_LOG` hides `warpui_core::core::app` and `warp_editor::render::model` Info lines | both modules added; selftest pins them | unchanged |
| `desktop_notification` | the title holds the command's first 16 characters by design; the token was later in the line | the command prints a token; the body must hold it and the title must start with the command's start | another block's notification has another body |
| `left_agent_mode_after_soak` | the latest `hop=view_mode` line was a closed throw-away tab's | the pass's own pane is recorded; when the latest line is another pane's the screen decides (OCR), else no verdict | the pass's own pane logging `terminal` still fails |
| `subshell_ready_relay_shape` | behind a relay the write waits for the first NEW prompt (`Wait(RelayNoPrompt)`); the case typed nothing | the write is required after the first typed line; judge lines printed on failure | no write after a new prompt fails |

## 3. Not decided: restore after a quit on Linux (#2110)

`restore_shows_history`, Wayland both: after quit and relaunch Find says `no_match` at the Find key and at Enter, in a
fresh app (so not the control fault above). Windows (stopped by pid) matches `block=7`. The delete paths found by
reading (`DetachType::Closed`, `BlockListCleared`) are not on the quit path as far as read. The case now records how
many stored block rows hold the token after the stop and after the relaunch (`rs_store`): rows present = the restore
does not show them; rows absent = the quit dropped them.
