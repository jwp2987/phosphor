# r19-winssh: the Windows Git Bash managed-ssh group of round 14

Round 14 candidate: build/r14a `42ef67839`, pipeline 3052606915, job app-test-windows 17921510251.
Scope: `ssh_gitbash_managed`, `ssh_gitbash_bootstrap_in_command`, `ssh_gitbash_typeahead_managed`, `ssh_connect_typing_wrapper`,
`ssh_nested_typeahead_wrapper`, `ssh_failing_not_local`, `remote_tab_no_helper`, `remote_tab_enter_no_helper`, their
`pane_not_local_after_*` rows, and `gitbash_typeahead_reps`. Nothing was compiled; nothing ran on Windows.

## What evidence exists

The job's `after_script` hit its time limit, so `run_slim/` (app log slices, `sshd.log`, screenshots) was never made or uploaded
(trace: `WARNING: appci-out/windows/run_slim/logs/: no matching files`). What exists: `results.json` (the rows, with the `sshd.log`
and app-hop lines the cases quote), `inject.log`, and round 13's full logs of the same test host. Every verdict below says which of
these it rests on. Where the app log would have been needed, the verdict says so.

The group fails in terminal@gitbash as well as agent@gitbash (the brief named agent@gitbash only).

## 1. The managed sessions: TEST HOST

### Symptom

Seven cases, both modes: `ssh fake` gives a remote prompt (`connect` returns, 8.1 to 8.5 s), then nothing typed runs remotely:
"not provably in the REMOTE shell (probe ran in: none)", "the line never reached the remote shell".

### Cause

The Windows test sshd (`fakesshd_win.py`) serves an exec with a pty, which is what a managed session is, on its `app` arm: a pseudo
console of the app's own `conpty.dll` (#2007). That console host talks to *its terminal* before any program runs. The sshd passed it
all on to the ssh client as if it were program output.

Evidence, round 14 `results.json`:

- `ssh_gitbash_typeahead_managed`, sshd.log: `conpty first output after 0.01 s: 4 bytes (starts 1b5b3174)` (`ESC [ 1 t`), and for
  the session that typed one line and its Enter: `client input ended: 89 bytes in 84 chunks ... ESC 5, CR 0, LF 0 ...; starts
  b'M\x1b[?62cecho T408_m8bgnr\x1b[17;29;0;1;8;1_\x1b[13;28;1'`.
  - `M` is the mode byte, `ESC [ ? 62 c` the app's answer to a DA1 request it was sent.
  - `ESC [ 17;29;0;1;8;1 _` is a win32-input-mode key record: Ctrl down. `ESC [ 13;28;1...` is Return. Five ESC = the DA1 answer and
    four records (Ctrl down, Return down, Return up, Ctrl up): the one Enter of the line, sent as key records. No CR, no LF.
- `ssh_nested_typeahead_wrapper` (agent@gitbash), the remote shell's own record of the line it finally got:
  `62cssh fake29;0;1;8;1_;28;10;1;8;1_;28;10;0;8;1_29;0;0;0;1_echo T178_mohi6h >> ~/rn.txt`. That is the DA1 answer and the four
  records with what readline swallowed of each removed. The Return record carries character 10.

Round 13 logs (`run_slim/logs/terminal@gitbash/sshd.log`), same arm, the bootstrap still typed: `75934 bytes in 62384 chunks ... ESC
3000, CR 0, LF 0` for a 62007-byte write. Every newline of the bootstrap arrived as four records. This is the "server received more
bytes than the app wrote" that r13-winssh2 left unexplained. A plain shell session of the same run (served by Windows' own pseudo
console): `ESC 0, CR 3, LF 0`.

Chain: the console host behind the `app` arm asks its terminal for win32-input-mode (`ESC [ ? 9001 h`). The terminal at the other
end of the ssh connection is the console host around msys `ssh.exe` in the app's pane. It honours the request, as it would for any
program in the pane, and hands ssh every control key as a key record. ssh sends the records as bytes. The remote console host gives
them to the remote bash as text. No Enter arrives, so no probe line and no typed line runs.

