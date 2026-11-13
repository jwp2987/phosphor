# build/r30b  ca9a20723  (pipeline 3065705355)

- date: 2026-11-13, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.*`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_managed_connect.*`, `lab_msys_ssh_switch.*`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_remote_session.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.*`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on linux-wayland: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `drag_drop_file`, `editor_memory_7mb`, `gpu_lost_device_recovers`, `gpu_real_render`, `history_survives_restart`, `history_up_recall`, `ime_compose`, `intl_compose_linux`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `network_gate_switch`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_plain_installed_head`, `routing_table`, `routing_table_powershell`, `routing_typo_head`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`
  - asked for on macos: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `editor_daily`, `editor_memory_7mb`, `history_survives_restart`, `history_up_recall`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `network_gate_switch`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_plain_installed_head`, `routing_table`, `routing_table_powershell`, `routing_typo_head`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `tab_pointer_use`, `ui_completions_as_you_type_local`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.*`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_freeze_and_wake.bash.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_managed_connect.*`, `lab_msys_ssh_switch.*`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_remote_session.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.*`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on windows: `agent_close_undo_call_status`, `agent_latency_enter`, `agent_latency_key`, `agent_lrc_no_false_takeover`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `alt_screen_programs`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `cb_agent_pickers`, `cb_ui_action_latency`, `ctrl_c_d`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `editor_memory_7mb`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `flood_scrollback_find`, `focus_cycle`, `gitbash_subshell`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `idle_cpu`, `ime_compose`, `intl_altgr_bash`, `intl_altgr_ps`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `job_control`, `key_loss_control`, `keybinding_rebind`, `keydown_count`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `mode_proof`, `network_gate_switch`, `output_frames`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `rollover`, `routing_plain_installed_head`, `routing_table`, `routing_table_powershell`, `routing_typo_head`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_connect_typing`, `ssh_connect_typing_wrapper`, `ssh_drop_reconnect`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead`, `ssh_nested_typeahead_wrapper`, `startup_shell_pane`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `tab_lifecycle_keys`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `wheel_scroll`, `zero_network_startup`
  - asked for on windows-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_connect_typeahead.*`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_managed_connect.*`, `lab_msys_ssh_switch.*`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_remote_session.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.*`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
