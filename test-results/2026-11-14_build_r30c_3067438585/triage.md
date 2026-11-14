# Triage: round 30 targeted run c, pipeline 3067438585 (build/r30c 36efe34b5)

The rerun after #2213 (lab ctl reader) and #2214: every zsh lab arm on Linux and macOS (void in r30b), and r30b's unplayed Windows chain.

## Units
Linux and macOS: all six suites green. Windows: test-workspace-rest-windows and test-warpui-core-windows green; test-windows 1 FAIL + 9 LEAK, so the Windows build, app and lab were NOT played (unit tests first).
- ai::blocklist::history_model::tests::forget_terminal_view_evicts_a_done_conversation_once_sqlite_has_written_it #2219 (FAIL, TEST: waited a fixed 50 yield_now turns for the eviction; same class as #2212. Fixed after r30c in d1d2149d6: test_util::wait_for, 10 s wall-clock give-up, 13 condition waits converted, assertions unchanged.)
- LEAK on Windows only, tracked in #2194 (present in every Windows run since r26w):
  - ai::blocklist::agent_fuzz::matrix::pairs_command::off #2194
  - ai::blocklist::agent_fuzz::matrix::triples_command::on #2194
  - context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers #2194
  - context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command #2194
  - terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell #2194
  - workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer #2194
  - workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state #2194
  - workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit #2194
  - workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it #2194

## Lab
- **macOS lab: VOID (coordinator fault, not a test or app result).** The cases file named lab_managed_connect.zsh.*, lab_msys_ssh_switch.zsh.* and lab_remote_session.zsh.* (those families have no zsh arm); case_selection refused the list and no case ran. A replay kept the job's original variables. The macOS zsh arms rerun in r30d.
- **Linux lab (job 18027460340, played by hand with the corrected list after the driver was stopped): 25 pass, 20 fail, 29 NOREF.** Every fail is one of three known causes:
  - lab_chips.zsh.{lan,150}.nohelper (both modes) and lab_fam_git_read / lab_fam_run_command .zsh.{lan,150}.nohelper (terminal): 2-3 exec channels per prompt. #2216 (APP). The extra channel is NOT the shell backend's tree listing (the batch logs one run_script); suspect the legacy generator executor, which now logs `shell exec: producer=legacy_generator` (diagnostic merged after r30c) so r30d names it.
  - lab_connect_typeahead.zsh.* (all 8 rows): the app's ssh bootstrap line counted as a received command. #2213 (TEST): the r30 filter ran only in LabRemote.commands(), not in Profile.commands(remote=True) which the winport families read. Fixed after r30c in f79a866c8. These rows are void.
  - lab_tab_no_helper.zsh.{lan,150}.nohelper (both modes): "The remote shell had no Tab answer; the completer answers instead" (app log case_lab_tab_no_helper.zsh.lan.nohelper.log 04:45:56). #2196 (APP) is not fixed for zsh; its fixed-untested label was removed.
- Proven: lab_ctrl_c_no_esc_i.zsh.{150,lan}.helper PASS in both modes (#2187 closed, the nu half was proven in r30b); every lab_ssh_drop_reconnect.zsh arm (12), lab_helper_reconnect.zsh, lab_enter_start.zsh.150.helper, lab_nohelper_tab_hands_back.zsh PASS.

## App
No app job played (Linux/macOS app tests were not in this run; Windows held by red units).

## Bands
Held for the owner (WOULD LOOSEN from collect; none from this run's rows, no app job played): the four Windows widenings already held (owner 2026-11-13: hold until a Windows app run), and macos agent latency_first_key_after_idle gap1s/gap5s, macos terminal ctrl_c_d ctrl_c_flood and latency_first_key_after_idle gap2s.

## Not proven by this run
Windows (#2203/#2207/#2208 gitbash_typeahead_reps, the Windows-only fixed-untested issues) and the macOS zsh lab: r30d.
