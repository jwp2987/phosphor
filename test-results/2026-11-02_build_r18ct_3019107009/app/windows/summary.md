# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=167012a7, owner_settings=sha256 a0613b641a27)

pass 29, fail 30, skip 9, no-reference 1 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal_proof_prekey.png), mode not machine-verified |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conh |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in t |
| terminal | pane_wedged_after_ctrl_c_d | FAIL | the powershell pane did not run a marker command (no effect file) after ctrl_c_d, twice: the app is relaunched so the remaining cases measure something |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T146_igh1pn never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | pane_wedged_after_ssh_connect_typing | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_connect_typing, twice: the app is relaunched so the remaining cases measure somethin |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T151_c5nz8v never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, ph |
| terminal | pane_wedged_after_ssh_enter_then_immediate_command | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_enter_then_immediate_command, twice: the app is relaunched so the remaining cases me |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T156_zrw4d9 never reached the REMOTE file (ssh did not give a re |
| terminal | pane_wedged_after_ssh_nested_typeahead | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_nested_typeahead, twice: the app is relaunched so the remaining cases measure someth |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 82.39 ms over conhost (powershell.exe) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_write me |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| terminal@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal@gitbash_proof_prekey.png), mode not machine-verified |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS |  |
| terminal@gitbash | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) ga |
| terminal@gitbash | fast_typing | FAIL | ft25: file has 29/30 lines, exact match False first diff: exp='t134_xvjqqqwyr3' got='t135_jiqome8m81' [first bad line 17: injector call #753 posted its ENTER at |
| terminal@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| terminal@gitbash | tab_enter | PASS |  |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 6/33 sample lines reached the shell NOT exactly as typed; e.g. one character LOST: type |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T168_emlmt1 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T170_ve0m65 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, ph |
| terminal@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T172_tz8qqt never reached the REMOTE file (ssh did not give a re |
| terminal@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 25.01 ms over conhost (bash) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_write median 0.88  |
| terminal@gitbash | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/b |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (look |
| terminal@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| agent | mode_proof | PASS | no OCR engine: screenshot only (shots/agent_proof_prekey.png), mode not machine-verified |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only; the overhead budge |
| agent | pane_wedged_after_ctrl_c_d | FAIL | the powershell pane did not run a marker command (no effect file) after ctrl_c_d, twice: the app is relaunched so the remaining cases measure something |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T55_srpy9a never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent | pane_wedged_after_ssh_connect_typing | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_connect_typing, twice: the app is relaunched so the remaining cases measure somethin |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T60_e6xt16 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, pho |
| agent | pane_wedged_after_ssh_enter_then_immediate_command | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_enter_then_immediate_command, twice: the app is relaunched so the remaining cases me |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T65_1zve5p never reached the REMOTE file (ssh did not give a rem |
| agent | pane_wedged_after_ssh_nested_typeahead | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_nested_typeahead, twice: the app is relaunched so the remaining cases measure someth |
| agent@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/agent@gitbash_proof_prekey.png), mode not machine-verified |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | FAIL | ft0: file has 28/30 lines, exact match False first diff: exp='t11_5w0wa88v3b' got='t12_l9lf9qcefb' [first bad line 9: injector call #1509 posted its ENTER at 12 |
| agent@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| agent@gitbash | tab_enter | PASS |  |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 6/33 sample lines reached the shell NOT exactly as typed; e.g. characters REORDERED (a  |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T53_fif2py never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T55_h180hs never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, pho |
| agent@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T57_bbpqnl never reached the REMOTE file (ssh did not give a rem |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | -2273.2 / -1072.6 | -355.9 / -322.8 (conhost (powershell.exe)) | -1917.25 | overhead <= 20 ms | - | pending | PASS | n=8 |
| terminal | latency_enter/enter_to_start | 89.2 / 106.8 | 6.8 / 9.7 (conhost (powershell.exe)) | 82.39 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 80.3 / 105.6 | 72.0 / 97.9 (conhost (bash)) | 8.25 | overhead <= 20 ms | - | pending | PASS | n=24 |
| terminal@gitbash | latency_enter/enter_to_start | 92.3 / 119.1 | 67.2 / 76.5 (conhost (bash)) | 25.01 | overhead <= 2 ms | - | pending | FAIL | n=29 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.7 / 10.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal@gitbash | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |

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