- unit: 50215 passed, 1 failed, 191 skipped of 50216 run (9 suites)
- app (pass/fail/skip): lab-macos 42/50/2 +124 no-ref; lab-wayland 61/58/2 +157 no-ref; macos 34/1/9 +14 no-ref; wayland 32/0/11 +26 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 157.8 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 163.5 | MacBookPro-gitrunner |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | success | 81.2 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 115.2 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
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
| build-windows-x86_64 | manual | - | - | not run |
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
| guards | success | 8.2 | - |  |
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
| sshlab-image | success | 1.0 | - |  |
| test-linux | success | 22.2 | - |  |
| test-macos | success | 29.2 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.6 | phosphor-windows-vm |  |
| test-windows | failed | 48.2 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.6 | - |  |
| test-workspace-rest-macos | success | 3.5 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.3 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on` LEAK (52.8s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.5s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.6s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.8s)
- `warp terminal::view::tests::a_waiting_question_with_no_card_in_the_pane_is_stopped` FAIL (0.3s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.5s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.1s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.7s)
- `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it` LEAK (1.9s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.bash.lan.nohelper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 386 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.bash.150.nohelper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 384 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a ne |
| terminal | lab_tab_remote_dir.zshc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.zshc.lan.nohelper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 409 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.fish.lan.nohelper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 411 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.fish.150.nohelper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.nu.lan.nohelper/tab_to_insert (a new m |
| terminal | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 569 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.nu.150.nohelper/tab_to_insert (a new |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 6434); the app is relaunched so later case |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.35/5, 1.35/5; side-call producers (#2156): shell ex |
| terminal | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.78/1, 1.78/1; side-call producers (#2156): shell ex |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1489; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper | PASS | injector accounting: injector sent 2361; app saw 2361; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2297 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.150.nohelper | PASS | injector accounting: injector sent 2361; app saw 2361; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2297 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2289 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2289 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.fish.lan.nohelper | PASS | injector accounting: injector sent 2428; app saw 2428; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2359 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.fish.150.nohelper | PASS | injector accounting: injector sent 2428; app saw 2428; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2359 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.13/5, 1.13/5, 4.87/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_git_read.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.32/1, 1.32/1, 5.27/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.87/5, 0.87/5, 4.60/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.39/1, 1.39/1, 5.52/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t624k2m0xi.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t627l3blhc.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t6300s6nde.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t633r4923f.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T662_5g8l6t >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |
| terminal | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T676_eziqcq >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |
| terminal | lab_managed_connect.bash.lan.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| terminal | lab_managed_connect.bash.150.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| terminal | lab_msys_ssh_switch.bash.lan.nohelper | SKIP | Windows Git Bash pane only: the one-connection managed ssh and its switch exist only there (#1933, #2051); injector accounting: injector sen |
| terminal | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| terminal | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t712zcqzzy/phxt712zc |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t720ilm6ys/phxt720il |
| terminal | lab_remote_session.bash.lan.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t744_l0i56o\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| terminal | lab_remote_session.bash.150.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t768_vd2qdd\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 165 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.bash.lan.helper/tab_to_insert (a new m |
| terminal | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 245 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.bash.150.helper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 109 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new me |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 241 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.zshc.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 163 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.zshc.lan.helper/tab_to_insert (a new m |
| terminal | lab_tab_remote_dir.zshc.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 248 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zshc.150.helper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 163 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.fish.lan.helper/tab_to_insert (a new m |
| terminal | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 241 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.fish.150.helper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 107 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.nu.lan.helper/tab_to_insert (a new met |
| terminal | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 243 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.nu.150.helper/tab_to_insert (a new m |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est838zymmi $EPOCHREALTIME >> /work/u_zsh/es_t83736f6.txt', 'echo est84 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.bash.lan.helper | PASS | injector accounting: injector sent 2384; app saw 2384; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2320 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.150.helper | PASS | injector accounting: injector sent 2407; app saw 2407; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2343 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 2399; app saw 2399; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2335 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 2399; app saw 2399; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2335 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.fish.lan.helper | PASS | injector accounting: injector sent 2475; app saw 2475; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2406 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.fish.150.helper | PASS | injector accounting: injector sent 2475; app saw 2475; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2406 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | PASS | injector accounting: injector sent 2889; app saw 2889; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2820 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | PASS | injector accounting: injector sent 2889; app saw 2889; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2820 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt1484p2xedx: app log chip update  |
| terminal | lab_helper_killed.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt1487owh27y: app log chip update  |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt149043jwnd: app log chip update  |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt1493zyzhwy: app log chip update  |
| terminal | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T1522_n5vlkm >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rec |
| terminal | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T1536_rfgv7z >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rec |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 162 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.bash.lan.nohelper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 412 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.bash.150.nohelper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 109 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new met |
| agent | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 410 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a new m |
| agent | lab_tab_remote_dir.zshc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.zshc.lan.nohelper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 406 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.fish.lan.nohelper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 402 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.fish.150.nohelper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 166 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.nu.lan.nohelper/tab_to_insert (a new metr |
| agent | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 572 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.nu.150.nohelper/tab_to_insert (a new me |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 87969); the app is relaunched so later cas |
| agent | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.33/5, 1.33/5; side-call producers (#2156): shell ex |
| agent | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.77/1, 1.77/1; side-call producers (#2156): shell ex |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1489; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_managed_connect.bash.lan.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| agent | lab_managed_connect.bash.150.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| agent | lab_msys_ssh_switch.bash.lan.nohelper | SKIP | Windows Git Bash pane only: the one-connection managed ssh and its switch exist only there (#1933, #2051); injector accounting: injector sen |
| agent | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| agent | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t237x2g1jr/phxt237x2 |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t2456umnop/phxt2456u |
| agent | lab_remote_session.bash.lan.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t269_icz9a6\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| agent | lab_remote_session.bash.150.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t293_k3xwj3\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_freeze_and_wake.bash.lan.nohelper | NOREF | NO-REFERENCE: keys typed while frozen: the line typed while the app was frozen never arrived (the display system did not hold the keys); the |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.bash.lan.helper/tab_to_insert (a new metr |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 99100 -> 10763); a relaunched app starts in a terminal session, so this is |
| agent | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 249 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.bash.150.helper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 164 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new metri |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 245 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a new met |
| agent | lab_tab_remote_dir.zshc.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 163 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.zshc.lan.helper/tab_to_insert (a new metr |
| agent | lab_tab_remote_dir.zshc.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 244 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zshc.150.helper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.fish.lan.helper/tab_to_insert (a new metr |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 245 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.fish.150.helper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 109 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.nu.lan.helper/tab_to_insert (a new metric |
| agent | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 241 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.nu.150.helper/tab_to_insert (a new metr |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est3491wklg $EPOCHREALTIME >> /work/u_zsh/es_t348982f.txt', 'echo est35 |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T558_mjeet0 >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |
| agent | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T572_ntw5sq >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 63 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.bash.lan.nohelper/tab_to_insert (a ne |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 381 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.bash.150.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 63 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 376 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a  |
| terminal | lab_tab_remote_dir.zshc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zshc.lan.nohelper/tab_to_insert (a ne |
| terminal | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 383 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.fish.lan.nohelper/tab_to_insert (a ne |
| terminal | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 379 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.fish.150.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.nu.lan.nohelper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 541 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.nu.150.nohelper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.p_banner.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 63 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_banner.lan.nohelper/tab_to_insert ( |
| terminal | lab_tab_remote_dir.p_banner.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 379 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_banner.150.nohelper/tab_to_inser |
| terminal | lab_tab_remote_dir.p_colon.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 60 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_colon.lan.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.p_colon.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 384 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_colon.150.nohelper/tab_to_insert |
| terminal | lab_tab_remote_dir.p_question.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 64 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_question.lan.nohelper/tab_to_insert |
| terminal | lab_tab_remote_dir.p_question.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 385 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_question.150.nohelper/tab_to_ins |
| terminal | lab_tab_remote_dir.p_slowrc.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/p_slowrc/tabfx_t50wgn1m5/zzqt50wgn1m5_only'], got []; |
| terminal | lab_tab_remote_dir.p_slowrc.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/p_slowrc/tabfx_t54zsbkmu/zzqt54zsbkmu_only'], got []; |
| terminal | lab_tab_remote_dir.p_rcerr.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 63 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_rcerr.lan.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.p_rcerr.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 384 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_rcerr.150.nohelper/tab_to_insert |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 2704139); the app is relaunched so later c |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/5, 1.02/5; side-call producers (#2156): batched  |
| terminal | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.44/1, 1.44/1; side-call producers (#2156): batched  |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper | PASS | injector accounting: injector sent 2361; app saw 2361; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2297 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper.rst | PASS | injector accounting: injector sent 599; app saw 599; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 580 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper.stall10 | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 202 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.bash.150.nohelper | PASS | injector accounting: injector sent 2361; app saw 2361; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2297 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.150.nohelper.rst | PASS | injector accounting: injector sent 599; app saw 599; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 580 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.bash.150.nohelper.stall10 | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 202 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2289 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper.rst | PASS | injector accounting: injector sent 597; app saw 597; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 578 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper.stall10 | PASS | injector accounting: injector sent 210; app saw 210; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 201 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | PASS | injector accounting: injector sent 2353; app saw 2353; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2290 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper.rst | PASS | injector accounting: injector sent 597; app saw 597; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 578 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper.stall10 | PASS | injector accounting: injector sent 210; app saw 210; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 201 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.fish.lan.nohelper | PASS | injector accounting: injector sent 2428; app saw 2428; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2359 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.fish.150.nohelper | PASS | injector accounting: injector sent 2428; app saw 2428; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2363 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/5, 1.02/5, 2.65/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_git_read.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/1, 1.02/1, 3.04/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.81/1, 0.81/1, 2.64/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/1, 1.02/1, 3.06/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 85 recorded lines) |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 85 recorded lines) |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t75005d8p7.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t753gu1lc6.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t756twb3vy.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t75954wk1d.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T788_9dg3rd >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |
| terminal | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T802_dra22l >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |
| terminal | lab_managed_connect.bash.lan.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| terminal | lab_managed_connect.bash.150.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| terminal | lab_msys_ssh_switch.bash.lan.nohelper | SKIP | Windows Git Bash pane only: the one-connection managed ssh and its switch exist only there (#1933, #2051); injector accounting: injector sen |
| terminal | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| terminal | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t838utbq13/phxt838ut |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t8460vja3n/phxt8460v |
| terminal | lab_remote_session.bash.lan.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t870_6kmlfb\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| terminal | lab_remote_session.bash.150.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t894_d0g9c7\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.bash.lan.helper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 205 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.bash.150.helper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new m |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 200 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a ne |
| terminal | lab_tab_remote_dir.zshc.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zshc.lan.helper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.zshc.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 198 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zshc.150.helper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 63 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.fish.lan.helper/tab_to_insert (a new  |
| terminal | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 196 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.fish.150.helper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.nu.lan.helper/tab_to_insert (a new me |
| terminal | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 196 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.nu.150.helper/tab_to_insert (a new |
| terminal | lab_tab_remote_dir.p_banner.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_banner.lan.helper/tab_to_insert (a  |
| terminal | lab_tab_remote_dir.p_banner.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 206 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_banner.150.helper/tab_to_insert  |
| terminal | lab_tab_remote_dir.p_colon.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 65 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_colon.lan.helper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.p_colon.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 206 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_colon.150.helper/tab_to_insert ( |
| terminal | lab_tab_remote_dir.p_question.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_question.lan.helper/tab_to_insert ( |
| terminal | lab_tab_remote_dir.p_question.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 208 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_question.150.helper/tab_to_inser |
| terminal | lab_tab_remote_dir.p_slowrc.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_slowrc.lan.helper/tab_to_insert (a  |
| terminal | lab_tab_remote_dir.p_slowrc.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 208 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_slowrc.150.helper/tab_to_insert  |
| terminal | lab_tab_remote_dir.p_rcerr.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_rcerr.lan.helper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.p_rcerr.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 205 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_rcerr.150.helper/tab_to_insert ( |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est994k5btc $EPOCHREALTIME >> /work/u_zsh/es_t993iv9g.txt', 'echo est99 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1593; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.bash.lan.helper | PASS | injector accounting: injector sent 2407; app saw 2407; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2343 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.lan.helper.rst | PASS | injector accounting: injector sent 613; app saw 613; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 594 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.bash.lan.helper.stall10 | PASS | injector accounting: injector sent 216; app saw 216; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 207 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.bash.150.helper | PASS | injector accounting: injector sent 2407; app saw 2407; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2343 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.150.helper.rst | PASS | injector accounting: injector sent 613; app saw 613; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 594 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.bash.150.helper.stall10 | PASS | injector accounting: injector sent 216; app saw 216; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 207 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 2399; app saw 2399; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2335 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper.rst | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 592 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper.stall10 | PASS | injector accounting: injector sent 215; app saw 215; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 206 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 2399; app saw 2399; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2336 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper.rst | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 592 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper.stall10 | PASS | injector accounting: injector sent 215; app saw 215; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 206 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.fish.lan.helper | PASS | injector accounting: injector sent 2475; app saw 2475; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2406 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.fish.150.helper | PASS | injector accounting: injector sent 2475; app saw 2475; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2411 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | PASS | injector accounting: injector sent 2889; app saw 2889; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2820 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | PASS | injector accounting: injector sent 2889; app saw 2889; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2820 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | PASS | injector accounting: injector sent 753; app saw 753; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 710 unbound keydown/typed |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 137; app saw 137; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt1736rz9hiz: app log chip update  |
| terminal | lab_helper_killed.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt1739mf0f5z: app log chip update  |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; app find hook gave no position for 'brt174 |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt1745n2rzem: app log chip update  |
| terminal | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T1774_vjf8nx >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rec |
| terminal | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T1788_7ytk5z >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rec |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.bash.lan.nohelper/tab_to_insert (a new m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 380 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.bash.150.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.lan.nohelper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.zsh.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 386 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert (a new |
| agent | lab_tab_remote_dir.zshc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 64 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.zshc.lan.nohelper/tab_to_insert (a new m |
| agent | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 360 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.fish.lan.nohelper/tab_to_insert (a new m |
| agent | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 384 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.fish.150.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 59 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.nu.lan.nohelper/tab_to_insert (a new met |
| agent | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 525 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.nu.150.nohelper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.p_banner.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_banner.lan.nohelper/tab_to_insert (a n |
| agent | lab_tab_remote_dir.p_banner.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 385 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_banner.150.nohelper/tab_to_insert ( |
| agent | lab_tab_remote_dir.p_colon.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_colon.lan.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.p_colon.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 380 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_colon.150.nohelper/tab_to_insert (a |
| agent | lab_tab_remote_dir.p_question.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 62 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_question.lan.nohelper/tab_to_insert (a |
| agent | lab_tab_remote_dir.p_question.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 376 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_question.150.nohelper/tab_to_insert |
| agent | lab_tab_remote_dir.p_slowrc.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/p_slowrc/tabfx_t50wgn1m5/zzqt50wgn1m5_only'], got []; |
| agent | lab_tab_remote_dir.p_slowrc.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/p_slowrc/tabfx_t54zsbkmu/zzqt54zsbkmu_only'], got []; |
| agent | lab_tab_remote_dir.p_rcerr.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 63 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_rcerr.lan.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.p_rcerr.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 376 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_rcerr.150.nohelper/tab_to_insert (a |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 2982657); the app is relaunched so later c |
| agent | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/1, 1.02/1; side-call producers (#2156): batched  |
| agent | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.43/1, 1.43/1; side-call producers (#2156): batched  |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['read -r -d \'\' WARP_BOOTSTRAP_VAR << \'EOM\' // :; eval "$WARP_BOOTSTRAP_VAR" // :; unset  |
| agent | lab_managed_connect.bash.lan.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| agent | lab_managed_connect.bash.150.nohelper | PASS | injector accounting: injector sent 117; app saw 117; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 108 unbound keydown/typed,  |
| agent | lab_msys_ssh_switch.bash.lan.nohelper | SKIP | Windows Git Bash pane only: the one-connection managed ssh and its switch exist only there (#1933, #2051); injector accounting: injector sen |
| agent | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| agent | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 367; app saw 367; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 347 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t269ct6s5t/phxt269ct |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received [], want 'cd /work/u_zsh/tnh_t277guiawp/phxt277gu |
| agent | lab_remote_session.bash.lan.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t301_4lc6mp\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| agent | lab_remote_session.bash.150.nohelper | FAIL | the full-screen stand-in received b'', want exactly b'ihello remote t325_3ucr7r\x1b:wq\r'; the pager-like stand-in received b'', want exactl |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 32 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.bash.lan.helper/tab_to_insert (a new met |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 3021311 -> 3053610); a relaunched app starts in a terminal session, so thi |
| agent | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 205 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.bash.150.helper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.lan.helper/tab_to_insert (a new metr |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 206 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zsh.150.helper/tab_to_insert (a new m |
| agent | lab_tab_remote_dir.zshc.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.zshc.lan.helper/tab_to_insert (a new met |
| agent | lab_tab_remote_dir.zshc.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 206 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zshc.150.helper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.fish.lan.helper/tab_to_insert (a new met |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 230 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.fish.150.helper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 65 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.nu.lan.helper/tab_to_insert (a new metri |
| agent | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 204 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.nu.150.helper/tab_to_insert (a new me |
| agent | lab_tab_remote_dir.p_banner.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_banner.lan.helper/tab_to_insert (a new |
| agent | lab_tab_remote_dir.p_banner.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 210 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_banner.150.helper/tab_to_insert (a  |
| agent | lab_tab_remote_dir.p_colon.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 39 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_colon.lan.helper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.p_colon.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 206 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_colon.150.helper/tab_to_insert (a n |
| agent | lab_tab_remote_dir.p_question.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 67 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_question.lan.helper/tab_to_insert (a n |
| agent | lab_tab_remote_dir.p_question.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 208 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_question.150.helper/tab_to_insert ( |
| agent | lab_tab_remote_dir.p_slowrc.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 66 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_slowrc.lan.helper/tab_to_insert (a new |
| agent | lab_tab_remote_dir.p_slowrc.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 207 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_slowrc.150.helper/tab_to_insert (a  |
| agent | lab_tab_remote_dir.p_rcerr.lan.helper | NOREF | NO-REFERENCE: Tab -> insert took 64 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_rcerr.lan.helper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.p_rcerr.150.helper | NOREF | NO-REFERENCE: Tab -> insert took 207 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_rcerr.150.helper/tab_to_insert (a n |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est403kqp0b $EPOCHREALTIME >> /work/u_zsh/es_t4026upj.txt', 'echo est40 |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |
| agent | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T625_fbw7ku >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |
| agent | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received [], want exactly ['echo T639_gc0tdd >> ~/rl.txt']; trial 2 (gap 0.0 s): the remote shell rece |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead 7.43 +/- 0.92 ms (SE) vs budget 8.09 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT |
| terminal | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=8 run( |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@37 ms, InitShell@78 ms, Bootstrapped@1920 ms; the first inner line ran 1.23 s after it |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.45 ms (p95 2.26, n=59); the wait itself: median 0.0 |
| terminal | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| terminal | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -1.43 +/- 1.13 ms (SE) vs budget 0.79 ms is within noise: INCONCLUSIVE/NO-COMPARE, the over |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); mode guard: the pane was in agent before this termina |
| terminal | routing_plain_installed_head | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 61 unbound keydown/typed, 6 t |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 3 t |
| terminal | latency_enter_ssh | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 2316; app saw 2316; lost-before-app 0 (Enter: sent 61, app saw 61); of the app's: 2251 unbound keydown/ty |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | startup_shell_pane | SKIP | Windows only: the pane-shell passes (PowerShell 7, cmd.exe, WSL chosen through the app's own shell settings) exist on the Windows runner; no |
| terminal | link_and_path_click | PASS | the path opened in an in-app code pane at line 140 (the line's text 'fcwuy4nkd' is drawn; last gained-focus hop in the app log: editor; the  |
| terminal | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 75; app saw 75; lost |
| terminal | editor_memory_7mb | FAIL | rs: -5 MB of phys_footprint still held 75 s after closing the editor (over the pre-open baseline; close via palette cmd+p + 'Close Current S |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (468, 413), tab 3 at (468, 538), tab 4 at (468, 662); tab 3's title was located by OCR (no `appci_find` line for 'v |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with palette cmd+p + 'Close Current Session'; NO-REFER |
| terminal | keybinding_rebind | PASS | control: Alt-R not played: macOS: Option-R types a character by design unless Option is set to act as Meta (terminal.input.extra_meta_keys); |
| terminal | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead 7.52 +/- 1.09 ms (SE) vs budget 9.05 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT |
| agent | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=8 run( |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@30 ms, InitShell@67 ms, Bootstrapped@1907 ms; the first inner line ran 1.17 s after it |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.52 ms (p95 2.23, n=60); the wait itself: median 0.0 |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: insufficient e |
| agent | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -3.24 +/- 1.16 ms (SE) vs budget -1.26 ms is within noise: INCONCLUSIVE/NO-COMPARE, the ove |
| agent | routing_table | PASS | injector accounting: injector sent 631; app saw 631; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| agent | routing_typo_head | PASS | injector accounting: injector sent 73; app saw 73; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 58 unbound keydown/typed, 6 t |
| agent | routing_plain_installed_head | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 61 unbound keydown/typed, 9 t |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter_ssh | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| agent | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| agent | scroll_keys_position | NOREF | control: the job printed 28 lines and none is on screen at the bottom of the agent view (shots/*skp_0_stream.png): the agent view does not d |
| agent | link_and_path_click | PASS | the path opened in an in-app code pane at line 140 (the line's text 'fcckh4fcc' is drawn; last gained-focus hop in the app log: editor; the  |
| agent | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 74; app saw 74; lost |
| agent | tab_pointer_use | NOREF | NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles could not be located in the strip (the rows are not top to bottom in tab order: |
| agent | keybinding_rebind | PASS | control: Alt-R not played: macOS: Option-R types a character by design unless Option is set to act as Meta (terminal.input.extra_meta_keys); |
| agent | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| agent@fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=8 run( |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@15 ms, InitShell@49 ms, Bootstrapped@1313 ms; the first inner line ran 0.80 s after it |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #1176 key -> pty write median 0.63 ms (p95 0.69, n=60): within the 1 ms target; #1995 notifies logged before 13 of 77 key frame build(s): In |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | NOREF | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); mode guard: the pane was in agent before this termina |
| terminal | routing_plain_installed_head | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 61 unbound keydown/typed, 12  |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 5 t |
| terminal | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| terminal | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 2316; app saw 2316; lost-before-app 0 (Enter: sent 61, app saw 61); of the app's: 2251 unbound keydown/ty |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | ime_compose | NOREF | injector accounting: injector sent 334; app saw 316; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 303 unbound keydown/typed, |
| terminal | startup_shell_pane | SKIP | Windows only: the pane-shell passes (PowerShell 7, cmd.exe, WSL chosen through the app's own shell settings) exist on the Windows runner; no |
| terminal | link_and_path_click | PASS | the path went to the desktop opener (system): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expect |
| terminal | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| terminal | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | editor_memory_7mb | PASS | heap prime (#2185): one unmeasured open+close of the txt file first; RSS before 318 MB, after open 443 MB, after the close trim 302 MB (clos |
| terminal | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows (OCR): tab 2 at (126, 156), tab 3 at (126, 219), tab 4 at (125, |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (125, 155), tab 3 at (125, 218), tab 4 at (125, 280); tab 3's title was located by OCR (no `appci_find` line for 'v |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| terminal | keybinding_rebind | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 129 unbound keydown/typed,  |
| terminal | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 16 frame(s) drawn after the rebuild; 6 frame(s) |
| terminal | upgrade_over_previous | NOREF | the previous build's state is packed in /home/<user>/builds/r_hQtIdMf/0/winters-phosphor/phosphor/appci-out/wayland/run/fixtures/prev-state. |
| terminal | history_survives_restart | PASS | injector accounting: injector sent 435; app saw 435; lost-before-app 0 (Enter: sent 15, app saw 15); of the app's: 383 unbound keydown/typed |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=8 run( |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@16 ms, InitShell@44 ms, Bootstrapped@1311 ms; the first inner line ran 0.80 s after it |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #1176 key -> pty write median 0.65 ms (p95 0.71, n=60): within the 1 ms target; #1995 notifies logged before 14 of 77 key frame build(s): In |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -0.9 +/- 0.72 ms (SE) vs budget 0.26 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overh |
| agent | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| agent | routing_typo_head | PASS | injector accounting: injector sent 71; app saw 71; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 58 unbound keydown/typed, 9 t |
| agent | routing_plain_installed_head | PASS | injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 61 unbound keydown/typed, 15  |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| agent | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| agent | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| agent | scroll_keys_position | NOREF | control: the job printed 16 lines and none is on screen at the bottom of the agent view (shots/*skp_0_stream.png): the agent view does not d |
| agent | ime_compose | NOREF | injector accounting: injector sent 332; app saw 314; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 303 unbound keydown/typed, |
| agent | link_and_path_click | PASS | the path went to the desktop opener (system): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expect |
| agent | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| agent | md_viewer | PASS | injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 48 unbound keydown/typed, 3 t |
| agent | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: the panel rows of tabs 2..4 could not be read by OCR (found  |
| agent | tab_pointer_use | NOREF | NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles could not be located in the strip (the rows are not top to bottom in tab order: |
| agent | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| agent | keybinding_rebind | PASS | injector accounting: injector sent 178; app saw 178; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 126 unbound keydown/typed,  |
| agent | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 18 frame(s) drawn after the rebuild; 7 frame(s) |
| agent | history_survives_restart | PASS | injector accounting: injector sent 467; app saw 467; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 420 unbound keydown/typed |
| agent@fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_latency_enter | macos | agent@fake | NOREF |
| agent_latency_enter | wayland | agent@fake | NOREF |
| agent_latency_key | macos | agent@fake | NOREF |
| agent_latency_key | wayland | agent@fake | NOREF |
| editor_daily | macos | agent | NOREF |
| editor_daily | macos | terminal | NOREF |
| editor_memory_7mb | macos | terminal | FAIL |
| editor_memory_7mb | wayland | terminal | PASS |
| gpu_real_render | wayland | agent | NOREF |
| gpu_real_render | wayland | terminal | NOREF |
| history_survives_restart | macos | agent | PASS |
| history_survives_restart | macos | terminal | PASS |
| history_survives_restart | wayland | agent | PASS |
| history_survives_restart | wayland | terminal | PASS |
| history_up_recall | macos | agent | PASS |
| history_up_recall | macos | terminal | PASS |
| history_up_recall | wayland | agent | PASS |
| history_up_recall | wayland | terminal | PASS |
| keybinding_rebind | macos | agent | PASS |
| keybinding_rebind | macos | terminal | PASS |
| keybinding_rebind | wayland | agent | PASS |
| keybinding_rebind | wayland | terminal | PASS |
| link_and_path_click | macos | agent | PASS |
| link_and_path_click | macos | terminal | PASS |
| link_and_path_click | wayland | agent | PASS |
| link_and_path_click | wayland | terminal | PASS |
| network_gate_switch | macos | terminal | NOREF |
| network_gate_switch | wayland | terminal | NOREF |
| remote_tab_enter_no_helper | macos | agent | PASS |
| remote_tab_enter_no_helper | macos | terminal | PASS |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | PASS |
| remote_tab_no_helper | macos | agent | PASS |
| remote_tab_no_helper | macos | terminal | PASS |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | PASS |
| routing_plain_installed_head | macos | agent | PASS |
| routing_plain_installed_head | macos | terminal | PASS |
| routing_plain_installed_head | wayland | agent | PASS |
| routing_plain_installed_head | wayland | terminal | PASS |
| routing_table_powershell | macos | agent | SKIP |
| routing_table_powershell | macos | terminal | SKIP |
| routing_table_powershell | wayland | agent | SKIP |
| routing_table_powershell | wayland | terminal | SKIP |
| routing_typo_head | macos | agent | PASS |
| routing_typo_head | wayland | agent | PASS |
| scroll_keys_position | macos | agent | NOREF |
| scroll_keys_position | macos | terminal | PASS |
| scroll_keys_position | wayland | agent | NOREF |
| scroll_keys_position | wayland | terminal | PASS |
| ssh_gitbash_bootstrap_in_command | macos | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | macos | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | wayland | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | wayland | terminal | SKIP |
| ssh_gitbash_kill_switch | macos | agent | SKIP |
| ssh_gitbash_kill_switch | macos | terminal | SKIP |
| ssh_gitbash_kill_switch | wayland | agent | SKIP |
| ssh_gitbash_kill_switch | wayland | terminal | SKIP |
| ssh_gitbash_managed | macos | agent | SKIP |
| ssh_gitbash_managed | macos | terminal | SKIP |
| ssh_gitbash_managed | wayland | agent | SKIP |
| ssh_gitbash_managed | wayland | terminal | SKIP |
| startup_shell_pane | macos | terminal | SKIP |
| startup_shell_pane | wayland | terminal | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_sh_probe_empty_ps1 | macos | agent | PASS |
| subshell_sh_probe_empty_ps1 | macos | terminal | PASS |
| subshell_sh_probe_empty_ps1 | wayland | agent | PASS |
| subshell_sh_probe_empty_ps1 | wayland | terminal | PASS |
| tab_pointer_use | macos | agent | NOREF |
| tab_pointer_use | macos | terminal | NOREF |
| tab_pointer_use | wayland | agent | NOREF |
| tab_pointer_use | wayland | terminal | NOREF |
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
| ui_vertical_tabs_panel_click | wayland | agent | NOREF |
| ui_vertical_tabs_panel_click | wayland | terminal | PASS |

passed everywhere it ran (11, candidates for the owner's decision): history_survives_restart, history_up_recall, keybinding_rebind, link_and_path_click, remote_tab_enter_no_helper, remote_tab_no_helper, routing_plain_installed_head, routing_typo_head, subshell_ready_rc_clear, subshell_sh_probe_empty_ps1, ui_completions_as_you_type_local
not passed somewhere (fail / skip / no-reference): agent_latency_enter, agent_latency_key, editor_daily, editor_memory_7mb, gpu_real_render, network_gate_switch, routing_table_powershell, scroll_keys_position, ssh_gitbash_bootstrap_in_command, ssh_gitbash_kill_switch, ssh_gitbash_managed, startup_shell_pane, tab_pointer_use, ui_vertical_tabs_panel_click
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run/junit.xml: 5813 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794608807.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794608807.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run/results.json: 6511 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.4.log.gz: 2178 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794603975.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794603975.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794603975.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794603975.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794603975.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 4671 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794606736.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run/results.jsonl: 6646 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794606736.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run/junit.xml: 6013 KB skipped (over 1 MB)
- note: wayland/run/results.json: 6729 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 1394 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.4.log.gz: 2428 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794602201.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794602201.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794602201.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794602201.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794602201.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4896 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 192 KB (1190 of 4391 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 192 KB (1211 of 3912 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 192 KB (1202 of 1535 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 192 KB (1234 of 37126 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 192 KB (1313 of 5793 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 192 KB (1203 of 52663 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter_ssh.log: latency evidence capped at 192 KB (1183 of 4138 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 192 KB (1199 of 1541 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/acct/launch0_1794599844.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794599844.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794599844.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794599844.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794599844.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent@fake/phosphor.log.gz: 1580 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794596386.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794596386.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794596386.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794596386.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794596386.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794596930.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 204 KB (1369 of 15947 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1597 of 16849 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1295 of 4419 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 204 KB (1254 of 1282 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 204 KB (1324 of 35646 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 204 KB (1673 of 12046 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 204 KB (1360 of 15130 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1565 of 15001 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1295 of 4386 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 204 KB (1255 of 1289 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/acct/launch0_1794598008.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794598008.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794598008.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794595516.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794595516.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794595516.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: run dir is 8.9 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell [test-windows]`
  - `warp terminal::view::tests::a_waiting_question_with_no_card_in_the_pane_is_stopped [test-windows]`
  - `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::on [test-windows]`
  - `warp terminal::input::tests::enter_submits_when_submit_on_ctrl_enter_is_false [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_remote_dir.p_slowrc.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.p_slowrc.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/pane_not_local_after_lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_chips.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_nohelper_tab_hands_back.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_nohelper_tab_hands_back.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_remote_session.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_remote_session.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_enter_start.zsh.150.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.lan.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.150.helper`
  - `lab-wayland/terminal/lab_helper_reconnect.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_helper_reconnect.zsh.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_connect_typeahead.zsh.150.helper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_remote_dir.p_slowrc.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_remote_dir.p_slowrc.150.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - ... and 19 more
- FIXED failures (app): 
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.nu.150.nohelper`
  - `lab-wayland/agent/lab_ctrl_c_no_esc_i.nu.150.nohelper`
- Latency regressions > 5%: none
- compared against: test-linux: build/r29a (3062372705); test-macos: build/r29a (3062372705); test-warpui-core: build/r29a (3062372705); test-warpui-core-macos: build/r29a (3062372705); test-warpui-core-windows: build/r29a (3062372705); test-windows: build/r29a (3062372705); test-workspace-rest: build/r29a (3062372705); test-workspace-rest-macos: build/r29a (3062372705); test-workspace-rest-windows: build/r29a (3062372705); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998)
