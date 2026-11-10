# Round 13: the round 12 unit failure of the #2038 view test (r13-pin2)

Branch `fix/r13-pin2`. Nothing here was compiled or run; Rust is `rustfmt --check` only. Wording:
"worked, unproven" until a run shows it.

Failure (round 12 unit run, `terminal::view::tests::a_precmd_functions_question_sends_the_pin_and_its_enter_to_the_read`,
`view_test.rs:27899`): `Legacy question_first=true: the first half is with the read`, left `[]`,
right `[[112],[105],[110]]`.

## 1. Verdict: TEST. No app change

The test read its pty recorder inside the view update that typed the keys.

- A pty write is an event: `ctx.emit(Event::WriteHandoffUserBytesToPty { .. })` (`view.rs`,
  `write_user_bytes_to_pty_after_paste_stage`; the released hold's keys in
  `view/paste_queue.rs` `release_queued_prompt_hold`).
- `ViewContext::emit` only queues the event (`warpui_core/src/core/view/context.rs:400`,
  `pending_effects.push_back`). Subscribers run in `AppContext::flush_effects`
  (`core/app.rs:4276`), and only when `pending_flushes` is back to 0: when the OUTERMOST update
  returns (`take_view_for_update` adds one, `finish_view_update` flushes).
- So inside `terminal.update(..)` the recorder of `collect_pty_writes` is always `[]`, whatever
  the app did. The assertion at 27899 could not pass. Every older test of this group reads the
  recorder for content after the update (`conpty_prompt_hold_releases_on_the_signal_through_the_queue`,
  `a_precmd_does_not_commit_input_to_the_editor_so_a_precmd_read_gets_the_pin`); inside an update
  they only assert `is_empty()`, which is true for the same reason and proves nothing there.
- Not the recorder kind: `collect_pty_writes` listens to both write events (the round 11
  `record_pty_writes` fault does not apply).

What the failure does show about the app: the assertion before it passed (`!prompt_gate_open`,
"nothing is held"). In that state (pty owns input, this prompt's wait pending or an enter window
open) a key that reaches the hand-off is either held behind the `LineEditorActive` gate or routed
to the reader. `the_look_for_a_reader_at_a_key_never_waits_for_the_parser` (PASS in the same run,
same fixture, model locked) shows the key does reach the hand-off and is held when the look
cannot run. So in the failing arm the reader was noted and the keys were routed `WriteCounted`:
the path #2038 added ran. The five `typeahead_handoff_tests.rs` tests of #2038 passed in the same
run.

Legacy vs Table: only the Legacy arm ran (the panic ends the test). Nothing on this path reads
`destination_mode()`: `write_user_bytes_to_pty` to `route_user_bytes` is the same code in both;
the mode changes `queue_signal_of` (^Z, ^\ only) and the delivery of `Text`/`Enter`/`Held`
entries, none of which this test queues. No Table difference is expected; it has never run.

## 2. Fix (test only)

`view_test.rs`: the same sequence in separate updates, the recorder read between them. Every
assertion of the old test is kept. Added, none removed:
- keys-first arm: the three keys are three queue entries, no reader is noted before the question
  is printed, and "nothing written while held" is now read after the update (it was vacuous);
- after the cap's release the queue is idle;
- the line editor's signal writes nothing more and leaves the editor empty.

Rejected: reading `prompt_reader_asks` instead of the writes (weaker: the writes are the claim);
a synchronous recorder hooked into the view (new test-only app code for one test).

## 3. Trace of the rewritten test (read, not run)

Fixture: `on_precmd`, signal, `on_command_written` (pty owned, 1 Enter, 0 Precmds),
`simulate_block` (Preexec, CommandFinished, Precmd: 1/1, wait pending, activity `Prompt`).

- question first: key `p`: `prompt_reader_look_due` true; `try_lock` free; the receiving block's
  tail is `pin: `; `note_prompt_reader_asks` true; `route_prompt_reader_bytes`: `WriteCounted`.
  `i`, `n` the same route. Recorder after the update: three writes.
- keys first: `p` held (`held_prompt_hold_started`), `i` `n` queue behind the gate; `pin: `
  printed; `on_paste_queue_give_up`: `prompt_hold_cap_fired` = `GiveUp` (the hold began in this
  wait), the screen asks, reader noted, `release_queued_prompt_hold(GaveUp)`:
  `route_released_chunk` true three times, three events.
- second half and Enter: `routes_to_prompt_reader`, no hold queued: `WriteCounted` each; counts
  stay 1/1, `wrote_user_bytes` false, no young hold.
- signal: `end_prompt_reader("line_editor_signal")`.

If the rewritten test fails, the message says where: "nothing is held" or "held for the line
editor" = the look; "the first half is with the read" with a non-empty left side = order or
splitting; with `[]` again = the keys were dropped (APP, a lost key); "the hand-off knows a reader"
= the note.

## 4. Re-read of the #2038 change for races

