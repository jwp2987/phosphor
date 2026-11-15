# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=6936b7cf, sha=6936b7cf2d8706e6acf95d65c1bb30e783f18c3b, owner_settings=sha256 81d188e9844d, tier=targeted, scale=1, display_scale=1, outputs=1)

pass 35, fail 8, skip 18, no-reference 2 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; the injector/O |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane |
| terminal | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.6 ms from ctrl_c_keydown to its pty write (pty_write_bytes): no band yet for windows/terminal/ctrl_c_behind_big_paste/ |
| terminal | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector accounting: injector sent 206; app saw 206; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 198 unbound keydown/typed, 6 taken by a key bin |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; paste inside typed accents: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; paste inside  |
| terminal | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| terminal | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git Bash pane has the wrapper |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shift by SendInput, positions from the app's find hook) |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| terminal | phosphor_scheme_handler | FAIL | scheme_end: file has 0/2 lines, exact match False first diff: exp='t73_rhlgozo52y' got=None [first bad line 0: injector call #127 posted its ENTER at 17:34:11.348]; an unknown action: exit 0, handed over: True, refused with a log line: True; another scheme: exit 0, handed over: True, refused with a log line: True; scheme_en |
| terminal@gitbash | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector accounting: inject |
| terminal@gitbash | expert_fuzz | PASS | injector accounting: injector sent 718; app saw 718; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 651 unbound keydown/typed, 42 taken by a key binding, 25 Enter by a binding; pty writes: 82, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Ente |
| terminal@gitbash | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.3 ms from ctrl_c_keydown to its pty write (pty_write_bytes): no band yet for windows/terminal@gitbash/ctrl_c_behind_bi |
| terminal@gitbash | powershell_setup_ctrl_c_keys | SKIP | not run in the Git Bash pane: #1890: a PowerShell-in-PowerShell Phosphorize setup and the PowerShell hold's post-abort prompt; the Git Bash pane has the bash subshell lab (cases_subshell_ready) |
| terminal@gitbash | routing_table | PASS | injector accounting: injector sent 645; app saw 645; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed, 70 taken by a key binding, 27 Enter by a binding; pty writes: 15, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Ente |
| terminal@gitbash | intl_wide_paste | PASS | mode guard: the pane was in agent before this terminal case; put back with escape; verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 time(s) (no rec.log);  |
| terminal@gitbash | ssh_gitbash_managed | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): the same behaviour is judged by lab_managed_connect in app-test-lab-windows (Git Bash pane), and by the same family on Linux and macOS; to run this fake-sshd version by hand set APPCI_WIN_FAKESSHD=1 |
| terminal@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.38 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1 when it ended (ConnectTimeout 6 s); the hanging listener accepted 2 connection(s) in all (1 when the line was typed): more than one is the Git Bash wrapper's banner check followed by ssh itself (t |
| terminal@gitbash | ui_completions_as_you_type_local | FAIL | no completion menu with both candidates after typing a shared prefix without Tab; fixture dir C:\gl\b\07xLPX_fG\0\winters-phosphor\phosphor\appci-out\windows\run\home/w\tc exists=True files=['cayt_alpha.txt', 'cayt_bravo.txt', 'zzcompl_alpha_ |
| terminal@gitbash | selection_modes | PASS | injector accounting: injector sent 292; app saw 292; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 267 unbound keydown/typed, 17 taken by a key binding, 8 Enter by a binding; pty writes: 18, of which carrying a CR (\r): 0; Enter(s) the input editor started: 8 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8457; app saw 8457; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8222 unbound keydown/typed, 2 taken by a key binding, 233 Enter by a binding; pty writes: 1576, of which carrying a CR (\r): 9; Enter(s) the input editor started: 245 (an Enter may ride in a multi-line write; a gap with held/handed-back E |
| terminal@gitbash | phosphor_scheme_handler | SKIP | not run in the Git Bash pane: #1546: the URL handler does not depend on the pane's shell: runs once, in the PowerShell pane (the app's default shell) |
| terminal@pwsh7 | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@pwsh7 | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.5 ms from ctrl_c_keydown to its pty write (pty_write_bytes): no band yet for windows/terminal@pwsh7/ctrl_c_behind_big_ |
| terminal@pwsh7 | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector accounting: injector sent 197; app saw 197; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 189 unbound keydown/typed, 6 taken by a key bin |
| agent | mode_proof | PASS |  |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.2 ms from ctrl_c_keydown to its pty write (pty_write_bytes): no band yet for windows/agent/ctrl_c_behind_big_paste/ctr |
| agent | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector accounting: injector sent 203; app saw 203; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 195 unbound keydown/typed, 6 taken by a key bin |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | intl_wide_paste | PASS | paste inside typed cjk: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; paste inside typed accents: rec.log empty (hook not run): the command line the shell RECEIVED was NOT verified; only the effect file's bytes were; paste inside  |
| agent | ssh_gitbash_managed | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| agent | ssh_gitbash_failing_not_local_managed | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git Bash pane has the wrapper |
| agent | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shift by SendInput, positions from the app's find hook) |
| agent | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| agent@gitbash | mode_proof | PASS |  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | expert_fuzz | PASS | injector accounting: injector sent 750; app saw 750; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 678 unbound keydown/typed, 47 taken by a key binding, 25 Enter by a binding; pty writes: 109, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Ent |
| agent@gitbash | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.4 ms from ctrl_c_keydown to its pty write (pty_write_bytes): no band yet for windows/agent@gitbash/ctrl_c_behind_big_p |
| agent@gitbash | powershell_setup_ctrl_c_keys | SKIP | not run in the Git Bash pane: #1890: a PowerShell-in-PowerShell Phosphorize setup and the PowerShell hold's post-abort prompt; the Git Bash pane has the bash subshell lab (cases_subshell_ready) |
| agent@gitbash | routing_table | PASS | injector accounting: injector sent 625; app saw 625; lost-before-app 0 (Enter: sent 27, app saw 27); of the app's: 548 unbound keydown/typed, 50 taken by a key binding, 27 Enter by a binding; pty writes: 15, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Ente |
| agent@gitbash | intl_wide_paste | PASS | verified: 10 effect-file byte comparison(s) against the exact UTF-8 of the text; received command line verified 5 time(s), NOT verified 0 time(s) (no rec.log); injector accounting: injector sent 125; app saw 125; lost-before-app 0 (Enter: sent 10, app saw 10); of the app's: 103 unbound keydown/typed, 12 taken by a key  |
| agent@gitbash | ssh_gitbash_managed | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): the same behaviour is judged by lab_managed_connect in app-test-lab-windows (Git Bash pane), and by the same family on Linux and macOS; to run this fake-sshd version by hand set APPCI_WIN_FAKESSHD=1 |
| agent@gitbash | ssh_gitbash_failing_not_local_managed | PASS | ssh_failing_not_local: ssh started 2.31 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1 when it ended (ConnectTimeout 6 s); the hanging listener accepted 2 connection(s) in all (1 when the line was typed): more than one is the Git Bash wrapper's banner check followed by ssh itself (t |
| agent@gitbash | selection_modes | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops the selection' (#2131) was NOT judged in this pass; injector accounting: injector sent 288; app saw 288; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 263 unbound keydown/typed, 17 taken by a key bi |
| agent@gitbash | gitbash_typeahead_reps | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+=(__tr_slow)" input_type=AI t=1794707365266.784; tr12: 1 Enter(s) of this rep were not routed to the shell: deferred=false buffer=".txt" input_type=AI t=1794707370393.411; tr17: 2 Enter(s) of this rep were not |
| agent@pwsh7 | mode_proof | PASS |  |
| agent@pwsh7 | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@pwsh7 | ctrl_c_behind_big_paste | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.4 ms from ctrl_c_keydown to its pty write (pty_write_bytes): no band yet for windows/agent@pwsh7/ctrl_c_behind_big_pas |
| agent@pwsh7 | powershell_setup_ctrl_c_keys | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector accounting: injector sent 197; app saw 197; lost-before-app 0 (Enter: sent 6, app saw 6); of the app's: 189 unbound keydown/typed, 6 taken by a key bin |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_code_recycle | NOREF | NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block when the key was sent (clipboard 0 bytes; the answer's text is scripted, so this is the selection, not the wording); copy across the rebuild is NOT verified; the case's own Escape left the agent view; it re-entered with /agent (agent view again); injector accounting: injector sent 557; app saw 557; lost-before-app 0  |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_code_recycle | NOREF | NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block when the key was sent (clipboard 0 bytes; the answer's text is scripted, so this is the selection, not the wording); copy across the rebuild is NOT verified; the case's own Escape left the agent view; it re-entered with /agent (agent view again); injector accounting: injector sent 557; app saw 557; lost-before-app 0  |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | conhost (powershell.exe) | 0.0 | 0 | 30 |
| terminal | conhost (powershell.exe) | 0.25 | 1 | 30 |
| terminal@gitbash | conhost (bash) | 0.0 | 0 | 30 |
| terminal@gitbash | conhost (bash) | 0.25 | 0 | 30 |

