# Each line starts with a leading space to leave these commands out of history.
#
# command -p resolves the given command with the system default PATH, ensuring the shell
# can find them even if the user has a clobbered PATH value.
 unsetopt ZLE
 WARP_SESSION_ID=@@WARP_SESSION_ID@@
 _hostname=$(command -pv hostname >/dev/null 2>&1 && command -p hostname 2>/dev/null || command -p uname -n)
 _user=$(command -pv whoami >/dev/null 2>&1 && command -p whoami 2>/dev/null || builtin echo $USER)
 # JSON-escaped copies for the hook payload; a value like CORP\tdoe must not decode as a tab (#879).
 _ju=${_user//\\/\\\\}; _ju=${_ju//\"/\\\"}; _jh=${_hostname//\\/\\\\}; _jh=${_jh//\"/\\\"}; for _i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 127; do _c=${(#)_i}; _x=$(( [##16] _i )); _h="\\u00${(l:2::0:)_x}"; _ju=${_ju//$_c/$_h}; _jh=${_jh//$_c/$_h}; done
 _msg=$(builtin printf "{\"hook\": \"InitShell\", \"value\": {\"session_id\": $WARP_SESSION_ID, \"shell\": \"zsh\", \"user\": \"%s\", \"hostname\": \"%s\"}}" "$_ju" "$_jh" | command -p od -An -v -tx1 | command -p tr -d " \n")
 WARP_USING_WINDOWS_CON_PTY=@@USING_CON_PTY_BOOLEAN@@
 # We send the InitShell hook via OSCs when on Windows and via DCSs otherwise.
 if [ "$WARP_USING_WINDOWS_CON_PTY" = true ]; then builtin printf '\e]9278;d;%s\x07' "$_msg"; else builtin printf '\e\x50\x24\x64%s\x9c' "$_msg"; fi
 unset _hostname _user _msg _ju _jh _i _c _h _x
