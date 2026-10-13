# Typeahead hand-off: why keystrokes are lost and commands fragment (#826, #827)

Status: design H (the design review's choice; it keeps the shell protocol and the
destructive report) is implemented behind the dark flag
`FeatureFlag::TypeaheadHandoff`, enabled with
`ZAP_UNSTABLE_FEATURES=typeahead_handoff`. It is unverified: not compiled on the
authoring machine and not yet run against the torture repros. See
"Implementation (design H)" at the end. The analysis below describes the code with
the flag off, which is unchanged. Line numbers are from `beee34583`.

The same mechanism is in the oracle pin `4111d08f9`, which has the same
50 ms activation delay, the same `reinitialize_buffer` on block completion, the
same `READLINE_LINE=""` and the same replacing `me.typeahead = data.buffer`.
It is an upstream defect, not a fork regression. The 0.1.11 reproduction is
consistent with that.

## The three destinations a keystroke can take

A key the user types ends up in one of three places, and the place is chosen
again for every key:

| destination | chosen when | code |
|---|---|---|
| **pty** (it becomes shell typeahead) | the model's active block is `started()` | `view.rs:8867` `should_write_typed_chars_to_pty`; `view.rs:8840` `keydown_on_terminal` (long-running branch) |
| **input editor** | the active block is not started (printable chars typed on the terminal view → `view.rs:8913` `input.system_insert`), or the editor has focus | `view.rs:8885` |
| **nowhere** (dropped) | a non-printable key (Enter, Backspace, ctrl-*) reaches the terminal view while the block is not long-running | `view.rs:8846-8856`: the else branch only clears the selection and refocuses. It never forwards the key |

The choice is made against state that changes at three different moments, on
two threads:

1. **T0, model thread.** The shell's precmd/CommandFinished hook is parsed. The
   block is finished (`block.rs:1619` `finish`), a new active block is created
   that is **not started**, and `BlockCompleted` is *sent through a channel*
   (`block.rs:1644`). From this point on, the routing check at `view.rs:8867`
   sends printable keys to the editor.
2. **T1, main thread, some time later.** The view handles
   `ModelEvent::BlockCompleted` (`view.rs:11557`). It calls
   `input.handle_block_completed_event` (`view.rs:11562`), which runs
   `editor.reinitialize_buffer(None)` (`input.rs:13520`) and wipes the editor,
   because the editor still holds the *previous* command's text: the clear is
   deliberately deferred to this point (`input.rs:13504-13508`). Only after that
   does it move focus to the editor (`view.rs:11612`).
3. **T2 = T0' + 50 ms, main thread.** `LineEditorStatus` saw `Precmd` (or zsh
   `EndPrompt`) and armed a 50 ms timer (`line_editor_status.rs:16`, `:81-94`).
   If no `Preexec` arrives in that time, it emits `Active` (`:117`).
   `PtyController` then pushes `ESC i` to the *front* of `pending_writes` and
   drains (`pty_controller.rs:321-334`).
4. **T3, the shell.** The shell reads `ESC i` as a key binding. The bound
   function reports the line editor's *in-memory* buffer in an `InputBuffer` DCS
   hook and **clears that buffer**: bash `bash_body.sh:816-828`
   (`READLINE_LINE=""`), zsh `zsh_body.sh:703-709` (`BUFFER=""`), fish
   `fish.sh:614-619` (`commandline ''`), pwsh `pwsh.ps1:351` (`Alt+1`).
5. **T4, model, then main thread.** `EarlyOutputHandler::input_buffer`
   (`early_output.rs:321`) ignores empty reports (`:322`). A non-empty report
   **replaces** the stored typeahead (`:344`) and sends `Typeahead`. The view
   (`view.rs:9255`) calls `advance_typeahead` (`early_output.rs:173`) and
   `insert_typeahead_text` (`input.rs:7616`), which replaces the first *N*
   characters of the editor, where *N* is the length of the previous report.

