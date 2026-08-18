  # Every identifier here is abbreviated to the point of ugliness on purpose: this script has to
  # fit in 1020 bytes once `convert_script_to_one_line` has run over it (macOS 15+ pty limit,
  # asserted by `test_mac_warpification_script_size`), and `_l` now also has to carry a 20-digit
  # session ID. Comments are free -- the one-liner strips them.
function _i
    command -v $argv[1] >/dev/null 2>&1
end

  # @@WARP_SESSION_ID@@ is substituted by `warpify_ssh_session_command` with an ID this client
  # minted and registered *before* the script reaches the pty, so the hooks below can be
  # validated against it (#532). Do not let the remote mint its own.
function _l
    set _m (printf "{\"hook\": \"%s\", \"value\": %s, \"session_id\": @@WARP_SESSION_ID@@}" $argv[1] $argv[2] | od -An -v -tx1 | tr -d " \n")
    printf '\033\120\044\144%s\234' $_m
end

function _e
    _l RemoteWarpificationIsUnavailable $argv[1]
end

function _sd
    set -l P ""

    if _i brew
      set P "homebrew"
    end

    printf '{"os": "Darwin", "pkg": "%s", "shell": "fish", "root_access": "no_root_access", "writable_home": %s}' "$P" $( [ -w ~ ] && echo true || echo false )
end

  # _ct is the macOS counterpart of the Linux script's _check_tmux. The Linux install scripts call
  # _check_tmux back after installing; the only macOS install script is
  # `install_tmux_and_warpify_brew.sh`, which does not, so the short name is safe here.
function _ct
    set -g _T "$HOME/.warp/tmux/execute_tmux.sh"
    if _i "$_T"
        _l SshTmuxInstaller "\"warp\""
    else if _i tmux
        set _T "tmux"
        _l SshTmuxInstaller "\"user\""
    end

    if test -n "$_T"
        $_T -V | awk '{print $2}' | read V;
        if test -z "$V"
            _e "\"TmuxFailed\""
        else if test (printf '%s\n' "$V" "2.9" | sort -V | tail -n1) = "2.9"
            _e "{\"UnsupportedTmuxVersion\": $(_sd)}"
        else;
          return 0
        end
    else;
        _e "{\"TmuxNotInstalled\": $(_sd)}"
    end
    return 1
end

_ct; and $_T -Lwarp -CC; and exit
