# Round 13: Git Bash managed ssh, the bootstrap travels in the remote command (#2051)

Nothing here was compiled, and nothing was run on Windows. What was run: the shell side, in bash 5.3 on Linux (the wrapper with stub
`ssh` / `ssh-keyscan`, and the remote command it builds in a real pty and on pipes), `script/test_init_shell_escaping`, the appci selftest.
Verdict and evidence for the defect are in `docs/design/r13-winssh2.md`; this note is the fix.

## Cause (APP, design of #1933)

`docs/design/1933-windows-managed-ssh.md` 3.4 delivers the bootstrap "through the one pty": `PtyController::initialize_shell`
(`app/src/terminal/writeable_pty/pty_controller.rs`) writes about 62 KB on `InitShell`, and on this path those bytes are console input of msys
`ssh.exe`, which forwards about 1.3 KB/s. 45 to 60 s on any server; the app's nets give up after 6.5 s and 15 s.

## Owner decisions (2026-11-10)

1. `PHOSPHOR_MSYS_MANAGED_SSH` defaults to OFF; `=1` switches the path on (commit 1 of this branch, `DECLINED.md`).
2. Only a remote whose LOGIN shell is bash is managed on this path. zsh, fish and every other login shell get a plain login.

## Fix

The remote command is an argument of ssh: it never touches the console.

- **Where the text comes from.** A Git Bash shell is bootstrapped by `source '<temp file>'`, and the app deletes the file at `Bootstrapped`. While
  it is being sourced the shell keeps its lines (`mapfile` from `BASH_SOURCE[0]`, one builtin read, msys only: `bash_body.sh`, top of the
  local-only block). At the first managed ssh `warp_ssh_msys_bootstrap_text` drops the lines from `: __warp_local_only_begin` to
  `: __warp_local_only_end` and keeps the result: byte for byte what `bootstrap::bootstrap_for_session` sends a bash over ssh in its
  here-document form (61,958 bytes today; checked against a Python copy of `script_for_shell` + `strip_local_only_block`). No app code,
  no second file, no path to convert. The winssh2 note proposed a second file written by the app; this needs no Windows-only Rust.
