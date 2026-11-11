# Round 19: the Windows rest of the round 14 candidate (task r19-winmisc)

Source: build/r14a 42ef67839, pipeline 3052606915, job app-test-windows (17921510251). Evidence is the job trace, `results.json` and
`inject.log`. The job kept no screenshot and no app log (section 7), so every verdict below rests on result lines, metrics and the code.
Nothing was compiled and no app run was made for this note. Issues: #2113 (app), #2114 (harness and CI), #2115 and #2116 (not judged).

Round 13 was a fix-tier run: of these cases only `latency_enter` in the two terminal arms ran there. The last full Windows run in
`test-results/` is 2026-11-05 (`2026-11-05_ssh_r18u28_windows`); "then" below means that run.

## 1. APP: PowerShell expressions are not shell syntax to the router (#2113)

Rows: `intl_wide_type` [agent], `intl_wide_paste` [terminal, agent], `intl_altgr_ps` [agent], `soak` [agent]. All passed then.

Evidence: the `enter_route` line of each row counts Enters the app routed to the agent: `AI=1` of 6, `AI=2` of 10, `AI=6` of 10, `AI=1`
of 1 (with `pty writes: 0`), and for soak `AI=2` of 56. The lines are `[IO.File]::AppendAllText('<file>', '<text>' + "`n", ...)`,
`[IO.File]::WriteAllText(...)` and, in soak's minute 5, the flood line `1..300000` followed by the sentinel.

