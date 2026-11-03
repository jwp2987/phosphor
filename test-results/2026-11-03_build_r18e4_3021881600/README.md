# build/r18e4  e253f6c14  (pipeline 3021881600)

- date: 2026-11-03, tier: release, pipeline status: manual
- compile (check-linux): ok, 52 warnings
- unit: 14789 passed, 3 failed, 55 skipped of 14792 run (3 suites)
- app (pass/fail/skip): macos 65/45/26 +9 no-ref; windows 15/9/15 +1 no-ref; soak: skip (macos)

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.3 | - |  |
| app-test-linux-wayland | canceled | - | - | no results artifact (job canceled) |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | canceled | - | - | no results artifact (job canceled) |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 132.7 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | failed | 21.3 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | success | 11.0 | - |  |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 34.6 | - |  |
| build-macos-arm64 | success | 20.0 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 51.6 | phosphor-windows-vm |  |
| check-linux | success | 7.3 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | failed | 64.3 | - |  |
| guards | success | 1.7 | - |  |
| guards-windows | manual | - | - | not run |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.4 | - |  |
| mutants | canceled | - | - |  |
| mutants-warpui-core | canceled | - | - |  |
| test-linux | failed | 6.4 | - |  |
| test-shuttle-loom | canceled | - | - |  |
| test-warpui-core | success | 1.2 | - |  |
| test-workspace-rest | success | 1.9 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell` FAIL (0.3s)
- `warp terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold` FAIL (0.3s)
- `warp terminal::view::tests::ssh_in_the_agent_view_on_an_unready_shell_is_routed_to_the_shell_and_held_not_sent_to_the_agent` FAIL (1.3s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1) |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2057; app saw 2055; lost-before-app 2 (Enter: sent 60, app saw 60) |
| terminal | rollover | PASS | injector accounting: injector sent 702; app saw 700; lost-before-app 2 (Enter: sent 20, app saw 20) |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 767; app saw 698; lost-before-app 69 (Enter: sent 17, app saw 25) |
| terminal | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6) |
| terminal | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 140; lost-before-app 17 (Enter: sent 5, app saw 5) |
| terminal | keydown_count | PASS | injector accounting: injector sent 564; app saw 560; lost-before-app 4 (Enter: sent 20, app saw 20) |
| terminal | big_paste | PASS | injector accounting: injector sent 21; app saw 16; lost-before-app 5 (Enter: sent 1, app saw 2) |
| terminal | tab_enter | PASS | injector accounting: injector sent 482; app saw 468; lost-before-app 14 (Enter: sent 0, app saw 12) |
| terminal | ctrl_c_d | FAIL | Ctrl-C under flood: Phosphor overhead 49.42 +/- 3.06 ms (SE) over Terminal.app (.command via open) exceeds the 20 ms budget (overhead - 2 SE |
| terminal | read_x | FAIL | read x received []; injector accounting: injector sent 77; app saw 75; lost-before-app 2 (Enter: sent 3, app saw 3) |
| terminal | subshells | FAIL | bash subshell (setup gap 0.25s): lines []; zsh subshell (setup gap 2.00s): lines []; fish subshell (setup gap 2.00s): lines []; nu subshell  |
| terminal | alt_screen | FAIL | shell usable after less/vim: []; vim received typed text: []; injector accounting: injector sent 81; app saw 79; lost-before-app 2 (Enter: s |
| terminal | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T194_vg274r never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | ssh_enter_then_immediate_command | PASS | injector accounting: injector sent 247; app saw 243; lost-before-app 4 (Enter: sent 12, app saw 12) |
| terminal | ssh_nested_typeahead | PASS | injector accounting: injector sent 137; app saw 154; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 17 more than were injected (ke |
| terminal | ssh_failing_not_local | NOREF | ssh_failing_not_local: ssh started 2.31 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS | injector accounting: injector sent 4; app saw 18; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 14 more than were injected (keys  |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 8.61 +/- 0.77 ms (SE) over Terminal.app (.command via open) exceeds the 2 ms budget (overhead - 2  |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal | latency_paint | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 12 blocks; NO-REFERENCE: Key to paint: overhead 2.38  |
| terminal | latency_first_key_after_idle | NOREF | First key after 1 s idle: first key after 1 s idle, kl3 post-to-first-changed-pixel at 240 Hz (4.17 ms quantum, not applied as a resolution  |
| terminal | routing_table | FAIL | 'what is 5 > 3' (prose) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shell received ['w |
| terminal | agent_mode_switch | PASS | injector accounting: injector sent 222; app saw 204; lost-before-app 18 (Enter: sent 4, app saw 4) |
| terminal | ui_tab_menu | FAIL | tab_return: shell received ['echo t213_4tqsni >> tc/menuzz_'], expected exactly one of ['echo t213_4tqsni >> tc/menuzz_one.txt', 'echo t213_ |
| terminal | flood_typed_line | FAIL | yes: the flood ended (-2.42 s) before the typing finished: not a flood test, raise APPCI_FLOOD_LINES; seq: the flood ended (-2.55 s) before  |
| terminal | remote_session_legacy | FAIL | [legacy] remote subshell clean-env bash (ksu/su stand-in) (setup gap 2.0s): want [outer pid, same pid as inner $PPID, t232_ebiuf78q, outer p |
| terminal | intl_wide_type | FAIL | typed zwj: shell received 'echo 👨\\u200d👩\\u200d👧 >> iw_zwj.txt' [utf8 6563686f20f09f91a85c7532303064f09f91a95c7532303064f09f91a7203e3e20697 |
| terminal | ui_ghost_accept | PASS | End probe received ['echo t236_c046eci >> gh']; injector accounting: injector sent 241; app saw 232; lost-before-app 9 (Enter: sent 4, app s |
| terminal | flood_typing_latency | FAIL | the flood in tab A ended before the typing finished (before=False, after=False): raise APPCI_FLOOD_LINES; no reference terminal in this case |
| terminal | remote_session_ext | FAIL | [ext] remote subshell clean-env bash (ksu/su stand-in) (setup gap 2.0s): want [outer pid, same pid as inner $PPID, t255_4hojhydl, outer pid  |
| terminal | intl_wide_paste | FAIL | paste inside typed zwj: shell received 'echo 👨\\u200d👩\\u200d👧 >> ip_zwj.txt' [utf8 6563686f20f09f91a85c7532303064f09f91a95c7532303064f09f91 |
| terminal | ui_ctrl_r | PASS | injector accounting: injector sent 104; app saw 94; lost-before-app 10 (Enter: sent 2, app saw 4) |
| terminal | flood_scrollback_find | FAIL | control: the marker is already visible before the Find (the flood did not scroll it away; raise APPCI_SCALE/lines); the Find typed into the  |
| terminal | ssh_hanging_host_not_local | FAIL | [refused host] the local shell is usable afterwards (and has none of the held line): expected ['t263_r1v4vf'], got ['MUSTNOT_T262_pjnz26', ' |
| terminal | intl_wide_edit | PASS | injector accounting: injector sent 123; app saw 127; lost-before-app 0 (Enter: sent 5, app saw 6); app logged 4 more than were injected (key |
| terminal | ui_palette | FAIL | palette did not open: 'Search for a command' not on screen (see shots/*palette_open.png); nonsense filter did not show 'No results found'; i |
| terminal | ssh_password_prompt | FAIL | after a wrong passphrase the next command ran in the LOCAL shell: expected ['t274_yrhqahg3'], got []; injector accounting: injector sent 142 |
| terminal | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 296; lost-before-app 0 (Enter: sent 12, app saw 17); app logged 14 more than were injected ( |
| terminal | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; NO-REFERENCE: Enter to command start over  |
| terminal | intl_compose_linux | SKIP | Linux compose/dead keys need the wayland (wtype) or x11 (xdotool) backend, this is mac; injector accounting: injector sent 2; app saw 0; los |
| terminal | intl_ime_japanese | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| terminal | intl_ime_pinyin | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| terminal | intl_deadkey_option | PASS | injector accounting: injector sent 813; app saw 807; lost-before-app 6 (Enter: sent 14, app saw 14) |
| terminal | intl_altgr_ps | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| terminal | intl_altgr_bash | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| terminal | ui_mouse_select_copy | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| terminal | editor_edit_save | SKIP | the file is opened by a second `phosphor <path>` process that the running instance receives over the D-Bus URL forward (Linux only); macOS ( |
| terminal | shell_zsh | FAIL | harness exception: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys'); APP DIED: pid 44304 is gone (kill -0 fail |
| terminal | shell_fish | PASS | injector accounting: injector sent 755; app saw 746; lost-before-app 9 (Enter: sent 21, app saw 25) |
| terminal | shell_nu | PASS | nu app log: 2026-11-03T14:05:08Z [DEBUG] frame 1: updated=["ActionButton", "FeaturePopup", "Menu", "SearchBar", "Dropdown", "AutosuggestionI |
| terminal | shell_pwsh | PASS | injector accounting: injector sent 689; app saw 680; lost-before-app 9 (Enter: sent 18, app saw 22) |
| terminal | ui_right_click_paste | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| terminal | editor_paste | SKIP | the file is opened by a second `phosphor <path>` process that the running instance receives over the D-Bus URL forward (Linux only); macOS ( |
| terminal | ui_drag_drop_file | SKIP | mac: no way to synthesize a drag source (XDND needs a client that owns the drag; Wayland wl_data_device likewise); injector accounting: inje |
| terminal | ui_resize_typing | PASS | injector accounting: injector sent 831; app saw 829; lost-before-app 2 (Enter: sent 17, app saw 17) |
| terminal | ui_split_panes | PASS | injector accounting: injector sent 1528; app saw 1520; lost-before-app 8 (Enter: sent 34, app saw 34) |
| terminal | md_viewer | PASS | injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |
| terminal | md_viewer_big | FAIL | 2 MB markdown file: first heading not visible within 90 s; injector accounting: injector sent 2; app saw 2; lost-before-app 0 (Enter: sent 0 |
| terminal | editor_memory_7mb | FAIL | txt: opening a 7 MB file cost 247 MB of RSS (> 100 MB target); txt: release on close NOT MEASURED: no close key on this platform (APPCI_KEY_ |
| terminal | ui_startup_time | PASS | injector accounting: injector sent 332; app saw 164; lost-before-app 168 (Enter: sent 8, app saw 4) |
| terminal | session_restore | SKIP | session_restore needs a quit + relaunch on the same profile: the Linux flow sends the quit key (ctrl+shift+q) and App.proc waits on the chil |
| terminal | ui_settings_live | PASS | injector accounting: injector sent 1131; app saw 1129; lost-before-app 2 (Enter: sent 27, app saw 27) |
| terminal | quit_24_tabs | SKIP | quit_24_tabs needs a quit + relaunch on the same profile: the Linux flow sends the quit key (ctrl+shift+q) and App.proc waits on the child;  |
| terminal | soak | SKIP | no keyboard close-pane/tab shortcut is usable on this platform (APPCI_KEY_CLOSE empty): Phosphor's close shortcut on macOS is cmd-shift-W, w |
| terminal | tabs_two_windows | PASS | injector accounting: injector sent 1080; app saw 1062; lost-before-app 18 (Enter: sent 27, app saw 28) |
| terminal | tabs_streaming | PASS | injector accounting: injector sent 953; app saw 936; lost-before-app 17 (Enter: sent 22, app saw 22) |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS | injector accounting: WARN loss case: injector sent 179; app saw 175; lost-before-app 4 (Enter: sent 1, app saw 1) |
| agent | fast_typing | PASS | injector accounting: WARN loss case: injector sent 2003; app saw 2001; lost-before-app 2 (Enter: sent 60, app saw 60) |
| agent | rollover | PASS | injector accounting: injector sent 682; app saw 680; lost-before-app 2 (Enter: sent 20, app saw 20) |
| agent | expert_fuzz | PASS | injector accounting: injector sent 759; app saw 673; lost-before-app 86 (Enter: sent 22, app saw 25) |
| agent | paste_instant_enter | PASS | injector accounting: injector sent 14; app saw 6; lost-before-app 8 (Enter: sent 0, app saw 6) |
| agent | arrows_behind_paste | PASS | injector accounting: injector sent 157; app saw 140; lost-before-app 17 (Enter: sent 5, app saw 5) |
| agent | keydown_count | PASS | injector accounting: injector sent 563; app saw 560; lost-before-app 3 (Enter: sent 20, app saw 20) |
| agent | big_paste | PASS | injector accounting: injector sent 21; app saw 16; lost-before-app 5 (Enter: sent 1, app saw 2) |
| agent | tab_enter | PASS | injector accounting: injector sent 474; app saw 460; lost-before-app 14 (Enter: sent 0, app saw 12) |
| agent | ctrl_c_d | FAIL | Ctrl-C under flood: Phosphor overhead 36.41 +/- 1.82 ms (SE) over Terminal.app (.command via open) exceeds the 20 ms budget (overhead - 2 SE |
| agent | read_x | FAIL | read x received []; injector accounting: injector sent 77; app saw 75; lost-before-app 2 (Enter: sent 3, app saw 3) |
| agent | subshells | FAIL | bash subshell (setup gap 0.25s): lines []; zsh subshell (setup gap 2.00s): lines []; fish subshell (setup gap 2.00s): lines []; nu subshell  |
| agent | alt_screen | FAIL | shell usable after less/vim: []; vim received typed text: []; injector accounting: injector sent 81; app saw 79; lost-before-app 2 (Enter: s |
| agent | gitbash_subshell | SKIP | Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case); injector accounting: injec |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T110_igh1pn never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| agent | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent | ssh_enter_then_immediate_command | PASS | injector accounting: injector sent 247; app saw 243; lost-before-app 4 (Enter: sent 12, app saw 12) |
| agent | ssh_nested_typeahead | PASS | injector accounting: injector sent 137; app saw 152; lost-before-app 0 (Enter: sent 7, app saw 8); app logged 15 more than were injected (ke |
| agent | ssh_failing_not_local | NOREF | ssh_failing_not_local: ssh started 2.30 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| agent | idle_cpu | PASS | injector accounting: injector sent 3; app saw 18; lost-before-app 0 (Enter: sent 0, app saw 1); app logged 15 more than were injected (keys  |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 10.59 +/- 1.23 ms (SE) over Terminal.app (.command via open) exceeds the 2 ms budget (overhead - 2 |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| agent | latency_paint | FAIL | Key to paint: Phosphor overhead 9.55 +/- 2.61 ms (SE) over Terminal.app (.command via open) exceeds the 2 ms budget (overhead - 2 SE > budge |
| agent | latency_first_key_after_idle | NOREF | NO-REFERENCE: First key after 1 s idle: overhead -1.61 +/- 5.39 ms (SE) vs budget 5 ms is within noise: INCONCLUSIVE/NO-COMPARE, the overhea |
| agent | approval_card_pause | PASS | injector accounting: injector sent 90; app saw 87; lost-before-app 3 (Enter: sent 1, app saw 2) |
| agent | agent_stream_cancel | FAIL | the first Ctrl-C did not clear the text typed while the reply streamed ('keepmet125_ohi6hr' still visible): the key did not reach the input; |
| agent | agent_approval | PASS | follow-up reply visible on screen (OCR; model wording varies, not a verdict); injector accounting: injector sent 256; app saw 251; lost-befo |
| agent | agent_input_exact | PASS | injector accounting: injector sent 159; app saw 153; lost-before-app 6 (Enter: sent 2, app saw 2) |
| agent | routing_table | FAIL | 'ls -la' (shell) was LOST or unproven: the shell received nothing and the app log shows no agent-routed Enter (dest lost, surface AgentConve |
| agent | agent_mode_switch | PASS | injector accounting: injector sent 220; app saw 204; lost-before-app 16 (Enter: sent 4, app saw 4) |
| agent | ui_tab_menu | FAIL | tab_return: shell received ['echo t135_nfvhei >> tc/menuzz_'], expected exactly one of ['echo t135_nfvhei >> tc/menuzz_one.txt', 'echo t135_ |
| agent | flood_typed_line | FAIL | yes: the flood ended (-1.88 s) before the typing finished: not a flood test, raise APPCI_FLOOD_LINES; seq: the flood ended (-2.55 s) before  |
| agent | remote_session_legacy | FAIL | [legacy] remote subshell clean-env bash (ksu/su stand-in) (setup gap 2.0s): want [outer pid, same pid as inner $PPID, t154_qpwukr4t, outer p |
| agent | intl_wide_type | FAIL | typed zwj: shell received 'echo 👨\\u200d👩\\u200d👧 >> iw_zwj.txt' [utf8 6563686f20f09f91a85c7532303064f09f91a95c7532303064f09f91a7203e3e20697 |
| agent | ui_ghost_accept | PASS | End probe received ['echo t158_ptuzpxd >> gh']; injector accounting: injector sent 241; app saw 232; lost-before-app 9 (Enter: sent 4, app s |
| agent | flood_typing_latency | FAIL | the flood in tab A ended before the typing finished (before=False, after=False): raise APPCI_FLOOD_LINES; no reference terminal in this case |
| agent | remote_session_ext | FAIL | [ext] remote subshell clean-env bash (ksu/su stand-in) (setup gap 2.0s): want [outer pid, same pid as inner $PPID, t177_xndfgg8y, outer pid  |
| agent | left_agent_mode_after_remote_session_ext | FAIL | after remote_session_ext the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases |
| agent | intl_wide_paste | FAIL | paste inside typed zwj: shell received 'echo 👨\\u200d👩\\u200d👧 >> ip_zwj.txt' [utf8 6563686f20f09f91a85c7532303064f09f91a95c7532303064f09f91 |
| agent | ui_ctrl_r | PASS | injector accounting: injector sent 104; app saw 94; lost-before-app 10 (Enter: sent 2, app saw 4) |
| agent | flood_scrollback_find | FAIL | control: the marker is already visible before the Find (the flood did not scroll it away; raise APPCI_SCALE/lines); injector accounting: inj |
| agent | ssh_hanging_host_not_local | FAIL | [refused host] the local shell is usable afterwards (and has none of the held line): expected ['t185_2ttyus'], got ['MUSTNOT_T184_urzay5', ' |
| agent | intl_wide_edit | PASS | injector accounting: injector sent 123; app saw 128; lost-before-app 0 (Enter: sent 5, app saw 6); app logged 5 more than were injected (key |
| agent | ui_palette | FAIL | palette did not open: 'Search for a command' not on screen (see shots/*palette_open.png); nonsense filter did not show 'No results found'; i |
| agent | ssh_password_prompt | FAIL | after a wrong passphrase the next command ran in the LOCAL shell: expected ['t196_h27i1ivu'], got []; injector accounting: injector sent 142 |
| agent | intl_wide_program | PASS | injector accounting: injector sent 282; app saw 293; lost-before-app 0 (Enter: sent 12, app saw 17); app logged 11 more than were injected ( |
| agent | latency_enter_ssh | NOREF | measurement mode: interleaved, blocks of 5 kept samples (+1 discarded lead per block), 24 blocks; NO-REFERENCE: Enter to command start over  |
| agent | intl_compose_linux | SKIP | Linux compose/dead keys need the wayland (wtype) or x11 (xdotool) backend, this is mac; injector accounting: injector sent 2; app saw 0; los |
| agent | intl_ime_japanese | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| agent | intl_ime_pinyin | SKIP | NEEDS_OWNER: the input method under test is not enabled for this user (enabled: com.apple.keylayout.US, com.apple.CharacterPaletteIM). In th |
| agent | intl_deadkey_option | PASS | injector accounting: injector sent 813; app saw 807; lost-before-app 6 (Enter: sent 14, app saw 14) |
| agent | intl_altgr_ps | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| agent | intl_altgr_bash | SKIP | Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option); injector accounting: in |
| agent | ui_mouse_select_copy | SKIP | macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping  |
| agent | editor_edit_save | SKIP | the file is opened by a second `phosphor <path>` process that the running instance receives over the D-Bus URL forward (Linux only); macOS ( |
| agent | shell_zsh | PASS | injector accounting: injector sent 592; app saw 583; lost-before-app 9 (Enter: sent 18, app saw 22) |
| agent | shell_fish | PASS | injector accounting: injector sent 755; app saw 746; lost-before-app 9 (Enter: sent 21, app saw 25) |
| agent | shell_nu | PASS | nu app log: 2026-11-03T15:11:14Z [DEBUG] frame 1: updated=["PaneView", "Terminal", "PaneHeader", "ActionButton", "Menu", "Workspace", "Warpi |
| agent | shell_pwsh | PASS | injector accounting: injector sent 689; app saw 680; lost-before-app 9 (Enter: sent 18, app saw 22) |
| agent | editor_paste | SKIP | the file is opened by a second `phosphor <path>` process that the running instance receives over the D-Bus URL forward (Linux only); macOS ( |
| agent | ui_resize_typing | PASS | injector accounting: injector sent 831; app saw 829; lost-before-app 2 (Enter: sent 17, app saw 17) |
| agent | ui_split_panes | PASS | injector accounting: injector sent 1528; app saw 1520; lost-before-app 8 (Enter: sent 34, app saw 34) |
| agent | md_viewer | PASS | injector accounting: injector sent 2; app saw 0; lost-before-app 2 (Enter: sent 0, app saw 0) |
| agent | md_viewer_ai_answer | PASS | injector accounting: injector sent 211; app saw 210; lost-before-app 1 (Enter: sent 1, app saw 0) |
| agent | ui_startup_time | PASS | injector accounting: injector sent 331; app saw 164; lost-before-app 167 (Enter: sent 8, app saw 4) |
| agent | left_agent_mode_after_ui_startup_time | FAIL | after ui_startup_time the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases me |
| agent | session_restore | SKIP | session_restore needs a quit + relaunch on the same profile: the Linux flow sends the quit key (ctrl+shift+q) and App.proc waits on the chil |
| agent | ui_settings_live | PASS | injector accounting: injector sent 1131; app saw 1129; lost-before-app 2 (Enter: sent 27, app saw 27) |
| agent | quit_24_tabs | SKIP | quit_24_tabs needs a quit + relaunch on the same profile: the Linux flow sends the quit key (ctrl+shift+q) and App.proc waits on the child;  |
| agent | soak | SKIP | no keyboard close-pane/tab shortcut is usable on this platform (APPCI_KEY_CLOSE empty): Phosphor's close shortcut on macOS is cmd-shift-W, w |
| agent | tabs_two_windows | PASS | injector accounting: injector sent 1079; app saw 1062; lost-before-app 17 (Enter: sent 27, app saw 28) |
| agent | tabs_streaming | PASS | injector accounting: injector sent 952; app saw 936; lost-before-app 16 (Enter: sent 22, app saw 22) |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | SKIP | not run in the PowerShell pane: compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it wi |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | FAIL | ft0: PowerShell received 1 of 30 lines NOT exactly as typed (PSReadLine history; first: got '\x15exit', want 'Add-Content r_ft0.txt t64_tego |
| terminal | rollover | PASS |  |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash  |
| terminal | paste_instant_enter | PASS |  |
| terminal | arrows_behind_paste | PASS |  |
| terminal | keydown_count | PASS |  |
| terminal | big_paste | SKIP | not run in the PowerShell pane: `cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | SKIP | NO-CONTROL: the injected Ctrl-C stopped the PowerShell sleep in neither Phosphor (0/3 tried) nor the conhost reference (0/4); this harness c |
| terminal | read_x | SKIP | not run in the PowerShell pane: bash `read x` / printf: runs in the Git Bash pane |
| terminal | subshells | SKIP | not run on Windows: uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subs |
| terminal | alt_screen | SKIP | not run in the PowerShell pane: `seq / less` and vim: runs in the Git Bash pane |
| terminal | gitbash_subshell | PASS |  |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T196_8tf2wm never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T201_9tk76t never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T206_swlrlb never reached the REMOTE file (s |
| terminal | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_failing_not_local | NOREF | ssh_failing_not_local: ssh started 1.98 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when |
| terminal | idle_cpu | PASS |  |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 39.98 +/- 0.74 ms (SE) over conhost (powershell.exe) exceeds the 2 ms budget (overhead - 2 SE > bu |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | latency_first_key_after_idle | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in th |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | ui_tab_menu | SKIP | not run in the PowerShell pane: asserts the exact line from rec.log: runs in the Git Bash pane |
| terminal | flood_typed_line | SKIP | not run on Windows: not yet ported: POSIX `seq`/`yes` floods and /proc sampling |
| terminal | remote_session_legacy | SKIP | not run in the PowerShell pane: uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane |
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: UnicodeEncodeError('charmap', 'save_case_log: UnicodeEncodeError(\'charmap\', \'2026-11-03T13:51:4 |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |
- note: windows/run/results.json: 1527 KB skipped (over 1 MB)

## vs previous run

- NEW failures (unit): none
- FIXED failures (unit): none
- NEW failures (app): 
  - `macos/terminal/ctrl_c_d (case not in previous run)`
  - `macos/terminal/read_x (case not in previous run)`
  - `macos/terminal/subshells (case not in previous run)`
  - `macos/terminal/alt_screen (case not in previous run)`
  - `macos/terminal/ssh_connect_typing (case not in previous run)`
  - `macos/terminal/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `macos/terminal/latency_enter (case not in previous run)`
  - `macos/terminal/routing_table (case not in previous run)`
  - `macos/terminal/ui_tab_menu (case not in previous run)`
  - `macos/terminal/flood_typed_line (case not in previous run)`
  - `macos/terminal/remote_session_legacy (case not in previous run)`
  - `macos/terminal/intl_wide_type (case not in previous run)`
  - `macos/terminal/flood_typing_latency (case not in previous run)`
  - `macos/terminal/remote_session_ext (case not in previous run)`
  - `macos/terminal/intl_wide_paste (case not in previous run)`
  - `macos/terminal/flood_scrollback_find (case not in previous run)`
  - `macos/terminal/ssh_hanging_host_not_local (case not in previous run)`
  - `macos/terminal/ui_palette (case not in previous run)`
  - `macos/terminal/ssh_password_prompt (case not in previous run)`
  - `macos/terminal/shell_zsh (case not in previous run)`
  - `macos/terminal/md_viewer_big (case not in previous run)`
  - `macos/terminal/editor_memory_7mb (case not in previous run)`
  - `macos/agent/ctrl_c_d (case not in previous run)`
  - `macos/agent/read_x (case not in previous run)`
  - `macos/agent/subshells (case not in previous run)`
  - `macos/agent/alt_screen (case not in previous run)`
  - `macos/agent/ssh_connect_typing (case not in previous run)`
  - `macos/agent/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `macos/agent/latency_enter (case not in previous run)`
  - `macos/agent/latency_paint (case not in previous run)`
  - `macos/agent/agent_stream_cancel (case not in previous run)`
  - `macos/agent/routing_table (case not in previous run)`
  - `macos/agent/ui_tab_menu (case not in previous run)`
  - `macos/agent/flood_typed_line (case not in previous run)`
  - `macos/agent/remote_session_legacy (case not in previous run)`
  - `macos/agent/intl_wide_type (case not in previous run)`
  - `macos/agent/flood_typing_latency (case not in previous run)`
  - `macos/agent/remote_session_ext (case not in previous run)`
  - `macos/agent/left_agent_mode_after_remote_session_ext (case not in previous run)`
  - `macos/agent/intl_wide_paste (case not in previous run)`
  - ... and 13 more
- FIXED failures (app): none
- Latency regressions > 5%: none
- compared against: test-linux: build/r18e3 (3021603921); test-warpui-core: build/r18d8 (3020799561); test-workspace-rest: build/r18e2 (3021361500); app macos: build/r18d8 (3020799561); app windows: build/r18d4 (3020340107)
