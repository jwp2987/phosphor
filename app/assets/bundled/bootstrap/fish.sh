# Note that WARP_SESSION_ID is expected to have been set when executing commands to
# emit the InitShell payload, which includes the session ID.
begin
# We wrap ourselves in a begin block because these are effectively injected
# keystrokes, and we want to execute this whole script as a single command.
# Of course we are fish script, not POSIX shell script.
# Set fish_private_mode to prevent saving to history.
# Note: We don't add a leading space to the `begin` command here because this entire script is
# pasted into fish with bracketed paste, and Fish strips out the leading whitespace. Instead,
# we insert the space _before_ pasting the script in, which has the same effect
set -l saved_fish_private_mode $fish_private_mode
set -g fish_private_mode 1

# Disable fish autosuggestions - because input goes through Warp's editor instead,
# they are never actionable, and the extra output can cause problems.
set -g fish_autosuggestion_enabled 0

# Byte sequence used to signal the start of a DCS. ([0x1b, 0x50, 0x24] which
# maps to <ESC>, P, $ in ASCII.)
set -g DCS_START \u1b\u50\u24

# Appended to $DCS_START to signal that the following message is JSON-encoded.
# The Rust app also receives non-JSON-encoded DCS's sent from
# _warp_run_generator_command_internal, which instead end in 'e' (0x65).
set -g DCS_JSON_MARKER 'd'

# Byte sequence used to signal the end of a DCS: the single byte 0x9c (ST).
# Must be \x9c, not \u9c: fish encodes \u9c as the UTF-8 form of U+009C (c2 9c),
# which prepends a stray 0xc2 to every hook. Every other emitter in this fork
# (`fish_init_shell.sh`, `fish_init_subshell.sh`, `bash_body.sh`, `zsh_body.sh`)
# writes the bare byte.
set -g DCS_END \x9c

set -g OSC_START (builtin printf '\e]9278;')

set -g OSC_END (builtin printf '\a')

set -g OSC_PARAM_SEPARATOR ';'

set -g RESET_GRID_OSC (builtin printf '\e]9279\a')

if test -n "$WARP_INITIAL_WORKING_DIR"
    builtin cd "$WARP_INITIAL_WORKING_DIR" >/dev/null 2>&1
    set -e WARP_INITIAL_WORKING_DIR
end

# Append additional PATH entries if provided via WARP_PATH_APPEND.
if test -n "$WARP_PATH_APPEND"
    set -gx --path PATH "$PATH:$WARP_PATH_APPEND"
    set -e WARP_PATH_APPEND
end

function warp_send_json_message
    # Sends a message to the controlling terminal as a DSC control sequence.
    set -l escaped_json (warp_hex_encode_string "$argv")
    if [ "$WARP_USING_WINDOWS_CON_PTY" = true ]
        __warp_emit "$OSC_START$DCS_JSON_MARKER$OSC_PARAM_SEPARATOR$escaped_json$OSC_END"
    else
        __warp_emit "$DCS_START$DCS_JSON_MARKER$escaped_json$DCS_END"
    end
end

# Hooks go to the controlling terminal, not fd 1, so that redirecting the
# interactive shell's own stdout (`exec >/dev/null`) cannot swallow them and
# leave the block running forever (#806). Falls back to fd 1 without a tty.
# (The fish_prompt functions below must keep writing to stdout: fish captures
# their output itself.)
function __warp_emit
    if begin; true > /dev/tty; end 2>/dev/null
        builtin echo -n $argv > /dev/tty
    else
        builtin echo -n $argv
    end
end

function warp_maybe_send_reset_grid_osc
    # Note that $WARP_USING_WINDOWS_CON_PTY is set in the init shell script.
    if [ "$WARP_USING_WINDOWS_CON_PTY" = true ]
        __warp_emit $RESET_GRID_OSC
    end
end


# warp_hex_encode_string hex-encodes the given string with `od`.
# Phosphor (#1170): `od` prints the bytes as space-separated lines, so the spaces and the line
# breaks are dropped with `string` (in-process) rather than a forked `tr -d ' \n'`; this runs three
# times per command.
function warp_hex_encode_string 
  builtin printf '%s' (string replace -a ' ' '' -- (builtin printf '%s' "$argv" | od -An -v -tx1))
end

# A list of PIDs for running in-band command(s). This is used to kill running
# in-band commands in preexec for a user command, so they do not interfere with
# user command output.
#
# Seeded as an empty list, NOT as `''`: `''` is a list holding one empty string, which
# survives every removal below and makes the preexec loop run `kill -9 ''` (a parse error,
# silenced by the redirect) once per user command forever.
set -g _warp_generator_pids

