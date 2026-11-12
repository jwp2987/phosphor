# build/r27b  e78b7c86b  (pipeline 3061436969)

- date: 2026-11-12, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.helper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_nohelper_tab_hands_back.zsh.150.nohelper`, `lab_nohelper_tab_hands_back.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
  - asked for on macos-lab: `lab_agent_nld_predict_ssh.bash.lan.nohelper`, `lab_chips.bash.150.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.fish.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_chips.nu.150.nohelper`, `lab_chips.nu.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_ctrl_c_no_esc_i.nu.150.nohelper`, `lab_ctrl_c_no_esc_i.zsh.150.helper`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`, `lab_enter_start.fish.150.helper`, `lab_enter_start.fish.150.nohelper`, `lab_enter_start.zsh.150.helper`, `lab_fam_fs_basic.bash.150.helper`, `lab_fam_fs_basic.bash.lan.helper`, `lab_fam_fs_basic.fish.150.helper`, `lab_fam_fs_basic.fish.lan.helper`, `lab_fam_fs_basic.nu.150.helper`, `lab_fam_fs_basic.nu.lan.helper`, `lab_fam_fs_basic.zsh.150.helper`, `lab_fam_fs_basic.zsh.lan.helper`, `lab_fam_git_read.bash.150.helper`, `lab_fam_git_read.bash.150.nohelper`, `lab_fam_git_read.bash.lan.helper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.fish.150.helper`, `lab_fam_git_read.fish.150.nohelper`, `lab_fam_git_read.fish.lan.helper`, `lab_fam_git_read.fish.lan.nohelper`, `lab_fam_git_read.nu.150.helper`, `lab_fam_git_read.nu.150.nohelper`, `lab_fam_git_read.nu.lan.helper`, `lab_fam_git_read.nu.lan.nohelper`, `lab_fam_git_read.zsh.150.helper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_git_read.zsh.lan.helper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.150.helper`, `lab_fam_run_command.bash.150.nohelper`, `lab_fam_run_command.bash.lan.helper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.fish.150.helper`, `lab_fam_run_command.fish.150.nohelper`, `lab_fam_run_command.fish.lan.helper`, `lab_fam_run_command.fish.lan.nohelper`, `lab_fam_run_command.nu.150.helper`, `lab_fam_run_command.nu.150.nohelper`, `lab_fam_run_command.nu.lan.helper`, `lab_fam_run_command.nu.lan.nohelper`, `lab_fam_run_command.zsh.150.helper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_fam_run_command.zsh.lan.helper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_freeze_and_wake.bash.lan.nohelper`, `lab_gh_chip_remote.fish.lan.nohelper`, `lab_gh_chip_remote.nu.lan.nohelper`, `lab_gh_chip_remote.zsh.lan.nohelper`, `lab_git_write_remote.bash.lan.nohelper`, `lab_git_write_remote.fish.lan.nohelper`, `lab_git_write_remote.nu.lan.nohelper`, `lab_git_write_remote.zsh.lan.nohelper`, `lab_helper_killed.bash.150.helper`, `lab_helper_killed.bash.lan.helper`, `lab_helper_killed.zsh.150.helper`, `lab_helper_killed.zsh.lan.helper`, `lab_helper_reconnect.zsh.150.helper`, `lab_helper_reconnect.zsh.lan.helper`, `lab_helper_route_unsupported.bash.lan.helper`, `lab_nohelper_listing_single_flight.bash.150.nohelper`, `lab_nohelper_listing_single_flight.bash.lan.nohelper`, `lab_nohelper_listing_single_flight.fish.150.nohelper`, `lab_nohelper_listing_single_flight.fish.lan.nohelper`, `lab_nohelper_listing_single_flight.nu.150.nohelper`, `lab_nohelper_listing_single_flight.nu.lan.nohelper`, `lab_nohelper_listing_single_flight.zsh.150.nohelper`, `lab_nohelper_listing_single_flight.zsh.lan.nohelper`, `lab_path_detect.nu.150.helper`, `lab_path_detect.nu.150.nohelper`, `lab_path_detect.nu.lan.helper`, `lab_path_detect.nu.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_raw_key_arrival.bash.150.helper`, `lab_raw_key_arrival.bash.150.nohelper`, `lab_raw_key_arrival.bash.lan.helper`, `lab_raw_key_arrival.bash.lan.nohelper`, `lab_raw_key_arrival.zsh.150.helper`, `lab_raw_key_arrival.zsh.150.nohelper`, `lab_raw_key_arrival.zsh.lan.helper`, `lab_raw_key_arrival.zsh.lan.nohelper`, `lab_remote_edit.bash.lan.nohelper`, `lab_remote_edit.fish.lan.nohelper`, `lab_remote_edit.nu.lan.nohelper`, `lab_remote_edit.zsh.lan.nohelper`, `lab_ssh_drop_reconnect.nu.150.helper`, `lab_ssh_drop_reconnect.nu.150.nohelper`, `lab_ssh_drop_reconnect.nu.lan.helper`, `lab_ssh_drop_reconnect.nu.lan.nohelper`, `lab_tab_remote_dir.bash.150.helper`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.bash.lan.helper`, `lab_tab_remote_dir.bash.lan.nohelper`, `lab_tab_remote_dir.fish.150.helper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.fish.lan.helper`, `lab_tab_remote_dir.fish.lan.nohelper`, `lab_tab_remote_dir.nu.150.helper`, `lab_tab_remote_dir.nu.150.nohelper`, `lab_tab_remote_dir.nu.lan.helper`, `lab_tab_remote_dir.nu.lan.nohelper`, `lab_tab_remote_dir.zsh.150.helper`, `lab_tab_remote_dir.zsh.150.nohelper`, `lab_tab_remote_dir.zsh.lan.helper`, `lab_tab_remote_dir.zsh.lan.nohelper`
