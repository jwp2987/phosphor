# #2230: the typed zsh bootstrap on the Windows one-connection ssh outlives its give-up

Follow-up of #2155. **Decided by the owner on 2026-11-14: the plain session (see the last
section). The budget design below was not taken and is kept as the record of the alternative.**

## What happened (r31b, Windows Git Bash, `lab_path_detect.p_exec.lan.nohelper`)

- 02:00:13 the zsh that replaced the remote bash announced itself (#2155) and the app wrote the
  zsh bootstrap: 64,019 bytes, 16 writes, done in 1.3 ms on the app's side.
- ConPTY then fed those bytes to `ssh.exe`'s console. The zsh was bootstrapped at 02:01:14:
  61 s, about 1.05 KB/s. The pty produced output the whole time (about 60 wakeups per second:
  the remote tty echoing the bootstrap lines).
- +7 s: the bootstrap give-up fired (`BOOTSTRAP_FAILED_DURATION`, `view.rs:989`), took the 3 s
  grace (`BOOTSTRAP_PROGRESS_GRACE`, `view.rs:1053`), and at +10 s marked the session failed.
- +15 s: the ssh typeahead window gave up (`give_up_ceiling`, `view.rs:42517`).
- The session worked from +61 s on. Nothing was typed during the wait in this run.

Two more faults: the 3 s grace was taken on the Precmd of the bash that is gone
(`bootstrap_precmd_seen_session`, `view.rs:20726`; same session id), and the typeahead give-up
writes held keys to the pty while the bootstrap text is still being read from it, so keys
typed during the wait would land inside the bootstrap.

## Options

**(b) Carry the zsh bootstrap in the remote command, as bash's is (#2051).** Not recommended.
The local Git Bash keeps only its own bootstrap lines (`warp_ssh_msys_bootstrap_text`,
`bash_body.sh:2925`); the app would have to hand it zsh's text as well, and every managed
connection would carry about 64 KB more for a profile few hosts have. The command is one
argument of the remote login shell; Linux limits one argument to 128 KB, and bash's bootstrap
plus zsh's is close to that, so `warp_ssh_msys_command_fits` would turn many connections
into plain ones. Size not measured here.

**(c) Type less.** Two forms, neither sufficient alone.
- A stub that sources a file: the remote bash has no zsh bootstrap to write, so the bytes
  still have to cross the console. Compressing them (gzip + base64) would cut 64 KB to
  roughly a third, about 20 s: still past the net, and it needs both tools on the host.
- A remote zsh is sent the whole ssh wrapper; a remote bash is not
  (`bootstrap::strip_local_only_block`, `bootstrap.rs:360`). Marking that block in
  `zsh_body.sh` would shorten every typed zsh bootstrap, but leaves tens of seconds here.
  A separate small issue, not part of this change.

**(a) A give-up that measures the bootstrap, not the clock.** Recommended.

## Recommended design

Scope: only a session whose bootstrap the app typed on a one-connection ssh
(`IsLegacySSHSession::Yes { single_connection: true, .. }` and `app_writes_bootstrap`), which
is the only path where the delivery rate is known to be about 1 KB/s. On that path only a
bash login is managed and its bootstrap rides in the command (#2051), so in practice this is
the zsh of #2155. Every other session keeps today's net unchanged.

1. **Budget from bytes.** When `PtyController::initialize_shell` writes a bootstrap for such a
   session (`pty_controller.rs:2122`), it reports the byte count. The view arms the give-up
   with `BOOTSTRAP_FAILED_DURATION + bytes / 512 B/s` (half the measured rate: 64 KB gives
   about 132 s) instead of 7 s. Change in `start_bootstrap_timer` (`view.rs:26739`) and its
   InitShell arm (`view.rs:20702`).
2. **Stall check inside the budget.** While the budget runs, the net checks every
   `BOOTSTRAP_FAILED_DURATION` whether the hidden bootstrap block of the pending session grew
   (the echo of the typed text). No growth in a whole window fails the session at once; growth
   re-arms. This counts output of one named block while typed bytes are outstanding, not any
   output byte, which the subshell net deliberately refuses (`view.rs:1064`). Change in
   `on_bootstrap_failed_timer_complete` (`view.rs:26874`).
3. **The typeahead window follows the same deadline.** `past_ceiling` (`view.rs:42517`) does
   not fire while that budget is live, so held keys stay held until the first prompt or the
   give-up, and are then written in typed order as today. No key is dropped or interleaved.
4. **The grace belongs to the shell that earned it.** A replacing InitShell (#2155) clears
   `bootstrap_precmd_seen_session` and `bootstrap_grace_used_session` for that id.
5. **Kill switch** `PHOSPHOR_SSH_TYPED_BOOTSTRAP_BUDGET=0` (the 7 s net as before); log hops
   `bootstrap_budget bytes= budget_ms=`, `bootstrap_budget_progress`, `bootstrap_budget_stalled`.

Cost to the user: on this path a host that really hangs is declared failed after one silent
window (7 s) if nothing is echoed, or after the byte budget (about 2 minutes for zsh) if it
keeps printing. Keys typed meanwhile wait for that long.

## Tests

Pure: the budget from a byte count (zero bytes and other sessions give the old value). View: a
one-connection zsh session with 64 KB written is not failed at 10 s while its bootstrap block
grows, is failed after one window without growth and at the budget's end; held keys are
written once, in order, at the first prompt; a replacing InitShell resets the grace; the kill
switch gives 7 s + 3 s. `docs/testing.md` in the same change.

## What the next Windows lab run must show

`lab_path_detect.p_exec.lan.nohelper` on Git Bash, terminal and agent mode: one
`bootstrap_budget bytes=64019` line, `bootstrap_budget_progress` lines, no
`Bootstrapping failed for shell "zsh"`, no `give_up_ceiling`, `Bootstrapped` for zsh, then the
case's own checks. The bash and zsh lab arms on Windows: no `bootstrap_budget` line for bash
(its bootstrap is in the command), unchanged results.

## Question for the owner

On Windows Git Bash, a host whose profile switches to zsh takes about a minute to become
usable, because the setup text has to be typed through a slow channel. Is waiting that minute
(with keys held, then delivered) what you want, or would you rather such a host be a plain ssh
session at once, with no Phosphor features, on Windows only?

## Owner answer (2026-11-14) and what was built

Plain session at once, on Windows only. Linux and macOS keep the #2155 hand-over.

- **Wrapper** (`bash_body.sh`, before `warp_rc_snippet`): when the session runs on one
  connection (`warp_single`), the remote bash's rcfile gets `WARP_SSH_PLAIN_HOOK`, the hex of
  this shell's `PlainSSHFallback`. Empty on every other path.
- **Remote** (`bash_body.sh`, the `__warp_exec_zsh` block): the value rides in
  `__WARP_EXEC_ZSH`. The zsh that is the replaced shell prints it instead of InitShell, puts
  ZDOTDIR back, reads the user's own startup files and is a normal login zsh. Nothing is typed.
- **Model** (`TerminalModel::ssh_wrapper_verdict`): a plain verdict while a one-connection
  session is announced and not bootstrapped drops that session
  (`make_pending_one_connection_ssh_session_plain`), marks the block list bootstrapped and
  continues in a running block named after the typed ssh command, as an abandoned subshell
  bootstrap does.
- **View** (`ssh_on_replaced_shell_plain`): the bootstrap give-up for that session is
  cancelled, its pending entry forgotten, the phase machine is fed the facts, and the connect
  window ends with its own release (`SshRelease::ReplacedShellPlain`). What it holds is
  delivered once, in typed order, to ssh, the way a plain ssh's keys are when ssh owns the tty.
  Hops: `ssh_wrapper replaced_shell_plain`, `ssh_gate_released how=replaced_shell_plain`,
  `kh_finish_end delivered=plain_session`.
- **After review of the first commit.** Only the wrapper's own verdict counts (it quotes the
  local shell's session; one quoting the pending remote session, or none, is ignored). The
  running block is named at block level with no `Preexec` event, so the view starts no new
  connect window, pending warpify host or end-of-login watch for it.
- **The two other faults.** The grace taken on the replaced bash's Precmd still applied to the
  Linux and macOS hand-over and is fixed: an InitShell under an id whose bootstrap prompt was
  already seen clears that evidence. The typeahead give-up writing into a bootstrap that is
  still being read is not specific to this path any more (the one-minute typed bootstrap is
  gone) and is NOT changed: it remains possible wherever a typed bootstrap outlasts the
  ceiling on a platform without the tty-read probe, and needs its own issue and design.
- **Lab.** `lab_path_detect.p_exec` on the Git Bash pane expects the hop above and no
  takeover line, then leaves the session; the managed-session checks are not judged there.
