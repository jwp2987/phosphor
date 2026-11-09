# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=2ac9f757, sha=2ac9f757a52596968ac1d0c6569a360037bd562b, owner_settings=sha256 a0613b641a27, tier=smoke, scale=1)

pass 32, fail 2, skip 0, no-reference 4 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; injector  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference Terminal.app (.command via open) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector  |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2057; app saw 2055; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 0, app saw 12); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | ctrl_c_d | FAIL | REGRESSION Ctrl-C under flood: median 289.67 vs baseline build +22.56 ms (+8.4%); Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 197.97 ms includes  |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/40 (2.5 %) rate: needs 119 clean trials; ssh trial verdicts: exact=12; injector accounting: WARN loss case: injector sent 976; app saw 972; lost-before-app 4 (Enter: sent 48, app saw 48); pty writes: 0 |
| terminal | ssh_enter_then_immediate_command | PASS (TAINTED: Enters lost before the app, see detail) | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 263; lost-before-app 0 (Enter: sent 12, app saw 13); app logged 16 more than were injected (keys from an earlier |
| terminal | ssh_nested_typeahead | PASS (TAINTED: Enters lost before the app, see detail) | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 149; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 12 more than were injected (keys from an earlier c |
| terminal | latency_enter | PASS (TAINTED: Enters lost before the app, see detail) | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin -> pty_write median 0.79 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.39 ms (p95 1.04, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; WARN Key to paint: median 50.67 vs baseline build +6.68 ms (+15.2%); re-measured once: median 34.0 (pass); NO-REFERENCE: Key to paint: overhead 4.12 +/- 3.44 ms |
| terminal | routing_table | PASS | injector accounting: injector sent 644; app saw 575; lost-before-app 69 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 746; lost-before-app 9 (Enter: sent 21, app saw 25); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | shell_nu | PASS | nu app log: 2026-11-09T15:21:40Z [DEBUG] frame 1: updated=["Input", "ActionButton", "ActionButton", "Menu", "ActionButton", "ActionButton", "UseAgentToolbar", "Terminal", "Workspace", "PaneView", "PaneHeader", "W; injector accounting: injector sent 901; app saw 892; lost-before-app 9 (Enter: sent 23, app saw 27); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 0, app saw 12); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | ctrl_c_d | FAIL | REGRESSION Ctrl-C under flood: median 287.57 vs baseline build +42.64 ms (+17.4%); Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 199.95 ms includes  |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/40 (2.5 %) rate: needs 119 clean trials; ssh trial verdicts: exact=12; injector accounting: WARN loss case: injector sent 955; app saw 951; lost-before-app 4 (Enter: sent 48, app saw 48); pty writes: 0 |
| agent | ssh_enter_then_immediate_command | PASS (TAINTED: Enters lost before the app, see detail) | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 259; lost-before-app 0 (Enter: sent 12, app saw 13); app logged 12 more than were injected (keys from an earlier |
| agent | ssh_nested_typeahead | PASS (TAINTED: Enters lost before the app, see detail) | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 151; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 14 more than were injected (keys from an earlier c |
| agent | latency_enter | PASS (TAINTED: Enters lost before the app, see detail) | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin -> pty_write median 1.35 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.38 ms (p95 0.76, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; WARN Key to paint: median 51.62 vs baseline build +5.15 ms (+11.1%); re-measured once: median 33.45 (pass); NO-REFERENCE: Key to paint: overhead -0.81 +/- 8.54  |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped (4 distinct frames in 5), typed text still in the input, app route cancel_conversation; Ctrl-C #2: stream stopped (5 distinct frames i |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | routing_table | PASS | injector accounting: injector sent 631; app saw 575; lost-before-app 56 (Enter: sent 27, app saw 27); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 220; app saw 204; lost-before-app 16 (Enter: sent 4, app saw 4); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 746; lost-before-app 9 (Enter: sent 21, app saw 25); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | shell_nu | PASS | nu app log: 2026-11-09T15:45:25Z [DEBUG] frame 1: updated=["ActionButton", "UseAgentToolbar", "Menu", "RightPanelView", "PaneHeader", "Workspace", "WarpifyFooterView", "Input", "Terminal", "PaneView"] removed=0 r; nu app log: 2026-11-09T15:49:16Z [DEBUG] frame 1: updated=["RightPanelView", "Workspace", "Terminal", "UseAgentToolbar", "WarpifyFooterView", "PaneView", "Actio |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 198.0 / 206.3 | 206.7 / 217.5 (Terminal.app (.command via open)) | -8.77 +/- 0.89 | overhead <= 20 ms | - | 211.5 (-13.55 ms, -6.4%) PASS | PASS | n=30 |
| terminal | ctrl_c_d/ctrl_c_flood | 289.7 / 314.2 | 187.0 / 277.7 (Terminal.app (.command via open)) | 102.65 +/- 9.09 | overhead <= 45 ms | - | 267.1 (+22.56 ms, +8.4%) FAIL | FAIL | n=30 |
| terminal | latency_enter/enter_to_start | 98.4 / 118.7 | 77.7 / 83.8 (Terminal.app (.command via open)) | 20.73 +/- 1.88 | overhead <= 2 ms | - | 96.1 (+2.30 ms, +2.4%) WARN | FAIL | n=60 |
| terminal | latency_key_log/key_to_frame | 6.4 / 9.4 | - | - | overhead <= 2 ms | - | 6.2 (+0.16 ms, +2.6%) PASS | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | - | - | - | - | - | - | PASS | n=0 |
| terminal | latency_paint/key_to_paint | 50.7 / 56.3 | 46.5 / 85.0 (Terminal.app (.command via open)) | 4.12 +/- 3.44 | overhead <= 2 ms | - | 44.0 (+6.68 ms, +15.2%) WARN | WARN | n=30 |
| | re-measure | 34.0 / 38.4 | | | | | | | |
| agent | ctrl_c_d/ctrl_c | 199.9 / 207.6 | 211.0 / 244.1 (Terminal.app (.command via open)) | -11.01 +/- 1.52 | overhead <= 20 ms | - | 212.3 (-12.31 ms, -5.8%) PASS | PASS | n=30 |
| agent | ctrl_c_d/ctrl_c_flood | 287.6 / 328.2 | 188.7 / 256.8 (Terminal.app (.command via open)) | 98.92 +/- 4.94 | overhead <= 45 ms | - | 244.9 (+42.64 ms, +17.4%) FAIL | FAIL | n=30 |
| agent | latency_enter/enter_to_start | 102.5 / 124.3 | 77.0 / 81.8 (Terminal.app (.command via open)) | 25.43 +/- 2.15 | overhead <= 2 ms | - | 93.3 (+9.14 ms, +9.8%) WARN | FAIL | n=60 |
| | re-measure | 78.5 / 84.5 | | | | | | | |
| agent | latency_key_log/key_to_frame | 6.0 / 9.3 | - | - | overhead <= 2 ms | - | 6.7 (-0.70 ms, -10.4%) PASS | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | - | - | - | - | - | - | PASS | n=0 |
| agent | latency_paint/key_to_paint | 51.6 / 58.9 | 52.4 / 92.6 (Terminal.app (.command via open)) | -0.81 +/- 8.54 | overhead <= 2 ms | - | 46.5 (+5.15 ms, +11.1%) WARN | WARN | n=30 |
| | re-measure | 33.5 / 37.3 | | | | | | | |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | reference | perline | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | Terminal.app (.command via open) | 0.0 | 0 | 30 |
| terminal | Terminal.app (.command via open) | 0.25 | 0 | 30 |

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
