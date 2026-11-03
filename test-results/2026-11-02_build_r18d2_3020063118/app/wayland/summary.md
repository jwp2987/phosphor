# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=332fd97b, owner_settings=sha256 a0613b641a27)

pass 24, fail 8, skip 2, no-reference 6 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | Phosphor lost keys the reference keeps with the SAME injection (perline-unprimed): Phosphor 12/12 bad lines (first char lost 12), foot 0/12 -- a Phosphor-side d |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 30/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted None Enters; reference foot gap 0.25s: 30/30 li |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); Ctrl-C: same tr |
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 0.40 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; NO-REFERENCE:  |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to shell (no routing log line, so no  |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-03T00:20:16Z [DEBUG] fr |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | left_agent_mode_after_fast_typing | FAIL | after fast_typing the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Candid |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 digest lines exact (first got [], expected ['5adab3d97a990aa6e1bec241daf9082a']) |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); Ctrl-C: same tr |
| agent | ssh_connect_typing | PASS |  |
| agent | left_agent_mode_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | PASS |  |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.64 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; NO-REFERENCE:  |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | left_agent_mode_after_latency_key_log | FAIL | after latency_key_log the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Ca |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | agent_stream_cancel | FAIL | text typed at 10-25 ms/key while the reply streamed is not visible in the input ('keepmet115_qm09d8') |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no routing log line, so no  |
| agent | left_agent_mode_after_routing_table | FAIL | after routing_table the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Cand |
| agent | shell_fish | PASS |  |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-03T00:33:34Z [DEBUG] fr |
| agent | left_agent_mode_after_shell_nu | FAIL | after shell_nu the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure agent mode. Candidate |
| agent | md_viewer | PASS |  |

## Latency (linux-wayland): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 35.3 / 40.9 | - | - | overhead <= 20 ms | - | pending | NO-REFERENCE | n=30 |
| terminal | latency_enter/enter_to_start | 40.4 / 45.1 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=30 |
| terminal | latency_key_log/key_to_frame | 5.3 / 12.3 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | ctrl_c_d/ctrl_c | 42.1 / 57.0 | - | - | overhead <= 20 ms | - | pending | NO-REFERENCE | n=30 |
| agent | latency_enter/enter_to_start | 44.7 / 45.5 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=30 |
| agent | latency_key_log/key_to_frame | 3.7 / 9.7 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline-unprimed | 12 | 0 | 0 | 12 |
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | phosphor | oneshot | 0 | 0 | 0 | 12 |
| terminal | reference | perline-unprimed | 0 | 0 | 0 | 12 |
| terminal | reference | perline | 0 | 0 | 0 | 12 |
| terminal | reference | oneshot | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | foot | 0.0 | 30 | 30 |
| terminal | foot | 0.25 | 30 | 30 |
