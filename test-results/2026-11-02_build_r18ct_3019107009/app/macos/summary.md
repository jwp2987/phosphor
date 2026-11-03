# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=167012a7, owner_settings=sha256 a0613b641a27)

pass 18, fail 12, skip 0, no-reference 2 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 198.12 ms includes  |
| terminal | ssh_connect_typing | FAIL | harness exception: ValueError("could not convert string to float: ''") |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T193_2cvu0h never reached the REMOTE file (ssh did not give a re |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | FAIL | Key to paint: Phosphor overhead 5.02 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-first-changed-pixel at |
| terminal | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| terminal | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes) |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 199.08 ms includes  |
| agent | ssh_connect_typing | FAIL | harness exception: ValueError("could not convert string to float: ''") |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T109_fayr3r never reached the REMOTE file (ssh did not give a re |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | latency_paint | FAIL | Key to paint: Phosphor overhead 3.99 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-first-changed-pixel at |
| agent | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| agent | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes) |
| agent | md_viewer | PASS |  |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 198.1 / 212.3 | 194.3 / 202.6 (Terminal.app (.command via open)) | 3.78 | overhead <= 20 ms | - | pending | PASS | n=30 |
| terminal | latency_key_log/key_to_frame | 13.8 / 22.5 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| terminal | latency_paint/key_to_paint | 36.3 / 45.0 | 31.3 / 76.0 (Terminal.app (.command via open)) | 5.02 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| agent | ctrl_c_d/ctrl_c | 199.1 / 208.0 | 195.2 / 205.4 (Terminal.app (.command via open)) | 3.83 | overhead <= 20 ms | - | pending | PASS | n=30 |
| agent | latency_key_log/key_to_frame | 15.8 / 23.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | latency_paint/key_to_paint | 37.5 / 45.4 | 33.5 / 75.9 (Terminal.app (.command via open)) | 3.99 | overhead <= 2 ms | - | pending | FAIL | n=30 |

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
