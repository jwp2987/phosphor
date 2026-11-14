# build/r30f  8476d176d  (pipeline 3068353168)

- date: 2026-11-14, tier: targeted, pipeline status: success
- compile (check-linux): ok, 125 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_connect_typeahead.bash.*`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`
  - asked for on linux-wayland: `expert_fuzz`, `fast_typing`, `enter_loss_control`, `key_loss_control`
  - asked for on macos: `expert_fuzz`, `fast_typing`, `enter_loss_control`, `key_loss_control`
  - asked for on macos-lab: `lab_connect_typeahead.zsh.*`, `lab_ctrl_c_no_esc_i.zsh.lan.helper`
  - asked for on windows: `routing_table_powershell`, `expert_fuzz`, `selection_modes`, `gitbash_typeahead_reps`, `enter_loss_control`
  - asked for on windows-lab: `lab_connect_typeahead.*`, `lab_chips.bash.lan.nohelper`
- unit: 50319 passed, 0 failed, 191 skipped of 50319 run (9 suites)
- app (pass/fail/skip): lab-macos 9/4/0; lab-wayland 11/0/0; lab-windows 7/24/18; macos 8/0/0; wayland 8/0/0; windows 18/1/8; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | success | 7.2 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 9.8 | MacBookPro-gitrunner |  |
| app-test-lab-windows | failed | 28.3 | phosphor-windows-vm |  |
| app-test-linux-wayland | success | 6.3 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | success | 6.7 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 18.2 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 31.2 | - |  |
| build-macos-arm64 | success | 18.8 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 37.3 | phosphor-windows-vm |  |
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
| guards | success | 8.7 | - |  |
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
| test-linux | success | 22.4 | - |  |
| test-macos | success | 29.2 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.2 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.0 | phosphor-windows-vm |  |
| test-windows | success | 45.5 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.6 | - |  |
| test-workspace-rest-macos | success | 3.6 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.0 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off` LEAK (51.8s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::off` LEAK (131.1s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.5s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.8s)
- `warp workspace::view::tests::test_active_tab_bar_position_id_tracks_layout` LEAK (0.7s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.3s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.1s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.2s)
- `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it` LEAK (2.2s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | SETTINGS LEFT CHANGED: the settings file differs from the one this case started on, in: warpify.ssh.remote_helper_denied_hosts; injector acc |
| terminal | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 2 (gap 0.0 s): the line typed behind `ssh lab-u_zsh` never reached the remote shell. Test host: sshd.log: debug1: session_by_pid: pid  |
| agent | pane_not_local_after_lab_connect_typeahead.zsh.lan.nohelper | FAIL | after lab_connect_typeahead.zsh.lan.nohelper the pane's shell was not the root shell (none; root pid 65065); the app is relaunched so later  |
| agent | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | SETTINGS LEFT CHANGED: the settings file differs from the one this case started on, in: warpify.ssh.remote_helper_denied_hosts; injector acc |
| agent | agent_view_not_restored_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | lab_ctrl_c_no_esc_i.zsh.lan.helper relaunched the app itself (pid 65746 -> 66806); a relaunched app starts in a terminal session, so this is |
| agent | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 4 (gap 0.1 s): the line typed behind `ssh lab-u_zsh` never reached the remote shell. Test host: sshd.log: debug1: session_by_pid: pid  |
| agent | pane_not_local_after_lab_connect_typeahead.zsh.lan.helper | FAIL | after lab_connect_typeahead.zsh.lan.helper the pane's shell was not the root shell (none; root pid 67392); the app is relaunched so later ca |
| agent | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | SETTINGS LEFT CHANGED: the settings file differs from the one this case started on, in: warpify.ssh.remote_helper_denied_hosts; injector acc |
| terminal | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | SETTINGS LEFT CHANGED: the settings file differs from the one this case started on, in: warpify.ssh.remote_helper_denied_hosts; injector acc |
| agent | agent_view_not_restored_after_lab_ctrl_c_no_esc_i.zsh.lan.helper | PASS | lab_ctrl_c_no_esc_i.zsh.lan.helper relaunched the app itself (pid 1110813 -> 1114246); a relaunched app starts in a terminal session, so thi |
| agent | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |

## app lab-windows (app-test-lab-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_chips.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.150.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.zsh.150.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.lan.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.150.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.zsh.lan.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.zsh.150.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | lab_chips.bash.lan.nohelper | FAIL | the app log has no update_chip_value ShellGitBranch -> brt35by1a2 line: the chip never took the branch; no side call at all with no helper:  |
| terminal@gitbash | lab_connect_typeahead.bash.lan.nohelper | FAIL | trial 3 (gap 0.0 s): the remote shell received ['exitssh lab-u_bash', 'echo T15_259owo >> ~/rl.txt'], want exactly ['echo T15_259owo >> ~/rl |
| terminal@gitbash | entered_agent_view_in_lab_connect_typeahead.bash.lan.nohelper | FAIL | after the TERMINAL case lab_connect_typeahead.bash.lan.nohelper the pane shows AGENT view: something the case typed was sent to the agent in |
| terminal@gitbash | lab_connect_typeahead.bash.150.nohelper | FAIL | trial 2 (gap 0.0 s): the line typed behind `ssh lab-u_bash` never reached the remote shell. Test host: sshd.log: 07:55:55 auth publickey use |
| terminal@gitbash | entered_agent_view_in_lab_connect_typeahead.bash.150.nohelper | FAIL | after the TERMINAL case lab_connect_typeahead.bash.150.nohelper the pane shows AGENT view: something the case typed was sent to the agent in |
| terminal@gitbash | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 2 (gap 0.0 s): the remote shell received ['exit', 'echo T40_4ctzkk >> ~/rl.txt'], want exactly ['echo T40_4ctzkk >> ~/rl.txt']; trial  |
| terminal@gitbash | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['exit', 'echo T53_9m605w >> ~/rl.txt'], want exactly ['echo T53_9m605w >> ~/rl.txt']; trial  |
| terminal@gitbash | lab_connect_typeahead.bash.lan.helper | FAIL | trial 2 (gap 0.0 s): the session was not managed (no ManagedSSHSessionStarting hook and no `verdict managed=true` in the app log of this con |
| terminal@gitbash | pane_switch_left_on_after_lab_connect_typeahead.bash.lan.helper | FAIL | after lab_connect_typeahead.bash.lan.helper: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: t |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | lab_connect_typeahead.bash.150.helper | FAIL | trial 2 (gap 0.0 s): the line typed behind `ssh lab-u_bash` never reached the remote shell. Test host: sshd.log: 07:55:55 connect from ('127 |
| terminal@gitbash | pane_switch_left_on_after_lab_connect_typeahead.bash.150.helper | FAIL | after lab_connect_typeahead.bash.150.helper: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: t |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['exit', 'echo T105_ys65bu >> ~/rl.txt'], want exactly ['echo T105_ys65bu >> ~/rl.txt']; tria |
| terminal@gitbash | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['exit', 'echo T120_vhsp5i >> ~/rl.txt'], want exactly ['echo T120_vhsp5i >> ~/rl.txt']; tria |
| agent | lab_chips.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.150.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.zsh.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.zsh.150.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.lan.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.150.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.zsh.lan.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.zsh.150.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | lab_chips.bash.lan.nohelper | FAIL | the app log has no update_chip_value ShellGitBranch -> brt35by1a2 line: the chip never took the branch; no side call at all with no helper:  |
| agent@gitbash | pane_switch_left_on_after_lab_chips.bash.lan.nohelper | FAIL | after lab_chips.bash.lan.nohelper: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: the proof l |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | lab_connect_typeahead.bash.lan.nohelper | FAIL | trial 2 (gap 0.0 s): the session was not managed (no ManagedSSHSessionStarting hook and no `verdict managed=true` in the app log of this con |
| agent@gitbash | lab_connect_typeahead.bash.150.nohelper | FAIL | trial 2 (gap 0.0 s): the session was not managed (no ManagedSSHSessionStarting hook and no `verdict managed=true` in the app log of this con |
| agent@gitbash | pane_switch_left_on_after_lab_connect_typeahead.bash.150.nohelper | FAIL | after lab_connect_typeahead.bash.150.nohelper: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | lab_connect_typeahead.zsh.lan.nohelper | FAIL | trial 2 (gap 0.0 s): the remote shell received ['exit', 'echo T51_605w0w >> ~/rl.txt'], want exactly ['echo T51_605w0w >> ~/rl.txt']; trial  |
| agent@gitbash | lab_connect_typeahead.zsh.150.nohelper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['exit', 'echo T64_sekkq7 >> ~/rl.txt'], want exactly ['echo T64_sekkq7 >> ~/rl.txt']; trial  |
| agent@gitbash | lab_connect_typeahead.bash.lan.helper | FAIL | trial 2 (gap 0.0 s): the session was not managed (no ManagedSSHSessionStarting hook and no `verdict managed=true` in the app log of this con |
| agent@gitbash | lab_connect_typeahead.bash.150.helper | FAIL | trial 2 (gap 0.0 s): the session was not managed (no ManagedSSHSessionStarting hook and no `verdict managed=true` in the app log of this con |
| agent@gitbash | lab_connect_typeahead.zsh.lan.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['exit', 'echo T114_xvvhsp >> ~/rl.txt'], want exactly ['echo T114_xvvhsp >> ~/rl.txt']; tria |
| agent@gitbash | lab_connect_typeahead.zsh.150.helper | FAIL | trial 1 (gap 0.0 s): the remote shell received ['exit', 'echo T128_hcmp0k >> ~/rl.txt'], want exactly ['echo T128_hcmp0k >> ~/rl.txt']; tria |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: injector sent 2058; app saw 2058; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1996 unbound keydown/ty |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 710; app saw 710; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 647 unbound keydown/typed |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot ga |
| terminal | fast_typing | PASS | injector accounting: injector sent 2072; app saw 2072; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2010 unbound keydown/ty |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 721; app saw 721; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 662 unbound keydown/typed |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| terminal | routing_table_powershell | PASS | 'make a PR' (ambiguous): no verdict, went to agent: no routing line was logged, so there is no rule to check; injector accounting: injector  |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| terminal@gitbash | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | expert_fuzz | PASS | injector accounting: injector sent 718; app saw 718; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 651 unbound keydown/typed |
| terminal@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| terminal@gitbash | selection_modes | FAIL | Ctrl-C did not end the streaming program: no prompt within 8 s (the SECOND Ctrl-C: the first copied the selection; if this one copied too, t |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8450; app saw 8450; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8217 unbound keydown/ |
| agent | mode_proof | PASS |  |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached th |
| agent | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| agent | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| agent@gitbash | mode_proof | PASS |  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | expert_fuzz | PASS | injector accounting: injector sent 750; app saw 750; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 678 unbound keydown/typed |
| agent@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| agent@gitbash | selection_modes | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops |
| agent@gitbash | gitbash_typeahead_reps | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+= |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| gitbash_typeahead_reps | windows | agent | SKIP |
| gitbash_typeahead_reps | windows | agent@gitbash | PASS |
| gitbash_typeahead_reps | windows | terminal | SKIP |
| gitbash_typeahead_reps | windows | terminal@gitbash | PASS |
| routing_table_powershell | windows | agent | PASS |
| routing_table_powershell | windows | agent@gitbash | SKIP |
| routing_table_powershell | windows | terminal | PASS |
| routing_table_powershell | windows | terminal@gitbash | SKIP |
| selection_modes | windows | agent | SKIP |
| selection_modes | windows | agent@gitbash | PASS |
| selection_modes | windows | terminal | SKIP |
| selection_modes | windows | terminal@gitbash | FAIL |

passed everywhere it ran (0, candidates for the owner's decision): none
not passed somewhere (fail / skip / no-reference): gitbash_typeahead_reps, routing_table_powershell, selection_modes
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: windows/run_slim/logs/agent@gitbash/phosphor.log.gz: 2010 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.log.gz: 1946 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off [test-windows]`
  - `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit [test-windows]`
- FIXED failures (unit): 
  - `warp util::git::tests::commit_chain_commit_only_creates_a_commit_and_reports_delta [test-windows]`
- NEW failures (app): 
  - `windows/terminal@gitbash/selection_modes (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r30e (3068181472); test-macos: build/r30e (3068181472); test-warpui-core: build/r30e (3068181472); test-warpui-core-macos: build/r30e (3068181472); test-warpui-core-windows: build/r30e (3068181472); test-windows: build/r30e (3068181472); test-workspace-rest: build/r30e (3068181472); test-workspace-rest-macos: build/r30e (3068181472); test-workspace-rest-windows: build/r30e (3068181472); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998); app windows: build/r13a (3051722998)
