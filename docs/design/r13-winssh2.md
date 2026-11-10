# Round 13: the Git Bash ssh failures of round 12 on Windows (#2051)

Nothing here was compiled or run on Windows. Evidence: the round 12 artifacts (build/r12a, integ/r12 501a18469, job app-test-windows:
`run_slim/logs/<pass>/sshd.log`, `case_*.log`), and `test-results/2026-11-05_ssh_r18u28_windows`.

## 1. The seven ssh cases of the Git Bash pane: APP (design of #1933)

Cases: `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_nested_typeahead`, `remote_tab_no_helper`,
`remote_tab_enter_no_helper`, `ssh_gitbash_managed`, `ssh_gitbash_typeahead_managed` (+ `pane_not_local_after_*`), terminal and agent mode.
`ssh_gitbash_kill_switch` (plain ssh) and the PowerShell pane (plain ssh) pass.

### What works (so it is not the test host starting, #2043, or the hooks, #2007)

- `sshd.log`: `exec pty arm app`, `exec output carried 2 OSC 9278 sequence start(s)`: the SSH hook and InitShell left the test host whole.
- App log, `case_ssh_gitbash_managed.log` (terminal@gitbash): `hop=ssh_wrapper sync stage=auth`, `auth_done`, the token, `stage=managed`,
  `verdict managed=true`, mode byte `M` at 18:22:29.882. The whole #1933 hand-shake ran as designed.

### Where it dies

- 18:22:34.177: InitShell arrives; `hop=pty_write_bytes class=UserKey wrote=62000 of_job=62000`: the bash bootstrap, one write
  (`pty_controller.rs` `write_bootstrap_script_to_shell`, the plain `write_bytes` branch; `bootstrap.rs` `bootstrap_for_session`).
- 18:22:40: `Bootstrapping failed for shell "bash"` (`view.rs` bootstrap give-up). 18:22:44: `ssh typeahead: gave up waiting for an event
  (give_up_ceiling)`. In `ssh_connect_typing` the held line is then written behind the bootstrap (`give_up_keys_to_pty ... written=29`).
- The remote line editor goes on accepting bootstrap lines for 22 s (about 270 `decset_2004` pairs, 18:22:34 to 18:22:56), and the transfer is
  still running when the case stops after 60 s.
- `sshd.log`, same session: `10:23:20 client input so far: 65536 bytes in 57530 chunks, 65535 written to the pseudo console`;
  `10:23:29 client input ended: 78814 bytes in 69915 chunks, 78814 written`. Other sessions of the run: 20050 bytes in 19 s, 19680 in 19 s,
  32936 in 30 s.

Reading: the server writes every byte it gets at once (written equals received on every line, no `conpty input: writing ... took` line), and
what it gets is about one byte per packet at 1.1 to 1.5 KB/s. An ssh client sends what its read of stdin returned, so the slow part is in
front of the network: msys `ssh.exe` reading its console.

That is a known property of msys programs on a Windows console, not of the app's console host: round r18u28 (`remote_heredoc_paste`,
Git Bash pane, PLAIN ssh session) logged `65536 bytes in 46333 chunks` per about 41 s for a paste, and its control (reference conhost, same
`ssh.exe`, no app) did not deliver a 142 KB paste in 120 s either; `big_paste`'s control showed the same for msys `cat`.

So: `docs/design/1933-windows-managed-ssh.md` 3.4 ("the app writes the bootstrap ... through the one pty") cannot work. 62 KB at that rate is
45 to 60 s on any server. Before #1933 the same ssh was plain and usable; with it a Git Bash ssh to a POSIX OpenSSH server sits for about a
minute echoing bootstrap text while the app has already given up.

Not explained: the server received more bytes (78814 and still coming) than the app wrote (62007: the mode byte, one device-attributes answer, the bootstrap). Plain sessions in r18u28 received what
was pasted plus 60 to 70 bytes. The test sshd now logs what the client input consists of (see 3).

### Proposed fix (not implemented: see "Why not implemented here")

The bootstrap must not be console input on this path. The remote command is an ssh ARGUMENT (the exec request), which never touches the
console:

1. The local Git Bash keeps the text of the bash bootstrap for a remote (what `bootstrap_for_session` would send: `bash.sh` with the
   local-only block of `bash_body.sh` cut). The script is static but for `@@USING_CON_PTY_BOOLEAN@@`. Simplest source: the app writes it once
   per msys bash session next to the bootstrap file it already writes and exports the path; the wrapper reads it when it builds the command.
2. On the one-connection path the remote command carries it as a here-document on a spare fd, opened at the top level of the `bash)` branch
   (not inside `<( )`: bash 3.2 counts parentheses in there), and the rcfile becomes `<(echo '<today's snippet>'; command -p cat <&9)`: bash
   runs the bootstrap as the rest of its rcfile, right after it printed InitShell. Same script, sourced instead of typed, as a local Git Bash
   already gets it (`source_bootstrap_script`).
3. The SSH hook says `"bootstrap_in_command": true`; `SSHValue` / `IsLegacySSHSession::Yes` carry it; for such a session
   `write_bootstrap_script_to_shell` writes nothing. Every later event (Bootstrapped, first Precmd, hand-over of the held keys) is unchanged.
