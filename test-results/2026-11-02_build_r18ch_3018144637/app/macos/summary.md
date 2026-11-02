# App test results (platform=macos, backend=mac, app_display=wayland, seed=1, commit=e01e2afa, owner_settings=sha256 a0613b641a27)

pass 14, fail 14, skip 0

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0 |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference Terminal.app (.command via op |
| terminal | fast_typing | FAIL | ft0: file has 29/30 lines, exact match False first diff: exp='t99_wdg90hqrl4' got='t99_wdg90hqrl4 t100_nfyh2s65zh' [first bad line 14: injector call #136 posted |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines; Tab+Enter ran the UNCOMPLETED line, stray files: ['zzcompl_alpha_u'] |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 223.75 ms over the harness floor exceeds the 20 ms budget; Ctrl-D in cat: file ['helloecho  |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T163_c71lmo never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | latency_enter | FAIL | only 18/30 Enter timings captured; Enter to command start: Phosphor overhead 11.34 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Enter to co |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/home/<user>/Logs/p |
| terminal | latency_paint | PASS |  |
| terminal | shell_fish | FAIL | fish: pasted lines + instant Enter: expected ['t176_olnzdp92cv', 't177_u0hbl6flno', 't178_ltgduje2jo', 't179_cswdf2molh'], got ['t176_olnzdp92cv', 't177_u0hbl6f |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t185_z46bxitkti', 't186_k3jikfqpwu', 't187_r4te1j9w2g', 't188_uel48ccmwx', 't189_6xvhlycrnd', 't190_tuzpxdosam', 't191_iox6rjkoet |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 225.40 ms over the harness floor exceeds the 20 ms budget; `cat > cd.txt` never started (it |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T79_1ereql never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent | latency_enter | FAIL | only 22/30 Enter timings captured; Enter to command start: Phosphor overhead 8.73 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Enter to com |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/home/<user>/Logs/p |
| agent | latency_paint | PASS |  |
| agent | shell_fish | FAIL | fish: after exit the pane's bash ran the next command: expected ['t100_q08s58nv'], got []; fish_rec.log empty: the fish_postexec hook from config.fish did not r |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t101_5gawrd82tg', 't102_6rrp0jiqm0', 't103_86j0rr4tr5', 't104_5x4pvll28j', 't105_6u7inu54vh', 't106_iqof8dlhom', 't107_1uabtoforv |
| agent | md_viewer | PASS |  |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 234.8 / 243.7 | - | 223.75 | <= 20 ms over floor | - | pending | FAIL | n=25 |
| terminal | latency_enter/enter_to_start | 201.4 / 218.6 | 190.1 / 195.0 (Terminal.app (.command via open)) | 11.34 | overhead <= 2 ms | - | pending | FAIL | n=18 |
| agent | ctrl_c_d/ctrl_c | 236.4 / 246.2 | - | 225.40 | <= 20 ms over floor | - | pending | FAIL | n=28 |
| agent | latency_enter/enter_to_start | 197.2 / 205.9 | 188.5 / 194.1 (Terminal.app (.command via open)) | 8.73 | overhead <= 2 ms | - | pending | FAIL | n=22 |

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
