# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=7f0be20b, owner_settings=sha256 a0613b641a27)

pass 25, fail 5, skip 0, no-reference 2 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 198.5 ms includes t |
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 6.02 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_w |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the same session |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t220_dfgg8yi2zl', 't221_27cm25yswl', 't222_lb9de9o2u2', 't223_gyd3r03v6g', 't224_z8146jujwi', 't225_on3jgg1d3j', 't226_9urzay52tt |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 200.36 ms includes  |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T111_jblqxi never reached the REMOTE file (ssh did not give a re |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 10.00 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_ |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the same session |
| agent | shell_fish | PASS |  |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t135_ywsv2p7jdv', 't136_6l85vhb4ny', 't137_zogpvvp34x', 't138_m12z8h5rij', 't139_y0gbel3y6s', 't140_j7gqb3zo8z', 't141_8p1klvpe89 |
| agent | md_viewer | PASS |  |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 198.5 / 207.2 | 193.5 / 212.4 (Terminal.app (.command via open)) | 5.01 | overhead <= 20 ms | - | pending | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 211.5 / 223.7 | 205.5 / 218.0 (Terminal.app (.command via open)) | 6.02 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| terminal | latency_key_log/key_to_frame | 7.1 / 9.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| terminal | latency_paint/key_to_paint | 36.3 / 44.5 | 35.2 / 75.2 (Terminal.app (.command via open)) | 1.09 | overhead <= 2 ms | - | pending | PASS | n=30 |
| agent | ctrl_c_d/ctrl_c | 200.4 / 214.5 | 195.3 / 210.4 (Terminal.app (.command via open)) | 5.10 | overhead <= 20 ms | - | pending | PASS | n=30 |
| agent | latency_enter/enter_to_start | 216.0 / 226.5 | 206.0 / 219.3 (Terminal.app (.command via open)) | 10.00 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| agent | latency_key_log/key_to_frame | 6.9 / 9.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | latency_paint/key_to_paint | 35.5 / 43.4 | 33.9 / 75.6 (Terminal.app (.command via open)) | 1.60 | overhead <= 2 ms | - | pending | PASS | n=30 |

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
