# r19-integev: what an integration run keeps, self-skips, and the three scenarios of #2122

Branch `fix/r19-integev` from integ/r15 `be5dd47ba`. Nothing was compiled or run when this was
written (`rustfmt`, and the Python self-tests of the two CI scripts): every change is worked,
unproven. Issues: #2122 (this work), #2127, #2128, #2129 (filed here, not worked).

## 1. Cause: why three failures could not be judged

Round 14 (build/r14a, pipeline 3052606915), `integration-linux`:

- The jobs kept no artifacts (`.gitlab-ci.yml`: no `artifacts:` block on `integration-linux` or
  `integration-linux-pwsh`). The scenario's output exists only in the job trace, and only for a
  scenario that failed.
- That output is the app's INFO log. The lines that say where a key went, what was written to
  the pty, which menu opened or what a hover did are on the `warp::input_order` target at debug
  level (`crates/warpui_core/src/input_order.rs`), which the run did not record. For the
  completion menu and the hover x-ray there was no line at any level.
- The driver logs an injected event only when a step carries it directly (`step.rs`,
  "Dispatching event"). A click or hover aimed at a saved position, the `TypedCharacters` that
  follows an unhandled key down, and whether the app handled an event were not logged.
- A scenario whose `set_should_run_test` predicate says no logs "Skipping test ..." and exits 0
  (`driver.rs` `run_test_and_cleanup`); the test shim maps 0 to success
  (`crates/integration/tests/common/mod.rs`), so nextest prints PASS. Round 14: five ssh
  scenarios, 1.50 to 1.76 s each, the time of an app start with no step.

## 2. Chosen fix: evidence

One directory per job, `$CI_PROJECT_DIR/integ-evidence` (`WARP_INTEGRATION_EVIDENCE_DIR`):

| what | who writes it | when it is kept |
|---|---|---|
| `logs/<scenario>.<attempt>.log`: the scenario's whole output, INFO plus `warp::input_order=debug` | the test shim (`tests/common/evidence.rs`), copying the child's stdout and stderr line by line, unbuffered | the scenario did not exit 0 (failed, timed out, killed). The log of a pass is removed at once: that is the size bound |
| `skips/<scenario>.skip`: `INTEG-SKIP <scenario> NEEDS: <reason>` | the driver (`warpui_core/src/integration/evidence.rs` `report_skip`) | always |
| `artifacts/<scenario>/<time>/recording.log`: the action log | the driver; now also written when a step fails (`record_failure_evidence`) | for scenarios with a kept log |
| `SUMMARY.txt`, `DROPPED.txt` | `script/ci/integ_evidence.py finish` in `after_script` | always |

The trace keeps reading as before: the shim passes every line on to its own stdout / stderr
except `[DEBUG]` lines, so a failed scenario still shows its INFO log and the trace stays under
GitLab's limit. Without the variable the shim does what it did (inherited stdio, no filter).

