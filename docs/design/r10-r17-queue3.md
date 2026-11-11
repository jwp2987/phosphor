# Round 17, queue 3: #1999, #2000, #1991 (r17-queue3)

Branch `fix/r17-queue3`, from integ/r14 `04e5193a2`. Nothing here was compiled or run; Rust is
`rustfmt --check` only. Wording: "worked, unproven" until a run shows it. Line numbers are those of
`04e5193a2`. The background for all three is `docs/design/r14-queueres.md` (sections 2 and 5),
which left them for the owner; the owner's answers (2026-11-11) are YES to #1999, YES to #2000 and
"build the #1991 design".

Kill switches (all in `app/src/terminal/input_queue/switches.rs`, read once per process, off for
`0` / `off` / `false` / `no`):

| variable | off restores |
|---|---|
| `PHOSPHOR_BARE_ENTER_HOLDS_LINES` | #1999: lines typed behind a bare deferred Enter held for an unready shell are written at once, the bare Enter later |
| `PHOSPHOR_KEEP_DEFERRED_ENTER_KEYS_WITH_LINE` | #2000: `keep_deferred_enter_text` releases the keys behind the unrun line (complete lines go to the pty) |
| `PHOSPHOR_SUBSHELL_LINE_EDITOR_PROOF` | #1991: a setup write to a line editor is judged by the echo filter again (R9 flush on "nothing echoed", R7 blind, the filter armed) |

Timers: none added, none changed.

## 1. #1999: lines typed behind a bare deferred Enter

