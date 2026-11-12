# build/r27a  aedd52495  (pipeline 3060739487)

- date: 2026-11-12, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on linux-wayland: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `drag_drop_file`, `editor_memory_7mb`, `gpu_lost_device_recovers`, `gpu_real_render`, `history_up_recall`, `ime_compose`, `intl_compose_linux`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `network_gate_switch`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_table`, `routing_table_powershell`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`
  - asked for on macos: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `editor_daily`, `editor_memory_7mb`, `history_up_recall`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `network_gate_switch`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_table`, `routing_table_powershell`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `tab_pointer_use`, `ui_completions_as_you_type_local`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_freeze_and_wake.bash.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on windows: `agent_latency_enter`, `agent_latency_key`, `ctrl_c_d`, `editor_memory_7mb`, `history_up_recall`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `link_and_path_click`, `md_viewer`, `mode_proof`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `routing_table`, `routing_table_powershell`, `scroll_keys_position`, `ssh_drop_reconnect`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `startup_shell_pane`, `subshell_ready_rc_clear`, `subshell_sh_probe_empty_ps1`, `ui_completions_as_you_type_local`, `cb_ui_action_latency`, `gitbash_subshell`, `intl_altgr_bash`, `tabs_streaming`, `tabs_two_windows`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `mac_pointer_chord_proof`, `md_viewer_ai_answer`, `md_viewer_big`, `minimize_fullscreen`, `network_gate_switch`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `wheel_scroll`, `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `cb_agent_pickers`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_survives_restart`, `hostile_output_recovers`, `intl_altgr_ps`, `job_control`, `md_viewer_first_frame`, `meta_and_word_keys`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `restore_shows_history`, `scroll_keys_foreground`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
- unit: 49980 passed, 1 failed, 191 skipped of 49981 run (9 suites)
- app (pass/fail/skip): lab-wayland 0/2/0; macos 25/5/9 +15 no-ref; wayland 28/2/11 +24 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | canceled | 0.4 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | canceled | - | - | no results artifact (job canceled) |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | failed | 77.8 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 110.7 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.5 | - |  |
| build-macos-arm64 | success | 18.9 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 8.4 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 8.4 | - |  |
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
| sshlab-image | success | 0.4 | - |  |
| test-linux | success | 22.2 | - |  |
| test-macos | success | 29.0 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.0 | phosphor-windows-vm |  |
| test-windows | failed | 44.2 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.6 | - |  |
| test-workspace-rest-macos | success | 3.5 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 6.9 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_question::off` LEAK (50.4s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on` LEAK (50.4s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_slow_clipboard_read_does_not_block_and_keys_follow_the_paste` FAIL (0.4s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (1.9s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.3s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.7s)
- `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it` LEAK (1.8s)

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | case_selection | FAIL | unknown case name(s) lab_tab_remote_dir, lab_ssh_drop_reconnect, lab_chips, lab_ctrl_c_no_esc_i (not in the registry: renamed or misspelled? |
| agent | case_selection | FAIL | unknown case name(s) lab_tab_remote_dir, lab_ssh_drop_reconnect, lab_chips, lab_ctrl_c_no_esc_i (not in the registry: renamed or misspelled? |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=6 run( |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@45 ms, InitShell@85 ms, Bootstrapped@1930 ms; the first inner line ran 1.22 s after it |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.45 ms (p95 1.76, n=60); the wait itself: median 0.0 |
| terminal | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| terminal | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -0.99 +/- 1.14 ms (SE) vs budget 1.01 ms is within noise: INCONCLUSIVE/NO-COMPARE, the over |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); the app log says agent but the screen reads as termin |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 3 t |
| terminal | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 2316; app saw 2316; lost-before-app 0 (Enter: sent 61, app saw 61); of the app's: 2251 unbound keydown/ty |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | startup_shell_pane | SKIP | Windows only: the pane-shell passes (PowerShell 7, cmd.exe, WSL chosen through the app's own shell settings) exist on the Windows runner; no |
| terminal | link_and_path_click | FAIL | harness exception: PtrError('pointer injector failed: mptr exit 4 (refused a click at (594.0, 634.0): the topmost window there belongs to No |
| terminal | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 75; app saw 75; lost |
| terminal | editor_memory_7mb | NOREF | judged memory figures: phys_footprint, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); heap prim |
| terminal | tab_pointer_use | FAIL | harness exception: PtrError('pointer injector failed: mptr exit 4 (refused a click at (234.0, 269.0): the topmost window there belongs to No |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with palette cmd+p + 'Close Current Session'; NO-REFER |
| terminal | keybinding_rebind | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; control: Alt-R not played: macOS: Option-R types a character |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead 8.1 +/- 1.04 ms (SE) vs budget 9.05 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT  |
| agent | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=6 run( |
| agent | subshell_ready_rc_clear | FAIL | the first inner line ran 1.32 s after its Enter (its history band replaces the fixed 2 s budget): 1.32 is outside its band (history band 0.8 |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #2037 key to frame without the wait for the display pass (the app's own share): median 1.50 ms (p95 2.04, n=60); the wait itself: median 0.0 |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: overhead 5.39  |
| agent | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -2.04 +/- 1.34 ms (SE) vs budget -1.22 ms is within noise: INCONCLUSIVE/NO-COMPARE, the ove |
| agent | routing_table | PASS | injector accounting: injector sent 632; app saw 632; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| agent | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| agent | scroll_keys_position | NOREF | control: the job printed 28 lines and none is on screen at the bottom of the agent view (shots/*skp_0_stream.png): the agent view does not d |
| agent | link_and_path_click | FAIL | after_links: file has 0/4 lines, exact match False first diff: exp='t43_0eyjivhye9' got=None [first bad line 0: injector call #3725 posted i |
| agent | pane_not_local_after_link_and_path_click | FAIL | after link_and_path_click the pane's shell was not the root shell (none; root pid 67137); the app is relaunched so later cases start clean |
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
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=6 run( |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@16 ms, InitShell@40 ms, Bootstrapped@1308 ms; the first inner line ran 0.80 s after it |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | PASS | #1176 key -> pty write median 0.65 ms (p95 0.71, n=60): within the 1 ms target; #1995 notifies logged before 13 of 76 key frame build(s): In |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -0.05 +/- 0.29 ms (SE) vs budget 0.25 ms is within noise: INCONCLUSIVE/NO-COMPARE, the over |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); mode guard: the pane was in agent before this termina |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 5 t |
| terminal | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| terminal | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | ssh_drop_reconnect | FAIL | [escape_after_cleared_line] the erased line (or the escape) ran a command: ['ssh fake']; injector accounting: injector sent 2316; app saw 23 |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | ime_compose | NOREF | injector accounting: injector sent 334; app saw 316; lost-before-app 18 (Enter: sent 6, app saw 6); of the app's: 303 unbound keydown/typed, |
| terminal | startup_shell_pane | SKIP | Windows only: the pane-shell passes (PowerShell 7, cmd.exe, WSL chosen through the app's own shell settings) exist on the Windows runner; no |
| terminal | link_and_path_click | PASS | the path went to the desktop opener (system): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expect |
| terminal | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| terminal | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | editor_memory_7mb | NOREF | heap prime (#2185): one unmeasured open+close of the txt file first; RSS before 320 MB, after open 443 MB, after the close trim 300 MB (clos |
| terminal | ui_vertical_tabs_panel_click | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; vertical tabs: on in the profile's settings (the owner's lay |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (125, 155), tab 3 at (125, 218), tab 4 at (125, 280); NO-REFERENCE: UNVERIFIABLE: horizontal bar: the tabs' titles  |
| terminal | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| terminal | keybinding_rebind | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; injector accounting: injector sent 182; app saw 182; lost-be |
| terminal | gpu_lost_device_recovers | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; device reported lost once; renderer rebuilt on Intel(R) UHD  |
| terminal | upgrade_over_previous | NOREF | the app log says terminal but the screen reads as agent (OCR); the log decides; the previous build's state is packed in /home/<user>/builds/ |
| agent | mode_proof | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | subshell_sh_probe_empty_ps1 | PASS | the first inner line ran 0.00 s after its Enter (owner 2026-11-12: its history band replaces the fixed 2 s budget): band 0-0.3 from n=6 run( |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@15 ms, InitShell@55 ms, Bootstrapped@1328 ms; the first inner line ran 0.85 s after it |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | PASS | #1176 key -> pty write median 0.66 ms (p95 0.73, n=60): within the 1 ms target; #1995 notifies logged before 13 of 77 key frame build(s): In |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -0.04 +/- 0.17 ms (SE) vs budget 0.26 ms is within noise: INCONCLUSIVE/NO-COMPARE, the over |
| agent | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| agent | left_agent_mode_after_latency_enter_ssh | FAIL | after latency_enter_ssh the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases  |
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
| agent | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 17 frame(s) drawn after the rebuild; 6 frame(s) |
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
| editor_memory_7mb | macos | terminal | NOREF |
| editor_memory_7mb | wayland | terminal | NOREF |
| gpu_real_render | wayland | agent | NOREF |
| gpu_real_render | wayland | terminal | NOREF |
| history_up_recall | macos | agent | PASS |
| history_up_recall | macos | terminal | PASS |
| history_up_recall | wayland | agent | PASS |
| history_up_recall | wayland | terminal | PASS |
| keybinding_rebind | macos | agent | PASS |
| keybinding_rebind | macos | terminal | PASS |
| keybinding_rebind | wayland | agent | PASS |
| keybinding_rebind | wayland | terminal | PASS |
| link_and_path_click | macos | agent | FAIL |
| link_and_path_click | macos | terminal | FAIL |
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
| routing_table_powershell | macos | agent | SKIP |
| routing_table_powershell | macos | terminal | SKIP |
| routing_table_powershell | wayland | agent | SKIP |
| routing_table_powershell | wayland | terminal | SKIP |
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
| subshell_ready_rc_clear | macos | agent | FAIL |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_sh_probe_empty_ps1 | macos | agent | PASS |
| subshell_sh_probe_empty_ps1 | macos | terminal | PASS |
| subshell_sh_probe_empty_ps1 | wayland | agent | PASS |
| subshell_sh_probe_empty_ps1 | wayland | terminal | PASS |
| tab_pointer_use | macos | agent | NOREF |
| tab_pointer_use | macos | terminal | FAIL |
| tab_pointer_use | wayland | agent | NOREF |
| tab_pointer_use | wayland | terminal | NOREF |
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
| ui_vertical_tabs_panel_click | wayland | agent | NOREF |
| ui_vertical_tabs_panel_click | wayland | terminal | PASS |

passed everywhere it ran (6, candidates for the owner's decision): history_up_recall, keybinding_rebind, remote_tab_enter_no_helper, remote_tab_no_helper, subshell_sh_probe_empty_ps1, ui_completions_as_you_type_local
not passed somewhere (fail / skip / no-reference): agent_latency_enter, agent_latency_key, editor_daily, editor_memory_7mb, gpu_real_render, link_and_path_click, network_gate_switch, routing_table_powershell, scroll_keys_position, ssh_gitbash_bootstrap_in_command, ssh_gitbash_kill_switch, ssh_gitbash_managed, startup_shell_pane, subshell_ready_rc_clear, tab_pointer_use, ui_vertical_tabs_panel_click
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 192 KB (1187 of 4380 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 192 KB (1283 of 46939 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 192 KB (1202 of 1533 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 192 KB (1227 of 37158 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 192 KB (1313 of 5809 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 192 KB (1207 of 52645 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter_ssh.log: latency evidence capped at 192 KB (1270 of 46454 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 192 KB (1199 of 1531 hop lines kept; the platform budget of 2048 KB is shared between 12 logs)
- note: macos/run_slim/logs/agent/acct/launch0_1794513310.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794513310.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794513310.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794513310.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794513310.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent@fake/phosphor.log.gz: 1575 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794510057.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794510057.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794510572.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794510057.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794510057.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794510057.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 204 KB (1372 of 15853 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1589 of 17411 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1297 of 4434 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 204 KB (1252 of 1276 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 204 KB (1321 of 35641 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 204 KB (1672 of 11999 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 204 KB (1362 of 4156 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1566 of 14993 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1299 of 4426 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 204 KB (1255 of 1288 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 2543 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794509049.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794509049.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794509049.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: run dir is 8.3 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on [test-windows]`
  - `warp terminal::input::tests::test_slow_clipboard_read_does_not_block_and_keys_follow_the_paste [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/case_selection (case not in previous run)`
  - `lab-wayland/agent/case_selection (case not in previous run)`
  - `macos/terminal/link_and_path_click (case not in previous run)`
  - `macos/terminal/tab_pointer_use (case not in previous run)`
  - `macos/agent/subshell_ready_rc_clear`
  - `macos/agent/link_and_path_click (case not in previous run)`
  - `macos/agent/pane_not_local_after_link_and_path_click (case not in previous run)`
  - `wayland/terminal/ssh_drop_reconnect (case not in previous run)`
  - `wayland/agent/left_agent_mode_after_latency_enter_ssh (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r25a (3059029972); test-macos: build/r25a (3059029972); test-warpui-core: build/r25a (3059029972); test-warpui-core-macos: build/r25a (3059029972); test-warpui-core-windows: build/r26w (3059595258); test-windows: build/r26w (3059595258); test-workspace-rest: build/r25a (3059029972); test-workspace-rest-macos: build/r25a (3059029972); test-workspace-rest-windows: build/r26w (3059595258); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998)
