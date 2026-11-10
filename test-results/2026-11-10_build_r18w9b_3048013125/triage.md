# Triage: r18w9b (pipeline 3048013125, build/r18w9b 413cd4241)

## Unit failures (test-linux: 11,818 tests, 40 failed)

All 40 were addressed on integ/r10 after this run (fixes unproven until the next run); verdicts and evidence in docs/design/r10-unit-*.md.

- ai::agent::conversation::tests::a_restored_turn_with_every_tool_call_answered_is_successful #2010: TEST: fixture tested a shape that was never Success; #1951 exemption
- ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants_with_the_allowlist_on #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::matrix::pairs_hung_command::off #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::matrix::pairs_hung_command::on #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::matrix::triples_hung_command::off #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::matrix::triples_hung_command::on #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::matrix::triples_read_files::off #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::matrix::triples_read_files::on #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- ai::blocklist::agent_fuzz::stop_tests::a_reject_after_a_finished_result_tells_the_model_at_once_and_the_agent_continues #2009: APP: BYOP readiness never found the live state of a later call of a multi-call reply
- ai::blocklist::agent_fuzz::stop_tests::a_stop_after_a_finished_result_opens_no_request_and_the_results_ride_on_the_next_message #2010: oracle/test fault (I2b/I4/I8 reading), see docs/design/r10-unit-lifecycle.md
- terminal::bootstrap::tests::bash_body_does_not_trap_sighup #1933: APP: the one-connection login script trapped HUP
- terminal::cli_agent_sessions::plugin_manager::claude::tests::a_failed_add_never_re_fetches_a_remote_previous_source #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::claude::tests::fresh_install_adds_the_bundled_folder_then_installs #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::claude::tests::install_over_a_remote_registration_replaces_it_with_the_bundle #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::claude::tests::move_re_registers_from_the_bundle_and_clears_the_offer #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::claude::tests::update_re_registers_the_bundle_with_no_network_step #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::codex::tests::a_failed_add_never_re_fetches_a_remote_previous_source #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::codex::tests::fresh_install_adds_the_bundled_folder_then_the_plugin #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::codex::tests::install_over_a_remote_registration_replaces_it_with_the_bundle #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::codex::tests::move_re_registers_from_the_bundle_and_clears_the_offer #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::codex::tests::update_re_registers_a_remote_marketplace_from_the_bundle_with_no_network_step #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::gemini::tests::fresh_install_installs_the_bundled_folder #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::gemini::tests::move_reinstalls_from_the_bundle #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::cli_agent_sessions::plugin_manager::gemini::tests::update_replaces_a_remote_install_with_the_bundle_and_clears_the_offer #2008: TEST: the box temp dir /home/gitlab-runner matched the "gitlab" remote word
- terminal::input::tests::editing_the_buffer_during_a_secret_read_never_requests_a_query_prediction #1927: TEST: the count raced a background thread
- terminal::model::typeahead_handoff::tests::a_command_the_latch_calls_interactive_gets_its_enter_and_releases_a_live_hold #1889: stale setup after #1889
- terminal::model::typeahead_handoff::tests::an_unengaged_user_command_makes_a_young_hold_follow_not_drop #1889: TEST: a bare Enter on an empty line is never a young hold
- terminal::model::typeahead_handoff::tests::keys_a_hold_gave_up_on_never_start_another_young_hold #1996: TEST: setup and literal enters_written
- terminal::model::typeahead_handoff::tests::the_prompt_commits_to_the_editor_when_no_keys_are_held #1923: stale expectation, owner decision #1923
- terminal::view::tests::a_users_own_command_cancels_the_armed_setup_and_nothing_is_written #1905: TEST: fixture stale since 37295f950
- terminal::view::tests::a_young_hold_is_written_as_typeahead_when_the_command_becomes_interactive #1889: stale setup after #1889
- terminal::view::tests::an_idle_dash_prompt_with_nothing_typed_is_written_on_the_look #1935: TEST: the closing check counted the user own x as a setup write
- terminal::view::tests::keys_behind_a_run_enter_are_written_to_the_pty_at_the_net_in_both_modes #1996: TEST: expected ls\r (#1996)
- terminal::view::tests::open_pane_mid_batch_cancel_sends_the_follow_up #2010: stale expectation, #1942 decision (Reject sends a request, a user stop sends none)
- terminal::view::tests::paste_during_a_subshell_setup_is_held_behind_earlier_keys_in_both_modes #1853: TEST: the pane had no running command
- terminal::view::tests::pty_paste_backstop_gives_up_then_the_late_result_is_ignored #1996: stale expectation, #1996 decision 2
- terminal::view::tests::route_parity_a_tests::suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does #1852: APP (Legacy chain only): ^C as a control sequence did not void a waiting Enter
- terminal::view::tests::stalled_pty_paste_hands_held_keys_back_in_order_at_the_backstop #1996: stale expectation, #1996 decision 2
- terminal::view::tests::the_readiness_tail_is_the_cursor_row_not_the_blank_rows_below_a_clear #1934: TEST: fixture too tall for the 7x10 mock terminal
