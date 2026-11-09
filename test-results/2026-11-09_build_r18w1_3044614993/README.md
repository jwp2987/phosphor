# build/r18w1  c0ed28adf  (pipeline 3044614993)

- date: 2026-11-09, tier: fix, pipeline status: manual
- compile (check-linux): ok, 107 warnings
- unit: 4416 passed, 0 failed, 26 skipped of 4416 run (2 suites)
- app (pass/fail/skip): macos 0/18/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | failed | 0.1 | - |  |
| app-test-lab-linux | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | manual | - | - | not run |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 0.9 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | manual | - | - | not run |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | failed | 30.2 | - |  |
| build-macos-arm64 | success | 19.0 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | failed | 14.0 | phosphor-windows-vm |  |
| check-linux | success | 10.6 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 2.0 | - |  |
| guards-helper-deps | success | 0.3 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | manual | - | - | not run |
| test-linux | failed | 23.8 | - |  |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.3 | - |  |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | success | 2.3 | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp ai::agent_providers::tools::web_runtime::websearch_tests::real_exa_anonymous_search` FAIL (0.3s)
- `warp ai::blocklist::agent_fuzz::corpus_replay_tests::corpus_replays_as_listed_and_reaches_the_wave0_cells` FAIL (48.1s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | cadence | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | key_loss_control | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | enter_loss_control | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | fast_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | paste_instant_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | tab_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | ctrl_c_d | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | ssh_connect_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | mode_proof | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | cadence | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | fast_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | paste_instant_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | tab_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | ctrl_c_d | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | ssh_connect_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | harness_crash | FAIL | the agent pass crashed outside a case: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/mode_proof`
  - `macos/terminal/cadence`
  - `macos/terminal/key_loss_control`
  - `macos/terminal/enter_loss_control`
  - `macos/terminal/fast_typing`
  - `macos/terminal/paste_instant_enter`
  - `macos/terminal/tab_enter`
  - `macos/terminal/harness_crash (case not in previous run)`
  - `macos/agent/mode_proof`
  - `macos/agent/cadence`
  - `macos/agent/fast_typing`
  - `macos/agent/paste_instant_enter`
  - `macos/agent/tab_enter`
  - `macos/agent/harness_crash (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-warpui-core: build/r18e4 (3021881600); test-workspace-rest: build/r18u5 (3025066346); app macos: build/r18e4 (3021881600)
