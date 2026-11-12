# build/r24a  443495ed3  (pipeline 3057403889)

- date: 2026-11-12, tier: targeted, pipeline status: success
- compile (check-linux): ok, 121 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_agent_remote_drop.nu.lan.nohelper`, `lab_agent_remote_errors.nu.lan.nohelper`, `lab_agent_remote_flow.bash.lan.nohelper`, `lab_agent_remote_flow.nu.lan.nohelper`, `lab_chips.bash.150.helper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.helper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.helper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.helper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.helper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.helper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.helper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.helper`, `lab_chips.zsh.lan.nohelper`, `lab_codebase_index_remote.bash.150.nohelper`, `lab_codebase_index_remote.bash.lan.nohelper`, `lab_codebase_index_remote.fish.150.nohelper`, `lab_codebase_index_remote.fish.lan.nohelper`, `lab_codebase_index_remote.nu.150.nohelper`, `lab_codebase_index_remote.nu.lan.nohelper`, `lab_codebase_index_remote.zsh.150.nohelper`, `lab_codebase_index_remote.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.nohelper`, `lab_diff_apply_remote.bash.lan.nohelper`, `lab_diff_apply_remote.nu.lan.nohelper`, `lab_diff_panel_remote.bash.lan.nohelper`, `lab_diff_panel_remote.fish.lan.nohelper`, `lab_diff_panel_remote.nu.lan.nohelper`, `lab_diff_panel_remote.zsh.lan.nohelper`, `lab_enter_no_network_wait.fish.150.helper`, `lab_enter_no_network_wait.fish.150.nohelper`, `lab_enter_no_network_wait.nu.150.helper`, `lab_enter_no_network_wait.nu.150.nohelper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_agent_edit_conflict.nu.lan.nohelper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_file_tree_remote.fish.150.nohelper`, `lab_file_tree_remote.fish.lan.nohelper`, `lab_file_tree_remote.nu.150.nohelper`, `lab_file_tree_remote.nu.lan.nohelper`, `lab_gh_chip_remote.bash.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.bash.150.helper`, `lab_helper_reconnect.bash.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_no_helper_chip.fish.lan.nohelper`, `lab_no_helper_chip.nu.lan.nohelper`, `lab_no_helper_chip.zsh.lan.nohelper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_nohelper_unavailable.bash.150.nohelper`, `lab_nohelper_unavailable.bash.lan.nohelper`, `lab_nu_agent_pager_env.nu.lan.nohelper`, `lab_path_detect.bash.150.helper`, `lab_path_detect.fish.150.helper`, `lab_path_detect.fish.150.nohelper`, `lab_path_detect.fish.lan.helper`, `lab_path_detect.fish.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_path_detect.p_tinypath.lan.nohelper`, `lab_path_detect.zsh.150.helper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_read_files_remote.bash.lan.nohelper`, `lab_read_files_remote.nu.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_skills_remote.fish.lan.nohelper`, `lab_skills_remote.nu.lan.nohelper`, `lab_skills_remote.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_ssh_drop_reconnect.zsh.150.helper`, `lab_ssh_drop_reconnect.zsh.150.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on linux-wayland: `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `cb_ui_action_latency`, `ctrl_c_d`, `desktop_notification_agent_question`, `drag_drop_file`, `editor_daily`, `editor_memory_7mb`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `flood_scrollback_find`, `gitbash_subshell`, `gpu_lost_device_recovers`, `gpu_real_render`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `meta_and_word_keys`, `minimize_fullscreen`, `mode_proof`, `network_gate_switch`, `pane_resize_maximize`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `scroll_keys_position`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `tab_lifecycle_keys`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_readline_keys`, `ui_right_click_paste`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `wheel_scroll`
  - asked for on macos: `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `cb_ui_action_latency`, `ctrl_c_d`, `editor_daily`, `editor_memory_7mb`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `gitbash_subshell`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `minimize_fullscreen`, `mode_proof`, `network_gate_switch`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `scroll_keys_position`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `wheel_scroll`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.*`, `lab_agent_remote_*`, `lab_agent_suggestions_ssh.*`, `lab_chips.*`, `lab_codebase_index_remote.*`, `lab_ctrl_c_no_esc_i.*`, `lab_diff_apply_remote.*`, `lab_diff_panel_remote.*`, `lab_enter_no_network_wait.*`, `lab_enter_start.*.150.*`, `lab_fam_agent_*`, `lab_fam_fs_basic.*`, `lab_fam_git_read.*`, `lab_fam_run_command.*`, `lab_file_tree_remote.*`, `lab_freeze_and_wake.*`, `lab_gh_chip_remote.*`, `lab_git_write_remote.*`, `lab_helper_*`, `lab_no_helper_chip.*`, `lab_nohelper_*`, `lab_nohelper_sh_login_keys.ash.lan.nohelper`, `lab_nohelper_sh_login_keys.dash.lan.nohelper`, `lab_nohelper_sh_login_profile.ash.lan.nohelper`, `lab_nohelper_sh_login_profile.dash.lan.nohelper`, `lab_nu_agent_pager_env.*`, `lab_password_prompt.p_kbdint.lan.nohelper`, `lab_password_prompt.p_password.lan.nohelper`, `lab_path_detect.*`, `lab_path_detect.p_exec.lan.nohelper`, `lab_path_detect.p_tinypath.lan.nohelper`, `lab_raw_key_arrival.*`, `lab_read_files_remote.*`, `lab_remote_edit.*`, `lab_skills_remote.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.bash.lan.nohelper.rst`, `lab_ssh_drop_reconnect.bash.lan.nohelper.stall10`, `lab_tab_remote_dir.*`
  - asked for on windows: `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `cb_ui_action_latency`, `ctrl_c_d`, `editor_memory_7mb`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `gitbash_subshell`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `minimize_fullscreen`, `mode_proof`, `network_gate_switch`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `scroll_keys_position`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `tabs_streaming`, `tabs_two_windows`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `wheel_scroll`, `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `cb_agent_pickers`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `intl_altgr_bash`, `intl_altgr_ps`, `job_control`, `keybinding_rebind`, `latency_enter_ssh`, `md_viewer_first_frame`, `meta_and_word_keys`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
