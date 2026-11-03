# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=332fd97b, owner_settings=sha256 a0613b641a27)

pass 27, fail 6, skip 0, no-reference 2 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 198.9 ms includes t |
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 9.77 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_w |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the same session |
| terminal | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shell received ['find th |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | PASS | nu app log: 2026-11-03T00:10:25Z [DEBUG] frame 1: updated=["WarpifyFooterView", "PaneView", "ActionButton", "Terminal", "ActionButton", "PaneHeader", "UseAgentT |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 199.9 ms includes t |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | PASS |  |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 9.50 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pty_w |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent | latency_paint | FAIL | Key to paint: Phosphor overhead 7.72 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-first-changed-pixel at |
| agent | agent_stream_cancel | FAIL | the screen kept changing 6 s after Ctrl-C (4 distinct frames in 5): the stream did not stop |
| agent | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shell received ['find th |
| agent | shell_fish | PASS |  |
| agent | shell_nu | PASS | nu app log: 2026-11-03T00:17:48Z [DEBUG] frame 1: updated=["ActionButton", "RightPanelView", "Terminal", "ActionButton", "ActionButton", "Workspace", "UseAgentT |
| agent | md_viewer | PASS |  |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 198.9 / 213.8 | 193.4 / 203.7 (Terminal.app (.command via open)) | 5.49 | overhead <= 20 ms | - | pending | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 215.8 / 222.8 | 206.0 / 216.4 (Terminal.app (.command via open)) | 9.77 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| terminal | latency_key_log/key_to_frame | 8.2 / 9.3 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| terminal | latency_paint/key_to_paint | 35.6 / 44.6 | 34.1 / 75.5 (Terminal.app (.command via open)) | 1.53 | overhead <= 2 ms | - | pending | PASS | n=30 |
| agent | ctrl_c_d/ctrl_c | 199.9 / 204.5 | 193.0 / 202.9 (Terminal.app (.command via open)) | 6.91 | overhead <= 20 ms | - | pending | PASS | n=30 |
| agent | latency_enter/enter_to_start | 215.8 / 224.2 | 206.3 / 216.3 (Terminal.app (.command via open)) | 9.50 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| agent | latency_key_log/key_to_frame | 7.3 / 9.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | latency_paint/key_to_paint | 35.9 / 45.4 | 28.2 / 77.5 (Terminal.app (.command via open)) | 7.72 | overhead <= 2 ms | - | pending | FAIL | n=30 |

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
