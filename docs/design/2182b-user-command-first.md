# #2182 (b): a submitted command is never dropped behind the app's own side calls (design, UNREVIEWED, no code)

Owner direction 2026-11-14 ("a"): a command the user submits is always written to the pty ahead
of, or between, the app's own in-band side calls; never dropped, never left in the editor; no
settle timer. Written by reading the code at `0e7de51ef` and the r30f Windows lab log; nothing
was compiled or run. Paths are relative to `app/src/terminal`; `pc` =
`writeable_pty/pty_controller.rs`.

## Root cause

r30f, `terminal@gitbash/case_lab_connect_typeahead.bash.lan.helper.log`, times in ms:

| t | line | what |
|---|---|---|
| 308 | 2452 | the app writes its side call `warp_run_generator_command ... git diff --shortstat` |
| 442 | 2533-2538 | the user's `exit`: `try_execute_done executed=true`, `hop=write_command`, and no `pty_write_bytes` ever |
| 464 | 2557 | the side call's Preexec (156 ms after its write: a remote shell) |
| 481-487 | 2578-2593 | its Precmd; the NEXT side call (`kubectl config current-context`) is written |

`PtyController::write_command` (`pc:2609`) asks the model to start the command's block
(`pc:2633`). The block lifecycle answers from its phase (`model/lifecycle/transition.rs:242-247`):
`Submitted` -> `CoalescedStart`, `Executing` -> `RejectedExecuting`. The app's own in-band
command occupies the active block from its write to its Precmd, so in that window every user
submit is refused, and `write_command` returns at `pc:2634-2645`: the command is not queued,
not written, and nothing is logged. The input does not know: `Input::can_execute_command`
(`input.rs`, "in-band commands are okay") lets the submit through, logs `executed=true` and
keeps the text as the submission that a block's completion will clear. No block completes, so
the editor keeps `exit` and the next line is typed onto it.

The window is one round trip of the side call (write to Preexec) plus its run time: wide on a
remote shell, which is why the lab found it and the local cases did not.

## The change

In `write_command`, when the start is refused AND the active block is an in-band command's AND
the source is the user's (`User`, `QueuedCommand`, `EnvVarCollection`):

1. **Park the command at the FRONT of `pending_writes`** (`pc:384`), ahead of every side call
   that is queued and not yet written. Nothing queued is cancelled. `is_user_command_executing`
   is set, so no new side call is accepted meanwhile (`queue_in_band_command`, `pc:1522`,
   already cancels a request made while a user command is pending).
2. **Its block starts when it is written**, in the write's `before_write_fn`, as a command held
   behind the user's line already does (`pc:2880-2897`): start the block, then the hand-off
   bookkeeping `write_command` does today for an accepted start (`pc:2656-2686`:
   `on_command_written` and the command's text when the shell reports its line, else the
   unengaged pair). That bookkeeping moves into one function both paths call.
3. **It is written by the event that already drains the queue:** the line editor becoming
   active at the side call's prompt (`execute_next_queued_write`, `pc:1870`). So the order at
   the pty is: the side call that was already running, the user's command, then the side calls
   that were queued. "Between", as directed; no timer.
4. **No `TypeaheadHandoffNotEngaged` for a parked command** (`pc:2642`). That event releases the
   keys typed behind the Enter because "nothing will take the command"; here something will.
   They stay behind the Enter's gate until the write emits `TypeaheadHandoffChanged`, as for any
   command that waits for the line editor.
5. **A marker on the queued write.** Today "a command that is not in-band and has a
   `before_write_fn`" means an agent command held behind the user's typeahead
   (`is_held_handoff_command`); the parked user command must not be taken for one (it would
   wait on the hand-off's holds and clear the held-agent record). `PtyWrite::Command` gets an
   explicit kind.
6. **If the start is still refused at the write** (it should not be: the shell is at a prompt),
   the command goes back to the front and waits for the next prompt, with a log line. It is
   dropped only when the shell exits, as every queued write is.

Every other refusal (a user command really running, a terminated shell) is unchanged, but it is
no longer silent: `write_command` logs `write_command_refused source= outcome= in_band_block=`.

## With #2209 / #2227

- The editor side is already right once the command is written: the submission is recorded at
  the submit and the block's completion clears it (#2223's `SubmittedPrefix` is set the same
  way). No change in `input.rs`.
- #2223's declined-Enter path does not fire here (an in-band block is not "a command is
  running" for `can_execute_command`).
- The in-band wait of #2182 (a side call waits while the user's keys are held) is untouched;
  this is the other half: the user's COMMAND against a side call already written.
- #2227's hand-over and the #2209 holds are not on this path.

## Tests

- `pty_controller_tests.rs`: a user command submitted while an in-band command's block is
  `Submitted` (before its Preexec) and while it is `Executing`: not written yet, parked at the
  front; at the next line-editor-active event it is written BEFORE a side call that was queued
  earlier; that side call is written after it and was not cancelled; the hand-off saw
  `on_command_written` at the write, not at the submit; no `TypeaheadHandoffNotEngaged`.
- A side call requested while the command is parked is refused (cancelled to its requester).
- The shell exits while parked: the command is dropped with the queue, with a log line.
- A user command over a REAL running command is still refused, now with the log line.
- View (`input_test.rs`): the r30f sequence end to end: side call written, `exit` + Enter
  before its Preexec, then its Preexec and Precmd: `exit` reaches the pty once, the editor is
  empty after the block completes, and a line typed next is not glued to `exit`.
- Lab, Windows lab server, terminal AND agent mode at Git Bash:
  `lab_connect_typeahead.bash.{lan,150}.{helper,nohelper}`, `lab_chips.bash.lan.nohelper`; and
  the same rows on the Linux and macOS lab servers. Pass criteria beyond the case's own: every
  `hop=write_command` is followed by a `pty_write_bytes class=UserCommand` for it, zero
  `write_command_refused` with a user source, and no `enter_begin` whose buffer starts with the
  previous submitted command.

## Risk

- A slow side call delays the user's command by its remaining run time (it cannot be
  un-written). Side calls already have their own time-outs; this adds none.
- The order "user command before the queued side calls" changes which prompt those side calls
  run at; they are stateless queries and their requesters already handle a cancel.

## Owner question

If one of the app's own background commands is already running in the shell when you press
Enter, your command runs as soon as that one finishes (typically well under a second; longer
only if that background command is slow). The alternative is to interrupt the background
command so yours runs at once. Recommended: wait; interrupting sends Ctrl-C into your shell.
