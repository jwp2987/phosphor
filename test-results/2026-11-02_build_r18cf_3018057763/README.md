# build/r18cf  7b6e80d30  (pipeline 3018057763)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 46 warnings
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): macos 9/16/0; wayland 19/6/2; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | failed | 14.3 | - |  |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | canceled | 26.5 | MacBookPro-gitrunner |  |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | failed | 1.5 | phosphor-windows-vm | no results artifact (job failed) |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 6.4 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.1 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## app macos (app-test-macos-reuse, REUSED build)

reuse: `{"reused_build": {"job_id": 17664724430, "job_name": "build-macos-arm64", "pipeline_id": "3017838858", "commit": "18fddcd8088da7ef506568624cc7936eff3a47e2", "this_pipeline": "3018057763", "this_commit": "7b6e80d304d0c34fea97a60ac0bf7c0a537be321", "fetched": "local cache (/Users/<user>/.cache/phospho`

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | Phosphor lost keys the reference keeps with the SAME injection (perline): Phosphor 11/12 bad lines (first char lost 0), Terminal.app (.comma |
| terminal | fast_typing | FAIL | ft25: file has 28/30 lines, exact match False first diff: exp='t59_k36cr6g1ew' got='t59_k36cr6g1ew t60_2bk6kfzrtn'; ft25: shell received 28  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines; Tab+Enter ran the UNCOMPLETED line, stray files: ['zzcompl_alpha_u', 'zzcompl_alpha_uecho |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 219.29 ms over the harness floor exceeds the 20 ms budget; nested bash  |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T103_6s0r1v never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | latency_enter | FAIL | only 15/30 Enter timings captured; Enter to command start: Phosphor overhead 12.31 ms over Terminal.app (.command via open) exceeds the 2 ms |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| terminal | latency_paint | PASS |  |
| terminal | shell_fish | FAIL | fish: 12 typed lines: expected ['t104_wrd82tgo6r', 't105_p0jiqm09d8', 't106_0rr4tr5n5x', 't107_pvll28jd6u', 't108_nu54vhiiqo', 't109_8dlhom6 |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t125_wqd10ywsv2', 't126_7jdvh6l85v', 't127_b4nylzogpv', 't128_p34x5m12z8', 't129_5rijay0gbe', 't130_3y6sjj7gq |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | FAIL | ft0: file has 28/30 lines, exact match False first diff: exp='t20_gn1m5gys65' got='t20_gn1m5gys65 t21_uzsbkmuiv1'; ft0: shell received 28 co |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines; Tab+Enter ran the UNCOMPLETED line, stray files: ['zzcompl_alpha_u'] |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 223.64 ms over the harness floor exceeds the 20 ms budget; `cat > cd.tx |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T79_1ereql never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | latency_enter | FAIL | only 22/30 Enter timings captured; Enter to command start: Phosphor overhead 10.00 ms over Terminal.app (.command via open) exceeds the 2 ms |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| agent | latency_paint | PASS |  |
| agent | shell_fish | FAIL | fish: 12 typed lines: expected ['t80_u76a7hjuuu', 't81_2r43xyfdid', 't82_qpvxxzt3v8', 't83_bjqoihl0dg', 't84_gnqe7fenl6', 't85_bx5som5p12',  |

## app wayland (app-test-linux-wayland-reuse, REUSED build)

reuse: `{"reused_build": {"job_id": 17664724431, "job_name": "build-linux-x86_64", "pipeline_id": "3017838858", "commit": "18fddcd8088da7ef506568624cc7936eff3a47e2", "this_pipeline": "3018057763", "this_commit": "7b6e80d304d0c34fea97a60ac0bf7c0a537be321", "fetched": "local cache (/home/<user>/.cache/phospho`

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector; phosphor / perline-unprimed: |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C: 39.48 ms over the harness floor exceeds the 20 ms budget |
| terminal | ssh_connect_typing | PASS |  |
| terminal | latency_enter | PASS | no reference terminal available (need xterm+DISPLAY / foot / Terminal.app); Enter to command start: no reference terminal measured in this s |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/run/h |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t148_x2qddukigh', 't149_pn66zhny7i', 't150_ahmy48orck', 't151_0r0zr5gil9', 't152_3c5nz8vpge', 't153_12ml6m6y7 |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C: 39.34 ms over the harness floor exceeds the 20 ms budget |
| agent | ssh_connect_typing | PASS |  |
| agent | latency_enter | PASS | no reference terminal available (need xterm+DISPLAY / foot / Terminal.app); Enter to command start: no reference terminal measured in this s |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/run/h |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only |
| agent | shell_fish | PASS |  |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t112_iiqof8dlho', 't113_6t1uabtofo', 't114_vr7ybhvwih', 't115_jcwefgtupr', 't116_xbfizxpgvr', 't117_6uhwirzf7 |
| agent | md_viewer | PASS |  |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/key_loss_control`
  - `macos/terminal/tab_enter`
  - `macos/terminal/shell_nu (case not in previous run)`
  - `macos/agent/fast_typing (case not in previous run)`
  - `macos/agent/tab_enter (case not in previous run)`
  - `macos/agent/ctrl_c_d (case not in previous run)`
  - `macos/agent/ssh_connect_typing (case not in previous run)`
  - `macos/agent/latency_enter (case not in previous run)`
  - `macos/agent/latency_key_log (case not in previous run)`
  - `macos/agent/shell_fish (case not in previous run)`
  - `wayland/terminal/latency_key_log (case not in previous run)`
  - `wayland/terminal/shell_nu (case not in previous run)`
  - `wayland/agent/ctrl_c_d (case not in previous run)`
  - `wayland/agent/latency_key_log (case not in previous run)`
  - `wayland/agent/shell_nu (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/latency_paint`
- Latency regressions > 5%: none
- compared against: app macos: build/r18ce (3017838858); app wayland: build/r18ce (3017838858)
