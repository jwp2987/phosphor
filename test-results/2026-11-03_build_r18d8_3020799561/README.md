# build/r18d8  9625b716c  (pipeline 3020799561)

- date: 2026-11-03, tier: release, pipeline status: manual
- compile (check-linux): ok, 52 warnings
- unit: 14756 passed, 34 failed, 55 skipped of 14790 run (3 suites)
- app (pass/fail/skip): macos 0/2/0; x11 0/0/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | failed | 95.0 | - | no results artifact (job failed) |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | failed | 0.2 | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 4.6 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | manual | - | - | not run |
| app-test-windows-reuse | manual | - | - | not run |
| bench | success | 23.4 | - |  |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 33.5 | - |  |
| build-macos-arm64 | success | 19.9 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | canceled | 142.6 | phosphor-windows-vm |  |
| check-linux | success | 9.1 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | failed | 6.9 | - |  |
| guards | success | 1.9 | - |  |
| guards-windows | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | success | 15.1 | - |  |
| mutants-warpui-core | success | 65.6 | - |  |
| test-linux | failed | 9.7 | - |  |
| test-shuttle-loom | failed | 2.5 | - |  |
| test-warpui-core | success | 1.4 | - |  |
| test-workspace-rest | success | 2.2 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp code::editor::model::tests::an_editor_without_a_diff_base_has_none_to_compute_against` FAIL (0.2s)
- `warp terminal::bootstrap::tests::bundled_bash_body_has_one_local_only_block_holding_the_ssh_wrapper` FAIL (0.0s)
- `warp terminal::bootstrap::tests::bundled_bootstrap_bodies_are_safe_to_read_through_a_cooked_tty` FAIL (0.0s)
- `warp terminal::bootstrap::tests::typed_bootstrap_lines_fit_macos_max_canon` FAIL (0.0s)
- `warp terminal::input::tests::test_cursor_keys_behind_a_large_paste_apply_after_the_whole_paste` FAIL (21.2s)
- `warp terminal::input::tests::test_enter_queue_tab_enter_then_typing_releases_the_keys_after_the_completed_line` FAIL (5.3s)
- `warp terminal::input::tests::test_live_end_after_an_edit_waits_for_the_next_layout` FAIL (0.2s)
- `warp terminal::input::tests::test_ready_paste_then_immediate_enter_classifies_the_pasted_line` FAIL (2.9s)
- `warp terminal::input::tests::test_text_typed_during_the_verdict_wait_never_merges_into_the_held_enter` FAIL (5.3s)
- `warp terminal::input::tests::test_unready_deferred_enter_keeps_both_later_lines_behind_it_in_order` FAIL (5.3s)
- `warp terminal::input::tests::test_unready_direct_enter_then_a_second_line_and_enter_run_as_two_commands` FAIL (5.3s)
- `warp terminal::model::terminal_model::tests::full_reset_clears_app_cursor_and_kitty_flags_and_publishes_them_1599` FAIL (0.0s)
- `warp terminal::model::typeahead_handoff::tests::a_committed_delivery_is_never_asked_for_and_stays_in_the_shell` FAIL (0.0s)
- `warp terminal::model::typeahead_handoff::tests::the_committed_delivery_mark_ends_when_its_line_cannot_hold_it` FAIL (0.0s)
- `warp terminal::view::tests::a_line_delivered_to_the_shell_as_final_stays_in_the_shell` FAIL (0.1s)
- `warp terminal::view::tests::a_prompt_hold_cap_that_fires_while_the_queue_is_busy_still_releases_the_keys` FAIL (0.2s)
- `warp terminal::view::tests::a_report_into_an_ai_mode_input_is_kept_as_shell_text_and_runs_once` FAIL (0.2s)
- `warp terminal::view::tests::group1_handover_tests::an_agent_write_does_not_overtake_user_bytes_waiting_in_the_queue` FAIL (0.2s)
- `warp terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell` FAIL (0.2s)
- `warp terminal::view::tests::group1_handover_tests::release_without_event_hands_over_before_the_next_key` FAIL (0.2s)
- `warp terminal::view::tests::group2_timer_tests::ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering` FAIL (0.2s)
- `warp terminal::view::tests::group2_timer_tests::uncovered_report_patience_keeps_the_text_instead_of_running_the_enter` FAIL (0.1s)
- `warp terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold` FAIL (0.1s)
- `warp terminal::view::tests::group4_prompt_tests::enter_on_a_pending_approval_card_stays_with_the_card` FAIL (0.2s)
- `warp terminal::view::tests::group4_prompt_tests::ssh_warpify_on_a_clean_line_writes_at_once` FAIL (0.1s)
- `warp terminal::view::tests::group4_prompt_tests::ssh_warpify_waits_for_a_line_holding_delivered_keys` FAIL (0.2s)
- `warp terminal::view::tests::group4_prompt_tests::subshell_bootstrap_waits_for_a_line_holding_delivered_keys` FAIL (0.2s)
- `warp terminal::view::tests::nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell` FAIL (0.2s)
- `warp terminal::view::tests::shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences` FAIL (0.2s)
- `warp terminal::view::tests::ssh_bootstrap_failure_hands_held_typeahead_back_without_stty_sane` FAIL (0.1s)
- `warp terminal::view::tests::ssh_ctrl_c_ends_the_hold` FAIL (0.1s)
- `warp terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys` FAIL (0.2s)
- `warp terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy` FAIL (0.5s)
- `warp terminal::view::tests::typeahead_handoff_returns_an_unfinished_held_line_to_the_editor_when_the_submit_runs_nothing` FAIL (0.1s)

## test-shuttle-loom failures

- error: error[E0277]: `sync_shim::model::MutexGuard<'_, Vec<u64>>` doesn't implement `std::fmt::Debug`
- error: error: could not compile `warpui_core` (lib test) due to 1 previous error; 1 warning emitted
- error: ERROR: Job failed: exit status 101

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | launch | FAIL | no shell prompt within 120s (see logs/app.stdout; GPU/Wayland init or keyring?) |
| agent | launch | FAIL | no shell prompt within 120s (see logs/app.stdout; GPU/Wayland init or keyring?) |

