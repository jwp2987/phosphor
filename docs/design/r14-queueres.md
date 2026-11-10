# Round 14: the open input-queue issues, reconciled (r14-queueres)

Branch `fix/r14-queueres`, from integ/r14 `4bf571aae`. Nothing here was compiled or run; Rust is
`rustfmt --check` only. Wording: "worked, unproven" until a run shows it. Line numbers are those of
`4bf571aae`.

Issues: #1900, #1997, #1998, #1999, #2000, #2047, #1991.

| issue | state before this branch | this branch |
|---|---|---|
| #1900 | NOT WORKED (real) | worked: synced input keeps `Origin::Synced` wherever the pane queue holds it |
| #1997 | FIXED-IN-CODE (`3d32ce1db`, an ancestor), unit-proven in r11a and r12a | nothing; label recommendation only |
| #1998 | NOT WORKED (real, reachability of the entry kinds not established) | worked: every entry behind a dropped ssh hold is handed on or counted |
| #1999 | NOT WORKED, needs an owner yes (the issue says so) | nothing; left for the owner |
| #2000 | NOT WORKED, needs an owner decision (the issue says so) | nothing; left for the owner |
| #2047 | NOT WORKED (real, rare) | worked: the young hold looks for a reader at its own events and at its net |
| #1991 | UNCONFIRMED | CONFIRMED by an echo capture; no app change (owner list; the cheap fix is wrong, see below) |

Timers: none added. The one timer this work touches is the young hold's existing net
(`YOUNG_HOLD_GIVE_UP`), which gains a look before it writes; that look is also made at two events
so the net is not the normal way out.

Kill switches (all in `app/src/terminal/input_queue/switches.rs`, read once per process, off for
`0` / `off` / `false` / `no`):

| variable | off restores |
|---|---|
| `PHOSPHOR_QUEUE_KEEP_SYNCED_ORIGIN` | #1900: held synced input is a keyboard entry again |
| `PHOSPHOR_SSH_DROP_ACCOUNTS_ENTRIES` | #1998: the silent skips on the hand-back and local-shell paths |
| `PHOSPHOR_YOUNG_HOLD_READER_LOOK` | #2047: a young Enter is never given to a reader; its net writes it as the shell's line |

## 1. #1900: synced origin lost for input held in the pane queue

