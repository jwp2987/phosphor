# build/r18w9  41340e2fd  (pipeline 3047937983)

- date: 2026-11-10, tier: fix, pipeline status: manual
- compile (check-linux): -
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): macos 55/13/40 +37 no-ref; windows 47/30/154 +46 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | skipped | - | - |  |
| app-test-lab-linux | skipped | - | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 131.7 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | skipped | - | - |  |
| app-test-windows | failed | 161.9 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | skipped | - | - |  |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | success | 19.2 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 37.1 | phosphor-windows-vm |  |
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
| guards | failed | 1.9 | - |  |
| guards-helper-deps | skipped | - | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | skipped | - | - |  |
| integration-linux-pwsh | skipped | - | - |  |
| integration-linux: [1] | skipped | - | - |  |
| integration-linux: [2] | skipped | - | - |  |
| integration-linux: [3] | skipped | - | - |  |
| licenses | skipped | - | - |  |
| mutants | skipped | - | - |  |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | skipped | - | - |  |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.3 | - |  |
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

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | NOREF | injector accounting: injector sent 1; app saw 0; lost-before-app 1 (Enter: sent 0, app saw 0); NO-REFERENCE: UNVERIFIABLE: injector accounti |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2058; app saw 2056; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| terminal | paste_instant_enter | NOREF | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); NO-REFERENCE: UNVERIFIABLE: injector account |
| terminal | tab_enter | NOREF | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 0, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector ac |
| terminal | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ctrl_c_flood_ab | NOREF | gate on vs off: Δp50 = -19.92 ms (on 182.3 / off 202.22; p95 196.11 / 215.43; n 30 / 30; blocks of 5, relaunch per block); gate on: 4550 fra |
| terminal | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 52; app saw 50; lost-before-app 2 (Enter: sent 1, app saw 1); NO-REFERENCE: UNVERI |
| terminal | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 12 more th |
| terminal | subshell_sh_probe | FAIL | dash: the first inner line ran at all (file []); dash: the first inner line ran 10.05 s after its Enter (budget 2 s, #1710); dash: exact lin |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 276; lost-before-app 0 (Enter: sent 9, app saw 11); app logged 15 more than were injected (k |
| terminal | subshell_ready_vim_typeahead | FAIL | the setup was written after vim (hop subshell_setup_ready_write); injector accounting: injector sent 298; app saw 309; lost-before-app 0 (En |
| terminal | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 235; app saw 246; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 11 more than were injected (ke |
| terminal | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| terminal | subshell_ready_secret_after_probe | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| terminal | subshell_sh_probe_empty_ps1 | FAIL | this dash has an EMPTY PS1 (else the case proved nothing about #1935): ['[%{\x1b]133;A\x07%~%# \x1b]133;B\x07%}]']; empty_ps1 typed-before-t |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 67; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 334; lost-before-app 0 (Enter: sent 8, app saw 9); app logged 12 more than were injected (ke |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 65; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty HOME (proved on a pty before the case types anything); injector accounting: injector sent |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 274; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 14 more than were injected (ke |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 262; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 150; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| terminal | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 18; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 16 more th |
| terminal | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 14 more th |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.33 ms (p95 0.92, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 1 (median 113 |
| terminal | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: overhead 3.46  |
| terminal | latency_paint_ab | NOREF | key frame at the end of the key press, on vs off: key->paint Δp50 = -4.72 ms (on 30.5 / off 35.22; p95 42.71 / 43.4; n 30 / 30; blocks of 5, |
| terminal | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.06 / 0.23 (Δ -0.17), f |
| terminal | routing_table | NOREF | injector accounting: injector sent 694; app saw 625; lost-before-app 69 (Enter: sent 28, app saw 28); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal | agent_mode_switch | NOREF | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 4, app saw 4); NO-REFERENCE: UNVERIFIABLE: injector acc |
| terminal | remote_tab_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| terminal | remote_tab_enter_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 15; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 13 more th |
| terminal | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 19; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 17 more th |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 19; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 17 more th |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 31; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 15 more than were injected (keys |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 761; lost-before-app 0 (Enter: sent 21, app saw 26); app logged 6 more than were injected (k |
| terminal | shell_nu | NOREF | nu app log: 2026-11-10T04:36:34Z [DEBUG] frame 1: updated=["GlobalSearchView", "Menu", "AutosuggestionIgnore", "Menu", "EditorView", "Editor |
| terminal | ctrl_c_no_esc_i_local | NOREF | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal | ps_read_host_secure | SKIP | PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret); |
| terminal | gitbash_precmd_secret | SKIP | Git Bash pane only (bash precmd_functions; ConPTY prompt hold); injector accounting: injector sent 2; app saw 13; lost-before-app 0 (Enter:  |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 15; lost- |
| terminal | zero_network_startup | SKIP | zero_network_startup (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0) |
| terminal | pane_not_local_after_zero_network_startup | FAIL | after zero_network_startup the pane's shell was not the root shell (none; root pid 26335); the app is relaunched so later cases start clean |
| terminal | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 2; app saw  |
| terminal | claude_plugin_offline | SKIP | claude_plugin_offline (this platform: macos); injector accounting: injector sent 2; app saw 15; lost-before-app 0 (Enter: sent 0, app saw 1) |
| terminal | agent_plugins_offline | SKIP | agent_plugins_offline (this platform: macos); injector accounting: injector sent 2; app saw 17; lost-before-app 0 (Enter: sent 0, app saw 1) |
| terminal | network_gate_switch | SKIP | network_gate_switch (this platform: macos); injector accounting: injector sent 2; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 1);  |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 22; app saw 32; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 10 more than were injected (keys |
| agent | mode_proof | FAIL | agent mode expected but the screen shows no agent markers ('New Phosphor Agent conversation' / 'Ask anything'); terminal hint present: False |
| agent | cadence | PASS | injector accounting: injector sent 179; app saw 192; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 13 more than were injected (ke |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2005; app saw 2003; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | NOREF | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); NO-REFERENCE: UNVERIFIABLE: injector account |
| agent | tab_enter | NOREF | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 0, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector ac |
| agent | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | NOREF | gate on vs off: Δp50 = -21.75 ms (on 182.9 / off 204.65; p95 194.86 / 216.57; n 30 / 30; blocks of 5, relaunch per block); gate on: 4377 fra |
| agent | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 51; app saw 49; lost-before-app 2 (Enter: sent 1, app saw 1); NO-REFERENCE: UNVERI |
| agent | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 15; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 13 more th |
| agent | subshell_sh_probe | FAIL | dash: the first inner line ran at all (file []); dash: the first inner line ran 10.02 s after its Enter (budget 2 s, #1710); dash: exact lin |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 256; app saw 269; lost-before-app 0 (Enter: sent 9, app saw 11); app logged 13 more than were injected (k |
| agent | subshell_ready_vim_typeahead | FAIL | the setup was written after vim (hop subshell_setup_ready_write); injector accounting: injector sent 298; app saw 309; lost-before-app 0 (En |
| agent | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 235; app saw 249; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 14 more than were injected (ke |
| agent | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| agent | subshell_ready_secret_after_probe | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| agent | subshell_sh_probe_empty_ps1 | FAIL | this dash has an EMPTY PS1 (else the case proved nothing about #1935): ['[%{\x1b]133;A\x07%~%# \x1b]133;B\x07%}]']; empty_ps1 typed-before-t |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 66; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 335; lost-before-app 0 (Enter: sent 8, app saw 9); app logged 13 more than were injected (ke |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 64; lost-bef |
| agent | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty HOME (proved on a pty before the case types anything); injector accounting: injector sent |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 272; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 12 more than were injected (ke |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 262; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 152; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| agent | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 12 more th |
| agent | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 12 more th |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.37 ms (p95 0.94, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 1 (median 82  |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: overhead 2.05  |
| agent | latency_paint_ab | NOREF | key frame at the end of the key press, on vs off: key->paint Δp50 = -2.08 ms (on 31.18 / off 33.26; p95 39.56 / 38.39; n 30 / 30; blocks of  |
| agent | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.24 / 0.37 (Δ -0.13), fram |
| agent | agent_stream_cancel | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | NOREF | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); NO-REFERENCE: UNVERIFIABLE: injector acco |
| agent | routing_table | NOREF | injector accounting: injector sent 630; app saw 575; lost-before-app 55 (Enter: sent 27, app saw 27); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent | agent_mode_switch | NOREF | injector accounting: injector sent 220; app saw 204; lost-before-app 16 (Enter: sent 4, app saw 4); NO-REFERENCE: UNVERIFIABLE: injector acc |
| agent | agent_lrc_no_false_takeover | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| agent | remote_tab_enter_no_helper | SKIP | the macOS test host's remote shell is /bin/bash 3.2: no in-band Tab below bash 4.0 (by design, #1801 review B3); injector accounting: inject |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 12 more th |
| agent | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 13; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 11 more th |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 19; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 17 more th |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 764; lost-before-app 0 (Enter: sent 21, app saw 26); app logged 9 more than were injected (k |
| agent | shell_nu | NOREF | injector accounting: injector sent 951; app saw 942; lost-before-app 9 (Enter: sent 24, app saw 28); NO-REFERENCE: UNVERIFIABLE: injector ac |
| agent | ctrl_c_no_esc_i_local | NOREF | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 16; lost- |
| agent | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 2; app saw  |
| agent | pane_not_local_after_keyring_unavailable_startup | FAIL | after keyring_unavailable_startup the pane's shell was not the root shell (none; root pid 74779); the app is relaunched so later cases start |
| agent@fake | agent_interactive_password | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_accept | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; partial accept: the diff c |
| agent@fake | agent_edit_reject | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; reject: no follow-up model |
| agent@fake | agent_edit_conflict | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_touch_only | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_endings | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_vim | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_vim: hand-over line in  |
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
| agent@fake | agent_question_steer_at_end | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_ssh | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; NO-REFERENCE: UNVERIFIABLE |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@fake | agent_restart_kill_blocked | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@fake | agent_steer_fast | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_less: hand-over line in |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | SKIP | network_gate_notice (this platform: macos); injector accounting: injector sent 2; app saw 14; lost-before-app 0 (Enter: sent 0, app saw 1);  |
| agent@fake | agent_gate_deny_allow | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 12; lost-before-app 0 (Enter: sent 0, app saw 1); app logge |
| agent@fake | agent_gate_off_live | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 19; lost-before-app 0 (Enter: sent 0, app saw 1); app logge |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 52; app saw 50; lost-before-app 2 (Enter: sent 1, app saw 1); NO-REFER |
| agent@fake | agent_multitab_streaming | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 4. |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine; injector accounting: injector sent 1 keys (0 Enter); ap |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 225; app saw 221; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 62, of which  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2277; app saw 2275; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 235, of w |
| terminal | paste_instant_enter | NOREF | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 6, app saw 6); NO-REFERENCE: UNVERIFIABLE: injector account |
| terminal | tab_enter | NOREF | injector accounting: injector sent 686; app saw 672; lost-before-app 14 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal | ctrl_c_d | NOREF | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 9.61, -> LineEditorReading 60.83; app: |
| terminal | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal | ps_ctrl_break | FAIL | the app wrote the Ctrl+Break win32-input sequence (CSI 3;70;...) to the pty in 0/3 runs (the app log's pty_write_bytes); Ctrl+Break reached  |
| terminal | ps_fast_command_young_enter | NOREF | injector accounting: injector sent 772; app saw 768; lost-before-app 4 (Enter: sent 24, app saw 24); NO-REFERENCE: UNVERIFIABLE: injector ac |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| terminal | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | NOREF | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 243; lost-before-app 4 (Enter: sent 12, app saw 12); NO-REFEREN |
| terminal | ssh_nested_typeahead | NOREF | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 133; lost-before-app 4 (Enter: sent 7, app saw 7); NO-REFERENCE |
| terminal | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| terminal | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.75 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| terminal | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.06 / 0.2 (Δ -0.14), fr |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| terminal | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| terminal | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| terminal | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane (#1933): only the Git Bash pane has the wrapper |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 0; lost-bef |
| terminal | giveup_keys_reach_shell | FAIL | 2/20 lines exact and in typed order (got 19 lines); first difference: ('t252_dxfpnf', 't253_2mvkbn'); KEEP-TOGETHER broken: line 3 of the fi |
| terminal | ps_read_host_secure | PASS | injector accounting: injector sent 568; app saw 589; lost-before-app 0 (Enter: sent 6, app saw 7); app logged 21 more than were injected (ke |
| terminal | gitbash_precmd_secret | SKIP | not run in the PowerShell pane: bash precmd_functions with `read -s` after the prompt's Precmd, rec.log to prove the PIN was not run as a co |
| terminal | md_viewer | NOREF | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine; injector accounting: injector sent 1 keys (0 Enter); ap |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 224; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 1 more than were injected (key |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2060; app saw 2058; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 165, of w |
| terminal@gitbash | paste_instant_enter | NOREF | injector accounting: injector sent 14; app saw 7; lost-before-app 7 (Enter: sent 6, app saw 7); NO-REFERENCE: UNVERIFIABLE: injector account |
| terminal@gitbash | tab_enter | NOREF | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| terminal@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's ms |
| terminal@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | subshell_ready_rc_questions | FAIL | both answers reached the rc's reads: ["me command -v shopt >/dev/null 2>&1 && eval '__hc0=${HISTCONTROL-}; HISTCONTROL=ignorespace:${HISTCON |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | FAIL | clear: exact lines in the inner shell then the pane's bash: got ['T174_5ckq9c'], wanted ['T174_5ckq9c', 'back_T179_aof7']; injector accounti |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 266; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 139; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 2 |
| terminal@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 23; lost-befor |
| terminal@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 23; lost-befor |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 38.8 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.67 ms (p95 0.90, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 0 (median 0 u |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal@gitbash | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| terminal@gitbash | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.07 / 0.2 (Δ -0.13), fr |
| terminal@gitbash | routing_table | NOREF | injector accounting: injector sent 645; app saw 575; lost-before-app 70 (Enter: sent 27, app saw 27); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal@gitbash | agent_mode_switch | NOREF | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 6, app saw 4); NO-REFERENCE: UNVERIFIABLE: injector acc |
| terminal@gitbash | remote_tab_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing with a non-bootstrapped shell." /  |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing with a non-bootstrapped shell." /  |
| terminal@gitbash | ssh_gitbash_managed | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing with a non-bootstrapped shell." /  |
| terminal@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: the line typed during the connect never reached the remote shell. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing  |
| terminal@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 178; app saw 203; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 25 more than were injected (ke |
| terminal@gitbash | ui_completions_as_you_type_local | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked; injector accounting: injector sent 16; a |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 0; lost-bef |
| terminal@gitbash | giveup_keys_reach_shell | NOREF | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal@gitbash | ps_read_host_secure | SKIP | not run in the Git Bash pane: #1882/#1891: Read-Host -AsSecureString is PowerShell; the Git Bash twin is gitbash_precmd_secret |
| terminal@gitbash | gitbash_precmd_secret | FAIL | the PIN did not reach `read -s` whole and in order: gbpin.txt holds [], want ['pint305uzylay'] (the first half was handed to the editor as t |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal@gitbash | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal@gitbash | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal@gitbash | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal@gitbash | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal@gitbash | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | injector accounting: injector sent 1106; app saw 1107; lost-before-app 0 (Enter: sent 30, app saw 31); app logged 1 more than were injected  |
| agent | paste_instant_enter | NOREF | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 6, app saw 6); NO-REFERENCE: UNVERIFIABLE: injector account |
| agent | tab_enter | NOREF | injector accounting: injector sent 674; app saw 660; lost-before-app 14 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent | ctrl_c_d | NOREF | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.74, -> LineEditorReading 70.66; app: |
| agent | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent | ps_ctrl_break | FAIL | the app wrote the Ctrl+Break win32-input sequence (CSI 3;70;...) to the pty in 0/3 runs (the app log's pty_write_bytes); Ctrl+Break reached  |
| agent | ps_fast_command_young_enter | NOREF | injector accounting: injector sent 748; app saw 744; lost-before-app 4 (Enter: sent 24, app saw 24); NO-REFERENCE: UNVERIFIABLE: injector ac |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| agent | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | NOREF | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 243; lost-before-app 4 (Enter: sent 12, app saw 12); NO-REFEREN |
| agent | ssh_nested_typeahead | NOREF | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 133; lost-before-app 4 (Enter: sent 7, app saw 7); NO-REFERENCE |
| agent | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| agent | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| agent | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| agent | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.15 / 0.23 (Δ -0.08), fram |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git  |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent | agent_lrc_no_false_takeover | FAIL | part 2: `printf 'Continue? [y/N] '; read` sat at its prompt for 60 s and was neither handed to the user (a proven tty read must be, #1811) n |
| agent | pane_needed_ctrl_c_after_agent_lrc_no_false_takeover | PASS | the first marker command after agent_lrc_no_false_takeover had no effect; it worked after Ctrl-C x3 (candidate APP finding: input focus/Ente |
| agent | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| agent | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| agent | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| agent | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| agent | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane (#1933): only the Git Bash pane has the wrapper |
| agent | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 0; lost-bef |
| agent | giveup_keys_reach_shell | FAIL | 2/20 lines exact and in typed order (got 15 lines); first difference: ('t165_fyh2s6', 't166_5zh4gj'); KEEP-TOGETHER broken: line 3 of the fi |
| agent | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | injector accounting: WARN loss case: injector sent 985; app saw 983; lost-before-app 2 (Enter: sent 30, app saw 30); pty writes: 150, of whi |
| agent@gitbash | paste_instant_enter | NOREF | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 6, app saw 6); NO-REFERENCE: UNVERIFIABLE: injector account |
| agent@gitbash | tab_enter | NOREF | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's ms |
| agent@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | subshell_ready_rc_questions | FAIL | both answers reached the rc's reads: ["me command -v shopt >/dev/null 2>&1 && eval '__hc0=${HISTCONTROL-}; HISTCONTROL=ignorespace:${HISTCON |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | FAIL | clear: the shell in front after leaving is the pane's ROOT shell (got none); clear: exact lines in the inner shell then the pane's bash: got |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 242; app saw 258; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 151; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| agent@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 15; lost-befor |
| agent@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 19; lost-befor |
| agent@gitbash | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| agent@gitbash | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.18 / 0.24 (Δ -0.06), fram |
| agent@gitbash | agent_stream_cancel | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while |
| agent@gitbash | agent_input_exact | NOREF | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); NO-REFERENCE: UNVERIFIABLE: injector acco |
| agent@gitbash | routing_table | NOREF | injector accounting: injector sent 625; app saw 575; lost-before-app 50 (Enter: sent 27, app saw 27); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent@gitbash | agent_mode_switch | NOREF | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); NO-REFERENCE: UNVERIFIABLE: injector acc |
| agent@gitbash | agent_lrc_no_false_takeover | FAIL | part 2: `printf 'Continue? [y/N] '; read` sat at its prompt for 60 s and was neither handed to the user (a proven tty read must be, #1811) n |
| agent@gitbash | remote_tab_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing with a non-bootstrapped shell." /  |
| agent@gitbash | remote_tab_enter_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing with a non-bootstrapped shell." /  |
| agent@gitbash | ssh_gitbash_managed | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing with a non-bootstrapped shell." /  |
| agent@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: the line typed during the connect never reached the remote shell. Test host: sshd.log:  echo "Failed to bootstrap warp. Continuing  |
| agent@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 177; app saw 182; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 5 more than were injected (key |
| agent@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 23; lost-be |
| agent@gitbash | giveup_keys_reach_shell | NOREF | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_interactive_password | FAIL | ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=None); a fresh agent conversation was started (Escape, /age |
| agent@fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_fault_429 | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@fake | agent_latency_key | SKIP | not run in the PowerShell pane: its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake mode |
| agent@fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@fake | agent_question_answer | FAIL | answer: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'P |
| agent@fake | agent_question_cut | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | FAIL | nodone: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'P |
| agent@fake | agent_question_trunc | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | FAIL | slow: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Pre |
| agent@fake | agent_question_ctrl_c | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | FAIL | after the command ended the ask never became answerable (verdict hung); after_command: the prompt after the question made no model request ( |
| agent@fake | agent_question_steer_at_end | FAIL | the turn is neither answerable nor ended (verdict hung, last turn state 'blocked'); the steer prompt never reached the model (after the card |
| agent@fake | agent_latency_enter | SKIP | not run in the PowerShell pane: its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_restart_kill_stream | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
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
| agent@gitbash+fake | agent_interactive_password | SKIP | not run in the Git Bash pane: #1882: the hand-over on ConPTY is the PowerShell bootstrap's Read-Host marker; bash `read -s` in a Git Bash pa |
| agent@gitbash+fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@gitbash+fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_fault_429 | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@gitbash+fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@gitbash+fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@gitbash+fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@gitbash+fake | agent_question_answer | FAIL | answer: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'P |
| agent@gitbash+fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_nodone | FAIL | nodone: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'P |
| agent@gitbash+fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_slow | FAIL | slow: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Pre |
| agent@gitbash+fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_after_command | FAIL | after the command ended the ask never became answerable (verdict hung); after_command: the prompt after the question made no model request ( |
| agent@gitbash+fake | agent_question_steer_at_end | FAIL | the turn is neither answerable nor ended (verdict hung, last turn state 'blocked'); the steer prompt never reached the model (after the card |
| agent@gitbash+fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_kill_stream | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@gitbash+fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a  |
| agent@gitbash+fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent c |
| agent@gitbash+fake | agent_steer_fast | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
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
| agent_edit_accept | windows | agent@fake | SKIP |
| agent_edit_accept | windows | agent@gitbash+fake | SKIP |
| agent_edit_conflict | macos | agent@fake | PASS |
| agent_edit_conflict | windows | agent@fake | SKIP |
| agent_edit_conflict | windows | agent@gitbash+fake | SKIP |
| agent_edit_endings | macos | agent@fake | PASS |
| agent_edit_endings | windows | agent@fake | SKIP |
| agent_edit_endings | windows | agent@gitbash+fake | SKIP |
| agent_edit_reject | macos | agent@fake | PASS |
| agent_edit_reject | windows | agent@fake | SKIP |
| agent_edit_reject | windows | agent@gitbash+fake | SKIP |
| agent_edit_touch_only | macos | agent@fake | PASS |
| agent_edit_touch_only | windows | agent@fake | SKIP |
| agent_edit_touch_only | windows | agent@gitbash+fake | SKIP |
| agent_fault_429 | macos | agent@fake | PASS |
| agent_fault_429 | windows | agent@fake | NOREF |
| agent_fault_429 | windows | agent@gitbash+fake | NOREF |
| agent_fault_500 | macos | agent@fake | PASS |
| agent_fault_500 | windows | agent@fake | PASS |
| agent_fault_500 | windows | agent@gitbash+fake | PASS |
| agent_fault_disconnect | macos | agent@fake | PASS |
| agent_fault_disconnect | windows | agent@fake | PASS |
| agent_fault_disconnect | windows | agent@gitbash+fake | PASS |
| agent_fault_down | macos | agent@fake | PASS |
| agent_fault_down | windows | agent@fake | PASS |
| agent_fault_down | windows | agent@gitbash+fake | PASS |
| agent_fault_queued_rows | macos | agent@fake | PASS |
| agent_fault_queued_rows | windows | agent@fake | SKIP |
| agent_fault_queued_rows | windows | agent@gitbash+fake | SKIP |
| agent_fault_slow_cancel | macos | agent@fake | PASS |
| agent_fault_slow_cancel | windows | agent@fake | PASS |
| agent_fault_slow_cancel | windows | agent@gitbash+fake | PASS |
| agent_gate_deny_allow | macos | agent@fake | SKIP |
| agent_gate_deny_allow | windows | agent@fake | SKIP |
| agent_gate_deny_allow | windows | agent@gitbash+fake | SKIP |
| agent_gate_key_never_leaves | macos | agent@fake | SKIP |
| agent_gate_key_never_leaves | windows | agent@fake | SKIP |
| agent_gate_key_never_leaves | windows | agent@gitbash+fake | SKIP |
| agent_gate_off_live | macos | agent@fake | SKIP |
| agent_gate_off_live | windows | agent@fake | SKIP |
| agent_gate_off_live | windows | agent@gitbash+fake | SKIP |
| agent_interactive_less | macos | agent@fake | PASS |
| agent_interactive_less | windows | agent@fake | SKIP |
| agent_interactive_less | windows | agent@gitbash+fake | SKIP |
| agent_interactive_password | macos | agent@fake | PASS |
| agent_interactive_password | windows | agent@fake | FAIL |
| agent_interactive_password | windows | agent@gitbash+fake | SKIP |
| agent_interactive_ssh | macos | agent@fake | NOREF |
| agent_interactive_ssh | windows | agent@fake | SKIP |
| agent_interactive_ssh | windows | agent@gitbash+fake | SKIP |
| agent_interactive_vim | macos | agent@fake | PASS |
| agent_interactive_vim | windows | agent@fake | SKIP |
| agent_interactive_vim | windows | agent@gitbash+fake | SKIP |
| agent_latency_enter | macos | agent@fake | NOREF |
| agent_latency_enter | windows | agent@fake | SKIP |
| agent_latency_enter | windows | agent@gitbash+fake | NOREF |
| agent_latency_key | macos | agent@fake | NOREF |
| agent_latency_key | windows | agent@fake | SKIP |
| agent_latency_key | windows | agent@gitbash+fake | NOREF |
| agent_lrc_no_false_takeover | macos | agent | NOREF |
| agent_lrc_no_false_takeover | windows | agent | FAIL |
| agent_lrc_no_false_takeover | windows | agent@gitbash | FAIL |
| agent_mcp_faults | macos | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@gitbash+fake | SKIP |
| agent_multitab_streaming | macos | agent@fake | NOREF |
| agent_multitab_streaming | windows | agent@fake | SKIP |
| agent_multitab_streaming | windows | agent@gitbash+fake | SKIP |
| agent_noninteractive_no_takeover | macos | agent@fake | PASS |
| agent_noninteractive_no_takeover | windows | agent@fake | SKIP |
| agent_noninteractive_no_takeover | windows | agent@gitbash+fake | SKIP |
| agent_plugins_offline | macos | terminal | SKIP |
| agent_plugins_offline | windows | terminal | SKIP |
| agent_plugins_offline | windows | terminal@gitbash | SKIP |
| agent_question_after_command | macos | agent@fake | PASS |
| agent_question_after_command | windows | agent@fake | FAIL |
| agent_question_after_command | windows | agent@gitbash+fake | FAIL |
| agent_question_answer | macos | agent@fake | PASS |
| agent_question_answer | windows | agent@fake | FAIL |
| agent_question_answer | windows | agent@gitbash+fake | FAIL |
| agent_question_ctrl_c | macos | agent@fake | PASS |
| agent_question_ctrl_c | windows | agent@fake | NOREF |
| agent_question_ctrl_c | windows | agent@gitbash+fake | PASS |
| agent_question_cut | macos | agent@fake | PASS |
| agent_question_cut | windows | agent@fake | NOREF |
| agent_question_cut | windows | agent@gitbash+fake | PASS |
| agent_question_empty | macos | agent@fake | PASS |
| agent_question_empty | windows | agent@fake | PASS |
| agent_question_empty | windows | agent@gitbash+fake | PASS |
| agent_question_nodone | macos | agent@fake | PASS |
| agent_question_nodone | windows | agent@fake | FAIL |
| agent_question_nodone | windows | agent@gitbash+fake | FAIL |
| agent_question_slow | macos | agent@fake | PASS |
| agent_question_slow | windows | agent@fake | FAIL |
| agent_question_slow | windows | agent@gitbash+fake | FAIL |
| agent_question_steer | macos | agent@fake | PASS |
| agent_question_steer | windows | agent@fake | NOREF |
| agent_question_steer | windows | agent@gitbash+fake | PASS |
| agent_question_steer_at_end | macos | agent@fake | PASS |
| agent_question_steer_at_end | windows | agent@fake | FAIL |
| agent_question_steer_at_end | windows | agent@gitbash+fake | FAIL |
| agent_question_trunc | macos | agent@fake | PASS |
| agent_question_trunc | windows | agent@fake | NOREF |
| agent_question_trunc | windows | agent@gitbash+fake | PASS |
| agent_restart_kill_blocked | macos | agent@fake | NOREF |
| agent_restart_kill_blocked | windows | agent@fake | SKIP |
| agent_restart_kill_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_kill_queued | macos | agent@fake | NOREF |
| agent_restart_kill_queued | windows | agent@fake | SKIP |
| agent_restart_kill_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_kill_stream | macos | agent@fake | PASS |
| agent_restart_kill_stream | windows | agent@fake | NOREF |
| agent_restart_kill_stream | windows | agent@gitbash+fake | NOREF |
| agent_restart_quit_blocked | macos | agent@fake | SKIP |
| agent_restart_quit_blocked | windows | agent@fake | SKIP |
| agent_restart_quit_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_queued | macos | agent@fake | SKIP |
| agent_restart_quit_queued | windows | agent@fake | SKIP |
| agent_restart_quit_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_stream | macos | agent@fake | SKIP |
| agent_restart_quit_stream | windows | agent@fake | SKIP |
| agent_restart_quit_stream | windows | agent@gitbash+fake | SKIP |
| agent_steer_fast | macos | agent@fake | NOREF |
| agent_steer_fast | windows | agent@fake | SKIP |
| agent_steer_fast | windows | agent@gitbash+fake | NOREF |
| cb_report_problem | macos | terminal | PASS |
| cb_report_problem | windows | terminal | SKIP |
| cb_report_problem | windows | terminal@gitbash | SKIP |
| claude_plugin_offline | macos | terminal | SKIP |
| claude_plugin_offline | windows | terminal | SKIP |
| claude_plugin_offline | windows | terminal@gitbash | SKIP |
| ctrl_c_flood_ab | macos | agent | NOREF |
| ctrl_c_flood_ab | macos | terminal | NOREF |
| ctrl_c_flood_ab | windows | agent | SKIP |
| ctrl_c_flood_ab | windows | agent@gitbash | SKIP |
| ctrl_c_flood_ab | windows | terminal | SKIP |
| ctrl_c_flood_ab | windows | terminal@gitbash | SKIP |
| ctrl_c_no_esc_i_local | macos | agent | NOREF |
| ctrl_c_no_esc_i_local | macos | terminal | NOREF |
| ctrl_c_no_esc_i_local | windows | agent | SKIP |
| ctrl_c_no_esc_i_local | windows | agent@gitbash | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal@gitbash | SKIP |
| gitbash_precmd_secret | macos | terminal | SKIP |
| gitbash_precmd_secret | windows | terminal | SKIP |
| gitbash_precmd_secret | windows | terminal@gitbash | FAIL |
| giveup_keys_reach_shell | macos | agent | PASS |
| giveup_keys_reach_shell | macos | terminal | PASS |
| giveup_keys_reach_shell | windows | agent | FAIL |
| giveup_keys_reach_shell | windows | agent@gitbash | NOREF |
| giveup_keys_reach_shell | windows | terminal | FAIL |
| giveup_keys_reach_shell | windows | terminal@gitbash | NOREF |
| keyring_unavailable_startup | macos | agent | SKIP |
| keyring_unavailable_startup | macos | terminal | SKIP |
| keyring_unavailable_startup | windows | agent | SKIP |
| keyring_unavailable_startup | windows | agent@gitbash | SKIP |
| keyring_unavailable_startup | windows | terminal | SKIP |
| keyring_unavailable_startup | windows | terminal@gitbash | SKIP |
| latency_key_views_ab | macos | agent | NOREF |
| latency_key_views_ab | macos | terminal | NOREF |
| latency_key_views_ab | windows | agent | NOREF |
| latency_key_views_ab | windows | agent@gitbash | NOREF |
| latency_key_views_ab | windows | terminal | NOREF |
| latency_key_views_ab | windows | terminal@gitbash | NOREF |
| latency_paint_ab | macos | agent | NOREF |
| latency_paint_ab | macos | terminal | NOREF |
| latency_paint_ab | windows | agent | SKIP |
| latency_paint_ab | windows | agent@gitbash | SKIP |
| latency_paint_ab | windows | terminal | SKIP |
| latency_paint_ab | windows | terminal@gitbash | SKIP |
| network_gate_notice | macos | agent@fake | SKIP |
| network_gate_notice | windows | agent@fake | SKIP |
| network_gate_notice | windows | agent@gitbash+fake | SKIP |
| network_gate_switch | macos | terminal | SKIP |
| network_gate_switch | windows | terminal | SKIP |
| network_gate_switch | windows | terminal@gitbash | SKIP |
| ps_ctrl_break | macos | agent | SKIP |
| ps_ctrl_break | macos | terminal | SKIP |
| ps_ctrl_break | windows | agent | FAIL |
| ps_ctrl_break | windows | agent@gitbash | SKIP |
| ps_ctrl_break | windows | terminal | FAIL |
| ps_ctrl_break | windows | terminal@gitbash | SKIP |
| ps_fast_command_young_enter | macos | agent | SKIP |
| ps_fast_command_young_enter | macos | terminal | SKIP |
| ps_fast_command_young_enter | windows | agent | NOREF |
| ps_fast_command_young_enter | windows | agent@gitbash | SKIP |
| ps_fast_command_young_enter | windows | terminal | NOREF |
| ps_fast_command_young_enter | windows | terminal@gitbash | SKIP |
| ps_read_host_secure | macos | terminal | SKIP |
| ps_read_host_secure | windows | terminal | PASS |
| ps_read_host_secure | windows | terminal@gitbash | SKIP |
| remote_tab_enter_no_helper | macos | agent | SKIP |
| remote_tab_enter_no_helper | macos | terminal | SKIP |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | FAIL |
| remote_tab_no_helper | macos | agent | SKIP |
| remote_tab_no_helper | macos | terminal | SKIP |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| ssh_connect_typing_wrapper | macos | agent | SKIP |
| ssh_connect_typing_wrapper | macos | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | agent | SKIP |
| ssh_connect_typing_wrapper | windows | agent@gitbash | SKIP |
| ssh_connect_typing_wrapper | windows | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | SKIP |
| ssh_gitbash_kill_switch | macos | agent | SKIP |
| ssh_gitbash_kill_switch | macos | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | agent | SKIP |
| ssh_gitbash_kill_switch | windows | agent@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | terminal@gitbash | PASS |
| ssh_gitbash_managed | macos | agent | SKIP |
| ssh_gitbash_managed | macos | terminal | SKIP |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | macos | agent | SKIP |
| ssh_gitbash_typeahead_managed | macos | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal@gitbash | FAIL |
| ssh_nested_typeahead_wrapper | macos | agent | SKIP |
| ssh_nested_typeahead_wrapper | macos | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent@gitbash | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | macos | agent | SKIP |
| subshell_ready_docker_t_without_i | macos | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | macos | agent | PASS |
| subshell_ready_exec_fish_rc | macos | terminal | PASS |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | agent@gitbash | FAIL |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | FAIL |
| subshell_ready_rc_questions | macos | agent | PASS |
| subshell_ready_rc_questions | macos | terminal | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | agent@gitbash | FAIL |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | FAIL |
| subshell_ready_relay_shape | macos | agent | SKIP |
| subshell_ready_relay_shape | macos | terminal | SKIP |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_probe | macos | agent | FAIL |
| subshell_ready_secret_after_probe | macos | terminal | FAIL |
| subshell_ready_secret_after_probe | windows | agent | SKIP |
| subshell_ready_secret_after_probe | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | terminal | SKIP |
| subshell_ready_secret_after_probe | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | macos | agent | FAIL |
| subshell_ready_secret_after_prompt | macos | terminal | FAIL |
| subshell_ready_secret_after_prompt | windows | agent | SKIP |
| subshell_ready_secret_after_prompt | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | macos | agent | PASS |
| subshell_ready_unsupported_tcsh | macos | terminal | PASS |
| subshell_ready_unsupported_tcsh | windows | agent | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | macos | agent | FAIL |
| subshell_ready_vim_typeahead | macos | terminal | FAIL |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | macos | agent | SKIP |
| subshell_ready_zsh_newuser_install | macos | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | macos | agent | FAIL |
| subshell_sh_probe | macos | terminal | FAIL |
| subshell_sh_probe | windows | agent | SKIP |
| subshell_sh_probe | windows | agent@gitbash | SKIP |
| subshell_sh_probe | windows | terminal | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | macos | agent | FAIL |
| subshell_sh_probe_empty_ps1 | macos | terminal | FAIL |
| subshell_sh_probe_empty_ps1 | windows | agent | SKIP |
| subshell_sh_probe_empty_ps1 | windows | agent@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | NOREF |
| zero_network_startup | macos | terminal | SKIP |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | SKIP |

passed everywhere it ran (5, candidates for the owner's decision): agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_slow_cancel, agent_question_empty
not passed somewhere (fail / skip / no-reference): agent_edit_accept, agent_edit_conflict, agent_edit_endings, agent_edit_reject, agent_edit_touch_only, agent_fault_429, agent_fault_queued_rows, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_less, agent_interactive_password, agent_interactive_ssh, agent_interactive_vim, agent_latency_enter, agent_latency_key, agent_lrc_no_false_takeover, agent_mcp_faults, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_plugins_offline, agent_question_after_command, agent_question_answer, agent_question_ctrl_c, agent_question_cut, agent_question_nodone, agent_question_slow, agent_question_steer, agent_question_steer_at_end, agent_question_trunc, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_kill_stream, agent_restart_quit_blocked, agent_restart_quit_queued, agent_restart_quit_stream, agent_steer_fast, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, gitbash_precmd_secret, giveup_keys_reach_shell, keyring_unavailable_startup, latency_key_views_ab, latency_paint_ab, network_gate_notice, network_gate_switch, ps_ctrl_break, ps_fast_command_young_enter, ps_read_host_secure, remote_tab_enter_no_helper, remote_tab_no_helper, ssh_connect_typing_wrapper, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, subshell_sh_probe_empty_ps1, ui_completions_as_you_type_local, zero_network_startup
- note: windows/run_slim/logs/agent@gitbash/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: windows/run_slim/logs/terminal/case_latency_enter.log: latency evidence skipped (platform budget 2048 KB spent)
- note: windows/run_slim/logs/terminal/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter.log: latency evidence skipped (platform budget 2048 KB spent)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_key_log.log: latency evidence skipped (platform budget 2048 KB spent)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: windows/run_slim/logs/agent/acct/launch0_1794287302.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794287302.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794292685.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794288597.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794292685.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794284457.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794284457.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794284457.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794284457.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794286107.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794286107.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/case_latency_key_log.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/agent/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/agent/case_latency_paint.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/agent/case_latency_paint_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/terminal/case_ctrl_c_flood_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/terminal/case_latency_key_log.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/terminal/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/terminal/case_latency_paint.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/terminal/case_latency_paint_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: macos/run_slim/logs/agent/acct/launch0_1794287826.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794286413.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794286413.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794287826.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794287826.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794289943.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794289943.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794289943.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794284921.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794284921.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794283426.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794283426.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794284921.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794284921.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: run dir is 5.3 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/subshell_sh_probe`
  - `macos/terminal/subshell_ready_secret_after_probe (case not in previous run)`
  - `macos/terminal/subshell_sh_probe_empty_ps1 (case not in previous run)`
  - `macos/agent/mode_proof`
  - `macos/agent/subshell_sh_probe`
  - `macos/agent/subshell_ready_secret_after_probe (case not in previous run)`
  - `macos/agent/subshell_sh_probe_empty_ps1 (case not in previous run)`
  - `macos/agent/pane_not_local_after_keyring_unavailable_startup (case not in previous run)`
  - `windows/terminal/giveup_keys_reach_shell (case not in previous run)`
  - `windows/terminal@gitbash/remote_tab_enter_no_helper`
  - `windows/terminal@gitbash/ssh_gitbash_managed (case not in previous run)`
  - `windows/terminal@gitbash/ssh_gitbash_typeahead_managed (case not in previous run)`
  - `windows/agent/agent_lrc_no_false_takeover (case not in previous run)`
  - `windows/agent/giveup_keys_reach_shell (case not in previous run)`
  - `windows/agent@gitbash/remote_tab_enter_no_helper`
  - `windows/agent@gitbash/ssh_gitbash_managed (case not in previous run)`
  - `windows/agent@gitbash/ssh_gitbash_typeahead_managed (case not in previous run)`
  - `windows/agent@fake/agent_question_answer (case not in previous run)`
  - `windows/agent@fake/agent_question_nodone (case not in previous run)`
  - `windows/agent@fake/agent_question_slow (case not in previous run)`
  - `windows/agent@fake/agent_question_after_command (case not in previous run)`
  - `windows/agent@fake/agent_question_steer_at_end (case not in previous run)`
- FIXED failures (app): 
  - `macos/agent@fake/agent_interactive_password`
  - `macos/agent@fake/agent_edit_endings`
  - `macos/agent@fake/agent_interactive_vim`
  - `macos/agent@fake/agent_question_steer_at_end`
  - `macos/agent@fake/agent_restart_kill_stream`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/ssh_nested_typeahead`
- Latency regressions > 5%: 
  - `macos/agent@fake/agent_latency_key streaming: 1.93 -> 3.60 ms (+86.5%)`
- compared against: app macos: build/r18w8 (3047198675); app windows: build/r18w8 (3047198675)
