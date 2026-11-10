# #1933 — managed ssh from Windows Git Bash (msys `ssh.exe`), one connection

Owner decision 2026-11-09: ssh from a Windows Git Bash pane is managed by default.
Status: design + implementation on `fix/1933-windows-managed-ssh`. Nothing here was compiled or
run (no builds on the laptop; the Windows VM could not be reached from the agent session).

## 1. What is true today (verified in the tree)

- `enable_ssh_warpification` and `enable_legacy_ssh_wrapper` already default to true. Windows
  exports `WARP_USE_SSH_WRAPPER` (`local_tty/windows/environment.rs`). Not a setting problem.
- `bootstrap/bash_body.sh` `warp_ssh_helper` returns early under `WARP_IN_MSYS2=true` with
  `PlainSSHFallback` (#1551). Hence `hop=ssh_wrapper verdict managed=false` on every Git Bash ssh,
  a plain pass-through pane, and no in-band Tab (`remote_tab_no_helper`, #1841).
- Why #1551 did that: the managed path is two-step. Step 1 is a background ControlMaster
  (`ssh -N -f`, where the user authenticates); step 2 is the session as a mux client of it. Git for
  Windows' `ssh.exe` cannot multiplex (msys has no fd passing over AF_UNIX): "Failed to connect to
  new control master".
- The "single-connection fall-through" the script has for a master without a socket is not usable
  as is: it runs the session with `ControlMaster=yes`, authenticates a second time, and (what
  matters most) sends `ManagedSSHSessionStarting` BEFORE that second login, so the app holds the
  keys through a password prompt. On Unix a termios / tty-read probe opens that hold for the
  prompt; ConPTY has no such probe.
- App side, already in place: a ConPTY connect window is born owned (`TTY_OWNED_AT_ARM`), and
  since #1884 the wrapper's announcement takes such a window back (`AwaitingVerdict`), so the
  wrapper's hooks drive the hold on Windows too. The in-band executor (`InBandCommandExecutor`)
  and the in-band Tab (#1801, bash >= 4 remote) need no second channel.

## 2. The problem a single connection has to solve

With a master, "authentication" and "the session whose first input is Phosphor's bootstrap" are two
processes, and the wrapper prints a hook between them. With one `ssh.exe` there is no gap the
shell wrapper can see: the same process asks for the password and then forwards stdin to the
remote command. Two things follow, and both are key-routing facts:

1. During authentication the keys must reach `ssh.exe` (password, passphrase, host-key yes/no,
   OTP). ConPTY gives no event for "ssh is asking", so the only rule that needs no guess is the
   plain console's: pass through (owner 2026-11-04: Windows ssh.exe typeahead reaching the
   password prompt is accepted).
2. Whatever was typed and NOT consumed by a prompt is still in the console input buffer when
   authentication ends. `ssh.exe` would forward it to the remote tty AHEAD of the bootstrap: a
   partial line is then glued to the bootstrap text, a complete one runs in a half-set-up shell
   (the #1085 / #1641 hazard). Holding in the app from "the SSH hook was seen" is too late: those
   bytes were written before it.

So the design needs (a) an EVENT for "authenticated, session not started", and (b) a way to know,
exactly, what was typed before that event and not consumed.

## 3. Design

### 3.1 The event: ssh's own `LocalCommand`

OpenSSH runs `LocalCommand` (with `PermitLocalCommand=yes`) on the local machine, synchronously,
after authentication succeeds and before it opens the session (before it reads stdin for the
remote). It inherits ssh's stdio, i.e. the console. That is the missing gap, produced by ssh
itself, not by a timer or the screen.

The wrapper passes `-o PermitLocalCommand=yes -o 'LocalCommand=eval "$PHOSPHOR_SSH_AUTH_DONE"'`
with the script in the environment of that one `ssh` (and `SHELL=$BASH` for it, the shell ssh
runs LocalCommand with). No `%` or `${}` in the option, so ssh's token expansion touches nothing.
A user `LocalCommand` for the destination (`ssh -G`) is never overridden: that ssh runs plain.

### 3.2 Hook sequence (one new hook, `SSHWrapperSync`, four stages)

All from the LOCAL side; every stage carries the local shell's session id and a 64-bit nonce the
wrapper mints per invocation. The nonce is never sent to the server (it is in the local hooks,
printed before any session output exists, and in the LocalCommand's environment), so server text
(a banner, keyboard-interactive instructions) cannot forge a stage.

| # | printed by | hook | app (connect window) |
|---|---|---|---|
| 1 | wrapper | `PreInteractiveSSHSession` (existing) | `AwaitingVerdict`: hold (existing, #1641/#1884) |
| 2 | wrapper, before `ssh` | `SSHWrapperSync stage=auth` | AUTH PASS-THROUGH: what was held since the Enter goes to the pty in typed order, later keys pass straight to `ssh.exe` (the plain console's rule). No ceiling runs: nothing is held |
| 3 | LocalCommand | `SSHWrapperSync stage=auth_done` | pass-through ends BY THIS EVENT: keys are held again; the app then writes a sync token (`:PHX<nonce>:`) to the pty |
| 4a | LocalCommand | `SSHWrapperSync stage=managed` | writes the mode byte `M`; then exactly the existing managed verdict (`ManagedSSHSessionStarting`): keys stay held to the remote shell's first prompt |
| 4b | LocalCommand | `SSHWrapperSync stage=plain typeahead=<hex>` | writes `P` + the reported bytes; then exactly the existing plain verdict (`PlainSSHFallback`): held keys go to the pty in typed order, later keys pass through |

Between 3 and 4 the LocalCommand reads the console (raw, no echo) up to the token. It is the only
reader: `ssh.exe` waits for it, the local shell waits for `ssh.exe`. Because the app holds from
stage 3 and the token is written after that, FIFO order of the pty guarantees that everything in
front of the token is every byte typed before the hold that no prompt consumed (call it T), and
that nothing comes after the token until the app's verdict write. No clock is involved: the wait
for the token has no deadline (the token is the answer of the very app that printed the script).
After 10 s without it the LocalCommand prints a one-line notice once and keeps reading; Ctrl-C
ends the ssh (the console is read verbatim but with `isig` left on) and the tty is restored.

The token and the mode byte are printable ASCII (`:PHX<nonce>:`, `M`, `P`) so the console's input
translation cannot change them.

- T empty (nobody typed blind during the connect; the usual case, and the password prompt flushes
  earlier typeahead anyway): 4a. The remote reads `M` and runs the bootstrap; the session is
  managed exactly like the multiplexed one from here (SSH hook, InitShell, bootstrap, first
  prompt, hand-over of the held keys).
- T not empty: 4b. The LocalCommand already consumed T, so the app writes it back (the bytes come
  from the nonce-authenticated local hook, and they go only to the pty they were typed for), behind
  `P`: the remote reads `P`, execs the user's login shell with nothing of Phosphor's, and T, then
  the keys held since stage 3, reach that shell in typed order, exactly as plain ssh would have
  delivered them. THIS CONNECTION IS PLAIN. Nothing is lost, reordered or run in the wrong place.

### 3.3 Remote side

The remote command (unchanged for the multiplexed path) gets a prefix only on this path: read ONE
byte from the tty (`stty -icanon -echo min 0 time 200; dd bs=1 count=1`, POSIX sh, no bash needed
yet). `M` continues into the existing bootstrap command. Anything else, or nothing for 20 s (the
give-up net: an app that never answered), prints the MotD as sshd would have and `exec "$SHELL" -l`:
a plain login. The byte is read before the `SSH` hook is printed, so a plain session never
announces a managed one.

The `SSH` hook carries `"single_connection": true` and an empty `socket_path`.

### 3.4 Bootstrap delivery, remote shell up, typeahead, Tab

Unchanged machinery, now reached from Windows:

- Bootstrap delivery: the remote bash's rcfile prints `InitShell` (as an OSC: the remote is told
  `WARP_USING_WINDOWS_CON_PTY`), the app writes the bootstrap (counted raw read when the remote
  reported `raw_bootstrap`) through the one pty. Because the session has no socket, the
  remote-server (helper) setup is not started for it (`SshInitShell` is not used), so nothing waits
  on a binary check.
- "Remote shell is up" is the remote shell's own first `Precmd` after `Bootstrapped` (#1572), as
  on every platform. No timer hands over; `SSH_CONNECT_FIRST_PROMPT_GRACE` and the no-probe
  ceiling stay what they are, give-up nets.
- Typeahead: held from stage 3 to that first prompt, delivered once in typed order by the existing
  hand-over (`deliver_ssh_typeahead_if_ready`), Legacy and Table routes alike.
- In-band Tab (#1801): the session is `WarpifiedRemote` with no helper, so
  `is_remote_without_helper()` holds and Tab asks the remote bash (>= 4) in-band. The
  `candidates from the remote shell` line is REQUIRED on Windows Git Bash again
  (`remote_tab_no_helper`).
- Side commands (generators: completions data, chips): the in-band executor, as for a
  Phosphorized subshell. `RemoteCommandExecutor` (ControlMaster) is never built for a session
  without a socket.

### 3.5 Fallbacks to plain ssh (no key loss in any of them)

Decided locally, BEFORE ssh starts (wrapper prints `PlainSSHFallback`, the existing verdict):
`PHOSPHOR_MSYS_MANAGED_SSH=0` (kill switch), stdin or stdout not a terminal, a user
`RemoteCommand` or `LocalCommand`, no nonce, a server that is not known to be POSIX OpenSSH (3.6).
Decided at stage 4: typed-ahead keys (4b); the console ending (no terminal) while the token is
awaited (4b with what was read).
Decided remotely: no mode byte within 20 s (the one clock in the design, a give-up net on the
REMOTE side for an app that never answered: the remote command then execs the plain login shell;
it reads at most the one byte, and in that state the app has written none).
ssh fails before authenticating (bad password, unreachable, Ctrl-C): no LocalCommand runs, the
block ends, the window ends as a failed connect does today (held keys, if any, become editor text
/ run locally per the existing rules; in pass-through nothing is held).

### 3.6 Which servers get the managed session (owner decision 2026-11-09: POSIX OpenSSH only)

The multiplexed path asks the remote "are you cmd/PowerShell" over the master (#1035) before it
sends a POSIX bootstrap. One connection cannot ask first, and a server that cannot run the
bootstrap command prints an error and closes the session (Windows OpenSSH: "'export' is not
recognized"; network gear: its own CLI's error). Before #1933 every ssh from Git Bash was plain,
so all of those worked. The rule is therefore an allow-list, not a deny-list: the session is
managed ONLY when the server's SSH identification string names POSIX OpenSSH. Everything else is
plain ssh exactly as before #1933.

`ssh-keyscan -T 5 -t ed25519 -p <port> <hostname>` (host and port as `ssh -G` resolves them)
prints the identification string as a comment before any key exchange result and never
authenticates, prompts or writes `known_hosts`.

| answer | session |
|---|---|
| `SSH-2.0-OpenSSH_<digit>...` or `SSH-1.99-OpenSSH_<digit>...`, without `for_Windows` | managed (one connection) |
| `SSH-2.0-OpenSSH_for_Windows_...` | plain |
| any other identification string (network gear, Dropbear, other implementations, a name that only starts like OpenSSH) | plain |
| no answer within 5 s, no `ssh-keyscan`, a `ProxyJump` / `ProxyCommand` (keyscan cannot follow it) | plain |

A definite answer goes into the existing 10-minute probe cache (same key, same file); "no
answer" is never cached, so the next ssh asks again.

Dropbear is deliberately not on the allow-list: its usual hosts are BusyBox systems whose login
shell is `ash`, where the bootstrap would end in an unmanaged shell anyway, and whose `stty` /
`dd` (the mode-byte read, 3.3) are not guaranteed. Nothing to gain, something to break.

What the allow-list cannot see: a host that runs real OpenSSH but gives the user a restricted or
non-POSIX login shell (some appliances). It gets the bootstrap command, as it does from Linux and
macOS today when their probe is inconclusive. Workaround: `PHOSPHOR_MSYS_MANAGED_SSH=0` or
`command ssh`. And the cost of the rule: a POSIX server behind a jump host is plain from Git Bash.

### 3.7 What is lost compared with the multiplexed session

- The remote helper (remote-server binary): it needs a second channel. So no helper-backed file
  tools, codebase index or helper completions; the existing no-helper behaviour applies
  (`DECLINED.md`: file tools refuse and name `run_shell_command`). This matches the owner's rule
  that no-helper is the primary path.
- The shell backend's out-of-band channel (`register_shell_mode_channel`): none, so shell-mode
  features are `Unavailable` (parity design R1: never type into the user's pty for those).
- Side commands are in-band: serialized through the user's pty, only at a prompt.
- The 4b case: a connection during whose login the user typed is a plain session.
- Remote zsh works like bash from the app's point of view but only the bash wrapper
  (`bash_body.sh`) gets this path; a Git Bash pane running msys zsh or fish keeps plain ssh.
- No `ControlPersist`, no shared authentication for split panes (each pane logs in).

### 3.8 Windows OpenSSH (`C:\Windows\System32\OpenSSH\ssh.exe`) vs msys `ssh.exe`

The wrapper is a bash function, so it only exists in Git Bash / msys panes, where `command ssh`
is msys `ssh.exe` unless the user's PATH says otherwise. `LocalCommand` exists in both builds; the
LocalCommand script needs bash, `stty` and a console msys can put in raw mode, which is why
`SHELL=$BASH` is set for that ssh. PowerShell and cmd panes have no wrapper: ssh there stays what
it is today (pass-through, #1638 decisions unchanged). If Win32-OpenSSH is first in a Git Bash
PATH, LocalCommand still runs through `$SHELL -c`; this combination is untested and is the first
thing to try on the VM.

### 3.9 Linux / macOS

Nothing changes. Every new branch in the script is behind `WARP_IN_MSYS2=true`; the new hook is
only printed there; `single_connection` defaults to false; the app's new state is only entered by
the new hook. The one shared edit is in the probe helper (`warp_ssh_remote_is_non_posix`), which
takes the keyscan branch only for an empty control path, which only the msys path passes.

## 4. Rejected alternatives

- Hold through authentication and release on a prompt-shaped screen: screen guessing, which the
  owner removed from ssh (#1611, #1612, #1621).
- `SSH_ASKPASS` + `SSH_ASKPASS_REQUIRE=force` to get "ssh is asking" events: puts every password
  and passphrase through a helper of ours and changes the login UX; too much risk on the login
  itself.
- Let the remote report the typeahead text it found before the bootstrap and adopt it in the app:
  text chosen by the remote would enter the app's input (and, in agent mode, could become a
  prompt). Only the local, nonce-authenticated LocalCommand reports bytes, and they go to the pty.
- A polyglot first line that also works in cmd.exe / PowerShell: cannot be verified here.

## 4a. App side, as implemented

- `SSHValue.single_connection` -> `IsLegacySSHSession::Yes { single_connection }`;
  `has_control_socket()` gates everything that needs the socket: the helper setup
  (`SshInitShell`, `notify_session_bootstrapped`), the ControlMaster executor, the shell-mode
  out-of-band channel, the fall-back-to-ControlMaster swap. Such a session takes the in-band
  executor and reports no "helper fallback" (nothing degraded: the helper was never possible).
- `TerminalView::ssh_on_wrapper_sync` handles the stages on the existing connect window: `auth`
  sets the window's `auth_pass_through` (an input to the existing `ssh_wrapper_secret_pass_through`,
  so both routes, Legacy and Table, read it the way they read the Unix secret-read probe) and
  delivers what was held; `auth_done` clears it, tells the phase machine (`TtyReclaimed`), and only
  then writes the token; `managed` / `plain` write the mode byte (and the drained bytes) and call
  the existing verdict handler. The window does not run its no-probe ceiling during the login.
- A stage is honoured only in order, with the `auth` stage's nonce, from the announcing session,
  and `auth` only on Windows (`SSH_SINGLE_CONNECTION_PLATFORM`).
- The three raw writes are an allow-listed pty door (`script/check_pty_doors`).

## 5. Open points for the owner (none blocks the implementation)

1. 4b makes "typed during the connect" a plain session for that connection. The alternative is to
   put T at the FRONT of the app's held keys and stay managed (then T goes through the first-prompt
   hand-over like on Unix). It needs a front insert into the one input queue behind the connect
   gate; it was not attempted blind. Filed as #1959.
2. The keyscan probe is one extra TCP connect + banner per destination per 10 minutes.
3. No banner tells the user the session has no helper; the log says so.
4. msys zsh / fish panes keep plain ssh. Decided 2026-11-09: only POSIX OpenSSH servers are
   managed (3.6); what is left of that point (jump hosts are plain, an OpenSSH host with a
   non-POSIX login shell is not recognised) is in #1960 with points 2-3.

## 6. Tests

- `bootstrap_test.rs`: the msys branch with a stub `ssh` and `ssh-keyscan` (options passed, the
  nonce, the remote command's mode-byte prefix, every local fallback), and the LocalCommand script
  itself in a real bash with a pipe standing in for the console.
- `ansi/mod_test.rs`: the new hook parses; unknown stage is ignored.
- `view_test.rs` (Legacy and Table): auth pass-through writes typed keys to the pty in order; the
  `auth_done` event writes the token and holds; `managed` writes `M` and holds to the first prompt;
  `plain` writes `P` + T and then the held keys, in order; a stage with a wrong nonce changes
  nothing.
- `command_executor_tests.rs`: a single-connection session never gets the ControlMaster executor.
- appci (`cases.PROVING`): Windows Git Bash managed ssh cases; `remote_tab_no_helper` keeps
  requiring the in-band line.
