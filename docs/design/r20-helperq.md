# r20-helperq: the remote-helper question and install (#2134, #2135), one scenario removed, the fixture banner

Branch `fix/r20-helperq` from integ/r15 `cf9aa87db`. Four owner decisions of 2026-11-11 on the open
points of `docs/design/r19-integfix.md`. Nothing was compiled or run when this was written
(`rustfmt --check`, the guards): every change is worked, unproven.

## 1. Ctrl-C at the remote-helper question ends the ssh (#2134): APP

**Cause.** With the question up, the remote session is pending and not bootstrapped, so
`TerminalView::ssh_bootstrap_interrupt_route` answers `Hold` (`app/src/terminal/view.rs`, the pure
rule `ssh_bootstrap_interrupt_route(failed)`). Both paths of the key use it:
`ctrl_c_to_active_block` (the cancel binding: "ssh_held", toast) and row P1a of the table
(`input_queue/table.rs` `decide_signal`: `ssh_interrupt_must_wait` -> `Outcome::HoldSignal`,
executed in `view/dest_table.rs` `execute_arrival_signal`). `Hold` exists so an interrupt does not
cut a bootstrap script that is being fed. At the question no script is being fed: the controller
holds it (`SshInitState::AwaitingUserChoice`). Since #2127 nothing ends the `Hold` but an answer.

**Fix.** One pure rule, `helper_question_interrupt_ends_ssh(enabled, route, question_session,
pending_session)`: the switch is on, an ssh-style bootstrap is pending (`route` is `Some`), and
the question on screen is about the pending session. Both paths ask it before their hold
(`helper_question_ended_by_interrupt`) and then run `end_ssh_at_helper_question`:

1. what was typed during the connect and is still held is void (`reset_ssh_typeahead`, the rule
   of row P1b: the tty flushes its input on an interrupt);
