# build/r18ch  e01e2afad  (pipeline 3018144637)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 46 warnings
- unit: 14355 passed, 5 failed, 55 skipped of 14360 run (3 suites)
- app (pass/fail/skip): macos 14/14/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 28.9 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | success | 19.8 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 6.8 | - |  |
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
| test-linux | success | 7.3 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | success | 1.3 | - |  |
| test-workspace-rest | failed | 2.2 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-workspace-rest failures

- `markdown_parser::corpus corpus_snapshots_gfm_tables` FAIL (0.2s)
- `markdown_parser::corpus corpus_snapshots_plain` FAIL (0.2s)
- `warp_editor::mermaid_corpus hostile_sources_never_panic` SIGABRT (8.4s)
- `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable` FAIL (0.2s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), injector posted 30 Enters; reference Terminal. |
| terminal | fast_typing | FAIL | ft0: file has 29/30 lines, exact match False first diff: exp='t99_wdg90hqrl4' got='t99_wdg90hqrl4 t100_nfyh2s65zh' [first bad line 14: injec |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines; Tab+Enter ran the UNCOMPLETED line, stray files: ['zzcompl_alpha_u'] |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 223.75 ms over the harness floor exceeds the 20 ms budget; Ctrl-D in ca |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T163_c71lmo never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | latency_enter | FAIL | only 18/30 Enter timings captured; Enter to command start: Phosphor overhead 11.34 ms over Terminal.app (.command via open) exceeds the 2 ms |
| terminal | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| terminal | latency_paint | PASS |  |
| terminal | shell_fish | FAIL | fish: pasted lines + instant Enter: expected ['t176_olnzdp92cv', 't177_u0hbl6flno', 't178_ltgduje2jo', 't179_cswdf2molh'], got ['t176_olnzdp |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t185_z46bxitkti', 't186_k3jikfqpwu', 't187_r4te1j9w2g', 't188_uel48ccmwx', 't189_6xvhlycrnd', 't190_tuzpxdosa |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: 225.40 ms over the harness floor exceeds the 20 ms budget; `cat > cd.tx |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T79_1ereql never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | latency_enter | FAIL | only 22/30 Enter timings captured; Enter to command start: Phosphor overhead 8.73 ms over Terminal.app (.command via open) exceeds the 2 ms  |
| agent | latency_key_log | FAIL | only 0 key->frame pairs in the app log (expected ~60); log /Users/<user>/builds/CR7LvsOfu/0/winters-phosphor/phosphor/appci-out/macos/run/ho |
| agent | latency_paint | PASS |  |
| agent | shell_fish | FAIL | fish: after exit the pane's bash ran the next command: expected ['t100_q08s58nv'], got []; fish_rec.log empty: the fish_postexec hook from c |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t101_5gawrd82tg', 't102_6rrp0jiqm0', 't103_86j0rr4tr5', 't104_5x4pvll28j', 't105_6u7inu54vh', 't106_iqof8dlho |
| agent | md_viewer | PASS |  |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/agent/shell_nu (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/key_loss_control`
  - `macos/agent/fast_typing`
  - `macos/agent/tab_enter`
- Latency regressions > 5%: none
- compared against: test-linux: build/r18ce (3017838858); test-warpui-core: build/r18ce (3017838858); test-workspace-rest: build/r18ce (3017838858); app macos: build/r18cf (3018057763)
