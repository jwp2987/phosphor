# #2209: no typed key is dropped when a hold ends (design; APPROVED by the owner 2026-11-14, built: section 9)

Status: design only. No code changed. Written by reading the code at `36efe34b5`
(`integ/r15`); nothing here was compiled or run. Round 2 answers the refute of `ad21f0890`
(section 7 maps each finding) and round 3 the re-check of `49bc0045a` (section 8). Paths are relative to `app/src/terminal`; `h` =
`model/typeahead_handoff.rs`, `pq` = `view/paste_queue.rs`, `view` = `view.rs`,
`giveup` = `view/give_up_to_pty.rs`, `pc` = `writeable_pty/pty_controller.rs`.

## 0. Three facts that change how the issue reads

1. **"The session ends" is the pane's pty dying, not ssh ending.** `reset_on_exit` (`h:5316`)
   has one caller, `TerminalModel::exit` (`model/terminal_model.rs:1677`), called only for
   `ShellProcessExited`, `PtyDisconnected`, `ShellNotFound`, `PtySpawnFailed`. When an ssh
   inside the pane ends, the local shell prompts and the hold ends by its normal events. On
   this path there is **no next reader**: a plain terminal closes its window.
2. **A held key has one way to be discarded:** `YoungHoldEnd::Dropped`, applied in one arm
   (`pq:1640-1642`).
3. **The pty-gone rule exists** (owner decision 2026-11-09, `giveup:53-54`, `giveup:445-483`):
   the keys become text in this pane's input, never dropped silently, never written elsewhere.

## 1. The drop paths

"Reachable" is by reading; none was reproduced.

