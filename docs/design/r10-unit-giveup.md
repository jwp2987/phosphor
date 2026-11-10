# Round 10: the ten red unit tests of the held-keys / give-up / young-hold group

Source: Linux unit run of 413cd4241 (pipeline 3048013125, job test-linux 17885063200): 11,818 tests, 40
failed; this note covers ten of them. Nothing here was compiled or run (rustfmt only). Line numbers are
those of `integ/r10` at cfe7249e8.

## What the round 10 branches already changed

`git log 413cd4241..HEAD` touches `view_test.rs` in seven commits (#1997, #1494, #1941, #1935, #1905,
#2006) and none of them touches any of the ten tests (the diff names none of them).
`typeahead_handoff.rs`, `typeahead_handoff_tests.rs` and `view/route_parity_a_tests.rs` are unchanged
since 413cd4241. So all ten fail on `integ/r10` exactly as they did in the run.

## Summary

Only three of the ten are about the 9 Nov give-up ruling (#1996). The others were left behind by three
other changes of 9 Nov (#1889, #1923, #1852/#1853) or were written that day with a setup fault and
never run. None shows the app handling a held key wrongly under the current rules; one (the last) is a
Legacy/Table difference in the app, which is changed.

| # | test | verdict | cause |
|---|---|---|---|
| 1 | `typeahead_handoff::a_command_the_latch_calls_interactive_gets_its_enter_and_releases_a_live_hold` | stale setup after #1889 | the hold is made with the stream unapplied, where the latch now leaves it |
| 2 | `typeahead_handoff::an_unengaged_user_command_makes_a_young_hold_follow_not_drop` | TEST fault (setup) | a bare Enter with nothing in the shell's line is never a young hold |
| 3 | `typeahead_handoff::keys_a_hold_gave_up_on_never_start_another_young_hold` | TEST fault (setup, and a wrong literal) | as 2; and `enters_written: 2` forgets the command's own Enter |
| 4 | `typeahead_handoff::the_prompt_commits_to_the_editor_when_no_keys_are_held` | stale expectation, owner decision #1923 | a ConPTY Precmd no longer commits before the line editor's signal |
| 5 | `view::a_young_hold_is_written_as_typeahead_when_the_command_becomes_interactive` | stale setup after #1889 | as 1, in the view |
| 6 | `view::keys_behind_a_run_enter_are_written_to_the_pty_at_the_net_in_both_modes` | TEST fault (setup not carried with the #1996 ruling) | expects `ls\r` but queues only `ls` |
| 7 | `view::paste_during_a_subshell_setup_is_held_behind_earlier_keys_in_both_modes` | TEST fault (setup), with one open doubt | the pane has no running command, so Legacy routes the paste to the editor |
| 8 | `view::pty_paste_backstop_gives_up_then_the_late_result_is_ignored` | stale expectation, #1996 decision 2 | the keys behind an unread paste are written to the pty |
| 9 | `view::route_parity_a_tests::suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does` | APP (Legacy only) | a `^C` arriving as a control sequence does not void the waiting Enter in Legacy |
| 10 | `view::stalled_pty_paste_hands_held_keys_back_in_order_at_the_backstop` | stale expectation, #1996 decision 2 | as 8 |

## Per test

### 1. `a_command_the_latch_calls_interactive_gets_its_enter_and_releases_a_live_hold`

Panic: `typeahead_handoff_tests.rs:5348 assertion failed: !handoff.young_hold_queued()`.

`queued_young_hold()` (tests 3490) builds on `young_after_submit()`, which marks the stream undrained
(`stream_undrained_for_test`). Commit 147451442 (#1889, 9 Nov) added to `note_command_interactive`
(`typeahead_handoff.rs:2026-2029`) the condition `stream_drained() != Some(false)`: where nothing
samples the shell, the latch leaves a live hold to the barrier while output is in flight. That commit
added `the_unsampled_latch_leaves_a_young_hold_to_the_barrier_while_output_is_in_flight` (passes) but
did not touch this older test, whose hold stands on exactly the state the latch now leaves alone.

Fix: the test states #1889's precondition. It first asserts the hold stays while the stream is
unapplied (new), then installs an applied barrier and asserts what it asserted before (the latch ends
the hold as `Typeahead`). The expectation is kept; nothing is removed.

Rejected: reverting to the pre-#1889 behaviour (#1889 is an approved fix, labelled v0.1.18).

### 2. `an_unengaged_user_command_makes_a_young_hold_follow_not_drop`

Panic: `typeahead_handoff_tests.rs:36 left: WriteCounted right: Held` (inside `hold_young_enter`).

The test (commit 37295f950, 9 Nov, never run) types a bare `\r` right after `young_after_submit()`.
`should_hold_young_enter_sampled` (`typeahead_handoff.rs:3571-3578`, unchanged since 1 Nov) only holds
an Enter when the state is `PtyOwned { wrote_user_bytes: true, .. }`: "an Enter the user typed on a line
that has content" (doc comment at 3545). Every other test types `frob` / `abc` first (tests 795, 891,
3492). Not an app fault: the precondition is by design and older than the test.

Fix: type `frob` before the Enter. Assertions unchanged.

### 3. `keys_a_hold_gave_up_on_never_start_another_young_hold`

Panic: `typeahead_handoff_tests.rs:1153 left: WriteCounted right: Held: a typed Enter in the window is held`.

Same setup fault as 2 (commit 930b71be5, 9 Nov, never run). A second fault sits behind the first
panic: the test expects `enters_written: 2` after the give-up writes `\rls\r`, but `on_command_written`
already counts the command's own Enter (`typeahead_handoff.rs:2716-2720`, `enters_before + 1`), so the
count after two more Enters is 3 (`scan_line_terminators`, 4664). The assert message says what is
meant: "both Enters are counted as the lines they are".

Fix: type `frob` first in both arms (same state for the typed and the given-up Enter); read the count
before the give-up and assert it grew by exactly two. Nothing about
the #1996 rule changes: a given-up Enter is written and counted, never held again.

### 4. `the_prompt_commits_to_the_editor_when_no_keys_are_held`

Panic: `typeahead_handoff_tests.rs:4078 assertion failed: handoff.try_commit_at_prompt()`.

Commit d925abad6 (#1923, "Owner decision 2026-11-09") added to `try_commit_at_prompt`
(`typeahead_handoff.rs:3040-3043`): on a ConPTY session whose shell signals, the Precmd does not hand
input to the editor while the line editor's ready signal is awaited (a precmd function may be reading a
secret). The helper `conpty_command_running_in_the_enter_window` is such a session, so the commit at
the Precmd is exactly what the owner decision removed. d925abad6 added
`the_precmd_does_not_commit_to_the_editor_before_the_line_editor_signals` but left this test.

Fix (expectation changed by the #1923 owner decision): no commit at the Precmd while the signal is
awaited; the signal (`release_queued_prompt_hold_on_signal`, no hold queued, 1418-1421) ends the wait,
and then the commit is back with no keys held. The test's claim (no hold live means the editor can
take over) is kept at the moment the decision moved it to.

### 5. `a_young_hold_is_written_as_typeahead_when_the_command_becomes_interactive`

Panic: `view_test.rs:23026 Legacy: the hold ended on the event`.

Same cause as 1. `young_enter_setup` (view_test 29368) calls `on_command_written`, which in tests marks
the stream unapplied (`note_fresh_activity_for_test`, `typeahead_handoff.rs:3828-3841`); the view's
`CommandBecameInteractive` handler (`view.rs:20055-20065`) calls `note_command_interactive`, which
since #1889 leaves the hold. Fails in the Legacy arm only because Legacy is first in the loop; the code
path is the hand-off's and is the same in Table.

Fix: the event with the stream unapplied leaves the hold and writes nothing (new assertion); once the
stream is applied the same event ends the hold and `\rx` is one write, as asserted before.

### 6. `keys_behind_a_run_enter_are_written_to_the_pty_at_the_net_in_both_modes`

Panic: `view_test.rs:15759 Legacy PtyNotReady ... left: "ls" right: "ls\r"`.

Under #1661 the `PtyNotReady` scenario ended with nothing at the pty, and the scenario queued only
`Text("ls")` behind the Enter ("where the keys end in the editor, only text"; view_test 15657-15668).
Commit 930b71be5 changed the expectation to `ls\r` for the #1996 ruling and left the setup: no Enter is
queued, so `ls` is all the net can write. The app wrote exactly what was queued. The wait loop above
the assertion also waits for a trailing `\r` that cannot come (2 s lost per mode).

Fix: `PtyNotReady` queues the Enter like `NoEvent` does (its keys go to the pty now, so an Enter among
them is a byte, `give_up_to_pty.rs:179`). The expectation `ls\r` stands and is now reachable; the test
also covers the Enter of a given-up line. Legacy and Table take the same path here
(`write_given_up_entries_to_pty`); Table would have failed the same way.

### 7. `paste_during_a_subshell_setup_is_held_behind_earlier_keys_in_both_modes`

Panic: `view_test.rs:27256 Legacy: typed order kept: "ls"`.

Not a give-up test: #1853 (commit f50b51c5b, 9 Nov, "written without a compiler"). The test announces a
subshell setup on a pane at an idle prompt and calls `paste()`. In Legacy the paste's destination is
`paste_routing` (`view.rs:26658-26671`): input box visible and block list bootstrapped means the input
editor, so `-la` goes to the editor and never to the pty. In the app a subshell setup always belongs to
a running command (`bash`, `sudo -s`): the input box is hidden (`is_input_box_visible_with`,
`view.rs:9769-9783`) and the paste is a pty paste that queues behind the setup gate
(`start_queued_background_paste`, `paste_queue.rs:3119-3121`). Table reads the setup from its plan (row
H7, `table.rs:598-607`) and does not need the block. The other subshell tests set the block up
(`simulate_long_running_block("bash", "")`, view_test 16756, 26389) and drain the startup events first.

Fix: the test gets the running `bash` block and `drain_model_startup_events`, like its siblings.
Assertions unchanged.

Open doubt (not settled, nothing changed for it): after the fix the Legacy arm depends on the
clipboard's background reader and the queue's `PasteReading` gate in the test platform; I could not
run it. If it is still red the log will say where the paste went.

Legacy arm: only the kill switch (`PHOSPHOR_QUEUE_DESTINATION=legacy`). The #1853 fix is Table-only by
its own commit message; the Legacy arm is the reference it is compared with.

### 8. `pty_paste_backstop_gives_up_then_the_late_result_is_ignored`

Panic: `view_test.rs:16264 left: [97, 108, 115, 13] right: [97]: nothing was written at the backstop`.

The test asserts the 8 Nov rule (#1577: the keys behind an unread paste are handed back to the editor).
Owner decision 2 of 2026-11-09 (#1996 follow-up; DECLINED.md "A hold that gives up WRITES its keys",
docs/testing.md "decision 2 ... (a)"): the paste is discarded with its own notice and the keys typed
behind it are written to the pty in typed order (`give_up_gate_keys_to_pty(PasteReading, .., "paste_cap")`).
`group2_timer_tests.rs::paste_cap_writes_the_keys_behind_the_unread_paste_to_the_pty` was changed for
it; this older test was not. The app did what the ruling says: `a`, then `ls\r`.

Fix (expectation changed by that decision): `als\r` at the backstop, the editor empty, and the late
answer still writes nothing (`PASTE` never reaches the pty).

### 9. `route_parity_a_tests::suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does`

Panic: `route_parity_a_tests.rs:34 Legacy key 0x3: left: false right: true`.

The test sends each key through `control_sequence_on_terminal`. Table decides it as a signal and
`execute_arrival_signal` voids the waiting Enter for `^C` / `^D` (`dest_table.rs:1095-1100`). Legacy
voids it only on the key path (`keydown_on_terminal_legacy`, `view.rs:11539-11542`);
`control_sequence_on_terminal_legacy` (`view.rs:11243-11282`) writes the `^C` and leaves the Enter and
the keys behind it waiting. No default binding sends `^C` as a control sequence (`view/init.rs`: `ctrl-c`
is `TerminalAction::CtrlC`), so no user reaches it with the default keymap, but the two modes differ
and the test is right to say so: this is the kind of difference #1852 was filed for.

Fix (app, Legacy chain only): `control_sequence_on_terminal_legacy` voids a waiting Enter for an abort
key before it routes the bytes, the same three lines as the key path. Test unchanged. `^Z` / `^\` are
not abort keys and stay as they are (those two Legacy arms passed in the run).

Rejected: sending the keys through `keydown_on_terminal` in the test. It would pass, and would hide the
difference.

Legacy arm: matters only for the kill switch; it is the arm that found the difference.

### 10. `stalled_pty_paste_hands_held_keys_back_in_order_at_the_backstop`

Panic: `view_test.rs:16161 the backstop wrote to the pty: [[108, 115, 13]]`.

Same decision as 8. Fix: `ls\r` at the pty, once, in typed order; nothing in the editor; the late
answer (`LATE`) writes nothing. Renamed to
`stalled_pty_paste_writes_held_keys_to_the_pty_in_order_at_the_backstop` (the old name states the
superseded rule), as the 9 Nov commits did for their tests.

## Not decided here

The open timer-rule question is not needed for any of the ten: 8 and 10 follow the written decision 2,
and 6 follows the first #1996 ruling (the line ran, so the keys go to the pty).

## Risk

Tests only, except the five lines in `control_sequence_on_terminal_legacy`, which run only under the
Legacy kill switch and only for a `^C` / `^D` control sequence while an Enter gate is open. Everything
is unverified until the next unit run.
