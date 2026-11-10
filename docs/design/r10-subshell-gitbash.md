# Round 10: the two Git Bash subshell failures of round 9 (design note)

Base: integ/r10 a26972f51. Round 9: pipeline 3047937983, job app-test-windows 17884561118, pane
`@gitbash`, terminal mode. Evidence available while this was written: the job's trace (one
truncated row per case; the job was still running, so no artifacts and no per-case app-log slice),
round 8's results in `test-results/2026-11-09_build_r18w8_3047198675/app/windows/results.json`, the
round 8 log lines quoted in #1924, and the code. Nothing was built or run. Each verdict says what a
log line proves and what is read from the code.

Issue: #2006.

## 1. subshell_ready_rc_questions: APP

Row: the answers file holds `me command -v shopt ... printf '...7771;<token>;0...'`: the first
answer is `me`, the second "answer" is setup line 0. `two answers re-armed the setup: 1`, and the
"no cancel" check failed. So: one ReArm, then the write, into the rc's second `read`, then a cancel.

Cause (`app/src/terminal/view/subshell_ready.rs`, no-probe branch of `subshell_ready`, #1886):

    ready = tail == ShellPrompt && prompt_moved_since_baseline
            && (bytes_since_arm == 0 || prompt_moved_since_line_end)

`prompt_moved_since_line_end` is `look.position != line_end_baseline && col > 0`
(`view.rs` `judge_armed_subshell_setup`), and a ReArm takes `line_end_baseline` from the last look
BEFORE the Enter (`note_user_bytes_for_subshell_readiness`, #1847). On ConPTY the echo of the
user's keys comes back a console frame later than the keys go out (12 ms per key in the case). So:

1. look at `Name: `: InputPrompt, wait. `tail_before_line = InputPrompt`.
2. `m`, `e`, Enter are written. The Enter is an answer: ReArm (#1924). Baselines = the position of
   the last look, at or before `Name: m`.
3. the echo arrives: the look shows `Name: me`. That is no question (`output_tail_is_input_prompt`;
   #1924's round 8 log shows this very look: "tail=ShellPrompt"), the cursor moved, the line is no
   longer dirty (the Enter ended it). Every term is true: Write.
4. bash has taken `me` and sits in `read -p "Team: "`: it gets the setup line.

With a tty probe the same look is `Cooked` under a line-editor shell: `Wait(CookedReadBuiltin)`.
Without one nothing vetoes it. #1924 turned the round 8 Cancel into a ReArm and so uncovered it.

The same term is true for a line typed AHEAD of the shell (nothing drawn yet): its cooked echo, and
later the line editor's redisplay of the line (`LAB$ echo x`) just before it runs it (the #1941
window, which the probe-less pane has no `TypedAhead` tracking for).

### Fix

A new fact in the sample, `user_line_may_be_at_cursor`, computed by the pure
`subshell_ready::user_line_may_be_at_cursor` and required false by the no-probe branch whenever
user bytes passed (`Wait(UserLineAtCursor)` otherwise). The cursor's row may show the user's last
line when:

- it is the row the line was ended on (the echo extends that row; a prompt drawn after the line
  was consumed is on another row: the Enter's newline, or a `clear`); or
- the line's text is known (plain printable, `LineTracker`) and the row ends with it or with a
  start of it, or continues a soft wrap and is the line's end (`row_shows_typed_line`, as #1941); or
- the text is not predictable (a control byte, a paste, too long) and the row continues a soft
  wrap, or the line was typed before any prompt was drawn (nothing can show the line editor ran
  it): that one is never proof in this attempt; or
- two or more non-empty lines are outstanding with the first typed before any prompt (only the last
  line's text is kept): never proof either.

An empty line (bare Enter, ^C, ^U) has no echo to mistake: unchanged. Panes with a probe: the fact
is always false, nothing changes on unix.

What the attempt does when it is never proof: it stays armed, keys pass through as in a plain
terminal, the 60 s ceiling ends it with the "click Phosphorize to try again" toast; a click re-arms
with a fresh ledger. Nothing is written, held or lost.

Accepted false veto (safe side): a prompt whose last visible character is the first character of
the user's last line (`$ ` and a line that starts with `$`) is taken for the start of the line's
echo; no setup until the next prompt or a click.

### Rejected

- A veto on the tail's shape (`Name: me` contains `: `): shape guessing; misses `Proceed? yes`,
  catches `user@host: ~$`.
- "Rows below the line end >= 2" (the #1941 burst rule): after the rc's `clear` the prompt is on
  row 1 and the rule never holds; `subshell_ready_rc_clear` would wait for the ceiling.
- Never writing on a probe-less pane once the user typed: gives #1886 back for every rc that asks
  a question. Owner question below.
- A Windows "line editor reads" fact from readline's bracketed-paste enable (DECSET 2004; bash 5.1+
  sets it when readline starts a line, `read -p` does not; the app bundles its own OpenConsole).
  Promising as the probe-less pane's POSITIVE proof, but whether ConPTY passes it in stream order
  cannot be checked without a Windows run, and `enable-bracketed-paste off` removes it. Not built
  blind. Owner question below.

### Risk

Low. One more condition on a branch only Windows takes; it can only turn a Write into a Wait. Not
compiled.

### What remains unproven on a probe-less pane (#1973, #1923)

The first write (no user byte yet) still goes out on a shell-shaped tail drawn since the baseline.
A read whose prompt is not shaped like a question (`read -s -p "pin> "`) gets the setup text there,
and the secret typed next is held behind the setup gate. Only an OS or stream fact closes that; see
the owner questions. The Written-state judge has no Secret fact on ConPTY either.

## 2. subshell_ready_rc_clear: APP, same trigger (probable); the tail of the row CANNOT TELL

Row: the inner line ran, the setup-written check passed, the root-shell check passed, and only the
line typed in the pane's bash after leaving did not run (8 s). `Enter: sent 8`, 312 keys,
`2 Enter(s) reached the input editor`, 30 s.

Proven by the rows:

- The harness's leave path works on this pane: round 8, same case, same VM, setup never written
  (cancelled): 6.0 s, 3 editor Enters, the `back_` line ran (results.json). The difference in round
  9 is that the setup WAS written.
- A probe line was held for its whole 8 s wait: macOS's passing row of the same case is 260 keys
  and 7 Enters (three probes); Windows has 8 Enters and 52 more keys = one more probe line plus the
  two chords of `reset()` (Ctrl-C, Ctrl-U), which `leave_nested` types only after a probe got no
  answer in 8 s. The inner line had run by then (`dt < 2 s` passed), so the keys after it were held
  by a written setup that did not finish.

Read from the code and #1924's round 8 log ("the rc is still sleeping (tail=None, nothing drawn)
when the first inner line is typed"): the inner line is typeahead, its Enter re-arms (#1924), and
the look at its late echo (or at readline's redisplay of it after the `clear`) is cause 1's Write.
The setup line then sits behind the user's line in a tty no line editor reads yet.

Not decidable from the trace (the hops are in the artifacts): why the setup then stalled (the echo
filter matching the cooked echo of the line-clear prefix and staying mid-line, which vetoes stage 2
in `stage_two_may_follow`, is the candidate), which cancel or abort ended it (the harness's Ctrl-C
is `abort_in_flight_subshell_setup` when lines are in flight), and why the `back_` line did not run
in the root shell afterwards (text handed back to the editor under its Enter guard, or
keys-follow-editor left set, are the candidates; only one of the two Enters typed in the root shell
reached `enter_begin`).

The 4 "lost-before-app" keys: they do not explain the extra probe and its 8 s (a held line, not a
damaged one: the probe's token is matched exactly). Whether one of them damaged the `back_` line
itself cannot be excluded from the trace. Left to the injector investigation.

No separate fix: cause 1's fix keeps the setup from being written on the user's own line. If the
next run shows the setup written at the prompt AFTER the inner line ran and the row still fails,
the tail is a second defect and needs the app-log slice.

## What the next Windows Git Bash run must show

`subshell_ready_rc_questions` (terminal and agent):

- `subshell_ready_line_end ... verdict=ReArm` twice;
- after each, `subshell_ready_judged ... decision=Wait(UserLineAtCursor) ... user_line_at_cursor=true`
  (the echo look), then `Wait(InputPrompt)` at `Team: `;
- one `subshell_setup_ready_write`, after the second ReArm; no `subshell_setup_cancelled`;
- answers file `me ops`.

`subshell_ready_rc_clear`: `verdict=ReArm` for the typed-ahead line, `Wait(UserLineAtCursor)` at its
echo and at `LAB$ echo ...`, the write at the next `LAB$ ` (a later row), no probe timeout (7 Enters
as on macOS), the `back_` line runs.

In job 17884561118's artifacts, the claim of this note is confirmed by a
`subshell_ready_judged ... decision=Write ... tail=ShellPrompt bytes=3 lines=1` directly after the
first `verdict=ReArm` in rc_questions, and in rc_clear by a `decision=Write` with `lines=1` before
the inner line's effect. If the judged hop before the write says something else, this note is wrong
there.

## Owner questions (not decided here)

1. Should a pane with no tty probe write on screen shape at all, or only on a positive fact?
2. Measure on Windows whether DECSET 2004 from Git Bash's readline reaches the app in stream order
   through the bundled ConPTY; if so, make it the probe-less "line editor reads" fact (and the
   Written judge's), which would also close the `read -s` hole above for bash 5.1+.

## Owner delegation 2026-11-10: decisions on the two open questions

Owner: "Do what you think is right for 1 and 2." Decided by the coordinator (Claude), recorded here so it is not re-litigated.

1. **Should a probe-less pane (Windows Git Bash/PowerShell) write the setup on screen shape at all? Yes: keep the #1886 behaviour.**
   The owner decision of 2026-11-09 is that subshell setup stays automatic; turning it off on Windows would remove a feature to avoid a narrow
   case. The narrow case is a rc `read` whose prompt is not question-shaped (for example `read -s -p "pin> "`) receiving the first setup
   write before any user byte. Its consequence is bounded: the secret typed next is held and never reaches the editor, the model or the
   suggestion provider (#1973, #1923, #1926), and the attempt ends at the existing 60 s ceiling with its toast. The fix in 1297b106f removes
   the common case (the echo of the user's own line taken for a prompt). The residual case is listed as a known limitation of Windows
   auto-setup; revisit only if a Windows run shows it happening.
2. **A Windows "a line editor is reading" fact from DECSET 2004: do not build blind; measure first.** The app now logs
   `input_order hop=decset_2004 on=true|false t=...` (app/src/terminal/model/blocks.rs, lazy, nothing decides on it). In the next Windows Git
   Bash run read the per-case app-log slices of `subshell_ready_rc_questions` and `subshell_ready_rc_clear`: if `decset_2004 on=true` arrives in
   stream order before the prompt that follows a line editor start, and does NOT arrive before a `read -p` prompt, the fact can be built from
   it; if it arrives late, never, or in the same chunk as the prompt, drop the idea. Needs `WARP_INPUT_ORDER_LOG` (the usual input-order log) on.
