# Round 12: three test-side owner decisions (2026-11-10)

Branch `fix/r12-tests`, from `integ/r12` 2437a20c4. Nothing here was compiled or run except the
appci self-test (`python3 script/realapp/appci/run.py --self-test`: 176 checks, 0 failed);
Rust is `rustfmt --check` only. Wording: "worked, unproven" until a run shows it.

Decisions (owner, 2026-11-10 morning):

1. Unit tests never reach the real search endpoint (#2022): add a dispatcher test hook.
2. A Ctrl-C into a running agent command interrupts the command and the turn goes on (#850,
   the question left open on #1942): the fuzzer's checks say exactly that.
3. The Windows regression gate judges Ctrl-C on the app's own segments, not on the noisy
   median; a hard fail stays for the app's segment over its budget.

No app behaviour changes. One test-only hook is added to app code paths (1), one test-only
method to the shell command executor (2); both are `cfg(any(test, feature = "test-util"))`.

## 1. #2022: a scripted websearch is answered locally

**Cause** (read from the code, as the issue says). The agent fuzz rig replaces the model's
stream, not the tools the app runs inside the stream. Web access is on by default
(`execution_profiles`), so a scripted `websearch` whose arguments parse or coerce goes
`chat_stream.rs` `dispatch_byop_web_tool` -> `web_runtime::run_websearch(.., None)` ->
`exa::endpoint_url` (`tools/exa.rs:16`), a POST to the real endpoint. Tests: both paced fuzz
tests, matrix `kinds_websearch`, `malformed_websearch`.

**Fix.** `web_runtime::search_test_hook`, asked by the dispatcher after the profile switch and
the argument parse and before the request:

* always on in a unit-test build (`cfg(test)`): no test of the app crate can send a query through
  the dispatcher, installed or not. This is the "never";
* in a `test-util` build that is not a unit test, only after `answer_locally(hold)`;
* not in a release build (neither the module nor the branch).

Second lock: `run_websearch` refuses a call with no endpoint override in a unit test, before the
request is built (every existing test of it passes a mockito URL).

The answer can be held. The rig installs it with 120 ms (`rig::LOCAL_SEARCH_HOLD`): a real search
keeps the reply stream open, and that window is where round 10's I4 failure came from (#2010: a
message sent while the search was out cancels a reply whose calls were never committed). The
oracle's allowance for it (`Oracle::reply_cut`) is unchanged; the window now has a fixed length.
The wait is a sleeping thread and a `futures` oneshot, so it does not depend on which executor
polls the stream.

**What the oracle checks: unchanged.** Web access stays on in the rig, so the cells still go
through the intercepted-tool path and I1-I8 judge them as before. What changes is the tool
result of a search that parses: always the local text (before: a real answer, or a network error
on a box without network). Malformed arguments are still the dispatcher's argument error.

**Rejected.**
* Turning web access off in the rig: stops the request, moves every cell to the "tool not
  offered" refusal and loses the intercepted path (the issue says the same).
* A loopback listener for `endpoint_override`: a socket and a server thread per test process to
  produce what a function can return; and the network gate / SSRF client would have to let
  loopback through.
* A per-test responder that can be uninstalled: tests share a process under `cargo test`, and a
  test that put the real endpoint back would put it back for its neighbours. The hook is
  process-wide and one-way.
* `webfetch`: not touched. The scripted URL is loopback http, refused before any request.

**Risk.** A unit test that wanted the real endpoint through the dispatcher cannot have it; none
exists (`grep run_websearch(`: every call but the dispatcher's passes `Some(url)`). A search cell
now takes 120 ms longer.

## 2. Ctrl-C into a running agent command: the checks

**What the app does** (unchanged; `git show` not needed, this is the fork's own design):

* `TerminalView::ctrl_c` -> `ctrl_c_to_active_block` (`view.rs:10770-10828`): with an agent
  command in flight (`agent_requested_command_in_flight`) the route is `interrupt_agent_command`
  (`view.rs:10001`): the ^C is written to the pty, nothing is cancelled.
* The command dies; the executor reports `Completed { exit_code: 130, interrupted: true }`
  (`shell_command.rs` `was_interrupted_by_ctrl_c`), with NO cancellation reason.
* The action model moves on to the next action of the reply (`action_model.rs:2076-2105`:
  pending actions left -> `try_to_execute_available_actions`).
* The step ends by `plan_finished_batch` (`controller.rs:615`) with reason `None`: a follow-up
  when any result of the step is not cancelled; an interrupted exit 130 counts as cancelled
  (`action_result/mod.rs:776-781`), so a step whose ONLY result is the interrupted command ends
  `Cancelled` with the reason shown and sends nothing (#1178).

**What the fuzzer did.** The rig has no pty and no block, so `press_ctrl_c_for_test` never saw a
command in flight. With an action running it fell through to
`maybe_handle_ctrl_c_in_rich_content_block` -> the status bar's `cancel_active_request_or_action`
-> `cancel_action_with_id(running, ManuallyCancelled)`: the Stop path (a user stop, #1942: no
follow-up, `Cancelled`). The checks then described that path, not the decision:

| check | said | should say |
|---|---|---|
| I2b | a Ctrl-C into a command running ALONE leaves the turn terminal within 2 s | not asked of a Ctrl-C into a running command at all |
| I2b exemption / I2c | any running action (also a read) with later actions waiting | a running agent COMMAND, with or without later actions |
| I2c | "at least one running action stopped" | that, and the action waiting behind the command for the user is still held (the turn goes on) |
| I8a | no line drawn while anything runs (right for a command) | unchanged |
| I8b | every Ctrl-C counted as "the user stopped", excusing a lost message | a Ctrl-C into a running command is not a stop |

**Fix.**
* Rig: a Ctrl-C with an agent command running (`Rig::running_commands`: `Running` stage and the
  shell command executor waits on it) is delivered the way the pty delivers it:
  `ShellCommandExecutor::interrupt_command_for_test` finishes the wait with exit 130,
  `interrupted: true`. Any other Ctrl-C still goes through the view's real handler.
* Runner / oracle: I2b never applies to that Ctrl-C; I2c gets a second half
  (`Oracle::turn_goes_on`, with `oracle::waits_for_the_user`); the Ctrl-C no longer sets the
  run's "stopped" flag (I8b). No check was deleted: `Oracle::interrupted` and `Oracle::stopped`
  are as they were, I2b now covers one case more (a non-command action running with later
  actions waiting, which rounds 10-11 exempted).
* New rig tests `ctrl_c_tests.rs`; new oracle unit test.

**Rejected.**
* Editing only the runner's condition. The checks would then pass on the Stop path the rig took,
  for "stop" and "go on" alike: they would encode nothing about the decision.
* Building a real block in the mock terminal so the view's own routing finds the command: the
  executor would then also need block events for it. Too much unverifiable machinery; the
  routing itself is covered by `view_test.rs`
  `ctrl_c_interrupts_running_agent_command_without_cancelling_conversation`.
* Requiring a request after every such Ctrl-C. For a command that ran alone the app sends
  nothing today (#1178, above). Whether "the turn goes on" should also mean "the model is told at
  once when the interrupted command was the only call" is an app question the decision's wording
  does not settle; it is left to the owner (report). I2c does not judge that ending either way.

**Risk.** The rig's Ctrl-C into a running command now takes a path it never took (a finished,
interrupted result with later actions pending; the end-of-step rule with reason `None`). Any
failure there in the next run is a first look at that path, not a regression. The second half of
I2c judges only a question, or a command with the allowlist off, waiting next in line: actions
that only the user can end. Not modelled: a Ctrl-C after a never-ending command was reported as
stalled (`lrc_alive`; nothing is `Running` then) is still judged as a stop.

**Read on the way, not changed, not observed in a run:** `cancel_action_with_id` cancels the rest
of the turn only in its pending branch (`action_model.rs:1665-1702`); the running branch
(`:1648-1664`) cancels the one action. So the status bar's Ctrl-C on a running NON-command action
(a read, an MCP call) with later actions queued leaves their cards to come up, and the step then
ends `Cancelled` without telling the model. I2b now reports that state if a case reaches it.

## 3. The Ctrl-C regression gate on Windows

**Cause.** `lat.regress` fails at +5 % and +0.5 ms of the end-to-end median. In the Git Bash pane
that median is 88 ms, of which the app's own segments are 0.65 ms (stable to 0.02 ms across
rounds 9 and 10); the rest is the input tool's spawn, ConPTY and bash's trap, and the standard
error of the median is 4.7-7.3 ms (`r11-win.md` section 6). The gate failed on that noise.

**Fix.** `lat.evaluate(app_share=...)`: when the case hands in the app's own share and it is
usable (n >= 30 and >= 90 % of the presses), the previous-build and baseline verdicts compare
that figure, same thresholds, same single re-measure. The median's delta is kept in the record
and reported as NOT JUDGED. The share over 2 ms (`APPCI_CTRLC_APP_SHARE_MAX_MS`) is a FAIL
whatever the basis and whatever n. `ctrl_c_d` passes the share on Windows only
(`cases.ctrl_c_regress_basis`); the overhead budget (20 ms over the reference) is untouched.

The figure is `keydown_to_write` when the presenter's keydown line was logged for >= 90 % of the
presses, else `handler_to_write` (new in `ctrl_c_segments`: `ctrl_c_keydown` -> pty write). Its
name is stored with it and two runs are compared only on the same figure.

**Rejected.**
* Widening the median gate by its standard error: a gate that passes +7 ms because the
  measurement is noisy also passes a real +7 ms.
* Dropping the regression verdict for Ctrl-C on Windows: nothing would then catch the app's
  share doubling under the 2 ms bound.
* Falling back to the median gate when an OLDER record has no share figure: that is the noise
  failure again (round 10 failed against the baseline). Such a record is not compared, a note
  says so, and the hard bound still holds.

**Risk / follow-up.** `baselines/windows.json` has no share figure, so the baseline comparison of
Ctrl-C checks nothing until the baseline is promoted again (`baseline.py promote` now stores
it). The first run compares with the previous run only if that run logged the same figure in
its `ctrl_c_app_segments` metric. The 2 ms bound is the Enter app-share bound reused; the owner
may want another number.