- **The remote command** (`warp_ssh_helper`, `warp_bs_in_cmd`): the mode-byte read of #1933, unchanged; then

  ```
  if test "${SHELL##*/}" != bash; then <PlainSSHFallback hook>; <MotD>; exec "$SHELL" -l; fi
  export ...
  {
  <SSH hook, "bootstrap_in_command": true>
  exec -a bash bash --rcfile <(echo '<InitShell snippet>'; command -p cat <&9) 9<&-
  } 9<<'__PHOSPHOR_BOOTSTRAP_<remote session id>__'
  <bootstrap text>
  __PHOSPHOR_BOOTSTRAP_<remote session id>__
  <PlainSSHFallback hook>; <MotD>; exec "$SHELL" -l
  ```

  bash runs the bootstrap as the rest of its rcfile, right after it printed `InitShell`. The here-document is on a group, not inside `<( )`
  (bash 3.2 counts parentheses in there) and not on `exec` (the process substitution is expanded before the redirection exists). The new bash
  does not inherit fd 9. The SSH hook is printed inside the group, so only when the here-document is open; if it cannot be opened (bash before
  5.1 needs a temp file) the group does not run and the lines after it give the plain login. The delimiter carries the 64-bit remote session
  id, so no line of the text can equal it. Both hooks are hex-encoded locally (nothing in them is the remote's to fill in).
- **The app** (`SSHValue::bootstrap_in_command` -> `IsLegacySSHSession::Yes { bootstrap_in_command }`): `initialize_shell` writes nothing for
  such a session (`bootstrap::app_writes_bootstrap`, hop `bootstrap_in_command ... wrote=0`). Everything after `InitShell` is unchanged:
  `Bootstrapped`, the first `Precmd`, the hand-over of the held keys. Measured in a pty on Linux: SSH, InitShell, Precmd, Bootstrapped, Precmd,
  then a typed line runs with Preexec / CommandFinished. (The typed delivery was not run side by side.)
- **A login shell that is not bash.** The app answered `M` before the remote command ran (the mode byte is the LocalCommand's stage), so the
  remote has to say it: it prints `PlainSSHFallback` with the local session id (the id is in the command already, the SSH hook carries it).
  `ssh_on_wrapper_verdict` (`view.rs`) accepts that ONE plain verdict from a `Managed` window, only when the one-connection sync made it managed
  and the remote has announced nothing (`ssh_single_remote_plain_applies`); then the keys held since `auth_done` go to ssh in typed order, as at
  the `plain` stage. After the SSH hook it is ignored like every second verdict. An event, no clock. Without it a zsh login would sit on the
  15 s ceiling.

## Limits, measured or reasoned

| leg | limit | handling |
|---|---|---|
| msys bash -> msys `ssh.exe` | Cygwin passes argv to a Cygwin program through the child's start-up block, not the 32,767-character Windows command line (that is why such processes show only their name as command line). REASONED from Cygwin's `spawn.cc`, not measured on Windows | `warp_ssh_msys_command_fits` MEASURES it: `ssh -V <command + 256 bytes>` once per shell (ssh prints its version and exits at `-V`). Fails: plain ssh |
| msys bash -> Win32 `ssh.exe` (first in PATH) | 32,767 characters: the command (about 66 KB) cannot be passed | the same measurement fails: plain ssh. No path heuristic |
| ssh protocol (`exec` request) | OpenSSH packets up to 256 KB | 66 KB fits |
| sshd -> login shell (`$SHELL -c <command>`) | Linux: 128 KiB per argument (`MAX_ARG_STRLEN`); macOS / BSD: total only | over 100,000 bytes: plain ssh. Today 66,033 bytes |
| here-document on the remote | bash >= 5.1: a pipe when it fits the pipe size, else a temp file; older: a temp file (from bash's change notes, not measured per version) | cannot be opened: plain login (above; the failure itself was not provoked in a test, only `{ ...; } 9<missing` was) |

Chunking is not needed and compression (gzip + base64, an owner-optional step in the task) is not proposed: the text fits with a third to spare.
If the bootstrap grows past about 95 KB the guard turns every such ssh plain; the unit test with the real size is the place that would say so
(see "Not done").

## Guards and switches

All before any connecting ssh starts, each the user's plain ssh with the `PlainSSHFallback` verdict: the switch not 1; no terminal; a user
`RemoteCommand` / `LocalCommand`; no nonce; not a POSIX OpenSSH banner (all #1933); no kept bootstrap lines (a shell not bootstrapped from a
file); lines that are not provably the bootstrap (not exactly one begin and one end marker in order, no `bash.sh` here-document); `ssh` cannot
be started with the command; the command over 100,000 bytes.
`PHOSPHOR_MSYS_SSH_BOOTSTRAP=typed` keeps the #1933 delivery (the app types the bootstrap), to compare the two on one build.

## Rejected

- A second file written by the app and its path exported (winssh2 proposal): Windows-only Rust that no Linux run compiles, a file to clean up,
  a path to convert for msys. The shell already has the file open.
- Stripping the local-only block with `${var%%pattern*}`: 4.6 s per call on 84 KB in bash 5.3 (quadratic). The line loop is about 30 ms on Linux, once per shell (not timed on msys).
- Reading at the first ssh instead of at bootstrap: the temp file is gone by then.
- A remote "I am bash, send the mode byte" round before `M`: changes the #1933 hand-shake that round 12 showed working.
- Letting the remote fall back to the typed delivery when fd 9 fails: that is the minute-long session this replaces.

## Risk

- Linux / macOS: the remote command of the multiplexed path and of the typed one-connection path is byte-identical to before (the two shared
  pieces moved into variables; compared with stub ssh before and after). The new code runs only under `WARP_IN_MSYS2=true` with the switch on.
- Every Git Bash shell keeps about 2,500 lines (88 KB) of its bootstrap in an array until its first managed ssh, with the switch on or off
  (the switch can be set later in the shell). One builtin read at start-up; not timed on msys.
- The bootstrap now runs INSIDE the rcfile, not at the first prompt. Proven only on bash 5.3. Unproven: bash 3.2 / 4.x remotes, a remote
  `.bashrc` that behaves differently before the first prompt.
- `ssh -v` prints the command it sends ("Sending command: ..."): 66 KB on the screen instead of 5 KB. Not guarded.
- The local bash bootstrap grew by about 3.6 KB (the new functions), on every platform.

## Not done

- No unit test pins the size of the real remote command against the 100,000-byte guard (the Rust tests use a small stand-in text).
- `zsh_body.sh` / `fish.sh` wrappers: a Git Bash pane running msys zsh or fish keeps plain ssh, as before.
