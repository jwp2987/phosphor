# Round 12: `gitbash_precmd_secret` on Windows, round 11 (#1923, #2038)

Branch `fix/r12-pin`, on `integ/r12` 592c5ce3d (merged in, so `fix/r12-ps1case` 1d389cfa4 is under
this). Nothing here was compiled or run; Rust is `rustfmt --check` only. Wording: "worked,
unproven" until a run shows it.

Failure (build/r11a c61b6a3b8, terminal@gitbash, the only failing Windows app case): "the PIN did
not reach `read -s` whole and in order: gbpin.txt holds [] ...; the line typed after the read did
not run". Same as round 10.

## 1. Verdict: APP. What the round 11 log shows

Slice `run_slim/logs/terminal@gitbash/case_gitbash_precmd_secret.log`, after `touch gbgo.txt`:

    hop=precmd_seen consumed=true seq_mode=true enters=1 precmds=0
    hop=commit_deferred_awaiting_line_editor
    hop=handoff_commit_at_prompt committed=false still_pty_owned=true
    hop=hand_route reason=held_prompt_hold_started state=pty_owned awaiting_ready=true ... prompt_fact=unsampled
    hop=kh_iq_queue ... gate_open kind=line_editor_active key=44 ... opened_by=precmd_on_conpty
    [WARN] typeahead prompt hold 44: no line-editor ready event within the cap; 6 held entr(ies) wait for the shell
    hop=prompt_hold_kept_overdue hold=44 entries=6
    (every later key: hop=kh_iq_hold queue=Q3_handoff_hold; no pty_write_bytes class=UserKey to the end)

- Hops that ran: the Precmd did not commit (`commit_deferred_awaiting_line_editor`), the first key
  started the prompt hold, the cap fired once and kept the hold. No `prompt_hold_signal`, no
  `handoff_decision`, no key release, for this prompt. The first prompt of the case shows the
  healthy order (`prompt_hold_signal since_precmd_ms=34.5`, `decset_2004 on=true`,
  `handoff_decision decision=Committed`).
- The PIN went nowhere: neither the editor nor the pty. All 13 keys, the Enter and the whole next
  line are `kh_iq_hold` entries behind gate 44.
- Honor PS1 is OFF in the CI owner settings: the screenshot's blocks show the app's prompt line
  (`~/w git:(c61b6a3b8) 3 ...`), not Git Bash's PS1.
- The screenshot shows `pin: ` and the cursor in a block of its own under the `touch` block.
- Was the grid started at the question: the question that matters is which block. The active
  block held nothing; see below. `f26ae8c12` (unstarted grid) did not cover this path.

Cause of that part, found independently here and by `fix/r12-ps1case` (`docs/design/r12-ps1case.md`
section 1, not repeated): text printed between a Precmd and the command's start is early output
in a background block; the fact read the active block. `Block::started()` is the COMMAND's start
(`header_grid.rs:862`), not the grid's, so a Precmd that carries a `ps1` (the Windows CI mode)
changes nothing. 1d389cfa4 reads `BlockList::block_receiving_output()`. With it the cap takes the
`screen_tail_asks_for_input()` branch and releases the six held keys to the pty.

## 2. What is still wrong after 1d389cfa4: the hand-off accounts the keys as the shell's line (#2038)

Read from the code. No log line shows it: no run has got past the cap. Traced with the state of
the log above (`seq_mode=true`, a shell that signalled, nothing samples the tty).

1. `release_queued_prompt_hold(GaveUp)` ends the precmd's wait (`expire_queued_prompt_hold`) and
   routes the keys with `route_user_bytes_unheld(chunk, true)`: `wrote_user_bytes = true`.
2. The second half goes to the pty unheld (no wait, no enter window). Its Enter is a young Enter:
   `PtyOwned { wrote_user_bytes: true, 1, 1 }`, `ShellPromptFact::Unknown if counts_say_prompt =>
   RequestDue` (`young_evidence_sampled`). It is held for a line report. The request is never
   written (`decision_awaits_signal`, and the editor is not proven reading), so the Enter waits for
   `YOUNG_HOLD_GIVE_UP`, 10 s, and the log gets a `young_hold_cap` warning. The case would still
   see the PIN (it waits 20 s).
3. Written by the net, the Enter is counted (`enters_written` 2, `precmds_seen` 1) and opens the
   enter window (`enter_window_may_open`: no wait pending, activity `Prompt`).
4. The `read` returns, bash sends the marker and enables bracketed paste. The controller finds a
   line "queued" (`has_unanswered_lines_for_activation`) and only remembers the signal. No decision.
