# build/r18bs  fdece35d3  (pipeline 3015931911)

- date: 2026-11-02, tier: fix, pipeline status: canceled
- compile (check-linux): -
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): macos 8/13/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | canceled | - | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | failed | 28.9 | MacBookPro-gitrunner |  |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | canceled | - | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 0.9 | - |  |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | canceled | - | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## app macos (app-test-macos-reuse, REUSED build)

reuse: `{"reused_build": {"job_id": 17651299171, "job_name": "build-macos-arm64", "pipeline_id": "3015898559", "commit": "85b18aaddbdb4f7a9ea84b6ee9f166533a806a6f", "this_pipeline": "3015931911", "this_commit": "fdece35d33d9ad5a487c6d74b4d928aaf5cce3db", "fetched": "local cache (/Users/<user>/.cache/phospho`

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | Phosphor lost keys the reference keeps with the SAME injection (perline): Phosphor 5/12 bad lines (first char lost 0), Terminal.app (.comman |
| terminal | fast_typing | FAIL | ft0: file has 29/30 lines, exact match False first diff: exp='t52_50hnynsgbh' got='t52_50hnynsgbh t53_8sie6xt16w'; ft0: shell received 29 co |
| terminal | paste_instant_enter | FAIL | paste + instant Enter: 2/6 md5 lines exact |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 209.78 ms over the harness floor exceeds the 20 ms budget; harness exce |
| terminal | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| terminal | latency_paint | FAIL | only 0 key->paint samples from kl3 (capture region or Screen Recording permission wrong) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 2/6 md5 lines exact |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 223.25 ms over the harness floor exceeds the 20 ms budget; harness exce |
| agent | ssh_connect_typing | FAIL | harness exception: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys'); APP DIED: pid 6912 is gone (kill -0 fails |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 8.48 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Enter to command start: ref |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| agent | latency_paint | PASS |  |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/key_loss_control (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/mode_proof`
  - `macos/terminal/cadence`
  - `macos/terminal/tab_enter`
  - `macos/agent/mode_proof`
  - `macos/agent/cadence`
  - `macos/agent/fast_typing`
  - `macos/agent/tab_enter`
  - `macos/agent/latency_paint`
- Latency regressions > 5%: none
- compared against: app macos: build/r18bn (3015761186)
