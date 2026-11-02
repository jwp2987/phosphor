# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=167012a7, owner_settings=sha256 a0613b641a27)

pass 18, fail 10, skip 2, no-reference 2 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector; phosphor / perline-unprimed: first char lost on  |
| terminal | enter_loss_control | PASS | enter control: no reference terminal (unavailable): a lost Enter in Phosphor cannot be attributed; no reference terminal available: Enter loss is not controlled |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | NOREF | no reference: absolute only (Ctrl-C): no reference terminal available; NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget |
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T145_ay0gbe never reached the REMOTE file (ssh did not give a re |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| terminal | latency_key_log | FAIL | key->frame p50 30.7 ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here); NO-REFERENCE: key to frame: no reference terminal meas |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| terminal | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes); nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was add |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | NOREF | no reference: absolute only (Ctrl-C): no reference terminal available; NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T109_fayr3r never reached the REMOTE file (ssh did not give a re |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| agent | latency_key_log | FAIL | key->frame p50 28.4 ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here); NO-REFERENCE: key to frame: no reference terminal meas |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| agent | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes); nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was add |
| agent | md_viewer | PASS |  |

## Latency (linux-wayland): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 36.1 / 37.9 | - | - | overhead <= 20 ms | - | pending | NO-REFERENCE | n=30 |
| terminal | latency_key_log/key_to_frame | 30.7 / 45.4 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | ctrl_c_d/ctrl_c | 41.6 / 42.6 | - | - | overhead <= 20 ms | - | pending | NO-REFERENCE | n=30 |
| agent | latency_key_log/key_to_frame | 28.4 / 41.6 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline-unprimed | 12 | 0 | 0 | 12 |
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | phosphor | oneshot | 0 | 0 | 0 | 12 |
