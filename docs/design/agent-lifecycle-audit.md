# Agent turn lifecycle audit (Fable 5.2, 2026-11-09)

Read-only audit at merge/w0.

Scope read: `controller.rs` (stream events, AfterStreamFinished, cancel, follow-up planning, send_request_input, BYOP preflight), `controller/response_stream.rs`, `controller/pending_response_streams.rs`, `action_model.rs` + `action_model/preprocess.rs` + `action_model/execute.rs` (cancel paths), `agent/conversation.rs` (status/exchange marking, restore), `chat_stream.rs` (call-id assembly, idle net, truncation guard), `queued_query.rs`, `terminal/view.rs` drain, `agent_fuzz/{oracle,runner,actor,matrix,rig}.rs`.

Key mechanics that drive several findings: `ctx.emit` is deferred (`crates/warpui_core/src/core/model/context.rs:161`), so `FinishedAction` / `AfterStreamFinished` are handled after the update that emitted them; `has_active_stream_for_conversation` is the intersection of the controller's stream map and `conversation.added_exchanges_by_response` (`pending_response_streams.rs:27-39`); `should_trigger_request_upon_completion` is simply `!is_cancelled()` (`crates/ai/src/agent/action_result/mod.rs:819`); the executor's cancel of a running action is asynchronous (emits `FinishedAction`, `execute.rs:960`) but removes the action from `async_executing_actions` synchronously (`execute.rs:943`).

## Ranked findings

### 1. Stop / Ctrl-C after a partially finished batch sends a new request (severity: double request, borderline stale run)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:586-625` (`plan_finished_batch`), subscriber at `:1277-1283`, status side at `/home/winters/git/phosphor-merge/app/src/ai/blocklist/action_model.rs:2020-2046`.
- Trigger: reply with two calls, e.g. `read_files` then `run_shell_command`. The read runs and succeeds; the command card blocks. User presses Stop -> `cancel_conversation_progress(ManuallyCancelled)` finds no stream (`:5183`) -> `cancel_all_pending_actions` -> the command gets a cancelled result -> `finished_action_results = [success, cancelled]` -> `handle_action_result` sets status **InProgress** (not all cancelled) -> `FinishedAction{ManuallyCancelled}` -> `plan_finished_batch`: `treat_as_success` is false, one non-cancelled result -> `follow_up = Some(Auto)` -> `send_follow_up_for_conversation` opens a request after the user stopped. The model's reply can queue auto-executable actions that then run.
- Only `[rejected()]` is tested for Stop (`controller_tests.rs:2909`); the `None`-reason comment at `action_model.rs:1963-1980` explains why mixed batches follow up when nobody asked for the stop, which does not cover an explicit Stop. Confirm intent with the owner; it contradicts "Stop".
- Fix (pure, no timer): in `plan_finished_batch`, when `cancellation_reason.map(conversation_outcome) == Some(Cancelled)` (ManuallyCancelled, Deleted, UserCommandExecuted, FollowUpSubmitted{false}) return `follow_up: None` unless `has_manual_follow_up`; `final_status: Cancelled`. The results stay in `finished_action_results` and ride on the user's next request (send_query drains them, preflight dedups).
- Fuzzer: `Policy::StopAfterFirst` with pair `[ReadFiles, Command]` reaches it, but I2b only requires "terminal within 2 s" and the fake model's "done" reply makes the follow-up finish inside the budget, so it passes. Add oracle **I8: after Stop/Ctrl-C/CloseTab, `rig.seam.requests().len()` must not grow** (and: exactly +1 for a FollowUp op, which also pins #1896/#1901).

