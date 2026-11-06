# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=local, owner_settings=sha256 a0613b641a27, tier=full, scale=1)

pass 146, fail 11, skip 168, no-reference 19 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it with -NoProfile): runs in the Git Bash pane |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; the injector/O |
| terminal | fast_typing | PASS | pty writes: 238, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | rollover | PASS | pty writes: 40, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane |
| terminal | paste_instant_enter | PASS | pty writes: 13, of which carrying a CR (\r): 1; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | arrows_behind_paste | PASS | pty writes: 13, of which carrying a CR (\r): 3; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | keydown_count | PASS | a doubled key of the typed text (e.g. `>>`) was handled twice < 2 ms apart 1 time(s), with exactly as many presses of it logged as sent (OS delivered a burst; not a duplicate): [('"d"', 1793910675805.087, 1.71)]; pty writes: 44, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Ente |
| terminal | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | paste_multiline_prompt | PASS | pty writes: 16, of which carrying a CR (\r): 4; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.58, -> LineEditorReading 63.72; app: line editor ready 65.9, active 66.0, next command written 2918.1, its Preexec 2934.4; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 4.73 ms includes th |
| terminal | paste_bracketed_off | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D and the bracketed-paste printf: runs in the Git Bash pane (#1538) |
| terminal | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| terminal | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subshell and ctrl_c_d cover bash inside the pane) |
| terminal | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| terminal | gitbash_subshell | PASS |  |
| terminal | alt_screen_programs | SKIP | not run in the PowerShell pane: vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not installed makes the case unverifiable (#1537) |
| terminal | alt_screen_mouse_reporting | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet, and the receiving program needs python3 termios (#1537) |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_enter_then_immediate_command trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 1.96 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1 when it ended (ConnectTimeout 6 s); ssh gate at the first MUSTNOT key: no kh_hold_entry logged (app key trace off or rotated); first gate release/exit after it: none logged; ssh failed (connection |
| terminal | idle_cpu | PASS |  |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.47 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pty_write -> first pty_read median 4.20 ms (n=63); PowerShell side of Enter->start (#1401, separate ungated batch of 20 with the marks on, medians m |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in the Git Bash pane |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | agent_code_recycle | NOREF | no OCR engine: rebuild stability is the screenshot hash only (stable); NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block, or the model w |
| terminal | ui_tab_menu | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | flood_typed_line | SKIP | not run on Windows: not yet ported: POSIX `seq`/`yes` floods and /proc sampling |
| terminal | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| terminal | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run), effect file checked only; typed accents: rec.log empty (hook not run), effect file checked only; typed emoji: rec.log empty (hook not run), effect file checked only; typed zwj: rec.log e |
| terminal | ui_ghost_accept | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | flood_typing_latency | SKIP | not run on Windows: not yet ported: POSIX flood and pty arrival reader |
| terminal | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| terminal | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run), effect file checked only; paste inside typed accents: rec.log empty (hook not run), effect file checked only; paste inside typed emoji: rec.log empty (hook not run), effect file checked  |
| terminal | ui_ctrl_r | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | flood_scrollback_find | SKIP | not run on Windows: needs OCR (tesseract) and the Linux find flow |
| terminal | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| terminal | ssh_hanging_host_not_local | SKIP | not run in the PowerShell pane: Ctrl-C while ssh.cmd (a batch file) runs would ask 'Terminate batch job (Y/N)?': runs in the Git Bash pane |
| terminal | intl_wide_edit | FAIL | edit 2 (BackSpace): file bytes None, expected 6b32743234315f3836676942710a (text 'k2t241_86giBq' [utf8 6b32743234315f383667694271]); edit 0: rec.log empty (hook not run), effect file checked only; edit 1: rec.log empty (hook not run), effect file checked only; edit 2: rec.log empty (hook not  |
| terminal | ui_palette | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | ssh_password_prompt | SKIP | not run in the PowerShell pane: `ssh fakepass` against fakesshd_win.py's password user (#1535) plus rec.log/prompt.log of the local shell: runs in the Git Bash pane (the passphrase half needs the system sshd and is noted as not run) |
| terminal | intl_wide_program | SKIP | not run in the PowerShell pane: `head -n 1 > file` is the receiving program: runs in the Git Bash pane |
| terminal | latency_enter_ssh | SKIP | not run in the PowerShell pane: the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` path (ssh.cmd) differs from the pane's: runs in the Git Bash pane (#1532) |
| terminal | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| terminal | sudo_su_real_shell | SKIP | not run on Windows: sudo/su/ksu do not exist in a Windows pane (#1536) |
| terminal | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| terminal | ssh_drop_reconnect | SKIP | not run on Windows: not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535) |
| terminal | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal | intl_altgr_ps | PASS | pty writes: 2, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | intl_altgr_bash | PASS |  |
| terminal | ui_mouse_select_copy | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet (xdotool / sway seat on Linux) |
| terminal | editor_edit_save | PASS | 1: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_terminal.txt; 2: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_terminal.txt |
| terminal | shell_zsh | SKIP | not run in the PowerShell pane: zsh as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_pwsh | SKIP | not run in the PowerShell pane: pwsh as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | ui_right_click_paste | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet |
| terminal | editor_paste | PASS | p: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/paste_me_terminal.txt |
| terminal | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness |
| terminal | startup_shell_pane | SKIP | runs only in a startup_shell_override pass (APPCI_WIN_SHELLS=pwsh7,cmd) |
| terminal | ui_resize_typing | SKIP | not run on Windows: no window resize implementation in the Windows backend yet |
| terminal | ui_split_panes | SKIP | not run on Windows: needs OCR word boxes + pointer synthesis; neither exists in the Windows backend yet |
| terminal | md_viewer | PASS | no OCR engine (tesseract / Apple Vision): screenshot shots/terminal_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| terminal | md_viewer_big | SKIP | no OCR engine: cannot time the render (screenshot only) |
| terminal | editor_memory_7mb | FAIL | trim A/B: the open trim put a 29.9 ms UI item on screen while it ran (> 4 ms budget; control default: 0.0 ms); KNOWN: rs: opening a 7 MB file cost 775 MB of rss (> 150 MB target) [ACCEPTED_FOR_RELEASE #1607 v0.1.17: owner 2026-11-04: open cost accepted, release after clo |
| terminal | ui_startup_time | SKIP | not run on Windows: not yet ported: reads the Linux log format / paths |
| terminal | session_restore | SKIP | not run in the PowerShell pane: the app is stopped by pid and relaunched (#1540); bash `cd && echo` lines and `pwd -W` per tab, rec.log proves nothing re-ran: runs in the Git Bash pane |
| terminal | ui_settings_live | SKIP | not run on Windows: not yet ported: rewrites the Linux settings path while the app runs |
| terminal | quit_24_tabs | SKIP | not run on Windows: not yet ported: Linux quit key and log check |
| terminal | crash_kill9_restore | PASS | no rec.log in this pane (PowerShell): the re-run check rests on the effect file alone; no prompt hook in this pane (PowerShell): `a shell started` rests on the window, the key proof and the exact typing below, not on a prompt count |
| terminal | cb_settings_ui_open | SKIP | not run on Windows: the Windows backend has no verified Ctrl-, chord and the page is not OCR-able there |
| terminal | crash_corrupt_db | PASS |  |
| terminal | cb_settings_migration | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle); the Windows relaunch is the separate runner flow |
| terminal | soak | SKIP | not run on Windows: not yet ported: samples /proc |
| terminal | cb_state_relaunch | SKIP | not run on Windows: needs the App stop/start handle on a profile with state; Windows has a separate flow |
| terminal | tabs_two_windows | PASS | pty writes: 143, of which carrying a CR (\r): 13; Enter(s) the input editor started: 28 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | upgrade_over_previous | SKIP | no previous-release data: set APPCI_PREV_FIXTURE (state tarball) or APPCI_PREV_BIN (previous release binary; macOS also APPCI_PREV_BUNDLE) -- where CI gets the previous release's artifact is the owner's decision (#1546) |
| terminal | tabs_streaming | PASS | pty writes: 129, of which carrying a CR (\r): 13; Enter(s) the input editor started: 22 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | cb_no_autoupdate | PASS |  |
| terminal | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal | cb_editor_mac | SKIP | not run on Windows: macOS-only case (Linux and Windows have editor_edit_save / editor_paste) |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; pty write |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters |
| terminal@gitbash | fast_typing | PASS | pty writes: 197, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | rollover | PASS | pty writes: 45, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | expert_fuzz | PASS | pty writes: 99, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | paste_instant_enter | PASS | pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | arrows_behind_paste | PASS | pty writes: 19, of which carrying a CR (\r): 1; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | keydown_count | PASS | pty writes: 48, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | big_paste | FAIL | big_paste: the shell never came back to a prompt after `cat` was interrupted (Ctrl-C x4 over ~45 s): the next case would type into `cat`; big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); NO-REFERENCE: big_paste: Phosphor's pane got 100 of 1000001 bytes and |
| terminal@gitbash | pane_wedged_after_big_paste | FAIL | the gitbash pane did not run a marker command (no effect file) after big_paste, twice: the app is relaunched so the remaining cases measure something |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | paste_multiline_prompt | PASS | pty writes: 9, of which carrying a CR (\r): 1; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 81.7 ms includes the input-tool spawn (harness floor 0.11560000712051988 ms); Ctrl-D half not run on Windows: Ctrl-D is not end-of-input in PowerShell, and under ConPTY an msys cat/bash has not been validated to see it as EOF (docs/testin |
| terminal@gitbash | paste_bracketed_off | PASS | pty writes: 9, of which carrying a CR (\r): 2; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | read_x | PASS | pty writes: 42, of which carrying a CR (\r): 1; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subshell and ctrl_c_d cover bash inside the pane) |
| terminal@gitbash | alt_screen | PASS | pty writes: 2, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | gitbash_subshell | SKIP | the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells |
| terminal@gitbash | alt_screen_programs | NOREF | NO-REFERENCE: UNVERIFIABLE: htop is not installed (package htop): that part of the alt-screen coverage did not run; NO-REFERENCE: UNVERIFIABLE: tmux is not installed (package tmux): that part of the alt-screen coverage did not run; pty writes: 67, of which carrying a CR (\r): |
| terminal@gitbash | alt_screen_mouse_reporting | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet, and the receiving program needs python3 termios (#1537) |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.13 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1 when it ended (ConnectTimeout 6 s); ssh gate at the first MUSTNOT key: no kh_hold_entry logged (app key trace off or rotated); first gate release/exit after it: none logged; ssh failed (connection |
| terminal@gitbash | idle_cpu | PASS |  |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 45.4 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Enter->start: enter_begin -> pty_write median 0.88 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.68 ms (p95 2.66, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | latency_first_key_after_idle | PASS | First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so the figure is the idle penalty, not an overhead vs another terminal; First key after 2 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so the figure is the idl |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | agent_code_recycle | NOREF | no OCR engine: rebuild stability is the screenshot hash only (stable); NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block, or the model w |
| terminal@gitbash | ui_tab_menu | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | flood_typed_line | SKIP | not run on Windows: not yet ported: POSIX `seq`/`yes` floods and /proc sampling |
| terminal@gitbash | remote_session_legacy | NOREF | NO-REFERENCE: [legacy] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the bash/sh subshell steps ran; vim not installed on the box (apt: vim): vim step skipped; less not installed on the box (apt: less): less step skipped |
| terminal@gitbash | intl_wide_type | PASS | pty writes: 13, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ui_ghost_accept | PASS | End probe received ['echo t274_z86rq9y >> gh.txt']; pty writes: 18, of which carrying a CR (\r): 0; Enter(s) the input editor started: 9 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal@gitbash | flood_typing_latency | SKIP | not run on Windows: not yet ported: POSIX flood and pty arrival reader |
| terminal@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the bash/sh subshell steps ran; vim not installed on the box (apt: vim): vim step skipped; less not installed on the box (apt: less): less step skipped |
| terminal@gitbash | intl_wide_paste | PASS | pty writes: 21, of which carrying a CR (\r): 0; Enter(s) the input editor started: 10 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ui_ctrl_r | PASS | pty writes: 8, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs OCR (tesseract) and the Linux find flow |
| terminal@gitbash | remote_heredoc_paste | FAIL | bash: 10 KB single line: remote file not byte-exact (expected 10001 bytes, got no file in the 60 s budget, 66.5s); test sshd: 10104 client bytes received of the 10032-byte paste (13:52:16 conpty output ended: 147 bytes sent to the client in total / 13:52:16 client input ended: 10104 bytes in 9840 chunks, 10104 written to the pseudo console) (control: the reference console's ssh failed the 2,000-line here-doc too, but this step's payload is a different kind/size, so that does not cover it); bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill /T /F of the remote  |
| terminal@gitbash | ssh_hanging_host_not_local | PASS | [refused host] timing: ssh started t0; ssh client first seen +1.20 s, exited +3.05 s; first MUSTNOT key +1.28 s, last MUSTNOT key +1.94 s; app ssh gate lines after the first key: 0; [hanging host] timing: ssh started t0; ssh client first seen +1.19 s, exited +5.34 s; first MUSTNOT key +1.29 s, last MUSTNOT key +1.95 s; app ssh gate lines af |
| terminal@gitbash | intl_wide_edit | PASS | pty writes: 10, of which carrying a CR (\r): 0; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); pty writes: 23, of which carrying a CR (\r): 1; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal@gitbash | ssh_password_prompt | PASS | the key PASSPHRASE prompt was not exercised: it needs the system OpenSSH sshd; only the password prompt ran; [password] no OCR here: the `password` prompt was not seen, a fixed 3 s wait stood in (nothing was typed before it); [password-wrong] no OCR here: the `password |
| terminal@gitbash | intl_wide_program | PASS |  |
| terminal@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NO-REFERENCE: Enter to command start over ssh: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as  |
| terminal@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| terminal@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| terminal@gitbash | sudo_su_real_shell | SKIP | not run on Windows: sudo/su/ksu do not exist in a Windows pane (#1536) |
| terminal@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| terminal@gitbash | ssh_drop_reconnect | SKIP | not run on Windows: not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535) |
| terminal@gitbash | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| terminal@gitbash | intl_altgr_ps | SKIP | PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane) |
| terminal@gitbash | intl_altgr_bash | PASS | pty writes: 6, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ui_mouse_select_copy | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet (xdotool / sway seat on Linux) |
| terminal@gitbash | editor_edit_save | PASS | 1: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_terminal.txt; 2: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_terminal.txt |
| terminal@gitbash | shell_zsh | SKIP | package zsh (apt install zsh) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | shell_pwsh | SKIP | package PowerShell 7 (Microsoft apt repo: packages-microsoft-prod.deb, then apt install powershell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | ui_right_click_paste | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet |
| terminal@gitbash | editor_paste | PASS | p: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/paste_me_terminal.txt |
| terminal@gitbash | ui_drag_drop_file | SKIP | not run on Windows: no way to synthesize a drag source on Windows from the harness |
| terminal@gitbash | startup_shell_pane | SKIP | runs only in a startup_shell_override pass (APPCI_WIN_SHELLS=pwsh7,cmd) |
| terminal@gitbash | ui_resize_typing | SKIP | not run on Windows: no window resize implementation in the Windows backend yet |
| terminal@gitbash | ui_split_panes | SKIP | not run on Windows: needs OCR word boxes + pointer synthesis; neither exists in the Windows backend yet |
| terminal@gitbash | md_viewer | PASS | no OCR engine (tesseract / Apple Vision): screenshot shots/terminal@gitbash_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| terminal@gitbash | md_viewer_big | SKIP | no OCR engine: cannot time the render (screenshot only) |
| terminal@gitbash | editor_memory_7mb | FAIL | rs: 71 MB of rss still held 75 s after closing the editor (> 60 MB over the pre-open baseline; close via key ctrl+shift+w); trim A/B: the open trim put a 28.6 ms UI item on screen while it ran (> 4 ms budget; control default: 0.0 ms); KNOWN: rs: opening a 7 MB file cost 790 MB of rss |
| terminal@gitbash | ui_startup_time | SKIP | not run on Windows: not yet ported: reads the Linux log format / paths |
| terminal@gitbash | session_restore | PASS | sr: the app was stopped by pid (stop-by-pid), not through a quit key: this proves the restore of a hard stop after the session writer ran (3 s settle), not of a quit |
| terminal@gitbash | ui_settings_live | SKIP | not run on Windows: not yet ported: rewrites the Linux settings path while the app runs |
| terminal@gitbash | quit_24_tabs | SKIP | not run on Windows: not yet ported: Linux quit key and log check |
| terminal@gitbash | crash_kill9_restore | PASS |  |
| terminal@gitbash | cb_settings_ui_open | SKIP | not run on Windows: the Windows backend has no verified Ctrl-, chord and the page is not OCR-able there |
| terminal@gitbash | crash_corrupt_db | PASS |  |
| terminal@gitbash | cb_settings_migration | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle); the Windows relaunch is the separate runner flow |
| terminal@gitbash | soak | SKIP | not run on Windows: not yet ported: samples /proc |
| terminal@gitbash | cb_state_relaunch | SKIP | not run on Windows: needs the App stop/start handle on a profile with state; Windows has a separate flow |
| terminal@gitbash | tabs_two_windows | PASS | pty writes: 181, of which carrying a CR (\r): 14; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | upgrade_over_previous | SKIP | no previous-release data: set APPCI_PREV_FIXTURE (state tarball) or APPCI_PREV_BIN (previous release binary; macOS also APPCI_PREV_BUNDLE) -- where CI gets the previous release's artifact is the owner's decision (#1546) |
| terminal@gitbash | tabs_streaming | PASS | KNOWN: typing under 12 streaming tabs: 1/10 exact lines (8 got, 2 expected line(s) missing), no key discarded in the app log [ACCEPTED_FOR_RELEASE #1658 v0.1.17: owner 2026-11-04: the Windows streaming-tabs young-hold reorder (#1658) is known for v0.1.17; design after the release]; pty writes: 147, of which carrying a CR (\r): 13; Enter(s) the input editor started: 21 (an Enter may ride in a multi-line write; a gap with held/handed-back En |
| terminal@gitbash | cb_no_autoupdate | PASS |  |
| terminal@gitbash | cb_ui_action_latency | SKIP | not run on Windows: needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend |
| terminal@gitbash | cb_editor_mac | SKIP | not run on Windows: macOS-only case (Linux and Windows have editor_edit_save / editor_paste) |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | fast_typing | PASS | pty writes: 235, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | rollover | PASS | pty writes: 41, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane |
| agent | paste_instant_enter | PASS | pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | arrows_behind_paste | PASS | pty writes: 13, of which carrying a CR (\r): 3; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | keydown_count | PASS | pty writes: 47, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.46, -> LineEditorReading 66.06; app: line editor ready 64.6, active 64.6, next command written 2995.9, its Preexec 3002.4; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 4.15 ms includes th |
| agent | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| agent | paste_agent_input | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1538) |
| agent | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subshell and ctrl_c_d cover bash inside the pane) |
| agent | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| agent | gitbash_subshell | PASS |  |
| agent | alt_screen_programs | SKIP | not run in the PowerShell pane: vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not installed makes the case unverifiable (#1537) |
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.0 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2 |
| agent | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 1.98 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1 when it ended (ConnectTimeout 6 s); ssh gate at the first MUSTNOT key: no kh_hold_entry logged (app key trace off or rotated); first gate release/exit after it: none logged; ssh failed (connection |
| agent | idle_cpu | PASS |  |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 2.86 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pty_write -> first pty_read median 4.20 ms (n=63); PowerShell side of Enter->start (#1401, separate ungated batch of 20 with the marks on, medians m |
| agent | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in the Git Bash pane |
| agent | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_approval | SKIP | not run in the PowerShell pane: uses rec.log and effect files in bash syntax: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent | agent_code_recycle | FAIL | no 'editor_memory agent_code_recycled' line after 24 later answers: the snippet answer's editors were never recycled (a build before #1267, PHOSPHOR_AGENT_CODE_EDITOR_RECYCLE=0, or the answer is never > 6 rich-content entries from a visible one); the case's own Escape left the agent view; it re-entered with /agent (agent view again); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input edito |
| agent | cb_agent_pickers | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane |
| agent | cb_agent_file_mention | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane |
| agent | ui_tab_menu | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| agent | flood_typed_line | SKIP | not run on Windows: not yet ported: POSIX `seq`/`yes` floods and /proc sampling |
| agent | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| agent | intl_wide_type | PASS | typed cjk: rec.log empty (hook not run), effect file checked only; typed accents: rec.log empty (hook not run), effect file checked only; typed emoji: rec.log empty (hook not run), effect file checked only; typed zwj: rec.log e |
| agent | ui_ghost_accept | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| agent | flood_typing_latency | SKIP | not run on Windows: not yet ported: POSIX flood and pty arrival reader |
| agent | remote_session_ext | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell: runs in the Git Bash pane |
| agent | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run), effect file checked only; paste inside typed accents: rec.log empty (hook not run), effect file checked only; paste inside typed emoji: rec.log empty (hook not run), effect file checked  |
| agent | ui_ctrl_r | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| agent | flood_scrollback_find | SKIP | not run on Windows: needs OCR (tesseract) and the Linux find flow |
| agent | remote_heredoc_paste | SKIP | not run in the PowerShell pane: POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane |
| agent | ssh_hanging_host_not_local | SKIP | not run in the PowerShell pane: Ctrl-C while ssh.cmd (a batch file) runs would ask 'Terminate batch job (Y/N)?': runs in the Git Bash pane |
| agent | intl_wide_edit | FAIL | edit 2 (BackSpace): file bytes None, expected 6b32743137345f3733656d42710a (text 'k2t174_73emBq' [utf8 6b32743137345f3733656d4271]); edit 0: rec.log empty (hook not run), effect file checked only; edit 1: rec.log empty (hook not run), effect file checked only; edit 2: rec.log empty (hook not  |
| agent | ui_palette | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| agent | ssh_password_prompt | SKIP | not run in the PowerShell pane: `ssh fakepass` against fakesshd_win.py's password user (#1535) plus rec.log/prompt.log of the local shell: runs in the Git Bash pane (the passphrase half needs the system sshd and is noted as not run) |
| agent | intl_wide_program | SKIP | not run in the PowerShell pane: `head -n 1 > file` is the receiving program: runs in the Git Bash pane |
| agent | latency_enter_ssh | SKIP | not run in the PowerShell pane: the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` path (ssh.cmd) differs from the pane's: runs in the Git Bash pane (#1532) |
| agent | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| agent | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| agent | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent | intl_altgr_ps | PASS | pty writes: 2, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | intl_altgr_bash | PASS |  |
| agent | ui_mouse_select_copy | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet (xdotool / sway seat on Linux) |
| agent | editor_edit_save | PASS | 1: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_agent.txt; 2: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_agent.txt |
| agent | shell_zsh | SKIP | not run in the PowerShell pane: zsh as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | shell_pwsh | SKIP | not run in the PowerShell pane: pwsh as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| agent | editor_paste | PASS | p: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/paste_me_agent.txt |
| agent | ui_resize_typing | SKIP | not run on Windows: no window resize implementation in the Windows backend yet |
| agent | ui_split_panes | SKIP | not run on Windows: needs OCR word boxes + pointer synthesis; neither exists in the Windows backend yet |
| agent | md_viewer | PASS | no OCR engine (tesseract / Apple Vision): screenshot shots/agent_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| agent | md_viewer_ai_answer | SKIP | no OCR engine: screenshot only |
| agent | ui_startup_time | SKIP | not run on Windows: not yet ported: reads the Linux log format / paths |
| agent | session_restore | SKIP | not run in the PowerShell pane: the app is stopped by pid and relaunched (#1540); bash `cd && echo` lines and `pwd -W` per tab, rec.log proves nothing re-ran: runs in the Git Bash pane |
| agent | ui_settings_live | SKIP | not run on Windows: not yet ported: rewrites the Linux settings path while the app runs |
| agent | quit_24_tabs | SKIP | not run on Windows: not yet ported: Linux quit key and log check |
| agent | cb_settings_ui_open | SKIP | not run on Windows: the Windows backend has no verified Ctrl-, chord and the page is not OCR-able there |
| agent | cb_settings_migration | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle); the Windows relaunch is the separate runner flow |
| agent | soak | SKIP | not run on Windows: not yet ported: samples /proc |
| agent | tabs_two_windows | PASS | KNOWN: typing/paste in window 2 under 12 streaming tabs: 15/16 exact lines (15 got, 1 expected line(s) missing), no key discarded in the app log [ACCEPTED_FOR_RELEASE #1658 v0.1.17: owner 2026-11-04: the Windows streaming-tabs young-hold reorder (#1658) is known for v0.1.17; design after the release]; pty writes: 149, of which carrying a CR (\r): 13; Enter(s) the input editor started: 28 (an Enter may ride in a multi-line write; a gap with held/handed-back En |
| agent | tabs_streaming | PASS | pty writes: 140, of which carrying a CR (\r): 13; Enter(s) the input editor started: 22 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | fast_typing | PASS | pty writes: 193, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | rollover | PASS | pty writes: 44, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | expert_fuzz | PASS | pty writes: 94, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | arrows_behind_paste | PASS | pty writes: 15, of which carrying a CR (\r): 1; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | keydown_count | PASS | pty writes: 60, of which carrying a CR (\r): 0; Enter(s) the input editor started: 20 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | big_paste | FAIL | big_paste: the shell never came back to a prompt after `cat` was interrupted (Ctrl-C x4 over ~45 s): the next case would type into `cat`; big_paste: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); NO-REFERENCE: big_paste: Phosphor's pane got 100 of 1000001 bytes and |
| agent@gitbash | pane_needed_ctrl_c_after_big_paste | PASS | the first marker command after big_paste had no effect; it worked after Ctrl-C x3 (candidate APP finding: input focus/Enter routing after a long-running command, see docs/testing.md Windows triage) |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: overhead 23.73 +/- 6.09 ms (SE) vs budget 20 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT judged (not a pass); Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median 94.28 ms includes t |
| agent@gitbash | read_x | PASS | pty writes: 42, of which carrying a CR (\r): 1; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | paste_agent_input | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subshell and ctrl_c_d cover bash inside the pane) |
| agent@gitbash | alt_screen | PASS | pty writes: 1, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | gitbash_subshell | SKIP | the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells |
| agent@gitbash | alt_screen_programs | NOREF | NO-REFERENCE: UNVERIFIABLE: htop is not installed (package htop): that part of the alt-screen coverage did not run; NO-REFERENCE: UNVERIFIABLE: tmux is not installed (package tmux): that part of the alt-screen coverage did not run; pty writes: 67, of which carrying a CR (\r): |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.0 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2 |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent@gitbash | ssh_failing_not_local | PASS | ssh_failing_not_local: ssh started 2.30 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1 when it ended (ConnectTimeout 6 s); ssh gate at the first MUSTNOT key: no kh_hold_entry logged (app key trace off or rotated); first gate release/exit after it: none logged; ssh failed (connection |
| agent@gitbash | idle_cpu | PASS |  |
| agent@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 42.3 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Enter->start: enter_begin -> pty_write median 0.86 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| agent@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.60 ms (p95 0.81, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| agent@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent@gitbash | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: insufficient evidence (n=29 < 30 samples): INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT judged (not a pass); First key after 1 s idle: key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so the figure is the idl |
| agent@gitbash | approval_card_pause | SKIP | tesseract missing: cannot detect the approval card on screen |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while-streaming landing is screenshot-only (shots/agent_typed_while_streaming.png); Ctrl-C #1: stream stopped (2 distinct frames in 5), typ |
| agent@gitbash | agent_approval | FAIL | approved command was written to the shell 0 times by the app (hop=pty_write_bytes class=UserCommand carrying APPROVEMET155_CQMUWW; expected exactly once; a bash pane's rec.log cannot show it, #904); the follow-up question in the same conversation did not open a model request within 60 s; a fresh agent conversation was started (Escape, /agent) so earlier cas |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_code_recycle | NOREF | no OCR engine: rebuild stability is the screenshot hash only (stable); NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block, or the model w |
| agent@gitbash | cb_agent_pickers | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | cb_agent_file_mention | PASS | no OCR engine: whether the picker lists cbmention_t167_5xldxf.txt is in shots/ only; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash | ui_tab_menu | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | flood_typed_line | SKIP | not run on Windows: not yet ported: POSIX `seq`/`yes` floods and /proc sampling |
| agent@gitbash | remote_session_legacy | NOREF | NO-REFERENCE: [legacy] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the bash/sh subshell steps ran; vim not installed on the box (apt: vim): vim step skipped; less not installed on the box (apt: less): less step skipped |
| agent@gitbash | intl_wide_type | PASS | pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ui_ghost_accept | PASS | End probe received ['echo t193_u59vu93 >> gh.txt']; pty writes: 18, of which carrying a CR (\r): 0; Enter(s) the input editor started: 9 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent@gitbash | flood_typing_latency | SKIP | not run on Windows: not yet ported: POSIX flood and pty arrival reader |
| agent@gitbash | remote_session_ext | NOREF | NO-REFERENCE: [ext] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the bash/sh subshell steps ran; vim not installed on the box (apt: vim): vim step skipped; less not installed on the box (apt: less): less step skipped |
| agent@gitbash | agent_view_not_restored_after_remote_session_ext | PASS | remote_session_ext relaunched the app itself (pid 9436 -> 11960); a relaunched app starts in a terminal session, so this is not an app finding. The app is relaunched again (fresh agent session) so the cases after it run in agent mode. |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | intl_wide_paste | PASS | pty writes: 20, of which carrying a CR (\r): 0; Enter(s) the input editor started: 10 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ui_ctrl_r | PASS | pty writes: 8, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | flood_scrollback_find | SKIP | not run on Windows: needs OCR (tesseract) and the Linux find flow |
| agent@gitbash | remote_heredoc_paste | FAIL | bash: 10 KB single line: remote file not byte-exact (expected 10001 bytes, got no file in the 60 s budget, 66.6s); test sshd: 10104 client bytes received of the 10032-byte paste (15:20:29 conpty output ended: 147 bytes sent to the client in total / 15:20:29 client input ended: 10104 bytes in 9843 chunks, 10104 written to the pseudo console) (control: the reference console's ssh failed the 2,000-line here-doc too, but this step's payload is a different kind/size, so that does not cover it); bash: 2000-line here-doc (142166 bytes): the remote shell was still busy with the payload after the timeout; the harness ended it (taskkill /T /F of the remote  |
| agent@gitbash | ssh_hanging_host_not_local | PASS | [refused host] timing: ssh started t0; ssh client first seen +1.00 s, exited +3.34 s; first MUSTNOT key +1.29 s, last MUSTNOT key +1.95 s; app ssh gate lines after the first key: 0; [hanging host] timing: ssh started t0; ssh client first seen +1.09 s, exited +5.23 s; first MUSTNOT key +1.31 s, last MUSTNOT key +1.97 s; app ssh gate lines af |
| agent@gitbash | intl_wide_edit | PASS | pty writes: 11, of which carrying a CR (\r): 0; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ui_palette | PASS | palette_reopen: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop); pty writes: 23, of which carrying a CR (\r): 1; Enter(s) the input editor started: 5 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent@gitbash | ssh_password_prompt | PASS | the key PASSPHRASE prompt was not exercised: it needs the system OpenSSH sshd; only the password prompt ran; [password] no OCR here: the `password` prompt was not seen, a fixed 3 s wait stood in (nothing was typed before it); [password-wrong] no OCR here: the `password |
| agent@gitbash | intl_wide_program | PASS |  |
| agent@gitbash | latency_enter_ssh | NOREF | measurement mode: sequential (this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)); NO-REFERENCE: Enter to command start over ssh: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as  |
| agent@gitbash | intl_compose_linux | SKIP | not run on Windows: Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash) |
| agent@gitbash | intl_ime_japanese | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| agent@gitbash | intl_ime_pinyin | SKIP | not run on Windows: macOS input methods (TISSelectInputSource) |
| agent@gitbash | intl_deadkey_option | SKIP | not run on Windows: macOS Option dead keys |
| agent@gitbash | intl_altgr_ps | SKIP | PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane) |
| agent@gitbash | intl_altgr_bash | PASS | pty writes: 6, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ui_mouse_select_copy | SKIP | not run on Windows: no pointer synthesis in the Windows backend yet (xdotool / sway seat on Linux) |
| agent@gitbash | editor_edit_save | PASS | 1: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_agent.txt; 2: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/edit_me_agent.txt |
| agent@gitbash | shell_zsh | SKIP | package zsh (apt install zsh) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| agent@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| agent@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| agent@gitbash | shell_pwsh | SKIP | package PowerShell 7 (Microsoft apt repo: packages-microsoft-prod.deb, then apt install powershell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| agent@gitbash | editor_paste | PASS | p: windows hand-over: phosphor.exe file:///C:/Users/<user>/ssh-r18u28/src/appci-out/windows/run/home/<user>/ed/paste_me_agent.txt |
| agent@gitbash | ui_resize_typing | SKIP | not run on Windows: no window resize implementation in the Windows backend yet |
| agent@gitbash | ui_split_panes | SKIP | not run on Windows: needs OCR word boxes + pointer synthesis; neither exists in the Windows backend yet |
| agent@gitbash | md_viewer | PASS | no OCR engine (tesseract / Apple Vision): screenshot shots/agent@gitbash_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| agent@gitbash | md_viewer_ai_answer | SKIP | no OCR engine: screenshot only |
| agent@gitbash | ui_startup_time | SKIP | not run on Windows: not yet ported: reads the Linux log format / paths |
| agent@gitbash | session_restore | PASS | sr: the app was stopped by pid (stop-by-pid), not through a quit key: this proves the restore of a hard stop after the session writer ran (3 s settle), not of a quit |
| agent@gitbash | ui_settings_live | SKIP | not run on Windows: not yet ported: rewrites the Linux settings path while the app runs |
| agent@gitbash | quit_24_tabs | SKIP | not run on Windows: not yet ported: Linux quit key and log check |
| agent@gitbash | cb_settings_ui_open | SKIP | not run on Windows: the Windows backend has no verified Ctrl-, chord and the page is not OCR-able there |
| agent@gitbash | cb_settings_migration | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle); the Windows relaunch is the separate runner flow |
| agent@gitbash | soak | SKIP | not run on Windows: not yet ported: samples /proc |
| agent@gitbash | tabs_two_windows | PASS | KNOWN: typing/paste in window 2 under 12 streaming tabs: 1/16 exact lines (14 got, 2 expected line(s) missing), no key discarded in the app log [ACCEPTED_FOR_RELEASE #1658 v0.1.17: owner 2026-11-04: the Windows streaming-tabs young-hold reorder (#1658) is known for v0.1.17; design after the release]; pty writes: 170, of which carrying a CR (\r): 14; Enter(s) the input editor started: 26 (an Enter may ride in a multi-line write; a gap with held/handed-back En |
| agent@gitbash | tabs_streaming | PASS | KNOWN: typing under 12 streaming tabs: 1/10 exact lines (3 got, 7 expected line(s) missing), no key discarded in the app log [ACCEPTED_FOR_RELEASE #1658 v0.1.17: owner 2026-11-04: the Windows streaming-tabs young-hold reorder (#1658) is known for v0.1.17; design after the release]; pty writes: 140, of which carrying a CR (\r): 13; Enter(s) the input editor started: 18 (an Enter may ride in a multi-line write; a gap with held/handed-back En |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 4.7 / 9.3 | 2.7 / 3.6 (conhost (powershell.exe)) | 2.03 +/- 0.25 | overhead <= 20 ms | - | pending | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 46.6 / 57.6 | 7.7 / 18.0 (conhost (powershell.exe)) | 38.88 +/- 0.98 | overhead <= 2 ms | - | pending | FAIL | n=60 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 81.7 / 102.8 | 74.2 / 97.5 (conhost (bash)) | 7.46 +/- 4.38 | overhead <= 20 ms | - | pending | PASS | n=30 |
| terminal@gitbash | latency_enter/enter_to_start | 93.0 / 153.8 | 59.8 / 74.0 (conhost (bash)) | 33.28 +/- 2.67 | overhead <= 2 ms | - | pending | FAIL | n=60 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.8 / 11.6 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| terminal@gitbash | latency_key_log/ui_work | - | - | - | - | - | - | WARN | n=12 |
| terminal@gitbash | latency_first_key_after_idle/gap1s | 3.3 / 4.3 | 3.3 / 4.3 (second key 0.3 s later (same session)) | 0.03 +/- 0.13 | overhead <= 5 ms | - | pending | PASS | n=30 |
| terminal@gitbash | latency_first_key_after_idle/gap2s | 3.5 / 4.3 | 3.5 / 4.6 (second key 0.3 s later (same session)) | 0.02 +/- 0.12 | overhead <= 5 ms | - | pending | PASS | n=30 |
| terminal@gitbash | latency_first_key_after_idle/gap5s | 3.5 / 4.4 | 3.5 / 4.0 (second key 0.3 s later (same session)) | -0.02 +/- 0.10 | overhead <= 5 ms | - | pending | PASS | n=30 |
| terminal@gitbash | latency_enter_ssh/ssh_enter_to_start | 100.8 / 118.4 | - | - | overhead <= 1 ms | - | pending | NO-REFERENCE | n=60 |
| agent | ctrl_c_d/ctrl_c | 4.2 / 5.5 | 2.7 / 3.8 (conhost (powershell.exe)) | 1.41 +/- 0.23 | overhead <= 20 ms | - | pending | PASS | n=30 |
| agent | latency_enter/enter_to_start | 43.7 / 53.0 | 7.8 / 12.5 (conhost (powershell.exe)) | 35.96 +/- 1.52 | overhead <= 2 ms | - | pending | FAIL | n=60 |
| agent@gitbash | ctrl_c_d/ctrl_c | 94.3 / 125.4 | 70.5 / 98.5 (conhost (bash)) | 23.73 +/- 6.09 | overhead <= 20 ms | - | pending | INCONCLUSIVE | n=30 |
| agent@gitbash | latency_enter/enter_to_start | 150.6 / 293.5 | 63.0 / 75.6 (conhost (bash)) | 87.60 +/- 3.39 | overhead <= 2 ms | - | pending | FAIL | n=60 |
| agent@gitbash | latency_key_log/key_to_frame | 3.3 / 4.0 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=60 |
| agent@gitbash | latency_key_log/ui_work | - | - | - | - | - | - | PASS | n=0 |
| agent@gitbash | latency_first_key_after_idle/gap1s | 3.4 / 3.6 | 3.5 / 4.9 (second key 0.3 s later (same session)) | -0.07 +/- 0.07 | overhead <= 5 ms | - | pending | INCONCLUSIVE | n=29 |
| agent@gitbash | latency_first_key_after_idle/gap2s | 3.8 / 4.1 | 3.6 / 4.7 (second key 0.3 s later (same session)) | 0.15 +/- 0.21 | overhead <= 5 ms | - | pending | PASS | n=30 |
| agent@gitbash | latency_first_key_after_idle/gap5s | 3.8 / 4.4 | 3.6 / 4.6 (second key 0.3 s later (same session)) | 0.19 +/- 0.19 | overhead <= 5 ms | - | pending | INCONCLUSIVE | n=29 |
| agent@gitbash | latency_enter_ssh/ssh_enter_to_start | 100.5 / 120.5 | - | - | overhead <= 1 ms | - | pending | NO-REFERENCE | n=60 |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal@gitbash | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal@gitbash | reference | perline | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | conhost (powershell.exe) | 0.0 | 0 | 30 |
| terminal | conhost (powershell.exe) | 0.25 | 1 | 30 |
| terminal@gitbash | conhost (bash) | 0.0 | 0 | 30 |
| terminal@gitbash | conhost (bash) | 0.25 | 0 | 30 |

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