- unit: 49718 passed, 0 failed, 191 skipped of 49718 run (9 suites)
- app (pass/fail/skip): lab-macos 174/89/9 +93 no-ref; lab-wayland 78/99/0 +75 no-ref; macos 98/9/21 +7 no-ref; wayland 117/4/18 +16 no-ref; windows 180/37/202 +31 no-ref; soak: noref (windows)

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 149.1 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 272.2 | MacBookPro-gitrunner |  |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 104.3 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 131.8 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 454.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 31.0 | - |  |
| build-macos-arm64 | success | 18.8 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 45.4 | phosphor-windows-vm |  |
| check-linux | success | 8.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 7.5 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| lab-helper | success | 0.7 | - |  |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.4 | - |  |
| test-linux | success | 22.2 | - |  |
| test-macos | success | 28.8 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.3 | phosphor-windows-vm |  |
| test-windows | success | 46.1 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.6 | - |  |
| test-workspace-rest-macos | success | 3.3 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.6 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on` LEAK (51.7s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::off` LEAK (130.0s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.9s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.3s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.0s)
- `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line` LEAK (2.3s)
- `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it` LEAK (1.9s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t227zx5b4/zzqt227zx5b4_only'], got []; Tab |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t266oam89/zzqt266oam89_only'], got []; Tab |
| terminal | lab_path_detect.bash.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| terminal | lab_path_detect.bash.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| terminal | lab_path_detect.zsh.lan.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 3 t |
| terminal | lab_path_detect.zsh.150.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 3 t |
| terminal | lab_path_detect.fish.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| terminal | lab_path_detect.fish.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 40950); the app is relaunched so later cas |
| terminal | lab_path_detect.p_tinypath.lan.nohelper | PASS | injector accounting: injector sent 105; app saw 105; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 95 unbound keydown/typed, 3 |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.bash.150.nohelper | FAIL | 35 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 34 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 55 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.fish.150.nohelper | FAIL | 35 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.nu.lan.nohelper | FAIL | 55 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.nu.150.nohelper | FAIL | 35 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_enter_start.bash.150.nohelper | PASS | injector accounting: injector sent 1949; app saw 1949; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1912 unbound keydown/ty |
| terminal | lab_enter_start.zsh.150.nohelper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_enter_start.nu.150.nohelper | PASS | injector accounting: injector sent 1838; app saw 1838; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1801 unbound keydown/ty |
| terminal | lab_enter_no_network_wait.bash.150.nohelper | PASS | worst Enter -> write 4.89 ms (limit 75 ms); median 0.74 ms delayed against 0.73 ms undelayed; injector accounting: injector sent 3122; app s |
| terminal | lab_enter_no_network_wait.zsh.150.nohelper | PASS | worst Enter -> write 4.26 ms (limit 75 ms); median 0.74 ms delayed against 0.67 ms undelayed; injector accounting: injector sent 3061; app s |
| terminal | lab_enter_no_network_wait.fish.150.nohelper | PASS | worst Enter -> write 3.92 ms (limit 75 ms); median 0.67 ms delayed against 0.61 ms undelayed; injector accounting: injector sent 3122; app s |
| terminal | lab_enter_no_network_wait.nu.150.nohelper | PASS | worst Enter -> write 2.67 ms (limit 75 ms); median 0.51 ms delayed against 0.48 ms undelayed; injector accounting: injector sent 3728; app s |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper | PASS | injector accounting: injector sent 1494; app saw 1494; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1449 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper.rst | PASS | injector accounting: injector sent 599; app saw 599; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 580 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper.stall10 | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 202 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.bash.150.nohelper | PASS | injector accounting: injector sent 1494; app saw 1494; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1449 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper | PASS | injector accounting: injector sent 1488; app saw 1488; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1443 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | PASS | injector accounting: injector sent 1488; app saw 1488; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1443 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | FAIL | escape_1: lines typed into the remote shell: expected ['t764_2k96dpc6', 't765_xdw9ioot', 't766_1dui6ho6', 't767_pewjoq65', 't768_7saiv20v'], |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | FAIL | escape_1: lines typed into the remote shell: expected ['t798_j90biiav', 't799_nu3v4fpp', 't800_3etoydt9', 't801_yg0a7s8b', 't802_rdrnyy1p'], |
| terminal | lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.nohelper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.fish.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.fish.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.nohelper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t923wo6ga4kq\\n" / save --append /work/u_nu/cc_t9225hqk.txt', ' _ |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t9367gvanmq7\\n" / save --append /work/u_nu/cc_t935m4qz.txt', ' _ |
| terminal | lab_no_helper_chip.bash.lan.nohelper | PASS | injector accounting: injector sent 82; app saw 82; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 2 t |
| terminal | lab_no_helper_chip.zsh.lan.nohelper | FAIL | a command typed after connecting did not run exactly once with the chip up; injector accounting: injector sent 81; app saw 81; lost-before-a |
| terminal | lab_no_helper_chip.fish.lan.nohelper | PASS | injector accounting: injector sent 82; app saw 82; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 2 t |
| terminal | lab_no_helper_chip.nu.lan.nohelper | FAIL | an ERROR text is on screen next to the no-helper chip: it must stay informational; injector accounting: injector sent 88; app saw 88; lost-b |
| terminal | lab_nohelper_sh_login_keys.dash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 357 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_keys.ash.lan.nohelper | PASS | injector accounting: injector sent 355; app saw 355; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 350 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_keys.bash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 349 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_profile.dash.lan.nohelper | PASS | injector accounting: injector sent 128; app saw 128; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 123 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_profile.ash.lan.nohelper | PASS | injector accounting: injector sent 126; app saw 126; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 121 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_profile.bash.lan.nohelper | PASS | injector accounting: injector sent 120; app saw 120; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 112 unbound keydown/typed,  |
| terminal | lab_nu_agent_pager_env.nu.lan.nohelper | PASS | injector accounting: injector sent 1111; app saw 1111; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 1097 unbound keydown/ty |
| terminal | lab_password_prompt.p_password.lan.nohelper | PASS | injector accounting: injector sent 198; app saw 198; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 184 unbound keydown/typed,  |
| terminal | lab_password_prompt.p_kbdint.lan.nohelper | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 180 unbound keydown/typed,  |
| terminal | lab_fam_fs_basic.bash.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.bash.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.zsh.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.zsh.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.fish.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.fish.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.nu.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.nu.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | branch brt1018cmy3o8 not on screen after cd into the repo on the remote; shell arm: 65 out-of-band channels for 2 prompt(s), expected 1..pro |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 51 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 104 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route li |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 51 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | shell arm: 92 out-of-band channels for 4 prompt(s), expected 1..prompts; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | shell arm: 50 out-of-band channels for 4 prompt(s), expected 1..prompts; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 51 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 49 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 104 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route li |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 51 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | shell arm: 92 out-of-band channels for 4 prompt(s), expected 1..prompts; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | shell arm: 51 out-of-band channels for 4 prompt(s), expected 1..prompts; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_nohelper_tab_hands_back.bash.lan.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_tab_hands_back.bash.150.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 192 recorded lines |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 192 recorded lines |
| terminal | lab_nohelper_tab_hands_back.fish.lan.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_tab_hands_back.fish.150.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_tab_hands_back.nu.lan.nohelper | PASS | injector accounting: injector sent 89; app saw 89; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 80 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_tab_hands_back.nu.150.nohelper | PASS | injector accounting: injector sent 89; app saw 89; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 80 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_unavailable.bash.lan.nohelper | PASS | injector accounting: injector sent 175; app saw 175; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 167 unbound keydown/typed,  |
| terminal | lab_nohelper_unavailable.bash.150.nohelper | PASS | injector accounting: injector sent 175; app saw 175; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 167 unbound keydown/typed,  |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_file_tree_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 110; app saw 110; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_file_tree_remote.bash.150.nohelper | PASS | injector accounting: injector sent 110; app saw 110; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_file_tree_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 98 unbound keydown/typed, 2 |
| terminal | lab_file_tree_remote.zsh.150.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 98 unbound keydown/typed, 2 |
| terminal | lab_file_tree_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 110; app saw 110; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_file_tree_remote.fish.150.nohelper | PASS | injector accounting: injector sent 110; app saw 110; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_file_tree_remote.nu.lan.nohelper | FAIL | the next prompt did not bring the new file into the tree (no hop=file_tree update naming it); the removal of the file was never reported (no |
| terminal | entered_agent_view_in_lab_file_tree_remote.nu.lan.nohelper | FAIL | after the TERMINAL case lab_file_tree_remote.nu.lan.nohelper the pane shows AGENT view: something the case typed was sent to the agent inste |
| terminal | lab_file_tree_remote.nu.150.nohelper | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; mode guard: the pane was in agent before this terminal case; |
| terminal | lab_skills_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 104; app saw 104; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 95 unbound keydown/typed, 2 |
| terminal | lab_skills_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 102; app saw 102; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 93 unbound keydown/typed, 2 |
| terminal | lab_skills_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 104; app saw 104; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 95 unbound keydown/typed, 2 |
| terminal | lab_skills_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 99 unbound keydown/typed, 2 |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t1175xf9d13.txt in the code editor: the palette's 'Toggle project explorer', |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t11789918f8.txt in the code editor: the palette's 'Toggle project explorer', |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t1181aonhux.txt in the code editor: the palette's 'Toggle project explorer', |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t1184sexj7d.txt in the code editor: the palette's 'Toggle project explorer', |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_codebase_index_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 128; app saw 128; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 119 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.bash.150.nohelper | PASS | injector accounting: injector sent 128; app saw 128; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 119 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 126; app saw 126; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 117 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.zsh.150.nohelper | PASS | injector accounting: injector sent 126; app saw 126; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 117 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.fish.lan.nohelper | FAIL | the prompt after a HEAD-changing git command did not re-sync the index; injector accounting: injector sent 128; app saw 128; lost-before-app |
| terminal | lab_codebase_index_remote.fish.150.nohelper | FAIL | the prompt after a HEAD-changing git command did not re-sync the index; injector accounting: injector sent 128; app saw 128; lost-before-app |
| terminal | lab_codebase_index_remote.nu.lan.nohelper | FAIL | second sync (files, fragments, embedded, already, stale) = (3, 3, 0, 3, 0): expected exactly the one edited fragment embedded; injector acco |
| terminal | lab_codebase_index_remote.nu.150.nohelper | FAIL | second sync (files, fragments, embedded, already, stale) = (3, 3, 0, 3, 0): expected exactly the one edited fragment embedded; injector acco |
| terminal | lab_diff_panel_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 166; app saw 166; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 157 unbound keydown/typed,  |
| terminal | lab_diff_panel_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 163; app saw 163; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 154 unbound keydown/typed,  |
| terminal | lab_diff_panel_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 166; app saw 166; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 157 unbound keydown/typed,  |
| terminal | lab_diff_panel_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 168; app saw 168; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 159 unbound keydown/typed,  |
| terminal | lab_gh_chip_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 109; app saw 109; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.lan.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t127334g16v/zzqt127334g16v_only'], got []; |
| terminal | lab_tab_remote_dir.nu.150.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t1277kd3elo/zzqt1277kd3elo_only'], got []; |
| terminal | lab_path_detect.bash.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_path_detect.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 101; ap |
| terminal | lab_path_detect.zsh.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_path_detect.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| terminal | lab_path_detect.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_path_detect.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 101; ap |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_chips.bash.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 96; app |
| terminal | lab_chips.zsh.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| terminal | lab_chips.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 96; app |
| terminal | lab_chips.nu.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.nu.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 100; ap |
| terminal | lab_enter_start.bash.150.helper | PASS | injector accounting: injector sent 2043; app saw 2043; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 2006 unbound keydown/ty |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est13931gxzv $EPOCHREALTIME >> /work/u_zsh/es_t1392ukfs.txt', 'echo est |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1593; |
| terminal | lab_enter_start.nu.150.helper | PASS | injector accounting: injector sent 1899; app saw 1899; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1862 unbound keydown/ty |
| terminal | lab_enter_no_network_wait.bash.150.helper | PASS | worst Enter -> write 5.02 ms (limit 75 ms); median 0.72 ms delayed against 0.67 ms undelayed; injector accounting: injector sent 3243; app s |
| terminal | lab_enter_no_network_wait.zsh.150.helper | PASS | worst Enter -> write 2.21 ms (limit 75 ms); median 0.61 ms delayed against 0.71 ms undelayed; injector accounting: injector sent 3182; app s |
| terminal | lab_enter_no_network_wait.fish.150.helper | PASS | worst Enter -> write 4.31 ms (limit 75 ms); median 0.68 ms delayed against 0.67 ms undelayed; injector accounting: injector sent 3243; app s |
| terminal | lab_enter_no_network_wait.nu.150.helper | PASS | worst Enter -> write 0.77 ms (limit 75 ms); median 0.41 ms delayed against 0.36 ms undelayed; injector accounting: injector sent 3849; app s |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.bash.lan.helper | PASS | injector accounting: injector sent 1526; app saw 1526; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1481 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.150.helper | PASS | injector accounting: injector sent 1526; app saw 1526; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1481 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 1520; app saw 1520; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1475 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 1520; app saw 1520; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1475 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | FAIL | escape_1: lines typed into the remote shell: expected ['t1980_aehene17', 't1981_3nv88p6u', 't1982_xy5d8ek0', 't1983_i1f7h9ak', 't1984_krj4eb |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | FAIL | escape_1: lines typed into the remote shell: expected ['t2014_kek2pvwu', 't2015_ynkdv407', 't2016_oosdg0g7', 't2017_e3ouc34f', 't2018_gs58d1 |
| terminal | lab_ctrl_c_no_esc_i.bash.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.bash.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.fish.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.fish.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.helper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t21395vrelb92\\n" / save --append /work/u_nu/cc_t2138pj9a.txt', ' |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t2152dhr7m63o\\n" / save --append /work/u_nu/cc_t2151afu7.txt', ' |
| terminal | lab_helper_idle_exit.bash.lan.helper | SKIP | slow (11 min grace wait): set APPCI_LAB_SLOW=1 for a candidate/release run; injector accounting: injector sent 2; app saw 2; lost-before-app |
| terminal | lab_helper_reconnect.bash.lan.helper | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 129 unbound keydown/typed,  |
| terminal | lab_helper_reconnect.bash.150.helper | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 129 unbound keydown/typed,  |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.lan.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt220019xik9', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_fs_basic.nu.150.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt2204j3tbct', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.lan.helper | FAIL | branch brt22208q3olm not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pen |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt2227yv64to', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_git_read.nu.150.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt2231dysfle', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.lan.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt2253owwp0y', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_run_command.nu.150.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt2257gzjre0', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_helper_killed.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: write half (er |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: write half (er |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| agent | lab_tab_remote_dir.nu.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t227zx5b4/zzqt227zx5b4_only'], got []; Tab |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t266oam89/zzqt266oam89_only'], got []; Tab |
| agent | lab_path_detect.bash.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| agent | lab_path_detect.bash.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| agent | lab_path_detect.zsh.lan.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 3 t |
| agent | lab_path_detect.zsh.150.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 3 t |
| agent | lab_path_detect.fish.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| agent | lab_path_detect.fish.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 3 t |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 70678); the app is relaunched so later cas |
| agent | lab_path_detect.p_tinypath.lan.nohelper | PASS | injector accounting: injector sent 105; app saw 105; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 95 unbound keydown/typed, 3 |
| agent | lab_chips.bash.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.bash.150.nohelper | FAIL | 35 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.zsh.150.nohelper | FAIL | 35 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.fish.lan.nohelper | FAIL | 55 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.fish.150.nohelper | FAIL | 37 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.nu.lan.nohelper | FAIL | 55 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.nu.150.nohelper | FAIL | 37 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_enter_start.bash.150.nohelper | PASS | injector accounting: injector sent 1949; app saw 1949; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1912 unbound keydown/ty |
| agent | lab_enter_start.zsh.150.nohelper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_enter_start.nu.150.nohelper | PASS | injector accounting: injector sent 1838; app saw 1838; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1801 unbound keydown/ty |
| agent | lab_enter_no_network_wait.bash.150.nohelper | PASS | worst Enter -> write 5.09 ms (limit 75 ms); median 0.77 ms delayed against 0.97 ms undelayed; injector accounting: injector sent 3122; app s |
| agent | lab_enter_no_network_wait.zsh.150.nohelper | PASS | worst Enter -> write 5.00 ms (limit 75 ms); median 0.77 ms delayed against 0.87 ms undelayed; injector accounting: injector sent 3061; app s |
| agent | lab_enter_no_network_wait.fish.150.nohelper | PASS | worst Enter -> write 1.72 ms (limit 75 ms); median 0.79 ms delayed against 0.83 ms undelayed; injector accounting: injector sent 3122; app s |
| agent | lab_enter_no_network_wait.nu.150.nohelper | PASS | worst Enter -> write 4.45 ms (limit 75 ms); median 0.49 ms delayed against 0.67 ms undelayed; injector accounting: injector sent 3728; app s |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.bash.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.bash.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.fish.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.fish.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.fish.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.fish.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.fish.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.fish.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.nu.lan.nohelper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t689gm5pb0tl\\n" / save --append /work/u_nu/cc_t688si8y.txt', ' _ |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t702rch4maoa\\n" / save --append /work/u_nu/cc_t701rgsd.txt', ' _ |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_nohelper_sh_login_keys.dash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 357 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_keys.ash.lan.nohelper | PASS | injector accounting: injector sent 355; app saw 355; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 350 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_keys.bash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 349 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_profile.dash.lan.nohelper | PASS | injector accounting: injector sent 128; app saw 128; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 123 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_profile.ash.lan.nohelper | PASS | injector accounting: injector sent 126; app saw 126; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 121 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_profile.bash.lan.nohelper | PASS | injector accounting: injector sent 120; app saw 120; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 112 unbound keydown/typed,  |
| agent | lab_password_prompt.p_password.lan.nohelper | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 180 unbound keydown/typed,  |
| agent | lab_password_prompt.p_kbdint.lan.nohelper | PASS | injector accounting: injector sent 189; app saw 189; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 176 unbound keydown/typed,  |
| agent | lab_read_files_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 246; app saw 246; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 235 unbound keydown/typed,  |
| agent | lab_read_files_remote.nu.lan.nohelper | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; injector accounting: injector sent 248; app saw 248; lost-be |
| agent | lab_diff_apply_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 259; app saw 259; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 246 unbound keydown/typed,  |
| agent | lab_diff_apply_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 248 unbound keydown/typed,  |
| agent | lab_agent_remote_flow.bash.lan.nohelper | PASS | injector accounting: injector sent 452; app saw 452; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 440 unbound keydown/typed,  |
| agent | lab_agent_remote_flow.nu.lan.nohelper | FAIL | grep for the token found [], expected exactly b.txt and src/lib.rs; glob *.txt returned []; injector accounting: injector sent 446; app saw  |
| agent | lab_agent_remote_errors.bash.lan.nohelper | PASS | injector accounting: injector sent 344; app saw 344; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 334 unbound keydown/typed,  |
| agent | lab_agent_remote_errors.nu.lan.nohelper | FAIL | a glob with no match returned {'status': 'error', 'message': "Error: nu::parser::shell_andand\n\n  x The '&&' operator is not supported in N |
| agent | lab_agent_remote_drop.bash.lan.nohelper | PASS | injector accounting: injector sent 266; app saw 266; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 255 unbound keydown/typed,  |
| agent | lab_agent_remote_drop.nu.lan.nohelper | PASS | injector accounting: injector sent 283; app saw 283; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 272 unbound keydown/typed,  |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_agent_suggestions_ssh.bash.lan.nohelper | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 165 unbound keydown/typed,  |
| agent | lab_fam_agent_edit_conflict.bash.lan.nohelper | PASS | partial accept: the diff card has no per-hunk accept (Up/Down only move between hunks, code_diff_view.rs); accept applies every hunk, reject |
| agent | lab_fam_agent_edit_conflict.nu.lan.nohelper | PASS | partial accept: the diff card has no per-hunk accept (Up/Down only move between hunks, code_diff_view.rs); accept applies every hunk, reject |
| agent | lab_fam_agent_delete_conflict.bash.lan.nohelper | PASS | injector accounting: injector sent 152; app saw 152; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 142 unbound keydown/typed,  |
| agent | lab_fam_agent_edit_endings.bash.lan.nohelper | PASS | injector accounting: injector sent 1308; app saw 1308; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 1286 unbound keydown/ty |
| agent | lab_fam_agent_edit_touch_only.bash.lan.nohelper | PASS | injector accounting: injector sent 309; app saw 309; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 299 unbound keydown/typed,  |
| agent | lab_freeze_and_wake.bash.lan.nohelper | NOREF | NO-REFERENCE: keys typed while frozen: the line typed while the app was frozen never arrived (the display system did not hold the keys); the |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 3523 -> 19825); a relaunched app starts in a terminal session, so this is  |
| agent | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t854a18dhy/zzqt854a18dhy_only'], got []; T |
| agent | lab_tab_remote_dir.nu.150.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t8586uj7ul/zzqt8586uj7ul_only'], got []; T |
| agent | lab_path_detect.bash.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=1 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_path_detect.bash.150.helper | FAIL | typing a remote path opened 10 exec and 0 subsystem channel(s) on the server (expected none); helper takeover settled: executor_swapped=True |
| agent | lab_path_detect.zsh.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_path_detect.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 97; app |
| agent | lab_path_detect.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_path_detect.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_chips.bash.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=1 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.bash.150.helper | FAIL | 40 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; helper takeover settled: executor_swapped=True pen |
| agent | lab_chips.zsh.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 92; app |
| agent | lab_chips.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| agent | lab_chips.nu.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.nu.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 98; app |
| agent | lab_enter_start.bash.150.helper | PASS | injector accounting: injector sent 1982; app saw 1982; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1945 unbound keydown/ty |
| agent | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est976huv67 $EPOCHREALTIME >> /work/u_zsh/es_t975oap2.txt', 'echo est97 |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1593; |
| agent | lab_enter_start.nu.150.helper | PASS | injector accounting: injector sent 1899; app saw 1899; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1862 unbound keydown/ty |
| agent | lab_enter_no_network_wait.bash.150.helper | PASS | worst Enter -> write 5.29 ms (limit 75 ms); median 0.85 ms delayed against 0.81 ms undelayed; injector accounting: injector sent 3243; app s |
| agent | lab_enter_no_network_wait.zsh.150.helper | PASS | worst Enter -> write 4.35 ms (limit 75 ms); median 0.78 ms delayed against 0.86 ms undelayed; injector accounting: injector sent 3182; app s |
| agent | lab_enter_no_network_wait.fish.150.helper | PASS | worst Enter -> write 2.70 ms (limit 75 ms); median 0.79 ms delayed against 0.83 ms undelayed; injector accounting: injector sent 3243; app s |
| agent | lab_enter_no_network_wait.nu.150.helper | PASS | worst Enter -> write 1.23 ms (limit 75 ms); median 0.48 ms delayed against 0.60 ms undelayed; injector accounting: injector sent 3849; app s |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; the DSCP class of  |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; the DSCP class of  |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; the DSCP class of  |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; the DSCP class of  |
| agent | lab_ctrl_c_no_esc_i.bash.lan.helper | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; injector accountin |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.lan.helper | PASS | after lab_ctrl_c_no_esc_i.bash.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.bash.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.150.helper | PASS | after lab_ctrl_c_no_esc_i.bash.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.fish.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.fish.lan.helper | PASS | after lab_ctrl_c_no_esc_i.fish.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.fish.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.fish.150.helper | PASS | after lab_ctrl_c_no_esc_i.fish.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.nu.lan.helper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t1527f5uoyyxz\\n" / save --append /work/u_nu/cc_t1526pjom.txt', ' |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.lan.helper | PASS | after lab_ctrl_c_no_esc_i.nu.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t15409m3cudzl\\n" / save --append /work/u_nu/cc_t15399s4g.txt', ' |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t227zx5b4/zzqt227zx5b4_only'], got []; Tab |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t266oam89/zzqt266oam89_only'], got []; Tab |
| terminal | lab_path_detect.fish.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.fish.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 88 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 1377037); the app is relaunched so later c |
| terminal | lab_path_detect.p_tinypath.lan.nohelper | PASS | injector accounting: injector sent 105; app saw 105; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 95 unbound keydown/typed, 6 |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.bash.150.nohelper | FAIL | 16 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 16 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 55 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.fish.150.nohelper | FAIL | 17 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.nu.lan.nohelper | FAIL | 55 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_chips.nu.150.nohelper | FAIL | 17 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1493; |
| terminal | lab_enter_no_network_wait.fish.150.nohelper | PASS | worst Enter -> write 1.37 ms (limit 75 ms); median 1.11 ms delayed against 1.15 ms undelayed; injector accounting: injector sent 3122; app s |
| terminal | lab_enter_no_network_wait.nu.150.nohelper | PASS | worst Enter -> write 1.37 ms (limit 75 ms); median 1.18 ms delayed against 1.15 ms undelayed; injector accounting: injector sent 3728; app s |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [escape_connect_bare] app log after the escape: no ssh_esca |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | FAIL | escape_1: lines typed into the remote shell: expected ['t317_vmiihicr', 't318_rwajae31', 't319_ztiz1x2w', 't320_s4lratod', 't321_5daf3api'], |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | FAIL | escape_1: lines typed into the remote shell: expected ['t351_sr45whhn', 't352_2yn0dly1', 't353_x7je6kcn', 't354_683tt4ib', 't355_21w1xnrn'], |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t401phxecn1v\\n" / save --append /work/u_nu/cc_t400mx5o.txt', ' _ |
| terminal | lab_no_helper_chip.zsh.lan.nohelper | PASS | injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 73 unbound keydown/typed, 4 t |
| terminal | lab_no_helper_chip.fish.lan.nohelper | FAIL | no 'shell mode' chip on screen in a no-helper ssh session; injector accounting: injector sent 82; app saw 82; lost-before-app 0 (Enter: sent |
| terminal | lab_no_helper_chip.nu.lan.nohelper | FAIL | an ERROR text is on screen next to the no-helper chip: it must stay informational; injector accounting: injector sent 88; app saw 88; lost-b |
| terminal | lab_nu_agent_pager_env.nu.lan.nohelper | PASS | injector accounting: injector sent 1100; app saw 1100; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 1086 unbound keydown/ty |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | branch brt427bqdsqu not on screen after cd into the repo on the remote; shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 32 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 32 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 104 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route li |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | branch brt447li1cew not on screen after cd into the repo on the remote; shell arm: 67 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | branch brt451xxt0z4 not on screen after cd into the repo on the remote; shell arm: 92 out-of-band channels for 4 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | branch brt455u0ji4q not on screen after cd into the repo on the remote; shell arm: 66 out-of-band channels for 4 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 32 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route lin |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | branch brt467k00tro not on screen after cd into the repo on the remote; shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | branch brt471wfq5z3 not on screen after cd into the repo on the remote; shell arm: 65 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 104 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route li |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | branch brt479hhuj47 not on screen after cd into the repo on the remote; shell arm: 67 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | branch brt4835jdmzv not on screen after cd into the repo on the remote; shell arm: 92 out-of-band channels for 4 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | branch brt487sc1fnb not on screen after cd into the repo on the remote; shell arm: 66 out-of-band channels for 4 prompt(s), expected 1..prom |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 113 recorded lines |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 113 recorded lines |
| terminal | lab_nohelper_unavailable.bash.lan.nohelper | PASS | injector accounting: injector sent 172; app saw 172; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 164 unbound keydown/typed,  |
| terminal | lab_nohelper_unavailable.bash.150.nohelper | PASS | injector accounting: injector sent 172; app saw 172; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 164 unbound keydown/typed,  |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_file_tree_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 98 unbound keydown/typed, 4 |
| terminal | lab_file_tree_remote.fish.150.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 98 unbound keydown/typed, 4 |
| terminal | lab_file_tree_remote.nu.lan.nohelper | FAIL | the next prompt did not bring the new file into the tree (no hop=file_tree update naming it); the removal of the file was never reported (no |
| terminal | entered_agent_view_in_lab_file_tree_remote.nu.lan.nohelper | FAIL | after the TERMINAL case lab_file_tree_remote.nu.lan.nohelper the pane shows AGENT view: something the case typed was sent to the agent inste |
| terminal | lab_file_tree_remote.nu.150.nohelper | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; mode guard: the pane was in agent before this terminal case; |
| terminal | lab_skills_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 101; app saw 101; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 92 unbound keydown/typed, 4 |
| terminal | lab_skills_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 103; app saw 103; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 94 unbound keydown/typed, 4 |
| terminal | lab_skills_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 107; app saw 107; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 98 unbound keydown/typed, 4 |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t555593x41.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t5580pyrb7.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t561g6p1qi.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t5649xqhfw.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_codebase_index_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 116 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.bash.150.nohelper | PASS | injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 116 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 123; app saw 123; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 114 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.zsh.150.nohelper | PASS | injector accounting: injector sent 123; app saw 123; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 114 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.fish.lan.nohelper | FAIL | the prompt after a HEAD-changing git command did not re-sync the index; injector accounting: injector sent 125; app saw 125; lost-before-app |
| terminal | lab_codebase_index_remote.fish.150.nohelper | FAIL | the prompt after a HEAD-changing git command did not re-sync the index; injector accounting: injector sent 125; app saw 125; lost-before-app |
| terminal | lab_codebase_index_remote.nu.lan.nohelper | FAIL | second sync (files, fragments, embedded, already, stale) = (3, 3, 0, 3, 0): expected exactly the one edited fragment embedded; injector acco |
| terminal | lab_codebase_index_remote.nu.150.nohelper | FAIL | second sync (files, fragments, embedded, already, stale) = (3, 3, 0, 3, 0): expected exactly the one edited fragment embedded; injector acco |
| terminal | lab_diff_panel_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 163; app saw 163; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 154 unbound keydown/typed,  |
| terminal | lab_diff_panel_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 160; app saw 160; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 151 unbound keydown/typed,  |
| terminal | lab_diff_panel_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 163; app saw 163; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 154 unbound keydown/typed,  |
| terminal | lab_diff_panel_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 165; app saw 165; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 156 unbound keydown/typed,  |
| terminal | lab_gh_chip_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 107; app saw 107; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 98 unbound keydown/typed, 4 |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.helper | FAIL | the shell received ['cd /work/u_bash/tabfx_t638ruginx/zzqt638ruginx'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.helper | FAIL | the shell received ['cd /work/u_fish/tabfx_t65154yuq9/zzqt65154yuq9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| terminal | lab_tab_remote_dir.nu.lan.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t655jkqg0h/zzqt655jkqg0h_only'], got []; T |
| terminal | lab_tab_remote_dir.nu.150.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t659fovyvw/zzqt659fovyvw_only'], got []; T |
| terminal | lab_path_detect.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| terminal | lab_path_detect.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 97; app |
| terminal | lab_path_detect.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_path_detect.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_chips.bash.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| terminal | lab_chips.zsh.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 92; app |
| terminal | lab_chips.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| terminal | lab_chips.nu.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| terminal | lab_chips.nu.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 98; app |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est7062fmc2 $EPOCHREALTIME >> /work/u_zsh/es_t705cfup.txt', 'echo est70 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_enter_no_network_wait.fish.150.helper | PASS | worst Enter -> write 1.51 ms (limit 75 ms); median 1.14 ms delayed against 1.13 ms undelayed; injector accounting: injector sent 3122; app s |
| terminal | lab_enter_no_network_wait.nu.150.helper | PASS | worst Enter -> write 1.35 ms (limit 75 ms); median 1.18 ms delayed against 1.18 ms undelayed; injector accounting: injector sent 3728; app s |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [escape_connect_bare] app log after the escape: no ssh_esca |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | FAIL | escape_1: lines typed into the remote shell: expected ['t1005_mpa4upi0', 't1006_y75mzq7s', 't1007_okxb1rxg', 't1008_wwl0feoo', 't1009_cnrium |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | FAIL | escape_1: lines typed into the remote shell: expected ['t1039_m2ek3tdy', 't1040_slk2ttb4', 't1041_4sx84uff', 't1042_49l31mvr', 't1043_iqyvaf |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] effect file: expected ['t1087qfs4563o', 't1089gm5 |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t11092ibdzftz\\n" / save --append /work/u_nu/cc_t1108kgja.txt', ' |
| terminal | lab_helper_reconnect.bash.lan.helper | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 129 unbound keydown/typed,  |
| terminal | lab_helper_reconnect.bash.150.helper | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 129 unbound keydown/typed,  |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | FAIL | branch brt11390n6d1o not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pen |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.lan.helper | FAIL | branch brt1152sl4rsx not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pen |
| terminal | lab_fam_fs_basic.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.lan.helper | FAIL | branch brt1159iao7t9 not on screen after cd into the repo on the remote; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_fam_fs_basic.nu.150.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt1163g892k0', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.lan.helper | FAIL | branch brt1179j5vcu3 not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pen |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | FAIL | the remote shell received lines the user did not type: ['cd /work/u_nu/famt1186jowb13', '__warp_report_input', 'ls', '__warp_report_input']; |
| terminal | lab_fam_git_read.nu.150.helper | FAIL | branch brt119049lbys not on screen after cd into the repo on the remote; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_fam_run_command.bash.lan.helper | FAIL | branch brt1194dzsfzl not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pen |
| terminal | lab_fam_run_command.bash.150.helper | FAIL | branch brt11989pkbl0 not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 sync |
| terminal | lab_fam_run_command.zsh.lan.helper | FAIL | branch brt1202qb9ufa not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pen |
| terminal | lab_fam_run_command.zsh.150.helper | FAIL | branch brt1206iu9xoo not on screen after cd into the repo on the remote; mode before the case could not be established (no hop=view_mode lin |
| terminal | lab_fam_run_command.fish.lan.helper | FAIL | branch brt1210utcgxs not on screen after cd into the repo on the remote; mode before the case could not be established (no hop=view_mode lin |
| terminal | lab_fam_run_command.fish.150.helper | FAIL | branch brt1214e44plu not on screen after cd into the repo on the remote; mode before the case could not be established (no hop=view_mode lin |
| terminal | lab_fam_run_command.nu.lan.helper | FAIL | branch brt1218k5e65v not on screen after cd into the repo on the remote; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_fam_run_command.nu.150.helper | FAIL | branch brt1222zcod2k not on screen after cd into the repo on the remote; the remote shell received lines the user did not type: ['cd /work/u |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; NO-REFERENCE: UNVE |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; helper takeover NO |
| terminal | lab_helper_killed.bash.150.helper | FAIL | branch brt1230oot1du not shown after the helper was killed: the read did not fall back to the shell; mode before the case could not be estab |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; helper takeover NO |
| terminal | lab_helper_killed.zsh.150.helper | FAIL | branch brt1237joq657 not shown after the helper was killed: the read did not fall back to the shell; mode before the case could not be estab |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| agent | lab_tab_remote_dir.nu.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t227zx5b4/zzqt227zx5b4_only'], got []; Tab |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t266oam89/zzqt266oam89_only'], got []; Tab |
| agent | lab_path_detect.fish.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| agent | lab_path_detect.fish.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 88 unbound keydown/typed, 6 t |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 1598701); the app is relaunched so later c |
| agent | lab_path_detect.p_tinypath.lan.nohelper | PASS | injector accounting: injector sent 105; app saw 105; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 95 unbound keydown/typed, 6 |
| agent | lab_chips.bash.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.bash.150.nohelper | FAIL | 16 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.zsh.150.nohelper | FAIL | 16 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.fish.lan.nohelper | FAIL | 55 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.fish.150.nohelper | FAIL | 17 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.nu.lan.nohelper | FAIL | 55 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_chips.nu.150.nohelper | FAIL | 17 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened,  |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1493; |
| agent | lab_enter_no_network_wait.fish.150.nohelper | PASS | worst Enter -> write 1.63 ms (limit 75 ms); median 1.43 ms delayed against 1.47 ms undelayed; injector accounting: injector sent 3122; app s |
| agent | lab_enter_no_network_wait.nu.150.nohelper | PASS | worst Enter -> write 1.70 ms (limit 75 ms); median 1.47 ms delayed against 1.47 ms undelayed; injector accounting: injector sent 3728; app s |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t3100po71qaa\\n" / save --append /work/u_nu/cc_t3095cyh.txt', ' _ |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_read_files_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 246; app saw 246; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 235 unbound keydown/typed,  |
| agent | lab_read_files_remote.nu.lan.nohelper | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; injector accounting: injector sent 248; app saw 248; lost-be |
| agent | lab_diff_apply_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 259; app saw 259; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 246 unbound keydown/typed,  |
| agent | lab_diff_apply_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 248 unbound keydown/typed,  |
| agent | lab_agent_remote_flow.bash.lan.nohelper | PASS | injector accounting: injector sent 452; app saw 452; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 440 unbound keydown/typed,  |
| agent | lab_agent_remote_flow.nu.lan.nohelper | FAIL | grep for the token found [], expected exactly b.txt and src/lib.rs; glob *.txt returned []; injector accounting: injector sent 446; app saw  |
| agent | lab_agent_remote_errors.nu.lan.nohelper | FAIL | a glob with no match returned {'status': 'error', 'message': "Error: nu::parser::shell_andand\n\n  x The '&&' operator is not supported in N |
| agent | lab_agent_remote_drop.nu.lan.nohelper | PASS | injector accounting: injector sent 283; app saw 283; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 272 unbound keydown/typed,  |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_fam_agent_edit_conflict.nu.lan.nohelper | PASS | partial accept: the diff card has no per-hunk accept (Up/Down only move between hunks, code_diff_view.rs); accept applies every hunk, reject |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 1636240 -> 1658874); a relaunched app starts in a terminal session, so thi |
| agent | lab_tab_remote_dir.bash.150.helper | FAIL | the shell received ['cd /work/u_bash/tabfx_t363et2ns0/zzqt363et2ns0'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.helper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t3702yn0dl/zzqt3702yn0dl'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | FAIL | the shell received ['cd /work/u_fish/tabfx_t377b21w1x/zzqt377b21w1x'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| agent | lab_tab_remote_dir.nu.lan.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t381r0sr2e/zzqt381r0sr2e_only'], got []; T |
| agent | lab_tab_remote_dir.nu.150.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t3856upjkq/zzqt3856upjkq_only'], got []; T |
| agent | lab_path_detect.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| agent | lab_path_detect.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 97; app |
| agent | lab_path_detect.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_path_detect.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_chips.bash.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=1 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| agent | lab_chips.zsh.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.zsh.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 92; app |
| agent | lab_chips.fish.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.fish.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| agent | lab_chips.nu.lan.helper | PASS | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; injector accounting: i |
| agent | lab_chips.nu.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 98; app |
| agent | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_enter_no_network_wait.fish.150.helper | PASS | worst Enter -> write 1.71 ms (limit 75 ms); median 1.45 ms delayed against 1.48 ms undelayed; injector accounting: injector sent 3122; app s |
| agent | lab_enter_no_network_wait.nu.150.helper | PASS | worst Enter -> write 1.72 ms (limit 75 ms); median 1.50 ms delayed against 1.50 ms undelayed; injector accounting: injector sent 3728; app s |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x1 gap 0 ms] the shell received ['sleep 30sec', ' __warp_report_input', '"t740wbvjefx7\\n" / save --append /work/u_nu/cc_t739ktwp.txt', ' _ |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | cadence | PASS | injector accounting: injector sent 179; app saw 179; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 174 unbound keydown/typed,  |
| terminal | mac_pointer_chord_proof | PASS | display scale 2 (screenshot 3024x1964 px over a display of 1512x982 points); close key for the cases after this one: cmd+w (APPCI_KEY_CLOSE, |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: injector sent 2064; app saw 2064; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2002 unbound keydown/ty |
| terminal | rollover | PASS | injector accounting: injector sent 702; app saw 702; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 680 unbound keydown/typed |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 750; app saw 750; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 647 unbound keydown/typed |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 1 ta |
| terminal | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 135 unbound keydown/typed,  |
| terminal | keydown_count | PASS | injector accounting: injector sent 564; app saw 564; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| terminal | big_paste | PASS | big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); injector accounting: injector sent 21; app saw 21 |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 74 unbound keydown/typed, 1 t |
| terminal | subshells | PASS | injector accounting: injector sent 963; app saw 963; lost-before-app 0 (Enter: sent 30, app saw 30); of the app's: 941 unbound keydown/typed |
| terminal | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| terminal | alt_screen | PASS | injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 75 unbound keydown/typed, 2 t |
| terminal | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 256 unbound keydown/typed,  |
| terminal | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 298; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 288 unbound keydown/typed,  |
| terminal | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 235; app saw 235; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 229 unbound keydown/typed,  |
| terminal | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 262 unbound keydown/typed,  |
| terminal | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 265 unbound keydown/typed,  |
| terminal | subshell_sh_probe_empty_ps1 | PASS | empty_ps1 typed-before-the-setup leg: needs the Linux blocked-in-read fact (/proc/<pid>/syscall); not run on darwin; injector accounting: in |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (`sudo -n true` failed (rc 1: sudo: a password is required) and APPCI_SUDO_USER is not set). NEEDS: a su |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available: docker is not installed. NEEDS: the docker engine on this test machine (Debian/Ubuntu pac |
| terminal | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty startup directory (proved on a pty before the case types anything); injector accounting:  |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@40 ms, InitShell@81 ms, Bootstrapped@1910 ms; injector accounting: injector sent 260;  |
| terminal | subshell_line_editor_proof | PASS | lep: setup armed decision=Write(None); InitSubshell@42 ms, InitShell@83 ms, Bootstrapped@1917 ms; injector accounting: injector sent 254; ap |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.25 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 2 take |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.55 ms (p95 2.28, n=60); the wait itself: median 0.0 |
| terminal | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| terminal | latency_first_key_after_idle | NOREF | First key after 1 s idle: first key after 1 s idle, kl3 post-to-first-changed-pixel at 240 Hz (4.17 ms quantum, not applied as a resolution  |
| terminal | intl_wide_type | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 151 unbound keydown/typed,  |
| terminal | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| terminal | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| terminal | ui_palette | PASS | palette_first_open: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); palette_reopen: no hop=ui_item line (not |
| terminal | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| terminal | intl_compose_linux | SKIP | Linux compose/dead keys need the wayland (wtype) or x11 (xdotool) backend, this is mac; injector accounting: injector sent 2; app saw 2; los |
| terminal | intl_ime_japanese | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| terminal | intl_ime_pinyin | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed,  |
| terminal | ime_compose | SKIP | macOS: NEEDS: the owner enables the Pinyin - Simplified input source for the runner user (System Settings > Keyboard > Text Input > Input So |
| terminal | intl_deadkey_option | PASS | injector accounting: injector sent 813; app saw 813; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 793 unbound keydown/typed |
| terminal | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| terminal | ui_right_click_paste | SKIP | macOS: this case's macOS arm is not enabled. The pointer tool exists since round 15 (mac/tools/mptr.swift, #2061), and the cases that pass m |
| terminal | primary_and_middle_click | SKIP | primary_and_middle_click: macOS has no PRIMARY selection and no middle-click paste (X11 / Wayland concepts): nothing to test there, by desig |
| terminal | ui_drag_drop_file | SKIP | mac: the harness has no drag source for this display system. NEEDS: a harness capability that is not built (a client that owns the drag: wl_ |
| terminal | link_and_path_click | FAIL | Cmd+click on `paedtch4x.rs:140`: the file opened in the app at its first line; the text of the clicked line is not on screen (the :line part |
| terminal | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 55980); the app is relaunched so later cases start clean |
| terminal | wheel_scroll | FAIL | harness exception: PtrError('pointer injector failed: mptr exit 4 (refused a wheel step at (409.5, 441.0): the topmost window there belongs  |
| terminal | resize_reflow_output | PASS | injector accounting: injector sent 585; app saw 585; lost-before-app 0 (Enter: sent 15, app saw 15); of the app's: 537 unbound keydown/typed |
| terminal | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| terminal | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| terminal | minimize_fullscreen | PASS | injector accounting: injector sent 883; app saw 883; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 850 unbound keydown/typed |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| terminal | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 1 t |
| terminal | editor_memory_7mb | FAIL | txt: 3 open/close cycles grew the post-close rss by 36 MB (> 30 MB): 1369, 1414, 1406; judged memory figures: phys_footprint, rss (owner: ea |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (468, 413), tab 3 at (468, 538), tab 4 at (468, 662); tab 3's title was located by OCR (no `appci_find` line for 'v |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key cmd+w; NO-REFERENCE: UNVERIFIABLE: the denial |
| terminal | tabs_two_windows | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; injector accounting: injector sent 2495; app saw 2495; lost- |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 1863; app saw 1863; lost-before-app 0 (Enter: sent 22, app saw 22); of the app's: 1824 unbound keydown/ty |
| terminal | cb_ui_action_latency | FAIL | after the close key on the throw-away tab the shell in front has pid None, expected the pass's own shell 52247 (the tab did not close, or an |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: injector sent 179; app saw 179; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 174 unbound keydown/typed,  |
| agent | mac_pointer_chord_proof | PASS | display scale 2 (screenshot 3024x1964 px over a display of 1512x982 points); close key for the cases after this one: cmd+w (APPCI_KEY_CLOSE, |
| agent | fast_typing | PASS | injector accounting: injector sent 2010; app saw 2010; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1948 unbound keydown/ty |
| agent | rollover | PASS | injector accounting: injector sent 682; app saw 682; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 660 unbound keydown/typed |
| agent | expert_fuzz | PASS | injector accounting: injector sent 735; app saw 735; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 642 unbound keydown/typed |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 1 ta |
| agent | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 135 unbound keydown/typed,  |
| agent | keydown_count | PASS | injector accounting: injector sent 563; app saw 563; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| agent | big_paste | PASS | big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); injector accounting: injector sent 21; app saw 21 |
| agent | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 74 unbound keydown/typed, 1 t |
| agent | subshells | PASS | injector accounting: injector sent 963; app saw 963; lost-before-app 0 (Enter: sent 30, app saw 30); of the app's: 941 unbound keydown/typed |
| agent | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| agent | alt_screen | PASS | injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 75 unbound keydown/typed, 2 t |
| agent | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 256 unbound keydown/typed,  |
| agent | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 298; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 288 unbound keydown/typed,  |
| agent | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 235; app saw 235; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 229 unbound keydown/typed,  |
| agent | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 262 unbound keydown/typed,  |
| agent | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 265 unbound keydown/typed,  |
| agent | subshell_sh_probe_empty_ps1 | PASS | empty_ps1 typed-before-the-setup leg: needs the Linux blocked-in-read fact (/proc/<pid>/syscall); not run on darwin; injector accounting: in |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (`sudo -n true` failed (rc 1: sudo: a password is required) and APPCI_SUDO_USER is not set). NEEDS: a su |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available: docker is not installed. NEEDS: the docker engine on this test machine (Debian/Ubuntu pac |
| agent | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty startup directory (proved on a pty before the case types anything); injector accounting:  |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@37 ms, InitShell@78 ms, Bootstrapped@1913 ms; injector accounting: injector sent 260;  |
| agent | subshell_line_editor_proof | PASS | lep: setup armed decision=Write(None); InitSubshell@38 ms, InitShell@79 ms, Bootstrapped@1903 ms; injector accounting: injector sent 254; ap |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; ssh trial verdicts |
| agent | ssh_failing_not_local | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; ssh_failing_not_lo |
| agent | idle_cpu | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; injector accountin |
| agent | latency_enter | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; measurement mode:  |
| agent | latency_key_log | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; #2037 key to frame |
| agent | latency_paint | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; measurement mode:  |
| agent | latency_first_key_after_idle | NOREF | First key after 1 s idle: first key after 1 s idle, kl3 post-to-first-changed-pixel at 240 Hz (4.17 ms quantum, not applied as a resolution  |
| agent | approval_card_pause | SKIP | no approval card appeared within 120 s (model did not propose the command); injector accounting: injector sent 88; app saw 88; lost-before-a |
| agent | intl_wide_type | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 151 unbound keydown/typed,  |
| agent | intl_wide_paste | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; verified: 10 effec |
| agent | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| agent | ui_palette | PASS | palette_first_open: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); palette_reopen: no hop=ui_item line (not |
| agent | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| agent | intl_compose_linux | SKIP | Linux compose/dead keys need the wayland (wtype) or x11 (xdotool) backend, this is mac; injector accounting: injector sent 2; app saw 2; los |
| agent | intl_ime_japanese | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| agent | intl_ime_pinyin | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| agent | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines wer |
| agent | ime_compose | SKIP | macOS: NEEDS: the owner enables the Pinyin - Simplified input source for the runner user (System Settings > Keyboard > Text Input > Input So |
| agent | intl_deadkey_option | PASS | injector accounting: injector sent 813; app saw 813; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 793 unbound keydown/typed |
| agent | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| agent | primary_and_middle_click | SKIP | primary_and_middle_click: macOS has no PRIMARY selection and no middle-click paste (X11 / Wayland concepts): nothing to test there, by desig |
| agent | link_and_path_click | FAIL | Cmd+click on `paxa4fnc7.rs:140`: the file opened in the app at its first line; the text of the clicked line is not on screen (the :line part |
| agent | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 18040); the app is relaunched so later cases start clean |
| agent | wheel_scroll | FAIL | harness exception: PtrError('pointer injector failed: mptr exit 4 (refused a wheel step at (409.5, 448.0): the topmost window there belongs  |
| agent | resize_reflow_output | PASS | injector accounting: injector sent 585; app saw 585; lost-before-app 0 (Enter: sent 15, app saw 15); of the app's: 537 unbound keydown/typed |
| agent | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| agent | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| agent | minimize_fullscreen | PASS | injector accounting: injector sent 883; app saw 883; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 850 unbound keydown/typed |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| agent | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent | tab_pointer_use | NOREF | the app log says agent but the screen reads as terminal (OCR); the log decides; NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles |
| agent | tabs_two_windows | PASS | injector accounting: injector sent 2444; app saw 2444; lost-before-app 0 (Enter: sent 28, app saw 28); of the app's: 2399 unbound keydown/ty |
| agent | tabs_streaming | PASS | injector accounting: injector sent 1862; app saw 1862; lost-before-app 0 (Enter: sent 22, app saw 22); of the app's: 1824 unbound keydown/ty |
| agent@fake | agent_suggestion_chip_keeps_output | PASS | the suggestion going is not judged in the agent view: Escape leaves the view there, a layout change of its own (the Rust view test a_late_pr |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | cadence | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 177 unbound keydown/typed,  |
| terminal | mac_pointer_chord_proof | SKIP | macOS only: this case proves the macOS harness's own pointer tool (mac/tools/mptr) and app-only chords (mac/tools/appkey); Linux and Windows |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot ga |
| terminal | fast_typing | PASS | injector accounting: injector sent 2072; app saw 2072; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2010 unbound keydown/ty |
| terminal | rollover | PASS | injector accounting: injector sent 702; app saw 702; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 680 unbound keydown/typed |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 659 unbound keydown/typed |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 135 unbound keydown/typed,  |
| terminal | keydown_count | PASS | injector accounting: injector sent 564; app saw 564; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| terminal | big_paste | PASS | big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); injector accounting: injector sent 21; app saw 21 |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 73 unbound keydown/typed, 2 t |
| terminal | subshells | PASS | nu not installed: skipped; injector accounting: injector sent 755; app saw 755; lost-before-app 0 (Enter: sent 24, app saw 24); of the app's |
| terminal | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| terminal | alt_screen | PASS | injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 75 unbound keydown/typed, 3 t |
| terminal | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 256 unbound keydown/typed,  |
| terminal | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 298; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 287 unbound keydown/typed,  |
| terminal | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unb |
| terminal | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 261 unbound keydown/typed,  |
| terminal | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 264 unbound keydown/typed,  |
| terminal | subshell_sh_probe_empty_ps1 | PASS | injector accounting: injector sent 692; app saw 692; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 678 unbound keydown/typed |
| terminal | subshell_ready_relay_shape | PASS | injector accounting: injector sent 477; app saw 477; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 463 unbound keydown/typed |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available: the docker daemon is not usable by the CI user (`docker info` failed, rc 125: Emulate Doc |
| terminal | subshell_ready_zsh_newuser_install | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@17 ms, InitShell@40 ms, Bootstrapped@1298 ms; injector accounting: injector sent 260;  |
| terminal | subshell_line_editor_proof | PASS | lep: setup armed decision=Write(None); InitSubshell@13 ms, InitShell@38 ms, Bootstrapped@1294 ms; injector accounting: injector sent 254; ap |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.39 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #1176 key -> pty write median 0.66 ms (p95 0.72, n=60): within the 1 ms target; #1995 notifies logged before 13 of 76 key frame build(s): In |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| terminal | intl_wide_type | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 151 unbound keydown/typed,  |
| terminal | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| terminal | flood_scrollback_find | PASS | flood_scrollback_find; app log: 31 find_scroll hop(s), moved=true in 1, last: result=scroll block=598 moved=true viewport_lines=52.11 render |
| terminal | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| terminal | ui_palette | PASS | palette_first_open: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); palette_reopen: no hop=ui_item line (not |
| terminal | ui_readline_keys | PASS | injector accounting: injector sent 215; app saw 215; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 204 unbound keydown/typed,  |
| terminal | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| terminal | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| terminal | intl_ime_japanese | SKIP | macOS input methods: needs the mac backend (this is wayland). NEEDS: on Linux an input-method daemon (ibus or fcitx5 with the Japanese / Pin |
| terminal | intl_ime_pinyin | SKIP | macOS input methods: needs the mac backend (this is wayland). NEEDS: on Linux an input-method daemon (ibus or fcitx5 with the Japanese / Pin |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed,  |
| terminal | ime_compose | NOREF | injector accounting: injector sent 337; app saw 319; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 306 unbound keydown/typed, |
| terminal | intl_deadkey_option | SKIP | macOS input methods: needs the mac backend (this is wayland). NEEDS: on Linux an input-method daemon (ibus or fcitx5 with the Japanese / Pin |
| terminal | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| terminal | ui_right_click_paste | PASS | injector accounting: injector sent 18; app saw 18; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 15 unbound keydown/typed, 2 t |
| terminal | primary_and_middle_click | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 218 unbound keydown/typed,  |
| terminal | ui_drag_drop_file | SKIP | wayland: the harness has no drag source for this display system. NEEDS: a harness capability that is not built (a client that owns the drag: |
| terminal | link_and_path_click | PASS | the path went to the desktop opener (system): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expect |
| terminal | wheel_scroll | PASS | injector accounting: injector sent 252; app saw 252; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 240 unbound keydown/typed,  |
| terminal | resize_reflow_output | PASS | injector accounting: injector sent 590; app saw 590; lost-before-app 0 (Enter: sent 15, app saw 15); of the app's: 542 unbound keydown/typed |
| terminal | editor_daily | PASS | replace: opener exit code 0; close: opener exit code 0; close_enter: opener exit code 0; external: opener exit code 0; injector accounting:  |
| terminal | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| terminal | pane_resize_maximize | PASS | injector accounting: injector sent 778; app saw 778; lost-before-app 0 (Enter: sent 19, app saw 19); of the app's: 745 unbound keydown/typed |
| terminal | minimize_fullscreen | PASS | injector accounting: injector sent 890; app saw 890; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 855 unbound keydown/typed |
| terminal | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| terminal | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | editor_memory_7mb | PASS | txt: opening the 7.0 MB file added 97 MB of rss (13.8x the file); txt: the app's own 'editor_memory open' breakdown walk took 0.61 ms on the |
| terminal | ui_vertical_tabs_panel_click | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; vertical tabs: on in the profile's settings (the owner's lay |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (125, 155), tab 3 at (125, 218), tab 4 at (125, 280); tab 3's title was located by OCR (no `appci_find` line for 'v |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| terminal | gpu_lost_device_recovers | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; device reported lost once; renderer rebuilt on Intel(R) UHD  |
| terminal | tabs_two_windows | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; injector accounting: injector sent 2570; app saw 2570; lost- |
| terminal | upgrade_over_previous | NOREF | the previous build's state is packed in /home/<user>/builds/r_hQtIdMf/0/winters-phosphor/phosphor/appci-out/wayland/run/fixtures/prev-state. |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 1963; app saw 1963; lost-before-app 0 (Enter: sent 23, app saw 23); of the app's: 1923 unbound keydown/ty |
| terminal | cb_ui_action_latency | FAIL | after the close key on the throw-away tab the shell in front has pid None, expected the pass's own shell 1176311 (the tab did not close, or  |
| terminal | meta_and_word_keys | PASS | injector accounting: injector sent 761; app saw 761; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 617 unbound keydown/typed |
| terminal | pane_not_local_after_meta_and_word_keys | FAIL | after meta_and_word_keys the pane's shell was not the root shell (other tab or pane: the shell in front (pid 1170937) was not started from t |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 177 unbound keydown/typed,  |
| agent | mac_pointer_chord_proof | SKIP | macOS only: this case proves the macOS harness's own pointer tool (mac/tools/mptr) and app-only chords (mac/tools/appkey); Linux and Windows |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | rollover | PASS | injector accounting: injector sent 682; app saw 682; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 660 unbound keydown/typed |
| agent | expert_fuzz | PASS | injector accounting: injector sent 759; app saw 759; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 648 unbound keydown/typed |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 135 unbound keydown/typed,  |
| agent | keydown_count | PASS | injector accounting: injector sent 563; app saw 563; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| agent | big_paste | PASS | big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); injector accounting: injector sent 21; app saw 21 |
| agent | tab_enter | PASS | injector accounting: injector sent 476; app saw 476; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 450 unbound keydown/typed |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 73 unbound keydown/typed, 2 t |
| agent | subshells | PASS | nu not installed: skipped; injector accounting: injector sent 755; app saw 755; lost-before-app 0 (Enter: sent 24, app saw 24); of the app's |
| agent | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| agent | alt_screen | PASS | injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 75 unbound keydown/typed, 3 t |
| agent | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 256 unbound keydown/typed,  |
| agent | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 298; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 287 unbound keydown/typed,  |
| agent | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unb |
| agent | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 261 unbound keydown/typed,  |
| agent | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 264 unbound keydown/typed,  |
| agent | subshell_sh_probe_empty_ps1 | PASS | injector accounting: injector sent 692; app saw 692; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 678 unbound keydown/typed |
| agent | subshell_ready_relay_shape | PASS | injector accounting: injector sent 477; app saw 477; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 463 unbound keydown/typed |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available: the docker daemon is not usable by the CI user (`docker info` failed, rc 125: Emulate Doc |
| agent | subshell_ready_zsh_newuser_install | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@15 ms, InitShell@37 ms, Bootstrapped@1311 ms; injector accounting: injector sent 260;  |
| agent | subshell_line_editor_proof | PASS | lep: setup armed decision=Write(None); InitSubshell@22 ms, InitShell@61 ms, Bootstrapped@1313 ms; injector accounting: injector sent 254; ap |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.43 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #1176 key -> pty write median 0.67 ms (p95 0.72, n=60): within the 1 ms target; #1995 notifies logged before 13 of 76 key frame build(s): In |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| agent | approval_card_pause | SKIP | no approval card appeared within 120 s (model did not propose the command); injector accounting: injector sent 88; app saw 88; lost-before-a |
| agent | intl_wide_type | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 151 unbound keydown/typed,  |
| agent | intl_wide_paste | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; verified: 10 effect-file byte comparison(s) against the exac |
| agent | flood_scrollback_find | PASS | flood_scrollback_find; app log: 31 find_scroll hop(s), moved=true in 1, last: result=scroll block=561 moved=true viewport_lines=49.40 render |
| agent | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| agent | ui_palette | PASS | palette_first_open: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); palette_reopen: no hop=ui_item line (not |
| agent | ui_readline_keys | PASS | injector accounting: injector sent 215; app saw 215; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 204 unbound keydown/typed,  |
| agent | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| agent | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| agent | intl_ime_japanese | SKIP | macOS input methods: needs the mac backend (this is wayland). NEEDS: on Linux an input-method daemon (ibus or fcitx5 with the Japanese / Pin |
| agent | intl_ime_pinyin | SKIP | macOS input methods: needs the mac backend (this is wayland). NEEDS: on Linux an input-method daemon (ibus or fcitx5 with the Japanese / Pin |
| agent | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines wer |
| agent | ime_compose | NOREF | injector accounting: injector sent 335; app saw 317; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 306 unbound keydown/typed, |
| agent | intl_deadkey_option | SKIP | macOS input methods: needs the mac backend (this is wayland). NEEDS: on Linux an input-method daemon (ibus or fcitx5 with the Japanese / Pin |
| agent | ui_mouse_select_copy | PASS | injector accounting: injector sent 200; app saw 200; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| agent | primary_and_middle_click | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 219 unbound keydown/typed,  |
| agent | link_and_path_click | PASS | the path went to the desktop opener (system): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expect |
| agent | wheel_scroll | PASS | injector accounting: injector sent 252; app saw 252; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 240 unbound keydown/typed,  |
| agent | resize_reflow_output | PASS | injector accounting: injector sent 590; app saw 590; lost-before-app 0 (Enter: sent 15, app saw 15); of the app's: 542 unbound keydown/typed |
| agent | editor_daily | PASS | replace: opener exit code 0; close: opener exit code 0; close_enter: opener exit code 0; external: opener exit code 0; injector accounting:  |
| agent | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| agent | pane_resize_maximize | PASS | injector accounting: injector sent 778; app saw 778; lost-before-app 0 (Enter: sent 19, app saw 19); of the app's: 745 unbound keydown/typed |
| agent | minimize_fullscreen | PASS | injector accounting: injector sent 890; app saw 890; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 855 unbound keydown/typed |
| agent | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| agent | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| agent | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent | ui_vertical_tabs_panel_click | NOREF | the app log says agent but the screen reads as terminal (OCR); the log decides; vertical tabs: on in the profile's settings (the owner's lay |
| agent | tab_pointer_use | NOREF | the app log says agent but the screen reads as terminal (OCR); the log decides; NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles |
| agent | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| agent | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 17 frame(s) drawn after the rebuild; 6 frame(s) |
| agent | tabs_two_windows | PASS | injector accounting: injector sent 2569; app saw 2569; lost-before-app 0 (Enter: sent 29, app saw 29); of the app's: 2523 unbound keydown/ty |
| agent | tabs_streaming | PASS | injector accounting: injector sent 1912; app saw 1912; lost-before-app 0 (Enter: sent 22, app saw 22); of the app's: 1874 unbound keydown/ty |
| agent | meta_and_word_keys | PASS | injector accounting: injector sent 761; app saw 761; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 617 unbound keydown/typed |
| agent | pane_not_local_after_meta_and_word_keys | FAIL | after meta_and_word_keys the pane's shell was not the root shell (other tab or pane: the shell in front (pid 1304115) was not started from t |
| agent@fake | agent_suggestion_chip_keeps_output | PASS | the suggestion going is not judged in the agent view: Escape leaves the view there, a layout change of its own (the Rust view test a_late_pr |
| agent@fake | desktop_notification_agent_question | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; injector accounting: injector sent 39; app saw 39; lost-befo |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | FAIL | terminal mode expected but the screen shows agent markers ('New agent conversation' / 'ESC for terminal' / 'Ask anything'); the app log says |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | injector accounting: injector sent 225; app saw 225; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 220 unbound keydown/typed,  |
| terminal | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it wi |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | injector accounting: injector sent 2278; app saw 2278; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2216 unbound keydown/ty |
| terminal | rollover | PASS | injector accounting: injector sent 782; app saw 782; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 760 unbound keydown/typed |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal | arrows_behind_paste | PASS | injector accounting: injector sent 132; app saw 132; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 110 unbound keydown/typed,  |
| terminal | keydown_count | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 620 unbound keydown/typed |
| terminal | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| terminal | tab_enter | PASS | injector accounting: injector sent 686; app saw 686; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 660 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 5.56, -> LineEditorReading 27.67; app: |
| terminal | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| terminal | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| terminal | gitbash_subshell | PASS | injector accounting: injector sent 1163; app saw 1163; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1152 unbound keydown/ty |
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
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.05 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal | latency_enter | FAIL | harness exception: RuntimeError("foreground window is 'phosphor-oss.exe' (pid 12068), not the reference console: refusing to send keys"); ap |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| terminal | routing_table_powershell | PASS | injector accounting: injector sent 861; app saw 861; lost-before-app 0 (Enter: sent 31, app saw 31); of the app's: 752 unbound keydown/typed |
| terminal | entered_agent_view_in_routing_table_powershell | FAIL | after the TERMINAL case routing_table_powershell the pane shows AGENT view: something the case typed was sent to the agent instead of the sh |
| terminal | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| terminal | intl_wide_type | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; typed cjk: rec.log empty (hook not run): the command line |
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
| terminal | history_up_recall | FAIL | hu.txt holds ['t247_l2j3g8h8', 't248_uu59vu93', 't249_u5z8nkp8', 't248_uu59vu93', 't248_uu59vu9e9z'], expected ['t247_l2j3g8h8', 't248_uu59v |
| terminal | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal | scroll_keys_position | SKIP | not run in the PowerShell pane: r23 (#2060): bash `cat` of numbered lines, a bash background job, rec.log; the scroll position is read by Wi |
| terminal | ime_compose | SKIP | not run in the PowerShell pane: r15 T39: the committed text is checked as the UTF-8 bytes of a file written by `echo ... >> file` (Windows P |
| terminal | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal | scroll_keys_foreground | SKIP | not run in the PowerShell pane: #2080: the foreground program is a POSIX raw reader started from a bash script (bytes_reader.sh), with a bas |
| terminal | intl_altgr_ps | PASS | injector accounting: injector sent 170; app saw 170; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 167 unbound keydown/typed,  |
| terminal | intl_altgr_bash | NOREF | injector accounting: injector sent 345; app saw 341; lost-before-app 4 (Enter: sent 6, app saw 6); of the app's: 337 unbound keydown/typed,  |
| terminal | ui_mouse_select_copy | SKIP | not run in the PowerShell pane: T31: the token is printed by bash `printf` and the command is checked against rec.log: runs in the Git Bash  |
| terminal | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| terminal | ui_copy_wrapped_drag | SKIP | not run in the PowerShell pane: T31 (#1976): `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in |
| terminal | ui_right_click_paste | SKIP | not run on Windows: not ported: it falls back to OCR for the menu entry and has never passed anywhere (r14-test-gaps F13); on Windows the sa |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| terminal | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal | resize_reflow_output | SKIP | not run in the PowerShell pane: T33 (r14 T26): a bash SIGWINCH trap, `stty size` and vim: runs in the Git Bash pane (window size by SetWindo |
| terminal | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal | suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: the fixture is printed with `cat` and its command is confirmed from the bash prompt hook; the chip's  |
| terminal | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| terminal | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| terminal | md_viewer | FAIL | mermaid flowchart node labels not visible (blank / failed diagram block?): missing ['gammanodeend']; OCR saw 'qsearchtabs...add-contentzugse |
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
| terminal | soak | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; soak: window medians [483, 489, 495, 501, 511, 515, 522, 526 |
| terminal | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 34.5 frames/s for 31.2 wakeups/s (0 Full); 4 hidden tabs streaming 2.2 frames/s, 124.2 hidden  |
| terminal | tabs_two_windows | PASS | injector accounting: injector sent 2475; app saw 2475; lost-before-app 0 (Enter: sent 28, app saw 28); of the app's: 2430 unbound keydown/ty |
| terminal | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 1953; app saw 1953; lost-before-app 0 (Enter: sent 22, app saw 22); of the app's: 1914 unbound keydown/ty |
| terminal | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal | meta_and_word_keys | PASS | raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it); injector account |
| terminal@gitbash | mode_proof | FAIL | terminal mode expected but the screen shows agent markers ('New agent conversation' / 'ESC for terminal' / 'Ask anything'); the app log says |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 223; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 218 unbound keydown/typed,  |
| terminal@gitbash | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | injector accounting: injector sent 2061; app saw 2061; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1999 unbound keydown/ty |
| terminal@gitbash | rollover | PASS | injector accounting: injector sent 702; app saw 702; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 680 unbound keydown/typed |
| terminal@gitbash | expert_fuzz | PASS | injector accounting: injector sent 743; app saw 743; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 658 unbound keydown/typed |
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
| terminal@gitbash | subshell_ready_rc_clear | PASS | clear: setup armed decision=Wait(Unknown); no setup progress event logged; clear: `exit` NOT typed: the probe ran in the local shell and the |
| terminal@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| terminal@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal@gitbash | ssh_connect_typing_wrapper | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 276; app saw 276; lost-before-app 0 (Enter: sent 10, app saw 10); of the app |
| terminal@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.23 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 47.6 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | FAIL | REGRESSION key to frame: median 4.35 vs baseline build +0.95 ms (+27.9%); re-measured once: median 4.29 (fail); #1176 key -> pty write media |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal@gitbash | latency_first_key_after_idle | FAIL | REGRESSION First key after 1 s idle: median 4.53 vs baseline build +0.71 ms (+18.6%); re-measured once: median 4.53 (fail); REGRESSION First |
| terminal@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| terminal@gitbash | remote_session_legacy | FAIL | [legacy] no local prompt after `exit` from the remote shell; [legacy] local rec.log lacks the exact command typed after exit; [legacy] secon |
| terminal@gitbash | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| terminal@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| terminal@gitbash | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t322_z' should have completed the REMOTE directory: the remote shell received ['22_z'], want 'cd phxtab_t322_z26r1v/';  |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received [], want the COMPLETED 'cd phxte_t332_4vfdi9/' (the uncompleted 'cd phxte_t332_4' must ne |
| terminal@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| terminal@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| terminal@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| terminal@gitbash | ssh_gitbash_managed | FAIL | managed session: the remote shell received [], want exactly one 'echo T355_4sqcz8'; not provably remote (probe ran in: none), `exit` NOT typ |
| terminal@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.21 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | ssh_gitbash_bootstrap_in_command | PASS | the test sshd logged no client-input count for this session (only the Windows fake sshd does); injector accounting: injector sent 392; app s |
| terminal@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: `exit` did not bring the local prompt back; trial 2: one-connection stages in the app log: [], want auth, auth_done, managed (a fin |
| terminal@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 391; app saw 391; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 379 unbound keydown/typed |
| terminal@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); control: no command row was readable in the  |
| terminal@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| terminal@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 0.0 ms (< 16 ms; before: 4.6 ms); injector accounting: injector sent 742; app saw 742; lost-before |
| terminal@gitbash | ui_completions_as_you_type_local | FAIL | no completion menu with both candidates after typing a shared prefix without Tab; injector accounting: injector sent 16; app saw 16; lost-be |
| terminal@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NO-REFERENCE: En |
| terminal@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal@gitbash | history_up_recall | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 118 unbound keydown/typed,  |
| terminal@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
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
| terminal@gitbash | selection_modes | PASS | injector accounting: injector sent 300; app saw 300; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 275 unbound keydown/typed,  |
| terminal@gitbash | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal@gitbash | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal@gitbash | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal@gitbash | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal@gitbash | resize_reflow_output | NOREF | NO-REFERENCE: UNVERIFIABLE: the resize case was not run: NEEDS: a desktop of at least 1300x800 px on the Windows VM (it is 1280x800): the wi |
| terminal@gitbash | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal@gitbash | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| terminal@gitbash | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8480; app saw 8480; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8249 unbound keydown/ |
| terminal@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal@gitbash | desktop_notification | FAIL | focused: the 12 s command did not finish as typed (dn_focused_t712_xs9k.txt has [], expected ['t711_waer9la8']); no recording notification s |
| terminal@gitbash | pane_wedged_after_desktop_notification | FAIL | the gitbash pane did not run a marker command (no effect file) after desktop_notification, twice: the app is relaunched so the remaining cas |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | md_viewer | FAIL | mermaid flowchart node labels not visible (blank / failed diagram block?): missing ['gammanodeend']; OCR saw 'qsearchtabs...sh-c•echozugsed3 |
| terminal@gitbash | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal@gitbash | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal@gitbash | editor_memory_7mb | PASS | judged memory figures: private, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); baselines taken  |
| terminal@gitbash | restore_shows_history | FAIL | setup: the shell received [] (effect file []), expected exactly the printf line once; injector accounting: injector sent 512; app saw 512; l |
| terminal@gitbash | pane_wedged_after_restore_shows_history | FAIL | the gitbash pane did not run a marker command (no effect file) after restore_shows_history, twice: the app is relaunched so the remaining ca |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| terminal@gitbash | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows located from the app's find hook (the newest sighting in each t |
| terminal@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal@gitbash | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal@gitbash | keybinding_rebind | FAIL | arm B: ctrl+shift+F9 did not open command search (no frame rendered CommandSearchView within 8 s) although keybindings.yaml binds it; the ap |
| terminal@gitbash | soak | NOREF | soak: window medians [452, 465, 471, 483, 484, 490, 495, 500, 514, 514] MB, monotonic=True, growth +63 MB (limit 150 MB); injector accountin |
| terminal@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 14 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams we |
| terminal@gitbash | tabs_two_windows | PASS | KNOWN: typing/paste in window 2 under 12 streaming tabs: 3/16 exact lines (15 got, 1 expected line(s) missing), no key discarded in the app  |
| terminal@gitbash | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal@gitbash | history_survives_restart | FAIL | quit round, c3 prefix + Up + Enter (the newest such line): the shell received nothing (the recalled line was not found, or not run); quit ro |
| terminal@gitbash | tabs_streaming | PASS | KNOWN: typing under 12 streaming tabs: 2/10 exact lines (9 got, 1 expected line(s) missing), no key discarded in the app log [ACCEPTED_FOR_R |
| terminal@gitbash | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| terminal@pwsh7 | startup_shell_pass | FAIL | the app did not start pwsh.exe for this pass: none of pwsh.exe runs under it (processes under the app: openconsole.exe). No case of this pas |
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
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 6.89, -> LineEditorReading 36.2; app:  |
| agent | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| agent | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| agent | gitbash_subshell | PASS | injector accounting: injector sent 1163; app saw 1163; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1152 unbound keydown/ty |
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
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.02 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 4.04 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| agent | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached th |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
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
| agent | history_up_recall | FAIL | hu.txt holds ['t190_uvs7ic2x', 't191_cbu0k9c7', 't192_1lmohi6h', 't191_cbu0k9c7', 't191_cbu0k9c79z'], expected ['t190_uvs7ic2x', 't191_cbu0k |
| agent | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent | scroll_keys_position | SKIP | not run in the PowerShell pane: r23 (#2060): bash `cat` of numbered lines, a bash background job, rec.log; the scroll position is read by Wi |
| agent | ime_compose | SKIP | not run in the PowerShell pane: r15 T39: the committed text is checked as the UTF-8 bytes of a file written by `echo ... >> file` (Windows P |
| agent | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent | scroll_keys_foreground | SKIP | not run in the PowerShell pane: #2080: the foreground program is a POSIX raw reader started from a bash script (bytes_reader.sh), with a bas |
| agent | intl_altgr_ps | PASS | injector accounting: injector sent 170; app saw 170; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 167 unbound keydown/typed,  |
| agent | intl_altgr_bash | NOREF | injector accounting: injector sent 345; app saw 341; lost-before-app 4 (Enter: sent 6, app saw 6); of the app's: 337 unbound keydown/typed,  |
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
| agent | md_viewer | FAIL | mermaid flowchart node labels not visible (blank / failed diagram block?): missing ['gammanodeend']; OCR saw 'windqsearchtabs...newagentconv |
| agent | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| agent | restore_shows_history | SKIP | not run in the PowerShell pane: r14 T20: the token is printed by bash `printf '%s%s\n' A B` so the command text never holds it, and rec.log  |
| agent | ui_vertical_tabs_many | SKIP | not run in the PowerShell pane: T33 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| agent | ui_vertical_tabs_panel_click | SKIP | not run in the PowerShell pane: T31 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| agent | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| agent | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent | keybinding_rebind | NOREF | NO-REFERENCE: UNVERIFIABLE: this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is ju |
| agent | soak | NOREF | soak: window medians [520, 529, 536, 543, 553, 558, 563, 569, 574, 586] MB, monotonic=True, growth +66 MB (limit 150 MB); injector accountin |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 35.2 frames/s for 31.3 wakeups/s (0 Full); 4 hidden tabs streaming 2.5 frames/s, 125.0 hidden  |
| agent | tabs_two_windows | PASS | injector accounting: injector sent 2474; app saw 2474; lost-before-app 0 (Enter: sent 28, app saw 28); of the app's: 2430 unbound keydown/ty |
| agent | history_survives_restart | PASS | mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown; no quit key on thi |
| agent | tabs_streaming | PASS | injector accounting: injector sent 1942; app saw 1942; lost-before-app 0 (Enter: sent 22, app saw 22); of the app's: 1904 unbound keydown/ty |
| agent | meta_and_word_keys | PASS | raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it); injector account |
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
| agent@gitbash | big_paste | NOREF | big_paste: time-to-Enter not measured (big.out never reached its full 1000001 bytes within the wait: the byte check below says why); NO-REFE |
| agent@gitbash | tab_enter | PASS | injector accounting: injector sent 479; app saw 479; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 453 unbound keydown/typed |
| agent@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 73 unbound keydown/typed, 2 t |
| agent@gitbash | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | alt_screen | PASS | injector accounting: injector sent 27; app saw 27; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 24 unbound keydown/typed, 2 t |
| agent@gitbash | gitbash_subshell | SKIP | the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells; injector accounting: injector sent 2; app saw 2; los |
| agent@gitbash | alt_screen_programs | NOREF | NO-REFERENCE: UNVERIFIABLE: htop is not installed (package htop): that part of the alt-screen coverage did not run; NO-REFERENCE: UNVERIFIAB |
| agent@gitbash | subshell_ready_rc_questions | PASS | rcq: `exit` NOT typed: the probe ran in the local shell and the shell count under the app is unknown (base None, now None); no root pid reco |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | FAIL | the first inner line ran 4.78 s after its Enter (budget 2 s); clear: setup armed decision=Write(None); InitSubshell@1373 ms, InitShell@3372  |
| agent@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| agent@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent@gitbash | ssh_connect_typing_wrapper | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 276; app saw 276; lost-before-app 0 (Enter: sent 10, app saw 10); of the app |
| agent@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.26 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent@gitbash | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 48.5 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| agent@gitbash | latency_key_log | PASS | #1176 key -> pty write median 0.77 ms (p95 0.99, n=60): within the 1 ms target; #1995 notifies logged before 6 of 78 key frame build(s): Inp |
| agent@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent@gitbash | latency_first_key_after_idle | FAIL | REGRESSION First key after 1 s idle: median 4.07 vs baseline build +0.57 ms (+16.3%); re-measured once: median 4.42 (fail); REGRESSION First |
| agent@gitbash | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| agent@gitbash | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent@gitbash | cb_agent_pickers | FAIL | at picker (@): the picker did not appear: the screen changed by 0.0006 (< 0.0040); injector accounting: injector sent 77; app saw 77; lost-b |
| agent@gitbash | remote_session_legacy | FAIL | [legacy] no local prompt after `exit` from the remote shell; [legacy] local rec.log lacks the exact command typed after exit; [legacy] secon |
| agent@gitbash | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| agent@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| agent@gitbash | agent_view_not_restored_after_remote_session_ext | PASS | remote_session_ext relaunched the app itself (pid 560 -> 11688); a relaunched app starts in a terminal session, so this is not an app findin |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t242_y' should have completed the REMOTE directory: the remote shell received [], want 'cd phxtab_t242_y52tty/'; after  |
| agent@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received [], want the COMPLETED 'cd phxte_t253_uslg5l/' (the uncompleted 'cd phxte_t253_u' must ne |
| agent@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| agent@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| agent@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| agent@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| agent@gitbash | ssh_gitbash_managed | FAIL | managed session: the remote shell received ['warp_run_generator_comm-phosphorand 1794485022892235 \'git -C \'"\'"\'/c/gl/b/07xLPX_fG/0/winte |
| agent@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.27 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent@gitbash | ssh_gitbash_bootstrap_in_command | FAIL | managed session, bootstrap in the command: the remote shell received [], want exactly one 'echo T288_g1awf0'; the test sshd logged no client |
| agent@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: `exit` did not bring the local prompt back; trial 2: the line typed during the connect never reached the remote shell. Test host: s |
| agent@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 391; app saw 391; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 379 unbound keydown/typed |
| agent@gitbash | ui_palette | NOREF | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); NO-REFERENCE: UNVERIFIABLE: nonsense filter  |
| agent@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| agent@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 10.0 ms (< 16 ms; before: 11.7 ms); injector accounting: injector sent 742; app saw 742; lost-befo |
| agent@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NO-REFERENCE: En |
| agent@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent@gitbash | history_up_recall | PASS | injector accounting: injector sent 138; app saw 138; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 118 unbound keydown/typed,  |
| agent@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent@gitbash | scroll_keys_position | FAIL | after one typed key the end of the numbered lines is not on screen (read none of 500): the key did not bring the view back to the end of wha |
| agent@gitbash | ime_compose | SKIP | Windows: NEEDS: the language Chinese (Simplified, China) with the Microsoft Pinyin keyboard added for the runner user on the Windows VM (Set |
| agent@gitbash | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent@gitbash | scroll_keys_foreground | PASS | the view's position was not read from the screen (NEEDS: an OCR engine on this runner: tesseract-ocr on Linux and Windows, Apple Vision on m |
| agent@gitbash | intl_altgr_ps | SKIP | PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane); injector accounting: injector sent 2; app s |
| agent@gitbash | intl_altgr_bash | PASS | injector accounting: injector sent 79; app saw 79; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 74 unbound keydown/typed, 2 t |
| agent@gitbash | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| agent@gitbash | ui_copy_wrapped_output | PASS | injector accounting: injector sent 238; app saw 238; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 222 unbound keydown/typed,  |
| agent@gitbash | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 223 unbound keydown/typed,  |
| agent@gitbash | selection_modes | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops |
| agent@gitbash | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| agent@gitbash | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| agent@gitbash | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| agent@gitbash | resize_reflow_output | NOREF | NO-REFERENCE: UNVERIFIABLE: the resize case was not run: NEEDS: a desktop of at least 1300x800 px on the Windows VM (it is 1280x800): the wi |
| agent@gitbash | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| agent@gitbash | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| agent@gitbash | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| agent@gitbash | gitbash_typeahead_reps | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+= |
| agent@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| agent@gitbash | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| agent@gitbash | md_viewer | FAIL | mermaid flowchart node labels not visible (blank / failed diagram block?): missing ['gammanodeend']; OCR saw 'windqsearchtabs...newagentconv |
| agent@gitbash | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| agent@gitbash | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| agent@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| agent@gitbash | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: UNVERIFIABLE: the panel rows of tabs 2..4 could not be locat |
| agent@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| agent@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent@gitbash | keybinding_rebind | FAIL | arm B: ctrl+shift+F9 did not open command search (no frame rendered CommandSearchView within 8 s) although keybindings.yaml binds it; the ap |
| agent@gitbash | soak | SKIP | not run: job time budget (39 min left, 75 min needed for this case plus report/teardown/upload) |
| agent@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 11 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams we |
| agent@gitbash | tabs_two_windows | PASS | KNOWN: typing/paste in window 2 under 12 streaming tabs: 4/16 exact lines (15 got, 1 expected line(s) missing), no key discarded in the app  |
| agent@gitbash | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| agent@gitbash | tabs_streaming | PASS | KNOWN: typing under 12 streaming tabs: 2/10 exact lines (2 got, 8 expected line(s) missing), no key discarded in the app log [ACCEPTED_FOR_R |
| agent@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| agent@pwsh7 | startup_shell_pass | FAIL | the app did not start pwsh.exe for this pass: none of pwsh.exe runs under it (processes under the app: openconsole.exe). No case of this pas |
| agent@wsl | startup_shell_pass | SKIP | a WSL distribution is not installed on this machine: the wsl pass cannot run (the harness installs nothing). NEEDS: the Windows Subsystem fo |
| agent@fake | agent_suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: as suggestion_chip_keeps_output (a `cat` confirmed from the bash prompt hook): runs once, in the Git  |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | desktop_notification_agent_question | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; injector accounting: inje |
| agent@fake | agent_close_undo_call_status | SKIP | not run in the PowerShell pane: #2103: the scripted command is a POSIX printf whose output is not in its command line; the agent path does n |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_suggestion_chip_keeps_output | PASS | the suggestion going is not judged in the agent view: Escape leaves the view there, a layout change of its own (the Rust view test a_late_pr |
| agent@gitbash+fake | desktop_notification_agent_question | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; no recording notification service on this platform: the verd |
| agent@gitbash+fake | agent_close_undo_call_status | PASS | the close key asked first (a waiting agent turn): answered with Enter; injector accounting: injector sent 307; app saw 307; lost-before-app  |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_lrc_no_false_takeover | windows | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent@gitbash | PASS |
| alt_screen_programs | windows | agent | SKIP |
| alt_screen_programs | windows | agent@gitbash | NOREF |
| alt_screen_programs | windows | terminal | SKIP |
| alt_screen_programs | windows | terminal@gitbash | NOREF |
| big_paste | macos | agent | PASS |
| big_paste | macos | terminal | PASS |
| big_paste | wayland | agent | PASS |
| big_paste | wayland | terminal | PASS |
| big_paste | windows | agent | SKIP |
| big_paste | windows | agent@gitbash | NOREF |
| big_paste | windows | terminal | SKIP |
| big_paste | windows | terminal@gitbash | NOREF |
| editor_daily | macos | agent | NOREF |
| editor_daily | macos | terminal | NOREF |
| editor_daily | wayland | agent | PASS |
| editor_daily | wayland | terminal | PASS |
| editor_daily | windows | agent | PASS |
| editor_daily | windows | agent@gitbash | PASS |
| editor_daily | windows | terminal | PASS |
| editor_daily | windows | terminal@gitbash | PASS |
| editor_memory_7mb | macos | terminal | FAIL |
| editor_memory_7mb | wayland | terminal | PASS |
| editor_memory_7mb | windows | terminal | PASS |
| editor_memory_7mb | windows | terminal@gitbash | PASS |
| flood_scrollback_find | wayland | agent | PASS |
| flood_scrollback_find | wayland | terminal | PASS |
| flood_scrollback_find | windows | agent | SKIP |
| flood_scrollback_find | windows | agent@gitbash | SKIP |
| flood_scrollback_find | windows | terminal | SKIP |
| flood_scrollback_find | windows | terminal@gitbash | SKIP |
| focus_cycle | windows | agent | PASS |
| focus_cycle | windows | agent@gitbash | PASS |
| focus_cycle | windows | terminal | PASS |
| focus_cycle | windows | terminal@gitbash | PASS |
| gitbash_typeahead_reps | windows | agent | SKIP |
| gitbash_typeahead_reps | windows | agent@gitbash | PASS |
| gitbash_typeahead_reps | windows | terminal | SKIP |
| gitbash_typeahead_reps | windows | terminal@gitbash | PASS |
| gpu_real_render | wayland | agent | NOREF |
| gpu_real_render | wayland | terminal | NOREF |
| history_survives_restart | windows | agent | PASS |
| history_survives_restart | windows | agent@gitbash | PASS |
| history_survives_restart | windows | terminal | PASS |
| history_survives_restart | windows | terminal@gitbash | FAIL |
| history_up_recall | windows | agent | FAIL |
| history_up_recall | windows | agent@gitbash | PASS |
| history_up_recall | windows | terminal | FAIL |
| history_up_recall | windows | terminal@gitbash | PASS |
| hostile_output_recovers | windows | agent | SKIP |
| hostile_output_recovers | windows | agent@gitbash | PASS |
| hostile_output_recovers | windows | terminal | SKIP |
| hostile_output_recovers | windows | terminal@gitbash | PASS |
| job_control | windows | agent | SKIP |
| job_control | windows | agent@gitbash | SKIP |
| job_control | windows | terminal | SKIP |
| job_control | windows | terminal@gitbash | SKIP |
| keybinding_rebind | windows | agent | NOREF |
| keybinding_rebind | windows | agent@gitbash | FAIL |
| keybinding_rebind | windows | terminal | NOREF |
| keybinding_rebind | windows | terminal@gitbash | FAIL |
| link_and_path_click | macos | agent | FAIL |
| link_and_path_click | macos | terminal | FAIL |
| link_and_path_click | wayland | agent | PASS |
| link_and_path_click | wayland | terminal | PASS |
| link_and_path_click | windows | agent | SKIP |
| link_and_path_click | windows | agent@gitbash | SKIP |
| link_and_path_click | windows | terminal | SKIP |
| link_and_path_click | windows | terminal@gitbash | SKIP |
| mac_pointer_chord_proof | macos | agent | PASS |
| mac_pointer_chord_proof | macos | terminal | PASS |
| mac_pointer_chord_proof | wayland | agent | SKIP |
| mac_pointer_chord_proof | wayland | terminal | SKIP |
| mac_pointer_chord_proof | windows | agent | SKIP |
| mac_pointer_chord_proof | windows | agent@gitbash | SKIP |
| mac_pointer_chord_proof | windows | terminal | SKIP |
| mac_pointer_chord_proof | windows | terminal@gitbash | SKIP |
| meta_and_word_keys | wayland | agent | PASS |
| meta_and_word_keys | wayland | terminal | PASS |
| meta_and_word_keys | windows | agent | PASS |
| meta_and_word_keys | windows | agent@gitbash | PASS |
| meta_and_word_keys | windows | terminal | PASS |
| meta_and_word_keys | windows | terminal@gitbash | PASS |
| minimize_fullscreen | macos | agent | PASS |
| minimize_fullscreen | macos | terminal | PASS |
| minimize_fullscreen | wayland | agent | PASS |
| minimize_fullscreen | wayland | terminal | PASS |
| minimize_fullscreen | windows | agent | SKIP |
| minimize_fullscreen | windows | agent@gitbash | SKIP |
| minimize_fullscreen | windows | terminal | SKIP |
| minimize_fullscreen | windows | terminal@gitbash | SKIP |
| network_gate_switch | macos | terminal | NOREF |
| network_gate_switch | wayland | terminal | NOREF |
| network_gate_switch | windows | terminal | SKIP |
| network_gate_switch | windows | terminal@gitbash | NOREF |
| output_frames | windows | agent | PASS |
| output_frames | windows | agent@gitbash | NOREF |
| output_frames | windows | terminal | PASS |
| output_frames | windows | terminal@gitbash | NOREF |
| pane_resize_maximize | wayland | agent | PASS |
| pane_resize_maximize | wayland | terminal | PASS |
| primary_and_middle_click | macos | agent | SKIP |
| primary_and_middle_click | macos | terminal | SKIP |
| primary_and_middle_click | wayland | agent | PASS |
| primary_and_middle_click | wayland | terminal | PASS |
| primary_and_middle_click | windows | agent | SKIP |
| primary_and_middle_click | windows | agent@gitbash | SKIP |
| primary_and_middle_click | windows | terminal | SKIP |
| primary_and_middle_click | windows | terminal@gitbash | SKIP |
| remote_session_legacy | windows | agent | SKIP |
| remote_session_legacy | windows | agent@gitbash | FAIL |
| remote_session_legacy | windows | terminal | SKIP |
| remote_session_legacy | windows | terminal@gitbash | FAIL |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | FAIL |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| resize_reflow_output | macos | agent | PASS |
| resize_reflow_output | macos | terminal | PASS |
| resize_reflow_output | wayland | agent | PASS |
| resize_reflow_output | wayland | terminal | PASS |
| resize_reflow_output | windows | agent | SKIP |
| resize_reflow_output | windows | agent@gitbash | NOREF |
| resize_reflow_output | windows | terminal | SKIP |
| resize_reflow_output | windows | terminal@gitbash | NOREF |
| restore_shows_history | windows | agent | SKIP |
| restore_shows_history | windows | agent@gitbash | PASS |
| restore_shows_history | windows | terminal | SKIP |
| restore_shows_history | windows | terminal@gitbash | FAIL |
| routing_table_powershell | windows | agent | PASS |
| routing_table_powershell | windows | agent@gitbash | SKIP |
| routing_table_powershell | windows | terminal | PASS |
| routing_table_powershell | windows | terminal@gitbash | SKIP |
| scroll_keys_foreground | windows | agent | SKIP |
| scroll_keys_foreground | windows | agent@gitbash | PASS |
| scroll_keys_foreground | windows | terminal | SKIP |
| scroll_keys_foreground | windows | terminal@gitbash | PASS |
| scroll_keys_position | macos | agent | FAIL |
| scroll_keys_position | macos | terminal | PASS |
| scroll_keys_position | wayland | agent | FAIL |
| scroll_keys_position | wayland | terminal | PASS |
| scroll_keys_position | windows | agent | SKIP |
| scroll_keys_position | windows | agent@gitbash | FAIL |
| scroll_keys_position | windows | terminal | SKIP |
| scroll_keys_position | windows | terminal@gitbash | PASS |
| selection_modes | windows | agent | SKIP |
| selection_modes | windows | agent@gitbash | PASS |
| selection_modes | windows | terminal | SKIP |
| selection_modes | windows | terminal@gitbash | PASS |
| soak | windows | agent | NOREF |
| soak | windows | agent@gitbash | SKIP |
| soak | windows | terminal | NOREF |
| soak | windows | terminal@gitbash | NOREF |
| ssh_connect_typing_wrapper | windows | agent | SKIP |
| ssh_connect_typing_wrapper | windows | agent@gitbash | PASS |
| ssh_connect_typing_wrapper | windows | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | PASS |
| ssh_gitbash_bootstrap_in_command | windows | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | agent@gitbash | FAIL |
| ssh_gitbash_bootstrap_in_command | windows | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | terminal@gitbash | PASS |
| ssh_gitbash_failing_not_local_managed | windows | agent | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | agent@gitbash | PASS |
| ssh_gitbash_failing_not_local_managed | windows | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | terminal@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | agent | SKIP |
| ssh_gitbash_kill_switch | windows | agent@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | terminal@gitbash | PASS |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | agent | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal@gitbash | FAIL |
| ssh_nested_typeahead_wrapper | windows | agent | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent@gitbash | PASS |
| ssh_nested_typeahead_wrapper | windows | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | PASS |
| subshell_line_editor_proof | macos | agent | PASS |
| subshell_line_editor_proof | macos | terminal | PASS |
| subshell_line_editor_proof | wayland | agent | PASS |
| subshell_line_editor_proof | wayland | terminal | PASS |
| subshell_line_editor_proof | windows | agent | SKIP |
| subshell_line_editor_proof | windows | agent@gitbash | SKIP |
| subshell_line_editor_proof | windows | terminal | SKIP |
| subshell_line_editor_proof | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | macos | agent | SKIP |
| subshell_ready_docker_t_without_i | macos | terminal | SKIP |
| subshell_ready_docker_t_without_i | wayland | agent | SKIP |
| subshell_ready_docker_t_without_i | wayland | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | macos | agent | PASS |
| subshell_ready_exec_fish_rc | macos | terminal | PASS |
| subshell_ready_exec_fish_rc | wayland | agent | PASS |
| subshell_ready_exec_fish_rc | wayland | terminal | PASS |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | agent@gitbash | FAIL |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | PASS |
| subshell_ready_rc_questions | macos | agent | PASS |
| subshell_ready_rc_questions | macos | terminal | PASS |
| subshell_ready_rc_questions | wayland | agent | PASS |
| subshell_ready_rc_questions | wayland | terminal | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | agent@gitbash | PASS |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | PASS |
| subshell_ready_relay_shape | macos | agent | SKIP |
| subshell_ready_relay_shape | macos | terminal | SKIP |
| subshell_ready_relay_shape | wayland | agent | PASS |
| subshell_ready_relay_shape | wayland | terminal | PASS |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_probe | macos | agent | PASS |
| subshell_ready_secret_after_probe | macos | terminal | PASS |
| subshell_ready_secret_after_probe | wayland | agent | PASS |
| subshell_ready_secret_after_probe | wayland | terminal | PASS |
| subshell_ready_secret_after_probe | windows | agent | SKIP |
| subshell_ready_secret_after_probe | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | terminal | SKIP |
| subshell_ready_secret_after_probe | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | macos | agent | PASS |
| subshell_ready_secret_after_prompt | macos | terminal | PASS |
| subshell_ready_secret_after_prompt | wayland | agent | PASS |
| subshell_ready_secret_after_prompt | wayland | terminal | PASS |
| subshell_ready_secret_after_prompt | windows | agent | SKIP |
| subshell_ready_secret_after_prompt | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | macos | agent | PASS |
| subshell_ready_unsupported_tcsh | macos | terminal | PASS |
| subshell_ready_unsupported_tcsh | wayland | agent | SKIP |
| subshell_ready_unsupported_tcsh | wayland | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | macos | agent | PASS |
| subshell_ready_vim_typeahead | macos | terminal | PASS |
| subshell_ready_vim_typeahead | wayland | agent | PASS |
| subshell_ready_vim_typeahead | wayland | terminal | PASS |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | macos | agent | SKIP |
| subshell_ready_zsh_newuser_install | macos | terminal | SKIP |
| subshell_ready_zsh_newuser_install | wayland | agent | PASS |
| subshell_ready_zsh_newuser_install | wayland | terminal | PASS |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | macos | agent | PASS |
| subshell_sh_probe | macos | terminal | PASS |
| subshell_sh_probe | wayland | agent | PASS |
| subshell_sh_probe | wayland | terminal | PASS |
| subshell_sh_probe | windows | agent | SKIP |
| subshell_sh_probe | windows | agent@gitbash | SKIP |
| subshell_sh_probe | windows | terminal | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | macos | agent | PASS |
| subshell_sh_probe_empty_ps1 | macos | terminal | PASS |
| subshell_sh_probe_empty_ps1 | wayland | agent | PASS |
| subshell_sh_probe_empty_ps1 | wayland | terminal | PASS |
| subshell_sh_probe_empty_ps1 | windows | agent | SKIP |
| subshell_sh_probe_empty_ps1 | windows | agent@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal@gitbash | SKIP |
| tab_lifecycle_keys | wayland | agent | PASS |
| tab_lifecycle_keys | wayland | terminal | PASS |
| tab_lifecycle_keys | windows | agent | PASS |
| tab_lifecycle_keys | windows | agent@gitbash | PASS |
| tab_lifecycle_keys | windows | terminal | PASS |
| tab_lifecycle_keys | windows | terminal@gitbash | PASS |
| tab_pointer_use | macos | agent | NOREF |
| tab_pointer_use | macos | terminal | NOREF |
| tab_pointer_use | wayland | agent | NOREF |
| tab_pointer_use | wayland | terminal | NOREF |
| tab_pointer_use | windows | agent | SKIP |
| tab_pointer_use | windows | agent@gitbash | SKIP |
| tab_pointer_use | windows | terminal | SKIP |
| tab_pointer_use | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | FAIL |
| ui_copy_wrapped_drag | windows | agent | SKIP |
| ui_copy_wrapped_drag | windows | agent@gitbash | PASS |
| ui_copy_wrapped_drag | windows | terminal | SKIP |
| ui_copy_wrapped_drag | windows | terminal@gitbash | PASS |
| ui_copy_wrapped_output | windows | agent | SKIP |
| ui_copy_wrapped_output | windows | agent@gitbash | PASS |
| ui_copy_wrapped_output | windows | terminal | SKIP |
| ui_copy_wrapped_output | windows | terminal@gitbash | PASS |
| ui_readline_keys | wayland | agent | PASS |
| ui_readline_keys | wayland | terminal | PASS |
| ui_vertical_tabs_many | windows | agent | SKIP |
| ui_vertical_tabs_many | windows | agent@gitbash | PASS |
| ui_vertical_tabs_many | windows | terminal | SKIP |
| ui_vertical_tabs_many | windows | terminal@gitbash | PASS |
| ui_vertical_tabs_panel_click | wayland | agent | NOREF |
| ui_vertical_tabs_panel_click | wayland | terminal | PASS |
| ui_vertical_tabs_panel_click | windows | agent | SKIP |
| ui_vertical_tabs_panel_click | windows | agent@gitbash | NOREF |
| ui_vertical_tabs_panel_click | windows | terminal | SKIP |
| ui_vertical_tabs_panel_click | windows | terminal@gitbash | PASS |
| wheel_scroll | macos | agent | FAIL |
| wheel_scroll | macos | terminal | FAIL |
| wheel_scroll | wayland | agent | PASS |
| wheel_scroll | wayland | terminal | PASS |
| wheel_scroll | windows | agent | SKIP |
| wheel_scroll | windows | agent@gitbash | SKIP |
| wheel_scroll | windows | terminal | SKIP |
| wheel_scroll | windows | terminal@gitbash | SKIP |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | PASS |

passed everywhere it ran (6, candidates for the owner's decision): agent_lrc_no_false_takeover, focus_cycle, meta_and_word_keys, pane_resize_maximize, tab_lifecycle_keys, ui_readline_keys
not passed somewhere (fail / skip / no-reference): alt_screen_programs, big_paste, editor_daily, editor_memory_7mb, flood_scrollback_find, gitbash_typeahead_reps, gpu_real_render, history_survives_restart, history_up_recall, hostile_output_recovers, job_control, keybinding_rebind, link_and_path_click, mac_pointer_chord_proof, minimize_fullscreen, network_gate_switch, output_frames, primary_and_middle_click, remote_session_legacy, remote_tab_enter_no_helper, remote_tab_no_helper, resize_reflow_output, restore_shows_history, routing_table_powershell, scroll_keys_foreground, scroll_keys_position, selection_modes, soak, ssh_connect_typing_wrapper, ssh_gitbash_bootstrap_in_command, ssh_gitbash_failing_not_local_managed, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, subshell_line_editor_proof, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, subshell_sh_probe_empty_ps1, tab_pointer_use, ui_completions_as_you_type_local, ui_copy_wrapped_drag, ui_copy_wrapped_output, ui_vertical_tabs_many, ui_vertical_tabs_panel_click, wheel_scroll, zero_network_startup
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run_slim/logs/agent/acct/launch0_1794477917.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794477917.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run/junit.xml: 12331 KB skipped (over 1 MB)
- note: macos/run/results.json: 13967 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.11.log.gz: 4899 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.2.log.gz: 4925 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794469985.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794469985.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794469985.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794469985.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794469985.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 4632 KB skipped (over 1 MB)
- note: wayland/run/results.jsonl: 4838 KB skipped (over 1 MB)
- note: wayland/run/results.json: 4928 KB skipped (over 1 MB)
- note: wayland/run/junit.xml: 4254 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.2.log.gz: 2042 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.6.log.gz: 3009 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794466246.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794466246.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794466246.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794466246.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794466246.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4580 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 204 KB (1225 of 48732 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_enter.log: latency evidence capped at 204 KB (1358 of 13849 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1213 of 21977 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1246 of 8236 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_key_log.log: latency evidence capped at 204 KB (1214 of 2449 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 204 KB (1223 of 48039 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter.log: latency evidence capped at 204 KB (1349 of 13224 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1214 of 21998 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1245 of 8162 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_key_log.log: latency evidence capped at 204 KB (1220 of 2456 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent/acct/launch0_1794477579.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794477579.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794477579.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794477579.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794481667.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794477579.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794481667.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794481667.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794481667.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794481667.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794484323.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794482224.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794485905.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794485905.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794485905.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794486348.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794461565.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794461565.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794461565.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794461565.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794461565.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794466209.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794466209.log.in_session.1: 10240 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794466209.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794466209.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794466209.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794468968.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794468968.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794467060.log.in_session.0: 10237 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794468968.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794470592.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794470592.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794470592.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794475747.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794475747.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794475747.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794475747.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794475747.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 525 KB (3229 of 4372 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 525 KB (3198 of 4362 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: macos/run_slim/logs/agent/phosphor.1.log.gz: 4452 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.log.gz: 2015 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 1410 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.1.log.gz: 4204 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 407 KB (2711 of 4103 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/case_latency_first_key_after_idle.log: latency evidence capped at 407 KB (2534 of 4393 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 407 KB (2681 of 15097 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/terminal/case_latency_first_key_after_idle.log: latency evidence capped at 407 KB (2529 of 4347 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/acct/launch0_1794462407.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794462407.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794462407.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794462407.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794462407.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794463040.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 1281 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794459092.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794459092.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794459092.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794459092.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794459092.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: run dir is 8.6 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::off [test-windows]`
  - `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing [test-linux]`
  - `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing [test-macos]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on [test-windows]`
  - `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing [test-windows]`
  - `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::truncate_chars_cuts_by_characters_not_bytes [test-windows]`
  - `remote_server shellcmd::git::write::tests::discard_validates_then_restores_and_removes [test-workspace-rest-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_remote_dir.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/pane_not_local_after_lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_no_helper_chip.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_no_helper_chip.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_nohelper_tab_hands_back.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_nohelper_tab_hands_back.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_file_tree_remote.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/entered_agent_view_in_lab_file_tree_remote.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_codebase_index_remote.fish.lan.nohelper (case not in previous run)`
  - ... and 97 more
- FIXED failures (app): 
  - `lab-wayland/agent/lab_agent_remote_drop.nu.lan.nohelper`
  - `lab-wayland/agent/lab_fam_agent_edit_conflict.nu.lan.nohelper`
  - `windows/terminal/fast_typing`
  - `windows/terminal@gitbash/ssh_connect_typing`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
  - `windows/agent@gitbash/ssh_connect_typing`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/ssh_nested_typeahead`
- Latency regressions > 5%: 
  - `windows/terminal@gitbash/ctrl_c_d ctrl_c: 71.58 -> 83.61 ms (+16.8%)`
  - `windows/terminal@gitbash/latency_enter enter_to_start: 93.61 -> 121.93 ms (+30.3%)`
  - `windows/terminal@gitbash/latency_key_log key_to_frame: 3.70 -> 4.35 ms (+17.6%)`
- compared against: test-linux: build/r15a (3054716634); test-macos: build/r15a (3054716634); test-warpui-core: build/r15a (3054716634); test-warpui-core-macos: build/r15a (3054716634); test-warpui-core-windows: build/r15a (3054716634); test-windows: build/r15a (3054716634); test-workspace-rest: build/r15a (3054716634); test-workspace-rest-macos: build/r15a (3054716634); test-workspace-rest-windows: build/r15a (3054716634); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998); app windows: build/r13a (3051722998)
