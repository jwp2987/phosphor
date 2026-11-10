# Test harness reliability audit (Fable 5.2, 2026-11-09)

Read-only audit of script/realapp/appci at merge/w0 b99bda5e0.

## Ranked findings

1. QUITS APP — stale / wrong root PID feeds leave_nested
   - Where: `Ctx.shell_pid` cases.py:816-832, `run.py:455-461` (root pid capture), `leave_nested` cases.py:834-863.
   - Sequence (Windows PowerShell pane): tokens are seeded (`Ctx.rng = Random(seed)`, cases.py:622/665) so every run types the same `pT<n>_xxxx`. `shell_pid` reads the REAL home's `~/.appci_pid` (where_homes() includes USERPROFILE for powershell) with `where_tokens(path)` and NO skip (cases.py:829) — unlike `.appci_where`, which got `where_skip` in r18u19. A previous run's `pT3_abc:4711` is found before this run's probe lands → `root_pid = 4711` (a dead process). Every later `leave_nested` sees `pid != root_pid`, the "second probe" (cases.py:853) agrees (same stale file, next stale token, same old pid) → `exit` typed at the ROOT shell → pane closes → app quits. Only self-consistent by luck when the previous run's token sequence aligns.
   - Sequence (all platforms): run.py:459 records `root_pid` lazily "at the first checked case", typed BEFORE `be.check_injector()/be.guard()` (run.py:468-470) and before `ctx.reset()`. If the previous (un-checked) case left a nested shell, a running program, or a leaked reference window frontmost (the Terminal.app case just fixed), the recorded "root" is that shell's PID; once it goes away, two consistent probes name the real root as "nested" → `exit` at the root.
   - Fix: (a) snapshot `.appci_pid` size per Ctx AND per probe (`skip = where_file_size()` right before typing; parse only tokens appended after); (b) draw probe tokens from a separate RNG seeded from `time/os.getpid()` — the seeded rng is only for cases_replay; (c) capture `root_pid` once right after `await_shell` (both at launch and in every relaunch branch), behind `be.guard()`, and on POSIX require `shell_count() == 1` else leave the depth check off with a visible note row.
   - Selftest: write `.appci_pid` containing this run's own first tokens (same seed) with bogus pids; `Ctx.shell_pid` must return None until a token appended after the snapshot appears. A run.py unit: root pid is not recorded when `shell_count()` returns 2.

2. QUITS APP — raw `exit` typed after an unchecked `until`
   - Where: cases.py:2431 (`c_sub`: `until(len>=2, 6)` result ignored), cases.py:2503 (`c_sub_sh_probe`), cases_caseremote.py:1150-1152 (comment literally says "if it started at all"), cases_caseremote.py:1620 (nested nu), cases_ui.py:889, cases_netless.py:135 (typed even when `secs is None`), cases_coverage_a.py:221.
   - Sequence: the subshell command (`fish`, `nu`, `zsh -f`, `ssh fake`) never ran (agent routing, lost Enter, binary missing on the remote); the wait times out; `exit` runs in the root shell (POSIX: pane closes → "APP DIED", classified by `classify_death` as a harness defect but the pass is still relaunched and the case lost).
   - Fix: a single `Ctx.exit_nested(base)` that types `exit` only when `in_subshell(base) is True`, or `shell_pid() != root_pid`, or `where_now() == "remote"` (per platform), and notes "exit NOT typed" otherwise; migrate every call site. Selftest: a source lint in selftest.py asserting `be.type("exit"` appears only inside `Ctx` and `cases_caseremote.leave`.

3. POISONS LATER CASES — probe-then-`exit` prompt race (the `tac` incident, still present in two helpers)
   - Where: `return_to_local` cases.py:810-813, `leave_nested` cases.py:859-862.
   - Sequence: `where_now()`/`shell_pid()` return when the token is in the EFFECT file; the probe's own `P` line is appended by PROMPT_COMMAND a few ms later. `before = prof.prompts()` is sampled in that window, the late probe prompt satisfies `until(prompts() > before)`, `sleep(0.5)` runs while the ssh client / subshell is still tearing down, and the next probe's first characters go to the dying process. r7w fixed this only in `leave_remote(level=True)` (cases.py:902-903).
   - Fix: one probe primitive (see cross-cutting A) that returns only after BOTH the effect and `prompts()` advancing past the pre-probe count (bounded, skipped when `not prof.local_prompts`); `exit` waits are then for a prompt newer than the probe's. Selftest: fake `prof.prompts()` that increments 100 ms after the effect file; assert the helper does not type `exit` until after the increment and then waits for a further increment.