### 2. Offline pre-action failure ends the turn with a wrong "unfinished exchange" error instead of waiting for connectivity (severity: visible-error-instead-of-recovery; hang-adjacent)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller/response_stream.rs:1282-1291` (`RetryWhenOnline` returns without emitting and without changing `current_request_id`), `:1348-1358` (`on_response_stream_complete`), controller `:5703-5723`.
- Trigger: `NetworkStatus` offline, request fails before any client action -> `defer_retry_until_online` -> the failed stream then ends -> `on_response_stream_complete(request_id)` still matches -> emits `AfterStreamFinished{None}` -> controller finds the exchange still `Streaming` -> marks `UnfinishedExchange` error (non-recoverable), `cleanup_stream` drops the `ResponseStream` handle. The parked retry dies with the model (or, if any handle survives, sends a request nobody listens to). `Retry` avoids this only because `retry()` rotates `current_request_id` first.
- Fix: in the `RetryWhenOnline` arm set `self.current_request_id = None` (so the dying stream's completion is ignored), keep `cancellation_tx`, and have the online callback go through `retry()` which assigns a fresh id; also emit the error with `recovery_pending = true` so the conversation shows TransientError/"waiting for network".
- Fuzzer: unreachable, no offline chaos (rig is always online). Add chaos kind `offline` (flip `NetworkStatus` before a `status` failure, flip back after N ms) and assert status TransientError then exactly one further request.

### 3. Stop while a turn's actions are all still preprocessing leaves the conversation InProgress (severity: hang)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/action_model.rs:1687-1725` (`cancel_all_pending_actions`: `abandon_all` returns only `Done` batches; `Pending` batches are dropped), `:1561-1579` + `:1489-1510` (`record_stale_cancelled` records results with no `FinishedAction` and no status write), controller `:5183-5192` (no status update in the no-stream branch).
- Trigger: reply contains only `request_file_edits` calls whose diff apply (preprocess) is slow or hung (#1894 family); user presses Stop during it. No action yields a synchronous result, no stream is in flight, so nothing writes a status; the preprocess future later resolves to "Action not found" -> stale cancelled results recorded silently. Conversation stays InProgress with nothing held until the user types.
- Fix: in `cancel_conversation_progress`'s else-branch, after `cancel_all_pending_actions`, if the conversation is InProgress and `!has_unfinished_actions_for_conversation` and no batch is still preprocessing, write `ConversationStatus::Cancelled` (or make `record_stale_cancelled` emit `FinishedAction` when the conversation holds nothing else). Event-driven, no timer.
- Fuzzer: unreachable; no `RequestFileEdits` kind in `matrix::Kind`, and command/read preprocessing resolves in one poll. I2b would catch it given a FileEdit kind with a slow-preprocess chaos.

### 4. A failed `update_for_new_request_input` leaves a live, unobserved stream and silently discards the prompt and the reply (severity: lost prompt + wasted provider request)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:5020-5049` (Err only `log::warn!`; the stream is still registered at `:5041`), events dropped at `:5414-5418`, end at `:5581-5593` (`finish_stream_without_exchanges(None)`: no status, no notice).
- Trigger: `send_user_query_in_conversation_internal` `:2793-2800` targets the active block's `subagent_task_id`; if that task is gone, `append_exchange_to_task` -> TaskNotFound. The request still goes to the provider; every event (including tool calls) is dropped; the user sees nothing.
- Fix: on Err, cancel the just-created stream (`response_stream.update(.., cancel(..))`), skip `register_new_stream`, and surface via `update_conversation_status_with_error_message(Error, ..)` or `AgentStopped`.
- Fuzzer: unreachable (needs a subagent task). Unit test with an injected TaskNotFound.

### 5. Calls of a cancelled (non-steer) stream get no result and their cards never settle until the next send (severity: lost result / cosmetic hang; owner rule "exactly one result" is met only later by preflight)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:5687-5702` (`cancel_discarded_actions` only when `is_follow_up_for_same_conversation`), window: `Finished` is processed in the stream callback, `AfterStreamFinished` in the later completion callback.
- Trigger: Stop (or a submit into another conversation, `FollowUpSubmitted{false}`, or Deleted) lands between the two; `actions_to_queue` is non-empty but dropped. The tool calls sit in history without results; their cards have no action-model status (`get_action_status -> None`), so a question card can stay on "Preparing question..." until the user's next message, when preflight synthesizes `interrupted_by_user` (`:4537`). Preflight is BYOP-only.
- Fix: call `cancel_discarded_actions(discarded, conversation_id, Some(reason), ..)` for every reason whose outcome is `Cancelled` too (not only same-conversation follow-up). `handle_action_result` then writes Cancelled and the cards finish; the results ride on the next request.
- Fuzzer: Stop with `Timing::AfterEnd` can hit the window, but no oracle looks at it. Add to I1: every ToolCall message of the last reply has an action-model status or a persisted result within delta.

### 6. A user send deferred for an attachment read fails if any request opens meanwhile (severity: lost prompt)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:2342-2426` (`finish_user_query_send` does not re-cancel), `:4835-4858` (in-flight guard: `safe_assert!` + Err, caller only logs).
- Trigger: `send_query` cancels the active turn at `:1894`, then `schedule_send` parks the submit for a disk read (`:2032-2110`, up to the 30 s give-up). During the read a monitored command completes -> `follow_up_on_monitored_command_completed_with` (`:1514`, facts: no stream, no unsent actions, no resume) -> `resume_conversation` opens a stream. The read lands -> `send_request_input` -> "in-flight request" Err -> `log::error!` only. The prompt is gone.
- Fix: in `finish_user_query_send`, if `has_active_stream_for_conversation`, cancel it with `FollowUpSubmitted{is_for_same_conversation: true}` first (the typed prompt is a steer, #851); or have the in-flight Err path restore the prompt into the input with a notice.
- Fuzzer: unreachable (no attachments, no pty blocks).

### 7. A stashed BYOP request is dropped when its trigger batch is all-cancelled (severity: lost prompt, latent)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:1283-1372` returns before `:1376` (`flush_pending_byop_request_after_finished_action`) whenever `plan.follow_up` is None; the stash (`:4900-4909`) survives only until the next `send_request_input` deletes it at `:4831`.
- Trigger: a request stashed as `PendingToolResults` (a tool call with a live action at preflight) whose live action then finishes with a cancelled result (reason `FollowUpSubmitted{same}` or `None`) -> plan None -> status Cancelled -> stash never flushed. Today the realistic stash sources (`resume_conversation`, monitored completion, slash commands) are each guarded, so the path is narrow, but every new `send_request_input` caller inherits the hazard.
- Fix: move the `flush_pending_byop_request_after_finished_action` call above the `plan.follow_up` early return: flush whenever a stash exists and nothing is unfinished, regardless of the plan.
- Fuzzer: unreachable (send_query cancels pending actions synchronously, so preflight never sees a live call).

### 8. `complete_byop_blocked_request` ends a turn without a stream end (severity: lost/parked queued prompt, state leak)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:4678-4772`: records `added_exchanges_by_response[local id]` (never cleaned, `is_processing_response_stream` true forever for that id), marks Error, but emits no `FinishedReceivingOutput`, so `terminal/view.rs:7058-7143` never runs `drain_queued_prompts` for this turn end; `PendingApprovalFollowUp`/LRC rows stay locked until some unrelated turn ends (I3 unlock).
- Fix: at the end, `conversation.cleanup_completed_response_stream(&id)` and `ctx.emit(FinishedReceivingOutput { stream_id, conversation_id })`.
- Fuzzer: unreachable (needs a corrupt history: duplicate/orphan/out-of-order result). A corpus scenario would need the seam to emit two results for one id.

### 9. `pending_passive_follow_ups` survives Stop (severity: request after Stop, low)
- `/home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs:3008` inserted; cleared only at `:3289` (after a send) and `:1828` (close). `cancel_conversation_progress` (`:5131`) leaves it, so a later Stop/Reject on that conversation yields `has_manual_follow_up` -> `UserRequested` follow-up (test `:2958` shows ManuallyCancelled + manual sends).
- Fix: remove it in `cancel_conversation_progress` when `reason.conversation_outcome() == Cancelled`.
- Fuzzer: no entry point for `request_follow_up_after_actions`.

### 10. Restore after relaunch reports Success for a turn with unanswered calls (severity: cosmetic / rule gap)
- `/home/winters/git/phosphor-merge/app/src/ai/agent/conversation.rs:974-994` (`derive_status_from_root_task`: `Streaming` or `Finished Success` with unanswered tool calls -> Success); `terminal/view/load_ai_conversation.rs:525-528` restores only results (`past_action_results`), never re-queues or cancels unanswered calls. The turn shows as finished with calls hanging; the next send's preflight synthesizes `interrupted_by_user`.
- Fix: in restore, if the last exchange is `Streaming` or has ToolCall messages without results, set `Cancelled` and show "interrupted by relaunch".
- Fuzzer: unreachable (no relaunch).

### Defensive note (not reachable today)
`AfterStreamFinished` with both `actions_to_queue` non-empty and `pending_resume` Some (`controller.rs:5744` + `:5857`) would queue the actions and arm a resume. `chat_stream` cannot yield Finished then Err, so it is theoretical; guard with `if pending_resume.is_some() && actions_were_queued { skip resume }`.

## Oracle / fuzzer gaps (what the fuzzer cannot see)
- No request-count invariant: I2b/I2a/I3 pass while an extra request is opened after Stop (#1) or twice on a steer (#1896/#1901). Add I8 (count frozen after Stop/Ctrl-C/CloseTab; exactly +1 per FollowUp).
- No "every ToolCall of the last reply has a status or a result" check (#5); I1 only inspects actions the model already holds.
- No chaos for offline (#2), slow/hung preprocess (#3, needs a FileEdit kind), attachments (#6), subagent/LRC task loss (#4), corrupt history (#8), relaunch/restore (#10), conversation switch (`FollowUpSubmitted{false}` paths). The rig has one conversation and no pty.
- `check_tool_messages` cross-checks only the last request against the script; fine for I4 but silent about requests sent after Stop.

### Critical Files for Implementation
- /home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs (plan_finished_batch 586-625; FinishedAction subscriber 1177-1380; finish_user_query_send 2342; AfterStreamFinished 5576-5895; cancel_conversation_progress 5131; send_request_input 4785)
- /home/winters/git/phosphor-merge/app/src/ai/blocklist/controller/response_stream.rs (RetryWhenOnline 1282-1291, on_response_stream_complete 1348)
- /home/winters/git/phosphor-merge/app/src/ai/blocklist/action_model.rs (cancel_all_pending_actions 1687, record_stale_cancelled 1489, handle_action_result 1912)
- /home/winters/git/phosphor-merge/app/src/ai/blocklist/agent_fuzz/oracle.rs (add I8 request-count rule and the last-reply tool-call status check)
- /home/winters/git/phosphor-merge/app/src/ai/agent/conversation.rs (derive_status_from_root_task 974; mark_request_cancelled 2328)