# Runs the given command in the background, records its PID in
# _WARP_GENERATOR_PIDS_STARTED_TMP_FILE, and adds its PID from the file when
# the job is completed.
#
# Usage:
#   _warp_run_generator_command_internal <command_id> '<command>'
#
# The first argument is the command's ID, which is included in the DCS string sent
# to the rust app. The second argument is the command string itself.
function  _warp_run_generator_command_internal
    set -l command_id $argv[1]
    set -l command (string join -- ' ' (string escape $argv[2]))
    # Fish cannot run shell functions in the background, so in order to run
    # the command in the the background we spawn a fish process, inlining the
    # code necessary to write the command's output OSC to the rust app. This
    # differs from the bash and zsh implementations, which rely on running 
    # functions in the background.
    #
    # The `IFS` has to be set to the empty string to prevent fish from converting
    # the command output into a list by splitting output on newlines (fish splits
    # strings on $IFS into lists). Essentially, setting IFS to the empty string
    # allows us to preserve newlines in command output (that might be relied
    # upon) for parsing output.
    #
    # N.B. Fish shell variables cannot contain null characters, so the command output must be
    # immediately hex encoded before being stored in a variable.
    fish -c "
        set -l warp_using_windows_con_pty $WARP_USING_WINDOWS_CON_PTY;
        set -l reset_grid_osc $RESET_GRID_OSC;
        function warp_maybe_send_reset_grid_osc
            if [ \"\$warp_using_windows_con_pty\" = true ]
                builtin printf \$reset_grid_osc
            end
        end
        set -l OSC_START_GENERATOR_OUTPUT \$(builtin printf '\e]9277;A\a')
        set -l OSC_END_GENERATOR_OUTPUT \$(builtin printf '\e]9277;B\a')
        set -l command_id $command_id;
        set -l command $command;
        set -l IFS;
        begin
          builtin echo -n \"\$command_id;\"
          eval \$command 2>&1
          builtin echo -n \";\$status\"
        end | od -An -v -tx1 | command tr -d ' \n' | builtin read -lz hex_encoded_message
        set -l LC_ALL \"C\"
        set -l byte_count (string length \"\$hex_encoded_message\")
        builtin echo -n \"\$OSC_START_GENERATOR_OUTPUT\$byte_count;\$hex_encoded_message\$OSC_END_GENERATOR_OUTPUT\"
        warp_maybe_send_reset_grid_osc" 2> /dev/null &
        
    set -l command_pid $last_pid
    set -a _warp_generator_pids $command_pid

    # Remove the command's PID from _warp_generator_pids when the command exits.
    function on_command_{$command_pid}_finish --on-process-exit $command_pid --inherit-variable command_pid
        # `string match -v` removes the ELEMENT equal to $command_pid. `string replace` was
        # wrong here: it edits every element in place, so a completed generator PID that is a
        # decimal substring of a live one rewrites that one -- observed in fish, ['', '40213',
        # '213'] minus 213 becomes ['', '40', ''] -- and the preexec below then sends SIGKILL
        # to PID 40, an unrelated user-owned process, with stderr discarded. Needs a pid_max
        # wrap to happen, but the failure is silent and unbounded in what it can kill.
        set -g _warp_generator_pids (string match -v -- $command_pid $_warp_generator_pids)

        # Erase this function after the pids list is updated above so we don't create an infinite number of
        # functions that could pollute the user's context (nested functions are still existing in the global
        # scope).
        functions -e on_command_{$command_pid}_finish

        # Note: If we're on windows, we send a reset grid to erase any cursor mutations caused by
        # the in-band command.
        warp_maybe_send_reset_grid_osc
    end
end

# Executes a generator command in the background, where the first argument is
# the "command ID" assigned by the Rust app and the second-nth arguments
# specify the command to be run.
#
# Note that the command string should be single-quoted so any variables are
# not substituted until the command string is actually evaluated.
#
# Usage:
#   warp_run_generator_command <command_id> '<command> <arg1> ... <argn>'
#
# MATCHED PAIR -- this function's NAME is a protocol surface shared with the Rust app,
# which builds the call string in `POSIX_GENERATOR_WRAPPER`
# (app/src/terminal/model/session/command_executor/in_band_command_executor.rs) and
# detects it again there and in
# app/src/ai/blocklist/action_model/execute/shell_command.rs. Nothing in the type system
# connects the two: rename one side and everything still compiles and every Rust test
# still passes, while in-band generators silently invoke a function that does not exist.
# That is exactly how #597 happened on the PowerShell side. The history guard below
# matches this name too. `script/check_generator_wrapper_names` enforces the pairing.
function warp_run_generator_command
    # Setting this environment variable allows warp_precmd to detect if a generator
    # command or a user command has just completed.
    set -g _WARP_GENERATOR_COMMAND 1
    _warp_run_generator_command_internal $argv
end

# Phosphor (#855): while an agent command runs, `exit` is shadowed by a function (see
# agent_shell_wrap.rs). If the command is interrupted, put the user's own `exit` back at the next prompt.
function __phosphor_ap_fish_restore --on-event fish_prompt
    if set -q __phosphor_ap_guard
        functions -e exit
        if functions -q __phosphor_ap_gx
            functions -c __phosphor_ap_gx exit
            functions -e __phosphor_ap_gx
        end
        set -e __phosphor_ap_guard
    end
end

# Phosphor (#1170): sets __warp_git_branch and __warp_git_head the way `git symbolic-ref --short
# HEAD`, then `git rev-parse --short HEAD`, would, but without forking when the answer is in
# .git/HEAD. Those two git processes ran on every prompt (about 4 ms in a repo, 2 in a plain
# directory). Returns 1, leaving the answer to git exactly as before, for anything it cannot
# answer with certainty: GIT_* variables that change discovery, a .git file (worktree, submodule),
# a bare repo or the .git directory itself, a repo owned by someone else (git refuses it), reftable
# (HEAD is `refs/heads/.invalid`), a detached HEAD (needs an abbreviated hash), and a branch whose
# short name git would qualify because a tag of the same name exists (a packed one is checked with
# git once per directory and branch). The walk up uses `..` instead of trimming $PWD, so a
# symlinked directory is followed the way git (which uses the physical path) follows it.
function __warp_git_head_fast
    for v in GIT_DIR GIT_WORK_TREE GIT_CEILING_DIRECTORIES GIT_DISCOVERY_ACROSS_FILESYSTEM GIT_NAMESPACE GIT_COMMON_DIR
        set -q $v; and return 1
    end
    set -l p .
    set -l gd
    set -l n 0
    while test $n -lt 64
        set n (math $n + 1)
        if test -d $p/.git
            set gd $p/.git
            break
        end
        if test -e $p/.git; or begin; test -f $p/HEAD; and test -d $p/refs; end
            return 1
        end
        if test $p/.. -ef $p
            # Reached / without finding a repo: git would print nothing for both.
            set -g __warp_git_branch ""
            set -g __warp_git_head ""
            return 0
        end
        set p $p/..
    end
    test -n "$gd"; and test -O $gd; and test -O $p; and test -d $gd/objects; and test -d $gd/refs; and test -r $gd/HEAD; or return 1
    read -l line < $gd/HEAD; or return 1
    switch "$line"
        case 'ref: refs/heads/.invalid'
            return 1
        case 'ref: refs/heads/*'
            set line (string replace 'ref: refs/heads/' '' -- "$line")
        case '*'
            return 1
    end
    test -n "$line"; or return 1
    if test -e "$gd/$line"; or test -e "$gd/refs/$line"; or test -e "$gd/refs/tags/$line"
        return 1
    end
    if test -e $gd/packed-refs; and test "$__warp_git_checked" != "$PWD|$line"
        # A packed tag of the same name cannot be seen without reading the whole file, so ask
        # git once per directory and branch instead of on every prompt.
        set -l short (warp_git symbolic-ref --short HEAD 2> /dev/null)
        test "$short" = "$line"; or return 1
        set -g __warp_git_checked "$PWD|$line"
    end
    set -g __warp_git_branch "$line"
    set -g __warp_git_head "$line"
end

# Run before a command is executed.
function warp_preexec --on-event fish_preexec
    set -l command (warp_escape_json "$argv")
    # Phosphor (#1353): one per-session counter for every Precmd and Preexec this shell sends.
    set -g __warp_seq (math "$__warp_seq + 1")
    warp_send_json_message "{\"hook\": \"Preexec\", \"value\": {\"command\": \"$command\", \"session_id\": $WARP_SESSION_ID, \"seq\": $__warp_seq}}"
    warp_maybe_send_reset_grid_osc

    # If this preexec is called for user command, kill ongoing generator command jobs.
    # `string trim` strips the leading space the Rust side prepends to fish generator
    # commands to keep them out of fish history (see POSIX_GENERATOR_WRAPPER in
    # in_band_command_executor.rs); `string match`'s glob is anchored at the start, so
    # without the trim no generator command would ever match its own name.
    #
    # The `test -n` is not redundant: unlike bash and zsh, fish fires fish_preexec for a
    # whitespace-only command line (verified under a pty: argv[1]=[   ], count=1), where bash's
    # equivalent guard at bash_body.sh:307 simply sees an unset $BASH_COMMAND and does nothing.
    # Such a line runs no user command, so it must not cancel in-flight generators.
    set -l trimmed_command (string trim -- $argv[1])
    if test -n "$trimmed_command"; and not string match -q "warp_run_generator_command*" -- $trimmed_command
        for pid in $_warp_generator_pids
            # Suppress stderr output; kill writes to stderr if the given PID is not running
            # (which might rarely be the case due to race conditions in checking which PIDs to
            # cancel and this kill command).
            kill -9 $pid >/dev/null 2>/dev/null
        end
        set -g _warp_generator_pids
    end
end

# The git prompts git commands are read-only and should not interfere with
# other processes. This environment variable is equivalent to running with `git
# --no-optional-locks`, but falls back gracefully for older versions of git.
# See git(1) for and git-status(1) for a description of that flag.
#
# We wrap in a local function instead of exporting the variable directly in
# order to avoid interfering with manually-run git commands by the user.
function warp_git
    GIT_OPTIONAL_LOCKS=0 command git $argv
end

# Phosphor (#1390, #1391): LineEditorReading marker, printed as the LAST bytes of the prompt so it
# reaches the terminal in the same paint that precedes fish's first read. fish captures prompt
# output itself and passes OSC through without counting its width (as it does for 133;A/B). It is
# part of the prompt, so a repaint that re-runs the prompt repeats it with the same seq (the app
# treats it as idempotent). fish has no PS2: an incomplete command line just gets a newline inside
# the same editor, so there is no ContinuationPrompt on fish. fish's `read` builtin does not run
# fish_prompt, so a builtin read never sends this. No fork: printf is a builtin.
function __warp_lr_print
    test -n "$WARP_SESSION_ID"; or return 0
    test "$PHOSPHOR_LINE_EDITOR_MARKERS" = 0; and return 0
    builtin printf '\e]9278;f;{"hook": "LineEditorReading", "value": {"session_id": %s, "seq": %s}}\a' $WARP_SESSION_ID $__warp_seq
end

# Wrap fish prompt function output with OSC prompt marker sequences,
# so that we can direct the prompt bytes to the appropriate grids.
function warp_update_prompt_vars
  # Back up the original fish_prompt if not already done
  if not functions -q warp_original_fish_prompt
    functions -c fish_prompt warp_original_fish_prompt
  end

  # Back up the original fish_right_prompt if it exists and not already backed up
  if functions -q fish_right_prompt; and not functions -q warp_original_fish_right_prompt
    functions -c fish_right_prompt warp_original_fish_right_prompt
  end

  # If not honoring PS1, set both prompts to be empty
  if test "$WARP_HONOR_PS1" = "0"
    function fish_prompt; __warp_lr_print; end
    function fish_right_prompt; builtin echo -n ""; end
  # If honoring PS1, add prefix/suffix to both prompts
  else

    function end_prompt        
      builtin echo -n (builtin printf '\x1b')
      if test "$WARP_HONOR_PS1" != "1" && [ "$WARP_USING_WINDOWS_CON_PTY" = true ]
        builtin echo -n "]133;B$RESET_GRID_OSC"
      else
        builtin echo -n ']133;B'
      end
      builtin echo -n (builtin printf '\x07')
    end

    # Redefine fish_prompt to include prefix and suffix
    function fish_prompt
      builtin echo -n (builtin printf '\x1b')
      builtin echo -n ']133;A'
      builtin echo -n (builtin printf '\x07')
      warp_original_fish_prompt
      end_prompt
      __warp_lr_print
    end

    # Check if warp_original_fish_right_prompt was backed up before redefining fish_right_prompt
    if functions -q warp_original_fish_right_prompt
      function fish_right_prompt
        builtin echo -n (builtin printf '\x1b')
        builtin echo -n ']133;P;k=r'
        builtin echo -n (builtin printf '\x07')
        warp_original_fish_right_prompt
        end_prompt
      end
    end
  end
end

# Changes the WARP_HONOR_PS1 variable to 1, to indicate we want to use the user's custom prompt. Restores
# the original fish prompt functions (which we set to empty for Warp prompt) by calling warp_update_prompt_vars
# to refresh the prompt. We force a repaint of the prompt to ensure the change is reflected immediately.
function warp_change_prompt_modes_to_ps1
  set -x WARP_HONOR_PS1 "1"

  # Restores fish_prompt and fish_right_prompt.
  warp_update_prompt_vars
  # Forces a repaint of the current prompt to ensure the change is reflected immediately.
  commandline -f repaint
end

# Changes the WARP_HONOR_PS1 variable to 0, to indicate we want to use the Warp prompt. Saves and clears
# the fish prompt functions (which we set to empty for Warp prompt) by calling warp_update_prompt_vars
# to refresh the prompt. We force a repaint of the prompt to ensure the change is reflected immediately.
function warp_change_prompt_modes_to_warp_prompt
  set -x WARP_HONOR_PS1 "0"

  # Updates fish_prompt and fish_right_prompt to be empty.
  warp_update_prompt_vars
  # Forces a repaint of the current prompt to ensure the change is reflected immediately.
  commandline -f repaint
end

set block_id 0
set -g __warp_seq 0
# Run before the prompt is displayed. We also need to trigger this on "fish_posterror", as
# submitting a command containing a syntax error will not trigger "fish_preexec" or "fish_prompt".
function warp_precmd --on-event fish_prompt --on-event fish_posterror
    # Handle prompt behavior (we do this first to make sure the exit status is from the command,
    # rather than from our own code)
    set -l exit_code $status

    # This function is triggered by both "fish_prompt" and "fish_posterror" events. If it was
    # "fish_prompt", then $status will be properly set. If it was "fish_posterror", it won't be.
    # We want to set the exit code to 1 for errors, but there is no direct way to check which event
    # triggered this function call. The length of $argv actually tells us, as "fish_posterror"
    # passes the erroneous command while "fish_prompt" passes nothing.
    if test (count $argv) -gt 0
        set exit_code 1
    end

    # Compute the next block ID once so the CommandFinished hook and the following Precmd hook
    # report the same ID. The Precmd carries this completion metadata so the client can classify it
    # and, if the CommandFinished was missed, recover the completion.
    set -l next_block_id "precmd-$WARP_SESSION_ID-$block_id"
    set -g __warp_seq (math "$__warp_seq + 1")
    warp_send_json_message "{\"hook\": \"CommandFinished\", \"value\": {\"exit_code\": $exit_code, \"next_block_id\": \"$next_block_id\", \"session_id\": $WARP_SESSION_ID}}"
    warp_maybe_send_reset_grid_osc

    set block_id (math $block_id + 1)

    if ! test -z $_WARP_GENERATOR_COMMAND
        set -e _WARP_GENERATOR_COMMAND
        set -l escaped_json "{\"hook\": \"Precmd\", \"value\": {
        \"exit_code\": $exit_code,
        \"next_block_id\": \"$next_block_id\",
        \"pwd\": \"\",
        \"ps1\": \"\",
        \"git_head\": \"\",
        \"git_branch\": \"\",
        \"virtual_env\": \"\",
        \"conda_env\": \"\",
        \"node_version\": \"\",
        \"session_id\": $WARP_SESSION_ID,
        \"seq\": $__warp_seq,
        \"is_after_in_band_command\": true
        }}"
        warp_send_json_message $escaped_json
        return 0
    end

    # Fish's vi mode causes a conflict with the input reporting binding.
    # Set the bindings back for this session. Note that just calling fish_default_key_bindings
    # affects other sessions so we do it this way instead.
    set -g fish_key_bindings fish_default_key_bindings

    # Do not use kill-line because of the kill ring interation.
    bind \cP "commandline ''"

    # We use the ESC-p bindkey for this ("p" for PS1/custom prompt).
    bind \ep warp_change_prompt_modes_to_ps1

    # We use the ESC-w bindkey for this ("w" for Warp prompt).
    bind \ew warp_change_prompt_modes_to_warp_prompt

    bind \ei warp_report_input

    # Define local variables in appropriate outer block for fish variable scoping.
    # See https://stackoverflow.com/a/53685510.
    set -l escaped_prompt
    set -l escaped_right_prompt

    set -l escaped_pwd
    if set -q WSL_DISTRO_NAME
        # In WSL, avoid symlinks b/c on Windows `std::fs` is unable to resolve symlink inside WSL containers.
        set escaped_pwd (warp_escape_json (builtin pwd -P))
    else
        set escaped_pwd (warp_escape_json $PWD)
    end

    set -l escaped_virtual_env ""
    set -l escaped_conda_env ""
    set -l escaped_node_version ""
    set -l escaped_git_head ""
    set -l escaped_git_branch ""

    # Only fill these fields once we've finished bootstrapping, as the
    # blocks created during the bootstrap process don't have visible
    # prompts, and we don't want to invoke `git` before we've sourced the
    # user's rcfiles and have a fully-populated PATH.
    if test -n "$WARP_BOOTSTRAPPED"
      if test -n "$VIRTUAL_ENV"
          set escaped_virtual_env (warp_escape_json "$VIRTUAL_ENV")
      end
      if test -n "$CONDA_DEFAULT_ENV"
          set escaped_conda_env (warp_escape_json "$CONDA_DEFAULT_ENV")
      end
      
        # Get the Node.js version, but only when the Node.js Version chip is enabled.
        # Warp sets WARP_PROMPT_NODE_VERSION_ENABLED to "0" when the chip is not in the
        # prompt (defaulting to enabled when unset), so we avoid spawning `node` on
        # every prompt when the chip is not shown.
        if test "$WARP_PROMPT_NODE_VERSION_ENABLED" != "0"; and command -v node > /dev/null 2>&1
            # Check for package.json in current directory and parent directories
            set current_dir (builtin pwd)
            set found_package_json false
            set package_json_dir ""
            while test -n "$current_dir"
                if test -f "$current_dir/package.json"
                    set found_package_json true
                    set package_json_dir "$current_dir"
                    break
                end
                if test "$current_dir" = "/"
                    break
                end
                # Strip the last path segment without spawning `dirname`.
                set current_dir (string replace -r '/[^/]*$' '' -- "$current_dir")
                if test -z "$current_dir"
                    set current_dir "/"
                end
            end
            
            # Only show node version if package.json is within a git repository
            if test "$found_package_json" = true
                set git_dir "$package_json_dir"
                set in_git_repo false
                while test -n "$git_dir"
                    if test -d "$git_dir/.git"
                        set in_git_repo true
                        break
                    end
                    if test "$git_dir" = "/"
                        break
                    end
                    set git_dir (string replace -r '/[^/]*$' '' -- "$git_dir")
                    if test -z "$git_dir"
                        set git_dir "/"
                    end
                end
                
                if test "$in_git_repo" = true
                    # Cache the resolved version keyed on PWD + PATH so we only spawn
                    # `node --version` when the directory or PATH changes (PATH changes
                    # on `nvm use`). Use global cache vars so they persist across calls.
                    set -l node_cache_key "$PWD:$PATH"
                    if test "$node_cache_key" = "$_WARP_NODE_VERSION_CACHE_KEY"
                        set escaped_node_version "$_WARP_NODE_VERSION_CACHE_VALUE"
                    else
                        set -l node_version (node --version 2>/dev/null)
                        if test -n "$node_version"
                            set escaped_node_version (warp_escape_json "$node_version")
                        end
                        set -g _WARP_NODE_VERSION_CACHE_KEY "$node_cache_key"
                        set -g _WARP_NODE_VERSION_CACHE_VALUE "$escaped_node_version"
                    end
                end
            end
        end

      set -l git_branch ""
      set -l git_head ""
      if command -q git
          if __warp_git_head_fast
              set git_branch "$__warp_git_branch"
              set git_head "$__warp_git_head"
          else
              set git_branch (warp_git symbolic-ref --short HEAD 2> /dev/null)
              if test -z "$git_branch"
                  # Fallback to the git commit hash if we aren't on a named branch.
                  set git_head (warp_git rev-parse --short HEAD 2> /dev/null)
              else
                  set git_head "$git_branch"
              end
          end
      end
      set escaped_git_head (warp_escape_json "$git_head")
      set escaped_git_branch (warp_escape_json "$git_branch")
    end

    warp_update_prompt_vars
    # This is used solely for prompt previews, when we're using prompt markers with combined grid.
    # We need to use this since fish does not have a way to ignore printable characters for cursor
    # positioning (unlike zsh/bash), so we need a separate mechanism to send the prompt to Warp
    # in the case of Warp prompt (for previewing the PS1). We send an escaped version of the raw prompt
    # bytes via a hex string (in a JSON payload) to Warp.
    # Note that we are CALLING the `warp_original_fish_prompt` function on the next line and assigning the
    # outputted string to the local variable `raw_prompt_for_preview`.
    #
    # Phosphor (#1170): when the user's prompt is honored the Precmd below carries an empty `ps1`
    # and fish itself draws the prompt right after this hook, so the preview would run the user's
    # fish_prompt (git status, nvm, ... in many themes) a second time per prompt only to throw the
    # text away. It is computed only when it is sent.
    if test "$WARP_HONOR_PS1" != "1"
        set -l raw_prompt_for_preview (warp_original_fish_prompt)
        # We encode the prompt as a hex string to pass it to Warp.
        set escaped_prompt (warp_escape_prompt "$raw_prompt_for_preview")
    end

    set -l escaped_json
    if test "$WARP_HONOR_PS1" = "1"
      # Don't send lprompt or rprompt in this case - we'll use prompt markers for both directly!
      set escaped_json "{\"hook\": \"Precmd\", \"value\": {
      \"exit_code\": $exit_code,
      \"next_block_id\": \"$next_block_id\",
      \"pwd\": \"$escaped_pwd\",
      \"ignore_prompt_markers\": false,
      \"ps1\": \"\",
      \"rprompt\": \"\",
      \"git_head\": \"$escaped_git_head\",
      \"git_branch\": \"$escaped_git_branch\",
      \"virtual_env\": \"$escaped_virtual_env\",
      \"conda_env\": \"$escaped_conda_env\",
      \"node_version\": \"$escaped_node_version\",
      \"session_id\": $WARP_SESSION_ID,
      \"seq\": $__warp_seq
      }}"
    else
      # We send an lprompt to use for prompt preview purposes only (we still use prompt markers for active prompts).
      set escaped_json "{\"hook\": \"Precmd\", \"value\": {
      \"exit_code\": $exit_code,
      \"next_block_id\": \"$next_block_id\",
      \"pwd\": \"$escaped_pwd\",
      \"ignore_prompt_markers\": true,
      \"ps1\": \"$escaped_prompt\",
      \"rprompt\": \"\",
      \"git_head\": \"$escaped_git_head\",
      \"git_branch\": \"$escaped_git_branch\",
      \"virtual_env\": \"$escaped_virtual_env\",
      \"conda_env\": \"$escaped_conda_env\",
      \"node_version\": \"$escaped_node_version\",
      \"session_id\": $WARP_SESSION_ID,
      \"seq\": $__warp_seq
      }}"
    end
    warp_send_json_message $escaped_json
