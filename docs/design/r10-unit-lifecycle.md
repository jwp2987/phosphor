# Round 10: the 12 agent-lifecycle unit failures

Source: pipeline 3048013125, job test-linux 17885063200, commit 413cd4241 (merge/w0 without the
round 10 branches). The round 10 branches changed no file under `app/src/ai/` except
`blocklist/input_model.rs` (+14 lines, unrelated), and nothing in these tests
(`git log 41340e2fd..HEAD -- app/src/ai app/src/terminal/view_test.rs`), so all 12 are judged
against this branch as they failed. Nothing was compiled or run here.

Background that matters for four of the verdicts: the earlier Linux unit runs (r18w1 to r18w8)
had their job log cut at GitLab's 4 MB cap, so only the first 3,600 to 4,400 tests of about
11,800 were ever seen (`test-results/2026-11-09_build_r18w*/README.md`). `agent_fuzz::matrix`
sorts after the cut. This is the first run in which the matrix, the stop tests and the restore
tests were visible at all; "it passed before" is not known for any of them.

## One app bug

### `stop_tests::a_reject_after_a_finished_result_tells_the_model_at_once_and_the_agent_continues` -- APP

The rejected call's result was `{"status":"cancelled","reason":"interrupted_by_user"}`, the text
of `byop_synthetic_cancellation_message` (`controller.rs:4756`), not the rejected form. So the
preflight classified the call as "no result anywhere" and invented one, although its real
cancelled result was in the request's input.

Cause: two places build a `ToolCallKey` for the same call differently.

* The readiness projection groups the adjacent tool calls of one reply and keys EVERY call of
  the group by the id of the group's FIRST call message
  (`chat_stream.rs` `SerializerProjectionBuilder::push_tool_call`, `pending_assistant_message_id`,
  lines 1063-1104).
* The live-call lists key each call by its OWN message id: `cancellation_live_tool_calls`
  (`chat_stream.rs:1892-1924`, cancelled results in the current input) and
  `byop_unfinished_live_tool_calls` (`controller.rs:4530-4580`, actions still pending or running).
* `Classifier::live_call_for_key` compares the whole key (`byop_readiness/mod.rs:1283-1288`).

Each tool call is its own `agent_proto::Message` with its own uuid, so for the second and later
calls of a reply the two keys never match. Effects:

