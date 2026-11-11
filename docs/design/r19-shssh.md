# r19-shssh: keys typed in an ssh session to a host whose login shell is sh / dash / ash / ksh (#2120)

Branch `fix/r19-shssh` from integ/r15 `be5dd47ba`. Nothing was compiled or run when this was
written (`rustfmt --check` and the appci self-test only): the change is worked, unproven.

## What happens

`shell_integration_tests::test_ssh_into_sh` (round 14, `integration-linux: [1]`): the line typed
at the remote `$ ` prompt is never written to the pty. On Linux every key typed in such a session
is held until ssh exits.

## Cause (read, not run)

The remote command the wrapper hands to ssh prints the `SSH` hook for EVERY login shell and then
bootstraps only two of them:

- `app/assets/bundled/bootstrap/bash_body.sh:2793-2795`: the hook is printed with
  `remote_shell = ${SHELL##*/}`.
- `bash_body.sh:2797-2817`: `if test "${SHELL##*/}" != "bash" -a "${SHELL##*/}" != "zsh"` prints
  the MotD, sources `/etc/profile` and runs `exec "$SHELL"`. No `InitShell`, `Bootstrapped` or
  `Precmd` can ever come from that session. (`zsh_body.sh` and `fish.sh` carry the same text.)

The wrapper took the managed way because its probe is definitive for such a host:
`warp_ssh_remote_is_non_posix` (`bash_body.sh:3144-3170`) gets the exact POSIX reply
`warp-probe-%COMSPEC%-` from sh / dash / ash / ksh and answers `posix` at once; the login shell's
name is asked only when that first answer is NOT definitive.

In the app the hook is taken as the announcement of a managed session, in three places:

1. `TerminalModel::ssh` (`app/src/terminal/model/terminal_model.rs:4231-4271`) stores the hook
   as `pending_legacy_ssh_session` whatever the shell is. Only an `InitShell` takes it
   (`:4455`), so for these shells it stays set, and `has_pending_ssh_session` stays true.
2. The view sets `gate.managed = true` on `ModelEvent::SSH(_)` (`view.rs:18678-18681`) and again
   from `facts.has_pending_ssh_session` (`refresh_ssh_connect_gate`, `view.rs:12932-12934`).
3. `SshPhaseMachine` sets `managed` from `SshObservation::hook_seen`
   (`input_queue/ssh_phase.rs:216-218`), which is the same model fact
   (`view/ssh_edges.rs:138`).

