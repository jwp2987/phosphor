# Round 14: the reader marker (#2052), bash and zsh

Branch `fix/r14-readmark`, from `integ/r14` 4bf571aae. Implements `docs/design/r13-secretui.md`
section 2 after the owner's decision of 2026-11-10: GO for bash and zsh, fish NOT in the first cut,
ONE kill switch name. Nothing here was compiled; Rust is `rustfmt --check` only. The shell side was
run here (bash 5.3.9, zsh 5.9) with `script/test_reader_marker`; bash 3.2 was not (none on this
machine). Wording: "worked, unproven" until an app run shows it.

## Cause (APP gap, by reading; `r13-secretui.md` section 2 has the long form)

A precmd function that runs after the prompt's Precmd asks with echo on (`read -p "continue? " x`).
On Linux and macOS the Precmd has already handed input to the editor
(`TypeaheadHandoff::try_commit_at_prompt`, `typeahead_handoff.rs`; the prompt hold that defers it
is ConPTY only). The answer is typed into the editor; Enter runs it as a command or sends it to the
agent, and the `read` keeps waiting. #2050's fact (canonical, echo off: `tty_is_reading_secret`)
does not see it: an echoing read leaves the tty as a shell between its Precmd and its line editor
has it. Only the shell knows the moment of the read.

## Fix

### Shell side (`bash_body.sh`, `zsh_body.sh`)

`warp_precmd` ends by opening the "user stage" (`__warp_reader_stage_open`) and
`warp_update_prompt_vars` begins by closing it (`__warp_reader_stage_close`). While it is open
`read` is a function:

    if fd 0 is a terminal and the options say this read is of fd 0:
        send ESC ] 9278 ; f ; {"hook": "TtyReaderAsks", "value": {"session_id": S, "seq": N}} BEL
    builtin read "$@"

(bash: `__warp_reader_announce` is the write; zsh: the body is set through `functions[read]` and
starts on its own line, because `script/check_input_path_timers` reads `read ... -t ` on one line
as a timed read and the `-t 0` there is the terminal test.)

Same OSC form, session id and `seq` as `LineEditorReading`, written with `__warp_emit` (to
`/dev/tty`), so it is in the stream after the Precmd and ahead of the question's bytes.

- **No local, no assignment, builtin last** in the function, so the caller's locals, `REPLY`, `-a`
  / `-A` arrays, an `IFS=: read` prefix and the status (EOF, timeout) are the builtin's. The write
  is `|| :` so the caller's `set -e` / ERR trap / `err_exit` cannot trip on it.
- **A bare `read` (bash).** Before bash 4.4 `"$@"` with no arguments is an unbound variable under
  the caller's `set -u`; in an interactive shell that aborts the whole command. bash 3.2 (macOS)
  is such a bash, so the function has a branch for `$# -eq 0` that never expands `"$@"`.
- **What is announced.** A read of fd 0 while fd 0 is a tty. Not announced: a file, pipe or
  here-string (fd 0 is not a tty), `-u N` for N != 0, the poll `read -t 0` (zsh: `-t` with no
  number), zsh `-p` (coprocess), `-z`, `-c`, `-l`. The options are looked at by
  `__warp_read_is_of_fd0`, called only when fd 0 is a tty; its locals are gone before the builtin
  runs. bash: `builtin getopts` with the builtin's own option string. zsh: a small loop that knows
  `-t` / `-k` take an optional number and `-d` / `-u` an argument.
  This is an addition to the r13 sketch, which announced on `-t 0` alone and accepted `-u`. Reason:
  gitstatus / powerlevel10k / zsh-async installed lazily (after the bootstrap) run inside the stage
  and call `read -u $fd` with a tty on fd 0 at EVERY prompt. Announcing those would take input from
  the editor and move focus at every prompt of a common zsh setup.
- **Never installed:** `PHOSPHOR_READER_MARKER=0`; before the bootstrap is done
  (`WARP_BOOTSTRAPPED` unset: the bootstrap calls `warp_precmd` itself long before its end, and its
  own code and the rc files run after that call); over a `read` function of the user's; bash in
  POSIX mode (an assignment in front of a function call can outlive the call on older bash).
- **Aliases.** bash defines it as `function read { ... }` (the form the bootstrap already uses for
  `ssh`): `read () { ... }` would expand a user's `read` alias when the file is parsed, in a
  subshell bootstrap where the rc files ran first. zsh defines it through `functions[read]=...`.