**The editor-first race (#1016) is the common trigger for 1b, 2 and 4.** Keys can reach the
focused editor before the terminal takes them (`input.rs:17444-17447`). With a young hold
queued behind an outstanding request (`AwaitingReport`), an Enter that reaches the editor
calls `defer_enter` (`h:5183`), which checks only the state. The result is
`HoldAfterEnter { released: false }` with `young_hold_queued` still set; the report arms for
that state (`h:5280-5297`) do not take the hold. Round 1 called this unreachable. It is not.

### Path 1a: young hold, pty exits

- Trigger: `reset_on_exit` -> `end_unowned_young_hold` (`h:5321`, `h:1662-1677`).
- Scenario: `make` finishes; the user types `ls`, Enter, `pwd`, Enter at once (young Enter held
  under a `PromptSeen` gate, `h:4214-4233`); the shell is killed before the report.
- Now: `Dropped` is recorded, and nothing applies it at exit: the view's `ModelEvent::Exit` arm
  (`view:19238-19319`) never drains the ends. The keys sit until an unrelated event or the
  gate's 10 s cap, then `pq:1640` discards them with no log line and no toast.
- Plain terminal: the window closes, the keys are lost. Next reader: **nothing** -> input text.

### Path 1b: young hold, state left `PtyOwned` / `AwaitingReport`

- Trigger: `drop_stale_hold` (`h:4472-4480`) from `check_queued_young_hold` (`h:1478`),
  `route_user_bytes` (`h:4175`), `expire_queued_young_hold` (`h:4884`).
- Reachable: the race above, then the report (`HoldAfterEnter { released: true }` with the
  hold queued); the next key drops it. Also any queue / state disagreement (`h:4177-4183`).
- Scenario: `abc`, Enter (held young), `k`, then a second Enter that lands in the editor.
- Plain terminal / next reader: the keys are in the tty; the shell reads them in typed order.

### Path 2: young hold, a command is written from a state other than `PtyOwned`

- Trigger: `on_command_written` (`h:3553-3565`), reached only for a
  `CommandExecutionSource::User` command on a reporting shell (`pc:2661-2669`).
- Reachable two ways: (a) the race above: the report runs the deferred Enter, the editor
  submits, and the command is written from `HoldAfterEnter` with the hold queued; (b) a submit
  that is not a key (a click: re-run, a workflow, an accepted suggestion) while the hold waits
  behind its request. `SharedSession` / `QueuedCommand` sources do not get here: they are held
  by `must_hold_command_behind_unreported_user_line` (`pc:2814-2840`).
- Now: the held Enter and the keys behind it are discarded.
- Next reader: the shell; but the head of the held Enter's line is in the shell's line and the
  command's kill prefix wipes it, so writing the Enter after the command runs later lines
  without the line before them (section 3, W).

### Path 3: keys behind a deferred Enter

- 3a, pty exits: `reset_on_exit` (`h:5322-5324`). Now: as 1a, except that the gate's 60 s leak
  net can get there first and then keeps them (`view:15670-15697`): the outcome depends on
  which timer wins. Next reader: nothing; the Enter's line is in the editor, unrun.
- 3b, "the deferred Enter is cancelled": `drop_deferred_gate` (`h:5509`) has one caller,
  `cancel_deferred_enter` (`h:5483`), called for Ctrl-C (`view:10708`) and for the app's own
  interrupts (`view:10604`). **This is path 5.**

### Path 4: young hold at a report-deferral give-up

- Trigger: `give_up_report_deferral` (`h:3343`), `give_up_deferred_gate` (`h:3364`).
- Reachable: the race above, then no report within the patience (a program ate the request).
- Now: the young hold's Enter and keys are discarded; the keys behind the deferred Enter are
  kept with the line (`view:15600-15661`).

### Path 5: signals (kept)

- `^C`, `^Z`, `^\`: a POSIX tty with `ISIG` and no `NOFLSH` discards its input queue on INTR /
  QUIT / SUSP. Real flush. Sites: `h:4171`, `h:5439`, `h:5502`, `pq:1454-1491`.
- `h:5439` (`on_interrupt_for_shell`, caller `view:16369`) runs for every `^C` written while
  the pty owns input, the app's own (Stop, a dismissed warpify / tmux-install block, rewind,
  #1774) included: those void a young hold as well as a deferred Enter's keys (Q3).
- `^D` is not a tty flush, and the code keeps keys across a lone EOF while the pty owns input
  (`h:4872-4874`, `pq:1484-1490`). One sub-case voids them: a young hold behind an outstanding
  request. Listed as F3, not changed here.
- Under ConPTY the tty does not flush bytes already written (`h:1370-1377`); voiding app-held
  keys on `^C` there is the owner's rule, not a tty fact.

## 2. Other places that can discard typed bytes

Found 13, filed 0 (the coordinator files).

| # | where | what | verdict |
|---|---|---|---|
| F1 | `view:19238-19319` | The exit arm applies no recorded hold end | in scope (V3) |
| F2 | `h:1665-1670`, `pq:1648-1664` | #2207's escape hold at pty exit is written to a pty that no longer exists. The write is lost, not refused: `send_message_to_event_loop` turns `Disconnected` into an event and drops the message (`pc:3555-3558`); with the channel open and the fd closed the event loop logs and shuts down (`local_tty/event_loop.rs:1594-1618`) | in scope (V2), Q1 |
| F3 | `h:4872-4874`, `pq:1484-1486` | `^D` voids a young hold behind an outstanding request | not a tty flush; separate issue |
| F4 | `pq:2288-2291` | `take_entry_bytes` warns and drops an entry kind it does not know (`Raw`, a `Held` paste) in a hand-off gate; `giveup:163-240` handles every kind | separate issue; the new ends use the complete converter |
| F5 | `input_queue/mod.rs:1060-1066`, `pq:685-697` | Queue full (64 x 1024 entries): newest key refused, toast | deliberate (#1442) |
| F6 | `view/ssh_queue.rs:471-478`, `view:11784` | ssh connect hold overflow discards the run, toast | deliberate (#1292) |
| F7 | `view:12476-12500` | ssh reads a secret: connect typeahead discarded like `TCSAFLUSH`, toast | deliberate (#1611) |
| F8 | `view:12551-12553` from `view:13056` | Every command submit calls `reset_ssh_typeahead` -> `ssh_hold_clear` -> `SshDropThen::Discard` (`ssh_queue.rs:728-730`), logged at info only. While a connect hold is live every key is held (table row `H3`), so the submit must be a click or an app command: keys typed during an ssh connect are then discarded silently | real by reading, not reproduced; separate issue |
| F9 | `input_queue/table.rs` rows `D8`, `D10` (`table.rs:42-43`), `destination.rs:77-91` | `D8`: a typed key, the pty refuses it and the input box is hidden -> `Drop(RefusedHiddenInput)`. `D10`: a control key / sequence / live Enter with nothing running -> `Drop(NotLongRunning)` | typed keys dropped by routing where no destination exists; pre-dates the hand-off; separate issue |
| F10 | `pq:1661` | The `Typeahead` arm emits `WriteHandoffUserBytesToPty` directly: no agent-in-control or hidden-input guard, no input-text fallback | pre-existing for every `Typeahead` end; the new ends do not use it; separate issue |
| F11 | `h:5147-5153`, `pc:1423` | A `Typeahead` end written while the state is not `PtyOwned` is routed `Write` (not counted) but the controller still decrements `writes_in_flight`: undercount, can read 0 early (`h:4107`, `pc:1432`). #2207's escape hold has it at the edge end (`view:15165`) | pre-existing; not widened by this design; separate issue |
| F12 | any pty write | Bytes already handed to the controller when the pty dies are lost (F2's mechanics), a typed key written a moment before the shell died included | same as a plain terminal's tty; Q5 |

| F13 | `input/enter_queue.rs:1403-1422`, `input.rs:17444-17451` | #1016's own path: `queue_enter_behind_deferred_enter` warns and returns when the `HandoffReport` gate is not open yet or the queue cannot be peeked, and its caller still answers "deferred": that Enter is consumed and lost | in scope: H5 replaces the function |

## 3. The design

### Rule

A key held by the hand-off leaves its gate in exactly one of three ways:

- **R, to the reader:** written to the pty in typed order; if the pty is gone or refuses the
  write, text in this pane's input;
- **W, with its line:** kept as input text after the line it follows, nothing written
  ("keep them together", 2026-11-09);
- **flushed by a signal** (path 5).

"Dropped because the state moved on" stops existing.

### Mechanism

One end point in the hand-off (#2207's `end_unowned_young_hold`) and one writer in the view
(the 2026-11-09 give-up writer). Round 1 widened #2207's `Typeahead` end to every hold; the
refute showed its view arm is unguarded (F10) and miscounts (F11), so it stays exactly as
#2207 shipped it, for the escape hold only.

`YoungHoldEnd` after the change: `FollowCommand`, `SubmitOnReport`, `Typeahead` (unchanged),
`ToReader` (new), `WithLine` (new), `FlushedBySignal` (was `Dropped`).

Hand-off (`h`):

- **H1** `end_unowned_young_hold` (`h:1671-1673`): the non-escape branch records `ToReader`.
  Covers 1a and 1b at all three `drop_stale_hold` callers.
- **H2** `on_command_written` (`h:3562-3564`): the non-escape branch records `WithLine`. The
  escape hold keeps `Typeahead` (#2207).
- **H3** `reset_on_exit`: the deferred gate's end is `WithLine` (`h:5323`); a `session_gone`
  flag is set and kept across the reset (as `escape_hold_id` is, `h:5330`).
- **H4** `give_up_report_deferral` / `give_up_deferred_gate`: `drop_young_hold()` ->
  `end_unowned_young_hold()` (path 4).
- **H5** The race is closed at its source. `defer_enter_for_typeahead_report`
  (`input.rs:17444-17451`) gets a second question beside #1016's: does a young Enter wait
  behind the request (`holds_young_enter_behind_request`, `h:2947`)? Both cases call ONE new
  function, `queue_enter_behind_handoff_gate(gate) -> bool`, which replaces
  `queue_enter_behind_deferred_enter` (F13). `gate` is `(PromptSeen, hold id)` or
  `(HandoffReport, deferred gate id)`, read from the hand-off. Its three outcomes:
  - **queue free:** under one `try_lock_ordered`, the hand-off is read again. Still live and
    the gate open: `EntryKind::Enter` is offered behind it. Still live and the gate not open
    yet (the view opens it on an event): the gate is opened here with the spec the view uses
    (`pq:1315`, `handoff_report_spec`; opening twice is harmless, `pq:1797`), then the Enter is
    offered. A young hold whose gate the report has just retagged (`pq:1604`) is found under
    `HandoffReport` by the same re-read. Returns `true`.
  - **queue busy:** `Deferred::Offer { Enter, Keyboard, stamp }` into the FIFO mailbox, the
    pattern of `enter_queue.rs:1386-1392`: applied in typed order when the queue is free,
    behind the gate if it is still open, as a live Enter if it is not. Returns `true`.
  - **the hand-off says the hold or deferral is over:** nothing is queued. Returns `false`.
  The caller answers "deferred" only on `true`. On `false` it decides again, once, from the
  hand-off as it now is (#1016's question, then `defer_enter`, then "not deferred": the editor
  handles the Enter as an ordinary Enter). An Enter is never reported consumed without being
  in the queue or the mailbox. `defer_enter` itself refuses while `young_hold_queued`.
  What happens to the queued Enter: when the report ends the hold with `SubmitOnReport`, the
  gate becomes the deferred Enter's `HandoffReport` gate in place and its first entry (the
  held Enter) leaves it (`pq:1597-1607`); the Enter queued here is then an entry behind a
  deferred Enter, the #1016 shape, and follows the command in typed order (`FollowCommand`).
  After H5, 1b, 2(a) and 4 need a queue / state disagreement; H1, H2 and H4 are their nets. A
  net delivers every key once, in gate order, but not in perfect typed order: `ToReader` in
  `HoldAfterEnter { released: true }` writes the older young line after the editor's command.
  That inversion is not a loss; H5 removes the only known way into that state, not the
  inversion in the net.
- **H6** `drop_young_hold` -> `flush_young_hold_for_signal` (callers `h:4171`, `5439`, `5502`);
  `drop_deferred_gate` -> `flush_deferred_gate_for_signal`. `FlushedBySignal` carries a token
  type whose constructor is private to those two functions; the view's one signal site
  (`pq:1476`) calls a hand-off method instead of naming the variant. The compiler, not a
  source scan, enforces "only a signal discards".

Not in the design: round 1's eager end for every hold. An ordinary unowned hold keeps its
lazy end (next key, a command, or the gate's cap, 10 s at most); the escape hold keeps its
edge end (`view:15165`). Nothing is lost by waiting, and nothing changes for #2207.

View:

- **V1** `ToReader` arm in `apply_young_hold_end`: the entries are taken under one queue lock
  and, in the same view turn, split at the first editor key that has no byte form (an arrow,
  a selection, Tab). Nothing is offered back to the queue.
  - The part before the split goes to `write_given_up_entries_to_pty` (`giveup:412`): every
    entry kind converted (`giveup:163-240`), written through
    `write_user_bytes_to_pty_after_paste_stage` (the stages every typed key takes: guarded,
    counted by the hand-off). A chunk is either written or, if refused, becomes input text
    with everything after it; never both (`giveup:441-470`).
  - The split key and everything after it, whatever its origin, go to
    `keep_given_up_keys_with_line` (`giveup:254`), an existing function with exactly this
    job: byte-form keys become input text in typed order, a no-byte key acts on the editor in
    its place, an Enter is a line break under the hand-back guard and submits nothing.
  Why text and not live keys: re-offered, the tail would pass the routing table again and
  could become an editor submit, a new young hold, or a `D8` / `D10` drop (F9), and a key
  arriving in between could pass it. As text in one turn it has one destination and nothing
  can get in front.
  With the pty gone there is no split: the whole segment is input text through the same
  writer.
- **V1b** `WithLine` arm: a `PromptSeen` gate's entries go to `keep_given_up_keys_with_line`
  (`giveup:254`): the held Enter and the keys behind it become lines of input text, the
  hand-back guard is armed, nothing reaches the pty; the stashed report then lands in front of
  them (the line's head), as after a report-patience give-up. A `HandoffReport` gate's entries
  go to `give_up_deferred_enter_keys` (`view:15600`).
- **V2** `write_given_up_chunks_to_pty` (`giveup:441`) reads `self.pty_gone ||
  handoff.session_gone()` and sets `self.pty_gone` when it first sees the session gone. Every
  give-up caller is covered, including a cap or a drain (`view:15166`) that runs after the
  model exited and before the view handled the exit event. The `Typeahead` arm (`pq:1648`)
  makes the same check and hands the chunks to that writer when the pty is gone (Q1).
- **V3** Exit arm (`view:19238`): after `pty_gone` is set and before the editor is disabled
  (`view:19303`), flush the queue's mailbox and drain the ends. If the queue is still busy the
  end is applied from the mailbox later; the text insert must therefore not depend on the
  editor's interaction state. A test pins it; if a disabled editor refuses `system_insert`, the
  exit arm disables the editor only after the mailbox has been flushed.
- **V4** `pq:1640` arm renamed with the variant; one log hop with counts only.

### State transitions touched

| from | event | today | after |
|---|---|---|---|
| any, young hold queued | pty exits | `Dropped`, applied late | `ToReader` -> input text, at exit |
| any, Enter deferred | pty exits | `Dropped`, or kept by the leak net | `WithLine` -> keys with the line in the input |
| `AwaitingReport`, young hold queued | an Enter reaches the editor | `defer_enter`, hold stranded | the Enter queues behind the hold's gate |
| `EditorOwned` / `HoldAfterEnter`, young hold queued | next key, cap | `Dropped` | `ToReader` -> pty |
| not `PtyOwned`, young hold queued | user command written | `Dropped` | `WithLine` -> input text |
| `HoldAfterEnter`, young hold queued | report / gate give-up | `Dropped` | `ToReader` -> pty |
| escape hold | every end | `Typeahead` | unchanged, except pty gone -> input text (Q1) |
| any | `^C` `^Z` `^\`, cancel of a deferred Enter | `Dropped` | `FlushedBySignal`, same behaviour |

### Typed order across the pty / editor boundary

- Entries leave the gate under one queue lock, in entry order (`pq:1622`); a key about to queue
  behind a stale hold ends it first (`pq:1497`).
- A busy queue defers the whole end into the FIFO mailbox (`pq:1590-1595`).
- Editor-typed text, Enter and Backspace in the segment are written as their bytes, in place
  (the rule of every give-up since 2026-11-09, `giveup:193-203`); from the first no-byte key
  on, the rest of the segment becomes input text in the same turn (V1), so it can neither
  run nor pass what was written.
- `WithLine` writes nothing, so no line can run ahead of the one before it.
- Pty gone: one insert of the concatenated text.

### Conflicts with the owner rules

- **#2207.** "Written in typed order when the session ends": at a pty exit nothing can read
  (F2). Q1.
- **"Keep them together" vs "write to the pty".** Path 2 follows "keep them together" (W),
  because writing after the command is the #2182 reorder and runs a later line without the
  one before it. Q2 asks the owner to confirm.
- **Bootstrap give-up (#1577 A7).** No conflict: the coordinator confirmed the code is current
  (#1996: the give-up writes to the pty, `view:26721-26781`, and hands back only when the
  shell is gone, `view:26783-26809`). The design touches neither and agrees with both halves.
- **No conflict found with:** ssh connect typeahead, managed ssh (#1641), no drain on exit
  255, #851, event-driven hand-overs (no timer added or changed).

### Owner questions

- **Q1** The #2207 escape hold when the pane's shell dies: show its Enter and keys as text in
  the input, like every other hold? Today they are written to a dead pty and vanish.
  Recommended: yes. Two #2207 view tests (`input_test.rs:21747`, `:21843`) model "ssh is
  gone" with `reset_on_exit`; they would switch to the path the app takes
  (`input_test.rs:21845-21850`) and keep their assertion.
- **Q2** A command started by a click while a typed Enter is still waiting: keep that Enter
  and what was typed after it as text in the input, unrun, instead of running them after the
  clicked command? Recommended: yes (it is the default in this design).
- **Q3** Stop, a dismissed warpify block and rewind send `^C` and so void typed-ahead keys
  although the user typed no `^C`. Keep? Recommended: keep; a real `^C` reaches the tty.
- **Q4** Keys typed in the input box while a hold is open: when the hold is written to the
  pty, plain typing (text, Enter, Backspace) goes with it. From the first arrow or Tab on,
  everything typed after it, in the box or on the terminal, is shown as text in the input
  box, unrun, with the "check the line" guard. Recommended: yes; nothing is lost, nothing
  runs out of order, and the mixed case is rare.
- **Q5** A key written a moment before the shell dies is lost in the dying tty, as in any
  terminal. Should the app keep the last write until the pty loop confirms it, to show it as
  input text instead? Recommended: no; it adds bookkeeping to every write for a case no
  terminal handles.
- **Q6** Pty-gone text is visible. If something was reading a password when the shell died,
  discard those keys instead of showing them? Recommended: yes, with a counted log line; it
  would be the only new discard.

## 4. Test plan

### Unit, `typeahead_handoff_tests.rs`: the 13 `Dropped` assertions

| line | test | change |
|---|---|---|
| 3711, 3779, 3845, 3911, 7080 | signal tests | variant rename only |
| 7982 | `a_young_enter_and_the_keys_behind_it_survive_an_uncounted_command_in_order` | rename only (asserts absence) |
| 8374 | `other_young_holds_are_still_dropped_when_their_session_goes` (`^C`, escape hold) | rename; its `if let Some` (`:8370`) becomes `.expect`, it passes vacuously today |
| 3816, 8365 | stale state | **`ToReader`** (1b) |
| 3828, 8355 | pty exit | **`ToReader`**, plus `session_gone()` (1a); `:8348` test renamed, it asserts what #2209 removes |
| 3890 | `a_command_written_in_a_state_that_cannot_own_the_hold_drops_it` | **`WithLine`**, renamed (2) |
| 3921 | deferred gate, pty exit | **`WithLine`** (3a) |

Six assertions in four tests change, each on a path #2209 names. None is loosened. These
pin labels only; the behaviour is pinned by the view tests.

New unit tests:

- `no_event_but_a_signal_voids_held_keys`: every hold fixture x every public non-signal event.
- `an_enter_that_reaches_the_editor_behind_a_young_hold_is_not_deferred` (H5).
- `a_young_hold_stranded_behind_a_deferred_enter_is_released_by_the_report_and_by_the_give_up`
  (H4 net, state forced).
- `a_young_hold_whose_pty_died_goes_to_the_reader_and_the_session_is_marked_gone`.
- `a_command_written_behind_a_request_keeps_the_held_enter_with_its_line` (H2).

New view tests (`input_test.rs`):

- `test_young_hold_keys_become_input_text_when_the_shell_exits`: exact text, no pty write.
- `test_hold_end_drained_after_the_model_exited_and_before_the_exit_event_writes_nothing`
  (V2: `session_gone` set, `pty_gone` not yet).
- `test_keys_behind_a_deferred_enter_stay_with_its_line_when_the_shell_exits`, with a bare
  deferred Enter as a second arm.
- `test_exit_with_a_busy_queue_still_puts_the_keys_in_the_input` (V3).
- `test_the_60s_leak_net_after_an_exit_end_delivers_nothing_twice`.
- `test_other_gates_at_exit_keep_their_keys`: a prompt hold, an ssh connect hold and a paste
  gate at exit end as input text at their own give-ups, none written.
- `test_an_editor_enter_behind_a_young_hold_runs_in_typed_order` (H5, end to end).
- `test_a_stale_young_hold_reaches_the_pty_once_in_typed_order`, asserting
  `writes_in_flight` before and after in each state.
- `test_an_arrow_in_the_segment_splits_the_hold`: keys before it written once; the arrow
  applied; every key after it (editor text, a terminal-typed key, an Enter, `^U`, an arrow
  sequence typed on the terminal) is input text in typed order, nothing submitted, nothing
  written, no new hold, queue idle.
- `test_an_enter_behind_a_young_hold_is_never_consumed_unqueued`: the three outcomes of
  `queue_enter_behind_handoff_gate` (free, busy, hold over), and the gate-not-open-yet arm
  for both gate kinds (F13).
- `test_an_enter_queued_behind_a_young_hold_follows_the_command_after_the_report`
  (`SubmitOnReport` retag, the #1016 shape).
- `test_a_clicked_command_keeps_the_line_head_report` (2b: the stashed report still arrives
  and lands in front of the kept Enter and keys).
- `test_a_stale_hold_under_agent_control_becomes_input_text` and the hidden-input-box arm.
- `test_a_clicked_command_behind_a_request_keeps_the_enter_and_keys_as_input_text` (2).
- `test_ctrl_c_after_an_undrained_release_writes_the_keys_then_the_interrupt`.
- `test_ctrl_c_still_voids_a_young_hold_and_a_deferred_gate`.
- The two #2207 tests: setup per Q1.

### Real app (appci / lab); names are the case functions

Existing, must stay green on every platform they run on: `c_giveup_keys_reach_shell`
(`cases_shells.py:196`), `c_ctrl_c_no_esc_i_local` (`cases_shells.py:101`), lab
`c_ctrl_c_no_esc_i` (`cases_lab.py:1448`), lab `c_lab_drop` (`cases_lab.py:1406`), lab
`c_password_prompt` (`cases_lab.py:1750`), `c_ssh_password` (`cases_caseremote.py:1545`),
`c_ssh_fail` (`cases.py:3790`), `c_connect_typeahead` (`cases_lab_winport.py:84`); Windows
`c_gitbash_typeahead_reps` (`cases_win_handover.py:143`), `c_gitbash_precmd_secret` (`:84`),
`c_ps_fast_command_young_enter` (`cases_win_input.py:171`),
`c_ssh_gitbash_typeahead_managed` (`:401`), `c_ssh_gitbash_failing_not_local_managed`
(`:336`), `c_ssh_nested_typeahead_wrapper` (`:92`); latency `c_agent_latency_key`,
`c_agent_latency_enter` (`cases_agent_latency.py:297,489`), `c_latency_echo_ab`
(`cases_keyframe_ab.py:171`) against their bands.

New, in `cases.PROVING`, agent and terminal mode:

- `exit_keeps_typed_keys` (Linux bash / zsh / fish, macOS zsh / bash, Windows Git Bash and
  PowerShell): keys at 10-25 ms/key while a hold is live, then the shell is killed. Oracle:
  the input holds exactly the typed text; the log has `give_up_keys_to_pty ... pty_gone=1`
  with `to_editor` equal to the bytes typed. Keeping a hold live until the kill needs the
  shell stopped (SIGSTOP on POSIX, a suspend on Windows): the harness side is NOT designed.
- `exit_keeps_keys_behind_deferred_enter`: the same with an Enter deferred behind its report.
- lab `lab_exit_keeps_typed_keys` on the Linux, macOS and Windows lab ssh servers.
- `editor_enter_behind_young_enter_in_order`: fast `cmd`, Enter, `cmd2`, Enter with focus in
  the input box, on every shell; both commands run once, in order.
- Run-wide oracle: flush hops never exceed the signals the harness sent.

`docs/testing.md` is updated in the same merge.

## 5. Risks

| risk | why | caught by |
|---|---|---|
| Latency | No per-key work on the pty path. H5 adds one hand-off read to an editor Enter | the latency cases against their bands |
| #2182 reorder class | Nothing is written behind a command any more (W); `ToReader` writes go through the normal key stages | the H2 tests, `c_gitbash_typeahead_reps` |
| H5 changes a hot path | Every editor Enter passes `defer_enter_for_typeahead_report`; the #1016 function is replaced, so #1016's own cases must stay green. The mailbox arm delivers a live Enter if the gate closed meanwhile | `test_an_editor_enter_behind_a_young_hold_runs_in_typed_order`, `editor_enter_behind_young_enter_in_order` |
| Counting | The claim that the key stages count and retire a write consistently in every state is unverified (F11 shows the direct path does not) | the `writes_in_flight` assertions; first check of phase 2 |
| ssh escape `~.` | The escape hold's ends are untouched except at pty exit | the #2207 unit tests unchanged, `c_lab_drop` |
| Secrets | `ToReader` writes to the pty, never the editor; only pty-gone text is visible (Q6) | `c_password_prompt`, `c_gitbash_precmd_secret`, a view test once Q6 is decided |
| The tail as text | `text_for_editor` was not read: what it does with control bytes other than Backspace (`^U`, an arrow sequence typed on the terminal) is open; they must not vanish | the split test pins them |
| Text into a disabled editor | No `can_edit` guard was found on the path read (`input.rs:15230` -> `insert_internal` -> `editor/view/mod.rs:5860` -> the model's `insert`), but the editor's inner `edit` was not read; V3 keeps the drain before the disable | `test_exit_with_a_busy_queue_still_puts_the_keys_in_the_input` |
| A leading line break | The held Enter is the first entry; as input text it is a line break | the exit tests pin the exact text |
| Double delivery | The leak net and the exit end can both reach a deferred gate | `test_the_60s_leak_net_after_an_exit_end_delivers_nothing_twice` |

## 6. Not determined

- What `text_for_editor` does with control bytes other than Backspace.
- Whether the stashed report of case 2(b) survives: by FIFO the shell answers the request
  before it reads the command's kill prefix, and `clear_typeahead_for_new_command` runs before
  the report arrives, but `reconcile_requests_at_own_preexec` (`h:3816-3822`) was not read.
  A phase-2 test (above).
- Whether the key stages count a write consistently outside `PtyOwned`.
- Whether the editor's inner `edit` refuses a programmatic insert when disabled.
- Whether path 2 has a non-key trigger besides a click.
- View tests that assert the old drop by its effect: only a box run will show them.
- The harness hook for `exit_keeps_typed_keys`, above all on Windows.

## 7. Refute of `ad21f0890`: resolution per finding

| # | resolution |
|---|---|
| 1 | FIXED: V2, the writer itself reads the hand-off's `session_gone`; view test added |
| 2 | FIXED: F2 states the write is lost with the evidence; F12 states the residual; Q5 |
| 3 | FIXED: path 4 is reachable (section 1 preamble); H5 closes the race, H4 is the net; tests added |
| 4 | FIXED: path 1b and 2 rewritten around the race; `input.rs:1116` claim removed |
| 5 | FIXED: path 2 defaults to `WithLine`; `Typeahead` behind a command stays escape-only |
| 6 | FIXED: cut at the first no-byte editor key (V1); the per-hold flag is not needed, the new variants carry the meaning; test added |
| 7 | FIXED: the new ends use the guarded writer; the direct emit is listed as F10, unchanged and not widened; agent-control and hidden-input tests added |
| 8 | FIXED: not widened (F11); the new path's counting is a phase-2 first check with before/after assertions |
| 9 | FIXED: V3 and its test |
| 10 | FIXED: test added; V2 makes those gates' own give-ups pty-gone aware |
| 11 | FIXED: F8 and F9 traced, verdicts given |
| 12 | FIXED: path 5 and Q3 widened; the one caller of `h:5439` is `view:16369` |
| 13 | FIXED: private constructor (H6); vacuous test tightened; the five tests added |
| 14 | FIXED: (a) Q4, (b) eager end removed from the design, (c) Q2 with the default; old Q3 dropped |

## 8. Re-check of `49bc0045a`: resolution

| # | resolution |
|---|---|
| N1 | FIXED: H5 respecified. One function for both gate kinds, three stated outcomes; the caller answers "deferred" only when the Enter is in the queue or the mailbox, otherwise decides again once; the `SubmitOnReport` retag and the #1016 shape are stated. The same hole in today's #1016 path is F13 and goes with it |
| N2 | FIXED: no re-offer. The tail has one explicit destination, input text, through `keep_given_up_keys_with_line`, in the same turn as the take; Q4 updated to say so |
| N3 | Confirmed in V1 (written or text, never both) |
| N4 | Stated in H5: an inversion in a net, not a loss; H5 removes the way in, not the inversion |
| finding 5 residual | Not settled by reading; listed in section 6 with a phase-2 test |
| finding 9 | No guard found on the path read; inner `edit` unread; V3 ordering and its test stay |

## 9. Owner answers (2026-11-14, "Go") and what was built

Answers to the questions of section 3:

| question | answer | where it is in the code |
|---|---|---|
| Q1 escape hold at a pty exit -> input text | yes | `view/paste_queue.rs`, the `Typeahead` arm asks `pane_pty_gone()` |
| Q2 a clicked command keeps the held Enter and keys as input text | yes | `h` `on_command_written` -> `WithLine`; `view/give_up_to_pty.rs` `keep_unowned_hold_with_line` |
| Q3 Stop / dismissed block / rewind still flush | keep | unchanged |
| Q4 plain typing goes with the hold; from the first arrow or Tab on, the input box | yes | `release_unowned_hold_to_reader`, `replay_hold_tail_in_editor` |
| Q5 keep the last write until the pty loop confirms it | no | not built |
| Q6 discard keys held when the pty died reading a secret | yes, counted log line; the only new discard | `terminal_model.rs` `exit`, `h` `session_gone_reading_secret`, `give_up_to_pty.rs` `discard_*_of_dead_secret_reader` |
| Q7 (added by the owner) KEEP EVERY KEY in the split tail | arrows, function keys, Esc, `^C`, Up / Down are replayed in order, never dropped; no leading line break, no cursor jump | below |

**Q7 changes V1.** Round 3 sent the tail to `keep_given_up_keys_with_line`, which moves the
cursor to the end of the buffer, puts a line break in front, and builds its text with
`text_for_editor`, which drops Up / Down, a lone ESC, `^C` and every sequence it does not know.
The tail now goes to `replay_hold_tail_in_editor`: every key is applied in the input editor, in
typed order, at the editor's cursor. Bytes typed on the terminal are named key by key by the new
pure function `editor_keys_of_bytes` (`h`), which names where `line_for_editor` applies and
drops. An Enter is still a line break under the hand-back guard (Q4). What cannot be replayed:
bytes of a key the input editor has no action for (a function key or a modified cursor key TYPED
ON THE TERMINAL after an arrow typed in the input box, inside one hold). They are counted in a
warning and in the `hold_tail_replayed_in_editor` hop, not dropped silently; the same key typed
in the input box is an editor key and is replayed as itself. `text_for_editor` itself is not
changed: its other callers hand text back as text.

**Design items as built.** H1-H4 and H6 as designed. H5 as designed (`Input::
queue_enter_behind_handoff_gate`, `TypeaheadHandoff::enter_queues_behind`, `defer_enter` refuses
with a young hold queued); one detail differs: a gate the view has not opened yet is opened
through the queue's mailbox (the open, then the Enter), not by the input directly, so the
view's host arms the gate's net and its give-up stays the hand-off's own. V1 with the Q7 change. V1b, V2, V3, V4 as designed; V2's
`pane_pty_gone()` is also asked by the `Typeahead` arm. Not built: nothing of the design was
left out; of the TEST plan, see `docs/testing.md` "Round 31" for what was not written.

### Two additions to the change set (same class: no typed key lost across the hand-off)

**#2115: cursor keys before the report are applied to the joined line.** Between the request
for the shell's line and its report the editor holds only what was typed after the request. A
cursor or deletion key applied then acts on that partial buffer and is clamped at its start
(r30d: `echo fz1wjn` in the shell, `t` in the editor, four Lefts, `y`: `fz1wyjnt`). Built like
#2107's history-recall gate: a new gate kind `GateKind::ReportEdit`. The first cursor or
deletion key typed in the editor while `report_pending_before_editor_text()` holds asks for the
gate (through the queue's mailbox, so the view's host arms the gate's net) and waits behind it;
from then on the queue holds every key of the pane behind it, in order. The report's insertion
closes it (`TerminalView::release_report_edit_gate_if_settled` -> `Input::
release_report_edit_gate`), as does the hand-off leaving the wait without a report, and the keys
are applied to the joined line. Typed text before any such key is not held, so plain typing is
not delayed. Net: the gate's own cap (3 s), which replays the keys in order on the buffer as it
is and opens no second gate for that report. `^C` discards the edits with the line, `^D` waits
in order (the history-recall rules). Kill switch `PHOSPHOR_HOLD_EDIT_KEYS_BEHIND_REPORT=0`.
Enter and Tab keep their own waits for the report (#1016, #1659); Up and Down are not held (they
recall history, they do not act on the line).

**#2223: an Enter declined because a command runs is not dropped.** After a Ctrl-C hand-off
that gave up with the program still running, the editor had the keys. Each Enter was refused
(`try_execute_done executed=false`, `enter_declined ... held_as_pending=false`) and dropped;
the text stayed and the next line was appended to it. The "seed" is not a restore: the editor
keeps a submitted command's text until its block completes (`Input::editor_submission`), so the
text of the command that was RUNNING was still in the buffer and every typed line landed behind
it. Built: `Input::send_declined_enter_line_to_pty`. For the refusal "a command is running" the
text typed since the submission and the Enter are handed to the view
(`Event::DeclinedEnterLineToPty`), which writes them to the pty in typed order through the
guarded give-up writer, as a plain terminal sends what is typed into a running program; the
editor goes back to exactly the running command's text, which is never written again. A bare
Enter is written too. Not changed: WHY the editor had the keys while the program ran (the
hand-off's state after `ctrl_c_handoff_give_up`); the r30d app log was not available on this
machine, and that routing is not touched. With it unfixed the keys still take the editor's
route, but no Enter and no line is lost and no lines merge.

### Review of the built change set (follow-up commit)

- With the pty gone the whole held segment is replayed in the input editor key by key (no
  split, nothing through `text_for_editor`), so an arrow acts in place and Up / Down / Esc
  typed on the terminal are kept. The exit arm keeps the editor editable until those keys have
  landed, for a bounded number of event-loop turns.
- Keys that still cannot be replayed (counted and warned, never silent): a `^C` (a replayed key
  must not become a new interrupt); every escape sequence other than the unmodified arrows,
  Home, End and Delete (function keys, Page Up / Page Down, Insert, Shift-Tab's `ESC [ Z`,
  cursor keys with a modifier, OSC and other reports); and the control characters NUL, `^G`,
  `^L`, `^O`, `^Q`, `^R`, `^S`, `^T`, `^V`, `^X`, `^Y`, `^Z`, `^\`, `^]`, `^^`, `^_` and the C1
  controls. Only for bytes typed ON THE TERMINAL inside a replayed tail; the same key typed in
  the input box is an editor key and is replayed as itself (except `^C`).
- Q6 reads only the secret signals: the shell's `TtyRead` marker with its secret flag and the
  last sample of the tty taken while the pty was alive. A reader that only asks a question is
  not a secret.
- #2223 finds the typed-since text by position (`SubmittedPrefix`), not by comparing texts:
  the running command's text edited in place writes nothing (the Enter is kept as a line
  break), an emptied buffer makes everything typed since, and a submission of another block is
  ignored. An Enter the hand-off deferred is left to the hand-off.
- Open: a key typed on the terminal (raw to the pty) between the characters of a line typed in
  the input box and that line's Enter still reaches the pty before the line (#2223, typed
  order across the two routes while a command runs).