5. The next typed line is held by the enter window. At its cap: no Preexec since the window opened
   (`GiveUp`), not a clean prompt, a shell that signalled, no question on screen:
   `keep_overdue_prompt_hold`. No event is left to release it. The case's last check ("the line
   typed after the read did not run") fails, and a user needs Ctrl-C.

The same happens with no hold at all: a first key typed later than 1.5 s after the Precmd is not
held (`should_hold_for_prompt`), the screen is never read, steps 2 to 5 follow. That is how a PIN
is usually typed.

## 3. Fix (#2038)

The hand-off learns the fact: `prompt_reader_asks` (`typeahead_handoff.rs`), "between this
prompt's Precmd and its line editor's signal something else reads the tty".

- Set by the view from the screen fact, in two places: at the hold's cap, before the release
  (`paste_queue.rs`, hop `prompt_hold_screen_asks`), and at a key that is about to be routed while
  no hold is queued (`note_prompt_reader_if_the_screen_asks`, called from the user-key path in
  `view.rs`, hop `prompt_key_screen_asks`). The second look is `try_lock` only: a key never waits
  for the parser; when the model is busy the key is held as before and the cap looks.
- Allowed only when the prompt hold is armed (ConPTY, a shell that signals), the pty owns input,
  the wait for this prompt's signal is pending, and the model thread has not parsed a
  line-editor marker for it (`prompt_reader_may_ask`).
- While set, every user write is the reader's (`route_prompt_reader_bytes`): written at once,
  counted in flight, not line content, not an Enter for the shell, no hold, no young Enter, no
  enter window. `try_commit_at_prompt` and `on_line_editor_active` do not hand input to the editor.
  The `hand_route` line carries the length only (the keys may be a secret).
- Ended by events only: the line editor's signal (`release_queued_prompt_hold_on_signal`, which
  both signal handlers call before they decide), a Preexec, the next Precmd, a new session. Keys
  written after the reader's last Enter may be unread in the tty; at the end they become the
  shell's line (`wrote_user_bytes`), so the decision requests them from the proven line editor.
  Nothing typed is left behind an editor that took over.

Result for the case: the question is on screen when the first key is typed, so no key is held;
PIN and Enter reach the `read` as typed; at the signal the counts are (1, 1) with an empty line:
`Committed`; the next line is typed into the editor and runs.

## 4. Rejected

- Only 1d389cfa4 and wait for the run: steps 2 to 5 are read from code that is not in doubt, and
  the run costs a round.
- Clearing the enter window and re-baselining the counts at the line editor's signal: the signal
  after a real typed-ahead Enter looks the same; that is #2018's ground.
- Holding keys typed past the cap too and letting the cap decide: a prompt whose signal went
  missing would hold keys with no event to release them, where today they flow.
- A bash-side marker for `read` (as PowerShell's `TtyRead`): the right long-term answer, #1973.
- Publishing the fact through `KeyFacts`: a string read of 16 rows on every parse slice during
  early output, for a fact read at a handful of keys.

## 5. Risk

Medium-low, ConPTY only, and only at a prompt whose line editor has not signalled with a
question-shaped unterminated line printed since its Precmd.
- A false "asks" (a precmd function printing `loading: ` with no newline while the user types):
  the keys go to the tty unheld, as a plain terminal does, and at the signal they are requested
  into the editor. An Enter among them runs the line in the shell, also as a plain terminal.
- Before the cap, keys at a question are no longer held 1.5 s; that is the intended change.
- A program in a precmd function that enables bracketed paste itself ends the reader early; the
  keys after that are routed as before this change.
- One more read of the hand-off's lock per user key (`prompt_reader_look_due`); the model is
  touched only in the state above.

## 6. Tests

- `typeahead_handoff_tests.rs`: the control (`without_the_reader_...`: steps 1 and 2 as they are
  without the note), the real sequence, the no-hold variants, where a reader cannot be noted, and
  the unread-keys rule.
- `view_test.rs`: `a_precmd_functions_question_sends_the_pin_and_its_enter_to_the_read` (question
  first, and keys first then the cap; Legacy and Table), and
  `the_look_for_a_reader_at_a_key_never_waits_for_the_parser`.
- No case change: `gitbash_precmd_secret` already checks every effect; it was not weakened.

## 7. Open

- The view tests print the question through the test terminal's model. If that model is not past
  its bootstrap Precmd the text goes to the active block instead of a background block; the fact
  is true either way (the unstarted-grid arm), so the tests do not prove the block, the
  `key_facts` tests of 1d389cfa4 do.
- `pty_write_bytes class=UserKey` logs each key's bytes with the debug opt-in on, so a PIN typed
  into a `read -s` is in the app log of a CI run, one key per line. Existing behaviour for any key
  to a program; PowerShell's `TtyRead` marker is what hides it there. Not changed here.
- zsh and fish on ConPTY were not traced. The rule is shell-neutral.