- **A stage that is never closed.** A Ctrl-C at the question (bash abandons the rest of
  `PROMPT_COMMAND`, zsh the rest of the precmd hooks) or a failing zsh precmd function skips
  `warp_update_prompt_vars`. Safety nets, one test each: bash closes in `__warp_preexec_impl`, zsh
  in `_warp_zle_line_init`; both close before they open. So the function never outlives the prompt
  it was installed for by more than the line being edited (bash) or at all (zsh).
- In-band generator cycles: `warp_precmd` returns before the open on that path, as designed.
- Cost: per prompt a handful of `[[ ]]` tests, one `declare -F` and one function definition; per
  announced read one `getopts` loop and one `printf`. No fork (`script/test_hook_forks` is green).

### App side

- Parser: `DProtoHook::TtyReaderAsks { value: LineEditorMarkerValue }` (`dcs_hooks.rs`), accepted
  unencoded like the line-editor markers (`ansi/mod.rs`), session-validated like every hook.
  `Handler::tty_reader_asks` (`handler.rs`), `TerminalModel::tty_reader_asks`
  (`terminal_model.rs`): on the parser thread, in stream order, no terminal event.
- Hand-off (`typeahead_handoff.rs`): `on_tty_reader_asks(session, seq)`, the third source of the one
  reader fact. Honoured only when the kill switch is on, the marker's session and `seq` ARE the
  last Precmd's (not older, not newer, not absent) and `reader_ahead_may_be_noted()` holds: the
  conditions `secret_reader_look_due` already had, minus "the platform samples the tty" (activity
  `Prompt`, no reader noted, no hold queued, no deferred Enter, editor or pty owns input outright).
  Then `take_input_for_reader_ahead("shell_marker")`, the body of `take_input_for_secret_reader`
  factored out unchanged: `PtyOwned { false, 1, 1 }` when the editor had input, keys-follow-editor
  ended, `prompt_reader_asks = true`. Log `hop=prompt_reader_asks by=shell_marker
  took_from_editor=..`; a refused marker logs `hop=tty_reader_marker_ignored why=..`.
  `prompt_reader_by_marker` makes `secret_reader_has_the_keys()` true on every platform (the pane's
  sink then routes the keys to the terminal whatever view has focus). `marker_reader_news` is the
  edge the view takes once. Both are cleared wherever the reader ends.