end

function warp_escape_prompt
    # To match the implementation of PS1 support in bash / zsh, we use the same method of encoding
    # the prompt as a hex string so that it can be interpreted on the Warp side
    # Note: before converting the prompt to a hex string, we remove any multi-line newlines and
    # replace them with a single space (to avoid prompts that span multiple empty lines)
    #
    # Phosphor (#1170): the encoding costs 4 processes and the prompt is the same on most prompts,
    # so the last result is kept. It depends on nothing but the argument, so reusing it is exact.
    if not set -q __warp_escape_prompt_in; or test "$argv" != "$__warp_escape_prompt_in"
        set -g __warp_escape_prompt_out (builtin echo "$argv" | command tr '\n\n' ' ' | command od -An -v -tx1 | command tr -d ' \n' | string collect)
        set -g __warp_escape_prompt_in "$argv"
    end
    builtin printf '%s' "$__warp_escape_prompt_out"
end

# sed commands that escape every C0 control character (and DEL) that has no short JSON escape as
# \u00XX. JSON forbids raw control characters inside strings, so a single one in a directory name,
# $VIRTUAL_ENV or $CONDA_DEFAULT_ENV made the whole Precmd payload unparseable, and the lost Precmd
# left the tab unable to run anything (#866). The script is built once, here, out of printf escapes
# so this file itself stays plain ASCII; NUL cannot occur in a fish string.
set -g WARP_ESCAPE_JSON_CTRL_SED ''
for warp_ctrl_hex in 01 02 03 04 05 06 07 0b 0e 0f 10 11 12 13 14 15 16 17 18 19 1a 1b 1c 1d 1e 1f 7f
    set WARP_ESCAPE_JSON_CTRL_SED "$WARP_ESCAPE_JSON_CTRL_SED s/"(builtin printf "\\x$warp_ctrl_hex")"/\\\\u00$warp_ctrl_hex/g;"
