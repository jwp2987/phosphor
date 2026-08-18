  # Every identifier here is abbreviated to the point of ugliness on purpose: this script has to
  # fit in 1020 bytes once `convert_script_to_one_line` has run over it (macOS 15+ pty limit,
  # asserted by `test_mac_warpification_script_size`), and `_log` now also has to carry a
  # 20-digit session ID. Comments are free -- the one-liner strips them.
_f() {
    command -v "$1" >/dev/null 2>&1
}

  # @@WARP_SESSION_ID@@ is substituted by `warpify_ssh_session_command` with an ID this client
  # minted and registered *before* the script reaches the pty, so the hooks below can be
  # validated against it (#532). Do not let the remote mint its own.
_log() {
    _m=$(printf "{\"hook\": \"$1\", \"value\": $2, \"session_id\": @@WARP_SESSION_ID@@}" | command -p od -An -v -tx1 | command -p tr -d " \n")
    printf '\033\120\044\144%s\234' "$_m"
}

_err() {
    _log RemoteWarpificationIsUnavailable "$1"
}

_sd() {
    if _f brew; then
        P="homebrew"
    fi

    WH=$( [ -w ~ ] && echo true || echo false )

    printf '{"os": "Darwin", "pkg": "%s", "shell": "%s", "root_access": "no_root_access", "writable_home": %s}' "$P" "$(basename $SHELL)" $WH
}

  # _ct is the macOS counterpart of the Linux script's _check_tmux. The Linux install scripts call
  # _check_tmux back after installing; the only macOS install script is
  # `install_tmux_and_warpify_brew.sh`, which does not, so the short name is safe here.
_ct() {
    _T="$HOME/.warp/tmux/execute_tmux.sh"
    if _f "$_T"; then
        _log SshTmuxInstaller "\"warp\""
    elif _f tmux; then
        _T="tmux"
        _log SshTmuxInstaller "\"user\""
    fi

    if [ $_T ]; then
        V=$($_T -V 2>/dev/null | awk '{print $2}')
        if [ -z "$V" ]; then
            _err "\"TmuxFailed\""
        elif [ "$(printf '%s\n' "$V" "2.9" | sort -V | tail -n1)" = "2.9" ]; then
            _err "{\"UnsupportedTmuxVersion\": $(_sd)}"
        else
            return 0
        fi
    else
        _err "{\"TmuxNotInstalled\": $(_sd)}"
    fi
    return 1
}

_ct && $_T -Lwarp -CC && exit
