# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=0b817b35, sha=0b817b355e525bbd5a86577296f927d05e30f92f, owner_settings=sha256 a0613b641a27, tier=smoke, scale=1)

pass 58, fail 24, skip 63, no-reference 3 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS | pty writes: 63, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; the injector/O |
| terminal | fast_typing | PASS | pty writes: 242, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | paste_instant_enter | PASS | pty writes: 14, of which carrying a CR (\r): 2; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 8.88, -> LineEditorReading 60.9; app: line editor ready 63.8, active 63.8, next command written 3109.3, its Preexec 3128.7; #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; Ctrl-C: same trap/t |
| terminal | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal | ps_ctrl_break | FAIL | Ctrl+Break reached the program as SIGBREAK in 0/3 runs: [['READY'], ['READY'], ['READY']]; pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys') |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS | pty writes: 62, of which carrying a CR (\r): 0; Enter(s) the input editor started: 1 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; pty write |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters |
| terminal@gitbash | fast_typing | PASS | pty writes: 202, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | paste_instant_enter | PASS |  |
| terminal@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | ctrl_c_d | FAIL | REGRESSION Ctrl-C: median 87.01 vs baseline build +6.77 ms (+8.4%); re-measured once: median 83.89 (warn); #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; NO-REFERENCE: Ctrl- |
| terminal@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| terminal@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's msys signals differ |
| terminal@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold gates instead |
| terminal@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| terminal@gitbash | subshell_ready_rc_questions | FAIL | two answers re-armed the setup (hops verdict=ReArm): 0; the setup was written (hop subshell_setup_ready_write); no cancel: the answers were ReArms, not UserDrove |
| terminal@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| terminal@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS facts on ConPTY (#1679 step 3, audit finding 5); the ConPTY prompt-hold half of the same secret is gitbash_precmd_secret (#1883) |
| terminal@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| terminal@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| terminal@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| terminal@gitbash | subshell_ready_rc_clear | FAIL | the setup was written after the clear |
| terminal@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| terminal@gitbash | ssh_enter_then_immediate_command | PASS | ssh trial verdicts: exact=3 |
| terminal@gitbash | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in the LOCAL shell, not the remote one: `exit` NOT typed; stopping; ssh trial verdicts: exact=1 |
| terminal@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| terminal@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| terminal@gitbash | latency_enter | NOREF | G1: Git Bash fork+exec costs 40.9 ms per process on this host (outside the app); every forking shell hook adds about that; app share of Enter->start: enter_begin -> pty_write median 0.90 ms (from the app's input_order log); the remaining overhead is shell/ConPTY side; shell side: pt |
| terminal@gitbash | latency_key_log | NOREF | #1176 key -> pty write median 0.68 ms (p95 0.92, n=60): within the 1 ms target; NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics); key to fr |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| terminal@gitbash | remote_tab_no_helper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| terminal@gitbash | remote_tab_enter_no_helper | PASS |  |
| terminal@gitbash | ui_completions_as_you_type_local | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, /snap/bin) |
| terminal@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| terminal@gitbash | ps_read_host_secure | SKIP | not run in the Git Bash pane: #1882/#1891: Read-Host -AsSecureString is PowerShell; the Git Bash twin is gitbash_precmd_secret |
| terminal@gitbash | gitbash_precmd_secret | FAIL | the PIN did not reach `read -s` whole and in order: gbpin.txt holds [], want ['pint242vjy1u8'] (the first half was handed to the editor as text, or keys were lost); the line typed after the read did not run; pty writes: 3, of which carrying a CR (\r): 0; Enter(s) the input editor started: 5 (an Enter may ride in a multi-lin |
| terminal@gitbash | pane_not_local_after_gitbash_precmd_secret | FAIL | after gitbash_precmd_secret the pane's shell was not the root shell (none; root pid 828); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | md_viewer | PASS | no pointer synthesis on this backend: the pointer stays where the last case left it; no OCR engine (tesseract / Apple Vision): screenshot shots/terminal@gitbash_md_small_noocr.png only, table text and diagram labels NOT machine-verified |
| terminal@gitbash | zero_network_startup | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |
| terminal@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| terminal@gitbash | claude_plugin_offline | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| terminal@gitbash | agent_plugins_offline | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty |
| terminal@gitbash | network_gate_switch | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| terminal@gitbash | cb_report_problem | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_DIAG_BUNDLE_DIR in the app's environment to enable) |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS | pty writes: 174, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | paste_instant_enter | PASS | pty writes: 15, of which carrying a CR (\r): 3; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent | ctrl_c_d | PASS | ^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt 16.21, -> LineEditorReading 66.24; app: line editor ready 66.6, active 66.6, next command written 3075.1, its Preexec 3097.4; #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; fix tier, agent mod |
| agent | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent | ps_ctrl_break | FAIL | Ctrl+Break reached the program as SIGBREAK in 0/3 runs: [['READY'], ['READY'], ['READY']]; pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | harness_crash | FAIL | the agent pass crashed outside a case: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys') |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | PASS | pty writes: 149, of which carrying a CR (\r): 0; Enter(s) the input editor started: 30 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | paste_instant_enter | PASS |  |
| agent@gitbash | tab_enter | PASS | pty writes: 24, of which carrying a CR (\r): 0; Enter(s) the input editor started: 12 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | ctrl_c_d | PASS | #1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells; fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only; the overhead budget is judged in terminal mode; Ctrl-D half not run on Windows: |
| agent@gitbash | ctrl_c_flood_ab | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| agent@gitbash | ps_ctrl_break | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's msys signals differ |
| agent@gitbash | ps_fast_command_young_enter | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold gates instead |
| agent@gitbash | subshell_sh_probe | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| agent@gitbash | subshell_ready_rc_questions | FAIL | two answers re-armed the setup (hops verdict=ReArm): 0; the setup was written (hop subshell_setup_ready_write); no cancel: the answers were ReArms, not UserDrove |
| agent@gitbash | subshell_ready_vim_typeahead | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_unsupported_tcsh | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| agent@gitbash | subshell_ready_secret_after_prompt | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS facts on ConPTY (#1679 step 3, audit finding 5); the ConPTY prompt-hold half of the same secret is gitbash_precmd_secret (#1883) |
| agent@gitbash | subshell_ready_relay_shape | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| agent@gitbash | subshell_ready_exec_fish_rc | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| agent@gitbash | subshell_ready_docker_t_without_i | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| agent@gitbash | subshell_ready_zsh_newuser_install | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| agent@gitbash | subshell_ready_rc_clear | FAIL | the setup was written after the clear |
| agent@gitbash | ssh_connect_typing | PASS | ssh_connect_typing gap 0.0 s trial 3: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, finish none; the app saw every key of the line; remote got None; ssh_connect_typing gap 0.1 s trial 1: #1292: no ssh hold was seen and the remote shell logged nothing for the line; held 0 key(s)/0 byte(s), hand-over none, fin |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T92_fmb5ir never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosphor.log). Test host: sshd.log: 15:29:26 client input ended: 70 bytes in 53 chunks, 70 written to the pseudo console / 15:29:26 connection closed: authenticated=False channels=1 / 15:29:27 connect from ('127.0.0.1', 61643) / 15:29:27 auth publickey user=Winters key=ssh-ed25519 8be4ca3a7389ebd5e0a47e6cdd44c995 -> ok / 15:29:27 auth publickey user=Winters key=ssh-ed25519 8be4ca3a7389ebd5e0a47e6cdd44c995 -> ok / 15:29:27 session shell None pty / 15:29:27 conpty shell pid 5312 / 15:29:28 conpty first output after 0.62 s: 68 bytes; fakesshd.stderr: ConnectionResetError: [WinError 10054] An existing connection was forcibly closed by the remote host / During handling of the above exception, another exception occurred: / Traceback (most recent call last): /   File "C:\Users\<user>\AppData\Local\phosphor-ci\python312\Lib\site-packages\paramiko\transport.py", line 2043, in run /     self._check_banner() /   File "C:\Users\<user>\AppData\Local\phosphor-ci\python312\Lib\site-packages\paramiko\transport.py", line 2231, in _check_banner /     raise SSHException( / paramiko.ssh_exception.SSHException: Error reading SSH protocol banner[WinError 10054] An existing connection was forcibly closed by the remote host; test sshd (fakesshd-win) listening on 127.0.0.1:61186; app ssh hops: [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=kh_hold_entry seq=6023 holding=false held_bytes=0 gate_armed=true pending_session=false table=1 t=1794266967457.065 / [DEBUG] input_order hop=kh_hold_exit seq=6023 branch=gate_open_live gate_armed=true table=1 t=1794266967457.072 / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=kh_hold_entry seq=6024 holding=false held_bytes=0 gate_armed=true pending_session=false table=1 t=1794266967469.967 / [DEBUG] input_order hop=kh_hold_exit seq=6024 branch=gate_open_live gate_armed=true table=1 t=1794266967469.974 / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte / [DEBUG] input_order hop=kh_hold_entry seq=6025 holding=false held_bytes=0 gate_armed=true pending_session=false table=1 t=1794266967470.572 / [DEBUG] input_order hop=kh_hold_exit seq=6025 branch=gate_open_live gate_armed=true table=1 t=1794266967470.578 / [DEBUG] input_order hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_writte; PROBE: Git Bash pane: the ControlMaster probe with the wrapper's options FAILED before the case (C:\Program Files\Git\usr\bin\ssh.exe cannot start a ControlMaster, exit 255: mux_client_request_session: read from master failed: Connection reset by peer Failed to connect to new control master); the app's bash ssh wrapper starts `-o ControlMaster=yes ... -N -f` on every ssh, so a real Git Bash user's ssh is expected to fail the same way (candidate APP finding: no fallback to plain ssh when the master cannot start); remote shell received 1/2 immediately-typed commands exactly: ['T90_if3tac', 'T92_mb5ir']; ssh_enter_then_immediate_command trial 1: #1292: no ssh hold was seen |
| agent@gitbash | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in the LOCAL shell, not the remote one: `exit` NOT typed; stopping; ssh trial verdicts: exact=1 |
| agent@gitbash | ssh_connect_typing_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| agent@gitbash | ssh_nested_typeahead_wrapper | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| agent@gitbash | agent_stream_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: typed-while-streaming landing is screenshot-only (shots/agent_typed_while_streaming.png); Ctrl-C #1: stream stopped (2 distinct frames in 5), typ |
| agent@gitbash | agent_input_exact | PASS | pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | routing_table | PASS | pty writes: 30, of which carrying a CR (\r): 0; Enter(s) the input editor started: 27 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_mode_switch | PASS | pty writes: 4, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at) |
| agent@gitbash | agent_lrc_no_false_takeover | FAIL | part 2: `printf 'Continue? [y/N] '; read` sat at its prompt for 60 s and was neither handed to the user (a proven tty read must be, #1811) nor given the take-over notice (agent_command_wait awaited=None); a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; detector saw the quiet `Waiting:` line and kep |
| agent@gitbash | remote_tab_no_helper | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote directory is not observable from the harness; not judged here (#1801 follow-up: prompt-hook cwd listing) |
| agent@gitbash | remote_tab_enter_no_helper | PASS |  |
| agent@gitbash | ctrl_c_no_esc_i_local | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| agent@gitbash | keyring_unavailable_startup | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| agent@fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@fake | agent_interactive_password | FAIL | ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=None); a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 7, of which carrying a CR (\r): 1; |
| agent@fake | harness_crash | FAIL | the agent@fake pass crashed outside a case: RuntimeError('Phosphor PID unknown (app not running?): refusing to send keys') |
| agent@gitbash+fake | agent_interactive_password | SKIP | not run in the Git Bash pane: #1882: the hand-over on ConPTY is the PowerShell bootstrap's Read-Host marker; bash `read -s` in a Git Bash pane announces nothing (no termios), so the POSIX form of the case does not apply there |
| agent@gitbash+fake | agent_edit_accept | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent@gitbash+fake | agent_edit_reject | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_conflict | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_touch_only | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | agent_edit_endings | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent@gitbash+fake | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash+fake | agent_fault_429 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is checked from the app log only (shots/agent_f429_ocr.png); pty writes: 3, of which carrying a CR (\r): 0; Enter(s) th |
| agent@gitbash+fake | agent_fault_500 | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is checked from the app log only (shots/agent_f500_ocr.png); pty writes: 1, of which carrying a CR (\r): 0; Enter(s) th |
| agent@gitbash+fake | agent_fault_disconnect | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is checked from the app log only (shots/agent_fdisc_ocr.png); pty writes: 1, of which carrying a CR (\r): 0; Enter(s) t |
| agent@gitbash+fake | agent_fault_down | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is checked from the app log only (shots/agent_fdown_ocr.png); pty writes: 1, of which carrying a CR (\r): 0; Enter(s) t |
| agent@gitbash+fake | agent_fault_slow_cancel | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 1, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_fault_queued_rows | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 1, of which carrying a CR (\r): 0; |
| agent@gitbash+fake | agent_latency_key | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent@gitbash+fake | agent_interactive_vim | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | agent_restart_quit_stream | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_queued | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_restart_quit_blocked | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent@gitbash+fake | agent_question_answer | FAIL | answer: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Preparing question...'; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent@gitbash+fake | agent_question_cut | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_question_nodone | FAIL | nodone: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Preparing question...'; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent@gitbash+fake | agent_question_trunc | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_question_empty | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_question_slow | FAIL | slow: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Preparing question...'; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent@gitbash+fake | agent_question_ctrl_c | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_question_steer | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 4 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent@gitbash+fake | agent_question_after_command | FAIL | after the command ended the ask never became answerable (verdict hung); after_command: the prompt after the question made no model request (the input is stuck); after_command: the prompt after the question ended None, expected `succ |
| agent@gitbash+fake | agent_question_steer_at_end | FAIL | the steer prompt never reached the model; the turn is neither answerable nor ended (verdict hung, last turn state 'blocked'); the steer prompt reached the model 0 times; steer_at_end: the prompt after t |
| agent@gitbash+fake | agent_latency_enter | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent@gitbash+fake | agent_interactive_ssh | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent@gitbash+fake | agent_restart_kill_stream | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the completed turn is unchecked (no OCR) on the relaunched screen (OCR; the store check above is the verdict); a fresh agent conversation was st |
| agent@gitbash+fake | agent_restart_kill_queued | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context |
| agent@gitbash+fake | agent_restart_kill_blocked | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context |
| agent@gitbash+fake | agent_steer_fast | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 2, of which carrying a CR (\r): 0; Enter(s) the input editor started: 6 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
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
| terminal | ctrl_c_d/ctrl_c | 4.5 / 5.7 | 2.5 / 3.6 (conhost (powershell.exe)) | 2.01 +/- 0.27 | overhead <= 20 ms | - | 4.6 (-0.08 ms, -1.7%) PASS | PASS | n=30 |
| terminal@gitbash | ctrl_c_d/ctrl_c | 87.0 / 103.5 | 69.2 / 108.5 (conhost (bash)) | 17.86 +/- 4.41 | overhead <= 20 ms | - | 80.2 (+6.77 ms, +8.4%) FAIL | FAIL | n=30 |
| | re-measure | 83.9 / 107.9 | | | | | | | |
| terminal@gitbash | ctrl_c_d/ctrl_c_flood | 74.3 / 115.1 | - | - | overhead <= 45 ms | - | - | NO-REFERENCE | n=30 |
| terminal@gitbash | latency_enter/enter_to_start | 95.5 / 199.1 | 65.3 / 82.5 (conhost (bash)) | 30.22 +/- 2.00 | overhead <= 2 ms | - | 95.0 (+0.55 ms, +0.6%) PASS | FAIL | n=60 |
| terminal@gitbash | latency_key_log/key_to_frame | 3.5 / 4.2 | - | - | overhead <= 2 ms | - | 3.4 (+0.14 ms, +4.1%) PASS | NO-REFERENCE | n=60 |
| terminal@gitbash | latency_key_log/ui_work | - | - | - | - | - | - | WARN | n=1 |
| agent@gitbash+fake | agent_latency_key/idle | 3.5 / 4.7 | 3.4 / 4.4 (same app, terminal-mode input editor) | 0.10 +/- 0.10 | overhead <= 2 ms | - | - | PASS | n=60 |
| agent@gitbash+fake | agent_latency_key/streaming | 1.4 / 3.5 | 3.4 / 4.4 (same app, terminal-mode input editor) | -1.91 +/- 0.08 | overhead <= 2 ms | - | - | PASS | n=87 |
| agent@gitbash+fake | agent_latency_enter/idle | 0.3 / 0.4 | 0.3 / 0.5 (same app, terminal-mode input editor) | -0.02 +/- 0.03 | overhead <= 2 ms | - | - | PASS | n=60 |
| agent@gitbash+fake | agent_latency_enter/streaming | 0.2 / 0.3 | 0.3 / 0.5 (same app, terminal-mode input editor) | -0.13 +/- 0.02 | overhead <= 2 ms | - | - | PASS | n=60 |

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
| agent_edit_accept | agent@gitbash+fake | SKIP | not run on Windows: #1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only |
| agent_edit_conflict | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_endings | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_reject | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_edit_touch_only | agent@gitbash+fake | SKIP | not run on Windows: #1835: same as agent_edit_accept |
| agent_fault_429 | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent_fault_500 | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent_fault_disconnect | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent_fault_down | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; no OCR engine: the error block's wording is ch |
| agent_fault_queued_rows | agent@gitbash+fake | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversa |
| agent_fault_slow_cancel | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 1, of which carrying a CR (\r): 0; |
| agent_gate_deny_allow | agent@gitbash+fake | SKIP | not run on Windows: Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml |
| agent_gate_key_never_leaves | agent@gitbash+fake | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers |
| agent_gate_off_live | agent@gitbash+fake | SKIP | not run on Windows: Linux-only: live edits of the Linux profile's settings.toml |
| agent_interactive_less | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_password | agent@fake | FAIL | ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=None); a fresh agent conversation was started (Escape, /agent) so earlier cases |
| agent_interactive_password | agent@gitbash+fake | SKIP | not run in the Git Bash pane: #1882: the hand-over on ConPTY is the PowerShell bootstrap's Read-Host marker; bash `read -s` in a Git Bash pane announces nothing |
| agent_interactive_ssh | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_interactive_vim | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_latency_enter | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent_latency_key | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; a fresh agent conversation was started (Escape |
| agent_lrc_no_false_takeover | agent@gitbash | FAIL | part 2: `printf 'Continue? [y/N] '; read` sat at its prompt for 60 s and was neither handed to the user (a proven tty read must be, #1811) nor given the take-ov |
| agent_mcp_faults | agent@gitbash+fake | SKIP | not run on Windows: #1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does not use in this harne |
| agent_multitab_streaming | agent@gitbash+fake | SKIP | not run on Windows: #1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Linux and macOS run it |
| agent_noninteractive_no_takeover | agent@gitbash+fake | SKIP | not run on Windows: #1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane |
| agent_plugins_offline | terminal@gitbash | SKIP | not run on Windows: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty |
| agent_question_after_command | agent@gitbash+fake | FAIL | after the command ended the ask never became answerable (verdict hung); after_command: the prompt after the question made no model request (the input is stuck); |
| agent_question_answer | agent@gitbash+fake | FAIL | answer: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Preparing question... |
| agent_question_ctrl_c | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent_question_cut | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent_question_empty | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent_question_nodone | agent@gitbash+fake | FAIL | nodone: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Preparing question... |
| agent_question_slow | agent@gitbash+fake | FAIL | slow: the turn is neither answerable nor ended after 150 s (verdict hung, last turn state 'blocked', ask actions []): the card hangs on 'Preparing question...'; |
| agent_question_steer | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent_question_steer_at_end | agent@gitbash+fake | FAIL | the steer prompt never reached the model; the turn is neither answerable nor ended (verdict hung, last turn state 'blocked'); the steer prompt reached the model |
| agent_question_trunc | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 0, of which carrying a CR (\r): 0; |
| agent_restart_kill_blocked | agent@gitbash+fake | SKIP | ar_kill_blocked: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven; a fresh agent conversation was star |
| agent_restart_kill_queued | agent@gitbash+fake | SKIP | the 2 /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged; a fresh agent conversa |
| agent_restart_kill_stream | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; ar_kill_stream: the completed turn is unchecke |
| agent_restart_quit_blocked | agent@gitbash+fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_queued | agent@gitbash+fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_restart_quit_stream | agent@gitbash+fake | SKIP | a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows) (this platform: windows) |
| agent_steer_fast | agent@gitbash+fake | PASS | a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context; pty writes: 2, of which carrying a CR (\r): 0; |
| cb_report_problem | terminal@gitbash | SKIP | not run on Windows: the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_D |
| claude_plugin_offline | terminal@gitbash | SKIP | not run on Windows: Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq |
| ctrl_c_flood_ab | terminal | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_flood_ab | terminal@gitbash | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_flood_ab | agent | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_flood_ab | agent@gitbash | SKIP | not run on Windows: #1838: the blocked-frame gate it A/Bs exists only in the macOS renderer |
| ctrl_c_no_esc_i_local | terminal@gitbash | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| ctrl_c_no_esc_i_local | agent@gitbash | SKIP | the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log |
| gitbash_precmd_secret | terminal@gitbash | FAIL | the PIN did not reach `read -s` whole and in order: gbpin.txt holds [], want ['pint242vjy1u8'] (the first half was handed to the editor as text, or keys were lo |
| keyring_unavailable_startup | terminal@gitbash | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| keyring_unavailable_startup | agent@gitbash | SKIP | not run on Windows: #1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files) |
| network_gate_notice | agent@gitbash+fake | SKIP | not run on Windows: needs the Linux pointer + OCR path and a relaunch on a rewritten settings file |
| network_gate_switch | terminal@gitbash | SKIP | not run on Windows: needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout |
| ps_ctrl_break | terminal | FAIL | Ctrl+Break reached the program as SIGBREAK in 0/3 runs: [['READY'], ['READY'], ['READY']]; pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input ed |
| ps_ctrl_break | terminal@gitbash | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's msys signals differ |
| ps_ctrl_break | agent | FAIL | Ctrl+Break reached the program as SIGBREAK in 0/3 runs: [['READY'], ['READY'], ['READY']]; pty writes: 12, of which carrying a CR (\r): 0; Enter(s) the input ed |
| ps_ctrl_break | agent@gitbash | SKIP | not run in the Git Bash pane: #1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's msys signals differ |
| ps_fast_command_young_enter | terminal@gitbash | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold gates instead |
| ps_fast_command_young_enter | agent@gitbash | SKIP | not run in the Git Bash pane: #1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold gates instead |
| ps_read_host_secure | terminal@gitbash | SKIP | not run in the Git Bash pane: #1882/#1891: Read-Host -AsSecureString is PowerShell; the Git Bash twin is gitbash_precmd_secret |
| remote_tab_enter_no_helper | terminal@gitbash | PASS |  |
| remote_tab_enter_no_helper | agent@gitbash | PASS |  |
| remote_tab_no_helper | terminal@gitbash | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote directory is not obse |
| remote_tab_no_helper | agent@gitbash | FAIL | the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801); path highlighting of a typed remote directory is not obse |
| ssh_connect_typing_wrapper | terminal@gitbash | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| ssh_connect_typing_wrapper | agent@gitbash | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| ssh_nested_typeahead_wrapper | terminal@gitbash | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| ssh_nested_typeahead_wrapper | agent@gitbash | SKIP | set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884) |
| subshell_ready_docker_t_without_i | terminal@gitbash | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| subshell_ready_docker_t_without_i | agent@gitbash | SKIP | not run on Windows: docker -t without -i against a Linux pty (#1679 step 3) |
| subshell_ready_exec_fish_rc | terminal@gitbash | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| subshell_ready_exec_fish_rc | agent@gitbash | SKIP | not run on Windows: fish exec'd from a bash rc on a real pty (#1679 step 3) |
| subshell_ready_rc_clear | terminal@gitbash | FAIL | the setup was written after the clear |
| subshell_ready_rc_clear | agent@gitbash | FAIL | the setup was written after the clear |
| subshell_ready_rc_questions | terminal@gitbash | FAIL | two answers re-armed the setup (hops verdict=ReArm): 0; the setup was written (hop subshell_setup_ready_write); no cancel: the answers were ReArms, not UserDrov |
| subshell_ready_rc_questions | agent@gitbash | FAIL | two answers re-armed the setup (hops verdict=ReArm): 0; the setup was written (hop subshell_setup_ready_write); no cancel: the answers were ReArms, not UserDrov |
| subshell_ready_relay_shape | terminal@gitbash | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| subshell_ready_relay_shape | agent@gitbash | SKIP | not run on Windows: sudo -s with use_pty does not exist on Windows (#1679 step 3) |
| subshell_ready_secret_after_prompt | terminal@gitbash | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS facts on ConPTY (#1679  |
| subshell_ready_secret_after_prompt | agent@gitbash | SKIP | not run on Windows: bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS facts on ConPTY (#1679  |
| subshell_ready_unsupported_tcsh | terminal@gitbash | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| subshell_ready_unsupported_tcsh | agent@gitbash | SKIP | not run on Windows: tcsh does not exist in a Windows pane (#1679 step 3) |
| subshell_ready_vim_typeahead | terminal@gitbash | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| subshell_ready_vim_typeahead | agent@gitbash | SKIP | not run on Windows: bash --rcfile + vim on a real pty (#1679 step 3) |
| subshell_ready_zsh_newuser_install | terminal@gitbash | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| subshell_ready_zsh_newuser_install | agent@gitbash | SKIP | not run on Windows: zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3) |
| subshell_sh_probe | terminal@gitbash | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| subshell_sh_probe | agent@gitbash | SKIP | not run on Windows: dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710) |
| ui_completions_as_you_type_local | terminal@gitbash | NOREF | NO-REFERENCE: UNVERIFIABLE: no OCR on this machine (tesseract): the completion menu is not checked |
| zero_network_startup | terminal@gitbash | SKIP | not run on Windows: Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet |

passed in every pass it ran in on this platform (candidates for the owner's graduation decision, 15): agent_fault_429, agent_fault_500, agent_fault_disconnect, agent_fault_down, agent_fault_slow_cancel, agent_latency_enter, agent_latency_key, agent_question_ctrl_c, agent_question_cut, agent_question_empty, agent_question_steer, agent_question_trunc, agent_restart_kill_stream, agent_steer_fast, remote_tab_enter_no_helper
not passed (fail / skip / no-reference): agent_edit_accept, agent_edit_conflict, agent_edit_endings, agent_edit_reject, agent_edit_touch_only, agent_fault_queued_rows, agent_gate_deny_allow, agent_gate_key_never_leaves, agent_gate_off_live, agent_interactive_less, agent_interactive_password, agent_interactive_ssh, agent_interactive_vim, agent_lrc_no_false_takeover, agent_mcp_faults, agent_multitab_streaming, agent_noninteractive_no_takeover, agent_plugins_offline, agent_question_after_command, agent_question_answer, agent_question_nodone, agent_question_slow, agent_question_steer_at_end, agent_restart_kill_blocked, agent_restart_kill_queued, agent_restart_quit_blocked, agent_restart_quit_queued, agent_restart_quit_stream, cb_report_problem, claude_plugin_offline, ctrl_c_flood_ab, ctrl_c_no_esc_i_local, gitbash_precmd_secret, keyring_unavailable_startup, network_gate_notice, network_gate_switch, ps_ctrl_break, ps_fast_command_young_enter, ps_read_host_secure, remote_tab_no_helper, ssh_connect_typing_wrapper, ssh_nested_typeahead_wrapper, subshell_ready_docker_t_without_i, subshell_ready_exec_fish_rc, subshell_ready_rc_clear, subshell_ready_rc_questions, subshell_ready_relay_shape, subshell_ready_secret_after_prompt, subshell_ready_unsupported_tcsh, subshell_ready_vim_typeahead, subshell_ready_zsh_newuser_install, subshell_sh_probe, ui_completions_as_you_type_local, zero_network_startup

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
