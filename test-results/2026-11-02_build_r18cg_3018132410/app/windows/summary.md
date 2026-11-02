**Tested a REUSED build**: job 17664724432 (build-windows-x86_64) of pipeline 3017838858, commit 18fddcd8088da7ef506568624cc7936eff3a47e2; this pipeline 3018132410, commit 53c0a38cae9ea415b31b85bd80ec96d2859e07d7 (app-identical).

# App test results (platform=windows)

pass=9 fail=4 skip=0

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | screenshot only (no OCR on the VM): shots\terminal_proof.png |
| terminal | key_delivery | PASS | marker typed and received via sendkeys |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | ctrl_c | FAIL | Ctrl-C delivered 0/5 times |
| terminal | latency_enter | FAIL | only 0/30 Enter timings (verdict: no-data) |
| agent | mode_proof | PASS | screenshot only (no OCR on the VM): shots\agent_proof.png |
| agent | key_delivery | PASS | marker typed and received via sendkeys |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | ctrl_c | FAIL | Ctrl-C delivered 0/5 times |
| agent | latency_enter | FAIL | only 0/30 Enter timings (verdict: no-data) |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | latency_enter/enter_to_start | - | - conhost (powershell.exe) |  | overhead <= 2 ms | - | pending | NO-DATA | n=0 |
| agent | latency_enter/enter_to_start | - | - conhost (powershell.exe) |  | overhead <= 2 ms | - | pending | NO-DATA | n=0 |

