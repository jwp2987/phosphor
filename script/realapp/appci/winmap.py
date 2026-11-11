# appci/winmap.py: which cases run in which Windows pane shell, and the stated reason for every one that does not (#1488: "every case runs on
# every platform or skips with a reason", never silently). Linux and macOS never consult this file.
#
# A Windows run has up to two PANE SHELLS per session mode (APPCI_WIN_SHELLS, default "powershell,gitbash"):
#   powershell  the app's default shell. It is started with -NoProfile, so there is no rc hook: no prompt.log / rec.log. Cases use effect files
#               (PowerShell syntax: Ctx.append_cmd('pwsh', ...)) and fixed settles where the POSIX harness waits for a prompt.
#   gitbash     session.startup_shell_override = Git for Windows' bash.exe with HOME = the scratch home, so ~/.bashrc logs exactly like on Linux.
#               The POSIX cases run here unchanged. Result rows of this pass carry the mode label '<mode>@gitbash'.
BOTH = {
    # shell-neutral (append_cmd / pane_shell) or self-skipping with their own explicit reason
    "mode_proof", "cadence", "fast_typing", "rollover", "keydown_count", "paste_instant_enter", "arrows_behind_paste", "tab_enter", "ctrl_c_d",
    "ssh_connect_typing", "ssh_enter_then_immediate_command", "ssh_nested_typeahead", "ssh_failing_not_local",
    "idle_cpu", "enter_loss_control", "latency_enter", "latency_paint", "latency_key_views_ab", "tabs_two_windows", "tabs_streaming",
    "paste_multiline_prompt", "intl_wide_type", "intl_wide_paste", "intl_wide_edit", "intl_altgr_bash", "intl_altgr_ps",
    "cb_no_autoupdate", "approval_card_pause", "md_viewer", "md_viewer_big", "md_viewer_ai_answer", "agent_code_recycle", "editor_memory_7mb", "gitbash_subshell",
    "editor_edit_save", "editor_paste", "editor_save_mixed_endings",         # #1544: phosphor.exe <file URL> reaches the running instance over its named pipe (cases.open_file_in_windows_app)
    "upgrade_over_previous",                 # #1546: skips itself without APPCI_PREV_FIXTURE / APPCI_PREV_BIN
    "startup_shell_pane",                    # #1534: itself skips outside a startup_shell_override pass
    "ctrl_c_no_esc_i_local",                 # #1798: itself skips on Windows with its reason
    "giveup_keys_reach_shell",               # owner decision 2026-11-09 (#1661 reversed): shell-neutral (append_cmd / pane_shell), effect file + app log
    "output_frames",                         # #1979: a bounded stream line for PowerShell and bash, verdicts from the app log only (skips itself in a cmd.exe pane)
    "agent_lrc_no_false_takeover",           # #1882: PowerShell / Git Bash commands in both panes; ConPTY has no tty-read probe, so a printed prompt gets the notice only (the Read-Host marker is the hand-over, agent_interactive_password)
    # #1831/#1832: the fake model runs on the host (127.0.0.1); effect files and the sqlite store, no rc hook needed. The quit variants skip themselves on Windows (no quit key).
    "agent_fault_429", "agent_fault_500", "agent_fault_disconnect", "agent_fault_down", "agent_fault_slow_cancel", "agent_fault_queued_rows",
    "agent_restart_quit_stream", "agent_restart_quit_queued", "agent_restart_quit_blocked", "agent_restart_kill_stream", "agent_restart_kill_queued", "agent_restart_kill_blocked",
    "agent_question_answer", "agent_question_cut", "agent_question_nodone", "agent_question_trunc", "agent_question_empty", "agent_question_slow",   # #1851: the fake model on the host
    "agent_question_ctrl_c", "agent_question_steer", "agent_question_after_command", "agent_question_steer_at_end",
    # #2072 (round 14 wave 1): the received line comes from rec.log or PSReadLine's history (cases_r14_w1a.Received); the cmd.exe pane skips itself
    "history_up_recall", "tab_lifecycle_keys",
    # #2072 (r14-test-gaps T9, T10): both handle the pane themselves. meta_and_word_keys reads PSReadLine history in the PowerShell pane and runs its
    # raw-reader half in the Git Bash pane only; first_run_defaults SKIPs in the Git Bash pane (that pane exists only through a setting)
    "meta_and_word_keys", "first_run_defaults",
    # r14 wave 2b. T16: the fake model on the host and app-log hops only; it skips itself when the profile has vertical tabs off (no live settings
    # rewrite on Windows). T19: append_cmd / pane_shell, effect file, rec.log or the PSReadLine history; stop by pid and taskkill.
    "agent_hidden_tab_attention", "history_survives_restart",
    "crash_kill9_restore", "crash_corrupt_db",       # #1541: taskkill /F /T + relaunch on the same profile (cases_robust.hard_kill; state under %LOCALAPPDATA%\\phosphor)
}
# POSIX syntax / rec.log / readline-style editing in the case: Git Bash pane only. Reason shown for the PowerShell pane.
GITBASH_ONLY = {
    "key_loss_control": "compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it with -NoProfile): runs in the Git Bash pane",
    "latency_key_log": "types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane",
    "latency_first_key_after_idle": "types into a running `cat > /dev/null` (key -> frame from the app log): no PowerShell equivalent: runs in the Git Bash pane",
    "expert_fuzz": "BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane",
    "big_paste": "`cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane",
    "read_x": "bash `read x` / printf: runs in the Git Bash pane",
    "alt_screen": "`seq | less` and vim: runs in the Git Bash pane",
    "intl_wide_program": "`head -n 1 > file` is the receiving program: runs in the Git Bash pane",
    "remote_session_legacy": "uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane",
    "remote_session_ext": "uses rec.log and prompt.log of the local shell: runs in the Git Bash pane",
    "remote_heredoc_paste": "POSIX here-doc / printf commands in the remote bash (#1129): runs in the Git Bash pane",
    "remote_tab_enter_no_helper": "rec.log of the remote bash for the completed `cd`, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash pane",
    "remote_tab_no_helper": "rec.log of the remote bash and `cd`/`pwd` there, the local shell's rec.log for the not-local check (#1801): runs in the Git Bash pane",
    "ssh_hanging_host_not_local": "Ctrl-C while ssh.cmd (a batch file) runs would ask 'Terminate batch job (Y/N)?': runs in the Git Bash pane",
    "latency_enter_ssh": "the plain-ssh control is the conhost reference running the pane's shell; a PowerShell reference's `ssh fake` path (ssh.cmd) differs from the pane's: runs in the Git Bash pane (#1532)",
    "ssh_password_prompt": "`ssh fakepass` against fakesshd_win.py's password user (#1535) plus rec.log/prompt.log of the local shell: runs in the Git Bash pane (the passphrase half needs the system sshd and is noted as not run)",
    "paste_bracketed_off": "`cat > file` + Ctrl-D and the bracketed-paste printf: runs in the Git Bash pane (#1538)",
    "paste_agent_input": "reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1538)",
    "alt_screen_programs": "vim/less/htop/tmux with bash effect lines and rec.log: runs in the Git Bash pane; a program that is not installed makes the case unverifiable (#1537)",
    "shell_zsh": "zsh as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "shell_fish": "fish as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "shell_nu": "nu as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "shell_pwsh": "pwsh as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "precmd_secret_honor_ps1": "a bash precmd function's `read -s`, with the app's prompt and with honor PS1 on (relaunches the app with terminal.input.honor_ps1; #1923): runs in the Git Bash pane",
    "precmd_question_echo": "a bash precmd function's echoing `read -p` announced by the bootstrap's reader marker (#2052): runs in the Git Bash pane",
    "gitbash_precmd_secret": "bash precmd_functions with `read -s` after the prompt's Precmd, rec.log to prove the PIN was not run as a command (#1883): runs in the Git Bash pane",
    "gitbash_typeahead_reps": "bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane",
    "ui_tab_menu": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "ui_ghost_accept": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "ui_ctrl_r": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "ui_palette": "asserts the exact line from rec.log: runs in the Git Bash pane",
    # round 14 wave 2 (cases_agent_w2.py; docs/design/r14-test-gaps.md 7.1 names the Git Bash pane for T11-T15)
    "agent_model_switch": "specified for the Git Bash pane (r14-test-gaps 7.1 T11). The case types no shell line, so nothing is missing in PowerShell: a second run there costs about 3 minutes and proves the same provider routing: runs in the Git Bash pane",
    "agent_anthropic_wire": "its approval step is agent_approval's (`echo tok >> file`, the Git Bash pane's rec.log / pty-write count): runs in the Git Bash pane",
    "agent_autorun_permissions": "the commands under test are POSIX (`ls`, `rm`, `;`, `$(...)`, a pipe to `sh`) and the allow list's matcher is tested with them: runs in the Git Bash pane",
    "agent_conversation_resume": "specified for the Git Bash pane (r14-test-gaps 7.1 T14). The relaunch waits for the shell's prompt.log, which a PowerShell pane does not write (-NoProfile): runs in the Git Bash pane",
    "agent_block_context": "`cat file` must be proven to have reached the shell from rec.log before its block is asked about; a PowerShell pane has no rc hook: runs in the Git Bash pane",
    "ui_completions_as_you_type_local": "asserts the menu on screen after typing in the Git Bash pane (the as-you-type default under test)",
    "ui_copy_wrapped_output": "#1976: `stty size` and `cat` of the fixture file in bash, the command checked against rec.log: runs in the Git Bash pane (keyboard and clipboard only)",
    "routing_table": "asserts what the shell received from rec.log: runs in the Git Bash pane",
    "routing_typo_head": "asserts what the shell received from rec.log (#1980, agent mode only): runs in the Git Bash pane",
    "routing_plain_installed_head": "asserts what the shell received from rec.log (#1981): runs in the Git Bash pane",
    "agent_stream_cancel": "uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model)",
    "agent_input_exact": "reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane",
    "agent_mode_switch": "asserts the exact shell line from rec.log after each view switch: runs in the Git Bash pane",
    "agent_latency_key": "its terminal-editor reference types into a POSIX shell line editor and the case needs the scripted fake model: runs in the Git Bash pane (#1828)",
    "agent_latency_enter": "its terminal-editor reference submits a POSIX `true # tok` line: runs in the Git Bash pane (#1828)",
    "agent_steer_fast": "uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (#1828)",
    "cb_agent_pickers": "reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane",
    "cb_agent_file_mention": "reads the agent input from the app log and uses rec.log to prove nothing reached the shell: runs in the Git Bash pane",
    "session_restore": "the app is stopped by pid and relaunched (#1540); bash `cd && echo` lines and `pwd -W` per tab, rec.log proves nothing re-ran: runs in the Git Bash pane",
    "subshell_ready_rc_questions": "bash --rcfile with read -p questions: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts)",
    "subshell_ready_rc_clear": "bash --rcfile with clear: the Git Bash pane (#1886: no tty probe on ConPTY, the judge uses screen facts)",
    "ssh_connect_typing_wrapper": "bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it",
    "ssh_nested_typeahead_wrapper": "bash pane with the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, #1884); skips itself without it",
    "gitbash_long_osc_reaches_app": "an msys program printing a long escape sequence to the pane's console (#2007): bash syntax and the msys console layer, the Git Bash pane",
    "ssh_gitbash_bootstrap_in_command": "the one-connection managed ssh sends the bootstrap in the remote command (#2051): only the Git Bash pane has the wrapper",
    "ssh_gitbash_managed": "the bash ssh wrapper's one-connection managed session (#1933): only the Git Bash pane has the wrapper",
    "ssh_gitbash_typeahead_managed": "keys typed during a one-connection managed ssh login stay managed (#1959): only the Git Bash pane has the wrapper",
    "ssh_gitbash_kill_switch": "the default and PHOSPHOR_MSYS_MANAGED_SSH=0 in the bash pane are plain ssh (#1933, #2051): only the Git Bash pane has the wrapper",
    "find_basic": "#2072: `cat` of fixture files and the exact line after Escape from rec.log: runs in the Git Bash pane, judged from the app log (hop=find_state, hop=find_scroll) without OCR",
    "agent_approval": "uses rec.log and effect files in bash syntax: runs in the Git Bash pane (needs the CI model)",
    "hostile_output_recovers": "#2072 (T8): `cat` of a binary file, a POSIX command list around hostile_out.py and rec.log for the exact line: runs in the Git Bash pane",
    "osc52_write_read": "r14 T18: the OSC 52 sequences are written with bash `printf` and every line is checked against rec.log; PowerShell has neither: runs in the Git Bash pane",
    "restore_shows_history": "r14 T20: the token is printed by bash `printf '%s%s\\n' A B` so the command text never holds it, and rec.log proves nothing re-ran: runs in the Git Bash pane",
}
# PowerShell-only: the same idea as a case that already exists for the other pane
PS_ONLY = {
    "ps_ctrl_break": "#1888: Ctrl+Break to a Python SIGBREAK receiver through the PowerShell pane's `&` call; the Git Bash pane's msys signals differ",
    "ps_ctrl_scrolllock_break": "#1906: Ctrl+ScrollLock (the other VK_CANCEL key) to the same SIGBREAK receiver; the Git Bash pane's msys signals differ",
    "ps_fast_command_young_enter": "#1889: PowerShell's Add-Content lines and its ConPTY young-Enter window; the Git Bash pane has the prompt-hold gates instead",
    "agent_interactive_password": "#1882: the hand-over on ConPTY is the PowerShell bootstrap's Read-Host marker; bash `read -s` in a Git Bash pane announces nothing (no termios), so the POSIX form of the case does not apply there",
    "routing_table_powershell": "#2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log)",
    "agent_basics_powershell": "#2072 (T6): the PowerShell pane's twin of agent_input_exact and agent_mode_switch (PSReadLine history instead of rec.log); the Git Bash pane runs those two",
    "ps_read_host_secure": "#1882/#1891: Read-Host -AsSecureString is PowerShell; the Git Bash twin is gitbash_precmd_secret",
}
# not on Windows at all, with the reason
NOT_WINDOWS = {
    "agent_interactive_vim": "#1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane",
    "agent_interactive_ssh": "#1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane",
    "agent_interactive_ssh_takeover": "#1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane",
    "agent_interactive_less": "#1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane",
    "agent_noninteractive_no_takeover": "#1829: POSIX read/vim/less/ssh programs under the agent hand-over (and the unix-only tty-read probe); not run in a Windows pane",
    "agent_edit_accept": "#1835: the file path typed to the scripted model and read back is POSIX-form; the fake-model pass is Git Bash only",
    "agent_edit_reject": "#1835: same as agent_edit_accept",
    "agent_edit_conflict": "#1835: same as agent_edit_accept",
    "agent_edit_endings": "#1835: same as agent_edit_accept",
    "agent_multitab_streaming": "#1833: needs the rc-hook received-command proof and the pty-echo frame pairing, neither verified for a Windows pane; Linux and macOS run it",
    "agent_mcp_faults": "#1834: the fake MCP servers are configured through the HOME-relative ~/.warp/.mcp.json, which the Windows profile does not use in this harness",
    "agent_edit_touch_only": "#1835: same as agent_edit_accept",
    "job_control": "#2072 (T7): Ctrl-Z / bg / fg are POSIX tty job control judged from `ps` process states; PowerShell has no job-control keys and an msys bash under ConPTY gets no SIGTSTP from the terminal",
    "subshells": "uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subshell and ctrl_c_d cover bash inside the pane)",
    "ctrl_c_flood_ab": "#1838: the blocked-frame gate it A/Bs exists only in the macOS renderer",
    "latency_paint_ab": "#1848: the key frame at the end of the key press it A/Bs exists only in the macOS renderer (and kl3 is macOS only)",
    "latency_echo_ab": "#2039: the echo frame drawn at once that it A/Bs exists only in the macOS renderer",
    "subshell_sh_probe": "dash / busybox ash / su do not exist in a Windows pane; the sh probe path is POSIX (#1710)",
    "subshell_ready_vim_typeahead": "bash --rcfile + vim on a real pty (#1679 step 3)",
    "subshell_ready_unsupported_tcsh": "tcsh does not exist in a Windows pane (#1679 step 3)",
    "subshell_ready_secret_after_prompt": "bash --rcfile + Phosphorize (ctrl+i) with a PROMPT_COMMAND read -s, on the subshell readiness judge, which has no OS facts on ConPTY (#1679 step 3, audit finding 5); the ConPTY prompt-hold half of the same secret is gitbash_precmd_secret (#1883)",
    "subshell_ready_relay_shape": "sudo -s with use_pty does not exist on Windows (#1679 step 3)",
    "subshell_ready_secret_after_probe": "as subshell_ready_secret_after_prompt: a PROMPT_COMMAND read -s behind the sh probe needs the tty facts ConPTY does not have (#1905)",
    "subshell_sh_probe_empty_ps1": "dash does not exist in a Windows pane; the sh probe path is POSIX (#1935)",
    "subshell_ready_exec_fish_rc": "fish exec'd from a bash rc on a real pty (#1679 step 3)",
    "subshell_ready_docker_t_without_i": "docker -t without -i against a Linux pty (#1679 step 3)",
    "subshell_ready_zsh_newuser_install": "zsh-newuser-install needs zsh and a POSIX HOME (#1679 step 3)",
    "intl_compose_linux": "Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash)",
    "intl_ime_japanese": "macOS input methods (TISSelectInputSource)",
    "intl_ime_pinyin": "macOS input methods (TISSelectInputSource)",
    "intl_deadkey_option": "macOS Option dead keys",
    "ui_mouse_select_copy": "no pointer synthesis in the Windows backend yet (xdotool / sway seat on Linux)",
    "ui_right_click_paste": "no pointer synthesis in the Windows backend yet",
    "ui_copy_wrapped_drag": "#1976: no pointer synthesis in the Windows backend yet (the keyboard half, ui_copy_wrapped_output, runs in the Git Bash pane)",
    "ui_readline_keys": "#1989: Alt-. / Alt-R are sent as X11 / Wayland key names; the Windows chord for them is not mapped yet (the binding itself is unit-tested per platform)",
    "ui_vertical_tabs_many": "#1990: not yet ported: switches the panel on by rewriting the settings file while the app runs (as ui_settings_live), and the Windows config path is not wired into cases_ui.settings_path",
    "ui_vertical_tabs_panel_click": "#1990: no pointer synthesis in the Windows backend yet, and the settings rewrite of ui_vertical_tabs_many",
    "ui_drag_drop_file": "no way to synthesize a drag source on Windows from the harness",
    "ui_resize_typing": "no window resize implementation in the Windows backend yet",
    "ui_split_panes": "needs OCR word boxes + pointer synthesis; neither exists in the Windows backend yet",
    "ui_startup_time": "not yet ported: reads the Linux log format / paths",
    "ui_settings_live": "not yet ported: rewrites the Linux settings path while the app runs",
    "upgrade_from_v0_1_17": "#1546: Linux first: the seed paths (~/.config/phosphor), the secret-tool keyring helper and the AppImage fetch are the Linux ones; the table and fixture are platform-neutral (docs/testing.md 2.18)",
    "keyring_unavailable_startup": "#1805: the no-keyring variants are D-Bus session configs of the Linux launcher (Windows has no Secret Service; DPAPI files)",
    "quit_24_tabs": "not yet ported: Linux quit key and log check",
    "flood_typed_line": "not yet ported: POSIX `seq`/`yes` floods and /proc sampling",
    "flood_typing_latency": "not yet ported: POSIX flood and pty arrival reader",
    "flood_scrollback_find": "needs OCR (tesseract) and the Linux find flow",
    "soak": "not yet ported: samples /proc",
    "alt_screen_mouse_reporting": "no pointer synthesis in the Windows backend yet, and the receiving program needs python3 termios (#1537)",
    "scroll_keys_position": "#2072: the scroll position is read from the screen by OCR and the Windows runner has no OCR engine; the app logs no scroll-position hop yet (T34's grid hop would serve)",
    "sudo_su_real_shell": "sudo/su/ksu do not exist in a Windows pane (#1536)",
    "ssh_drop_reconnect": "not yet ported: the `~.` escape and `kill -9 $$` need verifying against the msys/Win32 ssh clients (#1535)",
    "settings_ui_toggle_persists": "r14 T17: the Windows backend has no verified Ctrl-, chord, and the case hand-edits the settings file while the app runs (the Windows config path is not wired into cases_ui.settings_path)",
    "cb_settings_ui_open": "the Windows backend has no verified Ctrl-, chord and the page is not OCR-able there",
    "cb_settings_migration": "needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle); the Windows relaunch is the separate runner flow",
    "cb_report_problem": "the Windows Downloads folder is a known folder, not under the scratch HOME, so the bundle does not land where the case looks (set PHOSPHOR_DIAG_BUNDLE_DIR in the app's environment to enable)",
    "cb_state_relaunch": "needs the App stop/start handle on a profile with state; Windows has a separate flow",
    "cb_ui_action_latency": "needs Pillow screenshot diffs of new-tab/switch chords not mapped in the Windows backend",
    "cb_editor_mac": "macOS-only case (Linux and Windows have editor_edit_save / editor_paste)",
    "zero_network_startup": "Wave N: traces the app with Linux strace; a Windows/macOS network trace is not built yet",
    "network_gate_notice": "needs the Linux pointer + OCR path and a relaunch on a rewritten settings file",
    "agent_gate_deny_allow": "Linux-only: pointer + OCR for the toast and live edits of the Linux profile's settings.toml",
    "agent_gate_off_live": "Linux-only: live edits of the Linux profile's settings.toml",
    "agent_gate_key_never_leaves": "Linux-only: live edits of the Linux profile's settings.toml and the fake model's request headers",
    "network_gate_switch": "needs a relaunch of the app on a rewritten settings file (the Linux/macOS App handle) and the Linux profile layout",
    "claude_plugin_offline": "Wave N: needs `unshare -rn` (Linux network namespaces) and the Linux pty; the plugin's hook scripts are bash+jq",
    "agent_plugins_offline": "needs `unshare -rn` (Linux network namespaces) and the Linux pty; the Codex and Gemini hook scripts are bash+jq and write to /dev/tty",
}


