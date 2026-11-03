# Triage: r18d9 (pipeline 3020981970, commit d8ec2d194; unit-only fix tier, no builds)

## Unit failures (round 2 of #1603)
- terminal::input::tests::test_padded_ready_paste_then_immediate_enter_runs_on_time_and_hands_back_when_late #1603
- terminal::view::tests::a_line_delivered_to_the_shell_as_final_stays_in_the_shell #1603
- terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell #1603
- terminal::view::tests::group2_timer_tests::ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering #1603
- terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold #1603
- terminal::view::tests::nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell #1603
- terminal::view::tests::shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences #1603
