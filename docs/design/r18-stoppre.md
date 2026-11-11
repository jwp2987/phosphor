# Round 18: a stop while a tool batch is still preprocessing (#2104)

Fable 5.2, 2026-11-11, branch `fix/r18-stoppre` from integ/r15 ddebd1751. Nothing was compiled or run. Line numbers are
those of the base commit. Follows `docs/design/r18-retire.md` (its "Rejected", last item, and its "guard: kept" section),
`docs/design/r18-agentheal.md` and `docs/design/r13-agentctl.md` section 3.

## The three claims of #2104, by reading

A reply's tool calls are preprocessed as ONE batch before any of them is queued (`queue_actions_without_abandoning`,
`action_model.rs:1567`): the batch is entered in `PendingPreprocessedActions` with the calls' IDS only, and the actions
themselves are moved into the batch's future (`ctx.spawn(join_all(..))`, `:1595`).

**1. A plain Stop leaves the batch's calls with no status and no result, and the turn watch reports them as lost:
CONFIRMED** for every batch whose future does not come back within three main-loop hops.

- Stop: `cancel_conversation_progress` (`controller.rs:5887`) schedules a turn check (`:5908`; the watch has its entry
  from the stream end, `:6452`), and, with no stream in flight, calls `cancel_all_pending_actions` (`:5983`).
- `cancel_all_pending_actions` (`action_model.rs:1973`) calls `abandon_all` (`:1993`), which drains every batch and
  returns only the actions of batches that were `Done` (`preprocess.rs:229-236`). A `Pending` batch is dropped with
  nothing returned: its actions are in the future, not here.
- From then on `get_action_status` (`action_model.rs:1057`) answers `None` for those calls (not pending, not running, no
  result, and no longer in any batch), and the conversation holds no `ToolCallResult` for them.
- That is `turn_invariant::unsettled_actions` (`turn_invariant.rs:224`, the filter at `:244`). The reply finished
  cleanly, no stream is in flight, no close is involved, so `judge` reports `UnsettledCalls` (`:118`). The hops are ready
  futures; after `CONFIRM_ROUNDS` = 3 of them `check_turn_invariant` logs
  `hop=agent_turn_stuck fault=unsettled_calls healed=1` at ERROR (`controller.rs:5833-5842`) and cancels the calls
  through `cancel_discarded_actions` (`:5869`).
- Qualification: the stop also drops the cancel handle of a diff apply still in flight
  (`cancel_all_running_async_actions_for_conversation` -> `cancel_preprocessing`, `execute.rs:1055-1059`), so THAT
  batch's future is woken at the stop and may come back before the third hop; whether it does is a race between two
  chains of foreground tasks, which reading cannot order. A batch the stop cannot end (a hung apply, #1894; any other
  preprocess that is slow) certainly does not.

**2. The late result records a second cancelled result: CONFIRMED.** When the future comes back,
`handle_preprocess_actions_results` (`action_model.rs:1756`) finds no batch ("Action not found", `preprocess.rs:92-96`),
gets every action back as `stale` and passes them to `record_stale_cancelled` (`:1770`), which pushes a cancelled result
per action with no look at `finished_action_results` (`:1684-1705`, the push at `:1700-1703`). After the heal of claim 1
that is the second result of the same call; `cancel_discarded_actions` only skips calls that had a result when IT ran
(`:1956-1962`). `drain_finished_action_results` hands both to the next request.

**3. The late result emits no `FinishedAction`, so a pane close does not discard it: CONFIRMED.**
`record_stale_cancelled` writes the map and nothing else (its own comment: "no `FinishedAction` event, no status write").
A close discards results in two places only: `discard_pending_work_for_close` (before its stop) and the suspended
branch of the `FinishedAction` subscriber (`controller.rs:1366-1374`). A result recorded while the pane is closed, with
no event, passes both; after the undo it is in `finished_action_results` and rides on the user's next prompt
(`drain_finished_action_results`, `controller.rs:2207`). #848 says nothing of the stopped reply goes out after an undo.

## Root

One fact: a dropped batch's calls were settled when the batch's future came back, not when the batch was dropped. In
between nothing held them (claim 1), and the settling then happened behind the back of whatever the pane was doing by
that time (claims 2 and 3).

## Chosen fix

A batch's calls are settled when the batch is dropped. What its future brings back later is dropped.

1. **The batch keeps a copy of its actions** (`PreprocessActionBatch::held`, `insert_preprocess_actions`). The
   originals still travel with the future. This is what r18-retire's rejected item lacked ("the dropped batch holds
   action ids, not actions").
