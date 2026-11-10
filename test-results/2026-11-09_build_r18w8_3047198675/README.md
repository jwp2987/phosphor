# build/r18w8  0b817b355  (pipeline 3047198675)

- date: 2026-11-09, tier: fix, pipeline status: manual
- compile (check-linux): ok, 115 warnings
- unit: 3630 passed, 2 failed, 19 skipped of 3632 run (1 suites)
- app (pass/fail/skip): macos 76/16/35 +3 no-ref; wayland 83/24/22 +4 no-ref; windows 58/24/63 +3 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | manual | - | - | not run |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 99.0 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 109.5 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 74.9 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.6 | - |  |
| build-macos-arm64 | success | 18.6 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.2 | phosphor-windows-vm |  |
| check-linux | success | 8.1 | - |  |
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
| guards-helper-deps | success | 0.2 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.9 | - |  |
| test-linux | failed | 23.0 | - |  |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | failed | 2.3 | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp ai::agent_providers::tools::web_runtime::websearch_tests::real_exa_anonymous_search` FAIL (0.2s)
- `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants` FAIL (126.4s)

## test-workspace-rest failures

- `remote_server backend::contract_tests::the_shell_arm_handles_directory_symlink_hardlink_and_tilde_targets` FAIL (0.2s)
- `warp_terminal model::escape_sequences::tests::test_ctrl_break_win32_input_sequence` FAIL (0.0s)

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
| terminal | ctrl_c_flood_ab | PASS | gate on vs off: Δp50 = -20.85 ms (on 178.51 / off 199.36; p95 191.94 / 209.03; n 30 / 30; blocks of 5, relaunch per block); gate on: 4629 fr |
| terminal | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 52; app saw 50; lost-before-app 2 (Enter: sent 1, app saw 1); pty writes: 0, of wh |
| terminal | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 14 more th |
| terminal | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 211; app saw 225; lost-before-app 0 (Enter: sent 8, app saw 10); app logged 14 more than were injected (k |
| terminal | subshell_ready_vim_typeahead | FAIL | vim: the line typed into the inner shell ran (file []); no Write decision before the one that wrote: ['2026-11-09T22:54:51Z [DEBUG] input_or |
| terminal | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 185; app saw 198; lost-before-app 0 (Enter: sent 6, app saw 7); app logged 13 more than were injected (ke |
| terminal | subshell_ready_secret_after_prompt | FAIL | the secret read after our first line cancelled the attempt (hop why=Secret); the password reached the read -s, not a command: ['set / comman |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 70; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 272; app saw 286; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 14 more than were injected (ke |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 65; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty HOME (proved on a pty before the case types anything); injector accounting: injector sent |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 210; app saw 224; lost-before-app 0 (Enter: sent 6, app saw 8); app logged 14 more than were injected (ke |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 260; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 148; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| terminal | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 17; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 15 more th |
| terminal | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 14 more th |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.44 ms (p95 1.03, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal | latency_paint | FAIL | Key to paint: Phosphor overhead 7.45 +/- 1.76 ms (SE) over Terminal.app (.command via open) exceeds the 2 ms budget (overhead - 2 SE > budge |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 575; lost-before-app 69 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR  |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\ |
| terminal | remote_tab_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| terminal | remote_tab_enter_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 14 more than were injected (keys |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 761; lost-before-app 0 (Enter: sent 21, app saw 26); app logged 6 more than were injected (k |
| terminal | shell_nu | PASS | nu app log: 2026-11-09T23:11:13Z [DEBUG] frame 1: updated=["RightPanelView", "AIBlock", "ActionButton", "AIBlock", "Menu", "AIBlock", "AIBlo |
| terminal | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| terminal | ps_read_host_secure | SKIP | PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret); |
| terminal | gitbash_precmd_secret | SKIP | Git Bash pane only (bash precmd_functions; ConPTY prompt hold); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter:  |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 14; lost- |
| terminal | zero_network_startup | SKIP | zero_network_startup (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0) |
| terminal | pane_not_local_after_zero_network_startup | FAIL | after zero_network_startup the pane's shell was not the root shell (none; root pid 74698); the app is relaunched so later cases start clean |
| terminal | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 52; app saw |
| terminal | claude_plugin_offline | SKIP | claude_plugin_offline (this platform: macos); injector accounting: injector sent 2; app saw 17; lost-before-app 0 (Enter: sent 0, app saw 1) |
| terminal | agent_plugins_offline | SKIP | agent_plugins_offline (this platform: macos); injector accounting: injector sent 2; app saw 15; lost-before-app 0 (Enter: sent 0, app saw 1) |
| terminal | network_gate_switch | SKIP | network_gate_switch (this platform: macos); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1);  |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 22; app saw 31; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 9 more than were injected (keys  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); pty writes: 0, of which carrying a CR (\r):  |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 0, app saw 12); pty writes: 0, of which carrying a CR ( |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | PASS | gate on vs off: Δp50 = -20.35 ms (on 180.32 / off 200.67; p95 191.46 / 208.97; n 30 / 30; blocks of 5, relaunch per block); gate on: 4387 fr |
| agent | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 51; app saw 49; lost-before-app 2 (Enter: sent 1, app saw 1); pty writes: 0, of wh |
| agent | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 17; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 15 more th |
| agent | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 207; app saw 224; lost-before-app 0 (Enter: sent 8, app saw 10); app logged 17 more than were injected (k |
| agent | subshell_ready_vim_typeahead | FAIL | vim: the line typed into the inner shell ran (file []); no Write decision before the one that wrote: ['2026-11-09T23:32:24Z [DEBUG] input_or |
| agent | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 185; app saw 200; lost-before-app 0 (Enter: sent 6, app saw 7); app logged 15 more than were injected (ke |
| agent | subshell_ready_secret_after_prompt | FAIL | the secret read after our first line cancelled the attempt (hop why=Secret); the password reached the read -s, not a command: ['set / comman |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 64; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 272; app saw 285; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 13 more than were injected (ke |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 69; lost-bef |
| agent | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty HOME (proved on a pty before the case types anything); injector accounting: injector sent |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 210; app saw 223; lost-before-app 0 (Enter: sent 6, app saw 8); app logged 13 more than were injected (ke |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 260; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 149; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| agent | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 12 more th |
| agent | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 15; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 13 more th |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.44 ms (p95 1.56, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: overhead 5.47  |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | injector accounting: injector sent 631; app saw 575; lost-before-app 56 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR  |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 220; app saw 204; lost-before-app 16 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| agent | remote_tab_enter_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 758; lost-before-app 0 (Enter: sent 21, app saw 26); app logged 3 more than were injected (k |
| agent | shell_nu | PASS | nu app log: 2026-11-09T23:51:21Z [DEBUG] frame 1: updated=["Workspace", "Terminal", "AIBlock", "AIBlock", "AIBlock", "AIBlock", "Input", "AI |
| agent | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2 keys (0 Enter); ap |
| agent | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 52; app saw |
| agent@fake | agent_interactive_password | FAIL | ai_pw: request 2 carries the typed text 'PWT2NG5BY1A2' in a user message (it leaked into the conversation); a fresh agent conversation was s |
| agent@fake | agent_edit_accept | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; partial accept: the diff c |
| agent@fake | agent_edit_reject | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; reject: no follow-up model |
| agent@fake | agent_edit_conflict | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_touch_only | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_endings | FAIL | lf_no_eol: the accepted multi-hunk edit: 218 bytes on disk, 217 expected; first difference at byte 217 (b'\n' vs b''); line endings (crlf, b |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_vim | FAIL | ai_vim: neither hop=fullscreen_handover nor a pty-owned hand_route in the app log after typing into the full-screen program: the keys' route |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_question_answer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer_at_end | FAIL | the steer prompt never reached the model; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the m |
| agent@fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_ssh | FAIL | ai_ssh: the remote `read` at its prompt was never handed to the user within 60 s; a fresh agent conversation was started (Escape, /agent) so |
| agent@fake | agent_restart_kill_stream | FAIL | ar_kill_stream (relaunch): unexpected ERROR line: 2026-11-10T00:13:44Z [ERROR] Failed to read local control mode from secure storage: unknow |
| agent@fake | agent_restart_kill_queued | FAIL | ar_kill_queued (relaunch): unexpected ERROR line: 2026-11-10T00:14:40Z [ERROR] Failed to read local control mode from secure storage: unknow |
| agent@fake | agent_restart_kill_blocked | FAIL | ar_kill_blocked (relaunch): unexpected ERROR line: 2026-11-10T00:15:36Z [ERROR] Failed to read local control mode from secure storage: unkno |
| agent@fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | SKIP | ai_less: no hop=alt_screen_enter within 30 s, so less never took the screen and no keys were typed; a fresh agent conversation was started ( |
| agent@fake | pane_not_local_after_agent_interactive_less | FAIL | after agent_interactive_less the pane's shell was not the root shell (none; root pid 51061); the app is relaunched so later cases start clea |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | SKIP | network_gate_notice (this platform: macos); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1);  |
| agent@fake | agent_gate_deny_allow | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 13; lost-before-app 0 (Enter: sent 0, app saw 1); app logge |
| agent@fake | agent_gate_off_live | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1); app logge |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 52; app saw 50; lost-before-app 2 (Enter: sent 1, app saw 1); pty writ |
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
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 52; app saw 50; lost-before-app 2 (Ente |
| terminal | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more th |
| terminal | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| terminal | subshell_sh_probe | FAIL | dash: the app logged the unsupported-shell event (hop subshell_setup_unsupported); APPCI_SU_SH_USER unset: the `su` to an sh-shelled user le |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 211; app saw 239; lost-before-app 0 (Enter: sent 8, app saw 10); app logged 28 more than were injected (k |
| terminal | subshell_ready_vim_typeahead | FAIL | no Write decision before the one that wrote: ['2026-11-09T23:08:25Z [DEBUG] input_order hop=subshell_ready_judged source=output decision=Wri |
| terminal | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 80; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 28 more t |
| terminal | subshell_ready_secret_after_prompt | FAIL | the secret read after our first line cancelled the attempt (hop why=Secret); the password reached the read -s, not a command: ['set / comman |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 80; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 272; app saw 299; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 27 more than were injected (ke |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 80; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| terminal | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 2468278); the app is relaunched so later ca |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 258; lost-before-app 2 (Enter: sent 7, app saw 8); pty writes: 0, of which carrying a CR (\r |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 2: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 16 key(s)/1 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 19 key( |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 164; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| terminal | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| terminal | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 4.23 ms (p95 6.11, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in  |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 183; lost-before-app 0 (Enter: sent 10, app saw 11); app logged 23 more than were injected ( |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 41; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 25 more than were injected (keys |
| terminal | shell_fish | PASS | injector accounting: injector sent 751; app saw 776; lost-before-app 0 (Enter: sent 25, app saw 26); 4 paste chord(s) not counted as keys; a |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; injector accounting: injector  |
| terminal | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| terminal | ps_read_host_secure | SKIP | PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret); |
| terminal | gitbash_precmd_secret | SKIP | Git Bash pane only (bash precmd_functions; ConPTY prompt hold); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter:  |
| terminal | md_viewer | PASS | injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more than were injected (keys  |
| terminal | zero_network_startup | FAIL | the app tried to reach the network at startup/idle: connect <redacted-endpoint>; connect <redacted-endpoint>; connect <redacted-endpoint>; c |
| terminal | keyring_unavailable_startup | PASS | injector accounting: injector sent 595; app saw 590; lost-before-app 5 (Enter: sent 13, app saw 13); pty writes: 0, of which carrying a CR ( |
| terminal | claude_plugin_offline | PASS | bundle: /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/claude_plugin; networ |
| terminal | agent_plugins_offline | PASS | codex bundle: /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/codex_plugin; n |
| terminal | network_gate_switch | NOREF | NO-REFERENCE: UNVERIFIABLE: the denial notice (toast naming the purpose and host), its 'Allow this origin' link and the log row's 'Allow' bu |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 72; app saw 65; lost-before-app 7 (Enter: sent 2, app saw 0) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2002; app saw 2001; lost-before-app 1 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty wri |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 51; app saw 49; lost-before-app 2 (Ente |
| agent | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more th |
| agent | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 28; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 26 more th |
| agent | subshell_sh_probe | FAIL | dash: the app logged the unsupported-shell event (hop subshell_setup_unsupported); APPCI_SU_SH_USER unset: the `su` to an sh-shelled user le |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 207; app saw 233; lost-before-app 0 (Enter: sent 8, app saw 10); app logged 26 more than were injected (k |
| agent | subshell_ready_vim_typeahead | FAIL | vim: the line typed into the inner shell ran (file []); vim received exactly the typed text: []; while vim ran the judge waited (AltScreen o |
| agent | pane_not_local_after_subshell_ready_vim_typeahead | FAIL | after subshell_ready_vim_typeahead the pane's shell was not the root shell (none; root pid 2676700); the app is relaunched so later cases st |
| agent | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 102; app saw 100; lost-before-app 2 (Enter: sent 2, app saw 2); pty writes: 0, of  |
| agent | subshell_ready_secret_after_prompt | FAIL | the secret read after our first line cancelled the attempt (hop why=Secret); the password reached the read -s, not a command: ['set / comman |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 79; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 272; app saw 298; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 26 more than were injected (ke |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 80; lost-bef |
| agent | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| agent | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 2682283); the app is relaunched so later ca |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 258; lost-before-app 2 (Enter: sent 7, app saw 8); pty writes: 0, of which carrying a CR (\r |
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 10 key(s)/1 |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 20 key( |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 164; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| agent | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| agent | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 28; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 26 more th |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 16.50 ms (p95 17.29, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 180; lost-before-app 0 (Enter: sent 10, app saw 11); app logged 20 more than were injected ( |
| agent | shell_fish | PASS | injector accounting: injector sent 751; app saw 774; lost-before-app 0 (Enter: sent 25, app saw 26); 4 paste chord(s) not counted as keys; a |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-09T23:45:2 |
| agent | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 460; app saw 441; lost-before-app 19 (Enter: sent 18, app saw 18); pty writes: 0, of which carrying a CR  |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | md_viewer | PASS | injector accounting: injector sent 2 keys (0 Enter); app key trace not available, so nothing to compare |
| agent | keyring_unavailable_startup | PASS | injector accounting: injector sent 595; app saw 590; lost-before-app 5 (Enter: sent 13, app saw 12); pty writes: 0, of which carrying a CR ( |
| agent | agent_view_not_restored_after_keyring_unavailable_startup | PASS | keyring_unavailable_startup relaunched the app itself (pid 2710030 -> 2711388); a relaunched app starts in a terminal session, so this is no |
| agent@fake | agent_interactive_password | FAIL | ai_pw: request 2 carries the typed text 'PWT2NG5BY1A2' in a user message (it leaked into the conversation); a fresh agent conversation was s |
| agent@fake | agent_edit_accept | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; partial accept: the diff c |
| agent@fake | agent_edit_reject | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; reject: no follow-up model |
| agent@fake | agent_edit_conflict | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_touch_only | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_endings | FAIL | lf_no_eol: the accepted multi-hunk edit: 218 bytes on disk, 217 expected; first difference at byte 217 (b'\n' vs b''); line endings (crlf, b |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | FAIL | agent_latency_key/streaming: Phosphor overhead 7.24 +/- 0.21 ms (SE) over same app, terminal-mode input editor exceeds the 2 ms budget (over |
| agent@fake | agent_interactive_vim | FAIL | ai_vim: neither hop=fullscreen_handover nor a pty-owned hand_route in the app log after typing into the full-screen program: the keys' route |
| agent@fake | agent_restart_quit_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_stream: the comple |
| agent@fake | agent_restart_quit_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_queued: the comple |
| agent@fake | agent_restart_quit_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_blocked: the compl |
| agent@fake | agent_question_answer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer_at_end | FAIL | the steer prompt never reached the model; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the m |
| agent@fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_ssh | FAIL | ai_ssh: the remote `read` at its prompt was never handed to the user within 60 s; a fresh agent conversation was started (Escape, /agent) so |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@fake | agent_restart_kill_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | SKIP | ai_less: no hop=alt_screen_enter within 30 s, so less never took the screen and no keys were typed; a fresh agent conversation was started ( |
| agent@fake | pane_not_local_after_agent_interactive_less | FAIL | after agent_interactive_less the pane's shell was not the root shell (none; root pid 2732378); the app is relaunched so later cases start cl |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | FAIL | the remote image from an unlisted host was not refused by the gate (no 'network gate: denied "remote-asset" (host hash 714ef6f9d3c78218)' li |
| agent@fake | agent_gate_deny_allow | FAIL | the image from the unlisted loopback NAME localhost was not refused (no 'network gate: denied "remote-asset" (host hash db3f053403ad937a)' l |
| agent@fake | agent_gate_off_live | FAIL | precondition: the gate (on) did not refuse the unlisted origin; with the gate off the request did not reach the listener; with the gate back |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | FAIL | the denied origin was not refused (no 'network gate: denied "remote-asset" (host hash db3f053403ad937a)'); the allowed origin did not receiv |
| agent@fake | agent_multitab_streaming | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 3. |
| agent@fake | left_agent_mode_after_agent_multitab_streaming | FAIL | after agent_multitab_streaming the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 63, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with he |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | pty writes: 242, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with  |
| terminal | paste_instant_enter | PASS | pty writes: 14, of which carrying a CR (\r): 2; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with he |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.88, -> LineEditorReading 60.9; app:  |
| terminal | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal | ps_ctrl_break | FAIL | Ctrl+Break reached the program as SIGBREAK in 0/3 runs: [['READY'], ['READY'], ['READY']]; pty writes: 12, of which carrying a CR (\r): 0; E |
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys') |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with he |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | pty writes: 202, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with  |
| terminal@gitbash | paste_instant_enter | PASS |  |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| terminal@gitbash | ctrl_c_d | FAIL | REGRESSION Ctrl-C: median 87.01 vs baseline build +6.77 ms (+8.4%); re-measured once: median 83.89 (warn); #1570: the harness process ignore |
| terminal@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's ms |
| terminal@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | subshell_ready_rc_questions | FAIL | two answers re-armed the setup (hops verdict=ReArm): 0; the setup was written (hop subshell_setup_ready_write); no cancel: the answers were  |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | FAIL | the setup was written after the clear |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| terminal@gitbash | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in the LOCAL shell, not the remote one: `exit` NOT typed; stopping; ssh trial |
| terminal@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| terminal@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 40.9 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.68 ms (p95 0.92, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with h |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with hel |
| terminal@gitbash | remote_tab_no_helper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote d |
| terminal@gitbash | remote_tab_enter_no_helper | PASS |  |
| terminal@gitbash | ui_completions_as_you_type_local | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| terminal@gitbash | ps_read_host_secure | SKIP | not run in the Git Bash pane: #1882/#1891: Read-Host -AsSecureString is PowerShell; the Git Bash twin is gitbash_precmd_secret |
| terminal@gitbash | gitbash_precmd_secret | FAIL | the PIN did not reach `read -s` whole and in order: gbpin.txt holds [], want ['pint242vjy1u8'] (the first half was handed to the editor as t |
| terminal@gitbash | pane_not_local_after_gitbash_precmd_secret | FAIL | after gitbash_precmd_secret the pane's shell was not the root shell (none; root pid 828); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal@gitbash | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal@gitbash | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal@gitbash | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal@gitbash | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal@gitbash | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | pty writes: 174, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with  |
| agent | paste_instant_enter | PASS | pty writes: 15, of which carrying a CR (\r): 3; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with he |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 16.21, -> LineEditorReading 66.24; app |
| agent | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent | ps_ctrl_break | FAIL | Ctrl+Break reached the program as SIGBREAK in 0/3 runs: [['READY'], ['READY'], ['READY']]; pty writes: 12, of which carrying a CR (\r): 0; E |
| agent | harness_crash | FAIL | the agent pass crashed outside a case: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys') |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | pty writes: 149, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with  |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's ms |
| agent@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | subshell_ready_rc_questions | FAIL | two answers re-armed the setup (hops verdict=ReArm): 0; the setup was written (hop subshell_setup_ready_write); no cancel: the answers were  |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | FAIL | the setup was written after the clear |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T92_fmb5ir never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent@gitbash | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in the LOCAL shell, not the remote one: `exit` NOT typed; stopping; ssh trial |
| agent@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| agent@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with hel |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with h |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with hel |
| agent@gitbash | agent_lrc_no_false_takeover | FAIL | part 2: `printf 'Continue? [y/N] '; read` sat at its prompt for 60 s and was neither handed to the user (a proven tty read must be, #1811) n |
| agent@gitbash | remote_tab_no_helper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote d |
| agent@gitbash | remote_tab_enter_no_helper | PASS |  |
| agent@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| agent@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_interactive_password | FAIL | ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=None); a fresh agent conversation was started (Escape, /age |
| agent@fake | harness_crash | FAIL | the agent@fake pass crashed outside a case: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys') |
| agent@gitbash+fake | agent_interactive_password | SKIP | not run in the Git Bash pane: #1882: the hand-over on ConPTY is the PowerShell bootstrap's Read-Host marker; bash `read -s` in a Git Bash pa |
| agent@gitbash+fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@gitbash+fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 1, of which ca |
| agent@gitbash+fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@gitbash+fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_question_answer | FAIL | answer: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'P |
| agent@gitbash+fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which ca |
| agent@gitbash+fake | agent_question_nodone | FAIL | nodone: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'P |
| agent@gitbash+fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which ca |
| agent@gitbash+fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which ca |
| agent@gitbash+fake | agent_question_slow | FAIL | slow: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Pre |
| agent@gitbash+fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which ca |
| agent@gitbash+fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which ca |
| agent@gitbash+fake | agent_question_after_command | FAIL | after the command ended the ask never became answerable (verdict hung); after_command: the prompt after the question made no model request ( |
| agent@gitbash+fake | agent_question_steer_at_end | FAIL | the steer prompt never reached the model; the turn is neither answerable nor ended (verdict hung, last turn state 'blocked'); the steer prom |
| agent@gitbash+fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@gitbash+fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@gitbash+fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent c |
| agent@gitbash+fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 2, of which ca |
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
| agent_edit_accept | macos | agent@fake | PASS |
| agent_edit_accept | wayland | agent@fake | PASS |
| agent_edit_accept | windows | agent@gitbash+fake | SKIP |
| agent_edit_conflict | macos | agent@fake | PASS |
| agent_edit_conflict | wayland | agent@fake | PASS |
| agent_edit_conflict | windows | agent@gitbash+fake | SKIP |
| agent_edit_endings | macos | agent@fake | FAIL |
| agent_edit_endings | wayland | agent@fake | FAIL |
| agent_edit_endings | windows | agent@gitbash+fake | SKIP |
| agent_edit_reject | macos | agent@fake | PASS |
| agent_edit_reject | wayland | agent@fake | PASS |
| agent_edit_reject | windows | agent@gitbash+fake | SKIP |
| agent_edit_touch_only | macos | agent@fake | PASS |
| agent_edit_touch_only | wayland | agent@fake | PASS |
| agent_edit_touch_only | windows | agent@gitbash+fake | SKIP |
| agent_fault_429 | macos | agent@fake | PASS |
| agent_fault_429 | wayland | agent@fake | PASS |
| agent_fault_429 | windows | agent@gitbash+fake | PASS |
| agent_fault_500 | macos | agent@fake | PASS |
| agent_fault_500 | wayland | agent@fake | PASS |
| agent_fault_500 | windows | agent@gitbash+fake | PASS |
| agent_fault_disconnect | macos | agent@fake | PASS |
| agent_fault_disconnect | wayland | agent@fake | PASS |
| agent_fault_disconnect | windows | agent@gitbash+fake | PASS |
| agent_fault_down | macos | agent@fake | PASS |
| agent_fault_down | wayland | agent@fake | PASS |
| agent_fault_down | windows | agent@gitbash+fake | PASS |
| agent_fault_queued_rows | macos | agent@fake | PASS |
| agent_fault_queued_rows | wayland | agent@fake | PASS |
| agent_fault_queued_rows | windows | agent@gitbash+fake | SKIP |
| agent_fault_slow_cancel | macos | agent@fake | PASS |
| agent_fault_slow_cancel | wayland | agent@fake | PASS |
| agent_fault_slow_cancel | windows | agent@gitbash+fake | PASS |
| agent_gate_deny_allow | macos | agent@fake | SKIP |
| agent_gate_deny_allow | wayland | agent@fake | FAIL |
| agent_gate_deny_allow | windows | agent@gitbash+fake | SKIP |
| agent_gate_key_never_leaves | macos | agent@fake | SKIP |
| agent_gate_key_never_leaves | wayland | agent@fake | FAIL |
| agent_gate_key_never_leaves | windows | agent@gitbash+fake | SKIP |
| agent_gate_off_live | macos | agent@fake | SKIP |
| agent_gate_off_live | wayland | agent@fake | FAIL |
| agent_gate_off_live | windows | agent@gitbash+fake | SKIP |
| agent_interactive_less | macos | agent@fake | SKIP |
| agent_interactive_less | wayland | agent@fake | SKIP |
| agent_interactive_less | windows | agent@gitbash+fake | SKIP |
| agent_interactive_password | macos | agent@fake | FAIL |
| agent_interactive_password | wayland | agent@fake | FAIL |
| agent_interactive_password | windows | agent@fake | FAIL |
| agent_interactive_password | windows | agent@gitbash+fake | SKIP |
| agent_interactive_ssh | macos | agent@fake | FAIL |
| agent_interactive_ssh | wayland | agent@fake | FAIL |
| agent_interactive_ssh | windows | agent@gitbash+fake | SKIP |
| agent_interactive_vim | macos | agent@fake | FAIL |
| agent_interactive_vim | wayland | agent@fake | FAIL |
| agent_interactive_vim | windows | agent@gitbash+fake | SKIP |
| agent_latency_enter | macos | agent@fake | PASS |
| agent_latency_enter | wayland | agent@fake | PASS |
| agent_latency_enter | windows | agent@gitbash+fake | PASS |
| agent_latency_key | macos | agent@fake | PASS |
| agent_latency_key | wayland | agent@fake | FAIL |
| agent_latency_key | windows | agent@gitbash+fake | PASS |
| agent_lrc_no_false_takeover | macos | agent | PASS |
| agent_lrc_no_false_takeover | wayland | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent@gitbash | FAIL |
| agent_mcp_faults | macos | agent@fake | SKIP |
| agent_mcp_faults | wayland | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@gitbash+fake | SKIP |
| agent_multitab_streaming | macos | agent@fake | PASS |
| agent_multitab_streaming | wayland | agent@fake | PASS |
| agent_multitab_streaming | windows | agent@gitbash+fake | SKIP |
| agent_noninteractive_no_takeover | macos | agent@fake | PASS |
| agent_noninteractive_no_takeover | wayland | agent@fake | PASS |
| agent_noninteractive_no_takeover | windows | agent@gitbash+fake | SKIP |
| agent_plugins_offline | macos | terminal | SKIP |
| agent_plugins_offline | wayland | terminal | PASS |
| agent_plugins_offline | windows | terminal@gitbash | SKIP |
| agent_question_after_command | macos | agent@fake | PASS |
| agent_question_after_command | wayland | agent@fake | PASS |
| agent_question_after_command | windows | agent@gitbash+fake | FAIL |
| agent_question_answer | macos | agent@fake | PASS |
| agent_question_answer | wayland | agent@fake | PASS |
| agent_question_answer | windows | agent@gitbash+fake | FAIL |
| agent_question_ctrl_c | macos | agent@fake | PASS |
| agent_question_ctrl_c | wayland | agent@fake | PASS |
| agent_question_ctrl_c | windows | agent@gitbash+fake | PASS |
| agent_question_cut | macos | agent@fake | PASS |
| agent_question_cut | wayland | agent@fake | PASS |
| agent_question_cut | windows | agent@gitbash+fake | PASS |
| agent_question_empty | macos | agent@fake | PASS |
| agent_question_empty | wayland | agent@fake | PASS |
| agent_question_empty | windows | agent@gitbash+fake | PASS |
| agent_question_nodone | macos | agent@fake | PASS |
| agent_question_nodone | wayland | agent@fake | PASS |
| agent_question_nodone | windows | agent@gitbash+fake | FAIL |
| agent_question_slow | macos | agent@fake | PASS |
| agent_question_slow | wayland | agent@fake | PASS |
| agent_question_slow | windows | agent@gitbash+fake | FAIL |
| agent_question_steer | macos | agent@fake | PASS |
| agent_question_steer | wayland | agent@fake | PASS |
| agent_question_steer | windows | agent@gitbash+fake | PASS |
| agent_question_steer_at_end | macos | agent@fake | FAIL |
| agent_question_steer_at_end | wayland | agent@fake | FAIL |
| agent_question_steer_at_end | windows | agent@gitbash+fake | FAIL |
| agent_question_trunc | macos | agent@fake | PASS |
| agent_question_trunc | wayland | agent@fake | PASS |
| agent_question_trunc | windows | agent@gitbash+fake | PASS |
| agent_restart_kill_blocked | macos | agent@fake | FAIL |
| agent_restart_kill_blocked | wayland | agent@fake | PASS |
| agent_restart_kill_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_kill_queued | macos | agent@fake | FAIL |
| agent_restart_kill_queued | wayland | agent@fake | PASS |
| agent_restart_kill_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_kill_stream | macos | agent@fake | FAIL |
| agent_restart_kill_stream | wayland | agent@fake | PASS |
| agent_restart_kill_stream | windows | agent@gitbash+fake | PASS |
| agent_restart_quit_blocked | macos | agent@fake | SKIP |
| agent_restart_quit_blocked | wayland | agent@fake | PASS |
| agent_restart_quit_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_queued | macos | agent@fake | SKIP |
| agent_restart_quit_queued | wayland | agent@fake | PASS |
| agent_restart_quit_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_stream | macos | agent@fake | SKIP |
| agent_restart_quit_stream | wayland | agent@fake | PASS |
| agent_restart_quit_stream | windows | agent@gitbash+fake | SKIP |
| agent_steer_fast | macos | agent@fake | PASS |
| agent_steer_fast | wayland | agent@fake | PASS |
| agent_steer_fast | windows | agent@gitbash+fake | PASS |
| cb_report_problem | macos | terminal | PASS |
| cb_report_problem | wayland | terminal | PASS |
| cb_report_problem | windows | terminal@gitbash | SKIP |
| claude_plugin_offline | macos | terminal | SKIP |
| claude_plugin_offline | wayland | terminal | PASS |
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
| ctrl_c_no_esc_i_local | windows | agent@gitbash | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal@gitbash | SKIP |
| gitbash_precmd_secret | macos | terminal | SKIP |
| gitbash_precmd_secret | wayland | terminal | SKIP |
| gitbash_precmd_secret | windows | terminal@gitbash | FAIL |
| keyring_unavailable_startup | macos | agent | SKIP |
| keyring_unavailable_startup | macos | terminal | SKIP |
| keyring_unavailable_startup | wayland | agent | PASS |
| keyring_unavailable_startup | wayland | terminal | PASS |
| keyring_unavailable_startup | windows | agent@gitbash | SKIP |
| keyring_unavailable_startup | windows | terminal@gitbash | SKIP |
| network_gate_notice | macos | agent@fake | SKIP |
| network_gate_notice | wayland | agent@fake | FAIL |
| network_gate_notice | windows | agent@gitbash+fake | SKIP |
| network_gate_switch | macos | terminal | SKIP |
| network_gate_switch | wayland | terminal | NOREF |
| network_gate_switch | windows | terminal@gitbash | SKIP |
| ps_ctrl_break | macos | agent | SKIP |
| ps_ctrl_break | macos | terminal | SKIP |
| ps_ctrl_break | wayland | agent | SKIP |
| ps_ctrl_break | wayland | terminal | SKIP |
| ps_ctrl_break | windows | agent | FAIL |
| ps_ctrl_break | windows | agent@gitbash | SKIP |
| ps_ctrl_break | windows | terminal | FAIL |
| ps_ctrl_break | windows | terminal@gitbash | SKIP |
| ps_fast_command_young_enter | macos | agent | SKIP |
| ps_fast_command_young_enter | macos | terminal | SKIP |
| ps_fast_command_young_enter | wayland | agent | SKIP |
| ps_fast_command_young_enter | wayland | terminal | SKIP |
| ps_fast_command_young_enter | windows | agent@gitbash | SKIP |
| ps_fast_command_young_enter | windows | terminal@gitbash | SKIP |
| ps_read_host_secure | macos | terminal | SKIP |
| ps_read_host_secure | wayland | terminal | SKIP |
| ps_read_host_secure | windows | terminal@gitbash | SKIP |
| remote_tab_enter_no_helper | macos | agent | SKIP |
| remote_tab_enter_no_helper | macos | terminal | SKIP |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | PASS |
| remote_tab_enter_no_helper | windows | agent@gitbash | PASS |
| remote_tab_enter_no_helper | windows | terminal@gitbash | PASS |
| remote_tab_no_helper | macos | agent | SKIP |
| remote_tab_no_helper | macos | terminal | SKIP |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | PASS |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| ssh_connect_typing_wrapper | macos | agent | SKIP |
| ssh_connect_typing_wrapper | macos | terminal | SKIP |
| ssh_connect_typing_wrapper | wayland | agent | SKIP |
| ssh_connect_typing_wrapper | wayland | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | agent@gitbash | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | SKIP |
| ssh_nested_typeahead_wrapper | macos | agent | SKIP |
| ssh_nested_typeahead_wrapper | macos | terminal | SKIP |
| ssh_nested_typeahead_wrapper | wayland | agent | SKIP |
| ssh_nested_typeahead_wrapper | wayland | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent@gitbash | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | macos | agent | SKIP |
| subshell_ready_docker_t_without_i | macos | terminal | SKIP |
| subshell_ready_docker_t_without_i | wayland | agent | SKIP |
| subshell_ready_docker_t_without_i | wayland | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | macos | agent | PASS |
| subshell_ready_exec_fish_rc | macos | terminal | PASS |
| subshell_ready_exec_fish_rc | wayland | agent | PASS |
| subshell_ready_exec_fish_rc | wayland | terminal | PASS |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_ready_rc_clear | windows | agent@gitbash | FAIL |
| subshell_ready_rc_clear | windows | terminal@gitbash | FAIL |
| subshell_ready_rc_questions | macos | agent | PASS |
| subshell_ready_rc_questions | macos | terminal | PASS |
| subshell_ready_rc_questions | wayland | agent | PASS |
| subshell_ready_rc_questions | wayland | terminal | PASS |
| subshell_ready_rc_questions | windows | agent@gitbash | FAIL |
| subshell_ready_rc_questions | windows | terminal@gitbash | FAIL |
| subshell_ready_relay_shape | macos | agent | SKIP |
| subshell_ready_relay_shape | macos | terminal | SKIP |
| subshell_ready_relay_shape | wayland | agent | SKIP |
| subshell_ready_relay_shape | wayland | terminal | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | macos | agent | FAIL |
| subshell_ready_secret_after_prompt | macos | terminal | FAIL |
| subshell_ready_secret_after_prompt | wayland | agent | FAIL |
| subshell_ready_secret_after_prompt | wayland | terminal | FAIL |
| subshell_ready_secret_after_prompt | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | macos | agent | PASS |
| subshell_ready_unsupported_tcsh | macos | terminal | PASS |
| subshell_ready_unsupported_tcsh | wayland | agent | SKIP |
| subshell_ready_unsupported_tcsh | wayland | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | macos | agent | FAIL |
| subshell_ready_vim_typeahead | macos | terminal | FAIL |
| subshell_ready_vim_typeahead | wayland | agent | FAIL |
| subshell_ready_vim_typeahead | wayland | terminal | FAIL |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | macos | agent | SKIP |
| subshell_ready_zsh_newuser_install | macos | terminal | SKIP |
| subshell_ready_zsh_newuser_install | wayland | agent | FAIL |
| subshell_ready_zsh_newuser_install | wayland | terminal | FAIL |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | macos | agent | PASS |
| subshell_sh_probe | macos | terminal | PASS |
| subshell_sh_probe | wayland | agent | FAIL |
| subshell_sh_probe | wayland | terminal | FAIL |
| subshell_sh_probe | windows | agent@gitbash | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
| ui_completions_as_you_type_local | windows | terminal@gitbash | NOREF |
| zero_network_startup | macos | terminal | SKIP |
| zero_network_startup | wayland | terminal | FAIL |
| zero_network_startup | windows | terminal@gitbash | SKIP |

passed everywhere it ran (12, candidates for the owner's decision): agent_fault_429, agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_slow_cancel, agent_latency_enter, agent_question_ctrl_c, agent_question_cut, agent_question_empty, agent_question_steer, agent_question_trunc, agent_steer_fast
not passed somewhere (fail / skip / no-reference): agent_edit_accept, agent_edit_conflict, agent_edit_endings, agent_edit_reject, agent_edit_touch_only, agent_fault_queued_rows, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_less, agent_interactive_password, agent_interactive_ssh, agent_interactive_vim, agent_latency_key, agent_lrc_no_false_takeover, agent_mcp_faults, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_plugins_offline, agent_question_after_command, agent_question_answer, agent_question_nodone, agent_question_slow, agent_question_steer_at_end, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_kill_stream, agent_restart_quit_blocked, agent_restart_quit_queued, agent_restart_quit_stream, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, gitbash_precmd_secret, keyring_unavailable_startup, network_gate_notice, network_gate_switch, ps_ctrl_break, ps_fast_command_young_enter, ps_read_host_secure, remote_tab_enter_no_helper, remote_tab_no_helper, ssh_connect_typing_wrapper, ssh_nested_typeahead_wrapper, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, ui_completions_as_you_type_local, zero_network_startup
- note: windows/run_slim/logs/agent/phosphor.log.gz: 1191 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794268910.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794268910.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/phosphor.log.gz: 1403 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/phosphor.log.gz: 1357 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.1.log.gz: 2443 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794266698.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794266698.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794269603.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794269603.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.1.log.gz: 2695 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794264443.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794264443.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.1.log.gz: 2940 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794269577.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 1303 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.3.log.gz: 1251 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.1.log.gz: 1586 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794266280.log.in_session.0: 10239 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `remote_server backend::contract_tests::the_shell_arm_handles_directory_symlink_hardlink_and_tilde_targets [test-workspace-rest]`
  - `warp_terminal model::escape_sequences::tests::test_ctrl_break_win32_input_sequence [test-workspace-rest]`
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/agent@fake/agent_question_steer_at_end (case not in previous run)`
  - `wayland/agent/pane_not_local_after_subshell_ready_vim_typeahead (case not in previous run)`
  - `wayland/agent@fake/agent_question_steer_at_end (case not in previous run)`
  - `windows/terminal/ps_ctrl_break (case not in previous run)`
  - `windows/terminal/harness_crash (case not in previous run)`
  - `windows/terminal@gitbash/ctrl_c_d`
  - `windows/terminal@gitbash/subshell_ready_rc_questions`
  - `windows/terminal@gitbash/subshell_ready_rc_clear`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
  - `windows/terminal@gitbash/gitbash_precmd_secret (case not in previous run)`
  - `windows/terminal@gitbash/pane_not_local_after_gitbash_precmd_secret (case not in previous run)`
  - `windows/agent/ps_ctrl_break (case not in previous run)`
  - `windows/agent/harness_crash (case not in previous run)`
  - `windows/agent@gitbash/subshell_ready_rc_questions`
  - `windows/agent@gitbash/subshell_ready_rc_clear`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/ssh_nested_typeahead`
  - `windows/agent@gitbash/agent_lrc_no_false_takeover`
  - `windows/agent@fake/agent_interactive_password`
  - `windows/agent@fake/harness_crash (case not in previous run)`
  - `windows/agent@gitbash+fake/agent_question_answer (case not in previous run)`
  - `windows/agent@gitbash+fake/agent_question_nodone (case not in previous run)`
  - `windows/agent@gitbash+fake/agent_question_slow (case not in previous run)`
  - `windows/agent@gitbash+fake/agent_question_after_command (case not in previous run)`
  - `windows/agent@gitbash+fake/agent_question_steer_at_end (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/subshell_ready_rc_questions`
  - `macos/terminal/subshell_ready_exec_fish_rc`
  - `macos/agent/subshell_ready_rc_questions`
  - `macos/agent/subshell_ready_exec_fish_rc`
  - `macos/agent@fake/agent_edit_accept`
  - `macos/agent@fake/agent_edit_reject`
  - `macos/agent@fake/agent_edit_conflict`
  - `macos/agent@fake/agent_edit_touch_only`
  - `macos/agent@fake/agent_fault_429`
  - `macos/agent@fake/agent_fault_500`
  - `macos/agent@fake/agent_fault_disconnect`
  - `macos/agent@fake/agent_fault_down`
  - `macos/agent@fake/agent_fault_slow_cancel`
  - `macos/agent@fake/agent_fault_queued_rows`
  - `macos/agent@fake/agent_latency_enter`
  - `wayland/terminal/subshell_ready_exec_fish_rc`
  - `wayland/terminal/remote_tab_no_helper`
  - `wayland/terminal/remote_tab_enter_no_helper`
  - `wayland/terminal/md_viewer`
  - `wayland/agent/subshell_ready_exec_fish_rc`
  - `wayland/agent@fake/agent_edit_accept`
  - `wayland/agent@fake/agent_edit_reject`
  - `wayland/agent@fake/agent_edit_conflict`
  - `wayland/agent@fake/agent_edit_touch_only`
  - `wayland/agent@fake/agent_fault_429`
  - `wayland/agent@fake/agent_fault_500`
  - `wayland/agent@fake/agent_fault_disconnect`
  - `wayland/agent@fake/agent_fault_down`
  - `wayland/agent@fake/agent_fault_slow_cancel`
  - `wayland/agent@fake/agent_fault_queued_rows`
  - `wayland/agent@fake/agent_restart_quit_stream`
  - `wayland/agent@fake/agent_restart_quit_queued`
  - `wayland/agent@fake/agent_restart_quit_blocked`
  - `wayland/agent@fake/agent_restart_kill_stream`
  - `wayland/agent@fake/agent_restart_kill_queued`
  - `wayland/agent@fake/agent_restart_kill_blocked`
  - `windows/terminal@gitbash/remote_tab_enter_no_helper`
  - `windows/agent@gitbash/remote_tab_enter_no_helper`
  - `windows/agent@gitbash+fake/agent_fault_429`
  - `windows/agent@gitbash+fake/agent_fault_500`
  - ... and 3 more
- Latency regressions > 5%: 
  - `wayland/terminal/latency_enter enter_to_start: 32.51 -> 37.12 ms (+14.2%)`
- compared against: test-workspace-rest: build/r18w7 (3046481170); app macos: build/r18w7 (3046481170); app wayland: build/r18w7 (3046481170); app windows: build/r18w7 (3046481170)
