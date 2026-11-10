# build/r18w9b  413cd4241  (pipeline 3048013125)

- date: 2026-11-10, tier: fix, pipeline status: manual
- compile (check-linux): ok, 118 warnings
- unit: 16281 passed, 40 failed, 55 skipped of 16321 run (3 suites)
- app (pass/fail/skip): lab-wayland 2/21/0 +13 no-ref; wayland 57/16/30 +44 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | failed | 20.7 | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | skipped | - | - |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | failed | 112.3 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | manual | - | - | not run |
| app-test-windows | skipped | - | - |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 30.6 | - |  |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | manual | - | - | not run |
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
| guards | success | 1.9 | - |  |
| guards-helper-deps | success | 0.1 | - |  |
| guards-windows | manual | - | - | not run |
| input-timers-strict | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.1 | - |  |
| mutants | manual | - | - | not run |
| mutants-macos | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| mutants-warpui-core-macos | manual | - | - | not run |
| mutants-warpui-core-windows | manual | - | - | not run |
| mutants-windows | manual | - | - | not run |
| sshlab-image | success | 0.2 | - |  |
| test-linux | failed | 23.3 | - |  |
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

- `warp ai::agent::conversation::tests::a_restored_turn_with_every_tool_call_answered_is_successful` FAIL (0.0s)
- `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants` FAIL (112.2s)
- `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants_with_the_allowlist_on` FAIL (112.5s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_hung_command::off` FAIL (77.9s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_hung_command::on` FAIL (78.0s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_hung_command::off` FAIL (146.6s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_hung_command::on` FAIL (146.3s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_read_files::off` FAIL (132.8s)
- `warp ai::blocklist::agent_fuzz::matrix::triples_read_files::on` FAIL (133.2s)
- `warp ai::blocklist::agent_fuzz::stop_tests::a_reject_after_a_finished_result_tells_the_model_at_once_and_the_agent_continues` FAIL (0.6s)
- `warp ai::blocklist::agent_fuzz::stop_tests::a_stop_after_a_finished_result_opens_no_request_and_the_results_ride_on_the_next_message` FAIL (1.1s)
- `warp terminal::bootstrap::tests::bash_body_does_not_trap_sighup` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::a_failed_add_never_re_fetches_a_remote_previous_source` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::fresh_install_adds_the_bundled_folder_then_installs` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::install_over_a_remote_registration_replaces_it_with_the_bundle` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::move_re_registers_from_the_bundle_and_clears_the_offer` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::update_re_registers_the_bundle_with_no_network_step` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::a_failed_add_never_re_fetches_a_remote_previous_source` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::fresh_install_adds_the_bundled_folder_then_the_plugin` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::install_over_a_remote_registration_replaces_it_with_the_bundle` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::move_re_registers_from_the_bundle_and_clears_the_offer` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::update_re_registers_a_remote_marketplace_from_the_bundle_with_no_network_step` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::gemini::tests::fresh_install_installs_the_bundled_folder` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::gemini::tests::move_reinstalls_from_the_bundle` FAIL (0.0s)
- `warp terminal::cli_agent_sessions::plugin_manager::gemini::tests::update_replaces_a_remote_install_with_the_bundle_and_clears_the_offer` FAIL (0.0s)
- `warp terminal::input::tests::editing_the_buffer_during_a_secret_read_never_requests_a_query_prediction` FAIL (0.2s)
- `warp terminal::model::typeahead_handoff::tests::a_command_the_latch_calls_interactive_gets_its_enter_and_releases_a_live_hold` FAIL (0.0s)
- `warp terminal::model::typeahead_handoff::tests::an_unengaged_user_command_makes_a_young_hold_follow_not_drop` FAIL (0.0s)
- `warp terminal::model::typeahead_handoff::tests::keys_a_hold_gave_up_on_never_start_another_young_hold` FAIL (0.0s)
- `warp terminal::model::typeahead_handoff::tests::the_prompt_commits_to_the_editor_when_no_keys_are_held` FAIL (0.0s)
- `warp terminal::view::tests::a_users_own_command_cancels_the_armed_setup_and_nothing_is_written` FAIL (0.1s)
- `warp terminal::view::tests::a_young_hold_is_written_as_typeahead_when_the_command_becomes_interactive` FAIL (0.2s)
- `warp terminal::view::tests::an_idle_dash_prompt_with_nothing_typed_is_written_on_the_look` FAIL (0.2s)
- `warp terminal::view::tests::keys_behind_a_run_enter_are_written_to_the_pty_at_the_net_in_both_modes` FAIL (2.2s)
- `warp terminal::view::tests::open_pane_mid_batch_cancel_sends_the_follow_up` FAIL (0.1s)
- `warp terminal::view::tests::paste_during_a_subshell_setup_is_held_behind_earlier_keys_in_both_modes` FAIL (0.1s)
- `warp terminal::view::tests::pty_paste_backstop_gives_up_then_the_late_result_is_ignored` FAIL (0.2s)
- `warp terminal::view::tests::route_parity_a_tests::suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does` FAIL (0.2s)
- `warp terminal::view::tests::stalled_pty_paste_hands_held_keys_back_in_order_at_the_backstop` FAIL (0.2s)
- `warp terminal::view::tests::the_readiness_tail_is_the_cursor_row_not_the_blank_rows_below_a_clear` FAIL (0.1s)

## app lab-wayland (app-test-lab-linux)


| mode | case | status | detail |
|---|---|---|---|
| terminal | lab_fam_agent_context.bash.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.bash.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.zsh.lan.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.zsh.150.nohelper | NOREF | NO-REFERENCE: UNVERIFIABLE: no scripted app driver for capability agent_context yet (add one to DRIVERS in cases_lab_helper.py); the case is |
| terminal | lab_fam_agent_context.fish.lan.nohelper | FAIL | no remote prompt after `ssh lab-u_fish`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1 |
| terminal | lab_fam_agent_context.fish.150.nohelper | FAIL | no remote prompt after `ssh lab-u_fish`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1 |
| terminal | lab_fam_agent_context.nu.lan.nohelper | FAIL | no remote prompt after `ssh lab-u_nu`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1:  |
| terminal | lab_fam_agent_context.nu.150.nohelper | FAIL | no remote prompt after `ssh lab-u_nu`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1:  |
| terminal | harness_state_reset_after_lab_fam_agent_context.bash.lan.helper | FAIL | after lab_fam_agent_context.bash.lan.helper the harness's own state was not put back and was reset so later cases are not judged against it: |
| terminal | lab_fam_agent_context.bash.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.bash.150.helper | FAIL | after lab_fam_agent_context.bash.150.helper the harness's own state was not put back and was reset so later cases are not judged against it: |
| terminal | lab_fam_agent_context.bash.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.zsh.lan.helper | FAIL | after lab_fam_agent_context.zsh.lan.helper the harness's own state was not put back and was reset so later cases are not judged against it:  |
| terminal | lab_fam_agent_context.zsh.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.zsh.150.helper | FAIL | after lab_fam_agent_context.zsh.150.helper the harness's own state was not put back and was reset so later cases are not judged against it:  |
| terminal | lab_fam_agent_context.zsh.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.fish.lan.helper | FAIL | after lab_fam_agent_context.fish.lan.helper the harness's own state was not put back and was reset so later cases are not judged against it: |
| terminal | lab_fam_agent_context.fish.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.fish.150.helper | FAIL | after lab_fam_agent_context.fish.150.helper the harness's own state was not put back and was reset so later cases are not judged against it: |
| terminal | lab_fam_agent_context.fish.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.nu.lan.helper | FAIL | after lab_fam_agent_context.nu.lan.helper the harness's own state was not put back and was reset so later cases are not judged against it: t |
| terminal | lab_fam_agent_context.nu.lan.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| terminal | harness_state_reset_after_lab_fam_agent_context.nu.150.helper | FAIL | after lab_fam_agent_context.nu.150.helper the harness's own state was not put back and was reset so later cases are not judged against it: t |
| terminal | lab_fam_agent_context.nu.150.helper | NOREF | NO-REFERENCE: UNVERIFIABLE: helper arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary; injector accounting: injector sen |
| agent | lab_agent_remote_flow.bash.lan.nohelper | FAIL | the approved `cat` reached the remote shell 0 times ([]), expected exactly once; codebase step not run: APPCI_FAKE_EMBEDDINGS=1 is off, so t |
| agent | lab_agent_remote_flow.nu.lan.nohelper | FAIL | no remote prompt after `ssh lab-u_nu`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1:  |
| agent | lab_agent_remote_errors.bash.lan.nohelper | PASS | injector accounting: injector sent 326; app saw 337; lost-before-app 0 (Enter: sent 6, app saw 7); app logged 11 more than were injected (ke |
| agent | lab_agent_remote_errors.nu.lan.nohelper | FAIL | no remote prompt after `ssh lab-u_nu`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1:  |
| agent | lab_agent_remote_drop.bash.lan.nohelper | FAIL | the approved command never reached the remote shell, so the drop could not hit a running tool call; injector accounting: injector sent 97; a |
| agent | pane_not_local_after_lab_agent_remote_drop.bash.lan.nohelper | FAIL | after lab_agent_remote_drop.bash.lan.nohelper the pane's shell was not the root shell (none; root pid 3233666); the app is relaunched so lat |
| agent | lab_agent_remote_drop.nu.lan.nohelper | FAIL | no remote prompt after `ssh lab-u_nu`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1:  |
| agent | lab_fam_agent_edit_conflict.bash.lan.nohelper | NOREF | partial accept: the diff card has no per-hunk accept (Up/Down only move between hunks, code_diff_view.rs); accept applies every hunk, reject |
| agent | lab_fam_agent_edit_conflict.nu.lan.nohelper | FAIL | no remote prompt after `ssh lab-u_nu`. Test host: sshd.log: debug1: do_cleanup / debug1: temporarily_use_uid: 997/974 (e=997/974) / debug1:  |
| agent | lab_fam_agent_delete_conflict.bash.lan.nohelper | FAIL | lab_fam_agent_delete_conflict: no hop=backend_route family=file_write line: the edit was not written through the backend router; injector ac |
| agent | lab_fam_agent_edit_endings.bash.lan.nohelper | FAIL | crlf: the accepted multi-hunk edit: 207 bytes on disk, 228 expected; first difference at byte 25 (b'beta t329xf26g\r\nL03 gamm' vs b'BETA-ED |
| agent | lab_fam_agent_edit_touch_only.bash.lan.nohelper | PASS | injector accounting: injector sent 300; app saw 311; lost-before-app 0 (Enter: sent 6, app saw 6); app logged 11 more than were injected (ke |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | NOREF | injector accounting: injector sent 1; app saw 0; lost-before-app 1 (Enter: sent 0, app saw 0); NO-REFERENCE: UNVERIFIABLE: injector accounti |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 182; app saw 178; lost-before-app 4 (Enter: sent 1, app saw 1); pty writes: 0, of which c |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot ga |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2072; app saw 2070; lost-before-app 2 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| terminal | paste_instant_enter | NOREF | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; NO-REFE |
| terminal | tab_enter | NOREF | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: |
| terminal | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| terminal | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 27; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 25 more th |
| terminal | subshell_sh_probe | FAIL | dash: the first inner line ran at all (file []); dash: the first inner line ran 10.03 s after its Enter (budget 2 s, #1710); dash: exact lin |
| terminal | subshell_ready_rc_questions | PASS | injector accounting: injector sent 261; app saw 289; lost-before-app 0 (Enter: sent 9, app saw 11); app logged 28 more than were injected (k |
| terminal | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 324; lost-before-app 0 (Enter: sent 9, app saw 10); app logged 26 more than were injected (k |
| terminal | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 80; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 28 more t |
| terminal | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| terminal | subshell_ready_secret_after_probe | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| terminal | subshell_sh_probe_empty_ps1 | PASS | injector accounting: injector sent 620; app saw 646; lost-before-app 0 (Enter: sent 19, app saw 20); app logged 26 more than were injected ( |
| terminal | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 77; lost-before-app 0 (Enter: sent |
| terminal | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 346; lost-before-app 0 (Enter: sent 8, app saw 9); app logged 24 more than were injected (ke |
| terminal | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 76; lost-bef |
| terminal | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| terminal | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 2905396); the app is relaunched so later ca |
| terminal | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 288; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 28 more than were injected (ke |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 273; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T331_jnxirw never reached the REMOTE file (s |
| terminal | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| terminal | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 30; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 28 more th |
| terminal | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| terminal | latency_key_log | NOREF | #1176 key -> pty write median 0.65 ms (p95 0.70, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 1 (median 233 |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_paint_ab | SKIP | the key frame drawn at the end of the key press (#1848) exists only in the macOS renderer, and kl3 (ScreenCaptureKit) is macOS only; Linux m |
| terminal | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (terminal mode): views p50 1 / 6 (Δ -5), render_ms p50 0.04 / 0.07 (Δ -0.03), f |
| terminal | routing_table | NOREF | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to shell (no rout |
| terminal | agent_mode_switch | NOREF | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 6, app saw 4); NO-REFERENCE: UNVERIFIABLE: injector acc |
| terminal | remote_tab_no_helper | NOREF | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 179; lost-before-app 0 (Enter: sent 10, app saw 11); app logged 19 more than were injected ( |
| terminal | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more th |
| terminal | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 26; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 24 more th |
| terminal | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more th |
| terminal | ui_completions_as_you_type_local | PASS | injector accounting: injector sent 16; app saw 40; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 24 more than were injected (keys |
| terminal | shell_fish | PASS | injector accounting: injector sent 751; app saw 772; lost-before-app 0 (Enter: sent 25, app saw 26); 4 paste chord(s) not counted as keys; a |
| terminal | shell_nu | NOREF | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-10T05:11:3 |
| terminal | ctrl_c_no_esc_i_local | NOREF | injector accounting: injector sent 461; app saw 441; lost-before-app 20 (Enter: sent 18, app saw 18); NO-REFERENCE: UNVERIFIABLE: injector a |
| terminal | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| terminal | ps_read_host_secure | SKIP | PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret); |
| terminal | gitbash_precmd_secret | SKIP | Git Bash pane only (bash precmd_functions; ConPTY prompt hold); injector accounting: injector sent 2; app saw 26; lost-before-app 0 (Enter:  |
| terminal | md_viewer | PASS | injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more than were injected (keys  |
| terminal | zero_network_startup | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal | keyring_unavailable_startup | NOREF | injector accounting: injector sent 595; app saw 590; lost-before-app 5 (Enter: sent 13, app saw 13); NO-REFERENCE: UNVERIFIABLE: injector ac |
| terminal | claude_plugin_offline | NOREF | bundle: /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/claude_plugin; networ |
| terminal | agent_plugins_offline | NOREF | codex bundle: /home/<user>/builds/r_hQtIdMf/1/winters-phosphor/phosphor/appci-out/wayland/app/opt/phosphor/resources/bundled/codex_plugin; n |
| terminal | network_gate_switch | NOREF | NO-REFERENCE: UNVERIFIABLE: the denial notice (toast naming the purpose and host), its 'Allow this origin' link and the log row's 'Allow' bu |
| terminal | depth_check_off_before_cb_report_problem | SKIP | root shell pid not recorded after relaunch_by_case: the PID probe got no answer; the depth check is off until the next relaunch |
| terminal | cb_report_problem | NOREF | injector accounting: injector sent 72; app saw 65; lost-before-app 7 (Enter: sent 2, app saw 0); NO-REFERENCE: UNVERIFIABLE: injector accoun |
| agent | mode_proof | FAIL | agent mode expected but the screen shows no agent markers ('New Phosphor Agent conversation' / 'Ask anything'); terminal hint present: False |
| agent | cadence | PASS | injector accounting: injector sent 181; app saw 189; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 8 more than were injected (key |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2004; app saw 2003; lost-before-app 1 (Enter: sent 60, app saw 60); pty writes: 0, of whi |
| agent | paste_instant_enter | NOREF | injector accounting: injector sent 8; app saw 6; lost-before-app 2 (Enter: sent 6, app saw 6); 6 paste chord(s) not counted as keys; NO-REFE |
| agent | tab_enter | NOREF | injector accounting: injector sent 470; app saw 456; lost-before-app 14 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent | ctrl_c_d | NOREF | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ctrl_c_flood_ab | SKIP | the blocked-frame gate (#1838) exists only in the macOS renderer; injector accounting: injector sent 1; app saw 0; lost-before-app 1 (Enter: |
| agent | ps_ctrl_break | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 25; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 23 more th |
| agent | ps_fast_command_young_enter | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 26; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 24 more th |
| agent | subshell_sh_probe | FAIL | dash: the first inner line ran at all (file []); dash: the first inner line ran 10.05 s after its Enter (budget 2 s, #1710); dash: exact lin |
| agent | subshell_ready_rc_questions | PASS | injector accounting: injector sent 260; app saw 271; lost-before-app 0 (Enter: sent 9, app saw 11); app logged 11 more than were injected (k |
| agent | subshell_ready_vim_typeahead | PASS | injector accounting: injector sent 298; app saw 318; lost-before-app 0 (Enter: sent 9, app saw 10); app logged 20 more than were injected (k |
| agent | subshell_ready_unsupported_tcsh | SKIP | tcsh is not installed; injector accounting: injector sent 52; app saw 78; lost-before-app 0 (Enter: sent 1, app saw 2); app logged 26 more t |
| agent | subshell_ready_secret_after_prompt | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| agent | subshell_ready_secret_after_probe | FAIL | the password reached the read -s, not a command: []; the setup was written again after the secret was answered; the handed-back line ran onc |
| agent | subshell_sh_probe_empty_ps1 | PASS | injector accounting: injector sent 619; app saw 627; lost-before-app 0 (Enter: sent 19, app saw 20); app logged 8 more than were injected (k |
| agent | subshell_ready_relay_shape | SKIP | no passwordless sudo on this runner (sudo -n true failed); injector accounting: injector sent 52; app saw 78; lost-before-app 0 (Enter: sent |
| agent | subshell_ready_exec_fish_rc | PASS | injector accounting: injector sent 322; app saw 347; lost-before-app 0 (Enter: sent 8, app saw 9); app logged 25 more than were injected (ke |
| agent | subshell_ready_docker_t_without_i | SKIP | docker with a local alpine image is not available (never pulled by the harness); injector accounting: injector sent 52; app saw 74; lost-bef |
| agent | subshell_ready_zsh_newuser_install | FAIL | nothing was written into the newuser-install menu; zshnew: the line typed into the inner shell ran (file []); the menu footer read as an Inp |
| agent | pane_not_local_after_subshell_ready_zsh_newuser_install | FAIL | after subshell_ready_zsh_newuser_install the pane's shell was not the root shell (none; root pid 3154327); the app is relaunched so later ca |
| agent | subshell_ready_rc_clear | PASS | injector accounting: injector sent 260; app saw 284; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 24 more than were injected (ke |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 273; lost-before-app 0 (Enter: sent 12, app saw 13); app logged |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 164; lost-before-app 0 (Enter: sent 7, app saw 9); app logged 2 |
| agent | ssh_connect_typing_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 26; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 24 more th |
| agent | ssh_nested_typeahead_wrapper | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 26; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 24 more th |
| agent | latency_enter | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; app share of Enter->start: enter_begin ->  |
| agent | latency_key_log | NOREF | #1176 key -> pty write median 0.64 ms (p95 0.71, n=60): within the 1 ms target; #1958 platform samples while typing 63 key(s): 1 (median 246 |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_paint_ab | SKIP | the key frame drawn at the end of the key press (#1848) exists only in the macOS renderer, and kl3 (ScreenCaptureKit) is macOS only; Linux m |
| agent | latency_key_views_ab | NOREF | #1962 views re-rendered per key, skips on vs off, first keys (agent mode): views p50 3 / 6 (Δ -3), render_ms p50 0.2 / 0.25 (Δ -0.05), frame |
| agent | agent_stream_cancel | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; Ctrl-C #1: stream stopped  |
| agent | agent_input_exact | NOREF | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2); NO-REFERENCE: UNVERIFIABLE: injector acco |
| agent | routing_table | NOREF | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| agent | agent_mode_switch | NOREF | injector accounting: injector sent 219; app saw 204; lost-before-app 15 (Enter: sent 6, app saw 4); NO-REFERENCE: UNVERIFIABLE: injector acc |
| agent | agent_lrc_no_false_takeover | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | remote_tab_no_helper | PASS | path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent | remote_tab_enter_no_helper | PASS | injector accounting: injector sent 160; app saw 178; lost-before-app 0 (Enter: sent 10, app saw 11); app logged 18 more than were injected ( |
| agent | ssh_gitbash_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more th |
| agent | ssh_gitbash_typeahead_managed | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 26; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 24 more th |
| agent | ssh_gitbash_kill_switch | SKIP | Windows only (ConPTY); injector accounting: injector sent 2; app saw 25; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 23 more th |
| agent | shell_fish | PASS | injector accounting: injector sent 751; app saw 775; lost-before-app 0 (Enter: sent 25, app saw 26); 4 paste chord(s) not counted as keys; a |
| agent | shell_nu | NOREF | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; injector accounting: injector  |
| agent | ctrl_c_no_esc_i_local | NOREF | injector accounting: injector sent 460; app saw 441; lost-before-app 19 (Enter: sent 18, app saw 18); NO-REFERENCE: UNVERIFIABLE: injector a |
| agent | agent_view_left_by_double_ctrl_c_after_ctrl_c_no_esc_i_local | PASS | after ctrl_c_no_esc_i_local the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits |
| agent | giveup_keys_reach_shell | PASS | no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly; injec |
| agent | md_viewer | PASS | injector accounting: injector sent 2; app saw 29; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 27 more than were injected (keys  |
| agent | keyring_unavailable_startup | NOREF | injector accounting: injector sent 545; app saw 541; lost-before-app 4 (Enter: sent 12, app saw 12); NO-REFERENCE: UNVERIFIABLE: injector ac |
| agent | agent_view_not_restored_after_keyring_unavailable_startup | PASS | keyring_unavailable_startup relaunched the app itself (pid 3192621 -> 3194246); a relaunched app starts in a terminal session, so this is no |
| agent@fake | agent_interactive_password | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_accept | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; partial accept: the diff c |
| agent@fake | agent_edit_reject | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; reject: no follow-up model |
| agent@fake | agent_edit_conflict | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_touch_only | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_edit_endings | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_fault_queued_rows | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_key | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_vim | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_vim: hand-over line in  |
| agent@fake | agent_restart_quit_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_stream: the comple |
| agent@fake | agent_restart_quit_queued | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_queued: the comple |
| agent@fake | agent_restart_quit_blocked | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_quit_blocked: the compl |
| agent@fake | agent_question_answer | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_nodone | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_slow | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_after_command | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_question_steer_at_end | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_latency_enter | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation |
| agent@fake | agent_interactive_ssh | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; NO-REFERENCE: UNVERIFIABLE |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the comple |
| agent@fake | agent_restart_kill_queued | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_queued: the comple |
| agent@fake | agent_restart_kill_blocked | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_blocked: the compl |
| agent@fake | agent_steer_fast | NOREF | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | agent_interactive_less | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ai_less: hand-over line in |
| agent@fake | agent_noninteractive_no_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; injector accounting: injec |
| agent@fake | network_gate_notice | FAIL | the remote image from an unlisted host was not refused by the gate (no 'network gate: denied "remote-asset" (host hash 714ef6f9d3c78218)' li |
| agent@fake | agent_gate_deny_allow | FAIL | the image from the unlisted loopback NAME localhost was not refused (no 'network gate: denied "remote-asset" (host hash db3f053403ad937a)' l |
| agent@fake | agent_gate_off_live | FAIL | precondition: the gate (on) did not refuse the unlisted origin; with the gate off the request did not reach the listener; with the gate back |
| agent@fake | agent_mcp_faults | SKIP | precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: []; starts (0, 0)): t |
| agent@fake | agent_gate_key_never_leaves | FAIL | the denied origin was not refused (no 'network gate: denied "remote-asset" (host hash db3f053403ad937a)'); the allowed origin did not receiv |
| agent@fake | agent_multitab_streaming | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; #805 hop: the app built 3. |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_edit_accept | wayland | agent@fake | PASS |
| agent_edit_conflict | wayland | agent@fake | PASS |
| agent_edit_endings | wayland | agent@fake | NOREF |
| agent_edit_reject | wayland | agent@fake | PASS |
| agent_edit_touch_only | wayland | agent@fake | PASS |
| agent_fault_429 | wayland | agent@fake | PASS |
| agent_fault_500 | wayland | agent@fake | PASS |
| agent_fault_disconnect | wayland | agent@fake | PASS |
| agent_fault_down | wayland | agent@fake | PASS |
| agent_fault_queued_rows | wayland | agent@fake | PASS |
| agent_fault_slow_cancel | wayland | agent@fake | PASS |
| agent_gate_deny_allow | wayland | agent@fake | FAIL |
| agent_gate_key_never_leaves | wayland | agent@fake | FAIL |
| agent_gate_off_live | wayland | agent@fake | FAIL |
| agent_interactive_less | wayland | agent@fake | PASS |
| agent_interactive_password | wayland | agent@fake | PASS |
| agent_interactive_ssh | wayland | agent@fake | NOREF |
| agent_interactive_vim | wayland | agent@fake | NOREF |
| agent_latency_enter | wayland | agent@fake | NOREF |
| agent_latency_key | wayland | agent@fake | NOREF |
| agent_lrc_no_false_takeover | wayland | agent | NOREF |
| agent_mcp_faults | wayland | agent@fake | SKIP |
| agent_multitab_streaming | wayland | agent@fake | PASS |
| agent_noninteractive_no_takeover | wayland | agent@fake | PASS |
| agent_plugins_offline | wayland | terminal | NOREF |
| agent_question_after_command | wayland | agent@fake | NOREF |
| agent_question_answer | wayland | agent@fake | NOREF |
| agent_question_ctrl_c | wayland | agent@fake | PASS |
| agent_question_cut | wayland | agent@fake | PASS |
| agent_question_empty | wayland | agent@fake | PASS |
| agent_question_nodone | wayland | agent@fake | PASS |
| agent_question_slow | wayland | agent@fake | PASS |
| agent_question_steer | wayland | agent@fake | PASS |
| agent_question_steer_at_end | wayland | agent@fake | PASS |
| agent_question_trunc | wayland | agent@fake | PASS |
| agent_restart_kill_blocked | wayland | agent@fake | NOREF |
| agent_restart_kill_queued | wayland | agent@fake | NOREF |
| agent_restart_kill_stream | wayland | agent@fake | PASS |
| agent_restart_quit_blocked | wayland | agent@fake | NOREF |
| agent_restart_quit_queued | wayland | agent@fake | NOREF |
| agent_restart_quit_stream | wayland | agent@fake | PASS |
| agent_steer_fast | wayland | agent@fake | NOREF |
| cb_report_problem | wayland | terminal | NOREF |
| claude_plugin_offline | wayland | terminal | NOREF |
| ctrl_c_flood_ab | wayland | agent | SKIP |
| ctrl_c_flood_ab | wayland | terminal | SKIP |
| ctrl_c_no_esc_i_local | wayland | agent | NOREF |
| ctrl_c_no_esc_i_local | wayland | terminal | NOREF |
| gitbash_precmd_secret | wayland | terminal | SKIP |
| giveup_keys_reach_shell | wayland | agent | PASS |
| giveup_keys_reach_shell | wayland | terminal | PASS |
| keyring_unavailable_startup | wayland | agent | NOREF |
| keyring_unavailable_startup | wayland | terminal | NOREF |
| latency_key_views_ab | wayland | agent | NOREF |
| latency_key_views_ab | wayland | terminal | NOREF |
| latency_paint_ab | wayland | agent | SKIP |
| latency_paint_ab | wayland | terminal | SKIP |
| network_gate_notice | wayland | agent@fake | FAIL |
| network_gate_switch | wayland | terminal | NOREF |
| ps_ctrl_break | wayland | agent | SKIP |
| ps_ctrl_break | wayland | terminal | SKIP |
| ps_fast_command_young_enter | wayland | agent | SKIP |
| ps_fast_command_young_enter | wayland | terminal | SKIP |
| ps_read_host_secure | wayland | terminal | SKIP |
| remote_tab_enter_no_helper | wayland | agent | PASS |
| remote_tab_enter_no_helper | wayland | terminal | PASS |
| remote_tab_no_helper | wayland | agent | PASS |
| remote_tab_no_helper | wayland | terminal | NOREF |
| ssh_connect_typing_wrapper | wayland | agent | SKIP |
| ssh_connect_typing_wrapper | wayland | terminal | SKIP |
| ssh_gitbash_kill_switch | wayland | agent | SKIP |
| ssh_gitbash_kill_switch | wayland | terminal | SKIP |
| ssh_gitbash_managed | wayland | agent | SKIP |
| ssh_gitbash_managed | wayland | terminal | SKIP |
| ssh_gitbash_typeahead_managed | wayland | agent | SKIP |
| ssh_gitbash_typeahead_managed | wayland | terminal | SKIP |
| ssh_nested_typeahead_wrapper | wayland | agent | SKIP |
| ssh_nested_typeahead_wrapper | wayland | terminal | SKIP |
| subshell_ready_docker_t_without_i | wayland | agent | SKIP |
| subshell_ready_docker_t_without_i | wayland | terminal | SKIP |
| subshell_ready_exec_fish_rc | wayland | agent | PASS |
| subshell_ready_exec_fish_rc | wayland | terminal | PASS |
| subshell_ready_rc_clear | wayland | agent | PASS |
| subshell_ready_rc_clear | wayland | terminal | PASS |
| subshell_ready_rc_questions | wayland | agent | PASS |
| subshell_ready_rc_questions | wayland | terminal | PASS |
| subshell_ready_relay_shape | wayland | agent | SKIP |
| subshell_ready_relay_shape | wayland | terminal | SKIP |
| subshell_ready_secret_after_probe | wayland | agent | FAIL |
| subshell_ready_secret_after_probe | wayland | terminal | FAIL |
| subshell_ready_secret_after_prompt | wayland | agent | FAIL |
| subshell_ready_secret_after_prompt | wayland | terminal | FAIL |
| subshell_ready_unsupported_tcsh | wayland | agent | SKIP |
| subshell_ready_unsupported_tcsh | wayland | terminal | SKIP |
| subshell_ready_vim_typeahead | wayland | agent | PASS |
| subshell_ready_vim_typeahead | wayland | terminal | PASS |
| subshell_ready_zsh_newuser_install | wayland | agent | FAIL |
| subshell_ready_zsh_newuser_install | wayland | terminal | FAIL |
| subshell_sh_probe | wayland | agent | FAIL |
| subshell_sh_probe | wayland | terminal | FAIL |
| subshell_sh_probe_empty_ps1 | wayland | agent | PASS |
| subshell_sh_probe_empty_ps1 | wayland | terminal | PASS |
| ui_completions_as_you_type_local | wayland | terminal | PASS |
| zero_network_startup | wayland | terminal | NOREF |

passed everywhere it ran (32, candidates for the owner's decision): agent_edit_accept, agent_edit_conflict, agent_edit_reject, agent_edit_touch_only, agent_fault_429, agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_queued_rows, agent_fault_slow_cancel, agent_interactive_less, agent_interactive_password, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_question_ctrl_c, agent_question_cut, agent_question_empty, agent_question_nodone, agent_question_slow, agent_question_steer, agent_question_steer_at_end, agent_question_trunc, agent_restart_kill_stream, agent_restart_quit_stream, giveup_keys_reach_shell, remote_tab_enter_no_helper, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_vim_typeahead, subshell_sh_probe_empty_ps1, ui_completions_as_you_type_local
not passed somewhere (fail / skip / no-reference): agent_edit_endings, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_ssh, agent_interactive_vim, agent_latency_enter, agent_latency_key, agent_lrc_no_false_takeover, agent_mcp_faults, agent_plugins_offline, agent_question_after_command, agent_question_answer, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_quit_blocked, agent_restart_quit_queued, agent_steer_fast, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, gitbash_precmd_secret, keyring_unavailable_startup, latency_key_views_ab, latency_paint_ab, network_gate_notice, network_gate_switch, ps_ctrl_break, ps_fast_command_young_enter, ps_read_host_secure, remote_tab_no_helper, ssh_connect_typing_wrapper, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, subshell_ready_docker_t_without_i, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_zsh_newuser_install, subshell_sh_probe, zero_network_startup
- note: wayland/run_slim/logs/agent/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_enter.log: latency evidence skipped (platform budget 2048 KB spent)
- note: wayland/run_slim/logs/agent@fake/case_agent_latency_key.log: latency evidence skipped (platform budget 2048 KB spent)
- note: wayland/run_slim/logs/terminal/case_latency_enter.log: latency evidence skipped (platform budget 2048 KB spent)
- note: wayland/run_slim/logs/terminal/case_latency_key_log.log: latency evidence skipped (platform budget 2048 KB spent)
- note: wayland/run_slim/logs/terminal/case_latency_key_views_ab.log: latency evidence skipped (platform budget 2048 KB spent)
- note: wayland/run_slim/logs/agent/acct/launch0_1794290461.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/acct/launch0_1794291309.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/agent/phosphor.1.log.gz: 2257 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/acct/launch0_1794287733.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: wayland/run_slim/logs/terminal/phosphor.1.log.gz: 2439 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `warp ai::agent::conversation::tests::a_restored_turn_with_every_tool_call_answered_is_successful [test-linux]`
  - `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants [test-linux]`
  - `warp ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants_with_the_allowlist_on [test-linux]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_hung_command::off [test-linux]`
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_hung_command::on [test-linux]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_hung_command::off [test-linux]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_hung_command::on [test-linux]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_read_files::off [test-linux]`
  - `warp ai::blocklist::agent_fuzz::matrix::triples_read_files::on [test-linux]`
  - `warp ai::blocklist::agent_fuzz::stop_tests::a_reject_after_a_finished_result_tells_the_model_at_once_and_the_agent_continues [test-linux]`
  - `warp ai::blocklist::agent_fuzz::stop_tests::a_stop_after_a_finished_result_opens_no_request_and_the_results_ride_on_the_next_message [test-linux]`
  - `warp terminal::bootstrap::tests::bash_body_does_not_trap_sighup [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::a_failed_add_never_re_fetches_a_remote_previous_source [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::fresh_install_adds_the_bundled_folder_then_installs [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::install_over_a_remote_registration_replaces_it_with_the_bundle [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::move_re_registers_from_the_bundle_and_clears_the_offer [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::claude::tests::update_re_registers_the_bundle_with_no_network_step [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::a_failed_add_never_re_fetches_a_remote_previous_source [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::fresh_install_adds_the_bundled_folder_then_the_plugin [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::install_over_a_remote_registration_replaces_it_with_the_bundle [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::move_re_registers_from_the_bundle_and_clears_the_offer [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::codex::tests::update_re_registers_a_remote_marketplace_from_the_bundle_with_no_network_step [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::gemini::tests::fresh_install_installs_the_bundled_folder [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::gemini::tests::move_reinstalls_from_the_bundle [test-linux]`
  - `warp terminal::cli_agent_sessions::plugin_manager::gemini::tests::update_replaces_a_remote_install_with_the_bundle_and_clears_the_offer [test-linux]`
  - `warp terminal::input::tests::editing_the_buffer_during_a_secret_read_never_requests_a_query_prediction [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::a_command_the_latch_calls_interactive_gets_its_enter_and_releases_a_live_hold [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::an_unengaged_user_command_makes_a_young_hold_follow_not_drop [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::keys_a_hold_gave_up_on_never_start_another_young_hold [test-linux]`
  - `warp terminal::model::typeahead_handoff::tests::the_prompt_commits_to_the_editor_when_no_keys_are_held [test-linux]`
  - `warp terminal::view::tests::a_users_own_command_cancels_the_armed_setup_and_nothing_is_written [test-linux]`
  - `warp terminal::view::tests::a_young_hold_is_written_as_typeahead_when_the_command_becomes_interactive [test-linux]`
  - `warp terminal::view::tests::an_idle_dash_prompt_with_nothing_typed_is_written_on_the_look [test-linux]`
  - `warp terminal::view::tests::keys_behind_a_run_enter_are_written_to_the_pty_at_the_net_in_both_modes [test-linux]`
  - `warp terminal::view::tests::open_pane_mid_batch_cancel_sends_the_follow_up [test-linux]`
  - `warp terminal::view::tests::paste_during_a_subshell_setup_is_held_behind_earlier_keys_in_both_modes [test-linux]`
  - `warp terminal::view::tests::pty_paste_backstop_gives_up_then_the_late_result_is_ignored [test-linux]`
  - `warp terminal::view::tests::route_parity_a_tests::suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does [test-linux]`
  - `warp terminal::view::tests::stalled_pty_paste_hands_held_keys_back_in_order_at_the_backstop [test-linux]`
  - `warp terminal::view::tests::the_readiness_tail_is_the_cursor_row_not_the_blank_rows_below_a_clear [test-linux]`
- FIXED failures (unit): 
  - `remote_server backend::contract_tests::the_shell_arm_handles_directory_symlink_hardlink_and_tilde_targets [test-workspace-rest]`
  - `warp_terminal model::escape_sequences::tests::test_ctrl_break_win32_input_sequence [test-workspace-rest]`
- NEW failures (app): 
  - `lab-wayland/terminal/lab_fam_agent_context.fish.lan.nohelper`
  - `lab-wayland/terminal/lab_fam_agent_context.fish.150.nohelper`
  - `lab-wayland/terminal/lab_fam_agent_context.nu.lan.nohelper`
  - `lab-wayland/terminal/lab_fam_agent_context.nu.150.nohelper`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.bash.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.bash.150.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.zsh.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.zsh.150.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.fish.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.fish.150.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.nu.lan.helper (case not in previous run)`
  - `lab-wayland/terminal/harness_state_reset_after_lab_fam_agent_context.nu.150.helper (case not in previous run)`
  - `lab-wayland/agent/lab_agent_remote_flow.bash.lan.nohelper`
  - `lab-wayland/agent/lab_agent_remote_flow.nu.lan.nohelper`
  - `lab-wayland/agent/lab_agent_remote_errors.nu.lan.nohelper`
  - `lab-wayland/agent/lab_agent_remote_drop.bash.lan.nohelper`
  - `lab-wayland/agent/pane_not_local_after_lab_agent_remote_drop.bash.lan.nohelper (case not in previous run)`
  - `lab-wayland/agent/lab_agent_remote_drop.nu.lan.nohelper`
  - `lab-wayland/agent/lab_fam_agent_edit_conflict.nu.lan.nohelper`
  - `lab-wayland/agent/lab_fam_agent_delete_conflict.bash.lan.nohelper`
  - `lab-wayland/agent/lab_fam_agent_edit_endings.bash.lan.nohelper`
  - `wayland/terminal/subshell_ready_secret_after_probe (case not in previous run)`
  - `wayland/terminal/ssh_nested_typeahead`
  - `wayland/agent/mode_proof`
  - `wayland/agent/subshell_ready_secret_after_probe (case not in previous run)`
- FIXED failures (app): 
  - `wayland/terminal/subshell_ready_vim_typeahead`
  - `wayland/agent/subshell_ready_vim_typeahead`
  - `wayland/agent@fake/agent_interactive_password`
  - `wayland/agent@fake/agent_question_steer_at_end`
- Latency regressions > 5%: none
- compared against: test-linux: build/r18w5 (3045526679); test-warpui-core: build/r18w7 (3046481170); test-workspace-rest: build/r18w8 (3047198675); app lab-wayland: build/r18w7 (3046481170); app wayland: build/r18w8 (3047198675)