- View (`view/paste_queue.rs`, `hand_keys_to_a_secret_reader`): at the two sites it already runs
  (terminal wakeup, a key at the pane's sink) it first takes that edge and does the view's part of
  the transition (queue pin, focus to the terminal), log `hop=marker_reader_takes_input site=..`.
  One extra read of the hand-off on other wakeups.
- Nothing else changes: routing (`route_prompt_reader_bytes`), the end events (line editor's
  signal, Preexec, next Precmd, new session), the unread-keys rule, the retry in
  `pty_controller.rs::on_line_editor_reading_marker`, and #2053's visibility
  (`show_question_of_tty_reader` reads `prompt_reader_asks`) are the #2038 / #2050 code.
- ConPTY (Git Bash sends the same marker): with no hold queued the reader is noted at the
  question's output instead of at the first key, also for a `read` that prints no question. With
  keys already held the marker is ignored (`why=hold_queued`) and the hold's cap decides from the
  screen, as before.

### The one kill switch: `PHOSPHOR_READER_MARKER=0`

Shell side: the bootstraps read it from their environment (inherited from the app): no function is
defined, no marker is sent, the files behave as before. App side:
`typeahead_handoff::reader_marker_env_enabled()` (same name), read once per pane into the hand-off:
a marker that still arrives (a remote shell whose environment the app does not set) is parsed and
ignored.

## Rejected

- **Announcing on `-t 0` alone** (the r13 sketch): the p10k / gitstatus `read -u` case above.
- **Closing the stage at the line editor's start instead of in `warp_update_prompt_vars`:** bash has
  no hook there, and it would leave the function defined for completion code.
- **Keeping the stage open for functions that run after `warp_update_prompt_vars`** (see Limits):
  bash sends `LineEditorReading` from `warp_update_prompt_vars` itself in Warp-prompt mode, so the
  app would refuse the marker anyway; and the function would stay through the line being edited.
- **A new terminal event for the marker:** the hand-off holds the fact and the view already looks
  at the wakeup the marker's own bytes cause.
- **Two kill switch names:** owner decision.
- **Building bash 3.2 here to run the shell test:** downloading and building third-party code on
  this machine was not asked for. `script/test_reader_marker --bash /bin/bash` on the Mac box is
  the proof.

## Limits (known, not fixed here)

- **The first prompt after a function is registered.** Both dispatchers iterate a copy of
  `precmd_functions` (bash-preexec's `for ... in "${precmd_functions[@]}"`, zsh's `callhookfunc`),
  and `warp_update_prompt_vars` re-appends itself last only when it runs. A function appended
  after the bootstrap therefore runs AFTER `warp_update_prompt_vars` at the first prompt after its
  registration, outside the stage: a question at that one prompt is not announced (#2052 as
  before). From the second prompt on it is inside. bash `PROMPT_COMMAND` from the rc files is
  inside from the first prompt.
- Functions the rc files registered run BEFORE the Precmd and never needed this.
- An external program asking with echo on (`python -c 'input()'`), `select`, zsh `read -k` / `-q`
  when fd 0 is not the terminal, `vared`: not announced.
- A `read` function the user defines DURING the stage would be removed at its close (the close
  cannot tell without a fork). Not seen in practice.
- `set -x` users see the function's lines in their trace during the stage.
- fish: not in this cut (owner decision).

## Risk

- **Every pane loads these files.** With the switch at `0` nothing is defined. The function
  exists only between the app's two precmd functions (nets above). `script/test_reader_marker`
  pins the semantics in the real assembled bootstraps; the existing bootstrap scripts
  (`test_init_shell_escaping`, `test_hijack_bootstrap`, `test_bash_bootstrap_nounset`,
  `test_zsh_bootstrap_strict_options`, `test_hook_forks`, `test_child_ps1`,
  `test_line_editor_markers`) were run against the changed files.
- **bash 3.2 (macOS), read not run.** Everything used exists in 3.2 (`function name {}`,
  `builtin getopts` with a local `OPTIND`, `declare -F`, `[[ -t 0 ]]`), and the one 3.2 trap found
  by reading (an empty `"$@"` under `set -u`) has its own branch and its own check in the script. Known 3.2 quirk: a local
  `OPTIND` is not fully unwound for a caller that is itself in the middle of a `getopts` loop and
  calls `read` on the terminal inside the stage. `-i` / `-N` in the option string do not exist in
  3.2's `read`; the builtin rejects them itself.
- **A false announcement** (a tool reading fd 0 as a tty at every prompt for something that is not
  a question): input goes to the pty until the line editor's marker, milliseconds later; keys typed
  in that window are the shell's line and are requested into the editor (the unread-keys rule). The
  kill switch is the answer if a setup does that.
- **A forged marker** (a program printing the OSC): honoured only at a prompt, for the current
  session and `seq`, and undone by the next line-editor signal. It can move input to the pty for
  one prompt; it cannot make the app run or send anything.
- **App side:** the transition is #2050's with another trigger, taken on the parser thread
  instead of the main thread (as `on_preexec` already changes the owner there). The view's part
  follows at the next wakeup or key; a key that arrives in between is routed by
  `secret_reader_has_the_keys()`, which is already true.

## Tests

- `script/test_reader_marker` (new; bash and zsh, honor PS1 on and off): see its docstring.
- `ansi/mod_test.rs`: `parse_osc_tty_reader_asks_carries_session_and_sequence_number`.
- `terminal_model_test.rs`: `a_tty_reader_marker_notes_the_reader_in_the_handoff_in_stream_order`.
- `typeahead_handoff_tests.rs`: eight tests under "#2052" (takes input and every key; no tty
  sample needed; only for the prompt the app is at, kill switch included; the switch's values;
  parsed before the Precmd commits; ends on events and unread keys; keys-follow-editor; ConPTY with
  and without a hold).
- `view_test.rs`: `a_precmd_reads_marker_takes_the_keys_from_the_focused_editor` (Legacy and Table,
  control arm = no marker), `a_precmd_reads_marker_is_found_at_the_wakeup_of_its_question`.
- `fuzz/fuzz_targets/typeahead_handoff.rs`: event 28 = the marker and the view's look.
- appci `precmd_question_echo` (PROVING, both modes): see `docs/testing.md`.

## What the next run should show

`precmd_question_echo`, both arms, both modes, Linux and macOS: the answer file holds the answer,
no `enter_begin` with the answer in the editor, no command with the answer, no model request with
it, the question on screen, the next line runs. App log after the trigger:

    hop=prompt_reader_asks by=shell_marker took_from_editor=true
    hop=marker_reader_takes_input site=output took_from_editor=true     (site=key is a pass too)
    hop=hand_route reason=prompt_reader ...                              (one per key)
    hop=prompt_reader_end why=line_editor_signal

`hop=tty_reader_marker_ignored why=...` instead of the first line names why the marker was refused
(`not_this_prompt`: seq or session; `not_at_prompt`: the line editor's marker was parsed first).
No `TtyReaderAsks` in the stream at all means the function was not in the stage (the Limits above)
or the wrapper was not installed (POSIX mode, a user `read`, the switch).