4. FALSE PASS WITH CONTRADICTING DETAIL — ssh trial verdict read before the remote hook logged the line
   - Where: `ssh_trial_note` cases.py:2570-2581 called at 2621/2664/2746 right after `until(tok in rl.txt)`; `ssh_trial_facts` cases.py:328-331.
   - Sequence: the remote `rec.log` C-line is written by PROMPT_COMMAND after the command; the effect file exists first; `remote_new` is empty → note "#1292: no ssh hold was seen and the remote shell logged nothing for the line" (or "never reached the remote shell") while `r.check(got == exp)` passes. The app-log `hold_seen` can lag the same way.
   - Fix: before the verdict, `until(len(prof.commands(remote=True)) > r0, 3)`; pass `effect_seen=tok in effect file` into `ssh_trial_facts` and set `exact/remote` from it when rec.log is silent; make a non-exact verdict on a trial whose effect succeeded a `r.unverifiable`, not a note. Selftest: `ssh_trial_facts(lines, line, [], effect_seen=True)` text must not contain "logged nothing"; a verdict/effect disagreement yields noref.

5. FALSE PASS — injector accounting is informational and its window clips late Enters
   - Where: run.py:494-501 (`r.note` only), injacct.py:99-101 (`window` ends at `max(wall1)+3.0`), join 78-96.
   - Sequence: a held ssh-typeahead Enter or a slow-classifier Enter is processed >3 s after the last injector call → not counted → "Enter: sent 2, app saw 0"; or a genuine loss of 69 keys — either way the case passes on its effect check. Windows is excluded from accounting altogether (`a.backend in ("wayland","x11","mac")`) although `WindowsBackend.calls` exists.
   - Fix: window end = `time.time()` at accounting; include Windows; when `lost > 0` (or `enters_sent > enters_seen`) and the case is not in LOSS_CASES and has no fails → `r.unverifiable(...)` (status noref), not a note. Selftest: synthetic log with `enter_begin deferred=false` at wall1+5 s → `enters_seen == 2`; Result with empty fails + lost>0 → status "noref".

6. FALSE FAIL — harness probe lines counted as the case's commands
   - Where: `HARNESS_MARKER_RE` cases.py:1302 matches only `alive_*/proof_*.txt`; `cmds_since` used by `effect_run` (1675, 1701) and many cases.
   - Sequence: the root-pid probe (`sh -c 'echo p…:$PPID >> ~/.appci_pid'`, run.py:459) and `.appci_where` probes are logged by the rc hook at the NEXT prompt; on msys bash / a loaded Mac that lands after the case's `cmd_mark()` (0.4 s of `reset()` is the only margin) → "shell received 31 commands for 30 typed, first mismatch: echo wT… >> ~/.appci_where" (already seen for the remote side, cases_caseremote.py:34-38).
   - Fix: extend the regex with `\.appci_(pid|where)`; selftest: `cmds_since` drops both forms.

7. FLAKY — rec.log compared after a fixed sleep
   - Where: `effect_run` cases.py:1662-1664 (`until(effect) ; sleep(0.5) ; cmds_since`), also 2430-2434, 2498-2506, cases_coverage_a.py:217-220.
   - Sequence: the n-th command's C line arrives with its prompt; under load >0.5 s → "shell received n-1 commands" false fail.
   - Fix: `until(len(cmds_since(mark)) >= n, 5)` before comparing (cross-cutting D). Selftest: fake prof whose commands() fills 800 ms after the effect; effect_run must not fail.

8. FALSE FAIL (crash) — `Profile.read` has no PermissionError retry
   - Where: profile.py:542-549 vs `Ctx.lines` 683-690 and `LocalRemote.lines` lab.py:177-184 which retry (r18cm).
   - Sequence (Windows): `prof.prompts()/commands()` polled inside `until` lambdas while the shell holds rec.log open → PermissionError → "harness exception" fail, and in `return_to_local` the pane is left where it is.
   - Fix: same 5-try loop. Selftest: monkeypatched `open` raising PermissionError once → read succeeds.

