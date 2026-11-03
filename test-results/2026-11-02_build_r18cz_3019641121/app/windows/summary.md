# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=7f0be20b, owner_settings=sha256 a0613b641a27)

pass 26, fail 29, skip 5, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal_proof_prekey.png), mode not machine-verified |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conh |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that predates the key) on  |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T145_n3bsgx never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T152_nz8vpg never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, ph |
| terminal | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T157_w4d94b never reached the REMOTE file (ssh did not give a re |
| terminal | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 87.52 ms over conhost (powershell.exe) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_write me |
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
| terminal@gitbash | fast_typing | FAIL | ft0: file has 19/30 lines, exact match False first diff: exp='t91_8ht6u89s70' got='t92_t2ti62i9kq' [first bad line 4: injector call #660 posted its ENTER at 14: |
| terminal@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| terminal@gitbash | pane_needed_ctrl_c_after_paste_instant_enter | PASS | the first marker command after paste_instant_enter had no effect; it worked after Ctrl-C x3 (candidate APP finding: input focus/Enter routing after a long-runni |
| terminal@gitbash | tab_enter | FAIL | Tab+Enter: completed file has 6/12 expected lines |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 11/33 sample lines reached the shell NOT exactly as typed; e.g. characters REORDERED (a |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T172_8qqtac never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal@gitbash | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | FAIL | keys not delivered: vk: marker not in the file; unicode: marker not in the file; foreground=phosphor-oss.exe(pid 18208) app-pid=18208 SendInput-failures=0 |
| agent | mode_proof | PASS | no OCR engine: screenshot only (shots/agent_proof_prekey.png), mode not machine-verified |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that predates the key) on  |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T54_zku2i5 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T61_xt16w7 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, pho |
| agent | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in no shell, not the remote one: `exit` NOT typed; stopping |
| agent | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/agent@gitbash_proof_prekey.png), mode not machine-verified |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | FAIL | ft0: file has 5/30 lines, exact match False first diff: exp='t5_lshv616mts' got='t6_6zc4pz0lx9' [first bad line 3: injector call #1233 posted its ENTER at 14:59 |
| agent@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| agent@gitbash | tab_enter | FAIL | Tab+Enter: completed file has 2/12 expected lines |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 19/33 sample lines reached the shell NOT exactly as typed; e.g. differs (81 typed, 79 r |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T56_y9am72 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent@gitbash | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | FAIL | keys not delivered: vk: marker not in the file; unicode: marker not in the file; foreground=phosphor-oss.exe(pid 14596) app-pid=14596 SendInput-failures=0 |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | latency_enter/enter_to_start | 94.7 / 106.8 | 7.2 / 11.9 (conhost (powershell.exe)) | 87.52 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 79.7 / 104.7 | 72.4 / 110.9 (conhost (bash)) | 7.28 | overhead <= 20 ms | - | pending | PASS | n=19 |

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
