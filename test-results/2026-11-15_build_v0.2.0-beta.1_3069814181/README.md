# build/v0.2.0-beta.1  68dbe1e6c  (pipeline 3069814181)

- date: 2026-11-15, tier: targeted, pipeline status: success
- compile (check-linux): ok, 124 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-wayland: `mode_proof`
  - asked for on macos: `mode_proof`
  - asked for on windows: `mode_proof`
- unit: 50561 passed, 0 failed, 191 skipped of 50561 run (9 suites)
- app (pass/fail/skip): macos 2/0/0; wayland 2/0/0; windows 3/0/0; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| agent-fuzz-catches-wave0 | manual | - | - | not run |
| app-test-lab-linux | manual | - | - | not run |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | skipped | - | - |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | success | 0.9 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | success | 0.9 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-replay-macos | skipped | - | - |  |
| app-test-replay-windows | skipped | - | - |  |
| app-test-windows | success | 3.6 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.7 | - |  |
| build-macos-arm64 | success | 19.1 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 37.7 | phosphor-windows-vm |  |
| check-linux | success | 8.1 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 7.7 | - |  |
| guards-helper-deps | success | 0.0 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| lab-helper | manual | - | - | not run |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | manual | - | - | not run |
| test-linux | success | 22.3 | - |  |
| test-macos | success | 29.2 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.0 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.2 | phosphor-windows-vm |  |
| test-windows | success | 45.2 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.7 | - |  |
| test-workspace-rest-macos | success | 3.6 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 6.9 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on` LEAK (52.6s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off` LEAK (52.1s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_question::off` LEAK (52.3s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::off` LEAK (131.9s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::on` LEAK (131.1s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.7s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| agent | mode_proof | PASS |  |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| agent | mode_proof | PASS |  |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| agent | mode_proof | PASS |  |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::off [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
- NEW failures (app): none
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r31b (3069275695); test-macos: build/r31b (3069275695); test-warpui-core: build/r31b (3069275695); test-warpui-core-macos: build/r31b (3069275695); test-warpui-core-windows: build/r31b (3069275695); test-windows: build/r31b (3069275695); test-workspace-rest: build/r31b (3069275695); test-workspace-rest-macos: build/r31b (3069275695); test-workspace-rest-windows: build/r31b (3069275695); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998); app windows: build/r13a (3051722998)
