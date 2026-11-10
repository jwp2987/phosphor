# Round 13: the I4 oracle audited (fourth change), and the live Exa test

Source: the Linux unit run of `build/r12a` (integ/r12 501a18469). Read against `fix/r13-fuzz2`
(from integ/r13 d6dd5d4b7). Nothing here was compiled or run (rustfmt only). Tracking issue for
the fuzz failures: #2010.

## A. `random_agent_conversations_hold_the_invariants` (+ `_with_the_allowlist_on`)

```
I4: the request holds calls ["call_fixed", "call_3729_0_1"], which are in no reply. Held against,
newest first: the reply to request 0 (its stream broke: absent, or a leading part of the calls
shown early ["call_fixed"]); no earlier reply is left
```

This is the fourth change to the I4 cross-check (r10 lifecycle, r11-unit5 `reply_cut`, r12-fuzz
`early_call_ids`). By the owner's design-after-the-second-patch rule the whole check was audited
against the app before anything was changed.

### Verdict: TEST (oracle). No app change.

`call_3729_0_1` is legitimately in the request. The reply it belongs to did not break: its body
closed with no `End`, and the app commits such a reply in full, by design. The round 12 rule put
"no `End`" in one state with a disconnect and a stall and said "never whole".

### The audit: what a reply can leave in later requests

Where calls enter the conversation (`app/src/ai/agent_providers/chat_stream.rs`,
`generate_byop_output`):

1. **Early cards**, inside the stream loop: the `ToolCallChunk` arm (7998-8126) adds a tool-call
   message of its own for a call whose chunk parses (8111), unless the tool gets no early card
   (web tools, `search_codebase`, `get_relevant_files`, `todowrite`, `read_skill`, 8022-8030), is
   refused at dispatch (8034), or a call before it in the reply has no card yet (8082-8085).
   Chunks are accumulated by call id: `tool_bufs.insert(call_id, call)`, a repeated id replaces
   the earlier call (8120-8125).
2. **The commit**, after the loop: every accumulated call goes through the dispatch gate, the
   in-stream tools are RUN and awaited (`dispatch_byop_web_tool`, 8607-8609), and everything not
   already shown is yielded in one piece (`final_messages`, 8328, the yield at 8923-8927).
3. Only at an `End` (8127-8155) are calls taken from the captured content, where a repeated id
   is renamed `id#2` (`unique_call_ids`, 8139).

Nothing else adds a call, and nothing removes one:

* a failed exchange keeps its messages (`conversation.rs:2477`
  `mark_request_completed_with_error`; a cancelled one `2369`; both only commit an open
  transaction, no rollback);
* recovery is a verbatim retry when no client action was received, else a resume with a fresh
  request (`response_stream.rs:356-383`, `1204-1207`, `1303-1327`). A retry re-sends the same
  request and a resume sends the conversation as it stands;
* the request preflight writes a cancelled placeholder RESULT for a call that has none
  (`controller.rs:4291-4321`, `synthesize_byop_missing_cancellation_results` 4711). It answers
  calls, it never adds one.

The loop has exactly four ways out, and they decide the state of the reply:

| state | how the loop is left | commit | what later requests hold |
|---|---|---|---|
| open failed (`status`, `context_overflow`) | no stream at all | no | nothing |
| broke mid-stream (`disconnect`: the `Err` arm 7819-7875; `stall`: the idle timeout 7788-7801) | `yield Err; return` | **no** | nothing, or a leading part of the early cards |
| closed with no `End` (`never_end` alone: the stream yields `None`, 7805 `break`) | falls out of the loop like a finished reply | **yes, in full**; the `end_count == 0` guard (9084-9097) raises the error AFTER the yield at 8923 | the whole reply, one call per id (no `End`, so no rename) |
| finished (`End`) | `End`, then `None` | yes | the whole reply, a repeated id renamed |
| any of the last two, cut by the user while in flight (`take_until(cancellation_rx)`, 9105-9108) | the stream is dropped wherever it is, also inside the web tool's await | maybe | the whole reply (cut after the commit), nothing, or a leading part of the early cards |

The third row is the app's stated design, not an accident. The comment on the guard
(9058-9083): "The partial content is not lost -- it was already yielded as `AddMessages` events,
and `final_messages` above has been flushed -- so the user keeps the text and is told it is
incomplete". The error is retryable (`api_error.rs:205-233`, `AIApiError::Other`), client actions
were received, so the turn resumes after 0.5 s and the resume request carries the committed
calls.