end
set -e warp_ctrl_hex

# Escapes string(s) passed as input to be JSON-serializable.
#
# Specifically, special characters like backspace, tab, form feed, carriage return, and newlines
# are replaced with escaped equivalents, every other control character (and DEL) becomes \u00XX,
# and double quotes and literal backslash characters are backslash-escaped.
function warp_escape_json
    # Explanation of the sed replacements (each command is separated by a `;`):
    # s/(["\\])/\\\1/g - Replace all double-quote (") and backslash (\) characters with the escaped versions (\" and \\)
    # s/\b/\\b/g - Replace all backspace characters with \b
    # s/\t/\\t/g - Replace all tab characters with \t
    # s/\f/\\f/g - Replace all form feed characters with \f
    # s/\r/\\r/g - Replace all carriage return characters with \r
    # $WARP_ESCAPE_JSON_CTRL_SED - Replace every other control character (and DEL) with \u00XX
    # $!s/$/\\\\n/ - Replace any newlines (except the last trailing newline) with the literal \n character
    #
    # Additional note: In Fish, backslashes still need to be escaped in single quoted strings. So each `\\` below
    # represents a single `\` in the above regex explanation. We also close the single-quoted string in order to
    # insert literal characters (Fish supports literals with escapes when not quoting)
    #
    # LC_ALL=C (a per-command prefix assignment, which fish 3.1+ supports and this file already uses
    # for `git`, so fish's own locale is untouched and no `env` exec is added) makes sed and tr work
    # byte by byte, so a value that is not valid UTF-8 (a directory name is arbitrary bytes) passes through
    # unchanged instead of making a UTF-8-locale sed fail with "illegal byte sequence". Such bytes
    # are not valid JSON on their own; the app decodes the payload lossily and shows each invalid
    # byte as U+FFFD (see `handle_decoded_data` in app/src/terminal/model/ansi/mod.rs).
    #
    # We also use 'command sed' to ensure that we aren't accidentally executing a function named 'sed'
    #
    # Phosphor (#1170): a value made only of characters that need no escaping (letters, digits,
    # space and . _ / @ + : , = % ~ ( ) -) is printed as is, with no sed/tr fork. Preexec and Precmd
    # escape five values per command, and each used to cost two processes.
    if test (count $argv) -eq 1; and not string match -qr '[^[:alnum:] ._/@+:,=%~()-]' -- $argv[1]
        builtin printf '%s' $argv[1]
        return
    end
    string join \n $argv | LC_ALL=C command sed -E 's/(["\\\\])/\\\\\\1/g; s/'\b'/\\\\b/g; s/'\t'/\\\\t/g; s/'\f'/\\\\f/g; s/'\r'/\\\\r/g;'"$WARP_ESCAPE_JSON_CTRL_SED"' $!s/$/\\\\n/' | LC_ALL=C command tr -d '\n'
end

