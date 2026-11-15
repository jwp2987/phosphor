# build/r31b  6936b7cf2  (pipeline 3069275695)

- date: 2026-11-14, tier: targeted, pipeline status: success
- compile (check-linux): ok, 124 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on linux-lab: `lab_connect_typeahead.bash.*`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.bash.150.nohelper`, `lab_tab_remote_dir.zshc.150.nohelper`, `lab_tab_remote_dir.fish.150.nohelper`, `lab_tab_remote_dir.p_slowrc.lan.nohelper`, `lab_tab_remote_dir.p_slowrc.150.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_chips.bash.lan.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_agent_nld_predict_ssh.bash.lan.nohelper`
  - asked for on linux-wayland: `expert_fuzz`, `fast_typing`, `enter_loss_control`, `key_loss_control`, `agent_bg_job_output`, `scroll_keys_position`, `restore_shows_history`, `subshell_ready_rc_clear`, `ssh_drop_reconnect`
  - asked for on macos: `expert_fuzz`, `fast_typing`, `enter_loss_control`, `key_loss_control`, `agent_bg_job_output`, `scroll_keys_position`, `subshell_ready_rc_clear`
  - asked for on macos-lab: `lab_connect_typeahead.zsh.*`, `lab_tab_no_helper.*`, `lab_tab_remote_dir.zshc.150.nohelper`, `lab_tab_remote_dir.p_slowrc.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`, `lab_chips.zsh.lan.nohelper`, `lab_chips.zsh.150.nohelper`
  - asked for on windows: `expert_fuzz`, `selection_modes`, `gitbash_typeahead_reps`, `enter_loss_control`, `routing_table`, `ui_completions_as_you_type_local`, `intl_wide_paste`, `ctrl_c_behind_big_paste`, `powershell_setup_ctrl_c_keys`, `agent_code_recycle`, `ssh_gitbash_managed`, `ssh_gitbash_failing_not_local_managed`, `phosphor_scheme_handler`
  - asked for on windows-lab: `lab_connect_typeahead.bash.*`, `lab_chips.bash.lan.nohelper`, `lab_path_detect.p_exec.lan.nohelper`
- unit: 50526 passed, 0 failed, 191 skipped of 50526 run (9 suites)
- app (pass/fail/skip): lab-macos 21/6/0 +4 no-ref; lab-wayland 23/4/0 +11 no-ref; lab-windows 13/4/12; macos 12/1/0; wayland 14/2/0; windows 35/8/18 +2 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| agent-fuzz-catches-wave0 | manual | - | - | not run |
| app-test-lab-linux | failed | 16.3 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | failed | 17.1 | MacBookPro-gitrunner |  |
| app-test-lab-windows | failed | 12.2 | phosphor-windows-vm |  |
| app-test-linux-wayland | failed | 12.7 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 9.0 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-replay-macos | manual | - | - | not run |
| app-test-replay-windows | manual | - | - | not run |
| app-test-windows | failed | 30.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.3 | - |  |
| build-macos-arm64 | success | 18.9 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.6 | phosphor-windows-vm |  |
| check-linux | success | 11.5 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 8.9 | - |  |
| guards-helper-deps | success | 0.0 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| lab-helper | success | 0.1 | - |  |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.4 | - |  |
| test-linux | success | 25.9 | - |  |
| test-macos | success | 29.2 | MacBookPro-gitrunner |  |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.7 | - |  |
| test-warpui-core-macos | success | 1.1 | MacBookPro-gitrunner |  |
| test-warpui-core-windows | success | 5.0 | phosphor-windows-vm |  |
| test-windows | success | 45.4 | phosphor-windows-vm |  |
| test-workspace-rest | success | 3.7 | - |  |
| test-workspace-rest-macos | success | 3.6 | MacBookPro-gitrunner |  |
| test-workspace-rest-windows | success | 7.0 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on` LEAK (56.0s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on` LEAK (54.1s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_command::on` LEAK (133.6s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (3.7s)
- `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line` LEAK (2.1s)

## app lab-macos (app-test-lab-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 235 ms (RTT 150 ms): no band yet for macos/terminal/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a n |
| terminal | lab_tab_remote_dir.p_slowrc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 162 ms (RTT 0 ms): no band yet for macos/terminal/lab_tab_remote_dir.p_slowrc.lan.nohelper/tab_to_insert (a |
| terminal | lab_path_detect.p_exec.lan.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 3 t |
| terminal | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.58/5; side-call producers (#2156): shell exec lines |
| terminal | lab_chips.zsh.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| terminal | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| terminal | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 245 ms (RTT 150 ms): no band yet for macos/agent/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a new  |
| agent | lab_tab_remote_dir.p_slowrc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 161 ms (RTT 0 ms): no band yet for macos/agent/lab_tab_remote_dir.p_slowrc.lan.nohelper/tab_to_insert (a ne |
| agent | lab_path_detect.p_exec.lan.nohelper | PASS | injector accounting: injector sent 95; app saw 95; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 85 unbound keydown/typed, 3 t |
| agent | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.58/1; side-call producers (#2156): shell exec lines |
| agent | lab_chips.zsh.150.nohelper | FAIL | 3 side-call exec channels for 1 prompt(s): the app must batch to at most one per prompt; exec_channel_timeline (s after the window opened, b |
| agent | lab_connect_typeahead.zsh.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.zsh.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| agent | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | agent_view_not_restored_after_lab_connect_typeahead.zsh.lan.helper | PASS | lab_connect_typeahead.zsh.lan.helper relaunched the app itself (pid 30916 -> 37358); a relaunched app starts in a terminal session, so this  |
| agent | lab_connect_typeahead.zsh.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 258 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.bash.150.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 216 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 222 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.fish.150.nohelper/tab_to_insert (a |
| terminal | lab_tab_remote_dir.p_slowrc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 107 ms (RTT 0 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_slowrc.lan.nohelper/tab_to_insert  |
| terminal | lab_tab_remote_dir.p_slowrc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 238 ms (RTT 150 ms): no band yet for wayland/terminal/lab_tab_remote_dir.p_slowrc.150.nohelper/tab_to_inser |
| terminal | lab_path_detect.p_exec.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| terminal | lab_chips.bash.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/1; side-call producers (#2156): batched exec #3  |
| terminal | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/5; side-call producers (#2156): batched exec #4  |
| terminal | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zsh.lan.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| terminal | lab_tab_no_helper.zsh.150.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| terminal | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| terminal | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_tab_remote_dir.bash.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 242 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.bash.150.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.zshc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 219 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.zshc.150.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.fish.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 249 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.fish.150.nohelper/tab_to_insert (a ne |
| agent | lab_tab_remote_dir.p_slowrc.lan.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 110 ms (RTT 0 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_slowrc.lan.nohelper/tab_to_insert (a  |
| agent | lab_tab_remote_dir.p_slowrc.150.nohelper | NOREF | NO-REFERENCE: Tab -> insert took 264 ms (RTT 150 ms): no band yet for wayland/agent/lab_tab_remote_dir.p_slowrc.150.nohelper/tab_to_insert ( |
| agent | lab_path_detect.p_exec.lan.nohelper | PASS | injector accounting: injector sent 97; app saw 97; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 87 unbound keydown/typed, 6 t |
| agent | lab_chips.bash.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.03/1; side-call producers (#2156): batched exec #3  |
| agent | lab_chips.zsh.lan.nohelper | PASS | exec_channel_timeline (s after the window opened, backend_route lines logged by then): 1.02/5; side-call producers (#2156): batched exec #4  |
| agent | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_tab_no_helper.bash.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.bash.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zsh.lan.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| agent | lab_tab_no_helper.zsh.150.nohelper | FAIL | zsh without compinit answers an in-band Tab with nothing by design (#2174), so the completer answers: 0 candidate line(s), 0 empty answer(s) |
| agent | lab_tab_no_helper.zshc.lan.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_tab_no_helper.zshc.150.nohelper | PASS | injector accounting: injector sent 357; app saw 357; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 337 unbound keydown/typed |
| agent | lab_agent_nld_predict_ssh.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing; injector accountin |
| agent | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | agent_view_not_restored_after_lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead.bash.lan.helper relaunched the app itself (pid 2463890 -> 2481472); a relaunched app starts in a terminal session, so  |
| agent | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |

## app lab-windows (app-test-lab-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_path_detect.p_exec.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_chips.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.150.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.lan.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal | lab_connect_typeahead.bash.150.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | lab_path_detect.p_exec.lan.nohelper | FAIL | p_exec's profile execs zsh: the app log must say the zsh replaced the pending bash and took over its ssh session (#2155); it says: no such l |
| terminal@gitbash | lab_chips.bash.lan.nohelper | FAIL | the app log has no update_chip_value ShellGitBranch -> brt8ogubbb line: the chip never took the branch; no side call at all with no helper:  |
| terminal@gitbash | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal@gitbash | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal@gitbash | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| terminal@gitbash | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent | lab_path_detect.p_exec.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_chips.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.lan.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.150.nohelper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.lan.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent | lab_connect_typeahead.bash.150.helper | SKIP | not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | lab_path_detect.p_exec.lan.nohelper | FAIL | p_exec's profile execs zsh: the app log must say the zsh replaced the pending bash and took over its ssh session (#2155); it says: no such l |
| agent@gitbash | lab_chips.bash.lan.nohelper | FAIL | the app log has no update_chip_value ShellGitBranch -> brt8ogubbb line: the chip never took the branch; no side call at all with no helper:  |
| agent@gitbash | lab_connect_typeahead.bash.lan.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent@gitbash | lab_connect_typeahead.bash.150.nohelper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent@gitbash | lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| agent@gitbash | agent_view_not_restored_after_lab_connect_typeahead.bash.lan.helper | PASS | lab_connect_typeahead.bash.lan.helper relaunched the app itself (pid 11400 -> 13204); a relaunched app starts in a terminal session, so this |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | lab_connect_typeahead.bash.150.helper | PASS | lab_connect_typeahead loss: 0 of 6 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 39.3 %; this n cannot exclude a 1 |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: injector sent 2058; app saw 2058; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1996 unbound keydown/ty |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 710; app saw 710; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 647 unbound keydown/typed |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@41 ms, InitShell@83 ms, Bootstrapped@667 ms; the first inner line ran 0.00 s after its |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed,  |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@41 ms, InitShell@83 ms, Bootstrapped@665 ms; the first inner line ran 0.00 s after its |
| agent | scroll_keys_position | PASS | injector accounting: injector sent 192; app saw 192; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 178 unbound keydown/typed,  |
| agent | agent_bg_job_output | FAIL | the job's line is on screen 2 times: it was repeated into another block (shots/*abj_2_next.png); injector accounting: injector sent 68; app  |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot ga |
| terminal | fast_typing | PASS | injector accounting: injector sent 2072; app saw 2072; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2010 unbound keydown/ty |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 721; app saw 721; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 662 unbound keydown/typed |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@17 ms, InitShell@46 ms, Bootstrapped@261 ms; the first inner line ran 0.00 s after its |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 2313; app saw 2313; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2249 unbound keydown/ty |
| terminal | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines wer |
| terminal | restore_shows_history | PASS | injector accounting: injector sent 420; app saw 420; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 208 unbound keydown/typed,  |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@15 ms, InitShell@41 ms, Bootstrapped@268 ms; the first inner line ran 0.00 s after its |
| agent | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines wer |
| agent | agent_bg_job_output | PASS | injector accounting: injector sent 67; app saw 67; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 59 unbound keydown/typed, 6 t |
| agent | restore_shows_history | PASS | injector accounting: injector sent 607; app saw 607; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 260 unbound keydown/typed,  |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| terminal | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.6 ms from ctrl_c_keydown to its pty |
| terminal | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was n |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| terminal | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| terminal | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git  |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| terminal | phosphor_scheme_handler | FAIL | scheme_end: file has 0/2 lines, exact match False first diff: exp='t73_rhlgozo52y' got=None [first bad line 0: injector call #127 posted its |
| terminal@gitbash | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | expert_fuzz | PASS | injector accounting: injector sent 718; app saw 718; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 651 unbound keydown/typed |
| terminal@gitbash | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.3 ms from ctrl_c_keydown to its pty |
| terminal@gitbash | powershell_setup_ctrl_c_keys | SKIP | not run in the Git Bash pane: #1890: a PowerShell-in-PowerShell Phosphorize setup and the PowerShell hold's post-abort prompt; the Git Bash  |
| terminal@gitbash | routing_table | PASS | injector accounting: injector sent 645; app saw 645; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal@gitbash | intl_wide_paste | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; verified: 10 effect-file byte comparison(s) against the e |
| terminal@gitbash | ssh_gitbash_managed | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| terminal@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.38 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | ui_completions_as_you_type_local | FAIL | no completion menu with both candidates after typing a shared prefix without Tab; fixture dir C:\gl\b\07xLPX_fG\0\winters-phosphor\phosphor\ |
| terminal@gitbash | selection_modes | PASS | injector accounting: injector sent 292; app saw 292; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 267 unbound keydown/typed,  |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8457; app saw 8457; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8222 unbound keydown/ |
| terminal@gitbash | phosphor_scheme_handler | SKIP | not run in the Git Bash pane: #1546: the URL handler does not depend on the pane's shell: runs once, in the PowerShell pane (the app's defau |
| terminal@pwsh7 | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@pwsh7 | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.5 ms from ctrl_c_keydown to its pty |
| terminal@pwsh7 | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was n |
| agent | mode_proof | PASS |  |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.2 ms from ctrl_c_keydown to its pty |
| agent | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was n |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| agent | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| agent | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git  |
| agent | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| agent | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| agent@gitbash | mode_proof | PASS |  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | expert_fuzz | PASS | injector accounting: injector sent 750; app saw 750; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 678 unbound keydown/typed |
| agent@gitbash | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.4 ms from ctrl_c_keydown to its pty |
| agent@gitbash | powershell_setup_ctrl_c_keys | SKIP | not run in the Git Bash pane: #1890: a PowerShell-in-PowerShell Phosphorize setup and the PowerShell hold's post-abort prompt; the Git Bash  |
| agent@gitbash | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| agent@gitbash | ssh_gitbash_managed | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): |
| agent@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.31 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent@gitbash | selection_modes | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops |
| agent@gitbash | gitbash_typeahead_reps | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+= |
| agent@pwsh7 | mode_proof | PASS |  |
| agent@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@pwsh7 | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.4 ms from ctrl_c_keydown to its pty |
| agent@pwsh7 | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was n |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_code_recycle | NOREF | NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected b |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_code_recycle | NOREF | NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected b |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_bg_job_output | macos | agent | FAIL |
| agent_bg_job_output | wayland | agent | PASS |
| agent_code_recycle | windows | agent@fake | NOREF |
| agent_code_recycle | windows | agent@gitbash+fake | NOREF |
| ctrl_c_behind_big_paste | windows | agent | FAIL |
| ctrl_c_behind_big_paste | windows | agent@gitbash | FAIL |
| ctrl_c_behind_big_paste | windows | agent@pwsh7 | FAIL |
| ctrl_c_behind_big_paste | windows | terminal | FAIL |
| ctrl_c_behind_big_paste | windows | terminal@gitbash | FAIL |
| ctrl_c_behind_big_paste | windows | terminal@pwsh7 | FAIL |
| gitbash_typeahead_reps | windows | agent | SKIP |
| gitbash_typeahead_reps | windows | agent@gitbash | PASS |
| gitbash_typeahead_reps | windows | terminal | SKIP |
| gitbash_typeahead_reps | windows | terminal@gitbash | PASS |
| phosphor_scheme_handler | windows | terminal | FAIL |
| phosphor_scheme_handler | windows | terminal@gitbash | SKIP |
| powershell_setup_ctrl_c_keys | windows | agent | PASS |
| powershell_setup_ctrl_c_keys | windows | agent@gitbash | SKIP |
| powershell_setup_ctrl_c_keys | windows | agent@pwsh7 | PASS |
| powershell_setup_ctrl_c_keys | windows | terminal | PASS |
| powershell_setup_ctrl_c_keys | windows | terminal@gitbash | SKIP |
| powershell_setup_ctrl_c_keys | windows | terminal@pwsh7 | PASS |
| restore_shows_history | wayland | agent | PASS |
| restore_shows_history | wayland | terminal | PASS |
| scroll_keys_position | macos | agent | PASS |
| scroll_keys_position | macos | terminal | PASS |
| scroll_keys_position | wayland | agent | FAIL |
| scroll_keys_position | wayland | terminal | FAIL |
| selection_modes | windows | agent | SKIP |
| selection_modes | windows | agent@gitbash | PASS |
| selection_modes | windows | terminal | SKIP |
| selection_modes | windows | terminal@gitbash | PASS |
| ssh_gitbash_failing_not_local_managed | windows | agent | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | agent@gitbash | PASS |
| ssh_gitbash_failing_not_local_managed | windows | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | terminal@gitbash | PASS |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | agent@gitbash | SKIP |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | FAIL |

passed everywhere it ran (2, candidates for the owner's decision): restore_shows_history, subshell_ready_rc_clear
not passed somewhere (fail / skip / no-reference): agent_bg_job_output, agent_code_recycle, ctrl_c_behind_big_paste, gitbash_typeahead_reps, phosphor_scheme_handler, powershell_setup_ctrl_c_keys, scroll_keys_position, selection_modes, ssh_gitbash_failing_not_local_managed, ssh_gitbash_managed, ui_completions_as_you_type_local
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: macos/run_slim/logs/agent/acct/launch0_1794705167.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794704688.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794705981.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794705504.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/phosphor.log.gz: 2292 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.log.gz: 2291 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794704708.log.in_session.0: 10239 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_question::on [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::on [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_failing_command::off [test-windows]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::off [test-windows]`
  - `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers [test-windows]`
  - `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command [test-windows]`
  - `warp workspace::view::tests::test_active_tab_bar_position_id_tracks_layout [test-windows]`
  - `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer [test-windows]`
  - `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state [test-windows]`
  - `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/terminal/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_no_helper.zsh.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_tab_no_helper.zsh.150.nohelper (case not in previous run)`
  - `macos/agent/agent_bg_job_output (case not in previous run)`
  - `wayland/terminal/scroll_keys_position (case not in previous run)`
  - `wayland/agent/scroll_keys_position (case not in previous run)`
  - `windows/terminal/ctrl_c_behind_big_paste (case not in previous run)`
  - `windows/terminal/phosphor_scheme_handler (case not in previous run)`
  - `windows/terminal@gitbash/ctrl_c_behind_big_paste (case not in previous run)`
  - `windows/terminal@gitbash/ui_completions_as_you_type_local`
  - `windows/terminal@pwsh7/ctrl_c_behind_big_paste (case not in previous run)`
  - `windows/agent/ctrl_c_behind_big_paste (case not in previous run)`
  - `windows/agent@gitbash/ctrl_c_behind_big_paste (case not in previous run)`
  - `windows/agent@pwsh7/ctrl_c_behind_big_paste (case not in previous run)`
- FIXED failures (app): 
  - `windows/terminal@gitbash/routing_table`
- Latency regressions > 5%: none
- compared against: test-linux: build/r30f (3068353168); test-macos: build/r30f (3068353168); test-warpui-core: build/r30f (3068353168); test-warpui-core-macos: build/r30f (3068353168); test-warpui-core-windows: build/r30f (3068353168); test-windows: build/r30f (3068353168); test-workspace-rest: build/r30f (3068353168); test-workspace-rest-macos: build/r30f (3068353168); test-workspace-rest-windows: build/r30f (3068353168); app lab-wayland: build/r13a (3051722998); app macos: build/r13a (3051722998); app wayland: build/r13a (3051722998); app windows: build/r13a (3051722998)
