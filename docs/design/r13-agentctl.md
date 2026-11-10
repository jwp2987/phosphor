# Round 13: agent control (#1967 stuck turn, #1971 hand-back key, #1972 agent-run ssh with a remote read)

Fable 5.2, 2026-11-10, branch `fix/r13-agentctl` from integ/r13 574f83868. Nothing was compiled or run.
Line numbers are those of the base commit.

## 1. #1971: a key for "Give control back to agent"

**Finding: the key exists; what was missing is a name for it.** The issue is marked "suspected (no binding found)".

- The footer button already shows a key: `use_agent_footer/mod.rs:1270-1283` builds it with
  `KeystrokeSource::Fixed(USE_AGENT_KEYSTROKE)` (Cmd-Enter on macOS, Ctrl-Shift-Enter elsewhere, `mod.rs:186`).
- That key works with the terminal focused: the fixed binding at `view/init.rs:1187-1195` dispatches
  `TerminalAction::SetInputModeAgent` under `agent_conversation_predicate & agent_mode_predicate & command_predicate`, and
  the handler hands back first when `is_eligible_for_agent_handoff()` (`view.rs:39381-39390`). While the Phosphorize / ssh
  banner is up (`SubshellBanner`, outside `base_context`) the same key reaches `agent_shortcut_enter` (`view.rs:11473`),
  which calls the same handler.
- Missing: no editable binding and so no command palette entry, nothing the user can rebind, and no test that the predicate
  holds in the user-in-control state. `ToggleLongRunningCommandControl` (`view.rs:39836`) is bound to nothing.
- Ctrl-I / Cmd-I cannot be the hand-back key: once the user holds the command it is Tab for the program (#1024, the
  `take_over_key_passes_through_when_the_user_owns_the_command...` test).