function warp_bootstrapped
  set -l histfile_directory
  set histfile_directory "$XDG_DATA_HOME"
  if test -z "$histfile_directory"
        set histfile_directory "$HOME/.local/share"
  end
  set -l escaped_histfile (warp_escape_json "$histfile_directory/fish/fish_history")

  set -l vi_mode_enabled ""
  if [ "$fish_key_bindings" = "fish_vi_key_bindings" ]
      set vi_mode_enabled "1"
  end

  set -l kernel_name (uname)
  if test -n "$kernel_name"
    if [ "$kernel_name" = "Darwin" ]
      set os_category "MacOS"
    else if [ "$kernel_name" = "Linux" ]
      set os_category "Linux"
      set -l default_os_release_filepath "/etc/os-release"
      set -l fallback_os_release_filepath "/usr/lib/os-release"
      # We first try /etc/os-release and then try /usr/lib/os-release as a fallback.
      if test -f "$default_os_release_filepath"
        set os_release_file "$default_os_release_filepath";
      else if test -f "$fallback_os_release_filepath"
        set os_release_file "$fallback_os_release_filepath";
      end
      if test -f "$os_release_file"
        set linux_distribution (cat $os_release_file | sed -nE 's/^NAME="(.*)"$/\1/p')
      end
    end
  end

  set -l escaped_abbr (warp_escape_json (abbr --show))
  set -l escaped_aliases (warp_escape_json (alias))
  set -l env_var_names (warp_escape_json (set --names))
  set -l function_names (warp_escape_json (functions -an))
  set -l escaped_builtins (warp_escape_json (builtin -n))
  # Note "keywords" is set to an empty string since fish includes keywords as a
  # part of its builtins (e.g. "for", "while", etc.).
  set -l escaped_editor (warp_escape_json "$EDITOR")
  set -l escaped_shell_path (warp_escape_json (status fish-path))
  # home_dir, path, version and distro are free-form too (#879).
  set -l escaped_home (warp_escape_json "$HOME")
  set -l escaped_path (warp_escape_json "$PATH")
  set -l escaped_fish_version (warp_escape_json "$FISH_VERSION")
  set -l escaped_linux_distribution (warp_escape_json "$linux_distribution")
  set -l escaped_wsl_name (warp_escape_json "$WSL_DISTRO_NAME")
  set -l escaped_json "{\"hook\": \"Bootstrapped\", \"value\": {\"histfile\": \"$escaped_histfile\", \"session_id\": $WARP_SESSION_ID, \"shell\": \"fish\", \"home_dir\": \"$escaped_home\", \"path\": \"$escaped_path\", \"editor\": \"$escaped_editor\", \"abbreviations\": \"$escaped_abbr\", \"aliases\": \"$escaped_aliases\", \"function_names\": \"$function_names\", \"env_var_names\": \"$env_var_names\", \"builtins\": \"$escaped_builtins\", \"keywords\": \"\", \"shell_version\": \"$escaped_fish_version\", \"vi_mode_enabled\": \"$vi_mode_enabled\", \"os_category\": \"$os_category\", \"linux_distribution\": \"$escaped_linux_distribution\", \"wsl_name\": \"$escaped_wsl_name\", \"shell_path\": \"$escaped_shell_path\"}}"
  warp_send_json_message $escaped_json
end

function warp_init_shell
    set -l  init_shell "{\"hook\": \"InitShell\", \"value\": {\"shell\": \"$argv\"}}"
    warp_hex_encode_string "$init_shell"
end

# Add a key binding to report the current input buffer to Warp. We can override
# any user-defined binds here because user input goes through Warp's editor, not
# the fish line editor.
# This is arbitrarily bound to ESC-i in all supported shells ("i" for input).
# Binding to ESC-1 caused bootstrap failures with vi keybindings.
# #829: a reply to a terminal query (DA, an OSC colour query, a cursor position report)
# that nobody was reading can end up in the typeahead. Drop those before reporting it.
# Complete replies are removed anywhere. The ESC-less residue of one (`62c`, `R`, `10;rgb:...`)
# is not stripped: it cannot be told from typed text (#927); the app removes it with its reply
# ledger (#921). A reply ending in BEL or ESC \ may have 1-4 digit colour
# components; an unterminated one is matched at a consistent width of 1-4 digits (a greedy 1-4 would eat
# the leading hex letters of text typed right after it, e.g. `echo`).
function __warp_strip_terminal_replies
    # `string` splits piped input into lines, so the start-of-text anchor below has to
    # be applied to an argument, not to the pipe.
    set -l text (string join \n -- $argv | string collect \
        | string replace -r -a '\e\[[?>=]?[0-9;]*[cRt]' '' \
        | string replace -r -a '\e\[\??[0-9;]*\$y' '' \
        | string replace -r -a '\eP>\|[^\e]*\e\\\\' '' \
        | string replace -r -a '\e\][0-9;]*;rgba?:[0-9a-fA-F]{1,4}/[0-9a-fA-F]{1,4}/[0-9a-fA-F]{1,4}(?:\a|\e\\\\)' '' \
        | string replace -r -a '\e\][0-9;]*;rgba?:(?:[0-9a-fA-F]{4}/[0-9a-fA-F]{4}/[0-9a-fA-F]{4}|[0-9a-fA-F]{3}/[0-9a-fA-F]{3}/[0-9a-fA-F]{3}|[0-9a-fA-F]{2}/[0-9a-fA-F]{2}/[0-9a-fA-F]{2}|[0-9a-fA-F]{1}/[0-9a-fA-F]{1}/[0-9a-fA-F]{1})' '' \
        | string collect)
    string join \n -- $text | string collect
end

function warp_report_input
    set -l escaped_input (warp_escape_json (__warp_strip_terminal_replies (commandline)))
    warp_send_json_message "{ \"hook\": \"InputBuffer\", \"value\": { \"buffer\": \"$escaped_input\", \"session_id\": $WARP_SESSION_ID } }"
    # This prevents fish from rendering typeahead as background output once we've collected it.
    commandline ''
end

function clear
    warp_send_json_message "{\"hook\": \"Clear\", \"value\": {\"session_id\": $WARP_SESSION_ID}}"
end

function warp_finish_update
  set -l update_id "$argv[1]"
  warp_send_json_message "{\"hook\": \"FinishUpdate\", \"value\": { \"update_id\": \"$update_id\", \"session_id\": $WARP_SESSION_ID}}"
end


# Check if the warp apt source file has been renamed to `warpdotdev.list.distUpgrade` due to an ubuntu version update.
# If this occurred, we want to rename the source file back to `warpdotdev.list` to ensure updates can proceed.
# We purposefully skip this if either the `warpdotdev.list` file already exists (indicating that the user has already
# done this themselves) _or_ if a `warpdotdev.sources` file exists (which is the new Deb822 format for source files).
# The `.sources` file could only exist if a user manually created it; Ubuntu doesn't create one automatically for the
# warp source file due to a bug in its update flow where it considers our source file to be "invalid" because it
# contains a `signed-by` key.
function warp_handle_dist_upgrade
  set -l source_file_name "$argv[1]"

  # The `apt-config shell` command outputs an environment variable assignment in POSIX-compliant syntax. Therefore,
  # we need to run this from within an sh shell to actually get the correct directory for the sources dir.
  set -l APT_SOURCESDIR (command sh -c 'eval $(apt-config shell APT_SOURCESDIR "Dir::Etc::sourceparts/d"); echo $APT_SOURCESDIR')

if not test -e $APT_SOURCESDIR$source_file_name.list; and not test -e $APT_SOURCESDIR$source_file_name.sources; and test -e $APT_SOURCESDIR$source_file_name.list.distUpgrade
      # DO NOT DO THIS. We should never run a command for user with `sudo`. The only reason this is safe here is because
      # we insert this function into the input for the user to determine if they want to execute (we never run it on
      # their behalf without their permission).  To be transparent about what is being executed with sudo, we echo out the
      # command we're about to run.
      builtin echo "Executing: sudo cp \"$APT_SOURCESDIR$source_file_name.list.distUpgrade\" \"$APT_SOURCESDIR$source_file_name.list\""
      sudo cp "$APT_SOURCESDIR$source_file_name.list.distUpgrade" "$APT_SOURCESDIR$source_file_name.list"
  end
end

