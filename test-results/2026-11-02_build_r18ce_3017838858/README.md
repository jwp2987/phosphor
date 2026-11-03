# build/r18ce  18fddcd80  (pipeline 3017838858)

- date: 2026-11-02, tier: fix, pipeline status: failed
- compile (check-linux): ok, 46 warnings
- unit: 14354 passed, 5 failed, 55 skipped of 14359 run (3 suites)
- app (pass/fail/skip): macos 5/7/0; wayland 6/1/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | failed | 0.2 | - |  |
| app-test-linux-wayland | canceled | 4.7 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | canceled | 19.4 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | canceled | - | - | no results artifact (job canceled) |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 34.6 | - |  |
| build-macos-arm64 | success | 20.1 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 40.7 | phosphor-windows-vm |  |
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
| test-linux | success | 8.2 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | success | 1.2 | - |  |
| test-workspace-rest | failed | 1.6 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-workspace-rest failures

- `markdown_parser::corpus corpus_snapshots_gfm_tables` FAIL (0.2s)
- `markdown_parser::corpus corpus_snapshots_plain` FAIL (0.2s)
- `warp_editor::mermaid_corpus hostile_sources_never_panic` SIGABRT (8.3s)
- `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable` FAIL (0.2s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal | fast_typing | FAIL | ft0: file has 28/30 lines, exact match False first diff: exp='t27_9guc0eyjiv' got='t27_9guc0eyjiv t28_ye9ofrxs8h'; ft0: shell received 28 co |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 209.70 ms over the harness floor exceeds the 20 ms budget; Ctrl-C: n=27 |
| terminal | ssh_connect_typing | FAIL | remote shell received 0/12 typed-ahead commands exactly (missing 12) |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| terminal | latency_paint | FAIL | only 0 key->paint samples from kl3 (capture region or Screen Recording permission wrong) |
| terminal | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector; phosphor / perline-unprimed: |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C: 33.68 ms over the harness floor exceeds the 20 ms budget |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): 
  - `warp terminal::bootstrap::tests::test_nu_real_local_bootstrap_emits_hooks_and_keeps_history_clean [test-linux]`
  - `warp terminal::input::tests::test_voice_input_toggle_preserves_lock_state [test-linux]`
  - `warp terminal::view::tests::an_inline_editor_paste_then_an_immediate_enter_are_queued_in_order [test-linux]`
  - `warp terminal::view::tests::ssh_handover_enter_guard_does_not_swallow_a_new_line_after_clear [test-linux]`
  - `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer [test-linux]`
  - `warp_editor::mermaid_corpus valid_diagrams_render_with_their_labels [test-workspace-rest]`
- NEW failures (app): 
  - `macos/terminal/shell_fish (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/key_loss_control`
  - `macos/terminal/paste_instant_enter`
  - `wayland/terminal/cadence`
  - `wayland/terminal/fast_typing`
  - `wayland/terminal/paste_instant_enter`
  - `wayland/terminal/tab_enter`
- Latency regressions > 5%: none
- compared against: test-linux: build/r18cc (3017512071); test-warpui-core: build/r18cc (3017512071); test-workspace-rest: build/r18cc (3017512071); app macos: build/r18bs (3015931911); app wayland: build/r18bn (3015761186)
