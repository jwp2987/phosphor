# build/r18cj  48cabecef  (pipeline 3018419792)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 46 warnings
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): windows 24/25/19; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | failed | 51.7 | phosphor-windows-vm |  |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 9.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.4 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## app windows (app-test-windows-reuse, REUSED build)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal_proof_prekey.png), mode not machine-verified |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it wi |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference conhost (powershe |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: Phosphor overhead 47.23 ms over conhost (powershell.exe) exceeds the 20 |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T140_mfejdb never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T141_zy0ii8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T142_8epyis never reached the REMOTE file (s |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| terminal@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal@gitbash_proof_prekey.png), mode not machine-verified |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| terminal@gitbash | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0 |
| terminal@gitbash | fast_typing | FAIL | ft0: file has 28/30 lines, exact match False first diff: exp='t92_t2ti62i9kq' got='t94_r3rx4vy3h4' [first bad line 6: injector call #648 pos |
| terminal@gitbash | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| terminal@gitbash | tab_enter | PASS |  |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T164_3mdx3v never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T165_woaa5c never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T166_kq9cao never reached the REMOTE file (s |
| terminal@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 30.78 ms over conhost (bash) exceeds the 2 ms budget; Enter to command start: reference measured a |
| terminal@gitbash | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Pho |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| agent | mode_proof | PASS | no OCR engine: screenshot only (shots/agent_proof_prekey.png), mode not machine-verified |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T80_cdf646 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T81_xguci8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T82_c2iz2b never reached the REMOTE file (ss |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| agent@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/agent@gitbash_proof_prekey.png), mode not machine-verified |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | cadence | PASS |  |
| agent@gitbash | fast_typing | FAIL | ft0: file has 18/30 lines, exact match False first diff: exp='t2_5by1a2rogu' got='t14_q7krs3u54h' [first bad line 0: injector call #1612 pos |
| agent@gitbash | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| agent@gitbash | tab_enter | PASS |  |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T80_cdf646 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T81_xguci8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T82_c2iz2b never reached the REMOTE file (ss |
| agent@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 25.88 ms over conhost (bash) exceeds the 2 ms budget; Enter to command start: reference measured a |
| agent@gitbash | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Pho |
| agent@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| agent@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| agent@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `windows/terminal/ctrl_c_d (case not in previous run)`
  - `windows/terminal/ssh_connect_typing (case not in previous run)`
  - `windows/terminal/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal/ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal@gitbash/cadence (case not in previous run)`
  - `windows/terminal@gitbash/fast_typing (case not in previous run)`
  - `windows/terminal@gitbash/paste_instant_enter (case not in previous run)`
  - `windows/terminal@gitbash/ctrl_c_d (case not in previous run)`
  - `windows/terminal@gitbash/ssh_connect_typing (case not in previous run)`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal@gitbash/ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal@gitbash/latency_enter (case not in previous run)`
  - `windows/agent/ctrl_c_d (case not in previous run)`
  - `windows/agent/ssh_connect_typing (case not in previous run)`
  - `windows/agent/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/agent/ssh_nested_typeahead (case not in previous run)`
  - `windows/agent@gitbash/fast_typing (case not in previous run)`
  - `windows/agent@gitbash/paste_instant_enter (case not in previous run)`
  - `windows/agent@gitbash/ctrl_c_d (case not in previous run)`
  - `windows/agent@gitbash/ssh_connect_typing (case not in previous run)`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/agent@gitbash/ssh_nested_typeahead (case not in previous run)`
  - `windows/agent@gitbash/latency_enter (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: app windows: build/r18cg (3018132410)