## PROVING cases (owner decides graduation)

Platform windows. Nothing here moves a case: a passed case graduates only when the owner says so (docs/testing.md, 'New tests run in the fix tier first').

| case | mode | status | detail |
|---|---|---|---|
| agent_code_recycle | agent@fake | NOREF | NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block when the key wa |
| agent_code_recycle | agent@gitbash+fake | NOREF | NO-REFERENCE: UNVERIFIABLE: the copy key (ctrl+shift+c) put none of the snippets on the clipboard: the snippet answer was not the selected block when the key wa |
| ctrl_c_behind_big_paste | terminal | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.6 ms from ctrl_c_keydown to its pty write (pty_write_by |
| ctrl_c_behind_big_paste | terminal@gitbash | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.3 ms from ctrl_c_keydown to its pty write (pty_write_by |
| ctrl_c_behind_big_paste | terminal@pwsh7 | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.5 ms from ctrl_c_keydown to its pty write (pty_write_by |
| ctrl_c_behind_big_paste | agent | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.2 ms from ctrl_c_keydown to its pty write (pty_write_by |
| ctrl_c_behind_big_paste | agent@gitbash | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.4 ms from ctrl_c_keydown to its pty write (pty_write_by |
| ctrl_c_behind_big_paste | agent@pwsh7 | FAIL | the reader was stopped by the ^C: the next line ran (file []); NO-REFERENCE: ^C behind a 64 KiB paste: 2.4 ms from ctrl_c_keydown to its pty write (pty_write_by |
| gitbash_typeahead_reps | terminal | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| gitbash_typeahead_reps | terminal@gitbash | PASS | injector accounting: injector sent 8457; app saw 8457; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8222 unbound keydown/typed, 2 taken by a  |
| gitbash_typeahead_reps | agent | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| gitbash_typeahead_reps | agent@gitbash | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+=(__tr_slow)" input_t |
| phosphor_scheme_handler | terminal | FAIL | scheme_end: file has 0/2 lines, exact match False first diff: exp='t73_rhlgozo52y' got=None [first bad line 0: injector call #127 posted its ENTER at 17:34:11.3 |
| phosphor_scheme_handler | terminal@gitbash | SKIP | not run in the Git Bash pane: #1546: the URL handler does not depend on the pane's shell: runs once, in the PowerShell pane (the app's default shell) |
| powershell_setup_ctrl_c_keys | terminal | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector  |
| powershell_setup_ctrl_c_keys | terminal@gitbash | SKIP | not run in the Git Bash pane: #1890: a PowerShell-in-PowerShell Phosphorize setup and the PowerShell hold's post-abort prompt; the Git Bash pane has the bash su |
| powershell_setup_ctrl_c_keys | terminal@pwsh7 | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector  |
| powershell_setup_ctrl_c_keys | agent | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector  |
| powershell_setup_ctrl_c_keys | agent@gitbash | SKIP | not run in the Git Bash pane: #1890: a PowerShell-in-PowerShell Phosphorize setup and the PowerShell hold's post-abort prompt; the Git Bash pane has the bash su |
| powershell_setup_ctrl_c_keys | agent@pwsh7 | PASS | the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged; injector  |
| selection_modes | terminal | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shif |
| selection_modes | terminal@gitbash | PASS | injector accounting: injector sent 292; app saw 292; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 267 unbound keydown/typed, 17 taken by a key bi |
| selection_modes | agent | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shif |
| selection_modes | agent@gitbash | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops the selection' (#21 |
| ssh_gitbash_failing_not_local_managed | terminal | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git Bash pane has the wr |
| ssh_gitbash_failing_not_local_managed | terminal@gitbash | PASS | ssh_failing_not_local: ssh started 2.38 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1  |
| ssh_gitbash_failing_not_local_managed | agent | SKIP | not run in the PowerShell pane: a failing ssh behind the bash ssh wrapper with the one-connection path switched on (round 14): only the Git Bash pane has the wr |
| ssh_gitbash_failing_not_local_managed | agent@gitbash | PASS | ssh_failing_not_local: ssh started 2.31 s before the first MUSTNOT key; the hanging listener had accepted 1 connection(s), 1 still open when typing began and 1  |
| ssh_gitbash_managed | terminal | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| ssh_gitbash_managed | terminal@gitbash | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): the same behaviour  |
| ssh_gitbash_managed | agent | SKIP | not run in the PowerShell pane: the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper |
| ssh_gitbash_managed | agent@gitbash | SKIP | not played in the Windows app job (owner rule 2026-11-13: ssh tests run against the real lab, not the Windows fake sshd, #2007 #2051 #1992): the same behaviour  |
| ui_completions_as_you_type_local | terminal | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| ui_completions_as_you_type_local | terminal@gitbash | FAIL | no completion menu with both candidates after typing a shared prefix without Tab; fixture dir C:\gl\b\07xLPX_fG\0\winters-phosphor\phosphor\appci-out\windows\ru |

passed in every pass it ran in on this platform (candidates for the owner's graduation decision, 0): none
not passed (fail / skip / no-reference): agent_code_recycle, ctrl_c_behind_big_paste, gitbash_typeahead_reps, phosphor_scheme_handler, powershell_setup_ctrl_c_keys, selection_modes, ssh_gitbash_failing_not_local_managed, ssh_gitbash_managed, ui_completions_as_you_type_local

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