9. POISONS LATER CASES — reference-terminal state leaks via globals
   - Where: `prof.ref_active`, `be.guard_override` set at cases.py:3073-3075, 3093, 3125-3130, 3154-3181; reset only in `close()`/`switch()`.
   - Sequence: a case raises between `switch("reference")` and `close_ref` (or a `RuntimeError` from `guard()` inside `switch`) → every later `prompts()/commands()` reads refctl and every injection is guarded against the (closed) reference → cascades of false fails until the pass ends.
   - Fix: run.py post-case invariant: if `prof.ref_active or be.guard_override`, reset both, `be.refocus()`, and add a note row; also `ctx.remote`/`prof.remote` must be `LocalRemote` when `ctx.arm is None` (cases_lab.py:113-116/164-167 restore only via its own finally). Selftest: Result row emitted when the invariant trips.

10. EVIDENCE LOSS — per-case app-log slice is Windows-only
   - Where: run.py:547-552 (`save_case_log` under `if win`); Linux/Mac keep only the end-of-pass `phosphor.log` + retained `.in_session.N` chunks (run.py:735-742), so a long pass rotates the failing cases' lines away.
   - Fix: call `save_case_log` on every platform using `ctx.log_since(log_off)` (LogOffset is rotation-aware, cases.py:534-608) instead of the timestamp cut; cap as today. Selftest: a rotated pair of files + LogOffset → slice contains both halves.

11. FIXED SLEEPS that should be events
   - run.py:394/542/569/595/621/640/657/681 `sleep(4)` first-run settle → wait for `hop=view_mode` in the app log (already parsed by `log_view_mode`).
   - run.py:946 `sleep(12)` PowerShell await_shell → effect probe (cross-cutting A).
   - cases.py:908 `sleep(min(timeout, 2.0))` in leave_remote POSIX-less path → `wait_ssh_gone` already exists; use it on every platform and drop the sleep; cases.py:813/862 `sleep(0.5)` after exit → prompt-after-probe wait.
   - run.py:959 `sleep(1.5 + attempt)` set_ps_pane_state → effect probe.

12. Wedged-pane detection only on Windows (run.py:662): on POSIX a program left in the foreground that ignores Ctrl-C (vim/less/`cat`) silently fails every later case. `pane_responsive` is platform-neutral; run it after any failing case everywhere.

## Cross-cutting fixes (each removes a class)
A. One probe primitive `Ctx.probe(cmd, token, homes, remote=False)`: guard() first, snapshot every target file's size, type, wait for the token appended AFTER the snapshot AND for `prompts()` (local or remote, when a hook exists) to advance, return (where, pid). Implement `where_now`, `shell_pid`, `pane_responsive`, `set_ps_pane_state` on it. Kills #1 (stale tokens), #3 (prompt race), part of #11.
B. Root PID at launch + `exit_nested`: record `root_pid` in `await_shell`'s success path (all launch/relaunch sites go through one `relaunch()` helper, which run.py currently repeats 6 times), and a lint that bans raw `type("exit"`. Kills #1b, #2.
C. Per-case log slice on all platforms + accounting window to case end + `lost>0` → noref. Kills #5, #10 and makes "pass with contradicting detail" impossible for key loss.
D. `Ctx.await_commands(mark, n, timeout)` used wherever `cmds_since(mark)` is compared; extend `HARNESS_MARKER_RE`. Kills #6, #7.
E. Unseeded probe tokens (keep `ctx.rng` for replay) + size snapshot on every probe file. Kills the whole "same token in an earlier run/pass" family (r18u19 and #1).
F. run.py post-case invariants: `ref_active False`, `guard_override None`, `remote is LocalRemote` unless in a lab arm, `pane_responsive()` after a fail on all platforms, `prof.read` PermissionError retry. Kills #8, #9, #12.

### Critical Files for Implementation
- /home/winters/git/phosphor-merge/script/realapp/appci/cases.py (Ctx: where_now/shell_pid/leave_*/return_to_local 771-931, HARNESS_MARKER_RE 1302, effect_run 1641, ssh_trial_facts/note 215-342 & 2570-2581)
- /home/winters/git/phosphor-merge/script/realapp/appci/run.py (per-case loop 407-683: root pid capture 455-461, accounting 494-501, save_case_log 547-552, relaunch branches, await_shell 937-947)
- /home/winters/git/phosphor-merge/script/realapp/appci/injacct.py (window 99-101, join/account 78-125)
- /home/winters/git/phosphor-merge/script/realapp/appci/profile.py (read/commands/prompts 542-567)
- /home/winters/git/phosphor-merge/script/realapp/appci/selftest.py (pins: leave_nested 865-875, where-probe stale tokens 878-895, leave_remote 1448-1490 — add the new pins beside them)