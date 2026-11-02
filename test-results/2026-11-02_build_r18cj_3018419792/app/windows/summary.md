**Tested a REUSED build**: job 17664724432 (build-windows-x86_64) of pipeline 3017838858, commit 18fddcd8088da7ef506568624cc7936eff3a47e2; this pipeline 3018419792, commit 48cabecefdf0408489d717aedd9ec1d5f70ecf54 (app-identical).

# App test results (platform=windows, backend=windows, app_display=wayland, seed=1, commit=48cabece, owner_settings=sha256 a0613b641a27)

pass 24, fail 25, skip 19

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal_proof_prekey.png), mode not machine-verified |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it with -NoProfile): runs |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1 |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: Phosphor overhead 47.23 ms over conhost (powershell.exe) exceeds the 20 ms budget; Ctrl-C:  |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T140_mfejdb never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T141_zy0ii8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, ph |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T142_8epyis never reached the REMOTE file (ssh did not give a re |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| terminal@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal@gitbash_proof_prekey.png), mode not machine-verified |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| terminal@gitbash | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (m |
| terminal@gitbash | fast_typing | FAIL | ft0: file has 28/30 lines, exact match False first diff: exp='t92_t2ti62i9kq' got='t94_r3rx4vy3h4' [first bad line 6: injector call #648 posted its ENTER at 07: |
| terminal@gitbash | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| terminal@gitbash | tab_enter | PASS |  |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in t |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T164_3mdx3v never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T165_woaa5c never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, ph |
| terminal@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T166_kq9cao never reached the REMOTE file (ssh did not give a re |
| terminal@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 30.78 ms over conhost (bash) exceeds the 2 ms budget; Enter to command start: reference measured after Phosphor (seque |
| terminal@gitbash | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Phosphor-side keydown t |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/b |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (look |
| terminal@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| agent | mode_proof | PASS | no OCR engine: screenshot only (shots/agent_proof_prekey.png), mode not machine-verified |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in t |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T80_cdf646 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T81_xguci8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, pho |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T82_c2iz2b never reached the REMOTE file (ssh did not give a rem |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| agent@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/agent@gitbash_proof_prekey.png), mode not machine-verified |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | cadence | PASS |  |
| agent@gitbash | fast_typing | FAIL | ft0: file has 18/30 lines, exact match False first diff: exp='t2_5by1a2rogu' got='t14_q7krs3u54h' [first bad line 0: injector call #1612 posted its ENTER at 08: |
| agent@gitbash | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| agent@gitbash | tab_enter | PASS |  |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in t |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T80_cdf646 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T81_xguci8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, pho |
| agent@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T82_c2iz2b never reached the REMOTE file (ssh did not give a rem |
| agent@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 25.88 ms over conhost (bash) exceeds the 2 ms budget; Enter to command start: reference measured after Phosphor (seque |
| agent@gitbash | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Phosphor-side keydown t |
| agent@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/b |
| agent@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (look |
| agent@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 3575.3 / 3627.4 | 3528.1 / 3547.9 (conhost (powershell.exe)) | 47.23 | overhead <= 20 ms | - | pending | FAIL | n=8 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 84.0 / 110.5 | 71.8 / 96.9 (conhost (bash)) | 12.20 | overhead <= 20 ms | - | pending | PASS | n=29 |
| terminal@gitbash | latency_enter/enter_to_start | 99.7 / 180.9 | 68.9 / 80.7 (conhost (bash)) | 30.78 | overhead <= 2 ms | - | pending | FAIL | n=28 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.9 / 10.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal@gitbash | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | ctrl_c_d/ctrl_c | 3615.7 / 3638.4 | 3600.0 / 3660.2 (conhost (powershell.exe)) | 15.67 | overhead <= 20 ms | - | pending | PASS | n=8 |
| agent@gitbash | ctrl_c_d/ctrl_c | 80.2 / 105.5 | 79.2 / 105.5 (conhost (bash)) | 1.01 | overhead <= 20 ms | - | pending | PASS | n=26 |
| agent@gitbash | latency_enter/enter_to_start | 95.0 / 185.8 | 69.1 / 81.5 (conhost (bash)) | 25.88 | overhead <= 2 ms | - | pending | FAIL | n=29 |
| agent@gitbash | latency_key_log/key_to_frame | 3.5 / 9.5 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent@gitbash | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal@gitbash | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal@gitbash | reference | perline | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | conhost (powershell.exe) | 0.0 | 0 | 30 |
| terminal | conhost (powershell.exe) | 0.25 | 1 | 30 |
| terminal@gitbash | conhost (bash) | 0.0 | 0 | 30 |
| terminal@gitbash | conhost (bash) | 0.25 | 0 | 30 |

