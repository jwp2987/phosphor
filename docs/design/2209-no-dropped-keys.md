# #2209: no typed key is dropped when a hold ends (design, phase 1, UNREVIEWED)

Status: design only. No code changed. Written by reading the code at `36efe34b5`
(`integ/r15`); nothing here was compiled or run. Paths are relative to the repo root;
`handoff` = `app/src/terminal/model/typeahead_handoff.rs`, `pq` =
`app/src/terminal/view/paste_queue.rs`, `view` = `app/src/terminal/view.rs`,
`giveup` = `app/src/terminal/view/give_up_to_pty.rs`.

## 0. Three facts that change how the issue reads

1. **"The session ends" is the pane's pty dying, not ssh ending.** `reset_on_exit`
   (`handoff:5316`) has one caller, `TerminalModel::exit` (`terminal_model.rs:1677`), which is
   called only for `ShellProcessExited`, `PtyDisconnected`, `ShellNotFound`, `PtySpawnFailed`
   (`local_tty/event_loop.rs:1629,1761,1819,1838`, `local_tty/terminal_manager.rs:890,989,1017`,
   `writeable_pty/terminal_manager_util.rs:223`). When an ssh inside the pane ends, the local
   shell prompts and the hold ends by its normal events. So on this path there is **no next
   reader**: a plain terminal closes its window and the keys are gone with it.
2. **A held key has exactly one way to be discarded:** `YoungHoldEnd::Dropped`, applied in
   one arm (`pq:1640-1642`). Six hand-off sites record it (below); two are signals.
3. **The pty-gone rule already exists** (owner decision 2026-11-09, `giveup:53-54`,
   `giveup:445-483`): "the pty is gone: the keys become text in this pane's input, never
   dropped silently and never written anywhere else". #2209 is the last family that does not
   follow it.

## 1. The drop paths

"Reachable" is by reading; none was reproduced.

### Path 1a: young hold, pty exits

- Trigger: `reset_on_exit` -> `end_unowned_young_hold` (`handoff:5321`, `handoff:1662-1677`)
  records `Dropped` for every hold but the #2207 escape hold.
- Scenario: `make` finishes; the user types `ls`, Enter, `pwd`, Enter at once (young Enter held
  under a `PromptSeen` gate, `handoff:4214-4233`); the shell is killed or the pty disconnects
  before the report.
- Now: `Dropped` is recorded, but **nothing applies it at exit**: the view's `ModelEvent::Exit`
  arm (`view:19238-19319`) never calls `drain_young_hold_ends`. The keys sit under the gate
  until an unrelated hand-off event or the gate's 10 s cap (`pq:2047-2049`) drains the end, then
  `pq:1640` discards them silently (a `trace::release(.., "dropped")`, no log line, no toast).
- Plain terminal: the window closes; the keys are lost.
- Next reader: **nothing.** Phosphor keeps the pane (banner, and on Windows "Restart shell",
  which carries the input text to the new pane, `view:18213-18217`), so rule 0.3 applies: text
  in the input.

### Path 1b: young hold, state left `PtyOwned` / `AwaitingReport`

- Trigger: `drop_stale_hold` (`handoff:4472-4480`), from `check_queued_young_hold`
  (`handoff:1478`), `route_user_bytes` (`handoff:4175`) and `expire_queued_young_hold`
  (`handoff:4884`; the issue lists two callers, there are three).
- Reachability: **not reachable in normal operation, by reading.** A hold starts only in
  `PtyOwned { wrote_user_bytes: true }` (`handoff:4508-4515`). Every exit from there to
  `EditorOwned` requires `wrote_user_bytes: false` (`handoff:3919-3933`, `4024-4046`) or checks
  the hold first (`handoff:4052-4064`); the report takes the hold (`handoff:5256-5270`);
  `defer_enter` cannot be reached because every key, an editor Enter included, queues behind
  the open gate (`pq:732-733`). It is reached when the queue and the state disagree (the code's
  own words, `handoff:4177-4183`) and by the #2207 escape hold, which is the reason
  `end_escape_hold_if_unowned` exists (`handoff:1642-1653`).
- Now: `Dropped`, applied lazily at the next key.
- Plain terminal / next reader: the keys are in the tty; the shell's line editor (the local
  shell) reads them in typed order.

### Path 2: young hold, a command is written from a state other than `PtyOwned`

