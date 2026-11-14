# build/r30e  a997653cf  (pipeline 3068181472)

- date: 2026-11-14, tier: targeted, pipeline status: success
- compile (check-linux): ok, 125 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_chips.bash.lan.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_tab_no_helper.*`
  - asked for on macos-lab: `lab_chips.bash.lan.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_chips.zsh.150.nohelper`, `lab_chips.fish.lan.nohelper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_fam_git_read.zsh.lan.nohelper`, `lab_fam_git_read.zsh.150.nohelper`, `lab_fam_run_command.bash.lan.nohelper`, `lab_fam_run_command.zsh.lan.nohelper`, `lab_fam_run_command.zsh.150.nohelper`, `lab_tab_no_helper.*`
  - asked for on windows: `routing_table_powershell`
  - asked for on windows-lab: `lab_chips.bash.lan.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_fam_git_read.bash.lan.nohelper`, `lab_tab_no_helper.*`, `lab_connect_typeahead.*`
- unit: 50237 passed, 1 failed, 191 skipped of 50238 run (9 suites)
- app (pass/fail/skip): lab-macos 17/3/0 +6 no-ref; lab-wayland 14/4/0 +3 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-lab-linux | failed | 8.3 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 11.5 | MacBookPro-gitrunner |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | manual | - | - | not run |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | manual | - | - | not run |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.8 | - |  |
| build-macos-arm64 | success | 18.8 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
| check-linux | success | 10.8 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 8.7 | - |  |
| guards-helper-deps | success | 0.3 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| lab-helper | success | 0.7 | - |  |
| licenses | success | 0.3 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.4 | - |  |
| test-linux | success | 22.4 | - |  |
| test-macos | success | 29.2 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.1 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.1 | phosphor-windows-vm |  |
| test-windows | failed | 44.3 | phosphor-windows-vm |  |
| test-workspace-rest | success | 1.6 | - |  |
| test-workspace-rest-macos | success | 3.6 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.2 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::triples_command::off` LEAK (131.5s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.8s)
- `warp util::git::tests::commit_chain_commit_only_creates_a_commit_and_reports_delta` FAIL (1.4s)
- `warp workspace::view::tests::test_active_tab_bar_position_id_tracks_layout` LEAK (0.7s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.1s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.1s)
- `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it` LEAK (1.8s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_chips.bash.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.54/1; side-call producers (#2156): shell exec lines |
| terminal | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.55/5; side-call producers (#2156): shell exec lines |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_chips.fish.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.53/1; side-call producers (#2156): shell exec lines |
| terminal | lab_fam_git_read.bash.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.89/1, 4.89/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_git_read.zsh.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.66/1, 4.42/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_git_read.zsh.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.36/1, 5.26/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_run_command.bash.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.89/1, 4.64/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_run_command.zsh.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.11/1, 4.68/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_run_command.zsh.150.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.54/1, 5.28/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | PASS | injector accounting: injector sent 348; app saw 348; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 328 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received ['cd /work/u_zsh/tnh_t54q6n1bo/phxt54q6n1bo'], wa |
| terminal | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_chips.bash.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.57/1; side-call producers (#2156): shell exec lines |
| agent | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.36/1; side-call producers (#2156): shell exec lines |
| agent | lab_chips.zsh.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_chips.fish.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.54/1; side-call producers (#2156): shell exec lines |
| agent | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.lan.nohelper | PASS | injector accounting: injector sent 348; app saw 348; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 328 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.150.nohelper | PASS | injector accounting: injector sent 348; app saw 348; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 328 unbound keydown/typed |
| agent | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_chips.bash.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.43/5; side-call producers (#2156): batched exec #3  |
| terminal | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.22/1; side-call producers (#2156): batched exec #4  |
| terminal | lab_chips.fish.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/1; side-call producers (#2156): batched exec #3  |
| terminal | lab_fam_git_read.bash.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.03/5, 2.65/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_git_read.zsh.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.82/1, 2.65/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_fam_run_command.bash.lan.nohelper | NOREF | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 0.61/1, 2.44/9; NO-REFERENCE: UNVERIFIABLE: [hook] th |
| terminal | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | Tab + Enter 30 ms later (trial 1): the shell received ['cd /work/u_zsh/tnh_t3479n1d4/phxt36x9m605'], want exactly one line, the completed 'c |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received ['cd /work/u_zsh/tnh_t42cefb2a/phxt42cefb2a'], wa |
| terminal | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_chips.bash.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.43/1; side-call producers (#2156): batched exec #3  |
| agent | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.23/1; side-call producers (#2156): batched exec #4  |
| agent | lab_chips.fish.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.03/5; side-call producers (#2156): batched exec #3  |
| agent | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | Tab + Enter 30 ms later (trial 1): the shell received ['cd /work/u_zsh/tnh_t25ctzkk6/phxt27oam89o'], want exactly one line, the completed 'c |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | Tab after the prefix should have completed the REMOTE directory: the remote shell received ['cd /work/u_zsh/tnh_t33d4x9m6/phxt33d4x9m6'], wa |
| agent | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::off [test-windows]`
  - `warp util::git::tests::commit_chain_commit_only_creates_a_commit_and_reports_delta [test-windows]`
  - `warp workspace::view::tests::test_active_tab_bar_position_id_tracks_layout [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::on [test-windows]`
  - `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r30d (3067664947); test-macos: build/r30d (3067664947); test-warpui-core: build/r30d (3067664947); test-warpui-core-macos: build/r30d (3067664947); test-warpui-core-windows: build/r30d (3067664947); test-windows: build/r30d (3067664947); test-workspace-rest: build/r30d (3067664947); test-workspace-rest-macos: build/r30d (3067664947); test-workspace-rest-windows: build/r30d (3067664947); app lab-wayland: build/r13a (3051722998)
