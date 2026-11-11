# r17-step8: #1353 step 8, the destination table is the only destination path

Branch `fix/r17-step8` (from `integ/r14` 04e5193a2). Owner 2026-11-11: in round 14, merged LAST
so it can be dropped alone. Nothing here was compiled or run (no cargo on this machine).

## What this step is

Since 2026-11-02 the destination table (`input_queue/table.rs`, #1424) decides where every live
key, every delivered entry, every paste and every origin write goes. The legacy chain of
predicates stayed in the tree as (a) the `PHOSPHOR_QUEUE_DESTINATION=legacy` kill switch, (b) the
`shadow` comparison and (c) the fall-back of two "no leaf" arms. This step removes all three, so
there is one path and one place that states where an input goes.

No routing is meant to change for a process that ran with the switch unset (the default, `table`).
The four places where the table mode itself still reached legacy code are listed in "Behaviour"
below, with what replaces them.

## What is removed, and why each removal is safe

| removed | where | why it is safe |
|---|---|---|
| `DestinationMode`, `destination_mode()`, `set_destination_mode_for_test`, the env parse | `input_queue/facts.rs` | every call site keeps its `Table` branch only (list below) |
| `control_sequence_on_terminal_legacy`, `keydown_on_terminal_legacy`, `typed_characters_on_terminal_legacy`, `replay_held_key` + `replay_held_key_legacy` | `terminal/view.rs` | in `table` mode a live key never reached them (`arrives_via_table`); a delivery never did either (`deliver_via_table`); the two fall-back arms that did are replaced (see Behaviour) |
| `route_after_enter_input`, `PtyQueueHost::deliver_replayed_entry` | `view.rs`, `view/paste_queue.rs` | legacy-only delivery of text / Enter typed behind an Enter (`debug_assert!(mode != Table)` already said so, #1587); the table's `deliver_replayed_entry_table` is the delivery |
| `deliver_ssh_held`, `deliver_held_input` | `view/ssh_queue.rs` | legacy-only delivery of the ssh-held inputs; `deliver_held_via_table` is the delivery |
| `hold_input_during_ssh_connect`, `swallow_input_during_ssh_setup`, the legacy branch of `paste()`, `ssh_paste_to_hold` | `view.rs` | the legacy ssh hold probe (it ran the ssh refreshers on the key path). Rows H1-H3 and the edge-driven `SshPhase` (`view/ssh_edges.rs`, step 7) decide the hold; `paste()` already branched on the table's plan (#1853) |
| `pane_key::route_typed`, `after_pty_attempt`, `TypedRoute`, `TypedTail`, `TailAction`, `TypedRouteProbes`, the view's `TypedKeyProbes` | `terminal/pane_key.rs`, `view.rs` | the ordered rows of the legacy typed path; rows H1-H8 and D1-D8 of the table state them. Their unit tests (the pure row-order tests of `pane_key`, two snapshots) go with them; the table's own row tests (`table_tests.rs`, `table_enum_tests.rs`, `table_equiv_tests.rs`) stay |
| `typed_on_settled_ai_target`, `model_lock_trace::perf_agent_key_enabled` (`PHOSPHOR_PERF_AGENT_KEY`), `replayed_enter_goes_to_pty`, `is_ssh_interrupt_key`, `log_ssh_drop_state`, `ssh_edges::phase_name`, `PasteQueueState::delivering_stamp` / `delivering_origin` | `view.rs`, `model_lock_trace.rs`, `view/ssh_edges.rs`, `view/paste_queue.rs` | each had no caller left once the bodies above were gone (word-grep over `app/src`; the perf switch chose between two forms of a probe only `route_typed` ran) |
| the shadow comparison: `input_queue/shadow.rs`, `shadow_tests.rs`, `shadow_begin` / `shadow_end` / `shadow_origin`, every `note_shadow_leaf`, `nav_key_goes_to_pty`, `OriginRow::shadow`, `end_origin_row`, `check_editor_matrix`, `SSH_PHASE_DISAGREEMENTS` / `kh_ssh_phase_disagree`, the `derived` ssh phase of `build_route_facts` | several | shadow compared legacy with the table; with no legacy there is nothing to compare. It changed no routing in any mode |
| the legacy branches of `queue_signal_of` (`abort_signal`; the function is now `signal_of`), `ssh_machine_runs`, `ssh_events_own_the_gate`, `arm_ssh_connect_gate_at_preexec`, `note_queue_focus_changed`, `request_queue_pin_clear`, `begin_origin_row`, the editor's `legacy_passes`, `conpty_enter_hold_enabled` | several | each was `if table { A } else { B }`; A stays. `InputQueue::editor_keys_pass_gates` and `paste_hold::key_submits_or_completes` (the #1423 statement the editor's hold decision used in legacy) are KEPT as the oracle of two tests that hold the table's H8 editor column to them, marked `allow(dead_code)` outside tests |
| test-only counters `legacy_chain_deliveries`, `note_legacy_chain_delivery` | `view/dest_table.rs` | they told a table delivery from a chain delivery; there is no chain |

The 20 `destination_mode()` call sites (non-test), each reduced to its `Table` arm:
`facts.rs` (definition), `shadow.rs` x2 (file removed), `view/ssh_edges.rs` x3,
`view/dest_table.rs` x4 (`arrives_via_table`, `begin_origin_row`, `note_queue_focus_changed`,
`request_queue_pin_clear`), `view/paste_queue.rs` x4 (`queue_signal_of`, `deliver_entry` x3),
`model/typeahead_handoff.rs` (`conpty_enter_hold_enabled`), `editor/view/mod.rs`
(`hold_key_for_paste_on_queue`), `view.rs` x2 (`route_after_enter_input`, `paste`).

Kept on purpose (not part of the destination decision, used by the table path today):
`refresh_ssh_connect_gate` (the event-side arming and the signal rows read it),
`agent_pty_write_gate` (checked next to row D13), `paste_routing` (the delivery of a routed paste
and the give-up writers read the bracketed-paste bit from it), `deliver_paste_text`,
`write_user_bytes_to_pty` and its stages (the leaf writers every plan executes into),
`prepare_subshell_gates`, the take-and-write of the hand-off's and the PowerShell gate's keys as
bytes (`hold_arrival_as_bytes`; the module comment of `dest_table.rs` said "step 8 retires them":
it does not, see Rejected).

## The switch

`PHOSPHOR_QUEUE_DESTINATION` is no longer read as a mode. The app looks at it once, when the
first terminal view is built: unset, empty or `table` is silent; any other value logs one line
(`warn`): the legacy and shadow modes were removed and the value is ignored. So: the switch is
REMOVED, and a leftover `legacy` / `shadow` in someone's environment is refused with a log line
instead of silently doing nothing. The revert path is this branch (merged last, alone), as the
owner said for the holders ("we're in git with tags").

There is no new kill switch: this step adds no behaviour, and a switch back to the removed code
cannot exist. The four replacements below are the only lines that can run differently, and only
where `table` mode itself still ran legacy code (two of them logged `no leaf ... using the legacy
chain` when they did).

## Behaviour: the places `table` mode still reached legacy code

1. **A delivered entry whose plan was `Hold(SubshellSetup)`** (`table_unexpected`). By reading,
   this is the only plan that reached the fall-back: a delivery decides on `QueueFacts::default()`
   (no gate covers a delivered entry), but `build_route_facts` derives `SubshellPhase::SetupHeld`
   from an ANNOUNCED setup alone (`route_facts.rs`, the arrival's rule: the writer opens the gate
   with the first key). Mid-drain `prepare_subshell_gates` cannot open a gate (`try_peek` is
   `None`), so row H7 answered `Hold` for a gate that does not exist, the executor had no leaf and
   the input went through the legacy chain, which, mid-drain, passes every stage and ends at the
   destination the legacy predicates pick (the pty, or the editor).
   **Now:** `decide_delivery` reads an announced-only `SetupHeld` as `Announced`, which no row
   holds (`table::delivery_reads_setup_as_announced`), and the destination rows D1-D11 decide: the
   same destination the legacy chain reached, decided by the table. One log hop
   (`kh_table_delivery_past_setup`, counts only). This keeps today's behaviour. Whether text
   delivered while a subshell setup is announced SHOULD be held behind the setup instead is a
   separate question (see Findings).
2. **Any other plan the executors have no leaf for** (`table_no_leaf`, and the last arm of
   `execute_key_arrival`). By enumeration of `decide` for a delivery (origin `Held` / `Replay`,
   `delivering`, no gates) and for an arrival, none exists once (1) is gone:
   `every_delivery_plan_has_a_leaf`, `every_arrival_plan_has_a_leaf` (`table_enum_tests.rs`, the
   executors' arms written as predicates, about 9 million decisions each). The arms stay as a
   logged error (`log::error!`, row and outcome, never the text; hop `kh_table_no_leaf`) with a
   leaf that cannot run anything: text and pastes are put in the input editor (visible, not
   submitted); a control key or sequence is not applied. A test-only counter
   (`dest_table::no_leaf_hits`) and the appci run-wide row `table_no_leaf` watch for it.
3. **An entry point called while the queue is delivering** (`paste_queue.delivering`).
   `arrives_via_table()` was false there, so the legacy body ran and its writes passed "as part
   of the delivery". No production caller does this (the three entry points are called from event
   and action handlers, which the UI framework runs after the update that drains the queue). With
   the bodies gone such a key is offered to the pane queue with its stamp and origin (the queue is
   busy, so it is kept in the mailbox and applied in order when the drain ends; the table then
   decides it). A SIGNAL does not wait (it never does): it is decided at once by the signal rows,
   as any arrival, and its write passes the busy queue as it did. Logged
   (`kh_arrival_mid_delivery`). Order kept, nothing lost.
4. **A control sequence whose bytes are not UTF-8** (`control_sequence_on_terminal`). The table's
   arrival takes text, so such bytes took the legacy body even in `table` mode. They are now
   decided by the same rows as any sequence (`arrive_sequence_bytes_via_table`, class `Seq`) and
   written as bytes through the writer: the pty or the gate the writer holds them behind, the
   editor's key after a hand-off, or a logged drop (agent-owned pty, nothing running), which is
   what the legacy body did for them. As before, the ssh connect does not hold them (it holds
   text). No key binding is known to produce such bytes.

## What I rejected

* **Keeping `legacy` as a mode that refuses to start / panics.** A terminal that will not open
  because of a stale variable is worse than a log line.
* **Keeping `shadow`.** It needs the legacy leaves to compare with.
* **Retiring the bytes take-and-write of the hand-off gates and the PowerShell gate**
  (`hold_arrival_as_bytes`, `bytes_of_taken_entries`), which the `dest_table.rs` comment
  assigned to step 8. That is a change to how held keys are released (entries with `(major,
  minor)` ids, R11), not a removal of dead code; it touches every barrier comparison and cannot
  be proven by reading. Not done; said in the report.
* **Holding a delivered entry behind an announced subshell setup** (fixing (1) instead of
  keeping it): a behaviour change in a cleanup that must be droppable alone.
* **Deleting `paste_routing`** (the legacy paste predicate): still read by three writers.

## Tests

* Every test that ran `for mode in [Legacy, Table]` runs its body once, in the only mode; every
  assertion of the Table arm stays, the `{mode:?}` tags leave the messages. Where a test branched
  on the mode, the Table branch stays. About 250 sites in `view_test.rs`, `input_test.rs` and
  `view/*_tests.rs`, rewritten by a script and then checked by hand where the script reported a
  use of `mode` it could not remove.
* Tests of removed code are removed with it: the shadow comparison tests, the Legacy-only tests,
  the `pane_key::route_typed` row tests and their two snapshots. Listed in the report and in
  `docs/testing.md`.
* Tests whose oracle WAS the legacy chain keep their scenarios with a stated oracle:
  `table_mode_arrivals_reach_the_same_destinations_as_legacy` states each scenario's outcome
  (derived by reading, not by a run); the three shadow-comparison tests over the arrival scenarios
  and the CLI-submit gates assert that no plan is left without a leaf.
* Tests that were pinned to `legacy` or `shadow` for a reason that no longer holds run on the
  table: `ssh_ctrl_d_is_held_after_a_prompt_was_answered` (the only view-level test of #1111),
  `route_facts_derive_the_phases_*`, `a_non_ssh_command_starting_releases_the_window_by_event`,
  `the_ssh_phase_follows_the_events_through_a_connect`.
* New: the two leaf enumerations and the announced-setup unit test (`table_enum_tests.rs`), the
  switch notice (`facts.rs`), a view test for (1) and one for (3).
* appci: `cases.dest_mode` is always `table`; the `legacy` pass (`KILL_SWITCH_SMOKE`) and the
  shadow row (`dest_disagree`) are gone; `run.py` fails a pass whose app log has a "no leaf"
  error line (row `table_no_leaf`, added only then). No new case: the change has no behaviour of
  its own for a case to play; the oracle is the absence of that line in every pass.

## Risk

* Size: about 250 test sites rewritten mechanically and several thousand lines removed, with no
  compiler. Compile doubts are listed in the report. The likely failures are unused imports,
  a helper that became dead (clippy `-D warnings` in `script/presubmit`), and a test that
  read a removed helper.
* A Table arm that only passed because the Legacy arm ran first in the same test body (shared
  state inside one `App::test`): the loops built a fresh app per mode in the cases I read, and
  any that did not is named in the report.
* `script/check_large_deletions` will fire (deletion-dominant): this note and the commit
  messages are the statement that the removal is deliberate; `DECLINED.md` gets the entry.

## Findings (not changed here)

* `ssh_setup_drop_notified` (the "Typing is ignored while the SSH setup prompt is open" toast, once)
  was reset only by the legacy ssh probe. The table path never reset it, so in the default mode the
  toast already showed once per pane, not once per dialog. Unchanged by this step; now nothing
  resets it at all.
* (1) above: text or an Enter delivered by the queue while a subshell setup is announced and no
  setup gate is open is written to the destination the rows pick (the pty when the block
  started), not held for the setup. Pre-existing; before this step the log said `no leaf for
  Hold(SubshellSetup)`. Needs a run to say whether it is reachable with a real shell.
