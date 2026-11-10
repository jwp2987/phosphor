# r10: ssh_nested_typeahead on Linux Wayland (round 9b) (#2013)

Nothing here was compiled or run. Verdicts come from the round 9b artifacts (build/r18w9b
413cd4241, job app-test-linux-wayland) and from reading the code.

## Verdict

| case | mode | round 9b | verdict |
|---|---|---|---|
| `ssh_nested_typeahead` | terminal | FAIL | **APP bug**, #2013 (new). A race, not a steady regression. |
| `ssh_nested_typeahead` | agent | pass (`exact=1`) in the same run | same code path; the race went the other way |

History (`test-results/*/app/wayland/results.json`): Wayland passed in rounds 5, 7 and 8 in both
modes (`handovers` = `precmd@16` then `finish@7..10`). Round 3 failed with another shape (#1823,
closed, verified in round 5). Round 9b terminal is the first run with `precmd@16` then `output@7`
and a hand-back. So it is new in 9b and intermittent.

It is not the lab sshd and not the case: sshd served every session (`ssh client probe exit 0`,
forced-command sessions started), the inner ssh connected and gave a shell (the screenshot shows
its `~$` prompt and `31_jnxirw: command not found`). The case's expectation (the line runs exactly
once, whole, on the remote) is right; the app split the line.

## The failing step, with hops

Terminal log `case_ssh_nested_typeahead.log`, times are the last five digits of `t=`:

1. `18359.1` `enter_begin buffer="ssh fake"`, `ssh_pty_owner ssh=true site=ssh_submit`: first ssh.
   The next 16 keys (`ssh fake`, Enter, `echo T3`) are held (`kh_iq_hold queue=Q7_ssh_typeahead`).
2. `18683.2` `ssh_gate_released how=first_prompt`, `kh_handover source=precmd held_bytes=16`: at
   the first remote prompt the held `ssh fake` + Enter is submitted through the editor
   (`enter_begin buffer="ssh fake"`, `ssh_pty_owner ssh=true site=ssh_submit` at `18684.07`), and
   the remaining 7 keys are held for the second connect (`gate_open kind=ssh_connect key=17` at
   `18686.4`). Correct, this is #1421's design.
3. `18683.95` `handoff_request_skipped reason=line_editor_not_proven`,
   `handoff_decision decision=Legacy`, `18684.35` `pty_write bytes=10`: the line (Ctrl-P, the
   clear-line key, then `ssh fake`, then Enter) is written 1 ms after the Precmd hook was received.
4. `18687.87` `Received Preexec hook`, `18688.75` **`ssh_gate_released how=other_command`**. This is
   the failing step.
5. `18689.0` `ssh_held_delivery source=output held_bytes=7`, `ssh_handed_back chars=7
   dest=stashed_until_block_end`; from `18692` every key is `kh_hold_exit branch=gate_open_live` and
   is written to the pty. `echo T3` sits in the kept-text banner, `31_jnxirw >> ~/rn.txt` runs in
   the inner shell.

The passing agent run of the same job has the same hops up to the Preexec, no release there, and
then `ssh_pty_owner ssh=true site=tty_owned`, `kh_finish_end delivered=tty_owner`.

## Cause

`ssh_gate_released how=other_command` has one source: `app/src/terminal/view.rs:19391-19395`, the
`else` arm of `parse_interactive_ssh_command(warpify_command)` in the `AfterBlockStarted` handler
("A non-ssh command started: an unmanaged ssh gate is stale (#983)").

`warpify_command` is the event's `command`, and that is the block's command grid, the shell's ECHO
(`app/src/terminal/model/block.rs:3865-3868`, `command: self.command_to_string()`), not the Preexec
hook's payload. In step 3 the write reached the remote tty before the remote shell's line editor
had taken it (the Precmd hook is sent from the prompt command, before the editor switches the tty
to raw mode). The tty was still cooked with echo, so the kernel echoed `^Pssh fake`; the editor
then read the same bytes and echoed `ssh fake`. The screenshot shows the block's header as exactly
those two lines. `parse_interactive_ssh_command` needs the first token to be `ssh`
(`app/src/terminal/ssh/util.rs:278`); it is `^Pssh`, so the answer is "not ssh" and the window the
submit armed 4 ms earlier (`note_command_submitted_for_ssh_gate`, `view.rs:12888`) is declared
stale. The window is unmanaged at that point (`gate.managed` is set only by the ssh hook), so the
`!g.managed` guard does not protect it.

Who wins the race depends on load: on loopback the hook's way to the app plus the write's way back
is about as long as the remote shell needs to start its editor. That is why rounds 5 to 8 passed.
A real network makes the app's write later, so the race is rarer there, but any slow remote shell
start (a heavy prompt command) opens it again.

The ssh edge design says the window's lifetime never depends on a screen read
(`view/ssh_edges.rs:34-36`). This path was one.

## Fix

The decision uses what the app and the shell SAID, not what was echoed:

* `SshConnectGate` remembers the ssh command whose submit armed it (`submitted_command`), until the
  next block that is not in-band starts. That block start takes it (once).
* `AfterBlockStarted` carries the Preexec hook's own command (`reported_command`, the shell's
  `$BASH_COMMAND` / preexec argument), next to the echoed `command`.
* When the echoed text does not parse as an interactive ssh, the block is still the submitted
  ssh's block if the hook's reported command is the submitted command (or itself parses as an
  interactive ssh, for an alias the shell expanded). Then the handler takes the ssh arm with the
  submitted command: the window stays, the running block is noted, the pending ssh host is set.
  The decision is a pure function (`ssh_submit_running_in_started_block`), unit tested.

Everything else is unchanged: a block whose echo parses as ssh is handled as before; a block that
is not the submit's (the hook reports another command, or no submit armed the window, or a later
block) still ends an unmanaged window by #983's rule.

## Rejected

* Cleaning the echoed text (strip `^P`, take the last line): still a screen read, and the shape of
  the pollution depends on the remote tty and editor.
* Trusting the hook's reported command for every block (no submit needed): bash reports only the
  first simple command of a line, so `ssh host | tee log` would newly count as an interactive ssh.
  Wider than the defect.
* Keeping the window for the first block after an ssh submit with no second witness: if the
  submit's write never ran, another command's block would keep a window it has nothing to do with
  until a give-up net. The hook's report is the witness that makes it safe.
* Delaying the submit until the remote editor is proven ready: there is no tty probe for a remote
  shell (`line_editor_not_proven` is the normal answer there); it would need a timer.
* Fixing the doubled block header (`^Pssh fake` / `ssh fake`): cosmetic, same cause, noted on
  #2013, not changed here (replacing echoed text by the hook's command is wrong for bash, which
  reports only part of a line).

## Risk

Small. No key path changes; one more `String` on an event sent once per command. The new arm can
only KEEP a window that a user's ssh submit armed and whose own shell reports that command at the
next block start; such a window still ends by its events (tty owned, block completed, first
prompt) and its give-up nets. A shell whose hook sends no command (empty `reported_command`)
behaves exactly as before.
