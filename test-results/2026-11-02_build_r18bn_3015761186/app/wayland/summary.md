# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=2b245a37, owner_settings=sha256 a0613b641a27)

pass 2, fail 16, skip 2

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| terminal | fast_typing | FAIL | ft0: file has 0/30 lines, exact match False first diff: exp='t1_ieqh524yng' got=None; ft0: shell received 30 commands, not the typed ones (first mismatch: 'bcde |
| terminal | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| terminal | tab_enter | FAIL | Tab+Enter: completed file has 0/12 expected lines |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-D in cat: file []; Ctrl-D leaves the nested shell: SHLVL lines [] |
| terminal | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); input_order logging not on or the log path is wrong (/home/<user>/builds/r_hQtIdMf/0/winters-phosp |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
| agent | mode_proof | PASS |  |
| agent | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| agent | fast_typing | FAIL | ft0: file has 0/30 lines, exact match False first diff: exp='t1_ieqh524yng' got=None; ft0: shell received 12 commands, not the typed ones (first mismatch: 'bcde |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 lines exact |
| agent | tab_enter | FAIL | Tab+Enter: completed file has 0/12 expected lines |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-D in cat: file []; Ctrl-D leaves the nested shell: SHLVL lines [] |
| agent | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); input_order logging not on or the log path is wrong (/home/<user>/builds/r_hQtIdMf/0/winters-phosp |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