The request itself is not in the quoted log lines (the log shows 16 bytes of the first read only). That it is the test host's
console that asks is inferred from: key records appear only on `app` arm sessions; plain ssh in the same pane, same app, same
`ssh.exe`, passes (`ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_nested_typeahead`, `ssh_gitbash_kill_switch`: PASS
in both modes); the start-up probe of the arm, whose client is not a console, measured it byte-exact.

The DA1 request is a second fault of the same kind: the app answers it and the answer lands at the remote prompt as `62c` in front of
the first line. #2051's harness change (`strip_startup_da1`) was meant to stop that. It looked only at the head of the first read,
which is `ESC [ 1 t`, so it never matched (its own note predicted this: "if the 4 bytes are not `ESC [ c` nothing changes").

A POSIX sshd's pty says none of this to its client, and POSIX OpenSSH servers are the only ones the wrapper manages (#1992).

### What round 14 does show about #2051

`ssh_gitbash_bootstrap_in_command` metrics, both modes: `in_command: true, max_write: 24 / 25, failed: false`, connect 8.29 / 8.41 s.
The app logged `hop=bootstrap_in_command ... wrote=0`, wrote nothing bootstrap-sized, logged no `Bootstrapping failed`, and the
remote prompt came. That is the part of #2051 the run could reach: worked. Whether the session is usable was not judged.

### Fix (harness)

`ptyhost.ConsoleHostFilter`, used for the `app` arm only: removes the DA1 request (the sshd answers it), `ESC [ ? 9001 h/l` and
`ESC [ ? 1004 h/l` anywhere in the output, `ESC [ 1 t` before the first other byte; a sequence cut by a read boundary is held for the
next read. `sshd.log` names what was removed. `InputStats` counts win32-input key records in what the client sends, so the next run
can refute this verdict: if records still arrive on a managed session, the mode has another source.

How the cases can still fail: nothing in them changed except the wording below. They still need the remote shell to answer a probe
and run the typed line exactly once.

Rejected:

- Serve managed sessions on the `inbox` arm: Windows 10's pseudo console cuts the 470-byte hooks (#2007), which is why `app` exists.
- Make the app refuse win32-input-mode from a program in the pane: that is the local console host's behaviour, not the app's code,
  and it is what makes ssh to a real Windows server work.
- Mark the cases NOREF on this host: they are the only proof of #2051 and #1933.

Risk: the filter drops focus-event mode and a DA1 request for the whole session, so a remote program that wants either does not get
it through this arm. No case on this arm uses them. If the console host sends a request this filter does not know, it still leaks;
the `starts <hex>` line and the key-record count show it.

Not explained: why the remote console host hands the records to the remote bash as text instead of keys. It does not matter for the
fix (with the request removed no record is sent).

## 2. "probe ran in: remote" on a row judged not remote: TEST (wording)

`if ctx.where_now() != "remote": r.check(False, "... (probe ran in: %s)" % ctx.where_now())`: the message typed a *second* probe
and printed its answer. agent@gitbash `remote_tab_enter_no_helper` and `ssh_gitbash_bootstrap_in_command`: the judged probe said not
remote, the second said remote. The verdict (first probe) was right. Fixed at the seven places in `cases_caseremote.py` and
`cases_win_input.py`: one probe, and the row names it. `cases_agent_remote.py:264` has the same shape (a probe typed for a message
that is built even on a pass); it is the lab's file and was left.

## 3. `ssh_failing_not_local`, agent@gitbash only: TEST (isolation), the app question is open

The row: "got [] ... first gate release/exit: `kh_hold_exit ... branch=held`". The three passing rows of the same run (terminal,
agent, terminal@gitbash) show `branch=gate_open_live`.

- The case before it, `ssh_nested_typeahead_wrapper`, switches the managed path on (`export PHOSPHOR_MSYS_MANAGED_SSH=1`) and types
  `unset` at its end without checking. It failed with the pane in the remote shell, so the `unset` did not run locally.