### The scenario, in order

Turn 0 (`nth 0`): `[call_fixed not_a_real_tool {}, call_3729_0_1 websearch (valid), call_fixed
read_shell_command_output (valid)]`, chaos `never_end`. Turn 1 (`nth 1`): text and two
`file_glob` calls. Actor: `wait`, `give_back`, `approve`, `answer_question`, `answer_question`,
`approve` (five of them `on_card`).

1. "start the task" opens request 0. The seam plays three `ToolCallChunk`s and then ends the
   stream with no `End` and no error (`fake_stream_seam.rs`, `Next::Done`).
2. Chunk 1, `call_fixed` / `not_a_real_tool`: the name parses to no tool, so no card. Buffered.
3. Chunk 2, `call_3729_0_1` / `websearch`: a tool with no early card. Buffered.
4. Chunk 3, `call_fixed` / `read_shell_command_output`: the same id, so it REPLACES the buffered
   `not_a_real_tool` call. No card either: the call before it (`call_3729_0_1`) has none.
   Two calls are buffered; the app's own log line says so: `stream ended without a terminal End
   event ... tool_chunks=3 tools=2` (once per case in the trace).
5. The stream yields `None`; the loop breaks. `end_count` is 0, but the commit comes first:
   `call_fixed` is answered or carried (gate / parse), the search is run inside the stream (the
   rig's local answer, #2022) and awaited, and `[call_fixed, call_3729_0_1]` with their results
   are yielded in one piece.
6. Then the guard: ERROR log, `yield Err`. Retryable, client actions received: `Resume`.
7. The resume opens request 1 = `[user, assistant(call_fixed, call_3729_0_1), tool, tool]`.
   Turn 1 answers it; whether its two `file_glob` calls go back in a request 2 depends on the
   closing user and cannot be read off the trace. The message is the same either way: the
   group `[call_fixed, call_3729_0_1]` is in every request from 1 on.
8. I4, end of case. The player had recorded reply 0 as `aborted` with early calls
   `[call_fixed]`. The group is not a leading part of that, reply 0 is passed over, no reply is
   left: the message above.

So the request is correct for what happened, and the app did what its comment says it does.

### Cause in the oracle

`EmittedTurn::aborted` was "open failed, OR a `Fail` / `Stall` event, OR no `End`", documented as
"its calls were never committed". True for the first two, false for the third. Round 12 then
started to judge the leftovers of an `aborted` reply ("absent, or a leading part of its early
calls; never whole"); before that they were not judged, which is why `never_end` had not failed
before.

### Fix (fuzz player and oracle only)

* `player.rs`: `EmittedTurn::end: ReplyEnd` replaces the `aborted` flag. `Finished` (`End`
  delivered), `Broke` (failed open, or a `Fail` / `Stall` event: no commit), `ClosedEarly` (no
  `End`, no error: committed). `EmittedTurn::aborted()` stays as "did not end with an `End`" for
  I8, which only asks whether the app may ask again. `call_ids` of a `ClosedEarly` reply are the
  wire ids with a repeated id listed once at its first place (`merged_by_id`; an empty id is
  never merged, each gets its own invented id).
* `oracle.rs` `check_tool_messages`: the rule is the table above, written on the function.
  `Finished` and `ClosedEarly`, not cut: whole and in place, never passed over. Either of them
  cut: whole, absent, or a leading part of the early calls. `Broke`: absent or a leading part of
  the early calls.
* The failure text names the state: "whose body closed with no End (the app commits what it
  received)", "its stream broke before the commit", "closed with no End, cut by the user".

Net effect on what is accepted, against round 12:

| reply | round 12 | now |
|---|---|---|
| `never_end`, not cut | absent, or a leading part of the early calls | whole only (one call per id) |
| `never_end`, cut | the same | whole, absent, or a leading part of the early calls |
| everything else | | unchanged |

It is a named, scoped allowance and at the same time a tightening: a `never_end` reply may no
longer be missing from a later request.

Tests: `player::tests::a_body_that_closes_with_no_end_is_committed_and_a_broken_stream_is_not`,
`oracle::tests::a_reply_that_closed_with_no_end_is_committed_whole` (both fail on the code as it
was; the second also pins that a `Broke` reply with the same delivery is still refused whole).
Existing oracle tests keep their expectations; their helper takes a `ReplyEnd` where it took
`aborted: bool` (`true` = `Broke`). Corpus: `i4_reply_closed_without_end_is_committed.json`, the
failing scenario with its unused env commands removed.

### Rejected

* (b) "APP: do not commit a reply that has no `End`". It would throw away text and calls the
  user has already seen, against the comment at 9058-9083 and the idle-timeout text ("Anything
  already received is kept"), and nothing asks for it.
* Accepting "whole" for every reply without an `End`. A disconnect or a stall never reaches the
  commit; whole there would be a call from nowhere. The states are told apart instead.
* Keeping "absent" for an uncut `ClosedEarly` reply to be safe. By reading it cannot happen, and
  an allowance nothing needs is how the check went blind before.
* Treating an unexplained group as "not judged". Drops the check for the request.

### Seen while reading, not changed (for the coordinator)

1. **Two calls with one id on a body that closes early become one call.** With an `End` the
   repeat is renamed `id#2` (#1278); without one the later chunk replaces the earlier
   (8120-8125), here `not_a_real_tool` is gone and the model is never told. Inherent to
   accumulating by id (a real stream sends the same id many times as arguments grow), and it
   needs a provider that repeats an id AND drops the terminal frame. Not filed as a defect.