Submission: Enter in the editor calls `PtyController::write_command`
(`pty_controller.rs:886`). That marks the new block `started()`
(`:907`), so **from the moment Enter is pressed, keys go to the pty
again**. It also clears `pending_writes` (`:923`) and writes, or queues if the
line editor is not yet `Active` (`:933`), the bytes
`kill_buffer_bytes() + command + execute` (`pty_controller.rs:1215`). The kill
prefix is `^P` (bash, zsh, fish) or `ESC 2` (pwsh)
(`crates/warp_terminal/src/shell/mod.rs:572`). **Whatever is in the shell's
line buffer at that moment is silently destroyed.**

## The windows

The shell report functions read the line editor's own buffer, not the tty, so
the shell side never reads a byte and throws it away. Every loss happens
because the app sends the bytes to the wrong destination, or because a
destination is cleared or overwritten later.

**(c) Editor wipe, W1 = [T0, T1].** Printable keys reach the editor
(`view.rs:8913`) and are appended to the previous command's text. Then
`reinitialize_buffer` at T1 (`input.rs:13520`) erases them. Non-printable keys
in W1 are dropped outright (`view.rs:8846-8856`). An **Enter in W1 does
nothing**.
This is exactly #826's signature, `...UXYZ` with `VW` missing:
`...U` was typed before T0 and went to the pty, so the report returned it. `VW`
fell in W1 and was wiped. `XYZ` was typed after T1 and stayed in the editor.
At T4 the report was prefixed in front of it.

**(a) Sent to the pty, never read back.**
- Bytes written after `ESC i` (Enter pressed in the editor at the end of W2,
  below, puts the block in `started()`, so later keys go to the pty). The
  shell processes `ESC i` first (FIFO), clears the line, and then puts the
  later bytes into a fresh, unreported line. The next `write_command` kills
  them with `^P` (`pty_controller.rs:1215`).
- Typeahead that the shell holds when an editor submission is written before
  `ESC i`. This is possible because `write_command` is written straight away
  once `Active` has fired, and also covers the case where the report is still
  in flight. `^P` destroys it.

**(b) Reported but discarded by the app.**
- A second non-empty report in the same prompt cycle *replaces* the first
  (`early_output.rs:344`). The shell has already cleared the first, so the
  first is lost.
- A report that is inserted into the editor while the editor still holds a
  submitted command's text is wiped by the next `reinitialize_buffer`
  (`input.rs:13520`). Example: the report arrives after `write_command` but
  before that block's `BlockCompleted`.
- `EarlyOutput::precmd` (`early_output.rs:192`) clears the stored typeahead
  and the inserted-count on every precmd. A report that the view has not yet
  inserted when the next precmd is parsed survives only through the
  `Typeahead` event ordering.

**(d) `ESC i` interleaved with user bytes.** `ESC i` is written on a 50 ms
heuristic (`line_editor_status.rs:16`), not when the shell's line editor is
known to be reading. When the shell is executing a command that it took from
typeahead (`echo N >> ~/burst` with Enter already in the tty queue), `Preexec`
can arrive later than 50 ms. `ESC i` then lands in the tty in cooked mode,
between user bytes. The tty echoes it, with `ECHOCTL`, as `^[i`, and the kill
prefix as `^P`. `EarlyOutput::preexec` (`early_output.rs:206-225`) copies the
background block, echo included, into the next block's command grid. That is
where `^[iecho 48 >> ~/burst` and `^[i^P0` in #827 come from. A command that
reads stdin at that moment receives `ESC i` as real input.

**W2 = [T1, T4], Enter pressed during the hand-off.** After T1 the editor has
focus, but the typeahead report has not arrived. If T2 has not fired, it has
not even been requested. Enter submits *only the editor's part* of the line
(`write_command`). Then:
- if `Active` has not fired, the command waits in `pending_writes`. When
  `Active` fires, `ESC i` is pushed in front of it (`pty_controller.rs:330`).
  The shell reports and clears the typeahead prefix, then runs the editor
  fragment. That gives `rst`, `ho`, `3 >> ~/burst` as commands. The report
  is then inserted into an editor that is wiped at the next block completion,
  or that survives into the next command (`echo 47echo 48 >> ~/burst`);
- if `Active` has already fired, the command is written at once, and `^P`
  kills the unreported typeahead (window a).