The window was already `SshWrapperHold::Managed` (`ManagedSSHSessionStarting`,
`view.rs:11824-11829`): ownership is deferred (`ssh_defer_tty_owned_for_wrapper`) and the hold is
to end at the remote shell's first prompt. `SshConnectGate::should_release`
(`view.rs:41457-41479`) then needs one of: the block failed, the announced session came up
(`saw_pending`, needs `InitShell`), the block ended, or the ceiling. The ceiling exists only
where no tty-read probe works (`has_no_probe_net`, #1809). So on Linux no event and no clock ends
the hold; on macOS the 15 s net does, as a logged give-up.

Two more effects of the same stale fact, found while reading (same root, same change):

- `ssh_pre_init_abort_bytes(hook_seen, pending)` (`view.rs:41143`) is `Some("~.")` for as long as
  the hook is "pending" with no session: a Ctrl-C typed at the remote sh prompt is written as
  ssh's escape `~.` instead of the byte (`table.rs:1019`, `view.rs:10454`). The fact outlives the
  ssh too: it is cleared by the next `InitShell` of the pane only.
- `ssh_eot_must_wait` (`view/route_facts.rs:229-232`) holds a Ctrl-D while the window is open and
  the hook is "pending".

## Chosen fix: the app side, at the model (the root that covers both candidates)

**An `SSH` hook that names a shell the remote command does not bootstrap announces no session.**
The rule is one pure function, `input_queue::ssh_hook_announces_session(remote_shell)` (true for
exactly `bash` and `zsh`, the two names of the remote command's `case`), read by the model and by
the view, so the two state holders cannot disagree:

1. Model (`TerminalModel::ssh`): for such a shell nothing is stored
   (`pending_legacy_ssh_session` stays `None`) and the remote session id is not registered (no
   shell will ever use it). `Event::SSH(remote_shell)` is still sent. So `has_pending_ssh_session`
   is false, and with it `SshObservation::hook_seen`: `SshPhaseMachine::managed` is never set,
   Ctrl-C is the byte, Ctrl-D is not held.
2. View, on `ModelEvent::SSH(shell)`: `gate.managed` is set only for an announcing shell. For any
   other shell `ssh_on_remote_plain_login` runs: it is the PLAIN verdict of a window the wrapper
   still holds (`AwaitingVerdict` or `Managed`), exactly what `PlainSSHFallback` does and what
   #2051's remote plain verdict does: wrapper hold `Off`, then `ssh_on_tty_owned` for the
   window's block. The hook was printed by the remote command through the session's ssh, so ssh
   owns the tty at that event whatever the latch recorded (the #2051 arm makes the same
   statement). `ssh_on_tty_owned` sets the gate's `tty_owned`, sends `SshEdge::TtyOwned` to the
   machine, and delivers what is held to ssh in typed order (or the termios guard discards it,
   as for every ownership). Later keys pass straight through. A window no wrapper holds is left
   alone: its own ownership event releases it, as for any unmanaged ssh.

After that the window is an unmanaged one in both holders (`managed` false, `tty_owned` true):
`plain_ssh_owns_tty` is true (Ctrl-C is the byte, #2044), a clean end of the block is
`failed`/"nothing more is coming" in the gate and in the machine, and no give-up deadline exists
on Linux because none is needed.

No timer is added or changed. The release is the hook, an event in stream order. Nothing reads
the screen. Held keys are delivered by the existing hand-over (`deliver_held_ssh_typeahead_to_
tty_owner`), so order and count are that path's.

Where the keys land: the hook is printed before the MotD, `/etc/profile` and `exec "$SHELL"`, so
keys released at it wait in the remote tty's queue until the shell reads, as typeahead does in a
plain ssh (owner rule: hold until ssh owns the tty, then straight through).

### Why not the wrapper side (b)

- It cannot cover the hole. The probe's first answer is definitive `posix` for these shells, so
  the wrapper would have to ask every POSIX host for its login shell's name as well: one more
  channel and remote shell start per uncached connect (#1136, #1172 removed such costs). An
  inconclusive probe (5 s net, a failed channel) still goes the managed way and still meets the
  hook. A cached `posix` verdict, an external ControlMaster, the three wrappers (bash, zsh, fish)
  and every pane whose shell loaded the old wrapper would each need the same care.
- It changes what the session is. Today the remote command gives such a host the MotD,
  `/etc/profile`, `TERM_PROGRAM` and `WARP_IS_SSH`, and `test_ssh_into_sh` asserts
  `WARP_PROFILE_LOADED`. `DECLINED.md` (#2019) says a host with "another POSIX shell" is not
  affected by the fish / nu decision and that the wrapper must not guess. The app-side rule
  changes no byte the wrapper sends.
- The app has to be right anyway: the hook names the shell, and treating a named, unbootstrapped
  shell as a coming session is the defect. With the app side done the wrapper side adds nothing.

### Rejected

- A ceiling on Linux (undoing #1809): a timer as the release, against the owner rule.
- Releasing on screen text (the `$ ` prompt): screen guessing, removed in #1611 / #1612 / #1621.
- Keeping `pending_legacy_ssh_session` and adding a second flag for "plain": two facts for one
  thing, and the `~.` and Ctrl-D rules would each need the flag too.
- A new `Event` variant for the plain verdict: `Event::SSH` already carries the shell's name;
  the rule is one function both sides call.

## Kill switch

`PHOSPHOR_SSH_UNBOOTSTRAPPED_PLAIN=0` (`input_queue::switches`, read once per process): every
`SSH` hook announces a session again, in the model and in the view (the old behaviour, with the
hold of #2120).

## Risk

- A remote whose `$SHELL` ends in `bash` or `zsh` under another name (`bash5`, `zsh-5.9`) is not
  bootstrapped by the remote command either (it compares the same two names), so the rule is in
  step with the script. `ssh_hook_announces_session` and the script's `case` must stay in step;
  the function's comment says so.
- A warpified subshell started by hand inside such a session (`bash` typed at the remote `$`)
  used to take the stale pending hook at its `InitShell` and so counted as the ssh session's
  shell (`SessionInfo::create_pending`). It no longer does: it is a subshell of the pane, as a
  subshell started in a fish / nu plain session is. The stale hook was also taken by a LOCAL
  subshell started after the ssh ended, which was wrong.
- The agent-view zero state and the "abandoned before bootstrap" log read the same model fact;
  they no longer report a remote bootstrap for these hosts (there is none).
- The telemetry event `SSHBootstrapAttempt(shell)` is unchanged.

## Tests

- Pure: `ssh_phase_tests.rs` (the rule; the machine with and without the hook fact),
  `switches_tests.rs` (the name).
- Model: `terminal_model_test.rs` (`sh`, `dash`, `ash`, `ksh`: no pending session, the event is
  still sent; `bash` / `zsh` unchanged).
- View: `view_test.rs`, Legacy and Table are the same code since step 8, so one mode:
  the held keys are written once in typed order at the hook, later keys pass through, the gate
  and the machine both say "plain ssh owns the tty", Ctrl-C is not `~.`, no give-up; a clean end
  of the block ends the window in both; a `bash` hook still holds to the first prompt.
- Integration: `test_ssh_into_sh` / `test_ssh_into_ash` stay as they are and are the proof.
- Lab: the dash / ash arms of `lab_enter_start` and `lab_no_helper_chip` are declared but the
  fix tier's quick matrix leaves those shells out, and their connect step waits for the lab
  hook's `prompt.log`, which a dash started by the remote command never writes (see "Found on
  the way"). New case `lab_nohelper_sh_login_keys` (dash, ash; `PROVING_LAB`, exact arm names):
  typeahead and typed lines arrive once, in order, in the host's effect file; the app log has
  the plain-login hop and no give-up.

## Found on the way (not changed here)

The remote command starts these shells with `exec "$SHELL"`: not a login shell, so `~/.profile`
is not read (a plain `ssh host` reads it). `/etc/profile` is sourced by the command itself. The
one-connection path uses `exec "$SHELL" -l` (#2051). Filed as #2126; no change here.
