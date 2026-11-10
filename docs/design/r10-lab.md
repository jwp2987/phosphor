# Round 10: the lab's first run (job 17885063245)

The fix-tier lab job (`app-test-lab-linux`, build `413cd4241` = merge/w0 without the round 10
branches) ran the PROVING lab cases for the first time: 36 judged rows, 2 PASS, 21 FAIL, 13
NOREF. This note groups the 21 FAIL rows by root cause, gives a verdict per cause with the
first step that failed, and says what was changed. Nothing here was compiled or run against a
build: every statement about the next run is a prediction.

Line numbers are those of `integ/r10` `a77c86143` (the base of `fix/r10-lab`) unless a commit of
this branch is named. Logs are the job's artifacts (`run_slim/logs/{terminal,agent}/case_<name>.log`).

| # | cause | verdict | FAIL rows | issue |
|---|---|---|---|---|
| A | a fish or nu LOGIN shell refuses the ssh wrapper's POSIX bootstrap | APP | 8 (`*.fish.*.nohelper`, `*.nu.*.nohelper`) | #2019 |
| B | the lab's fish hook logs no prompt (hidden behind A) | TEST | 0 of its own (would keep the 4 fish rows red after A) | #2021 |
| C | `lab_prepare` leaves `ctx.remote` on the lab arm when it returns early | TEST | 8 (`harness_state_reset_after_*.helper`) | #2021 |
| D | no helper binary is provided to the job | TEST / CI | 0 FAIL, 8 NOREF (the trigger of C) | #2021, owner decision |
| E | `rec.log` is asked whether an agent command reached the host | TEST | 3 (`lab_agent_remote_flow.bash`, `lab_agent_remote_drop.bash`, `pane_not_local_after_...drop.bash`) | #2021 |
| F | a refused delete is required to log a write route | TEST | 1 (`lab_fam_agent_delete_conflict.bash`) | #2021 |
| G | the remote diff read is LF-normalised | APP | 1 (`lab_fam_agent_edit_endings.bash`) | #2020 |

8 + 8 + 3 + 1 + 1 = 21.

## A. fish / nu login shell: the managed ssh session ends at once (APP)

