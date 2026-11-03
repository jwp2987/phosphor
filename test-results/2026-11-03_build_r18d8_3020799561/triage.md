# Triage: r18d8 (pipeline 3020799561, commit 9625b716c)

## Unit failures (test-linux): all fixed in d8ec2d194, tracked in #1603
- code::editor::model::tests::an_editor_without_a_diff_base_has_none_to_compute_against #1603
- terminal::bootstrap::tests::bundled_bash_body_has_one_local_only_block_holding_the_ssh_wrapper #1603
- terminal::bootstrap::tests::bundled_bootstrap_bodies_are_safe_to_read_through_a_cooked_tty #1603
- terminal::bootstrap::tests::typed_bootstrap_lines_fit_macos_max_canon #1603
- terminal::input::tests::test_cursor_keys_behind_a_large_paste_apply_after_the_whole_paste #1603
- terminal::input::tests::test_enter_queue_tab_enter_then_typing_releases_the_keys_after_the_completed_line #1603
- terminal::input::tests::test_live_end_after_an_edit_waits_for_the_next_layout #1603
- terminal::input::tests::test_ready_paste_then_immediate_enter_classifies_the_pasted_line #1603
- terminal::input::tests::test_text_typed_during_the_verdict_wait_never_merges_into_the_held_enter #1603
- terminal::input::tests::test_unready_deferred_enter_keeps_both_later_lines_behind_it_in_order #1603
- terminal::input::tests::test_unready_direct_enter_then_a_second_line_and_enter_run_as_two_commands #1603
- terminal::model::terminal_model::tests::full_reset_clears_app_cursor_and_kitty_flags_and_publishes_them_1599 #1603
- terminal::model::typeahead_handoff::tests::a_committed_delivery_is_never_asked_for_and_stays_in_the_shell #1603
- terminal::model::typeahead_handoff::tests::the_committed_delivery_mark_ends_when_its_line_cannot_hold_it #1603
- terminal::view::tests::a_line_delivered_to_the_shell_as_final_stays_in_the_shell #1603
- terminal::view::tests::a_prompt_hold_cap_that_fires_while_the_queue_is_busy_still_releases_the_keys #1603
- terminal::view::tests::a_report_into_an_ai_mode_input_is_kept_as_shell_text_and_runs_once #1603
- terminal::view::tests::group1_handover_tests::an_agent_write_does_not_overtake_user_bytes_waiting_in_the_queue #1603
- terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell #1603
- terminal::view::tests::group1_handover_tests::release_without_event_hands_over_before_the_next_key #1603
- terminal::view::tests::group2_timer_tests::ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering #1603
- terminal::view::tests::group2_timer_tests::uncovered_report_patience_keeps_the_text_instead_of_running_the_enter #1603
- terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold #1603
- terminal::view::tests::group4_prompt_tests::enter_on_a_pending_approval_card_stays_with_the_card #1603
- terminal::view::tests::group4_prompt_tests::ssh_warpify_on_a_clean_line_writes_at_once #1603
- terminal::view::tests::group4_prompt_tests::ssh_warpify_waits_for_a_line_holding_delivered_keys #1603
- terminal::view::tests::group4_prompt_tests::subshell_bootstrap_waits_for_a_line_holding_delivered_keys #1603
- terminal::view::tests::nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell #1603
- terminal::view::tests::shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences #1603
- terminal::view::tests::ssh_bootstrap_failure_hands_held_typeahead_back_without_stty_sane #1603
- terminal::view::tests::ssh_ctrl_c_ends_the_hold #1603
- terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys #1603
- terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy #1603
- terminal::view::tests::typeahead_handoff_returns_an_unfinished_held_line_to_the_editor_when_the_submit_runs_nothing #1603

## Other jobs
- build-windows-x86_64: Inno Setup compile hung after the last file, cancelled (#1602); app-test-windows not run.
- app-test-macos: both modes blocked at launch, zsh bootstrap line over macOS MAX_CANON (#1603, fixed in d8ec2d194); launch-failure logging improved (e791c8497).
- app-test-linux-wayland: killed at the 90 min job timeout, no artifacts; harness lifecycle/accuracy fixes merged (09dc24b0b). Nothing verifiable.
- app-test-linux-x11: harness prerequisites missing on the box (xsel, ffmpeg/imagemagick, gnome-keyring-daemon); installing needs owner approval.
- test-shuttle-loom, fuzz-smoke: compile errors (MutexGuard Debug; fuzz workspace lacked the root [patch.crates-io]), fixed in e14eec1d2.
