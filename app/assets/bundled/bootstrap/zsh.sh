 setopt interactivecomments
 # Set PS2 to an empty string to prevent zsh from printing a secondary prompt
 # (e.g.: 'heredoc> '), which would otherwise be printed repeatedly when we
 # paste the bootstrap script into the PTY.
 if (( ${+PS2} )); then
   ORIGINAL_PS2="$PS2"
 fi
 PS2=""
 # `setopt PRINT_EXIT_VALUE` (#974) reports every failing top-level command, including the `read`
 # below (it returns 1 at the end of the heredoc even with `|| :`), as `zsh: exit 1`. Turn it off
 # before anything of ours can fail and hand the user's setting to the tail of this script. `[[ ]]`
 # is not reported when it is false; the zsh_body.sh guard skips this when it is already off.
 [[ -o print_exit_value ]] && { WARP_RESTORE_PRINT_EXIT_VALUE=1; unsetopt print_exit_value; }
 # Similar to our approach in bash, we start a shell with the minimal amount of
 # startup (i.e. --no-rcs) and then take over by executing the shell startup.
 # We only support the local zsh case for now.
 #
 # Note that we indent everything in this top-level script by one space in order
 # to hide from history.
 #
 # Also, note that we put the 'eval' on the same line as the 'read' separated by a semi-colon
 # rather than on its own line after the HEREDOC.  This seems to work around a bug in zsh
 # where the buffer was getting messed up after processing the heredoc about 1/50 of the time.
 # `|| :` after both: `read -d ''` returns 1 at the end of the heredoc and the eval returns the
 # body's last status; `setopt PRINT_EXIT_VALUE` reports either as `zsh: exit 1` from our own
 # bootstrap (#974).
 read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :; eval "$WARP_BOOTSTRAP_VAR" || :; unset WARP_BOOTSTRAP_VAR; if [[ -n ${WARP_RESTORE_PIPE_FAIL-} ]]; then setopt pipe_fail; fi; if [[ -n ${WARP_RESTORE_NOUNSET-} ]]; then unsetopt unset; fi; [[ -z ${WARP_RESTORE_PRINT_EXIT_VALUE-} ]] || setopt print_exit_value; unset WARP_RESTORE_PRINT_EXIT_VALUE WARP_RESTORE_PIPE_FAIL WARP_RESTORE_NOUNSET
#include bundled/bootstrap/zsh_body.sh
EOM
