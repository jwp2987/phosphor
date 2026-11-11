# Round 16, agent 2: emitted order for a repeated call id (#2054), completion-result notifies (#1995), suggestions in ssh, unreadable secure storage

Branch `fix/r16-agent2` (from integ/r14 fae4cecfd). Owner decisions 2026-11-11 (item 21 of the
decision list). Nothing here was compiled or run (rustfmt only).

## 1. #2054: a repeated call id is sent back where it was emitted

### Cause (by reading, `app/src/ai/agent_providers/chat_stream.rs`)

* A finished reply's calls are taken from the `End`'s captured content, where a repeated id is
  renamed `id#2` (`unique_call_ids`). The renamed call gets its message only then, after the
  messages of the calls whose cards the stream had already shown.
* So reply `[x, x, y]` (y had its card) is recorded `[x, y, x#2]`. The request builder
  (`AssistantBuffer::flush_into_with_group`) moved every `id#n` next to `id`
  (`repeats_beside_their_originals`, #1819) to undo that.
* Reply `[x, y, x]` is recorded `[x, y, x#2]` as well, and the same move sent it back as
  `[x, x#2, y]`: not the order the model emitted, and not what I4's `uniquify` expects. The
  two cases are the same stored messages; the builder cannot tell them apart.

### Fix

Make the recorded order the emitted order, so the builder has nothing to undo.

* The stream loop sets `repeat_may_be_pending` at the first chunk that may be another call
  under an id that is already buffered (`may_repeat_call_id`): a chunk that does not carry on
  the buffered call (`continues_stream_call`, #2048), or one equal to a buffered call that is
  whole (its card is shown, or its arguments are complete JSON). From then on no NEW early card
  is shown; those calls get their messages at the commit, in captured order, after the repeat.
  This is the wait a call behind a `websearch` or a call that failed its first parse already
  has (`tool_order ... all(has a card)`).
* The builder sends the calls as recorded (`request_call_order`).
* Kill switch `PHOSPHOR_AGENT_REPEAT_ID_EMITTED_ORDER=0` (`off`, `false`, `no`): no card is
  held and the builder moves the repeat as before.

Why recorded = emitted then: cards are a leading run of the stream-time order (existing rule).
A renamed repeat never has a card. It sits before a carded call in the captured order only if
that call got its card after the repeat's chunk arrived, which the hold rules out.

### Rejected

* **A marker on the late message that says where it was emitted** (in `server_message_data`,
  or in the message id). `server_message_data` is already the carrier format of a call that
  failed to parse (`<name>\n<args>`), which a repeat can be; every branch of the commit loop
  would have to write it; readers in the controller, the converter and compaction look at the
  field.
* **Swap at the `End`** (put the repeat into the later call's shown card and append that
  call). A card would change the call it stands for under the user's eyes.
* **Only hold after a DETECTED other call, keep or drop the move for the rest.** A repeat that
  equals its original (`dup_call_ids`: grep, the same grep, file_glob) cannot be told from a
  chunk sent twice, and either choice lists one of `[x, x, y]` / `[x, y, x]` out of order.
* **State the builder's rule in I4.** The owner decided on the emitted order.

### Risk

* A false "possible repeat" costs the early cards of the calls after it in that reply (they
  appear when the reply ends). Known case: a tool whose arguments may be empty, on the
  Anthropic adapter, which sends `""` at the block start and `""` again as its first delta; if
  the tool's card shows for `""`, the calls after it wait. For a tool that needs arguments
  (every built-in) `""` shows no card and is not complete JSON: nothing is held.
* A reply that breaks after a possible repeat shows fewer early cards than before (I4 accepts
  any leading part of the early list).
* A conversation saved before this change with a late-RECORDED repeat (`[x, y, x#2]` stored
  for `[x, x, y]`) is now sent as stored. Every call still has its one result.
* One comparison of the buffered arguments per repeated-id chunk, and one JSON validity scan
  when they are equal. Not measured.

### Tests

Unit (`chat_stream::duplicate_call_id_tests`):
`a_chunk_that_may_repeat_its_id_is_told_from_a_growing_call`,
`a_repeat_that_is_not_next_to_its_original_keeps_its_emitted_place`,
`the_request_lists_a_late_repeat_where_it_was_emitted`,
`the_emitted_order_kill_switch_reads_the_usual_off_words`;
`oracle::tests::a_repeat_stays_where_it_was_emitted`.
Corpus: `i4_repeat_id_emitted_late_same_call`, `i4_repeat_id_emitted_late_other_tool`,
`i4_repeat_id_adjacent_then_later_calls`; `dup_call_ids` unchanged.

Not a pin divergence: the BYOP stream is this fork's code, no DECLINED.md row.

## 2. #1995: a completion result notifies `Input` only when render-read state changed

### Cause (named in `docs/design/r14-echoframe.md` section 5 from the round 12 logs)

`Input::handle_completion_suggestions_results` (`app/src/terminal/input.rs`) ended in an
unconditional `ctx.notify()` (cause `handle_completion_suggestions_results_3`). With
completions as you type it runs for every typed character whose result answers the current
buffer. When the menu is closed and stays closed, `set_mode(Closed)` returns early, nothing
else is notified and `Input` is re-rendered alone beside the editor: 16 % (macOS) to 31 %
(Wayland) of terminal-mode key frames.

### Fix (owner decision 2026-11-11: yes, change input.rs)

What the handler can change that `render` or its children read:

| outcome | changes | notify |
|---|---|---|
| result for another buffer | nothing (early return, as before) | no |
| no result / no suggestions, menu closed, not stale, as you type | nothing | **no (new)** |
| no result / no suggestions, a menu up | the menu closes | yes |
| no result / no suggestions, stale flag set | the flag (accept gating) | yes |
| no result / no suggestions, answers a Tab | maybe nothing | yes, kept (once per Tab) |
| suggestions (open, insert, insert prefix + open, no action) | menu, buffer, prepared completions | yes, as before |

`completion_result_needs_render(answers_a_tab, menu_was_up, menu_was_stale, skip_redundant)` is
the decision; `handle_completion_suggestions_results` now returns whether it notified (its one
caller ignores it; the tests read it). Kill switch
`PHOSPHOR_SKIP_REDUNDANT_COMPLETION_NOTIFY=0` (read once).

`completion_menu_stale` is not read by `render` today (it gates Tab/Enter/click accepts), but
the task names it and a set flag implies a menu that was up, so it is kept as a reason.

### Rejected

* **Drop the final notify altogether and rely on `ModeChanged` (`new_10`).** The open branch
  also sets the prepared completions and can clear the editor ghost; nothing proves by
  reading that every such path emits a mode change (an open menu refilled with an equal mode
  does not).
* **Skip the notify for an answered Tab too.** Not per-key, so nothing is gained, and a Tab is
  where a missed frame would be seen.
* **Observe the notify through the window invalidation in the test.** A notify is an effect
  flushed at the end of the outermost update, where the unit-test window builds its scene and
  consumes the invalidation; the return value is exact.

### Risk

A view state this reading missed that only this notify drew. The cases kept (menu up, stale,
Tab, any suggestions) are every branch that writes state; the skipped case writes nothing
(`set_mode` on an equal mode returns before any write). Kill switch if a run shows otherwise.

### Proof a run should give

`latency_key_log`: `input_notify_causes` no longer led by
`Input:handle_completion_suggestions_results_3`; `hop=key_frame_build views=1` for a character
typed into a non-empty buffer in terminal mode.
