# build/r18bn  2b245a376  (pipeline 3015761186)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 61 warnings
- unit: 14348 passed, 8 failed, 55 skipped of 14356 run (3 suites)
- app (pass/fail/skip): macos 0/20/0; wayland 2/16/2; windows 2/8/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | failed | 33.4 | - |  |
| app-test-linux-x11 | success | 0.4 | - | no results artifact (job success) |
| app-test-macos | failed | 1.4 | MacBookPro-gitrunner |  |
| app-test-windows | failed | 28.0 | phosphor-windows-vm |  |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 34.3 | - |  |
| build-macos-arm64 | success | 20.1 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 37.2 | phosphor-windows-vm |  |
| check-linux | success | 5.7 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 0.9 | - |  |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 11.6 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | success | 1.5 | - |  |
| test-workspace-rest | failed | 2.1 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp code::editor::view::vim_handler::tests::test_vim_system_register_paste_holds_the_rest_of_the_same_input` FAIL (0.1s)
- `warp code::editor::view::vim_handler::tests::test_vim_system_register_visual_paste_waits_for_the_read` FAIL (10.9s)
- `warp terminal::model::terminal_model::tests::line_editor_markers_update_the_handoffs_account_in_stream_order` FAIL (0.0s)
- `warp terminal::model::typeahead_handoff::tests::a_signal_with_no_wait_pending_releases_nothing` FAIL (0.0s)
- `warp terminal::view::tests::a_user_key_does_not_wait_for_the_parser` FAIL (3.1s)
- `warp terminal::view::tests::ssh_handover_enter_guard_does_not_swallow_enter_for_a_replaced_line` FAIL (0.5s)
- `warp terminal::view::tests::test_after_enter_replay_drains_in_typed_order` FAIL (0.2s)

## test-workspace-rest failures

- `ai project_context::model::tests::request_before_prewarm_waits_for_the_scan_instead_of_scanning` FAIL (0.0s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | cadence | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | fast_typing | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | paste_instant_enter | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | tab_enter | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | ctrl_c_d | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | ssh_connect_typing | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | latency_enter | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | latency_key_log | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| terminal | latency_paint | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39023): refusing to send keys") |
| agent | mode_proof | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | cadence | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | fast_typing | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | paste_instant_enter | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | tab_enter | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | ctrl_c_d | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | ssh_connect_typing | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | latency_enter | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | latency_key_log | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |
| agent | latency_paint | FAIL | harness exception: RuntimeError("frontmost is '' (pid None), not Phosphor (pid 39292): refusing to send keys") |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| terminal | fast_typing | FAIL | ft0: file has 0/30 lines, exact match False first diff: exp='t1_ieqh524yng' got=None; ft0: shell received 30 commands, not the typed ones (f |
| terminal | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| terminal | tab_enter | FAIL | Tab+Enter: completed file has 0/12 expected lines |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-D in cat: file []; Ctrl-D leaves the nested shell: SHLVL lines [] |
| terminal | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); input_order logging not on or the log path is wrong (/home/<user>/builds/r_hQtIdMf/0/ |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
| agent | mode_proof | PASS |  |
| agent | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| agent | fast_typing | FAIL | ft0: file has 0/30 lines, exact match False first diff: exp='t1_ieqh524yng' got=None; ft0: shell received 12 commands, not the typed ones (f |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| agent | tab_enter | FAIL | Tab+Enter: completed file has 0/12 expected lines |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-D in cat: file []; Ctrl-D leaves the nested shell: SHLVL lines [] |
| agent | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); input_order logging not on or the log path is wrong (/home/<user>/builds/r_hQtIdMf/0/ |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | screenshot only (no OCR on the VM): shots\terminal_proof.png |
| terminal | fast_typing | FAIL | fast typing: 0/30 lines, exact match false |
| terminal | paste_instant_enter | FAIL | paste + instant Enter: 0/6 exact lines |
| terminal | ctrl_c | FAIL | Ctrl-C delivered 0/5 times |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 1793585450692.5 ms over conhost (powershell.exe) exceeds the 2 ms budget; only 0/30 Enter timings |
| agent | mode_proof | PASS | screenshot only (no OCR on the VM): shots\agent_proof.png |
| agent | fast_typing | FAIL | fast typing: 0/30 lines, exact match false |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 0/6 exact lines |
| agent | ctrl_c | FAIL | Ctrl-C delivered 0/5 times |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 1793586220536 ms over conhost (powershell.exe) exceeds the 2 ms budget; only 0/30 Enter timings |

## vs previous run

first run in the archive
