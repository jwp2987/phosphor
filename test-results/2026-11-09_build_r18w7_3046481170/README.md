# build/r18w7  a10b7c89e  (pipeline 3046481170)

- date: 2026-11-09, tier: fix, pipeline status: manual
- compile (check-linux): ok, 110 warnings
- unit: 4430 passed, 0 failed, 26 skipped of 4430 run (2 suites)
- app (pass/fail/skip): lab-wayland 0/0/27; macos 52/37/23 +2 no-ref; wayland 51/47/13 +4 no-ref; windows 55/15/130 +4 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 1.5 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 121.9 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 134.9 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 83.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.7 | - |  |
| build-macos-arm64 | success | 18.7 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.1 | phosphor-windows-vm |  |
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
| guards | success | 2.3 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.4 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | manual | - | - | not run |
| test-linux | failed | 23.6 | - |  |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.3 | - |  |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | success | 2.3 | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp ai::agent_providers::tools::web_runtime::websearch_tests::real_exa_anonymous_search` FAIL (0.2s)
- `warp ai::blocklist::agent_fuzz::corpus_replay_tests::corpus_replays_as_listed_and_reaches_the_wave0_cells` FAIL (48.4s)

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_fam_agent_context.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.bash.150.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.zsh.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.zsh.150.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.fish.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.fish.150.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.nu.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.nu.150.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.bash.lan.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.bash.150.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.zsh.lan.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.zsh.150.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.fish.lan.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.fish.150.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.nu.lan.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| terminal | lab_fam_agent_context.nu.150.helper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_agent_remote_flow.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_agent_remote_flow.nu.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_agent_remote_errors.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_agent_remote_errors.nu.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_agent_remote_drop.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_agent_remote_drop.nu.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_fam_agent_edit_conflict.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_fam_agent_edit_conflict.nu.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_fam_agent_delete_conflict.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_fam_agent_edit_endings.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |
| agent | lab_fam_agent_edit_touch_only.bash.lan.nohelper | SKIP | labctl start failed (1): image localhost/phosphor-sshlab:full-025cf2afab536cb8 is not built: run the sshlab-image job (labctl.py build); inj |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2057; app saw 2055; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); pty writes: 0, of which carrying a CR (\r):  |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 0, app saw 12); pty writes: 0, of which carrying a CR ( |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ctrl_c_flood_ab | PASS | gate on vs off: Δp50 = -18.41 ms (on 180.07 / off 198.48; p95 195.94 / 210.43; n 30 / 30; blocks of 5, relaunch per block); gate on: 4611 fr |
| terminal | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| terminal | subshell_ready_rc_questions | FAIL | rcq: exact lines in the inner shell then the pane's bash: got ['T171_bg1vve'], wanted ['T171_bg1vve', 'back_T174_qtac']; injector accounting |
| terminal | subshell_ready_vim_typeahead | FAIL | the setup was written after vim (hop subshell_setup_ready_write); injector accounting: injector sent 226; app saw 241; lost-before-app 0 (En |
| terminal | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 163; app saw 176; lost-before-app 0 (Enter: sent 6, app saw 7); app logged 13 more than were injected (ke |
| terminal | subshell_ready_secret_after_prompt | FAIL | the secret read after our first line cancelled the attempt (hop why=Secret); the password reached the read -s, not a command: ['set / comman |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 41; app saw 55; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | FAIL | relay_or_fish: the shell in front after leaving is the pane's ROOT shell (got none); the pane's bash runs the next command; injector account |
| terminal | pane_not_local_after_subshell_ready_exec_fish_rc | FAIL | after subshell_ready_exec_fish_rc the pane's shell was not the root shell (none; root pid 63853); the app is relaunched so later cases start |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 80; app saw 78; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| terminal | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 65360); the app is relaunched so later case |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 227; app saw 225; lost-before-app 2 (Enter: sent 7, app saw 8); pty writes: 0, of which carrying a CR (\r |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 262; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 154; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.38 ms (p95 0.72, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal | latency_paint | FAIL | Key to paint: Phosphor overhead 6.96 +/- 1.61 ms (SE) over Terminal.app (.command via open) exceeds the 2 ms budget (overhead - 2 SE > budge |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 575; lost-before-app 69 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR  |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\ |
| terminal | remote_tab_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| terminal | remote_tab_enter_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 13 more than were injected (keys |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 763; lost-before-app 0 (Enter: sent 21, app saw 26); app logged 8 more than were injected (k |
| terminal | shell_nu | PASS | nu app log: 2026-11-09T19:29:55Z [DEBUG] frame 1: updated=["ActionButton", "ActionButton", "Terminal", "ActionButton", "ActionButton", "Menu |
| terminal | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 20; lost- |
| terminal | zero_network_startup | SKIP | zero_network_startup (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0) |
| terminal | pane_not_local_after_zero_network_startup | FAIL | after zero_network_startup the pane's shell was not the root shell (none; root pid 66030); the app is relaunched so later cases start clean |
| terminal | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 41; app saw |
| terminal | claude_plugin_offline | SKIP | claude_plugin_offline (this platform: macos); injector accounting: injector sent 2; app saw 17; lost-before-app 0 (Enter: sent 0, app saw 1) |
| terminal | agent_plugins_offline | SKIP | agent_plugins_offline (this platform: macos); injector accounting: injector sent 2; app saw 17; lost-before-app 0 (Enter: sent 0, app saw 1) |
| terminal | network_gate_switch | SKIP | network_gate_switch (this platform: macos); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1);  |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 22; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 7 more than were injected (keys  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); pty writes: 0, of which carrying a CR (\r):  |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 0, app saw 12); pty writes: 0, of which carrying a CR ( |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | PASS | gate on vs off: Δp50 = -18.56 ms (on 182.8 / off 201.36; p95 193.63 / 215.17; n 30 / 30; blocks of 5, relaunch per block); gate on: 4379 fra |
| agent | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| agent | subshell_ready_rc_questions | FAIL | rcq: exact lines in the inner shell then the pane's bash: got ['T87_6z4gi2'], wanted ['T87_6z4gi2', 'back_T90_ect8']; injector accounting: i |
| agent | subshell_ready_vim_typeahead | FAIL | the setup was written after vim (hop subshell_setup_ready_write); injector accounting: injector sent 222; app saw 234; lost-before-app 0 (En |
| agent | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 161; app saw 176; lost-before-app 0 (Enter: sent 6, app saw 7); app logged 15 more than were injected (ke |
| agent | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: ['echo T103_y65ez6 >> lab_secret.txthunter2']; the handed-back line ran once when the user  |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 41; app saw 54; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | FAIL | relay_or_fish: the shell in front after leaving is the pane's ROOT shell (got none); the pane's bash runs the next command; injector account |
| agent | pane_not_local_after_subshell_ready_exec_fish_rc | FAIL | after subshell_ready_exec_fish_rc the pane's shell was not the root shell (none; root pid 2853); the app is relaunched so later cases start  |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 80; app saw 78; lost-bef |
| agent | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| agent | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 4462); the app is relaunched so later cases |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 227; app saw 225; lost-before-app 2 (Enter: sent 7, app saw 8); pty writes: 0, of which carrying a CR (\r |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 260; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 152; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.39 ms (p95 0.83, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | FAIL | Key to paint: Phosphor overhead 6.66 +/- 0.73 ms (SE) over Terminal.app (.command via open) exceeds the 2 ms budget (overhead - 2 SE > budge |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | injector accounting: injector sent 630; app saw 575; lost-before-app 55 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR  |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 220; app saw 204; lost-before-app 16 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| agent | remote_tab_enter_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 764; lost-before-app 0 (Enter: sent 21, app saw 26); app logged 9 more than were injected (k |
| agent | shell_nu | PASS | nu app log: 2026-11-09T20:10:01Z [DEBUG] frame 1: updated=["AIBlock", "AIBlock", "AIBlock", "AIBlock", "AIBlock", "AIBlock", "Input", "UseAg |
| agent | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2 keys (0 Enter); ap |
| agent | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 41; app saw |
| agent@fake | agent_interactive_password | FAIL | ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=True); a fresh agent conversation was started (Escape, /age |
| agent@fake | agent_edit_accept | FAIL | lf: the accepted multi-hunk edit: 187 bytes on disk, 208 expected; first difference at byte 23 (b'beta t4yn1b7o\nL03 gamma ' vs b'BETA-EDITE |
| agent@fake | agent_edit_reject | FAIL | after_reject: the accepted multi-hunk edit: 187 bytes on disk, 208 expected; first difference at byte 23 (b'beta t7v616mt\nL03 gamma ' vs b' |
| agent@fake | agent_edit_conflict | FAIL | local: the conflict was not reported (no 'Failed to save file for accepted AgentMode diffs' / 'changed on disk' line in the app log); a fres |
| agent@fake | agent_edit_touch_only | FAIL | local: an accept after a touch-only change (bytes identical): 197 bytes on disk, 218 expected; first difference at byte 24 (b'beta t11tzkk6o |
| agent@fake | agent_edit_endings | FAIL | crlf: the accepted multi-hunk edit: 207 bytes on disk, 228 expected; first difference at byte 25 (b'beta t133r9ay6\r\nL03 gamm' vs b'BETA-ED |
| agent@fake | agent_fault_429 | FAIL | unexpected ERROR line: 2026-11-09T20:24:29Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Man |
| agent@fake | agent_fault_500 | FAIL | unexpected ERROR line: 2026-11-09T20:25:01Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Interna |
| agent@fake | agent_fault_disconnect | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 reque |
| agent@fake | agent_fault_down | FAIL | unexpected ERROR line: 2026-11-09T20:25:51Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/st |
| agent@fake | agent_fault_slow_cancel | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier case |
| agent@fake | agent_fault_queued_rows | FAIL | unexpected turn state while queueing: None; ERROR lines: ['unexpected ERROR line: 2026-11-09T20:26:41Z [ERROR] MultiAgent request failed aft |
| agent@fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_vim | FAIL | ai_vim: no hop=fullscreen_handover in the app log after typing into the full-screen program: the hand-over is not proven; a fresh agent conv |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_latency_enter | FAIL | harness exception: CaseTimeout('case exceeded the 1500s per-case limit (APPCI_CASE_TIMEOUT)'); a fresh agent conversation was started (Escap |
| agent@fake | agent_interactive_ssh | FAIL | ai_ssh: the remote `read` at its prompt was never handed to the user within 60 s; a fresh agent conversation was started (Escape, /agent) so |
| agent@fake | agent_restart_kill_stream | FAIL | ar_kill_stream (relaunch): unexpected ERROR line: 2026-11-09T20:56:37Z [ERROR] Failed to read local control mode from secure storage: unknow |
| agent@fake | agent_restart_kill_queued | FAIL | ar_kill_queued (relaunch): unexpected ERROR line: 2026-11-09T20:57:31Z [ERROR] Failed to read local control mode from secure storage: unknow |
| agent@fake | agent_restart_kill_blocked | FAIL | ar_kill_blocked (relaunch): unexpected ERROR line: 2026-11-09T20:58:26Z [ERROR] Failed to read local control mode from secure storage: unkno |
| agent@fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | SKIP | ai_less: no hop=alt_screen_enter within 30 s, so less never took the screen and no keys were typed; a fresh agent conversation was started ( |
| agent@fake | pane_not_local_after_agent_interactive_less | FAIL | after agent_interactive_less the pane's shell was not the root shell (none; root pid 42809); the app is relaunched so later cases start clea |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | SKIP | network_gate_notice (this platform: macos); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1);  |
| agent@fake | agent_gate_deny_allow | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 15; lost-before-app 0 (Enter: sent 0, app saw 1); app logge |
| agent@fake | agent_gate_off_live | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 12; lost-before-app 0 (Enter: sent 0, app saw 1); app logge |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 41; app saw 39; lost-before-app 2 (Enter: sent 1, app saw 1); pty writ |
| agent@fake | agent_multitab_streaming | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 4. |
| agent@fake | left_agent_mode_after_agent_multitab_streaming | FAIL | after agent_multitab_streaming the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot ga |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2072; app saw 2070; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty wri |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR  |
| terminal | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead 10.65 +/- 5.97 ms (SE) vs budget 20 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT  |
| terminal | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 41; app saw 39; lost-before-app 2 (Ente |
| terminal | subshell_sh_probe | FAIL | dash: the app logged the unsupported-shell event (hop subshell_setup_unsupported); APPCI_SU_SH_USER unset: the `su` to an sh-shelled user le |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 189; app saw 217; lost-before-app 0 (Enter: sent 8, app saw 10); app logged 28 more than were injected (k |
| terminal | subshell_ready_vim_typeahead | FAIL | vim: the line typed into the inner shell ran (file []); vim received exactly the typed text: []; while vim ran the judge waited (AltScreen o |
| terminal | pane_not_local_after_subshell_ready_vim_typeahead | FAIL | after subshell_ready_vim_typeahead the pane's shell was not the root shell (none; root pid 2087650); the app is relaunched so later cases st |
| terminal | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 80; app saw 78; lost-before-app 2 (Enter: sent 2, app saw 2); pty writes: 0, of wh |
| terminal | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: ['echo T238_souzyl >> lab_secret.txthunter2']; the handed-back line ran once when the user  |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 41; app saw 69; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | FAIL | relay_or_fish: the shell in front after leaving is the pane's ROOT shell (got none); the pane's bash runs the next command; injector account |
| terminal | pane_not_local_after_subshell_ready_exec_fish_rc | FAIL | after subshell_ready_exec_fish_rc the pane's shell was not the root shell (none; root pid 2123284); the app is relaunched so later cases sta |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 80; app saw 78; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| terminal | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 2131321); the app is relaunched so later ca |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 227; app saw 225; lost-before-app 2 (Enter: sent 7, app saw 8); pty writes: 0, of which carrying a CR (\r |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 2: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 15 key(s)/1 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 20 key( |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 164; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 4.49 ms (p95 20.07, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to shell (no rout |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| terminal | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t309_0' should have completed the REMOTE directory: the remote shell received ['echo wT310_3w6fvcr7 >> ~/.appci_where'] |
| terminal | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT317_vb3vpowd >> ~/.appci_where'], want the COMPLETED 'cd phxte_t314_3xz24g/' (th |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 41; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 25 more than were injected (keys |
| terminal | shell_fish | PASS | injector accounting: injector sent 751; app saw 776; lost-before-app 0 (Enter: sent 25, app saw 26); 4 paste chord(s) not counted as keys; a |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; injector accounting: injector  |
| terminal | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| terminal | md_viewer | FAIL | mermaid flowchart node labels not visible (blank / failed diagram block?): missing ['alphanodestart', 'gammanodeend']; OCR saw 'oxqsearchtab |
| terminal | zero_network_startup | FAIL | the scripted ssh connect to the fake host did not reach a remote prompt; scripted session: settings page walked with the keyboard (best effo |
| terminal | keyring_unavailable_startup | PASS | injector accounting: injector sent 584; app saw 579; lost-before-app 5 (Enter: sent 13, app saw 13); pty writes: 0, of which carrying a CR ( |
| terminal | claude_plugin_offline | PASS | bundle: /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/claude_plugin; networ |
| terminal | agent_plugins_offline | PASS | codex bundle: /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/codex_plugin; n |
| terminal | network_gate_switch | FAIL | before the keyboard Remove, settings.toml no longer holds the allowed origin the case wrote (the page then has no Remove stop); keyboard Rem |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 61; app saw 54; lost-before-app 7 (Enter: sent 2, app saw 0) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2002; app saw 2001; lost-before-app 1 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty wri |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 40; app saw 38; lost-before-app 2 (Ente |
| agent | subshell_sh_probe | FAIL | dash: the app logged the unsupported-shell event (hop subshell_setup_unsupported); APPCI_SU_SH_USER unset: the `su` to an sh-shelled user le |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 185; app saw 210; lost-before-app 0 (Enter: sent 8, app saw 10); app logged 25 more than were injected (k |
| agent | subshell_ready_vim_typeahead | FAIL | the setup was written after vim (hop subshell_setup_ready_write); injector accounting: injector sent 223; app saw 246; lost-before-app 0 (En |
| agent | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 41; app saw 66; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 25 more t |
| agent | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: ['echo T104_z61cw3 >> lab_secret.txthunter2']; the handed-back line ran once when the user  |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 41; app saw 67; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | FAIL | relay_or_fish: the shell in front after leaving is the pane's ROOT shell (got none); the pane's bash runs the next command; injector account |
| agent | pane_not_local_after_subshell_ready_exec_fish_rc | FAIL | after subshell_ready_exec_fish_rc the pane's shell was not the root shell (none; root pid 2293434); the app is relaunched so later cases sta |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 80; app saw 78; lost-bef |
| agent | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| agent | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 2302794); the app is relaunched so later ca |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 227; app saw 225; lost-before-app 2 (Enter: sent 7, app saw 8); pty writes: 0, of which carrying a CR (\r |
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.2 s trial 2: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 6 key(s)/6  |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 19 key( |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 164; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 15.85 ms (p95 16.08, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | ambiguous 'make a PR': went to agent (no routing log line, so no rule to check); ambiguous 'cat the error log and tell me what is wrong': we |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 181; lost-before-app 0 (Enter: sent 10, app saw 11); app logged 21 more than were injected ( |
| agent | shell_fish | PASS | injector accounting: injector sent 751; app saw 774; lost-before-app 0 (Enter: sent 25, app saw 26); 4 paste chord(s) not counted as keys; a |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-09T20:05:0 |
| agent | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 460; app saw 441; lost-before-app 19 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | md_viewer | PASS | injector accounting: injector sent 2 keys (0 Enter); app key trace not available, so nothing to compare |
| agent | keyring_unavailable_startup | PASS | injector accounting: injector sent 584; app saw 579; lost-before-app 5 (Enter: sent 13, app saw 12); pty writes: 0, of which carrying a CR ( |
| agent | agent_view_not_restored_after_keyring_unavailable_startup | PASS | keyring_unavailable_startup relaunched the app itself (pid 2328629 -> 2330103); a relaunched app starts in a terminal session, so this is no |
| agent@fake | agent_interactive_password | FAIL | ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=True); a fresh agent conversation was started (Escape, /age |
| agent@fake | agent_edit_accept | FAIL | lf: the accepted multi-hunk edit: 187 bytes on disk, 208 expected; first difference at byte 23 (b'beta t4yn1b7o\nL03 gamma ' vs b'BETA-EDITE |
| agent@fake | agent_edit_reject | FAIL | after_reject: the accepted multi-hunk edit: 187 bytes on disk, 208 expected; first difference at byte 23 (b'beta t7v616mt\nL03 gamma ' vs b' |
| agent@fake | agent_edit_conflict | FAIL | local: the conflict was not reported (no 'Failed to save file for accepted AgentMode diffs' / 'changed on disk' line in the app log); a fres |
| agent@fake | agent_edit_touch_only | FAIL | local: an accept after a touch-only change (bytes identical): 197 bytes on disk, 218 expected; first difference at byte 24 (b'beta t11tzkk6o |
| agent@fake | agent_edit_endings | FAIL | crlf: the accepted multi-hunk edit: 207 bytes on disk, 228 expected; first difference at byte 25 (b'beta t133r9ay6\r\nL03 gamm' vs b'BETA-ED |
| agent@fake | agent_fault_429 | FAIL | unexpected ERROR line: 2026-11-09T20:20:20Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Man |
| agent@fake | agent_fault_500 | FAIL | unexpected ERROR line: 2026-11-09T20:20:49Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Interna |
| agent@fake | agent_fault_disconnect | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 reque |
| agent@fake | agent_fault_down | FAIL | unexpected ERROR line: 2026-11-09T20:21:33Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/st |
| agent@fake | agent_fault_slow_cancel | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier case |
| agent@fake | agent_fault_queued_rows | FAIL | unexpected turn state while queueing: None; ERROR lines: ['unexpected ERROR line: 2026-11-09T20:22:16Z [ERROR] MultiAgent request failed aft |
| agent@fake | agent_latency_key | FAIL | agent_latency_key/streaming: Phosphor overhead 7.21 +/- 0.7 ms (SE) over same app, terminal-mode input editor exceeds the 2 ms budget (overh |
| agent@fake | agent_interactive_vim | FAIL | ai_vim: no hop=fullscreen_handover in the app log after typing into the full-screen program: the hand-over is not proven; a fresh agent conv |
| agent@fake | agent_restart_quit_stream | FAIL | ar_quit_stream (relaunch): unexpected ERROR line: 2026-11-09T20:24:57Z [ERROR] error creating connection to Xorg server: $DISPLAY variable n |
| agent@fake | agent_restart_quit_queued | FAIL | ar_quit_queued (relaunch): unexpected ERROR line: 2026-11-09T20:26:13Z [ERROR] error creating connection to Xorg server: $DISPLAY variable n |
| agent@fake | agent_restart_quit_blocked | FAIL | ar_quit_blocked (relaunch): unexpected ERROR line: 2026-11-09T20:27:30Z [ERROR] error creating connection to Xorg server: $DISPLAY variable  |
| agent@fake | agent_latency_enter | SKIP | no Enter keydown -> hop=enter_begin pair in the app log (input-order logging off, or the Enters never reached the editor); a fresh agent con |
| agent@fake | agent_interactive_ssh | FAIL | ai_ssh: the remote `read` at its prompt was never handed to the user within 60 s; a fresh agent conversation was started (Escape, /agent) so |
| agent@fake | agent_restart_kill_stream | FAIL | ar_kill_stream (relaunch): unexpected ERROR line: 2026-11-09T20:54:46Z [ERROR] error creating connection to Xorg server: $DISPLAY variable n |
| agent@fake | agent_restart_kill_queued | FAIL | ar_kill_queued (relaunch): unexpected ERROR line: 2026-11-09T20:55:59Z [ERROR] error creating connection to Xorg server: $DISPLAY variable n |
| agent@fake | agent_restart_kill_blocked | FAIL | ar_kill_blocked (relaunch): unexpected ERROR line: 2026-11-09T20:57:14Z [ERROR] error creating connection to Xorg server: $DISPLAY variable  |
| agent@fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | SKIP | ai_less: no hop=alt_screen_enter within 30 s, so less never took the screen and no keys were typed; a fresh agent conversation was started ( |
| agent@fake | pane_not_local_after_agent_interactive_less | FAIL | after agent_interactive_less the pane's shell was not the root shell (none; root pid 2355765); the app is relaunched so later cases start cl |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | FAIL | the remote image from an unlisted host was not refused by the gate (no 'network gate: denied "remote-asset" (host hash 714ef6f9d3c78218)' li |
| agent@fake | agent_gate_deny_allow | FAIL | the image from the unlisted loopback NAME localhost was not refused (no 'network gate: denied "remote-asset" (host hash db3f053403ad937a)' l |
| agent@fake | agent_gate_off_live | FAIL | precondition: the gate (on) did not refuse the unlisted origin; with the gate off the request did not reach the listener; with the gate back |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | FAIL | the denied origin was not refused (no 'network gate: denied "remote-asset" (host hash db3f053403ad937a)'); the allowed origin did not receiv |
| agent@fake | agent_multitab_streaming | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 4. |
| agent@fake | left_agent_mode_after_agent_multitab_streaming | FAIL | after agent_multitab_streaming the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with he |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | pty writes: 257, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with  |
| terminal | paste_instant_enter | PASS | pty writes: 14, of which carrying a CR (\r): 2; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with he |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.39, -> LineEditorReading 59.2; app:  |
| terminal | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| terminal | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| terminal | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.66 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with he |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | pty writes: 194, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with  |
| terminal@gitbash | paste_instant_enter | PASS |  |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| terminal@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| terminal@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 41.3 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.66 ms (p95 0.92, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with h |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with hel |
| terminal@gitbash | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t211_z' should have completed the REMOTE directory: the remote shell received ['echo wT212_pfec7644 >> ~/.appci_where'] |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT220_to913369 >> ~/.appci_where'], want the COMPLETED 'cd phxte_t217_aod2k6/' (th |
| terminal@gitbash | ui_completions_as_you_type_local | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal@gitbash | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal@gitbash | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal@gitbash | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal@gitbash | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal@gitbash | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | pty writes: 175, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with  |
| agent | paste_instant_enter | PASS | pty writes: 15, of which carrying a CR (\r): 3; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with he |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 9.18, -> LineEditorReading 56.36; app: |
| agent | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| agent | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| agent | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte |
| agent | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in no shell, not the remote one: `exit` NOT typed; stopping; ssh trial verdic |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git  |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent | agent_lrc_no_false_takeover | SKIP | not run on Windows: #1796: the interactive-prompt take-over it tests (PromptWatch, termios + /proc tty-read probe) is unix-only; ConPTY has  |
| agent | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| agent | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| agent | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| agent | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | pty writes: 139, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with  |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with hel |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with h |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with hel |
| agent@gitbash | agent_lrc_no_false_takeover | SKIP | not run on Windows: #1796: the interactive-prompt take-over it tests (PromptWatch, termios + /proc tty-read probe) is unix-only; ConPTY has  |
| agent@gitbash | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t99_7' should have completed the REMOTE directory: the remote shell received ['echo wT100_kmxy7ug0 >> ~/.appci_where'], |
| agent@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT108_oihl0dg8 >> ~/.appci_where'], want the COMPLETED 'cd phxte_t105_d75qpv/' (th |
| agent@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| agent@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent@fake | agent_interactive_password | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_fault_429 | FAIL | unexpected ERROR line: 2026-11-09T19:52:49Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Man |
| agent@fake | agent_fault_500 | FAIL | unexpected ERROR line: 2026-11-09T19:53:18Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Interna |
| agent@fake | agent_fault_disconnect | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 reque |
| agent@fake | agent_fault_down | FAIL | unexpected ERROR line: 2026-11-09T19:54:11Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/st |
| agent@fake | agent_fault_slow_cancel | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier case |
| agent@fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@fake | agent_latency_key | SKIP | not run in the PowerShell pane: its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake mode |
| agent@fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@fake | agent_latency_enter | SKIP | not run in the PowerShell pane: its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent c |
| agent@fake | agent_steer_fast | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_less | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_noninteractive_no_takeover | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | network_gate_notice | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| agent@fake | agent_gate_deny_allow | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent@fake | agent_gate_off_live | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent@fake | agent_mcp_faults | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does n |
| agent@fake | agent_gate_key_never_leaves | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent@fake | agent_multitab_streaming | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Lin |
| agent@gitbash+fake | agent_interactive_password | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@gitbash+fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_fault_429 | FAIL | unexpected ERROR line: 2026-11-09T19:59:40Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Man |
| agent@gitbash+fake | agent_fault_500 | FAIL | unexpected ERROR line: 2026-11-09T20:00:09Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Interna |
| agent@gitbash+fake | agent_fault_disconnect | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 reque |
| agent@gitbash+fake | agent_fault_down | FAIL | unexpected ERROR line: 2026-11-09T20:01:01Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/st |
| agent@gitbash+fake | agent_fault_slow_cancel | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier case |
| agent@gitbash+fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@gitbash+fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_latency_enter | SKIP | no Enter keydown -> hop=enter_begin pair in the app log (input-order logging off, or the Enters never reached the editor); a fresh agent con |
| agent@gitbash+fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@gitbash+fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@gitbash+fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent c |
| agent@gitbash+fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 3, of which ca |
| agent@gitbash+fake | agent_interactive_less | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_noninteractive_no_takeover | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | network_gate_notice | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| agent@gitbash+fake | agent_gate_deny_allow | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent@gitbash+fake | agent_gate_off_live | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent@gitbash+fake | agent_mcp_faults | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does n |
| agent@gitbash+fake | agent_gate_key_never_leaves | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent@gitbash+fake | agent_multitab_streaming | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Lin |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_edit_accept | macos | agent@fake | FAIL |
| agent_edit_accept | wayland | agent@fake | FAIL |
| agent_edit_accept | windows | agent@fake | SKIP |
| agent_edit_accept | windows | agent@gitbash+fake | SKIP |
| agent_edit_conflict | macos | agent@fake | FAIL |
| agent_edit_conflict | wayland | agent@fake | FAIL |
| agent_edit_conflict | windows | agent@fake | SKIP |
| agent_edit_conflict | windows | agent@gitbash+fake | SKIP |
| agent_edit_endings | macos | agent@fake | FAIL |
| agent_edit_endings | wayland | agent@fake | FAIL |
| agent_edit_endings | windows | agent@fake | SKIP |
| agent_edit_endings | windows | agent@gitbash+fake | SKIP |
| agent_edit_reject | macos | agent@fake | FAIL |
| agent_edit_reject | wayland | agent@fake | FAIL |
| agent_edit_reject | windows | agent@fake | SKIP |
| agent_edit_reject | windows | agent@gitbash+fake | SKIP |
| agent_edit_touch_only | macos | agent@fake | FAIL |
| agent_edit_touch_only | wayland | agent@fake | FAIL |
| agent_edit_touch_only | windows | agent@fake | SKIP |
| agent_edit_touch_only | windows | agent@gitbash+fake | SKIP |
| agent_fault_429 | macos | agent@fake | FAIL |
| agent_fault_429 | wayland | agent@fake | FAIL |
| agent_fault_429 | windows | agent@fake | FAIL |
| agent_fault_429 | windows | agent@gitbash+fake | FAIL |
| agent_fault_500 | macos | agent@fake | FAIL |
| agent_fault_500 | wayland | agent@fake | FAIL |
| agent_fault_500 | windows | agent@fake | FAIL |
| agent_fault_500 | windows | agent@gitbash+fake | FAIL |
| agent_fault_disconnect | macos | agent@fake | FAIL |
| agent_fault_disconnect | wayland | agent@fake | FAIL |
| agent_fault_disconnect | windows | agent@fake | FAIL |
| agent_fault_disconnect | windows | agent@gitbash+fake | FAIL |
| agent_fault_down | macos | agent@fake | FAIL |
| agent_fault_down | wayland | agent@fake | FAIL |
| agent_fault_down | windows | agent@fake | FAIL |
| agent_fault_down | windows | agent@gitbash+fake | FAIL |
| agent_fault_queued_rows | macos | agent@fake | FAIL |
| agent_fault_queued_rows | wayland | agent@fake | FAIL |
| agent_fault_queued_rows | windows | agent@fake | SKIP |
| agent_fault_queued_rows | windows | agent@gitbash+fake | SKIP |
| agent_fault_slow_cancel | macos | agent@fake | FAIL |
| agent_fault_slow_cancel | wayland | agent@fake | FAIL |
| agent_fault_slow_cancel | windows | agent@fake | FAIL |
| agent_fault_slow_cancel | windows | agent@gitbash+fake | FAIL |
| agent_gate_deny_allow | macos | agent@fake | SKIP |
| agent_gate_deny_allow | wayland | agent@fake | FAIL |
| agent_gate_deny_allow | windows | agent@fake | SKIP |
| agent_gate_deny_allow | windows | agent@gitbash+fake | SKIP |
| agent_gate_key_never_leaves | macos | agent@fake | SKIP |
| agent_gate_key_never_leaves | wayland | agent@fake | FAIL |
| agent_gate_key_never_leaves | windows | agent@fake | SKIP |
| agent_gate_key_never_leaves | windows | agent@gitbash+fake | SKIP |
| agent_gate_off_live | macos | agent@fake | SKIP |
| agent_gate_off_live | wayland | agent@fake | FAIL |
| agent_gate_off_live | windows | agent@fake | SKIP |
| agent_gate_off_live | windows | agent@gitbash+fake | SKIP |
| agent_interactive_less | macos | agent@fake | SKIP |
| agent_interactive_less | wayland | agent@fake | SKIP |
| agent_interactive_less | windows | agent@fake | SKIP |
| agent_interactive_less | windows | agent@gitbash+fake | SKIP |
| agent_interactive_password | macos | agent@fake | FAIL |
| agent_interactive_password | wayland | agent@fake | FAIL |
| agent_interactive_password | windows | agent@fake | SKIP |
| agent_interactive_password | windows | agent@gitbash+fake | SKIP |
| agent_interactive_ssh | macos | agent@fake | FAIL |
| agent_interactive_ssh | wayland | agent@fake | FAIL |
| agent_interactive_ssh | windows | agent@fake | SKIP |
| agent_interactive_ssh | windows | agent@gitbash+fake | SKIP |
| agent_interactive_vim | macos | agent@fake | FAIL |
| agent_interactive_vim | wayland | agent@fake | FAIL |
| agent_interactive_vim | windows | agent@fake | SKIP |
| agent_interactive_vim | windows | agent@gitbash+fake | SKIP |
| agent_latency_enter | macos | agent@fake | FAIL |
| agent_latency_enter | wayland | agent@fake | SKIP |
| agent_latency_enter | windows | agent@fake | SKIP |
| agent_latency_enter | windows | agent@gitbash+fake | SKIP |
| agent_latency_key | macos | agent@fake | PASS |
| agent_latency_key | wayland | agent@fake | FAIL |
| agent_latency_key | windows | agent@fake | SKIP |
| agent_latency_key | windows | agent@gitbash+fake | PASS |
| agent_lrc_no_false_takeover | macos | agent | PASS |
| agent_lrc_no_false_takeover | wayland | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent | SKIP |
| agent_lrc_no_false_takeover | windows | agent@gitbash | SKIP |
| agent_mcp_faults | macos | agent@fake | SKIP |
| agent_mcp_faults | wayland | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@gitbash+fake | SKIP |
| agent_multitab_streaming | macos | agent@fake | PASS |
| agent_multitab_streaming | wayland | agent@fake | PASS |
| agent_multitab_streaming | windows | agent@fake | SKIP |
| agent_multitab_streaming | windows | agent@gitbash+fake | SKIP |
| agent_noninteractive_no_takeover | macos | agent@fake | PASS |
| agent_noninteractive_no_takeover | wayland | agent@fake | PASS |
| agent_noninteractive_no_takeover | windows | agent@fake | SKIP |
| agent_noninteractive_no_takeover | windows | agent@gitbash+fake | SKIP |
| agent_plugins_offline | macos | terminal | SKIP |
| agent_plugins_offline | wayland | terminal | PASS |
| agent_plugins_offline | windows | terminal | SKIP |
| agent_plugins_offline | windows | terminal@gitbash | SKIP |
| agent_restart_kill_blocked | macos | agent@fake | FAIL |
| agent_restart_kill_blocked | wayland | agent@fake | FAIL |
| agent_restart_kill_blocked | windows | agent@fake | SKIP |
| agent_restart_kill_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_kill_queued | macos | agent@fake | FAIL |
| agent_restart_kill_queued | wayland | agent@fake | FAIL |
| agent_restart_kill_queued | windows | agent@fake | SKIP |
| agent_restart_kill_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_kill_stream | macos | agent@fake | FAIL |
| agent_restart_kill_stream | wayland | agent@fake | FAIL |
| agent_restart_kill_stream | windows | agent@fake | PASS |
| agent_restart_kill_stream | windows | agent@gitbash+fake | PASS |
| agent_restart_quit_blocked | macos | agent@fake | SKIP |
| agent_restart_quit_blocked | wayland | agent@fake | FAIL |
| agent_restart_quit_blocked | windows | agent@fake | SKIP |
| agent_restart_quit_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_queued | macos | agent@fake | SKIP |
| agent_restart_quit_queued | wayland | agent@fake | FAIL |
| agent_restart_quit_queued | windows | agent@fake | SKIP |
| agent_restart_quit_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_stream | macos | agent@fake | SKIP |
| agent_restart_quit_stream | wayland | agent@fake | FAIL |
| agent_restart_quit_stream | windows | agent@fake | SKIP |
| agent_restart_quit_stream | windows | agent@gitbash+fake | SKIP |
| agent_steer_fast | macos | agent@fake | PASS |
| agent_steer_fast | wayland | agent@fake | PASS |
| agent_steer_fast | windows | agent@fake | SKIP |
| agent_steer_fast | windows | agent@gitbash+fake | PASS |
| cb_report_problem | macos | terminal | PASS |
| cb_report_problem | wayland | terminal | PASS |
| cb_report_problem | windows | terminal | SKIP |
| cb_report_problem | windows | terminal@gitbash | SKIP |
| claude_plugin_offline | macos | terminal | SKIP |
| claude_plugin_offline | wayland | terminal | PASS |
| claude_plugin_offline | windows | terminal | SKIP |
| claude_plugin_offline | windows | terminal@gitbash | SKIP |
| ctrl_c_flood_ab | macos | agent | PASS |
| ctrl_c_flood_ab | macos | terminal | PASS |
| ctrl_c_flood_ab | wayland | agent | SKIP |
| ctrl_c_flood_ab | wayland | terminal | SKIP |
| ctrl_c_flood_ab | windows | agent | SKIP |
| ctrl_c_flood_ab | windows | agent@gitbash | SKIP |
| ctrl_c_flood_ab | windows | terminal | SKIP |
| ctrl_c_flood_ab | windows | terminal@gitbash | SKIP |
| ctrl_c_no_esc_i_local | macos | agent | PASS |
| ctrl_c_no_esc_i_local | macos | terminal | PASS |
| ctrl_c_no_esc_i_local | wayland | agent | PASS |
| ctrl_c_no_esc_i_local | wayland | terminal | PASS |
| ctrl_c_no_esc_i_local | windows | agent | SKIP |
| ctrl_c_no_esc_i_local | windows | agent@gitbash | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal@gitbash | SKIP |
| keyring_unavailable_startup | macos | agent | SKIP |
| keyring_unavailable_startup | macos | terminal | SKIP |
| keyring_unavailable_startup | wayland | agent | PASS |
| keyring_unavailable_startup | wayland | terminal | PASS |
| keyring_unavailable_startup | windows | agent | SKIP |
| keyring_unavailable_startup | windows | agent@gitbash | SKIP |
| keyring_unavailable_startup | windows | terminal | SKIP |
| keyring_unavailable_startup | windows | terminal@gitbash | SKIP |
| network_gate_notice | macos | agent@fake | SKIP |
| network_gate_notice | wayland | agent@fake | FAIL |
| network_gate_notice | windows | agent@fake | SKIP |
| network_gate_notice | windows | agent@gitbash+fake | SKIP |
| network_gate_switch | macos | terminal | SKIP |
| network_gate_switch | wayland | terminal | FAIL |
| network_gate_switch | windows | terminal | SKIP |
| network_gate_switch | windows | terminal@gitbash | SKIP |
| remote_tab_enter_no_helper | macos | agent | SKIP |
| remote_tab_enter_no_helper | macos | terminal | SKIP |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | FAIL |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | FAIL |
| remote_tab_no_helper | macos | agent | SKIP |
| remote_tab_no_helper | macos | terminal | SKIP |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | FAIL |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| subshell_ready_docker_t_without_i | macos | agent | SKIP |
| subshell_ready_docker_t_without_i | macos | terminal | SKIP |
| subshell_ready_docker_t_without_i | wayland | agent | SKIP |
| subshell_ready_docker_t_without_i | wayland | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | macos | agent | FAIL |
| subshell_ready_exec_fish_rc | macos | terminal | FAIL |
| subshell_ready_exec_fish_rc | wayland | agent | FAIL |
| subshell_ready_exec_fish_rc | wayland | terminal | FAIL |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | agent@gitbash | SKIP |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_questions | macos | agent | FAIL |
| subshell_ready_rc_questions | macos | terminal | FAIL |
| subshell_ready_rc_questions | wayland | agent | PASS |
| subshell_ready_rc_questions | wayland | terminal | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | agent@gitbash | SKIP |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | SKIP |
| subshell_ready_relay_shape | macos | agent | SKIP |
| subshell_ready_relay_shape | macos | terminal | SKIP |
| subshell_ready_relay_shape | wayland | agent | SKIP |
| subshell_ready_relay_shape | wayland | terminal | SKIP |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | macos | agent | FAIL |
| subshell_ready_secret_after_prompt | macos | terminal | FAIL |
| subshell_ready_secret_after_prompt | wayland | agent | FAIL |
| subshell_ready_secret_after_prompt | wayland | terminal | FAIL |
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
| subshell_ready_vim_typeahead | macos | agent | FAIL |
| subshell_ready_vim_typeahead | macos | terminal | FAIL |
| subshell_ready_vim_typeahead | wayland | agent | FAIL |
| subshell_ready_vim_typeahead | wayland | terminal | FAIL |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | macos | agent | FAIL |
| subshell_ready_zsh_newuser_install | macos | terminal | FAIL |
| subshell_ready_zsh_newuser_install | wayland | agent | FAIL |
| subshell_ready_zsh_newuser_install | wayland | terminal | FAIL |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | macos | agent | PASS |
| subshell_sh_probe | macos | terminal | PASS |
| subshell_sh_probe | wayland | agent | FAIL |
| subshell_sh_probe | wayland | terminal | FAIL |
| subshell_sh_probe | windows | agent | SKIP |
| subshell_sh_probe | windows | agent@gitbash | SKIP |
| subshell_sh_probe | windows | terminal | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | NOREF |
| zero_network_startup | macos | terminal | SKIP |
| zero_network_startup | wayland | terminal | FAIL |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | SKIP |

passed everywhere it ran (0, candidates for the owner's decision): none
not passed somewhere (fail / skip / no-reference): agent_edit_accept, agent_edit_conflict, agent_edit_endings, agent_edit_reject, agent_edit_touch_only, agent_fault_429, agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_queued_rows, agent_fault_slow_cancel, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_less, agent_interactive_password, agent_interactive_ssh, agent_interactive_vim, agent_latency_enter, agent_latency_key, agent_lrc_no_false_takeover, agent_mcp_faults, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_plugins_offline, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_kill_stream, agent_restart_quit_blocked, agent_restart_quit_queued, agent_restart_quit_stream, agent_steer_fast, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, keyring_unavailable_startup, network_gate_notice, network_gate_switch, remote_tab_enter_no_helper, remote_tab_no_helper, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, ui_completions_as_you_type_local, zero_network_startup
- note: windows/run_slim/logs/agent/phosphor.log.gz: 1366 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/phosphor.log.gz: 2281 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794256047.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.log.gz: 1495 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794253412.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794257776.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794253412.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.3.log.gz: 1856 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794251083.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794251083.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.3.log.gz: 2041 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 1332 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794253068.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.1.log.gz: 1382 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/subshell_ready_rc_questions (case not in previous run)`
  - `macos/terminal/subshell_ready_vim_typeahead (case not in previous run)`
  - `macos/terminal/subshell_ready_secret_after_prompt (case not in previous run)`
  - `macos/terminal/subshell_ready_exec_fish_rc (case not in previous run)`
  - `macos/terminal/pane_not_local_after_subshell_ready_exec_fish_rc (case not in previous run)`
  - `macos/terminal/subshell_ready_zsh_newuser_install (case not in previous run)`
  - `macos/terminal/pane_not_local_after_subshell_ready_zsh_newuser_install (case not in previous run)`
  - `macos/terminal/latency_paint`
  - `macos/terminal/pane_not_local_after_zero_network_startup (case not in previous run)`
  - `macos/agent/subshell_ready_rc_questions (case not in previous run)`
  - `macos/agent/subshell_ready_vim_typeahead (case not in previous run)`
  - `macos/agent/subshell_ready_secret_after_prompt (case not in previous run)`
  - `macos/agent/subshell_ready_exec_fish_rc (case not in previous run)`
  - `macos/agent/pane_not_local_after_subshell_ready_exec_fish_rc (case not in previous run)`
  - `macos/agent/subshell_ready_zsh_newuser_install (case not in previous run)`
  - `macos/agent/pane_not_local_after_subshell_ready_zsh_newuser_install (case not in previous run)`
  - `macos/agent/latency_paint`
  - `macos/agent@fake/agent_interactive_password (case not in previous run)`
  - `macos/agent@fake/agent_edit_accept (case not in previous run)`
  - `macos/agent@fake/agent_edit_reject (case not in previous run)`
  - `macos/agent@fake/agent_edit_conflict (case not in previous run)`
  - `macos/agent@fake/agent_edit_touch_only (case not in previous run)`
  - `macos/agent@fake/agent_edit_endings (case not in previous run)`
  - `macos/agent@fake/agent_fault_429 (case not in previous run)`
  - `macos/agent@fake/agent_fault_500 (case not in previous run)`
  - `macos/agent@fake/agent_fault_disconnect (case not in previous run)`
  - `macos/agent@fake/agent_fault_down (case not in previous run)`
  - `macos/agent@fake/agent_fault_slow_cancel (case not in previous run)`
  - `macos/agent@fake/agent_fault_queued_rows (case not in previous run)`
  - `macos/agent@fake/agent_interactive_vim (case not in previous run)`
  - `macos/agent@fake/agent_latency_enter (case not in previous run)`
  - `macos/agent@fake/agent_interactive_ssh (case not in previous run)`
  - `macos/agent@fake/agent_restart_kill_stream (case not in previous run)`
  - `macos/agent@fake/agent_restart_kill_queued (case not in previous run)`
  - `macos/agent@fake/agent_restart_kill_blocked (case not in previous run)`
  - `macos/agent@fake/pane_not_local_after_agent_interactive_less (case not in previous run)`
  - `macos/agent@fake/left_agent_mode_after_agent_multitab_streaming (case not in previous run)`
  - `wayland/terminal/subshell_sh_probe (case not in previous run)`
  - `wayland/terminal/subshell_ready_vim_typeahead (case not in previous run)`
  - `wayland/terminal/pane_not_local_after_subshell_ready_vim_typeahead (case not in previous run)`
  - ... and 59 more
- FIXED failures (app): 
  - `macos/terminal/ctrl_c_d`
  - `macos/agent/ctrl_c_d`
  - `windows/agent/ssh_connect_typing`
- Latency regressions > 5%: 
  - `macos/agent/latency_key_log key_to_frame: 6.00 -> 7.06 ms (+17.7%)`
  - `wayland/agent/ctrl_c_d ctrl_c_flood: 39.06 -> 42.01 ms (+7.6%)`
- compared against: test-warpui-core: build/r18w1 (3044614993); test-workspace-rest: build/r18w5 (3045526679); app macos: build/r18w5 (3045526679); app wayland: build/r18w5 (3045526679); app windows: build/r18w5 (3045526679)