2. **A stop cancels them at the stop.** `cancel_all_pending_actions` takes the copies of every batch still
   preprocessing (`abandon_all_settling`) and gives each call its cancelled result through `cancel_pending_action`,
   the path every parked, queued and blocked action of the stopped turn already takes: one result, a `FinishedAction`
   with the stop's reason, the executor's per-action state released. They are cancelled last, after the queue is
   empty, so settling one cannot start a queued action of the turn being stopped. Consequences:
   - the turn watch finds them settled on its first hop: no false ERROR (claim 1);
   - a plain Stop ends the turn through those `FinishedAction`s like any stop with a card waiting; the special
     "nothing will report" arm of #1944 (`controller.rs:6003`) is no longer reached for this case (it stays for the
     kill switch and for #2029's deferred follow-up);
   - a pane close's stop emits them while the pane is suspended, so the suspended branch discards them with the
     others and their calls keep their cancelled status (#2103) (claim 3);
   - a superseding submit (`send_query`) now has a result for those calls in the same request, where they used to
     arrive with some later one.
3. **The late result is dropped.** The model remembers the calls it settled this way
   (`settled_while_preprocessing`, keyed by conversation and id). `handle_preprocess_actions_results` takes them out of
   the stale list before anything is recorded (`drop_late_preprocess_results`), releases what the late preprocess left
   in the executor, and logs once per late batch:
   `hop=agent_late_preprocess_result dropped=<n> recorded=<m> conversation=<id>`. The same step drops a late result for
   a call that already has a finished, unsent result for any other reason (claim 2 for every writer, not only the
   heal).
4. **The other place a batch is dropped gets the same rule.** A new turn gives up on earlier batches that are still
   preprocessing (`queue_actions`, #1859) and so far left their calls to the late result as well. They are now recorded
   as cancelled there (`take_actions_of_abandoned`, then the existing `record_stale_cancelled`, which is how that
   site already settles the earlier turn's parked actions: silently, the new turn is starting). With this, no result
   is ever recorded by a late preprocess result while the switch is on, which is what makes claim 3 hold for every
   path and not only for the stop.
5. **Kill switch** `PHOSPHOR_STOP_SETTLES_PREPROCESSING=0` (also `off`, `false`, `no`), read at each stop, new turn and
   late result: all of the above is off and the code does what it did (the copies are kept and ignored).

No timer was added (`script/check_input_path_timers`: ok). Nothing on the key path changed.

## The r18-agentheal guard and the r18-retire table

`replies_discarded_for_close` stays. With both switches on it now has nothing left to excuse (a close's stop settles
the preprocessing calls and the close keeps their status); it remains the cover for the two kill switches. Its comments
say so. Removing it is a separate, later step once this has run.

r18-retire's table row "still preprocessing at the close (no result yet): cancelled icon (orphaned)" becomes: a
cancelled result, kept by the close, cancelled icon. The look is the same; the call now has a status.

## Rejected

- **Teach the turn watch about a dropped batch whose future is outstanding** (the issue's second option): it answers
  claim 1 only, leaves the calls with no status for as long as the apply hangs (forever, for a hung one), and leaves
  claims 2 and 3.
- **Keep the dropped batch as a marker and settle on its late result, with a de-duplication there only:** claim 1 and
  the hung apply remain.
- **Make `record_stale_cancelled` emit `FinishedAction`:** at a new turn it would run the end-of-step planning in the
  middle of queueing the new batch; and for a late result it would end a turn that ended long ago a second time.
- **Read the calls from the conversation's last exchange at the stop** instead of keeping copies: the action model
  would depend on the history model's view of the reply, and the new-turn site drops batches of an exchange that is no
  longer the last one.
- **Performance:** the copy is one clone per tool call per reply (the controller already clones each call out of the
  reply to queue it). Not measured; a diff's text is copied once more and freed when the batch completes.

## Risk

Medium-low. The stop path gains results it used to get later (or never); the paths they then take are the existing ones
for a cancelled pending action. What cannot be seen without a run:

- a plain Stop during preprocessing now ends the turn through the `FinishedAction` subscriber instead of the #1944 arm
  (status, queued-row unlock, `QueuedPromptsReleased`): the subscriber does all three for a stop on a waiting card, and
  the fuzz I2b / I3 judge them;
- cards: a call that was shown while the reply streamed and never left preprocessing now gets a `FinishedAction` with
  no `QueuedAction` before it. The #1967 heal and #1861 / #1946 already do that through `cancel_discarded_actions`;
- `settled_while_preprocessing` keeps one id per call whose batch never comes back (a hung apply). Bounded by the
  number of such calls; never read for anything but dropping that call's late result.

## Tests

See `docs/testing.md`, "Round 18: a stop while a tool batch is still preprocessing". In short: three pure tests of the
batch bookkeeping, six controller tests with the batch held in preprocessing by a test hold (the stop, the late result
after a second writer, the pane close, the new turn, the kill switch's arm as the control, the stop's report), two
agent fuzz corpus scenarios (`stop2104_*`: a hung apply; a late result followed by the user's next message) with a
schema field `env.preprocess` to hold a batch, and one fuzz test that pins what the replay cannot say. The paced fuzz
oracle is unchanged.

## Files not touched

`app/src/terminal/input.rs`, `input/enter_queue.rs`, `view/give_up_to_pty.rs`, `agent_fuzz/oracle.rs`,
`agent_fuzz/runner.rs`.