**Cause.** The delivery decides "this input is a synced relay, it must not take over an agent-run
command" from the queue ENTRY's origin: `PtyQueueHost::deliver` sets
`delivering_synced = matches!(entry.origin, Origin::Synced)` (`view/paste_queue.rs:613`), read by
`input_is_synced` and `decide_delivery` (`view/dest_table.rs:328`, #1893). Three producers drop
that origin:

1. `Input::queue_behind_enter_with` offers with `guard.offer` / `Origin::Keyboard`
   (`input/enter_queue.rs:1377-1384`). Synced text reaches it from the table's `hold_arrival`
   (`view/dest_table.rs:967`) and from the legacy chain's `enter_queue_takes_key`
   (`view.rs:40284`); `write_to_pty_for_syncing_long_running_commands` sends synced text through
   `typed_characters_on_terminal` (`view.rs:15047`).
2. `ssh_hold_push` and `ssh_hold_defer_busy` offer the connect's `Held` entries with
   `Origin::Keyboard` (`view/ssh_queue.rs:182`, `188`, `214`). Not in the issue; same loss.
3. `InputQueue::apply_follow_ups` gives a re-offered entry `Origin::Replay(from)`
   (`input_queue/mod.rs:1709`). No production code calls `FollowUps::offer` today (only the mock
   host of `input_queue/tests.rs`), so this one is latent.

Not affected: pty bytes (`EntryKind::PtyBytes` carries `takes_over_agent_control` itself, and the
synced writer passes `false`), and `offer_arrival_to_queue`, which already carries the origin.

**Fix.**
- `TerminalView::held_input_origin()` (`dest_table.rs`): `Synced` when the input being held is
  synced (`input_is_synced`, so a synced entry re-held mid-delivery stays synced), else `Keyboard`.
  The ssh hold offers with it.
- `TerminalView::queue_synced_behind_enter` (`dest_table.rs`): the view offers synced text / Enter
  behind the open Enter gate itself, to the same pane queue, ordered like `queue_behind_enter`
  (`try_lock_ordered`, else the mailbox), with `Origin::Synced` and the key's stamp. Both callers
  use it for synced input only; the user's keys keep the input's route unchanged.
- `follow_up_origin` (`input_queue/mod.rs`): a follow-up of a synced entry is synced.

**Rejected.** An origin parameter on `Input::queue_behind_enter` (the issue's wording: "carry the
origin in `AfterEnterInput`"): it edits `input/enter_queue.rs`, which this round asks to leave
alone, for a caller that only the view has. A field on `AfterEnterInput`: every constructor of the
enum would change for one producer.

**Risk.** Low. The entries are the same kinds in the same place; only `origin` differs, and its one
reader at delivery is `delivering_synced`. `PtyQueueHost` is the host of the view's other offers.
Whether synced input can meet an Enter gate at all needs a long-running command AND an open Enter
gate in the same pane; not reproduced in an app run.

## 2. #1997 / #1999 / #2000: lines typed behind a deferred Enter

`git log --grep` finds one commit for the group: `3d32ce1db` (#1997), an ancestor of this branch.

- **#1997: FIXED-IN-CODE, unit-proven.** `hand_deferred_gate_lines_to_waiting_enter`
  (`view/paste_queue.rs:1718`) hands the keys to the input's Enter gate in place
  (`InputQueue::hand_gate_to`, `input_queue/mod.rs:1427`) when `Input::enter_gate_holding_a_line`
  (`input/enter_queue.rs:588`) names one. Tests: `a_line_typed_behind_a_deferred_enter_never_reaches_the_pty_before_that_enters_held_line`,
  `a_line_typed_behind_a_deferred_enter_follows_that_enters_held_line_at_readiness`,
  `typeahead_handoff_keys_the_editor_took_after_a_deferred_enter_do_not_join_its_line`
  (`view_test.rs:18199-18359`), `hand_gate_to_*` (`input_queue/tests.rs:882-1020`). Runs: the
  `_at_readiness` test FAILED in r10a (a test fault, #2010: the recorder did not listen for
  `WriteHandoffUserBytesToPty`); none of them is among the failures of r11a (11879 run, 3 failed)
  or r12a (11905 run, 4 failed) in `test-results/*/unit/test-linux.json`. The app-level row
  `giveup_keys_reach_shell` passes in r11a but says "no hold gave up in this run", so the path is
  proven by unit and view tests only. DECLINED.md records the decision ("Keep-together holds on
  the event path too"). Recommended label: `verified` on the unit evidence, or keep
  `fixed-untested` until an app run exercises an unready shell; the coordinator's call.
- **#1999: NOT WORKED.** `enter_gate_holding_a_line` still returns `None` for an empty line
  (`has_line`, `enter_queue.rs:596-600`), so lines typed behind a BARE deferred Enter are written
  at once and the bare Enter later. The issue's fix flips the expectation of
  `typeahead_handoff_releases_held_shell_lines_to_the_pty_when_the_submit_runs_nothing` and says
  "needs an owner yes"; DECLINED.md says the opposite today ("Lines typed behind a BARE deferred
  Enter are still written at once"). Left for the owner. If yes: drop the `has_line` condition
  (one line in `enter_queue.rs`) and change that test's expectation, naming the decision.
- **#2000: NOT WORKED.** `keep_deferred_enter_text` (`view.rs:16155`) still calls
  `release_keys_held_behind_unsubmitted_enter`, and `release_held_keys` writes held keys that
  contain an Enter (`view.rs:16216`). Callers: `view.rs:15523` (the report refused, #1569),
  `15624` (`CannotRun`), `16030` (`KeepInEditor`). No Enter gate is open on these paths, so the
  #1997 hand-over does not apply. The issue asks for the timer path's rule
  (`keep_given_up_keys_with_line`) on three event paths and says "owner decision needed". Left for
  the owner. If yes: `keep_deferred_enter_text` takes the gate's keys with
  `GateKeysThen::GiveUp`-like handling that ends in `keep_given_up_keys_with_line` when the input
  holds a line; that touches `give_up_to_pty.rs` only as a caller.

## 3. #1998: entries behind a dropped ssh hold that are skipped

**Cause.** `TerminalView::ssh_drop_detached` (`view/ssh_queue.rs:403-509`), last arms: an `Enter`
and `PtyBytes` become held inputs only `if for_pty_give_up`; `Control`, `Seq`, `EditorKey`, `Raw`
never; the fall-through arm logs only at a give-up. On `HandBackText` and `WriteToLocalPty` the
entry is gone without a line in the log. Also: the unreachable busy branch defers the drop as
`SshDropThen::Discard` whatever the caller asked for.

Whether such entries can sit in the hold's segment is still not established (the hold's own keys
are `Held` entries; `Text` and pastes queued behind an Enter that opened the connect are already
handled). The fix is the accounting the issue asks for, not a claim that keys were being lost.

**Fix.** A pure function, `dropped_entry_inputs(kind, then, accounts)`, decides every kind except
a paste (whose content lives in the view's and the editor's stores):

| kind | `GiveUpToPty` | `WriteToLocalPty` | `HandBackText` | `Discard` |
|---|---|---|---|---|
| `Held` (not a signal), `Text` | as before | as before | as before | as before (counted by the caller) |
| `Held` signal | its byte (as before) | discarded (OD-5) | discarded | discarded |
| `Enter` | `\r` (as before) | `\r` | a line break | not handed on |
| `PtyBytes` | the bytes (as before) | the bytes | split into the keys the live path holds (`held_inputs_from_tty_bytes`); not UTF-8: counted | not handed on |
| `Control`, `Seq` | the key | the key | the key (`unexecuted_text` edits or skips it, as for a held key) | not handed on |
| `EditorKey` | text, or `\r` for an Enter; another action: counted | same | same | not handed on |
| `Raw`, `Signal` | counted | counted | counted | not handed on |

"Counted" is one warning per drop with the kinds and a count, plus the hop
`kh_ssh_drop_no_form then=<..> entries=<n> kinds=<..>` (never content). `ssh_drop_detached` takes
the `SshDropThen` instead of a bool, and the busy branch defers with the caller's `then`.

**Rejected.** Handing an editor cursor action back as text (it has none). Writing `Raw` bytes
(mouse reports) to the local shell: they are passive pty-only bytes, never typed.

**Risk.** Low: the paths that already handed entries on are unchanged row for row (first row of
the table, and the give-up column for Enter and pty bytes). New behaviour only for entries that
were dropped before.

## 4. #2047: a young Enter held while a reader ahead of the line editor has the tty

**Cause.** A precmd function's `read -s` reads the tty between a prompt's Precmd and its line
editor. The hand-off learns of that reader at a key: #2038 on ConPTY
(`note_prompt_reader_if_the_screen_asks`, `view/paste_queue.rs:2216`: the screen, `try_lock` only)
and #2050 where the tty is sampled (`hand_keys_to_a_secret_reader`, `paste_queue.rs:2247`). Both
looks are skipped once a hold is queued (`prompt_reader_look_due` and `secret_reader_look_due`
require `!young_hold_queued`, `model/typeahead_handoff.rs:1789`, `1839`). When the look misses at
the Enter that ends the read (the model was busy; r13-pin2 race 3), `route_user_bytes` finds no
reader, the wait is past the prompt hold's cap, and the Enter is a young Enter with evidence
`RequestDue` (`typeahead_handoff.rs:3562-3580`). No request can be honoured while the `read` has
the tty, so the hold ends only at its net, 10 s later (`give_up_queued_young_hold`,
`paste_queue.rs:1858`), which writes the Enter with `route_expired_young_chunks` ->
`route_user_bytes_unheld`: counted as a line of the shell's that no prompt answers. The unit
control for that state exists: `without_the_reader_the_pin_is_the_shells_line_and_its_enter_is_held_for_a_report`.

**Fix.** The hold looks again, with the same two facts, in the same states those looks are allowed
in, at the events it has.

- Hand-off: `young_hold_reader_look() -> Option<Screen | Tty>` is the state gate: a young hold is
  queued, the pty owns input, no reader is noted, no prompt hold is queued, no Enter is deferred,
  and either #2038's state (`prompt_reader_may_ask`: ConPTY, a shell that signals, this prompt's
  line editor has not signalled) or #2050's (the tty is sampled, a Precmd parsed and neither the
  line editor's marker nor a Preexec since). `give_young_hold_to_prompt_reader()` ends the hold and
  notes the reader; `route_expired_young_chunks` then routes that hold's keys with
  `route_prompt_reader_bytes` (in flight, not line content, the Enter not counted, no enter window).
  If the reader ended before the keys were routed (the release waited in the queue's mailbox) they
  are the shell's line, as any released hold's.
- View: `give_young_hold_to_a_reader_ahead_of_the_line_editor(site)` reads the fact (ConPTY: the
  screen tail, `block_tail_asks_for_input`; sampled tty: `tty_is_reading_secret` through the
  hand-off's probe) and releases the hold with the existing `release_young_hold_keys`. Sites:
  - `cap`: in `give_up_queued_young_hold`, before the expiry. The model is waited for there, as the
    prompt hold's cap does (a timer callback, not a key).
  - `key`: in `settle_stale_young_hold_before_key`, when a key is about to queue behind the hold
    (`try_lock`). The key then finds the queue idle and takes #2038's route behind the Enter.
  - `output`: in `handle_terminal_wakeup` (`try_lock`). The parser that held the model at the
    Enter wakes the view when it lets go, and a question printed after the keys arrives with one.
- Ordering: the release is the hold's existing one (`close_gate_taking`, one write per key event
  on ConPTY); a release that finds the queue busy is the existing `Deferred::ReleaseYoungHoldKeys`,
  applied before any later key (`offer_to_paste_queue` flushes the mailbox first).

**Why the two event sites and not the net alone.** The net alone is correct but leaves the user
10 s behind an Enter. The event looks decide exactly what the Enter's own look would have decided
had the model been free one event earlier: same state gate, same fact. No new class of false
"asks" (the stale-tail row of r13-pin2, item 5, is #2038's and unchanged).

**Rejected.** A retry timer for the missed look (a timer deciding routing). A blocking model lock
at the key (the key path never waits for the parser). Ending the hold as typeahead
(`YoungHoldEnd::Typeahead`): it counts the Enter as the shell's line, which is the defect.

**Risk.** Medium-low. The new routing runs only when a young hold is live in a state where a key
with no hold would already be given to the reader. A wrong "asks" would send a young Enter to the
pty uncounted instead of running it in the editor on the report; the same wrong fact already does
that to every unheld key.

## 5. #1991: the line-clear prefix echo under a line editor

On the owner list of this round; no app change. What was unconfirmed is now measured.

**Evidence (an echo capture on the laptop, a pty and `python3`, no app).** ` ^K^U` + a setup-shaped
line + CR written to an interactive shell at its prompt:

| reader | what comes back before the line |
|---|---|
| dash (cooked tty) | ` ^K\b \b\b \b\b \b`: exactly `LINE_CLEAR_ECHO` |
| bash (readline) | ` \b` then `ESC[K` |
| zsh (zle) | ` \b \b` |

So under readline and zle the cooked form never arrives. In `SetupEchoFilter::swallow`
(`model/setup_echo_filter.rs:130`) the prefix stays outstanding (`swallow_prefix` only releases
what cannot start it), the expected lines are never looked at, and for the whole attempt
`started()` is false and `lines_ended()` is 0. The issue's three consequences follow from the code
as written: R9 reads "queue holds bytes, nothing echoed" as provably unread
(`cancel_flushes_unread_setup`, `view/subshell_ready.rs:996`), R7 cannot confirm an unsupported
line-editor shell (`judge_written` needs `echo_ended`, `subshell_ready.rs:844`), and the echo is
not hidden. One more, seen in the trace: every space is held back as a possible prefix start until
the next character, so a prompt's trailing space is not on the grid while the filter is armed.

Not confirmed from an app log: no committed run carries `subshell_written_judged` lines
(`test-results/` keeps verdicts, not app logs).

**Why no patch here.** The cheap fix, "a write that went to a line editor never flushes", is wrong:
the same flush is what keeps unread stage-2 setup bytes out of a question prompt when a cancel is
a real one (#1014's class), and under a line editor `echo_started` is false for those too. The
other cheap fix, expecting the editor's forms of the prefix, does not survive a setup line longer
than the terminal is wide (readline redraws at the margin). This is a second look at the same
path, so it gets a design, not a patch:

1. Record at the write whether the reader was a line editor (the judge knows:
   `SubshellTtyKind::LineReader`) and, there, do not use the echo as proof at all.
2. R9 for such a write: flush only when the tty's input queue still holds the WHOLE write (a byte
   count from the same sample that gives `queue_empty` today, against the bytes written). Half read
   means the editor is taking the line: never cut it.
3. R7 for such a write: the proof that the shell took the line is the line feed the editor prints
   at accept-line followed by a fresh prompt with no marker, read from the grid position, not from
   the filter.
4. Hide the echo there by the block reset at `InitShell` only (as today in practice).

Owner question, unchanged: whether to spend a round on this now. To confirm from a run first: one
plain bash or zsh subshell setup with the text log on, and read `echo_ended=` in
`subshell_written_judged`.

## 6. What the next run should show

- Unit: the new tests listed in `docs/testing.md` ("Round 14: the open input-queue issues") pass;
  no existing expectation changed.
- App: no case exercises these paths by construction (synced panes behind an Enter gate; foreign
  entries behind an ssh hold; a missed look at a precmd `read -s`). `gitbash_precmd_secret` and
  `precmd_secret_honor_ps1` must stay PASS; in their logs `hop=young_hold_reader_asks` should not
  appear in a normal pass (it names the rescue), and if it does, the PIN's Enter still reaches the
  read at once.
