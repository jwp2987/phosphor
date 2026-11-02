# build/r18ci  1159a084f  (pipeline 3018380432)

- date: 2026-11-02, tier: fix, pipeline status: success
- compile (check-linux): ok, 46 warnings
- unit: 0 passed, 0 failed, 0 skipped of 0 run (0 suites)
- app (pass/fail/skip): macos 14/14/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | failed | 33.5 | MacBookPro-gitrunner |  |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 9.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.0 | - |  |
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

reuse: `{"reused_build": {"job_id": 17667172373, "job_name": "build-macos-arm64", "pipeline_id": "3018144637", "commit": "e01e2afad5f0ec958a271cfe949eed8bdc56fe8b", "this_pipeline": "3018380432", "this_commit": "1159a084f849ea8dc489348bdd165974557dbb81", "fetched": "local cache (/Users/<user>/.cache/phospho`

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | harness exception: RuntimeError("frontmost app is 'Phosphor', not Terminal: refusing to send keys"); reference windows left open: Terminal w |
| terminal | enter_loss_control | PASS | reference windows left open: Terminal was already running (pid 66832); reference Terminal.app (.command via open) gap 0.00s: 0/30 lines miss |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-D in cat: file ['helloecho done1 >> cd.txt']; nested bash not entered (SHL |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T163_c71lmo never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | latency_enter | FAIL | only 22/30 Enter timings captured; Enter to command start: Phosphor overhead 10.33 ms over Terminal.app (.command via open) exceeds the 2 ms |
| terminal | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Pho |
| terminal | latency_paint | FAIL | Key to paint: Phosphor overhead 7.33 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-fi |
| terminal | shell_fish | FAIL | fish: autosuggestion (a history line sharing the typed prefix) did not change the executed line: expected ['AUTOSUGG_alpha_t180_dmdhfosq', ' |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t185_z46bxitkti', 't186_k3jikfqpwu', 't187_r4te1j9w2g', 't188_uel48ccmwx', 't189_6xvhlycrnd', 't190_tuzpxdosa |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines; Tab+Enter ran the UNCOMPLETED line, stray files: ['zzcompl_alpha_u'] |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; `cat > cd.txt` never started (its Enter was lost?): Ctrl-D NOT sent (it would h |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T79_1ereql never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | latency_enter | FAIL | only 20/30 Enter timings captured; Enter to command start: Phosphor overhead 8.88 ms over Terminal.app (.command via open) exceeds the 2 ms  |
| agent | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Pho |
| agent | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the sam |
| agent | shell_fish | FAIL | fish: after exit the pane's bash ran the next command: expected ['t100_q08s58nv'], got []; fish_rec.log empty: the fish_postexec hook from c |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t101_5gawrd82tg', 't102_6rrp0jiqm0', 't103_86j0rr4tr5', 't104_5x4pvll28j', 't105_6u7inu54vh', 't106_iqof8dlho |
| agent | md_viewer | PASS |  |

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/key_loss_control`
  - `macos/terminal/latency_paint`
  - `macos/agent/cadence`
  - `macos/agent/tab_enter`
- FIXED failures (app): 
  - `macos/terminal/fast_typing`
  - `macos/terminal/tab_enter`
  - `macos/terminal/latency_key_log`
  - `macos/agent/latency_key_log`
- Latency regressions > 5%: none
- compared against: app macos: build/r18ch (3018144637)