- Keys typed after that Enter go to the pty (the block is `started()`). They
  become the next typeahead, and the cycle repeats with every command of a
  burst.

## Why this is not a local fix

Each destination is correct for some sub-interval. The problem is that the
interval boundaries are decided by three unsynchronized signals: model-thread
`started()`, view-thread `BlockCompleted`, and a 50 ms timer. On top of that,
the report protocol is destructive, because it clears the shell line. That
means a key typed after `ESC i` can never safely go back to the pty. A narrow
patch for any one window is not enough:
- routing W1 keys to the pty fixes the #826 repro, but it leaves W2. #827 is
  mostly W2;
- deferring Enter in the editor while a report is outstanding needs the keys
  typed after that Enter to be held too, or they land in the deferred command;
- re-sending `ESC i` when bytes followed it executes the fragment typed after
  the first request, if that fragment contains Enter.

## Design options

**Option 1: the shell stays the only authority until a quiet hand-off
(recommended).**
- Every key goes to the pty, which includes Enter, Backspace and ctrl-keys,
  from command start until the hand-off *commits*. Focus stays on the terminal
  view until then; the `redetermine_terminal_focus` call at `view.rs:11612`
  is deferred, and `keydown_on_terminal` stops dropping keys.
- `ESC i` becomes a non-destructive **peek**. Remove the clearing line in all
  four shells. `PtyController` records, for each request, how many user bytes
  had been written when it was sent, in a FIFO matched against reports.
- A report *commits* only when no user byte was written after its request.
  On the main thread, in one turn, the app switches routing to the editor,
  inserts the reported text, and writes `kill_buffer_bytes()` so the shell
  line is emptied. Because nothing was written after the request, the line
  equals the report exactly.
- A non-quiet report is ignored. If no line terminator was written since the
  request, another peek is sent. If one was, the shell will run that line
  itself (`Preexec`), and the next precmd starts a new cycle.
- Enter-separated typeahead then runs exactly once, in order, by the shell.
  This is the path Warp already uses for commands typed during a running
  command, with the command grid repaired by `EarlyOutput::preexec`.

Risks:
- If the shell never answers (an un-warpified shell, a non-readline state, an
  old bash in `InputMatching` mode, a stuck precmd as in F4), keys would flow
  into an invisible shell line. This **needs a commit timeout** (about 0.5-1 s)
  and an immediate commit when the mode is not `ShellReported`.
- Request/report FIFO matching breaks if a request is consumed by a program.
  The direction of that error is conservative: the report is judged
  "not quiet".
- The `kill_buffer_bytes` write redraws the prompt, so a background block may
  show the killed text briefly. `block.rs:186` describes the existing
  suppression delay.
- This changes the shell protocol in bash, zsh, fish and pwsh.
- The focus change is visible for one round-trip.

**Option 2: hold and replay into the editor.**
- Keep the destructive report.
- From T0 until the report is applied, put every key in an app-side queue
  instead of sending it anywhere.
- After the report is inserted into the freshly reinitialized editor, replay
  the queue *into the editor*. A replayed Enter submits, and the keys after it
  are replayed only after that submission's `BlockCompleted`.

Risks:
- Replay needs real keystroke dispatch into the editor (keybindings, IME,
  completions menu, vim mode). `characters` byte strings do not carry that.
- If `ESC i` is never answered, the queue holds the keys forever. This also
  needs a timeout.
- It changes no shell code. It is the larger app-side change.

**Option 3: no typeahead hand-off at all.**
- Stop routing keys to the pty when a command does not read stdin. That is
  not knowable, so this would break typing into interactive programs. Rejected.

## Tests the fix must carry

Rust unit tests on the hand-off state machine (`early_output_tests.rs`,
`pty_controller_tests.rs`, `view_test.rs` all already have typeahead and
`WriteBytesToPty` harnesses):
- keys during T0..commit are delivered in order, with none dropped and none
  wiped by `reinitialize_buffer`;
- `a\rb\r` typed across the hand-off produces exactly two submissions, `a`
  then `b`;