- Trigger: `on_command_written` (`handoff:3553-3565`): the hold follows the command only when
  the state was `PtyOwned`; else `Dropped` (escape hold: `Typeahead`).
- Reachability: **narrow.** `on_command_written` runs only for a `CommandExecutionSource::User`
  command on a reporting shell (`pty_controller.rs:2661-2669`). With a hold queued behind an
  outstanding request (`AwaitingReport`) the user's own Enter is in the queue, so the command
  must come from a submit that is not a queued key: a click (re-run, a workflow, an accepted
  suggestion), in the window between the request and its report (a round trip: milliseconds
  locally, hundreds over ssh/VPN). #2198 closed the same window for app commands
  (`pty_controller.rs:2814-2840`) but exempts this one (`:2827`).
- Now: Enter and the keys behind it are discarded.
- Plain terminal: no click-to-run exists; typed keys run in typed order.
- Next reader: the shell, after the command.

### Path 3: keys behind a deferred Enter

- 3a, pty exits: `reset_on_exit` (`handoff:5322-5324`) records `Dropped` for the
  `HandoffReport` gate. Scenario: `ls` typed into a line whose report is outstanding, Enter
  (deferred, `handoff:5183-5206`), `pwd`, Enter, then the shell dies. Now: as 1a (applied
  late, then discarded; the gate's 60 s leak net can get there first and then keeps them,
  `view:15670-15697`, so the outcome depends on which timer wins). Next reader: nothing; the
  Enter's line is in the editor, unrun.
- 3b, "the deferred Enter is cancelled": `drop_deferred_gate` (`handoff:5509-5513`) has ONE
  caller, `cancel_deferred_enter` (`handoff:5483-5505`), and that has two: Ctrl-C
  (`view:10708`) and the app's own interrupts (`view:10604`: Stop, a dismissed warpify /
  tmux-install block, rewind, #1774). **This is path 5, not a separate drop.**

### Path 4: young hold at a report-deferral give-up

- Trigger: `give_up_report_deferral` (`handoff:3343`), `give_up_deferred_gate`
  (`handoff:3364`) call `drop_young_hold`.
- Reachability: **unreachable, by reading.** Both require `HoldAfterEnter`; a young hold cannot
  be queued there (same argument as 1b: the report takes it, `defer_enter` cannot be reached
  behind its gate, and no hold starts outside `PtyOwned`). The calls are defensive.
- The keys behind the deferred Enter on these two paths are NOT dropped today: they stay with
  the unrun line, or follow a bare Enter to the pty (`view:15600-15661`).

### Path 5: signals (kept)