Lock order: the key-time look reads the hand-off, drops it, `try_lock`s the model, drops it, then
writes the hand-off. The cap's look locks the model (blocking, in a timer callback, not on a key),
drops it, then writes the hand-off. The model thread takes model then hand-off. No path holds the
hand-off while it waits for the model: no deadlock found.

Found, none changed (each needs a design decision in the hand-off; none is hit by the case's
timing):

1. **The parser holds the model at a key, inside the cap** (first key less than 1.5 s after the
   Precmd): no reader noted, the key is held for the prompt as before #2038, the keys after it
   queue behind it, the cap looks with a blocking lock and releases them to the reader. Cost: up
   to 1.5 s, order kept. Covered by `the_look_for_a_reader_at_a_key_never_waits_for_the_parser`
   up to the hold; the cap half by the keys-first arm.
2. **The parser holds the model at a key, past the cap, and the key is not an Enter**: it is
   written at once as the shell's line (`wrote_user_bytes`), the next key looks again and the
   reader is noted. The PIN arrives whole and in order. At the signal the decision asks the line
   editor for a report of a line that is empty: one request more than needed, then `Committed`.
3. **The same, and the key IS the Enter that ends the read, with no reader noted yet** (every
   earlier look also missed, or the whole answer is one Enter): the Enter is a young Enter again
   (`held_young_enter_started`, evidence `RequestDue`), and with a young hold queued the look is
   not due for the keys behind it. That is the pre-#2038 path: 10 s net, the Enter counted, an
   enter window nothing closes. Narrow: during a `read -s` the shell prints nothing, so the
   parser has nothing to hold the model for; another holder of the model at that instant would
   do it. Not closed by this change. A fix would be for the young hold's give-up to look at the
   screen as the prompt hold's cap does; that touches `give_up_to_pty.rs` (another agent's file
   this round) and is a design item, not a patch.
4. **A key typed after the line editor's signal was parsed but before the main thread handled
   it**, with `pin: ` still the tail (a `read -s` echoes no newline): noted as the reader's and
   written; the signal then ends the reader with `line_dirty=true`, the keys count as the shell's
   line and the request collects them. Handled by the #2038 "unread keys" rule
   (`keys_the_reader_left_unread_are_the_shells_line_once_the_line_editor_signals`).
5. **A stale question tail at a later prompt**: an empty Enter gives a Precmd with no command
   start. If the early-output block still ends in the answered `pin: ` (the precmd function
   printed nothing after its read), a key typed between that Precmd and the signal (about 35 ms
   on ConPTY) is taken for the reader's and written unheld; at the signal it is requested into
   the editor. This is the "false asks" row of the r12 risk list, with a concrete trigger. Not
   verified: whether the block list reuses that background block across an empty prompt cycle
   was not traced. The case's function prints a newline after the read, so the case cannot hit it.
6. **A SetBracketedPaste event of an earlier prompt handled after the next Precmd was parsed**
   ends the wait and the reader of the new prompt (it did the same to the prompt hold before
   #2038). After it no reader can be noted for this prompt and the keys are the shell's line: the
   pre-#2038 path. bash turns bracketed paste off at accept-line and on at the next readline, so
   this needs the main thread to be a whole command behind the parser.
7. `end_prompt_reader` at a Precmd or Preexec marks the line dirty when keys were written after
   the reader's last Enter. A Ctrl-C clears it (`scan_line_terminators`), so the usual way out of
   a read does not leave a false mark.

## 5. What a failure looks like in the Windows `gitbash_precmd_secret` run

Timing of the case: `pin: ` is on screen about 0.6 s before the first key; the second half comes
2.7 s later, then the Enter; then a line that must run.

- Healthy: `prompt_key_screen_asks reader_noted=true` (or, if the first look missed,
  `hand_route reason=held_prompt_hold_started`, then at 1.5 s `prompt_hold_screen_asks
  reader_noted=true`), `prompt_reader_asks`, `hand_route reason=prompt_reader bytes_len=1` per key
  and for the Enter, `prompt_reader_end why=line_editor_signal line_dirty=false`,
  `handoff_decision decision=Committed`; the last line goes through the editor.
- The look never says yes (wrong block again): `prompt_hold_kept_overdue`, every key `kh_iq_hold`;
  `gbpin.txt` holds `[]`. The round 11 picture.
- Reader noted late or lost before the Enter (items 3, 6): the PIN keys have
  `pty_write_bytes class=UserKey`, the Enter has `hand_route reason=held_young_enter_started`,
  about 10 s later a `young_hold_cap` warning; `gbpin.txt` is right (the case waits 20 s), then the
  last line is `kh_iq_hold` behind a new `line_editor_active` gate with `prompt_hold_kept_overdue`:
  "the line typed after the read did not run".
- The reader never ends (no signal handled): no `prompt_reader_end`; `commit_deferred_prompt_reader`
  at the prompt; the last line's keys are `hand_route reason=prompt_reader`. The case PASSES (the
  line reaches bash directly) while the editor never takes input back. Only the hops show it:
  read the log of a PASS too.
- `line_dirty=true` at `prompt_reader_end` followed by `decision=SendRequest`: a key was written
  after the read's Enter (item 4). Not a failure by itself.
