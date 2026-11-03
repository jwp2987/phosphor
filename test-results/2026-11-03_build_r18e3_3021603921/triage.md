# Triage: r18e3 (pipeline 3021603921, commit 30d6229f0; unit-only fix tier)

## Unit failures (round 4 of #1603)
- terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell #1603
- terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold #1603
- terminal::view::tests::ssh_in_the_agent_view_on_an_unready_shell_is_routed_to_the_shell_and_held_not_sent_to_the_agent #1608
