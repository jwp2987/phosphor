# Round 12: the round 11 failures in the Linux lab

Branch `fix/r12-lab2`, base integ/r12 `501a18469`. Run judged: build/r11a (integ/r11 `c61b6a3b8`, pipeline 3049602519), job `app-test-lab-linux`
17896783499: 29 PASS / 21 FAIL / 50 NOREF (round 10: 17 / 75 / 44). Evidence: the job's per-case app-log slices and screenshots (job artifacts) and
`test-results/2026-11-10_build_r11a_3049602519/app/lab-wayland/`. Nothing here was compiled or run against a build (laptop: rustfmt,
`run.py --self-test`). Wording: merged-untested work is "worked, unproven".

Result: the 21 FAIL rows have four causes. One app defect (#2044, new, 8 rows), two test faults (#2045, new, 9 rows; one of them is the regression),
one open owner question (#2019, 4 rows). One more defect was found by reading and filed without a change (#2046).

| # | cause | verdict | rows | state |
|---|---|---|---|---|
| 1 | The fake model reads the app's attached context as the prompt | **TEST**, #2045 | 3: `lab_fam_agent_edit_conflict.bash` (the REGRESSION), `lab_agent_remote_drop.bash`, `pane_not_local_after_lab_agent_remote_drop.bash` | worked, unproven |
| 2 | Ctrl-C in a plain ssh session is SIGINT to the local ssh | **APP**, #2044 | 8: `lab_ctrl_c_no_esc_i.{fish,nu}.{lan,150}.nohelper`, both modes | worked, unproven |
| 3 | The harness's where-probe is POSIX syntax | **TEST**, #2045 | 6: `lab_enter_start.nu.150`, `lab_enter_no_network_wait.nu.150` (both modes), `lab_fam_agent_context.nu.{lan,150}` (terminal) | worked, unproven |
| 4 | Agent tools on a host whose login shell gets a plain session | owner question (#2019) | 4: `lab_agent_remote_{flow,errors,drop}.nu`, `lab_fam_agent_edit_conflict.nu` | not changed |

## What round 11 already proved, and what integ/r12 adds

The round 10 lab changes were all in the r11a build (`2d2a4db20` #2019, `226240a5e` #2020, `c18d5665a` #2021, `c478c9b9b` #2027), and the run shows
each of them working:

- #2019: every fish and nu arm connects (`Received PlainSSHFallback hook`, a remote prompt). 24 round 10 rows gone.
- #2020: `lab_fam_agent_edit_endings.bash` PASS.
- #2021: no `harness_state_reset_after_*` row (40 in round 10); `lab_agent_remote_flow.bash` and `lab_fam_agent_delete_conflict.bash` PASS.
- #2027: `lab_enter_start.{bash,zsh}.150` and `lab_ctrl_c_no_esc_i.bash.150` PASS in both modes.

Nothing merged into integ/r12 after r11a touches any of the 21 rows. `fd9a4e721` (the cached lab helper) and `56e3eeb87` (#2042) are about the
`.helper` arms, which are NOREF rows, not FAIL rows. All four causes below are new in the sense that round 10 could not reach them: no fish or nu arm
got past the connect then.

## 1. The regression: `lab_fam_agent_edit_conflict.bash.lan.nohelper` (agent). TEST, #2045

**Rows.** "remote: the accepted multi-hunk edit: 207 bytes on disk, 228 expected", "no hop=backend_route family=file_write line", "no follow-up model
request within 40 s", "no action card for the proposed edit". Round 10: PASS in 36.7 s.

**Evidence.** The case typed its edit prompt (244 characters). The app log:

```
16:42:15 [byop-diag] build_chat_request UserQuery: query_len=244 ... prefixes=1 suffixes=0 full_text_len=3053
16:42:15 [byop] tool_call_in: name=run_shell_command call_id=call_bc5508d7
16:42:19 Not accepting requested command AIAgentActionId("call_bc5508d7"): part of it is hidden on the card; showing it in full instead
```

The model was asked for an edit and answered with a shell command. The card screenshot (`agent_edit_card_ae_remote_*`) shows the command: the text
of a nushell error, `/usr/bin/touch /work/u_nu/started_...; /usr/bin/sleep 300; ...`, then the closing `</attached_context>` and the real prompt.

**Cause.** `chat_stream.rs:2903-2918` puts the user's attachments in front of the query (`"{prefix}\n\n{query}"`), and the blocks run since the last
query are attached by the app itself (`user_context.rs:121-137`, `auto_attached="true"`, command and output). `fake_model.py` searched the whole
message for its triggers, and `PLAN_RE` is tried first. The case before this one was `lab_agent_remote_drop.nu`, whose prompt
(`Please carry out these steps in order: run ...`) had been typed into the remote nu (cause 4); nu's error message quotes the line. That block was
attached to this case's request, and the plan trigger matched it.

`lab_agent_remote_drop.bash` is the same fault one case earlier: `query_len=129 ... full_text_len=4683`, then `tool_call_in: name=read_files`, and the
screenshot shows a read of `/work/u_nu/age_.../nope_....txt`, the first step of `lab_agent_remote_errors.nu`. The approved command therefore never
started on the host, the case stopped with the ssh session open, and the runner's own check added `pane_not_local_after_*`.

**Not an app defect.** Attaching the recent blocks is what the app is meant to do, and the real prompt was sent intact.

**Fix.** `fake_model.prompt_text`: a user message's text without the `<attached_context>` block(s) it STARTS with. `user_prompt`, `turn_state` and
the sticky `STREAMLONG` check read it, so every trigger and every fault `match` sees what the user typed. The block's text is XML-escaped by the app
(`xml_text`), so the first literal `</attached_context>` is its real end.

**Rejected.** Stripping in `clean_messages`: the request log, `fake.fresh` and `history_check` prove what the model was SENT (#1829: "no key typed into
a program leaked into the conversation"); they must keep seeing the attachment. Matching the triggers at the end of the message only: every trigger
would need the rule, and a new one would forget it.

**Risk.** Low. A message that does not start with the block is untouched.

## 2. `lab_ctrl_c_no_esc_i` on fish and nu, both latencies, both modes. APP, #2044

**Rows.** fish: every round "effect file: expected [...], got []" and "the shell received []", then "`exit` did not return to the local shell (local)".
nu: "the shell received ['sleep 30']" for the first round, "no prompt after the Ctrl-C burst" for the rest.

**Evidence** (`lab_ctrl_c_no_esc_i.fish.lan.nohelper`, terminal):

```
15:49:23.186 hop=enter_begin buffer="ssh lab-u_fish"
15:49:23.421 Received PlainSSHFallback hook; hop=ssh_wrapper verdict managed=false tty_owned_deferred=false
15:49:23.445 hop=ssh_pty_owner ssh=true site=tty_owned
15:49:24.5   `sleep 30` + Enter: nine pty_write bytes=1, each with gate_armed=true
15:49:25.576 hop=ctrl_c_route route=pty long_running=true
15:49:25.595 Received CommandFinished hook; hop=ssh_gate_released how=failed
```

No `ctrl_c_to_event_loop`, `ctrl_c_received` or `ctrl_c_pty_write` follows the route line, and the local ssh has ended 19 ms later. The later rounds
of the same case, now at the local bash, show all three. The nu screenshot shows `sleep 30` / `^C` and the echo lines as local blocks
(`bash: /work/u_nu/cc_....txt: No such file or directory`).

**Cause.** `user_write_ctrl_c_to_pty` (`view.rs:10310`) asks `ssh_connect_sigint_applies` (`view.rs:13153`), and the table asks its twin
(`view/route_facts.rs:316`): no ssh hook, no pending session, the connect window armed, its block not ended. Then the Ctrl-C is
`Event::SigintForegroundToPty` (#1001: a connect that hangs must be abortable). The rule never asks whether ssh already owns the tty. Since #1809
(`3244df761`) the window has no clock where the tty-read probe works (`has_no_probe_net`, unit test `ssh_gate_with_a_tty_read_probe_has_no_clock`),
so for an ssh that never becomes a managed session nothing ends it but the end of the ssh block: `should_release` (`view.rs:41632`) has no branch for
it. The window is armed for the whole session and every Ctrl-C is a SIGINT to the local ssh client, which exits.

`DECLINED.md` ("Kept: ... Ctrl-C before ownership as SIGINT (after it, the byte)") already records the intended rule. The code did not have the
second half.

**Scope.** Proven in the run for a plain fallback under the wrapper on Linux. By reading: the same for an ssh the wrapper is not part of; on macOS
until the no-probe give-up releases the window. ConPTY: no SIGINT route (`set_foreground_sigint` is unix only), not affected. A managed session is not
affected: the wrapper defers the ownership (`ssh_on_tty_owned`, `view.rs:11869-11877`) and the window ends at the remote shell's first prompt.

**Fix.** `SshConnectGate::plain_ssh_owns_tty`: `tty_owned && tty_owned_by_event && wrapper == Off`. `SshConnectGate::interrupt_is_sigint`:
`!block_done && !plain_ssh_owns_tty()`, used by both sites. After the ownership event of a window no wrapper holds, the Ctrl-C is the byte:

- ssh still connecting or asking a question: the tty is cooked with `ISIG`, the kernel turns the byte into the same SIGINT to the same group;
- session up: the tty is raw, the byte goes to the remote and interrupts the remote command, as in any terminal.

Under the wrapper nothing changes before the verdict (`AwaitingVerdict`: ownership deferred, `tty_owned` unset) or after a managed verdict.

**Rejected.**
- Releasing the window at the ownership event: other rules read the armed window (the block-end bookkeeping, `ssh_owned_block_without_window`, the
  release reasons); a wider change than the defect needs.
- A flag set only by `PlainSSHFallback`: it would leave the same defect for an ssh the wrapper is not part of.
- Changing the Ctrl-D rule in the same way (`ssh_connect_holds_eot`): see #2046 below.

**Risk.** A Ctrl-C between the ownership event and the session, on a tty that ssh has made raw, in a window no wrapper holds, now reaches the remote
login instead of ending ssh. That is the plain terminal's behaviour and the recorded decision. The legacy route for a remote announced by the ssh
hook is unchanged from the hook on (`ssh_pre_init_abort_bytes`, the bootstrap hold).

**Tests.** `route_parity_a_tests.rs`: `ssh_gate_interrupt_is_the_byte_once_a_plain_ssh_owns_the_tty` (pure),
`ctrl_c_in_a_plain_ssh_session_is_the_byte_not_sigint` (announcement, ownership, plain verdict; the keyboard route and the Ctrl-C action; Legacy and
Table), `ctrl_c_after_the_ownership_event_of_an_unwrapped_ssh_is_the_byte`. No existing expectation changed: the SIGINT tests
(`ssh_ctrl_c_while_silently_connecting_aborts_with_sigint`, `ctrl_c_before_the_connect_gate_is_armed_is_sigint`,
`managed_ssh_ctrl_c_while_awaiting_the_verdict_interrupts_and_flushes`) have no ownership event in an unheld window.

### The case's own lines (TEST, #2045)

Hidden behind the app defect: the case typed `echo <tok> >> <file>` and `sleep 30` into every shell. In nushell 0.116.1 the first prints a table (see
cause 3) and the second is not a valid `sleep` (it takes a duration). The case now types `ctx.append_cmd(<shell>, ...)` and
`lab.long_sleep_line(<shell>)` (`sleep 30sec` for nu). The checks are the same; "the shell received exactly [the sleep, the echo]" compares with the
line as typed.

### Found by reading, filed, not changed: #2046

`ssh_connect_holds_eot` holds a Ctrl-D with a notice while the window is armed and the output does not end in an unterminated line. In a session that
lasts as long as its window that is a Ctrl-D held at `cat`, or at a remote prompt that ends in a newline. Not changed: the unit test
`ssh_ctrl_d_is_held_after_a_prompt_was_answered` (#1111) requires exactly this hold for an owned window with no wrapper, before the ssh hook, and
that state cannot be told from an established plain session without a new fact. It needs a decision and a run.

## 3. "`exit` did not return to the local shell (none)" on the nu terminal arms. TEST, #2045

**Evidence.** Screenshot `terminal_lab_fam_agent_context.nu.lan.nohelper_FAIL`:

```
nu> echo wT627_baqv3a3w >> ~/.appci_where
 0  wT627_baqv3a3w
 1  >>
 2  ~/.appci_where
```

`where_now` (`cases.py`) proves which shell is in front by that line's effect. nushell 0.116.1 prints the three words and writes nothing, so the pane
is "none", `CR.leave` does not type `exit`, and the row fails. The cases' own checks had passed or were NOREF: `lab_enter_start.nu` paired verdict
pass (app median 117 ms), `lab_enter_no_network_wait.nu` and `lab_fam_agent_context.nu` NOREF.

**Fix.** `cases.where_probe_cmd(remote_shell, tok)`: for a nu lab arm the append is done by a child `sh` (`sh -c 'echo <tok> >> ~/.appci_where'`),
a line nu, bash, zsh and fish run the same way, so it is right whichever shell is in front. Every other arm and every non-lab case keeps the plain
line. The selftest runs the line in every shell the machine has. `depth_probe_cmd` already uses the same form for the pid probe.

**Rejected.** The `sh -c` form for every case: a Windows PowerShell or cmd.exe pane has no `sh`, and it would change the probe of every case for a
fault of one shell.

**Risk.** nu was not run here (not installed). That nushell passes a single-quoted word to an external command unchanged is from its syntax.

## 4. The agent-mode nu arms. Owner question, not changed

`lab_agent_remote_{flow,errors,drop}.nu`, `lab_fam_agent_edit_conflict.nu`: "the feature typed into the remote shell", "the model received 0 of N tool
results", "no approval card". A nu login shell gets a plain session (#2019); in a plain session the keys are ssh's, so the agent prompt goes to the
remote shell. The round 10 report predicted exactly this ("expected FAIL at the first remote tool call (decision 1)"). Whether fish and nu login
shells get a managed session is the open question on #2019. The rows stay FAIL until it is answered; they are not turned into NOREF here.

## What the next Linux lab run should show (this branch, no helper binary)

- `lab_fam_agent_edit_conflict.bash`: PASS, as in round 10. The app log has `tool_call_in: name=apply_file_diffs` for both proposals.
- `lab_agent_remote_drop.bash`: `tool_call_in: name=run_shell_command` with its own command; its real checks (the drop, the tool result, the pane
  local again) run for the first time, so a new finding is possible. No `pane_not_local_after_*` row if it passes.
- `lab_ctrl_c_no_esc_i.{fish,nu}` (8 rows): after `ctrl_c_route route=pty` the log has `ctrl_c_pty_write bytes=1` and no `CommandFinished` for the
  ssh block; the session stays remote; effect files exact. These rounds have never run on fish or nu, so a new finding is possible (fish logs a
  command at `fish_postexec`; nu's prompt after an interrupted `sleep 30sec`).
- `lab_enter_start.nu.150` (2 rows): PASS (the paired verdict already passed). `lab_enter_no_network_wait.nu.150` (2) and
  `lab_fam_agent_context.nu` (2): NOREF with their own reasons, no `exit` line.
- The four agent-mode nu rows: FAIL as now, until #2019's question is decided.

Expected totals if nothing new appears: 4 FAIL (the nu agent arms), down from 21.

## Seen, not changed

- `lab_enter_start.nu.150`: the plain-ssh control took 1.3 to 1.9 s per line after the first (91 ms), against 113 to 125 ms in the app. With a
  control that slow the paired verdict cannot fail. Not looked into (noted on #2045).
- `lab_enter_no_network_wait.{fish,nu}`: NOREF because a plain session logs no `hop=enter_begin` (each key is written as it is typed). The question
  "Enter never waits on the network" is unmeasured there, not answered.
