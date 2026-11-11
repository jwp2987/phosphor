# r19-integfix: the follow-ups of r19-integev (#2127, #2128, #2133)

Branch `fix/r19-integfix` from integ/r15 `5a9529bf3`. Nothing was compiled or run when this was
written (`rustfmt --check`, the guards, the Python self-tests): every change is worked, unproven.

## A. Four ssh scenarios that never ran (#2128): TEST fault, the gate

**Cause.** `crates/integration/src/test/ssh.rs`: `test_legacy_ssh_into_bash`, `test_legacy_ssh_into_zsh`,
`test_ssh_remote_bash_bootstrapped_hook` and `test_ssh_typeahead_during_connect_runs_remotely_once`
were gated on `!FeatureFlag::SSHTmuxWrapper.is_enabled()`. That is upstream's gate, where the flag
chose the tmux wrapper over the legacy one. Here:

- the flag is on in every default build (`ssh_tmux_wrapper` in `app/Cargo.toml` `default`) because
  it also gates tmux control-mode handling (`DECLINED.md`, "SSH tmux wrapper option -- REMOVED
  2026-11-08": "`FeatureFlag::SSHTmuxWrapper` stays ON");
- the tmux ssh-wrapper option itself is gone: `evaluate_warpify_ssh_host`
  (`app/src/terminal/ssh/ssh_detection.rs`) answers `FeatureDisabled` for every host;
- the wrapper a typed `ssh` runs is the legacy (ControlMaster) one. With the flag on,
  `terminal_manager.rs` takes `enable_ssh_wrapper` from `warpify.ssh.enable_ssh_warpification`
  (default true); round 14's `test_ssh_with_shell_override`, which has no flag gate, ran it in the
  same build (`ManagedSSHSessionStarting`, `SSH`, `InitShell`).

So the predicate was false on every machine.

**Fix.** The gate asks the fact that decides which wrapper runs, not the flag:
`ssh_detection::TMUX_SSH_WRAPPER_TAKES_SSH` (a `const false` next to the function it describes, with
the reason) read through `integration_testing::subshell::util::legacy_ssh_wrapper_takes_ssh()`. The
four scenarios need it true, the two tmux scenarios (not in the run list) need it false and say why
they cannot run. The gate is kept, not deleted: if the tmux option ever returns, one constant
changes and both sets of scenarios follow. A unit test pins the fact.

**Read for what will obviously fail on a first run** (they have never run):

- Helper question: all four already set `never_install` (`without_helper_question`, r19-integev).
- The connect, the passphrase prompt and the password step of the two `legacy` scenarios are the
  very steps round 14's `test_ssh_with_shell_override` passed (its override was not applied, D
  below, so it WAS `test_legacy_ssh_into_bash` up to the helper question). What comes after the
  remote bootstrap has never been seen: the remote user / host check, the login-shell check, the
  MotD assertion.
- `wait_until_bootstrapped_single_pane_for_tab` needs the input box visible, which it is not while
  the ssh block runs: the wait is a real wait for the remote prompt, not a stale pass.
- **MotD.** `assert_motd_shown` looked for the text `Welcome to Ubuntu`. That text is the banner of
  the fixture's `ForceCommand` script, which prints it only for a session WITHOUT a remote
  command. A session through the wrapper has one, so what is printed is the MotD file of the
  machine the scenario runs on (the "remote" is that machine): by the remote bootstrap for bash and
  zsh (`bash_body.sh`, "Before calling rcfiles, print the MotD"), by the remote command for sh /
  ash. r18-bootsub met the same fact in a unit test. The old assertion therefore held only where
  the machine's own MotD starts with those words (a stock Ubuntu, which the CI box is said to
  be), and could not pass on a machine with another MotD or none. It now looks for the start
  (40 characters) of the first plain line of this machine's MotD file (first readable of the
  bootstrap's own list), or the fixture's banner. On a stock Ubuntu that is the same line as
  before. On a machine with no MotD nothing is printed: the assertion logs that it has nothing to
  look for and holds (before: unpassable there). `test_ssh_into_sh` / `_ash` use the same
  assertion and get the same change.
- Not judged by reading, the first run decides: `assert_active_block_is_remote` returns
  `PreconditionFailed` when the active block has no session yet; the typeahead scenario types
  while the `ssh` line may still be in the editor (that IS the case it tests; a local run of the
  typed line is an APP verdict).

**Rejected.** Deleting the flag check (the task forbids it, and the scenarios do depend on which
wrapper runs). `FeatureFlag::SSHTmuxWrapper.is_enabled() || ...` style gates: the flag does not
decide the wrapper here. Reading `enable_ssh_warpification` in the predicate: the predicate has no
app context, and the scenario's own home has the default.

