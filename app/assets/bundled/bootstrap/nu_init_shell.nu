# Evaluated by `nu --execute '<this file, one statement per line, joined with ;>'` when a local
# nushell session starts (see `arguments_for_session_spawning_command` in local_tty/shell.rs).
# `--execute` runs after the user's env.nu / config.nu / login.nu, and before the first prompt.
#
# RULES (the loader joins the lines with ";", drops blank and comment lines):
#   * one complete statement per line, braces opened and closed on the same line;
#   * no line may begin with '#'.
$env.WARP_SESSION_ID = "@@WARP_SESSION_ID@@"
$env.WARP_USING_WINDOWS_CON_PTY = "@@USING_CON_PTY_BOOLEAN@@"
$env.config.show_banner = false
# Lines that start with a space are kept out of history by this option (nushell's default). The
# `source` line that loads the bootstrap file is typed with a leading space. nu.nu puts the
# user's own value back (saved in $env.__wbh) as its last statement.
$env.__wbh = $env.config.history.ignore_space_prefixed
$env.config.history.ignore_space_prefixed = true
let _user = (do -i { ^whoami } | default ($env.USER? | default "") | into string | str trim | str replace -a -r '[\x00-\x1f\x7f]' '' | to json -r)
let _host = (do -i { sys host | get hostname } | default ($env.HOSTNAME? | default "") | into string | str trim | str replace -a -r '[\x00-\x1f\x7f]' '' | to json -r)
let _hex = ($'{"hook": "InitShell", "value": {"session_id": ($env.WARP_SESSION_ID), "shell": "nu", "user": ($_user), "hostname": ($_host)}}' | encode hex --lower)
if $env.WARP_USING_WINDOWS_CON_PTY == "true" { print -n $"\e]9278;d;($_hex)\u{7}" } else { print -n $"\eP$d($_hex)"; print -n -r 0x[9c] }
