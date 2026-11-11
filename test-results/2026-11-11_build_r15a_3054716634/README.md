# build/r15a  b06800e98  (pipeline 3054716634)

- date: 2026-11-11, tier: targeted, pipeline status: success
- compile (check-linux): ok, 121 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.*`, `lab_agent_remote_*`, `lab_agent_suggestions_ssh.*`, `lab_chips.*`, `lab_codebase_index_remote.*`, `lab_ctrl_c_no_esc_i.*`, `lab_diff_apply_remote.*`, `lab_diff_panel_remote.*`, `lab_enter_no_network_wait.*`, `lab_enter_start.*.150.*`, `lab_fam_agent_*`, `lab_fam_fs_basic.*`, `lab_fam_git_read.*`, `lab_fam_run_command.*`, `lab_file_tree_remote.*`, `lab_freeze_and_wake.*`, `lab_gh_chip_remote.*`, `lab_git_write_remote.*`, `lab_helper_*`, `lab_no_helper_chip.*`, `lab_nohelper_*`, `lab_nohelper_sh_login_keys.ash.lan.nohelper`, `lab_nohelper_sh_login_keys.bash.lan.nohelper`, `lab_nohelper_sh_login_keys.dash.lan.nohelper`, `lab_nohelper_sh_login_profile.ash.lan.nohelper`, `lab_nohelper_sh_login_profile.dash.lan.nohelper`, `lab_nu_agent_pager_env.*`, `lab_password_prompt.p_kbdint.lan.nohelper`, `lab_password_prompt.p_password.lan.nohelper`, `lab_path_detect.*`, `lab_path_detect.p_exec.lan.nohelper`, `lab_path_detect.p_tinypath.lan.nohelper`, `lab_raw_key_arrival.*`, `lab_read_files_remote.*`, `lab_remote_edit.*`, `lab_skills_remote.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.bash.lan.nohelper.rst`, `lab_ssh_drop_reconnect.bash.lan.nohelper.stall10`, `lab_tab_remote_dir.*`
  - asked for on linux-wayland: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `big_paste`, `cb_ui_action_latency`, `desktop_notification`, `desktop_notification_agent_question`, `drag_drop_file`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `gpu_lost_device_recovers`, `gpu_real_render`, `history_up_recall`, `hostile_output_recovers`, `ime_compose`, `intl_compose_linux`, `job_control`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `link_and_path_click`, `md_viewer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `network_gate_switch`, `pane_resize_maximize`, `primary_and_middle_click`, `remote_heredoc_paste`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `subshell_ready_relay_shape`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_readline_keys`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
  - asked for on macos: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `big_paste`, `cb_ui_action_latency`, `context_menu_actions`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_up_recall`, `hostile_output_recovers`, `ime_compose`, `job_control`, `keybinding_rebind`, `latency_enter`, `latency_first_key_after_idle`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `precmd_question_echo`, `precmd_secret_honor_ps1`, `remote_heredoc_paste`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `subshell_ready_relay_shape`, `tab_pointer_use`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_mouse_select_copy`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.*`, `lab_agent_remote_*`, `lab_agent_suggestions_ssh.*`, `lab_chips.*`, `lab_codebase_index_remote.*`, `lab_ctrl_c_no_esc_i.*`, `lab_diff_apply_remote.*`, `lab_diff_panel_remote.*`, `lab_enter_no_network_wait.*`, `lab_enter_start.*.150.*`, `lab_fam_agent_*`, `lab_fam_fs_basic.*`, `lab_fam_git_read.*`, `lab_fam_run_command.*`, `lab_file_tree_remote.*`, `lab_freeze_and_wake.*`, `lab_gh_chip_remote.*`, `lab_git_write_remote.*`, `lab_helper_*`, `lab_no_helper_chip.*`, `lab_nohelper_*`, `lab_nohelper_sh_login_keys.ash.lan.nohelper`, `lab_nohelper_sh_login_keys.dash.lan.nohelper`, `lab_nohelper_sh_login_profile.ash.lan.nohelper`, `lab_nohelper_sh_login_profile.dash.lan.nohelper`, `lab_nu_agent_pager_env.*`, `lab_password_prompt.p_kbdint.lan.nohelper`, `lab_password_prompt.p_password.lan.nohelper`, `lab_path_detect.*`, `lab_path_detect.p_exec.lan.nohelper`, `lab_path_detect.p_tinypath.lan.nohelper`, `lab_raw_key_arrival.*`, `lab_read_files_remote.*`, `lab_remote_edit.*`, `lab_skills_remote.*`, `lab_ssh_drop_reconnect.*`, `lab_ssh_drop_reconnect.bash.lan.nohelper.rst`, `lab_ssh_drop_reconnect.bash.lan.nohelper.stall10`, `lab_tab_remote_dir.*`
  - asked for on windows: `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `big_paste`, `cb_agent_pickers`, `cb_ui_action_latency`, `ctrl_c_d`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `expert_fuzz`, `flood_scrollback_find`, `focus_cycle`, `gitbash_subshell`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `ime_compose`, `intl_altgr_bash`, `intl_altgr_ps`, `intl_wide_paste`, `intl_wide_type`, `job_control`, `keybinding_rebind`, `latency_enter`, `latency_enter_ssh`, `latency_first_key_after_idle`, `md_viewer`, `md_viewer_big`, `md_viewer_first_frame`, `meta_and_word_keys`, `minimize_fullscreen`, `network_gate_switch`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `resize_reflow_output`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `scroll_keys_position`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_failing_not_local`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `subshell_ready_relay_shape`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`
- unit: 49754 passed, 5 failed, 909 skipped of 49759 run (12 suites)
- app (pass/fail/skip): lab-wayland 106/148/9 +93 no-ref; macos 29/4/10 +3 no-ref; wayland 58/16/15 +20 no-ref; soak: pass (linux-wayland)

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 222.9 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 360.0 | MacBookPro-gitrunner | no results artifact (job failed) |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 208.3 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 58.1 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | manual | - | - | not run |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 32.8 | - |  |
| build-macos-arm64 | success | 19.0 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.6 | phosphor-windows-vm |  |
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
| guards | success | 6.5 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | success | 8.0 | - |  |
| integration-linux: [2] | success | 2.8 | - |  |
| integration-linux: [3] | success | 2.7 | - |  |
| lab-helper | success | 0.7 | - |  |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.3 | - |  |
| test-linux | failed | 25.4 | - |  |
| test-macos | failed | 29.0 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.4 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 3.8 | phosphor-windows-vm |  |
| test-windows | canceled | 243.4 | phosphor-windows-vm |  |
| test-workspace-rest | success | 3.6 | - |  |
| test-workspace-rest-macos | success | 3.3 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | failed | 6.6 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## integration-linux: [1] skipped scenarios (5; not coverage)

- `integration test_auto_title`: zsh as the local shell under test
- `integration test_pwsh_vi_edit_mode_does_not_corrupt_commands`: PowerShell as the local shell under test
- `integration test_ssh_into_ash`: the local sshd fixture with remote shell `ash`: the remote shell `ash` is not installed
- `integration test_undo_close_stack_timeout_cleanup`: macOS
- `integration test_zsh_bootstraps_with_nounset_option`: zsh as the local shell under test

## integration-linux: [2] skipped scenarios (4; not coverage)

- `integration test_color_overrides_in_prompt_dont_crash`: zsh as the local shell under test
- `integration test_detect_powerlevel10k`: zsh as the local shell under test (Powerlevel10k)
- `integration test_warp_honors_user_title_zsh`: zsh as the local shell under test
- `integration test_zshrc_keypress`: zsh as the local shell under test

## integration-linux: [3] skipped scenarios (5; not coverage)

- `integration test_block_filtering_keybinding`: macOS
- `integration test_input_reporting_powershell`: PowerShell as the local shell under test
- `integration test_instant_prompt_bootstrap`: zsh as the local shell under test
- `integration test_open_new_tab_with_specific_shell_from_new_session_menu`: Windows
- `integration test_warp_prompt_unsets_zsh_rprompt`: zsh as the local shell under test

## test-linux failures

- `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing` FAIL (0.1s)

## test-macos failures

- `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing` FAIL (0.2s)

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on` LEAK (53.1s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on` LEAK (50.8s)
- `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing` FAIL (0.2s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::truncate_chars_cuts_by_characters_not_bytes` FAIL (0.3s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (1.9s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.5s)
- `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line` LEAK (2.2s)

## test-workspace-rest-windows failures

- `remote_server shellcmd::git::write::tests::discard_validates_then_restores_and_removes` FAIL (1.3s)

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t8b8ayn1/ambt8b8ayn1' are [], expected both ambt8b8ayn1_a and ambt8b8ayn1_b; NO-REFER |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t12woo3sb/ambt12woo3sb' are [], expected both ambt12woo3sb_a and ambt12woo3sb_b; the  |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t227zx5b4/zzqt227zx5b4_only'], got []; the |
| terminal | lab_tab_remote_dir.nu.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t266oam89/zzqt266oam89_only'], got []; the |
| terminal | lab_path_detect.bash.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.bash.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.zsh.lan.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.zsh.150.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 3937885); the app is relaunched so later c |
| terminal | lab_path_detect.p_tinypath.lan.nohelper | FAIL | no remote prompt after `ssh lab-p_tinypath`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / de |
| terminal | lab_chips.bash.lan.nohelper | FAIL | no branch chip (branch brt62obzjck) on screen after cd into a git directory on the remote; 54 side-call exec channels for 1 prompt(s): the a |
| terminal | lab_chips.bash.150.nohelper | FAIL | no branch chip (branch brt66618o72) on screen after cd into a git directory on the remote; 54 side-call exec channels for 1 prompt(s): the a |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; injector accounting: injector sent 90; app saw 90; |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 16 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; injector accounting: injector sent 90; app saw 90; |
| terminal | lab_chips.fish.lan.nohelper | FAIL | no branch chip (branch brt78tk0qia) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| terminal | lab_chips.fish.150.nohelper | FAIL | no branch chip (branch brt82n3k6cy) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| terminal | lab_chips.nu.lan.nohelper | FAIL | no branch chip (branch brt86wgn1m5) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| terminal | lab_chips.nu.150.nohelper | FAIL | no branch chip (branch brt90ys65bu) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| terminal | lab_enter_start.bash.150.nohelper | PASS | injector accounting: injector sent 1949; app saw 1949; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1912 unbound keydown/ty |
| terminal | lab_enter_start.zsh.150.nohelper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_enter_start.nu.150.nohelper | PASS | 30 of 30 Enters have no hop=enter_begin line with the line's text in the app log: those are timed from before the injector call, which count |
| terminal | lab_enter_no_network_wait.bash.150.nohelper | PASS | worst Enter -> write 1.44 ms (limit 75 ms); median 1.21 ms delayed against 1.19 ms undelayed; injector accounting: injector sent 3122; app s |
| terminal | lab_enter_no_network_wait.zsh.150.nohelper | PASS | worst Enter -> write 1.32 ms (limit 75 ms); median 1.15 ms delayed against 1.14 ms undelayed; injector accounting: injector sent 3061; app s |
| terminal | lab_enter_no_network_wait.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| terminal | lab_enter_no_network_wait.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper | PASS | injector accounting: injector sent 1494; app saw 1494; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1449 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper.rst | PASS | injector accounting: injector sent 599; app saw 599; lost-before-app 0 (Enter: sent 17, app saw 17); of the app's: 580 unbound keydown/typed |
| terminal | lab_ssh_drop_reconnect.bash.lan.nohelper.stall10 | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 202 unbound keydown/typed,  |
| terminal | lab_ssh_drop_reconnect.bash.150.nohelper | PASS | injector accounting: injector sent 1494; app saw 1494; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1449 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.nohelper | PASS | injector accounting: injector sent 1488; app saw 1488; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1443 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.nohelper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [escape_connect_bare] app log after the escape: no ssh_esca |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | FAIL | escape_1: lines typed into the remote shell: expected ['t763_vlgzcod2', 't764_k96dpc6x', 't765_dw9ioot1', 't766_dui6ho6p', 't767_ewjoq657'], |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | FAIL | escape_1: lines typed into the remote shell: expected ['t787_vogojcvo', 't788_zh1udq7h', 't789_mjczq0k9', 't790_irb7wxe2', 't791_lyqru3nf'], |
| terminal | lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.nohelper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.fish.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 588 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.fish.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 588 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 711 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 50 ms] effect file: expected ['t899lu4b291q', 't900l2tl1nmd', 't901kx0ona2x', 't90267jho6fk', 't903gvb7td0u'], got ['t899lu4b291q',  |
| terminal | lab_no_helper_chip.bash.lan.nohelper | PASS | injector accounting: injector sent 82; app saw 82; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 4 t |
| terminal | lab_no_helper_chip.zsh.lan.nohelper | FAIL | no 'shell mode' chip on screen in a no-helper ssh session; a command typed after connecting did not run exactly once with the chip up; injec |
| terminal | lab_no_helper_chip.fish.lan.nohelper | FAIL | no 'shell mode' chip on screen in a no-helper ssh session; injector accounting: injector sent 82; app saw 82; lost-before-app 0 (Enter: sent |
| terminal | lab_no_helper_chip.nu.lan.nohelper | FAIL | no 'shell mode' chip on screen in a no-helper ssh session; injector accounting: injector sent 88; app saw 88; lost-before-app 0 (Enter: sent |
| terminal | lab_nohelper_sh_login_keys.dash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 357 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_keys.ash.lan.nohelper | PASS | injector accounting: injector sent 355; app saw 355; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 350 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_keys.bash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 349 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_profile.dash.lan.nohelper | PASS | injector accounting: injector sent 128; app saw 128; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 123 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_profile.ash.lan.nohelper | PASS | injector accounting: injector sent 126; app saw 126; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 121 unbound keydown/typed,  |
| terminal | lab_nohelper_sh_login_profile.bash.lan.nohelper | PASS | injector accounting: injector sent 120; app saw 120; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 112 unbound keydown/typed,  |
| terminal | lab_nu_agent_pager_env.nu.lan.nohelper | FAIL | pagers not set during the command: ['after:mypager:unset:t9658fpq4:pgsubt9658fpq4', 'd3:mypager']; after an external command that exits non- |
| terminal | lab_password_prompt.p_password.lan.nohelper | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 180 unbound keydown/typed,  |
| terminal | lab_password_prompt.p_kbdint.lan.nohelper | PASS | injector accounting: injector sent 190; app saw 190; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 176 unbound keydown/typed,  |
| terminal | lab_fam_fs_basic.bash.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.bash.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.zsh.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.zsh.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.fish.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.fish.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.nu.lan.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_fs_basic.nu.150.nohelper | SKIP | shell arm for fs_basic not implemented yet; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | branch brt979c6tu9m not on screen after cd into the repo on the remote; shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 35 out-of-band channels for 2 prompt(s), expected 1..prompts; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | branch brt987bmx2ai not on screen after cd into the repo on the remote; shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 33 out-of-band channels for 2 prompt(s), expected 1..prompts; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | branch brt9952ja993 not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | branch brt999y84spv not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | branch brt1003tipve4 not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | branch brt1007y1opo3 not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | branch brt1015lndgoc not on screen after cd into the repo on the remote; shell arm: 64 out-of-band channels for 2 prompt(s), expected 1..pro |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 90 out-of-band channels for 2 prompt(s), expected 1..prompts; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | branch brt1023wg7dg0 not on screen after cd into the repo on the remote; shell arm: 64 out-of-band channels for 2 prompt(s), expected 1..pro |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | branch brt1027myf893 not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | branch brt1031neqgld not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | branch brt1035g9msns not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | branch brt1039lzuaxz not on screen after cd into the repo on the remote; shell arm: 0 out-of-band channels for 2 prompt(s), expected 1..prom |
| terminal | lab_nohelper_tab_hands_back.bash.lan.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 5 t |
| terminal | lab_nohelper_tab_hands_back.bash.150.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 74 unbound keydown/typed, 5 t |
| terminal | lab_nohelper_tab_hands_back.zsh.lan.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; injector accounting: injector sent 82; app saw 82; lost-before-app 0 |
| terminal | lab_nohelper_tab_hands_back.zsh.150.nohelper | FAIL | the typed line reached the remote shell 0 times, expected exactly once; injector accounting: injector sent 82; app saw 82; lost-before-app 0 |
| terminal | lab_nohelper_tab_hands_back.fish.lan.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 78 unbound keydown/typed, 4 t |
| terminal | lab_nohelper_tab_hands_back.fish.150.nohelper | PASS | injector accounting: injector sent 83; app saw 83; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 78 unbound keydown/typed, 4 t |
| terminal | lab_nohelper_tab_hands_back.nu.lan.nohelper | PASS | injector accounting: injector sent 89; app saw 89; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 84 unbound keydown/typed, 4 t |
| terminal | lab_nohelper_tab_hands_back.nu.150.nohelper | PASS | injector accounting: injector sent 89; app saw 89; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 84 unbound keydown/typed, 4 t |
| terminal | lab_nohelper_unavailable.bash.lan.nohelper | FAIL | no 'unavailable here' message in a session without an out-of-band channel; injector accounting: injector sent 175; app saw 175; lost-before- |
| terminal | lab_nohelper_unavailable.bash.150.nohelper | FAIL | no 'unavailable here' message in a session without an out-of-band channel; injector accounting: injector sent 175; app saw 175; lost-before- |
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
| terminal | lab_file_tree_remote.zsh.lan.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 98 unbound keydown/typed, 4 |
| terminal | lab_file_tree_remote.zsh.150.nohelper | PASS | injector accounting: injector sent 108; app saw 108; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 98 unbound keydown/typed, 4 |
| terminal | lab_file_tree_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=file_tree snapshot in the app log: the shell-mode file tree needs the session's out-of-band channe |
| terminal | lab_file_tree_remote.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=file_tree snapshot in the app log: the shell-mode file tree needs the session's out-of-band channe |
| terminal | lab_file_tree_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=file_tree snapshot in the app log: the shell-mode file tree needs the session's out-of-band channe |
| terminal | lab_file_tree_remote.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=file_tree snapshot in the app log: the shell-mode file tree needs the session's out-of-band channe |
| terminal | lab_skills_remote.bash.lan.nohelper | PASS | injector accounting: injector sent 104; app saw 104; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 95 unbound keydown/typed, 4 |
| terminal | lab_skills_remote.zsh.lan.nohelper | FAIL | the host's skill never reached the app (no hop=agent_context line naming it); the context came from [], expected shell; the global rules fil |
| terminal | lab_skills_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=agent_context line in the app log: the shell-mode agent context needs the session's out-of-band ch |
| terminal | lab_skills_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=agent_context line in the app log: the shell-mode agent context needs the session's out-of-band ch |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t1140erl8g1.txt in the code editor: the palette's 'Toggle project explorer', |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t1143sz6tc0.txt in the code editor: the palette's 'Toggle project explorer', |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is fish, which gets a plain ssh session by deci |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| terminal | lab_git_write_remote.bash.lan.nohelper | FAIL | harness exception: TypeError('not enough arguments for format string'); injector accounting: injector sent 2; app saw 2; lost-before-app 0 ( |
| terminal | lab_git_write_remote.zsh.lan.nohelper | FAIL | harness exception: TypeError('not enough arguments for format string'); injector accounting: injector sent 2; app saw 2; lost-before-app 0 ( |
| terminal | lab_git_write_remote.fish.lan.nohelper | FAIL | harness exception: TypeError('not enough arguments for format string'); injector accounting: injector sent 2; app saw 2; lost-before-app 0 ( |
| terminal | lab_git_write_remote.nu.lan.nohelper | FAIL | harness exception: TypeError('not enough arguments for format string'); injector accounting: injector sent 2; app saw 2; lost-before-app 0 ( |
| terminal | lab_codebase_index_remote.bash.lan.nohelper | FAIL | first sync (files, fragments, embedded, already, stale) = (114, 33, 22, 11, 0): expected >=3 files embedded, none already; second sync (file |
| terminal | lab_codebase_index_remote.bash.150.nohelper | FAIL | first sync (files, fragments, embedded, already, stale) = (117, 34, 0, 34, 0): expected >=3 files embedded, none already; no hop=remote_inde |
| terminal | lab_codebase_index_remote.zsh.lan.nohelper | FAIL | first sync (files, fragments, embedded, already, stale) = (112, 31, 22, 9, 0): expected >=3 files embedded, none already; second sync (files |
| terminal | lab_codebase_index_remote.zsh.150.nohelper | FAIL | first sync (files, fragments, embedded, already, stale) = (115, 32, 0, 32, 0): expected >=3 files embedded, none already; no hop=remote_inde |
| terminal | lab_codebase_index_remote.fish.lan.nohelper | FAIL | no hop=remote_index_sync done after cd into the repository (the sync never ran or never finished); injector accounting: injector sent 97; ap |
| terminal | lab_codebase_index_remote.fish.150.nohelper | FAIL | no hop=remote_index_sync done after cd into the repository (the sync never ran or never finished); injector accounting: injector sent 97; ap |
| terminal | lab_codebase_index_remote.nu.lan.nohelper | FAIL | no hop=remote_index_sync done after cd into the repository (the sync never ran or never finished); injector accounting: injector sent 101; a |
| terminal | lab_codebase_index_remote.nu.150.nohelper | FAIL | no hop=remote_index_sync done after cd into the repository (the sync never ran or never finished); injector accounting: injector sent 101; a |
| terminal | lab_diff_panel_remote.bash.lan.nohelper | FAIL | git status on the host did not show the edit and the untracked file: rc=0 ' .M N... 100644 100644 100644 5626abf0f72e58d7a153368ba57db4c673c |
| terminal | lab_diff_panel_remote.zsh.lan.nohelper | FAIL | git status on the host did not show the edit and the untracked file: rc=0 ' .M N... 100644 100644 100644 5626abf0f72e58d7a153368ba57db4c673c |
| terminal | lab_diff_panel_remote.fish.lan.nohelper | FAIL | git diff --numstat HEAD on the host: '1\t0\tdiffp_t1204soija6/a.txt\n0\t0\tgw_t1158xhfubm/w\n'; NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION |
| terminal | lab_diff_panel_remote.nu.lan.nohelper | FAIL | git diff --numstat HEAD on the host: '1\t0\tdiffp_t12082bn1k5/a.txt\n0\t0\tgw_t1161mw6ays/w\n'; NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION |
| terminal | lab_gh_chip_remote.bash.lan.nohelper | FAIL | a clean repo shows no changed files, got ('shell', 1, 0, 0, 0, 0, False); injector accounting: injector sent 109; app saw 109; lost-before-a |
| terminal | lab_gh_chip_remote.zsh.lan.nohelper | FAIL | a clean repo shows no changed files, got ('shell', 1, 0, 0, 0, 0, False); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=gh_chip pr=42 line: eith |
| terminal | lab_gh_chip_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=git_chip line in the app log: the remote git chip did not populate for this session (it needs the  |
| terminal | lab_gh_chip_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=git_chip line in the app log: the remote git chip did not populate for this session (it needs the  |
| terminal | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.helper | FAIL | the shell received ['cd /work/u_bash/tabfx_t1229jy343x/zzqt1229jy343x'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no  |
| terminal | lab_tab_remote_dir.zsh.lan.helper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t1233y6ge4s/ambt1233y6ge4s' are [], expected both ambt1233y6ge4s_a and ambt1233y6ge4s |
| terminal | lab_tab_remote_dir.zsh.150.helper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t1237b4cn5b/ambt1237b4cn5b' are [], expected both ambt1237b4cn5b_a and ambt1237b4cn5b |
| terminal | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.lan.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t1247f9d132/zzqt1247f9d132_only'], got []; |
| terminal | lab_tab_remote_dir.nu.150.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t1251nhux96/zzqt1251nhux96_only'], got []; |
| terminal | lab_path_detect.bash.lan.helper | PASS | injector accounting: injector sent 101; app saw 101; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 91 unbound keydown/typed, 6 |
| terminal | lab_path_detect.bash.150.helper | FAIL | typing a remote path opened 10 exec and 0 subsystem channel(s) on the server (expected none); injector accounting: injector sent 101; app sa |
| terminal | lab_path_detect.zsh.lan.helper | PASS | injector accounting: injector sent 99; app saw 99; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 89 unbound keydown/typed, 6 t |
| terminal | lab_path_detect.zsh.150.helper | FAIL | typing a remote path opened 12 exec and 0 subsystem channel(s) on the server (expected none); injector accounting: injector sent 99; app saw |
| terminal | lab_path_detect.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_chips.bash.lan.helper | FAIL | no branch chip (branch brt1281uigde3) on screen after cd into a git directory on the remote; injector accounting: injector sent 96; app saw  |
| terminal | lab_chips.bash.150.helper | FAIL | no branch chip (branch brt1285m4iblb) on screen after cd into a git directory on the remote; injector accounting: injector sent 96; app saw  |
| terminal | lab_chips.zsh.lan.helper | FAIL | no branch chip (branch brt1289yuzwy7) on screen after cd into a git directory on the remote; injector accounting: injector sent 94; app saw  |
| terminal | lab_chips.zsh.150.helper | FAIL | 23 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; injector accounting: injector sent 94; app saw 94; |
| terminal | lab_chips.fish.lan.helper | FAIL | no branch chip (branch brt1297cyqfj7) on screen after cd into a git directory on the remote; injector accounting: injector sent 96; app saw  |
| terminal | lab_chips.fish.150.helper | FAIL | no branch chip (branch brt1301x7r5xh) on screen after cd into a git directory on the remote; injector accounting: injector sent 96; app saw  |
| terminal | lab_chips.nu.lan.helper | FAIL | no branch chip (branch brt1305canwya) on screen after cd into a git directory on the remote; injector accounting: injector sent 100; app saw |
| terminal | lab_chips.nu.150.helper | FAIL | no branch chip (branch brt13094dtgqy) on screen after cd into a git directory on the remote; injector accounting: injector sent 100; app saw |
| terminal | lab_enter_start.bash.150.helper | PASS | injector accounting: injector sent 2043; app saw 2043; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 2006 unbound keydown/ty |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est1377sftr4 $EPOCHREALTIME >> /work/u_zsh/es_t1376iy3l.txt', 'echo est |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1593; |
| terminal | lab_enter_start.nu.150.helper | PASS | 30 of 30 Enters have no hop=enter_begin line with the line's text in the app log: those are timed from before the injector call, which count |
| terminal | lab_enter_no_network_wait.bash.150.helper | PASS | worst Enter -> write 1.32 ms (limit 75 ms); median 1.17 ms delayed against 1.18 ms undelayed; injector accounting: injector sent 3243; app s |
| terminal | lab_enter_no_network_wait.zsh.150.helper | PASS | worst Enter -> write 1.26 ms (limit 75 ms); median 1.13 ms delayed against 1.12 ms undelayed; injector accounting: injector sent 3182; app s |
| terminal | lab_enter_no_network_wait.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| terminal | lab_enter_no_network_wait.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| terminal | lab_ssh_drop_reconnect.bash.lan.helper | PASS | injector accounting: injector sent 1526; app saw 1526; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1481 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.bash.150.helper | PASS | injector accounting: injector sent 1526; app saw 1526; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1481 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.lan.helper | PASS | injector accounting: injector sent 1520; app saw 1520; lost-before-app 0 (Enter: sent 43, app saw 43); of the app's: 1475 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.zsh.150.helper | FAIL | [escape_connect_bare] no local prompt within 20 s after the connection was lost; [escape_connect_bare] app log after the escape: no ssh_esca |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | FAIL | escape_1: lines typed into the remote shell: expected ['t1962_hadfwzp1', 't1963_rtir6hst', 't1964_npsrqyfl', 't1965_qsuqypk9', 't1966_dv33i2 |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | FAIL | escape_1: lines typed into the remote shell: expected ['t1986_7v0u2eqd', 't1987_ip7b6oda', 't1988_27mq1drg', 't1989_omfon3yk', 't1990_n5r9k5 |
| terminal | lab_ctrl_c_no_esc_i.bash.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.bash.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.fish.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 607 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.fish.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 607 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.helper | PASS | injector accounting: injector sent 753; app saw 753; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 730 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x2 gap 50 ms] effect file: expected ['t2098ybxh2j3t', 't2099hju4suz4', 't2100mjk3b0tr', 't210131qp5o0y', 't2102yfbsfvne'], got ['t2098ybxh2 |
| terminal | lab_helper_idle_exit.bash.lan.helper | SKIP | slow (11 min grace wait): set APPCI_LAB_SLOW=1 for a candidate/release run; injector accounting: injector sent 2; app saw 2; lost-before-app |
| terminal | lab_helper_reconnect.bash.lan.helper | FAIL | after the reconnect there are 4 helper daemons, expected exactly 1 (no duplicate); injector accounting: injector sent 139; app saw 139; lost |
| terminal | lab_helper_reconnect.bash.150.helper | FAIL | after the reconnect there are 5 helper daemons, expected exactly 1 (no duplicate); injector accounting: injector sent 139; app saw 139; lost |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | after the reconnect there are 4 helper daemons, expected exactly 1 (no duplicate); reconnect: a command typed into the session ran 0 times,  |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | after the reconnect there are 5 helper daemons, expected exactly 1 (no duplicate); reconnect: a command typed into the session ran 0 times,  |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=fs_basic backend=..` line: the route is not proven; injector  |
| terminal | lab_fam_fs_basic.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=fs_basic backend=..` line: the route is not proven; injector  |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=fs_basic backend=..` line: the route is not proven; injector  |
| terminal | lab_fam_fs_basic.zsh.150.helper | FAIL | branch brt2139rr48ut not on screen after cd into the repo on the remote; 66 extra sshd exec channels with the helper present: fs_basic must  |
| terminal | lab_fam_fs_basic.fish.lan.helper | FAIL | branch brt2143e49vq8 not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_fs_basic.fish.150.helper | FAIL | branch brt2147eu2vlp not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_fs_basic.nu.lan.helper | FAIL | branch brt2151f285bf not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_fs_basic.nu.150.helper | FAIL | branch brt2155zz1zxb not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_git_read.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=git_read backend=..` line: the route is not proven; injector  |
| terminal | lab_fam_git_read.bash.150.helper | FAIL | 32 extra sshd exec channels with the helper present: git_read must be served through the helper; NO-REFERENCE: UNVERIFIABLE: [hook] the app  |
| terminal | lab_fam_git_read.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=git_read backend=..` line: the route is not proven; injector  |
| terminal | lab_fam_git_read.zsh.150.helper | FAIL | branch brt21699zwcm1 not on screen after cd into the repo on the remote; 63 extra sshd exec channels with the helper present: git_read must  |
| terminal | lab_fam_git_read.fish.lan.helper | FAIL | branch brt21738lao6h not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_git_read.fish.150.helper | FAIL | branch brt2177mwsszm not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_git_read.nu.lan.helper | FAIL | branch brt2181p8ba8h not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_git_read.nu.150.helper | FAIL | branch brt218567uc8m not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.bash.lan.helper | FAIL | branch brt2189m4auma not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=run_command backend=..` line: the route is not proven; inject |
| terminal | lab_fam_run_command.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_route family=run_command backend=..` line: the route is not proven; inject |
| terminal | lab_fam_run_command.zsh.150.helper | FAIL | branch brt21995hhtb9 not on screen after cd into the repo on the remote; 63 extra sshd exec channels with the helper present: run_command mu |
| terminal | lab_fam_run_command.fish.lan.helper | FAIL | branch brt22037b8slh not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.fish.150.helper | FAIL | branch brt22077rnszz not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.nu.lan.helper | FAIL | branch brt2211qy8vsx not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_fam_run_command.nu.150.helper | FAIL | branch brt221546xbej not on screen after cd into the repo on the remote; NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=backend_r |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | FAIL | no single helper daemon to kill; branch brt2220ojyama not shown after the helper was killed: the read did not fall back to the shell; NO-REF |
| terminal | lab_helper_killed.bash.150.helper | FAIL | no single helper daemon to kill; NO-REFERENCE: UNVERIFIABLE: write half (error reported, no second attempt) needs a scripted app write; the  |
| terminal | lab_helper_killed.zsh.lan.helper | FAIL | no single helper daemon to kill; NO-REFERENCE: UNVERIFIABLE: write half (error reported, no second attempt) needs a scripted app write; the  |
| terminal | lab_helper_killed.zsh.150.helper | FAIL | no single helper daemon to kill; NO-REFERENCE: UNVERIFIABLE: write half (error reported, no second attempt) needs a scripted app write; the  |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t8b8ayn1/ambt8b8ayn1' are [], expected both ambt8b8ayn1_a and ambt8b8ayn1_b; NO-REFER |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t12woo3sb/ambt12woo3sb' are [], expected both ambt12woo3sb_a and ambt12woo3sb_b; the  |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t227zx5b4/zzqt227zx5b4_only'], got []; the |
| agent | lab_tab_remote_dir.nu.150.nohelper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t266oam89/zzqt266oam89_only'], got []; the |
| agent | lab_path_detect.bash.lan.nohelper | PASS | injector accounting: injector sent 96; app saw 96; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 5 t |
| agent | lab_path_detect.bash.150.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| agent | lab_path_detect.zsh.lan.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 6 t |
| agent | lab_path_detect.zsh.150.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 86 unbound keydown/typed, 6 t |
| agent | lab_path_detect.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 4127305); the app is relaunched so later c |
| agent | lab_path_detect.p_tinypath.lan.nohelper | FAIL | no remote prompt after `ssh lab-p_tinypath`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / de |
| agent | lab_chips.bash.lan.nohelper | FAIL | 54 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; injector accounting: injector sent 91; app saw 91; |
| agent | lab_chips.bash.150.nohelper | FAIL | no branch chip (branch brt66618o72) on screen after cd into a git directory on the remote; 54 side-call exec channels for 1 prompt(s): the a |
| agent | lab_chips.zsh.lan.nohelper | FAIL | no branch chip (branch brt707bzu1d) on screen after cd into a git directory on the remote; 54 side-call exec channels for 1 prompt(s): the a |
| agent | lab_chips.zsh.150.nohelper | FAIL | no branch chip (branch brt74indtee) on screen after cd into a git directory on the remote; 54 side-call exec channels for 1 prompt(s): the a |
| agent | lab_chips.fish.lan.nohelper | FAIL | no branch chip (branch brt78tk0qia) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| agent | lab_chips.fish.150.nohelper | FAIL | no branch chip (branch brt82n3k6cy) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| agent | lab_chips.nu.lan.nohelper | FAIL | no branch chip (branch brt86wgn1m5) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| agent | lab_chips.nu.150.nohelper | FAIL | no branch chip (branch brt90ys65bu) on screen after cd into a git directory on the remote; no side call at all with no helper: where did the |
| agent | lab_enter_start.bash.150.nohelper | PASS | injector accounting: injector sent 1948; app saw 1948; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1912 unbound keydown/ty |
| agent | lab_enter_start.zsh.150.nohelper | PASS | injector accounting: injector sent 1951; app saw 1951; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1914 unbound keydown/ty |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_enter_start.nu.150.nohelper | PASS | 30 of 30 Enters have no hop=enter_begin line with the line's text in the app log: those are timed from before the injector call, which count |
| agent | lab_enter_no_network_wait.bash.150.nohelper | PASS | worst Enter -> write 1.78 ms (limit 75 ms); median 1.51 ms delayed against 1.51 ms undelayed; injector accounting: injector sent 3122; app s |
| agent | lab_enter_no_network_wait.zsh.150.nohelper | PASS | worst Enter -> write 1.69 ms (limit 75 ms); median 1.44 ms delayed against 1.40 ms undelayed; injector accounting: injector sent 3061; app s |
| agent | lab_enter_no_network_wait.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| agent | lab_enter_no_network_wait.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.bash.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.bash.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.fish.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 588 unbound keydown/typed |
| agent | lab_ctrl_c_no_esc_i.fish.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 588 unbound keydown/typed |
| agent | lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 711 unbound keydown/typed |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 50 ms] effect file: expected ['t699rch4maoa', 't700m4jw7cwj', 't701rpfd93kg', 't702k2ibdzft', 't703t6xepdaq'], got ['t699rch4maoa',  |
| agent | lab_nohelper_sh_login_keys.dash.lan.nohelper | PASS | injector accounting: injector sent 361; app saw 361; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 357 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_keys.ash.lan.nohelper | PASS | injector accounting: injector sent 355; app saw 355; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 350 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_keys.bash.lan.nohelper | PASS | injector accounting: injector sent 362; app saw 362; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 349 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_profile.dash.lan.nohelper | PASS | injector accounting: injector sent 128; app saw 128; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 123 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_profile.ash.lan.nohelper | PASS | injector accounting: injector sent 126; app saw 126; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 121 unbound keydown/typed,  |
| agent | lab_nohelper_sh_login_profile.bash.lan.nohelper | PASS | injector accounting: injector sent 120; app saw 120; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 112 unbound keydown/typed,  |
| agent | lab_password_prompt.p_password.lan.nohelper | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 180 unbound keydown/typed,  |
| agent | lab_password_prompt.p_kbdint.lan.nohelper | PASS | injector accounting: injector sent 189; app saw 189; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 176 unbound keydown/typed,  |
| agent | lab_read_files_remote.bash.lan.nohelper | FAIL | the setup file was not created on the host; injector accounting: injector sent 246; app saw 246; lost-before-app 0 (Enter: sent 7, app saw 7 |
| agent | lab_read_files_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| agent | lab_diff_apply_remote.bash.lan.nohelper | FAIL | the created file is not on the host; injector accounting: injector sent 259; app saw 259; lost-before-app 0 (Enter: sent 9, app saw 9); of t |
| agent | lab_diff_apply_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| agent | lab_agent_remote_flow.bash.lan.nohelper | FAIL | the agent turn did not finish: the model received 5 of 6 tool results; injector accounting: injector sent 458; app saw 458; lost-before-app  |
| agent | lab_agent_remote_flow.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| agent | lab_agent_remote_errors.bash.lan.nohelper | PASS | injector accounting: injector sent 342; app saw 342; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 334 unbound keydown/typed,  |
| agent | lab_agent_remote_errors.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| agent | lab_agent_remote_drop.bash.lan.nohelper | PASS | injector accounting: injector sent 266; app saw 266; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 255 unbound keydown/typed,  |
| agent | lab_agent_remote_drop.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_agent_suggestions_ssh.bash.lan.nohelper | PASS | injector accounting: injector sent 181; app saw 181; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 165 unbound keydown/typed,  |
| agent | lab_fam_agent_edit_conflict.bash.lan.nohelper | PASS | partial accept: the diff card has no per-hunk accept (Up/Down only move between hunks, code_diff_view.rs); accept applies every hunk, reject |
| agent | lab_fam_agent_edit_conflict.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: KNOWN LIMITATION (owner decision, #2019): the host's login shell is nu, which gets a plain ssh session by decisi |
| agent | lab_fam_agent_delete_conflict.bash.lan.nohelper | PASS | injector accounting: injector sent 152; app saw 152; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 142 unbound keydown/typed,  |
| agent | lab_fam_agent_edit_endings.bash.lan.nohelper | PASS | injector accounting: injector sent 1308; app saw 1308; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 1286 unbound keydown/ty |
| agent | lab_fam_agent_edit_touch_only.bash.lan.nohelper | PASS | injector accounting: injector sent 309; app saw 309; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 299 unbound keydown/typed,  |
| agent | lab_freeze_and_wake.bash.lan.nohelper | PASS | ssh tab after the wake: the session is alive: the host received the line exactly; agent tab after the wake: the turn ended: every chunk and  |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 4160598 -> 650); a relaunched app starts in a terminal session, so this is |
| agent | lab_tab_remote_dir.bash.150.helper | FAIL | the shell received ['cd /work/u_bash/tabfx_t834ita0ge/zzqt834ita0ge'], not the completed directory; NO-REFERENCE: UNVERIFIABLE: [hook] no ta |
| agent | lab_tab_remote_dir.zsh.lan.helper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t83862mq4f/ambt83862mq4f' are [], expected both ambt83862mq4f_a and ambt83862mq4f_b;  |
| agent | lab_tab_remote_dir.zsh.150.helper | FAIL | the lab's own zsh candidates for 'cd /work/u_zsh/tabfx_t842ie6ij9/ambt842ie6ij9' are [], expected both ambt842ie6ij9_a and ambt842ie6ij9_b;  |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t8528brdrn/zzqt8528brdrn_only'], got []; t |
| agent | lab_tab_remote_dir.nu.150.helper | FAIL | unique prefix + Tab: the remote directory the completed `cd` entered: expected ['/work/u_nu/tabfx_t856ofbgum/zzqt856ofbgum_only'], got []; t |
| agent | lab_path_detect.bash.lan.helper | PASS | injector accounting: injector sent 98; app saw 98; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 89 unbound keydown/typed, 5 t |
| agent | lab_path_detect.bash.150.helper | FAIL | typing a remote path opened 11 exec and 0 subsystem channel(s) on the server (expected none); injector accounting: injector sent 99; app saw |
| agent | lab_path_detect.zsh.lan.helper | PASS | injector accounting: injector sent 96; app saw 96; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 5 t |
| agent | lab_path_detect.zsh.150.helper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| agent | lab_path_detect.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_chips.bash.lan.helper | FAIL | no branch chip (branch brt885yscs51) on screen after cd into a git directory on the remote; injector accounting: injector sent 94; app saw 9 |
| agent | lab_chips.bash.150.helper | FAIL | no branch chip (branch brt88946458g) on screen after cd into a git directory on the remote; injector accounting: injector sent 93; app saw 9 |
| agent | lab_chips.zsh.lan.helper | FAIL | no branch chip (branch brt893pe7xqi) on screen after cd into a git directory on the remote; injector accounting: injector sent 91; app saw 9 |
| agent | lab_chips.zsh.150.helper | FAIL | no branch chip (branch brt897qez1qr) on screen after cd into a git directory on the remote; injector accounting: injector sent 91; app saw 9 |
| agent | lab_chips.fish.lan.helper | FAIL | no branch chip (branch brt901rmxuwe) on screen after cd into a git directory on the remote; injector accounting: injector sent 93; app saw 9 |
| agent | lab_chips.fish.150.helper | FAIL | no branch chip (branch brt9055ubvut) on screen after cd into a git directory on the remote; injector accounting: injector sent 93; app saw 9 |
| agent | lab_chips.nu.lan.helper | FAIL | no branch chip (branch brt909fm7u35) on screen after cd into a git directory on the remote; injector accounting: injector sent 97; app saw 9 |
| agent | lab_chips.nu.150.helper | FAIL | no branch chip (branch brt9137h6bmn) on screen after cd into a git directory on the remote; injector accounting: injector sent 97; app saw 9 |
| agent | lab_enter_start.bash.150.helper | PASS | injector accounting: injector sent 1981; app saw 1981; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1945 unbound keydown/ty |
| agent | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est981a3h1q $EPOCHREALTIME >> /work/u_zsh/es_t980wyi6.txt', 'echo est98 |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1592; |
| agent | lab_enter_start.nu.150.helper | PASS | 30 of 30 Enters have no hop=enter_begin line with the line's text in the app log: those are timed from before the injector call, which count |
| agent | lab_enter_no_network_wait.bash.150.helper | PASS | worst Enter -> write 1.77 ms (limit 75 ms); median 1.55 ms delayed against 1.54 ms undelayed; injector accounting: injector sent 3243; app s |
| agent | lab_enter_no_network_wait.zsh.150.helper | PASS | worst Enter -> write 2.02 ms (limit 75 ms); median 1.45 ms delayed against 1.52 ms undelayed; injector accounting: injector sent 3182; app s |
| agent | lab_enter_no_network_wait.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| agent | lab_enter_no_network_wait.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| agent | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| agent | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| agent | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| agent | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0x0']; plain ssh: 0xb8 (NOT the class plain ssh uses); NO-REFERENCE: UNVERIFIABLE: the app log has 32  |
| agent | lab_ctrl_c_no_esc_i.bash.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.lan.helper | PASS | after lab_ctrl_c_no_esc_i.bash.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.bash.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 587 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.150.helper | PASS | after lab_ctrl_c_no_esc_i.bash.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.helper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation windo |
| agent | lab_ctrl_c_no_esc_i.fish.lan.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 607 unbound keydown/typed |
| agent | lab_ctrl_c_no_esc_i.fish.150.helper | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 607 unbound keydown/typed |
| agent | lab_ctrl_c_no_esc_i.nu.lan.helper | PASS | injector accounting: injector sent 753; app saw 753; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 730 unbound keydown/typed |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x2 gap 50 ms] effect file: expected ['t1523qt00yjo7', 't1524idtmzw2i', 't152551rog85m', 't1526nanypics', 't1527vjwc8wi9'], got ['t1523qt00y |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mac_pointer_chord_proof | FAIL | a click on the menu bar at (756, 6) points was NOT refused by mptr ('pointer injector failed: mptr exit 4 ( Window Server (pid 547, layer 24 |
| terminal | big_paste | PASS | injector accounting: injector sent 21; app saw 21; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 15 unbound keydown/typed, 2 t |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (`sudo -n true` failed (rc 1: sudo: a password is required) and APPCI_SUDO_USER is not set). NEEDS: a su |
| terminal | job_control | PASS | injector accounting: injector sent 231; app saw 231; lost-before-app 0 (Enter: sent 11, app saw 11); of the app's: 217 unbound keydown/typed |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.27 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_first_key_after_idle | NOREF | First key after 1 s idle: first key after 1 s idle, kl3 post-to-first-changed-pixel at 240 Hz (4.17 ms quantum, not applied as a resolution  |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | flood_scrollback_find | PASS | flood_scrollback_find; app log: 31 find_scroll hop(s), moved=true in 1, last: result=scroll block=165 moved=true viewport_lines=39.61 render |
| terminal | remote_heredoc_paste | PASS | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (SIGKILL t |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_failing_not_local_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | hostile_output_recovers | PASS | largest main-thread item after the output 0.0 ms (< 16 ms; before: 0.0 ms); injector accounting: injector sent 683; app saw 683; lost-before |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | ime_compose | SKIP | macOS: NEEDS: the owner enables the Pinyin - Simplified input source for the runner user (System Settings > Keyboard > Text Input > Input So |
| terminal | scroll_keys_foreground | PASS | injector accounting: injector sent 132; app saw 132; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 99 unbound keydown/typed, 2 |
| terminal | ui_mouse_select_copy | PASS | injector accounting: injector sent 196; app saw 196; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 187 unbound keydown/typed,  |
| terminal | ui_copy_wrapped_output | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 218 unbound keydown/typed,  |
| terminal | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 230; app saw 230; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 219 unbound keydown/typed,  |
| terminal | context_menu_actions | PASS | injector accounting: injector sent 161; app saw 161; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 149 unbound keydown/typed,  |
| terminal | selection_modes | PASS | injector accounting: injector sent 289; app saw 289; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 267 unbound keydown/typed,  |
| terminal | link_and_path_click | FAIL | Cmd+click on `pan2yw2r4.rs:140`: nothing opened: no call to the desktop opener and the file's text is not on screen; after_links: file has 0 |
| terminal | resize_reflow_output | PASS | injector accounting: injector sent 580; app saw 580; lost-before-app 0 (Enter: sent 15, app saw 15); of the app's: 532 unbound keydown/typed |
| terminal | editor_daily | NOREF | NO-REFERENCE: macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the |
| terminal | focus_cycle | PASS | injector accounting: injector sent 346; app saw 346; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 336 unbound keydown/typed,  |
| terminal | gitbash_typeahead_reps | SKIP | Git Bash pane only (#1393: the ConPTY prompt hold of a bash pane on Windows); injector accounting: injector sent 2; app saw 2; lost-before-a |
| terminal | precmd_secret_honor_ps1 | PASS | injector accounting: injector sent 537; app saw 537; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 522 unbound keydown/typed |
| terminal | minimize_fullscreen | FAIL | fullscreen off: the window says it left fullscreen, but `stty size` is (56, 153), it was (45, 124) before fullscreen (the grid did not follo |
| terminal | precmd_question_echo | PASS | injector accounting: injector sent 365; app saw 365; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 353 unbound keydown/typed |
| terminal | desktop_notification | SKIP | macOS: NEEDS: on the Mac runner the owner answers the macOS prompt 'Phosphor would like to send you notifications' once for the test bundle, |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 75; app saw 75; lost |
| terminal | md_viewer_big | PASS | injector accounting: injector sent 75; app saw 75; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 70 unbound keydown/typed, 1 t |
| terminal | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 985x652.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with palette cmd+p + 'Close Current Session'; scripted |
| terminal | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| terminal | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); no close key on this platform: the throw-away tab was ended with Ctrl-D (u |
| terminal | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows (OCR): tab 2 at (483, 416), tab 3 at (485, 540), tab 4 at (483, |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (468, 415), tab 3 at (468, 540), tab 4 at (468, 664); tab 3's title was located by OCR (no `appci_find` line for 'v |
| terminal | keybinding_rebind | PASS | control: Alt-R not played: macOS: Option-R types a character by design unless Option is set to act as Meta (terminal.input.extra_meta_keys); |
| terminal | needs_owner | FAIL | NEEDS OWNER: a macOS system dialog is in front and would take our keys: window of UserNotificationCenter (pid 13893) on screen: 13893	UserNo |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | big_paste | PASS | big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); injector accounting: injector sent 21; app saw 21 |
| terminal | subshell_ready_relay_shape | FAIL | the setup was written behind sudo: not after Ctrl-I and not within 10 s of the first typed line's prompt (the first new prompt behind the re |
| terminal | job_control | PASS | injector accounting: injector sent 231; app saw 231; lost-before-app 0 (Enter: sent 11, app saw 11); of the app's: 217 unbound keydown/typed |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.38 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| terminal | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 2; app saw 2; lost |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| terminal | flood_scrollback_find | NOREF | flood_scrollback_find; app log: 31 find_scroll hop(s), moved=true in 1, last: result=scroll block=127 moved=true viewport_lines=52.11 render |
| terminal | remote_heredoc_paste | PASS | injector accounting: injector sent 403; app saw 403; lost-before-app 0 (Enter: sent 19, app saw 19); of the app's: 385 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_failing_not_local_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_readline_keys | NOREF | injector accounting: injector sent 211; app saw 210; lost-before-app 1 (Enter: sent 3, app saw 3); of the app's: 199 unbound keydown/typed,  |
| terminal | hostile_output_recovers | PASS | largest main-thread item after the output 0.0 ms (< 16 ms; before: 5.7 ms); injector accounting: injector sent 686; app saw 686; lost-before |
| terminal | latency_enter_ssh | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| terminal | intl_compose_linux | NOREF | injector accounting: injector sent 703; app saw 695; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| terminal | history_up_recall | PASS | injector accounting: injector sent 135; app saw 135; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 193; app saw 193; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 179 unbound keydown/typed,  |
| terminal | ime_compose | FAIL | ime_nihao_raw: marked text / dead key leaked ['6e', '69', '68', '61', '6f'] to the pty before the commit; ime_nihao_raw: pty received 20, ex |
| terminal | scroll_keys_foreground | PASS | injector accounting: injector sent 181; app saw 181; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 147 unbound keydown/typed,  |
| terminal | ui_copy_wrapped_output | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 217 unbound keydown/typed,  |
| terminal | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 230; app saw 230; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 219 unbound keydown/typed,  |
| terminal | selection_modes | PASS | injector accounting: injector sent 289; app saw 289; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 267 unbound keydown/typed,  |
| terminal | primary_and_middle_click | FAIL | [copy_on_select off] `echo ` + middle click in the input + ` >> pm_off_t73_m6t1u.txt`: the shell received ['echo SENTINEL_NOT_THE_PASTE >> p |
| terminal | link_and_path_click | FAIL | Ctrl+click on `paxywuhde.rs:140`: nothing opened: no call to the desktop opener and the file's text is not on screen; injector accounting: i |
| terminal | resize_reflow_output | PASS | injector accounting: injector sent 634; app saw 634; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 585 unbound keydown/typed |
| terminal | editor_daily | FAIL | a file changed on disk with no unsaved edits was not reloaded: an edit typed and saved afterwards wrote 'AFTER_T106_t14xb5_alpha line one\ns |
| terminal | pane_resize_maximize | FAIL | command palette 'move divider left' twice: the right pane went -1 columns and the left pane +1: expected the right one wider and the left on |
| terminal | focus_cycle | PASS | injector accounting: injector sent 349; app saw 349; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 339 unbound keydown/typed,  |
| terminal | gitbash_typeahead_reps | SKIP | Git Bash pane only (#1393: the ConPTY prompt hold of a bash pane on Windows); injector accounting: injector sent 2; app saw 2; lost-before-a |
| terminal | minimize_fullscreen | PASS | injector accounting: injector sent 890; app saw 890; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 855 unbound keydown/typed |
| terminal | desktop_notification | PASS | injector accounting: injector sent 311; app saw 311; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 301 unbound keydown/typed,  |
| terminal | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| terminal | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 1305x852.76 (1 held edit(s)); no block was painted without a width; with PHOSPHOR_MD_FIRST_LA |
| terminal | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal | restore_shows_history | PASS | injector accounting: injector sent 470; app saw 470; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 257 unbound keydown/typed,  |
| terminal | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 3413; app saw 3413; lost-before-app 0 ( |
| terminal | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows (OCR): tab 2 at (125, 156), tab 3 at (126, 219), tab 4 at (125, |
| terminal | tab_pointer_use | NOREF | vertical panel: tab 2 at (125, 155), tab 3 at (125, 218), tab 4 at (125, 280); NO-REFERENCE: UNVERIFIABLE: horizontal bar: the tabs' titles  |
| terminal | network_gate_switch | NOREF | NO-REFERENCE: UNVERIFIABLE: the denial notice (toast naming the purpose and host), its 'Allow this origin' link and the log row's 'Allow' bu |
| terminal | depth_check_off_before_tab_lifecycle_keys | SKIP | root shell pid not recorded after relaunch_by_case: the PID probe got no answer; the depth check is off until the next relaunch |
| terminal | tab_lifecycle_keys | FAIL | precondition: before the case opened any tab, the pane that has the keyboard answered no command ('<none>': no shell is in front), expected  |
| terminal | pane_wedged_after_tab_lifecycle_keys | FAIL | the default pane did not run a marker command (no effect file) after tab_lifecycle_keys, twice: the app is relaunched so the remaining cases |
| terminal | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| terminal | keybinding_rebind | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 129 unbound keydown/typed,  |
| terminal | soak | PASS | soak: window medians [369, 378, 383, 387, 390, 401, 404, 411, 413, 420] MB, monotonic=True, growth +51 MB (limit 150 MB); injector accountin |
| terminal | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 15 frame(s) drawn after the rebuild; 4 frame(s) |
| terminal | upgrade_over_previous | NOREF | the previous build's state is packed in /home/<user>/builds/r_hQtIdMf/0/winters-phosphor/phosphor/appci-out/wayland/run/fixtures/prev-state. |
| terminal | cb_ui_action_latency | PASS | injector accounting: injector sent 471; app saw 471; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 451 unbound keydown/typed,  |
| terminal | meta_and_word_keys | NOREF | injector accounting: injector sent 761; app saw 760; lost-before-app 1 (Enter: sent 18, app saw 18); of the app's: 616 unbound keydown/typed |
| terminal | pane_not_local_after_meta_and_word_keys | FAIL | after meta_and_word_keys the pane's shell was not the root shell (none; root pid 3727867); the app is relaunched so later cases start clean |
| agent | big_paste | PASS | big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); injector accounting: injector sent 21; app saw 21 |
| agent | subshell_ready_relay_shape | FAIL | the setup was written behind sudo: not after Ctrl-I and not within 10 s of the first typed line's prompt (the first new prompt behind the re |
| agent | job_control | PASS | injector accounting: injector sent 230; app saw 230; lost-before-app 0 (Enter: sent 11, app saw 11); of the app's: 217 unbound keydown/typed |
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.38 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| agent | routing_table_powershell | SKIP | Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log); injector accounting: injector sent 1; app saw 1; lost |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 155; app saw 155; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 138 unbound keydown/typed |
| agent | flood_scrollback_find | PASS | flood_scrollback_find; app log: 31 find_scroll hop(s), moved=true in 1, last: result=scroll block=130 moved=true viewport_lines=49.40 render |
| agent | remote_heredoc_paste | PASS | injector accounting: injector sent 403; app saw 403; lost-before-app 0 (Enter: sent 19, app saw 19); of the app's: 385 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_failing_not_local_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ui_readline_keys | NOREF | injector accounting: injector sent 210; app saw 209; lost-before-app 1 (Enter: sent 3, app saw 3); of the app's: 199 unbound keydown/typed,  |
| agent | hostile_output_recovers | PASS | largest main-thread item after the output 4.2 ms (< 16 ms; before: 0.0 ms); injector accounting: injector sent 686; app saw 686; lost-before |
| agent | latency_enter_ssh | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start over ssh: enter_ |
| agent | intl_compose_linux | NOREF | injector accounting: injector sent 702; app saw 694; lost-before-app 8 (Enter: sent 6, app saw 6); of the app's: 682 unbound keydown/typed,  |
| agent | history_up_recall | PASS | injector accounting: injector sent 134; app saw 134; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 114 unbound keydown/typed,  |
| agent | scroll_keys_position | FAIL | block-up then Home did not move the view above where PageUp left it (numbered lines on screen 462..500, before 316..363; shots/*skp_3_top.pn |
| agent | ime_compose | FAIL | ime_nihao_raw: marked text / dead key leaked ['6e', '69', '68', '61', '6f'] to the pty before the commit; ime_nihao_raw: pty received 20, ex |
| agent | scroll_keys_foreground | PASS | injector accounting: injector sent 179; app saw 179; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 147 unbound keydown/typed,  |
| agent | ui_copy_wrapped_output | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 217 unbound keydown/typed,  |
| agent | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 230; app saw 230; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 219 unbound keydown/typed,  |
| agent | selection_modes | PASS | injector accounting: injector sent 289; app saw 289; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 267 unbound keydown/typed,  |
| agent | primary_and_middle_click | FAIL | [copy_on_select off] `echo ` + middle click in the input + ` >> pm_off_t78_wihqj.txt`: the shell received ['echo SENTINEL_NOT_THE_PASTE >> p |
| agent | link_and_path_click | NOREF | NO-REFERENCE: Wayland harness cannot synthesize pointer input / read this text by OCR reliably; feature verified manually by the owner on a  |
| agent | resize_reflow_output | PASS | injector accounting: injector sent 637; app saw 637; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 588 unbound keydown/typed |
| agent | editor_daily | FAIL | a file changed on disk with no unsaved edits was not reloaded: an edit typed and saved afterwards wrote 'AFTER_T110_424kr7_alpha line one\ns |
| agent | pane_resize_maximize | FAIL | command palette 'move divider left' twice: the right pane went -1 columns and the left pane +1: expected the right one wider and the left on |
| agent | focus_cycle | PASS | injector accounting: injector sent 348; app saw 348; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 339 unbound keydown/typed,  |
| agent | gitbash_typeahead_reps | SKIP | Git Bash pane only (#1393: the ConPTY prompt hold of a bash pane on Windows); injector accounting: injector sent 2; app saw 2; lost-before-a |
| agent | minimize_fullscreen | PASS | injector accounting: injector sent 889; app saw 889; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 855 unbound keydown/typed |
| agent | desktop_notification | PASS | injector accounting: injector sent 309; app saw 309; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 301 unbound keydown/typed,  |
| agent | drag_drop_file | NOREF | NO-REFERENCE: UNVERIFIABLE: the drag source reported no drag at all (events []): the pointer press and motion did not start a drag on it. Th |
| agent | md_viewer | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| agent | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 1305x852.76 (1 held edit(s)); no block was painted without a width; with PHOSPHOR_MD_FIRST_LA |
| agent | restore_shows_history | PASS | injector accounting: injector sent 658; app saw 658; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 310 unbound keydown/typed,  |
| agent | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 3413; app saw 3413; lost-before-app 0 ( |
| agent | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: the panel rows of tabs 2..4 could not be read by OCR (found  |
| agent | tab_pointer_use | NOREF | NO-REFERENCE: UNVERIFIABLE: vertical panel: the tabs' titles could not be located in the strip (the rows are not top to bottom in tab order: |
| agent | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent | gpu_real_render | NOREF | 'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers (a renderer test |
| agent | keybinding_rebind | PASS | injector accounting: injector sent 181; app saw 181; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 129 unbound keydown/typed,  |
| agent | soak | PASS | soak: window medians [349, 390, 397, 404, 409, 414, 420, 428, 434, 442] MB, monotonic=True, growth +93 MB (limit 150 MB); injector accountin |
| agent | gpu_lost_device_recovers | NOREF | device reported lost once; renderer rebuilt on Intel(R) UHD Graphics 630 (CFL GT2) (Vulkan); 17 frame(s) drawn after the rebuild; 5 frame(s) |
| agent | meta_and_word_keys | NOREF | injector accounting: injector sent 811; app saw 810; lost-before-app 1 (Enter: sent 19, app saw 19); of the app's: 665 unbound keydown/typed |
| agent@fake | desktop_notification_agent_question | FAIL | question: the card arrived while the reference terminal had the focus: the notification service received 2 notification(s), expected 1: ["'F |
| agent@fake | agent_close_undo_call_status | PASS | the close key asked first (a waiting agent turn): answered with Enter; injector accounting: injector sent 386; app saw 386; lost-before-app  |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_lrc_no_false_takeover | wayland | agent | PASS |
| big_paste | macos | terminal | PASS |
| big_paste | wayland | agent | PASS |
| big_paste | wayland | terminal | PASS |
| context_menu_actions | macos | terminal | PASS |
| editor_daily | macos | terminal | NOREF |
| editor_daily | wayland | agent | FAIL |
| editor_daily | wayland | terminal | FAIL |
| flood_scrollback_find | macos | terminal | PASS |
| flood_scrollback_find | wayland | agent | PASS |
| flood_scrollback_find | wayland | terminal | NOREF |
| focus_cycle | macos | terminal | PASS |
| focus_cycle | wayland | agent | PASS |
| focus_cycle | wayland | terminal | PASS |
| gitbash_typeahead_reps | macos | terminal | SKIP |
| gitbash_typeahead_reps | wayland | agent | SKIP |
| gitbash_typeahead_reps | wayland | terminal | SKIP |
| gpu_real_render | wayland | agent | NOREF |
| gpu_real_render | wayland | terminal | NOREF |
| history_up_recall | macos | terminal | PASS |
| history_up_recall | wayland | agent | PASS |
| history_up_recall | wayland | terminal | PASS |
| hostile_output_recovers | macos | terminal | PASS |
| hostile_output_recovers | wayland | agent | PASS |
| hostile_output_recovers | wayland | terminal | PASS |
| job_control | macos | terminal | PASS |
| job_control | wayland | agent | PASS |
| job_control | wayland | terminal | PASS |
| keybinding_rebind | macos | terminal | PASS |
| keybinding_rebind | wayland | agent | PASS |
| keybinding_rebind | wayland | terminal | PASS |
| link_and_path_click | macos | terminal | FAIL |
| link_and_path_click | wayland | agent | NOREF |
| link_and_path_click | wayland | terminal | FAIL |
| mac_pointer_chord_proof | macos | terminal | FAIL |
| meta_and_word_keys | wayland | agent | NOREF |
| meta_and_word_keys | wayland | terminal | NOREF |
| minimize_fullscreen | macos | terminal | FAIL |
| minimize_fullscreen | wayland | agent | PASS |
| minimize_fullscreen | wayland | terminal | PASS |
| network_gate_switch | wayland | terminal | NOREF |
| pane_resize_maximize | wayland | agent | FAIL |
| pane_resize_maximize | wayland | terminal | FAIL |
| precmd_question_echo | macos | terminal | PASS |
| precmd_secret_honor_ps1 | macos | terminal | PASS |
| primary_and_middle_click | wayland | agent | FAIL |
| primary_and_middle_click | wayland | terminal | FAIL |
| remote_tab_enter_no_helper | macos | terminal | PASS |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | PASS |
| remote_tab_no_helper | macos | terminal | PASS |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | PASS |
| resize_reflow_output | macos | terminal | PASS |
| resize_reflow_output | wayland | agent | PASS |
| resize_reflow_output | wayland | terminal | PASS |
| restore_shows_history | macos | terminal | PASS |
| restore_shows_history | wayland | agent | PASS |
| restore_shows_history | wayland | terminal | PASS |
| routing_table_powershell | macos | terminal | SKIP |
| routing_table_powershell | wayland | agent | SKIP |
| routing_table_powershell | wayland | terminal | SKIP |
| scroll_keys_foreground | macos | terminal | PASS |
| scroll_keys_foreground | wayland | agent | PASS |
| scroll_keys_foreground | wayland | terminal | PASS |
| scroll_keys_position | macos | terminal | PASS |
| scroll_keys_position | wayland | agent | FAIL |
| scroll_keys_position | wayland | terminal | PASS |
| selection_modes | macos | terminal | PASS |
| selection_modes | wayland | agent | PASS |
| selection_modes | wayland | terminal | PASS |
| soak | wayland | agent | PASS |
| soak | wayland | terminal | PASS |
| ssh_gitbash_bootstrap_in_command | macos | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | wayland | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | wayland | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | macos | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | wayland | agent | SKIP |
| ssh_gitbash_failing_not_local_managed | wayland | terminal | SKIP |
| ssh_gitbash_kill_switch | macos | terminal | SKIP |
| ssh_gitbash_kill_switch | wayland | agent | SKIP |
| ssh_gitbash_kill_switch | wayland | terminal | SKIP |
| ssh_gitbash_managed | macos | terminal | SKIP |
| ssh_gitbash_managed | wayland | agent | SKIP |
| ssh_gitbash_managed | wayland | terminal | SKIP |
| ssh_gitbash_typeahead_managed | macos | terminal | SKIP |
| ssh_gitbash_typeahead_managed | wayland | agent | SKIP |
| ssh_gitbash_typeahead_managed | wayland | terminal | SKIP |
| subshell_ready_relay_shape | macos | terminal | SKIP |
| subshell_ready_relay_shape | wayland | agent | FAIL |
| subshell_ready_relay_shape | wayland | terminal | FAIL |
| tab_lifecycle_keys | wayland | agent | PASS |
| tab_lifecycle_keys | wayland | terminal | FAIL |
| tab_pointer_use | macos | terminal | NOREF |
| tab_pointer_use | wayland | agent | NOREF |
| tab_pointer_use | wayland | terminal | NOREF |
| ui_copy_wrapped_drag | macos | terminal | PASS |
| ui_copy_wrapped_drag | wayland | agent | PASS |
| ui_copy_wrapped_drag | wayland | terminal | PASS |
| ui_copy_wrapped_output | macos | terminal | PASS |
| ui_copy_wrapped_output | wayland | agent | PASS |
| ui_copy_wrapped_output | wayland | terminal | PASS |
| ui_readline_keys | wayland | agent | NOREF |
| ui_readline_keys | wayland | terminal | NOREF |
| ui_vertical_tabs_many | macos | terminal | PASS |
| ui_vertical_tabs_many | wayland | agent | PASS |
| ui_vertical_tabs_many | wayland | terminal | PASS |
| ui_vertical_tabs_panel_click | macos | terminal | PASS |
| ui_vertical_tabs_panel_click | wayland | agent | NOREF |
| ui_vertical_tabs_panel_click | wayland | terminal | PASS |
| zero_network_startup | macos | terminal | PASS |
| zero_network_startup | wayland | terminal | PASS |

passed everywhere it ran (21, candidates for the owner's decision): agent_lrc_no_false_takeover, big_paste, context_menu_actions, focus_cycle, history_up_recall, hostile_output_recovers, job_control, keybinding_rebind, precmd_question_echo, precmd_secret_honor_ps1, remote_tab_enter_no_helper, remote_tab_no_helper, resize_reflow_output, restore_shows_history, scroll_keys_foreground, selection_modes, soak, ui_copy_wrapped_drag, ui_copy_wrapped_output, ui_vertical_tabs_many, zero_network_startup
not passed somewhere (fail / skip / no-reference): editor_daily, flood_scrollback_find, gitbash_typeahead_reps, gpu_real_render, link_and_path_click, mac_pointer_chord_proof, meta_and_word_keys, minimize_fullscreen, network_gate_switch, pane_resize_maximize, primary_and_middle_click, routing_table_powershell, scroll_keys_position, ssh_gitbash_bootstrap_in_command, ssh_gitbash_failing_not_local_managed, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, subshell_ready_relay_shape, tab_lifecycle_keys, tab_pointer_use, ui_readline_keys, ui_vertical_tabs_panel_click
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: wayland/run_slim/logs/agent/acct/launch0_1794436057.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794436057.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.2.log.gz: 4265 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.7.log.gz: 3558 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794429488.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794429488.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794429488.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794429488.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794429488.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4155 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794412809.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794412809.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794412809.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794412809.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794413575.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 409 KB (2843 of 4422 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/case_latency_enter_ssh.log: latency evidence capped at 409 KB (2756 of 4090 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/case_latency_first_key_after_idle.log: latency evidence capped at 409 KB (2563 of 4466 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 409 KB (2811 of 18024 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/terminal/case_latency_first_key_after_idle.log: latency evidence capped at 409 KB (2569 of 4430 hop lines kept; the platform budget of 2048 KB is shared between 6 logs)
- note: wayland/run_slim/logs/agent/acct/launch0_1794420166.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794420166.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794421172.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794425000.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794425000.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794425000.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794425000.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794425000.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794413697.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794414899.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794418878.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794413697.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794418878.log.in_session.2: 10240 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794418878.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794418878.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: run dir is 8.3 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing [test-linux]`
- FIXED failures (unit): none
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_remote_dir.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_remote_dir.nu.150.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/pane_not_local_after_lab_path_detect.p_exec.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_path_detect.p_tinypath.lan.nohelper (case not in previous run)`
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
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.lan.nohelper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.150.nohelper`
  - `lab-wayland/terminal/lab_no_helper_chip.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_no_helper_chip.fish.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_no_helper_chip.nu.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_nu_agent_pager_env.nu.lan.nohelper (case not in previous run)`
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
  - ... and 126 more
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r13a (3051722998); test-warpui-core: build/r13a (3051722998); test-workspace-rest: build/r13a (3051722998); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998)
