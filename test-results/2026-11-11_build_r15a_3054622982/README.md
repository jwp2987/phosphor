# build/r15a  f546422bd  (pipeline 3054622982)

- date: 2026-11-11, tier: targeted, pipeline status: running (collected before it finished)
- compile (check-linux): -
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.*`, `lab_agent_remote_*`, `lab_agent_suggestions_ssh.*`, `lab_chips.*`, `lab_codebase_index_remote.*`, `lab_ctrl_c_no_esc_i.*`, `lab_diff_apply_remote.*`, `lab_diff_panel_remote.*`, `lab_enter_no_network_wait.*`, `lab_enter_start.*.150.*`, `lab_fam_agent_*`, `lab_fam_fs_basic.*`, `lab_fam_git_read.*`, `lab_fam_run_command.*`, `lab_file_tree_remote.*`, `lab_freeze_and_wake.*`, `lab_gh_chip_remote.*`, `lab_git_write_remote.*`, `lab_helper_*`, `lab_no_helper_chip.*`, `lab_nohelper_*`, `lab_nohelper_sh_login_keys.ash.lan.nohelper`, `lab_nohelper_sh_login_keys.bash.lan.nohelper`, `lab_nohelper_sh_login_keys.dash.lan.nohelper`, `lab_nohelper_sh_login_profile.ash.lan.nohelper`, `lab_nohelper_sh_login_profile.dash.lan.nohelper`, `lab_nu_agent_pager_env.*`, `lab_password_prompt.p_kbdint.lan.nohelper`, `lab_password_prompt.p_password.lan.nohelper`, `lab_path_detect.*`, `lab_path_detect.p_exec.lan.nohelper`, `lab_path_detect.p_tinypath.lan.nohelper`, `lab_raw_key_arrival.*`, `lab_read_files_remote.*`, `lab_remote_edit.*`, `lab_skills_remote.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.bash.lan.nohelper.rst`, `lab_ssh_drop_reconnect.bash.lan.nohelper.stall10`, `lab_tab_remote_dir.*`
  - asked for on linux-wayland: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `big_paste`, `cb_ui_action_latency`, `desktop_notification`, `desktop_notification_agent_question`, `drag_drop_file`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `gpu_lost_device_recovers`, `gpu_real_render`, `history_up_recall`, `hostile_output_recovers`, `ime_compose`, `intl_compose_linux`, `job_control`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `link_and_path_click`, `md_viewer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `network_gate_switch`, `pane_resize_maximize`, `primary_and_middle_click`, `remote_heredoc_paste`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `subshell_ready_relay_shape`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_readline_keys`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
  - asked for on macos: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `big_paste`, `cb_ui_action_latency`, `context_menu_actions`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_up_recall`, `hostile_output_recovers`, `ime_compose`, `job_control`, `keybinding_rebind`, `latency_enter`, `latency_first_key_after_idle`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `precmd_question_echo`, `precmd_secret_honor_ps1`, `remote_heredoc_paste`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `subshell_ready_relay_shape`, `tab_pointer_use`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_mouse_select_copy`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.*`, `lab_agent_remote_*`, `lab_agent_suggestions_ssh.*`, `lab_chips.*`, `lab_codebase_index_remote.*`, `lab_ctrl_c_no_esc_i.*`, `lab_diff_apply_remote.*`, `lab_diff_panel_remote.*`, `lab_enter_no_network_wait.*`, `lab_enter_start.*.150.*`, `lab_fam_agent_*`, `lab_fam_fs_basic.*`, `lab_fam_git_read.*`, `lab_fam_run_command.*`, `lab_file_tree_remote.*`, `lab_freeze_and_wake.*`, `lab_gh_chip_remote.*`, `lab_git_write_remote.*`, `lab_helper_*`, `lab_no_helper_chip.*`, `lab_nohelper_*`, `lab_nohelper_sh_login_keys.ash.lan.nohelper`, `lab_nohelper_sh_login_keys.dash.lan.nohelper`, `lab_nohelper_sh_login_profile.ash.lan.nohelper`, `lab_nohelper_sh_login_profile.dash.lan.nohelper`, `lab_nu_agent_pager_env.*`, `lab_password_prompt.p_kbdint.lan.nohelper`, `lab_password_prompt.p_password.lan.nohelper`, `lab_path_detect.*`, `lab_path_detect.p_exec.lan.nohelper`, `lab_path_detect.p_tinypath.lan.nohelper`, `lab_raw_key_arrival.*`, `lab_read_files_remote.*`, `lab_remote_edit.*`, `lab_skills_remote.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.bash.lan.nohelper.rst`, `lab_ssh_drop_reconnect.bash.lan.nohelper.stall10`, `lab_tab_remote_dir.*`
  - asked for on windows: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `big_paste`, `cb_agent_pickers`, `cb_ui_action_latency`, `ctrl_c_d`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `expert_fuzz`, `flood_scrollback_find`, `focus_cycle`, `gitbash_subshell`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `ime_compose`, `intl_altgr_bash`, `intl_altgr_ps`, `intl_wide_paste`, `intl_wide_type`, `job_control`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `md_viewer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `network_gate_switch`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `subshell_ready_relay_shape`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): -; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | skipped | - | - |  |
| app-test-lab-linux | skipped | - | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | skipped | - | - |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | created | - | - | not run |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | skipped | - | - |  |
| app-test-windows | created | - | - | not run |
| app-test-windows-reuse | manual | - | - | not run |
| bench | skipped | - | - |  |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | running | 3.1 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | pending | - | - | not run |
| check-linux | skipped | - | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | skipped | - | - |  |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | failed | 2.9 | - |  |
| guards-helper-deps | skipped | - | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | skipped | - | - |  |
| integration-linux-pwsh | skipped | - | - |  |
| integration-linux: [1] | skipped | - | - |  |
| integration-linux: [2] | skipped | - | - |  |
| integration-linux: [3] | skipped | - | - |  |
| lab-helper | manual | - | - | not run |
| licenses | skipped | - | - |  |
| mutants | skipped | - | - |  |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | skipped | - | - |  |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | manual | - | - | not run |
| test-linux | skipped | - | - |  |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | skipped | - | - |  |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | skipped | - | - |  |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | skipped | - | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | skipped | - | - |  |
| usage-test-macos | manual | - | - | not run |
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- nothing comparable in earlier runs
