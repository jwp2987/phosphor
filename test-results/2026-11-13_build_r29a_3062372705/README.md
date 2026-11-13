# build/r29a  eede509b1  (pipeline 3062372705)

- date: 2026-11-13, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on linux-wayland: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `drag_drop_file`, `editor_memory_7mb`, `gpu_lost_device_recovers`, `gpu_real_render`, `history_up_recall`, `ime_compose`, `intl_compose_linux`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `network_gate_switch`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_table`, `routing_table_powershell`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`
  - asked for on macos: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `editor_daily`, `editor_memory_7mb`, `history_up_recall`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `network_gate_switch`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_table`, `routing_table_powershell`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `tab_pointer_use`, `ui_completions_as_you_type_local`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_freeze_and_wake.bash.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on windows: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `editor_memory_7mb`, `history_up_recall`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_table`, `routing_table_powershell`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `ui_completions_as_you_type_local`, `cb_ui_action_latency`, `gitbash_subshell`, `intl_altgr_bash`, `tabs_streaming`, `tabs_two_windows`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `mac_pointer_chord_proof`, `md_viewer_ai_answer`, `md_viewer_big`, `minimize_fullscreen`, `network_gate_switch`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `wheel_scroll`, `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `cb_agent_pickers`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_survives_restart`, `hostile_output_recovers`, `intl_altgr_ps`, `job_control`, `md_viewer_first_frame`, `meta_and_word_keys`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `restore_shows_history`, `scroll_keys_foreground`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
