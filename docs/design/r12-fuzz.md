# Round 12: the paced fuzz tests fail I4 a third time (build/r11a)

Tests: `ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants` and
`_with_the_allowlist_on`. Message, the same as in round 10:

```
I4: the request holds calls ["call_0_0_0"], but the model emitted ["call_0_1_0"] (expected ["call_0_1_0"])
```

Source: the Linux unit run of `build/r11a` (c61b6a3b8). That build HAS the round 11 oracle change
(fb6236c58, `Oracle::reply_cut`) and does NOT have the local websearch hook (ace8a2242, #2022), so
the scripted search went to the network. Read against `integ/r12` d1ec60c7b. Nothing here was
compiled or run (rustfmt only). Tracking issue: #2010.

## Verdict: TEST (oracle). No app change.

The round 11 note said: if the message returns with the cut recorded, a committed reply was
dropped, an app bug. That test does not hold, for two reasons found by reading:

1. The message is the same on both paths. Not recorded: the request's calls are held against the
   search reply and fail at once. Recorded: the search reply is passed over, no reply is left, and
   the failure is reported against the reply that was passed over. Same text.
2. The round 11 reading of the scenario was wrong about reply 0. Its turn 0 carries
   `chaos.disconnect_after: 1`. Reply 0 is one tool call, so the stream delivers the call and then
   fails ("connection reset by peer", in the trace once per case). It was never committed, never
   approved and never run. The round 11 note has it approved and running.

No committed reply was dropped. The request is correct for what happened.

## The scenario, in order (seed 0: a fast step is 10 ms after the 60 ms watch)

Actor: `approve, follow_up, follow_up, reject, close_tab`, all `fast`.

1. "start the task" opens request 0. The seam plays `[ToolCall call_0_0_0 run_shell_command c0,
   Fail(Transport)]`, no `End`. The player records reply 0 as `aborted`, no calls.
2. The app's stream loop gets the call chunk and shows its card at once: a tool-call message of
   its own, added to the conversation (`chat_stream.rs:7998-8115`, the `ToolCallChunk` arm,
   `make_add_messages_event` at 8111). Then the error: `yield Err; return` (7819-7874). The
   reply's commit (`final_messages`, 8328, yielded once at 8923-8927) is never reached.
3. `ResponseStream` saw client actions and a retryable error: `RecoveryAction::Resume`, backoff
   0.5 s (`response_stream.rs:1304-1327`, `recovery_backoff_after_attempts`). The exchange ends
   as an error with a resume pending (`conversation.rs:2477`, `mark_request_completed_with_error`
   removes no message). No action is queued: only a `Success` exchange queues its actions
   (`controller.rs:6042-6048`).
4. `approve`: nothing is pending, a no-op.
5. `follow_up` 1. Not queued (no pty block, queue toggle off: `input.rs:17101-17139`), a plain
   submit. `cancel_conversation_progress` aborts the pending resume (`controller.rs:5475-5478`).
   The request preflight finds call_0_0_0 with no result and writes the cancelled placeholder
   result for it (`controller.rs:4287-4321`, `synthesize_byop_missing_cancellation_results`).
   Request 1 = `[user, assistant(call_0_0_0), tool(call_0_0_0), user]`.
6. Reply 1 (`nth 1`): `websearch {"query": 7}`. A search gets no early card (8022-8023). At `End`
   the loop awaits the search inside the stream (`dispatch_byop_web_tool`, 8608); in r11a that
   was a real request, in r12 it is the rig's 120 ms hold.
7. `follow_up` 2, about 70 ms after the first, with the search still out. A plain submit: it
   cancels the stream. Reply 1's carrier and result were to be pushed after the await
   (8644-8656), so nothing of reply 1 is in the conversation. The runner sees a sent message with
   a stream in flight and calls `reply_cut(1)`. Request 2 =
   `[user, assistant(call_0_0_0), tool(call_0_0_0), user, user]`; its reply is the implicit `END`.
8. `reject`: no front action. `close_tab`. The closing user finds nothing to answer.
9. I4, end of case. Last request: 2. Its only call group is `[call_0_0_0]`. Replies on record
   before it: reply 0 (aborted, filtered out) and reply 1 `[call_0_1_0]` (cut). The group does
   not match reply 1, reply 1 is passed over, nothing is left: the round 11 rule "calls that match
   no reply fail even when every reply before them was cut" fires and names reply 1.

If the resume of step 3 fires before `follow_up` 1 (a shorter backoff), request 1 is the resume,
`follow_up` 1 is the message that cuts the search, and the last request holds the same single
group. Same verdict.

Whether the cut was on record in the r11a run cannot be read off the trace (reason 1). By reading
it was: reply 1 is absent, so its stream was cancelled before the commit, and the only step that
cancels it is a sent message with the stream in flight, which is what the runner records.

## Cause in the oracle

The player says of an aborted reply "no calls were committed" and records no ids. True of the
commit, but the app puts calls into the conversation earlier than the commit: the card of every
call whose chunk parses, as long as every call before it in the reply has one. Web tools,
`search_codebase`, `get_relevant_files`, `todowrite` and `read_skill` get none (8022-8030), nor
does a call refused at dispatch. Those messages stay when the stream errors (`conversation.rs:2477`)
or is cancelled (`conversation.rs:2369`), and the next request answers them as cancelled.

So I4 had no record of where `call_0_0_0` came from. Before round 11 a leftover group like this
was "not judged" when it was the oldest group, and failed when a finished reply came before it
(the pairs shift by one). Round 11 added a third outcome: fail when a cut reply came after it.

## Fix

`EmittedTurn::early_call_ids` (player): the ids the stream delivered, in order, up to the first
call of a tool that gets no early card; a repeated id once; nothing for a failed open.

`check_tool_messages` (oracle), last request against the replies before it, newest first:

| reply | accepted in the request |
|---|---|
| finished, not cut | whole, in place (unchanged; never passed over) |
| cut by the user (`Oracle::reply_cut`) | whole, or absent, or a leading part of its early calls |
| stream broke (`aborted`) | absent, or a leading part of its early calls; never whole |

"A leading part": the app shows calls in order and stops at the first it cannot show (arguments
that do not parse, a refused tool), so any prefix can be there, and a later call without the ones
before it cannot. Calls that fit no reply fail as before.

The failure text now names every reply the calls were held against, with its state ("cut by the
user" / "its stream broke") and the forms it may take, and a direct mismatch says the reply
"finished and was not cut by the user". The next failure says which path it took.

Stricter than before in one place: a broken reply's leftovers are judged now (they must be a
leading part of what that reply delivered); they used to be invisible. Looser in exactly the two
named rows above, both for a reply that never reached its commit.

Corpus: the failing scenario as `corpus/i4_broken_reply_then_follow_up_cuts_search.json`
(unused env commands removed). Hermetic since #2022. It passes whether `follow_up` 2 lands inside
the 120 ms hold (reply 1 absent) or after it (reply 1 committed).

## Rejected

* Clearing a broken reply's early cards in the app when the stream errors. It would change what
  the user sees ("Anything already received is kept", the idle-timeout text at 7793-7800) and
  what the model is told on the resume. Nothing asks for it.
* Treating any unexplained group as "not judged" when a broken reply exists. That drops the check
  for the whole request.
* Accepting any subset of a broken reply's calls. The app cannot produce a later call without
  the earlier ones, so a prefix is the exact form.
* Marking reply 0 as cut. It was not: no user step touched it.

## Not changed, noted

* The runner takes "the reply that was cut" as the newest request the seam has recorded. A
  request is recorded when its stream opens, a moment after it is sent. A cut that lands in that
  moment would be put on the request before. Not seen; the steps are 70 ms apart.
* The cross-check walks the request's groups, so a finished reply that is missing from the FRONT
  of the last request is not detected (as before).
* Found by reading, filed separately: the cancelled result written for a call left by a broken
  stream says `"reason": "interrupted_by_user"` (`controller.rs:4761-4764`), also on the automatic
  resume, where no user did anything.

## Risk

Fuzz player and oracle only; one corpus file. Unverified until the next unit run. The checks that
run after I4 at the end of a case (I8b / I8c, I6) have never run for this scenario, in the paced
tests or the corpus: a failure there is a first look, not a regression. The paced proptest stops
at its first failing case, so its later cases are unknown until it passes this one.