- `ESC i` / the kill prefix are never written while user bytes written after
  the last request are outstanding;
- a report that is not quiet is not committed;
- the commit timeout releases the editor when no report arrives.

Then re-run the tester's `ta.sh` and `bburst.sh` (torture round 2026-10-12)
on bash, zsh and fish. The pass criterion is 50/50 lines and 5/5 uncorrupted.

## Implementation (design H)

`app/src/terminal/model/typeahead_handoff.rs` is a pure state machine owned by
`TerminalModel` (so every routing and request decision is made under the model
lock). It engages only for commands submitted from the input editor
(`CommandExecutionSource::User`), and only in shells that report their line
(zsh, bash >= 4, fish, pwsh; not macOS's bash 3.2, which keeps the old routing
because nothing could hand keys left in its line back to the editor). Agent,
shared-session, in-band and queued-prompt-panel commands keep the old routing.
`ZAP_UNSTABLE_FEATURES=all` does not enable it; it must be named.

| state | entered on | keys typed on the terminal | focus |
|---|---|---|---|
| `EditorOwned` | start; a commit; a report | old routing (flag-off predicates) | old rules |
| `PtyOwned { wrote_user_bytes, enters_written, precmds_seen }` | `write_command` accepted; a preexec for a block nobody started | every key class to the pty, counted (`\r`/`\n` = Enter; not counted as shell lines on the alternate screen) | terminal |
| `AwaitingReport` | `ESC i` written | old routing (the editor has focus) | old rules (editor) |
| `HoldAfterEnter { held, released }` | Enter in the editor while `AwaitingReport` | held, in order | terminal |

Transitions out of `PtyOwned`:
- precmd (main thread, after `BlockCompleted` reinitialized the editor) with no
  user bytes in the shell's line and no queued line: back to `EditorOwned`, no
  request.
- `LineEditorStatus::Active`: request only if `precmds_seen >= enters_written`,
  the lifecycle is `AtPrompt` with nothing started, and no counted user write is
  in flight; otherwise wait, hold `PtyController`'s queued writes (an in-band
  command's kill prefix would destroy the unreported line), and retry when the
  in-flight count reaches zero or after 250 ms with no precmd/preexec (stale
  counts are then reset). With nothing in the shell's line but an Enter still
  unanswered, queued writes drain and the same 250 ms quiet period resets the
  counts and decides again: an Enter consumed by a program (`read`, a password
  prompt, a subshell's `exit`) is answered by no prompt.
- Ctrl-C: written to the pty; a running command's queued lines die with it, so
  the counts reset; at the prompt the editor takes over after 250 ms or the next
  precmd, whichever comes first. If the user typed after the Ctrl-C and no prompt
  cycle came (zsh), that timer asks `PtyController` to re-run the request
  decision, since no line-editor activation will.
- A command the hand-off does not own (a queued-prompt-panel, agent or in-band
  command) written while the pty owns input still has its Enter counted, so its
  precmd cannot make lines the user queued ahead of it look answered. If the
  shell's line may hold unreported user bytes, it is queued until the request,
  which is written ahead of it, instead of killing them; its block is started and
  its Enter counted only when it is written, so the prompt still reads as a
  prompt meanwhile. Ctrl-C discards the shell's line and so releases it (it goes
  out behind the Ctrl-C, and an agent still gets its command run).
  Only a command whose start would be accepted right now (at the prompt, nothing
  started) is held; one arriving while a block is started or running takes the
  normal path and is coalesced or rejected exactly as without the hand-off. One
  command is held at a time: another non-editor command meanwhile is coalesced.
  A command from the editor meanwhile goes first and the held one starts at the
  prompt after it. If the shell exits, a held command is dropped (with a log
  line; commands carry no completion channel, so an agent that sent it gets no
  result).
- With #848/#850: a discard of parked commands (tab close, an agent interrupt)
  drops a held command like any other parked command and forgets it. A held
  agent command records its action, so cancelling that action discards it even
  though no block holds it yet. An agent interrupt (ctrl-c on an agent command in
  flight, a conversation stop) cancels a deferred Enter and its held keys first,
  so its single ^C is written rather than held.
