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
    "mode_proof", "cadence", "fast_typing", "rollover", "paste_instant_enter", "arrows_behind_paste", "tab_enter", "ctrl_c_d",
    "ssh_connect_typing", "ssh_enter_then_immediate_command", "ssh_nested_typeahead", "ssh_failing_not_local",
    "idle_cpu", "enter_loss_control", "latency_enter", "latency_paint", "tabs_two_windows", "tabs_streaming",
    "intl_wide_type", "intl_wide_paste", "intl_wide_edit", "intl_altgr_bash", "intl_altgr_ps",
    "approval_card_pause", "md_viewer", "md_viewer_big", "md_viewer_ai_answer", "editor_memory_7mb", "gitbash_subshell",
}
# POSIX syntax / rec.log / readline-style editing in the case: Git Bash pane only. Reason shown for the PowerShell pane.
GITBASH_ONLY = {
    "key_loss_control": "compares the typed line with the shell's own rec.log; a PowerShell pane has no rc hook (the app starts it with -NoProfile): runs in the Git Bash pane",
    "latency_key_log": "types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash pane",
    "expert_fuzz": "BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane",
    "big_paste": "`cat > file` + Ctrl-D end-of-input: runs in the Git Bash pane",
    "read_x": "bash `read x` / printf: runs in the Git Bash pane",
    "alt_screen": "`seq | less` and vim: runs in the Git Bash pane",
    "intl_wide_program": "`head -n 1 > file` is the receiving program: runs in the Git Bash pane",
    "remote_session_legacy": "uses rec.log and prompt.log of the local shell and `ssh.cmd` Ctrl-C paths: runs in the Git Bash pane",
    "remote_session_ext": "uses rec.log and prompt.log of the local shell: runs in the Git Bash pane",
    "ssh_hanging_host_not_local": "Ctrl-C while ssh.cmd (a batch file) runs would ask 'Terminate batch job (Y/N)?': runs in the Git Bash pane",
    "ssh_password_prompt": "needs the system sshd's passphrase-key prompt (fakesshd cannot prompt); skips itself with that reason",
    "shell_zsh": "zsh as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "shell_fish": "fish as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "shell_nu": "nu as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "shell_pwsh": "pwsh as a subshell of bash: Git Bash pane only (skips itself when not installed)",
    "ui_tab_menu": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "ui_ghost_accept": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "ui_ctrl_r": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "ui_palette": "asserts the exact line from rec.log: runs in the Git Bash pane",
    "routing_table": "asserts what the shell received from rec.log: runs in the Git Bash pane",
    "agent_stream_cancel": "uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model)",
    "agent_approval": "uses rec.log and effect files in bash syntax: runs in the Git Bash pane (needs the CI model)",
}
# PowerShell-only: the same idea as a case that already exists for the other pane
PS_ONLY = {}
# not on Windows at all, with the reason
NOT_WINDOWS = {
    "subshells": "uses `sh -c 'ps -o comm= -p $PPID'` to prove the subshell's parent; msys ps output differs. Not yet ported (gitbash_subshell and ctrl_c_d cover bash inside the pane)",
    "intl_compose_linux": "Linux xkb compose / dead keys (Windows has intl_altgr_ps / intl_altgr_bash)",
    "intl_ime_japanese": "macOS input methods (TISSelectInputSource)",
    "intl_ime_pinyin": "macOS input methods (TISSelectInputSource)",
    "intl_deadkey_option": "macOS Option dead keys",
    "ui_mouse_select_copy": "no pointer synthesis in the Windows backend yet (xdotool / sway seat on Linux)",
    "ui_right_click_paste": "no pointer synthesis in the Windows backend yet",
    "ui_drag_drop_file": "no way to synthesize a drag source on Windows from the harness",
    "ui_resize_typing": "no window resize implementation in the Windows backend yet",
    "ui_split_panes": "needs OCR word boxes + pointer synthesis; neither exists in the Windows backend yet",
    "ui_startup_time": "not yet ported: reads the Linux log format / paths",
    "ui_settings_live": "not yet ported: rewrites the Linux settings path while the app runs",
    "session_restore": "not yet ported: needs the Linux quit key + /proc to prove nothing re-runs",
    "quit_24_tabs": "not yet ported: Linux quit key and log check",
    "flood_typed_line": "not yet ported: POSIX `seq`/`yes` floods and /proc sampling",
    "flood_typing_latency": "not yet ported: POSIX flood and pty arrival reader",
    "flood_scrollback_find": "needs OCR (tesseract) and the Linux find flow",
    "editor_edit_save": "not yet ported: Linux D-Bus URL forward opens the file",
    "editor_paste": "not yet ported: Linux D-Bus URL forward opens the file",
    "soak": "not yet ported: samples /proc",
}


def skip_reason(name, pane):
    """None when the case runs in this pane shell, else the reason it is skipped there"""
    if name in NOT_WINDOWS:
        return "not run on Windows: " + NOT_WINDOWS[name]
    if name in GITBASH_ONLY and pane == "powershell":
        return "not run in the PowerShell pane: " + GITBASH_ONLY[name]
    if name in PS_ONLY and pane == "gitbash":
        return "not run in the Git Bash pane: " + PS_ONLY[name]
    if name in BOTH or name in GITBASH_ONLY:
        return None
    return "not yet classified for Windows (winmap.py): add it to BOTH, GITBASH_ONLY or NOT_WINDOWS"


def unclassified(names):
    return [n for n in names if n not in BOTH and n not in GITBASH_ONLY and n not in PS_ONLY and n not in NOT_WINDOWS]


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
                             "ssh_connect_typing", "ssh_enter_then_immediate_command", "ssh_nested_typeahead"),
                      ref=False, ft_gaps=(0.0,), cc_short=True),
    },
}
# a case that exists only in one mode/pane of the fix tier (key_loss_control: Git Bash terminal only; it is also GITBASH_ONLY above)
FIX_ONLY_IN = {"key_loss_control": (("terminal", "gitbash"),), "enter_loss_control": (("terminal", "powershell"), ("terminal", "gitbash"))}
FULL_OPTS = dict(cases=None, ref=True, ft_gaps=(0.0, 0.25), cc_short=False)


def tier_plan(platform, tier, mode, pane, smoke):
    """-> (case names or None for 'all of the full tier', options dict) for one pass. smoke = the shared SMOKE tuple."""
    if tier != "smoke":
        return None, dict(FULL_OPTS)
    o = dict(FIX_TIER[platform][mode])
    names = [n for n in smoke if o["cases"] is None or n in o["cases"]]
    names = [n for n in names if n not in FIX_ONLY_IN or (mode, pane) in FIX_ONLY_IN[n]]
    return names, o