## app x11 (app-test-linux-x11)


| mode | case | status | detail |
|---|---|---|---|

## vs previous run

- NEW failures (unit): 
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
  - `warp terminal::view::tests::a_line_delivered_to_the_shell_as_final_stays_in_the_shell [test-linux]`
  - `warp terminal::view::tests::a_prompt_hold_cap_that_fires_while_the_queue_is_busy_still_releases_the_keys [test-linux]`
  - `warp terminal::view::tests::a_report_into_an_ai_mode_input_is_kept_as_shell_text_and_runs_once [test-linux]`
  - `warp terminal::view::tests::group1_handover_tests::an_agent_write_does_not_overtake_user_bytes_waiting_in_the_queue [test-linux]`
  - `warp terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell [test-linux]`
  - `warp terminal::view::tests::group1_handover_tests::release_without_event_hands_over_before_the_next_key [test-linux]`
  - `warp terminal::view::tests::group2_timer_tests::ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering [test-linux]`
  - `warp terminal::view::tests::group2_timer_tests::uncovered_report_patience_keeps_the_text_instead_of_running_the_enter [test-linux]`
  - `warp terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::enter_on_a_pending_approval_card_stays_with_the_card [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::ssh_warpify_on_a_clean_line_writes_at_once [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::ssh_warpify_waits_for_a_line_holding_delivered_keys [test-linux]`
  - `warp terminal::view::tests::group4_prompt_tests::subshell_bootstrap_waits_for_a_line_holding_delivered_keys [test-linux]`
  - `warp terminal::view::tests::nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell [test-linux]`
  - `warp terminal::view::tests::shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences [test-linux]`
  - `warp terminal::view::tests::ssh_bootstrap_failure_hands_held_typeahead_back_without_stty_sane [test-linux]`
  - `warp terminal::view::tests::ssh_ctrl_c_ends_the_hold [test-linux]`
  - `warp terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys [test-linux]`
  - `warp terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy [test-linux]`
  - `warp terminal::view::tests::typeahead_handoff_returns_an_unfinished_held_line_to_the_editor_when_the_submit_runs_nothing [test-linux]`
- FIXED failures (unit): 
  - `warp terminal::view::tests::keys_held_across_the_prompt_all_go_to_the_pty_once_and_the_editor_stays_empty [test-linux]`
  - `warp terminal::view::tests::ssh_bootstrap_timer_second_expiry_after_the_grace_still_fails_the_session [test-linux]`
  - `warp terminal::view::tests::ssh_handover_holds_the_text_after_a_nested_ssh_for_the_second_connect [test-linux]`
  - `warp terminal::view::tests::ssh_line_typed_across_gate_release_is_never_run_locally [test-linux]`
  - `warp terminal::view::tests::table_mode_a_control_key_behind_a_gate_is_delivered_at_the_gates_event [test-linux]`
  - `warp terminal::view::tests::table_mode_a_ctrl_c_in_the_ssh_setup_window_is_held_and_reaches_the_remote_shell_once [test-linux]`
  - `warp terminal::view::tests::table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover [test-linux]`
  - `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted [test-workspace-rest]`
  - `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable [test-workspace-rest]`
- NEW failures (app): 
  - `macos/terminal/launch (case not in previous run)`
  - `macos/agent/launch (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18d2 (3020063118); test-warpui-core: build/r18d4 (3020340107); test-workspace-rest: build/r18d4 (3020340107); app macos: build/r18d2 (3020063118)