# The SSH logic only applies to local sessions, because we don't yet have support for bootstrapping
# recursive SSH sessions.
if test "$WARP_IS_LOCAL_SHELL_SESSION" = "1"
    function is_interactive_ssh_session
        # Parse through all ssh options, as defined in the ssh man pages.  Send
        # stderr to /dev/null to silence argparse output when an option is invalid.
        argparse '1' '2' '4' '6' 'A' 'a' 'C' 'f' 'g' 'K' 'k' 'M' 'N' 'n' 'q' 's' 'T' 't' 'V' 'v' 'X' 'x' 'Y' 'y' 'b=' 'c=' 'D=' 'e=' 'F=' 'i=' 'J=' 'L=' 'l=' 'm=' 'O=' 'o=' 'p=' 'R=' 'S=' 'W=' 'w=' -- $argv 2>/dev/null
        # If argparse returned a non-success value, propagate it up.
        or return

        if [ $_flag_T ]
            # -T disables pty allocation (aka a non-interactive session)
            return 1
        end
        if [ $_flag_W ]
            # -W implies -T
            return 1
        end
        # #999: -O (control-master command), -N (no command), -f (background) and -V (version)
        # never start an interactive login, so the wrapper must not touch them. -G and -Q are
        # not declared above, so argparse already rejects them and they go to real ssh. Keep in
        # step with InteractiveSshCommand::parse_ssh_command (ssh/util.rs).
        #1003: -J <jump[,jump2]> is an option WITH an argument (it is in the optstring), so a jump-host login is
        # still wrapped. Every probe and the wrapped ssh get the original arguments, so `ssh -G`/`ssh -O check`
        # resolve the jump chain, and the multiplexed follow-up ssh calls reuse the master (and so its jump).
        # #1035: -w (tun device), -M (master) and -S (control socket) cannot ride the wrapper's
        # multiplexed session (it owns the master and the ControlPath), so they go to plain ssh too.
        if set -q _flag_O; or set -q _flag_N; or set -q _flag_f; or set -q _flag_V; or set -q _flag_w; or set -q _flag_M; or set -q _flag_S
            return 1
        end

        # If there is more than one positional argument, the user is attempting to
        # run a command, not start an interactive session.  If there is less than
        # one positional argument, the user should be shown the usage text.
        if [ (count $argv) -ne 1 ]
            return 1
        end
    end

    function warp_ssh_helper
        # #1172: the two InitShell hook encodings computed here were never read (forks before every ssh).
        # Mint the remote session's ID *here*, on the local (already
        # registered) side, from /dev/urandom -- 64 bits, unguessable. It is
        # handed to the remote shell as $WARP_SESSION_ID and reported to the
        # client in the SSH hook below, so the client can register it before
        # the remote shell can emit anything. Do NOT derive it from the clock
        # or a shell PRNG: it is the credential every subsequent DCS hook from
        # that session is validated against.
        set -l remote_session_id (command od -An -N8 -tu8 /dev/urandom 2>/dev/null | command tr -d ' \n')
        if test -z "$remote_session_id"; or test "$remote_session_id" = "0"
            # If we cannot generate a non-zero random token, run plain SSH instead.
            command ssh $argv
            return
        end

        # If the user's SSH config sets a RemoteCommand for this destination,
        # OpenSSH refuses to also run our bootstrap as a command-line remote
        # command, aborting with "Cannot execute command-line and remote
        # command." Warpification is structurally impossible there, so fall back
        # to plain SSH. `ssh -G` prints `remotecommand none` when unset.
        # #1172: ONE `ssh -G` per invocation (it was four, ~7 ms each). Every probe below -- RemoteCommand,
        # ControlPath, IPQoS, ServerAlive*, the probe-cache key -- reads this one capture.
        set -l warp_ssh_g (command ssh -G $argv 2>/dev/null)
        set -l user_remote_command (printf '%s\n' $warp_ssh_g | command sed -n 's/^remotecommand //p')
        if test -n "$user_remote_command"; and test "$user_remote_command" != "none"
            command ssh $argv
            return
        end

        # Hex-encode the ZSH environment script we use to bootstrap remote zsh b/c it contains control characters
        # We decode on the SSH server using xxd if its available, otherwise fall back to a for-loop over each byte
        # and use printf to convert back to plaintext
        set -l zsh_env_script (builtin printf '%s' 'unsetopt ZLE RCS GLOBAL_RCS; WARP_SESSION_ID='$remote_session_id'; WARP_USING_WINDOWS_CON_PTY=@@USING_CON_PTY_BOOLEAN@@; WARP_HONOR_PS1='$WARP_HONOR_PS1'; _hostname=$(command -pv hostname >/dev/null 2>&1 && command -p hostname 2>/dev/null || uname -n); _user=$(command -pv whoami >/dev/null 2>&1 && command -p whoami 2>/dev/null || builtin echo $USER); _ju=${${_user//\\\\/\\\\\\\\}//\\"/\\\\\\"}; _ju=${_ju//[[:cntrl:]]/}; _jh=${${_hostname//\\\\/\\\\\\\\}//\\"/\\\\\\"}; _jh=${_jh//[[:cntrl:]]/}; _msg=$(builtin printf "{\"hook\": \"InitShell\", \"value\": {\"session_id\": $WARP_SESSION_ID, \"shell\": \"zsh\", \"user\": \"%s\", \"hostname\": \"%s\"}}" "$_ju" "$_jh" | command -p od -An -v -tx1 | command -p tr -d " \n"); builtin printf '"'"'\x1b\x50\x24\x64%s\x9c'"'"' $_msg; unset _hostname _user _msg _ju _jh' | command od -An -v -tx1 | command tr -d ' \n')

        # Optionally attach to an existing ControlMaster the user already
        # runs for this destination instead of creating our own. Resolve
        # the user's configured ControlPath with `ssh -G` (which expands
        # tokens like %h/%p/%r/%C into a literal path), then verify the
        # master is alive with `ssh -O check`. Both probes are local-only
        # commands, and neither reads the user's ssh config directly --
        # `ssh -G` is ssh's own config resolver. On any failure we fall
        # back to creating a Warp-owned master, preserving the existing
        # behavior.
        set -l control_path "$SSH_SOCKET_DIR/$WARP_SESSION_ID"
        set -l control_master_mode "yes"
        set -l external_control_master "false"
        if test "$WARP_SSH_REUSE_CONTROL_MASTER" = "1"
            set -l user_control_path (printf '%s\n' $warp_ssh_g | command sed -n 's/^controlpath //p')
            # Skip when no ControlPath is configured, and reject resolved
            # paths containing characters we cannot safely embed in the SSH
            # hook JSON below (e.g. an unexpanded % token, quotes, or
            # whitespace); in those cases fall back to a Warp-owned master.
            if test -n "$user_control_path"
                and test "$user_control_path" != "none"
                and string match --quiet --regex '^[A-Za-z0-9._/~@:+,-]+$' -- "$user_control_path"
                # #1083: guarded (warp_ssh_mux_command): an -O check can never become a login.
                if warp_ssh_mux_command check "$user_control_path"
                    # A live master exists: multiplex through it and let the
                    # client know Warp does not own it.
                    set control_path "$user_control_path"
                    set control_master_mode "no"
                    set external_control_master "true"
                end
            end
        end

        # #985: pin both IPQoS classes to the interactive one (the remote-server-proxy channel
        # would otherwise mark the connection "bulk"), but ONLY when `ssh -G` reports exactly
        # OpenSSH's current default (`af21 cs1`): a user-configured IPQoS is never overridden,
        # and an older OpenSSH is left alone.
        set -l warp_ipqos_args
        set -l warp_ipqos_default (printf '%s\n' $warp_ssh_g | command sed -n 's/^ipqos //p')
        if test "$warp_ipqos_default" = "af21 cs1"
            set warp_ipqos_args -o "IPQoS=af21 af21"
        end
        # #1171: ssh's default ServerAliveInterval is 0, so a silently dead link (laptop sleep, wifi
        # handover) is never noticed while idle and takes ~15 min to notice with unacked data. Add
        # 30 s x 6 (180 s tolerance: a 2-minute sleep survives) ONLY when the SAME `ssh -G` output
        # reports interval 0, so any user config or command-line -o always wins. Opt out with the
        # warpify.ssh.keepalive setting (WARP_SSH_KEEPALIVE=0).
        if test "$WARP_SSH_KEEPALIVE" != "0"; and string match -q -- 'serveraliveinterval 0' $warp_ssh_g
            set -a warp_ipqos_args -o ServerAliveInterval=30 -o ServerAliveCountMax=6
        end
        # #1243: OpenSSH >= 9.5 sends keystrokes on a 20 ms tick (ObscureKeystrokeTiming). Turn it
        # off ONLY when the warpify.ssh.obscure_keystroke_timing setting says so
        # (WARP_SSH_OBSCURE_KEYSTROKE_TIMING=0) AND the same `ssh -G` output reports the default
        # `yes`: a user's own ssh config or command-line -o always wins, and an older OpenSSH
        # without the option is left alone.
        if test "$WARP_SSH_OBSCURE_KEYSTROKE_TIMING" = "0"; and string match -q -- 'obscurekeystroketiming yes' $warp_ssh_g
            set -a warp_ipqos_args -o ObscureKeystrokeTiming=no
        end

        # #1035: decide AFTER connecting, with ONE authentication. The bootstrap below is a POSIX
        # shell script, which a Windows OpenSSH server (default shell cmd.exe or PowerShell)
        # cannot run ("'export' is not recognized", then the connection closes). So first start
        # the Phosphor-owned ControlMaster in the background: the user's authentication (password,
        # key passphrase, host-key prompt) happens here, in the foreground terminal -- `-f` only
        # backgrounds ssh once it is authenticated -- and Ctrl-C aborts it like any ssh. Then
        # ask the remote what shell family it runs over that master (no second connection, no
        # second authentication) and start the session as a managed POSIX bootstrap or as a
        # plain interactive session. `ControlPersist` is only the safety net: a master that
        # nobody connects to (interrupted between steps, shell killed) exits by itself.
        # `ClearAllForwardings` keeps -L/-R/-D on the session client alone (it asks the master),
        # so a forward is not requested twice.
        set -l warp_owns_master 0
        if test "$control_master_mode" = "yes"
            # #1084: something is already at this pane's ControlPath. A DEAD socket (its master
            # was SIGKILLed) would make the new master print "ControlSocket ... already exists,
            # disabling multiplexing" and stay behind as a plain background connection for good,
            # so it is removed first. A LIVE master there belongs to another ssh from this shell
            # (e.g. a suspended one) and anything that is not a socket is not ours to delete: the
            # path is busy, so this ssh runs unwrapped instead.
            # ssh expands a leading ~ in ControlPath itself; the shell must do it for the tests.
            # $HOME matches the app (shellexpand::tilde in shutdown_for_discard).
            set -l warp_socket_file (string replace -r '^~' -- "$HOME" "$control_path")
            if test -e "$warp_socket_file"; or test -L "$warp_socket_file"
                warp_ssh_mux_command check "$control_path"
                set -l warp_check_rc $status
                if test $warp_check_rc -eq 255; and test -S "$warp_socket_file"; and not test -L "$warp_socket_file"
                    command rm -f -- "$warp_socket_file"
                end
                if test -e "$warp_socket_file"; or test -L "$warp_socket_file"
                    command ssh $argv
                    return
                end
            end
            command ssh $warp_ipqos_args -o ControlMaster=yes -o ControlPath="$control_path" -o ControlPersist=10 -o ClearAllForwardings=yes -N -f $argv
            set -l warp_master_rc $status
            if test $warp_master_rc -ne 0
                # #1551: the master (-N, authenticate-only) failed, so no remote command ever ran and a
                # plain ssh cannot double-execute. A Ctrl-C (rc > 128, not ssh's own 255) is respected.
                if test $warp_master_rc -gt 128; and test $warp_master_rc -ne 255
                    return $warp_master_rc
                end
                command ssh $argv
                return
            end
            # Own it only if a master really answers there. #1172: the socket we just waited for IS the proof: the
            # path was verified empty above, so a socket there can only be this master's, and `-f` returns only
            # after it listens. That saves the `-O check` helper (five processes) on the critical path.
            # WARP_SSH_VERIFY_MASTER=1 restores the answer-from-the-master check.
            set -l warp_socket_now (string replace -r '^~' -- "$HOME" "$control_path")
            if begin; test -S "$warp_socket_now"; and not test -L "$warp_socket_now"; and begin; test "$WARP_SSH_VERIFY_MASTER" != "1"; or warp_ssh_mux_command check "$control_path"; end; end
                set warp_owns_master 1
                set control_master_mode no
            end
            # No socket (e.g. ControlPath too long): fall through to the single-connection
            # managed session below; the idle background master expires via ControlPersist.
        end
        if test $warp_owns_master -eq 1; or test "$external_control_master" = "true"
            set -g __warp_ssh_g $warp_ssh_g
            if warp_ssh_remote_is_non_posix_cached "$control_path" $argv
                command ssh -o ControlMaster=no -o ControlPath="$control_path" -t $argv
                set -l warp_plain_rc $status
                if test $warp_owns_master -eq 1
                    warp_ssh_stop_master "$control_path"
                end
                return $warp_plain_rc
            end
        end

        # Note that in this command, we're passing a string to the remote shell. Any variable expansions need to be
        # escaped with "''" to avoid the local shell from expanding them before they're passed to the remote shell.
        # We check the SHELL env var and use shell string manipulation to get the contents after the last slash to
        # determine what shell is the login shell on the remote machine.  We perform a preliminary check to see if
        # the remote shell is the Bourne shell to avoid asking it to parse later lines that use syntax it doesn't
        # support.
        # The remote script goes through a variable, not the ssh argument list: `jobs` shows the
        # command as written, and the whole script in it was dumped into the scrollback (#995).
        set -l warp_remote_bootstrap "
