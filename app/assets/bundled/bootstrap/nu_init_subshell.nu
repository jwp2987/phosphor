# The nushell subshell stage-one script: typed into a running `nu`, written to a file and
# sourced (see `nu_subshell_lines` in terminal/bootstrap.rs). It only announces the subshell;
# the full bootstrap (nu.nu) is sourced afterwards, as for a local session.
#
# RULES (the loader joins the lines with ";", drops blank and comment lines):
#   * one complete statement per line, braces opened and closed on the same line;
#   * no line may begin with '#'.
$env.WARP_SESSION_ID = "@@WARP_SESSION_ID@@"
$env.WARP_USING_WINDOWS_CON_PTY = "@@USING_CON_PTY_BOOLEAN@@"
let _user = (do -i { ^whoami } | default ($env.USER? | default "") | into string | str trim | str replace -a -r '[\x00-\x1f\x7f]' '' | to json -r)
let _host = (do -i { sys host | get hostname } | default ($env.HOSTNAME? | default "") | into string | str trim | str replace -a -r '[\x00-\x1f\x7f]' '' | to json -r)
let _hex = ($'{"hook": "InitShell", "value": {"session_id": ($env.WARP_SESSION_ID), "user": ($_user), "hostname": ($_host), "shell": "nu", "is_subshell": true, "wsl_name": ""}}' | encode hex --lower)
if $env.WARP_USING_WINDOWS_CON_PTY == "true" { print -n $"\e]9278;d;($_hex)\u{7}" } else { print -n $"\eP$d($_hex)"; print -n -r 0x[9c] }
