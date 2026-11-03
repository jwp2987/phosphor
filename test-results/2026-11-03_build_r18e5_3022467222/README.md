# build/r18e5  8a2b9c39b  (pipeline 3022467222)

- date: 2026-11-03, tier: fix, pipeline status: manual
- compile (check-linux): ok, 52 warnings
- unit: 14022 passed, 2 failed, 48 skipped of 14024 run (2 suites)
- app (pass/fail/skip): -; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.3 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 7.6 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.9 | - |  |
| guards-windows | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 8.6 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | success | 1.4 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell` FAIL (0.2s)
- `warp terminal::view::tests::ssh_in_the_agent_view_on_an_unready_shell_is_routed_to_the_shell_and_held_not_sent_to_the_agent` FAIL (1.2s)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): 
  - `warp terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold [test-linux]`
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18e4 (3021881600); test-workspace-rest: build/r18e4 (3021881600)
