# Round 14: remote/ssh latency items #985 and #1393

Branch `fix/r14-remoteprox`, from `integ/r14` (4bf571aae). Nothing was compiled and no app test
was run when this was written. Script tests and the appci self-test were run on the laptop.

## #985: keys to a raw-input program arrive 40-100 ms late over managed ssh

### State

- **The cause is the DSCP class of the connection, not the proxy's traffic.** The interactive
  session and the `remote-server-proxy` session are both clients of one ControlMaster, so they
  share one TCP connection (`crates/remote_server/src/ssh.rs:29-39`,
  `app/src/remote_server/ssh_transport.rs:930`; wrapper: `bash_body.sh:2452` starts the master
  with `-N -f`, `:2693` starts the session on it). One socket has one TOS byte. In OpenSSH up to
  10.0 every session that opens sets the whole connection to the first `IPQoS` value when it
  has a tty and to the second when it has none, and the default is `af21 cs1`. The proxy (and
  every side command) has no tty, so from its start the keystrokes travel as CS1, the
  "lower effort" class, which a Wi-Fi access point sends from its background queue. That matches
  the issue's own table: an IDLE second session on a plain Konsole ssh costs +83/+100 ms,
  killing the helper's processes removes the delay, and turning `ObscureKeystrokeTiming` off
  changes nothing.
- **Worked, unproven, since 124ff6485 / 34bd7f589 (2026-10-25):** the bash, zsh and fish
  wrappers pass `-o "IPQoS=af21 af21"` to the master when `ssh -G` reports exactly
  `ipqos af21 cs1` (`bash_body.sh:2378-2388`, `zsh_body.sh:1414-1422`, `fish.sh:1000-1008`). A
  user's own IPQoS is never overridden. The two measurements of 2026-10-26 in the issue (LAN,
  0.7 ms RTT: managed 4.0-4.2 ms against plain 12.8 ms; Mac no worse than plain) came after it
  and show no delay, but neither was taken on a link that treats classes differently, so they do
  not prove the pin; they only show there is no other delay.
- **OpenSSH 10.1 and later behave differently, and the pin does not apply there.** Read from the
  10.2p1 source (`packet.c` `apply_qos`, `channels.c` `channel_has_bulk`, `channels.h`
  `CHANNEL_BULK_NOTTY "direct-*,forwarded-*,tun-*,x11-*,session*"`, `clientloop.c`): the
  connection is "interactive" only while NO session without a tty is open, re-decided on every
  loop pass; the defaults are `ef` for interactive and the system default for the rest
  (`ssh -G` on this laptop, OpenSSH 10.2p1: `ipqos ef cs0`). So with the helper running, a
  managed session is best effort for its whole life while plain ssh is EF; without the helper it
  drops to best effort only for the moments a side command runs. Best effort is the class of
  all ordinary traffic; no queue penalises it the way CS1 is penalised. The 40-100 ms of the
  issue is therefore not expected on OpenSSH >= 10.1, but a managed session there does not get
  the EF boost plain ssh gets.
- **Measurable?** Not as time in the netem lab: `sshlab/.../netem.py` delays every packet of the
  ssh ports alike, whatever its class. What the lab can show is (a) the class itself, read from
  the client socket, and (b) that sharing the connection adds no OTHER delay.

### Chosen

1. Keep the pin as it is. Comments in the three wrappers and the unit test's doc now say which
   OpenSSH it is for. `script/test_ssh_keepalive` gains the cases that hold both halves: the
   `af21 cs1` default is pinned; `ef cs0`, `ef none`, a user's `af21 af21` / `cs1 cs1` and a
   single value are passed through.
2. New lab case `lab_raw_key_arrival` (PROVING_LAB; bash and zsh, LAN and 150 ms, with and
   without the helper, both modes): a program on the server stamps every byte it reads from its
   tty; keys go into the app's managed session and, interleaved, into a plain `ssh -tt` session
   running the same program. Delivery time = server stamp minus the moment the key was handed to
   ssh (the app's `input_order pty_write` line; the control's write). Verdict: the logger read
   exactly the typed keys in order, and median(app) <= median(plain) + max(15 ms, 10 %), the
   owner's remote budget (`lab.paired_verdict`, unchanged). On Linux the TOS byte of each
   connection is read with `ss --tos`: a connection of the app in CS1 fails the arm (that is the
   regression test of the pin on a client with OpenSSH <= 10.0); any other difference from the
   control is a note and the metrics `tos_app` / `tos_plain`.

### Rejected

