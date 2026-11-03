# build/r18e2  95a9705c4  (pipeline 3021361500)

- date: 2026-11-03, tier: fix, pipeline status: manual
- compile (check-linux): ok, 52 warnings
- unit: 14012 passed, 3 failed, 48 skipped of 14015 run (2 suites)
- app (pass/fail/skip): -; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
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
| check-linux | success | 7.2 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.4 | - |  |
| guards-windows | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.4 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 7.4 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | success | 1.5 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell` FAIL (0.2s)
- `warp terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold` FAIL (0.1s)
- `warp terminal::view::tests::nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell` FAIL (1.2s)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): 
  - `warp terminal::input::tests::test_padded_ready_paste_then_immediate_enter_runs_on_time_and_hands_back_when_late [test-linux]`
  - `warp terminal::view::tests::a_line_delivered_to_the_shell_as_final_stays_in_the_shell [test-linux]`
  - `warp terminal::view::tests::group2_timer_tests::ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering [test-linux]`
  - `warp terminal::view::tests::shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences [test-linux]`
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18d9 (3020981970); test-workspace-rest: build/r18d9 (3020981970)