2. **A truncated reply's in-stream tools take effect, its other calls do not.** On a body that
   closes early the websearch / webfetch / `todowrite` of the reply are run in the commit, while
   its commands and reads are never queued (only a `Success` exchange queues actions, round 12
   note) and are
   answered as cancelled on the resume. Consistent with "what was received is kept"; whether a
   search should run for a reply the app is about to call incomplete is a product question.
3. Already noted in round 12 and still so: the cancelled placeholder says
   `"reason": "interrupted_by_user"` also on an automatic resume (`controller.rs:4763`).
4. The cross-check still cannot see a committed reply missing from the FRONT of the last request
   (it walks the request's groups), and "the reply that was cut" is still the newest request the
   seam has recorded (round 12 note, "Not changed").

### Risk

Fuzz player, oracle, one corpus file. Unverified until the next unit run. The random proptests
use a fixed seed and stop at their first failing case: every case after this one is a first
look, in all of I1-I8. The corpus scenario has never got past I4, so I8b / I8c / I6 on it are a
first look too; it has five `on_card` steps that find no card (3 s each), about 15 s.
The stricter `ClosedEarly` row rests on reading: if a later run reports "whose body closed with
no End ... not cut by the user", the app dropped a committed reply (APP), or a cut went
unrecorded (the runner's `reply_cut`, TEST).

## B. `websearch_tests::real_exa_anonymous_search`

Not a defect in the test or the app: two decisions met. The test calls the real anonymous Exa
endpoint on purpose; round 12's second lock (`web_runtime::run_websearch`, #2022) refuses a unit
test with no endpoint override, so the test failed at once ("unit tests never reach the real
search endpoint"). Owner decision 2026-11-10: unit tests never reach the real search endpoint.

Change: the test stays, with its assertions, as an opt-in live check outside every unit run.

* `#[ignore = "live network ..."]`: nextest and `cargo test` skip it; no CI job passes
  `--run-ignored` / `--ignored`.
* It also needs `WARP_RUN_WEB_INTEGRATION` (the variable the live webfetch checks already use),
  so a `--run-ignored all` sweep sends nothing. Asked for without it, the test fails with the
  instruction rather than report a pass that checked nothing.
* It passes the real URL outright (`exa::endpoint_url(None)`): the lock stays as it is for every
  other test, and this is the one test that names the endpoint.
* `WARP_SKIP_WEB_INTEGRATION` is gone (its only reader).

By hand, on a machine that may reach the network (not the unit job, not the laptop):

```
WARP_RUN_WEB_INTEGRATION=1 script/agent-cargo <name> nextest run -p warp -p warp_tui --lib \
    --features warp/gui --run-ignored only real_exa_anonymous_search
```

Listed in `docs/testing.md`, "Live checks", with the five live webfetch checks.

Rejected: deleting the test (the brief says no, and it is the only check of the real response
shape); an environment switch that opens the lock itself (it would open it for every test in the
process); a manual CI job (a job is one click from a schedule, and the owner's rule is that heavy
or outward jobs stay by hand; the command above is enough).

Doubt: the live check has not run since #2022. The network gate (`AGENT_WEB`) is asked for the
same URL as before, so it should behave as it did in round 11.

Seen, not changed: the five `webfetch_tests::real_*` tests are not `#[ignore]`d; without the
variable they return at once and are counted as passes.
