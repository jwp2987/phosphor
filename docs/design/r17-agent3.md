# Round 17, agent 3: Ctrl-C on a running tool (#2034), the result of a call a broken reply left (#2036), no early `success` (#2029)

Branch `fix/r17-agent3` (from integ/r14 04e5193a2). Nothing here was compiled or run (rustfmt
and the appci self-test only). All three are agent lifecycle items found by reading or in a
harness log; none was seen as wrong behaviour by a user.

## 1. #2034: Ctrl-C on a running action that is not a command

### Cause (by reading)

* Ctrl-C with no agent command in flight: `TerminalView::ctrl_c` ->
  `maybe_handle_ctrl_c_in_rich_content_block` (`view.rs`) -> the status bar's
  `cancel_active_request_or_action` (`block/status_bar.rs`) ->
  `BlocklistAIActionModel::cancel_action_with_id(id, ManuallyCancelled)` (`action_model.rs`).
* That function has two branches. A card that still waits: the action and every later action of
  the turn are cancelled (#1799). An action that already RUNS: only that action is cancelled,
  with the user-stop reason. The later actions stay queued and their cards come up.
* The status bar's Stop button emits `Stop`, which the view answers with the same `ctrl_c`
  (`view.rs`, the `BlocklistAIStatusBarEvent::Stop` arm). So a Stop on a running read did the
  same: one action cancelled as "the user stopped", the next card shown anyway.
* How that step then ended depended on which result came last (the controller reads the reason
  off the last `FinishedAction`): the cancelled read last -> `Cancelled`, nothing sent; a later
  card answered last -> a follow-up after all.

So neither half of the owner rule held for a tool: the key was a half stop, and Stop did not
stop.

### The rule (DECLINED.md, #850 and #1942)

* #850 / owner decision 2026-11-10: "a Ctrl-C typed into a running agent command is not a stop:
  it interrupts the command and the turn goes on. The later actions of the reply still come up,
  and the step ends by the rule above" (a follow-up when any result finished; a step whose only
  result is the interrupted one ends `Cancelled` with the reason shown, #1178).
* #1942: "Stop means stop": a user stop sends nothing and ends the turn `Cancelled`; "a reject of
  an action that is already running ... are stops".

Applied to a tool that is not a command:

| control | state | does |
|---|---|---|
| Ctrl-C key | a tool runs (read, search, MCP call), no reply streaming | interrupts everything that runs (work aborted, result `cancelled`, NO cancellation reason); the later actions come up; the step ends by the ordinary rule. Exactly what the interrupted command's exit 130 does |
| Ctrl-C key | nothing runs (a card waits, a reply streams) | stops the turn, unchanged (nothing to interrupt; #1989 "Ctrl-C in the agent view pauses the agent" kept) |
| Stop button | a tool runs | stops the turn: `cancel_conversation_progress(ManuallyCancelled)` |
| a stop of a running action by id (the card's Reject of a running MCP call, `cancel_action_with_id`) | | stops the turn: everything the conversation holds is cancelled with the stop's reason |
| either, on a running agent COMMAND | | unchanged: the pty's ^C (#850) |

### Fix

* `action_model.rs`: `interrupt_running_actions(conversation_id) -> bool`. True only when
  something runs, the executor holds all of it and none of it drives a shell command
  (`ctrl_c_can_interrupt_actions`, pure). Cancels each with reason `None`. Returns false and does
  nothing otherwise, so the caller keeps its old path. `cancel_action_with_id`, running branch,
  user-stop reason: `cancel_all_pending_actions` (the set a Stop has always cancelled).
* `block/status_bar.rs`: `StopPress { CtrlC, StopButton }`, `active_action_stop` (pure). The
  button's press is remembered (`stop_button_press`) while the view's Ctrl-C routing runs,
  because the button has to keep that routing (a running command, a selection are the view's).
  Running + key -> interrupt; running + button -> stop the turn; a waiting card -> as before.
  The "no action of the shown exchange" branch (#1799) interrupts for the key as well.
* `view.rs`: the Stop arm clears the press after `ctrl_c`; the cardless route
  (`cancel_blocked_turn`) tries the interrupt first for the key.
* Kill switch `PHOSPHOR_CTRL_C_INTERRUPTS_ACTION=0`: all of it off (key and button cancel the
  one running action as a user stop, as before).
* Hop `input_order hop=agent_action_interrupt count=<n>` for the app case.

### Fuzz oracle (I2b / I2c)

Rounds 12-16: "the one exception is a Ctrl-C typed into a running agent command; a Ctrl-C with
any other action running is a stop like any other". That sentence was the app's inconsistency
written as a rule (r12-tests.md said so and filed #2034). Now: a Ctrl-C interrupts what runs
(I2c: it stops within 2 s, and the action that waited behind it for the user is still held); a
Ctrl-C with nothing to interrupt is a stop (I2b). `oracle::ctrl_c_interrupts(commands, running,
stream_in_flight)` is the one place that says which; the runner uses it for I2b/I2c and for
I8 (such a key is not a stop in I8a or I8b). A tool that runs while a reply still streams is
left with I2b: the key cancels the reply there. No check was removed; I2b applies to fewer
keys and I2c to more, for the stated reason.

### Rejected

* A new `CancellationReason::Interrupted`. Every exhaustive match on the enum would need a
  verdict with no compiler here, and `None` already is "nobody stopped the turn": the command's
  interrupt uses it, `plan_finished_batch` and `silent_stop_reason` are written for it.
* Making the key a stop on a running tool (what I2b said). It contradicts the 2026-11-10
  decision's wording for the same key one tool kind over, and leaves "Ctrl-C a slow read, keep
  the turn" impossible.
* Changing the MCP card's own ctrl-c binding. The card claims the key in every state
  (`reject_disposition`, DECLINED #850, pinned by `ctrl_c_reject_tests`), which makes a Ctrl-C
  with focus ON a running MCP card a Reject, hence a stop. After Allow focus goes back to the
  input (#1095), so the usual Ctrl-C is the input's and interrupts. The focus-dependent
  difference is left for the owner (report).
* Threading the press through `ctrl_c` / `ctrl_c_internal` (two cfg variants, eight
  parameters). The flag on the status bar is set and cleared around one synchronous pass.

### Risk

* The interrupted tool's result reads `cancelled` to the model with nothing saying the user
  pressed the key; the model may make the call again. That is what "the turn goes on" means
  for a command too (exit 130).
* A step whose only result is the interrupted tool ends `Cancelled` and shows "Agent stopped: a
  tool call was cancelled before it returned a result" (#1178 text, written for a tool that
  cancelled itself). True, slightly odd after the user's own key. Not changed.
* A file edit whose save is in flight is not cut by the key either: it waits for the save's real
  outcome (bounded, #1277), then the turn goes on.
* The rig has no tool that stays running, so the end-to-end path is covered by the view tests
  (real executor, a grep that never finishes) and the app case, not by the fuzzer.

## 2. #2036: a call left by a broken reply is not "interrupted_by_user"

### Cause

`synthesize_byop_missing_cancellation_results` (`controller.rs`) writes one placeholder for
every tool call the preflight finds with no result and nothing that could produce one:
`{"status":"cancelled","reason":"interrupted_by_user"}`. Written for #147 / #222 (the user's
stop or new message lost a result). A reply whose stream breaks after delivering a call leaves
the same shape (`chat_stream.rs` adds the call's message when its chunk parses; the reply is
never committed, the call never queued), and the request that finds it is usually the
automatic resume: no user did anything.

### Can the preflight tell them apart?

Yes, from state it already has. The readiness key carries the id of the assistant message that
holds the call; the exchange whose `added_message_ids` contain it ended
`FinishedAIAgentOutput::Error` for a broken stream, `Cancelled` for a user stop, `Success` for
the old race. `mark_request_cancelled` skips exchanges that already finished, so a later Stop
does not rewrite an errored exchange.

### Fix

`missing_result_cause_for(exchange output)` (pure) -> `ReplyCutOff` only for `Error`;
`byop_synthetic_cancellation_message(.., cause)` writes
`{"status":"cancelled","reason":"connection_lost","detail": CONNECTION_LOST_DETAIL}` for it
(the call did not run, the user did not cancel it, issue it again if needed) and the old text
otherwise. Kill switch `PHOSPHOR_CONNECTION_LOST_TOOL_RESULT=0`.

**Card.** Unchanged: a result with no structured part and this text is
`NoClientRepresentation` (`convert_from.rs`, pinned by `unrelated_result_none_payload_stays_invisible`),
so the card of such a call shows what it showed before. Not looked at on a build.

**Rejected:** a per-conversation set of "calls cut by an error" on the controller (state to
clean up, lost on restart, same answer). **Not covered:** an exchange that cannot be found (a
restored conversation) keeps `interrupted_by_user`; an error of any kind (not only a dropped
connection) reads "connection lost".

## 3. #2029: `success` for a few ms before the automatic follow-up

### Cause

`handle_response_stream_finished` -> `mark_request_completed` sets `Success` whenever the reply
has no actions. A reply whose tool call the app rejected has none (the call got a local error
result). The follow-up is decided in the NEXT event, `AfterStreamFinished`
(`needs_byop_local_resume` -> `schedule_auto_resume_after_error`), and the resume itself waits
on a zero backoff and on the network. Until that request starts the turn reads `success`.

### Fix

Decide before publishing. The same predicate, now `reply_needs_local_follow_up` (pure, over the
messages this reply added), is asked when the stream finishes. When it holds, the exchange is
completed without the status write (`mark_request_completed_with(.., publish_success: false)`)
and the conversation is noted in `deferred_turn_success`. The turn stays `InProgress`.

Every way out writes the status or hands it to a request, so nothing is left `InProgress` with
nothing coming:

| what happens | status |
|---|---|
| the resume is scheduled and sends | the request's (`InProgress`, then its own end) |
| no resume after all (allowance spent, pane closing) | `Success` at `AfterStreamFinished` |
| the resume fires but is not sent | `Success` then (`publish_turn_success_if_idle`) |
| the user stops while it waits (offline) | `Cancelled`, queued rows released (the #1944 block) |
| another request replaces it (a submit, a queued prompt) | that request's; if it never reaches a stream, `Success` on the next main-loop turn |
| the pane closes | the close's |

`publish_turn_success_if_idle` writes only when the conversation is still `InProgress` and has
no stream, no scheduled resume, no stashed request and no unfinished action. The "next
main-loop turn" check is a ready future, not a timer. Kill switch
`PHOSPHOR_DEFER_TURN_SUCCESS=0`.

### Rejected

* Publishing `in_progress` straight after `success` (the issue's second idea): the false
  `success` is still published, and its subscribers still run.
* Moving the scheduling itself into the stream-finished handler: the stream's own cleanup and
  the `pending_resume` precedence live in `AfterStreamFinished`.

### Risk

* While the resume waits for the network the turn now reads in progress (it read done). A
  prompt typed then queues behind the turn instead of replacing the resume; Stop ends it.
* Not changed: the view's queue drain at `FinishedReceivingOutput` reads the block's finish
  reason, not this status. A queued prompt can still be sent in the window before the resume's
  request and replace it. Out of the issue's scope; in the report.