- unit: 50108 passed, 0 failed, 191 skipped of 50108 run (9 suites)
- app (pass/fail/skip): lab-macos 17/64/0 +83 no-ref; lab-wayland 45/56/0 +67 no-ref; macos 29/5/9 +12 no-ref; wayland 26/2/11 +25 no-ref; windows 211/26/209 +33 no-ref; soak: pass (windows)

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 89.1 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 104.4 | MacBookPro-gitrunner |  |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 78.1 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 112.3 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 358.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 29.2 | - |  |
| build-macos-arm64 | success | 18.7 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 39.3 | phosphor-windows-vm |  |
| check-linux | success | 8.0 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 8.5 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| lab-helper | success | 0.7 | - |  |
| licenses | success | 0.4 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.3 | - |  |
| test-linux | success | 22.2 | - |  |
| test-macos | success | 29.0 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.0 | phosphor-windows-vm |  |
| test-windows | success | 46.2 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.7 | - |  |
| test-workspace-rest-macos | success | 3.5 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 6.9 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off` LEAK (51.3s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off` LEAK (51.2s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::on` LEAK (130.2s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.5s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::enter_submits_when_submit_on_ctrl_enter_is_false` LEAK (0.6s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.4s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.6s)
- `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line` LEAK (2.0s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | FAIL | Tab -> insert took 303 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188): the app logged no remote_tab_wa |
| terminal | lab_tab_remote_dir.bash.150.nohelper | FAIL | Tab -> insert took 546 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188): the app logged no remote_tab_ |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | FAIL | Tab -> insert took 309 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab -> insert took 562 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | FAIL | Tab -> insert took 308 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| terminal | lab_tab_remote_dir.fish.150.nohelper | FAIL | Tab -> insert took 540 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | FAIL | Tab -> insert took 313 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | Tab -> insert took 678 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 18839); the app is relaunched so later cas |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.31/5, 1.31/5; side-call producers (#2156): shell ex |
| terminal | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.73/1, 1.73/1; side-call producers (#2156): shell ex |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1490; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.87/5, 0.87/5, 4.56/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_git_read.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.35/5, 1.35/5, 5.29/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.11/5, 1.11/5, 4.77/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.56/5, 1.56/5, 5.51/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t3420wr5cy.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t345t0po71.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t348aaj4jw.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t351p99jiz.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | FAIL | Tab -> insert took 261 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| terminal | lab_tab_remote_dir.bash.150.helper | FAIL | Tab -> insert took 382 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_tab_remote_dir.zsh.lan.helper | FAIL | Tab -> insert took 312 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); Tab then Enter 14 ms later: rec.log has no `cd` line for |
| terminal | lab_tab_remote_dir.zsh.150.helper | FAIL | Tab -> insert took 370 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); Tab then Enter 12 ms later: rec.log has no `cd` line f |
| terminal | lab_tab_remote_dir.fish.lan.helper | FAIL | Tab -> insert took 255 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| terminal | lab_tab_remote_dir.fish.150.helper | FAIL | Tab -> insert took 360 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_tab_remote_dir.nu.lan.helper | FAIL | Tab -> insert took 267 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| terminal | lab_tab_remote_dir.nu.150.helper | FAIL | Tab -> insert took 391 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est418yfbet $EPOCHREALTIME >> /work/u_zsh/es_t41796n3.txt', 'echo est42 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
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
| terminal | lab_fam_git_read.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.fish.150.helper | FAIL | branch brt770o26k4o not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt781pkhfvm: a |
| terminal | lab_helper_killed.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt784jxkhp4: app log chip update n |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt787k4he4g: a |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt79000uwcx: app log chip update n |
| agent | lab_tab_remote_dir.bash.lan.nohelper | FAIL | Tab -> insert took 309 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188): the app logged no remote_tab_wa |
| agent | lab_tab_remote_dir.bash.150.nohelper | FAIL | Tab -> insert took 522 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188): the app logged no remote_tab_ |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | FAIL | Tab -> insert took 263 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab -> insert took 525 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.fish.lan.nohelper | FAIL | Tab -> insert took 298 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | lab_tab_remote_dir.fish.150.nohelper | FAIL | Tab -> insert took 540 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.nu.lan.nohelper | FAIL | Tab -> insert took 264 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | Tab -> insert took 704 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 64719); the app is relaunched so later cas |
| agent | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.39/5, 1.39/5; side-call producers (#2156): shell ex |
| agent | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.78/1, 1.78/1; side-call producers (#2156): shell ex |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1490; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_freeze_and_wake.bash.lan.nohelper | NOREF | NO-REFERENCE: keys typed while frozen: the line typed while the app was frozen never arrived (the display system did not hold the keys); the |
| agent | lab_tab_remote_dir.bash.lan.helper | FAIL | Tab -> insert took 290 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 75228 -> 76570); a relaunched app starts in a terminal session, so this is |
| agent | lab_tab_remote_dir.bash.150.helper | FAIL | Tab -> insert took 384 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.zsh.lan.helper | FAIL | Tab -> insert took 262 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | lab_tab_remote_dir.zsh.150.helper | FAIL | Tab -> insert took 390 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.fish.lan.helper | FAIL | Tab -> insert took 253 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | lab_tab_remote_dir.fish.150.helper | FAIL | Tab -> insert took 387 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.nu.lan.helper | FAIL | Tab -> insert took 315 ms; the budget is 1 RTT (0 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released why= |
| agent | lab_tab_remote_dir.nu.150.helper | FAIL | Tab -> insert took 380 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| agent | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
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

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | PASS | remote Tab wait (#2188): the app logged no remote_tab_wait line for this step (a build before #2188, PHOSPHOR_REMOTE_TAB_ENTER_WAIT=0, the r |
| terminal | lab_tab_remote_dir.bash.150.nohelper | FAIL | Tab -> insert took 382 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188): the app logged no remote_tab_ |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 10 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab -> insert took 376 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 17 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| terminal | lab_tab_remote_dir.fish.150.nohelper | FAIL | Tab -> insert took 377 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 37 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | Tab -> insert took 536 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 1429853); the app is relaunched so later c |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; the app log says terminal but the screen reads as a |
| terminal | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.23/5, 1.23/5; side-call producers (#2156): batched  |
| terminal | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.64/1, 1.64/1; side-call producers (#2156): batched  |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1488; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | FAIL | [escape] no local prompt within 20 s after the connection was lost; [escape] app log after the escape: no ssh_escape hop (a build without th |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | branch brt247kvx1yh not on screen after cd into the repo on the remote; shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | branch brt2594lc6mp not on screen after cd into the repo on the remote; shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | branch brt267tjiek0 not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | branch brt271emk0nz not on screen after cd into the repo on the remote; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.23/5, 1.23/5, 3.27/9; NO-REFERENCE: UNVERIFIABLE: [ |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | branch brt278j9qexf not on screen after cd into the repo on the remote; shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | branch brt282l4199o not on screen after cd into the repo on the remote; shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | branch brt28611xo29 not on screen after cd into the repo on the remote; shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | branch brt290lddx3k not on screen after cd into the repo on the remote; shell arm: 3 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | branch brt294wj3m84 not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | branch brt302t5dynd not on screen after cd into the repo on the remote; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | branch brt306n1a3u0 not on screen after cd into the repo on the remote; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | PASS | injector accounting: injector sent 80; app saw 80; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 71 unbound keydown/typed, 5 t |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | PASS | injector accounting: injector sent 80; app saw 80; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 71 unbound keydown/typed, 5 t |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t3402mz3fz.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t343asnwxa.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t3462ztlto.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t349vncdbl.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/ |
| terminal | lab_tab_remote_dir.bash.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 14 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; SETTI |
| terminal | lab_tab_remote_dir.bash.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 165 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
| terminal | lab_tab_remote_dir.zsh.lan.helper | PASS | Tab then Enter 18 ms later: rec.log has no `cd` line for this step, so the received line is judged by the effect file alone; remote Tab wait |
| terminal | lab_tab_remote_dir.zsh.150.helper | PASS | Tab then Enter 21 ms later: rec.log has no `cd` line for this step, so the received line is judged by the effect file alone; remote Tab wait |
| terminal | lab_tab_remote_dir.fish.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 10 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| terminal | lab_tab_remote_dir.fish.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 161 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
| terminal | lab_tab_remote_dir.nu.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 14 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| terminal | lab_tab_remote_dir.nu.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 165 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est408gq0wc $EPOCHREALTIME >> /work/u_zsh/es_t407oxvi.txt', 'echo est41 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.bash.150.helper | FAIL | branch brt712kpa2am not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.fish.lan.helper | FAIL | branch brt722j89hsy not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_fs_basic.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_fs_basic.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.bash.150.helper | FAIL | branch brt738019wrd not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_git_read.zsh.lan.helper | FAIL | branch brt742vyza5d not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.zsh.150.helper | FAIL | branch brt746tbzf07 not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_git_read.fish.lan.helper | FAIL | branch brt750mvefd9 not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.fish.150.helper | FAIL | branch brt754ueqfha not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.nu.150.helper | FAIL | branch brt761nbq00z not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.bash.150.helper | FAIL | branch brt768o26k4o not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.zsh.lan.helper | FAIL | branch brt772lxv3me not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_run_command.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.fish.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.nu.150.helper | FAIL | branch brt788uwcx6v not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt793kkigkt: a |
| terminal | lab_helper_killed.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt796powbvj: app log chip update n |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; app find hook gave no  |
| terminal | lab_helper_killed.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; branch brt802f30dym: app log chip update n |
| agent | lab_tab_remote_dir.bash.lan.nohelper | PASS | remote Tab wait (#2188): the app logged no remote_tab_wait line for this step (a build before #2188, PHOSPHOR_REMOTE_TAB_ENTER_WAIT=0, the r |
| agent | lab_tab_remote_dir.bash.150.nohelper | FAIL | Tab -> insert took 379 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188): the app logged no remote_tab_ |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 31 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter hel |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab -> insert took 386 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.fish.lan.nohelper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 17 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| agent | lab_tab_remote_dir.fish.150.nohelper | FAIL | Tab -> insert took 385 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_tab_remote_dir.nu.lan.nohelper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 25 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | Tab -> insert took 514 ms; the budget is 1 RTT (150 ms) + 100 ms (+50 ms poll slack); remote Tab wait (#2188) logged by the app: released wh |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 1589692); the app is relaunched so later c |
| agent | lab_chips.bash.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 2 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.23/5, 1.23/5; side-call producers (#2156): batched  |
| agent | lab_chips.nu.150.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.64/1, 1.64/1; side-call producers (#2156): batched  |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1488; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 692 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_tab_remote_dir.bash.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 14 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; SETTI |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 1611112 -> 1612311); a relaunched app starts in a terminal session, so thi |
| agent | lab_tab_remote_dir.bash.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 165 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
| agent | lab_tab_remote_dir.zsh.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 16 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| agent | lab_tab_remote_dir.zsh.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 166 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
| agent | lab_tab_remote_dir.fish.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 19 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| agent | lab_tab_remote_dir.fish.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 161 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
| agent | lab_tab_remote_dir.nu.lan.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered waited 14 ms, 0 key(s) held, enter_held=0; 0 of 1 with an Enter held; injec |
| agent | lab_tab_remote_dir.nu.150.helper | PASS | remote Tab wait (#2188) logged by the app: released why=answered_applied waited 164 ms, 1 key(s) held, enter_held=1; 1 of 1 with an Enter he |
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
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=7 run( |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@42 ms, InitShell@82 ms, Bootstrapped@1926 ms; the first inner line ran 1.28 s after it |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.53 ms (p95 2.58, n=60); the wait itself: median 0.0 |
| terminal | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| terminal | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -0.01 +/- 1.18 ms (SE) vs budget 0.79 ms is within noise: INCONCLUSIVE/NO-COMPARE, the over |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); mode guard: the pane was in agent before this termina |
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
| terminal | link_and_path_click | FAIL | the in-app code pane has the keyboard and this platform has no close-pane key (APPCI_KEY_CLOSE empty): the pane was left open and nothing wa |
| terminal | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 73992); the app is relaunched so later cases start clean |
| terminal | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 75; app saw 75; lost |
| terminal | editor_memory_7mb | PASS | judged memory figures: phys_footprint, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); heap prim |
| terminal | tab_pointer_use | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; vertical panel: tab 2 at (468, 413), tab 3 at (468, 538), ta |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with palette cmd+p + 'Close Current Session'; NO-REFER |
| terminal | keybinding_rebind | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; control: Alt-R not played: macOS: Option-R types a character |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead 6.59 +/- 1.51 ms (SE) vs budget 9.05 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT |
| agent | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=7 run( |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@38 ms, InitShell@78 ms, Bootstrapped@1916 ms; the first inner line ran 1.28 s after it |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.49 ms (p95 2.35, n=60); the wait itself: median 0.0 |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: insufficient e |
| agent | latency_first_key_after_idle | FAIL | First key after 1 s idle: 1.98 ms (SE 1.02) is outside its band (history band -2.04--1.26 from n=4 run(s) (median -1.56), PROVISIONAL (n < 5 |
| agent | routing_table | PASS | injector accounting: injector sent 631; app saw 631; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter_ssh | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| agent | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| agent | scroll_keys_position | NOREF | control: the job printed 28 lines and none is on screen at the bottom of the agent view (shots/*skp_0_stream.png): the agent view does not d |
| agent | link_and_path_click | FAIL | the in-app code pane has the keyboard and this platform has no close-pane key (APPCI_KEY_CLOSE empty): the pane was left open and nothing wa |
| agent | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 9094); the app is relaunched so later cases start clean |
| agent | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 74; app saw 74; lost |
| agent | tab_pointer_use | NOREF | NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles could not be located in the strip (the rows are not top to bottom in tab order: |
| agent | keybinding_rebind | PASS | control: Alt-R not played: macOS: Option-R types a character by design unless Option is set to act as Meta (terminal.input.extra_meta_keys); |
| agent@fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead -0.85 +/- 1.0 ms (SE) vs budget 0.88 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT |
| terminal | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=7 run( |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@17 ms, InitShell@40 ms, Bootstrapped@1310 ms; the first inner line ran 0.80 s after it |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #1176 key -> pty write median 0.65 ms (p95 0.72, n=60): within the 1 ms target; #1995 notifies logged before 13 of 77 key frame build(s): In |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: insufficient evidence (Phosphor series drifted +1.74 ms between its halves (sequential A-then-B meas |
| terminal | routing_table | FAIL | 'what is 5 > 3' (ambiguous) expected shell, went to agent (surface PlainTerminal, rule model); shell received []; injector accounting: injec |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 5 t |
| terminal | latency_enter_ssh | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| terminal | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 2316; app saw 2316; lost-before-app 0 (Enter: sent 61, app saw 61); of the app's: 2251 unbound keydown/ty |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | ime_compose | NOREF | injector accounting: injector sent 334; app saw 316; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 303 unbound keydown/typed, |
| terminal | startup_shell_pane | SKIP | Windows only: the pane-shell passes (PowerShell 7, cmd.exe, WSL chosen through the app's own shell settings) exist on the Windows runner; no |
| terminal | link_and_path_click | PASS | the path went to the desktop opener (system): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expect |
| terminal | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| terminal | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | editor_memory_7mb | FAIL | trim A/B: the close trim put a 9.4 ms UI item on screen while it ran (budget 4 ms; control no_trim: 0.0 ms); heap prime (#2185): one unmeasu |
| terminal | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows (OCR): tab 2 at (126, 156), tab 3 at (126, 219), tab 4 at (126, |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (125, 155), tab 3 at (125, 218), tab 4 at (125, 280); NO-REFERENCE: UNVERIFIABLE: horizontal bar: the tabs' titles  |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| terminal | keybinding_rebind | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; injector accounting: injector sent 182; app saw 182; lost-be |
| terminal | gpu_lost_device_recovers | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; device reported lost once; renderer rebuilt on Intel(R) UHD  |
| terminal | upgrade_over_previous | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; the previous build's state is packed in /home/<user>/builds/ |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=7 run( |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@16 ms, InitShell@41 ms, Bootstrapped@1312 ms; the first inner line ran 0.80 s after it |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #1176 key -> pty write median 0.65 ms (p95 0.71, n=60): within the 1 ms target; #1995 notifies logged before 14 of 77 key frame build(s): In |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | NOREF | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| agent | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
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
| agent | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 17 frame(s) drawn after the rebuild; 5 frame(s) |
| agent@fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |

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
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 6.31, -> LineEditorReading 27.11; app: |
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
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.07 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal | latency_enter | PASS | app share of Enter->start: enter_begin -> pty_write median 1.86 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | routing_table_powershell | PASS | injector accounting: injector sent 861; app saw 861; lost-before-app 0 (Enter: sent 31, app saw 31); of the app's: 752 unbound keydown/typed |
| terminal | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| terminal | intl_wide_type | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; typed cjk: rec.log empty (hook not run): the command line |
| terminal | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| terminal | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| terminal | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| terminal | intl_wide_edit | PASS | edit 0: the PowerShell history already holds 1 line(s) typed with this seed's lead ('Write-Output k0t240_d3jx10d1c6A日本Z語 / ForEach-Object {  |
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
| terminal | editor_memory_7mb | NOREF | judged memory figures: private, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); baselines taken  |
| terminal | restore_shows_history | SKIP | not run in the PowerShell pane: r14 T20: the token is printed by bash `printf '%s%s\n' A B` so the command text never holds it, and rec.log  |
| terminal | ui_vertical_tabs_many | SKIP | not run in the PowerShell pane: T33 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| terminal | ui_vertical_tabs_panel_click | SKIP | not run in the PowerShell pane: T31 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| terminal | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal | network_gate_switch | SKIP | not run in the PowerShell pane: the gate does not depend on the pane's shell (three relaunches): runs once, in the Git Bash pane (#2077) |
| terminal | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal | keybinding_rebind | NOREF | NO-REFERENCE: UNVERIFIABLE: this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is ju |
| terminal | soak | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; soak: window medians [454, 475, 466] MB, monotonic=True, gro |
| terminal | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 35.0 frames/s for 31.3 wakeups/s (0 Full); 4 hidden tabs streaming 2.3 frames/s, 124.5 hidden  |
| terminal | tabs_two_windows | PASS | injector accounting: injector sent 3458; app saw 3458; lost-before-app 0 (Enter: sent 49, app saw 49); of the app's: 3354 unbound keydown/ty |
| terminal | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal | history_survives_restart | FAIL | after the quit the shell's history file holds a typed command more than once: {'hsvjqyq1': 3, 'hsvjqyq2': 3, 'hsvjqyq3': 3}; after the kill  |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 2691; app saw 2691; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2608 unbound keydown/ty |
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
| terminal@gitbash | subshell_ready_rc_clear | PASS | clear: setup armed decision=Wait(Unknown); no setup progress event logged; the first inner line ran 0.05 s after its Enter (its history band |
| terminal@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| terminal@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal@gitbash | ssh_connect_typing_wrapper | FAIL | ssh_connect_typing (gap 0.1 s): T248_c24u8x never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 276; app saw 276; lost-before-app 0 (Enter: sent 10, app saw 10); of the app |
| terminal@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.23 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 43.8 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | FAIL | key to frame: 4.37 ms (SE None) is outside its band (history band 3.21-4.35 from n=8 run(s) (median 3.67); above 4.35) [windows/terminal@git |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal@gitbash | latency_first_key_after_idle | NOREF | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| terminal@gitbash | routing_table | FAIL | 'ls -la' (shell) was LOST or unproven: the shell received nothing and the app log shows no agent-routed Enter (dest lost, surface PlainTermi |
| terminal@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| terminal@gitbash | remote_session_legacy | FAIL | [legacy] no local prompt after `exit` from the remote shell; [legacy] local rec.log lacks the exact command typed after exit; [legacy] secon |
| terminal@gitbash | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| terminal@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| terminal@gitbash | remote_tab_no_helper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote d |
| terminal@gitbash | pane_needed_ctrl_c_after_remote_tab_no_helper | PASS | the first marker command after remote_tab_no_helper had no effect; it worked after Ctrl-C x3 (candidate APP finding: input focus/Enter routi |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: 20:11:58 console host start-up request 1b5b3f3930303168 dropped by the test ss |
| terminal@gitbash | pane_switch_left_on_after_remote_tab_enter_no_helper | FAIL | after remote_tab_enter_no_helper: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: the proof li |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| terminal@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| terminal@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| terminal@gitbash | ssh_gitbash_managed | PASS | injector accounting: injector sent 392; app saw 392; lost-before-app 0 (Enter: sent 11, app saw 11); of the app's: 384 unbound keydown/typed |
| terminal@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.28 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | ssh_gitbash_bootstrap_in_command | PASS | the test sshd logged no client-input count for this session (only the Windows fake sshd does); injector accounting: injector sent 250; app s |
| terminal@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: the line typed during the connect never reached the remote shell. Test host: sshd.log: 20:26:35 console host start-up request 1b5b3 |
| terminal@gitbash | pane_not_local_after_ssh_gitbash_typeahead_managed | FAIL | after ssh_gitbash_typeahead_managed the pane could not be proven back in the local shell (remote); the app is relaunched so later cases star |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 391; app saw 391; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 379 unbound keydown/typed |
| terminal@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); control: no command row was readable in the  |
| terminal@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| terminal@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 5.0 ms (< 16 ms; before: 0.0 ms); injector accounting: injector sent 742; app saw 742; lost-before |
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
| terminal@gitbash | selection_modes | PASS | injector accounting: injector sent 300; app saw 300; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 275 unbound keydown/typed,  |
| terminal@gitbash | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal@gitbash | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal@gitbash | startup_shell_pane | SKIP | runs in the pane-shell passes only (terminal@pwsh7, terminal@cmd, terminal@wsl: APPCI_WIN_SHELLS of the app-test-windows job names them; a p |
| terminal@gitbash | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal@gitbash | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal@gitbash | resize_reflow_output | NOREF | NO-REFERENCE: UNVERIFIABLE: the resize case was not run: NEEDS: a desktop of at least 1300x800 px on the Windows VM (it is 1280x800): the wi |
| terminal@gitbash | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal@gitbash | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| terminal@gitbash | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8480; app saw 8480; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8248 unbound keydown/ |
| terminal@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal@gitbash | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| terminal@gitbash | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal@gitbash | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal@gitbash | editor_memory_7mb | NOREF | judged memory figures: private, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); baselines taken  |
| terminal@gitbash | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| terminal@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| terminal@gitbash | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows located from the app's find hook (the newest sighting in each t |
| terminal@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal@gitbash | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal@gitbash | keybinding_rebind | PASS | injector accounting: injector sent 848; app saw 848; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 785 unbound keydown/typed |
| terminal@gitbash | soak | PASS | soak: window medians [426, 445, 442] MB, monotonic=True, growth +15 MB (limit 150 MB); injector accounting: injector sent 3992; app saw 3992 |
| terminal@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 11 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams we |
| terminal@gitbash | tabs_two_windows | PASS | injector accounting: injector sent 3271; app saw 3271; lost-before-app 0 (Enter: sent 48, app saw 48); of the app's: 3170 unbound keydown/ty |
| terminal@gitbash | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal@gitbash | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| terminal@gitbash | tabs_streaming | PASS | injector accounting: injector sent 2551; app saw 2551; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2468 unbound keydown/ty |
| terminal@gitbash | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| terminal@pwsh7 | mode_proof | FAIL | terminal mode expected but the screen shows agent markers ('New agent conversation' / 'ESC for terminal' / 'Ask anything'); the app log says |
| terminal@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@pwsh7 | fast_typing | PASS | injector accounting: injector sent 2247; app saw 2247; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2185 unbound keydown/ty |
| terminal@pwsh7 | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal@pwsh7 | tab_enter | PASS | injector accounting: injector sent 674; app saw 674; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 648 unbound keydown/typed |
| terminal@pwsh7 | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 7.3, -> LineEditorReading 30.1; app: l |
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
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 7.1, -> LineEditorReading 36.84; app:  |
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
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.09 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 4.41 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| agent | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
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
| agent | intl_wide_edit | PASS | edit 0: the PowerShell history already holds 1 line(s) typed with this seed's lead ('Write-Output k0t184_kr0f10d1c6A日本Z語 / ForEach-Object {  |
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
| agent | soak | PASS | soak: window medians [503, 522, 509] MB, monotonic=False, growth +6 MB (limit 150 MB); injector accounting: injector sent 4414; app saw 4414 |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 34.7 frames/s for 31.0 wakeups/s (0 Full); 4 hidden tabs streaming 2.2 frames/s, 124.0 hidden  |
| agent | tabs_two_windows | PASS | injector accounting: injector sent 3458; app saw 3458; lost-before-app 0 (Enter: sent 49, app saw 49); of the app's: 3354 unbound keydown/ty |
| agent | history_survives_restart | FAIL | after the quit the shell's history file holds a typed command more than once: {'hsb9gxq1': 3, 'hsb9gxq2': 3, 'hsb9gxq3': 3}; after the kill  |
| agent | tabs_streaming | PASS | injector accounting: injector sent 2690; app saw 2690; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2608 unbound keydown/ty |
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
| agent@gitbash | subshell_ready_rc_clear | PASS | clear: setup armed decision=Wait(Unknown); no setup progress event logged; the first inner line ran 0.05 s after its Enter (its history band |
| agent@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| agent@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent@gitbash | ssh_connect_typing_wrapper | FAIL | ssh_connect_typing (gap 0.1 s): T163_2xcbu0 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| agent@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 501; app saw 501; lost-before-app 0 (Enter: sent 16, app saw 16); of the app |
| agent@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.42 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent@gitbash | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 51.0 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| agent@gitbash | latency_key_log | FAIL | key to frame: 4.57 ms (SE None) is outside its band (history band 3.46-4.45 from n=3 run(s) (median 3.52), PROVISIONAL (n < 5); above 4.45)  |
| agent@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent@gitbash | latency_first_key_after_idle | PASS | NOTE (not judged; the band decides): First key after 1 s idle: median 4.64 vs baseline build +1.14 ms (+32.6%); First key after 1 s idle: ke |
| agent@gitbash | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent@gitbash | routing_table | FAIL | 'ls -la' (shell) was LOST or unproven: the shell received nothing and the app log shows no agent-routed Enter (dest lost, surface AgentConve |
| agent@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| agent@gitbash | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent@gitbash | cb_agent_pickers | FAIL | at picker (@): the picker did not appear: the screen changed by 0.0006 (< 0.0040); injector accounting: injector sent 78; app saw 78; lost-b |
| agent@gitbash | remote_session_legacy | FAIL | [legacy] no local prompt after `exit` from the remote shell; [legacy] local rec.log lacks the exact command typed after exit; [legacy] secon |
| agent@gitbash | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| agent@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| agent@gitbash | agent_view_not_restored_after_remote_session_ext | PASS | remote_session_ext relaunched the app itself (pid 13244 -> 7844); a relaunched app starts in a terminal session, so this is not an app findi |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | remote_tab_no_helper | FAIL | after the completed cd the remote cwd should be .../phxtab_t234_gg1d3j, got []; the app log has no 'candidates from the remote shell' line:  |
| agent@gitbash | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 299; app saw 299; lost-before-app 0 (Enter: sent 13, app saw 13); of the app's: 283 unbound keydown/typed |
| agent@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| agent@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| agent@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| agent@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| agent@gitbash | ssh_gitbash_managed | PASS | injector accounting: injector sent 250; app saw 250; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 240 unbound keydown/typed,  |
| agent@gitbash | ssh_gitbash_failing_not_local_managed | FAIL | HARNESS: the ssh never connected to the hanging listener on 127.0.0.1:53683 within 8 s (no TCP accept): not a test of typing behind a connec |
| agent@gitbash | pane_switch_left_on_after_ssh_gitbash_failing_not_local_managed | FAIL | after ssh_gitbash_failing_not_local_managed: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: t |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_bootstrap_in_command | PASS | the test sshd logged no client-input count for this session (only the Windows fake sshd does); injector accounting: injector sent 392; app s |
| agent@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 2: `exit` did not bring the local prompt back; trial 3: the line typed during the connect never reached the remote shell. Test host: s |
| agent@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 391; app saw 391; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 379 unbound keydown/typed |
| agent@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); control: no command row was readable in the  |
| agent@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| agent@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 6.2 ms (< 16 ms; before: 0.0 ms); injector accounting: injector sent 742; app saw 742; lost-before |
| agent@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NOTE (not judged |
| agent@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent@gitbash | history_up_recall | PASS | injector accounting: injector sent 138; app saw 138; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 118 unbound keydown/typed,  |
| agent@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent@gitbash | scroll_keys_position | FAIL | after PageUp, 3 s of new output: after the wait 1 of the job's new lines are on screen: the view followed the output to the bottom (shots/*s |
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
| agent@gitbash | gitbash_typeahead_reps | FAIL | tr13: file has 5/10 lines, exact match False first diff: exp='t513_lteqkztl3z' got=None [first bad line 5: injector call #10463 posted its E |
| agent@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| agent@gitbash | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| agent@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| agent@gitbash | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| agent@gitbash | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| agent@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| agent@gitbash | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: UNVERIFIABLE: the panel rows of tabs 2..4 could not be locat |
| agent@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| agent@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent@gitbash | keybinding_rebind | PASS | injector accounting: injector sent 835; app saw 835; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 772 unbound keydown/typed |
| agent@gitbash | soak | PASS | soak: window medians [393, 411, 403] MB, monotonic=False, growth +10 MB (limit 150 MB); injector accounting: injector sent 4020; app saw 402 |
| agent@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 11 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams we |
| agent@gitbash | tabs_two_windows | PASS | no hop=view_mode line in the app log: the mode is read from the screen (OCR): agent; injector accounting: injector sent 3271; app saw 3271;  |
| agent@gitbash | history_survives_restart | PASS | no hop=view_mode line in the app log: the mode is read from the screen (OCR): agent; no quit key on this platform: the app was stopped by pi |
| agent@gitbash | tabs_streaming | PASS | injector accounting: injector sent 2550; app saw 2550; lost-before-app 0 (Enter: sent 38, app saw 38); of the app's: 2468 unbound keydown/ty |
| agent@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| agent@pwsh7 | mode_proof | PASS |  |
| agent@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@pwsh7 | fast_typing | PASS | injector accounting: injector sent 2246; app saw 2246; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2184 unbound keydown/ty |
| agent@pwsh7 | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent@pwsh7 | tab_enter | PASS | injector accounting: injector sent 674; app saw 674; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 648 unbound keydown/typed |
| agent@pwsh7 | ctrl_c_d | NOREF | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 7.35, -> LineEditorReading 35.99; app: |
| agent@pwsh7 | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached th |
| agent@pwsh7 | history_up_recall | PASS | injector accounting: injector sent 146; app saw 146; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 127 unbound keydown/typed,  |
| agent@wsl | startup_shell_pass | SKIP | a WSL distribution is not installed on this machine: the wsl pass cannot run (the harness installs nothing). NEEDS: the Windows Subsystem fo |
| agent@fake | agent_latency_key | SKIP | not run in the PowerShell pane: its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake mode |
| agent@fake | agent_latency_enter | SKIP | not run in the PowerShell pane: its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828) |
| agent@fake | agent_suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: as suggestion_chip_keeps_output (a `cat` confirmed from the bash prompt hook): runs once, in the Git  |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | desktop_notification_agent_question | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; injector accounting: inje |
| agent@fake | agent_close_undo_call_status | SKIP | not run in the PowerShell pane: #2103: the scripted command is a POSIX printf whose output is not in its command line; the agent path does n |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_suggestion_chip_keeps_output | PASS | the suggestion going is not judged in the agent view: Escape leaves the view there, a layout change of its own (the Rust view test a_late_pr |
| agent@gitbash+fake | desktop_notification_agent_question | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; no recording notification service on this platform: the verd |
| agent@gitbash+fake | agent_close_undo_call_status | PASS | the close key asked first (a waiting agent turn): answered with Enter; injector accounting: injector sent 311; app saw 311; lost-before-app  |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_latency_enter | macos | agent@fake | NOREF |
| agent_latency_enter | wayland | agent@fake | NOREF |
| agent_latency_enter | windows | agent@fake | SKIP |
| agent_latency_enter | windows | agent@gitbash+fake | NOREF |
| agent_latency_key | macos | agent@fake | NOREF |
| agent_latency_key | wayland | agent@fake | NOREF |
| agent_latency_key | windows | agent@fake | SKIP |
| agent_latency_key | windows | agent@gitbash+fake | NOREF |
| agent_lrc_no_false_takeover | windows | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent@gitbash | PASS |
| alt_screen_programs | windows | agent | SKIP |
| alt_screen_programs | windows | agent@gitbash | NOREF |
| alt_screen_programs | windows | terminal | SKIP |
| alt_screen_programs | windows | terminal@gitbash | NOREF |
| big_paste | windows | agent | SKIP |
| big_paste | windows | agent@gitbash | NOREF |
| big_paste | windows | terminal | SKIP |
| big_paste | windows | terminal@gitbash | NOREF |
| editor_daily | macos | agent | NOREF |
| editor_daily | macos | terminal | NOREF |
| editor_daily | windows | agent | PASS |
| editor_daily | windows | agent@gitbash | PASS |
| editor_daily | windows | terminal | PASS |
| editor_daily | windows | terminal@gitbash | PASS |
| editor_memory_7mb | macos | terminal | PASS |
| editor_memory_7mb | wayland | terminal | FAIL |
| editor_memory_7mb | windows | terminal | NOREF |
| editor_memory_7mb | windows | terminal@gitbash | NOREF |
| flood_scrollback_find | windows | agent | SKIP |
| flood_scrollback_find | windows | agent@gitbash | SKIP |
| flood_scrollback_find | windows | terminal | SKIP |
| flood_scrollback_find | windows | terminal@gitbash | SKIP |
| focus_cycle | windows | agent | PASS |
| focus_cycle | windows | agent@gitbash | PASS |
| focus_cycle | windows | terminal | PASS |
| focus_cycle | windows | terminal@gitbash | PASS |
| gitbash_typeahead_reps | windows | agent | SKIP |
| gitbash_typeahead_reps | windows | agent@gitbash | FAIL |
| gitbash_typeahead_reps | windows | terminal | SKIP |
| gitbash_typeahead_reps | windows | terminal@gitbash | PASS |
| gpu_real_render | wayland | agent | NOREF |
| gpu_real_render | wayland | terminal | NOREF |
| history_survives_restart | windows | agent | FAIL |
| history_survives_restart | windows | agent@gitbash | PASS |
| history_survives_restart | windows | terminal | FAIL |
| history_survives_restart | windows | terminal@gitbash | PASS |
| history_up_recall | macos | agent | PASS |
| history_up_recall | macos | terminal | PASS |
| history_up_recall | wayland | agent | PASS |
| history_up_recall | wayland | terminal | PASS |
| history_up_recall | windows | agent | PASS |
| history_up_recall | windows | agent@gitbash | PASS |
| history_up_recall | windows | agent@pwsh7 | PASS |
| history_up_recall | windows | terminal | PASS |
| history_up_recall | windows | terminal@gitbash | PASS |
| history_up_recall | windows | terminal@pwsh7 | PASS |
| hostile_output_recovers | windows | agent | SKIP |
| hostile_output_recovers | windows | agent@gitbash | PASS |
| hostile_output_recovers | windows | terminal | SKIP |
| hostile_output_recovers | windows | terminal@gitbash | PASS |
| job_control | windows | agent | SKIP |
| job_control | windows | agent@gitbash | SKIP |
| job_control | windows | terminal | SKIP |
| job_control | windows | terminal@gitbash | SKIP |
| keybinding_rebind | macos | agent | PASS |
| keybinding_rebind | macos | terminal | PASS |
| keybinding_rebind | wayland | agent | PASS |
| keybinding_rebind | wayland | terminal | PASS |
| keybinding_rebind | windows | agent | NOREF |
| keybinding_rebind | windows | agent@gitbash | PASS |
| keybinding_rebind | windows | terminal | NOREF |
| keybinding_rebind | windows | terminal@gitbash | PASS |
| link_and_path_click | macos | agent | FAIL |
| link_and_path_click | macos | terminal | FAIL |
| link_and_path_click | wayland | agent | PASS |
| link_and_path_click | wayland | terminal | PASS |
| link_and_path_click | windows | agent | SKIP |
| link_and_path_click | windows | agent@gitbash | SKIP |
| link_and_path_click | windows | terminal | SKIP |
| link_and_path_click | windows | terminal@gitbash | SKIP |
| mac_pointer_chord_proof | windows | agent | SKIP |
| mac_pointer_chord_proof | windows | agent@gitbash | SKIP |
| mac_pointer_chord_proof | windows | terminal | SKIP |
| mac_pointer_chord_proof | windows | terminal@gitbash | SKIP |
| meta_and_word_keys | windows | agent | PASS |
| meta_and_word_keys | windows | agent@gitbash | PASS |
| meta_and_word_keys | windows | terminal | PASS |
| meta_and_word_keys | windows | terminal@gitbash | PASS |
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
| primary_and_middle_click | windows | agent | SKIP |
| primary_and_middle_click | windows | agent@gitbash | SKIP |
| primary_and_middle_click | windows | terminal | SKIP |
| primary_and_middle_click | windows | terminal@gitbash | SKIP |
| remote_session_legacy | windows | agent | SKIP |
| remote_session_legacy | windows | agent@gitbash | FAIL |
| remote_session_legacy | windows | terminal | SKIP |
| remote_session_legacy | windows | terminal@gitbash | FAIL |
| remote_tab_enter_no_helper | macos | agent | PASS |
| remote_tab_enter_no_helper | macos | terminal | PASS |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | PASS |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | agent@gitbash | PASS |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | FAIL |
| remote_tab_no_helper | macos | agent | PASS |
| remote_tab_no_helper | macos | terminal | PASS |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | PASS |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| resize_reflow_output | windows | agent | SKIP |
| resize_reflow_output | windows | agent@gitbash | NOREF |
| resize_reflow_output | windows | terminal | SKIP |
| resize_reflow_output | windows | terminal@gitbash | NOREF |
| restore_shows_history | windows | agent | SKIP |
| restore_shows_history | windows | agent@gitbash | PASS |
| restore_shows_history | windows | terminal | SKIP |
| restore_shows_history | windows | terminal@gitbash | PASS |
| routing_table_powershell | macos | agent | SKIP |
| routing_table_powershell | macos | terminal | SKIP |
| routing_table_powershell | wayland | agent | SKIP |
| routing_table_powershell | wayland | terminal | SKIP |
| routing_table_powershell | windows | agent | PASS |
| routing_table_powershell | windows | agent@gitbash | SKIP |
| routing_table_powershell | windows | agent@pwsh7 | PASS |
| routing_table_powershell | windows | terminal | PASS |
| routing_table_powershell | windows | terminal@gitbash | SKIP |
| routing_table_powershell | windows | terminal@pwsh7 | PASS |
| scroll_keys_foreground | windows | agent | SKIP |
| scroll_keys_foreground | windows | agent@gitbash | PASS |
| scroll_keys_foreground | windows | terminal | SKIP |
| scroll_keys_foreground | windows | terminal@gitbash | PASS |
| scroll_keys_position | macos | agent | NOREF |
| scroll_keys_position | macos | terminal | PASS |
| scroll_keys_position | wayland | agent | NOREF |
| scroll_keys_position | wayland | terminal | PASS |
| scroll_keys_position | windows | agent | SKIP |
| scroll_keys_position | windows | agent@gitbash | FAIL |
| scroll_keys_position | windows | terminal | SKIP |
| scroll_keys_position | windows | terminal@gitbash | PASS |
| selection_modes | windows | agent | SKIP |
| selection_modes | windows | agent@gitbash | PASS |
| selection_modes | windows | terminal | SKIP |
| selection_modes | windows | terminal@gitbash | PASS |
| soak | windows | agent | PASS |
| soak | windows | agent@gitbash | PASS |
| soak | windows | terminal | PASS |
| soak | windows | terminal@gitbash | PASS |
| ssh_connect_typing_wrapper | windows | agent | SKIP |
| ssh_connect_typing_wrapper | windows | agent@gitbash | FAIL |
| ssh_connect_typing_wrapper | windows | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | FAIL |
| ssh_gitbash_bootstrap_in_command | macos | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | macos | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | wayland | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | wayland | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | agent@gitbash | PASS |
| ssh_gitbash_bootstrap_in_command | windows | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | terminal@gitbash | PASS |
| ssh_gitbash_failing_not_local_managed | windows | agent | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_failing_not_local_managed | windows | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | terminal@gitbash | PASS |
| ssh_gitbash_kill_switch | macos | agent | SKIP |
| ssh_gitbash_kill_switch | macos | terminal | SKIP |
| ssh_gitbash_kill_switch | wayland | agent | SKIP |
| ssh_gitbash_kill_switch | wayland | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | agent | SKIP |
| ssh_gitbash_kill_switch | windows | agent@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | terminal@gitbash | PASS |
| ssh_gitbash_managed | macos | agent | SKIP |
| ssh_gitbash_managed | macos | terminal | SKIP |
| ssh_gitbash_managed | wayland | agent | SKIP |
| ssh_gitbash_managed | wayland | terminal | SKIP |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | agent@gitbash | PASS |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | PASS |
| ssh_gitbash_typeahead_managed | windows | agent | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal@gitbash | FAIL |
| ssh_nested_typeahead_wrapper | windows | agent | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent@gitbash | PASS |
| ssh_nested_typeahead_wrapper | windows | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | PASS |
| startup_shell_pane | macos | terminal | SKIP |
| startup_shell_pane | wayland | terminal | SKIP |
| startup_shell_pane | windows | terminal | SKIP |
| startup_shell_pane | windows | terminal@gitbash | SKIP |
| startup_shell_pane | windows | terminal@pwsh7 | PASS |
| subshell_line_editor_proof | windows | agent | SKIP |
| subshell_line_editor_proof | windows | agent@gitbash | SKIP |
| subshell_line_editor_proof | windows | terminal | SKIP |
| subshell_line_editor_proof | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | agent@gitbash | PASS |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | agent@gitbash | PASS |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | PASS |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | agent | SKIP |
| subshell_ready_secret_after_probe | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | terminal | SKIP |
| subshell_ready_secret_after_probe | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | agent | SKIP |
| subshell_ready_secret_after_prompt | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
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
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
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
| wheel_scroll | windows | agent | SKIP |
| wheel_scroll | windows | agent@gitbash | SKIP |
| wheel_scroll | windows | terminal | SKIP |
| wheel_scroll | windows | terminal@gitbash | SKIP |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | PASS |

passed everywhere it ran (6, candidates for the owner's decision): agent_lrc_no_false_takeover, focus_cycle, history_up_recall, meta_and_word_keys, soak, tab_lifecycle_keys
not passed somewhere (fail / skip / no-reference): agent_latency_enter, agent_latency_key, alt_screen_programs, big_paste, editor_daily, editor_memory_7mb, flood_scrollback_find, gitbash_typeahead_reps, gpu_real_render, history_survives_restart, hostile_output_recovers, job_control, keybinding_rebind, link_and_path_click, mac_pointer_chord_proof, minimize_fullscreen, network_gate_switch, output_frames, primary_and_middle_click, remote_session_legacy, remote_tab_enter_no_helper, remote_tab_no_helper, resize_reflow_output, restore_shows_history, routing_table_powershell, scroll_keys_foreground, scroll_keys_position, selection_modes, ssh_connect_typing_wrapper, ssh_gitbash_bootstrap_in_command, ssh_gitbash_failing_not_local_managed, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, startup_shell_pane, subshell_line_editor_proof, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, subshell_sh_probe_empty_ps1, tab_pointer_use, ui_completions_as_you_type_local, ui_copy_wrapped_drag, ui_copy_wrapped_output, ui_vertical_tabs_many, ui_vertical_tabs_panel_click, wheel_scroll, zero_network_startup
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run/junit.xml: 3162 KB skipped (over 1 MB)
- note: macos/run/results.json: 3663 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.4.log.gz: 1742 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794542926.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794542926.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794542926.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794542926.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 4948 KB skipped (over 1 MB)
- note: wayland/run/results.jsonl: 4938 KB skipped (over 1 MB)
- note: wayland/run/junit.xml: 4421 KB skipped (over 1 MB)
- note: wayland/run/results.json: 4982 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.4.log.gz: 1374 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794540764.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794540764.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794540764.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4326 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 170 KB (1030 of 48806 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent@gitbash+fake/case_agent_latency_enter.log: latency evidence capped at 170 KB (1086 of 33888 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent@gitbash+fake/case_agent_latency_key.log: latency evidence capped at 170 KB (1198 of 4790 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_enter.log: latency evidence capped at 170 KB (1142 of 14021 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_enter_ssh.log: latency evidence capped at 170 KB (1004 of 22082 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_first_key_after_idle.log: latency evidence capped at 170 KB (1048 of 4260 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_key_log.log: latency evidence capped at 170 KB (1021 of 2466 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 170 KB (1115 of 3864 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter.log: latency evidence capped at 170 KB (1127 of 13230 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter_ssh.log: latency evidence capped at 170 KB (1011 of 22133 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_first_key_after_idle.log: latency evidence capped at 170 KB (1042 of 6880 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_key_log.log: latency evidence capped at 170 KB (1021 of 2458 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: windows/run_slim/logs/agent/acct/launch0_1794549752.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794549752.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794549752.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794549752.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794549752.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794550889.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794550889.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794551439.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794553688.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794553688.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794553688.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794555499.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794555499.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794555499.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794555935.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794556898.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794558077.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@pwsh7/phosphor.log.gz: 1989 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794538660.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794538660.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794538660.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794540429.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794538660.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794540429.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794538660.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794541135.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794543056.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794543056.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794543056.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794545225.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794545225.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794545225.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794547312.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@pwsh7/phosphor.log.gz: 2188 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 192 KB (1189 of 4392 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 192 KB (1201 of 3723 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 192 KB (1201 of 1530 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 192 KB (1228 of 37129 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 192 KB (1312 of 5792 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 192 KB (1169 of 4377 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter_ssh.log: latency evidence capped at 192 KB (1192 of 3837 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 192 KB (1200 of 1534 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/acct/launch0_1794540293.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794540293.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794540293.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794540293.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794540293.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent@fake/phosphor.log.gz: 1572 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794536936.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794536936.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794536936.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794536936.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794536936.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 204 KB (1372 of 15922 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1585 of 16205 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1295 of 4394 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 204 KB (1254 of 1283 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 204 KB (1324 of 35614 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 204 KB (1671 of 12001 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 204 KB (1359 of 15132 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1297 of 1867 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1299 of 4436 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 204 KB (1256 of 1290 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/acct/launch0_1794538256.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794538256.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794538256.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794535865.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794535865.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794535865.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: run dir is 11.7 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::on [test-windows]`
  - `warp terminal::input::tests::enter_submits_when_submit_on_ctrl_enter_is_false [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on [test-windows]`
  - `warp terminal::input::tests::test_slow_clipboard_read_does_not_block_and_keys_follow_the_paste [test-windows]`
  - `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_remote_dir.bash.150.nohelper (case not in previous run)`
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
  - `lab-wayland/terminal/lab_ssh_drop_reconnect.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.bash.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.fish.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_run_command.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_enter_start.zsh.150.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.lan.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.150.helper`
  - `lab-wayland/terminal/lab_helper_reconnect.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_helper_reconnect.zsh.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.bash.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.fish.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.fish.150.helper (case not in previous run)`
  - ... and 42 more
- FIXED failures (app): 
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.nu.150.nohelper`
  - `lab-wayland/agent/lab_ctrl_c_no_esc_i.nu.150.nohelper`
  - `windows/terminal/fast_typing`
  - `windows/terminal@gitbash/ssh_connect_typing`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
  - `windows/terminal@gitbash/ssh_gitbash_managed`
  - `windows/agent@gitbash/ssh_connect_typing`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/ssh_nested_typeahead`
  - `windows/agent@gitbash/remote_tab_enter_no_helper`
  - `windows/agent@gitbash/ssh_gitbash_managed`
- Latency regressions > 5%: 
  - `windows/terminal@gitbash/latency_enter enter_to_start: 93.61 -> 121.37 ms (+29.7%)`
  - `windows/terminal@gitbash/latency_key_log key_to_frame: 3.70 -> 4.37 ms (+18.1%)`
- compared against: test-linux: build/r27b (3061436969); test-macos: build/r27b (3061436969); test-warpui-core: build/r27b (3061436969); test-warpui-core-macos: build/r27b (3061436969); test-warpui-core-windows: build/r27a (3060739487); test-windows: build/r27a (3060739487); test-workspace-rest: build/r27b (3061436969); test-workspace-rest-macos: build/r27b (3061436969); test-workspace-rest-windows: build/r27a (3060739487); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998); app windows: build/r13a (3051722998)