- A command that starts its own block when written (an in-band or held command)
  waits while an Enter the user typed is unanswered by a prompt, so the output of
  the user's lines does not land in its block.
- A precmd for a prompt that ran no command (an empty Enter, a syntax error, a
  prompt cycle after Ctrl-C) brings no line-editor activation, so the view asks
  the controller to decide again, whether the editor took over at that precmd
  (draining writes held for this prompt) or the pty still owns input.
- A second non-editor command while one is held is coalesced and the surface
  shows the same "Cannot run `...` (command already running)." toast the input
  shows for a running command. A held command's cloud workflow is recorded on the
  block it starts, when it starts. Every Enter typed also counts as activity for the quiet period,
  and a flushed Enter re-decides (re-arming it).
- Ctrl-C while keys are held behind a deferred Enter cancels the deferred Enter
  and drops the held keys; nothing is written to the pty.
- Ctrl-D goes to the pty only when something reads it (a command running or
  queued, or bytes in the shell's line); EOT on an empty shell line would exit
  the shell.

Report handling: inserted at the front of the editor (the user's cursor stays
put relative to their own text) and the editor keeps input. Any Enter while it is
outstanding is deferred, before menus or natural-language detection see the
buffer (which holds only what was typed after the request); the report runs it,
then the held keys are written after the command as its typeahead. If no report
arrives within 1 s the deferred Enter is not run (it would submit a fragment): the
held keys go into the editor and the late report is inserted at its front when it
comes. The deadline runs from when the view handles the request
(`PtyControllerEvent::TypeaheadHandoffChanged`), not from the Enter. A report that
arrives after the next command was written is stashed and inserted when the editor
next owns input; a deferred Enter that wrote no command hands the held keys back
too. Keys handed back are applied the way the shell's line editor would have:
Backspace/^H, ^U and ^W edit, Enter becomes a newline and Tab stays (so a
multi-line paste survives), well-formed CSI/SS3/OSC sequences are dropped whole,
other control characters are dropped, and a lone ESC does not take the next
character with it.

Known gaps, not addressed:
- keybindings gated on the `LongRunningCommand` context (e.g. shift-enter) are
  inactive while the pty owns input but the command is not yet long-running;
- held keys skip `emit_non_editor_typed_event`, so synced panes and shared-session
  viewers do not see them; synced follower panes write without counting;
- bash's prompt-mode switch (`\ep`/`\ew` plus an empty command) writes a `\r` the
  counts do not see (covered by the quiet-period reset);
- a request after the quiet period can still land in cooked mode if the shell was
  merely slow (no prompt within 250 ms of the last hook).

Gated model fixes (flag on only): additive reports, precmd keeps a shell-reported
report the editor has not inserted (input-matched typeahead, which can hold a
program's stdin echo, is still cleared), and an empty report still notifies the
hand-off.
Ungated shell fix: `\ei` (and bash's `\C-p`) bound in the vi command keymaps,
recorded under `IMPROVED` in `DECLINED.md`.

Residual risks, to check with the torture repros (`ta.sh`, `bburst.sh` at delays 3
and 12, on bash, zsh and fish, plus `ta.sh` variants with Backspace and Enter
across the boundary):
- focus moves to the terminal for every editor command and back at the prompt;
  watch the queued-prompt draft, agent view, CLI-agent rich input, the find bar
  and synced inputs (synced panes still use the long-running predicate);
- Ctrl-C at the prompt may run a prompt cycle (bash, zsh) that arrives after the
  250 ms settle and reinitializes the editor under the user;
- text typed into the shell's line between the prompt and the request is echoed
  by the shell and can show briefly as background output;
- `\ei` makes ESC a key prefix in the shells' vi keymaps (keyseq timeout on a bare
  ESC typed into the shell's own line editor);
- a request still waits on stale counts for 250 ms after a line the shell never
  answers with a prompt (a program reading stdin, a continuation line, a
  bracketed paste with newlines);
- if the line editor became active before the state became requestable and no
  later activation or counted write arrives, the pty keeps input until the next
  Enter;
- IME composition on the terminal is not routed by the hand-off.
