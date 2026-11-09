# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=a10b7c89, sha=a10b7c89e88ec97526f470035cb73d459117f655, owner_settings=sha256 a0613b641a27, tier=smoke, scale=1)

pass 55, fail 15, skip 130, no-reference 4 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; the injector/O |
| terminal | fast_typing | PASS | pty writes: 257, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | paste_instant_enter | PASS | pty writes: 14, of which carrying a CR (\r): 2; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.39, -> LineEditorReading 59.2; app: line editor ready 65.8, active 65.9, next command written 3081.5, its Preexec 3096.4; #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; Ctrl-C: same trap/t |
| terminal | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| terminal | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| terminal | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| terminal | ssh_connect_typing | PASS | ssh_connect_typing gap 0.1 s trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_enter_then_immediate_command trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, |
| terminal | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.66 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pty_write -> first pty_read median 4.30 ms (n=63); PowerShell side of Enter->start (#1401, separate ungated batch of 20 with the marks on, medians m |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| terminal | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash pane |
| terminal | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash pane |
| terminal | ui_completions_as_you_type_local | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| terminal | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot shots/terminal_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| terminal | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| terminal | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty |
| terminal | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_DIAG_BUNDLE_DIR in the app's environment to enable) |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; pty write |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters |
| terminal@gitbash | fast_typing | PASS | pty writes: 194, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | paste_instant_enter | PASS |  |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; WARN Ctrl-C: median 83.47 vs baseline build +3.23 ms (+4.0%); Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in  |
| terminal@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| terminal@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 41.3 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Enter->start: enter_begin -> pty_write median 0.90 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.66 ms (p95 0.92, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t211_z' should have completed the REMOTE directory: the remote shell received ['echo wT212_pfec7644 >> ~/.appci_where'], want 'cd phxtab_t211_z2j4pc/'; the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote directory is not obse |
| terminal@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT220_to913369 >> ~/.appci_where'], want the COMPLETED 'cd phxte_t217_aod2k6/' (the uncompleted 'cd phxte_t217_a' must never run) |
| terminal@gitbash | ui_completions_as_you_type_local | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot shots/terminal@gitbash_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| terminal@gitbash | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| terminal@gitbash | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal@gitbash | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty |
| terminal@gitbash | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal@gitbash | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_DIAG_BUNDLE_DIR in the app's environment to enable) |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | pty writes: 175, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | paste_instant_enter | PASS | pty writes: 15, of which carrying a CR (\r): 3; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 9.18, -> LineEditorReading 56.36; app: line editor ready 59.7, active 59.9, next command written 3148.7, its Preexec 3155.4; #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; fix tier, agent mod |
| agent | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| agent | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| agent | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| agent | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| agent | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 2: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2 |
| agent | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in no shell, not the remote one: `exit` NOT typed; stopping; ssh trial verdicts: exact=1 |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | agent_input_exact | SKIP | not run in the PowerShell pane: reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent | agent_mode_switch | SKIP | not run in the PowerShell pane: asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane |
| agent | agent_lrc_no_false_takeover | SKIP | not run on Windows: #1796: the interactive-prompt take-over it tests (PromptWatch, termios + /proc tty-read probe) is unix-only; ConPTY has no termios |
| agent | remote_tab_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash pane |
| agent | remote_tab_enter_no_helper | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash pane |
| agent | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| agent | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | pty writes: 139, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only; the overhead budget is judged in terminal mode; Ctrl-D half not run on Windows: |
| agent@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | subshell_ready_rc_questions | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.2 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| agent@gitbash | ssh_enter_then_immediate_command | PASS | ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh trial verdicts: #1292=1, exact=2 |
| agent@gitbash | ssh_nested_typeahead | PASS | ssh trial verdicts: exact=1 |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while-streaming landing is screenshot-only (shots/agent_typed_while_streaming.png); Ctrl-C #1: stream stopped (2 distinct frames in 5), typ |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_lrc_no_false_takeover | SKIP | not run on Windows: #1796: the interactive-prompt take-over it tests (PromptWatch, termios + /proc tty-read probe) is unix-only; ConPTY has no termios |
| agent@gitbash | remote_tab_no_helper | FAIL | Tab after 'cd phxtab_t99_7' should have completed the REMOTE directory: the remote shell received ['echo wT100_kmxy7ug0 >> ~/.appci_where'], want 'cd phxtab_t99_7978bs/'; the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote directory is not obse |
| agent@gitbash | remote_tab_enter_no_helper | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT108_oihl0dg8 >> ~/.appci_where'], want the COMPLETED 'cd phxte_t105_d75qpv/' (the uncompleted 'cd phxte_t105_d' must never run) |
| agent@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| agent@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| agent@fake | agent_interactive_password | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_fault_429 | FAIL | unexpected ERROR line: 2026-11-09T19:52:49Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Many Requests: BYOP open stream failed: HTTP status 429: Too Many Requests: {"error": "fake model fault: HTTP 429"}; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent@fake | agent_fault_500 | FAIL | unexpected ERROR line: 2026-11-09T19:53:18Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Internal Server Error: BYOP open stream failed: HTTP status 500: Internal Server Error: {"error": "fake model fault: HTTP 500"}; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent@fake | agent_fault_disconnect | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 requests): a retry would repeat output / tool calls; unexpected ERROR line: 2026-11-09T19:53:33Z [ERROR] Multi |
| agent@fake | agent_fault_down | FAIL | unexpected ERROR line: 2026-11-09T19:54:11Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/streaming request failed: Web stream error for model 'fake-model (adapter: Ollama)'.; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent@fake | agent_fault_slow_cancel | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 1, of which carrying a CR (\r): 0; |
| agent@fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 1, of which carrying a CR (\r): 0; |
| agent@fake | agent_latency_key | SKIP | not run in the PowerShell pane: its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake model: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@fake | agent_latency_enter | SKIP | not run in the PowerShell pane: its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the completed turn is unchecked (no OCR) on the relaunched screen (OCR; the store check above is the verdict); a fresh agent conversation was st |
| agent@fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 7, of which carrying a CR (\r): 1; |
| agent@fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 7, of which carrying a CR (\r): 1; |
| agent@fake | agent_steer_fast | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1828) |
| agent@fake | agent_interactive_less | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@fake | agent_noninteractive_no_takeover | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@fake | network_gate_notice | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| agent@fake | agent_gate_deny_allow | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent@fake | agent_gate_off_live | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent@fake | agent_mcp_faults | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does not use in this harness |
| agent@fake | agent_gate_key_never_leaves | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent@fake | agent_multitab_streaming | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Linux and macOS run it |
| agent@gitbash+fake | agent_interactive_password | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@gitbash+fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_fault_429 | FAIL | unexpected ERROR line: 2026-11-09T19:59:40Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Many Requests: BYOP open stream failed: HTTP status 429: Too Many Requests: {"error": "fake model fault: HTTP 429"}; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent@gitbash+fake | agent_fault_500 | FAIL | unexpected ERROR line: 2026-11-09T20:00:09Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Internal Server Error: BYOP open stream failed: HTTP status 500: Internal Server Error: {"error": "fake model fault: HTTP 500"}; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent@gitbash+fake | agent_fault_disconnect | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 requests): a retry would repeat output / tool calls; unexpected ERROR line: 2026-11-09T20:00:24Z [ERROR] Multi |
| agent@gitbash+fake | agent_fault_down | FAIL | unexpected ERROR line: 2026-11-09T20:01:01Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/streaming request failed: Web stream error for model 'fake-model (adapter: Ollama)'.; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent@gitbash+fake | agent_fault_slow_cancel | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent@gitbash+fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent@gitbash+fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent@gitbash+fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_latency_enter | SKIP | no Enter keydown -> hop=enter_begin pair in the app log (input-order logging off, or the Enters never reached the editor); a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent@gitbash+fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the completed turn is unchecked (no OCR) on the relaunched screen (OCR; the store check above is the verdict); a fresh agent conversation was st |
| agent@gitbash+fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context |
| agent@gitbash+fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context |
| agent@gitbash+fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 3, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_interactive_less | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | agent_noninteractive_no_takeover | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | network_gate_notice | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| agent@gitbash+fake | agent_gate_deny_allow | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent@gitbash+fake | agent_gate_off_live | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent@gitbash+fake | agent_mcp_faults | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does not use in this harness |
| agent@gitbash+fake | agent_gate_key_never_leaves | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent@gitbash+fake | agent_multitab_streaming | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Linux and macOS run it |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Latency (windows): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 4.1 / 5.5 | 2.6 / 3.8 (conhost (powershell.exe)) | 1.54 +/- 0.18 | overhead <= 20 ms | - | 4.6 (-0.47 ms, -10.2%) PASS | PASS | n=30 |
| terminal | latency_enter/enter_to_start | 45.1 / 63.3 | 7.4 / 18.4 (conhost (powershell.exe)) | 37.64 +/- 1.55 | overhead <= 2 ms | - | 49.6 (-4.57 ms, -9.2%) PASS | FAIL | n=60 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 83.5 / 103.8 | 76.2 / 96.8 (conhost (bash)) | 7.25 +/- 4.25 | overhead <= 20 ms | - | 80.2 (+3.23 ms, +4.0%) WARN | WARN | n=30 |
| terminal@gitbash | latency_enter/enter_to_start | 94.8 / 169.9 | 63.6 / 76.8 (conhost (bash)) | 31.24 +/- 2.49 | overhead <= 2 ms | - | 95.0 (-0.16 ms, -0.2%) PASS | FAIL | n=60 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.5 / 4.2 | - | - | overhead <= 2 ms | - | 3.4 (+0.10 ms, +2.9%) PASS | NO-REFERENCE | n=60 |
| terminal@gitbash | latency_key_log/ui_work | - | - | - | - | - | - | WARN | n=2 |
| agent@gitbash+fake | agent_latency_key/idle | 3.5 / 6.2 | 3.4 / 4.3 (same app, terminal-mode input editor) | 0.14 +/- 0.11 | overhead <= 2 ms | - | - | PASS | n=60 |
| agent@gitbash+fake | agent_latency_key/streaming | 1.5 / 3.9 | 3.4 / 4.3 (same app, terminal-mode input editor) | -1.95 +/- 0.09 | overhead <= 2 ms | - | - | PASS | n=87 |

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

