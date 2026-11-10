# Round 11: the macOS app-test failures of round 10

Source: the macOS app-test job of `build/r10a` (`integ/r10` 5dd50d49c, smoke tier, pipeline
3048440714). Evidence: the committed `results.json` / `summary.md`, and the job's per-case app-log
slices and screenshots read from the Mac runner's job directory (read-only). Nothing was built or
run except the appci self-test and the Mac's two OCR tools on one saved screenshot. Issue: #2023.

## Summary

Seven FAIL rows, four causes. All four are TEST faults. No app code changes.

| rows | verdict | cause |
|---|---|---|
| `subshell_ready_secret_after_probe` (terminal, agent) | TEST | the lab's hop filter drops the hop the case asserts |
| `ui_vertical_tabs_many` (terminal, agent) | TEST (leftover of `md_viewer`) | `md_viewer` leaves its viewer tab open and in front |
| `pane_not_local_after_ui_vertical_tabs_many` (terminal, agent) | follow-on of the row above | the viewer tab was in front again after the case |
| `mode_proof` (agent) | TEST (OCR engine) | tesseract was used on macOS and does not read the model chip that is on screen |

## 1. subshell_ready_secret_after_probe

Only one check failed, in both modes: "stage 2 waited for the prompt after the probe's hook (hop
subshell_stage_two_pending)". Password, second write, handed-back line, root shell: all passed.
The case's own app-log slice (terminal pass) has the hop, in the order the case wants:

```
08:03:23 hop=subshell_setup_ready_write trigger=banner shell=None relay=false bytes_passed=0 age_ms=0
08:03:23 hop=subshell_stage_two_pending shell=Bash
08:03:24 hop=subshell_setup_cancelled why=Secret dest=Editor chunks=35 bytes=35 flush=false ...
08:03:24 hop=subshell_setup_ready_write trigger=banner shell=Some(Bash) relay=false bytes_passed=8 age_ms=337
```

Cause: `cases_subshell_ready.hops()` keeps only lines that contain one of seven listed hop names.
`subshell_stage_two_pending` (logged by `stage_two_after_init_subshell`, `app/src/terminal/view.rs`)
was not one of them, so `first_index(hs, "subshell_stage_two_pending")` was always `None`. The
case was added with the hop (e42f74743) and has never been able to pass this check on any platform;
the Wayland row of round 10 fails with the same single message.

Fix: the list is the module constant `HOPS`, with the name added; `hops_in(text)` is the pure part.
The self-test reads every hop name the module looks up (`first_index(..., "subshell_...")`,
`h == "subshell_..."`) from its source and fails when one is not in `HOPS`, and runs the four
lines above through `hops_in` and the case's own index arithmetic.
Rejected: keeping every `subshell_` line (the cases slice `hs[:write]` and count judged lines; a
wider list changes nothing they assert today, but a closed list checked by a test is the smaller
change). Risk: none for the app; the other cases filter by hop name, so one more kept name does not
move any of their checks.

## 2. ui_vertical_tabs_many, and the pane_not_local row after it

