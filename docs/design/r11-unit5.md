# Round 11: the five unit tests still red in round 10

Source: the Linux unit run of `build/r10a` (`integ/r10` 5dd50d49c). Judged against `integ/r11`
61487fea1, which changes nothing in these tests or the code they drive
(`git diff --stat 5dd50d49c..61487fea1`: bootstrap scripts, the remote edit model, appci). Nothing
here was compiled or run (rustfmt only). Issues: #2010 (the verdicts), #2022 (new, a rig defect).

## Summary

All five are TEST faults. No app code changes. None of the five shows the Legacy chain missing the
#1996 keep-together behaviour, and none shows a Legacy/Table difference: in tests 3 and 4 the loop
runs Legacy first and the panic ends the test, so **the Table arm of both has never run**. "Passes
in Table" is not known for either.

| # | test | verdict | cause |
|---|---|---|---|
| 1 | `agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants` (+ `_with_the_allowlist_on`) | TEST (oracle), not the fault the round 10 note named | I4 expects every reply in the last request, also one the user cancelled before the app committed it |
| 2 | `terminal::view::tests::open_pane_mid_batch_cancel_sends_the_follow_up` | TEST (fixture), exposed by #1945 | the fixture's actions name a task the conversation does not have |
| 3 | `terminal::view::tests::a_line_typed_behind_a_deferred_enter_follows_that_enters_held_line_at_readiness` | TEST (recorder) | listens for `WriteBytesToPty` only; the release is a counted hand-off write |
| 4 | `give_up_to_pty_tests::an_enter_that_finds_the_gate_gone_over_other_text_sends_only_the_held_line_and_then_acts` | TEST (wrong count) | a command that ran stays in the buffer until its block completes, so "in the editor + ran" is 2 |

Each change keeps or tightens what the test can catch; where a state that used to fail now passes,
it is named below with the app rule that makes it correct.

## 1. The paced fuzz tests (I4)

Panic: `I4: the request holds calls ["call_0_0_0"], but the model emitted ["call_0_1_0"]`.

**The round 10 diagnosis did not fit this scenario.** `r10-unit-lifecycle.md` describes the shrunk
case as "reply 1 asks for another call, the user presses Stop". The shrunk scenario has no Stop:
its actor is `approve, follow_up, follow_up, reject, close_tab`, all `fast`. The round 10 fix
(compare the last request with the replies to EARLIER requests) is right for a stopped turn but
gives the same verdict as before on this scenario, which is why the message came back unchanged.

**What the scenario does** (read from the code, not from a run):

1. Request 0, reply 0: `run_shell_command c0`. Approved; `c0` is `silent` (`env.rs`: stays running).
2. `follow_up` 1. Nothing queues it: the rig has no pty block for the LRC auto-queue and the queue
   toggle is off (`Input::maybe_queue_input_for_in_progress_conversation`, `input.rs:17063-17139`),
   so it is a plain submit. A submit cancels the turn in progress
   (`cancel_active_conversation_for_follow_up`, `controller.rs:3105`) and opens request 1.