# #1534: the startup_shell_override passes (APPCI_WIN_SHELLS="pwsh7,cmd", never in the default list: cmd.exe and pwsh 7 are not installed on every VM).
# The pane's shell is chosen through the app's `session.startup_shell_override` (a custom shell path, profile.Profile.gitbash_toml), not by the harness. Only the
# cases below run in a variant pass, each with the reason it is safe in that shell; a pass lists NO skip rows for the other cases (they are the PowerShell/Git Bash
# panes' job). pwsh 7 is the PowerShell family (Add-Content, PSReadLine, `;`), cmd.exe has its own effect-file syntax (Ctx.append_cmd 'cmd').
VARIANT_FAMILY = {"pwsh7": "powershell", "cmd": "cmd"}
VARIANT_CASES = {
    "pwsh7": ("mode_proof", "startup_shell_pane", "fast_typing", "paste_instant_enter", "tab_enter", "ctrl_c_d"),
    "cmd": ("mode_proof", "startup_shell_pane"),
}


def variant_cases(variant):
    """case names that run in the startup_shell_override pass `variant`, in SMOKE-independent registry order (run.py sorts by case order)"""
    return VARIANT_CASES[variant]


def lab_skip_reason(name, pane):
    """the fake-ssh-lab cases (cases_lab.py, lab_<case>.<shell|scenario>.<lat>.<helper>[.<fault>]): they read the local shell's rec.log / prompt.log and type bash-style
    lines into the pane, so they run in the Git Bash pane only; Win32-OpenSSH has no ControlMaster, so the one-session scenario user cannot be exercised on Windows."""
    if pane == "powershell":
        return "not run in the PowerShell pane: the lab cases use the local shell's rec.log / prompt.log and bash syntax: they run in the Git Bash pane"
    if ".p_onesess." in name:
        return "not run on Windows: p_onesess (MaxSessions 1) needs ssh ControlMaster, which Win32-OpenSSH does not have"
    return None


