# r10 fakesshd: the four Git Bash managed-ssh failures of round 9 (#2007)

Round 9 (pipeline 3047937983, `app-test-windows`, pane `@gitbash`, terminal mode):
`remote_tab_no_helper`, `remote_tab_enter_no_helper`, `ssh_gitbash_managed` ("no remote shell
prompt after `ssh fake`"), `ssh_gitbash_typeahead_managed` ("the line typed during the connect
never reached the remote shell"). First run with #1933 / #1959 / #1960 / #1992.

Nothing here was compiled or run on Windows. The job was still running, so only the job log was
available (rows cut at about 760 characters; no artifacts, so no app log and no full `sshd.log`).

## Verdict

**TEST fault** in the Windows fake sshd (`script/realapp/appci/fakesshd_win.py`). An app fault
behind it cannot be excluded until the test host carries a managed session; the harness now
measures the test host at start, so the next run says which.

## What the log proves

1. **Nothing reset the session.** `sshd.log` ends with `session exec ... pty`, `conpty exec pid`,
   `conpty first output after 1.2-1.5 s: 292 bytes` (296 once) and then nothing: no
   `conpty output ended`, no `client input ended`. The session was open when the case gave up.
2. **The `WinError 10054 / 10053` tracebacks are not the session.** They are raised in paramiko's
   `_check_banner`, i.e. for a TCP connection closed before it sent an SSH banner. The harness
   makes those itself: `sshd.py:49` at start and `cases.py` `ssh_diag` (`socket.create_connection
   (...).close()`) once per failing row.
3. **The login, the LocalCommand and the mode byte worked.** ssh sends `exec` only after its
   LocalCommand returned, and the server logged the `exec`. The remote command did not become the
   plain login (`exec "$SHELL" -l`, `bash_body.sh:2493`): that shell reads the scratch rc and logs
   a prompt, and no prompt was logged in 60 s. So `dd` read `M` (`bash_body.sh:2477`) and the
   script went on to `exec -a bash bash --rcfile <(...)` (`bash_body.sh:2541`).
4. **The remote then sat at a bare prompt, never bootstrapped.** `ssh_connect_typing`,
   `ssh_enter_then_immediate_command` and `ssh_nested_typeahead` PASS in the same pane on the same
   managed path, at about 23 s a trial: the app's 15 s connect give-up (`kh_handover
   source=give_up remote_prompt_seen=false`, in the typeahead row) writes the typed line into the
   shell that is there, it runs, and those cases accept the effect file without a `rec.log` line
   (`ssh_trial_facts`, `effect_seen`). The four failing cases need a prompt or a command logged by
   the scratch rc, which only a bootstrapped (or plain login) shell sources.

## The step where it dies

The remote's two hooks never leave the test host whole:

- `bash_body.sh:2511`: the SSH hook, `ESC ] 9278 ; d ; <hex JSON> BEL`, about 470 bytes;
- `bash_body.sh:2553`: the rcfile's InitShell hook, same form, about 280 bytes.

They are written by an msys bash to the pseudo console `fakesshd_win.py:98` creates with
kernel32's `CreatePseudoConsole`, which on the test VM is the console host of Windows 10 19041.
Two properties meet there (from reading, not measured):

- the msys runtime's console layer collects an escape sequence in a 256-byte buffer and hands it
  to the console piece by piece when it is longer;
- that console host passes an unknown OSC through to the terminal side only from the write it
  ended in (the fix for sequences split across writes is from 2020 and is not in that build).

So of every OSC longer than 256 bytes only the last piece comes out. The byte counts fit: the
first output is the console's start-up frame plus the parts of BOTH hooks past byte 256, and it
is 4 bytes longer (292 -> 296) for one more digit of the remote session id, which is in both
hooks and hex-doubled in each. The app gets hex text and a BEL, no hook, and writes no bootstrap.

The bootstrap already avoids this for a LOCAL msys shell (key-value hooks, short start/end
markers around generator output: `bash_init_shell.sh`, `warp_send_hook_via_kv_pairs_*`). The
app's own panes do not go through the Windows 10 console host: the app ships `conpty.dll` /
`OpenConsole.exe`. A POSIX sshd gives the remote a pty that changes no byte. The fake host says
it is one (`SSH-2.0-OpenSSH_9.6 fakesshd`, #1992) and is not.

Linux and macOS pass the same cases because their `ssh fake` host is the system OpenSSH sshd
(`sshd.py` `start`, kind `openssh`; the paramiko fallback there forks a real pty).

## Fix (harness only)

`exec` + pty in the fake sshd gets the first of three arms that the machine can give, and the
harness MEASURES each arm at start instead of trusting this note:

1. **`msys`**: Git for Windows' own `sshd.exe` on a second loopback port as a pty host; the
   paramiko front relays the channel to it (`ptyhost.BackendPty`, paramiko client: pty request,
   exec, window changes, exit status). A real msys pty, byte-exact both ways, like the POSIX arm.
2. **`app`**: a pseudo console from the app's own `conpty.dll` (next to the app binary), the
   console host every app pane uses.
3. **`inbox`**: kernel32's pseudo console, as before.

At start `sshd.py` runs `ptyhost.probe` through the front server for each available arm: a
600-byte OSC out (must arrive whole) and a 4 KB counted raw read in (`head -c N | md5sum`, the
shape of the app's counted-read bootstrap). The first arm that passes both is written to
`ssh/exec_pty_arm`; the server reads it per request. The result is printed in the job log and
added to every `ssh_diag` (so to every failing ssh row). If no arm passes, the best one is used
and the rows say that the test host cannot carry a managed session.

Checked here: the appci self-test, and the relay plus the probe end to end on Linux (the front
server with its Windows calls stubbed, `linux/bin/fakesshd.py` as the pty host): the 610-byte
OSC arrives whole, the counted read is exact, exit status, resize and the client leaving work.

The `shell` request (plain ssh, every Windows ssh case that passes today, both panes) is not
touched. paramiko's own log no longer goes to `fakesshd.stderr`; a connection closed before its
banner is one `serve err` line in `sshd.log`. The stale `ssh_hint` ("the wrapper starts
`ControlMaster=yes -N -f`", untrue on msys since #1551) is corrected.

## Rejected

- Replacing the whole Windows test host with msys `sshd.exe` (as Linux uses the system sshd):
  it would move every passing Windows ssh case, in both panes, onto an untested server.
- Changing the app's remote hooks to short pieces: a real POSIX remote has no such limit, and
  the local leg (msys `ssh.exe` to the app's own console host) is the app's to prove, not the
  remote's to work around. The probe result decides whether there is anything to file there.
- Skipping the four cases on Windows: needs an owner decision; not needed if an arm passes.

## Risk

- Nothing was run on Windows. Every new step is guarded: a pty host that does not start, a DLL
  that does not load, a probe that throws, all leave the server on the next arm, at worst
  `inbox`, which is today's behaviour.
- The msys sshd runs as the CI user (pubkey for the scratch key only, loopback only,
  `ForceCommand` into the scratch remote home). Whether an unprivileged msys sshd accepts a
  login on the VM is not known; the probe answers it.
- With a working arm the three "passing" ssh typeahead cases of the Git Bash pane stop passing
  through the 15 s give-up and exercise the managed path for real. New failures there would be
  app findings (#1933 was never run), not harness ones.
- The local leg (msys `ssh.exe` writing the remote's long hooks to the app's own console host)
  is the same msys console layer in front of a different console host. Nothing had proved it
  carries a long sequence. The new PROVING case `gitbash_long_osc_reaches_app` measures it
  without a server (a bash script prints a 128-byte and a 480-byte `Clear` hook for unknown
  session ids; the app must log both refusals). If it fails with "APP side", no managed session
  from Git Bash can start whatever the server is: an app finding against #1933.

## Owner decisions

None needed for this change. One becomes due only if the next run reports that no arm passes:
either a Windows test host with a real sshd (an environment change on the VM), or skipping the
managed Git Bash cases on Windows.