Rows: the effect file has 7 of 8 tokens (the FIRST tab's is missing), the shells received every
line but the first, "the window had [9] tab(s) at the tab keys, expected 8", tab 2 answered
`<none>`, and after the case "the pane's shell was not the root shell (none)".

- `md_viewer` ran 7 s before (four skipped cases between them). Its log:
  `08:30:30 [INFO] dispatching action for "root_view:add_file_pane"`. That action is
  `RootView::add_file_pane` -> `workspace.add_tab_for_file_notebook` (`app/src/root_view.rs`): a
  new TAB, focused. `md_viewer` never closes it.
- `ui_vertical_tabs_many` then typed its first line (`08:30:42 typed_characters "e" "c" "h" "o"`)
  with the viewer in front: no shell got it. Its seven new tabs came after the viewer:
  `hop=tab_activate index=3 count=9`, and every later hop says `count=9`. Tab 1 (the root pane)
  answered correctly, tab 2 (the viewer) answered nothing, tabs 3..9 are the case's 2..8.
- The close loop ended the case's tabs with Ctrl-D (no close key on macOS) and stopped at 2 tabs;
  the viewer was in front, the runner's root-shell probe got no answer and relaunched the app.
- Why not on Linux: `zero_network_startup` runs between the two cases there and relaunches the
  app. On macOS it skips. `cases.post_case_invariants_apply` (round 9) had already met the same
  leftover and only stopped the probe after skipped cases.

Verdict: TEST. The app did what it was asked: keys typed at a markdown viewer are not shell input.

Fix:
1. `cases.close_file_viewer`, called in a `finally` by `md_viewer` and `md_viewer_big`: when the
   app log shows the tab opening, close it with `editor_close()` (the close key; on macOS the
   command palette's "Close Current Session", the route the editor cases use) and prove with the
   PID probe that a shell, the pane's root shell when its pid is recorded, is in front. No log
   line: nothing is pressed. No proof: the case fails with the reason.
2. `md_viewer*` joins `DEPTH_PREFIXES`: the runner's root-shell probe runs after it and relaunches
   the app if the pane is still not a shell, so one failed close cannot spoil the cases after it.
3. `ui_vertical_tabs_many` and `ui_vertical_tabs_panel_click` prove where their first key goes
   (`first_tab_in_front`: `pwd` answers with the work directory) before they open a tab or touch
   the settings file, and stop with that one failure otherwise. This does not make a red run
   green: it replaces twelve derived failures after 260 s by the one true one after a few seconds.

Rejected: pressing the "tab 1" key at the start of the vertical tabs case (hides the leftover, and
the tab count stays wrong); a relaunch after every `md_viewer` (slow, and it hides whether the
close works); Cmd-Shift-W on macOS (the frontmost-app guard refuses every Cmd-W chord by owner
rule). Not changed: `cases_agent_netgate.close_viewer` (close key only; those cases do not run on
macOS) and `cb_editor_mac` (documented to run last and rely on the runner's relaunch).

Risk: the palette close of a VIEWER tab on macOS is unproven (the editor cases use it for code
panes; it is the same `PaneGroupAction::RemoveActive`). If it does not close the tab, `md_viewer`
fails on macOS with "no shell answered the PID probe" and the runner relaunches: visible, and the
later cases are still clean. On Linux and Windows the close key is the one `close_viewer` already
uses on such a tab.

## 3. mode_proof (agent)

Row: "model chip 'gpt-oss:20b' not visible before any key was typed (the CI Ollama model is not
the active one)"; metrics `log_view_mode=agent`, `ocr_agent_markers=true`,
`ocr_model_chip_gpt_oss=false`.

- The pre-key screenshot (`shots/agent_proof_prekey.png`, written 02:37:12 local, the case's
  second) shows the agent view and, bottom right, the chip "CI Ollama / gpt-oss:20b". The Mac's
  machine-local `ci.env` names that model. So the model is active and drawn: not the environment,
  not the app.
- `ctx.ocr` runs tesseract whenever one is on the PATH and uses the Mac's Apple Vision tool only
  when there is none. The Mac runner has a Homebrew tesseract. Both engines, run again on the
  saved file: Vision returns `(ESC) for terminal`, `New agent conversation`,
  `Cl Ollama / gpt-oss:20b`; tesseract returns `at New agent conversation` and no chip line. That
  is exactly the row: markers true, chip false.
- Round 9's agent `mode_proof` failed on the marker strings (fixed in round 10); the chip check
  was reached for the first time on macOS in round 10.

Verdict: TEST (the wrong OCR engine for the platform).

Fix: `cases.ocr_engines`: on macOS the Vision read always comes first and tesseract's read is
added when a tesseract exists (as the X11 2x read is added, never replacing one). Other platforms
are unchanged. Rejected: Vision only (a case that passes today on a word only tesseract reads would
turn red for no app reason); dropping or softening the chip check (it is the proof that the agent
pass talks to the CI model). Risk: negative checks (`not is_agent` in the terminal pass) now see
two reads of the same screen; neither engine invents an agent header that is not drawn.
Not changed: the check's literal `gpt-oss` although `APPCI_OLLAMA_MODEL` is configurable; every
runner uses the default today.

## What the next macOS run must show

- `subshell_ready_secret_after_probe`: no "stage 2 waited" failure in either mode (the row is PASS
  or NOREF as its sibling `..._after_prompt` is; the injector-accounting NOREF is a separate
  matter).
- `md_viewer`: no failed check; no "nothing was closed" note; if "no shell answered the PID
  probe" appears, the palette close does not work on a viewer tab and needs another route.
- `ui_vertical_tabs_many`: every `hop=tab_activate` with `count=8`, the effect file with 8 tokens,
  no `pane_not_local_after_ui_vertical_tabs_many` row. A "precondition:" failure means some other
  case left a tab in front; the message names what answered.
- agent `mode_proof`: `ocr_model_chip_gpt_oss` true.