def skip_reason(name, pane):
    """None when the case runs in this pane shell, else the reason it is skipped there"""
    if name.startswith("lab_"):
        return lab_skip_reason(name, pane)
    if name in NOT_WINDOWS:
        return "not run on Windows: " + NOT_WINDOWS[name]
    if name in GITBASH_ONLY and pane == "powershell":
        return "not run in the PowerShell pane: " + GITBASH_ONLY[name]
    if name in PS_ONLY and pane == "gitbash":
        return "not run in the Git Bash pane: " + PS_ONLY[name]
    if name in BOTH or name in GITBASH_ONLY or name in PS_ONLY:
        return None
    return "not yet classified for Windows (winmap.py): add it to BOTH, GITBASH_ONLY or NOT_WINDOWS"


def unclassified(names):
    return [n for n in names if not n.startswith("lab_") and n not in BOTH and n not in GITBASH_ONLY and n not in PS_ONLY and n not in NOT_WINDOWS]


# ---------------------------------------------------------------- the Windows fix tier (APPCI_TIER=smoke), as data
# r18co: the unified smoke list took 70 min on Windows (2 modes x 2 panes). The mode-independent reference measurements (conhost enter control, Ctrl-C
# reference + remeasure) say nothing new in agent mode, so only TERMINAL mode takes them; agent mode keeps the input path (typing, paste, Tab+Enter, a short
# Ctrl-C) and the ssh hand-over cases (#1349), once per pane. The full tier (APPCI_TIER empty / full) is NOT touched by this table.
# cases: None = the shared SMOKE list; else the subset (a case outside SMOKE is never added). Options, read by the cases through ctx.opts:
#   ref        run the reference terminal measurements (enter control for fast_typing attribution, Ctrl-C reference + remeasure)
#   ft_gaps    fast_typing burst gaps in seconds
#   cc_short   ctrl_c_d without reference or remeasure
FIX_TIER = {
    "windows": {
        "terminal": dict(cases=None, ref=True, ft_gaps=(0.0, 0.25), cc_short=False),
        "agent": dict(cases=("mode_proof", "fast_typing", "paste_instant_enter", "tab_enter", "ctrl_c_d",
                             "ssh_connect_typing", "ssh_enter_then_immediate_command", "ssh_nested_typeahead", "routing_table", "agent_stream_cancel", "agent_input_exact", "agent_mode_switch"),
                      ref=False, ft_gaps=(0.0,), cc_short=True),
    },
}
# a case that exists only in one mode/pane of the fix tier (key_loss_control: Git Bash terminal only; it is also GITBASH_ONLY above)
FIX_ONLY_IN = {"key_loss_control": (("terminal", "gitbash"),), "enter_loss_control": (("terminal", "powershell"), ("terminal", "gitbash"))}
FULL_OPTS = dict(cases=None, ref=True, ft_gaps=(0.0, 0.25), cc_short=False)


