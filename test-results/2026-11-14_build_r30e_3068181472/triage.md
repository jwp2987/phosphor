# Triage: round 30 targeted run e, pipeline 3068181472 (build/r30e a997653cf)

Trimmed targeted run (issue-tied cases only, 91 min wall clock): #2216 (no-helper side channels), #2196 (zsh in-band Tab, new zshc arm), #2220 (routing_table_powershell), #2213 on Windows Git Bash.

## Units
Linux and macOS: all six suites green. Windows: test-warpui-core-windows and test-workspace-rest-windows green; test-windows 1 FAIL, so the Windows build, app and lab were NOT played (unit tests first).
- util::git::tests::commit_chain_commit_only_creates_a_commit_and_reports_delta #2226 (FAIL, APP: on Windows the commit message fed to `git commit -F -` was lost because the child's stdin was dropped without close; fixed after r30e in 7f4853871 with a 4000-line message test).
- LEAK on Windows only, tracked in #2194:
  - ai::blocklist::agent_fuzz::matrix::triples_command::off #2194
  - context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers #2194
  - context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command #2194
  - terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell #2194
  - workspace::view::tests::test_active_tab_bar_position_id_tracks_layout #2194
  - workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer #2194
  - workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state #2194
  - workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it #2194

## Lab
- **Linux lab: 13 pass, 4 fail, 3 NOREF. macOS lab: 18 pass, 4 fail, 6 NOREF.**
- #2216 (APP, fixed in ff55a2b1d): lab_chips bash/zsh/fish .lan PASS on Linux and macOS in both modes (all failed in r30d); lab_fam_git_read / lab_fam_run_command .nohelper rows no longer fail the channel count (NOREF: no git_read backend_route line, a correctness proof per owner rule). Remaining: lab_chips.zsh.150.nohelper (macOS, both modes) FAIL, 3 channels for 1 prompt, two of them at 0.23 s after the window opened with no backend_route logged yet (before a 150 ms connect could exist): suspected TEST (the previous case's channels counted in this window) #2216, verdict pending the lab sshd log.
- #2196 (TEST fix a997653cf): lab_tab_no_helper.zshc.{lan,150} PASS on Linux and macOS in both modes (zsh WITH compinit answers in-band).
- lab_tab_no_helper.zsh.{lan,150}.nohelper FAIL (Linux both modes; macOS terminal .150): #2188 regression from #2216 (APP). After the empty in-band answer the completer never answers (no exec channel, no backend request in the 5 s remote_tab gate; the net runs the uncompleted cd). In r30c/r30d the same rows failed only the in-band check, so the completer worked before ff55a2b1d. #2188's fixed-untested label removed; fix in progress.
- Windows lab: not played (red units).

## App
Windows app (routing_table_powershell, #2220): not played (red units).

## Bands
Held for the owner (WOULD LOOSEN from collect, not applied): the same ten keys as r30d (macos latency_first_key_after_idle gap1s/gap5s/gap2s, ctrl_c_d ctrl_c_flood; windows latency_enter x2, latency_key_log, pwsh7 ctrl_c_d, gitbash editor_memory_7mb rs private (#2224), subshell_ready_rc_questions). No band row from this run.

## Not proven by this run
Windows (#2220, #2213 on Git Bash, #2226): the next run after the box gate.