## PROVING cases (owner decides graduation)

Platform windows. Nothing here moves a case: a passed case graduates only when the owner says so (docs/testing.md, 'New tests run in the fix tier first').

| case | mode | status | detail |
|---|---|---|---|
| agent_edit_accept | agent@fake | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent_edit_accept | agent@gitbash+fake | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent_edit_conflict | agent@fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_conflict | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_endings | agent@fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_endings | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_reject | agent@fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_reject | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_touch_only | agent@fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_touch_only | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_fault_429 | agent@fake | FAIL | unexpected ERROR line: 2026-11-09T19:52:49Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Many Requests: BYOP ope |
| agent_fault_429 | agent@gitbash+fake | FAIL | unexpected ERROR line: 2026-11-09T19:59:40Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 429 Too Many Requests: BYOP ope |
| agent_fault_500 | agent@fake | FAIL | unexpected ERROR line: 2026-11-09T19:53:18Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Internal Server Error: BYOP |
| agent_fault_500 | agent@gitbash+fake | FAIL | unexpected ERROR line: 2026-11-09T20:00:09Z [ERROR] MultiAgent request failed after 0 recovery attempts: Failed with status code 500 Internal Server Error: BYOP |
| agent_fault_disconnect | agent@fake | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 requests): a retry would  |
| agent_fault_disconnect | agent@gitbash+fake | FAIL | the turn ended in 'success' (hop=turn), expected `error` after the disconnect fault; a stream cut AFTER output was sent was re-sent (2 requests): a retry would  |
| agent_fault_down | agent@fake | FAIL | unexpected ERROR line: 2026-11-09T19:54:11Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/streaming request fail |
| agent_fault_down | agent@gitbash+fake | FAIL | unexpected ERROR line: 2026-11-09T20:01:01Z [ERROR] MultiAgent request failed after 0 recovery attempts: BYOP open stream failed: Network/streaming request fail |
| agent_fault_queued_rows | agent@fake | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversa |
| agent_fault_queued_rows | agent@gitbash+fake | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversa |
| agent_fault_slow_cancel | agent@fake | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not i |
| agent_fault_slow_cancel | agent@gitbash+fake | FAIL | during the first-token delay the turn is None, expected in_progress; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not i |
| agent_gate_deny_allow | agent@fake | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent_gate_deny_allow | agent@gitbash+fake | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent_gate_key_never_leaves | agent@fake | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent_gate_key_never_leaves | agent@gitbash+fake | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent_gate_off_live | agent@fake | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent_gate_off_live | agent@gitbash+fake | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent_interactive_less | agent@fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_less | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_password | agent@fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_password | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_ssh | agent@fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_ssh | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_vim | agent@fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_vim | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_latency_enter | agent@fake | SKIP | not run in the PowerShell pane: its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828) |
| agent_latency_enter | agent@gitbash+fake | SKIP | no Enter keydown -> hop=enter_begin pair in the app log (input-order logging off, or the Enters never reached the editor); a fresh agent conversation was starte |
| agent_latency_key | agent@fake | SKIP | not run in the PowerShell pane: its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake model: runs in the Git B |
| agent_latency_key | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent_lrc_no_false_takeover | agent | SKIP | not run on Windows: #1796: the interactive-prompt take-over it tests (PromptWatch, termios + /proc tty-read probe) is unix-only; ConPTY has no termios |
| agent_lrc_no_false_takeover | agent@gitbash | SKIP | not run on Windows: #1796: the interactive-prompt take-over it tests (PromptWatch, termios + /proc tty-read probe) is unix-only; ConPTY has no termios |
| agent_mcp_faults | agent@fake | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does not use in this harne |
| agent_mcp_faults | agent@gitbash+fake | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does not use in this harne |
| agent_multitab_streaming | agent@fake | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Linux and macOS run it |
| agent_multitab_streaming | agent@gitbash+fake | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Linux and macOS run it |
| agent_noninteractive_no_takeover | agent@fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_noninteractive_no_takeover | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_plugins_offline | terminal | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty |
| agent_plugins_offline | terminal@gitbash | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty |
| agent_restart_kill_blocked | agent@fake | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent conversation was star |
| agent_restart_kill_blocked | agent@gitbash+fake | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent conversation was star |
| agent_restart_kill_queued | agent@fake | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversa |
| agent_restart_kill_queued | agent@gitbash+fake | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversa |
| agent_restart_kill_stream | agent@fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the completed turn is unchecke |
| agent_restart_kill_stream | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the completed turn is unchecke |
| agent_restart_quit_blocked | agent@fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_blocked | agent@gitbash+fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_queued | agent@fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_queued | agent@gitbash+fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_stream | agent@fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_stream | agent@gitbash+fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_steer_fast | agent@fake | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1828) |
| agent_steer_fast | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 3, of which carrying a CR (\r): 0; |
| cb_report_problem | terminal | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_D |
| cb_report_problem | terminal@gitbash | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_D |
| claude_plugin_offline | terminal | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| claude_plugin_offline | terminal@gitbash | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| ctrl_c_flood_ab | terminal | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_flood_ab | terminal@gitbash | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_flood_ab | agent | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_flood_ab | agent@gitbash | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_no_esc_i_local | terminal | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| ctrl_c_no_esc_i_local | terminal@gitbash | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| ctrl_c_no_esc_i_local | agent | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| ctrl_c_no_esc_i_local | agent@gitbash | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| keyring_unavailable_startup | terminal | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| keyring_unavailable_startup | terminal@gitbash | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| keyring_unavailable_startup | agent | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| keyring_unavailable_startup | agent@gitbash | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| network_gate_notice | agent@fake | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| network_gate_notice | agent@gitbash+fake | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| network_gate_switch | terminal | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| network_gate_switch | terminal@gitbash | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| remote_tab_enter_no_helper | terminal | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801): runs in the Git Ba |
| remote_tab_enter_no_helper | terminal@gitbash | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT220_to913369 >> ~/.appci_where'], want the COMPLETED 'cd phxte_t217_aod2k6/' (the uncompleted 'cd ph |
| remote_tab_enter_no_helper | agent | SKIP | not run in the PowerShell pane: rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801): runs in the Git Ba |
| remote_tab_enter_no_helper | agent@gitbash | FAIL | Tab + Enter 30 ms later: the remote shell received ['echo wT108_oihl0dg8 >> ~/.appci_where'], want the COMPLETED 'cd phxte_t105_d75qpv/' (the uncompleted 'cd ph |
| remote_tab_no_helper | terminal | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash |
| remote_tab_no_helper | terminal@gitbash | FAIL | Tab after 'cd phxtab_t211_z' should have completed the REMOTE directory: the remote shell received ['echo wT212_pfec7644 >> ~/.appci_where'], want 'cd phxtab_t2 |
| remote_tab_no_helper | agent | SKIP | not run in the PowerShell pane: rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash |
| remote_tab_no_helper | agent@gitbash | FAIL | Tab after 'cd phxtab_t99_7' should have completed the REMOTE directory: the remote shell received ['echo wT100_kmxy7ug0 >> ~/.appci_where'], want 'cd phxtab_t99 |
| subshell_ready_docker_t_without_i | terminal | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| subshell_ready_docker_t_without_i | terminal@gitbash | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| subshell_ready_docker_t_without_i | agent | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| subshell_ready_docker_t_without_i | agent@gitbash | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| subshell_ready_exec_fish_rc | terminal | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| subshell_ready_exec_fish_rc | terminal@gitbash | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| subshell_ready_exec_fish_rc | agent | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| subshell_ready_exec_fish_rc | agent@gitbash | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| subshell_ready_rc_clear | terminal | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| subshell_ready_rc_clear | terminal@gitbash | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| subshell_ready_rc_clear | agent | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| subshell_ready_rc_clear | agent@gitbash | SKIP | not run on Windows: bash --rcfile with clear on a real pty (#1679 step 3) |
| subshell_ready_rc_questions | terminal | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| subshell_ready_rc_questions | terminal@gitbash | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| subshell_ready_rc_questions | agent | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| subshell_ready_rc_questions | agent@gitbash | SKIP | not run on Windows: bash --rcfile with read -p questions on a real pty; the readiness judge's OS facts are POSIX (#1679 step 3) |
| subshell_ready_relay_shape | terminal | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| subshell_ready_relay_shape | terminal@gitbash | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| subshell_ready_relay_shape | agent | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| subshell_ready_relay_shape | agent@gitbash | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| subshell_ready_secret_after_prompt | terminal | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| subshell_ready_secret_after_prompt | terminal@gitbash | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| subshell_ready_secret_after_prompt | agent | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| subshell_ready_secret_after_prompt | agent@gitbash | SKIP | not run on Windows: bash PROMPT_COMMAND read -s on a real pty (#1679 step 3) |
| subshell_ready_unsupported_tcsh | terminal | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| subshell_ready_unsupported_tcsh | terminal@gitbash | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| subshell_ready_unsupported_tcsh | agent | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| subshell_ready_unsupported_tcsh | agent@gitbash | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| subshell_ready_vim_typeahead | terminal | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| subshell_ready_vim_typeahead | terminal@gitbash | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| subshell_ready_vim_typeahead | agent | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| subshell_ready_vim_typeahead | agent@gitbash | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| subshell_ready_zsh_newuser_install | terminal | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| subshell_ready_zsh_newuser_install | terminal@gitbash | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| subshell_ready_zsh_newuser_install | agent | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| subshell_ready_zsh_newuser_install | agent@gitbash | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| subshell_sh_probe | terminal | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| subshell_sh_probe | terminal@gitbash | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| subshell_sh_probe | agent | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| subshell_sh_probe | agent@gitbash | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| ui_completions_as_you_type_local | terminal | SKIP | not run in the PowerShell pane: asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test) |
| ui_completions_as_you_type_local | terminal@gitbash | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked |
| zero_network_startup | terminal | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| zero_network_startup | terminal@gitbash | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |

passed in every pass it ran in on this platform (candidates for the owner's graduation decision, 1): agent_restart_kill_stream
not passed (fail / skip / no-reference): agent_edit_accept, agent_edit_conflict, agent_edit_endings, agent_edit_reject, agent_edit_touch_only, agent_fault_429, agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_queued_rows, agent_fault_slow_cancel, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_less, agent_interactive_password, agent_interactive_ssh, agent_interactive_vim, agent_latency_enter, agent_latency_key, agent_lrc_no_false_takeover, agent_mcp_faults, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_plugins_offline, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_quit_blocked, agent_restart_quit_queued, agent_restart_quit_stream, agent_steer_fast, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, keyring_unavailable_startup, network_gate_notice, network_gate_switch, remote_tab_enter_no_helper, remote_tab_no_helper, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, ui_completions_as_you_type_local, zero_network_startup

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