- unit: 33609 passed, 0 failed, 113 skipped of 33609 run (6 suites)
- app (pass/fail/skip): lab-macos 13/44/0 +107 no-ref; lab-wayland 14/62/0 +92 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 96.2 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 106.0 | MacBookPro-gitrunner |  |
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
| build-linux-x86_64 | success | 30.7 | - |  |
| build-macos-arm64 | success | 18.8 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 3.2 | - |  |
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
| sshlab-image | success | 0.3 | - |  |
| test-linux | success | 17.9 | - |  |
| test-macos | success | 29.0 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | success | 1.4 | - |  |
| test-workspace-rest-macos | success | 3.6 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab then Enter 23 ms later: the shell received ['cd /work/u_zsh/tabfx_t113sb09g/zzqt113sb09g'], want exactly one line, the completed 'cd /wo |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 81332); the app is relaunched so later cas |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 6 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1487; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_nohelper_listing_single_flight.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_nohelper_listing_single_flight.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] the app logged no `hop=listing_inflight` line: single-flight is not proven; injector accounting: injector |
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t3410wr5cy.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t344t0po71.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t347aaj4jw.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t350p99jiz.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no poin |
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
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est409yfbet $EPOCHREALTIME >> /work/u_zsh/es_t40896n3.txt', 'echo est41 |
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
| terminal | lab_fam_run_command.fish.150.helper | FAIL | branch brt761o26k4o not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt772pkhfvm: a |
| terminal | lab_helper_killed.bash.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt779k4he4g: a |
| terminal | lab_helper_killed.zsh.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab then Enter 23 ms later: the shell received ['cd /work/u_zsh/tabfx_t113sb09g/zzqt113sb09g'], want exactly one line, the completed 'cd /wo |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 28773); the app is relaunched so later cas |
| agent | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1487; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged; NO-REFERENCE: UNVERIFIABLE: th |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | injector accounting: injector sent 734; app saw 734; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 691 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_freeze_and_wake.bash.lan.nohelper | NOREF | NO-REFERENCE: keys typed while frozen: the line typed while the app was frozen never arrived (the display system did not hold the keys); the |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 39447 -> 40844); a relaunched app starts in a terminal session, so this is |
| agent | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
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
| terminal | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab then Enter 23 ms later: the shell received ['cd /work/u_zsh/tabfx_t113sb09g/zzqt113sb09g'], want exactly one line, the completed 'cd /wo |
| terminal | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| terminal | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| terminal | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| terminal | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 174219); the app is relaunched so later ca |
| terminal | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; the app log says terminal but the screen reads as a |
| terminal | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1487; |
| terminal | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.nu.lan.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.nohelper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 50 ms] effect file: expected ['t242msrq4jwj', 't243eqe5n3tc', 't244a5111x5m', 't245sgfkvx1y', 't246d1mho4yl'], got ['t242msrq4jwj',  |
| terminal | lab_fam_git_read.bash.lan.nohelper | FAIL | branch brt254le5pne not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.bash.150.nohelper | FAIL | branch brt258mj9qex not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.zsh.lan.nohelper | FAIL | branch brt2627rl419 not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.zsh.150.nohelper | FAIL | branch brt2669h11xo not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.fish.lan.nohelper | FAIL | branch brt270vlddx3 not on screen after cd into the repo on the remote; shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.fish.150.nohelper | FAIL | branch brt274xwj3m8 not on screen after cd into the repo on the remote; shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.nu.lan.nohelper | FAIL | branch brt278niosgf not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..promp |
| terminal | lab_fam_git_read.nu.150.nohelper | FAIL | branch brt2826nt5dy not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.bash.lan.nohelper | FAIL | branch brt286ttn1a3 not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.bash.150.nohelper | FAIL | branch brt290pgk6dy not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.zsh.lan.nohelper | FAIL | shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
| terminal | lab_fam_run_command.zsh.150.nohelper | FAIL | branch brt298yp7a9c not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.fish.lan.nohelper | FAIL | branch brt302ci2l2j not on screen after cd into the repo on the remote; shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.fish.150.nohelper | FAIL | branch brt3066yf6he not on screen after cd into the repo on the remote; shell arm: 7 out-of-band channels for 2 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.nu.lan.nohelper | FAIL | branch brt310xnc70o not on screen after cd into the repo on the remote; shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..promp |
| terminal | lab_fam_run_command.nu.150.nohelper | FAIL | shell arm: 5 out-of-band channels for 4 prompt(s), expected 1..prompts; exec_channel_timeline (s after the window opened, backend_route line |
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
| terminal | lab_remote_edit.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t348jwep99.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t351izivmi.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t354hicrrw.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_remote_edit.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the keyboard driver did not open zz_t357jae31z.txt in the code editor: the palette's 'Toggle project explorer',  |
| terminal | lab_git_write_remote.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
| terminal | lab_git_write_remote.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the word 'Commit' was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, th |
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
| terminal | lab_path_detect.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_path_detect.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] no hop= |
| terminal | lab_enter_start.zsh.150.helper | FAIL | rec.log: the shell received exactly the typed lines: expected ['echo est416r45wh $EPOCHREALTIME >> /work/u_zsh/es_t415p5as.txt', 'echo est41 |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_raw_key_arrival.bash.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.bash.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.lan.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_raw_key_arrival.zsh.150.helper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| terminal | lab_ssh_drop_reconnect.nu.lan.helper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2773 unbound keydown/ty |
| terminal | lab_ssh_drop_reconnect.nu.150.helper | PASS | injector accounting: injector sent 2842; app saw 2842; lost-before-app 0 (Enter: sent 63, app saw 63); of the app's: 2774 unbound keydown/ty |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] the shell received [], expected exactly [sleep 30 |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | FAIL | [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x1 gap 50 ms] effect file: expected ['t674awbxclf4', 't676ok6i0 |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x2 gap 50 ms] effect file: expected ['t696a06ffalu', 't697kpa2amo6', 't698fsnetgu5', 't699a91alj89', 't700yjktzkis'], got ['t696a06ffalu',  |
| terminal | lab_helper_reconnect.zsh.lan.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_helper_reconnect.zsh.150.helper | FAIL | reconnect: a command typed into the session ran 0 times, expected once; injector accounting: injector sent 134; app saw 134; lost-before-app |
| terminal | lab_fam_fs_basic.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.bash.150.helper | FAIL | branch brt721r9c77d not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_fs_basic.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_fs_basic.zsh.150.helper | FAIL | branch brt728k5lfmo not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_fs_basic.fish.lan.helper | FAIL | branch brt732o26k4o not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_fs_basic.fish.150.helper | FAIL | branch brt736lxv3me not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_fs_basic.nu.lan.helper | FAIL | branch brt740ccxhsi not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_fs_basic.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.bash.lan.helper | FAIL | branch brt747xkhp43 not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.zsh.lan.helper | FAIL | branch brt754uwcx6v not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_git_read.zsh.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_git_read.fish.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.fish.150.helper | FAIL | branch brt7647w43wf not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_git_read.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_git_read.nu.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.bash.150.helper | NOREF | helper takeover settled: executor_swapped=True pending_syncs=0 syncs=0 shell_routes_after_swap=0; NO-REFERENCE: UNVERIFIABLE: [hook] the app |
| terminal | lab_fam_run_command.zsh.lan.helper | FAIL | branch brt780r4gqvt not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_run_command.zsh.150.helper | FAIL | branch brt784o8m3e8 not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.fish.lan.helper | FAIL | branch brt788i9jgvh not on screen after cd into the repo on the remote; helper takeover NOT settled within 30 s: executor_swapped=False pend |
| terminal | lab_fam_run_command.fish.150.helper | FAIL | branch brt792dja0kp not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_fam_run_command.nu.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; NO-REFERENCE: UNVERIFI |
| terminal | lab_fam_run_command.nu.150.helper | FAIL | branch brt799byfri4 not on screen after cd into the repo on the remote; helper takeover settled: executor_swapped=True pending_syncs=0 syncs |
| terminal | lab_helper_route_unsupported.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists i |
| terminal | lab_helper_killed.bash.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; branch brt804lxkera: a |
| terminal | lab_helper_killed.bash.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| terminal | lab_helper_killed.zsh.lan.helper | NOREF | helper takeover NOT settled within 30 s: executor_swapped=False pending_syncs=0 syncs=0 shell_routes_after_swap=None; app find hook gave no  |
| terminal | lab_helper_killed.zsh.150.helper | FAIL | no out-of-band channel after the helper died: the read did not use the shell arm; helper takeover settled: executor_swapped=True pending_syn |
| agent | lab_tab_remote_dir.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.nohelper | FAIL | Tab then Enter 23 ms later: the shell received ['cd /work/u_zsh/tabfx_t113sb09g/zzqt113sb09g'], want exactly one line, the completed 'cd /wo |
| agent | lab_tab_remote_dir.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_path_detect.nu.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.nu.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the |
| agent | lab_path_detect.p_exec.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: [hook] no hop=cwd_listing line with entries=2 after the cd (the |
| agent | pane_not_local_after_lab_path_detect.p_exec.lan.nohelper | FAIL | after lab_path_detect.p_exec.lan.nohelper the pane's shell was not the root shell (none; root pid 350314); the app is relaunched so later ca |
| agent | lab_chips.bash.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.bash.150.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.lan.nohelper | FAIL | 4 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.zsh.150.nohelper | FAIL | 6 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.150.nohelper | FAIL | 5 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.lan.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.nu.150.nohelper | FAIL | 4 side-call exec channels for 2 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1487; |
| agent | lab_raw_key_arrival.bash.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.bash.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.lan.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_raw_key_arrival.zsh.150.nohelper | NOREF | TOS of the app's ssh connection(s): ['0xb8']; plain ssh: 0xb8 (the same class); NO-REFERENCE: UNVERIFIABLE: the app log has 32 byte(s) in `p |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x2 gap 0 ms] no prompt after the Ctrl-C burst; [x2 gap 50 ms] effect file: expected ['t146r7vik1t2', 't147yb6qhsqb', 't1486losfk3x', 't1494 |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_tab_remote_dir.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | agent_view_not_restored_after_lab_tab_remote_dir.bash.lan.helper | PASS | lab_tab_remote_dir.bash.lan.helper relaunched the app itself (pid 375435 -> 376795); a relaunched app starts in a terminal session, so this  |
| agent | lab_tab_remote_dir.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
| agent | lab_tab_remote_dir.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: [hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not m |
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
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | FAIL | [x2 gap 50 ms] effect file: expected ['t35615cuoe1f', 't3574pwmsk2h', 't358q8tgsm3o', 't359ev3229o1', 't360fzzggxww'], got ['t35615cuoe1f',  |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.helper | PASS | after lab_ctrl_c_no_esc_i.nu.150.helper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window |
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run/junit.xml: 3152 KB skipped (over 1 MB)
- note: macos/run/results.json: 3674 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.4.log.gz: 1672 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794519251.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794519251.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794519251.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794519251.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 4882 KB skipped (over 1 MB)
- note: wayland/run/junit.xml: 4446 KB skipped (over 1 MB)
- note: wayland/run/results.jsonl: 4963 KB skipped (over 1 MB)
- note: wayland/run/results.json: 5009 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.4.log.gz: 1373 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794518392.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794518392.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794518392.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4381 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_remote_dir.zsh.150.nohelper (case not in previous run)`
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
  - `lab-wayland/terminal/lab_enter_start.zsh.150.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.lan.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.zsh.150.helper`
  - `lab-wayland/terminal/lab_ctrl_c_no_esc_i.nu.150.helper`
  - `lab-wayland/terminal/lab_helper_reconnect.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_helper_reconnect.zsh.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.bash.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.zsh.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.fish.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.fish.150.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_fs_basic.nu.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.bash.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/lab_fam_git_read.zsh.lan.helper (case not in previous run)`
  - ... and 20 more
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r27a (3060739487); test-macos: build/r27a (3060739487); test-warpui-core: build/r27a (3060739487); test-warpui-core-macos: build/r27a (3060739487); test-workspace-rest: build/r27a (3060739487); test-workspace-rest-macos: build/r27a (3060739487); app lab-wayland: build/r13a (3051722998)
