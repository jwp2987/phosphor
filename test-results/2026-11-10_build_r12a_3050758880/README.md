# build/r12a  501a18469  (pipeline 3050758880)

- date: 2026-11-10, tier: fix, pipeline status: manual
- compile (check-linux): ok, 118 warnings
- unit: 16423 passed, 4 failed, 55 skipped of 16427 run (3 suites)
- app (pass/fail/skip): lab-wayland 29/21/0 +50 no-ref; macos 111/0/44 +5 no-ref; wayland 118/2/34 +8 no-ref; windows 131/29/169 +7 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 77.7 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | manual | - | - | not run |
| app-test-lab-windows | manual | - | - | not run |
| app-test-linux-wayland | failed | 108.3 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | success | 134.9 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | failed | 128.0 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.6 | - |  |
| build-macos-arm64 | success | 18.7 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 37.8 | phosphor-windows-vm |  |
| check-linux | success | 8.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 2.5 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
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
| sshlab-image | success | 0.3 | - |  |
| test-linux | failed | 22.7 | - |  |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | success | 1.3 | - |  |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | manual | - | - | not run |
| test-windows | manual | - | - | not run |
| test-workspace-rest | success | 2.4 | - |  |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp ai::agent_providers::tools::web_runtime::websearch_tests::real_exa_anonymous_search` FAIL (0.0s)
- `warp ai::blocklist::agent_fuzz::fuzz_tests::random_agent_conversations_hold_the_invariants` FAIL (162.0s)
- `warp ai::blocklist::agent_fuzz::fuzz_tests::random_agent_conversations_hold_the_invariants_with_the_allowlist_on` FAIL (162.1s)
- `warp terminal::view::tests::a_precmd_functions_question_sends_the_pin_and_its_enter_to_the_read` FAIL (0.1s)

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_enter_start.bash.150.nohelper | PASS | injector accounting: injector sent 1887; app saw 1887; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1850 unbound keydown/ty |
| terminal | lab_enter_start.zsh.150.nohelper | PASS | injector accounting: injector sent 1904; app saw 1904; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1867 unbound keydown/ty |
| terminal | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| terminal | lab_enter_start.nu.150.nohelper | FAIL | `exit` did not return to the local shell (none); 30 of 30 Enters have no hop=enter_begin line with the line's text in the app log: those are |
| terminal | lab_enter_no_network_wait.bash.150.nohelper | PASS | worst Enter -> write 1.38 ms (limit 75 ms); median 1.13 ms delayed against 1.12 ms undelayed; injector accounting: injector sent 3122; app s |
| terminal | lab_enter_no_network_wait.zsh.150.nohelper | PASS | worst Enter -> write 1.27 ms (limit 75 ms); median 1.10 ms delayed against 1.06 ms undelayed; injector accounting: injector sent 3061; app s |
| terminal | lab_enter_no_network_wait.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| terminal | lab_enter_no_network_wait.nu.150.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample |
| terminal | lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| terminal | lab_ctrl_c_no_esc_i.fish.lan.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t557yrfjap5u'], got []; [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x |
| terminal | lab_ctrl_c_no_esc_i.fish.150.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t570l5qgmtgx'], got []; [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t583ait449c2'], got []; [x1 gap 0 ms] the shell received ['sleep 30'], expected exactly [sleep 30, the |
| terminal | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t596r9m0pyng'], got []; [x1 gap 0 ms] the shell received ['sleep 30'], expected exactly [sleep 30, the |
| terminal | lab_fam_agent_context.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.fish.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.nu.lan.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add on |
| terminal | lab_fam_agent_context.nu.150.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add on |
| terminal | lab_enter_start.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_start.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_start.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_no_network_wait.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_no_network_wait.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_no_network_wait.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_enter_no_network_wait.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_ctrl_c_no_esc_i.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| terminal | lab_fam_agent_context.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_start.bash.150.nohelper | PASS | injector accounting: injector sent 1887; app saw 1887; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1850 unbound keydown/ty |
| agent | lab_enter_start.zsh.150.nohelper | PASS | injector accounting: injector sent 1904; app saw 1904; lost-before-app 0 (Enter: sent 33, app saw 33); of the app's: 1867 unbound keydown/ty |
| agent | lab_enter_start.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no paired timing for fish (no timestamped history hook for this shell); injector accounting: injector sent 1532; |
| agent | lab_enter_start.nu.150.nohelper | FAIL | `exit` did not return to the local shell (none); 30 of 30 Enters have no hop=enter_begin line with the line's text in the app log: those are |
| agent | lab_enter_no_network_wait.bash.150.nohelper | PASS | worst Enter -> write 1.63 ms (limit 75 ms); median 1.43 ms delayed against 1.42 ms undelayed; injector accounting: injector sent 3121; app s |
| agent | lab_enter_no_network_wait.zsh.150.nohelper | PASS | worst Enter -> write 1.61 ms (limit 75 ms); median 1.42 ms delayed against 1.37 ms undelayed; injector accounting: injector sent 3061; app s |
| agent | lab_enter_no_network_wait.fish.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample(s) at the delayed link, 8 needed): input-order l |
| agent | lab_enter_no_network_wait.nu.150.nohelper | FAIL | `exit` did not return to the local shell (none); NO-REFERENCE: UNVERIFIABLE: no Enter -> write times in the app log (0 Enter -> write sample |
| agent | lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | injector accounting: injector sent 610; app saw 610; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.bash.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | injector accounting: injector sent 611; app saw 611; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 568 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.bash.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.bash.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | injector accounting: injector sent 601; app saw 601; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 558 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.zsh.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.zsh.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation win |
| agent | lab_ctrl_c_no_esc_i.fish.lan.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t561yrfjap5u'], got []; [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.fish.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.fish.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.fish.150.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t574l5qgmtgx'], got []; [x1 gap 0 ms] the shell received [], expected exactly [sleep 30, the echo]; [x |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.fish.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.fish.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wi |
| agent | lab_ctrl_c_no_esc_i.nu.lan.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t587ait449c2'], got []; [x1 gap 0 ms] the shell received ['sleep 30'], expected exactly [sleep 30, the |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.lan.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.lan.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_ctrl_c_no_esc_i.nu.150.nohelper | FAIL | [x1 gap 0 ms] effect file: expected ['t600r9m0pyng'], got []; [x1 gap 0 ms] the shell received ['sleep 30'], expected exactly [sleep 30, the |
| agent | agent_view_left_by_double_ctrl_c_after_lab_ctrl_c_no_esc_i.nu.150.nohelper | PASS | after lab_ctrl_c_no_esc_i.nu.150.nohelper the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation wind |
| agent | lab_agent_remote_flow.bash.lan.nohelper | PASS | codebase step not run: APPCI_FAKE_EMBEDDINGS=1 is off, so the host has no embeddings provider to index with; injector accounting: injector s |
| agent | lab_agent_remote_flow.nu.lan.nohelper | FAIL | the agent turn did not finish: the model received 0 of 5 tool results; the edited file on the host is ['alpha tkat615zwbsih', 'second line'] |
| agent | lab_agent_remote_errors.bash.lan.nohelper | PASS | injector accounting: injector sent 343; app saw 343; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 334 unbound keydown/typed,  |
| agent | lab_agent_remote_errors.nu.lan.nohelper | FAIL | the agent turn did not finish: the model received 0 of 4 tool results; no hop=backend_route line for file_context: the call never reached th |
| agent | lab_agent_remote_drop.bash.lan.nohelper | FAIL | the approved command never started on the host (no /work/u_bash/started_t626vv6ca0sa there), so the drop could not hit a running tool call;  |
| agent | pane_not_local_after_lab_agent_remote_drop.bash.lan.nohelper | FAIL | after lab_agent_remote_drop.bash.lan.nohelper the pane's shell was not the root shell (none; root pid 450069); the app is relaunched so late |
| agent | lab_agent_remote_drop.nu.lan.nohelper | FAIL | no approval card for the run_shell_command within 90 s; injector accounting: injector sent 150; app saw 150; lost-before-app 0 (Enter: sent  |
| agent | lab_fam_agent_edit_conflict.bash.lan.nohelper | FAIL | remote: the accepted multi-hunk edit: 207 bytes on disk, 228 expected; first difference at byte 25 (b'beta t633nmdwrx\nL03 gamm' vs b'BETA-E |
| agent | lab_fam_agent_edit_conflict.nu.lan.nohelper | FAIL | lab_fam_agent_edit_conflict: no hop=backend_route family=file_write line: the edit was not written through the backend router; `exit` did no |
| agent | lab_fam_agent_delete_conflict.bash.lan.nohelper | PASS | injector accounting: injector sent 151; app saw 151; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 142 unbound keydown/typed,  |
| agent | lab_fam_agent_edit_endings.bash.lan.nohelper | PASS | injector accounting: injector sent 1308; app saw 1308; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 1286 unbound keydown/ty |
| agent | lab_fam_agent_edit_touch_only.bash.lan.nohelper | PASS | injector accounting: injector sent 309; app saw 309; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 299 unbound keydown/typed,  |
| agent | lab_enter_start.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_start.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_start.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_start.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_no_network_wait.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_no_network_wait.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_no_network_wait.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_enter_no_network_wait.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |
| agent | lab_ctrl_c_no_esc_i.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helpe |

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | cadence | PASS | injector accounting: injector sent 179; app saw 179; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 174 unbound keydown/typed,  |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: injector sent 2058; app saw 2058; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1996 unbound keydown/ty |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 1 ta |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ctrl_c_flood_ab | PASS | gate on vs off: Δp50 = -25.11 ms (on 177.74 / off 202.85; p95 190.27 / 210.32; n 30 / 30; blocks of 5, relaunch per block); gate on: 4536 fr |
| terminal | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unb |
| terminal | ps_ctrl_scrolllock_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 256 unbound keydown/typed,  |
| terminal | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 298; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 288 unbound keydown/typed,  |
| terminal | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 235; app saw 235; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 229 unbound keydown/typed,  |
| terminal | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 261 unbound keydown/typed,  |
| terminal | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 264 unbound keydown/typed,  |
| terminal | subshell_sh_probe_empty_ps1 | PASS | empty_ps1 typed-before-the-setup leg: needs the Linux blocked-in-read fact (/proc/<pid>/syscall); not run on darwin; injector accounting: in |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 52; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty startup directory (proved on a pty before the case types anything); injector accounting:  |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 260; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| terminal | gitbash_long_osc_reaches_app | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #2037 key to frame without the wait for the display pass (the app's own share): median 1.57 ms (p95 2.06, n=60); the wait itself: median 5.0 |
| terminal | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| terminal | latency_paint_ab | PASS | key frame at the end of the key press, on vs off: key->paint Δp50 = -4.61 ms (on 29.56 / off 34.17; p95 37.84 / 37.66; n 30 / 30; blocks of  |
| terminal | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.06 / 0.24 (Δ -0.18), f |
| terminal | routing_table | PASS | injector accounting: injector sent 694; app saw 694; lost-before-app 0 (Enter: sent 28, app saw 28); of the app's: 597 unbound keydown/typed |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 222; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 200 unbound keydown/typed,  |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 160; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 143 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 3 t |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 755; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 744 unbound keydown/typed |
| terminal | shell_nu | PASS | injector accounting: injector sent 951; app saw 951; lost-before-app 0 (Enter: sent 28, app saw 28); of the app's: 940 unbound keydown/typed |
| terminal | ui_copy_wrapped_output | PASS | injector accounting: injector sent 233; app saw 233; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| terminal | ui_copy_wrapped_drag | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| terminal | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 461; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 423 unbound keydown/typed |
| terminal | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal | ps_read_host_secure | SKIP | PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret); |
| terminal | gitbash_precmd_secret | SKIP | Git Bash pane only (bash precmd_functions; ConPTY prompt hold); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: s |
| terminal | precmd_secret_honor_ps1 | NOREF | NO-REFERENCE: UNVERIFIABLE: [hp0] the pane is not a bash with WARP_HONOR_PS1=0 after the relaunch with terminal.input.honor_ps1 = false (the |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 125; app saw 125; lo |
| terminal | zero_network_startup | SKIP | zero_network_startup (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0);  |
| terminal | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 2; app saw  |
| terminal | claude_plugin_offline | SKIP | claude_plugin_offline (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); |
| terminal | agent_plugins_offline | SKIP | agent_plugins_offline (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); |
| terminal | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); no close key on this platform: the throw-away tab was ended with Ctrl-D (u |
| terminal | ui_vertical_tabs_panel_click | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| terminal | network_gate_switch | SKIP | network_gate_switch (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| terminal | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 31.5 frames/s for 29.0 wakeups/s (0 Full); 4 hidden tabs streaming 2.5 frames/s, 131.5 hidden  |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 22; app saw 22; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 16 unbound keydown/typed, 2 t |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: injector sent 179; app saw 179; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 174 unbound keydown/typed,  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 1 ta |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 470; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 444 unbound keydown/typed |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | PASS | gate on vs off: Δp50 = -19.47 ms (on 183.78 / off 203.25; p95 197.27 / 210.94; n 30 / 30; blocks of 5, relaunch per block); gate on: 4358 fr |
| agent | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 51; app saw 51; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 48 unb |
| agent | ps_ctrl_scrolllock_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 256; app saw 256; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 251 unbound keydown/typed,  |
| agent | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 293; app saw 293; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 283 unbound keydown/typed,  |
| agent | subshell_ready_unsupported_tcsh | PASS | injector accounting: injector sent 235; app saw 235; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 229 unbound keydown/typed,  |
| agent | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 261 unbound keydown/typed,  |
| agent | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 264 unbound keydown/typed,  |
| agent | subshell_sh_probe_empty_ps1 | PASS | empty_ps1 typed-before-the-setup leg: needs the Linux blocked-in-read fact (/proc/<pid>/syscall); not run on darwin; injector accounting: in |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 52; lost-bef |
| agent | subshell_ready_zsh_newuser_install | SKIP | this zsh runs no zsh-newuser-install with an empty startup directory (proved on a pty before the case types anything); injector accounting:  |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 260; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| agent | gitbash_long_osc_reaches_app | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #2037 key to frame without the wait for the display pass (the app's own share): median 1.57 ms (p95 2.20, n=59); the wait itself: median 4.8 |
| agent | latency_paint | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; Key to paint: kl3 measures post-to-first-c |
| agent | latency_paint_ab | PASS | key frame at the end of the key press, on vs off: key->paint Δp50 = -2.41 ms (on 31.83 / off 34.24; p95 37.47 / 38.2; n 30 / 30; blocks of 5 |
| agent | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.25 / 0.34 (Δ -0.09), fram |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 151 unbound keydown/typed,  |
| agent | routing_table | PASS | injector accounting: injector sent 630; app saw 630; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent | routing_typo_head | PASS | injector accounting: injector sent 73; app saw 73; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 58 unbound keydown/typed, 6 t |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 220; app saw 220; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 200 unbound keydown/typed,  |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 160; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 143 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 755; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 744 unbound keydown/typed |
| agent | shell_nu | PASS | nu app log: 2026-11-10T18:55:12Z [DEBUG] frame 1: updated=["WarpifyFooterView", "PaneView", "Terminal", "ActionButton", "Menu", "ActionButto |
| agent | ui_copy_wrapped_output | PASS | injector accounting: injector sent 233; app saw 233; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| agent | ui_copy_wrapped_drag | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| agent | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 461; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 423 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent | precmd_secret_honor_ps1 | NOREF | NO-REFERENCE: UNVERIFIABLE: [hp0] the pane is not a bash with WARP_HONOR_PS1=0 after the relaunch with terminal.input.honor_ps1 = false (the |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 125; app saw 125; lo |
| agent | keyring_unavailable_startup | SKIP | needs the Linux App launcher (its private D-Bus session decides which Secret Service exists); injector accounting: injector sent 2; app saw  |
| agent | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); no close key on this platform: the throw-away tab was ended with Ctrl-D (u |
| agent | ui_vertical_tabs_panel_click | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 32.5 frames/s for 29.3 wakeups/s (0 Full); 4 hidden tabs streaming 2.5 frames/s, 124.0 hidden  |
| agent@fake | agent_interactive_password | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_accept | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; partial accept: the diff c |
| agent@fake | agent_edit_reject | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; reject: no follow-up model |
| agent@fake | agent_edit_conflict | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_touch_only | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_endings | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_vim | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_vim: hand-over line in  |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: macos); injector account |
| agent@fake | agent_question_answer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer_at_end | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_ssh | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; NO-REFERENCE: UNVERIFIABLE |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@fake | agent_restart_kill_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_less: hand-over line in |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | SKIP | network_gate_notice (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); o |
| agent@fake | agent_gate_deny_allow | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app |
| agent@fake | agent_gate_off_live | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | SKIP | agent_gate (this platform: macos); injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the a |
| agent@fake | agent_multitab_streaming | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 4. |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 take |
| terminal | cadence | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 177 unbound keydown/typed,  |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot ga |
| terminal | fast_typing | PASS | injector accounting: injector sent 2072; app saw 2072; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2010 unbound keydown/ty |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: |
| terminal | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ps_ctrl_scrolllock_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 256 unbound keydown/typed,  |
| terminal | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 298; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 287 unbound keydown/typed,  |
| terminal | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unb |
| terminal | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 261 unbound keydown/typed,  |
| terminal | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 264 unbound keydown/typed,  |
| terminal | subshell_sh_probe_empty_ps1 | PASS | injector accounting: injector sent 692; app saw 692; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 678 unbound keydown/typed |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 52; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 260; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| terminal | gitbash_long_osc_reaches_app | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.61 ms (p95 0.69, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 1 (median 228 |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_paint_ab | SKIP | the key frame drawn at the end of the key press (#1848) exists only in the macOS renderer, and kl3 (ScreenCaptureKit) is macOS only; Linux m |
| terminal | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.04 / 0.14 (Δ -0.1), fr |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to shell (no rout |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 222; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 200 unbound keydown/typed,  |
| terminal | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 160; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 143 unbound keydown/typed |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 16; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 11 unbound keydown/typed, 5 t |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 755; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 744 unbound keydown/typed |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-10T18:29:4 |
| terminal | ui_copy_wrapped_output | PASS | injector accounting: injector sent 233; app saw 233; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| terminal | ui_copy_wrapped_drag | PASS | app find hook gave no position for 'cndh7nx' (no `appci_find` line in the app log: an app without the hook, or RUST_LOG without warpui::appc |
| terminal | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 461; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 423 unbound keydown/typed |
| terminal | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal | ps_read_host_secure | SKIP | PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret); |
| terminal | gitbash_precmd_secret | SKIP | Git Bash pane only (bash precmd_functions; ConPTY prompt hold); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: s |
| terminal | precmd_secret_honor_ps1 | FAIL | [hp0] the PIN did not reach `read -s` whole and in order: pp_hp0_pin.txt holds ['\x10pint42792p31d'], want ['pint42792p31d'] (keys went to t |
| terminal | md_viewer | PASS | injector accounting: injector sent 103; app saw 103; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 98 unbound keydown/typed, 3 |
| terminal | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal | keyring_unavailable_startup | PASS | injector accounting: injector sent 595; app saw 595; lost-before-app 0 (Enter: sent 13, app saw 13); of the app's: 577 unbound keydown/typed |
| terminal | claude_plugin_offline | PASS | bundle: /home/<user>/builds/r_hQtIdMf/0/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/claude_plugin; networ |
| terminal | agent_plugins_offline | PASS | codex bundle: /home/<user>/builds/r_hQtIdMf/0/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/codex_plugin; n |
| terminal | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 3363; app saw 3363; lost-before-app 0 ( |
| terminal | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows (OCR): tab 2 at (126, 156), tab 3 at (126, 218), tab 4 at (126, |
| terminal | network_gate_switch | NOREF | NO-REFERENCE: UNVERIFIABLE: the denial notice (toast naming the purpose and host), its 'Allow this origin' link and the log row's 'Allow' bu |
| terminal | depth_check_off_before_output_frames | SKIP | root shell pid not recorded after relaunch_by_case: the PID probe got no answer; the depth check is off until the next relaunch |
| terminal | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: visible streaming tab: the app log has no 'wakeup:' line (a build older than the #1979 measurement, or warpui::f |
| terminal | cb_report_problem | PASS | injector accounting: injector sent 22; app saw 22; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 16 unbound keydown/typed, 5 t |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: injector sent 182; app saw 182; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 177 unbound keydown/typed,  |
| agent | fast_typing | PASS | injector accounting: injector sent 2003; app saw 2003; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/ty |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent | tab_enter | PASS | injector accounting: injector sent 470; app saw 470; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 444 unbound keydown/typed |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: |
| agent | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ps_ctrl_scrolllock_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | subshell_sh_probe | PASS | APPCI_SU_SH_USER unset: the `su` to an sh-shelled user leg did not run (needs a passwordless su on this runner); injector accounting: inject |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 256; app saw 256; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 251 unbound keydown/typed,  |
| agent | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 297; app saw 297; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 286 unbound keydown/typed,  |
| agent | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unb |
| agent | subshell_ready_secret_after_prompt | PASS | injector accounting: injector sent 271; app saw 271; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 261 unbound keydown/typed,  |
| agent | subshell_ready_secret_after_probe | PASS | injector accounting: injector sent 274; app saw 274; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 264 unbound keydown/typed,  |
| agent | subshell_sh_probe_empty_ps1 | PASS | injector accounting: injector sent 692; app saw 692; lost-before-app 0 (Enter: sent 21, app saw 21); of the app's: 678 unbound keydown/typed |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 52; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 322; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 311 unbound keydown/typed,  |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 52; lost-bef |
| agent | subshell_ready_zsh_newuser_install | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 260; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 251 unbound keydown/typed,  |
| agent | gitbash_long_osc_reaches_app | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | latency_enter | PASS | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.64 ms (p95 0.71, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 1 (median 231 |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_paint_ab | SKIP | the key frame drawn at the end of the key press (#1848) exists only in the macOS renderer, and kl3 (ScreenCaptureKit) is macOS only; Linux m |
| agent | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.12 / 0.25 (Δ -0.13), fram |
| agent | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 151 unbound keydown/typed,  |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| agent | routing_typo_head | PASS | injector accounting: injector sent 71; app saw 71; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 58 unbound keydown/typed, 9 t |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 219; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 200 unbound keydown/typed,  |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 160; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 143 unbound keydown/typed |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unboun |
| agent | shell_fish | PASS | injector accounting: injector sent 754; app saw 754; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 744 unbound keydown/typed |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; injector accounting: injector  |
| agent | ui_copy_wrapped_output | PASS | injector accounting: injector sent 232; app saw 232; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| agent | ui_copy_wrapped_drag | PASS | app find hook gave no position for '4kpfare' (no `appci_find` line in the app log: an app without the hook, or RUST_LOG without warpui::appc |
| agent | ctrl_c_no_esc_i_local | PASS | injector accounting: injector sent 461; app saw 461; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 423 unbound keydown/typed |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent | precmd_secret_honor_ps1 | FAIL | [hp0] the PIN did not reach `read -s` whole and in order: pp_hp0_pin.txt holds [], want ['pint303x3qgsi'] (keys went to the input editor, st |
| agent | md_viewer | PASS | injector accounting: injector sent 103; app saw 103; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 98 unbound keydown/typed, 3 |
| agent | keyring_unavailable_startup | PASS | injector accounting: injector sent 545; app saw 545; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 528 unbound keydown/typed |
| agent | agent_view_not_restored_after_keyring_unavailable_startup | PASS | keyring_unavailable_startup relaunched the app itself (pid 323563 -> 324972); a relaunched app starts in a terminal session, so this is not  |
| agent | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 3363; app saw 3363; lost-before-app 0 ( |
| agent | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: the panel rows of tabs 2..4 could not be read by OCR (found  |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 37.7 frames/s for 33.2 wakeups/s (0 Full); 4 hidden tabs streaming 1.8 frames/s, 128.0 hidden  |
| agent@fake | agent_interactive_password | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_accept | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; partial accept: the diff c |
| agent@fake | agent_edit_reject | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; reject: no follow-up model |
| agent@fake | agent_edit_conflict | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_touch_only | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_endings | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_vim | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_vim: hand-over line in  |
| agent@fake | agent_restart_quit_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_stream: the comple |
| agent@fake | agent_restart_quit_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_queued: the comple |
| agent@fake | agent_restart_quit_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_blocked: the compl |
| agent@fake | agent_question_answer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer_at_end | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_ssh | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; NO-REFERENCE: UNVERIFIABLE |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@fake | agent_restart_kill_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_less: hand-over line in |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | PASS | injector accounting: injector sent 5; app saw 5; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 5 take |
| agent@fake | agent_gate_deny_allow | NOREF | NO-REFERENCE: UNVERIFIABLE: pointer Allow: the page or its Allow button was not found by OCR (shots/agdn_ptr_allow.png); injector accounting |
| agent@fake | agent_gate_off_live | PASS | never uploaded: the whole-case strace oracle (NetOracle) judges every connect/sendto/sendmsg destination of this case; injector accounting:  |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | PASS | injector accounting: injector sent 88; app saw 88; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 79 unbound keydown/typed, 7 t |
| agent@fake | agent_multitab_streaming | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 3. |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine; injector accounting: injector sent 1 keys (0 Enter); ap |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | injector accounting: injector sent 225; app saw 225; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 220 unbound keydown/typed,  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | injector accounting: injector sent 2277; app saw 2277; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2215 unbound keydown/ty |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal | tab_enter | PASS | injector accounting: injector sent 686; app saw 686; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 660 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 11.12, -> LineEditorReading 58.06; app |
| terminal | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal | ps_ctrl_break | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 735 unbound keydown/typed,  |
| terminal | ps_ctrl_scrolllock_break | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 735 unbound keydown/typed,  |
| terminal | ps_fast_command_young_enter | PASS | injector accounting: injector sent 772; app saw 772; lost-before-app 0 (Enter: sent 24, app saw 24); of the app's: 744 unbound keydown/typed |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| terminal | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| terminal | gitbash_long_osc_reaches_app | SKIP | not run in the PowerShell pane: an msys program printing a long escape sequence to the pane's console (#2007): bash syntax and the msys cons |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| terminal | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.77 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| terminal | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.08 / 0.19 (Δ -0.11), f |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| terminal | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| terminal | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| terminal | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane (#1933): only the Git Bash pane has the wrapper |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| terminal | ui_copy_wrapped_drag | SKIP | not run on Windows: #1976: no pointer synthesis in the Windows backend yet (the keyboard half, ui_copy_wrapped_output, runs in the Git Bash  |
| terminal | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 2; lost-bef |
| terminal | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal | ps_read_host_secure | PASS | injector accounting: injector sent 568; app saw 568; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 562 unbound keydown/typed,  |
| terminal | gitbash_precmd_secret | SKIP | not run in the PowerShell pane: bash precmd_functions with `read -s` after the prompt's Precmd, rec.log to prove the PIN was not run as a co |
| terminal | precmd_secret_honor_ps1 | SKIP | not run in the PowerShell pane: a bash precmd function's `read -s`, with the app's prompt and with honor PS1 on (relaunches the app with ter |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal | ui_vertical_tabs_many | SKIP | not run on Windows: #1990: not yet ported: switches the panel on by rewriting the settings file while the app runs (as ui_settings_live), an |
| terminal | ui_vertical_tabs_panel_click | SKIP | not run on Windows: #1990: no pointer synthesis in the Windows backend yet, and the settings rewrite of ui_vertical_tabs_many |
| terminal | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: visible streaming tab: the app log has no 'wakeup:' line (a build older than the #1979 measurement, or warpui::f |
| terminal | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine; injector accounting: injector sent 1 keys (0 Enter); ap |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 223; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 218 unbound keydown/typed,  |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | injector accounting: injector sent 2060; app saw 2060; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1998 unbound keydown/ty |
| terminal@gitbash | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 3 unbound keydown/typed, 8 ta |
| terminal@gitbash | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| terminal@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's ms |
| terminal@gitbash | ps_ctrl_scrolllock_break | SKIP | not run in the Git Bash pane: #1906: Ctrl+ScrollLock (the other VK_CANCEL key) to the same SIGBREAK receiver; the Git Bash pane's msys signa |
| terminal@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 261; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 253 unbound keydown/typed,  |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 260; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 252 unbound keydown/typed,  |
| terminal@gitbash | gitbash_long_osc_reaches_app | PASS | the app parsed both hooks: an escape sequence over 256 bytes printed by an msys program in the pane arrives whole; injector accounting: inje |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T181_sp56xu never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal@gitbash | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T186_5bg1vv never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal@gitbash | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T191_kr7tz8 never reached the REMOTE file (s |
| terminal@gitbash | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 2; lost-before |
| terminal@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 2; lost-before |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 42.1 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.69 ms (p95 0.89, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 0 (median 0 u |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal@gitbash | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| terminal@gitbash | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.07 / 0.2 (Δ -0.13), fr |
| terminal@gitbash | routing_table | PASS | injector accounting: injector sent 645; app saw 645; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal@gitbash | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 222; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 200 unbound keydown/typed,  |
| terminal@gitbash | remote_tab_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ZDOTDIR:-$HOME} ZDOTDIR=$WARP_TMP_ |
| terminal@gitbash | pane_not_local_after_remote_tab_no_helper | FAIL | after remote_tab_no_helper the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ZDOTDIR:-$HOME} ZDOTDIR=$WARP_TMP_ |
| terminal@gitbash | pane_not_local_after_remote_tab_enter_no_helper | FAIL | after remote_tab_enter_no_helper the pane could not be proven back in the local shell (none); the app is relaunched so later cases start cle |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_managed | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ZDOTDIR:-$HOME} ZDOTDIR=$WARP_TMP_ |
| terminal@gitbash | pane_not_local_after_ssh_gitbash_managed | FAIL | after ssh_gitbash_managed the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: the line typed during the connect never reached the remote shell. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ |
| terminal@gitbash | pane_not_local_after_ssh_gitbash_typeahead_managed | FAIL | after ssh_gitbash_typeahead_managed the pane could not be proven back in the local shell (none); the app is relaunched so later cases start  |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 178; app saw 178; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 171 unbound keydown/typed,  |
| terminal@gitbash | ui_completions_as_you_type_local | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked; injector accounting: injector sent 16; a |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | ui_copy_wrapped_output | PASS | injector accounting: injector sent 233; app saw 233; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| terminal@gitbash | ui_copy_wrapped_drag | SKIP | not run on Windows: #1976: no pointer synthesis in the Windows backend yet (the keyboard half, ui_copy_wrapped_output, runs in the Git Bash  |
| terminal@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 2; lost-bef |
| terminal@gitbash | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal@gitbash | ps_read_host_secure | SKIP | not run in the Git Bash pane: #1882/#1891: Read-Host -AsSecureString is PowerShell; the Git Bash twin is gitbash_precmd_secret |
| terminal@gitbash | gitbash_precmd_secret | PASS | injector accounting: injector sent 227; app saw 227; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 221 unbound keydown/typed,  |
| terminal@gitbash | precmd_secret_honor_ps1 | PASS | [hp0] no OCR engine: whether the question is drawn is in the screenshot pp_hp0_asked only; [hp1] no OCR engine: whether the question is draw |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot sh |
| terminal@gitbash | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| terminal@gitbash | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal@gitbash | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and writ |
| terminal@gitbash | ui_vertical_tabs_many | SKIP | not run on Windows: #1990: not yet ported: switches the panel on by rewriting the settings file while the app runs (as ui_settings_live), an |
| terminal@gitbash | ui_vertical_tabs_panel_click | SKIP | not run on Windows: #1990: no pointer synthesis in the Windows backend yet, and the settings rewrite of ui_vertical_tabs_many |
| terminal@gitbash | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: visible streaming tab: the app log has no 'wakeup:' line (a build older than the #1979 measurement, or warpui::f |
| terminal@gitbash | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case l |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | injector accounting: injector sent 1106; app saw 1106; lost-before-app 0 (Enter: sent 30, app saw 30); of the app's: 1074 unbound keydown/ty |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent | tab_enter | PASS | injector accounting: injector sent 674; app saw 674; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 648 unbound keydown/typed |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.74, -> LineEditorReading 72.49; app: |
| agent | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent | ps_ctrl_break | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 735 unbound keydown/typed,  |
| agent | ps_ctrl_scrolllock_break | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 735 unbound keydown/typed,  |
| agent | ps_fast_command_young_enter | PASS | injector accounting: injector sent 748; app saw 748; lost-before-app 0 (Enter: sent 24, app saw 24); of the app's: 720 unbound keydown/typed |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| agent | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| agent | gitbash_long_osc_reaches_app | SKIP | not run in the PowerShell pane: an msys program printing a long escape sequence to the pane's console (#2007): bash syntax and the msys cons |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| agent | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it |
| agent | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| agent | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.14 / 0.24 (Δ -0.1), frame |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git  |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | routing_typo_head | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log (#1980, agent mode only): runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| agent | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| agent | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| agent | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| agent | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane (#1933): only the Git Bash pane has the wrapper |
| agent | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| agent | ui_copy_wrapped_drag | SKIP | not run on Windows: #1976: no pointer synthesis in the Windows backend yet (the keyboard half, ui_copy_wrapped_output, runs in the Git Bash  |
| agent | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 2; lost-bef |
| agent | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent | precmd_secret_honor_ps1 | SKIP | not run in the PowerShell pane: a bash precmd function's `read -s`, with the app's prompt and with honor PS1 on (relaunches the app with ter |
| agent | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent | ui_vertical_tabs_many | SKIP | not run on Windows: #1990: not yet ported: switches the panel on by rewriting the settings file while the app runs (as ui_settings_live), an |
| agent | ui_vertical_tabs_panel_click | SKIP | not run on Windows: #1990: no pointer synthesis in the Windows backend yet, and the settings rewrite of ui_vertical_tabs_many |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 48.3 frames/s for 46.7 wakeups/s (0 Full); 4 hidden tabs streaming 2.7 frames/s, 185.5 hidden  |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | injector accounting: injector sent 985; app saw 985; lost-before-app 0 (Enter: sent 30, app saw 30); of the app's: 953 unbound keydown/typed |
| agent@gitbash | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 3 unbound keydown/typed, 8 ta |
| agent@gitbash | tab_enter | PASS | injector accounting: injector sent 470; app saw 470; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 444 unbound keydown/typed |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's ms |
| agent@gitbash | ps_ctrl_scrolllock_break | SKIP | not run in the Git Bash pane: #1906: Ctrl+ScrollLock (the other VK_CANCEL key) to the same SIGBREAK receiver; the Git Bash pane's msys signa |
| agent@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | subshell_ready_rc_questions | PASS | injector accounting: injector sent 256; app saw 256; lost-before-app 0 (Enter: sent 9, app saw 9); of the app's: 248 unbound keydown/typed,  |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | FAIL | the first inner line ran 3.75 s after its Enter (budget 2 s); injector accounting: injector sent 255; app saw 255; lost-before-app 0 (Enter: |
| agent@gitbash | gitbash_long_osc_reaches_app | PASS | the app parsed both hooks: an escape sequence over 256 bytes printed by an msys program in the pane arrives whole; injector accounting: inje |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T67_if2py1 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent@gitbash | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T72_0hsrpy never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent@gitbash | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T77_lsj8mr never reached the REMOTE file (ss |
| agent@gitbash | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 2; app saw 2; lost-before |
| agent@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884); injector accounting: injector sent 1; app saw 1; lost-before |
| agent@gitbash | latency_paint_ab | SKIP | not run on Windows: #1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only) |
| agent@gitbash | latency_key_views_ab | PASS | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.2 / 0.25 (Δ -0.05), frame |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while |
| agent@gitbash | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 151 unbound keydown/typed,  |
| agent@gitbash | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent@gitbash | routing_typo_head | PASS | injector accounting: injector sent 71; app saw 71; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 58 unbound keydown/typed, 9 t |
| agent@gitbash | agent_mode_switch | PASS | injector accounting: injector sent 219; app saw 219; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 200 unbound keydown/typed,  |
| agent@gitbash | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent@gitbash | remote_tab_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: 10:53:53 conpty exec pid 5904 / 10:53:53 conpty first output after 0.01 s: 4 b |
| agent@gitbash | remote_tab_enter_no_helper | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ZDOTDIR:-$HOME} ZDOTDIR=$WARP_TMP_ |
| agent@gitbash | pane_not_local_after_remote_tab_enter_no_helper | FAIL | after remote_tab_enter_no_helper the pane could not be proven back in the local shell (none); the app is relaunched so later cases start cle |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_managed | FAIL | no remote shell prompt after `ssh fake`. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ZDOTDIR:-$HOME} ZDOTDIR=$WARP_TMP_ |
| agent@gitbash | pane_not_local_after_ssh_gitbash_managed | FAIL | after ssh_gitbash_managed the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: the line typed during the connect never reached the remote shell. Test host: sshd.log: TMPPREFIX=$HOME/.zshtmp- WARP_SSH_RCFILES=${ |
| agent@gitbash | pane_not_local_after_ssh_gitbash_typeahead_managed | FAIL | after ssh_gitbash_typeahead_managed the pane could not be proven back in the local shell (none); the app is relaunched so later cases start  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 178; app saw 178; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 171 unbound keydown/typed,  |
| agent@gitbash | ui_copy_wrapped_output | PASS | injector accounting: injector sent 233; app saw 233; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| agent@gitbash | ui_copy_wrapped_drag | SKIP | not run on Windows: #1976: no pointer synthesis in the Windows backend yet (the keyboard half, ui_copy_wrapped_output, runs in the Git Bash  |
| agent@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log; injector accounting: injector sent 2; app saw 2; lost-bef |
| agent@gitbash | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent@gitbash | precmd_secret_honor_ps1 | PASS | [hp0] no OCR engine: whether the question is drawn is in the screenshot pp_hp0_asked only; [hp1] no OCR engine: whether the question is draw |
| agent@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI fil |
| agent@gitbash | ui_vertical_tabs_many | SKIP | not run on Windows: #1990: not yet ported: switches the panel on by rewriting the settings file while the app runs (as ui_settings_live), an |
| agent@gitbash | ui_vertical_tabs_panel_click | SKIP | not run on Windows: #1990: no pointer synthesis in the Windows backend yet, and the settings rewrite of ui_vertical_tabs_many |
| agent@gitbash | output_frames | FAIL | 4 hidden streaming tabs: 7.1 in-place re-renders of hidden views per tab per second (limit 5; views: Terminal x67, DisplayChip x38, AgentVie |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_interactive_password | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | SKIP | not run in the PowerShell pane: its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake mode |
| agent@fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@fake | agent_question_answer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR on this runner: the |
| agent@fake | agent_question_steer_at_end | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_enter | SKIP | not run in the PowerShell pane: its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@fake | agent_restart_kill_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@fake | agent_steer_fast | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_less | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | agent_noninteractive_no_takeover | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@fake | network_gate_notice | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| agent@fake | agent_gate_deny_allow | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent@fake | agent_gate_off_live | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent@fake | agent_mcp_faults | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does n |
| agent@fake | agent_gate_key_never_leaves | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent@fake | agent_multitab_streaming | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Lin |
| agent@gitbash+fake | agent_interactive_password | SKIP | not run in the Git Bash pane: #1882: the hand-over on ConPTY is the PowerShell bootstrap's Read-Host marker; bash `read -s` in a Git Bash pa |
| agent@gitbash+fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@gitbash+fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error b |
| agent@gitbash+fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@gitbash+fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@gitbash+fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows); injector accou |
| agent@gitbash+fake | agent_question_answer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_question_after_command | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR on this runner: the |
| agent@gitbash+fake | agent_question_steer_at_end | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@gitbash+fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@gitbash+fake | agent_restart_kill_queued | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@gitbash+fake | agent_restart_kill_blocked | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@gitbash+fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@gitbash+fake | agent_interactive_less | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | agent_noninteractive_no_takeover | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windo |
| agent@gitbash+fake | network_gate_notice | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| agent@gitbash+fake | agent_gate_deny_allow | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent@gitbash+fake | agent_gate_off_live | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent@gitbash+fake | agent_mcp_faults | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does n |
| agent@gitbash+fake | agent_gate_key_never_leaves | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent@gitbash+fake | agent_multitab_streaming | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Lin |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_edit_accept | macos | agent@fake | PASS |
| agent_edit_accept | wayland | agent@fake | PASS |
| agent_edit_accept | windows | agent@fake | SKIP |
| agent_edit_accept | windows | agent@gitbash+fake | SKIP |
| agent_edit_conflict | macos | agent@fake | PASS |
| agent_edit_conflict | wayland | agent@fake | PASS |
| agent_edit_conflict | windows | agent@fake | SKIP |
| agent_edit_conflict | windows | agent@gitbash+fake | SKIP |
| agent_edit_endings | macos | agent@fake | PASS |
| agent_edit_endings | wayland | agent@fake | PASS |
| agent_edit_endings | windows | agent@fake | SKIP |
| agent_edit_endings | windows | agent@gitbash+fake | SKIP |
| agent_edit_reject | macos | agent@fake | PASS |
| agent_edit_reject | wayland | agent@fake | PASS |
| agent_edit_reject | windows | agent@fake | SKIP |
| agent_edit_reject | windows | agent@gitbash+fake | SKIP |
| agent_edit_touch_only | macos | agent@fake | PASS |
| agent_edit_touch_only | wayland | agent@fake | PASS |
| agent_edit_touch_only | windows | agent@fake | SKIP |
| agent_edit_touch_only | windows | agent@gitbash+fake | SKIP |
| agent_fault_429 | macos | agent@fake | PASS |
| agent_fault_429 | wayland | agent@fake | PASS |
| agent_fault_429 | windows | agent@fake | PASS |
| agent_fault_429 | windows | agent@gitbash+fake | PASS |
| agent_fault_500 | macos | agent@fake | PASS |
| agent_fault_500 | wayland | agent@fake | PASS |
| agent_fault_500 | windows | agent@fake | PASS |
| agent_fault_500 | windows | agent@gitbash+fake | PASS |
| agent_fault_disconnect | macos | agent@fake | PASS |
| agent_fault_disconnect | wayland | agent@fake | PASS |
| agent_fault_disconnect | windows | agent@fake | PASS |
| agent_fault_disconnect | windows | agent@gitbash+fake | PASS |
| agent_fault_down | macos | agent@fake | PASS |
| agent_fault_down | wayland | agent@fake | PASS |
| agent_fault_down | windows | agent@fake | PASS |
| agent_fault_down | windows | agent@gitbash+fake | PASS |
| agent_fault_queued_rows | macos | agent@fake | PASS |
| agent_fault_queued_rows | wayland | agent@fake | PASS |
| agent_fault_queued_rows | windows | agent@fake | PASS |
| agent_fault_queued_rows | windows | agent@gitbash+fake | PASS |
| agent_fault_slow_cancel | macos | agent@fake | PASS |
| agent_fault_slow_cancel | wayland | agent@fake | PASS |
| agent_fault_slow_cancel | windows | agent@fake | PASS |
| agent_fault_slow_cancel | windows | agent@gitbash+fake | PASS |
| agent_gate_deny_allow | macos | agent@fake | SKIP |
| agent_gate_deny_allow | wayland | agent@fake | NOREF |
| agent_gate_deny_allow | windows | agent@fake | SKIP |
| agent_gate_deny_allow | windows | agent@gitbash+fake | SKIP |
| agent_gate_key_never_leaves | macos | agent@fake | SKIP |
| agent_gate_key_never_leaves | wayland | agent@fake | PASS |
| agent_gate_key_never_leaves | windows | agent@fake | SKIP |
| agent_gate_key_never_leaves | windows | agent@gitbash+fake | SKIP |
| agent_gate_off_live | macos | agent@fake | SKIP |
| agent_gate_off_live | wayland | agent@fake | PASS |
| agent_gate_off_live | windows | agent@fake | SKIP |
| agent_gate_off_live | windows | agent@gitbash+fake | SKIP |
| agent_interactive_less | macos | agent@fake | PASS |
| agent_interactive_less | wayland | agent@fake | PASS |
| agent_interactive_less | windows | agent@fake | SKIP |
| agent_interactive_less | windows | agent@gitbash+fake | SKIP |
| agent_interactive_password | macos | agent@fake | PASS |
| agent_interactive_password | wayland | agent@fake | PASS |
| agent_interactive_password | windows | agent@fake | PASS |
| agent_interactive_password | windows | agent@gitbash+fake | SKIP |
| agent_interactive_ssh | macos | agent@fake | NOREF |
| agent_interactive_ssh | wayland | agent@fake | NOREF |
| agent_interactive_ssh | windows | agent@fake | SKIP |
| agent_interactive_ssh | windows | agent@gitbash+fake | SKIP |
| agent_interactive_vim | macos | agent@fake | PASS |
| agent_interactive_vim | wayland | agent@fake | PASS |
| agent_interactive_vim | windows | agent@fake | SKIP |
| agent_interactive_vim | windows | agent@gitbash+fake | SKIP |
| agent_latency_enter | macos | agent@fake | PASS |
| agent_latency_enter | wayland | agent@fake | PASS |
| agent_latency_enter | windows | agent@fake | SKIP |
| agent_latency_enter | windows | agent@gitbash+fake | PASS |
| agent_latency_key | macos | agent@fake | PASS |
| agent_latency_key | wayland | agent@fake | PASS |
| agent_latency_key | windows | agent@fake | SKIP |
| agent_latency_key | windows | agent@gitbash+fake | PASS |
| agent_lrc_no_false_takeover | macos | agent | PASS |
| agent_lrc_no_false_takeover | wayland | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent@gitbash | PASS |
| agent_mcp_faults | macos | agent@fake | SKIP |
| agent_mcp_faults | wayland | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@fake | SKIP |
| agent_mcp_faults | windows | agent@gitbash+fake | SKIP |
| agent_multitab_streaming | macos | agent@fake | PASS |
| agent_multitab_streaming | wayland | agent@fake | PASS |
| agent_multitab_streaming | windows | agent@fake | SKIP |
| agent_multitab_streaming | windows | agent@gitbash+fake | SKIP |
| agent_noninteractive_no_takeover | macos | agent@fake | PASS |
| agent_noninteractive_no_takeover | wayland | agent@fake | PASS |
| agent_noninteractive_no_takeover | windows | agent@fake | SKIP |
| agent_noninteractive_no_takeover | windows | agent@gitbash+fake | SKIP |
| agent_plugins_offline | macos | terminal | SKIP |
| agent_plugins_offline | wayland | terminal | PASS |
| agent_plugins_offline | windows | terminal | SKIP |
| agent_plugins_offline | windows | terminal@gitbash | SKIP |
| agent_question_after_command | macos | agent@fake | PASS |
| agent_question_after_command | wayland | agent@fake | PASS |
| agent_question_after_command | windows | agent@fake | PASS |
| agent_question_after_command | windows | agent@gitbash+fake | PASS |
| agent_question_answer | macos | agent@fake | PASS |
| agent_question_answer | wayland | agent@fake | PASS |
| agent_question_answer | windows | agent@fake | PASS |
| agent_question_answer | windows | agent@gitbash+fake | PASS |
| agent_question_ctrl_c | macos | agent@fake | PASS |
| agent_question_ctrl_c | wayland | agent@fake | PASS |
| agent_question_ctrl_c | windows | agent@fake | PASS |
| agent_question_ctrl_c | windows | agent@gitbash+fake | PASS |
| agent_question_cut | macos | agent@fake | PASS |
| agent_question_cut | wayland | agent@fake | PASS |
| agent_question_cut | windows | agent@fake | PASS |
| agent_question_cut | windows | agent@gitbash+fake | PASS |
| agent_question_empty | macos | agent@fake | PASS |
| agent_question_empty | wayland | agent@fake | PASS |
| agent_question_empty | windows | agent@fake | PASS |
| agent_question_empty | windows | agent@gitbash+fake | PASS |
| agent_question_nodone | macos | agent@fake | PASS |
| agent_question_nodone | wayland | agent@fake | PASS |
| agent_question_nodone | windows | agent@fake | PASS |
| agent_question_nodone | windows | agent@gitbash+fake | PASS |
| agent_question_slow | macos | agent@fake | PASS |
| agent_question_slow | wayland | agent@fake | PASS |
| agent_question_slow | windows | agent@fake | PASS |
| agent_question_slow | windows | agent@gitbash+fake | PASS |
| agent_question_steer | macos | agent@fake | PASS |
| agent_question_steer | wayland | agent@fake | PASS |
| agent_question_steer | windows | agent@fake | PASS |
| agent_question_steer | windows | agent@gitbash+fake | PASS |
| agent_question_steer_at_end | macos | agent@fake | PASS |
| agent_question_steer_at_end | wayland | agent@fake | PASS |
| agent_question_steer_at_end | windows | agent@fake | PASS |
| agent_question_steer_at_end | windows | agent@gitbash+fake | PASS |
| agent_question_trunc | macos | agent@fake | PASS |
| agent_question_trunc | wayland | agent@fake | PASS |
| agent_question_trunc | windows | agent@fake | PASS |
| agent_question_trunc | windows | agent@gitbash+fake | PASS |
| agent_restart_kill_blocked | macos | agent@fake | PASS |
| agent_restart_kill_blocked | wayland | agent@fake | PASS |
| agent_restart_kill_blocked | windows | agent@fake | PASS |
| agent_restart_kill_blocked | windows | agent@gitbash+fake | PASS |
| agent_restart_kill_queued | macos | agent@fake | PASS |
| agent_restart_kill_queued | wayland | agent@fake | PASS |
| agent_restart_kill_queued | windows | agent@fake | PASS |
| agent_restart_kill_queued | windows | agent@gitbash+fake | PASS |
| agent_restart_kill_stream | macos | agent@fake | PASS |
| agent_restart_kill_stream | wayland | agent@fake | PASS |
| agent_restart_kill_stream | windows | agent@fake | PASS |
| agent_restart_kill_stream | windows | agent@gitbash+fake | PASS |
| agent_restart_quit_blocked | macos | agent@fake | SKIP |
| agent_restart_quit_blocked | wayland | agent@fake | PASS |
| agent_restart_quit_blocked | windows | agent@fake | SKIP |
| agent_restart_quit_blocked | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_queued | macos | agent@fake | SKIP |
| agent_restart_quit_queued | wayland | agent@fake | PASS |
| agent_restart_quit_queued | windows | agent@fake | SKIP |
| agent_restart_quit_queued | windows | agent@gitbash+fake | SKIP |
| agent_restart_quit_stream | macos | agent@fake | SKIP |
| agent_restart_quit_stream | wayland | agent@fake | PASS |
| agent_restart_quit_stream | windows | agent@fake | SKIP |
| agent_restart_quit_stream | windows | agent@gitbash+fake | SKIP |
| agent_steer_fast | macos | agent@fake | PASS |
| agent_steer_fast | wayland | agent@fake | PASS |
| agent_steer_fast | windows | agent@fake | SKIP |
| agent_steer_fast | windows | agent@gitbash+fake | PASS |
| cb_report_problem | macos | terminal | PASS |
| cb_report_problem | wayland | terminal | PASS |
| cb_report_problem | windows | terminal | SKIP |
| cb_report_problem | windows | terminal@gitbash | SKIP |
| claude_plugin_offline | macos | terminal | SKIP |
| claude_plugin_offline | wayland | terminal | PASS |
| claude_plugin_offline | windows | terminal | SKIP |
| claude_plugin_offline | windows | terminal@gitbash | SKIP |
| ctrl_c_flood_ab | macos | agent | PASS |
| ctrl_c_flood_ab | macos | terminal | PASS |
| ctrl_c_flood_ab | wayland | agent | SKIP |
| ctrl_c_flood_ab | wayland | terminal | SKIP |
| ctrl_c_flood_ab | windows | agent | SKIP |
| ctrl_c_flood_ab | windows | agent@gitbash | SKIP |
| ctrl_c_flood_ab | windows | terminal | SKIP |
| ctrl_c_flood_ab | windows | terminal@gitbash | SKIP |
| ctrl_c_no_esc_i_local | macos | agent | PASS |
| ctrl_c_no_esc_i_local | macos | terminal | PASS |
| ctrl_c_no_esc_i_local | wayland | agent | PASS |
| ctrl_c_no_esc_i_local | wayland | terminal | PASS |
| ctrl_c_no_esc_i_local | windows | agent | SKIP |
| ctrl_c_no_esc_i_local | windows | agent@gitbash | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal | SKIP |
| ctrl_c_no_esc_i_local | windows | terminal@gitbash | SKIP |
| gitbash_long_osc_reaches_app | macos | agent | SKIP |
| gitbash_long_osc_reaches_app | macos | terminal | SKIP |
| gitbash_long_osc_reaches_app | wayland | agent | SKIP |
| gitbash_long_osc_reaches_app | wayland | terminal | SKIP |
| gitbash_long_osc_reaches_app | windows | agent | SKIP |
| gitbash_long_osc_reaches_app | windows | agent@gitbash | PASS |
| gitbash_long_osc_reaches_app | windows | terminal | SKIP |
| gitbash_long_osc_reaches_app | windows | terminal@gitbash | PASS |
| gitbash_precmd_secret | macos | terminal | SKIP |
| gitbash_precmd_secret | wayland | terminal | SKIP |
| gitbash_precmd_secret | windows | terminal | SKIP |
| gitbash_precmd_secret | windows | terminal@gitbash | PASS |
| giveup_keys_reach_shell | macos | agent | PASS |
| giveup_keys_reach_shell | macos | terminal | PASS |
| giveup_keys_reach_shell | wayland | agent | PASS |
| giveup_keys_reach_shell | wayland | terminal | PASS |
| giveup_keys_reach_shell | windows | agent | PASS |
| giveup_keys_reach_shell | windows | agent@gitbash | PASS |
| giveup_keys_reach_shell | windows | terminal | PASS |
| giveup_keys_reach_shell | windows | terminal@gitbash | PASS |
| keyring_unavailable_startup | macos | agent | SKIP |
| keyring_unavailable_startup | macos | terminal | SKIP |
| keyring_unavailable_startup | wayland | agent | PASS |
| keyring_unavailable_startup | wayland | terminal | PASS |
| keyring_unavailable_startup | windows | agent | SKIP |
| keyring_unavailable_startup | windows | agent@gitbash | SKIP |
| keyring_unavailable_startup | windows | terminal | SKIP |
| keyring_unavailable_startup | windows | terminal@gitbash | SKIP |
| latency_key_views_ab | macos | agent | PASS |
| latency_key_views_ab | macos | terminal | PASS |
| latency_key_views_ab | wayland | agent | PASS |
| latency_key_views_ab | wayland | terminal | PASS |
| latency_key_views_ab | windows | agent | PASS |
| latency_key_views_ab | windows | agent@gitbash | PASS |
| latency_key_views_ab | windows | terminal | PASS |
| latency_key_views_ab | windows | terminal@gitbash | PASS |
| latency_paint_ab | macos | agent | PASS |
| latency_paint_ab | macos | terminal | PASS |
| latency_paint_ab | wayland | agent | SKIP |
| latency_paint_ab | wayland | terminal | SKIP |
| latency_paint_ab | windows | agent | SKIP |
| latency_paint_ab | windows | agent@gitbash | SKIP |
| latency_paint_ab | windows | terminal | SKIP |
| latency_paint_ab | windows | terminal@gitbash | SKIP |
| network_gate_notice | macos | agent@fake | SKIP |
| network_gate_notice | wayland | agent@fake | PASS |
| network_gate_notice | windows | agent@fake | SKIP |
| network_gate_notice | windows | agent@gitbash+fake | SKIP |
| network_gate_switch | macos | terminal | SKIP |
| network_gate_switch | wayland | terminal | NOREF |
| network_gate_switch | windows | terminal | SKIP |
| network_gate_switch | windows | terminal@gitbash | SKIP |
| output_frames | macos | agent | PASS |
| output_frames | macos | terminal | PASS |
| output_frames | wayland | agent | PASS |
| output_frames | wayland | terminal | NOREF |
| output_frames | windows | agent | PASS |
| output_frames | windows | agent@gitbash | FAIL |
| output_frames | windows | terminal | NOREF |
| output_frames | windows | terminal@gitbash | NOREF |
| precmd_secret_honor_ps1 | macos | agent | NOREF |
| precmd_secret_honor_ps1 | macos | terminal | NOREF |
| precmd_secret_honor_ps1 | wayland | agent | FAIL |
| precmd_secret_honor_ps1 | wayland | terminal | FAIL |
| precmd_secret_honor_ps1 | windows | agent | SKIP |
| precmd_secret_honor_ps1 | windows | agent@gitbash | PASS |
| precmd_secret_honor_ps1 | windows | terminal | SKIP |
| precmd_secret_honor_ps1 | windows | terminal@gitbash | PASS |
| ps_ctrl_break | macos | agent | SKIP |
| ps_ctrl_break | macos | terminal | SKIP |
| ps_ctrl_break | wayland | agent | SKIP |
| ps_ctrl_break | wayland | terminal | SKIP |
| ps_ctrl_break | windows | agent | PASS |
| ps_ctrl_break | windows | agent@gitbash | SKIP |
| ps_ctrl_break | windows | terminal | PASS |
| ps_ctrl_break | windows | terminal@gitbash | SKIP |
| ps_ctrl_scrolllock_break | macos | agent | SKIP |
| ps_ctrl_scrolllock_break | macos | terminal | SKIP |
| ps_ctrl_scrolllock_break | wayland | agent | SKIP |
| ps_ctrl_scrolllock_break | wayland | terminal | SKIP |
| ps_ctrl_scrolllock_break | windows | agent | PASS |
| ps_ctrl_scrolllock_break | windows | agent@gitbash | SKIP |
| ps_ctrl_scrolllock_break | windows | terminal | PASS |
| ps_ctrl_scrolllock_break | windows | terminal@gitbash | SKIP |
| ps_fast_command_young_enter | macos | agent | SKIP |
| ps_fast_command_young_enter | macos | terminal | SKIP |
| ps_fast_command_young_enter | wayland | agent | SKIP |
| ps_fast_command_young_enter | wayland | terminal | SKIP |
| ps_fast_command_young_enter | windows | agent | PASS |
| ps_fast_command_young_enter | windows | agent@gitbash | SKIP |
| ps_fast_command_young_enter | windows | terminal | PASS |
| ps_fast_command_young_enter | windows | terminal@gitbash | SKIP |
| ps_read_host_secure | macos | terminal | SKIP |
| ps_read_host_secure | wayland | terminal | SKIP |
| ps_read_host_secure | windows | terminal | PASS |
| ps_read_host_secure | windows | terminal@gitbash | SKIP |
| remote_tab_enter_no_helper | macos | agent | PASS |
| remote_tab_enter_no_helper | macos | terminal | PASS |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | PASS |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | FAIL |
| remote_tab_no_helper | macos | agent | PASS |
| remote_tab_no_helper | macos | terminal | PASS |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | PASS |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| routing_typo_head | macos | agent | PASS |
| routing_typo_head | wayland | agent | PASS |
| routing_typo_head | windows | agent | SKIP |
| routing_typo_head | windows | agent@gitbash | PASS |
| ssh_connect_typing_wrapper | macos | agent | SKIP |
| ssh_connect_typing_wrapper | macos | terminal | SKIP |
| ssh_connect_typing_wrapper | wayland | agent | SKIP |
| ssh_connect_typing_wrapper | wayland | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | agent | SKIP |
| ssh_connect_typing_wrapper | windows | agent@gitbash | SKIP |
| ssh_connect_typing_wrapper | windows | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | SKIP |
| ssh_gitbash_kill_switch | macos | agent | SKIP |
| ssh_gitbash_kill_switch | macos | terminal | SKIP |
| ssh_gitbash_kill_switch | wayland | agent | SKIP |
| ssh_gitbash_kill_switch | wayland | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | agent | SKIP |
| ssh_gitbash_kill_switch | windows | agent@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | terminal@gitbash | PASS |
| ssh_gitbash_managed | macos | agent | SKIP |
| ssh_gitbash_managed | macos | terminal | SKIP |
| ssh_gitbash_managed | wayland | agent | SKIP |
| ssh_gitbash_managed | wayland | terminal | SKIP |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | macos | agent | SKIP |
| ssh_gitbash_typeahead_managed | macos | terminal | SKIP |
| ssh_gitbash_typeahead_managed | wayland | agent | SKIP |
| ssh_gitbash_typeahead_managed | wayland | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal@gitbash | FAIL |
| ssh_nested_typeahead_wrapper | macos | agent | SKIP |
| ssh_nested_typeahead_wrapper | macos | terminal | SKIP |
| ssh_nested_typeahead_wrapper | wayland | agent | SKIP |
| ssh_nested_typeahead_wrapper | wayland | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent@gitbash | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | macos | agent | SKIP |
| subshell_ready_docker_t_without_i | macos | terminal | SKIP |
| subshell_ready_docker_t_without_i | wayland | agent | SKIP |
| subshell_ready_docker_t_without_i | wayland | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | macos | agent | PASS |
| subshell_ready_exec_fish_rc | macos | terminal | PASS |
| subshell_ready_exec_fish_rc | wayland | agent | PASS |
| subshell_ready_exec_fish_rc | wayland | terminal | PASS |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | macos | agent | PASS |
| subshell_ready_rc_clear | macos | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | agent@gitbash | FAIL |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | PASS |
| subshell_ready_rc_questions | macos | agent | PASS |
| subshell_ready_rc_questions | macos | terminal | PASS |
| subshell_ready_rc_questions | wayland | agent | PASS |
| subshell_ready_rc_questions | wayland | terminal | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | agent@gitbash | PASS |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | PASS |
| subshell_ready_relay_shape | macos | agent | SKIP |
| subshell_ready_relay_shape | macos | terminal | SKIP |
| subshell_ready_relay_shape | wayland | agent | SKIP |
| subshell_ready_relay_shape | wayland | terminal | SKIP |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_probe | macos | agent | PASS |
| subshell_ready_secret_after_probe | macos | terminal | PASS |
| subshell_ready_secret_after_probe | wayland | agent | PASS |
| subshell_ready_secret_after_probe | wayland | terminal | PASS |
| subshell_ready_secret_after_probe | windows | agent | SKIP |
| subshell_ready_secret_after_probe | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | terminal | SKIP |
| subshell_ready_secret_after_probe | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | macos | agent | PASS |
| subshell_ready_secret_after_prompt | macos | terminal | PASS |
| subshell_ready_secret_after_prompt | wayland | agent | PASS |
| subshell_ready_secret_after_prompt | wayland | terminal | PASS |
| subshell_ready_secret_after_prompt | windows | agent | SKIP |
| subshell_ready_secret_after_prompt | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | macos | agent | PASS |
| subshell_ready_unsupported_tcsh | macos | terminal | PASS |
| subshell_ready_unsupported_tcsh | wayland | agent | SKIP |
| subshell_ready_unsupported_tcsh | wayland | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | macos | agent | PASS |
| subshell_ready_vim_typeahead | macos | terminal | PASS |
| subshell_ready_vim_typeahead | wayland | agent | PASS |
| subshell_ready_vim_typeahead | wayland | terminal | PASS |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | macos | agent | SKIP |
| subshell_ready_zsh_newuser_install | macos | terminal | SKIP |
| subshell_ready_zsh_newuser_install | wayland | agent | PASS |
| subshell_ready_zsh_newuser_install | wayland | terminal | PASS |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | macos | agent | PASS |
| subshell_sh_probe | macos | terminal | PASS |
| subshell_sh_probe | wayland | agent | PASS |
| subshell_sh_probe | wayland | terminal | PASS |
| subshell_sh_probe | windows | agent | SKIP |
| subshell_sh_probe | windows | agent@gitbash | SKIP |
| subshell_sh_probe | windows | terminal | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | macos | agent | PASS |
| subshell_sh_probe_empty_ps1 | macos | terminal | PASS |
| subshell_sh_probe_empty_ps1 | wayland | agent | PASS |
| subshell_sh_probe_empty_ps1 | wayland | terminal | PASS |
| subshell_sh_probe_empty_ps1 | windows | agent | SKIP |
| subshell_sh_probe_empty_ps1 | windows | agent@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | macos | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | NOREF |
| ui_copy_wrapped_drag | macos | agent | SKIP |
| ui_copy_wrapped_drag | macos | terminal | SKIP |
| ui_copy_wrapped_drag | wayland | agent | PASS |
| ui_copy_wrapped_drag | wayland | terminal | PASS |
| ui_copy_wrapped_drag | windows | agent | SKIP |
| ui_copy_wrapped_drag | windows | agent@gitbash | SKIP |
| ui_copy_wrapped_drag | windows | terminal | SKIP |
| ui_copy_wrapped_drag | windows | terminal@gitbash | SKIP |
| ui_copy_wrapped_output | macos | agent | PASS |
| ui_copy_wrapped_output | macos | terminal | PASS |
| ui_copy_wrapped_output | wayland | agent | PASS |
| ui_copy_wrapped_output | wayland | terminal | PASS |
| ui_copy_wrapped_output | windows | agent | SKIP |
| ui_copy_wrapped_output | windows | agent@gitbash | PASS |
| ui_copy_wrapped_output | windows | terminal | SKIP |
| ui_copy_wrapped_output | windows | terminal@gitbash | PASS |
| ui_vertical_tabs_many | macos | agent | PASS |
| ui_vertical_tabs_many | macos | terminal | PASS |
| ui_vertical_tabs_many | wayland | agent | PASS |
| ui_vertical_tabs_many | wayland | terminal | PASS |
| ui_vertical_tabs_many | windows | agent | SKIP |
| ui_vertical_tabs_many | windows | agent@gitbash | SKIP |
| ui_vertical_tabs_many | windows | terminal | SKIP |
| ui_vertical_tabs_many | windows | terminal@gitbash | SKIP |
| ui_vertical_tabs_panel_click | macos | agent | SKIP |
| ui_vertical_tabs_panel_click | macos | terminal | SKIP |
| ui_vertical_tabs_panel_click | wayland | agent | NOREF |
| ui_vertical_tabs_panel_click | wayland | terminal | PASS |
| ui_vertical_tabs_panel_click | windows | agent | SKIP |
| ui_vertical_tabs_panel_click | windows | agent@gitbash | SKIP |
| ui_vertical_tabs_panel_click | windows | terminal | SKIP |
| ui_vertical_tabs_panel_click | windows | terminal@gitbash | SKIP |
| zero_network_startup | macos | terminal | SKIP |
| zero_network_startup | wayland | terminal | PASS |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | SKIP |

passed everywhere it ran (22, candidates for the owner's decision): agent_fault_429, agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_queued_rows, agent_fault_slow_cancel, agent_lrc_no_false_takeover, agent_question_after_command, agent_question_answer, agent_question_ctrl_c, agent_question_cut, agent_question_empty, agent_question_nodone, agent_question_slow, agent_question_steer, agent_question_steer_at_end, agent_question_trunc, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_kill_stream, giveup_keys_reach_shell, latency_key_views_ab
not passed somewhere (fail / skip / no-reference): agent_edit_accept, agent_edit_conflict, agent_edit_endings, agent_edit_reject, agent_edit_touch_only, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_less, agent_interactive_password, agent_interactive_ssh, agent_interactive_vim, agent_latency_enter, agent_latency_key, agent_mcp_faults, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_plugins_offline, agent_restart_quit_blocked, agent_restart_quit_queued, agent_restart_quit_stream, agent_steer_fast, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, gitbash_long_osc_reaches_app, gitbash_precmd_secret, keyring_unavailable_startup, latency_paint_ab, network_gate_notice, network_gate_switch, output_frames, precmd_secret_honor_ps1, ps_ctrl_break, ps_ctrl_scrolllock_break, ps_fast_command_young_enter, ps_read_host_secure, remote_tab_enter_no_helper, remote_tab_no_helper, routing_typo_head, ssh_connect_typing_wrapper, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, subshell_sh_probe_empty_ps1, ui_completions_as_you_type_local, ui_copy_wrapped_drag, ui_copy_wrapped_output, ui_vertical_tabs_many, ui_vertical_tabs_panel_click, zero_network_startup
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 3761 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.log.gz: 4240 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/case_latency_key_views_ab.log: latency evidence capped at 229 KB (1404 of 3967 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/agent@gitbash+fake/case_agent_latency_enter.log: latency evidence capped at 229 KB (1528 of 3902 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/agent@gitbash+fake/case_agent_latency_key.log: latency evidence capped at 229 KB (1703 of 3498 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_key_views_ab.log: latency evidence capped at 229 KB (1393 of 3855 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 229 KB (1414 of 48606 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/terminal/case_latency_key_views_ab.log: latency evidence capped at 229 KB (1415 of 3826 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter.log: latency evidence capped at 229 KB (1563 of 13467 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_key_views_ab.log: latency evidence capped at 229 KB (1341 of 3676 hop lines kept; the platform budget of 2048 KB is shared between 9 logs)
- note: windows/run_slim/logs/agent/acct/launch0_1794335881.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794335881.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794339369.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794339369.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794333213.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794333213.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794333213.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794333213.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.1.log.gz: 1703 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/case_ctrl_c_flood_ab.log: latency evidence capped at 158 KB (1181 of 4176 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 158 KB (965 of 4351 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent/case_latency_key_log.log: latency evidence capped at 158 KB (972 of 1422 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent/case_latency_key_views_ab.log: latency evidence capped at 158 KB (1336 of 5147 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent/case_latency_paint_ab.log: latency evidence capped at 158 KB (1423 of 5036 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 158 KB (1020 of 4260 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 158 KB (1064 of 2753 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/terminal/case_ctrl_c_flood_ab.log: latency evidence capped at 158 KB (1149 of 4104 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 158 KB (965 of 4343 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/terminal/case_latency_key_log.log: latency evidence capped at 158 KB (980 of 1448 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/terminal/case_latency_key_views_ab.log: latency evidence capped at 158 KB (1327 of 5072 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/terminal/case_latency_paint_ab.log: latency evidence capped at 158 KB (1408 of 4926 hop lines kept; the platform budget of 2048 KB is shared between 14 logs)
- note: macos/run_slim/logs/agent/acct/launch0_1794334816.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794336054.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794334816.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794336054.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794336054.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794338458.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794338458.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/agent/acct/launch0_1794338458.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794331750.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794333072.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794333072.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794333072.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794331750.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794333072.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: macos/run_slim/logs/terminal/acct/launch0_1794333867.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 271 KB (1891 of 4258 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: wayland/run_slim/logs/agent/case_latency_key_views_ab.log: latency evidence capped at 271 KB (2826 of 6201 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence capped at 271 KB (1861 of 4020 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence capped at 271 KB (2416 of 4496 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 271 KB (1807 of 14960 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: wayland/run_slim/logs/terminal/case_latency_key_views_ab.log: latency evidence capped at 271 KB (2683 of 5931 hop lines kept; the platform budget of 2048 KB is shared between 8 logs)
- note: wayland/run_slim/logs/agent/acct/launch0_1794337225.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794337225.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794337225.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794337225.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794338798.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794339649.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794335673.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794335136.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794335136.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794335136.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794335136.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794336155.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: run dir is 8.5 MB (> 5 MB soft limit)

## vs previous run

- NEW failures (unit): 
  - `warp ai::agent_providers::tools::web_runtime::websearch_tests::real_exa_anonymous_search [test-linux]`
  - `warp ai::blocklist::agent_fuzz::fuzz_tests::random_agent_conversations_hold_the_invariants [test-linux]`
  - `warp ai::blocklist::agent_fuzz::fuzz_tests::random_agent_conversations_hold_the_invariants_with_the_allowlist_on [test-linux]`
  - `warp terminal::view::tests::a_precmd_functions_question_sends_the_pin_and_its_enter_to_the_read [test-linux]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants [test-linux]`
  - `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants_with_the_allowlist_on [test-linux]`
  - `warp terminal::model::key_facts::tests::a_question_printed_after_the_precmd_is_seen_on_the_grid_the_block_writes_to [test-linux]`
- NEW failures (app): 
  - `wayland/terminal/precmd_secret_honor_ps1 (case not in previous run)`
  - `wayland/agent/precmd_secret_honor_ps1 (case not in previous run)`
  - `windows/terminal@gitbash/ssh_connect_typing`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal@gitbash/remote_tab_no_helper`
  - `windows/terminal@gitbash/pane_not_local_after_remote_tab_no_helper (case not in previous run)`
  - `windows/terminal@gitbash/remote_tab_enter_no_helper`
  - `windows/terminal@gitbash/pane_not_local_after_remote_tab_enter_no_helper (case not in previous run)`
  - `windows/terminal@gitbash/ssh_gitbash_managed`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_gitbash_managed (case not in previous run)`
  - `windows/terminal@gitbash/ssh_gitbash_typeahead_managed`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_gitbash_typeahead_managed (case not in previous run)`
  - `windows/agent@gitbash/subshell_ready_rc_clear`
  - `windows/agent@gitbash/ssh_connect_typing`
  - `windows/agent@gitbash/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/pane_not_local_after_ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/agent@gitbash/ssh_nested_typeahead`
  - `windows/agent@gitbash/pane_not_local_after_ssh_nested_typeahead (case not in previous run)`
  - `windows/agent@gitbash/remote_tab_no_helper`
  - `windows/agent@gitbash/remote_tab_enter_no_helper`
  - `windows/agent@gitbash/pane_not_local_after_remote_tab_enter_no_helper (case not in previous run)`
  - `windows/agent@gitbash/ssh_gitbash_managed`
  - `windows/agent@gitbash/pane_not_local_after_ssh_gitbash_managed (case not in previous run)`
  - `windows/agent@gitbash/ssh_gitbash_typeahead_managed`
  - `windows/agent@gitbash/pane_not_local_after_ssh_gitbash_typeahead_managed (case not in previous run)`
  - `windows/agent@gitbash/output_frames`
- FIXED failures (app): 
  - `wayland/terminal/subshell_ready_zsh_newuser_install`
  - `wayland/agent/subshell_ready_zsh_newuser_install`
  - `windows/terminal@gitbash/gitbash_precmd_secret`
- Latency regressions > 5%: 
  - `macos/terminal/latency_paint key_to_paint: 30.69 -> 34.11 ms (+11.1%)`
  - `windows/terminal@gitbash/ctrl_c_d ctrl_c: 70.74 -> 79.36 ms (+12.2%)`
- compared against: test-linux: build/r11a (3049602519); test-warpui-core: build/r11a (3049602519); test-workspace-rest: build/r11a (3049602519); app lab-wayland: build/r11a (3049602519); app macos: build/r11a (3049602519); app wayland: build/r11a (3049602519); app windows: build/r11a (3049602519)
