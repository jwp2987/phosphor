# build/r18d4  86aa856a6  (pipeline 3020340107)

- date: 2026-11-03, tier: fix, pipeline status: manual
- compile (check-linux): ok, 52 warnings
- unit: 4025 passed, 2 failed, 26 skipped of 4027 run (2 suites)
- app (pass/fail/skip): macos 0/0/0; wayland 0/0/0; windows 1/4/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | failed | 0.3 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 0.4 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | failed | 2.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 34.5 | - |  |
| build-macos-arm64 | success | 19.9 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 35.2 | phosphor-windows-vm |  |
| check-linux | success | 7.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.8 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 5.9 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | success | 1.4 | - |  |
| test-workspace-rest | failed | 2.5 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- error: error[E0004]: non-exhaustive patterns: `&FormattedTextLine::BlockQuote { .. }` not covered
- error: error: could not compile `warp_tui` (lib test) due to 1 previous error
- error: error: command `/home/<user>/.rustup/toolchains/1.92.0-x86_64-unknown-linux-gnu/bin/cargo test --no-run --message-format json-render-diagnostics --package warp --package warp_tui --lib --features war
- error: error: nextest exited 101 (not 0=passed / 100=test failures).
- error: ERROR: Job failed: exit status 2

## test-workspace-rest failures

- `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable` FAIL (0.2s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| terminal@gitbash | harness_crash | FAIL | the terminal@gitbash pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| agent | harness_crash | FAIL | the agent pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| agent@gitbash | harness_crash | FAIL | the agent@gitbash pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `windows/terminal/harness_crash (case not in previous run)`
  - `windows/terminal@gitbash/harness_crash (case not in previous run)`
  - `windows/agent/harness_crash (case not in previous run)`
  - `windows/agent@gitbash/harness_crash (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-warpui-core: build/r18ct (3019107009); test-workspace-rest: build/r18ct (3019107009); app windows: build/r18d2 (3020063118)
