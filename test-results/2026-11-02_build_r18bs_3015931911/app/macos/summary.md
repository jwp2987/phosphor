**Tested a REUSED build**: job 17651299171 (build-macos-arm64) of pipeline 3015898559, commit 85b18aaddbdb4f7a9ea84b6ee9f166533a806a6f; this pipeline 3015931911, commit fdece35d33d9ad5a487c6d74b4d928aaf5cce3db (app-identical).

# App test results (platform=macos, backend=mac, app_display=wayland, seed=1, commit=fdece35d, owner_settings=sha256 a0613b641a27)

pass 8, fail 13, skip 0

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | Phosphor lost keys the reference keeps with the SAME injection (perline): Phosphor 5/12 bad lines (first char lost 0), Terminal.app (.command via open) 0/12 --  |
| terminal | fast_typing | FAIL | ft0: file has 29/30 lines, exact match False first diff: exp='t52_50hnynsgbh' got='t52_50hnynsgbh t53_8sie6xt16w'; ft0: shell received 29 commands, not the type |
| terminal | paste_instant_enter | FAIL | paste + instant Enter: 2/6 md5 lines exact |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 209.78 ms over the harness floor exceeds the 20 ms budget; harness exception: RuntimeError( |
| terminal | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/home/<user>/Logs/p |
| terminal | latency_paint | FAIL | only 0 key->paint samples from kl3 (capture region or Screen Recording permission wrong) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 2/6 md5 lines exact |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 223.25 ms over the harness floor exceeds the 20 ms budget; harness exception: RuntimeError( |
| agent | ssh_connect_typing | FAIL | harness exception: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys'); APP DIED: pid 6912 is gone (kill -0 fails) and no process of  |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 8.48 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Enter to command start: reference measured afte |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/home/<user>/Logs/p |
| agent | latency_paint | PASS |  |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 231.6 / 241.2 | - | 209.78 | <= 20 ms over floor | - | pending | FAIL | n=29 |
| agent | ctrl_c_d/ctrl_c | 233.8 / 243.8 | - | 223.25 | <= 20 ms over floor | - | pending | FAIL | n=25 |
| agent | latency_enter/enter_to_start | 197.6 / 209.4 | 189.1 / 199.4 (Terminal.app (.command via open)) | 8.48 | overhead <= 2 ms | - | pending | FAIL | n=25 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline | 0 | 4 | 1 | 12 |
| terminal | reference | perline | 0 | 0 | 0 | 12 |
