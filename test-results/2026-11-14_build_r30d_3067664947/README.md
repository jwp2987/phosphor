# build/r30d  2355e1da0  (pipeline 3067664947)

- date: 2026-11-14, tier: targeted, pipeline status: success
- compile (check-linux): ok, 125 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_connect_typeahead.zsh.*`, `lab_chips.zsh.lan.nohelper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_run_command.bash.lan.nohelper`
  - asked for on macos-lab: `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.zsh.*`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.zsh.*`, `lab_tab_no_helper.zsh.*`, `lab_tab_remote_dir.zsh.*`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_run_command.bash.lan.nohelper`
  - asked for on windows: `agent_close_undo_call_status`, `agent_latency_enter`, `agent_latency_key`, `agent_lrc_no_false_takeover`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `alt_screen_programs`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `cb_agent_pickers`, `cb_ui_action_latency`, `ctrl_c_d`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `editor_memory_7mb`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `flood_scrollback_find`, `focus_cycle`, `gitbash_subshell`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `idle_cpu`, `ime_compose`, `intl_altgr_bash`, `intl_altgr_ps`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `job_control`, `key_loss_control`, `keybinding_rebind`, `keydown_count`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `mode_proof`, `network_gate_switch`, `output_frames`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `rollover`, `routing_plain_installed_head`, `routing_table`, `routing_table_powershell`, `routing_typo_head`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_connect_typing`, `ssh_connect_typing_wrapper`, `ssh_drop_reconnect`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead`, `ssh_nested_typeahead_wrapper`, `startup_shell_pane`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `tab_lifecycle_keys`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `wheel_scroll`, `zero_network_startup`, `agent_basics_powershell`, `first_run_defaults`
  - asked for on windows-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.*`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_managed_connect.*`, `lab_msys_ssh_switch.*`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_remote_session.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.*`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
