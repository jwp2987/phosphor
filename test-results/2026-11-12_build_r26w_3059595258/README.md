# build/r26w  943954d29  (pipeline 3059595258)

- date: 2026-11-12, tier: targeted, pipeline status: success
- compile (check-linux): ok, 122 warnings
- **TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.**
  - asked for on windows: `cb_ui_action_latency`, `ctrl_c_d`, `editor_memory_7mb`, `gitbash_subshell`, `intl_altgr_bash`, `latency_enter`, `latency_first_key_after_idle`, `latency_key_log`, `latency_paint`, `mode_proof`, `ssh_drop_reconnect`, `tabs_streaming`, `tabs_two_windows`, `agent_suggestion_chip_keeps_output`, `alt_screen`, `approval_card_pause`, `arrows_behind_paste`, `big_paste`, `cadence`, `enter_loss_control`, `expert_fuzz`, `fast_typing`, `idle_cpu`, `ime_compose`, `intl_compose_linux`, `intl_deadkey_option`, `intl_ime_japanese`, `intl_ime_pinyin`, `intl_wide_edit`, `intl_wide_paste`, `intl_wide_program`, `intl_wide_type`, `key_loss_control`, `keydown_count`, `link_and_path_click`, `mac_pointer_chord_proof`, `md_viewer`, `md_viewer_ai_answer`, `md_viewer_big`, `minimize_fullscreen`, `network_gate_switch`, `paste_instant_enter`, `primary_and_middle_click`, `read_x`, `resize_reflow_output`, `rollover`, `scroll_keys_position`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_failing_not_local`, `ssh_nested_typeahead`, `subshell_line_editor_proof`, `subshell_ready_docker_t_without_i`, `subshell_ready_exec_fish_rc`, `subshell_ready_rc_clear`, `subshell_ready_rc_questions`, `subshell_ready_relay_shape`, `subshell_ready_secret_after_probe`, `subshell_ready_secret_after_prompt`, `subshell_ready_unsupported_tcsh`, `subshell_ready_vim_typeahead`, `subshell_ready_zsh_newuser_install`, `subshell_sh_probe`, `subshell_sh_probe_empty_ps1`, `subshells`, `suggestion_chip_keeps_output`, `tab_enter`, `ui_drag_drop_file`, `ui_mouse_select_copy`, `ui_palette`, `ui_right_click_paste`, `wheel_scroll`, `agent_close_undo_call_status`, `agent_lrc_no_false_takeover`, `alt_screen_programs`, `cb_agent_pickers`, `desktop_notification`, `desktop_notification_agent_question`, `editor_daily`, `flood_scrollback_find`, `focus_cycle`, `gitbash_typeahead_reps`, `history_survives_restart`, `history_up_recall`, `hostile_output_recovers`, `intl_altgr_ps`, `job_control`, `keybinding_rebind`, `latency_enter_ssh`, `md_viewer_first_frame`, `meta_and_word_keys`, `output_frames`, `remote_heredoc_paste`, `remote_session_ext`, `remote_session_legacy`, `remote_tab_enter_no_helper`, `remote_tab_no_helper`, `restore_shows_history`, `routing_table_powershell`, `scroll_keys_foreground`, `selection_modes`, `soak`, `ssh_connect_typing_wrapper`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_failing_not_local_managed`, `ssh_gitbash_kill_switch`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed`, `ssh_nested_typeahead_wrapper`, `tab_lifecycle_keys`, `tab_pointer_use`, `ui_completions_as_you_type_local`, `ui_copy_wrapped_drag`, `ui_copy_wrapped_output`, `ui_vertical_tabs_many`, `ui_vertical_tabs_panel_click`, `upgrade_over_previous`, `zero_network_startup`, `routing_table`, `startup_shell_pane`
- unit: 16335 passed, 0 failed, 78 skipped of 16335 run (3 suites)
- app (pass/fail/skip): windows 215/43/207 +31 no-ref; soak: pass (windows)

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.1 | - |  |
| app-test-lab-linux | skipped | - | - |  |
| app-test-lab-linux-reuse | manual | - | - | not run |
| app-test-lab-macos | skipped | - | - |  |
| app-test-lab-windows | skipped | - | - |  |
| app-test-linux-wayland | skipped | - | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | skipped | - | - |  |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | skipped | - | - |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-replay-linux | skipped | - | - |  |
| app-test-windows | failed | 354.5 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| bench-macos | manual | - | - | not run |
| bench-remote-server | manual | - | - | not run |
| bench-windows | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | manual | - | - | not run |
| build-macos-arm64 | manual | - | - | not run |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.0 | phosphor-windows-vm |  |
| check-linux | success | 8.6 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| collect-macos-run | manual | - | - | not run |
| collect-windows-run | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| fuzz-smoke-macos | manual | - | - | not run |
| fuzz-smoke-windows | manual | - | - | not run |
| guards | success | 7.3 | - |  |
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
| sshlab-image | manual | - | - | not run |
| test-linux | manual | - | - | not run |
| test-macos | manual | - | - | not run |
| test-shuttle-loom | manual | - | - | not run |
| test-shuttle-loom-macos | manual | - | - | not run |
| test-shuttle-loom-windows | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-warpui-core-macos | manual | - | - | not run |
| test-warpui-core-windows | success | 4.8 | phosphor-windows-vm |  |
| test-windows | success | 44.2 | phosphor-windows-vm |  |
| test-workspace-rest | manual | - | - | not run |
| test-workspace-rest-macos | manual | - | - | not run |
| test-workspace-rest-windows | success | 7.0 | phosphor-windows-vm |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-windows failures

- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off` LEAK (51.2s)
- `warp ai::blocklist::agent_fuzz::matrix::pairs_command::on` LEAK (51.2s)
- `warp context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers` LEAK (0.4s)
- `warp context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command` LEAK (0.5s)
- `warp terminal::input::tests::test_history_file_does_not_route_requests_or_bare_english_words_to_the_shell` LEAK (0.8s)
- `warp workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer` LEAK (2.2s)
- `warp workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state` LEAK (2.0s)
- `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit` LEAK (2.7s)

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | FAIL | terminal mode expected but the screen shows agent markers ('New agent conversation' / 'ESC for terminal' / 'Ask anything'); the app log says |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | injector accounting: injector sent 225; app saw 225; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 220 unbound keydown/typed,  |
| terminal | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it wi |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS | injector accounting: injector sent 2278; app saw 2278; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2216 unbound keydown/ty |
| terminal | rollover | PASS | injector accounting: injector sent 782; app saw 782; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 760 unbound keydown/typed |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal | arrows_behind_paste | PASS | injector accounting: injector sent 132; app saw 132; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 110 unbound keydown/typed,  |
| terminal | keydown_count | PASS | injector accounting: injector sent 644; app saw 644; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 620 unbound keydown/typed |
| terminal | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| terminal | tab_enter | PASS | injector accounting: injector sent 686; app saw 686; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 660 unbound keydown/typed |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 7.26, -> LineEditorReading 31.71; app: |
| terminal | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| terminal | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| terminal | gitbash_subshell | PASS | injector accounting: injector sent 1118; app saw 1118; lost-before-app 0 (Enter: sent 32, app saw 32); of the app's: 1107 unbound keydown/ty |
| terminal | alt_screen_programs | SKIP | not run in the PowerShell pane: vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not inst |
| terminal | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| terminal | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| terminal | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| terminal | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| terminal | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| terminal | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| terminal | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.08 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.77 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | routing_table_powershell | PASS | injector accounting: injector sent 861; app saw 861; lost-before-app 0 (Enter: sent 31, app saw 31); of the app's: 752 unbound keydown/typed |
| terminal | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| terminal | intl_wide_type | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; typed cjk: rec.log empty (hook not run): the command line |
| terminal | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| terminal | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| terminal | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| terminal | intl_wide_edit | PASS | edit 0: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; edit 1: rec.l |
| terminal | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| terminal | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git  |
| terminal | ssh_gitbash_bootstrap_in_command | SKIP | not run in the PowerShell pane: the one-connection managed ssh sends the bootstrap in the remote command (#2051): only the Git Bash pane has |
| terminal | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| terminal | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: the default and PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane are plain ssh (#1933, #2051): only the Git Bash |
| terminal | ui_palette | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | intl_wide_program | SKIP | not run in the PowerShell pane: `head -n 1 > file` is the receiving program: runs in the Git Bash pane |
| terminal | hostile_output_recovers | SKIP | not run in the PowerShell pane: #2072 (T8): `cat` of a binary file, a POSIX command list around hostile_out.py and rec.log for the exact lin |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | latency_enter_ssh | SKIP | not run in the PowerShell pane: the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` |
| terminal | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal | history_up_recall | FAIL | hu.txt holds ['t245_l2j3g8h8', 't246_uu59vu93', 't247_u5z8nkp8', 't246_uu59vu93', 't246_uu59vu93e9z'], expected ['t245_l2j3g8h8', 't246_uu59 |
| terminal | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal | ssh_drop_reconnect | SKIP | not run on Windows: not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535) |
| terminal | scroll_keys_position | SKIP | not run in the PowerShell pane: r23 (#2060): bash `cat` of numbered lines, a bash background job, rec.log; the scroll position is read by Wi |
| terminal | ime_compose | SKIP | not run in the PowerShell pane: r15 T39: the committed text is checked as the UTF-8 bytes of a file written by `echo ... >> file` (Windows P |
| terminal | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal | scroll_keys_foreground | SKIP | not run in the PowerShell pane: #2080: the foreground program is a POSIX raw reader started from a bash script (bytes_reader.sh), with a bas |
| terminal | intl_altgr_ps | PASS | injector accounting: injector sent 170; app saw 170; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 167 unbound keydown/typed,  |
| terminal | intl_altgr_bash | NOREF | injector accounting: injector sent 300; app saw 297; lost-before-app 3 (Enter: sent 5, app saw 5); of the app's: 293 unbound keydown/typed,  |
| terminal | ui_mouse_select_copy | SKIP | not run in the PowerShell pane: T31: the token is printed by bash `printf` and the command is checked against rec.log: runs in the Git Bash  |
| terminal | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| terminal | ui_copy_wrapped_drag | SKIP | not run in the PowerShell pane: T31 (#1976): `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in |
| terminal | ui_right_click_paste | SKIP | not run on Windows: not ported: it falls back to OCR for the menu entry and has never passed anywhere (r14-test-gaps F13); on Windows the sa |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| terminal | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal | startup_shell_pane | SKIP | runs in the pane-shell passes only (terminal@pwsh7, terminal@cmd, terminal@wsl: APPCI_WIN_SHELLS of the app-test-windows job names them; a p |
| terminal | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal | resize_reflow_output | SKIP | not run in the PowerShell pane: T33 (r14 T26): a bash SIGWINCH trap, `stty size` and vim: runs in the Git Bash pane (window size by SetWindo |
| terminal | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal | suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: the fixture is printed with `cat` and its command is confirmed from the bash prompt hook; the chip's  |
| terminal | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| terminal | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 48; app saw 48; lost |
| terminal | md_viewer_big | PASS | injector accounting: injector sent 48; app saw 48; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 44 unbound keydown/typed, 3 t |
| terminal | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal | zero_network_startup | SKIP | not run in the PowerShell pane: the network trace does not depend on the pane's shell and one run is about 4 minutes: runs once, in the Git  |
| terminal | editor_memory_7mb | PASS | judged memory figures: private, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); baselines taken  |
| terminal | restore_shows_history | SKIP | not run in the PowerShell pane: r14 T20: the token is printed by bash `printf '%s%s\n' A B` so the command text never holds it, and rec.log  |
| terminal | ui_vertical_tabs_many | SKIP | not run in the PowerShell pane: T33 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| terminal | ui_vertical_tabs_panel_click | SKIP | not run in the PowerShell pane: T31 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| terminal | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal | network_gate_switch | SKIP | not run in the PowerShell pane: the gate does not depend on the pane's shell (three relaunches): runs once, in the Git Bash pane (#2077) |
| terminal | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal | keybinding_rebind | NOREF | NO-REFERENCE: UNVERIFIABLE: this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is ju |
| terminal | soak | PASS | the app log says terminal but the screen reads as agent (OCR); the log decides; soak: window medians [422, 437, 432] MB, monotonic=True, gro |
| terminal | output_frames | PASS | #1979: idle 2.2 frames/s; shown tab streaming 35.3 frames/s for 31.0 wakeups/s (0 Full); 4 hidden tabs streaming 2.3 frames/s, 124.8 hidden  |
| terminal | tabs_two_windows | FAIL | cleanup of tabs_two_windows: after 13 close key(s) the shell in front has pid None, expected the pass's own shell 10988 (a tab is still open |
| terminal | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| terminal | tabs_streaming | FAIL | typing under 12 streaming tabs: 1/10 exact lines (9 got, 1 expected line(s) missing); injector accounting: injector sent 2644; app saw 2644; |
| terminal | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal | meta_and_word_keys | PASS | raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it); injector account |
| terminal@gitbash | mode_proof | FAIL | terminal mode expected but the screen shows agent markers ('New agent conversation' / 'ESC for terminal' / 'Ask anything'); the app log says |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 223; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 218 unbound keydown/typed,  |
| terminal@gitbash | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS | injector accounting: injector sent 2061; app saw 2061; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1999 unbound keydown/ty |
| terminal@gitbash | rollover | PASS | injector accounting: injector sent 702; app saw 702; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 680 unbound keydown/typed |
| terminal@gitbash | expert_fuzz | PASS | injector accounting: injector sent 743; app saw 743; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 658 unbound keydown/typed |
| terminal@gitbash | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal@gitbash | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 136 unbound keydown/typed,  |
| terminal@gitbash | keydown_count | PASS | injector accounting: injector sent 564; app saw 564; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| terminal@gitbash | big_paste | NOREF | big_paste: time-to-Enter not measured (big.out never reached its full 1000001 bytes within the wait: the byte check below says why); NO-REFE |
| terminal@gitbash | tab_enter | PASS | injector accounting: injector sent 482; app saw 482; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 456 unbound keydown/typed |
| terminal@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| terminal@gitbash | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 73 unbound keydown/typed, 2 t |
| terminal@gitbash | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | alt_screen | PASS | injector accounting: injector sent 27; app saw 27; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 24 unbound keydown/typed, 2 t |
| terminal@gitbash | gitbash_subshell | SKIP | the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells; injector accounting: injector sent 2; app saw 2; los |
| terminal@gitbash | alt_screen_programs | NOREF | NO-REFERENCE: UNVERIFIABLE: htop is not installed (package htop): that part of the alt-screen coverage did not run; NO-REFERENCE: UNVERIFIAB |
| terminal@gitbash | subshell_ready_rc_questions | PASS | rcq: `exit` NOT typed: the probe ran in the local shell and the shell count under the app is unknown (base None, now None); no root pid reco |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| terminal@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| terminal@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | PASS | clear: setup armed decision=Wait(Unknown); no setup progress event logged; Windows Git Bash: known issue #2025, the ~83 KB bootstrap is type |
| terminal@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| terminal@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| terminal@gitbash | ssh_connect_typing_wrapper | FAIL | ssh_connect_typing (gap 0.0 s): T244_5hxqt8 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal@gitbash | pane_switch_left_on_after_ssh_connect_typing_wrapper | FAIL | after ssh_connect_typing_wrapper: the `unset PHOSPHOR_MSYS_MANAGED_SSH` after the case is not proven in the pane's local shell: the proof li |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 672; app saw 672; lost-before-app 0 (Enter: sent 23, app saw 23); of the app |
| terminal@gitbash | pane_not_local_after_ssh_nested_typeahead_wrapper | FAIL | after ssh_nested_typeahead_wrapper the pane could not be proven back in the local shell (remote); the app is relaunched so later cases start |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.21 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | idle_cpu | PASS | injector accounting: injector sent 4; app saw 4; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 4 take |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 44.0 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| terminal@gitbash | latency_key_log | FAIL | REGRESSION key to frame: median 3.96 vs baseline build +0.56 ms (+16.5%); re-measured once: median 4.02 (fail); #1176 key -> pty write media |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| terminal@gitbash | latency_first_key_after_idle | FAIL | REGRESSION First key after 2 s idle: median 4.2 vs baseline build +0.59 ms (+16.3%); re-measured once: median 4.26 (fail); First key after 1 |
| terminal@gitbash | routing_table | PASS | injector accounting: injector sent 645; app saw 645; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| terminal@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| terminal@gitbash | remote_session_legacy | NOREF | mode guard: the pane was in agent before this terminal case; put back with escape; NO-REFERENCE: [legacy] clean-env subshell step NOT RUN on |
| terminal@gitbash | intl_wide_type | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 151 unbound keydown/typed,  |
| terminal@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| terminal@gitbash | remote_tab_no_helper | FAIL | after the completed cd the remote cwd should be .../phxtab_t325_c8ebxp, got []; the app log has no 'candidates from the remote shell' line:  |
| terminal@gitbash | pane_not_local_after_remote_tab_no_helper | FAIL | after remote_tab_no_helper the pane could not be proven back in the local shell (remote); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['cd ~cd phxte_t348_f'], want the COMPLETED 'cd phxte_t348_faeblr/' (the uncompleted 'cd  |
| terminal@gitbash | pane_not_local_after_remote_tab_enter_no_helper | FAIL | after remote_tab_enter_no_helper the pane could not be proven back in the local shell (none); the app is relaunched so later cases start cle |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| terminal@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| terminal@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| terminal@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| terminal@gitbash | ssh_gitbash_managed | FAIL | not provably remote (probe ran in: None), `exit` NOT typed; injector accounting: injector sent 646; app saw 646; lost-before-app 0 (Enter: s |
| terminal@gitbash | pane_not_local_after_ssh_gitbash_managed | FAIL | after ssh_gitbash_managed the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.33 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal@gitbash | ssh_gitbash_bootstrap_in_command | FAIL | not provably remote (probe ran in: None), `exit` NOT typed; the test sshd logged no client-input count for this session (only the Windows fa |
| terminal@gitbash | pane_not_local_after_ssh_gitbash_bootstrap_in_command | FAIL | after ssh_gitbash_bootstrap_in_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: `exit` did not bring the local prompt back; trial 2: the line typed during the connect never reached the remote shell. Test host: s |
| terminal@gitbash | pane_not_local_after_ssh_gitbash_typeahead_managed | FAIL | after ssh_gitbash_typeahead_managed the pane could not be proven back in the local shell (remote); the app is relaunched so later cases star |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 391; app saw 391; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 379 unbound keydown/typed |
| terminal@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); control: no command row was readable in the  |
| terminal@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| terminal@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 0.0 ms (< 16 ms; before: 0.0 ms); injector accounting: injector sent 742; app saw 742; lost-before |
| terminal@gitbash | ui_completions_as_you_type_local | FAIL | no completion menu with both candidates after typing a shared prefix without Tab; fixture dir C:\gl\b\07xLPX_fG\0\winters-phosphor\phosphor\ |
| terminal@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NO-REFERENCE: En |
| terminal@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal@gitbash | history_up_recall | PASS | injector accounting: injector sent 139; app saw 139; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 118 unbound keydown/typed,  |
| terminal@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| terminal@gitbash | ssh_drop_reconnect | SKIP | not run on Windows: not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535) |
| terminal@gitbash | scroll_keys_position | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed,  |
| terminal@gitbash | ime_compose | SKIP | Windows: NEEDS: the language Chinese (Simplified, China) with the Microsoft Pinyin keyboard added for the runner user on the Windows VM (Set |
| terminal@gitbash | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal@gitbash | scroll_keys_foreground | PASS | the view's position was not read from the screen (NEEDS: an OCR engine on this runner: tesseract-ocr on Linux and Windows, Apple Vision on m |
| terminal@gitbash | intl_altgr_ps | SKIP | PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane); injector accounting: injector sent 2; app s |
| terminal@gitbash | intl_altgr_bash | PASS | injector accounting: injector sent 79; app saw 79; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 74 unbound keydown/typed, 2 t |
| terminal@gitbash | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| terminal@gitbash | ui_copy_wrapped_output | PASS | injector accounting: injector sent 238; app saw 238; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 221 unbound keydown/typed,  |
| terminal@gitbash | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 223 unbound keydown/typed,  |
| terminal@gitbash | ui_right_click_paste | SKIP | not run on Windows: not ported: it falls back to OCR for the menu entry and has never passed anywhere (r14-test-gaps F13); on Windows the sa |
| terminal@gitbash | selection_modes | PASS | injector accounting: injector sent 300; app saw 300; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 275 unbound keydown/typed,  |
| terminal@gitbash | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| terminal@gitbash | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness. NEEDS: a harness capability that is not built (an OLE dr |
| terminal@gitbash | startup_shell_pane | SKIP | runs in the pane-shell passes only (terminal@pwsh7, terminal@cmd, terminal@wsl: APPCI_WIN_SHELLS of the app-test-windows job names them; a p |
| terminal@gitbash | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| terminal@gitbash | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| terminal@gitbash | resize_reflow_output | NOREF | NO-REFERENCE: UNVERIFIABLE: the resize case was not run: NEEDS: a desktop of at least 1300x800 px on the Windows VM (it is 1280x800): the wi |
| terminal@gitbash | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| terminal@gitbash | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| terminal@gitbash | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8480; app saw 8480; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8251 unbound keydown/ |
| terminal@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| terminal@gitbash | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| terminal@gitbash | md_viewer_big | PASS | injector accounting: injector sent 53; app saw 53; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 49 unbound keydown/typed, 3 t |
| terminal@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| terminal@gitbash | zero_network_startup | PASS | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; scripted session: settings page |
| terminal@gitbash | editor_memory_7mb | PASS | judged memory figures: private, rss (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages); baselines taken  |
| terminal@gitbash | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| terminal@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| terminal@gitbash | ui_vertical_tabs_panel_click | PASS | vertical tabs: on in the profile's settings (the owner's layout); panel rows located from the app's find hook (the newest sighting in each t |
| terminal@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| terminal@gitbash | network_gate_switch | NOREF | Escape left the settings page open (the shell did not take the probe line); closed it with key ctrl+shift+w; NO-REFERENCE: UNVERIFIABLE: the |
| terminal@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| terminal@gitbash | keybinding_rebind | PASS | injector accounting: injector sent 848; app saw 848; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 784 unbound keydown/typed |
| terminal@gitbash | soak | PASS | soak: window medians [427, 440, 438] MB, monotonic=True, growth +12 MB (limit 150 MB); injector accounting: injector sent 3992; app saw 3992 |
| terminal@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 9 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams wer |
| terminal@gitbash | tabs_two_windows | FAIL | typing/paste in window 2 under 12 streaming tabs: 1/16 exact lines (1 got, 15 expected line(s) missing); cleanup of tabs_two_windows: after  |
| terminal@gitbash | upgrade_over_previous | NOREF | NO-REFERENCE: UNVERIFIABLE: the PREVIOUS release could not be brought up and driven on this pass's profile, so the build under test was not  |
| terminal@gitbash | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| terminal@gitbash | tabs_streaming | FAIL | typing under 12 streaming tabs: 2/10 exact lines (9 got, 1 expected line(s) missing); injector accounting: injector sent 2524; app saw 2524; |
| terminal@gitbash | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| terminal@pwsh7 | mode_proof | FAIL | terminal mode expected but the screen shows agent markers ('New agent conversation' / 'ESC for terminal' / 'Ask anything'); the app log says |
| terminal@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@pwsh7 | fast_typing | PASS | injector accounting: injector sent 2247; app saw 2247; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2185 unbound keydown/ty |
| terminal@pwsh7 | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| terminal@pwsh7 | tab_enter | PASS | injector accounting: injector sent 674; app saw 674; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 648 unbound keydown/typed |
| terminal@pwsh7 | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 6.88, -> LineEditorReading 29.64; app: |
| terminal@pwsh7 | routing_table_powershell | PASS | injector accounting: injector sent 860; app saw 860; lost-before-app 0 (Enter: sent 31, app saw 31); of the app's: 751 unbound keydown/typed |
| terminal@pwsh7 | history_up_recall | FAIL | hu.txt holds ['t86_xy7ug0wi', 't87_ect8u0tu', 't88_wru76a7h', 't87_ect8u0tu', 't87_ect8u0tu9z'], expected ['t86_xy7ug0wi', 't87_ect8u0tu', ' |
| terminal@pwsh7 | startup_shell_pane | PASS | pwsh7 pane shell resolved to: C:\Program Files\WindowsApps\Microsoft.PowerShell_7.7.0.0_x64__8wekyb3d8bbwe\pwsh.exe; injector accounting: in |
| terminal@wsl | startup_shell_pass | SKIP | a WSL distribution is not installed on this machine: the wsl pass cannot run (the harness installs nothing). NEEDS: the Windows Subsystem fo |
| agent | mode_proof | PASS |  |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | cadence | PASS | injector accounting: injector sent 225; app saw 225; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 220 unbound keydown/typed,  |
| agent | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| agent | fast_typing | PASS | injector accounting: injector sent 2247; app saw 2247; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2185 unbound keydown/ty |
| agent | rollover | PASS | injector accounting: injector sent 762; app saw 762; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 740 unbound keydown/typed |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent | arrows_behind_paste | PASS | injector accounting: injector sent 132; app saw 132; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 110 unbound keydown/typed,  |
| agent | keydown_count | PASS | injector accounting: injector sent 643; app saw 643; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 620 unbound keydown/typed |
| agent | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| agent | tab_enter | PASS | injector accounting: injector sent 682; app saw 682; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 656 unbound keydown/typed |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 7.2, -> LineEditorReading 33.11; app:  |
| agent | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| agent | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| agent | gitbash_subshell | PASS | injector accounting: injector sent 1118; app saw 1118; lost-before-app 0 (Enter: sent 32, app saw 32); of the app's: 1107 unbound keydown/ty |
| agent | alt_screen_programs | SKIP | not run in the PowerShell pane: vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not inst |
| agent | subshell_ready_rc_questions | SKIP | not run in the PowerShell pane: bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses scree |
| agent | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| agent | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| agent | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent | subshell_ready_rc_clear | SKIP | not run in the PowerShell pane: bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts) |
| agent | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| agent | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| agent | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent | ssh_connect_typing_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| agent | ssh_nested_typeahead_wrapper | SKIP | not run in the PowerShell pane: bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a variable of the app-test-windows jo |
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.13 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 4.24 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| agent | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached th |
| agent | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent | cb_agent_pickers | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git  |
| agent | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| agent | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; typed acce |
| agent | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| agent | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801):  |
| agent | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801) |
| agent | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes wer |
| agent | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| agent | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| agent | intl_wide_edit | PASS | edit 0: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; edit 1: rec.l |
| agent | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| agent | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git  |
| agent | ssh_gitbash_bootstrap_in_command | SKIP | not run in the PowerShell pane: the one-connection managed ssh sends the bootstrap in the remote command (#2051): only the Git Bash pane has |
| agent | ssh_gitbash_typeahead_managed | SKIP | not run in the PowerShell pane: keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wr |
| agent | ssh_gitbash_kill_switch | SKIP | not run in the PowerShell pane: the default and PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane are plain ssh (#1933, #2051): only the Git Bash |
| agent | ui_palette | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| agent | intl_wide_program | SKIP | not run in the PowerShell pane: `head -n 1 > file` is the receiving program: runs in the Git Bash pane |
| agent | hostile_output_recovers | SKIP | not run in the PowerShell pane: #2072 (T8): `cat` of a binary file, a POSIX command list around hostile_out.py and rec.log for the exact lin |
| agent | latency_enter_ssh | SKIP | not run in the PowerShell pane: the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` |
| agent | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent | history_up_recall | FAIL | hu.txt holds ['t189_uvs7ic2x', 't190_cbu0k9c7', 't191_1lmohi6h', 't190_cbu0k9c7', 't190_cbu0k9c79z'], expected ['t189_uvs7ic2x', 't190_cbu0k |
| agent | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent | scroll_keys_position | SKIP | not run in the PowerShell pane: r23 (#2060): bash `cat` of numbered lines, a bash background job, rec.log; the scroll position is read by Wi |
| agent | ime_compose | SKIP | not run in the PowerShell pane: r15 T39: the committed text is checked as the UTF-8 bytes of a file written by `echo ... >> file` (Windows P |
| agent | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent | scroll_keys_foreground | SKIP | not run in the PowerShell pane: #2080: the foreground program is a POSIX raw reader started from a bash script (bytes_reader.sh), with a bas |
| agent | intl_altgr_ps | PASS | injector accounting: injector sent 170; app saw 170; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 167 unbound keydown/typed,  |
| agent | intl_altgr_bash | NOREF | injector accounting: injector sent 300; app saw 297; lost-before-app 3 (Enter: sent 5, app saw 5); of the app's: 293 unbound keydown/typed,  |
| agent | ui_mouse_select_copy | SKIP | not run in the PowerShell pane: T31: the token is printed by bash `printf` and the command is checked against rec.log: runs in the Git Bash  |
| agent | ui_copy_wrapped_output | SKIP | not run in the PowerShell pane: #1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the G |
| agent | ui_copy_wrapped_drag | SKIP | not run in the PowerShell pane: T31 (#1976): `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in |
| agent | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (p |
| agent | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| agent | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| agent | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| agent | resize_reflow_output | SKIP | not run in the PowerShell pane: T33 (r14 T26): a bash SIGWINCH trap, `stty size` and vim: runs in the Git Bash pane (window size by SetWindo |
| agent | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| agent | suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: the fixture is printed with `cat` and its command is confirmed from the bash prompt hook; the chip's  |
| agent | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| agent | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in |
| agent | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| agent | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| agent | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 48; app saw 48; lost |
| agent | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| agent | restore_shows_history | SKIP | not run in the PowerShell pane: r14 T20: the token is printed by bash `printf '%s%s\n' A B` so the command text never holds it, and rec.log  |
| agent | ui_vertical_tabs_many | SKIP | not run in the PowerShell pane: T33 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| agent | ui_vertical_tabs_panel_click | SKIP | not run in the PowerShell pane: T31 (#1990): every tab's `cd`, `echo` and `pwd -W` lines are bash and are checked against rec.log: runs in t |
| agent | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| agent | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent | keybinding_rebind | NOREF | NO-REFERENCE: UNVERIFIABLE: this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is ju |
| agent | soak | PASS | soak: window medians [508, 526, 512] MB, monotonic=False, growth +5 MB (limit 150 MB); injector accounting: injector sent 4414; app saw 4414 |
| agent | output_frames | PASS | #1979: idle 2.0 frames/s; shown tab streaming 34.8 frames/s for 31.3 wakeups/s (0 Full); 4 hidden tabs streaming 2.2 frames/s, 124.2 hidden  |
| agent | tabs_two_windows | FAIL | cleanup of tabs_two_windows: after 13 close key(s) the shell in front has pid None, expected the pass's own shell 10520 (a tab is still open |
| agent | history_survives_restart | PASS | no hop=view_mode line in the app log: the mode is read from the screen (OCR): agent; no quit key on this platform: the app was stopped by pi |
| agent | tabs_streaming | PASS | injector accounting: injector sent 2643; app saw 2643; lost-before-app 0 (Enter: sent 37, app saw 37); of the app's: 2564 unbound keydown/ty |
| agent | meta_and_word_keys | PASS | raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it); injector account |
| agent@gitbash | mode_proof | PASS |  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | cadence | PASS | injector accounting: injector sent 223; app saw 223; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 218 unbound keydown/typed,  |
| agent@gitbash | mac_pointer_chord_proof | SKIP | not run on Windows: r15 T32 (#2061): macOS only by design: it proves the Mac harness's own pointer tool (mptr) and app-only chords (appkey) |
| agent@gitbash | fast_typing | PASS | injector accounting: injector sent 2006; app saw 2006; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1944 unbound keydown/ty |
| agent@gitbash | rollover | PASS | injector accounting: injector sent 682; app saw 682; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 660 unbound keydown/typed |
| agent@gitbash | expert_fuzz | PASS | injector accounting: injector sent 742; app saw 742; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 664 unbound keydown/typed |
| agent@gitbash | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 4 unbound keydown/typed, 8 ta |
| agent@gitbash | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 157; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 137 unbound keydown/typed,  |
| agent@gitbash | keydown_count | PASS | injector accounting: injector sent 563; app saw 563; lost-before-app 0 (Enter: sent 20, app saw 20); of the app's: 540 unbound keydown/typed |
| agent@gitbash | big_paste | NOREF | big_paste: time-to-Enter not measured (big.out never reached its full 1000001 bytes within the wait: the byte check below says why); NO-REFE |
| agent@gitbash | tab_enter | PASS | injector accounting: injector sent 479; app saw 479; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 453 unbound keydown/typed |
| agent@gitbash | ctrl_c_d | NOREF | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; |
| agent@gitbash | read_x | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 73 unbound keydown/typed, 2 t |
| agent@gitbash | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | alt_screen | PASS | injector accounting: injector sent 27; app saw 27; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 24 unbound keydown/typed, 2 t |
| agent@gitbash | gitbash_subshell | SKIP | the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells; injector accounting: injector sent 2; app saw 2; los |
| agent@gitbash | alt_screen_programs | NOREF | NO-REFERENCE: UNVERIFIABLE: htop is not installed (package htop): that part of the alt-screen coverage did not run; NO-REFERENCE: UNVERIFIAB |
| agent@gitbash | subshell_ready_rc_questions | PASS | rcq: `exit` NOT typed: the probe ran in the local shell and the shell count under the app is unknown (base None, now None); no root pid reco |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS fac |
| agent@gitbash | subshell_ready_secret_after_probe | SKIP | not run on Windows: as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not  |
| agent@gitbash | subshell_sh_probe_empty_ps1 | SKIP | not run on Windows: dash does not exist in a Windows pane; the sh probe path is POSIX (#1935) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3); by design, no machine change makes it run here |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3); by design, no machine change makes it run here |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | PASS | clear: setup armed decision=Wait(Unknown); no setup progress event logged; Windows Git Bash: known issue #2025, the ~83 KB bootstrap is type |
| agent@gitbash | subshell_line_editor_proof | SKIP | not run on Windows: the line-editor write is recorded from the tty probe (termios + /proc), which a ConPTY pane does not have (#1991) |
| agent@gitbash | job_control | SKIP | not run on Windows: #2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control ke |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing loss: 0 of 12 typed-ahead commands lost (observed 0.0 %); 95 % upper confidence bound 22.1 %; this n cannot exclude a 1/4 |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3; injector accounting: injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12); of the app |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 137; app saw 137; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's |
| agent@gitbash | ssh_connect_typing_wrapper | FAIL | ssh_connect_typing (gap 0.0 s): T159_6tn8e9 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| agent@gitbash | pane_not_local_after_ssh_connect_typing_wrapper | FAIL | after ssh_connect_typing_wrapper the pane could not be proven back in the local shell (remote); the app is relaunched so later cases start c |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_nested_typeahead_wrapper | PASS | ssh trial verdicts: exact=1; injector accounting: injector sent 672; app saw 672; lost-before-app 0 (Enter: sent 23, app saw 23); of the app |
| agent@gitbash | pane_not_local_after_ssh_nested_typeahead_wrapper | FAIL | after ssh_nested_typeahead_wrapper the pane could not be proven back in the local shell (remote); the app is relaunched so later cases start |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.26 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent@gitbash | idle_cpu | PASS | injector accounting: injector sent 3; app saw 3; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 3 take |
| agent@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 42.4 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Ente |
| agent@gitbash | latency_key_log | PASS | #1176 key -> pty write median 0.66 ms (p95 0.84, n=60): within the 1 ms target; #1995 notifies logged before 10 of 78 key frame build(s): In |
| agent@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare; injector accounting:  |
| agent@gitbash | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so t |
| agent@gitbash | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sen |
| agent@gitbash | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed |
| agent@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| agent@gitbash | agent_lrc_no_false_takeover | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Wa |
| agent@gitbash | cb_agent_pickers | PASS | injector accounting: injector sent 77; app saw 77; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 65 unbound keydown/typed, 11  |
| agent@gitbash | remote_session_legacy | NOREF | NO-REFERENCE: [legacy] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6);  |
| agent@gitbash | intl_wide_type | PASS | injector accounting: injector sent 159; app saw 159; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 151 unbound keydown/typed,  |
| agent@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the |
| agent@gitbash | agent_view_not_restored_after_remote_session_ext | PASS | remote_session_ext relaunched the app itself (pid 13000 -> 10532); a relaunched app starts in a terminal session, so this is not an app find |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | remote_tab_no_helper | FAIL | after the completed cd the remote cwd should be .../phxtab_t254_282xnd, got []; the app log has no 'candidates from the remote shell' line:  |
| agent@gitbash | pane_not_local_after_remote_tab_no_helper | FAIL | after remote_tab_no_helper the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['cd ~cd phxte_t272_y'], want the COMPLETED 'cd phxte_t272_yswlrl/' (the uncompleted 'cd  |
| agent@gitbash | pane_not_local_after_remote_tab_enter_no_helper | FAIL | after remote_tab_enter_no_helper the pane could not be proven back in the local shell (remote); the app is relaunched so later cases start c |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 ti |
| agent@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs the Linux find flow and POSIX floods (the OCR half exists since r23: winocr) |
| agent@gitbash | remote_heredoc_paste | NOREF | bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill  |
| agent@gitbash | intl_wide_edit | PASS | injector accounting: injector sent 208; app saw 208; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 190 unbound keydown/typed,  |
| agent@gitbash | ssh_gitbash_managed | FAIL | not provably remote (probe ran in: None), `exit` NOT typed; injector accounting: injector sent 646; app saw 646; lost-before-app 0 (Enter: s |
| agent@gitbash | pane_not_local_after_ssh_gitbash_managed | FAIL | after ssh_gitbash_managed the pane could not be proven back in the local shell (remote); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.30 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent@gitbash | ssh_gitbash_bootstrap_in_command | FAIL | not provably remote (probe ran in: None), `exit` NOT typed; the test sshd logged no client-input count for this session (only the Windows fa |
| agent@gitbash | pane_not_local_after_ssh_gitbash_bootstrap_in_command | FAIL | after ssh_gitbash_bootstrap_in_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_typeahead_managed | FAIL | trial 1: `exit` did not bring the local prompt back; trial 2: the line typed during the connect never reached the remote shell. Test host: s |
| agent@gitbash | pane_not_local_after_ssh_gitbash_typeahead_managed | FAIL | after ssh_gitbash_typeahead_managed the pane could not be proven back in the local shell (remote); the app is relaunched so later cases star |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_gitbash_kill_switch | PASS | injector accounting: injector sent 391; app saw 391; lost-before-app 0 (Enter: sent 14, app saw 14); of the app's: 379 unbound keydown/typed |
| agent@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); control: no command row was readable in the  |
| agent@gitbash | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 282; lost-before-app 0 (Enter: sent 16, app saw 16); of the app's: 268 unbound keydown/typed |
| agent@gitbash | hostile_output_recovers | PASS | largest main-thread item after the output 0.0 ms (< 16 ms; before: 7.6 ms); injector accounting: injector sent 742; app saw 742; lost-before |
| agent@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NO-REFERENCE: En |
| agent@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent@gitbash | history_up_recall | PASS | injector accounting: injector sent 138; app saw 138; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 118 unbound keydown/typed,  |
| agent@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource). NEEDS: a Windows IME arm in the harness (not built; r14-test-gaps T39) |
| agent@gitbash | scroll_keys_position | NOREF | control: the job printed 23 lines and none is on screen at the bottom of the agent view (shots/*skp_0_stream.png): the agent view does not d |
| agent@gitbash | ime_compose | SKIP | Windows: NEEDS: the language Chinese (Simplified, China) with the Microsoft Pinyin keyboard added for the runner user on the Windows VM (Set |
| agent@gitbash | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent@gitbash | scroll_keys_foreground | PASS | the view's position was not read from the screen (NEEDS: an OCR engine on this runner: tesseract-ocr on Linux and Windows, Apple Vision on m |
| agent@gitbash | intl_altgr_ps | SKIP | PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane); injector accounting: injector sent 2; app s |
| agent@gitbash | intl_altgr_bash | PASS | injector accounting: injector sent 79; app saw 79; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 74 unbound keydown/typed, 2 t |
| agent@gitbash | ui_mouse_select_copy | PASS | injector accounting: injector sent 201; app saw 201; lost-before-app 0 (Enter: sent 5, app saw 5); of the app's: 192 unbound keydown/typed,  |
| agent@gitbash | ui_copy_wrapped_output | PASS | injector accounting: injector sent 238; app saw 238; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 222 unbound keydown/typed,  |
| agent@gitbash | ui_copy_wrapped_drag | PASS | injector accounting: injector sent 234; app saw 234; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 223 unbound keydown/typed,  |
| agent@gitbash | selection_modes | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops |
| agent@gitbash | primary_and_middle_click | SKIP | not run on Windows: T23: the PRIMARY selection is an X11 / Wayland concept; Windows has one clipboard |
| agent@gitbash | link_and_path_click | SKIP | not run on Windows: T21: not ported: the recording opener is a POSIX xdg-open on the app's PATH and the network trace is strace (the pointer |
| agent@gitbash | wheel_scroll | SKIP | not run on Windows: T24: not ported: bytes_reader.py needs termios (the wheel event exists since r15 wave 4: winapi.mouse_wheel_event; the O |
| agent@gitbash | resize_reflow_output | NOREF | NO-REFERENCE: UNVERIFIABLE: the resize case was not run: NEEDS: a desktop of at least 1300x800 px on the Windows VM (it is 1280x800): the wi |
| agent@gitbash | editor_daily | PASS | replace: windows hand-over: phosphor.exe file:///C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/appci-out/windows/run/home/<user>/edd/replace |
| agent@gitbash | suggestion_chip_keeps_output | PASS | the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output reads |
| agent@gitbash | focus_cycle | PASS | focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as ESC[I / ESC[ |
| agent@gitbash | gitbash_typeahead_reps | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+= |
| agent@gitbash | minimize_fullscreen | SKIP | not run on Windows: r14 T30: no window-state call (minimise, restore, fullscreen) in the Windows backend yet (r14-test-gaps T33 built the re |
| agent@gitbash | desktop_notification | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; 'under the threshold: non |
| agent@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; injector accounting: injector sent 53; app saw 53; lost |
| agent@gitbash | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 211; lost-before-app 0 (Enter: sent 1, app saw 1); of the app's: 207 unbound keydown/typed,  |
| agent@gitbash | md_viewer_first_frame | PASS | the viewer laid its file out at its first size 937x564.76 (1 held edit(s)); no block was painted without a width; control arm (the same file |
| agent@gitbash | restore_shows_history | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this proves the restore after a stop, not after a quit; injector acc |
| agent@gitbash | ui_vertical_tabs_many | PASS | vertical tabs: on in the profile's settings (the owner's layout); injector accounting: injector sent 2974; app saw 2974; lost-before-app 0 ( |
| agent@gitbash | ui_vertical_tabs_panel_click | NOREF | vertical tabs: on in the profile's settings (the owner's layout); NO-REFERENCE: UNVERIFIABLE: the panel rows of tabs 2..4 could not be locat |
| agent@gitbash | tab_pointer_use | SKIP | not run on Windows: r14 T28: not ported in r15 wave 4 (r14-test-gaps T31 names five cases, this is not one): the pointer, the middle button  |
| agent@gitbash | tab_lifecycle_keys | PASS | the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are); injector accounting: injector  |
| agent@gitbash | keybinding_rebind | PASS | injector accounting: injector sent 835; app saw 835; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 772 unbound keydown/typed |
| agent@gitbash | soak | PASS | soak: window medians [406, 413, 405] MB, monotonic=True, growth -0 MB (limit 150 MB); injector accounting: injector sent 4020; app saw 4020; |
| agent@gitbash | output_frames | NOREF | NO-REFERENCE: UNVERIFIABLE: hidden streaming tabs: only 11 wakeups of hidden views in 6 s with 4 tabs meant to stream: the hidden streams we |
| agent@gitbash | tabs_two_windows | FAIL | typing/paste in window 2 under 12 streaming tabs: 5/16 exact lines (5 got, 11 expected line(s) missing); cleanup of tabs_two_windows: after  |
| agent@gitbash | history_survives_restart | PASS | no quit key on this platform: the app was stopped by pid (stop-by-pid); this round proves history after a stop, the next after kill -9; inje |
| agent@gitbash | tabs_streaming | FAIL | typing under 12 streaming tabs: 2/10 exact lines (9 got, 1 expected line(s) missing); injector accounting: injector sent 2498; app saw 2498; |
| agent@gitbash | meta_and_word_keys | PASS | injector accounting: injector sent 741; app saw 741; lost-before-app 0 (Enter: sent 18, app saw 18); of the app's: 597 unbound keydown/typed |
| agent@pwsh7 | mode_proof | PASS |  |
| agent@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@pwsh7 | fast_typing | PASS | injector accounting: injector sent 2246; app saw 2246; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2184 unbound keydown/ty |
| agent@pwsh7 | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 14; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 0 unbound keydown/typed, 8 ta |
| agent@pwsh7 | tab_enter | PASS | injector accounting: injector sent 674; app saw 674; lost-before-app 0 (Enter: sent 12, app saw 12); of the app's: 648 unbound keydown/typed |
| agent@pwsh7 | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 7.62, -> LineEditorReading 30.74; app: |
| agent@pwsh7 | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached th |
| agent@pwsh7 | history_up_recall | FAIL | hu.txt holds ['t85_0wiect8u', 't86_0tuwru76', 't87_a7hjuuue', 't86_0tuwru76', 't86_0tuwru769z'], expected ['t85_0wiect8u', 't86_0tuwru76', ' |
| agent@wsl | startup_shell_pass | SKIP | a WSL distribution is not installed on this machine: the wsl pass cannot run (the harness installs nothing). NEEDS: the Windows Subsystem fo |
| agent@fake | agent_suggestion_chip_keeps_output | SKIP | not run in the PowerShell pane: #2138: as suggestion_chip_keeps_output (a `cat` confirmed from the bash prompt hook): runs once, in the Git  |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | desktop_notification_agent_question | PASS | no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines; injector accounting: inje |
| agent@fake | agent_close_undo_call_status | SKIP | not run in the PowerShell pane: #2103: the scripted command is a POSIX printf whose output is not in its command line; the agent path does n |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_suggestion_chip_keeps_output | PASS | the suggestion going is not judged in the agent view: Escape leaves the view there, a layout change of its own (the Rust view test a_late_pr |
| agent@gitbash+fake | desktop_notification_agent_question | PASS | the app log says agent but the screen reads as terminal (OCR); the log decides; no recording notification service on this platform: the verd |
| agent@gitbash+fake | agent_close_undo_call_status | PASS | the close key asked first (a waiting agent turn): answered with Enter; injector accounting: injector sent 307; app saw 307; lost-before-app  |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## PROVING cases (owner decides graduation)

| case | platform | mode | status |
|---|---|---|---|
| agent_lrc_no_false_takeover | windows | agent | PASS |
| agent_lrc_no_false_takeover | windows | agent@gitbash | PASS |
| alt_screen_programs | windows | agent | SKIP |
| alt_screen_programs | windows | agent@gitbash | NOREF |
| alt_screen_programs | windows | terminal | SKIP |
| alt_screen_programs | windows | terminal@gitbash | NOREF |
| big_paste | windows | agent | SKIP |
| big_paste | windows | agent@gitbash | NOREF |
| big_paste | windows | terminal | SKIP |
| big_paste | windows | terminal@gitbash | NOREF |
| editor_daily | windows | agent | PASS |
| editor_daily | windows | agent@gitbash | PASS |
| editor_daily | windows | terminal | PASS |
| editor_daily | windows | terminal@gitbash | PASS |
| editor_memory_7mb | windows | terminal | PASS |
| editor_memory_7mb | windows | terminal@gitbash | PASS |
| flood_scrollback_find | windows | agent | SKIP |
| flood_scrollback_find | windows | agent@gitbash | SKIP |
| flood_scrollback_find | windows | terminal | SKIP |
| flood_scrollback_find | windows | terminal@gitbash | SKIP |
| focus_cycle | windows | agent | PASS |
| focus_cycle | windows | agent@gitbash | PASS |
| focus_cycle | windows | terminal | PASS |
| focus_cycle | windows | terminal@gitbash | PASS |
| gitbash_typeahead_reps | windows | agent | SKIP |
| gitbash_typeahead_reps | windows | agent@gitbash | PASS |
| gitbash_typeahead_reps | windows | terminal | SKIP |
| gitbash_typeahead_reps | windows | terminal@gitbash | PASS |
| history_survives_restart | windows | agent | PASS |
| history_survives_restart | windows | agent@gitbash | PASS |
| history_survives_restart | windows | terminal | PASS |
| history_survives_restart | windows | terminal@gitbash | PASS |
| history_up_recall | windows | agent | FAIL |
| history_up_recall | windows | agent@gitbash | PASS |
| history_up_recall | windows | agent@pwsh7 | FAIL |
| history_up_recall | windows | terminal | FAIL |
| history_up_recall | windows | terminal@gitbash | PASS |
| history_up_recall | windows | terminal@pwsh7 | FAIL |
| hostile_output_recovers | windows | agent | SKIP |
| hostile_output_recovers | windows | agent@gitbash | PASS |
| hostile_output_recovers | windows | terminal | SKIP |
| hostile_output_recovers | windows | terminal@gitbash | PASS |
| job_control | windows | agent | SKIP |
| job_control | windows | agent@gitbash | SKIP |
| job_control | windows | terminal | SKIP |
| job_control | windows | terminal@gitbash | SKIP |
| keybinding_rebind | windows | agent | NOREF |
| keybinding_rebind | windows | agent@gitbash | PASS |
| keybinding_rebind | windows | terminal | NOREF |
| keybinding_rebind | windows | terminal@gitbash | PASS |
| link_and_path_click | windows | agent | SKIP |
| link_and_path_click | windows | agent@gitbash | SKIP |
| link_and_path_click | windows | terminal | SKIP |
| link_and_path_click | windows | terminal@gitbash | SKIP |
| mac_pointer_chord_proof | windows | agent | SKIP |
| mac_pointer_chord_proof | windows | agent@gitbash | SKIP |
| mac_pointer_chord_proof | windows | terminal | SKIP |
| mac_pointer_chord_proof | windows | terminal@gitbash | SKIP |
| meta_and_word_keys | windows | agent | PASS |
| meta_and_word_keys | windows | agent@gitbash | PASS |
| meta_and_word_keys | windows | terminal | PASS |
| meta_and_word_keys | windows | terminal@gitbash | PASS |
| minimize_fullscreen | windows | agent | SKIP |
| minimize_fullscreen | windows | agent@gitbash | SKIP |
| minimize_fullscreen | windows | terminal | SKIP |
| minimize_fullscreen | windows | terminal@gitbash | SKIP |
| network_gate_switch | windows | terminal | SKIP |
| network_gate_switch | windows | terminal@gitbash | NOREF |
| output_frames | windows | agent | PASS |
| output_frames | windows | agent@gitbash | NOREF |
| output_frames | windows | terminal | PASS |
| output_frames | windows | terminal@gitbash | NOREF |
| primary_and_middle_click | windows | agent | SKIP |
| primary_and_middle_click | windows | agent@gitbash | SKIP |
| primary_and_middle_click | windows | terminal | SKIP |
| primary_and_middle_click | windows | terminal@gitbash | SKIP |
| remote_session_legacy | windows | agent | SKIP |
| remote_session_legacy | windows | agent@gitbash | NOREF |
| remote_session_legacy | windows | terminal | SKIP |
| remote_session_legacy | windows | terminal@gitbash | NOREF |
| remote_tab_enter_no_helper | windows | agent | SKIP |
| remote_tab_enter_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_enter_no_helper | windows | terminal | SKIP |
| remote_tab_enter_no_helper | windows | terminal@gitbash | FAIL |
| remote_tab_no_helper | windows | agent | SKIP |
| remote_tab_no_helper | windows | agent@gitbash | FAIL |
| remote_tab_no_helper | windows | terminal | SKIP |
| remote_tab_no_helper | windows | terminal@gitbash | FAIL |
| resize_reflow_output | windows | agent | SKIP |
| resize_reflow_output | windows | agent@gitbash | NOREF |
| resize_reflow_output | windows | terminal | SKIP |
| resize_reflow_output | windows | terminal@gitbash | NOREF |
| restore_shows_history | windows | agent | SKIP |
| restore_shows_history | windows | agent@gitbash | PASS |
| restore_shows_history | windows | terminal | SKIP |
| restore_shows_history | windows | terminal@gitbash | PASS |
| routing_table_powershell | windows | agent | PASS |
| routing_table_powershell | windows | agent@gitbash | SKIP |
| routing_table_powershell | windows | agent@pwsh7 | PASS |
| routing_table_powershell | windows | terminal | PASS |
| routing_table_powershell | windows | terminal@gitbash | SKIP |
| routing_table_powershell | windows | terminal@pwsh7 | PASS |
| scroll_keys_foreground | windows | agent | SKIP |
| scroll_keys_foreground | windows | agent@gitbash | PASS |
| scroll_keys_foreground | windows | terminal | SKIP |
| scroll_keys_foreground | windows | terminal@gitbash | PASS |
| scroll_keys_position | windows | agent | SKIP |
| scroll_keys_position | windows | agent@gitbash | NOREF |
| scroll_keys_position | windows | terminal | SKIP |
| scroll_keys_position | windows | terminal@gitbash | PASS |
| selection_modes | windows | agent | SKIP |
| selection_modes | windows | agent@gitbash | PASS |
| selection_modes | windows | terminal | SKIP |
| selection_modes | windows | terminal@gitbash | PASS |
| soak | windows | agent | PASS |
| soak | windows | agent@gitbash | PASS |
| soak | windows | terminal | PASS |
| soak | windows | terminal@gitbash | PASS |
| ssh_connect_typing_wrapper | windows | agent | SKIP |
| ssh_connect_typing_wrapper | windows | agent@gitbash | FAIL |
| ssh_connect_typing_wrapper | windows | terminal | SKIP |
| ssh_connect_typing_wrapper | windows | terminal@gitbash | FAIL |
| ssh_gitbash_bootstrap_in_command | windows | agent | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | agent@gitbash | FAIL |
| ssh_gitbash_bootstrap_in_command | windows | terminal | SKIP |
| ssh_gitbash_bootstrap_in_command | windows | terminal@gitbash | FAIL |
| ssh_gitbash_failing_not_local_managed | windows | agent | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | agent@gitbash | PASS |
| ssh_gitbash_failing_not_local_managed | windows | terminal | SKIP |
| ssh_gitbash_failing_not_local_managed | windows | terminal@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | agent | SKIP |
| ssh_gitbash_kill_switch | windows | agent@gitbash | PASS |
| ssh_gitbash_kill_switch | windows | terminal | SKIP |
| ssh_gitbash_kill_switch | windows | terminal@gitbash | PASS |
| ssh_gitbash_managed | windows | agent | SKIP |
| ssh_gitbash_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_managed | windows | terminal | SKIP |
| ssh_gitbash_managed | windows | terminal@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | agent | SKIP |
| ssh_gitbash_typeahead_managed | windows | agent@gitbash | FAIL |
| ssh_gitbash_typeahead_managed | windows | terminal | SKIP |
| ssh_gitbash_typeahead_managed | windows | terminal@gitbash | FAIL |
| ssh_nested_typeahead_wrapper | windows | agent | SKIP |
| ssh_nested_typeahead_wrapper | windows | agent@gitbash | PASS |
| ssh_nested_typeahead_wrapper | windows | terminal | SKIP |
| ssh_nested_typeahead_wrapper | windows | terminal@gitbash | PASS |
| startup_shell_pane | windows | terminal | SKIP |
| startup_shell_pane | windows | terminal@gitbash | SKIP |
| startup_shell_pane | windows | terminal@pwsh7 | PASS |
| subshell_line_editor_proof | windows | agent | SKIP |
| subshell_line_editor_proof | windows | agent@gitbash | SKIP |
| subshell_line_editor_proof | windows | terminal | SKIP |
| subshell_line_editor_proof | windows | terminal@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | agent | SKIP |
| subshell_ready_docker_t_without_i | windows | agent@gitbash | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal | SKIP |
| subshell_ready_docker_t_without_i | windows | terminal@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | agent | SKIP |
| subshell_ready_exec_fish_rc | windows | agent@gitbash | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal | SKIP |
| subshell_ready_exec_fish_rc | windows | terminal@gitbash | SKIP |
| subshell_ready_rc_clear | windows | agent | SKIP |
| subshell_ready_rc_clear | windows | agent@gitbash | PASS |
| subshell_ready_rc_clear | windows | terminal | SKIP |
| subshell_ready_rc_clear | windows | terminal@gitbash | PASS |
| subshell_ready_rc_questions | windows | agent | SKIP |
| subshell_ready_rc_questions | windows | agent@gitbash | PASS |
| subshell_ready_rc_questions | windows | terminal | SKIP |
| subshell_ready_rc_questions | windows | terminal@gitbash | PASS |
| subshell_ready_relay_shape | windows | agent | SKIP |
| subshell_ready_relay_shape | windows | agent@gitbash | SKIP |
| subshell_ready_relay_shape | windows | terminal | SKIP |
| subshell_ready_relay_shape | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | agent | SKIP |
| subshell_ready_secret_after_probe | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_probe | windows | terminal | SKIP |
| subshell_ready_secret_after_probe | windows | terminal@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | agent | SKIP |
| subshell_ready_secret_after_prompt | windows | agent@gitbash | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal | SKIP |
| subshell_ready_secret_after_prompt | windows | terminal@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent | SKIP |
| subshell_ready_unsupported_tcsh | windows | agent@gitbash | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal | SKIP |
| subshell_ready_unsupported_tcsh | windows | terminal@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | agent | SKIP |
| subshell_ready_vim_typeahead | windows | agent@gitbash | SKIP |
| subshell_ready_vim_typeahead | windows | terminal | SKIP |
| subshell_ready_vim_typeahead | windows | terminal@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent | SKIP |
| subshell_ready_zsh_newuser_install | windows | agent@gitbash | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal | SKIP |
| subshell_ready_zsh_newuser_install | windows | terminal@gitbash | SKIP |
| subshell_sh_probe | windows | agent | SKIP |
| subshell_sh_probe | windows | agent@gitbash | SKIP |
| subshell_sh_probe | windows | terminal | SKIP |
| subshell_sh_probe | windows | terminal@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | windows | agent | SKIP |
| subshell_sh_probe_empty_ps1 | windows | agent@gitbash | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal | SKIP |
| subshell_sh_probe_empty_ps1 | windows | terminal@gitbash | SKIP |
| tab_lifecycle_keys | windows | agent | PASS |
| tab_lifecycle_keys | windows | agent@gitbash | PASS |
| tab_lifecycle_keys | windows | terminal | PASS |
| tab_lifecycle_keys | windows | terminal@gitbash | PASS |
| tab_pointer_use | windows | agent | SKIP |
| tab_pointer_use | windows | agent@gitbash | SKIP |
| tab_pointer_use | windows | terminal | SKIP |
| tab_pointer_use | windows | terminal@gitbash | SKIP |
| ui_completions_as_you_type_local | windows | terminal | SKIP |
| ui_completions_as_you_type_local | windows | terminal@gitbash | FAIL |
| ui_copy_wrapped_drag | windows | agent | SKIP |
| ui_copy_wrapped_drag | windows | agent@gitbash | PASS |
| ui_copy_wrapped_drag | windows | terminal | SKIP |
| ui_copy_wrapped_drag | windows | terminal@gitbash | PASS |
| ui_copy_wrapped_output | windows | agent | SKIP |
| ui_copy_wrapped_output | windows | agent@gitbash | PASS |
| ui_copy_wrapped_output | windows | terminal | SKIP |
| ui_copy_wrapped_output | windows | terminal@gitbash | PASS |
| ui_vertical_tabs_many | windows | agent | SKIP |
| ui_vertical_tabs_many | windows | agent@gitbash | PASS |
| ui_vertical_tabs_many | windows | terminal | SKIP |
| ui_vertical_tabs_many | windows | terminal@gitbash | PASS |
| ui_vertical_tabs_panel_click | windows | agent | SKIP |
| ui_vertical_tabs_panel_click | windows | agent@gitbash | NOREF |
| ui_vertical_tabs_panel_click | windows | terminal | SKIP |
| ui_vertical_tabs_panel_click | windows | terminal@gitbash | PASS |
| wheel_scroll | windows | agent | SKIP |
| wheel_scroll | windows | agent@gitbash | SKIP |
| wheel_scroll | windows | terminal | SKIP |
| wheel_scroll | windows | terminal@gitbash | SKIP |
| zero_network_startup | windows | terminal | SKIP |
| zero_network_startup | windows | terminal@gitbash | PASS |

passed everywhere it ran (8, candidates for the owner's decision): agent_lrc_no_false_takeover, editor_daily, editor_memory_7mb, focus_cycle, history_survives_restart, meta_and_word_keys, soak, tab_lifecycle_keys
not passed somewhere (fail / skip / no-reference): alt_screen_programs, big_paste, flood_scrollback_find, gitbash_typeahead_reps, history_up_recall, hostile_output_recovers, job_control, keybinding_rebind, link_and_path_click, mac_pointer_chord_proof, minimize_fullscreen, network_gate_switch, output_frames, primary_and_middle_click, remote_session_legacy, remote_tab_enter_no_helper, remote_tab_no_helper, resize_reflow_output, restore_shows_history, routing_table_powershell, scroll_keys_foreground, scroll_keys_position, selection_modes, ssh_connect_typing_wrapper, ssh_gitbash_bootstrap_in_command, ssh_gitbash_failing_not_local_managed, ssh_gitbash_kill_switch, ssh_gitbash_managed, ssh_gitbash_typeahead_managed, ssh_nested_typeahead_wrapper, startup_shell_pane, subshell_line_editor_proof, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_probe, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, subshell_sh_probe_empty_ps1, tab_pointer_use, ui_completions_as_you_type_local, ui_copy_wrapped_drag, ui_copy_wrapped_output, ui_vertical_tabs_many, ui_vertical_tabs_panel_click, wheel_scroll, zero_network_startup
- note: TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on those platforms and nothing else; a job with the note `not run` was not part of it.
- note: windows/run_slim/logs/agent/case_latency_enter.log: latency evidence capped at 204 KB (1236 of 48771 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_enter.log: latency evidence capped at 204 KB (1422 of 14130 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1213 of 22101 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1265 of 4319 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent@gitbash/case_latency_key_log.log: latency evidence capped at 204 KB (1234 of 1287 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal/case_latency_enter.log: latency evidence capped at 204 KB (1212 of 48033 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter.log: latency evidence capped at 204 KB (1408 of 13551 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_enter_ssh.log: latency evidence capped at 204 KB (1218 of 22182 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_first_key_after_idle.log: latency evidence capped at 204 KB (1269 of 5741 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/terminal@gitbash/case_latency_key_log.log: latency evidence capped at 204 KB (1237 of 2552 hop lines kept; the platform budget of 2048 KB is shared between 10 logs)
- note: windows/run_slim/logs/agent/acct/launch0_1794508681.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794508681.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794508681.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794508681.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794509799.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794509799.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent/acct/launch0_1794508681.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794510385.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794514622.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794512258.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794514622.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794514622.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794515045.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794516021.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/acct/launch0_1794516021.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@gitbash/phosphor.1.log.gz: 1756 KB skipped (over 1 MB)
- note: windows/run_slim/logs/agent@pwsh7/phosphor.log.gz: 1998 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794497653.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794497653.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794499352.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794497653.log.in_session.3: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794497653.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794497653.log.in_session.4: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal/acct/launch0_1794499352.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794501806.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794500142.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794504198.log.in_session.1: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794504198.log.in_session.0: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794504198.log.in_session.2: 10239 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/acct/launch0_1794506262.log.in_session.0: 10240 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@gitbash/phosphor.1.log.gz: 1624 KB skipped (over 1 MB)
- note: windows/run_slim/logs/terminal@pwsh7/phosphor.log.gz: 2167 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::pairs_command::off [test-windows]`
  - `warp workspace::view::tests::test_code_editor_entry_points_refuse_a_file_past_the_size_limit [test-windows]`
- FIXED failures (unit): 
  - `warp ai::blocklist::agent_fuzz::matrix::triples_command::off [test-windows]`
  - `warp workspace::view::tests::test_open_file_with_target_event_preserves_requested_line [test-windows]`
  - `warp workspace::view::tests::test_reopening_a_file_whose_code_pane_was_moved_away_still_opens_it [test-windows]`
- NEW failures (app): 
  - `windows/terminal/mode_proof`
  - `windows/terminal/history_up_recall (case not in previous run)`
  - `windows/terminal/tabs_two_windows (case not in previous run)`
  - `windows/terminal/tabs_streaming (case not in previous run)`
  - `windows/terminal@gitbash/mode_proof`
  - `windows/terminal@gitbash/ssh_connect_typing_wrapper`
  - `windows/terminal@gitbash/pane_switch_left_on_after_ssh_connect_typing_wrapper (case not in previous run)`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_nested_typeahead_wrapper (case not in previous run)`
  - `windows/terminal@gitbash/latency_key_log`
  - `windows/terminal@gitbash/latency_first_key_after_idle (case not in previous run)`
  - `windows/terminal@gitbash/ssh_gitbash_bootstrap_in_command (case not in previous run)`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_gitbash_bootstrap_in_command (case not in previous run)`
  - `windows/terminal@gitbash/ui_completions_as_you_type_local`
  - `windows/terminal@gitbash/tabs_two_windows (case not in previous run)`
  - `windows/terminal@gitbash/tabs_streaming (case not in previous run)`
  - `windows/terminal@pwsh7/mode_proof (case not in previous run)`
  - `windows/terminal@pwsh7/history_up_recall (case not in previous run)`
  - `windows/agent/history_up_recall (case not in previous run)`
  - `windows/agent/tabs_two_windows (case not in previous run)`
  - `windows/agent@gitbash/ssh_connect_typing_wrapper`
  - `windows/agent@gitbash/pane_not_local_after_ssh_connect_typing_wrapper (case not in previous run)`
  - `windows/agent@gitbash/pane_not_local_after_ssh_nested_typeahead_wrapper (case not in previous run)`
  - `windows/agent@gitbash/pane_not_local_after_remote_tab_no_helper (case not in previous run)`
  - `windows/agent@gitbash/ssh_gitbash_bootstrap_in_command (case not in previous run)`
  - `windows/agent@gitbash/pane_not_local_after_ssh_gitbash_bootstrap_in_command (case not in previous run)`
  - `windows/agent@gitbash/tabs_two_windows (case not in previous run)`
  - `windows/agent@gitbash/tabs_streaming (case not in previous run)`
  - `windows/agent@pwsh7/history_up_recall (case not in previous run)`
- FIXED failures (app): 
  - `windows/terminal/fast_typing`
  - `windows/terminal@gitbash/ssh_connect_typing`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command`
  - `windows/terminal@gitbash/ssh_nested_typeahead`
  - `windows/terminal@gitbash/routing_table`
  - `windows/agent@gitbash/ssh_connect_typing`
  - `windows/agent@gitbash/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/ssh_nested_typeahead`
- Latency regressions > 5%: 
  - `windows/terminal@gitbash/ctrl_c_d ctrl_c: 71.58 -> 76.16 ms (+6.4%)`
- compared against: test-warpui-core-windows: build/r25a (3059029972); test-windows: build/r24a (3057403889); test-workspace-rest-windows: build/r25a (3059029972); app windows: build/r13a (3051722998)
