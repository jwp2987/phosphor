set -g WARP_SESSION_ID @@WARP_SESSION_ID@@
set _hostname (command -v hostname >/dev/null 2>&1 && command hostname 2>/dev/null || uname -n)
set _user (command -v whoami >/dev/null 2>&1 && command whoami 2>/dev/null || builtin echo $USER)
# JSON-escaped copies for the hook payload (#879).
set _ju (string join '\n' -- (string replace -a '\\' '\\\\' -- $_user | string replace -a '"' '\\"'))
set _jh (string join '\n' -- (string replace -a '\\' '\\\\' -- $_hostname | string replace -a '"' '\\"'))
for _i in 1 2 3 4 5 6 7 8 9 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 127; set _c (builtin printf (builtin printf '\\\\x%02x' $_i)); set _h (builtin printf '\\\\u%04x' $_i); set _ju (string replace -a -- $_c $_h "$_ju"); set _jh (string replace -a -- $_c $_h "$_jh"); end
set -g WARP_IN_MSYS2 (test "$OS" = Windows_NT; and builtin echo true; or builtin echo false)
if test "$WARP_IN_MSYS2" = true; set _msg "\e]9278;k;A;InitShell\a\e]9278;k;B;session_id;$WARP_SESSION_ID\a\e]9278;k;B;shell;fish\a\e]9278;k;B;user;$_user\a\e]9278;k;B;hostname;$_hostname\a\e]9278;k;B;is_subshell;true\a\e]9278;k;C\a"; else; set _msg (builtin printf "{\"hook\": \"InitShell\", \"value\": {\"session_id\": $WARP_SESSION_ID, \"user\": \"%s\", \"hostname\": \"%s\", \"shell\": \"fish\", \"is_subshell\": true, \"wsl_name\": \"$WSL_DISTRO_NAME\"}}" "$_ju" "$_jh" | command od -An -v -tx1 | command tr -d " \n"); end
set WARP_USING_WINDOWS_CON_PTY @@USING_CON_PTY_BOOLEAN@@
if test "$WARP_USING_WINDOWS_CON_PTY" = true; if test "$WARP_IN_MSYS2" = true; builtin printf "$_msg"; else; builtin printf '\e]9278;d;%s\x07' "$_msg"; end; else; builtin printf '\e\x50\x24\x64%s\x9c' "$_msg"; end
set -e _hostname _user _msg _ju _jh _i _c _h