## B. The bootstrap give-up judged a wait the app caused (#2127): APP

**Cause.** `TerminalView`, `ModelEvent::SSH(shell)` arms `start_bootstrap_timer(BOOTSTRAP_FAILED_DURATION)`
(7 s) for a bash / zsh remote. With no helper on the host and `always_ask`,
`RemoteServerController::on_binary_check_complete` (`HelperDecision::Ask`) goes to
`AwaitingUserChoice`, shows the question (`ModelEvent::RemoteServerBlockRequested` ->
`show_ssh_remote_server_choice_block`) and writes the bootstrap only when it is answered
(`handle_ssh_remote_server_skip` / `_install`). Nothing touched the timer: 7 s after the hook
`on_bootstrap_failed_timer_complete` ran on a session whose bootstrap had not been written.

**What the user sees after answering late, old code (read, not run).** At 7 s, question still up:

1. `Bootstrapping failed for shell "bash" on ssh false` (the `false` is `has_pending_ssh_session`,
   which `InitShell` already consumed: a wrong word in the log, not a second defect).
2. The session is marked failed (`ssh_bootstrap_failed_session`) and the ssh phase gets
   `BootstrapFailed`: the app stops holding keys for this session. An interrupt key is no longer
   held for the bootstrap (`ssh_bootstrap_interrupt_route` -> `Forward`).
3. A line typed during the connect and held (`ssh_typeahead_holding`) is WRITTEN TO THE PTY
   (`hand_held_ssh_typeahead_to_unbootstrapped_remote`). The remote bash is at its raw,
   unbootstrapped prompt (`stty raw`, `InitShell` sent, waiting for the bootstrap), so that line
   runs there, before the bootstrap, and the toast says "SSH setup didn't finish; what you typed
   was sent to the remote as typed. The remote terminal may be in raw mode: run `stty sane`
   there" while the setup is only waiting for the user. This is the rule "held keys must never
   reach the bootstrap payload" (#1641) broken by a clock.
4. The hidden ssh block is unhidden (raw output of the half-set-up session), `Event::SlowBootstrap`
   goes to the agent driver, and a first-session pane gets the slow-bootstrap banner.
5. The user answers. The bootstrap is written and the remote shell bootstraps normally; the
   session ends up usable. But it stays marked failed until the next session, keys typed between
   the answer and `Bootstrapped` are no longer held (they can land in the bootstrap being fed),
   and whatever step 3 ran has already run in the wrong place.

**New code.** Showing the question takes the give-up off the clock; the question leaving the
screen arms it afresh:

