# #2227: held input is delivered on fresh facts (design, UNREVIEWED, no code)

Written by reading the code at `a811316f4`; nothing was compiled or run. Paths are relative to
`app/src/terminal`.

## The defect

`decide_delivery` (`view/dest_table.rs:228-266`) decides where a held input goes with
`build_route_facts` (`view/route_facts.rs:150`), which never waits for another thread:

- the model facts come from `model_facts_nonblocking` (`route_facts.rs:47-63`): the exact model
  if `try_lock` succeeds, else the last PUBLISHED snapshot (`exact: false`);
- the hand-off comes from `try_read` (`route_facts.rs:160-173`): live if free, else the last
  value this view cached (`handoff_known: false`).

`pty_typed` (`input_queue/table.rs:468`) is true when `active_block_started` (model) or
`handoff_routes` (hand-off) is. In r30f the ssh connect hold was released in the same view pass
that committed the prompt to the editor, one of the two reads was stale, and the held keys were
decided `D6 Pty(Typed)` instead of `D7 EditorInsert`. The log does not say which read it was.

Only DELIVERIES are in scope (`dest_table.rs:249`, origins `Held` and `Replay`). A live key
(`dest_table.rs:834`, `:918`, `:1283`) keeps its non-blocking facts: it is on the latency path
and has its own stamp rules.

## Options

| | what | verdict |
|---|---|---|
| A | Deliver from the event that has fresh facts (pass the precmd handler's state in) | The precmd handler has no fresher model than anyone: it also cannot take the model lock. It knows the hand-off just committed, which B gives for free |
| B | Hand-off: an exact read at a delivery | **Recommended, part 1.** The hand-off lock is a leaf held for one method call, never across a parse (`model/typeahead_handoff.rs:831-838`); the write path already takes it blocking for every key (`view.rs`, `write_user_bytes_to_pty_after_paste_stage`). It is not the pty reader's lock |
| C | Model: do not start the hand-over on an inexact snapshot; retry at the next model wakeup | **Recommended, part 2.** An event, not a timer: the pty reader wakes the view after each parsed batch, and `deliver_ssh_typeahead_if_ready("output", ..)` already runs there (`view.rs:16571`) |
| D | Take the model lock blocking for a delivery | Rejected: that is the pty reader's lock (#1403) |

## The design (B + C)

1. **Hand-off exact at a delivery.** In `decide_delivery`, when `facts.handoff_known` is false,
   read the hand-off with `self.handoff.read()` and set `handoff_routes`, `handoff_known`,
   `editor_takeover_open`, `keys_follow_editor` from it (and refresh the cache). One short
   leaf-lock wait, only for a delivered entry, only when `try_read` just failed.
2. **The ssh hand-over starts on an exact model read.** In `deliver_ssh_typeahead_if_ready`
   (`view.rs:12567`), after the gate and readiness checks and before `finish_ssh_typeahead`:
   call `model_facts_nonblocking()`. If `exact` is false, the hand-over does not run now: the
   keys stay held, a flag `ssh_handover_awaits_exact_model` is set, and the function returns
   false. The next call (the model's wakeup after the batch it was parsing, `source="output"`,
   or any later event that already calls it: precmd, block completed, session bootstrapped, a
   key) tries again. The exact read is stored into the published cells by
   `model_facts_nonblocking` itself (`route_facts.rs:52-53`), so a `try_lock` that fails during
   the delivery falls back to a snapshot no older than that read.
3. **No new timer.** If the model is never free at an event, the hold's existing give-up
   (`schedule_ssh_typeahead_give_up`) ends it as today: the keys go to the pty in typed order.
4. **Other held deliveries** (`Replay`: keys behind an Enter that waited; a deferred Enter's
   keys) get part 1 through the same `decide_delivery`. Part 2 is the ssh hand-over's only:
   those other holds are released by hand-off events whose handlers act on the hand-off, not
   on `active_block_started`. If the diagnostic below ever fires for them, the same "start on
   an exact read" check goes into their release function.

## With a811316f4

Keep it, as the safety net. It acts at the write, after the decision: if a delivered ssh key
still reaches the pty at an editor-owned prompt (the give-up of point 3, or a stale source this
design did not foresee), the line becomes the pty's and is not split. With this design it
should stop firing on the hand-over path; its hop `handoff_line_taken_for_delivered_typeahead`
then becomes a defect indicator a run can be failed on.

## Diagnostic

One hop per held delivery that was decided on anything but fresh facts, counts and names only:
`delivery_facts origin=Held row=.. model_exact=0|1 model_consistent=0|1 facts_gen=N
handoff_known=0|1 active_block_started=0|1 handoff_routes=0|1`. And when the hand-over is put
off: `ssh_handover_awaits_exact_model source=<event>` with the count of deferrals, so a run
shows how often and for how many events the model was busy.

## Tests

- View (`input_test.rs`), the r30f sequence with the stale source forced: the model lock held
  by a second thread while the gate releases and the prompt commits. Expected: no delivery
  while it is held; after the lock is released and a wakeup arrives, every held key is decided
  `D7` and lands in the editor in typed order; nothing reaches the pty.
- View: the hand-off write-locked by a second thread at the delivery: the delivery waits for
  it and decides on the committed state (`D7`).
- View: the model never free: the hold's give-up delivers to the pty and a811316f4 takes the
  line (pty gets the whole line in order, editor empty).
- Unit (`input_queue/table_tests.rs`): `pty_typed` with `handoff_known=false` is unchanged
  (the table is not touched).
- Lab, agent AND terminal mode, Linux, macOS and Windows lab servers:
  `lab_connect_typeahead.zsh.{lan,150}.{helper,nohelper}` and the bash arms. Pass criteria
  beyond the case's own: zero `origin=Held row=D6` after an `ssh_held_delivery` at a remote
  prompt, zero `handoff_line_taken_for_delivered_typeahead`, and
  `ssh_handover_awaits_exact_model` counts reported per run. `lab_ssh_drop_reconnect`,
  `connect_typeahead` (Windows port) and the latency cases unchanged against their bands.

## Risks

- The hand-over waits for one parsed batch while the pty reader is busy: milliseconds
  normally, longer under an output flood at connect. The keys are already held then, so
  nothing is reordered; they only appear later.
- Part 1 adds a blocking leaf-lock read on the main thread for delivered entries. The same
  thread already takes that lock for writing on every key.

## Owner question

While a remote shell is still printing at connect, the keys you typed ahead would wait a
moment longer (until the app has finished reading that burst of output) before they show up,
so that they always land in one place. Is that wait acceptable? Recommended: yes; it is an
event, not a timer, and the existing give-up still bounds it.
