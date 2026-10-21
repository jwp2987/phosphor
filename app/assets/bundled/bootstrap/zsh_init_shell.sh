# Each line starts with a leading space to leave these commands out of history.
#
# command -p resolves the given command with the system default PATH, ensuring the shell
# can find them even if the user has a clobbered PATH value.
 unsetopt ZLE
 WARP_SESSION_ID=@@WARP_SESSION_ID@@
 _hostname=$(command -pv hostname >/dev/null 2>&1 && command -p hostname 2>/dev/null || command -p uname -n)
 _user=$(command -pv whoami >/dev/null 2>&1 && command -p whoami 2>/dev/null || builtin echo $USER)
 # JSON-escaped copies for the hook payload (#879): escape \ and ", drop control chars. Keep this file short: it is typed as ONE line and macOS caps a canonical line at 1024 bytes.
 _ju=${${_user//\\/\\\\}//\"/\\\"}; _ju=${_ju//[[:cntrl:]]/}; _jh=${${_hostname//\\/\\\\}//\"/\\\"}; _jh=${_jh//[[:cntrl:]]/}
 _msg=$(builtin printf "{\"hook\": \"InitShell\", \"value\": {\"session_id\": $WARP_SESSION_ID, \"shell\": \"zsh\", \"user\": \"%s\", \"hostname\": \"%s\"}}" "$_ju" "$_jh" | command -p od -An -v -tx1 | command -p tr -d " \n")
 WARP_USING_WINDOWS_CON_PTY=@@USING_CON_PTY_BOOLEAN@@
 # We send the InitShell hook via OSCs when on Windows and via DCSs otherwise.
 if [ "$WARP_USING_WINDOWS_CON_PTY" = true ]; then builtin printf '\e]9278;d;%s\x07' "$_msg"; else builtin printf '\e\x50\x24\x64%s\x9c' "$_msg"; fi
 unset _hostname _user _msg _ju _jh