- unit: 50216 passed, 0 failed, 191 skipped of 50216 run (9 suites)
- app (pass/fail/skip): lab-macos 23/18/0 +29 no-ref; lab-wayland 9/9/0; windows 148/8/190 +19 no-ref; soak: pass (windows)

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 8.0 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 41.9 | MacBookPro-gitrunner |  |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | manual | - | - | not run |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | manual | - | - | not run |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | canceled | 228.7 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 29.3 | - |  |
| build-macos-arm64 | success | 19.3 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 38.9 | phosphor-windows-vm |  |
| check-linux | success | 7.9 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 8.6 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| lab-helper | success | 0.1 | - |  |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.4 | - |  |
| test-linux | success | 22.1 | - |  |
| test-macos | success | 29.1 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.2 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.1 | phosphor-windows-vm |  |
| test-windows | success | 47.9 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.4 | - |  |
| test-workspace-rest-macos | success | 3.5 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.3 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on` LEAK (52.6s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on` LEAK (52.5s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::on` LEAK (131.3s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (1.0s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.2s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.0s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.4s)
- `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line` LEAK (2.4s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 109 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 399 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a ne |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper | PASS | injector accounting: injector sent 2307; app saw 2307; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2243 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | PASS | injector accounting: injector sent 2330; app saw 2330; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2266 unbound keydown/ty |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t154zhjrsa.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 35 |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 35 |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 161 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new me |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 243 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a new  |
| terminal | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2289 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2289 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_helper_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 134; app saw 134; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 124 unbound keydown/typed,  |
| terminal | lab_helper_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 134; app saw 134; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 124 unbound keydown/typed,  |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt425mh0n0i: app log chip update n |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt428ybyizw: app log chip update n |
| terminal | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 108 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new met |
| agent | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 409 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a new m |
| agent | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 34 |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 34 |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 162 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new metri |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.zsh.lan.helper | PASS | lab_tab_remote_dir.zsh.lan.helper relaunched the app itself (pid 94338 -> 969); a relaunched app starts in a terminal session, so this is no |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 242 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a new met |
| agent | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1910; app saw 1910; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1873 unbound keydown/ty |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | agent_view_not_restored_after_lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead.zsh.lan.helper relaunched the app itself (pid 12737 -> 23199); a relaunched app starts in a terminal session, so this  |
| agent | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | injector accounting: injector sent 225; app saw 225; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 220 unbound keydown/typed,  |
| terminal | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it wi |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | injector accounting: injector sent 2277; app saw 2277; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2215 unbound keydown/ty |
| terminal | rollover | PASS | injector accounting: injector sent 782; app saw 782; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 760 unbound keydown/typed |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal | arrows_behind_paste | PASS | injector accounting: injector sent 132; app saw 132; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 110 unbound keydown/typed,  |
| terminal | keydown_count | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 620 unbound keydown/typed |
| terminal | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| terminal | tab_enter | PASS | injector accounting: injector sent 686; app saw 686; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 660 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 6.38, -> LineEditorReading 30.39; app: |
| terminal | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| terminal | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| terminal | gitbash_subshell | PASS | injector accounting: injector sent 1118; app saw 1118; lost-before-app 0 (Enter: sent 32, app saw 32); of the app's: 1107 unbound keydown/ty |
| terminal | alt_screen_programs | SKIP | not run in the PowerShell pane: vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not inst |
| terminal | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| terminal | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| terminal | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| terminal | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| terminal | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| terminal | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| terminal | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.06 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal | latency_enter | PASS | app share of Enter->start: enter_begin -> pty_write median 1.87 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | routing_table_powershell | FAIL | 'make a PR' (ambiguous) expected shell, went to agent (surface PlainTerminal, rule model); PSReadLine's history got []; injector accounting: |
| terminal | routing_plain_installed_head | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log (#1981): runs in the Git Bash pane |
| terminal | agent_basics_powershell | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; injector accounting: injector sent 212; app saw 212; lost |
| terminal | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| terminal | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| terminal | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| terminal | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| terminal | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| terminal | intl_wide_edit | PASS | edit 0: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; edit 1: rec.l |
| terminal | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| terminal | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git  |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | not run in the PowerShell pane: the one-connection managed ssh sends the bootstrap in the remote command (#2051): only the Git Bash pane has |
| terminal | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| terminal | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: the default and PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane are plain ssh (#1933, #2051): only the Git Bash |
| terminal | ui_palette | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | intl_wide_program | SKIP | not run in the PowerShell pane: `head -n 1 > file` is the receiving program: runs in the Git Bash pane |
| terminal | hostile_output_recovers | SKIP | not run in the PowerShell pane: #2072 (T8): `cat` of a binary file, a POSIX command list around hostile_out.py and rec.log for the exact lin |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | latency_enter_ssh | SKIP | not run in the PowerShell pane: the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` |
| terminal | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal | history_up_recall | PASS | injector accounting: injector sent 151; app saw 151; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 131 unbound keydown/typed,  |
| terminal | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal | ssh_drop_reconnect | SKIP | not run on Windows: not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535) |
| terminal | scroll_keys_position | SKIP | not run in the PowerShell pane: r23 (#2060): bash `cat` of numbered lines, a bash background job, rec.log; the scroll position is read by Wi |
| terminal | ime_compose | SKIP | not run in the PowerShell pane: r15 T39: the committed text is checked as the UTF-8 bytes of a file written by `echo ... >> file` (Windows P |
| terminal | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal | scroll_keys_foreground | SKIP | not run in the PowerShell pane: #2080: the foreground program is a POSIX raw reader started from a bash script (bytes_reader.sh), with a bas |
| terminal | intl_altgr_ps | PASS | injector accounting: injector sent 170; app saw 170; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 167 unbound keydown/typed,  |
| terminal | intl_altgr_bash | NOREF | injector accounting: injector sent 300; app saw 297; lost-before-app 3 (Enter: sent 5, app saw 5); of the app's: 293 unbound keydown/typed,  |
| terminal | ui_mouse_select_copy | SKIP | not run in the PowerShell pane: T31: the token is printed by bash `printf` and the command is checked against rec.log: runs in the Git Bash  |
| terminal | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| terminal | ui_copy_wrapped_drag | SKIP | not run in the PowerShell pane: T31 (#1976): `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in |
| terminal | ui_right_click_paste | SKIP | not run on Windows: not ported: it falls back to OCR for the menu entry and has never passed anywhere (r14-test-gaps F13); on Windows the sa |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| terminal | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal | startup_shell_pane | SKIP | runs in the pane-shell passes only (terminal@pwsh7, terminal@cmd, terminal@wsl: APPCI_WIN_SHELLS of the app-test-windows job names them; a p |
| terminal | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal | resize_reflow_output | SKIP | not run in the PowerShell pane: T33 (r14 T26): a bash SIGWINCH trap, `stty size` and vim: runs in the Git Bash pane (window size by SetWindo |
| terminal | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal | suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: the fixture is printed with `cat` and its command is confirmed from the bash prompt hook; the chip's  |
| terminal | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| terminal | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 48; app saw 48; lost |
| terminal | md_viewer_big | PASS | injector accounting: injector sent 48; app saw 48; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 44 unbound keydown/typed, 3 t |
| terminal | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal | zero_network_startup | SKIP | not run in the PowerShell pane: the network trace does not depend on the pane's shell and one run is about 4 minutes: runs once, in the Git  |
| terminal | editor_memory_7mb | PASS | judged memory figures: private, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); baselines taken  |
| terminal | restore_shows_history | SKIP | not run in the PowerShell pane: r14 T20: the token is printed by bash `printf '%s%s\n' A B` so the command text never holds it, and rec.log  |
| terminal | ui_vertical_tabs_many | SKIP | not run in the PowerShell pane: T33 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| terminal | ui_vertical_tabs_panel_click | SKIP | not run in the PowerShell pane: T31 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| terminal | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal | network_gate_switch | SKIP | not run in the PowerShell pane: the gate does not depend on the pane's shell (three relaunches): runs once, in the Git Bash pane (#2077) |
| terminal | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal | keybinding_rebind | NOREF | NO-REFERENCE: UNVERIFIABLE: this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is ju |
| terminal | soak | PASS | soak: window medians [458, 477, 469] MB, monotonic=True, growth +11 MB (limit 150 MB); injector accounting: injector sent 4386; app saw 4386 |
| terminal | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 35.7 frames/s for 31.5 wakeups/s (0 Full); 4 hidden tabs streaming 2.5 frames/s, 127.3 hidden  |
| terminal | tabs_two_windows | PASS | injector accounting: injector sent 3458; app saw 3458; lost-before-app 0 (Enter: sent 49, app saw 49); of the app's: 3354 unbound keydown/ty |
| terminal | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 2691; app saw 2691; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2608 unbound keydown/ty |
| terminal | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal | meta_and_word_keys | PASS | raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it); injector account |
| terminal | first_run_defaults | PASS | injector accounting: injector sent 618; app saw 618; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 608 unbound keydown/typed,  |
| terminal@gitbash | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 223; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 218 unbound keydown/typed,  |
| terminal@gitbash | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | injector accounting: injector sent 2060; app saw 2060; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1998 unbound keydown/ty |
| terminal@gitbash | rollover | PASS | injector accounting: injector sent 702; app saw 702; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 680 unbound keydown/typed |
| terminal@gitbash | expert_fuzz | FAIL | fuzz: 25/25 lines, mismatches [('fz1ywjnt', 'fz1wyjnt')]; injector accounting: injector sent 743; app saw 743; lost-before-app 0 (Enter: sen |
| terminal@gitbash | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal@gitbash | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 136 unbound keydown/typed,  |
| terminal@gitbash | keydown_count | PASS | injector accounting: injector sent 564; app saw 564; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| terminal@gitbash | big_paste | NOREF | big_paste: time-to-Enter not measured (big.out never reached its full 1000001 bytes within the wait: the byte check below says why); NO-REFE |
| terminal@gitbash | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| terminal@gitbash | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 73 unbound keydown/typed, 2 t |
| terminal@gitbash | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | alt_screen | PASS | injector accounting: injector sent 27; app saw 27; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 24 unbound keydown/typed, 2 t |
| terminal@gitbash | gitbash_subshell | SKIP | the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells; injector accounting: injector sent 2; app saw 2; los |
| terminal@gitbash | alt_screen_programs | NOREF | NO-REFERENCE: UNVERIFIABLE: htop is not installed (package htop): that part of the alt-screen coverage did not run; NO-REFERENCE: UNVERIFIAB |
| terminal@gitbash | subshell_ready_rc_questions | PASS | rcq: `exit` NOT typed: the probe ran in the local shell and the shell count under the app is unknown (base None, now None); no root pid reco |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | PASS | clear: setup armed decision=Wait(Unknown); no setup progress event logged; the first inner line ran 0.05 s after its Enter (its history band |
| terminal@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| terminal@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| terminal@gitbash | ssh_connect_typing | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_enter_then_immediate_command | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal@gitbash | ssh_connect_typing_wrapper | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 276; app saw 276; lost-before-app 0 (Enter: sent 10, app saw 10); of the app |
| terminal@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.17 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 52.0 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | PASS | #1176 key -> pty write median 0.79 ms (p95 1.09, n=60): within the 1 ms target; #1995 notifies logged before 6 of 78 key frame build(s): Inp |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal@gitbash | latency_first_key_after_idle | NOREF | NOTE (not judged; the band decides): First key after 1 s idle: median 4.77 vs baseline build +0.95 ms (+24.9%); First key after 1 s idle: ke |
| terminal@gitbash | routing_table | FAIL | 'date' (shell) expected shell, went to agent (surface PlainTerminal, rule model); shell received []; injector accounting: injector sent 646; |
| terminal@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| terminal@gitbash | routing_plain_installed_head | FAIL | 'find the bug in main.rs' typed on the plain terminal went to agent by model; an installed first word must run in the shell (#1981). The she |
| terminal@gitbash | agent_basics_powershell | SKIP | not run in the Git Bash pane: #2072 (T6): the PowerShell pane's twin of agent_input_exact and agent_mode_switch (PSReadLine history instead  |
| terminal@gitbash | remote_session_legacy | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| terminal@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| terminal@gitbash | remote_tab_no_helper | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | remote_tab_enter_no_helper | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| terminal@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| terminal@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| terminal@gitbash | ssh_gitbash_managed | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.36 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | ssh_gitbash_bootstrap_in_command | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_gitbash_typeahead_managed | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_gitbash_kill_switch | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); control: no command row was readable in the  |
| terminal@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| terminal@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 5.0 ms (< 16 ms; before: 5.0 ms); injector accounting: injector sent 742; app saw 742; lost-before |
| terminal@gitbash | ui_completions_as_you_type_local | FAIL | no completion menu with both candidates after typing a shared prefix without Tab; fixture dir C:\gl\b\07xLPX_fG\0\winters-phosphor\phosphor\ |
| terminal@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NOTE (not judged |
| terminal@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal@gitbash | history_up_recall | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 118 unbound keydown/typed,  |
| terminal@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal@gitbash | ssh_drop_reconnect | SKIP | not run on Windows: not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535) |
| terminal@gitbash | scroll_keys_position | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed,  |
| terminal@gitbash | ime_compose | SKIP | Windows: NEEDS: the language Chinese (Simplified, China) with the Microsoft Pinyin keyboard added for the runner user on the Windows VM (Set |
| terminal@gitbash | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal@gitbash | scroll_keys_foreground | PASS | the view's position was not read from the screen (NEEDS: an OCR engine on this runner: tesseract-ocr on Linux and Windows, Apple Vision on m |
| terminal@gitbash | intl_altgr_ps | SKIP | PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane); injector accounting: injector sent 2; app s |
| terminal@gitbash | intl_altgr_bash | PASS | injector accounting: injector sent 79; app saw 79; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 74 unbound keydown/typed, 2 t |
| terminal@gitbash | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| terminal@gitbash | ui_copy_wrapped_output | PASS | injector accounting: injector sent 238; app saw 238; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| terminal@gitbash | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 223 unbound keydown/typed,  |
| terminal@gitbash | ui_right_click_paste | SKIP | not run on Windows: not ported: it falls back to OCR for the menu entry and has never passed anywhere (r14-test-gaps F13); on Windows the sa |
| terminal@gitbash | selection_modes | FAIL | Ctrl-C did not end the streaming program: no prompt within 8 s (the SECOND Ctrl-C: the first copied the selection; if this one copied too, t |
| terminal@gitbash | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal@gitbash | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal@gitbash | startup_shell_pane | SKIP | runs in the pane-shell passes only (terminal@pwsh7, terminal@cmd, terminal@wsl: APPCI_WIN_SHELLS of the app-test-windows job names them; a p |
| terminal@gitbash | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal@gitbash | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal@gitbash | resize_reflow_output | NOREF | NO-REFERENCE: UNVERIFIABLE: the resize case was not run: NEEDS: a desktop of at least 1300x800 px on the Windows VM (it is 1280x800): the wi |
| terminal@gitbash | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal@gitbash | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| terminal@gitbash | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8480; app saw 8480; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8244 unbound keydown/ |
| terminal@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal@gitbash | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| terminal@gitbash | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal@gitbash | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal@gitbash | editor_memory_7mb | FAIL | rs: 10 MB of private still held 75 s after closing the editor (over the pre-open baseline; close via key ctrl+shift+w): 10.05 is outside its |
| terminal@gitbash | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| terminal@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| terminal@gitbash | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows located from the app's find hook (the newest sighting in each t |
| terminal@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal@gitbash | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal@gitbash | keybinding_rebind | PASS | injector accounting: injector sent 848; app saw 848; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 784 unbound keydown/typed |
| terminal@gitbash | soak | PASS | soak: window medians [431, 442, 441] MB, monotonic=True, growth +10 MB (limit 150 MB); injector accounting: injector sent 3992; app saw 3992 |
| terminal@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 11 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams we |
| terminal@gitbash | tabs_two_windows | PASS | injector accounting: injector sent 3271; app saw 3271; lost-before-app 0 (Enter: sent 48, app saw 48); of the app's: 3171 unbound keydown/ty |
| terminal@gitbash | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal@gitbash | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| terminal@gitbash | tabs_streaming | PASS | injector accounting: injector sent 2551; app saw 2551; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2469 unbound keydown/ty |
| terminal@gitbash | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| terminal@gitbash | first_run_defaults | SKIP | the gitbash pane exists only through session.startup_shell_override, a non-default setting: the default-settings first run is judged in the  |
| terminal@pwsh7 | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@pwsh7 | fast_typing | PASS | injector accounting: injector sent 2246; app saw 2246; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2184 unbound keydown/ty |
| terminal@pwsh7 | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal@pwsh7 | tab_enter | PASS | injector accounting: injector sent 674; app saw 674; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 648 unbound keydown/typed |
| terminal@pwsh7 | ctrl_c_d | NOREF | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 6.79, -> LineEditorReading 29.45; app: |
| terminal@pwsh7 | routing_table_powershell | PASS | injector accounting: injector sent 860; app saw 860; lost-before-app 0 (Enter: sent 31, app saw 31); of the app's: 751 unbound keydown/typed |
| terminal@pwsh7 | history_up_recall | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; injector accounting: injector sent 147; app saw 147; lost |
| terminal@pwsh7 | startup_shell_pane | PASS | pwsh7 pane shell resolved to: C:\Program Files\WindowsApps\Microsoft.PowerShell_7.7.0.0_x64__8wekyb3d8bbwe\pwsh.exe; injector accounting: in |
| terminal@wsl | startup_shell_pass | SKIP | a WSL distribution is not installed on this machine: the wsl pass cannot run (the harness installs nothing). NEEDS: the Windows Subsystem fo |
| agent | mode_proof | PASS |  |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | cadence | PASS | injector accounting: injector sent 225; app saw 225; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 220 unbound keydown/typed,  |
| agent | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| agent | fast_typing | PASS | injector accounting: injector sent 2247; app saw 2247; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2185 unbound keydown/ty |
| agent | rollover | PASS | injector accounting: injector sent 762; app saw 762; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 740 unbound keydown/typed |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent | arrows_behind_paste | PASS | injector accounting: injector sent 132; app saw 132; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 110 unbound keydown/typed,  |
| agent | keydown_count | PASS | injector accounting: injector sent 643; app saw 643; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 620 unbound keydown/typed |
| agent | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| agent | tab_enter | PASS | injector accounting: injector sent 682; app saw 682; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 656 unbound keydown/typed |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 6.78, -> LineEditorReading 34.1; app:  |
| agent | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| agent | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| agent | gitbash_subshell | PASS | injector accounting: injector sent 1118; app saw 1118; lost-before-app 0 (Enter: sent 32, app saw 32); of the app's: 1107 unbound keydown/ty |
| agent | alt_screen_programs | SKIP | not run in the PowerShell pane: vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not inst |
| agent | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| agent | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| agent | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| agent | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| agent | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| agent | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| agent | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.14 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 4.28 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| agent | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached th |
| agent | routing_typo_head | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log (#1980, agent mode only): runs in the Git Bash pane |
| agent | routing_plain_installed_head | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log (#1981): runs in the Git Bash pane |
| agent | agent_basics_powershell | PASS | injector accounting: injector sent 210; app saw 210; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 193 unbound keydown/typed,  |
| agent | agent_lrc_no_false_takeover | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; a fresh agent conversation was started (Escape, /agent) so e |
| agent | cb_agent_pickers | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git  |
| agent | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| agent | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| agent | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| agent | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| agent | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| agent | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| agent | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| agent | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| agent | intl_wide_edit | PASS | edit 0: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; edit 1: rec.l |
| agent | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| agent | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git  |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | not run in the PowerShell pane: the one-connection managed ssh sends the bootstrap in the remote command (#2051): only the Git Bash pane has |
| agent | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| agent | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: the default and PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane are plain ssh (#1933, #2051): only the Git Bash |
| agent | ui_palette | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| agent | intl_wide_program | SKIP | not run in the PowerShell pane: `head -n 1 > file` is the receiving program: runs in the Git Bash pane |
| agent | hostile_output_recovers | SKIP | not run in the PowerShell pane: #2072 (T8): `cat` of a binary file, a POSIX command list around hostile_out.py and rec.log for the exact lin |
| agent | latency_enter_ssh | SKIP | not run in the PowerShell pane: the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` |
| agent | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent | history_up_recall | PASS | injector accounting: injector sent 150; app saw 150; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 131 unbound keydown/typed,  |
| agent | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent | scroll_keys_position | SKIP | not run in the PowerShell pane: r23 (#2060): bash `cat` of numbered lines, a bash background job, rec.log; the scroll position is read by Wi |
| agent | ime_compose | SKIP | not run in the PowerShell pane: r15 T39: the committed text is checked as the UTF-8 bytes of a file written by `echo ... >> file` (Windows P |
| agent | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent | scroll_keys_foreground | SKIP | not run in the PowerShell pane: #2080: the foreground program is a POSIX raw reader started from a bash script (bytes_reader.sh), with a bas |
| agent | intl_altgr_ps | PASS | injector accounting: injector sent 170; app saw 170; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 167 unbound keydown/typed,  |
| agent | intl_altgr_bash | NOREF | injector accounting: injector sent 300; app saw 297; lost-before-app 3 (Enter: sent 5, app saw 5); of the app's: 293 unbound keydown/typed,  |
| agent | ui_mouse_select_copy | SKIP | not run in the PowerShell pane: T31: the token is printed by bash `printf` and the command is checked against rec.log: runs in the Git Bash  |
| agent | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| agent | ui_copy_wrapped_drag | SKIP | not run in the PowerShell pane: T31 (#1976): `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in |
| agent | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| agent | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| agent | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| agent | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| agent | resize_reflow_output | SKIP | not run in the PowerShell pane: T33 (r14 T26): a bash SIGWINCH trap, `stty size` and vim: runs in the Git Bash pane (window size by SetWindo |
| agent | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| agent | suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: the fixture is printed with `cat` and its command is confirmed from the bash prompt hook; the chip's  |
| agent | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| agent | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| agent | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| agent | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 48; app saw 48; lost |
| agent | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| agent | restore_shows_history | SKIP | not run in the PowerShell pane: r14 T20: the token is printed by bash `printf '%s%s\n' A B` so the command text never holds it, and rec.log  |
| agent | ui_vertical_tabs_many | SKIP | not run in the PowerShell pane: T33 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| agent | ui_vertical_tabs_panel_click | SKIP | not run in the PowerShell pane: T31 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| agent | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| agent | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent | keybinding_rebind | NOREF | NO-REFERENCE: UNVERIFIABLE: this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is ju |
| agent | soak | PASS | soak: window medians [502, 522, 509] MB, monotonic=False, growth +8 MB (limit 150 MB); injector accounting: injector sent 4414; app saw 4414 |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 35.0 frames/s for 31.0 wakeups/s (0 Full); 4 hidden tabs streaming 2.2 frames/s, 122.8 hidden  |
| agent | tabs_two_windows | PASS | no hop=view_mode line in the app log: the mode is read from the screen (OCR): agent; injector accounting: injector sent 3458; app saw 3458;  |
| agent | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| agent | tabs_streaming | PASS | injector accounting: injector sent 2690; app saw 2690; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2608 unbound keydown/ty |
| agent | meta_and_word_keys | PASS | raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it); injector account |
| agent | first_run_defaults | PASS | injector accounting: injector sent 616; app saw 616; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 605 unbound keydown/typed,  |
| agent@gitbash | mode_proof | PASS |  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 223; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 218 unbound keydown/typed,  |
| agent@gitbash | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| agent@gitbash | fast_typing | PASS | injector accounting: injector sent 2006; app saw 2006; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1944 unbound keydown/ty |
| agent@gitbash | rollover | PASS | injector accounting: injector sent 682; app saw 682; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 660 unbound keydown/typed |
| agent@gitbash | expert_fuzz | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 664 unbound keydown/typed |
| agent@gitbash | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 4 unbound keydown/typed, 8 ta |
| agent@gitbash | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 137 unbound keydown/typed,  |
| agent@gitbash | keydown_count | PASS | injector accounting: injector sent 563; app saw 563; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| agent@gitbash | harness_crash | FAIL | the agent@gitbash pass crashed outside a case: SystemExit(143) |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_basics_powershell | windows | agent | PASS |
| agent_basics_powershell | windows | terminal | PASS |
| agent_basics_powershell | windows | terminal@gitbash | SKIP |
| agent_lrc_no_false_takeover | windows | agent | PASS |
| alt_screen_programs | windows | agent | SKIP |
| alt_screen_programs | windows | terminal | SKIP |
| alt_screen_programs | windows | terminal@gitbash | NOREF |
| big_paste | windows | agent | SKIP |
| big_paste | windows | terminal | SKIP |
| big_paste | windows | terminal@gitbash | NOREF |
| editor_daily | windows | agent | PASS |
| editor_daily | windows | terminal | PASS |
| editor_daily | windows | terminal@gitbash | PASS |
| editor_memory_7mb | windows | terminal | PASS |
| editor_memory_7mb | windows | terminal@gitbash | FAIL |
| first_run_defaults | windows | agent | PASS |
| first_run_defaults | windows | terminal | PASS |
| first_run_defaults | windows | terminal@gitbash | SKIP |
| flood_scrollback_find | windows | agent | SKIP |
| flood_scrollback_find | windows | terminal | SKIP |
| flood_scrollback_find | windows | terminal@gitbash | SKIP |
| focus_cycle | windows | agent | PASS |
| focus_cycle | windows | terminal | PASS |
| focus_cycle | windows | terminal@gitbash | PASS |
| gitbash_typeahead_reps | windows | agent | SKIP |
| gitbash_typeahead_reps | windows | terminal | SKIP |
| gitbash_typeahead_reps | windows | terminal@gitbash | PASS |
| history_survives_restart | windows | agent | PASS |
| history_survives_restart | windows | terminal | PASS |
| history_survives_restart | windows | terminal@gitbash | PASS |
| history_up_recall | windows | agent | PASS |
| history_up_recall | windows | terminal | PASS |
| history_up_recall | windows | terminal@gitbash | PASS |
| history_up_recall | windows | terminal@pwsh7 | PASS |
| hostile_output_recovers | windows | agent | SKIP |
| hostile_output_recovers | windows | terminal | SKIP |
| hostile_output_recovers | windows | terminal@gitbash | PASS |
| job_control | windows | agent | SKIP |
| job_control | windows | terminal | SKIP |
| job_control | windows | terminal@gitbash | SKIP |
| keybinding_rebind | windows | agent | NOREF |
| keybinding_rebind | windows | terminal | NOREF |
| keybinding_rebind | windows | terminal@gitbash | PASS |
| link_and_path_click | windows | agent | SKIP |
| link_and_path_click | windows | terminal | SKIP |
| link_and_path_click | windows | terminal@gitbash | SKIP |
| mac_pointer_chord_proof | windows | agent | SKIP |
| mac_pointer_chord_proof | windows | agent@gitbash | SKIP |
| mac_pointer_chord_proof | windows | terminal | SKIP |
| mac_pointer_chord_proof | windows | terminal@gitbash | SKIP |
| meta_and_word_keys | windows | agent | PASS |
| meta_and_word_keys | windows | terminal | PASS |
| meta_and_word_keys | windows | terminal@gitbash | PASS |
| minimize_fullscreen | windows | agent | SKIP |
| minimize_fullscreen | windows | terminal | SKIP |
| minimize_fullscreen | windows | terminal@gitbash | SKIP |
| network_gate_switch | windows | terminal | SKIP |
| network_gate_switch | windows | terminal@gitbash | NOREF |
| output_frames | windows | agent | PASS |
| output_frames | windows | terminal | PASS |
| output_frames | windows | terminal@gitbash | NOREF |
| primary_and_middle_click | windows | agent | SKIP |
| primary_and_middle_click | windows | terminal | SKIP |
| primary_and_middle_click | windows | terminal@gitbash | SKIP |
| remote_session_legacy | windows | agent | SKIP |
| remote_session_legacy | windows | terminal | SKIP |
| remote_session_legacy | windows | terminal@gitbash | SKIP |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | SKIP |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | SKIP |
| resize_reflow_output | windows | agent | SKIP |
| resize_reflow_output | windows | terminal | SKIP |
| resize_reflow_output | windows | terminal@gitbash | NOREF |
| restore_shows_history | windows | agent | SKIP |
| restore_shows_history | windows | terminal | SKIP |
| restore_shows_history | windows | terminal@gitbash | PASS |
| routing_plain_installed_head | windows | agent | SKIP |
| routing_plain_installed_head | windows | terminal | SKIP |
| routing_plain_installed_head | windows | terminal@gitbash | FAIL |
| routing_table_powershell | windows | agent | PASS |
| routing_table_powershell | windows | terminal | FAIL |
| routing_table_powershell | windows | terminal@gitbash | SKIP |
| routing_table_powershell | windows | terminal@pwsh7 | PASS |
| routing_typo_head | windows | agent | SKIP |
| scroll_keys_foreground | windows | agent | SKIP |
| scroll_keys_foreground | windows | terminal | SKIP |
| scroll_keys_foreground | windows | terminal@gitbash | PASS |
| scroll_keys_position | windows | agent | SKIP |
| scroll_keys_position | windows | terminal | SKIP |
| scroll_keys_position | windows | terminal@gitbash | PASS |
| selection_modes | windows | agent | SKIP |
| selection_modes | windows | terminal | SKIP |
| selection_modes | windows | terminal@gitbash | FAIL |
| soak | windows | agent | PASS |
| soak | windows | terminal | PASS |
| soak | windows | terminal@gitbash | PASS |
| ssh_connect_typing_wrapper | windows | agent | SKIP |
| ssh_connect_typing_wrapper | windows | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | terminal@gitbash | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | agent | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | terminal@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | agent | SKIP |
| ssh_gitbash_kill_switch | windows | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | terminal@gitbash | SKIP |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal@gitbash | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | PASS |
| startup_shell_pane | windows | terminal | SKIP |
| startup_shell_pane | windows | terminal@gitbash | SKIP |
| startup_shell_pane | windows | terminal@pwsh7 | PASS |
| subshell_line_editor_proof | windows | agent | SKIP |
| subshell_line_editor_proof | windows | terminal | SKIP |
| subshell_line_editor_proof | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | PASS |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | agent | SKIP |
| subshell_ready_secret_after_probe | windows | terminal | SKIP |
| subshell_ready_secret_after_probe | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | agent | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | windows | agent | SKIP |
| subshell_sh_probe | windows | terminal | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | windows | agent | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal@gitbash | SKIP |
| tab_lifecycle_keys | windows | agent | PASS |
| tab_lifecycle_keys | windows | terminal | PASS |
| tab_lifecycle_keys | windows | terminal@gitbash | PASS |
| tab_pointer_use | windows | agent | SKIP |
| tab_pointer_use | windows | terminal | SKIP |
| tab_pointer_use | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | FAIL |
| ui_copy_wrapped_drag | windows | agent | SKIP |
| ui_copy_wrapped_drag | windows | terminal | SKIP |
| ui_copy_wrapped_drag | windows | terminal@gitbash | PASS |
| ui_copy_wrapped_output | windows | agent | SKIP |
| ui_copy_wrapped_output | windows | terminal | SKIP |
| ui_copy_wrapped_output | windows | terminal@gitbash | PASS |
| ui_vertical_tabs_many | windows | agent | SKIP |
| ui_vertical_tabs_many | windows | terminal | SKIP |
| ui_vertical_tabs_many | windows | terminal@gitbash | PASS |
| ui_vertical_tabs_panel_click | windows | agent | SKIP |
| ui_vertical_tabs_panel_click | windows | terminal | SKIP |
| ui_vertical_tabs_panel_click | windows | terminal@gitbash | PASS |
| wheel_scroll | windows | agent | SKIP |
| wheel_scroll | windows | terminal | SKIP |
| wheel_scroll | windows | terminal@gitbash | SKIP |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | PASS |

passed everywhere it ran (8, candidates for the owner's decision): agent_lrc_no_false_takeover, editor_daily, focus_cycle, history_survives_restart, history_up_recall, meta_and_word_keys, soak, tab_lifecycle_keys
not passed somewhere (fail / skip / no-reference): agent_basics_powershell, alt_screen_programs, big_paste, editor_memory_7mb, first_run_defaults, flood_scrollback_find, gitbash_typeahead_reps, hostile_output_recovers, job_control, keybinding_rebind, link_and_path_click, mac_pointer_chord_proof, minimize_fullscreen, network_gate_switch, output_frames, primary_and_middle_click, remote_session_legacy, remote_tab_enter_no_helper, remote_tab_no_helper, resize_reflow_output, restore_shows_history, routing_plain_installed_head, routing_table_powershell, routing_typo_head, scroll_keys_foreground, scroll_keys_position, selection_modes, ssh_connect_typing_wrapper, ssh_gitbash_bootstrap_in_command, ssh_gitbash_failing_not_local_managed, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, startup_shell_pane, subshell_line_editor_proof, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, subshell_sh_probe_empty_ps1, tab_pointer_use, ui_completions_as_you_type_local, ui_copy_wrapped_drag, ui_copy_wrapped_output, ui_vertical_tabs_many, ui_vertical_tabs_panel_click, wheel_scroll, zero_network_startup
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run_slim/logs/agent/acct/launch0_1794642200.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794640973.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794640973.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 2686 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off [test-windows]`
  - `warp ai::blocklist::history_model::tests::forget_terminal_view_evicts_a_done_conversation_once_sqlite_has_written_it [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_chips.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_chips.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_chips.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_chips.fish.lan.nohelper (case not in previous run)`
  - `windows/terminal/routing_table_powershell (case not in previous run)`
  - `windows/terminal@gitbash/expert_fuzz (case not in previous run)`
  - `windows/terminal@gitbash/routing_plain_installed_head (case not in previous run)`
  - `windows/terminal@gitbash/ui_completions_as_you_type_local`
  - `windows/terminal@gitbash/selection_modes (case not in previous run)`
  - `windows/terminal@gitbash/editor_memory_7mb (case not in previous run)`
  - `windows/agent@gitbash/harness_crash (case not in previous run)`
- FIXED failures (app): 
  - `windows/terminal/fast_typing`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
- Latency regressions > 5%: 
  - `windows/terminal@gitbash/ctrl_c_d ctrl_c: 71.58 -> 85.51 ms (+19.5%)`
  - `windows/terminal@gitbash/latency_enter enter_to_start: 93.61 -> 126.47 ms (+35.1%)`
  - `windows/terminal@gitbash/latency_key_log key_to_frame: 3.70 -> 4.35 ms (+17.6%)`
- compared against: test-linux: build/r30c (3067438585); test-macos: build/r30c (3067438585); test-warpui-core: build/r30c (3067438585); test-warpui-core-macos: build/r30c (3067438585); test-warpui-core-windows: build/r30c (3067438585); test-windows: build/r30c (3067438585); test-workspace-rest: build/r30c (3067438585); test-workspace-rest-macos: build/r30c (3067438585); test-workspace-rest-windows: build/r30c (3067438585); app lab-wayland: build/r13a (3051722998); app windows: build/r13a (3051722998)