export TERM_PROGRAM='WarpTerminal'
test -n '$WARP_CLIENT_VERSION' && export WARP_CLIENT_VERSION='$WARP_CLIENT_VERSION'
# Only forward the protocol version if it was set locally (i.e. the HOANotifications feature flag is on).
test -n '$WARP_CLI_AGENT_PROTOCOL_VERSION' && export WARP_CLI_AGENT_PROTOCOL_VERSION='$WARP_CLI_AGENT_PROTOCOL_VERSION'
warp_rb=false
if test "'"${SHELL##*/}" = "bash"'" && printf x | command -p head -c 1 >/dev/null 2>&1; then warp_rb=true; fi
hook="'$(printf "{\"hook\": \"SSH\", \"value\": {\"socket_path\": \"'$control_path'\", \"remote_shell\": \"%s\", \"session_id\": '"$WARP_SESSION_ID"', \"remote_session_id\": '"$remote_session_id"', \"external_control_master\": '"$external_control_master"', \"raw_bootstrap\": %s}}" "${SHELL##*/}" "$warp_rb" | command od -An -v -tx1 | command tr -d " \n")'"
printf '$DCS_START$DCS_JSON_MARKER%s$DCS_END' "'$hook'"

if test "'"${SHELL##*/}" != "bash" -a "${SHELL##*/}" != "zsh"'"; then
  # Emulate the SSHD logic to print the MotD. Because the Warp SSH wrapper passes
  # a command to run, SSHD does a quiet login, updating utmp and other login
  # state, but not printing the MotD. For bash and zsh, this is instead handled
  # by our bootstrap script.
  if test ! -e "'$HOME/.hushlogin'"; then
    # This uses an if-else chain instead of a for-loop to avoid expansion issues on older shells.
    if test -r /etc/motd; then
      cat /etc/motd
    elif test -r /run/motd; then
      cat /run/motd
    elif test -r /run/motd.dynamic; then
      cat /run/motd.dynamic
    elif test -r /usr/lib/motd; then
      cat /usr/lib/motd
    elif test -r /usr/lib/motd.dynamic; then
      cat /usr/lib/motd.dynamic
    fi
  fi
  # Likewise, emulate a login shell by sourcing /etc/profile
  if test -r /etc/profile; then
    . /etc/profile
  fi
  exec "'$SHELL'"
fi

case "'${SHELL##*/}'" in
bash)
    exec -a bash bash --rcfile <(echo '"'
      stty raw
      HISTCONTROL=ignorespace
      HISTIGNORE=" *"
      WARP_SESSION_ID='$remote_session_id'
      WARP_HONOR_PS1="'$WARP_HONOR_PS1'"
      _hostname=$(command -pv hostname >/dev/null 2>&1 && command -p hostname 2>/dev/null || uname -n)
      _user=$(command -pv whoami >/dev/null 2>&1 && command -p whoami 2>/dev/null || echo $USER)
      _ju=${_user//\\\\/\\\\\\\\}; _ju=${_ju//\\"/\\\\\\"}; _jh=${_hostname//\\\\/\\\\\\\\}; _jh=${_jh//\\"/\\\\\\"}; for _i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 127; do printf -v _o %03o $_i; printf -v _c "\\\\$_o"; printf -v _h "\\\\\\\\u%04x" $_i; _ju=${_ju//$_c/$_h}; _jh=${_jh//$_c/$_h}; done
      _msg=$(printf "{\"hook\": \"InitShell\", \"value\": {\"session_id\": $WARP_SESSION_ID, \"shell\": \"bash\", \"user\": \"%s\", \"hostname\": \"%s\"}}" "$_ju" "$_jh" | command -p od -An -v -tx1 | command -p tr -d " \n")'"
      WARP_USING_WINDOWS_CON_PTY=@@USING_CON_PTY_BOOLEAN@@
      if [[ "'$OS'" == Windows_NT ]]; then WARP_IN_MSYS2=true; else WARP_IN_MSYS2=false; fi
      printf '\''"'\eP$d%s\x9c'"'\'' \""'$_msg'"\"'
      unset _hostname _user _msg _ju _jh _i _o _c _h
    )
    ;;
zsh) WARP_TMP_DIR="'$(mktemp -d warptmp.XXXXXX)'"
local ZSH_ENV_SCRIPT='$zsh_env_script'
if [[ "'$?'" == 0 ]]; then
  if command -v xxd >/dev/null 2>&1; then
    echo "'$ZSH_ENV_SCRIPT'" | command xxd -p -r > "'$WARP_TMP_DIR'"/.zshenv
  else
    for i in {0..\$((\${#ZSH_ENV_SCRIPT} - 1))..2}; do
      builtin printf "'"\x${ZSH_ENV_SCRIPT:$i:2}"'"
    done > "'$WARP_TMP_DIR'"/.zshenv
  fi
else
  echo \"Failed to bootstrap warp. Continuing with a non-bootstrapped shell.\"
fi
TMPPREFIX="'$HOME/.zshtmp-'" WARP_SSH_RCFILES="'${ZDOTDIR:-$HOME}'" ZDOTDIR="'$WARP_TMP_DIR'" exec -l zsh -g $TRACE_FLAG_IF_WARP_SHELL_DEBUG_MODE
    ;;
esac
"
        command ssh $warp_ipqos_args -o ControlMaster=$control_master_mode -o ControlPath="$control_path" \
        -t $argv $warp_remote_bootstrap
        set -l warp_session_rc $status
        # The session is over: take the master down with it. #1083: this is the ONLY `-O exit`
        # in the normal exit path (the app's ExitShell hook no longer sends one; the app stops the
        # master only when the pane is discarded and this code cannot run).
        if test $warp_owns_master -eq 1
            warp_ssh_stop_master "$control_path"
        end
        return $warp_session_rc
    end

    # #1035: succeeds when the remote answers like a non-POSIX shell (cmd.exe or PowerShell),
    # asked over the already-authenticated master at $argv[1] (no new connection). POSIX shells
    # print the literal %COMSPEC% and an empty $PSVersionTable; cmd.exe expands %COMSPEC% and
    # leaves $PSVersionTable; PowerShell leaves %COMSPEC% and expands $PSVersionTable. Anything
    # else (timeout after 5 s, failure, csh/fish errors) is inconclusive and fails, i.e. the
    # managed session as before.
    function warp_ssh_remote_is_non_posix
        set -l control_path $argv[1]
        set -e argv[1]
        set -l probe_out (command mktemp 2>/dev/null)
        or return 1
        command ssh -o BatchMode=yes -o ControlMaster=no -o ControlPath="$control_path" $argv -T 'echo warp-probe-%COMSPEC%-$PSVersionTable' </dev/null >$probe_out 2>/dev/null &
        set -l probe_pid (jobs --last --pid)
        for i in (seq 1 50)
            if not kill -0 $probe_pid 2>/dev/null
                break
            end
            command sleep 0.1
        end
        kill $probe_pid 2>/dev/null
        set -l out (command cat $probe_out 2>/dev/null | string collect)
        command rm -f $probe_out
        # #1136: record a DEFINITIVE answer for warp_ssh_remote_is_non_posix_cached; anything else
        # leaves the verdict empty (inconclusive, never cached).
        set -g warp_ssh_probe_verdict ""
        if string match -q -r -i -- 'Hashtable|cmd\.exe-\$PSVersionTable' "$out"
            set -g warp_ssh_probe_verdict nonposix
            return 0
        end
        if test "$out" = 'warp-probe-%COMSPEC%-'
            set -g warp_ssh_probe_verdict posix
        end
        return 1
    end

    # #1136: the probe above costs a channel on the master plus a remote shell start on every
    # connect. Its answer for a host only changes if the machine behind the name is replaced, so a
    # DEFINITIVE answer is remembered for ten minutes (from when it was measured), keyed by what
    # `ssh -G` resolves the destination to (user, hostname, port). Only the exact POSIX reply or
    # a cmd.exe/PowerShell reply is remembered; a timeout, a failure or any other output is
    # inconclusive, is never cached and is probed again next time. Same arguments and return
    # values as warp_ssh_remote_is_non_posix.
    function warp_ssh_remote_is_non_posix_cached
        set -l control_path $argv[1]
        set -e argv[1]
        # #1172: reuse the wrapper's single `ssh -G` capture (handed over in the global
        # __warp_ssh_g, read once and erased) when there is one; a direct caller without it asks ssh.
        set -l key_g
        if set -q __warp_ssh_g
            set key_g $__warp_ssh_g
            set -e __warp_ssh_g
        end
        if test (count $key_g) -eq 0
            set key_g (command ssh -G $argv 2>/dev/null)
        end
        set -l key (printf '%s\n' $key_g | command awk '$1=="user"||$1=="hostname"||$1=="port"{printf "%s ", $0}' | string collect)
        if test -z "$key"
            warp_ssh_remote_is_non_posix "$control_path" $argv
            return
        end
        set -l now (command date +%s)
        set -l kept
        set -l verdict ""
        for entry in $warp_ssh_probe_cache
            set -l parts (string split -r -m 2 '|' -- $entry)
            test (count $parts) -eq 3; or continue
            string match -q -r '^[0-9]+$' -- $parts[3]; or continue
            if test $now -ge $parts[3]; and test (math $now - $parts[3]) -lt 600
                set -a kept $entry
                if test "$parts[1]" = "$key"
                    set verdict $parts[2]
                end
            end
        end
        set -g warp_ssh_probe_cache $kept
        switch "$verdict"
            case nonposix
                return 0
            case posix
                return 1
        end
        warp_ssh_remote_is_non_posix "$control_path" $argv
        set -l probe_rc $status
        if test -n "$warp_ssh_probe_verdict"
            set -ga warp_ssh_probe_cache "$key|$warp_ssh_probe_verdict|$now"
        end
        return $probe_rc
    end

    # Ends the Phosphor-owned master at $argv[1].
    function warp_ssh_stop_master
        warp_ssh_mux_command exit $argv[1]
        return 0
    end

    # #1083: runs `ssh -O $argv[1]` (check / exit) against the master at ControlPath $argv[2] so that
    # it can NEVER fall back to a real connection. OpenSSH falls through from a failed mux hello (a
    # master that is shutting down) to a full login, which with the pane's PTY as stdin ran the
    # user's later keystrokes on the remote. Guards: -F /dev/null (no user config, so no
    # ControlMaster=yes / ProxyJump; the user's arguments are deliberately NOT passed, -O only needs
    # the literal ControlPath), ControlMaster=no, ProxyCommand=false (any fallback "connects" to a
    # local `false` run by /bin/sh: no DNS, no TCP), BatchMode=yes (no prompt), -n and stdin
    # /dev/null (never the PTY), output discarded, and at most 5 s. Keep in step with mux_control_args in
    # crates/remote_server/src/ssh.rs and the bash/zsh copies. Status: 0 = a live master answered,
    # 255 = no master (missing / dead socket), anything else = unknown (timed out).
    function warp_ssh_mux_command
        command sh -c '
            SHELL=/bin/sh ssh -F /dev/null -o ControlMaster=no -o ControlPath="$2" -o ProxyCommand=false -o BatchMode=yes -o ConnectTimeout=5 -o ClearAllForwardings=yes -n -q -O "$1" phosphor-mux-control </dev/null >/dev/null 2>&1 &
            p=$!
            (
                trap "kill \$s; exit 0" TERM
                sleep 5 &
                s=$!
                wait "$s"
                kill "$p"
            ) </dev/null >/dev/null 2>&1 &
            k=$!
            wait "$p"
            r=$?
            kill "$k" >/dev/null 2>&1
            exit "$r"
        ' warp-ssh-mux "$argv[1]" "$argv[2]" </dev/null >/dev/null 2>&1
    end

    function ssh
        if is_interactive_ssh_session $argv
            warp_send_json_message "{\"hook\": \"PreInteractiveSSHSession\", \"value\": {\"session_id\": $WARP_SESSION_ID}}"

            if [ "$WARP_USE_SSH_WRAPPER" = "1" ]
                if test $WARP_SHELL_DEBUG_MODE
                    set -g TRACE_FLAG_IF_WARP_SHELL_DEBUG_MODE "-x"
                else
                    set -g TRACE_FLAG_IF_WARP_SHELL_DEBUG_MODE ""
                end
                warp_ssh_helper $argv
            else
                command ssh $argv
            end
        else
            command ssh $argv
        end
    end
end

warp_precmd

# Print the MotD if this is a login shell. Normally, login(1) or pam_motd(8)
# would do this. However, Warp does not use login(1) for local sessions and for
# remote sessions, SSHD thinks it is starting a non-interactive session, so it
# does not print PAM messages.
if status --is-login
  and test ! -e "$HOME/.hushlogin"
  for motd_file in /etc/motd /run/motd /run/motd.dynamic /usr/lib/motd /usr/lib/motd.dynamic;
    if test -r "$motd_file"
      command cat "$motd_file"
      break
    end
  end
end

warp_bootstrapped

set -g WARP_BOOTSTRAPPED 1
set -g fish_private_mode $saved_fish_private_mode
end