def tier_plan(platform, tier, mode, pane, smoke, proving=()):
    """-> (case names or None for 'all of the full tier', options dict) for one pass. smoke = the shared SMOKE tuple; proving = cases.PROVING (owner rule
    2026-11-09: a new case runs in the fix tier until it has passed). A PROVING case is added to every pass's list (agent mode included: the `cases`
    subset below is the SMOKE trim, not a PROVING filter); the ones Windows cannot run SKIP with their reason (skip_reason), they are not dropped."""
    if tier != "smoke":
        return None, dict(FULL_OPTS)
    o = dict(FIX_TIER[platform][mode])
    names = [n for n in smoke if o["cases"] is None or n in o["cases"]] + [n for n in proving if n not in smoke]
    names = [n for n in names if n not in FIX_ONLY_IN or (mode, pane) in FIX_ONLY_IN[n]]
    return names, o


# #1532: the fix tier runs the app in its DEFAULT destination mode (table). Run again with PHOSPHOR_QUEUE_DESTINATION=legacy (the shipped kill
# switch) and the tier narrows to this list: the cases whose routing the table decides (paste, Tab+Enter, Ctrl-C, both ssh hand-overs, the
# routing table), so the legacy chain is exercised in the real app for roughly a third of the smoke time. Each name must be in cases.SMOKE
# (selftest). Shadow (PHOSPHOR_QUEUE_DESTINATION=shadow) runs the whole smoke list and the run fails on any untagged kh_dest_disagree line.
KILL_SWITCH_SMOKE = ("mode_proof", "paste_instant_enter", "tab_enter", "ctrl_c_d", "ssh_connect_typing", "ssh_enter_then_immediate_command", "routing_table")


def smoke_for(dest, smoke):
    """The smoke case names for destination mode `dest` ('table' | 'shadow' | 'legacy')."""
    return tuple(n for n in smoke if n in KILL_SWITCH_SMOKE) if dest == "legacy" else tuple(smoke)
