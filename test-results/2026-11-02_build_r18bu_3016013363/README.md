# build/r18bu  297b58f30  (pipeline 3016013363)

- date: 2026-11-02, tier: fix, pipeline status: running (collected before it finished)
- compile (check-linux): ok, 63 warnings
- unit: 10419 passed, 10 failed, 29 skipped of 10429 run (1 suites)
- app (pass/fail/skip): -; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.3 | - |  |
| app-test-linux-wayland | canceled | - | - | no results artifact (job canceled) |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | canceled | - | - | no results artifact (job canceled) |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | manual | - | - | not run |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | canceled | - | - | no results artifact (job canceled) |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | running | 16.0 | - |  |
| build-macos-arm64 | success | 20.3 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | running | 25.5 | phosphor-windows-vm |  |
| check-linux | success | 8.8 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.2 | - |  |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 8.9 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | canceled | - | - |  |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::input_queue::tests::a_full_queue_still_takes_a_signal` FAIL (0.0s)
- `warp terminal::view::tests::agent_pty_writes_are_refused_during_a_subshell_setup` FAIL (0.1s)
- `warp terminal::view::tests::an_ssh_gate_drop_under_a_busy_queue_is_not_lost` FAIL (0.1s)
- `warp terminal::view::tests::key_path_helpers_do_not_wait_for_the_parser` FAIL (0.1s)
- `warp terminal::view::tests::programmatic_insert_waits_for_a_waiting_enter` FAIL (0.1s)
- `warp terminal::view::tests::programmatic_submits_wait_for_a_waiting_enter` FAIL (0.1s)
- `warp terminal::view::tests::ssh_editor_backspace_in_the_gap_edits_the_held_text` FAIL (0.1s)
- `warp terminal::view::tests::ssh_handover_enter_guard_does_not_swallow_enter_for_a_replaced_line` FAIL (0.8s)
- `warp terminal::view::tests::ssh_held_and_editor_queued_keys_are_delivered_in_order_at_the_precmd_event` FAIL (0.6s)
- `warp terminal::view::tests::synced_input_is_held_not_written_into_an_open_hand_over_gate` FAIL (0.1s)

## vs previous run

- NEW failures (unit): 
  - `warp terminal::input_queue::tests::a_full_queue_still_takes_a_signal [test-linux]`
  - `warp terminal::view::tests::agent_pty_writes_are_refused_during_a_subshell_setup [test-linux]`
  - `warp terminal::view::tests::an_ssh_gate_drop_under_a_busy_queue_is_not_lost [test-linux]`
  - `warp terminal::view::tests::key_path_helpers_do_not_wait_for_the_parser [test-linux]`
  - `warp terminal::view::tests::programmatic_insert_waits_for_a_waiting_enter [test-linux]`
  - `warp terminal::view::tests::programmatic_submits_wait_for_a_waiting_enter [test-linux]`
  - `warp terminal::view::tests::ssh_editor_backspace_in_the_gap_edits_the_held_text [test-linux]`
  - `warp terminal::view::tests::ssh_held_and_editor_queued_keys_are_delivered_in_order_at_the_precmd_event [test-linux]`
  - `warp terminal::view::tests::synced_input_is_held_not_written_into_an_open_hand_over_gate [test-linux]`
- FIXED failures (unit): 
  - `warp code::editor::view::vim_handler::tests::test_vim_system_register_paste_holds_the_rest_of_the_same_input [test-linux]`
  - `warp code::editor::view::vim_handler::tests::test_vim_system_register_visual_paste_waits_for_the_read [test-linux]`
  - `warp terminal::model::terminal_model::tests::line_editor_markers_update_the_handoffs_account_in_stream_order [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::a_signal_with_no_wait_pending_releases_nothing [test-linux]`
  - `warp terminal::view::tests::a_user_key_does_not_wait_for_the_parser [test-linux]`
  - `warp terminal::view::tests::test_after_enter_replay_drains_in_typed_order [test-linux]`
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18bn (3015761186)
