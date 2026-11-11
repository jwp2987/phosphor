# r19-loginshell: an ssh login shell the remote command does not bootstrap starts as a login shell (#2126)

Branch `fix/r19-loginshell` from integ/r15 `b3c93c659`. Nothing was compiled when this was written
(`rustfmt --check`, the appci self-test, and the wrapper text run by hand against a real dash and a
real busybox ash): worked, unproven.

## Cause

The remote command the three ssh wrappers hand to ssh ends its branch for a login shell that is
neither bash nor zsh the same way (before this change `app/assets/bundled/bootstrap/bash_body.sh:2797-2817`,
`zsh_body.sh:1645-1668`, `fish.sh:1143-1166`): print the MotD, `. /etc/profile`, `exec "$SHELL"`.
No `-l`, no dash in `argv[0]`: the shell is not a login shell and never reads `~/.profile`. sshd
starts the login shell of a plain `ssh host` with `argv[0] = -<name>`, which reads `/etc/profile`
and `~/.profile`. The pinned upstream text is the same (`42effe840`), so this is an `IMPROVED`
entry in `DECLINED.md`, not a regression.

Already right, not touched: the one-connection paths of `bash_body.sh` (`exec "$SHELL" -l` in the
mode-byte prefix and in the bootstrap-in-command plain login, #1933 / #2051); the fish / nu plain
session (#2019: the wrapper runs the user's own ssh, sshd starts the login shell); the bash and zsh
arms (the bootstrap makes them login shells itself).

## Fix (the same text in all three wrappers)

    if test <1|0> = 1 && "$SHELL" -n -l -c : </dev/null >/dev/null 2>&1; then
      exec "$SHELL" -l
    fi
    if test -r /etc/profile; then . /etc/profile; fi     # the old start
    exec "$SHELL"

- **`-l`, not `exec -a "-${SHELL##*/}"`.** sshd runs the remote command as `<login shell> -c`, so
  the `exec` is the login shell's own builtin, and dash has no `exec -a` (it would try to run a
  program named `-a`). `-l` is taken by dash, busybox ash, ksh93, mksh, pdksh / oksh, yash and by
  bash / zsh under another name (`rbash`, `bash5`). The one visible difference from sshd's start is
  `$0`: `/bin/dash`, not `-dash`.
- **The question first.** A shell that does not know `-l` (the issue's open point) would print an
  option error and end the session at once if it were exec'ed with it. `"$SHELL" -n -l -c :` asks
  without running anything: `-n` makes the shell read its profile files and the `:` without
  executing a command, stdin is `/dev/null`, output is discarded. Checked with a real dash 0.5.12
  and busybox 1.37 ash: a `~/.profile` that appends a line to a file leaves exactly one line after
  the whole remote command (the login start), none from the question. One local process start on
  the host per connect; no extra channel or round trip.
- **`/etc/profile` is not sourced by the command any more** on the login way (the shell reads it;
  twice would double PATH entries). `test_ssh_into_sh` / `test_ssh_into_ash`
  (`WARP_PROFILE_LOADED`) hold through the shell's own read.
- **MotD once.** The command's MotD chain is unchanged and stays the only printer: sshd is quiet
  for a session with a command, and a login shell prints no MotD (login / PAM do).
- **Kill switch** `PHOSPHOR_SSH_LOGIN_SHELL=0`, read by the wrapper in the pane's local shell at
  each `ssh` and sent as a digit (`test 0 = 1`), the pattern of `WARP_SSH_KEEPALIVE`: the old start,
  no question.

## Rejected

- `exec "$SHELL" -l` with no question: ends the session on a shell without `-l`.
- A list of shell names that take `-l`: `sh` is dash, busybox, FreeBSD sh or something else; the
  name says nothing.
- A symlink named `-dash` in a temp dir to get the dashed `argv[0]`: files on the user's host for a
  `$0` nobody reads.
- Sourcing `~/.profile` from the command as well: the shell would still not be a login shell
  (`logout`, `$-`, ksh's login rules), and `ENV` handling differs per shell.

## Risk

- A shell that takes `-l` but does not honour `-n` for its profile files would run them once more
  with no terminal and no output. None of the shells above does; that run could not change the
  session's environment, only repeat a profile's side effects on files.
- A `~/.profile` that was never read in a Phosphor ssh session is read now. That is the point, and
  it is what the same user gets from any other terminal; the kill switch is the way back.
- A profile that tests `$0` for a leading dash sees none (see above).
- A pane whose shell loaded the old wrapper keeps the old start until a new shell starts.

## Tests

- `terminal/bootstrap_test.rs` `ssh_remote_command_starts_an_unbootstrapped_login_shell_as_a_login_shell`:
  the real helper of each wrapper with a stub `ssh`, then the remote command run as the server runs
  it with stub login shells that record `argv[0]` and arguments (takes `-l`; refuses `-l`), with a
  real dash and busybox ash where installed (`~/.profile` read exactly once), and with the kill
  switch.
- Lab: new `lab_nohelper_sh_login_profile` (dash, ash by exact name in `PROVING_LAB`; bash control
  through `lab_nohelper_*`): the remote prompt of the lab's sh hook and `$ENV`, both of which only
  `~/.profile` gives.

## The lab arms the issue names: can they pass now?

Not by this change alone, and not as a pass. The issue says the lab's dash hook writes
`prompt.log` only from `~/.profile`. The hook (`profile.SH_HOOK`) wrote no `prompt.log` line at
all, from anywhere, so `cases_caseremote.connect` could never see a dash / ash prompt: a TEST
fault next to the app defect. The hook's `PS1` now logs a `P` line per prompt (lab only; whole
seconds, busybox `date` has no nanoseconds). With both changes:

- `lab_enter_start.{dash,ash}.*`: connects, judges its effect file, then ends UNVERIFIABLE by its
  own rule (no timestamped history hook, so no paired timing). It can not be a pass.
- `lab_no_helper_chip.{dash,ash}.*`: connects, then fails its `rec.log` check: dash / ash have no
  history hook, `rec.log` stays empty (TEST fault for these shells). Its 'shell mode' chip is also
  an open question for a session that is a plain login since #2120. Not changed here.

Neither proves #2126, so neither was added by name; `lab_nohelper_sh_login_profile` was.
