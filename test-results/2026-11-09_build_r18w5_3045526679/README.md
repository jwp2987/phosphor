# build/r18w5  2ac9f757a  (pipeline 3045526679)

- date: 2026-11-09, tier: fix, pipeline status: manual
- compile (check-linux): ok, 110 warnings
- unit: 5230 passed, 0 failed, 9009 skipped of 5230 run (2 suites)
- app (pass/fail/skip): macos 32/2/0 +4 no-ref; wayland 32/0/2 +4 no-ref; windows 48/1/13 +4 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | success | 34.0 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 60.8 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 41.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.3 | - |  |
| build-macos-arm64 | success | 18.7 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 35.2 | phosphor-windows-vm |  |
| check-linux | success | 7.8 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 2.0 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
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
| sshlab-image | manual | - | - | not run |
| test-linux | success | 6.0 | - |  |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | success | 2.9 | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

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
| terminal | ctrl_c_d | FAIL | REGRESSION Ctrl-C under flood: median 289.67 vs baseline build +22.56 ms (+8.4%); Ctrl-C: same trap/timestamp command typed in both terminal |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 263; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 149; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.39 ms (p95 1.04, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; WARN Key to paint: median 50.67 vs baselin |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 575; lost-before-app 69 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR  |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\ |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 746; lost-before-app 9 (Enter: sent 21, app saw 25); pty writes: 0, of which carrying a CR ( |
| terminal | shell_nu | PASS | nu app log: 2026-11-09T15:21:40Z [DEBUG] frame 1: updated=["Input", "ActionButton", "ActionButton", "Menu", "ActionButton", "ActionButton",  |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 0; lost-b |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); pty writes: 0, of which carrying a CR (\r):  |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 0, app saw 12); pty writes: 0, of which carrying a CR ( |
| agent | ctrl_c_d | FAIL | REGRESSION Ctrl-C under flood: median 287.57 vs baseline build +42.64 ms (+17.4%); Ctrl-C: same trap/timestamp command typed in both termina |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 259; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 151; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 1 |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.38 ms (p95 0.76, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; WARN Key to paint: median 51.62 vs baselin |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | injector accounting: injector sent 631; app saw 575; lost-before-app 56 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR  |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 220; app saw 204; lost-before-app 16 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 746; lost-before-app 9 (Enter: sent 21, app saw 25); pty writes: 0, of which carrying a CR ( |
| agent | shell_nu | PASS | nu app log: 2026-11-09T15:45:25Z [DEBUG] frame 1: updated=["ActionButton", "UseAgentToolbar", "Menu", "RightPanelView", "PaneHeader", "Works |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 0; lost-b |

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
| terminal | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: insufficient evidence (reference series drifted -12.00 ms between its halves (sequential A-then-B measurement)): INCON |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.2 s trial 2: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 8 key(s)/8  |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 17 key( |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 164; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 15.72 ms (p95 16.00, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to shell (no rout |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| terminal | shell_fish | PASS | injector accounting: injector sent 751; app saw 746; lost-before-app 5 (Enter: sent 25, app saw 25); 4 paste chord(s) not counted as keys; p |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; injector accounting: injector  |
| terminal | md_viewer | PASS | injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2002; app saw 2001; lost-before-app 1 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty wri |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 16 key( |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 163; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 16.03 ms (p95 16.42, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | shell_fish | PASS | injector accounting: injector sent 751; app saw 746; lost-before-app 5 (Enter: sent 25, app saw 25); 4 paste chord(s) not counted as keys; p |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-09T15:45:5 |
| agent | md_viewer | PASS | injector accounting: injector sent 1; app saw 0; lost-before-app 1 (Enter: sent 0, app saw 0) |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with he |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | pty writes: 223, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with  |
| terminal | paste_instant_enter | PASS | pty writes: 14, of which carrying a CR (\r): 2; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with he |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.46, -> LineEditorReading 56.97; app: |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.65 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with he |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | pty writes: 190, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with  |
| terminal@gitbash | paste_instant_enter | PASS |  |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| terminal@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 44.2 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.61 ms (p95 0.75, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with h |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with hel |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | pty writes: 171, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with  |
| agent | paste_instant_enter | PASS | pty writes: 15, of which carrying a CR (\r): 3; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with he |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 9.07, -> LineEditorReading 54.33; app: |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.1 s): T58_j8mrtq never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git  |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | pty writes: 144, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with  |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with h |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with hel |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with h |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with hel |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |
- note: windows/run_slim/logs/agent/phosphor.log.gz: 1158 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/phosphor.log.gz: 2143 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.log.gz: 1320 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/phosphor.log.gz: 3529 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/phosphor.log.gz: 3196 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.log.gz: 1821 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 1892 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `windows/agent/ssh_connect_typing (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/mode_proof`
  - `macos/terminal/cadence`
  - `macos/terminal/key_loss_control`
  - `macos/terminal/enter_loss_control`
  - `macos/terminal/fast_typing`
  - `macos/terminal/paste_instant_enter`
  - `macos/terminal/tab_enter`
  - `macos/terminal/ssh_connect_typing`
  - `macos/agent/mode_proof`
  - `macos/agent/cadence`
  - `macos/agent/fast_typing`
  - `macos/agent/paste_instant_enter`
  - `macos/agent/tab_enter`
  - `macos/agent/ssh_connect_typing`
  - `wayland/terminal/ssh_nested_typeahead`
  - `wayland/agent/ssh_nested_typeahead`
  - `wayland/agent/md_viewer`
- Latency regressions > 5%: 
  - `wayland/terminal/ctrl_c_d ctrl_c: 37.76 -> 41.48 ms (+9.9%)`
  - `wayland/terminal/latency_enter enter_to_start: 32.78 -> 43.20 ms (+31.8%)`
  - `wayland/terminal/latency_key_log key_to_frame: 6.86 -> 17.69 ms (+157.9%)`
- compared against: test-linux: build/r18u5 (3025066346); test-workspace-rest: build/r18w3 (3044955787); app macos: build/r18w1 (3044614993); app wayland: build/r18w3 (3044955787); app windows: build/r18w3 (3044955787)
