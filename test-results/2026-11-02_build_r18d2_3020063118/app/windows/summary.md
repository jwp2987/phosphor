# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=332fd97b, owner_settings=sha256 a0613b641a27)

pass 32, fail 19, skip 12, no-reference 1 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conh |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that predates the key) on  |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.1 s): T153_8vpgec never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T163_v68aku never reached the REMOTE file (ssh did not give a re |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 86.47 ms over conhost (powershell.exe) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_write me |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS |  |
| terminal@gitbash | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) ga |
| terminal@gitbash | fast_typing | PASS |  |
| terminal@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| terminal@gitbash | tab_enter | PASS |  |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C: Phosphor overhead 20.10 ms over conhost (bash) exceeds the 20 ms budget; Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 4/33 s |
| terminal@gitbash | ssh_connect_typing | PASS |  |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T194_duje2j never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, ph |
| terminal@gitbash | ssh_nested_typeahead | PASS |  |
| terminal@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 16.97 ms over conhost (bash) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_write median 0.88  |
| terminal@gitbash | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shell received ['find th |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/b |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (look |
| terminal@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that predates the key) on  |
| agent | left_agent_mode_after_ctrl_c_d | FAIL | after ctrl_c_d the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Candidate |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T85_39yn2q never reached the REMOTE file (ssh did not give a rem |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | FAIL | ft0: shell received 31 commands, not the typed ones (first mismatch: "echo t1_ieqh524yng >> 'C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run |
| agent@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| agent@gitbash | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 14/33 sample lines reached the shell NOT exactly as typed; e.g. differs (81 typed, 80 r |
| agent@gitbash | left_agent_mode_after_ctrl_c_d | FAIL | after ctrl_c_d the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Candidate |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_connect_typing | PASS |  |
| agent@gitbash | ssh_enter_then_immediate_command | PASS |  |
| agent@gitbash | ssh_nested_typeahead | PASS |  |
| agent@gitbash | agent_stream_cancel | FAIL | no model request ('[byop] opening stream') within 60 s of the prompt's Enter; prompt not routed to the agent or provider unreachable |
| agent@gitbash | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shell received ['find th |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | latency_enter/enter_to_start | 93.4 / 107.8 | 6.9 / 10.1 (conhost (powershell.exe)) | 86.47 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 87.0 / 108.1 | 66.9 / 98.0 (conhost (bash)) | 20.10 | overhead <= 20 ms | - | pending | FAIL | n=26 |
| terminal@gitbash | latency_enter/enter_to_start | 86.6 / 213.2 | 69.7 / 77.9 (conhost (bash)) | 16.97 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.6 / 10.3 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
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
