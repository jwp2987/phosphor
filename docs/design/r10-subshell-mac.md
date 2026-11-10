# Round 10: the macOS subshell failures of round 9 (design note)

Base: merge/w0 41340e2fd (round 9, pipeline 3047937983, job app-test-macos 17884561117).
Evidence available while this was written: the job's trace (one truncated row per case; the job was
still running, so no artifacts, no per-case app-log slice), the round 7 and 8 results in
`test-results/`, and the code. Nothing was built or run. Each verdict below says what is proven by a
log line and what is read from the code, and names the hop lines the next run must show.

Round 9 ran NO Linux app tests (the Linux jobs were skipped), so the subshell redesign
(#1934-#1941, #1905) met a real shell for the first time in this macOS job. None of the five causes
below is specific to macOS.

## Harness note first: "app logged N more than were injected" and TAINTED

Every macOS row of rounds 8 and 9, SKIP rows included, says "app logged 10-15 more than were
injected" and "Enter: sent k, app saw k+1"; every round 8 macOS row is TAINTED with exactly
"1 of k+1 Return(s) ... 1 LOST". Wayland rows show the same surplus (19-26 keys), untainted only
because `taint.py` runs on macOS alone. A constant surplus on every row, on rows that typed a single
probe line too, is not keys leaking into a case. It is the harness's own previous line:

- after each case `run.py` types the PID probe (`leave_nested` -> `shell_pid`,
  `sh -c 'echo tok:$PPID >> ~/.appci_pid'`), AFTER that case's accounting;
- the next case's accounting window starts 0.5 s before its first injector call
  (`injacct.window`: `min(wall0) - 0.5`), and its Return tap window at `t0 - 0.5`
  (`taint.case_taint` call in `run.py`), while its calls are sliced by index (`be.calls[k_start:]`)
  and its app-log slice starts at `log_off`;
- so the last 0.5 s of that probe (about a dozen keys at the Mac injector's speed, and its Return)
  is counted on the app side of the next case and not on the injector side, and the Return has no
  `app_send_event` inside the slice: "1 LOST".

Verdict: TEST (accounting artifact). The keys were typed, consumed and answered before the case's
first key, so the cases' own verdicts are not made unreliable by it. But the flag is worthless while
every row carries it. Fix: neither window may start before the case does (`t0`): `injacct.window`
takes a floor, `taint.case_taint` is called with `t0`. Nothing else changes; a real surplus or a
real lost Return inside the case is reported as before.

## 1. subshell_sh_probe: APP

Row: "dash: the first inner line ran at all (file []); ... ran 10.05 s after its Enter ...;
exact lines ...: got []"; evidence line "8 Enter(s) reached the input editor".

- The 10.05 s is the harness: `ctx.until(lambda: tok in ctx.lines(f), 10)` timed out. No app timer
  is 10 s on this path (the notice toast is, and routes nothing).
- The line was HELD, not lost and not run (read from the row and the code; the hop lines are in
  the artifacts, not the trace): `got []` for every later line and for the pane's own `back_` line
  too, and 8 Enters reached the input editor where the case types about 4 there when its inner
  lines go to the pty (round 8's passing macOS row: 2, before the case typed its `ENV` lines).
  Keys typed after Ctrl-I were held behind the SubshellSetup gate, the
  setup stalled, the 30 s quiet net (`SUBSHELL_SETUP_QUIET_NET`, view.rs) handed them to the input
  editor as text, and the rest of the case typed into the editor behind them.
- Why it stalled (code): `write_init_subshell_bytes_to_pty` put the echo of EVERY setup line into
  the echo filter at the first write (view.rs, `for line in &all_lines { model.expect_setup_echo }`).
  Every setup line starts with a space (`bootstrap.rs` `init_subshell_script_for_unknown_shell`:
  `format!(" {script}; ...")`). After line 0's echo matched, dash ran it, printed the marker and
  its prompt `$ ` (the case gives dash `PS1='$ '` through `$ENV` since 366c86857). The filter
  (`setup_echo_filter.rs` `swallow`) took the prompt's trailing space for the first character of
  line 1's echo: `matched_in_line = 1`, `is_mid_line()` true. `advance_subshell_bootstrap_on_marker`
  asks `stage_two_may_follow(tail, echo_mid_line, ...)` (#1905, c2addb7d6), whose first line is
  `if echo_mid_line ... return false`. dash prints nothing more, so no later look: stalled.
- Why now: the #1905 gate was merged after round 8, and round 8's Linux dash drew no prompt.
  Round 9 is the first run of the gate against a cooked shell that draws a prompt.
- Corroboration inside round 9: `subshell_sh_probe_empty_ps1` on the same build, same dash, passed
  its "first inner line < 2 s" and "unsupported event" checks. Its dash drew a prompt that does not
  end in a space (the zsh pane's exported PS1, see 5), so nothing was swallowed.
- macOS and the Linux "reader idle" fact: not involved here (nothing is typed before the write:
  `bytes_since_arm == 0`, `idle_reader_untouched` writes). For an editor-less shell the user DID
  type into, macOS has no equivalent: `leader_blocked_reading_tty` is `None` off Linux (no
  `/proc/<pid>/syscall`; XNU exposes no wait channel without task_for_pid), so the judge keeps
  `Wait(CookedNoPrompt)` when no prompt is drawn. That attempt is ARMED, not written: keys pass
  through as in a plain terminal, nothing is held, and the 60 s `PRE_READY_CEILING` ends it with a
  toast. No 10 s net exists. With a prompt drawn (any dash that has one) the old proof (new prompt
  + empty queue) works on macOS as before. No macOS path is added (5).

Fix: expect a line's echo when the line is written (`write_subshell_bootstrap_line`), only line 0
at the first write. Rejected: dropping the `echo_mid_line` veto for an editor-less shell (it is
right when a written line is half echoed, the macOS TTYHOG case); matching the prompt's space
tentatively in the filter (more state for a wrong expectation). Risk: low; a line editor never
matches the line-clear prefix, so the filter stays inert for bash/zsh/fish as before.

Next Mac run: `subshell_stage_two_waits_for_prompt` absent (or followed by the write),
`subshell_setup_unsupported` within the first second, no `subshell_setup_given_up`. If the artifacts
of job 17884561117 are read first: `subshell_stage_two_waits_for_prompt tail=ShellPrompt
text_since_marker=true kind=Cooked`, then `subshell_setup_given_up why=quiet_net` about 30 s after
`subshell_setup_ready_write`.

## 2. subshell_ready_vim_typeahead: APP (most likely; the deciding hop is not in the trace)

Row: only "the setup was written after vim (hop subshell_setup_ready_write)" failed. The typed lines
all ran exactly, vim got exactly `hello`, the root shell was reached: nothing was lost or misrouted.
The setup was never written after vim; the next typed line then (correctly) cancelled the attempt
as UserDrove.

By elimination in `subshell_ready` for the look at the prompt after vim (LineReader, shell prompt at
the cursor; the same macOS build writes on that shape with a proven empty queue in
`subshell_ready_rc_questions`): the only wait left is `Wait(TypedLineRuns)`, the #1941 state. It
sticks like this:

- `vim ...⏎` is typed during the rc's `sleep` (cooked bash): `typed_ahead = Unread`.
- The rc ends; readline prompt, its echo of the line, the accept and vim's start reach the app
  before one look is taken (a look reads the tty when the wakeup is handled, not when the bytes
  were written). The first look already has vim in front: `Unknown`, so `Unread` stays.
- `:wq⏎` is a line end with a program in front: ReArm, which re-takes `line_end_baseline` from the
  last look (inside vim).
- After vim: `typed_ahead_after_look(Unread, LineReader, ...)` needs "the row no longer shows the
  line AND the cursor is two rows below where the line was ended" to know the line already ran.
  The rows are measured from `line_end_baseline`, which the `:wq` ReArm just moved: rows = 0, so
  `Unread -> EditorRuns`, `Wait(TypedLineRuns)`, and no further output ever comes.

Fix: the row the typed-ahead line was ended on is kept on its own (`typed_ahead_row`), not shared
with the baseline later line ends re-take. Rejected: clearing `Unread` when a program is in front
(the program may not have read the line; the editor would still run it later). Risk: low; with no
later line end the two rows are the same value.

The TAINTED part: see the harness note. Not an isolation fault of the app, and not a reason to
distrust this row.

Next Mac run: `subshell_ready_typed_ahead from=Unread to=None ... rows=2` at the prompt after vim,
then `decision=Write`, `subshell_setup_ready_write`. In job 17884561117's artifacts the claim is
confirmed by `from=Unread to=EditorRuns ... rows=0` after the alt screen and a last
`decision=Wait(TypedLineRuns)`; if the last judged hop says something else, this verdict is wrong
and the row needs its own look.

## 3 and 4. subshell_ready_secret_after_prompt, subshell_ready_secret_after_probe: APP

Rows: the `why=Secret` cancel check PASSED in both (it is not in the fail list; in round 8 it was
the failing one, so the #1905 part works). Then "the password reached the read -s: []", "the setup
was written again" (it was NOT: these are the names of the failed assertions), the handed-back line
never ran, the root shell was not reached.

Cause (code, and round 7's rows as log evidence): the Secret cancel hands the held line to the
editor (`hand_back_gate_keys_inner`, paste_queue.rs), which ends with `note_keys_follow_editor()`
(#1658: keys typed after a kept line follow it into the editor). The hand-off is pty-owned (a
running block), so the flag is live, and the next keys, the PASSWORD, are routed to the input
editor: shown in clear after the handed-back line, never sent to the `read -s`. The read never
ends, so no second write, no prompt, no exit. Round 7 (Wayland and macOS) shows the same thing in
its pw file: `['echo T238_souzyl >> lab_secret.txthunter2']`, the handed-back line with the
password glued to it, submitted by the case's later Return.

On ConPTY the equivalent event already ends the flag (`on_tty_read_marker`: "keys follow the pty,
never the editor"). The unix Secret cancel did not.

Fix: `end_written_subshell_setup` ends keys-follow-editor after a Secret or a Question cancel
(both show the toast "the shell is asking for input; answer it first"), and the hand-back does not
set it for those two reasons (a hand-back retried on a busy queue). The line stays in the editor
under its Enter guard, the keys typed next go to the pty. Rejected here: a general "never follow
the editor while the tty reads a secret" check in every hand-back (that is #1973's shared fact,
open, and wider than this task; the setup net giving up while a secret read is open still sets the
flag); it is recorded in the report. Risk: low, one call on one path plus one condition.

Next Mac run: `subshell_setup_cancelled why=Secret dest=Editor`, `keys_follow_editor_end
why=subshell_secret` (when the flag had been set), the pw file is `hunter2`, a second
`subshell_setup_ready_write`. Not proven by this note: the rest of the case after the password
(the second write completing, the handed-back line run by the user's second Enter) has never
passed on any platform, so further findings there are possible.

## 5. subshell_sh_probe_empty_ps1: TEST (precondition), and no macOS app path

Row: only "this dash has an EMPTY PS1" failed, with PS1 = `%{ESC]133;A BEL%~%# ESC]133;B BEL%}`:
the macOS pane's shell is zsh, and `export PS1` exports zsh's prompt string, not the blanked PS1 a
Linux bash pane has. Every app check of the case passed (first inner line < 2 s, unsupported event,
no cancel, no give-up). The case proved nothing about #1935 there, as its own check says.

Fix (test): where the pane's shell does not hand a child an empty PS1 (not Linux), the case gives
the dash one through `$ENV` (`PS1=` in a file an interactive dash reads before its first prompt).
The "PS1 is empty" check stays as it is. The second leg stays Linux-only: there is no macOS fact.

App: no macOS path for a typed-into sh/dash is proposed (see 1). What the user gets there is a
plain shell for up to 60 s and then a toast; nothing is held or lost.

Side observation, not changed here: a zsh pane that exports PS1 hands children Phosphor's OSC 133
prompt marks; a non-Phosphorized dash then prints them at every prompt. Same family as #1907.