New lines (all without typed text; lengths and names only, #2040):

- `integ_event seq=N step="..." kind=... handled=...` (INFO, driver): every injected event in
  order, with the saved position a click or hover was aimed at and what the app answered.
- `integ_failure_state step="..." terminal[i]: bootstrapped= ... helper_question= menu=[...]
  xray= editor="..." block_command="..." block_output="..."` (ERROR, once, when a step fails;
  `app/src/integration_testing/evidence.rs`). This one shows text: it is what the scenario
  itself typed into a scratch home.
- `hop=suggestions_mode from=[...] to=[...]` (debug): every menu open and close, with the
  trigger and the number of candidates of a completion menu (`suggestions_mode_model.rs`).
- `hop=xray_hover arm|ask|no_ask` and `hop=xray set|cleared` (debug; editor element and view).
- `remote helper: asking the user about <host>; the bootstrap of session ... is held until the
  question is answered` (INFO; `remote_server_controller.rs`). The one state in which a remote
  shell is not bootstrapped because the app waits for a person had no log line.
- pty writes: nothing new. `hop=pty_write_bytes` (class, length) exists; the run now records it.

Bounds: per file 7 MiB (first MiB and the end are kept, with a line saying what was cut; under
the 8 MiB above which `results_scrub.py --tree` leaves a file alone), 150 MiB in all (largest
first, each named in `DROPPED.txt`), 7 days. The artifact is scrubbed like the app jobs' (#1516).

## 3. Chosen fix: a self-skip is a SKIP

nextest has no "skipped at run time" result and the usage suite relies on the binary's exit
codes, so the exit status stays 0. What changes is that the skip is said and counted:

- the driver prints `INTEG-SKIP <scenario> NEEDS: <reason>` and writes the marker file;
- a predicate names its reason with `warpui::integration::skip_because(..)` / `needs(cond, ..)`.
  Named here: the ssh fixture (what is missing), the PowerShell gates of the ssh scenarios and
  `skip_if_powershell_core_2303` (15 scenarios), and the tmux-wrapper flag. A predicate that
  names none gets "its set_should_run_test predicate said no ... no reason given";
- `integ_evidence.py finish` prints one line per skipped scenario at the end of the job and a
  count line (`INTEG-EVIDENCE ... skipped_themselves=N`);
- `collect_results.py` reads those lines from the trace: the scenarios leave "passed" and "run",
  join "skipped", and are listed with their reason in the round's README (the section the usage
  suite's skips already have). nextest's own summary line is kept verbatim next to it.

Which five skipped in round 14, by reading: `test_ssh_into_ash` (no `ash` on the runner: a
legitimate skip) and four gated on `!FeatureFlag::SSHTmuxWrapper.is_enabled()`. That flag is on in
every default build of this fork while the legacy wrapper is what runs, so those four have never
run and never will until the gate changes: #2128. Their skip line says so. The gate is not
changed here (section 5).

## 4. The three scenarios, judged again

| scenario | verdict | evidence by reading | change |
|---|---|---|---|
| `test_ssh_with_shell_override` | TEST | The trace: remote `InitShell`, then the helper check finds no binary, then nothing until the 7 s give-up. `RemoteServerController::on_ssh_init_shell_requested` stashes the bootstrap; with no helper on the host and the default `ssh_extension_install_mode = always_ask`, `on_binary_check_complete` takes `HelperDecision::Ask`: state `AwaitingUserChoice`, the question is shown, the bootstrap stays held until it is answered. No scenario answers it. The real-app harness sets `never_install` for exactly this (docs/testing.md, `ssh_extension_install`: "no case may be stopped by the install question") | every ssh scenario sets `SshExtensionInstallModeSetting` to `never_install` (`without_helper_question`). App: the Ask arm now logs at INFO |
| `test_command_xray_hover` | TEST, probable | The hover arm of `EditorEvent::TryToShowXRay` does nothing while `suggestions_mode_model.is_visible()` (same at the pin). `completions_open_while_typing` defaults to on for local sessions in this fork (pin: off), so typing `git status` leaves the as-you-type menu up. The scenario was written for the pin's default. Not provable from the round 14 trace: whether the menu was up at the hover | the scenario sets `CompletionsOpenWhileTyping` to false (the as-you-type scenarios already set it true explicitly). Product question filed: #2129 |
| `test_autosuggestions_are_hidden_when_opening_tab_completions` | TEST | Two causes, both by reading. (a) The app hides the autosuggestion for a Tab-opened menu only when as-you-type is off (`handle_completion_suggestions_results`: `RemoveAutosuggestionDuringTabCompletions && !is_completions_while_typing_turned_on`), and the fork's default is on. (b) `SuggestionResults::explicit_tab_completion` returns `InsertSingle` for exactly one prefix candidate (same at the pin): no menu. The test home holds one visible directory (`foo`); hidden ones are not offered for an empty word (`engine/path.rs`); on macOS the app's own `~/Library` was a second one | as-you-type set to false; a second directory `bar` (no common prefix with `foo`, so the decision is `Open`) |

No assertion of any scenario changed. Each can still fail: the ssh one if the remote shell does
not bootstrap or is not remote; the hover one if no x-ray shows within 5 s; the Tab one if the
menu does not open, the autosuggestion stays, or it does not come back after Escape.

Also seen, not changed: `test_ssh_with_shell_override` sets the startup shell to zsh and the
trace shows `/bin/bash` started in all 28 scenario starts of the three shards. If zsh is not
installed on the runner the override silently falls back and the scenario does not test what
its name says. Nothing in the trace says which; the next run's log shows the shell line.

## 5. Rejected

- A non-zero exit code for a self-skip: nextest would print FAIL (there is no third result), and
  `crates/usage_suite` treats the binary's exit codes as a contract.
- `RUST_LOG` on the whole job: it would also reach cargo, rustc and every build script. The shim
  sets it for the scenario process only, and a `RUST_LOG` of the caller's own still wins.
- Keeping every scenario's log: about 300 logs per run for nothing; a pass is not evidence.
- `success-output` / junit `store-success-output` in nextest to surface skip lines: it would put
  the INFO log of every passing scenario into the trace or the junit file.
- Logging the editor text or the candidates in the `hop=` lines: lengths and counts only. The
  failure-state line shows text because it is the scenario's own.
- Lines in `app/src/terminal/input.rs` (why a hover's ask was refused: AI input, a visible menu,
  no session context, no description; which branch a Tab took): that file is another task's.
  `hop=suggestions_mode` and `hop=xray` bracket those decisions from outside; the lines are
  listed in the report as wanted.
- Changing the gate of the four ssh scenarios (#2128): they have never run, they drive the ssh
  connect window, and `integration-linux` gates the candidate tier. Turning them on is a
  decision about what a candidate run may fail on, not part of making failures judgeable.
- Stopping the bootstrap give-up timer while the helper question waits (#2127): app behaviour on
  the keyboard path, and whether a question the app asks counts toward a give-up is the owner's.

## 6. Risk

- Nothing was compiled. Doubts are listed in the report.
- The scenarios now run with a debug log target. The `hop=` closures only run when the target is
  on; with it on, each key, pty write and frame-split line costs a formatted line written to a
  pipe. A timing-sensitive scenario could shift. `WARP_INTEGRATION_EVIDENCE_LOG=off` as a
  pipeline variable keeps the logs and turns the target off, to tell.
- The shim's two copy threads end when the scenario's pipes close. A process the scenario leaves
  behind inherits those pipes (the terminal server it spawns, a leaked sshd), where with
  inherited stdio it held nextest's. The shim therefore waits for the copy threads at most 3 s
  after the scenario process itself has exited, then returns the scenario's own exit status and
  writes `INTEG-EVIDENCE the scenario exited but something still holds its output open` into the
  log and the trace. If the terminal server routinely outlives the app, every scenario gets that
  line and 3 s: the first run shows it, and the bound is the thing to shorten.
- The collector now subtracts self-skips from "passed": totals of a round with this change are
  not comparable with round 14's for the integration jobs (five fewer passes, five more skips).
- `skip_if_powershell_core_2303` records a reason as a side effect; it is only ever called as a
  predicate.
