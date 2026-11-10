# command -p resolves the given command with the system default PATH, ensuring the shell
# can find them even if the user has a clobbered PATH value.
command -p stty raw
unset PROMPT_COMMAND
HISTCONTROL=ignorespace
HISTIGNORE=" *"
WARP_IS_SUBSHELL=1
WARP_SESSION_ID=@@WARP_SESSION_ID@@
_hostname=$(command -pv hostname >/dev/null 2>&1 && command -p hostname 2>/dev/null || command -p uname -n)
_user=$(command -pv whoami >/dev/null 2>&1 && command -p whoami 2>/dev/null || builtin echo ${USER:-})
# JSON-escaped copies for the hook payload; a value like CORP\tdoe must not decode as a tab (#879).
# wsl_name (as zsh_init_subshell.sh reports it) is empty outside WSL; only the characters a distribution name may hold are kept,
# which keeps the payload valid JSON without a second escaping pass (the typed setup stays two chunk lines).
_ju=${_user//\\/\\\\}; _ju=${_ju//\"/\\\"}; _jh=${_hostname//\\/\\\\}; _jh=${_jh//\"/\\\"}; _jw=${WSL_DISTRO_NAME:-}; _jw=${_jw//[^A-Za-z0-9._-]/}; for _i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 127; do builtin printf -v _o %03o $_i; builtin printf -v _c "\\$_o"; builtin printf -v _h "\\\\u%04x" $_i; _ju=${_ju//$_c/$_h}; _jh=${_jh//$_c/$_h}; done
_msg=$(builtin printf "{\"hook\": \"InitShell\", \"value\": {\"session_id\": $WARP_SESSION_ID, \"shell\": \"bash\", \"user\": \"%s\", \"hostname\": \"%s\", \"is_subshell\": true, \"wsl_name\": \"%s\", \"pid\": $$}}" "$_ju" "$_jh" "$_jw" | command -p od -An -v -tx1 | command -p tr -d " \n")
if [[ "${OS:-}" == Windows_NT ]]; then WARP_IN_MSYS2=true; else WARP_IN_MSYS2=false; fi
WARP_USING_WINDOWS_CON_PTY=@@USING_CON_PTY_BOOLEAN@@
if [ "$WARP_USING_WINDOWS_CON_PTY" = true ]; then builtin printf '\e]9278;d;%s\x07' "$_msg"; else builtin printf '\e\x50\x24\x64%s\x9c' "$_msg"; fi
unset _hostname _user _msg _ju _jh _jw _i _o _c _h
