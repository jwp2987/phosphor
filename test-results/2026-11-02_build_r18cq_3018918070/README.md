# build/r18cq  c67c37415  (pipeline 3018918070)

- date: 2026-11-02, tier: fix, pipeline status: manual
- compile (check-linux): ok, 49 warnings
- unit: 14463 passed, 6 failed, 55 skipped of 14469 run (3 suites)
- app (pass/fail/skip): -; soak: n/a

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
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 8.7 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.8 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 9.2 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | success | 1.4 | - |  |
| test-workspace-rest | failed | 2.4 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::view::tests::table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover` FAIL (0.1s)

## test-workspace-rest failures

- `markdown_parser::corpus corpus_snapshots_gfm_tables` FAIL (0.2s)
- `markdown_parser::corpus corpus_snapshots_plain` FAIL (0.2s)
- `warp_editor::mermaid_corpus hostile_sources_never_panic` SIGABRT (0.9s)
- `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable` FAIL (0.2s)

## vs previous run

- NEW failures (unit): 
  - `warp terminal::view::tests::table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover [test-linux]`
- FIXED failures (unit): none
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18cn (3018726027); test-warpui-core: build/r18cn (3018726027); test-workspace-rest: build/r18ch (3018144637)
