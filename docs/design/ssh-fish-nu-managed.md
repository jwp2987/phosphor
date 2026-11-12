# Managed ssh sessions for fish and nu login shells (#2019 follow-up)

Owner decision 2026-11-12: a host whose login shell is fish or nu gets a MANAGED (bootstrapped) session like bash and
zsh, not the plain fallback #2019 introduced (`DECLINED.md`, #2019 entry; #2120, #2126, #2160). Second change on this
path, so this note records what was found before the change; the change is in the same commit series.

## 1. How the wrapper decided before

- `warp_ssh_helper` (`bash_body.sh`, `zsh_body.sh`, `fish.sh`) starts a Phosphor-owned ControlMaster, then asks
  `warp_ssh_remote_is_non_posix_cached` (`bash_body.sh` ~3293, `zsh_body.sh` ~1831, `fish.sh` ~1346). The probe
  (`warp_ssh_remote_is_non_posix`) sends `echo warp-probe-%COMSPEC%-$PSVersionTable`; only cmd.exe and PowerShell
  answer definitively. fish and nu answer nothing usable, so (#2019) the probe asks one more question,
  `sh -c 'echo warp-login-$SHELL-'`, and for fish, nu, csh, tcsh, xonsh, elvish returned "non-POSIX" (0), the
  verdict `nonposix`, cached 10 minutes per `ssh -G` destination.
- A 0 sends `PlainSSHFallback` and runs `ssh -o ControlMaster=no -o ControlPath=... -t <args>` with no remote command.
  The app logs `verdict managed=false`; the keys are ssh's.
- A 1 builds the remote command: POSIX text, one ssh argument, that sshd gives to `<login shell> -c`. It prints the
  `SSH` hook and ends in `case ${SHELL##*/} in bash) exec -a bash "${BASH:-$SHELL}" --rcfile <(echo '...') ... ;;
  zsh) ... exec -l zsh ...`. bash and zsh bootstrap from InitShell: the app writes the script (bash: counted read,
  `raw_bootstrap`). fish and nu never got that far: they parse the whole text first and refuse `warp_rb=false`
  and `${SHELL##*/}`.

## 2. The local fish and nu bootstraps, and what can carry them

- `fish.sh` (83 KB raw) and `nu.nu` (23 KB) set up every hook the app needs (Precmd, Preexec, CommandFinished,
  InputBuffer, Bootstrapped, cwd listing, input reporting). InitShell comes from a separate small file
  (`fish_init_shell.sh`, `nu_init_shell.nu`) that a local shell runs first; the bootstrap is then SOURCED from a
  file (`source '<tmp>'`) because nu cannot evaluate text. `fish.sh` skips its ssh wrapper unless
  `WARP_IS_LOCAL_SHELL_SESSION=1`; `nu.nu` has none. Both tests that guard them
  (`script/test_fish_bootstrap`, `test_nu_bootstrap_hook_equivalence`, `test_nu_agent_pager_env`) drive the file form.
- The remote command is ONE ssh argument parsed by the login shell, at most ~128 KiB. Carrying 60-80 KB of fish
  text in it would need a quoting that sh, fish and nu read alike (single quotes: fish treats `\\` and `\'` as
  escapes, nu none; a quote cannot occur at all). Not worth it: the app already writes bootstraps over the pty.
  `fish --init-command` / `nu --execute` take a small string; the size goes through the tty instead.
- A remote nu or fish cannot be fed a here-document. What both can do is `source <file>`. So the file is written on
  the remote by `sh`.

## 3. What on the host assumes a POSIX shell

| feature | verdict |
|---|---|
| shell backend scripts (`crates/remote_server/src/shellcmd/*`, `backend/shell/*`) | already shell-agnostic: `ShellKind::wrap` (`shell_kind.rs`) puts every script into `sh -c '<fish-quoted>'` for Fish and `^sh -c r#'...'#` for Nu (with a legacy double-quoted retry); the kind comes from the session's shell (`command_executor.rs::shell_kind_for_remote`) |
| helper install / launch (`setup.rs`) | already `sh -c '...'`; the upload itself is `ssh ... "bash -s"` (`ssh.rs`), a plain command that any login shell runs; needs `bash` on the host |
| side calls (git status, file tree, index), agent `run_shell_command` | through the shell backend or the helper: as above |
| cwd listing, completions | in-shell: `fish.sh` / `nu.nu` bootstraps (the local ones, unchanged) |
| ExitShell (tears down `remote-server-proxy`) | needs a hook when the remote shell exits: bash traps EXIT, fish and nu have no usable equivalent, so the shim stays the parent and prints it |
| ssh escape / drop handling | in the app, keyed off hooks, not the shell: unchanged |
| the `SSH` hook rule (`input_queue::ssh_remote_command_bootstraps`) | needed a change: fish and nu now announce a session |

## 4. Design as built

1. Probe: unchanged questions. For `fish` / `nu` the verdict is cached as `nonposix-fish` / `nonposix-nu` and the
   probe still returns "non-POSIX" (0), so no other caller changes; it also sets `warp_ssh_login_kind`.
2. Choose: at the one place that used to send `PlainSSHFallback`, `warp_ssh_login_kind` set and
   `PHOSPHOR_SSH_FISH_NU_MANAGED` not `0` builds the command with `warp_ssh_fishnu_command` and runs the managed ssh
   (`ManagedSSHSessionStarting` first, master stopped afterwards, as for bash and zsh).
3. Deliver: the command is `sh -c '<shim>' sh '<kind>' '<local sid>' '<remote sid>' '<socket>' '<external>' '<ver>' '<proto>'`.
   Every argument is single-quoted and the shim holds no quote, no doubled backslash and no newline, so sh, fish and
   nu read the same text (checked under real bash, sh and fish by `script/test_ssh_fish_nu_managed`; nu is
   checked where a nu is installed). The shim is one line, identical in the three wrappers (unit test).
   The shim runs in `sh`: makes a temp dir (else `PlainSSHFallback` and a plain login shell), prints the `SSH` hook
   (`remote_shell` fish|nu, `bootstrap_in_command` false) and the MotD, raw mode, prints `InitShell`, then reads a
   counted bootstrap from the tty (`<bytes>\n` then the bytes; 20 s give-up net, a short read is the plain
   fallback), restores the tty, and runs `"$SHELL" -l -C 'source F; rm' ` (fish) or `-l -e 'source F; ^rm'` (nu). It
   stays the parent so it can print `ExitShell` when the shell ends.
4. App: `ssh_remote_command_bootstraps` is true for fish and nu, so the model keeps the pending session, the gate
   and the phase machine call it managed. On InitShell `staged_ssh_bootstrap_applies` (legacy ssh session, fish or
   nu, not in-command, not a subshell) makes `initialize_shell` write `staged_ssh_bootstrap_payload`: the byte
   count line, then `fish.sh` / `nu.nu` (nu: after `$env.config.show_banner = false`). Hop `bootstrap_staged`.
5. Kill switch: `PHOSPHOR_SSH_FISH_NU_MANAGED=0` in the pane's shell restores the #2019 plain session exactly.
6. Lab: the #2160 gates read the connect's own log, so a managed fish / nu connect is judged in full and the gates
   fire again only for a plain (kill-switch) connect; a managed connect without `hop=bootstrap_staged` fails its
   row (`cases_lab.managed_fish_nu_evidence`).

## Risks

- nu: flags `-l`, `-e`, `source <literal path>` are those of 0.116 (the lab image). `script/test_ssh_fish_nu_managed`
  skips the nu rows where no nu is installed; no nu was available when this was written, so the nu path is
  UNPROVEN until a lab run or a machine with nu runs it. fish was run here (4.2.1) under fish, bash and sh as login
  parsers. Old fish (< 3.0, no `--init-command` spelling `-C` is old, ok) is not covered.
- The shim stays alive as the parent: a SIGKILL of it leaves the shell without an ExitShell (the same as a bash
  killed by SIGKILL).
- A host without `mktemp`, `head -c`, `stty`, `wc` or `od` falls back to the plain session by itself.
- Phases: fish first (proven here), nu second (same shim; only the last `case` arm differs). Both ship together
  because the shim and the app rule are shared; the kill switch covers either.
