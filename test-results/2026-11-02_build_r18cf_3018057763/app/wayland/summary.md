**Tested a REUSED build**: job 17664724431 (build-linux-x86_64) of pipeline 3017838858, commit 18fddcd8088da7ef506568624cc7936eff3a47e2; this pipeline 3018057763, commit 7b6e80d304d0c34fea97a60ac0bf7c0a537be321 (app-identical).

# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=7b6e80d3, owner_settings=sha256 a0613b641a27)

pass 19, fail 6, skip 2

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector; phosphor / perline-unprimed: first char lost on  |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C: 39.48 ms over the harness floor exceeds the 20 ms budget |
| terminal | ssh_connect_typing | PASS |  |
| terminal | latency_enter | PASS | no reference terminal available (need xterm+DISPLAY / foot / Terminal.app); Enter to command start: no reference terminal measured in this session, overhead ver |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/run/home/<user>/st |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t148_x2qddukigh', 't149_pn66zhny7i', 't150_ahmy48orck', 't151_0r0zr5gil9', 't152_3c5nz8vpge', 't153_12ml6m6y7x', 't154_oxevd3cljs |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C: 39.34 ms over the harness floor exceeds the 20 ms budget |
| agent | ssh_connect_typing | PASS |  |
| agent | latency_enter | PASS | no reference terminal available (need xterm+DISPLAY / foot / Terminal.app); Enter to command start: no reference terminal measured in this session, overhead ver |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/run/home/<user>/st |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
| agent | shell_fish | PASS |  |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t112_iiqof8dlho', 't113_6t1uabtofo', 't114_vr7ybhvwih', 't115_jcwefgtupr', 't116_xbfizxpgvr', 't117_6uhwirzf74', 't118_8ztot9id6h |
| agent | md_viewer | PASS |  |

## Latency (linux-wayland): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 41.5 / 57.6 | - | 39.48 | <= 20 ms over floor | - | pending | FAIL | n=30 |
| terminal | latency_enter/enter_to_start | 33.5 / 42.2 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=30 |
| agent | ctrl_c_d/ctrl_c | 41.4 / 42.1 | - | 39.34 | <= 20 ms over floor | - | pending | FAIL | n=30 |
| agent | latency_enter/enter_to_start | 35.6 / 40.3 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=30 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline-unprimed | 12 | 0 | 0 | 12 |
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | phosphor | oneshot | 0 | 0 | 0 | 12 |