Cause: `crates/input_classifier/src/util/routing.rs`, `route()`. The first word of the `[IO.File]::...('` lines carries a quote, and
`if first.quoted { return Decision::undecided(Rule::Model) }` returns before any rule runs. `has_shell_specific_command_shape` knows a
cmdlet, a `$variable` assignment and, for PowerShell, `@(` / `@{` (#1344), nothing else. An undecided line goes to the history close match
and then the model (`app/src/ai/blocklist/input_model.rs`, the job spawned by `detect_and_set_input_type`), so the verdict follows the
model's reading of the path and the text: typed CJK went to the shell and pasted CJK to the agent in the same pass. A lone `1..300000` in a
conversation has no evidence (`conversation_lacks_evidence`, lone branch) and goes to the agent by rule; the sentinel typed while that
reply ran was a steer (#851, by design), so it never reached the shell.

Fix: in a PowerShell session, a first word that opens with a type literal followed by `::member` or `$variable`
(`[IO.File]::Exists(`, `[int]$x`, `[Collections.Generic.List[string]]::new()`), or a numeric range (`1..10`, `0..$n`), routes to the
shell with rule `shell-syntax`, decided before the quoted-first-word return. Bracket nesting is matched; the type name must start with a
letter; `[WIP] fix it`, `[1]: note`, `[int]5`, `1.5` and `1..` do not match. No other shell family gets the rule.

Rejected: (a) a rule in the safety gate or the old V1 path: neither knows the shell family; (b) treating any `[` head as shell in
PowerShell: `[WIP] ...` is prose; (c) changing the cases to avoid these lines: the lines are ordinary PowerShell and the routing is the
defect. Risk: a PowerShell user who types English that opens with `[Type]::word` or `1..2` gets a shell error instead of an agent turn;
no such sentence was found.

Tests: `routing_tests.rs` (`powershell_type_and_range_heads_are_shell_syntax_in_a_powershell_session`,
`powershell_expression_rule_takes_nothing_else`); `routing_table_powershell` (PROVING) types `[math]::Sqrt(2)`,
`[IO.File]::Exists('log.txt')` and `1..3` in both modes.

## 2. TEST: a Git Bash started from the PowerShell pane is never left (#2114 item 1)

Rows: `gitbash_subshell` [terminal, agent], `pane_wedged_after_gitbash_subshell` [terminal, agent], `pane_wedged_after_editor_daily`
[terminal, agent]. `gitbash_subshell` passed then.

Evidence: the note "`exit` NOT typed: the probe ran in the local shell and the shell count under the app is unknown (base None, now
None)" on `gitbash_subshell` and on `intl_altgr_bash` (noref) in both PowerShell arms. `gitbash_subshell`'s 20 typed lines, 4 pastes and
3 Ctrl-C all passed; only the PowerShell line after the missing `exit` failed. Between `intl_altgr_bash` and `editor_daily` only cases
that open files by hand-over ran; `editor_daily` is the first that types a PowerShell probe ("the shell pane did not take typing").

Cause: `Ctx.exit_nested` (2026-11-09, harness audit #2) replaced the raw `exit`. In a PowerShell pane the PID probe is
`Add-Content ~/.appci_pid tok:$PID`, which bash cannot run; the where probe runs in both shells and lands in the same home; and
`shell_count()` is None on Windows. No proof, no `exit`: bash stays in front and takes every PowerShell line after it.

Fix: `posix_probe_cmd`, a line only bash/zsh can run (`[ -n "$BASH_VERSION$ZSH_VERSION" ] && echo tok:$$ >> ~/.appci_pid`; PowerShell 5.1
and 7 refuse it at parse time, so no part of it runs there). In a PowerShell pane an answer to it proves a nested POSIX shell, and
`exit_nested` and the post-case `leave_nested` then type `exit`. The rule "no proof, no exit" stands: no answer, no `exit`.

Rejected: comparing the bash pid with the root pid (msys and Windows pid spaces differ); a Windows process count (the app's own helper
shells would fool it, as `shell_count` says).

## 3. TEST: the previous case's command is logged after the next case's mark (#2114 item 2)

Row: `cb_agent_pickers` [agent@gitbash]; passed then. The "picker key" it reports is `printf 'Continue? [y/N] '; read ans; echo LRCYN... >>
lrc_yn.txt`, the line of `agent_lrc_no_false_takeover` part 2, whose own note says the command was still at its prompt after 60 s. That
case's `finally` pressed Ctrl-C and returned; bash logs an interrupted command at its next prompt, 0.7 s later by the wall times.

Fix: `lrc_wait_logged`: after the Ctrl-C the case waits (10 s net) until rec.log holds its line. `cb_agent_pickers` is unchanged.

## 4. TEST: the Windows launch types a command inside the idle window (#2114 item 3)

Row: `zero_network_startup` [terminal@gitbash], "held a connection ... port 11434 (idle)". Passes on Wayland (strace) and macOS (polled).
On Windows `cases_robust.start_app` ends with the key proof (`ctx.reprove`: a typed marker command, the cwd, the root-pid probe), and a
command that ran makes the app ask the configured model for a suggestion, which the case allows in its scripted session only.

Fix: the polled case launches with the key proof taken out, idles, then runs the key proof as the first step of the session. Launch and
idle still allow nothing. Rejected: allowing the provider during a "key proof window": the poll keys a row by process and destination,
so a later idle connect to the same endpoint would not be seen.

## 5. TEST: one sample set decided a 0.04 ms margin (#2114 item 4)

Row: `latency_first_key_after_idle` [agent@gitbash] gap1s: 4.04 ms vs baseline 3.5 (+0.54 ms; the floor is 0.5). The case's control (the
key 0.3 s later) is 4.10 ms: idle penalty -0.06 +/- 0.17 ms. `latency_key_log` in the same arm: 3.93 vs 3.85 (+2.1 %, pass). The gap2s and
gap5s baselines are 3.8. `lat.py` re-measures a suspected regression once; this path passed no `remeasure`.

Fix: the path hands `lat.evaluate` a re-measure of the same block. A clean second set is a WARN, a second regressed set stays FAIL.
The gate, its thresholds and the baseline are unchanged.

## 6. TEST: the previous release's stage fails the build under test (#2114 item 5)

Rows: `upgrade_over_previous` [terminal] ("uo_old: after the relaunch keys not delivered", the previous release's window in front) and
[terminal@gitbash] ("uo_old: no window / shell after the relaunch within 90 s"). First Windows run of the case (always SKIP until #2067).
Both stop before the build under test is started on the old data.

Fix: the `uo_old` stage reports into a scratch result; if it has a failure the case is UNVERIFIABLE (noref), naming it and what is
needed. Everything after it (the build on the old data) is judged as before. Why the previous release cannot be driven on the Windows
VM is not known from this run (no screenshot); it needs a look at the machine.

## 7. CI: the job lost its screenshots and logs (#2114 item 6)

Trace: `after_script could not run to completion because the timeout was exceeded` 5.5 minutes after it started, inside after_job.ps1
(1067 recorded pids) and the scrub; `slim_appci_run.py` never ran; `run_slim/shots` and `run_slim/logs`: "no matching files".
Fix: `RUNNER_AFTER_SCRIPT_TIMEOUT: "45m"` on `app-test-windows` (inherited by the lab and reuse jobs).

## 8. Not changed

- `big_paste` [terminal@gitbash, agent@gitbash], `pane_not_local_after_big_paste`: PLATFORM, as then. The conhost control falls short
  too (2 of 1000001 bytes; Phosphor's pane 308309 and 160937). `cat` is still being fed the backlog when the Ctrl-C arrive behind it.
- `remote_heredoc_paste` [terminal@gitbash]: as then. The test sshd counted 65536 bytes in 62577 chunks at 1.2 to 1.5 KB/s from the msys
  ssh client; the reference console's ssh did not deliver the here-doc either.
- `pane_not_local_after_md_viewer`, `pane_not_local_after_md_viewer_first_frame` [terminal@gitbash], `pane_wedged_after_md_viewer_first_frame`
  [terminal, agent, agent@gitbash]: consequence of `md_viewer_first_frame` / `md_viewer` (task r19-tabs): the file opens somewhere the
  harness does not see ("no add_file_pane line"), nothing is closed, and the probe types into it. Pointer for that task:
  `uri::open_file` dispatches `root_view:add_file_pane` only when a window id came with the URL; otherwise it opens a new window
  (`root_view:open_new_with_file_notebook`), which logs no such line.
- `expert_fuzz` [terminal@gitbash] (#2115), `history_survives_restart` [terminal@gitbash], `desktop_notification` [terminal@gitbash] and
  its wedge row, `pane_wedged_after_restore_shows_history` [terminal@gitbash], `latency_enter` [agent@gitbash] (#2116): not judged; each
  needs a targeted rerun with the app log kept.