**Cause.** `Input::enter_gate_holding_a_line` (`input/enter_queue.rs:588-602`) answers `None` for
an empty line (`has_line`). A bare Enter deferred behind the shell's report, then held for an
unready shell (#1571, pending text empty), therefore does not take the lines typed behind it:
`hand_deferred_gate_lines_to_waiting_enter` (`view/paste_queue.rs:1781`) says "not this case", the
keys are taken and `release_held_keys` (`view.rs:16258`) writes them at once. The bare Enter is
written at readiness or at its 60 s net: after the lines it was typed before.

**Fix (owner: yes).** An Enter held for an unready shell holds what was typed behind it whatever
its own line is. One condition: for a `held_unready` gate the answer no longer depends on the
line's text. The S2 test's expectation changes with the decision (nothing at the pty while the
bare Enter waits; its give-up writes `\r`, then the lines, in typed order). DECLINED.md recorded
the opposite; that sentence is replaced.

**Not widened.** A gate that waits for a verdict, a completion or its route with an EMPTY buffer
keeps the old answer. That state is an Enter that already submitted (the buffer was taken), not
a bare Enter; the issue and the decision are about the unready hold only.

**Rejected.** Writing the bare `\r` at once ahead of the lines (the shell is not ready: #1571 is
why it is held).

**Risk.** Low. The hand-over itself is #1997's (`InputQueue::hand_gate_to`, unit-proven); only
which gates qualify changes. `input/enter_queue.rs` is edited because the condition lives there
(the task names it).

## 2. #2000: `keep_deferred_enter_text` and the keys behind the unrun line

**Cause.** `keep_deferred_enter_text` (`view.rs:16210`) ends a deferred Enter without running it
on three event paths: the editor refused the report as another line's text (`view.rs:15578`,
#1569), `YoungTypeaheadDetection::CannotRun` (`15679`), `YoungSubmit::KeepInEditor` (`16085`). It
releases the keys behind the Enter with `GateKeysThen::Release`, and `release_held_keys` writes
them to the pty when an Enter is among them: line two runs, line one stays unrun in the input.

**Fix (owner: yes, as the timer paths do).** A third `GateKeysThen`, `KeepWithLine(why)`. With a
line in the input the taken keys end in `keep_given_up_keys_with_line` (text on the lines after
the unrun line, typed order, the hand-back guard armed, nothing at the pty), then the same guard
and keys-follow-editor bookkeeping the timer path does. With an EMPTY input (a bare Enter) it is
`Release` in every respect, the #1997/#1999 hand-over included: today's behaviour, as the issue
asks. The decision between the two is made when the keys are taken (a deferred take runs later,
and the buffer then is what matters).

No notice is added: these paths show none today and the guard's own "check the line" notice
appears at the next Enter.

**Rejected.** Calling `keep_deferred_enter_text_hand_back` (the timer path) from the three
callers: with an empty input and no gate it writes nothing and with a gate it writes a bare `\r`,
which is the timer rule for a bare Enter, not today's behaviour on these paths.

**Risk.** Low-medium: three event paths change what they do with held keys, but only when an
unrun line is in the input, and to the placement the owner already chose for the timer paths.
`give_up_to_pty.rs` is not edited (only called).

## 3. #1991: a setup write that went to a line editor

Design: `r14-queueres.md` section 5, items 1-4. What is built:

1. **The reader at the write is recorded.** `WrittenSubshellSetup.line_editor_write`: the look
   that decided the write said `SubshellTtyKind::LineReader` (the armed judge's sample, the
   pending stage 2's look, the paced line's look). With it: the bytes of the latest write
   (`last_write_bytes`) and, for the first line of a stage, what the grid needs
   (`subshell_ready::LineEditorWrite`: the line, the cursor's row and column, the pane's columns).
   The echo filter is not armed for such a write (it can never match there; armed, it held back
   every space as a possible prefix start, so a prompt's trailing space was not on the grid).
2. **R9.** `cancel_flushes_unread_line_editor_write`: never for a Secret cancel, never after a
   user byte (both as #1940), and otherwise only when the tty's input queue holds EXACTLY the
   bytes of the latest write. The count is a new probe (`slave_input_queue_bytes`: `FIONREAD` on
   the slave peer on Linux, on the master on macOS), sampled at the cancel under the same model
   lock as the flush, only when the other conditions already allow one. Fewer bytes: the editor
   is taking the line, never cut it. More bytes: something else waits there too (typed before
   the arming), never discarded. No count (no probe, a failed ioctl): no flush. This keeps the
   flush that keeps unread setup bytes out of a question prompt: bytes written at a raw tty that
   nobody read are all still there, and count in full after the tty turns canonical (`n_tty`
   marks pending raw input as a complete line at the switch).
3. **R7.** `line_editor_took_the_line`: a prompt-shaped tail on a row at least
   `(col + line chars) / columns + 1` rows below the row the line was written at (the rows the
   editor's own drawing of the line can reach, plus the line feed of accept-line), the pane as
   wide as at the write, the row not a soft-wrap continuation, and the row's text neither a
   piece of the written line nor ending in a start of it. Such a look is "the echo ended and a
   fresh prompt followed"; the rest of `judge_written` is unchanged (a marker of this attempt is
   progress and wins; local confirmation still needs the empty queue). Not for PowerShell
   (PSReadLine's marker comes when it reads again, i.e. after the prompt).
4. **The echo is hidden by the block reset at `InitShell` only**, as in practice today.

A write whose reader was not a line editor (an editor-less cooked shell, a relay, no probe) keeps
every rule it had.

Logged, counts and names only: `subshell_setup_ready_write` gains `line_editor=`,
`subshell_written_judged` gains `line_editor= proof=grid|echo`, `subshell_setup_cancelled` gains
`line_editor= write_bytes=`, and a cancel that asked the tty for its count logs
`subshell_line_editor_flush why= queue_bytes= write_bytes= flush= flushed=`. The appci case
`subshell_line_editor_proof` (PROVING) reads the first two on a healthy bash setup.

**Accepted losses, all on the safe side.** A prompt whose whole text occurs inside the setup line
(`$`), or that ends in the line's first character, is never proof: R7 stays blind there and the
quiet net ends the attempt, as today. A write judged `LineReader` that reached a tty which had
turned canonical meanwhile is counted three bytes short (the kernel's kill erased the prefix):
no flush. macOS holds 1024 bytes of input (`TTYHOG`): a longer write is never "whole": no flush.

**Rejected.** The two cheap fixes of section 5 of the r14 note. A byte count inside
`SubshellTtySample` (every look would pay an ioctl it does not need, and the count would be older
than the flush it decides). Expecting the editors' forms of the prefix. A proof from DECSET 2004
going off at accept-line: stream-ordered and attractive, but not every readline has bracketed
paste on, and it needs a counter in the model; left as a possible second proof.

**Risk.** Medium. The bash/zsh subshell setup is the common path and its first line is always
written to a line editor. What changes there on a healthy setup: the filter is not armed (no
space held back), nothing else (the marker is progress before any prompt can be seen). The R7
proof can only end an attempt that printed no marker of its own before a fresh prompt.

## 4. What the next run should show

- Unit: the tests listed in `docs/testing.md` ("Round 17, queue 3") pass. Three existing
  expectations changed, each with the decision named in the test and the commit: the S2 test
  (#1999), and for #1991 the R9 flush test's Question case and the E-UNSUPPORTED test (both
  drove a line-editor write with a cooked echo, which a line editor never produces).
- App: `subshell_*` and `subshell_ready_*` stay PASS on Linux and macOS. In a bash or zsh
  subshell setup's log, `subshell_written_judged` now carries `line_editor=1 proof=grid`, and a
  healthy setup never shows `verdict=Unsupported` there. `subshell_line_editor_proof` (new,
  PROVING) asserts exactly that: PASS on Linux and macOS, SKIP on Windows (no tty probe).
- Not provable from outside the app: the R9 window itself (a cancel while the editor is half way
  through the line) and a real unsupported line-editor shell. Those rest on the unit and view
  tests, and on the real-pty byte-count test on Linux.
