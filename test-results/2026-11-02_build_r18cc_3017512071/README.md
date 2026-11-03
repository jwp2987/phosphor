# build/r18cc  77a7760c3  (pipeline 3017512071)

- date: 2026-11-02, tier: fix, pipeline status: failed
- compile (check-linux): ok, 58 warnings
- unit: 14343 passed, 11 failed, 55 skipped of 14354 run (3 suites)
- app (pass/fail/skip): -; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | failed | 0.2 | - |  |
| app-test-linux-wayland | manual | - | - | not run |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | manual | - | - | not run |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | manual | - | - | not run |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | failed | 6.5 | - |  |
| build-macos-arm64 | failed | 6.0 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | failed | 14.4 | phosphor-windows-vm |  |
| check-linux | success | 7.5 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.6 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 10.4 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | success | 1.4 | - |  |
| test-workspace-rest | failed | 3.6 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::bootstrap::tests::test_nu_real_local_bootstrap_emits_hooks_and_keeps_history_clean` FAIL (12.3s)
- `warp terminal::input::tests::test_voice_input_toggle_preserves_lock_state` TIMEOUT (60.0s)
- `warp terminal::view::tests::an_inline_editor_paste_then_an_immediate_enter_are_queued_in_order` FAIL (0.1s)
- `warp terminal::view::tests::ssh_handover_enter_guard_does_not_swallow_a_new_line_after_clear` FAIL (0.1s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` FAIL (11.1s)

## test-workspace-rest failures

- `markdown_parser::corpus corpus_snapshots_gfm_tables` FAIL (0.2s)
- `markdown_parser::corpus corpus_snapshots_plain` FAIL (0.1s)
- `warp_editor::mermaid_corpus hostile_sources_never_panic` TIMEOUT (60.0s)
- `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagrams_render_with_their_labels` FAIL (0.0s)

## vs previous run

- NEW failures (unit): 
  - `warp terminal::bootstrap::tests::test_nu_real_local_bootstrap_emits_hooks_and_keeps_history_clean [test-linux]`
  - `warp terminal::input::tests::test_voice_input_toggle_preserves_lock_state [test-linux]`
  - `warp terminal::view::tests::an_inline_editor_paste_then_an_immediate_enter_are_queued_in_order [test-linux]`
  - `warp terminal::view::tests::ssh_handover_enter_guard_does_not_swallow_a_new_line_after_clear [test-linux]`
  - `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer [test-linux]`
  - `markdown_parser::corpus corpus_snapshots_gfm_tables [test-workspace-rest]`
  - `markdown_parser::corpus corpus_snapshots_plain [test-workspace-rest]`
  - `warp_editor::mermaid_corpus hostile_sources_never_panic [test-workspace-rest]`
  - `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted [test-workspace-rest]`
  - `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable [test-workspace-rest]`
  - `warp_editor::mermaid_corpus valid_diagrams_render_with_their_labels [test-workspace-rest]`
- FIXED failures (unit): 
  - `ai project_context::model::tests::request_before_prewarm_waits_for_the_scan_instead_of_scanning [test-workspace-rest]`
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18bu (3016013363); test-linux ran fewer tests (10364 vs 10429): missing failures not counted as fixed; test-warpui-core: build/r18bn (3015761186); test-workspace-rest: build/r18bn (3015761186)
