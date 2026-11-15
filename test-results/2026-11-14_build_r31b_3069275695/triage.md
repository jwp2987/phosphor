# Triage: round 31 targeted run b, pipeline 3069275695 (build/r31b 6936b7cf2)

First run on a fully code-complete branch (owner rule 2026-11-14). Issue-tied cases only.

## Units
All nine suites green on Linux, macOS and Windows. Windows test-windows: 12126 passed, 5 leaky (down from 8): agent_fuzz::matrix::{pairs_failing_command,pairs_question,triples_command}::on, terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell, workspace::view::tests::test_open_file_with_target_event_preserves_requested_line #2194 (root-cause fix merged after r31b: Sessions ends its executors' children on drop, test-time TerminalManager waits for its reader).

## App
- Linux: all PASS except scroll_keys_position (both modes): TEST, the view was at the bottom but CI OCR read 0/8 tail words (fix merged 182d1dfd5, unproven). restore_shows_history PASS both modes (#2110 closed); agent_bg_job_output PASS (#2145 closed); subshell_ready_rc_clear PASS (#2190).
- macOS: 12/13 PASS; agent_bg_job_output FAIL was TEST (zsh's job-done notice re-read by OCR), fixed 45b27c4bc.
- Windows: selection_modes PASS both Git Bash modes (#2223 #2228 #2117 closed); expert_fuzz PASS (#2115 closed); gitbash_typeahead_reps PASS (#2203 #2208 closed); powershell_setup_ctrl_c_keys PASS in every pass (#1890 closed); routing_table PASS (#2200 closed); ssh_gitbash_failing_not_local_managed PASS (#2206 closed). FAIL: ctrl_c_behind_big_paste in all 6 passes (the measurement works: ^C to pty write 2.6 ms; the follow-up line step fails; verdict in progress), phosphor_scheme_handler (final typing step; verdict in progress), ui_completions_as_you_type_local #2222 (diagnostics were invisible under the Windows RUST_LOG; visible hops merged 94298f8ae). agent_code_recycle NOREF (#1787 diagnostic).

## Lab
- Linux: 23 PASS, 11 NOREF; the 4 FAIL are lab_tab_no_helper.zsh.{lan,150} (both modes), an outdated #2174 expectation now that #2225 skips the in-band request (judge fixed c4b0756ef); the Tab completed the cd (#2188 closed).
- macOS: lab_connect_typeahead.zsh 8/8 PASS both modes (#2209 #2227 closed); lab_path_detect.p_exec PASS both modes (#2155); the zsh Tab FAILs are the same outdated expectation; lab_chips.zsh.150.nohelper FAIL is TEST: the connect-time file-tree snapshots were counted (harness waits for them now, 370ff55fd); the double snapshot itself is #2229 (fix merged).
- Windows: lab_connect_typeahead.bash 8/8 PASS both modes (managed Git Bash ssh against the real lab; #2182 closed). FAIL: lab_chips.bash.lan.nohelper (no side call, chip never took the branch) and lab_path_detect.p_exec (no exec-zsh takeover on the one-connection path): verdicts in progress.

## Bands
Held for the owner (WOULD LOOSEN, not applied): the same keys as r30d-r30f (macOS latency_first_key_after_idle, ctrl_c_d; Windows latency_enter, latency_key_log, pwsh7 ctrl_c_d, gitbash editor_memory_7mb rs private (#2224), subshell_ready_rc_questions).
