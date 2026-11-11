# r19-decide-tests: three owner decisions of 2026-11-11 in the app-test harness

Harness only. No app code changes. Nothing was run against an app: every rule below is **worked, unproven** until a
Windows run (4, 5) and a lab run with dash / ash arms (9) play it. Source rows: build/r14a 42ef67839, job
app-test-windows (`docs/design/r19-winmisc.md` section 8); `docs/design/r19-shssh.md` (#2120).

## (4) Windows `big_paste` and `remote_heredoc_paste`: a control that fails too makes the row NOREF

**Today.** Both cases already have a control, and both rows still FAIL every full Windows run.

- `big_paste` (`cases.py` `c_big`): the byte check is NOREF when the conhost control falls short too
  (`big_paste_control` -> `same-shortfall`). The row FAILs anyway on `big_paste_back_at_prompt`
  ("the shell never came back to a prompt after `cat` was interrupted"), which is the same shortfall seen from the
  other end: ConPTY still holds the unread rest of the 1 MB paste as console input, `cat` is still being fed it,
  and the Ctrl-C queue behind it. `run.py` then adds `pane_not_local_after_big_paste` (FAIL) because the root-shell
  probe gets no answer from that pane. Round 14: conhost 2 of 1000001 bytes, Phosphor 308309 and 160937.
- `remote_heredoc_paste` (`cases_caseremote.py` `classify_timeout`): the control is the 2,000-line here-doc pasted
  into the reference console's ssh. It covers the two bash here-doc steps (NOREF, class `platform`); the
  10 KB single-line step FAILs with "this step's payload is a different kind/size, so that does not cover it".

**Change.**

- `big_paste`: when the control is `same-shortfall`, "no prompt after the Ctrl-C" is reported as part of the same
  NOREF (with the control's numbers), not as a FAIL, and the case relaunches the app itself so the next case gets a
  clean pane (no `pane_not_local_after_big_paste` row from this cause). Unchanged: control `exact` -> the byte
  FAIL and the prompt FAIL stand; control `none` (no reference console, or it received nothing at all) -> as
  today; a byte-exact paste whose shell does not come back -> FAIL; a relaunch that does not come up -> FAIL.
- `remote_heredoc_paste`: the 10 KB single line gets its own control (`L`: the same line pasted into the reference
  console's ssh, same 60 s budget), run only on Windows and only when that step produced no file. Control fails
  too -> NOREF naming the control's numbers (bytes the test sshd counted, the budget). Control delivers -> FAIL
  class `app`, as today. Control could not run -> the plain FAIL stands, as today.

**Rejected.** Letting the 2,000-line control cover the 10 KB line: it is a different payload, and the existing code
refuses that on purpose; a NOREF must rest on a control of the same bytes. Dropping the two cases on Windows: the
control passing one day is exactly what would make them judge the app again.

**Not changed, on purpose.** The `big_paste` UI-item budget (owner budget 2026-11-10: largest main-thread item
while the paste streamed) still fails the row when it is exceeded, control or not: it is Phosphor's own main
thread, the control says nothing about it, and the existing NO-REFERENCE rows have the same shape (a failed check
beats a NOREF). Flagged in the report as a reading of the decision.

## (5) `latency_enter` on Windows Git Bash: the gate is the app's own segment

**Today.** `c_lat_enter` hands `lat.evaluate` the end-to-end samples only, so the regression verdicts (previous
build, baseline) are on the end-to-end median. Round 14 agent@gitbash: median 115.16 vs baseline 92.77 (+24 %),
re-measured 103.59, FAIL; of that, 2.00 ms is the app's (`enter_begin -> pty_write`) and the rest is after the
write (pty_write -> first pty_read 39 ms; Git Bash fork cost 45 ms at that hour).

**Change.** On the Windows Git Bash pane, both modes, `c_lat_enter` passes `app_share=` (the mechanism of the
2026-11-10 Ctrl-C ruling, `lat.py` "app-share basis"): figure `enter_to_write`, the median of
`enter_begin -> pty_write` the case already records in `app_segments_ms`. Usable (n >= 30 and >= 90 % of the
Enters asked for) -> the previous-build and baseline verdicts are judged on it with the same thresholds
(+5 % and +0.5 ms FAIL), the re-measure rule is kept (a second set, judged on its own share; a clean second set
is a WARN, never a pass), and the end-to-end median stays in the record and in the row as REPORTED, not gated.
Not usable -> the median gate, as before. Hard bound as in the Ctrl-C ruling: the share over 2 ms
(`APPCI_ENTER_APP_SHARE_MAX_MS`, the owner's Enter budget and the bound macOS already uses) fails the row.

`lat.old_app_share` also reads the flat shape older runs recorded (`app_segments_ms.enter_to_write_ms` / `_n`),
so the previous-build comparison works from the first run. **The committed baseline has no app-share figure for
these two records** (promoted 2026-11-06 before the figure was part of a record): until a baseline is promoted
from a run that has it, the baseline comparison says "not compared on the app share" and the previous build and
the 2 ms bound are what judge. That is a coordinator action, named in the report.

**The attribution control.** `enter_segment_samples` gains the two hops the log already has but nothing read:
`enter_to_execute_ms` (`enter_begin -> try_execute_done`: routing and the submit decision) and
`command_to_pty_ms` (`write_command -> pty_write`: model lock, the hand-over to the pty thread, write(2)). With
`submit_bookkeeping_ms` and `effect_flush_ms` they are the four consecutive hops of `enter_begin -> pty_write`.
The row gets one line with all four, and the difference per hop against (a) the terminal-mode arm of the same
run and pane (same build, same host, same hour: the control for "agent mode costs +1 ms") and (b) the previous
run's same arm, naming the hop that grew most. Metric `enter_hops_ms`. No app change: `input.rs` is another
task's file and the hops are there.

**Rejected.** Gating the PowerShell pane the same way (not in the decision; its Enter goes through a pipe and a
2-byte chord). A new hop inside the app (not needed, and `input.rs` is off limits here).

## (9) dash / ash hosts: the agent-mode lab arms report the plain session as the known limitation

**Today.** `cases_lab.plain_session_limit` knows fish and nu only (#2019). Since #2120 a host whose login shell
the remote command does not bootstrap (dash, ash; also sh, ksh) is a plain login at its SSH hook, in the app's
state too, so the agent has no tools there; the agent-mode arms on dash / ash would FAIL. They also cannot get
past `connect_or_fail` today: it waits for `prompt.log`, which the lab's dash / ash hook writes only from
`~/.profile`, and the remote command starts those shells non-login (#2126, another task).

**Change.** `plain_session_limit` gets a second family: login shell dash or ash AND the app log of this connect
has the plain-login event of the SSH hook (`hop=ssh_wrapper remote_plain_login`, or the model's INFO line; the
reader `sh_login_log_facts` of #2120). `agent_connect` on such an arm takes the remote prompt OR that event as
"connected", so it works with and without #2126. Then the row is UNVERIFIABLE (NOREF) with the DECLINED
reference, the agent steps are not typed, and the pane is left through `finish_remote`, whose check stands.
Still a FAIL: neither a prompt nor the event (no connect at all, or the kill switch
`PHOSPHOR_SSH_UNBOOTSTRAPPED_PLAIN=0`); a dash / ash arm that connects without the event is judged in full.
Terminal-mode arms are untouched: `plain_session_feature_limit` stays fish / nu only.

`DECLINED.md` gets the entry next to #2019 (and #2019's sentence "another POSIX shell is not affected" is
corrected); `docs/release-notes-draft.md` gets the known limit.

## Risk

All three only move rows between FAIL and NOREF under a stated premise, and each premise is read from evidence of
that run (the control's bytes, the app log). The risk is a premise that is true for the wrong reason: a reference
console that cannot paste at all is already excluded (`none`), a dash arm with no event is judged. The Enter gate
can now pass a run whose end-to-end median grew for a reason inside the app but after the write (the app reading
the pty slowly): `write_to_first_read_ms` and the median stay in the row for that, not gated, by the decision.
