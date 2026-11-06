# App test results (platform=linux-xwayland, backend=wayland, app_display=x11, seed=1, commit=local, owner_settings=sha256 a0613b641a27, tier=full, scale=1)

pass 10, fail 0, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| terminal | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 48.35 ms includes the input-tool spawn (harness floor 2.1435990929603577 ms); measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Ctrl-C under flood, app side (#1606): key_to_write p50 7.35 ma |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin -> pty_write median 0.82 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 48.33 ms includes the input-tool spawn (harness floor 1.9913893193006516 ms); measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Ctrl-C under flood, app side (#1606): key_to_write p50 7.16 ma |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin -> pty_write median 1.62 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |

## Latency (linux-xwayland): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 48.4 / 71.5 | 42.5 / 43.5 (foot) | 5.80 +/- 0.73 | overhead <= 20 ms | - | missing | PASS | n=30 |
| terminal | ctrl_c_d/ctrl_c_flood | 41.6 / 47.7 | 34.8 / 60.0 (foot) | 6.81 +/- 0.81 | overhead <= 45 ms | - | missing | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 44.4 / 46.2 | 40.8 / 42.0 (foot) | 3.65 +/- 0.29 | overhead <= 2 ms | - | missing | FAIL | n=60 |
| agent | ctrl_c_d/ctrl_c | 48.3 / 71.7 | 42.5 / 42.8 (foot) | 5.78 +/- 2.04 | overhead <= 20 ms | - | missing | PASS | n=30 |
| agent | ctrl_c_d/ctrl_c_flood | 41.9 / 87.4 | 34.5 / 56.6 (foot) | 7.32 +/- 1.08 | overhead <= 45 ms | - | missing | PASS | n=30 |
| agent | latency_enter/enter_to_start | 45.0 / 46.4 | 40.9 / 42.4 (foot) | 4.08 +/- 0.19 | overhead <= 2 ms | - | missing | FAIL | n=60 |

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
