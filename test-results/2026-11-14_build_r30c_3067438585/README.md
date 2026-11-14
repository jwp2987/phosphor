# build/r30c  36efe34b5  (pipeline 3067438585)

- date: 2026-11-14, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.zsh.*`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.zsh.*`, `lab_tab_no_helper.zsh.*`, `lab_tab_remote_dir.zsh.*`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on macos-lab: `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.zsh.*`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.zsh.*`, `lab_tab_no_helper.zsh.*`, `lab_tab_remote_dir.zsh.*`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on windows: `agent_close_undo_call_status`, `agent_latency_enter`, `agent_latency_key`, `agent_lrc_no_false_takeover`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `alt_screen_programs`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `cb_agent_pickers`, `cb_ui_action_latency`, `ctrl_c_d`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `editor_memory_7mb`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `flood_scrollback_find`, `focus_cycle`, `gitbash_subshell`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `idle_cpu`, `ime_compose`, `intl_altgr_bash`, `intl_altgr_ps`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `job_control`, `key_loss_control`, `keybinding_rebind`, `keydown_count`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `mode_proof`, `network_gate_switch`, `output_frames`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `rollover`, `routing_plain_installed_head`, `routing_table`, `routing_table_powershell`, `routing_typo_head`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_connect_typing`, `ssh_connect_typing_wrapper`, `ssh_drop_reconnect`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead`, `ssh_nested_typeahead_wrapper`, `startup_shell_pane`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `tab_lifecycle_keys`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `wheel_scroll`, `zero_network_startup`
  - asked for on windows-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.*`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_managed_connect.*`, `lab_msys_ssh_switch.*`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_remote_session.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.*`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
- unit: 50215 passed, 1 failed, 191 skipped of 50216 run (9 suites)
- app (pass/fail/skip): lab-macos 0/2/0; lab-wayland 25/20/0 +29 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 34.7 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 0.4 | MacBookPro-gitrunner |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | manual | - | - | not run |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | manual | - | - | not run |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 31.2 | - |  |
| build-macos-arm64 | success | 19.1 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 5.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 8.7 | - |  |
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
| test-linux | success | 21.8 | - |  |
| test-macos | success | 29.2 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.1 | phosphor-windows-vm |  |
| test-windows | failed | 47.2 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.4 | - |  |
| test-workspace-rest-macos | success | 3.6 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.3 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off` LEAK (52.1s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::on` LEAK (131.8s)
- `warp ai::blocklist::history_model::tests::forget_terminal_view_evicts_a_done_conversation_once_sqlite_has_written_it` FAIL (0.1s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.7s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.4s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.0s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.7s)
- `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it` LEAK (2.1s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | case_selection | FAIL | unknown case name(s) lab_managed_connect.zsh.*, lab_msys_ssh_switch.zsh.*, lab_remote_session.zsh.* (not in the registry: renamed or misspel |
| agent | case_selection | FAIL | unknown case name(s) lab_managed_connect.zsh.*, lab_msys_ssh_switch.zsh.*, lab_remote_session.zsh.* (not in the registry: renamed or misspel |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 378 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a  |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper | PASS | injector accounting: injector sent 2307; app saw 2307; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2243 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper.rst | PASS | injector accounting: injector sent 583; app saw 583; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 564 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper.stall10 | PASS | injector accounting: injector sent 205; app saw 205; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 196 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | PASS | injector accounting: injector sent 2343; app saw 2343; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2280 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper.rst | PASS | injector accounting: injector sent 597; app saw 597; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 578 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper.stall10 | PASS | injector accounting: injector sent 210; app saw 210; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 201 unbound keydown/typed,  |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | PASS | injector accounting: injector sent 80; app saw 80; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 71 unbound keydown/typed, 5 t |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | PASS | injector accounting: injector sent 80; app saw 80; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 71 unbound keydown/typed, 5 t |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t186h1pn66.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| terminal | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 35 |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 35 |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 61 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new m |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 204 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a ne |
| terminal | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2289 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper.rst | PASS | injector accounting: injector sent 597; app saw 597; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 578 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper.stall10 | PASS | injector accounting: injector sent 210; app saw 210; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 201 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2290 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper.rst | PASS | injector accounting: injector sent 597; app saw 597; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 578 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper.stall10 | PASS | injector accounting: injector sent 210; app saw 210; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 201 unbound keydown/typed,  |
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
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; app find hook gave no position for 'brt501 |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt504p4vx2t: app log chip update n |
| terminal | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| terminal | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 376 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a new |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 34 |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); injector accounting: injector sent 34 |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 65 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new metr |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.zsh.lan.helper | PASS | lab_tab_remote_dir.zsh.lan.helper relaunched the app itself (pid 3720274 -> 3732622); a relaunched app starts in a terminal session, so this |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 204 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a new m |
| agent | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1907; app saw 1907; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1870 unbound keydown/ty |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: wayland/run/results.jsonl: 3609 KB skipped (over 1 MB)
- note: wayland/run/results.json: 3636 KB skipped (over 1 MB)
- note: wayland/run/junit.xml: 3867 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794631656.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794631656.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 2597 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::on [test-windows]`
  - `warp ai::blocklist::history_model::tests::forget_terminal_view_evicts_a_done_conversation_once_sqlite_has_written_it [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp terminal::view::tests::a_waiting_question_with_no_card_in_the_pane_is_stopped [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_chips.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.150.helper (case not in previous run)`
  - `lab-wayland/agent/lab_chips.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_chips.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_connect_typeahead.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_connect_typeahead.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_connect_typeahead.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/agent/lab_connect_typeahead.zsh.150.helper (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r30b (3065705355); test-macos: build/r30b (3065705355); test-warpui-core: build/r30b (3065705355); test-warpui-core-macos: build/r30b (3065705355); test-warpui-core-windows: build/r30b (3065705355); test-windows: build/r30b (3065705355); test-workspace-rest: build/r30b (3065705355); test-workspace-rest-macos: build/r30b (3065705355); test-workspace-rest-windows: build/r30b (3065705355); app lab-wayland: build/r13a (3051722998)
