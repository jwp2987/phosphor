# Round 18: the note of a restored pane whose close discarded results (#2103, #848)

Fable 5.2, 2026-11-11, branch `fix/r18-closenote` from integ/r15 4714d38f3. Nothing was compiled or run. Follows
`docs/design/r18-retire.md` ("Owner decisions needed", item 1), which the owner decided on 2026-11-11.

## Decision

After a pane close and its undo, the card of a call can say it ran (the close kept its status, #2103) while the model is
told on the next request that the reply's calls were interrupted (#848: a discarded result is never sent). The restored
reply now says so:

> Results from before this was closed were not sent to the agent. Tell it what happened if it matters.

Owner decisions of 2026-11-11 on the first version (branch `fix/r18-closenote2`, nothing compiled or run):

1. **Narrowed.** The note shows only when at least one of the reply's results kept by the close is not a cancellation
   (the call finished: it succeeded or failed). A reply whose discarded results are all cancelled calls (including the
   ones the close's own stop or #2104's stop-settling cancelled) has no note: card (cancelled) and model (interrupted)
   already agree. "Cancelled" is the result type's own notion, `AIAgentActionResultType::is_cancelled`
   (`crates/ai/src/agent/action_result/mod.rs`): cancelled before it ran, or a requested command interrupted with exit
   130. No string matching.
2. **Wording.** "this was closed", not "this tab was closed": the same line fits a tab and a split pane.
3. **Lifetime** stays as history (below, unchanged).

It is for the screen only: never part of a request, of copy / export, or of the conversation.

## Where the facts are (base commit)

- The close keeps the discarded results in `BlocklistAIActionModel::results_discarded_by_close`
  (`action_model.rs`, `discard_finished_action_results_for_close`), only when the kill switch
  `PHOSPHOR_CLOSE_KEEPS_CALL_STATUS` is not off, and logs `hop=agent_close_discard results=<n> kept_status=<0|1>`
  (`controller.rs`, `discard_finished_results_for_close`) when `n > 0`.
- A reply is drawn by its own block: `block/view_impl/output.rs` `render`, which has the pane's action model
  (`Props::action_model`) and the reply's calls (`AIAgentOutput::actions`).
- Requests are built from inputs and task messages; copy / export from `format_for_copy` (messages of the output and
  `get_action_result_for_export`). Neither reads anything a block draws around the reply (the "Stopped task" row, the
  footers).

## Chosen fix

1. No new state. The note is derived: a reply shows it exactly when one of its calls has a result in
   `results_discarded_by_close` that is not a cancellation (`is_cancelled()` is false) and is not also a sent or
   pending result (`BlocklistAIActionModel::results_withheld_by_close_note`, `is_result_withheld_by_close`). The map
   gets an entry only on the path where the close logs `results>=1 kept_status=1`; that log line still counts cancelled
   results, so it is necessary for the note and no longer sufficient. With the kill
   switch off the map stays empty, so there is no note. Nothing to set at the close, clear at the undo or keep in step.
2. `render` adds it as the last line of the reply (after the stop row), so in a restored pane it is the line right
   above the input. `results_withheld_by_close_note(props, app)` asks the action model one cheap question first
   (`has_results_withheld_by_close`: is the map empty) and reads the reply's calls only when a close kept something in
   this pane; every other frame of every block pays one `is_empty`.
3. The text is the constant `RESULTS_WITHHELD_BY_CLOSE_NOTE` (`action_model.rs`). It is not an input, an output message
   or a result; it exists only as the element `render` builds. So it cannot be in a request or in the history, and
   `format_for_copy` / `export_to_markdown` (the text `terminal/input.rs` can attach to a prompt) never see it. The
   element is not selectable, so a selection dragged over the reply does not copy it either.
4. Look: plain wrapping text (`Text::new`, soft wrap) in the secondary text colour (`blended_colors::text_sub`, the colour
   of the hint under a card), at the reply's left margin. No icon, no colour carrying the meaning, not the disabled grey
   of `render_informational_footer` (and not its `new_inline`, which does not wrap: the line is long and a narrow pane
   would cut it).

## Lifetime: it stays as history (picked)

The note stays under its reply for as long as the cards it explains show the kept status: until the conversation is
reverted past the reply or the app restarts (the kept results are not persisted, r18-retire; after a restart the cards
have no status and there is no note). It does not go away when the next prompt is sent. Why:

- What it explains does not go away then. The card still says the command ran, and the first reply after the next prompt
  is exactly where the model shows it does not know ("the command was interrupted"). Removing the line at the send would
  hide the explanation at the moment the user needs it.
- Its first sentence is a fact about that reply and stays true. Its second ("Tell it ...") stays possible.
- It needs no state. "Gone at the next prompt" needs a per-conversation marker written at the close and cleared on every
  send path (prompt, queued prompt, resume, steer), plus a redraw of an old block: a second thing to keep in step with
  the map, for a line that is one row of text.
- "Once per restore": one line per reply, however many of its results were discarded and however often the pane is
  closed and restored afterwards. A later close that discards results of a later reply puts its own line under that one.

## Rejected

- A marker in the controller next to `replies_discarded_for_close`: that map is also written when nothing was discarded
  and in the kill switch's arm, and the block's `Props` does not carry the controller.
- An output message or an input of a "note" kind in the conversation: it would be in the history, in export, and one
  mapping away from a request, which is the one thing the decision forbids.
- A banner on the pane or the input: not "with that reply", and it needs dismissal state.
- (First version only) counting every discarded result, cancelled ones included: the owner narrowed it, see "Decision".

## Decided by the owner (was the open question)

The first version showed the note for any discarded result, so a pane closed while its only call was still waiting or
running showed it after the undo although card and model agreed. The owner narrowed it (item 1 under "Decision") and
picked wording that names neither tab nor pane (item 2). The app case `agent_close_undo_call_status` closes on a command
that ran to the end (action status `done`, exit 0, not interrupted), so it still expects the note; the question the
close cancels would not bring it by itself.

One consequence to know: a command the user or the close interrupted (exit 130, `interrupted`) counts as cancelled by
`is_cancelled`, even if it printed output before that. Its card shows it as cancelled, so the rule holds as stated.

## Risk

Low. One read-only derivation in `render`, behind an `is_empty` check (the check is on the whole map, so a pane whose
close kept only cancelled results reads its replies' call ids each frame and finds nothing: a few map lookups); no new state, no timer, no key path, no request
path. Redraw: the block must be redrawn after the close for the line to appear, exactly as for the kept status on its
cards (r18-retire); a restored pane is laid out again. Not compiled.

## Files not touched

`app/src/terminal/input.rs`, `input/enter_queue.rs`, `view/give_up_to_pty.rs`.
