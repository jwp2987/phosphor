# Triage: round 30 targeted run, pipeline 3065705355 (build/r30b ca9a20723)

The one targeted run of the r30 input-judge change set (#2203, #2202, #2207, #2208), #2196 fish native Tab, and the r30 lab port of the Windows fake-sshd cases.

## Units
Linux and macOS: all suites green. Windows: test-windows 1 FAIL + 8 LEAK. The other two Windows suites are green. Windows build, app and lab were NOT played (unit tests first).
- terminal::view::tests::a_waiting_question_with_no_card_in_the_pane_is_stopped #2212 (FAIL, TEST: pump_until counted 64 yields; fixed after r30b in 63932ddee, wall-clock 10 s give-up).
- LEAK on Windows only, present in every Windows run since r26w (8-9 per run), tracked in #2194:
  - ai::blocklist::agent_fuzz::matrix::pairs_command::on #2194
  - context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers #2194
  - context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command #2194
  - terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell #2194
  - workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer #2194
  - workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state #2194
  - workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit #2194
  - workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it #2194
  (The guard flags them only from r30b: earlier collects stored the test under `test`, r30b under `name`. #2215 was a duplicate and is closed.)

## App
- Wayland 32 pass / 0 fail (26 NOREF, 11 skip).
- macOS 34 pass / 1 fail: editor_memory_7mb [terminal] measured -4.67 MB phys_footprint held 75 s after closing, against a band of -24.12..-5.16. The band is PROVISIONAL (n=2), so this is a band question, not a finding (#2185 is the measurement history). Collect AUTO-LOOSENED it -5.16 -> -1.2 MB under its 15 MB auto-loosen limit (bands.json in this commit); anything past that limit needs the owner.

## Lab (Linux local lab, macOS mac slot)
- **#2213 (TEST, root cause proven): every zsh-arm verdict that read rec.log after the first MB of a lease is VOID.** labd `/ctl` returns at most 1,000,000 bytes, and the reader read once from offset 0. The zsh hook logs the app's ~64 KB ssh bootstrap once per session, so after about 15 sessions nothing newer is visible. Evidence: the app side says `exact=6, delivered=remote`, the screenshots show every line run, hook.enter has every line, and u_zsh rec.log on the Mac slot is 1,300,611 bytes. A plain-ssh probe (no Phosphor) of u_zsh recorded 5/5. Void rows (Linux and Mac): lab_connect_typeahead.zsh.* (8 each), lab_tab_no_helper.zsh.* (4), lab_nohelper_tab_hands_back.zsh.* (Linux 2), lab_ctrl_c_no_esc_i.zsh.*.helper (4), lab_enter_start.zsh.150.helper (2), lab_helper_reconnect.zsh.* (2), and the zsh rows of lab_chips / lab_fam_git_read / lab_fam_run_command. Fixed on fix/2213-ctl-reader (0530cc03c); all zsh arms rerun.
- lab_chips (bash, fish, zsh) and lab_fam_git_read / lab_fam_run_command (bash, fish, zsh) nohelper: 2-5 exec channels per prompt, the batch plus a run_script tree listing. #2216 (APP vs TEST needs a spec decision; #2156's one-batch-per-prompt holds).
- lab_remote_session.bash.* (new r30 port): the stand-ins received nothing, and the pager keys landed in the shell line. #2217 (not yet separated).
- lab_tab_remote_dir.p_slowrc.* (Linux): Tab never completed the remote cd. #2218 (not yet separated).
- lab_path_detect.p_exec.lan.nohelper and pane_not_local_after_lab_path_detect.p_exec: `exit` does not return after `exec zsh -l`. #2155 (APP, open, older).
- The new r30 lab families passed on bash: lab_connect_typeahead.bash (all arms), lab_managed_connect.bash, lab_tab_no_helper.bash. lab_msys_ssh_switch SKIPs off Windows by design.

## Found while triaging
- #2214: `selftest.py`'s `__main__` guard sat before the r26-r30 registration lines, so CI ran 436 of 513 harness self-checks. Fixed on fix/2213-ctl-reader (c3bfb5cc1): 514 checks, 0 failed, in the CI form.

## Bands
Held for the owner (WOULD LOOSEN, from collect): windows/agent/latency_enter/enter_to_start 35.1->38.6; windows/agent@gitbash/latency_enter/enter_to_start 50.36->72.24; windows/agent@gitbash/latency_key_log/key_to_frame 4.45->5.41; windows/agent@pwsh7/ctrl_c_d/ctrl_c 1.85->2.47. No Windows app job ran in r30b, so these come from earlier rows.

## Not proven by this run
Nothing Windows-side (build, app and lab not played); r30w covers it. #2203 / #2207 / #2208 stay fixed-untested until a Windows Git Bash run (gitbash_typeahead_reps) and the zsh lab reruns pass.