**First failing step.** `connect_or_fail` (`cases_lab.py:179`): "no remote prompt after `ssh
lab-u_nu`". App log of `lab_fam_agent_context.nu.lan.nohelper`, 06:40:58:

```
hop=ssh_wrapper announced reclaimed=false holding=false held_bytes=0
Received ManagedSSHSessionStarting hook
hop=ssh_wrapper verdict managed=true tty_owned_deferred=false     (t=...858747)
Received CommandFinished hook                                     (30 ms later)
hop=ssh_gate_released how=failed
```

Lab sshd: `Starting session: command for u_nu` (the wrapper's probe), then `Starting session:
command on pts/0 for u_nu` and `Close session` straight after. The fish rows are the same.

**Cause.** `warp_ssh_helper` (`bash_body.sh:2211`, the same text in `zsh_body.sh` and `fish.sh`)
sends a POSIX sh script as the remote command (`bash_body.sh:2512-2597`, `command ssh ... -t "$@"
"$warp_remote_bootstrap"`). sshd runs it as `<login shell> -c <script>`. fish and nu parse the
whole text before running anything, and `warp_rb=false` (`:2523`) / `${SHELL##*/}` are syntax
errors there. Checked with the installed fish 4.2.1 on a cut-down script: `fish: Unsupported use
of '='`, exit 127, nothing printed by the script; dash runs the same text and reaches the "not
bash or zsh" branch (`:2528-2552`), which is the branch the authors meant fish to take.

The wrapper's probe (`warp_ssh_remote_is_non_posix`, `:2805`, `echo
warp-probe-%COMSPEC%-$PSVersionTable`) cannot tell: fish prints an empty line (an unset variable
makes the whole word vanish; checked), nu reports a parse error, and both count as "inconclusive
= managed session as before" (`:2803`).

The text is the same at the upstream pin, so this is upstream's defect, not a regression. No
fish or nu remote login shell had been run before: the same-machine test sshd uses a forced
command (`force.sh`), which hides the login shell.

**Chosen fix.** In the three wrappers, when the first answer is not definitive and the probe
ended by itself, ask ONE more question over the same master: `sh -c 'echo warp-login-$SHELL-'`.
It is one single-quoted word list with no backslash, quote or newline inside, so sh, bash, zsh,
fish, nu, csh, tcsh, xonsh and elvish hand the same text to `sh`, and sshd has exported `SHELL`
(the passwd entry) to the session. A login shell named fish, nu, csh, tcsh, xonsh or elvish is
answered like cmd.exe (#1035): `PlainSSHFallback`, then the plain interactive session over the
same master. bash / zsh (an rc file printed in front of the first answer) stay managed and the
answer is cached; anything else is inconclusive as before. A first probe that the 5 s net killed
(bash / zsh: also one whose ssh returned 255) is not followed by the second question.

Why the plain session: it is an existing, tested path with an event that releases held keys
(ssh owns the tty), which is the owner's rule for ssh typeahead. Cost: one more exec channel on
the master per connect to such a host, only when the first answer is not definitive, cached for
ten minutes like the first probe. POSIX and Windows hosts are not touched (one probe, as today).

**Rejected.**
- *Run the POSIX script under `sh` on the remote (`exec sh -c '<script>'`) so fish / nu get the
  managed session.* The script would reach `exec "$SHELL"` and the app would see the SSH hook,
  but the remote shell is never bootstrapped, and in a managed window the ownership edge is
  deferred for good (`view.rs` `ssh_on_wrapper_verdict`, `SshConnectGate::should_release`): typed
  keys would then depend on the tty-read probe (Linux) or the 15 s give-up (elsewhere). That is a
  design of its own, and quoting the script for fish / nu / csh differs per shell. Left as an
  owner question (below).
- *Treat every inconclusive answer as non-POSIX.* A bash host with a noisy rc file is
  inconclusive too and would lose its managed session.
- *Change the first probe.* cmd.exe and PowerShell detection rest on its exact text (#1035).

**Risk.** The decision acts only on a positive `warp-login-<path>-` line that names one of six
shells, all of which refuse the bootstrap today, so no working session changes. nu could not be
run here (not installed; running the lab's pinned release was not permitted): that
`sh -c '...'` is an external command with a raw single-quoted argument in nu is from its syntax,
not from a run. csh / tcsh / xonsh / elvish likewise.

**Owner question.** fish / nu login hosts now get a working plain session and no remote features
(agent tools on the host, remote completions). Whether they should get a managed session is a
product decision plus a design (see Rejected). Until then the agent-mode `nu` arms
(`lab_agent_remote_*`, `lab_fam_agent_edit_conflict.nu`) get past the connect and are expected to
fail on their first remote tool call.

## B. The lab's fish hook logs no prompt (TEST)

`connect` (`cases_caseremote.py:45`) waits for a new `P` line in the remote `prompt.log`. The
lab's fish hook is the local scratch config `FISH` (`profile.py:121-126`), which logs commands to
`fish_rec.log` and no prompt: locally fish runs inside the bash pane and shares its ctl dir. In
the lab fish is the login shell, so no fish arm could ever have connected, with or without A.
Fix: `lab_hooks()` appends `FISH_LAB_TAIL` (an `--on-event fish_prompt` function that logs `P`);
`FISH` itself is unchanged. The hooks are pushed at lease time, so the lab image is not rebuilt.

Not changed: `SH_HOOK` (dash / ash, `profile.py:151`) logs no prompt either. Those arms are
outside the quick matrix; a PS1 with a command substitution would do it, but it has to be proven
in the lab first.

## C. `lab_prepare` leaves the remote swapped on an early return (TEST)

`lab_prepare` sets `ctx.remote = ctx.prof.remote = LabRemote(...)` (`cases_lab.py:113-116`) and
then leaves at the helper-binary gate (`:118-121`), the RTT gate (`:144-150`) or a skip
(`:141`). `_runner` (`:38-47`) called `lab_cleanup` only in a `finally` around the case body.
All eight `.helper` arms left at `:120`; the runner's post-case check (`run.py:744-751`) found
"the remote side still pointed at a lab arm" and, because the case had not failed, reported
`harness_state_reset_after_<case>` FAIL. Fix: the `try` covers `lab_prepare`.

## D. No helper binary in the job (TEST / CI, owner decision)

`APPCI_LAB_HELPER_BIN` is set nowhere in `.gitlab-ci.yml`; `build-remote-helper` (`:1768`) is a
release job and the dev build (`build-linux-x86_64`, `:279`) does not build `phosphor-remote`.
`docs/testing.md` lists it as an open setup item (2.15 checklist, item 6). So every `.helper`
arm is UNVERIFIABLE (NOREF) by construction, in this run and in the next.

Not changed here. The helper has to come from the same tree as the app (the file name carries
the version) and has to run inside the lab container (debian trixie), so the right source is a
static musl build in the dev build job, handed to the lab job with the build. That adds a
compile to every dev build and needs the musl target and zig on the box: an owner / coordinator
decision, not a harness edit.

## E. `rec.log` cannot see an agent command (TEST)

`lab_agent_remote_flow.bash`: "the approved `cat` reached the remote shell 0 times".
`lab_agent_remote_drop.bash`: "the approved command never reached the remote shell", then the
case's `ctx.reset()` leaves `sleep 300` running on the host (`pane_not_local_after_...`).

The command did reach the host. Flow, 06:44:49: `hop=agent_action ... kind=run_shell_command
status=running`, `pty_write bytes=2043`, `status=done`, and the model's tool result held the
edited content (that check passed). Drop, 06:48:24: `pty_write bytes=2041`, `Received Preexec
hook`; `status=cancelled` at 06:48:54 when the case gave up.

The bash hook logs `history 1` from `PROMPT_COMMAND` (`profile.py:31`): (1) the agent types its
command in a wrapper that bash keeps out of the history on purpose (`HISTIGNORE=
...*__phosphor_ap_*`, `bash_body.sh:3156`, #904), so it is never logged; (2) the log is written
at the NEXT prompt, so a command that is still running is never there.

Fix: the approved command's own effect on the host is the oracle. Flow: `cat <file> |
/usr/bin/tee -a <ran file>`; exactly one copy of the edited file = it ran exactly once (the
`rec.log` count stays as "at most once" for the shells whose hook does log it). Drop:
`/usr/bin/touch <marker> && sleep 300 && echo fin_<tok>`; the marker on the host = it started
(`LabRemote.exists`, an empty file is not a missing one). No quote, `&&` or redirection in the
flow command, so nu and pwsh run it as written; the nu drop command uses `;` and external paths
(nu has no `&&`, its own `sleep` wants a duration), unproven in nu.

## F. A refused delete has no write route (TEST)

`lab_fam_agent_delete_conflict.bash`: the delete was refused and the file kept (those checks
passed); the only failed check was `check_write_route`: "no hop=backend_route family=file_write
line". App log, 06:53:23: `hop=backend_route family=fs_basic backend=shell`, then `Failed to
save file for accepted AgentMode diffs`. The accept re-reads the host file
(`warp_files/src/lib.rs` `delete_if_unchanged`, `read_file_bytes`), finds the user's bytes and
stops; no delete is sent, so no `file_write` line can exist. The check was copied from the edit
cases, where an accepted edit comes first. Fix: the delete case requires the route of that
re-read after the accept (`family=fs_basic` on the arm's backend; a `file_write` line, if the
host-side guard was the one to refuse, must be on the arm too).

## G. Remote agent edits lose the file's line endings (APP)

`lab_fam_agent_edit_endings.bash.lan.nohelper`; the local `agent_edit_endings` wrote the right
bytes in the same pipeline.

- `crlf`, `crlf_no_eol`: file unchanged. 06:53:50 `hop=backend_route family=fs_basic
  backend=shell`, `Failed to save file for accepted AgentMode diffs`, `status=failed`.
- `mixed`, `mixed_no_eol`: `family=file_write` logged, `status=done`, and the host file has 0 CRLF
  + 10 LF lines where 7 + 3 were expected.
- `lf_no_eol`: passed.

**Cause.** `apply_diff_model.rs:99` reads a local file with `std::fs::read_to_string`;
`read_remote_file` (`:119-131`) reads a remote one with `ReadFileContext`, whose text is
LF-normalised by design (`remote_server/src/shellcmd/read.rs:289`, `strip_suffix('\r')`, shared
by the helper and the shell backend). On a remote host the diff base, the buffer and the
recorded original are therefore LF. At accept the guard reads the real bytes
(`warp_files/src/lib.rs:1406`): a CRLF file is "same text, endings converted" and refused
(`compare_pre_image`, `:252-264`); a mixed file passes through the guard's stated blind spot
(`line_endings_preserved`, `:218-222`) and is written as LF. `accepted_update_text` (#1925 /
#1956) applies the deltas to an original that is already LF.

**Chosen fix.** After a successful `ReadFileContext` read, `read_remote_file` also reads the
bytes (`read_file_bytes`, family `fs_basic`) and uses them when they are valid UTF-8 and
normalise (the same per-line rule as `read_text`) to exactly the text that was returned;
otherwise the normalised text, as before. The rest of the path is the local one.

**Rejected.** *Read the bytes instead of `ReadFileContext`*: the missing-file / binary /
too-large answers come from `ReadFileContext` and a raw read of a huge file is unbounded; the
bytes are only fetched after the capped read succeeded. *A line-ending field in the proto*: a
protocol and helper-capability change for what one existing call answers.

**Risk and cost.** The raw text is used only when it is provably the same file, so the worst
case is today's behaviour. One more side call per file the agent proposes to edit on a remote
host. Only the no-helper arm ran; the helper arm shares the read and is expected to gain the
same.

## What the next Linux lab run should show

With this branch and no helper binary (D open):

- no `harness_state_reset_after_*` row;
- `*.fish.*.nohelper` and `*.nu.*.nohelper`: the connect step passes; app log `Received
  PlainSSHFallback hook` and no `ManagedSSHSessionStarting` for those hosts; lab sshd shows two
  `Starting session: command for u_<shell>` lines (the two probes) and a `shell on pts/N`
  session. Terminal `lab_fam_agent_context.{fish,nu}` then ends as bash / zsh do (NOREF, no
  scripted driver). The agent-mode `nu` arms are expected to FAIL at their first remote tool
  call (owner question in A);
- `lab_agent_remote_flow.bash` and `lab_agent_remote_drop.bash`: judged on the effect file / the
  start marker; no `pane_not_local_after_*` row;
- `lab_fam_agent_delete_conflict.bash`: passes on the `fs_basic` route;
- `lab_fam_agent_edit_endings.bash`: all five shapes byte-exact, a `family=fs_basic` line next
  to each `family=file_context` line at proposal time;
- the eight `.helper` rows stay NOREF until D is decided.
