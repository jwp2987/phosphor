 # We need to prepend a space to all the top-level commands here to prevent
 # Warp bootstrap script contents from showing up in the history.

 # Keep track of whether bash-preexec was included in /etc/bash.bashrc.
 if [[ ${bash_preexec_imported:-} == "defined" || ${__bp_imported:-} == "defined" ]]; then
     BASH_PREEXEC_IN_ETC_BASHRC=1
 fi
 # Ensure that nothing already loaded by the shell (e.g.: bash-preexec sourced
 # in /etc/bash.bashrc) unset the HISTCONTROL variable.
 HISTCONTROL=ignorespace
 # Unset HISTIGNORE, which we set via the initial bash command in shell.rs, as
 # we only need it long enough to ensure HISTCONTROL is set to ignorespace.
 unset HISTIGNORE

 # This function defines all the bash-preexec functions and sets up a bunch of the environment variables.
 # Another function, install_bashpreexec will actually install the plugin.
 # Its contents are pretty much pasted from bash-preexec.
 # The three differences are :
 # 1. We remove the guard for duplicate inclusion since we have different logic in warp where we execute this function.
 # 2. As mentioned, we separate the actual installation step to install_bashpreexec. I just took those last three lines
 # out of bash-preexec and separated them to that different function.
 # 3. We don't modify the HISTCONTROL var in bp_install to avoid clobbering the variable for users. 
 function define_bashpreexec_functions() {
     # bash-preexec.sh -- Bash support for ZSH-like 'preexec' and 'precmd' functions.
     # https://github.com/rcaloras/bash-preexec
     #
     #
     # 'preexec' functions are executed before each interactive command is
     # executed, with the interactive command as its argument. The 'precmd'
     # function is executed before each prompt is displayed.
     #
     # Author: Ryan Caloras (ryan@bashhub.com)
     # Forked from Original Author: Glyph Lefkowitz
     #
     # V0.4.1
     #
     
     # General Usage:
     #
     #  1. Source this file at the end of your bash profile so as not to interfere
     #     with anything else that's using PROMPT_COMMAND.
     #
     #  2. Add any precmd or preexec functions by appending them to their arrays:
     #       e.g.
     #       precmd_functions+=(my_precmd_function)
     #       precmd_functions+=(some_other_precmd_function)
     #
     #       preexec_functions+=(my_preexec_function)
     #
     #  3. Consider changing anything using the DEBUG trap or PROMPT_COMMAND
     #     to use preexec and precmd instead. Preexisting usages will be
     #     preserved, but doing so manually may be less surprising.
     #
     #  Note: This module requires two Bash features which you must not otherwise be
     #  using: the "DEBUG" trap, and the "PROMPT_COMMAND" variable. If you override
     #  either of these after bash-preexec has been installed it will most likely break.
    
 #### In Warp's inclusion of bash-preexec, we don't include these two guards
 #### at the beginning of the script. Instead, we use bash_preexec_imported and __bp_imported to
 #### determine whether we want to execute bash-preexec or not.
     # Make sure this is bash that's running and return otherwise.
     #if [[ -z "${BASH_VERSION:-}" ]]; then
     #    return 1;
     #fi
     #
     ## Avoid duplicate inclusion
     #if [[ -n "${bash_preexec_imported:-}" ]]; then
     #    return 0
     #fi
 #### End of difference 1/4 between Warp and bash-preexec's original source.
     bash_preexec_imported="defined"

     # WARNING: This variable is no longer used and should not be relied upon.
     # Use ${bash_preexec_imported} instead.
     # shellcheck disable=SC2034
     __bp_imported="${bash_preexec_imported}"

     # Should be available to each precmd and preexec
     # functions, should they want it. $? and $_ are available as $? and $_, but
     # $PIPESTATUS is available only in a copy, $BP_PIPESTATUS.
     # TODO: Figure out how to restore PIPESTATUS before each precmd or preexec
     # function.
     __bp_last_ret_value="$?"
     BP_PIPESTATUS=("${PIPESTATUS[@]}")
     __bp_last_argument_prev_command="$_"

     __bp_inside_precmd=0
     __bp_inside_preexec=0

     # Initial PROMPT_COMMAND string that is removed from PROMPT_COMMAND post __bp_install
     __bp_install_string=$'__bp_trap_string="$(trap -p DEBUG)"\ntrap - DEBUG\n__bp_install'

     # Fails if any of the given variables are readonly
     # Reference https://stackoverflow.com/a/4441178
     __bp_require_not_readonly() {
       local var
       for var; do
         if ! ( unset "$var" 2> /dev/null ); then
           echo "bash-preexec requires write access to ${var}" >&2
           return 1
         fi
       done
     }

     # Remove ignorespace and or replace ignoreboth from HISTCONTROL
     # so we can accurately invoke preexec with a command from our
     # history even if it starts with a space.
     __bp_adjust_histcontrol() {
         local histcontrol
         histcontrol="${HISTCONTROL:-}"
         histcontrol="${histcontrol//ignorespace}"
         # Replace ignoreboth with ignoredups
         if [[ "$histcontrol" == *"ignoreboth"* ]]; then
             histcontrol="ignoredups:${histcontrol//ignoreboth}"
         fi;
         export HISTCONTROL="$histcontrol"
     }

     # This variable describes whether we are currently in "interactive mode";
     # i.e. whether this shell has just executed a prompt and is waiting for user
     # input.  It documents whether the current command invoked by the trace hook is
     # run interactively by the user; it's set immediately after the prompt hook,
     # and unset as soon as the trace hook is run.
     __bp_preexec_interactive_mode=""

     # These arrays are used to add functions to be run before, or after, prompts.
     declare -a precmd_functions
     declare -a preexec_functions

     # Trims leading and trailing whitespace from $2 and writes it to the variable
     # name passed as $1
     __bp_trim_whitespace() {
         local var=${1:?} text=${2:-}
         text="${text#"${text%%[![:space:]]*}"}"   # remove leading whitespace characters
         text="${text%"${text##*[![:space:]]}"}"   # remove trailing whitespace characters
         printf -v "$var" '%s' "$text"
     }


     # Trims whitespace and removes any leading or trailing semicolons from $2 and
     # writes the resulting string to the variable name passed as $1. Used for
     # manipulating substrings in PROMPT_COMMAND
     __bp_sanitize_string() {
         local var=${1:?} text=${2:-} sanitized
         __bp_trim_whitespace sanitized "$text"
         sanitized=${sanitized%;}
         sanitized=${sanitized#;}
         __bp_trim_whitespace sanitized "$sanitized"
         printf -v "$var" '%s' "$sanitized"
     }

     # This function is installed as part of the PROMPT_COMMAND;
     # It sets a variable to indicate that the prompt was just displayed,
     # to allow the DEBUG trap to know that the next command is likely interactive.
     __bp_interactive_mode() {
         __bp_preexec_interactive_mode="on";
     }


     # This function is installed as part of the PROMPT_COMMAND.
     # It will invoke any functions defined in the precmd_functions array.
     __bp_precmd_invoke_cmd_impl() {
         # Save the returned value from our last command, and from each process in
         # its pipeline. Note: this MUST be the first thing done in this function.
         # BP_PIPESTATUS may be unused, ignore
         # shellcheck disable=SC2034

         __bp_last_ret_value="$?" BP_PIPESTATUS=("${PIPESTATUS[@]}")

         # Don't invoke precmds if we are inside an execution of an "original
         # prompt command" by another precmd execution loop. This avoids infinite
         # recursion.
         if (( __bp_inside_precmd > 0 )); then
           return
         fi
         local __bp_inside_precmd=1

         # Invoke every function defined in our function array.
         local precmd_function
         for precmd_function in "${precmd_functions[@]:-}"; do

             # Only execute this function if it actually exists.
             # Test existence of functions with: declare -[Ff]
             if type -t "$precmd_function" 1>/dev/null; then
                 __bp_set_ret_value "$__bp_last_ret_value" "$__bp_last_argument_prev_command"
                 # Quote our function invocation to prevent issues with IFS
                 "$precmd_function"
             fi
         done

         __bp_set_ret_value "$__bp_last_ret_value"
     }
 #### Phosphor (#837): the dispatchers re-set `$?` with a bare `return N`
 #### (__bp_set_ret_value), which under `set -e` exits the shell after any
 #### command that left a non-zero status errexit itself exempts (`false && true`).
 #### Run them with errexit/nounset/pipefail relaxed, like every other hook. The
#### precmd dispatcher ends by handing `$?` back, so its own status is the last
#### command's; bash restores `$?` itself for the user's next line, so report
#### success rather than let errexit see that status once the option is restored.
     function __bp_precmd_invoke_cmd { __warp_run_relaxed __bp_precmd_invoke_cmd_impl "$@" || :; }

 #### Phosphor (#807): put __bp_precmd_invoke_cmd back in front of whatever the
 #### user assigned to PROMPT_COMMAND, keeping their command, and run the precmd
 #### the prompt cycle in progress skipped -- so the block of the previous command
 #### (whose status is $1) gets its CommandFinished now instead of never.
     __bp_reinstall_prompt_command() {
         local user_prompt_command line
         # Keep the user's commands, dropping any of our own that survived the
         # reassignment (e.g. `PROMPT_COMMAND+=(...)` on bash 5.1 keeps the old
         # elements), so the repaired value never carries a hook twice.
         user_prompt_command=
         for line in "${PROMPT_COMMAND[@]:-}"; do
             case "$line" in
                 __bp_precmd_invoke_cmd|__bp_interactive_mode|"") continue ;;
             esac
             user_prompt_command+=${user_prompt_command:+$'\n'}$line
         done
         # Mirror bash_body.sh: never keep a PROMPT_COMMAND that re-assigns
         # PROMPT_COMMAND, it would undo this repair at every prompt.
         if [[ "$user_prompt_command" == *"PROMPT_COMMAND="* ]]; then
             user_prompt_command=
         fi
         __bp_set_ret_value "${1:-0}"
         __bp_precmd_invoke_cmd
         unset PROMPT_COMMAND
         PROMPT_COMMAND='__bp_precmd_invoke_cmd'
         PROMPT_COMMAND+=${user_prompt_command:+$'\n'$user_prompt_command}
         PROMPT_COMMAND+=$'\n__bp_interactive_mode'
         __bp_interactive_mode
     }

     # Sets a return value in $?. We may want to get access to the $? variable in our
     # precmd functions. This is available for instance in zsh. We can simulate it in bash
     # by setting the value here.
     __bp_set_ret_value() {
         return ${1:+"$1"}
     }

     __bp_in_prompt_command() {

         local prompt_command_array IFS=$'\n;'
         read -rd '' -a prompt_command_array <<< "${PROMPT_COMMAND[*]:-}"

         local trimmed_arg
         __bp_trim_whitespace trimmed_arg "${1:-}"

         local command trimmed_command
         for command in "${prompt_command_array[@]:-}"; do
             __bp_trim_whitespace trimmed_command "$command"
             if [[ "$trimmed_command" == "$trimmed_arg" ]]; then
                 return 0
             fi
         done

         return 1
     }

     # This function is installed as the DEBUG trap.  It is invoked before each
     # interactive prompt display.  Its purpose is to inspect the current
     # environment to attempt to detect if the current command is being invoked
     # interactively, and invoke 'preexec' if so.
     __bp_preexec_invoke_exec_impl() {
 #### Phosphor (#807): the exit status of the command that ran before this trap,
 #### saved before anything else can clobber it; __bp_reinstall_prompt_command
 #### reports it for the prompt cycle that lost our precmd.
         local __bp_status_at_trap="$?"

         # Save the contents of $_ so that it can be restored later on.
         # https://stackoverflow.com/questions/40944532/bash-preserve-in-a-debug-trap#40944702
         __bp_last_argument_prev_command="${1:-}"
         # Don't invoke preexecs if we are inside of another preexec.
         if (( __bp_inside_preexec > 0 )); then
           return
         fi
         local __bp_inside_preexec=1

 #### Phosphor (#807): if PROMPT_COMMAND was reassigned at the prompt
 #### (`PROMPT_COMMAND='echo pc'`, or a sourced script doing the same), our
 #### precmd hook is gone: CommandFinished is never emitted again and every
 #### block from here on keeps its running timer, with later commands typed
 #### raw into it. This DEBUG trap still fires -- for the user's new
 #### PROMPT_COMMAND at the very next prompt -- so repair it here. Only once
 #### __bp_install has put the hook in place; before that its absence is normal.
 #### Limit: with PROMPT_COMMAND unset or empty nothing runs at the prompt, so
 #### the repair waits for the user's next command; that command's block then
 #### starts without its command line (its echo already landed in the stuck
 #### block). Still a recovery, where before it was a permanent lock-up.
         if [[ -n "${__bp_prompt_command_installed:-}" \
               && "${PROMPT_COMMAND[*]:-}" != *"__bp_precmd_invoke_cmd"* ]]; then
             __bp_reinstall_prompt_command "$__bp_status_at_trap"
         fi

         # Checks if the file descriptor is not standard out (i.e. '1')
         # __bp_delay_install checks if we're in test. Needed for bats to run.
         # Prevents preexec from being invoked for functions in PS1
         if [[ ! -t 1 && -z "${__bp_delay_install:-}" ]]; then
             return
         fi

         if [[ -n "${COMP_LINE:-}" ]]; then
             # We're in the middle of a completer. This obviously can't be
             # an interactively issued command.
             return
         fi

 #### For Warp, the clause below prevents the typeahead readline binding from triggering
 #### preexec, which would otherwise break the typeahead/background process implementation.
         if [[ -n "${READLINE_LINE+x}" ]]; then
             # We're in the middle of a readline binding to a shell command. Since
             # this means the line editor is active, there can't be an interactive
             # command running.
             return
         fi
 #### End of difference 2/4 between Warp and bash-preexec's original source.

         if [[ -z "${__bp_preexec_interactive_mode:-}" ]]; then
             # We're doing something related to displaying the prompt.  Let the
             # prompt set the title instead of me.
             return
         else
             # If we're in a subshell, then the prompt won't be re-displayed to put
             # us back into interactive mode, so let's not set the variable back.
             # In other words, if you have a subshell like
             #   (sleep 1; sleep 2)
             # You want to see the 'sleep 2' as a set_command_title as well.
             if [[ 0 -eq "${BASH_SUBSHELL:-}" ]]; then
                 __bp_preexec_interactive_mode=""
             fi
         fi

         if  __bp_in_prompt_command "${BASH_COMMAND:-}"; then
             # If we're executing something inside our prompt_command then we don't
             # want to call preexec. Bash prior to 3.1 can't detect this at all :/
             __bp_preexec_interactive_mode=""
             return
         fi

         local this_command
         this_command=$(
             export LC_ALL=C
             HISTTIMEFORMAT='' builtin history 1 | sed '1 s/^ *[0-9][0-9]*[* ] //'
         )

         # Sanity check to make sure we have something to invoke our function with.
         if [[ -z "$this_command" ]]; then
             return
         fi

         # Invoke every function defined in our function array.
         local preexec_function
         local preexec_function_ret_value
         local preexec_ret_value=0
         for preexec_function in "${preexec_functions[@]:-}"; do

             # Only execute each function if it actually exists.
             # Test existence of function with: declare -[fF]
             if type -t "$preexec_function" 1>/dev/null; then
                 __bp_set_ret_value "${__bp_last_ret_value:-}"
                 # Quote our function invocation to prevent issues with IFS
                 "$preexec_function" "$this_command"
                 preexec_function_ret_value="$?"
                 if [[ "$preexec_function_ret_value" != 0 ]]; then
                     preexec_ret_value="$preexec_function_ret_value"
                 fi
             fi
         done

         # Restore the last argument of the last executed command, and set the return
         # value of the DEBUG trap to be the return code of the last preexec function
         # to return an error.
         # If `extdebug` is enabled a non-zero return value from any preexec function
         # will cause the user's command not to execute.
         # Run `shopt -s extdebug` to enable
         __bp_set_ret_value "$preexec_ret_value" "$__bp_last_argument_prev_command"
     }
 #### Phosphor (#837): see __bp_precmd_invoke_cmd.
     function __bp_preexec_invoke_exec { __warp_run_relaxed __bp_preexec_invoke_exec_impl "$@"; }

     __bp_install() {
         # Exit if we already have this installed.
         if [[ "${PROMPT_COMMAND[*]:-}" == *"__bp_precmd_invoke_cmd"* ]]; then
             return 1;
         fi

         trap '__bp_preexec_invoke_exec "$_"' DEBUG

         # Preserve any prior DEBUG trap as a preexec function
         local prior_trap
         # we can't easily do this with variable expansion. Leaving as sed command.
         # shellcheck disable=SC2001
         prior_trap=$(sed "s/[^']*'\(.*\)'[^']*/\1/" <<<"${__bp_trap_string:-}")
         unset __bp_trap_string
         if [[ -n "$prior_trap" ]]; then
             eval '__bp_original_debug_trap() {
               '"$prior_trap"'
             }'
             preexec_functions+=(__bp_original_debug_trap)
         fi

 # Note: this method diverges from the bash_preexec script in that
 # we don't call __bp_adjust_histcontrol because of
 # https://linear.app/warpdotdev/issue/WAR-2592.
 # End of difference 3/4 between Warp and bash-preexec's original source.

         # Issue #25. Setting debug trap for subshells causes sessions to exit for
         # backgrounded subshell commands (e.g. (pwd)& ). Believe this is a bug in Bash.
         #
         # Disabling this by default. It can be enabled by setting this variable.
         if [[ -n "${__bp_enable_subshells:-}" ]]; then

             # Set so debug trap will work be invoked in subshells.
             set -o functrace > /dev/null 2>&1
             shopt -s extdebug > /dev/null 2>&1
         fi;

         local existing_prompt_command
         # Remove setting our trap install string and sanitize the existing prompt command string
         existing_prompt_command="${PROMPT_COMMAND:-}"
         # Edge case of appending to PROMPT_COMMAND
         existing_prompt_command="${existing_prompt_command//$__bp_install_string/:}" # no-op
         existing_prompt_command="${existing_prompt_command//$'\n':$'\n'/$'\n'}" # remove known-token only
         existing_prompt_command="${existing_prompt_command//$'\n':;/$'\n'}" # remove known-token only
         __bp_sanitize_string existing_prompt_command "$existing_prompt_command"
         if [[ "${existing_prompt_command:-:}" == ":" ]]; then
             existing_prompt_command=
         fi

         # Install our hooks in PROMPT_COMMAND to allow our trap to know when we've
         # actually entered something.
         PROMPT_COMMAND='__bp_precmd_invoke_cmd'
         PROMPT_COMMAND+=${existing_prompt_command:+$'\n'$existing_prompt_command}
         if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
             PROMPT_COMMAND+=('__bp_interactive_mode')
         else
             # shellcheck disable=SC2179 # PROMPT_COMMAND is not an array in bash <= 5.0
             PROMPT_COMMAND+=$'\n__bp_interactive_mode'
         fi

         # Add two functions to our arrays for convenience
         # of definition.
         precmd_functions+=(precmd)
         preexec_functions+=(preexec)

 #### Phosphor (#807): from here on, PROMPT_COMMAND without our hook means the
 #### user removed it; see __bp_preexec_invoke_exec.
         __bp_prompt_command_installed=1

         # Invoke our two functions manually that were added to $PROMPT_COMMAND
         __bp_precmd_invoke_cmd
         __bp_interactive_mode
     }

     # Sets an installation string as part of our PROMPT_COMMAND to install
     # after our session has started. This allows bash-preexec to be included
     # at any point in our bash profile.
     __bp_install_after_session_init() {
         # bash-preexec needs to modify these variables in order to work correctly
         # if it can't, just stop the installation
         __bp_require_not_readonly PROMPT_COMMAND HISTCONTROL HISTTIMEFORMAT || return

         local sanitized_prompt_command
         __bp_sanitize_string sanitized_prompt_command "${PROMPT_COMMAND:-}"
         if [[ -n "$sanitized_prompt_command" ]]; then
             # shellcheck disable=SC2178 # PROMPT_COMMAND is not an array in bash <= 5.0
             PROMPT_COMMAND=${sanitized_prompt_command}$'\n'
         fi;
         # shellcheck disable=SC2179 # PROMPT_COMMAND is not an array in bash <= 5.0
         PROMPT_COMMAND+=${__bp_install_string}
     }
 ### In Warp, we run this last install step in the install_bashpreexec
 ### function so this is commented out here.
     ## Run our install so long as we're not delaying it.
     #if [[ -z "${__bp_delay_install:-}" ]]; then
     #    __bp_install_after_session_init
     #fi;
 ### End of difference 4/4 between Warp and bash-preexec's original source.
 }

 function install_bashpreexec() {
     # Run our install so long as we're not delaying it.
     if [[ -z "${__bp_delay_install:-}" ]]; then
         __bp_install_after_session_init
     fi;
 }

 # This string, after interpolation of the contents of bash_body.sh, contains
 # the warp bootstrap script. We take over startup from the shell so we have
 # more control over when the various startup files run. At a high level,
 # whenever we start a shell, we try and get it to do as little as possible and
 # then take over when we input these contents into the PTY. We also minimize
 # the side effects of doing this to give the user a seamless experience.
 #
 # Starting a shell happens before this string is evaluated, and that happens either
 # locally where we create a new PTY, or remotely, in a SSH wrapper. In both cases,
 # we use the --rcfile option so *no* startup files are evaluated. In our
 # custom rcfile, we turn off the line editor to work around a bug that
 # prevented us from reliably inputting our script. And then
 # we notify the terminal to input these characters in. For the remote case,
 # we actually start in an interactive non-login shell (i.e. it runs ~/.bashrc),
 # but it gets replaced by a new shell that we fully control.
 # `read -d ''` returns 1 at end of input even though it read everything; in a
 # subshell bootstrap this runs in a shell whose rcfiles may have set `set -e`,
 # before bash_body.sh relaxes anything, so that status must not escape (#825).
 read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :
#include bundled/bootstrap/bash_body.sh
EOM
 # We need to restore the line editor before we evaluate the bootstrap logic
 # or everything freezes up
 stty sane
 eval "$WARP_BOOTSTRAP_VAR"; unset WARP_BOOTSTRAP_VAR
