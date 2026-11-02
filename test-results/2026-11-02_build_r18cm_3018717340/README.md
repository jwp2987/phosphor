# build/r18cm  ac70362eb  (pipeline 3018717340)

- date: 2026-11-02, tier: fix, pipeline status: waiting_for_resource
- compile (check-linux): -
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): windows 2/0/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | waiting_for_resource | - | - | not run |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | failed | 1.1 | phosphor-windows-vm |  |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | waiting_for_resource | - | - | not run |
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
| licenses | waiting_for_resource | - | - | not run |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## app windows (app-test-windows-reuse, REUSED build)

reuse: `{"reused_build": {"commit": "18fddcd8088da7ef506568624cc7936eff3a47e2", "fetched": "local cache (C:\\Users\\Winters\\.cache\\phosphor-ci\\builds\\windows\\17664724432)", "this_pipeline": "3018717340", "job_name": "build-windows-x86_64", "this_commit": "ac70362eb89bf37d171f00d50babecac5f5c1dc8", "job`

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal_proof_prekey.png), mode not machine-verified |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: app windows: build/r18cj (3018419792)