2. the question is removed (`remove_ssh_remote_server_choice_block`, which arms the give-up afresh,
   #2127: the net for an interrupt that does not end the ssh);
3. `Event::RemoteServerQuestionAbandoned` -> `RemoteServerController::
   handle_ssh_remote_server_question_abandoned`: `AwaitingUserChoice` -> `Idle`, the stash dropped,
   not written;
4. `Event::SigintForegroundToPty`: SIGINT to the pty's foreground group, the ssh client.

**Why SIGINT and not the byte.** After login the ssh client has the local tty in raw mode: a ^C
byte is data, forwarded to the remote shell, which sits at its raw, unbootstrapped prompt and ends
nothing. SIGINT ends the client in raw and cooked mode; it is the route #1001 already uses for a
Ctrl-C during the connect (`sigint_foreground_or_write_etx`). `~.` (the pre-InitShell abort) needs
the client's escape state at a line start, which is not known here.

**Why the stash must be dropped.** `on_ssh_init_shell_requested` releases the stash of a previous
unfinished init by WRITING it (`flush_stashed_bootstrap(old_info)`), and `debug_assert!`s that the
state is `Idle`. An abandoned question would therefore have its bootstrap written into the next
ssh of the pane. Until now that needed a connection lost under the question; with Ctrl-C an
ordinary way out it has to be closed.

**Rejected.** A new `SshBootstrapInterruptRoute` variant: the route also feeds Ctrl-D
(`ssh_connect_holds_eot`), `ssh_interrupt_must_wait` and the agent write gate, none of which this
decision changes. A new `RouteFacts` field and table row: the table's enumeration tests build the
facts positionally and nothing can be compiled here; the view-side check sits in the one executor
of `HoldSignal` and uses the same rule as the binding path. Writing ETX (old `Forward`): does not
end the ssh. Sharing #2127's kill switch: that switch turns a clock rule off; turning the key
rule off with it would bring #2127 back for whoever only wants the old key behaviour.

**Kill switch.** `PHOSPHOR_HELPER_QUESTION_INTERRUPT_ENDS_SSH=0` (read once per pane).

**Risk.** Ctrl-D at the question is unchanged (held). On a platform without a foreground-group
SIGINT the existing fallback writes ETX, which does not end the ssh; the give-up armed in step 2
then judges the session after 7 s and the route becomes `Forward`, as before #2127. Removing the
question locks the model on the key path (as its other callers do off the key path); the remote
is silent at that point.

## 2. The give-up ran while the app installed the helper (#2135): APP

**Cause.** After "Install" (`handle_ssh_remote_server_install`), a stored `allow`
(`HelperDecision::Install`) or an old helper (`Ok(false) if has_old_binary`) the controller sits
in `AwaitingInstall` with the bootstrap unwritten until `BinaryInstallComplete`. The give-up armed
at the `SSH` hook, or at the answer (#2127), runs meanwhile: an upload longer than 7 s is judged a
failed bootstrap (`on_bootstrap_failed_timer_complete`, the effects listed in r19-integfix B).

**Fix.** The view already receives the manager's events for its sessions:

- `SetupStateChanged { Installing | Updating }` -> `on_remote_helper_setup_state`: by the rule of
  #2127 (`helper_question_pauses_bootstrap_give_up`: switch on, the pending session, not already
  judged) the armed give-up is superseded and the session remembered in
  `bootstrap_give_up_paused_for_helper_install`. Progress updates find the pause set.
- `BinaryInstallComplete` (either result) -> `on_remote_helper_install_finished`: the pause is
  taken and the give-up armed for the full 7 s, if the session is still the pending one.
- `start_bootstrap_timer` arms nothing while the pause is set for the pending session; a pause
  left by a session that is gone is dropped there.

**An install that never reports.** `install_binary` is bounded by its own timeouts
(`INSTALL_TIMEOUT`, `SCP_INSTALL_TIMEOUT`, `CHECK_TIMEOUT` in `ssh_transport.rs`); a timeout is an
`Err`, reported as `BinaryInstallComplete`, which arms the give-up and makes the controller write
the bootstrap. No timer was added; `check_input_path_timers` is unchanged.

**Why armed at the install's end and not at the bootstrap write.** After a successful install the
controller still connects before it writes (`AwaitingConnect { bootstrap_flushed: false }`). That
connect has no bound of its own that was found by reading, so the net has to cover it. A connect
slower than 7 s after an install is still judged; not changed here.

**Kill switch.** The one of #2127 (`PHOSPHOR_BOOTSTRAP_GIVE_UP_PAUSED_BY_HELPER_QUESTION=0`): same
rule, same clock.

## 3. `test_open_launch_config_from_add_tab_menu_legacy` deleted: TEST

Gated on `!FeatureFlag::ShellSelector.is_enabled()`; the flag is on in every build of this fork
and the scenario's own TODO (CORE-2300) says to remove it with the flag's legacy side. Removed
from `launch_configs.rs`, `integration_tests!` and `register_tests()`, with the two imports only
it used. `test_launch_config.yaml`, `NEW_TAB_BUTTON_POSITION_ID` and
`with_right_click_on_saved_position` have other users and stay.

## 4. The fixture banner for wrapped sessions: TEST

The local sshd fixture's `ForceCommand` script printed `FIXTURE_MOTD` only for a session without
a remote command; a wrapped session has one, so on a machine without a MotD file nothing was
printed and `assert_motd_shown` held with nothing looked for. The script now prints the banner
for every session with a terminal (`[ -t 1 ]`; the wrapper runs `ssh -t`) before it starts the
remote command. A remote command without a terminal gets none: those are the app's multiplexed
commands and scp, whose output is parsed.

The banner is printed before the remote command, so it is in the output of the block that ran
`ssh`, not in the bootstrap block where the machine's MotD goes. The assertion therefore:

- machine with a MotD file: the first plain line of it, in the place the app prints it. This arm
  is kept, and the banner does not satisfy it (with the banner always printed an "or the banner"
  would make the arm pass without the app printing anything);
- machine without: the banner in any block of the pane (command or output text).

**Risk.** Where the banner lands for bash / zsh (the hidden ssh block) was read, not seen; it only
decides the outcome on a machine with no MotD file. Text before the remote command's first hook is
what a real host with a banner produces too.
