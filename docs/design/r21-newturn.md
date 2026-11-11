# r21-newturn: a new turn over a batch still preprocessing left its calls unsettled (#2136, follow-up of #2104)

Nothing was compiled or run when this was written.

## Evidence

Round 15 targeted unit run, macOS: 12078 of 12079 passed. The one failure:
`ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing`,
at its first assertion (`controller_tests.rs:4663`, "earlier-a is cancelled when the new turn gives up on its batch").
The log line just before the panic shows the new-turn branch was taken:
`hop=agent_action status=parked abandoned unresolved preprocess batches ids=[earlier-b, earlier-a] at new turn`.
The other tests of the module (the stop path) passed.

## Verdict: APP

The test states the #2104 rule (design `r18-stoppre.md`, point 4): a new turn settles the calls of an earlier batch
that is still preprocessing, then and there. The code did not.

Where they part, `action_model.rs`, `queue_actions`, in this order:

1. `batches.abandon_pending()`: the earlier batch becomes `Abandoned`. No batch is `Pending` after this.
2. `batches.release_ready()` (`preprocess.rs`): pops every leading batch that is not `Pending`, and for an
   `Abandoned` one does nothing with it. The marker of the batch just abandoned is dropped here, and its kept copies
   (`held`) with it.
3. `batches.take_actions_of_abandoned()`: walks an empty queue, returns nothing.

So nothing was marked settled and nothing recorded. `get_action_status` found the ids nowhere (the batch was gone
from `pending_preprocessed_actions` too) and returned `None`; the test's `is_some_and(is_cancelled)` failed. In the
app: the earlier calls have no status until the batch's future comes back, when the pre-#2104 path records the
cancelled result silently (missing batch, stale, not marked settled, so `record_stale_cancelled`), or never if the
apply hangs. That is the state #2104 set out to remove, on the new-turn path only. The stop path uses
`abandon_all_settling`, which reads the copies as it drains.

The unit test `take_actions_of_abandoned_hands_the_calls_back_once_and_keeps_the_markers` did not catch it: it goes
abandon, take, with no `release_ready` in between.

## Fix

`queue_actions` takes the kept copies BEFORE `release_ready`. Nothing else moves: the parked actions are still
recorded first, then the calls that were still preprocessing; the kill switch arm still takes nothing. The late
result of the abandoned batch is that of a missing batch, handed back as stale, and `drop_late_preprocess_results`
drops it because the calls are marked settled (what the test's second half asserts: one result per call).

## Rejected

- Changing the test (expecting the result only after the late future): that is the pre-#2104 behaviour and would
  weaken it.
- Making `release_ready` keep abandoned markers: #1859 needs them gone (a new batch must not park behind them), and
  the self-heal's `parked_count` reads the same queue.
- One combined `abandon_for_new_turn` method returning both lists: cleaner, but a larger change to code five passing
  tests cover; the order is now stated at both methods and pinned by a unit test.

## Risk

Small. Two statements change places; both lists are computed before either is recorded, as before. No timer, nothing
on the key path.