- `^C`, `^Z`, `^\`: a POSIX tty with `ISIG` and without `NOFLSH` discards its input queue on
  INTR / QUIT / SUSP. Real flush. Sites: `handoff:4171`, `handoff:5439`, `handoff:5502`,
  `pq:1454-1491`, `pq:1385-1440` (prompt hold), `dest_table.rs:1188` (ssh hold). Confirmed.
- `^D` is NOT a tty flush (EOF is not a signal), and the code knows: with the pty owning input
  the held keys go out in front of a lone EOF (`handoff:4872-4874`, `pq:1484-1490`). One
  sub-case still voids them: a young hold behind an outstanding request (`AwaitingReport`),
  where `before_eof` is false. Listed in section 2, not changed here.
- Under ConPTY the tty does not flush bytes already written (`handoff:1370-1377`); voiding
  app-held keys on `^C` there is the owner's rule, not a tty fact.

## 2. Other places that can discard typed bytes

Found 9, filed 0 (none is filed by this document; the coordinator files).

| # | where | what | verdict |
|---|---|---|---|
| F1 | `view:19238-19319` | The exit arm applies no recorded hold end; ends wait for an unrelated event or a cap | **in scope**, fixed by the design (V3) |
| F2 | `handoff:1665-1670` + `pq:1648-1664` | #2207's escape hold at pty exit is "written" with `WriteHandoffUserBytesToPty` to a pty that no longer exists: a silent drop in the real app. The unit tests model "ssh is gone" with `reset_on_exit` and a mock writer, so they pass | **in scope**, owner question Q1 |
| F3 | `handoff:4872-4874`, `pq:1484-1486` | `^D` voids a young hold that waits behind an outstanding request | not a tty flush; separate issue, owner rule says signals stay |
| F4 | `pq:2288-2291` | `take_entry_bytes` warns and drops an entry kind it does not know (`Raw`, a `Held` paste) when it sits in a hand-off gate. `giveup:163-240` handles every kind | separate issue (#1922 was one instance) |
| F5 | `input_queue/mod.rs:1060-1066`, `pq:685-697` | Queue full (64 x 1024 entries): the newest key is refused, with a toast | deliberate (#1442), bounded memory |
| F6 | `view/ssh_queue.rs:471-478`, `view:11784` | ssh connect hold overflow discards the whole run, with a toast | deliberate (#1292) |
| F7 | `view:12476-12500` | ssh reads a secret: held connect typeahead discarded like `TCSAFLUSH`, with a toast | deliberate (#1611, `DECLINED.md:2867+`) |
| F8 | `view:12551-12553`, callers `view:13056`, `view:21387` | `reset_ssh_typeahead` on a new command and at the helper question's interrupt | `21387` is an interrupt; `13056` NOT traced (can it run with keys held?) |
| F9 | `input_queue/table.rs:683-1052`, `destination.rs:74-91` | Table rows that drop (`AgentControl`, `RefusedHiddenInput`, `NotLongRunning`, `AgentRefusedSetup`) | NOT traced: which carry typed keys rather than mouse / wheel bytes |

## 3. The design

### Rule

A key held by the hand-off leaves its gate in exactly one of two ways:

- **written out, in typed order**, to whatever reads the pty next; when nothing can read it
  (the pty is gone) it becomes text in this pane's input (rule 0.3);
- **flushed by a signal** (path 5).

There is no third way. "Dropped because the state moved on" stops existing.

### Mechanism: #2207's, with its special case removed

#2207 added `end_unowned_young_hold` (`handoff:1662`) with one branch: the escape hold ends
as `YoungHoldEnd::Typeahead`, every other hold as `Dropped`. The design deletes the branch.
No new state, gate kind, timer or end.

Hand-off (`handoff`):

- **H1** `end_unowned_young_hold` always records `Typeahead` (delete `handoff:1671-1673`).
  Covers 1a and 1b at all three `drop_stale_hold` callers.
- **H2** `on_command_written`: `handoff:3558-3564` becomes one `else { Typeahead }`. Path 2:
  the Enter and keys are written behind the command, as #2207 already does for its hold.
- **H3** `reset_on_exit`: the deferred gate's end is `Typeahead` (`handoff:5323`), and a
  `session_gone` flag is set and kept across the reset (as `escape_hold_id` is, `handoff:5330`).
- **H4** `give_up_report_deferral` / `give_up_deferred_gate`: `drop_young_hold()` ->
  `end_unowned_young_hold()` (path 4; unreachable, made structural).
- **H5** `end_escape_hold_if_unowned` loses its escape-only condition
  (`handoff:1644`): any hold the state no longer owns ends at the edge (the view already calls
  it on every hand-off change, `view:15165`), not lazily at the next key. Same reason as #2207
  review F4: before a later command can get in front.
- **H6** Rename, no behaviour change: `drop_young_hold` -> `flush_young_hold_for_signal`
  (callers left: `handoff:4171`, `5439`, `5502`), `drop_deferred_gate` ->
  `flush_deferred_gate_for_signal` (caller: `cancel_deferred_enter`), `YoungHoldEnd::Dropped`
  -> `YoungHoldEnd::FlushedBySignal`. After this the discard variant is constructed in two
  functions and one view site (`pq:1476`, a signal), which a test pins (section 4).

View:

- **V1** `pq:1648` (the `Typeahead` arm), first thing: if `self.pty_gone ||
  handoff.session_gone()`, the chunks go to `write_given_up_chunks_to_pty(.., "session_gone")`
  (`giveup:429`), whose pty-gone branch inserts them as input text in typed order and arms the
  hand-back Enter guard. For a `HandoffReport` gate (only H3 records `Typeahead` for one) the
  taken entries go to `give_up_deferred_enter_keys(.., "session_gone")` (`view:15600`): the
  keys stay with the unrun line, "keep them together".
- **V2** #2207 review F2's rule (`pq:1628-1637`: every key of the hold, editor-typed ones too,
  is the pty's, in typed order) applies to every hold that ended unowned, not only the
  escape's: `is_escape_hold(id)` becomes "escape hold, or ended by H1/H2".
- **V3** `view:19238` exit arm: after `pty_gone` is set (`view:18227`) and before the editor is
  disabled (`view:19303`), call `drain_young_hold_ends`. Ends are applied at exit, not at a cap.
- **V4** `pq:1640` arm is renamed with the variant; it gains one log hop with counts only.

### State transitions touched

| from | event | today | after |
|---|---|---|---|
| any, young hold queued | pty exits | `Dropped` (late) | `Typeahead` -> input text, at exit |
| any, Enter deferred | pty exits | `Dropped` (late, or kept by the leak net) | `Typeahead` -> keys with the line in the input |
| `EditorOwned` / `HoldAfterEnter`, young hold queued | next key, cap, any hand-off change | `Dropped` at the next key | `Typeahead` at the edge -> pty |
| `AwaitingReport` / `EditorOwned`, young hold queued | user command written | `Dropped` | `Typeahead` -> pty, behind the command |
| `HoldAfterEnter`, young hold queued | report / gate give-up | `Dropped` | `Typeahead` -> pty |
| any | `^C` `^Z` `^\`, cancel of a deferred Enter | `Dropped` | `FlushedBySignal` (same behaviour) |

### Typed order across the pty / editor boundary

- The entries leave the gate under one queue lock, in entry order (`pq:1622`).
- A busy queue defers the whole end into the FIFO mailbox (`pq:1590-1595`), so a later key
  cannot pass it.
- V2 turns editor-typed keys in the segment into pty bytes in place, so they are not applied
  to the editor ahead of the hold's Enter.
- Behind a parked command the bytes are appended to `handoff_bytes_after_command`
  (`pty_controller.rs:1414-1417`).
- Pty gone: one insert of the concatenated text.

### Why this and not four patches

One deleted branch (H1) covers three of the four paths, and H6 makes the remaining discard
greppable and testable. The alternative that was considered for path 2, holding the clicked
command until the hold ends (extending #2198 to user commands), gives the exact plain-terminal
order but puts the editor's submit through the held-command machinery built for agent commands
(`pty_controller.rs:2616-2624`): too much change on this path for a window this narrow.

### Conflicts with the owner rules

- **Q1 (#2207, owner's call).** #2207 says the escape hold's Enter and keys are "written in
  typed order when the session ends". At a real pty exit there is nothing to write to (F2).
  Proposal: at pty exit they become input text like every other hold; the real ssh-gone case
  (the local shell took input back, `view:15165`) is unchanged. Two view tests then change
  their SETUP, not their assertion: they model "ssh is gone" with `reset_on_exit`
  (`input_test.rs:21747`, `:21843`) and would use the path the app takes
  (`session_ended = false`, `input_test.rs:21845-21850`). If the owner wants them untouched,
  V1 must test only the view's `pty_gone`, which leaves a window between the model's exit and
  the view's exit event where the write still goes to the dead pty.
- **Q2 (path 2, owner's call).** After H2 a clicked command runs, then the held Enter and the
  later lines; the head of the Enter's line is in the shell and comes back into the editor
  with the stashed report. So `cd build`, Enter, `make clean`, Enter plus a click can run
  `make clean` without `cd build`, which is what "keep them together" (2026-11-09) forbids.
  Alternative: keep Enter and keys with the line as input text (`keep_given_up_keys_with_line`,
  `giveup:254`); nothing runs, nothing is lost. Recommended: the alternative, if the owner
  reads "keep them together" as outranking "write to the pty" here.
- **Q3 (bootstrap give-up).** The brief states #1577 A7 as "held keys are handed back to the
  input with a toast". The code no longer does that: since #1996 the 7 s give-up writes them
  to the pty (`view:26721-26781`) and hands back only when the shell is gone
  (`view:26783-26809`). This design touches neither; its pty-gone rule agrees with the
  shell-gone half. The owner should confirm which statement is current.
- **Q4 (app interrupts).** Stop, a dismissed warpify block and rewind cancel a deferred Enter
  and void its keys with no key typed (`view:10602-10610`). Kept as a flush (a real `^C`
  follows). Say so if a click must never void typed keys.
- **Q5 (secrets).** Pty-gone text is visible. If a reader ahead of the line editor was noted
  when the pty died, should those keys be discarded instead of shown? Proposal: yes, with a
  counted log line; it is the only new discard and it needs the owner's word.
- No conflict found with: ssh connect typeahead, managed ssh (#1641), no drain on exit 255,
  #851, event-driven hand-overs (no timer added or changed).

## 4. Test plan

### Unit, `typeahead_handoff_tests.rs`: the 13 `Dropped` assertions

| line | test | change |
|---|---|---|
| 3711 | `a_signal_ends_a_queued_young_hold_as_it_ends_the_buffered_one` | rename of the variant only |
| 3779 | `an_expired_timer_cannot_release_a_later_queued_hold` | rename only |
| 3816 | `a_queued_young_hold_does_not_outlive_the_state_it_was_made_in` (stale state) | **`Typeahead`** (1b) |
| 3828 | same test (exit) | **`Typeahead`**, plus `session_gone()` (1a) |
| 3845 | `ctrl_c_cancels_a_queued_young_hold_behind_the_request` | rename only |
| 3890 | `a_command_written_in_a_state_that_cannot_own_the_hold_drops_it` | **`Typeahead`**, test renamed (2) |
| 3911 | `a_deferred_enter_names_its_gate_and_every_end_is_recorded_for_the_view` (`^C`) | rename only |
| 3921 | same test (exit) | **`Typeahead`** (3a) |
| 7080 | `a_signal_behind_a_deferred_enter_waits_in_order_r18u23_fuzz` | rename only |
| 7982 | `a_young_enter_and_the_keys_behind_it_survive_an_uncounted_command_in_order` | rename only (asserts absence) |
| 8355 | `other_young_holds_are_still_dropped_when_their_session_goes` (exit) | **`Typeahead`**, test renamed; this guard asserts the behaviour #2209 removes |
| 8365 | same test (stale state) | **`Typeahead`** |
| 8374 | same test (`^C` on the escape hold) | rename only |

Six assertions in four tests change, each because its path is one #2209 names; seven are the
rename. No assertion is loosened.

New unit tests:

- `no_event_but_a_signal_voids_held_keys`: every hold fixture (young hold in `PtyOwned`, behind
  a request, forced into `EditorOwned` and `HoldAfterEnter`; a deferred Enter, released and
  not) x every public event that is not a signal: no `FlushedBySignal` is recorded.
- `flushed_by_signal_is_built_only_by_the_signal_paths`: source scan of the two files.
- `a_young_hold_whose_pty_died_is_written_out_and_the_session_is_marked_gone`.
- `keys_behind_a_deferred_enter_are_written_out_when_the_pty_dies`.
- `a_hold_the_state_no_longer_owns_ends_at_the_edge_not_at_the_next_key` (H5).
- `a_command_written_behind_a_request_takes_the_held_enter_and_keys_after_it` (H2).
- `a_deferral_give_up_never_voids_a_young_hold` (H4, forced state).

New view tests (`input_test.rs`, beside the #2207 ones):

- `test_young_hold_keys_become_input_text_when_the_shell_exits`: text in typed order, no pty
  write after the exit, queue idle.
- `test_keys_behind_a_deferred_enter_stay_with_its_line_when_the_shell_exits`.
- `test_the_exit_applies_hold_ends_without_waiting_for_a_cap` (F1).
- `test_a_hold_end_recorded_at_exit_before_the_exit_event_still_reaches_the_input` (Q1 window).
- `test_a_stale_young_hold_is_written_in_typed_order_with_its_editor_keys` (1b + V2).
- `test_a_clicked_command_behind_a_request_keeps_the_held_enter_and_keys` (2; expectation
  per Q2).
- `test_ctrl_c_still_voids_a_young_hold_and_a_deferred_gate` (5, guard).
- The two #2207 tests: setup per Q1.

### Real app (appci / lab); names are the case functions

Existing, must stay green on every platform they run on:

- `c_giveup_keys_reach_shell` (`cases_shells.py:196`, PROVING): give-ups write, none dropped.
- `c_ctrl_c_no_esc_i_local` (`cases_shells.py:101`), lab `c_ctrl_c_no_esc_i`
  (`cases_lab.py:1448`): `^C` still flushes.
- lab `c_lab_drop` (`cases_lab.py:1406`): `~.` arms, the #2207 path.
- lab `c_password_prompt` (`cases_lab.py:1750`), `c_ssh_password` (`cases_caseremote.py:1545`).
- `c_ssh_fail` (`cases.py:3790`), `c_connect_typeahead` (`cases_lab_winport.py:84`).
- Windows: `c_gitbash_typeahead_reps` (`cases_win_handover.py:143`),
  `c_ps_fast_command_young_enter` (`cases_win_input.py:171`),
  `c_ssh_gitbash_typeahead_managed` (`:401`), `c_ssh_gitbash_failing_not_local_managed`
  (`:336`), `c_ssh_nested_typeahead_wrapper` (`:92`).
- Latency: `c_agent_latency_key`, `c_agent_latency_enter` (`cases_agent_latency.py:297,489`),
  `c_latency_echo_ab` (`cases_keyframe_ab.py:171`), against their bands.

New, in `cases.PROVING`, both agent and terminal mode:

- `exit_keeps_typed_keys` (Linux bash / zsh / fish, macOS zsh / bash, Windows Git Bash and
  PowerShell): keys typed at 10-25 ms/key while a hold is live, then the shell is killed.
  Oracle: the pane's input holds exactly the typed text in order; the log has
  `give_up_keys_to_pty why=session_gone ... pty_gone=1` with `to_editor` equal to the bytes
  typed. Keeping a hold live until the kill needs the shell stopped (SIGSTOP on POSIX; a
  suspend on Windows): the harness side is NOT designed yet.
- `exit_keeps_keys_behind_deferred_enter`: same, with an Enter deferred behind its report.
- lab `lab_exit_keeps_typed_keys` on the Linux, macOS and Windows lab ssh servers: the same
  with the pane's shell killed while a remote prompt is in front.
- Run-wide oracle, every case: the count of the new flush hop never exceeds the signals the
  harness sent; any `"dropped"` release without one fails the run.

Paths 1b, 2 and 4 have no deterministic real-app trigger (1b and 4 are unreachable; 2 needs a
click inside a round trip); the unit and view tests are their proof. `docs/testing.md` is
updated in the same merge.

## 5. Risks

| risk | why | caught by |
|---|---|---|
| Latency | None expected: no per-key work added. H5 is one flag read per hand-off change; `session_gone` is read only inside the `Typeahead` arm | the latency cases against their bands |
| #2182 reorder class | H2 writes keys behind a command; an app in-band command must not pass them | `a_young_enter_and_the_keys_behind_it_survive_an_uncounted_command_in_order`, the new H2 tests, `c_gitbash_typeahead_reps` |
| Uncounted bytes | A hold written out while the editor owns input is routed `Write`, not counted (`handoff:5147`), yet the writer still decrements `writes_in_flight` (`pty_controller.rs:1423`) and the bytes are untracked line bytes (#1089). #2207 already has this for its hold; H5 widens it to states that are unreachable today | new view test for 1b asserts the counts and that the next report covers the bytes; to be checked first in phase 2 |
| ssh escape `~.` | The escape hold keeps its variant and its tests; only its pty-exit end changes (Q1) | the #2207 unit tests unchanged, `c_lab_drop` |
| Password-prompt flush | A written-out hold goes to the pty, never to the editor, so a secret is not shown; the pty-gone branch does show text (Q5) | `c_password_prompt`, `c_gitbash_precmd_secret`, a view test for Q5 once decided |
| Text into a disabled editor | The exit arm disables the editor (`view:19303`); V3 must run before it, and `system_insert` on a dead pane is unverified | `test_young_hold_keys_become_input_text_when_the_shell_exits` |
| A leading line break | The held Enter is the first byte; `text_for_editor` makes it a line break at the top of the input | the same test pins the exact text |
| Double delivery | The leak net (`view:15670`) and the exit end can both reach a deferred gate | `test_the_exit_applies_hold_ends_without_waiting_for_a_cap` asserts once-only |

## 6. Not determined

- Whether a write to a dead pty returns "refused" (which would already route to the input) or
  is silently lost; F2 assumes lost.
- Whether path 2 has a trigger besides a click (a synced input, `input.rs:1116`).
- F8 (`view:13056`) and F9: not traced.
- View tests that assert the old drop by its effect rather than by `YoungHoldEnd::Dropped`:
  the grep finds none outside the hand-off tests, but only a box run will show them.
- The harness hook for `exit_keeps_typed_keys` on Windows.