4. Guards, all falling back to plain ssh before ssh starts (`PlainSSHFallback`, as every other local fallback of #1933): no bootstrap text;
   `ssh` is not the msys one (a Win32 `ssh.exe` gets its arguments through a 32,767-character command line); the command over 100 KB
   (Linux passes one argument of at most 128 KB to `sh -c`).

Nothing changes for Linux and macOS: the new text is only added under `warp_single -eq 1`, the hook field defaults to false.

Rejected:
- Raising the two nets: a managed session that needs a minute is worse than a plain one.
- Sending less (compress + base64 in the typed stream): 15 to 20 KB is still 10 to 15 s, and needs `gzip` / `base64` on the remote.
- A second connection for the bootstrap: a second login (the reason #1933 is one connection).
- stdin of ssh from a pipe fed by a relay: ssh then does not own the console (raw mode, Ctrl-C, window size) and the relay reads the console
  as slowly for everything typed later.
- The Windows `ssh.exe` instead of Git's: another client, other config, agent and known_hosts rules.

### Why not implemented here

- It changes the hook protocol and the remote command of every one-connection session, and none of it can be run before the next Windows
  build; the first design of this path (#1933) was written the same way and round 12 is what it cost. Owner rule: design review before code;
  second fix on a path: stop and design.
- It needs two owner decisions (below).

### Owner decisions

1. **A remote whose login shell is zsh.** The wrapper cannot know the remote shell before it connects, and both bootstraps do not fit one
   argument. Either the one-connection path manages bash logins only and gives a zsh login a plain login shell (as before #1933), or the
   bash and zsh texts go compressed (needs `gzip` and `base64` on the remote, plain login when they are missing).
2. **Until the fix is built**: Git Bash ssh stays managed by default and takes about a minute to become usable on every POSIX OpenSSH server,
   or `PHOSPHOR_MSYS_MANAGED_SSH` defaults to off (plain ssh, what v0.1.17 does) until then. This reverses the 2026-11-09 default for a
   while, so it is the owner's.

## 2. `subshell_ready_rc_clear`, agent@gitbash, "3.75 s after its Enter (budget 2 s)": the shell's setup time, not an app wait

App log (`run_slim/logs/agent@gitbash/case_subshell_ready_rc_clear.log`), ages from the first setup write:

| event | time | age |
|---|---|---|
| `subshell_ready_judged source=armed decision=Write(None) ... tail=ShellPrompt line_editor_announced=true`, setup line 1 written | 18:51:55.230 | 0 |
| keys of the typed line: `Hold(SubshellBootstrapWrite)` | from 18:51:55.239 | |
| the line's Enter | 18:51:55.652 | 422 ms |
| `InitSubshell` (`slow pty echo ... 473 ms` for the probe line) | 18:51:56.419 | 1189 ms |
| stage 2 written, three chunks, each echoed after about 450 ms | 18:51:56.437 to 57.430 | |
| `InitShell`, ` source '<bootstrap file>'` written | 18:51:58.009 | 2778 ms |
| `Bootstrapped` | 18:51:58.973 | 3742 ms |

No judge wait (the decision is at age 0), no net, no timer: every step follows the event before it within a millisecond or two, and the time
between events is the inner msys bash running each line (a pipeline of forked programs per line). Sourcing the bootstrap is 0.96 s of the
3.74 s.

terminal@gitbash passed because the race against `BANNER_S` (1.6 s) went the other way: at Ctrl-I the inner prompt was not drawn yet
(`decision=Wait(Unknown) ... tail=None line_editor_announced=false`), the typed keys went to the pty live (`hand_route reason=unheld`), the
line ran in the inner shell at once and the setup was written after it. Same app, same machine, two orders of the same two events.

So the 3.75 s is Git Bash on that machine, in the order "setup first". The budget is not changed (the case still fails whenever that order
happens there): whether more than 2 s for a Phosphorize in Git Bash is acceptable, or the setup should cost fewer round trips on msys, is not a
harness question. The row now says where the time went (`setup_timeline`).

## 3. Harness changes in this branch (TEST side, no expectation changed)

- `fakesshd_win.py`, `app` arm: the console host's own start-up device-attributes request is answered by the test sshd and not passed on.
  Evidence that it is the console host's: `conpty first output after 0.01 s: 4 bytes` before any program output, on that arm only (`inbox`:
  58 bytes after 0.16 s); the app answered `ESC [ ? 62 c` 47 ms after it saw the 4 bytes. Evidence that it costs time: the start-up probe
  (whose client answers nothing) took 5 s on `app` and 1 s on `inbox`; in the session the hooks came 4.3 s after the mode byte. Inferred, not
  measured: that the console host waits about 3 s for the answer, and that the app's answer does not get there in time through the local
  console. A POSIX pty asks nothing, so the stand-in should not either. Risk: if the 4 bytes are not what they look like, nothing matches and
  nothing changes; the first-output line now shows them in hex.
- `client input ended` carries duration, rate and composition of the client's bytes.
- `subshell_ready_rc_clear` notes the setup's timeline.

Risk of the whole change: Python only, selftest `fakesshd_app_arm_da1_and_input_stats_2051`; `fakesshd_win.py` cannot be imported off
Windows, so its edits are checked by `compile` and by needles.