- In agent@gitbash the runner then got the pane back to the local shell (no `pane_not_local_after_ssh_nested_typeahead_wrapper` row,
  no relaunch), so the next case ran in the same shell with the switch on. In terminal@gitbash the app was relaunched in between and
  the case passed.
- With the switch on, the wrapper first asks the server for its banner (`ssh-keyscan -T 5`,
  `app/assets/bundled/bootstrap/bash_body.sh:3117`), a connection the hanging listener never answers, and only then starts ssh. The
  case takes the first moment no connection is open for the failure and reads the file 6 s later. If that first close was the banner
  check (the row: closed 4.74 s after typing began, 7.1 s after the command; the listener count is taken only while typing), ssh was
  still inside its 6 s ConnectTimeout when the file was read. This step is read from the wrapper's code, not from a log.

So the row does not prove the line was lost: the case ran in a configuration it was not written for and may have looked early.
Whether the managed path delivers the line could not be told (no app log).

Fix: `msys_managed_ssh_restore` proves the `unset` (a line the local shell answers with the variable's value), retries once after
bringing the pane back, else hands the runner a reason and the runner relaunches (`pane_switch_left_on_after_<case>`).
`wait_ssh_gave_up`: the failure is the last close with none following for 3 s. New `PROVING` case
`ssh_gitbash_failing_not_local_managed` asks the open question on purpose, with the unchanged oracle.

Rejected: a longer fixed sleep (a timer where an event exists); skipping `ssh_failing_not_local` when the switch is on (the leak
would stay).

## 4. `pane_not_local_after_*` behind these cases

All "could not be proven back in the local shell (none)": the same lost Enter, now from the runner's own probes. They follow from 1.
(`pane_not_local_after_big_paste`, `_md_viewer`, `_md_viewer_first_frame` are "not the root shell" rows behind other cases and are
not part of this group.)

## 5. `gitbash_typeahead_reps`, agent@gitbash: APP, #1393 (open), not worked

tr13: 6 of 10 lines, the last four missing. tr14: lines 0 and 1 ran as one command (`echo t645... >> r_tr14.txtecho t646... >>
r_tr14.txt`). Injector: 8479 of 8479 keys and 242 of 242 Enters reached the app. The case's own attribution: "the app's own pty
writes carry the corrupted line". The row's `enter_route`: `AI=7`, in a case that types only shell lines. terminal@gitbash: PASS.
Two bad reps of 24 is the rate #1393 was filed with, and "the rest of the typeahead went to the agent" is in its title.

Not worked: which Enter went to the agent first, and with what text in the editor, is only in the app log, which this job did not
keep. Both bad reps are the first two with the slow precmd function, so the set-up line itself is a suspect.

Change (evidence only): every rep notes its Enters that were not routed to the shell with the buffer they carried
(`hop=enter_begin ... buffer= input_type=`, logged for #1393 already), and the case says if the set-up line was routed elsewhere.
No check added or removed. The next agent@gitbash run then names the first wrong Enter.

No row of this case exists in the committed runs before round 14 (`test-results/TREND.md`).

## Round 13 comparison

`ssh_gitbash_managed`, `remote_tab_no_helper`: FAIL in rounds 12 and 13 (`test-results/2026-11-10_build_r12a_*`, `_r13a_*`), then
"no remote shell prompt" (the typed bootstrap, #2051). Round 14 gets the prompt and fails one step later. The `*_wrapper` variants
were SKIP before round 14 (the job did not pin the wrapper).

## Also seen, not in this group

- app-test-windows: `after_script` exceeded its time limit, so no `run_slim` logs or screenshots for any Windows row of round 14.
- The typed-ahead Enter of a managed session arrived as Ctrl+Return with character 10: the console host was given a line feed, not
  a carriage return (plain sessions show CR). Harmless for a POSIX remote. Not followed further.
