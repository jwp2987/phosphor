# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=7f0be20b, owner_settings=sha256 a0613b641a27)

pass 20, fail 5, skip 2, no-reference 6 (not passes)

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
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.2 s): T131_kjy2zh never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 0.34 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; no reference t |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t166_qxcepqz9sf', 't167_kr0fis9qpn', 't168_r4d6tn8e9u', 't169_s7ic2xcbu0', 't170_9c71lmohi6', 't171_r3mdx3vwoa', 't172_5ckq9caof7 |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | NOREF | no reference: absolute only (Ctrl-C): no reference terminal available; NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.2 s): T93_r43xyf never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | PASS |  |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 0.78 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; no reference t |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | left_agent_mode_after_latency_key_log | FAIL | after latency_key_log the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Ca |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | shell_fish | PASS |  |
| agent | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes); nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was add |
| agent | md_viewer | PASS |  |

## Latency (linux-wayland): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 35.5 / 51.9 | - | - | overhead <= 20 ms | - | pending | NO-REFERENCE | n=30 |
| terminal | latency_enter/enter_to_start | 33.6 / 35.0 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=30 |
| terminal | latency_key_log/key_to_frame | 1.0 / 12.7 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | ctrl_c_d/ctrl_c | 41.5 / 56.8 | - | - | overhead <= 20 ms | - | pending | NO-REFERENCE | n=30 |
| agent | latency_enter/enter_to_start | 43.8 / 45.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=30 |
| agent | latency_key_log/key_to_frame | 4.5 / 11.8 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline-unprimed | 12 | 0 | 0 | 12 |
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | phosphor | oneshot | 0 | 0 | 0 | 12 |