3. Reply 1 (`when: nth 1`): `websearch` with `{"query": 7}`. The app runs web tools INSIDE the
   stream (`chat_stream.rs:8545-8658`): `parse_with_coerce` turns `7` into `"7"`
   (`tools/coerce.rs`), and `dispatch_byop_web_tool` awaits `web_runtime::run_websearch`, a real
   HTTP request (see #2022 below).
4. `follow_up` 2, about 70 ms later (60 ms watch + 10 ms fast), while that request is still out.
   Again a plain submit: it cancels the stream in flight and opens request 2.
5. A reply's tool calls are committed in ONE piece, after the stream loop has gone through all of
   them (`final_messages`, pushed at `chat_stream.rs:8644-8656`, yielded once at `8923-8927`). The
   cancelled stream never got there, so reply 1 is not in the conversation and request 2 holds
   only `call_0_0_0`.

The player records a reply as emitted when the request ARRIVES (`player.rs` `answer`), and marks
it aborted only for its own chaos. So I4 holds request 2 against reply 1 and reports a lost call.

**Verdict: TEST (oracle).** A message sent while a reply is in flight cancels that reply; that is
what a submit does everywhere, and what is in history afterwards is protocol-clean (no call
without a result, which the first half of I4 checks on every request). The search result is
discarded with the reply.

**Fix.** The runner already reads `obs.stream_in_flight` and the request count just before every
step. When the step is a message that was sent (not queued), a Stop, or a Ctrl-C with no command
running, and a stream was in flight, it tells the oracle that the reply to the newest request was
cut (`Oracle::reply_cut`). `check_tool_messages` then walks the last request's call groups against
the earlier replies, newest first, as before, with one difference: a reply marked cut that does
not match is stepped over. Every other reply must match in place. A request whose calls match no
reply fails even if every reply before it was cut.

Not weaker than before for any reply the user did not cut. For a cut reply the accepted states are
"whole" and "absent"; a cut reply that is partly there still fails (cannot happen with the single
commit, and would be worth seeing).

**This fix also tells TEST from APP in the next run.** If the same message returns with the cut
recorded, then a reply that WAS committed is missing from a later request: an app bug, to file.

Rejected:
* Marking the reply `aborted` in the player. The player cannot see a cancel, and the reply may
  equally have been committed just before it.
* Committing the carrier call with a "cancelled" result when a stream is cancelled mid-tool
  (an app change). Nothing asks for it, and it would put a call the user never saw finish into
  history on every interrupted search.
* A corpus file for the scenario. It would make a real network request on every run (#2022) and
  pass or fail on its latency.

**Found on the way, filed as #2022, not changed here.** The rig leaves web access on, so every
scripted `websearch` that parses (`fuzz_tests`, matrix `kinds_websearch` / `malformed_websearch`)
POSTs its query (`rust`, `7`) to the real search endpoint from a unit test. Not changed in this
branch because the hermetic fix needs a test hook in the dispatcher and would change what the
matrix cells see (a real answer or a network error today, depending on the box), with no run to
check it against.

Not examined: whether the "Searching" card of a search cancelled this way is left showing a
search in progress (`make_web_search_searching_message` is yielded before the await and the
`done` frame never comes). I did not follow the render path; a real-app look at a search
interrupted by a typed message would answer it.

## 2. `open_pane_mid_batch_cancel_sends_the_follow_up`

Panic: `view_test.rs:12356 left: 0 right: 1`. The helper already sends `Rejected` (round 10), so
the #1942 expectation is current. The captured stderr says what happened:

```
Not sending the request: no exchange could be added to conversation ...: Conversation(NoActiveTask)
Failed to send follow-up with action results ...: no exchange could be added ...
```

The app did what the test asks for: the Reject made the controller send the step's results. The
send was refused at the last step. A follow-up's exchange is appended to the task its results
name (`AIConversation::update_for_new_request_input`, `conversation.rs:1925-1950`;
`TaskStore::append_exchange` returns false for an unknown task). The fixture's blocked action and
its finished result both carry `TaskId::new("task")`, which the conversation does not have.
Before #1945 (2f17df184) such a request was registered and `SentRequest` was emitted anyway, with
every reply event then dropped; #1945 refuses it before that (`controller.rs:5319-5356`).

**Verdict: TEST (fixture), exposed by #1945.** The #1942 decision is implemented and the test's
expectation matches it.

**Fix.** `mid_batch` takes the conversation's root task id and uses it for the action and the
result. The fixture's conversation already has a root task (its first exchange is appended to it).
No BYOP provider is configured in this rig (`run_byop_request_preflight` returns at once), so the
readiness pass does not look at the made-up call ids.

Side effect, a gain: `closing_mid_batch_sends_no_follow_up_...` and
`suspended_controller_mid_batch_cancel_sends_nothing_...` assert "0 requests". With the old task id
they could not fail (no request could be sent from that fixture at all). They can now.

## 3. `a_line_typed_behind_a_deferred_enter_follows_that_enters_held_line_at_readiness`

Panic: `Legacy: make clean was delivered exactly once ...; pty=[] executed=["cd build"]
buffer="cd build"`, left 0 right 1. The assertion just before it passed: the queue is idle, which
is `entries.is_empty() && gates.is_empty()` (`input_queue/mod.rs:719`). So `make clean⏎` left the
queue and no gate holds it; it is not in the editor and did not run as a command.

Where it went: the entry is `PtyBytes` (typed with `write_user_bytes_to_pty` behind the deferred
Enter's gate, handed to the Enter gate by #1997). Released, it goes through
`write_user_bytes_to_pty_after_paste_stage` (`view.rs:14494`). The test has by then made `cd build`
a long-running block that the hand-off did not write (nothing in a view test calls
`on_command_written`; only `pty_controller.rs:2267` does). Keys for such a command are adopted by
the hand-off (#1089: `keys_reach_a_command_the_handoff_does_not_own` -> `adopt_untracked_typeahead`,
`EditorOwned` -> `PtyOwned`), and a `PtyOwned` write is `UserBytesRoute::WriteCounted`
(`typeahead_handoff.rs:4138-4172`), emitted as `Event::WriteHandoffUserBytesToPty`
(`view.rs:14718-14719`). The test's recorder, `record_pty_writes`, matches `Event::WriteBytesToPty`
only; `record_all_pty_writes` next to it exists for exactly this ("both reach the pty").

In the app the same release is a counted write too: the held line's own write makes the hand-off
pty-owned. The sibling test at the 60 s net passes with the narrow recorder because there the
session has no running command: no adoption, a plain write.

`buffer="cd build"` is not a second finding: the input box is not cleared when a command is
executed, only when its block completes (`input.rs`, comment in
`try_execute_command_from_source`), and no block completes in the test.

**Verdict: TEST (recorder).** The Legacy chain did deliver the line, once, after `cd build`. This is
the #1997 path, and nothing here says it lacks keep-together.

**Fix.** The test listens with `record_all_pty_writes`. Its "nothing reached the pty before"
assertions now cover both kinds of write (stricter); "exactly once" counts both. If the bytes were
in fact dropped, the test still fails, with the same message.

## 4. `an_enter_that_finds_the_gate_gone_over_other_text_sends_only_the_held_line_and_then_acts`

Panic: `Legacy: ls is still in the editor or was run by it, once; executed=1`, left 2 right 1.
The assertion before it passed: the pty got `cd build⏎` and nothing else.

The test counts `usize::from(buffer == "ls") + executed`. With `executed == 1` the sum is 2 only
if the buffer still reads `ls`. It does, by design: as in 3, a command the editor ran stays in the
input box until `BlockCompleted`. So one `ls`, run once, counts twice. The test was written on
2026-11-09 without a compiler (1eaa9dd3c) and its own doc allows both outcomes ("kept in the editor
... or run by the editor").

Why the editor could run `ls` when `cd build` was held "for an unready shell": the held Enter was
a DEFERRED one, and `hold_refused_unready_enter` also holds a deferred Enter that was refused
although the session could take a command (`input.rs:7138-7139`, #1558). The second Enter is a
direct one on a session that can execute.

**Verdict: TEST (wrong count).** The app sent only the held line to the pty and then let the Enter
act on what the editor showed, once: the 2026-11-09 ruling (the Enter is not absorbed).

**Fix.** The test records the commands that ran, by text, and asserts:
* only `ls` ran, at most once (new: `cd build`, already at the pty as bytes, must not also run as
  a command; the old count could not tell which command ran);
* `ls` ran or is still in the editor (not lost);
* the editor shows `ls` or nothing (no second copy).

The one state that failed before and passes now is "ran once, text still shown", which is the
app's rule for every command. A state the old sum accepted and the new check rejects: `cd build`
executed with `ls` gone.

## Legacy chain and #1353 step 8

Nothing in these five needs a decision about the Legacy chain. It is still the kill switch
(`PHOSPHOR_QUEUE_DESTINATION=legacy`) until #1353 step 8 removes it; no Legacy arm was deleted or
skipped, and both loops still run Legacy first.

## Risk

Tests and the fuzz oracle only. Unverified until the next unit run. Compile doubts are listed in
the report. The Table arms of 3 and 4 will run for the first time; a failure there is a first
look, not a regression.
