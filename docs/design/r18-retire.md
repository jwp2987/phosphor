# Round 18: a pane close keeps the status of the calls whose results it discards (#2103, #848)

Fable 5.2, 2026-11-11, branch `fix/r18-retire` from integ/r15 a63a5f626. Nothing was compiled or run. Follows
`docs/design/r18-agentheal.md` (section A, "Rejected", second item), which the owner decided on 2026-11-11.

## Decision and its condition

When a pane close discards the finished, unsent results of the reply it stops (`discard_pending_work_for_close`, and the
suspended `FinishedAction` branch, #848), the calls of those results keep their status instead of being left with none.

Condition (owner, through the coordinator): #848 erased those results on purpose, so that a reply the user abandoned by
closing its pane is never sent to the model later. The kept results are for display and bookkeeping only and must never
reach a request. If that could not be done cleanly, nothing was to change.

## What the code did (base commit)

- `BlocklistAIActionModel::clear_finished_action_results` (`action_model.rs:2100`) removed the conversation's
  `finished_action_results`. A call of them then had no status at all (`get_action_status`, `:1020`).
- Cards: a call with no status in a finished reply is "orphaned" and gets the cancelled icon
  (`is_orphaned_by_finished_output`, `block/view_impl/output.rs:3289`, used by `action_icon`), and is never hidden by "hide
  completed tool cards" (`should_hide_completed_action`, `:209`, returns false with no status). So after an undo-close a
  command that had run and succeeded showed as cancelled.
- The turn watch (#1967) and the fuzz oracle (I1) read such a call as held by nothing; r18-agentheal added a guard for the
  first (`replies_discarded_for_close`) and the oracle had an exception for the second (`Oracle::tab_closed`).

## Every path from a kept result to a request

`past_action_results` (results that were sent, or restored from a persisted conversation) is read in two places only:
`get_action_result` and `restore_action_results_from_exchanges` (a write). No request is built from it. Requests take
results from `finished_action_results` alone: `drain_finished_action_results` (`controller.rs:2166` the user's prompt,
`:3428` the follow-up, `:3622`), `get_finished_action_results` (`:1419`, `:1877`, `:3293`: whether to follow up),
`finished_action_results_matching` / `remove_...` (`:4627`, the BYOP preflight). The preflight's repair for a tool call with
no result is a synthesized "interrupted" placeholder (`synthesize_byop_missing_cancellation_results`); it does not look at
the action model's old results. Resume, restore after undo and restore after a restart build from the conversation's
tasks; the action model's maps are not persisted.

One reader of `get_action_result` does lead to a request: `AIAgentOutput::format_for_copy` (`ai/agent/mod.rs:625`), through
`AIConversation::export_to_markdown`, which `terminal/input.rs:10511` turns into a plain-text attachment when a
conversation is attached to a prompt (the other callers write the clipboard or a file). A result kept in
`past_action_results` would have been in that text.

## Chosen fix

1. The kept results live in their own map, `results_discarded_by_close`, not in `past_action_results`.
   `discard_finished_action_results_for_close(conversation, keep_status)` moves the finished results there (or drops
   them when `keep_status` is false). `get_action_result` (cards, status) reads finished, past, then this map;
   the new `get_action_result_for_export` reads finished and past only, and `format_for_copy` uses it. So copy, export and
   the conversation attachment are exactly what they were when the results were erased, and no request path can see a
   discarded result. `terminal/input.rs` is not edited.
2. Both close sites go through `BlocklistAIController::discard_finished_results_for_close`, which logs
   `hop=agent_close_discard conversation=<id> results=<n> kept_status=<0|1>` when it discarded something.
3. Kill switch `PHOSPHOR_CLOSE_KEEPS_CALL_STATUS=0` (also `off`, `false`, `no`), read at each close: the results are
   erased as before.
4. The real result is kept, not a rewritten one: the card of a call that ran says it ran. The model is told by the next
   request's preflight that the reply's unanswered calls were interrupted, as before; see the owner note in the report.

## What the cards of a restored pane show (read from the card code; pinned by `discarded_by_close_tests`)

| the call, before the close | "hide completed tool cards" off | on |
|---|---|---|
| ran and succeeded | its card, green check (`action_icon`) | hidden, like every completed card of an older reply |
| failed | its card, red cross | shown |
| cancelled (by the user, or by the close's stop) | its card, cancelled icon, the card's own cancelled wording | shown |
| still preprocessing at the close (no result yet) | cancelled icon (orphaned), as before | shown |

Before: every row was the last one. A long-running command's snapshot result keeps the running icon, as it does after an
ordinary send (`action_icon`, `LongRunningCommandSnapshot`); the close interrupts only the agent's own command.

## The r18-agentheal guard: kept

`replies_discarded_for_close` / `TurnFacts::reply_discarded_for_close` is NOT redundant. The kept status settles every
call that had a result. It does not cover:

- a batch still preprocessing at the close. The close's stop drops it (`cancel_all_pending_actions` ->
  `PendingPreprocessedActions::abandon_all`, `action_model/preprocess.rs:229`); its calls get a cancelled result only when
  the batch's future comes back (`action_model.rs`, "actions in hand whose batch was abandoned", #1894), which a hung
  apply never does. Until then: no status, no result, on purpose. Without the guard the next Stop or superseding submit
  after the undo logs a false `hop=agent_turn_stuck`;
- the kill switch's arm, where the close erases as before.

Its comments now say this. Its cost is unchanged from r18-agentheal: a really lost call in a reply a close stopped is not
healed until the conversation has a newer exchange.

## The fuzz oracle's `tab_closed` exception: removed (kept only for the kill switch)

What it excused: after a `CloseTab` the "status or result" half of I1 was off for the rest of the scenario, because the
rig's close (`discard_pending_work_for_close` alone: no suspension, no stop) erased finished results and left their calls
with no status. Why it no longer applies: the same call now moves those results to `results_discarded_by_close`, and
`Rig::unsettled_calls` reads `get_action_status`, which answers `Finished` for them. The rig's close touches no other
action state (it drops a stashed request, pending passive work, an auto-resume handle, the deferred success, the
compaction state), so a call that had a status before it has one after it. `Oracle::tab_closed(results_erased)` keeps the
exception for `results_erased = true` only, which the runner passes when the kill switch is off. The oracle judges more
than before, never less.

## Rejected

- Moving the results into `past_action_results` (the three-line version): the copy / export text reads that map and can
  be attached to a prompt, so the condition would not hold.
- Keeping only a "cancelled" marker per call instead of the result: the card of a command that ran would still say
  cancelled, which is the display defect the decision removes.
- Arming the guard only in the kill switch's arm: the preprocessing case above.
- Giving the dropped preprocessing calls a cancelled result at the close: the dropped batch holds action ids, not
  actions (`PreprocessActionBatch`), and the late result path already cancels them; a second writer for the same ids is a
  double-result risk for no user-visible gain.

## Risk

Low. Requests read the same single map as before, and it is emptied at the close exactly as before. New state: one map,
written at a close, read by `get_action_result`. Stale entries (a later revert, a closed conversation) are never looked
up for a request and cost a few results per close. With "hide completed tool cards" on, completed cards of the stopped
reply are hidden after an undo where they used to reappear as cancelled: the intended reading of the setting.

## Files not touched

`app/src/terminal/input.rs`, `input/enter_queue.rs`, `view/give_up_to_pty.rs`. No timer was added.
