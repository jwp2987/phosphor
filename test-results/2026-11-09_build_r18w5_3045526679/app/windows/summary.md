# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=2ac9f757, sha=2ac9f757a52596968ac1d0c6569a360037bd562b, owner_settings=sha256 a0613b641a27, tier=smoke, scale=1)

pass 48, fail 1, skip 13, no-reference 4 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; the injector/O |
| terminal | fast_typing | PASS | pty writes: 223, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | paste_instant_enter | PASS | pty writes: 14, of which carrying a CR (\r): 2; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.46, -> LineEditorReading 56.97; app: line editor ready 56.2, active 56.2, next command written 3127.7, its Preexec 3140.3; #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; Ctrl-C: same trap/t |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_enter_then_immediate_command trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.65 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pty_write -> first pty_read median 4.40 ms (n=63); PowerShell side of Enter->start (#1401, separate ungated batch of 20 with the marks on, medians m |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot shots/terminal_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; pty write |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters |
| terminal@gitbash | fast_typing | PASS | pty writes: 190, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | paste_instant_enter | PASS |  |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; WARN Ctrl-C: median 82.04 vs baseline build +1.80 ms (+2.2%); NO-REFERENCE: Ctrl-C: overhead 12.91 +/- 5.24 ms (SE) vs budget 20 ms is within noise: INCONCLUSIV |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2 |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 44.2 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Enter->start: enter_begin -> pty_write median 0.87 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.61 ms (p95 0.75, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot shots/terminal@gitbash_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | pty writes: 171, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | paste_instant_enter | PASS | pty writes: 15, of which carrying a CR (\r): 3; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 9.07, -> LineEditorReading 54.33; app: line editor ready 58.1, active 58.1, next command written 3154.6, its Preexec 3173.6; #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; fix tier, agent mod |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.1 s): T58_j8mrtq never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosphor.log). Test host: sshd.log: 07:42:29 auth publickey user=Winters key=ssh-ed25519 4b7880db9e519b5a90fce69fa10073ca -> ok / 07:42:29 auth publickey user=Winters key=ssh-ed25519 4b7880db9e519b5a90fce69fa10073ca -> ok / 07:42:29 session shell None pty / 07:42:29 conpty shell pid 7388 / 07:42:30 conpty first output after 0.66 s: 68 bytes / 07:42:32 conpty output ended: 202 bytes sent to the client in total / 07:42:34 client input ended: 104 bytes in 93 chunks, 104 written to the pseudo console / 07:42:34 connection closed: authenticated=False channels=1; fakesshd.stderr: Traceback (most recent call last): /   File "C:\Users\<user>\AppData\Local\phosphor-ci\python312\Lib\site-packages\paramiko\transport.py", line 2043, in run /     self._check_banner() /   File "C:\Users\<user>\AppData\Local\phosphor-ci\python312\Lib\site-packages\paramiko\transport.py", line 2231, in _check_banner /     raise SSHException( / paramiko.ssh_exception.SSHException: Error reading SSH protocol banner[WinError 10054] An existing connection was forcibly closed by the remote host / Socket exception: An existing connection was forcibly closed by the remote host (10054) / Socket exception: An existing connection was forcibly closed by the remote host (10054); test sshd (fakesshd-win) listening on 127.0.0.1:64082; app ssh hops: [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=handoff_commit_source ssh_gate_armed=false ssh_holding=false remote_prompt_seen=false t=1794238954822.269 / [DEBUG] input_order hop=handoff_commit_at_prompt committed=true still_pty_owned=false t=1794238954822.331 / [DEBUG] input_order hop=handoff_commit_source ssh_gate_armed=false ssh_holding=false remote_prompt_seen=false t=1794238955146.062 / [DEBUG] input_order hop=handoff_commit_at_prompt committed=true still_pty_owned=false t=1794238955146.111; remote shell received 3/4 typed-ahead commands exactly (missing 1); remote rec.log lacks the exact typed command for some trials; ssh_connect_typing gap 0.0 s t |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2 |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | pty writes: 144, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only; the overhead budget is judged in terminal mode; Ctrl-D half not run on Windows: |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while-streaming landing is screenshot-only (shots/agent_typed_while_streaming.png); Ctrl-C #1: stream stopped (2 distinct frames in 5), typ |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 4.3 / 5.9 | 2.8 / 3.8 (conhost (powershell.exe)) | 1.59 +/- 0.22 | overhead <= 20 ms | - | 4.6 (-0.26 ms, -5.6%) PASS | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 48.2 / 58.6 | 7.1 / 10.4 (conhost (powershell.exe)) | 41.06 +/- 0.77 | overhead <= 2 ms | - | 49.6 (-1.45 ms, -2.9%) PASS | FAIL | n=60 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 82.0 / 104.6 | 69.1 / 97.9 (conhost (bash)) | 12.91 +/- 5.24 | overhead <= 20 ms | - | 80.2 (+1.80 ms, +2.2%) WARN | WARN | n=30 |
| terminal@gitbash | latency_enter/enter_to_start | 96.0 / 204.7 | 65.6 / 80.4 (conhost (bash)) | 30.43 +/- 2.02 | overhead <= 2 ms | - | 95.0 (+1.02 ms, +1.1%) PASS | FAIL | n=60 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.5 / 4.4 | - | - | overhead <= 2 ms | - | 3.4 (+0.05 ms, +1.5%) PASS | NO-REFERENCE | n=60 |
| terminal@gitbash | latency_key_log/ui_work | - | - | - | - | - | - | PASS | n=0 |

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

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