- `show_ssh_remote_server_choice_block` -> `pause_bootstrap_give_up_for_helper_question`: when the
  question is about the pending session and that session was not already judged
  (`helper_question_pauses_bootstrap_give_up`, pure), the armed timer is superseded
  (`cancel_bootstrap_failed_timer`, the #1566 generation) and the session is remembered in
  `bootstrap_give_up_paused_for_helper_question`.
- `remove_ssh_remote_server_choice_block` (the Install and Skip handlers come through it before
  they tell the controller; also a session that was deregistered) and the clear of all rich
  content -> `resume_bootstrap_give_up_after_helper_question`: the pause is taken and
  `start_bootstrap_timer(BOOTSTRAP_FAILED_DURATION)` runs, if the session is still the pending one.
  Every way the question can leave goes through one of the two, so a pause cannot outlive it.
- `start_bootstrap_timer` arms nothing while the pause is set and the question is showing.

Same late answer, new code: at 7 s nothing happens (one INFO line when the question was shown).
The held line stays held, the block stays hidden, no toast, no banner. At the answer the bootstrap
is written and the 7 s net starts; `Bootstrapped` and the first prompt deliver the held line to
the bootstrapped shell, as in a session that never asked. If the bootstrap then never completes,
the net judges it 7 s after the answer exactly as before.

**No new timer.** The existing call site and constant are used; `check_input_path_timers` passes
unchanged. The event is the answer.

**Kill switch.** `PHOSPHOR_BOOTSTRAP_GIVE_UP_PAUSED_BY_HELPER_QUESTION=0` (read once per pane): the
clock runs through the question as before.

**Tests.** `view_test.rs`: `bootstrap_give_up_does_not_judge_a_session_waiting_for_the_helper_answer`
(fails on the old code: the expiry under the question marks the session failed),
`bootstrap_give_up_runs_through_the_helper_question_with_the_kill_switch` (the control: the same
scenario does show the failure with the switch off), `helper_question_pause_rule`.

**Rejected.** Checking "is the question showing" when the timer fires and re-arming a short timer:
a poll. Not arming at the `SSH` hook at all: the net must cover a helper check that never answers.
Moving the arm into `RemoteServerController::flush_stashed_bootstrap`: right in principle (the net
would measure from the write on every path, install included) but it moves the net for every ssh
session, not only the asked one; too wide for this change.

**Risk, and two things left for the owner.**

- While the question is up an interrupt key stays on `SshBootstrapInterruptRoute::Hold` for as
  long as the question is (before: `Forward` after 7 s). Typed keys go to the card and show its
  hint, as before. Whether Ctrl-C at the question should end the ssh is a key-path decision and is
  not made here.
- "Install": the clock starts at the answer and the install runs inside the 7 s, exactly as it
  already does for a host whose stored policy is `allow` (no question). A slow install is judged
  failed while the app itself installs. Same class of defect as #2127, not changed here.
- The pause is view state keyed by the session; if a path removed the question's view without
  going through the two functions above, the net would stay off for that session. Read: there is
  no third path (`rich_content_views` is only otherwise retained by AI-block filters).

## C. Two log lines in `app/src/terminal/input.rs` (log only)

- `input_order hop=xray_ask_refused reason=ai_input | trigger=hover reason=visible_menu |
  trigger=hover reason=no_text_at_point | trigger=.. reason=no_session_context |
  trigger=.. reason=no_description`: where an ask for the x-ray goes no further
  (`EditorEvent::TryToShowXRay`, `start_xray_at_offset`, `show_xray`).
- `input_order hop=input_tab branch=ai_context_menu_accept | menu_insert_common_prefix prefix_len=
  word_len= single= | menu_select_next | workflow_enum_select_next | open_completions |
  to_editor tab_opens_completions=`: the branch a Tab took in `Input::input_tab`.

Debug level on the `warp::input_order` target, built only when the target is on. Lengths and flags,
never typed text (#2040). No condition was reordered and none is evaluated twice: each line sits
in a branch that already existed or in a new `else` that does nothing else.

## D. `test_ssh_with_shell_override` never ran zsh (#2133): APP, plus a scenario that could not tell

**Cause (read, not run).** The scenario sets the legacy key `StartupShellOverride` to
`StartupShell::Zsh`. `AvailableShells::get_user_preferred_shell_setting` uses the new key when set
and otherwise `get_user_preferred_shell_setting_fallback`, which looked the name `zsh` up in
`self.scanned().shells`. The scan runs in the background since #1310 / #1364 and `scanned()` is an
empty stand-in until it completes, so for the first tab the lookup found nothing and the answer
was `NewSessionShell::SystemDefault`: `/bin/bash` on the runner, in every start. #1310 closed this
hole for the new key (`resolve_preferred_directly`) and not for the legacy fallback. The scenario
sets the key correctly; the app did not apply it.

**Fix (app).** `legacy_startup_shell_preference` (pure): a finished scan decides as before; with no
finished scan the one name is resolved directly (`supported_shell_path_and_type`, one PATH lookup)
and becomes `NewSessionShell::Executable(path)`, which `shell_for_preference` already resolves
without the scan. One INFO line says which way it went. Unit tests:
`legacy_startup_shell_name_is_resolved_while_the_scan_is_unfinished`,
`legacy_startup_shell_name_uses_the_finished_scan_as_before`.

**Fix (scenario).** It now needs zsh on PATH (`INTEG-SKIP ... NEEDS: zsh installed`), and a new
second step runs `test -n "$ZSH_VERSION"` in the pane before any ssh: the scenario can no longer
pass or fail on a shell it did not ask for. A new assertion; none was removed.

**Rejected.** Waiting for the scan before the first tab (#1310 exists to avoid that). Setting the
new key in the scenario instead: it would hide the app defect, and the scenario's subject is the
legacy override.

## E. Every `set_should_run_test` predicate names what it needs

43 predicates in `crates/integration/src/test*.rs` returned a bare bool. Each is now wrapped in
`warpui::integration::needs(<the same expression>, "<what the scenario needs>")` (shell, platform,
feature flag), so no `INTEG-SKIP` line says "no reason given". No condition changed. One finding
while naming them: `test_open_launch_config_from_add_tab_menu_legacy` is gated on
`!FeatureFlag::ShellSelector.is_enabled()`, and that flag is on in every default build: a legacy
scenario that never runs, by its own TODO (CORE-2300). Its skip line says so; not changed.

## Risk

Nothing was compiled; doubts are in the report. The four ssh scenarios and the shell-override
scenario will run their later steps for the first time: each failure needs a TEST-vs-APP verdict
from the kept log.
