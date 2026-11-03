# build/r18d9  d8ec2d194  (pipeline 3020981970)

- date: 2026-11-03, tier: fix, pipeline status: manual
- compile (check-linux): ok, 52 warnings
- unit: 14008 passed, 7 failed, 48 skipped of 14015 run (2 suites)
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
| check-linux | success | 6.4 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.8 | - |  |
| guards-windows | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 7.5 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | success | 2.1 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::input::tests::test_padded_ready_paste_then_immediate_enter_runs_on_time_and_hands_back_when_late` FAIL (1.6s)
- `warp terminal::view::tests::a_line_delivered_to_the_shell_as_final_stays_in_the_shell` FAIL (0.1s)
- `warp terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell` FAIL (0.2s)
- `warp terminal::view::tests::group2_timer_tests::ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering` FAIL (0.1s)
- `warp terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold` FAIL (0.1s)
- `warp terminal::view::tests::nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell` FAIL (1.2s)
- `warp terminal::view::tests::shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences` FAIL (0.3s)

## vs previous run

- NEW failures (unit): 
  - `warp terminal::input::tests::test_padded_ready_paste_then_immediate_enter_runs_on_time_and_hands_back_when_late [test-linux]`
- FIXED failures (unit): 
  - `warp code::editor::model::tests::an_editor_without_a_diff_base_has_none_to_compute_against [test-linux]`
  - `warp terminal::bootstrap::tests::bundled_bash_body_has_one_local_only_block_holding_the_ssh_wrapper [test-linux]`
  - `warp terminal::bootstrap::tests::bundled_bootstrap_bodies_are_safe_to_read_through_a_cooked_tty [test-linux]`
  - `warp terminal::bootstrap::tests::typed_bootstrap_lines_fit_macos_max_canon [test-linux]`
  - `warp terminal::input::tests::test_cursor_keys_behind_a_large_paste_apply_after_the_whole_paste [test-linux]`
  - `warp terminal::input::tests::test_enter_queue_tab_enter_then_typing_releases_the_keys_after_the_completed_line [test-linux]`
  - `warp terminal::input::tests::test_live_end_after_an_edit_waits_for_the_next_layout [test-linux]`
  - `warp terminal::input::tests::test_ready_paste_then_immediate_enter_classifies_the_pasted_line [test-linux]`
  - `warp terminal::input::tests::test_text_typed_during_the_verdict_wait_never_merges_into_the_held_enter [test-linux]`
  - `warp terminal::input::tests::test_unready_deferred_enter_keeps_both_later_lines_behind_it_in_order [test-linux]`
  - `warp terminal::input::tests::test_unready_direct_enter_then_a_second_line_and_enter_run_as_two_commands [test-linux]`
  - `warp terminal::model::terminal_model::tests::full_reset_clears_app_cursor_and_kitty_flags_and_publishes_them_1599 [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::a_committed_delivery_is_never_asked_for_and_stays_in_the_shell [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::the_committed_delivery_mark_ends_when_its_line_cannot_hold_it [test-linux]`
  - `warp terminal::view::tests::a_prompt_hold_cap_that_fires_while_the_queue_is_busy_still_releases_the_keys [test-linux]`
  - `warp terminal::view::tests::a_report_into_an_ai_mode_input_is_kept_as_shell_text_and_runs_once [test-linux]`
  - `warp terminal::view::tests::group1_handover_tests::an_agent_write_does_not_overtake_user_bytes_waiting_in_the_queue [test-linux]`
  - `warp terminal::view::tests::group1_handover_tests::release_without_event_hands_over_before_the_next_key [test-linux]`
  - `warp terminal::view::tests::group2_timer_tests::uncovered_report_patience_keeps_the_text_instead_of_running_the_enter [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::enter_on_a_pending_approval_card_stays_with_the_card [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::ssh_warpify_on_a_clean_line_writes_at_once [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::ssh_warpify_waits_for_a_line_holding_delivered_keys [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::subshell_bootstrap_waits_for_a_line_holding_delivered_keys [test-linux]`
  - `warp terminal::view::tests::ssh_bootstrap_failure_hands_held_typeahead_back_without_stty_sane [test-linux]`
  - `warp terminal::view::tests::ssh_ctrl_c_ends_the_hold [test-linux]`
  - `warp terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys [test-linux]`
  - `warp terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy [test-linux]`
  - `warp terminal::view::tests::typeahead_handoff_returns_an_unfinished_held_line_to_the_editor_when_the_submit_runs_nothing [test-linux]`
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18d8 (3020799561); test-workspace-rest: build/r18d8 (3020799561)