**Change.** `TerminalAction::GiveControlBackToAgent` (hand back if the user holds an agent command, else nothing) and an
editable binding `terminal:give_control_back_to_agent` ("Give control back to agent", group Warp AI), offered while
`LONG_RUNNING_AGENT_REQUESTED_COMMAND_USER_TOOK_OVER` is set. No default key of its own: the default stays the fixed
Cmd-Enter / Ctrl-Shift-Enter the button shows ("what is displayed is exactly what works", #976), so there are not two
bindings on one keystroke. `agent_conversation_predicate` moved into `input_mode_predicates()` so a test can evaluate the
fixed key's predicate.

**Rejected.** A second default key (two bindings on one key, or a new chord nobody asked for); pointing the button at the
editable binding (an unset binding would show no key on the button).

**Risk.** Low: one new action, one palette entry, no key changes hands.

## 2. #1972: agent-run `ssh -tt host '<a program that reads>'` is never handed to the user

**Cause.** The hand-over needs the fact "the foreground reads the tty" (#1811, owner rule: no timer hands a command over).
For a read made on the REMOTE host no local fact exists:

- `ssh -tt` puts the LOCAL tty in raw mode. `PromptDetector::observe` (`interaction_mode.rs:950-1040`) reads the password
  shape as echo off AND canonical (`:958`), and asks for a prompt-shaped line only when the tty is cooked with echo on
  (`:968-972`). Raw mode is neither, so the detector never produces a candidate.
- The local foreground is `ssh`, asleep in `poll`/`select` over the socket and the tty for the whole session.
  `task_tty_read_verdict` (`terminal_attributes.rs:538-562`) says `NotReading` for it, and that is correct: ssh waits the
  same way while the remote runs `du` for a minute. "ssh polls the tty" does not say the remote program reads.
- So only `FullscreenWatch` sees it (`terminal_manager.rs:1556-1580`: raw mode, screen unchanged for `STALL_NOTICE_AFTER`
  = 5 s) and shows the notice "This command may be waiting for input -- Take over". That is the designed outcome for an
  unprovable read, and what rounds 8 to 12 logged (`hop=may_be_waiting_notice`; the case reports NOREF).

**The defect found: the notice's "Take over" does nothing.** `show_may_be_waiting_for_input_notice` (`view.rs:12842`)
dispatched `ToggleLongRunningCommandControl`, which acts only when `is_agent_in_control()` (`view.rs:39836-39851`), that
is when a CLI subagent holds a control state. The notice is shown for commands a tool call WAITS on
(`agent_interactive_prompt_candidate` and `fullscreen_stall_fingerprint` both require `is_awaiting_completion`), and
those have no control state for their whole life (`interaction_mode.rs:163-170`). So the click did nothing in every case
the notice is shown for. Typed keys do reach the remote program when its line is prompt-shaped (`key_facts.rs:529-531`),
but nothing says "you have it", focus stays where it was, and a prompt without `:`/`?` sends the keys to the agent input.

**Change (inside the owner rule: a user's act, never a timer).**
- `TerminalAction::TakeOverAgentCommand`: a command a CLI subagent controls is taken over manually, as before; a command a
  tool call waits on gets the `BlockedOnInput` hand-over a proven read makes (`take_over_for_user` admits exactly that
  with no control state, `interaction_mode.rs:505-508`). The conversation stays alive and the tool call gets the
  command's real result. A `Manual` take-over would cancel the turn and is refused there.
- The notice's click dispatches it. A palette entry "Take over the running agent command"
  (`terminal:take_over_agent_command`, no default key) does the same from the keyboard, offered while
  `AgentDrivesRunningCommand` is set.
- appci: `agent_interactive_ssh_takeover` (PROVING): notice, the user's take-over from the palette, the hand-over hop,
  the typed text exact in the REMOTE file and in the model's next request. `agent_interactive_ssh` is unchanged and still
  asks for a hand-over the app makes by itself.

**Not done, needs the owner: an automatic hand-over of a remote read.** Section 5.

## 3. #1967: a stuck turn has no self-healing and no escape

**What "stuck" is.** The fuzzer's oracle states it as I1 (`agent_fuzz/oracle.rs:6`). Three parts of I1 can be decided by
the running app EXACTLY, from its own state, with no clock:

| fault | exact condition | heal |
|---|---|---|
| unsettled calls (#1946 class) | last exchange `Finished/Success`, no stream in flight, a tool call of it has no action-model status and no `ToolCallResult` in the conversation | `cancel_discarded_actions` (the path #1861/#1946 already use): one cancelled result per call. Reason: none, so results go back to the model when another result of the step finished, else the turn ends `Cancelled` and the existing `AgentStopped` toast says "a tool call was cancelled before it returned a result". If the user had already stopped the turn (status `Cancelled`) the reason is the stop's, so no request follows (#1942). |
| parked actions (#1859 class) | a `Done` preprocess batch with no `Pending` batch ahead (`PendingPreprocessedActions::parked_count`); every path that completes or abandons a batch drains these in the same call | `release_ready` + enqueue: the cards surface, the turn goes on |
| blocked question without a card (the "Preparing question..." hang; r18w8 Windows "state blocked, ask actions []") | the action model has an `ask_user_question` blocked at the front of the pane's ACTIVE conversation and no AI block of the pane holds a question card for that id. A question card takes answers exactly while its action is blocked (`ask_user_question_view.rs:930-934`), so "the card exists" is "the user can answer" | the user's Stop, done for them (`cancel_conversation_progress(ManuallyCancelled)`), and an error toast: "The agent asked a question, but its card could not be shown, so the turn was stopped. Send a message to continue." |

**When (the bounded, event-based condition).** No timer. A check is scheduled by the events that end a step: a reply's
stream ended (`AfterStreamFinished`), an action finished (`FinishedAction`), a stop (`cancel_conversation_progress`), and
for the question `ActionBlockedOnUserConfirmation`. It runs as a main-loop hop (a future that is ready at once, so after
the event's own handlers and the events they posted). A fault must be seen UNCHANGED on `CONFIRM_ROUNDS` = 3 consecutive
hops (`turn_invariant::StuckWatch`), then it is healed, once. Every state the lifecycle passes through is held by a
future or a posted event (a stream in flight, a `Pending` batch) and reads as "no fault".

**Kill switch.** `PHOSPHOR_AGENT_TURN_SELF_HEAL=0` (`off`, `false`, `no`): faults are logged, nothing is healed.

**The heal does not hide the defect.** Each confirmed fault logs `hop=agent_turn_stuck fault=<name> healed=<0|1>` at
ERROR. The fuzzer's I6 and the app cases count ERROR lines, so a lifecycle path that loses an action still fails its
test; the user just is not left hanging.

**Guards against healing a healthy turn.**
- A conversation is watched only after a live step in this pane (`turn_watch` entry made at a stream end or a finished
  action), so a restored conversation that was never continued is not judged.
- `suspended_for_close`: a close discards results on purpose (the oracle makes the same exception).
- A revert or an optimistic completion (`CancellationOutcome::Succeeded`) ends the watch: it rewrites the conversation.
- The question check: only the pane's active conversation, only `ask_user_question`, not a view-only pane.

**Rejected.**
- "In progress with nothing held" as a fault. It is also a new empty conversation, a turn waiting for the network, a
  monitored command, and a send parked on an attachment read; I could not tell those apart by reading. Stop already ends
  such a turn (`controller.rs:5527-5581`).
- "Blocked without a card" for every action kind. Command, MCP, file-edit, document and computer-use cards live in
  different maps and some are drawn elsewhere; a wrong "no card" would cancel an approval the user was about to give.
  Only the question, the verified hang, is covered.
- A check on the key path (the user's next key as the confirming event): nothing may be added there without a measurement.
- A "turn stuck, cancel?" notice after N seconds (the issue's second half): a timer; and with the three exact faults
  healed and Stop ending the rest, it would be shown only for the undecidable shape above.
- "Surface a card" for the missing question card: nothing in `AIBlock` rebuilds a question view outside a stream update.

**Risk.** Medium. The controller wiring cannot be unit-tested without the fuzz rig, and nothing was compiled. A false
positive would cancel calls of a live turn; the guards and the three-hop rule are what stand against it, and the kill
switch turns it off without a build. If the next fuzz run shows `hop=agent_turn_stuck` in a scenario that passed before,
that is either a real lost action the 2 s oracle window tolerated, or a false positive of this check: look before
raising the allowance.

## 4. Files not touched

`app/src/terminal/input.rs`, `input/enter_queue.rs`, `view/give_up_to_pty.rs`: untouched. No timer was added
(`script/check_input_path_timers`: ok).

## 5. Owner question (not decided here): hand a remote read over automatically?

Today: no local fact proves a remote read, so the app shows the notice after 5 s of an unchanged raw-mode screen and the
user takes over (now working). If an automatic hand-over is wanted, these are the only signals there are:

- **A. Shape + relay.** Hand over when the tty is raw, the foreground is a known relay (`ssh`, `mosh-client`, `docker
  exec -it`, `kubectl exec -it`) asleep in `poll`, and the cursor line is prompt-shaped (`interactive_prompt_shape`).
  No timer. False hand-overs for `ssh -tt host 'printf "Scanning: "; du ...'`: exactly the #1796 complaint, moved to ssh.
  With no quiet floor a line that ends in `:` for one output burst also triggers it.
- **B. Explicit markers only over a relay.** As A but only for `InteractivePromptShape::Explicit` (`[y/N]`, "Press any
  key", `(yes/no)`) and the ssh client's own prompts. `rpw: ` and `Password:` from a remote program stay with the notice.
- **C. The remote side says so.** In a Phosphorized ssh session the remote shell's hooks (or the helper) can report "the
  foreground reads the tty" as a marker, like the PowerShell `TtyRead` marker (#1882). Exact, but only for managed
  sessions; `ssh -tt host '<command>'` runs no remote shell hooks.
- **D. Keep the notice (today).** One click or one palette entry.

Recommendation: D now, C when the helper work reaches it. A reintroduces the false take-overs the owner removed.
