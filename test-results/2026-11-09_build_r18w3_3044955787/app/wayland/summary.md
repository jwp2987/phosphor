# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=b964572f, sha=b964572fc8e79ff7abc56044e7b6ca0edce73de9, owner_settings=sha256 a0613b641a27, tier=smoke, scale=1)

pass 31, fail 3, skip 2, no-reference 2 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline-unprimed: first char lost on 0/12 lines, other bad 0, missing 0; phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; p |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector accounting: WARN loss case:  |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2072; app saw 2070; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 37.76 ms includes the input-tool spawn (harness floor 3.5362206399440765 ms); measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Ctrl-C under flood, app side (#1606): key_to_write p50 12.78 m |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 19 key(s)/19 byte(s), hand-over precmd@19, finish delivered=remote; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.0 s trial 2: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 18 key(s)/18 byte(s), hand-over |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 22 key(s)/22 byte(s), hand-over precmd@22, finish delivered=remote; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2; injector accounting: injector sent 247; app saw 273; lost-before-app 0 (Enter: sent 12, app saw 13); app logged 26 more th |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T243_27i1iv never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosph |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin -> pty_write median 0.34 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 6.04 ms (p95 6.66, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no routing log line, so no rule to check); ambiguous 'cat the error log and tell me what is wrong': went to agent (no rout |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | shell_fish | PASS | injector accounting: injector sent 751; app saw 746; lost-before-app 5 (Enter: sent 25, app saw 25); 4 paste chord(s) not counted as keys; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; injector accounting: injector sent 948; app saw 943; lost-before-app 5 (Enter: sent 28, app saw 28); 4 paste chord(s) not counted as keys; pty writes: 0, of whi |
| terminal | md_viewer | PASS | injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2002; app saw 2001; lost-before-app 1 (Enter: sent 60, app saw 60); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 48.75 ms includes the input-tool spawn (harness floor 1.9998354837298393 ms); measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Ctrl-C under flood, app side (#1606): key_to_write p50 7.74 ma |
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 3: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 10 key(s)/10 byte(s), hand-over precmd@10, finish delivered=remote; the app saw every key of the line; remote got None; ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/40 (2.5 %) rate: need |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: the line never reached the remote shell (discards=0 table_drops=0 give_ups=0); held 17 key(s)/17 byte(s), hand-over precmd@17, finish delivered=remote; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2; injector accounting: injector sent 247; app saw 272; lost-before-app 0 (Enter: sent 12, app saw 13); app logged 25 more th |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T111_jblqxi never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosph |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin -> pty_write median 1.71 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 15.81 ms (p95 16.10, n=60): OVER the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting: injector sent 1; app saw 0; lost-before-app 1 (Enter: sent 0, app saw 0) |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped (2 distinct frames in 5), typed text still in the input, app route cancel_conversation; Ctrl-C #2: stream stopped (2 distinct frames i |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no routing log line, so no rule to check); ambiguous 'cat the error log and tell me what is wrong': went to agent (no rout |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | shell_fish | PASS | injector accounting: injector sent 751; app saw 746; lost-before-app 5 (Enter: sent 25, app saw 25); 4 paste chord(s) not counted as keys; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-09T13:34:48Z [DEBUG] frame 1: updated=["ActionButton", "AIBlock", "ActionButton", "UseAgentToolbar", "AIBlock", "ActionButton", "AIBlock", " |
| agent | md_viewer | FAIL | markdown viewer: heading/table text ['phosphormdprobe', 'zebraapple', 'quinceberry'] not visible within 20 s (OCR saw 'orqsearchsessions,agents,files.qsearchtabszx=+o©0aad©xprobedfenderedraw14,pleaseexplainwhythe120.wlcancated~wnd964572108pleaseexplainwhythet120_hywingtestrunifailing,©provedwsoddasphormdprobe‘atphosphoragentvpast=pleaseexplainwhythet120_hyaw“mineintroparagraphwithinlinecodeandalink.=pleaseexplainwhy'); injector accounting: injector sent 1; app saw 0; lost-before-app 1 (Enter: sent 0, app saw 0) |

## Latency (linux-wayland): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 37.8 / 48.9 | 30.2 / 31.3 (foot) | 7.52 +/- 0.27 | overhead <= 20 ms | - | pending | PASS | n=30 |
| terminal | ctrl_c_d/ctrl_c_flood | 48.4 / 58.1 | 35.7 / 38.6 (foot) | 12.66 +/- 1.21 | overhead <= 45 ms | - | pending | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 32.8 / 34.5 | 30.1 / 31.1 (foot) | 2.65 +/- 0.12 | overhead <= 2 ms | - | pending | FAIL | n=60 |
| terminal | latency_key_log/key_to_frame | 6.9 / 7.5 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | - | - | - | - | - | - | WARN | n=1 |
| agent | ctrl_c_d/ctrl_c | 48.8 / 73.2 | 42.5 / 43.2 (foot) | 6.28 +/- 4.43 | overhead <= 20 ms | - | pending | PASS | n=30 |
| agent | ctrl_c_d/ctrl_c_flood | 41.9 / 94.8 | 35.2 / 62.2 (foot) | 6.72 +/- 1.40 | overhead <= 45 ms | - | pending | PASS | n=30 |
| agent | latency_enter/enter_to_start | 45.0 / 46.5 | 41.5 / 42.6 (foot) | 3.58 +/- 0.44 | overhead <= 2 ms | - | pending | FAIL | n=60 |
| agent | latency_key_log/key_to_frame | 17.8 / 18.1 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | - | - | - | - | - | - | WARN | n=1 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline-unprimed | 0 | 0 | 0 | 12 |
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | phosphor | oneshot | 0 | 0 | 0 | 12 |
| terminal | reference | perline-unprimed | 0 | 0 | 0 | 12 |
| terminal | reference | perline | 0 | 0 | 0 | 12 |
| terminal | reference | oneshot | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | foot | 0.0 | 0 | 30 |
| terminal | foot | 0.25 | 0 | 30 |

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
