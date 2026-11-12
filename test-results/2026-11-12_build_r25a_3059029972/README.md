# build/r25a  083c9f018  (pipeline 3059029972)

- date: 2026-11-12, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_agent_remote_errors.nu.lan.nohelper`, `lab_agent_remote_flow.nu.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_codebase_index_remote.fish.150.nohelper`, `lab_codebase_index_remote.fish.lan.nohelper`, `lab_codebase_index_remote.nu.150.nohelper`, `lab_codebase_index_remote.nu.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_file_tree_remote.nu.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_no_helper_chip.fish.lan.nohelper`, `lab_no_helper_chip.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_ssh_drop_reconnect.zsh.150.helper`, `lab_ssh_drop_reconnect.zsh.150.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on linux-wayland: `cb_ui_action_latency`, `ctrl_c_d`, `drag_drop_file`, `editor_memory_7mb`, `gitbash_subshell`, `gpu_lost_device_recovers`, `gpu_real_render`, `ime_compose`, `intl_altgr_bash`, `intl_compose_linux`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `meta_and_word_keys`, `mode_proof`, `network_gate_switch`, `scroll_keys_position`, `ssh_drop_reconnect`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`
  - asked for on macos: `cb_ui_action_latency`, `ctrl_c_d`, `editor_daily`, `editor_memory_7mb`, `gitbash_subshell`, `intl_altgr_bash`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `mode_proof`, `network_gate_switch`, `scroll_keys_position`, `ssh_drop_reconnect`, `tab_pointer_use`, `tabs_streaming`, `tabs_two_windows`, `wheel_scroll`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_agent_remote_errors.nu.lan.nohelper`, `lab_agent_remote_flow.nu.lan.nohelper`, `lab_chips.bash.150.helper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_codebase_index_remote.fish.150.nohelper`, `lab_codebase_index_remote.fish.lan.nohelper`, `lab_codebase_index_remote.nu.150.nohelper`, `lab_codebase_index_remote.nu.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.nu.lan.helper`, `lab_ctrl_c_no_esc_i.nu.lan.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.nohelper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_file_tree_remote.nu.lan.nohelper`, `lab_freeze_and_wake.bash.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_no_helper_chip.nu.lan.nohelper`, `lab_no_helper_chip.zsh.lan.nohelper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.bash.150.helper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on windows: `cb_ui_action_latency`, `ctrl_c_d`, `editor_memory_7mb`, `gitbash_subshell`, `intl_altgr_bash`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `mode_proof`, `ssh_drop_reconnect`, `tabs_streaming`, `tabs_two_windows`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `minimize_fullscreen`, `network_gate_switch`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `scroll_keys_position`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `wheel_scroll`, `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `cb_agent_pickers`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `intl_altgr_ps`, `job_control`, `keybinding_rebind`, `latency_enter_ssh`, `md_viewer_first_frame`, `meta_and_word_keys`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