1. A cancelled result in the input for a non-first call is never `NeedsCancellationCommit`; it
   falls to `MissingResultWithoutRepairSource { NoResult }` and gets the synthesized placeholder.
   The real result (and with it the "rejected by the user" wording of #1942) is dropped. This is
   the failing test: the read is call 1, the rejected command is call 2.
2. A non-first call whose action is still pending or running is never `PendingToolResults`. A
   request built in that window does not wait (stash) for it: the call gets a synthesized
   "cancelled" result while its action is still alive, and the action's real result arrives
   later for a call that is already answered. That is a lifecycle fault of the stuck-turn family
   (#1966/#1967: a result with nowhere to go); I cannot show from the code that it is the cause
   of either issue.

Every existing readiness test uses a single call, where the two keys coincide.

Fix: in `classify_byop_controller_readiness_with_live_tool_calls`, after the projection is built,
re-key each live call that matches no projected call exactly to the projected key with the same
`(task_id, tool_call_id)`. One place, covers both lists. Callers only use `(task_id,
tool_call_id)` of the returned calls (`commit_byop_cancellation_results`,
`synthesize_byop_missing_cancellation_results`), so nothing downstream changes.

Rejected: keying the projection per message (changes `normalize_projection`'s group inference and
the repair records' keys, which are persisted); comparing live calls by `(task_id, tool_call_id)`
only inside the classifier (the readiness module's own tests pin exact-key matching).

Risk: a request that used to go out with a synthesized result for a still-live non-first call now
waits in the stash for that call's `FinishedAction`, as it always did for a first call. A user's
own submit cancels pending and running actions first, so only automatic requests can wait.

## Test faults (the expectation was never true; no owner decision involved)

### `conversation::tests::a_restored_turn_with_every_tool_call_answered_is_successful` -- TEST

Fails at the third fixture (`conversation_tests.rs:1599`): a turn ending in a subagent call and a
carrier call with no tool. `has_unanswered_trailing_tool_calls` (`conversation.rs:1013`) does
exempt both, but the status is decided earlier: `into_exchanges` marks an exchange whose last
message is any tool call as Cancelled (`convert_conversation.rs:474-479`, upstream rule, older than
#1951), and `derive_status_from_root_task` returns on that (`conversation.rs:982-984`). The
fixture never restored as Success, before or after #1951; the test was written uncompiled.

Fix (test only): keep the exemption under test with a fixture the older rule does not decide (a
client call answered, with the subagent and carrier calls between it and its result), and pin
what a turn that ENDS in such calls really restores as (Cancelled).

### `stop_tests::a_stop_after_a_finished_result_opens_no_request_and_the_results_ride_on_the_next_message` -- TEST

Fails at `stop_tests.rs:158`: the request's last message is the `<environment_context>` block,
not the user's text. On the OpenAI-compatible path the environment tail is always appended as its
own user message (`chat_stream.rs:4524-4568`, deliberate since 2026-07-19 for the prefix cache).
The assertions after it never ran.

The same wrong assumption is in the oracle written with it (5419e35cc): I8b counts "requests
whose last message is the user's text", which no request of the rig ever is, so I8b's "one
message, one request" half has never judged anything; `RecordedRequest::last_user_text`
(`Match::UserContains`, I3 `rows_sent`) and `Match::LastToolResult` read the tail too.

Fix (harness only): the seam's record knows the tail. `RecordedRequest::newest_message()` is the
last message that is not the environment tail; `last_user_text()` skips it; the stop test, I8a's
message, I8b, I8c and `Match::LastToolResult` use them. I8b judges real requests for the first
time: a failure there in the next run is a first look, not a regression.

### `fuzz_tests::paced_random_sequences_hold_the_invariants` (+ `_with_the_allowlist_on`) -- TEST (oracle), exposed by #1942

I4 "the request holds [call_0_0_0], but the model emitted [call_0_1_0]". Shrunk case: reply 0 asks
for a command, its result goes back (request 1), reply 1 asks for another call, the user presses
Stop. Stop means stop (#1942): no request follows, so the last request is request 1 and it cannot
hold the calls of its own reply. The cross-check (`oracle.rs:413-441`) zips "calls in the last
request" with "all calls emitted", newest first, which is only aligned when every emitted reply
was followed by a request.

Fix (oracle only): compare the last request with the replies to EARLIER requests
(`EmittedTurn::request_index < last.index`). Not weaker: a reply can only be in a later request.

### `matrix::pairs_hung_command`, `triples_hung_command`, `triples_read_files` (off and on) -- TEST (oracle)

All failing cells are `CtrlCAfterFirst` with a never-ending command RUNNING at the key and later
actions of the same reply still waiting. The Ctrl-C ends the command; the next action then shows
its card (`Blocked`), and the runner applies I2b ("terminal, nothing held") after every Ctrl-C.

A Ctrl-C into a running agent command is that command's interrupt, not a stop of the
conversation: DECLINED.md IMPROVED "#850" ("the executor then reports exit 130 ... and the
conversation continues"), `ctrl_c_interrupts_running_agent_command_without_cancelling_conversation`,
and the runner's own I8a comment. Commit 9728314e8 corrected the same expectation in corpus
`hang1851c2` ("by design; I2b is for Stop") but not the runner.

Fix (runner only): I2b applies to Stop, and to Ctrl-C unless a command was running AND other
actions of the turn were still waiting behind it; in that case the new I2c applies (the
interrupted action stops running within the same 2 s), the cards that follow are answered by the
closing user, and I2a still requires the turn to end. Every cell that passes today is judged
exactly as before.

Owner question this touches, NOT decided here (#1942, first comment): should a Ctrl-C typed into a
running agent command stop the turn too? If yes, the app changes (cancel the rest of the turn on
that path) and I2b goes back to every Ctrl-C: a one-line revert in `runner.rs`.

## Stale expectation (owner decision changed the behaviour)

### `terminal::view::tests::open_pane_mid_batch_cancel_sends_the_follow_up` -- STALE

The positive control cancels the blocked action with `ManuallyCancelled` and expects one request.
Since #1942 (owner decision 2026-11-09) that reason is a user stop and sends nothing; a card's
Reject has its own reason, `Rejected`, and does send the step's results at once (second part of
the decision, d43024ea7, which updated the controller tests' Reject helper but missed this one).
The helper is documented as "what the reject button does", so it now sends `Rejected`. The
suspended-controller test that shares the helper keeps its meaning (it must send nothing for a
cancel that WOULD send in an open pane).

## Not changed

`app/src/terminal/input.rs`, `input/enter_queue.rs`, `view/give_up_to_pty.rs`: untouched.
