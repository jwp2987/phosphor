# build/r18w3  b964572fc  (pipeline 3044955787)

- date: 2026-11-09, tier: fix, pipeline status: running (collected before it finished)
- compile (check-linux): ok, 107 warnings
- unit: 3587 passed, 0 failed, 19 skipped of 3587 run (1 suites)
- app (pass/fail/skip): wayland 31/3/2 +2 no-ref; windows 1/0/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | failed | 0.1 | - |  |
| app-test-lab-linux | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 35.2 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | running | 6.1 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 2.5 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 29.5 | - |  |
| build-macos-arm64 | success | 18.5 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 35.8 | phosphor-windows-vm |  |
| check-linux | success | 7.7 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 1.7 | - |  |
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
| sshlab-image | failed | 0.2 | - |  |
| test-linux | manual | - | - | not run |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | success | 1.7 | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

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
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 19 key(s)/1 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 22 key( |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T243_27i1iv never reached the REMOTE file (s |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 6.04 ms (p95 6.66, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in  |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
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
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 10 key(s)/1 |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 17 key( |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T111_jblqxi never reached the REMOTE file (s |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 15.81 ms (p95 16.10, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured i |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\ |
| agent | shell_fish | PASS | injector accounting: injector sent 751; app saw 746; lost-before-app 5 (Enter: sent 25, app saw 25); 4 paste chord(s) not counted as keys; p |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-09T13:34:4 |
| agent | md_viewer | FAIL | markdown viewer: heading/table text ['phosphormdprobe', 'zebraapple', 'quinceberry'] not visible within 20 s (OCR saw 'orqsearchsessions,age |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |
- note: wayland/run_slim/logs/agent/phosphor.log.gz: 1812 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 1889 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `wayland/terminal/ssh_nested_typeahead`
  - `wayland/agent/ssh_nested_typeahead`
  - `wayland/agent/md_viewer`
- FIXED failures (app): 
  - `wayland/terminal/key_loss_control`
  - `wayland/agent/paste_instant_enter`
  - `wayland/agent/agent_stream_cancel`
- Latency regressions > 5%: 
  - `wayland/terminal/ctrl_c_d ctrl_c: 35.29 -> 37.76 ms (+7.0%)`
  - `wayland/terminal/latency_key_log key_to_frame: 5.34 -> 6.86 ms (+28.5%)`
  - `wayland/agent/ctrl_c_d ctrl_c: 42.10 -> 48.75 ms (+15.8%)`
  - `wayland/agent/latency_key_log key_to_frame: 3.67 -> 17.81 ms (+385.3%)`
- compared against: test-workspace-rest: build/r18w1 (3044614993); app wayland: build/r18d2 (3020063118); app windows: build/r18e4 (3021881600)
