# r16-fastwin: round 13 Windows `fast_typing` 29/30 (PowerShell pane, terminal mode) — #2093

Nothing was compiled or run on a Windows machine for this note. The verdict is read from the round 13 artifacts
(build/r13a = integ/r13 `b43018198`, pipeline 3051722998): the case log of terminal `fast_typing`, `ps_history_ft0.txt`,
`results.json`.

## Verdict: TEST fault. The typed line was not lost.

What the failure said: `ft0: file has 29/30 lines ... exp='t94_j0jblqxis0' got=None ... the reference kept every line:
Phosphor-side (#1482)`.

What happened to the 30th line (`Add-Content r_ft0.txt t94_j0jblqxis0`), from the app's `input_order` log:

| t (ms, last 6 digits) | hop |
|---|---|
| 089969.157 | `kh_editor_action kind=enter hold_active=false buffer="Add-Content r_ft0.txt t94_j0jblqxis0"` |
| 089969.279 | `enter_begin deferred=false ... input_type=Shell` |
| 089969.971 | `write_command` |
| 089970.454 | `pwsh_command_via_pipe bytes=36` |
| 089970.572 | `pty_write_bytes class=UserCommand wrote=2 of_job=2 bytes="\x1b3"` (the insert chord) |
| 089980.989 | `preexec_received signal=preexec` (PowerShell has the line, 11 ms after the write) |
| 090313.159 | `precmd_seen consumed=true` (the command ended, 332 ms later) |
| 090322.611 | `handoff_commit_at_prompt committed=true still_pty_owned=false` |
| 090345.417 | `handoff_decision decision=Legacy`, request `\x1b1`; `input_report reported_chars=0` (nothing left over) |

- No hold, requeue or drop: the 30 lines of the burst each have one `enter_begin`, one `write_command`, one
  `pwsh_command_via_pipe`, one `preexec_received`, one `precmd_seen`. Line 30 has no keys behind it, so no typeahead
  hand-off, young hold, prompt hold or #2012 anchor is involved; `hold_active=false`, `deferred=false`.
- PowerShell received it exactly: `ps_history_ft0.txt` is `hmark=16166 total=16196 new=30 want=30`, 30 lines in order, the
  last `Add-Content r_ft0.txt t94_j0jblqxis0`.
- So the command ran and its write did not land. The harness is the other party on that file. `effect_run` types the
  burst with no pause after the last Enter (gap 0) and at once polls `len(ctx.lines(fname))`: an open and read every
  50 ms. Lines 1 to 29 run while the harness is still typing and nobody reads the file. Only the last line of a gap-0
  burst runs while the file is being read, and the last line is the one that is missing. In `ft25` (0.25 s after every
  line) the last command is done before the first read; `ft25` passed 30/30 in the same pass.
- Windows PowerShell's `Add-Content` opens the file with a share mode that excludes readers. The harness has met the
  mirror image before: `Ctx.lines` retries `PermissionError` ("the shell still has the file open for writing", r18cm),
  which a read-only open can only get from a writer that does not share reading. With the reader first, the writer is
  the one refused: `Add-Content : The process cannot access the file ... because it is being used by another process`,
  and no line is written.
- Two things in the log fit an error in that block: preexec to precmd took 332 ms (lines 1 to 25: 28 to 62 ms; the
  first error record a PowerShell session formats is slow), and 15 s later the idle prompt's next-command suggestion
  was `Add-Content r_ft0.txt t94_j0jblqxis0 -Force`, the same line again with `-Force`: a retry of a failed command.
  No other `fast_typing` log of rounds 11 to 13 has a suggestion there.

Not captured, so the mechanism is worked out and unproven: the text of that block. The failure screenshot is taken
after `ft25`, whose 30 blocks scrolled it away.

"The reference kept every line: Phosphor-side" was wrong in this case: the reference burst polls the same way and did
not hit the window. Rounds 10 to 12 and the other three round 13 passes did not hit it either (a read is short, the
poll is every 50 ms).

## r12a to r13a (`501a18469..b43018198`), `app/src/terminal` and the PowerShell bootstrap

`e19d0b057` #2053 (agent view shows a precmd function's question), `f35389322` #2050 (secret read by a precmd function,
Linux and macOS), `5f9de29a3` #2044 (Ctrl-C in an ssh session the wrapper called plain), `2ec4c1035` (a unit test).
None is on the path of a typed PowerShell command, and the trace above shows the path did its job. No app regression,
and no older app race: the line was delivered. The r14 queue changes (#1900, #1998, #2047) are not involved either
way; this failure can recur on any build until the harness stops reading the file early.

## Fix (harness only)

- `Ctx.await_file_size(name, size, timeout)`: stat only, no handle with read access.
- `effect_file_bytes(shell, toks, win)`: the complete file's size (CR LF from Windows PowerShell and cmd.exe, LF
  elsewhere).
- `await_effect_file(...)`: on Windows waits for that size, then the caller sleeps 0.5 s and reads once, as before. A
  file that never reaches the size is read after the same timeout as before, so a lost line fails exactly as it did.
  Used by `effect_run` (fast_typing, rollover, the callers in cases_ui / cases_win_handover / cases_r14_w1a /
  cases_macptr) and by the reference burst in `run_enter_control` (a reference line lost this way would count as a
  reference loss and forgive one Phosphor loss). Off Windows the old line poll is kept: a reader cannot fail an append
  there.
- When PSReadLine history has every typed line exactly and the file still lacks lines, a note says the lines were
  accepted by PowerShell (not lost typing). The failing check stays.

The exact-lines check, the history check, the burst, the cadence and the timeout are unchanged. No expectation was
weakened.

## Rejected

- Forgiving a missing last line when history is exact: that would pass a run whose command did not take effect. The
  race is removed instead.
- Retrying or re-typing the line: hides a real loss.
- A sleep before the first read: a timer in place of the fact (the size).
- Caching `Ctx.lines` by stat for all callers: it does not protect the first read, which is the one that collided.
- Changing the typed command (`[IO.File]::AppendAllText`, `>>`): changes what every Windows case types and what the
  history check compares.

## Risk

- If `effect_file_bytes` is wrong for some shell (another newline, a BOM), the wait runs to its timeout and the read
  happens then: same verdict, 35 s slower, and a `HARNESS:` note names it. Windows PowerShell 5.1 and pwsh 7 write
  CR LF with no BOM for ASCII tokens; cmd.exe `echo` CR LF; Git Bash LF.
- `os.path.getsize` on Windows opens with attribute access only, which takes part in no share check. Unproven until a
  Windows run.

## Left open (same exposure, not changed here)

About 150 other places poll an effect file with `ctx.until(lambda: ... ctx.lines(...))` right after an Enter. In the
PowerShell pane each can make its own `Add-Content` fail the same way (rarely: one read per 50 ms). They wait for
"some line", not for a known size, so this fix does not carry over one to one; they need a per-case look. Until then:
a PowerShell-pane failure of the form "file has N-1 lines" whose history or `preexec_received` shows the command ran
is this fault first, an app loss second.