- **The proxy on its own ssh connection** (the issue's first candidate). A second connection
  needs a second authentication (password, passphrase, MFA), which the single-login design of
  #1035 exists to avoid.
- **Pinning `IPQoS=ef ef` for the 10.1+ default.** It would give the managed session plain ssh's
  class, but it also marks the helper's bulk traffic (file reads and writes, the helper upload)
  EF. OpenSSH 10.1 switches classes at run time precisely so bulk data is not sent as EF; a
  network that polices EF, or Wi-Fi's voice queue (no aggregation), can then hurt both the
  transfer and the keystrokes behind it. No measurement says best effort costs anything, so by
  the "measured, small and safe" rule it is not done. It is an owner decision (below).
- **A timer or a flush in the input path.** There is nothing to flush: ssh sets TCP_NODELAY
  itself, and the app writes each key to the pty at once.

### Risk

Comment-only change in the wrappers; script cases only add expectations for present behaviour.
The lab case is new and unproven: its agent-mode arms assume a key typed while a shell command
runs goes to that command (as `lab_ctrl_c_no_esc_i` assumes for Ctrl-C).

### Owner decision

On a client with OpenSSH >= 10.1 and the helper running, should the managed session be pinned to
EF (`IPQoS=ef ef`, parity with plain ssh, bulk helper traffic also EF) or stay best effort
(today)? `lab_raw_key_arrival.*.helper` shows the present state as `tos_app` / `tos_plain`.

## #1393: Git Bash 25 ms/key typeahead loses lines

### State

- The issue is about the LOCAL Git Bash pane (10 `echo Ln >> file` lines typed back to back; 6 of
  84 reps lost a line on build/r18bj, 2026-11-01). It is not an ssh issue; ssh shares only the
  hold machinery.
- **Code since then (worked, partly proven by passing runs of other cases):** #1349/#1353
  (event-driven hand-over, one input queue), 930b71be5 (#1996: a hold that gives up writes its
  keys to the pty, nothing typed after an Enter is dropped), d925abad6 (#1923: a ConPTY Precmd
  leaves input with the pty until the line editor signals), 3d32ce1db (#1997), eb65694b7 (#2012:
  the shell's report goes behind the text the editor held, the shape of the issue's rep 23 where
  the report came back rotated).
- **Evidence in `test-results/`:** `fast_typing` in the Git Bash pane (30 lines back to back and
  30 with a 0.25 s gap, 10-25 ms/key, effect file and rec.log exact) passed in both modes in
  every Windows run since r18w5: r18w5, r18w7, r18w8, r18w9, r10a, r11a, r12a = 14 passes
  (r12a terminal@gitbash: 60 Enters, injector sent 2060 keys, app saw 2060, 0 lost; the other
  rows were read for their status only). If every pass typed 60 lines that is 420 back-to-back
  lines, 42 of the issue's ten-line reps. At the issue's rate (6 of 84 reps lost a line) 42 clean
  reps have a chance of about 1 in 20. So the loss is probably gone, but the issue's own protocol
  (24 reps, and typing during a slow prompt) has not been rerun, and no run had
  `PHOSPHOR_CONPTY_PROMPT_HOLD=0`.
- **Does it apply to plain ssh from the Git Bash pane (managed ssh is off by default, #2051)?**
  Not by the same mechanism. In a plain ssh session the app treats ssh as a running command:
  keys go to the pty as typed, there is no per-line hold, report or hand-back, so the window of
  #1393 (between a command and the line editor of the LOCAL shell) does not exist per remote
  line. The one window is the connect itself (keys typed behind `ssh host` Enter, held until
  ssh owns the tty), which `ssh_connect_typing` and `ssh_nested_typeahead` cover. Their Git
  Bash result is unknown: r12a failed there because the managed path could not start
  (`never reached the REMOTE file`, #2051), and no run exists after c82ff4af0. The PowerShell
  pane passed both in r12a.

### Chosen

New case `gitbash_typeahead_reps` (PROVING, Git Bash pane only, both modes): 24 reps of 10
back-to-back lines (`cases.effect_run`, gap 0), the second 12 with a precmd function that sleeps
250 ms (the `__git_ps1` cost the issue's analysis names), each rep's effect file and rec.log
exact. No app change: there is no present evidence of an app fault to change code against.

### Rejected

- Closing on the `fast_typing` passes alone: not the issue's count and no slow prompt.
- A hold-OFF arm (`PHOSPHOR_CONPTY_PROMPT_HOLD=0`): it needs an app relaunch with another
  environment and tests a kill switch, not the default; worth adding only if the default arm
  fails.
- An ssh arm inside this case: the connect window already has its cases; they need a run, not a
  twin.

### What the next run shows

- `gitbash_typeahead_reps` PASS in both modes: #1393 can be closed with the run.
- A FAIL names the rep and whether it was a slow-precmd rep; the case log's `input_order` text
  (the harness sets `WARP_INPUT_ORDER_LOG_TEXT=1`) then answers the issue's open question
  (report rotated by the shell or by the app).
- `ssh_connect_typing` / `ssh_nested_typeahead` in `terminal@gitbash` and `agent@gitbash` after
  #2051: PASS means the plain path from Git Bash keeps typeahead; they are SMOKE cases already.
