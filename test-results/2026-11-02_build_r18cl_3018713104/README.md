# build/r18cl  44f229f3b  (pipeline 3018713104)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 46 warnings
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): macos 8/4/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | canceled | 14.3 | MacBookPro-gitrunner |  |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 3.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.6 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## app macos (app-test-macos-reuse, REUSED build)

reuse: `{"reused_build": {"job_id": 17667172373, "job_name": "build-macos-arm64", "pipeline_id": "3018144637", "commit": "e01e2afad5f0ec958a271cfe949eed8bdc56fe8b", "this_pipeline": "3018713104", "this_commit": "44f229f3ba84270398d494591a35f69a31222b7b", "fetched": "local cache (/Users/<user>/.cache/phospho`

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference Terminal. |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T163_c71lmo never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T164_hi6hr3 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T165_mdx3vw never reached the REMOTE file (s |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/ssh_enter_then_immediate_command (case not in previous run)`
  - `macos/terminal/ssh_nested_typeahead (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/key_loss_control`
  - `macos/terminal/ctrl_c_d`
- Latency regressions > 5%: none
- compared against: app macos: build/r18ci (3018380432)