- unit: 37924 passed, 0 failed, 144 skipped of 37924 run (8 suites)
- app (pass/fail/skip): lab-macos 38/53/0 +103 no-ref; lab-wayland 19/79/0 +81 no-ref; macos 19/6/4 +7 no-ref; wayland 17/2/6 +18 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-lab-linux | failed | 102.8 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 125.9 | MacBookPro-gitrunner |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | failed | 59.8 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 92.8 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 28.9 | - |  |
| build-macos-arm64 | success | 18.9 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 8.2 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 7.9 | - |  |
| guards-helper-deps | success | 0.3 | - |  |
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
| sshlab-image | success | 0.3 | - |  |
| test-linux | success | 22.3 | - |  |
| test-macos | success | 28.8 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.0 | phosphor-windows-vm |  |
| test-windows | canceled | 31.2 | phosphor-windows-vm |  |
| test-workspace-rest | success | 2.4 | - |  |
| test-workspace-rest-macos | success | 3.5 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.1 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- error: error: nextest exited 127 (not 0=passed / 100=test failures).
- error: ERROR: Job failed: exit status 2

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | the shell received ['cd /work/u_nu/tabfx_t256oam89/zzqt256oam89'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-in |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 39479); the app is relaunched so later cas |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 6 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1488; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [kill] no local prompt within 20 s after the connection was |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | FAIL | [escape_connect_enter] no local prompt within 20 s after the connection was lost; [escape_connect_bare] no local prompt within 20 s after th |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_no_helper_chip.zsh.lan.nohelper | PASS | shell-mode chip: app log has the chip view; OCR reads it; injector accounting: injector sent 81; app saw 81; lost-before-app 0 (Enter: sent  |
| terminal | lab_no_helper_chip.nu.lan.nohelper | PASS | shell-mode chip: app log has the chip view; OCR reads it; injector accounting: injector sent 88; app saw 88; lost-before-app 0 (Enter: sent  |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | branch brt29270o4mk not on screen after cd into the repo on the remote; shell arm: 18 out-of-band channels for 4 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | PASS | injector accounting: injector sent 80; app saw 80; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 71 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | PASS | injector accounting: injector sent 80; app saw 80; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 71 unbound keydown/typed, 3 t |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_file_tree_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 110; app saw 110; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t365wl5f31.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t368eeqvzx.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t37117em0w.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t374wh2vao.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_codebase_index_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 116 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.fish.150.nohelper | PASS | injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 116 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 129; app saw 129; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 120 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.nu.150.nohelper | PASS | injector accounting: injector sent 129; app saw 129; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 120 unbound keydown/typed,  |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_path_detect.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_chips.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est451ciiyu $EPOCHREALTIME >> /work/u_zsh/es_t4506v9f.txt', 'echo est45 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [kill] no local prompt within 20 s after the connection was |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | FAIL | [escape_connect_enter] no local prompt within 20 s after the connection was lost; [escape_connect_bare] no local prompt within 20 s after th |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.lan.helper | FAIL | branch brt771o26k4o not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt809yrfjap: a |
| terminal | lab_helper_killed.bash.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt8166m66dr: a |
| terminal | lab_helper_killed.zsh.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| agent | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | the shell received ['cd /work/u_nu/tabfx_t256oam89/zzqt256oam89'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-in |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 92349); the app is relaunched so later cas |
| agent | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 6 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1488; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 0 ms] effect file: expected ['t187hqahg3j3', 't188cofggchc', 't189jwhdyok8', 't190wz6ku7ed'], got ['t187hqahg3j3', 't188cofggchc', ' |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_remote_flow.nu.lan.nohelper | PASS | injector accounting: injector sent 446; app saw 446; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 434 unbound keydown/typed,  |
| agent | lab_agent_remote_errors.nu.lan.nohelper | PASS | injector accounting: injector sent 342; app saw 342; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 332 unbound keydown/typed,  |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_freeze_and_wake.bash.lan.nohelper | NOREF | NO-REFERENCE: keys typed while frozen: the line typed while the app was frozen never arrived (the display system did not hold the keys); the |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 6054 -> 8138); a relaunched app starts in a terminal session, so this is n |
| agent | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_path_detect.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 99; app |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_chips.bash.150.helper | PASS | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; injector accounting: injector sent 94; app |
| agent | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.nu.lan.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.lan.helper | PASS | after lab_ctrl_c_no_esc_i.nu.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t11woo3sb/zzqt11woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t18pz0lx9/zzqt18pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | the shell received ['cd /work/u_nu/tabfx_t256oam89/zzqt256oam89'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-in |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 2691384); the app is relaunched so later c |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | FAIL | 3 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1488; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | PASS | injector accounting: injector sent 1488; app saw 1488; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1444 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [kill] no local prompt within 20 s after the connection was |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | FAIL | [escape] no local prompt within 20 s after the connection was lost; [escape_connect_bare] no local prompt within 20 s after the connection w |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 50 ms] effect file: expected ['t241a70ldz0m', 't242f288vqm6', 't243psqjrxr6', 't2448bgnrkuo', 't245oyqqnqyc'], got ['t241a70ldz0m',  |
| terminal | lab_no_helper_chip.fish.lan.nohelper | PASS | shell-mode chip: app log has the chip view; OCR does not read it; injector accounting: injector sent 82; app saw 82; lost-before-app 0 (Ente |
| terminal | lab_no_helper_chip.nu.lan.nohelper | PASS | shell-mode chip: app log has the chip view; OCR does not read it; injector accounting: injector sent 88; app saw 88; lost-before-app 0 (Ente |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | branch brt263y4lc6m not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | branch brt271dtjiek not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | branch brt275iemk0n not on screen after cd into the repo on the remote; shell arm: 6 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | branch brt283mj9qex not on screen after cd into the repo on the remote; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | branch brt2877rl419 not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | branch brt2919h11xo not on screen after cd into the repo on the remote; shell arm: 4 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | branch brt299xwj3m8 not on screen after cd into the repo on the remote; shell arm: 4 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | branch brt3076nt5dy not on screen after cd into the repo on the remote; shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | branch brt311ttn1a3 not on screen after cd into the repo on the remote; shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | branch brt315pgk6dy not on screen after cd into the repo on the remote; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 80 recorded lines) |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; recorder evidence (nohelper_tab_hands_back, after 80 recorded lines) |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_file_tree_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 110; app saw 110; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 100 unbound keydown/typed,  |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t360ai0wr5.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t363yht0po.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t366qaaj4j.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t369ep99ji.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_codebase_index_remote.fish.lan.nohelper | PASS | injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 116 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.fish.150.nohelper | PASS | injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 117 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.nu.lan.nohelper | PASS | injector accounting: injector sent 129; app saw 129; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 120 unbound keydown/typed,  |
| terminal | lab_codebase_index_remote.nu.150.nohelper | PASS | injector accounting: injector sent 129; app saw 129; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 120 unbound keydown/typed,  |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.helper | FAIL | the shell received ['cd /work/u_bash/tabfx_t412i6o15e/zzqt412i6o15e'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| terminal | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.helper | FAIL | the shell received ['cd /work/u_fish/tabfx_t425uttyty/zzqt425uttyty'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| terminal | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.150.helper | FAIL | the shell received ['cd /work/u_nu/tabfx_t432ss3396/zzqt432ss3396'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est443whhn2 $EPOCHREALTIME >> /work/u_zsh/es_t442sr45.txt', 'echo est44 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | PASS | injector accounting: injector sent 1488; app saw 1488; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1444 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [kill] no local prompt within 20 s after the connection was |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | FAIL | [escape] no local prompt within 20 s after the connection was lost; [escape_connect_bare] no local prompt within 20 s after the connection w |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x2 gap 50 ms] effect file: expected ['t720ci1lhr0j', 't72101tv1k8q', 't722dc2e79qs', 't723pkat8qhd', 't724w8eaw7j6'], got ['t720ci1lhr0j',  |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.zsh.150.helper | FAIL | branch brt7519wrdw8 not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_fs_basic.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.fish.150.helper | FAIL | branch brt758bzf07y not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_fs_basic.nu.lan.helper | FAIL | branch brt762vefd97 not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_fs_basic.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.lan.helper | FAIL | branch brt7699c77d9 not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | FAIL | branch brt776k5lfmo not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.zsh.150.helper | FAIL | branch brt780o26k4o not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_git_read.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.bash.150.helper | FAIL | branch brt799uwcx6v not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.zsh.lan.helper | FAIL | branch brt803kkigkt not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | FAIL | branch brt8107w43wf not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_run_command.fish.150.helper | FAIL | branch brt814dym9sc not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.nu.lan.helper | FAIL | branch brt818zbz51t not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_run_command.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt826uftjla: a |
| terminal | lab_helper_killed.bash.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| terminal | lab_helper_killed.zsh.lan.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover NOT settled within 30 s: executor_swapped= |
| terminal | lab_helper_killed.zsh.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t8b8ayn1/zzqt8b8ayn1'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-ins |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t12woo3sb/zzqt12woo3sb'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-i |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | FAIL | the shell received ['cd /work/u_fish/tabfx_t19pz0lx9/zzqt19pz0lx9'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| agent | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | the shell received ['cd /work/u_nu/tabfx_t266oam89/zzqt266oam89'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab-in |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 2867700); the app is relaunched so later c |
| agent | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 6 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1489; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 0 ms] effect file: expected ['t149r7vik1t2', 't150yb6qhsqb', 't1516losfk3x', 't1524g5fcdbr'], got ['t149r7vik1t2', 't150yb6qhsqb', ' |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_remote_flow.nu.lan.nohelper | PASS | injector accounting: injector sent 446; app saw 446; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 434 unbound keydown/typed,  |
| agent | lab_agent_remote_errors.nu.lan.nohelper | PASS | injector accounting: injector sent 342; app saw 342; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 332 unbound keydown/typed,  |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 2893079 -> 2895958); a relaunched app starts in a terminal session, so thi |
| agent | lab_tab_remote_dir.bash.150.helper | FAIL | the shell received ['cd /work/u_bash/tabfx_t173jhydle/zzqt173jhydle'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.helper | FAIL | the shell received ['cd /work/u_zsh/tabfx_t180ysfo29/zzqt180ysfo29'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | FAIL | the shell received ['cd /work/u_fish/tabfx_t1878n4ow7/zzqt1878n4ow7'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| agent | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.helper | FAIL | the shell received ['cd /work/u_nu/tabfx_t194nz26r1/zzqt194nz26r1'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no tab- |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| agent | lab_enter_start.zsh.150.helper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x2 gap 50 ms] effect file: expected ['t36915cuoe1f', 't3704pwmsk2h', 't371q8tgsm3o', 't372ev3229o1', 't373fzzggxww'], got ['t36915cuoe1f',  |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.46 ms (p95 2.24, n=60); the wait itself: median 0.0 |
| terminal | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| terminal | latency_first_key_after_idle | NOREF | First key after 1 s idle: first key after 1 s idle, kl3 post-to-first-changed-pixel at 240 Hz (4.17 ms quantum, not applied as a resolution  |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 1418; app saw 1418; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1373 unbound keydown/ty |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | intl_altgr_bash | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| terminal | link_and_path_click | FAIL | after_links: file has 0/4 lines, exact match False first diff: exp='t45_pobpzer9ee' got=None [first bad line 0: injector call #1265 posted i |
| terminal | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 59251); the app is relaunched so later cases start clean |
| terminal | wheel_scroll | PASS | injector accounting: injector sent 196; app saw 196; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 185 unbound keydown/typed,  |
| terminal | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| terminal | editor_memory_7mb | FAIL | txt: opening a 7 MB file cost 157 MB of rss (> 100 MB target); judged memory figures: phys_footprint, rss (owner: each one is checked; macOS |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (461, 413), tab 3 at (461, 538), tab 4 at (461, 662); tab 3's title was located by OCR (no `appci_find` line for 'v |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with palette cmd+p + 'Close Current Session'; NO-REFER |
| terminal | tabs_two_windows | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; no close key on this platform: the throw-away tab was ended  |
| terminal | tabs_streaming | PASS | no close key on this platform: the throw-away tab was ended with Ctrl-D (unverified that the tab disappears); no close key on this platform: |
| terminal | cb_ui_action_latency | PASS | no close key on this platform: the throw-away tab was ended with Ctrl-D (unverified that the tab disappears); injector accounting: injector  |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.47 ms (p95 2.17, n=60); the wait itself: median 0.0 |
| agent | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| agent | latency_first_key_after_idle | NOREF | First key after 1 s idle: first key after 1 s idle, kl3 post-to-first-changed-pixel at 240 Hz (4.17 ms quantum, not applied as a resolution  |
| agent | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines wer |
| agent | intl_altgr_bash | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| agent | link_and_path_click | FAIL | after_links: file has 0/4 lines, exact match False first diff: exp='t12_prhlwsekkq' got=None [first bad line 0: injector call #2802 posted i |
| agent | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 58488); the app is relaunched so later cases start clean |
| agent | wheel_scroll | PASS | injector accounting: injector sent 196; app saw 196; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 185 unbound keydown/typed,  |
| agent | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| agent | tab_pointer_use | NOREF | NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles could not be located in the strip (the rows are not top to bottom in tab order: |
| agent | tabs_two_windows | PASS | no close key on this platform: the throw-away tab was ended with Ctrl-D (unverified that the tab disappears); no close key on this platform: |
| agent | tabs_streaming | PASS | no close key on this platform: the throw-away tab was ended with Ctrl-D (unverified that the tab disappears); no close key on this platform: |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #1176 key -> pty write median 0.64 ms (p95 0.71, n=60): within the 1 ms target; #1995 notifies logged before 13 of 77 key frame build(s): In |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| terminal | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 1418; app saw 1418; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1373 unbound keydown/ty |
| terminal | scroll_keys_position | NOREF | NO-REFERENCE: UNVERIFIABLE: OCR read 0 of the numbered lines on a screen that shows the last of 500 (shots/*skp_0_bottom.png): the scroll po |
| terminal | ime_compose | NOREF | injector accounting: injector sent 334; app saw 316; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 303 unbound keydown/typed, |
| terminal | intl_altgr_bash | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| terminal | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| terminal | editor_memory_7mb | FAIL | txt: opening a 7 MB file cost 129 MB of rss (> 100 MB target); txt: opening the 7.0 MB file added 129 MB of rss (18.4x the file); txt: the a |
| terminal | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows (OCR): tab 2 at (122, 156), tab 3 at (122, 217), tab 4 at (122, |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (122, 155), tab 3 at (122, 218), tab 4 at (122, 280); NO-REFERENCE: UNVERIFIABLE: horizontal bar: the tabs' titles  |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| terminal | gpu_lost_device_recovers | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; device reported lost once; renderer rebuilt on Intel(R) UHD  |
| terminal | tabs_two_windows | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; injector accounting: injector sent 3315; app saw 3315; lost- |
| terminal | upgrade_over_previous | NOREF | the previous build's state is packed in /home/<user>/builds/r_hQtIdMf/0/winters-phosphor/phosphor/appci-out/wayland/run/fixtures/prev-state. |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 2739; app saw 2739; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2658 unbound keydown/ty |
| terminal | cb_ui_action_latency | PASS | injector accounting: injector sent 672; app saw 672; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 647 unbound keydown/typed |
| terminal | meta_and_word_keys | PASS | injector accounting: injector sent 761; app saw 761; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 617 unbound keydown/typed |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 4.64 +/- 0.34 ms (SE) over foot exceeds the 2 ms budget (overhead - 2 SE > budget); measurement mo |
| agent | latency_key_log | PASS | #1176 key -> pty write median 0.61 ms (p95 0.67, n=60): within the 1 ms target; #1995 notifies logged before 13 of 77 key frame build(s): In |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| agent | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| agent | scroll_keys_position | NOREF | control: the job printed 16 lines and none is on screen at the bottom of the agent view (shots/*skp_0_stream.png): the agent view does not d |
| agent | ime_compose | NOREF | injector accounting: injector sent 329; app saw 311; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 300 unbound keydown/typed, |
| agent | intl_altgr_bash | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| agent | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| agent | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: the panel rows of tabs 2..4 could not be read by OCR (found  |
| agent | tab_pointer_use | NOREF | NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles could not be located in the strip (the rows are not top to bottom in tab order: |
| agent | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| agent | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 18 frame(s) drawn after the rebuild; 6 frame(s) |
| agent | tabs_two_windows | PASS | injector accounting: injector sent 3315; app saw 3315; lost-before-app 0 (Enter: sent 44, app saw 44); of the app's: 3227 unbound keydown/ty |
| agent | tabs_streaming | PASS | injector accounting: injector sent 2664; app saw 2664; lost-before-app 0 (Enter: sent 37, app saw 37); of the app's: 2584 unbound keydown/ty |
| agent | meta_and_word_keys | PASS | injector accounting: injector sent 761; app saw 761; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 617 unbound keydown/typed |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| editor_daily | macos | agent | NOREF |
| editor_daily | macos | terminal | NOREF |
| editor_memory_7mb | macos | terminal | FAIL |
| editor_memory_7mb | wayland | terminal | FAIL |
| gpu_real_render | wayland | agent | NOREF |
| gpu_real_render | wayland | terminal | NOREF |
| link_and_path_click | macos | agent | FAIL |
| link_and_path_click | macos | terminal | FAIL |
| meta_and_word_keys | wayland | agent | PASS |
| meta_and_word_keys | wayland | terminal | PASS |
| network_gate_switch | macos | terminal | NOREF |
| network_gate_switch | wayland | terminal | NOREF |
| scroll_keys_position | macos | agent | FAIL |
| scroll_keys_position | macos | terminal | PASS |
| scroll_keys_position | wayland | agent | NOREF |
| scroll_keys_position | wayland | terminal | NOREF |
| tab_pointer_use | macos | agent | NOREF |
| tab_pointer_use | macos | terminal | NOREF |
| tab_pointer_use | wayland | agent | NOREF |
| tab_pointer_use | wayland | terminal | NOREF |
| ui_vertical_tabs_panel_click | wayland | agent | NOREF |
| ui_vertical_tabs_panel_click | wayland | terminal | PASS |
| wheel_scroll | macos | agent | PASS |
| wheel_scroll | macos | terminal | PASS |

passed everywhere it ran (2, candidates for the owner's decision): meta_and_word_keys, wheel_scroll
not passed somewhere (fail / skip / no-reference): editor_daily, editor_memory_7mb, gpu_real_render, link_and_path_click, network_gate_switch, scroll_keys_position, tab_pointer_use, ui_vertical_tabs_panel_click
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run/junit.xml: 3129 KB skipped (over 1 MB)
- note: macos/run/results.json: 3683 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.7.log.gz: 1754 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794493824.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794493824.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794493824.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794493824.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 4901 KB skipped (over 1 MB)
- note: wayland/run/junit.xml: 3488 KB skipped (over 1 MB)
- note: wayland/run/results.jsonl: 3925 KB skipped (over 1 MB)
- note: wayland/run/results.json: 3973 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.4.log.gz: 1411 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794491795.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794491795.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794491795.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794491795.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4258 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 522 KB (3214 of 4388 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 522 KB (3179 of 4375 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: macos/run_slim/logs/agent/phosphor.log.gz: 1910 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.1.log.gz: 3012 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 1576 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.1.log.gz: 3869 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 407 KB (2719 of 15916 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/case_latency_first_key_after_idle.log: latency evidence capped at 407 KB (2556 of 4441 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 407 KB (2684 of 15083 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/terminal/case_latency_first_key_after_idle.log: latency evidence capped at 407 KB (2554 of 4423 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/acct/launch0_1794489313.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794489313.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.log.gz: 1429 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794487271.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794487271.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794488130.log.in_session.0: 10239 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_remote_dir.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.fish.150.nohelper (case not in previous run)`
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
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.150.nohelper (case not in previous run)`
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
  - `lab-wayland/terminal/lab_tab_remote_dir.bash.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.fish.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.nu.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_enter_start.zsh.150.helper`
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.lan.helper`
  - ... and 45 more
- FIXED failures (app): 
  - `lab-wayland/agent/lab_agent_remote_flow.nu.lan.nohelper`
  - `lab-wayland/agent/lab_agent_remote_errors.nu.lan.nohelper`
- Latency regressions > 5%: none
- compared against: test-linux: build/r24a (3057403889); test-macos: build/r24a (3057403889); test-warpui-core: build/r24a (3057403889); test-warpui-core-macos: build/r24a (3057403889); test-warpui-core-windows: build/r24a (3057403889); test-workspace-rest: build/r24a (3057403889); test-workspace-rest-macos: build/r24a (3057403889); test-workspace-rest-windows: build/r24a (3057403889); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998)
