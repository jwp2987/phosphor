# build/r18cg  53c0a38ca  (pipeline 3018132410)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 46 warnings
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): windows 9/4/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | failed | 24.5 | phosphor-windows-vm |  |
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
| guards | success | 1.7 | - |  |
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

## app windows (app-test-windows-reuse, REUSED build)


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

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): none
- FIXED failures (app): 
  - `windows/terminal/fast_typing`
  - `windows/terminal/paste_instant_enter`
  - `windows/agent/fast_typing`
  - `windows/agent/paste_instant_enter`
- Latency regressions > 5%: none
- compared against: app windows: build/r18bn (3015761186)
