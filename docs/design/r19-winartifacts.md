# r19-winartifacts: a failing app-test run keeps its logs and screenshots (#2123)

## Cause (TEST/CI fault, not an app bug)

Round 14, pipeline 3052606915, `app-test-windows` (job 17921510251). From the job trace:

- `step_script` ended 11:37:06 (the run itself took 7 h 06 min); `after_script` began 11:37:06 and was stopped 11:42:36, 5 min 30 s later:
  `WARNING: after_script could not run to completion because the timeout was exceeded` (exit status 0xc000013a; the
  `Entering debug mode` line before it is PowerShell answering the runner's Ctrl-Break, a result of the stop, not its cause).
- In those 5.5 minutes the trace shows 1067 `after_job: recorded ... pid N` lines (1061 `already gone`, 6 `not ours`), then
  `leftover processes ... none`, `restore already done`, then the `$ ... results_scrub.py --tree appci-out` command line and nothing after it.
  `slim_appci_run.py` never started, so `run_slim/` did not exist: `appci-out/windows/run_slim/shots/: no matching files`, same for `logs/`.
- `script/realapp/windows/ci/after_job.ps1` ran `Get-CimInstance Win32_Process -Filter "ProcessId = N"` once per recorded line. The app's Job
  Object members are recorded at every app stop (`jobmember <pid> <creation time>`), so a long run records about a thousand. The trace lines
  carry no timestamps, so the split between that loop and the scrub is not measured; 330 s over 1067 queries is 0.3 s each, a usual cost of a
  WMI query on that VM, and the scrub printed nothing. The loop is the step that used the limit up.

Same round, `app-test-linux-wayland` (job 17921510248): `step_script` ended 10:11:08, the `after_script` printed its two `teardown` lines and
`$ python3 script/ci/results_scrub.py --tree appci-out`, and was stopped 15 minutes later (10:26:08, `Session terminated, killing shell`,
then `failed to kill process PID=..., likely process is dormant`) with no line from the scrub: no `run_slim`. Cause, measured here:
`results_scrub.SECRET`'s `url with credentials` rule, `\w+://...`, retries `\w+` from every character of a word run. One 40 kB line of hex takes
3.2 s, and the cost is quadratic (a megabyte: hours). The scrub walks the whole `appci-out` tree (extracted app, private HOME, logs), so one long
unbroken line anywhere holds it. The Windows scrub step had the same rule and had just started when the limit fell.

The fault in the design is larger than that loop and that rule: the two steps that decide whether a failing run can be diagnosed (scrub, slim: Pillow
re-encodes every screenshot) sat in `after_script`, behind GitLab's default 5 minute limit, which all `after_script` steps share. Linux and
macOS have the same layout (`teardown.sh`, scrub, slim); they have not hit the limit yet, and macOS re-encodes with `sips` when Pillow is absent.

## Fix

1. The job script makes the artifacts ready itself, on every exit path it controls: `script/ci/appci_artifacts.py OUT` (whole-tree scrub, then an
   unbudgeted slim) from `wayland_session.sh` (EXIT trap, after teardown), `mac/ci/app_test.sh` (EXIT trap, after cleanup) and
   `windows/ci/app_test.ps1` (after its try/catch/finally and the final report). Only the job timeout applies there.
2. `after_script` keeps a bounded net, `appci_artifacts.py --fallback --root appci-out`: a finished pass that still matches the run
   (`run_slim/SLIM_DONE.json`) is kept and only the few small files written after the job script are scrubbed again; otherwise the pass is
   made there within 480 s, logs first, then the shots of non-passing cases, the rest listed in `shots/OMITTED.txt`.
3. `after_job.ps1`: one process snapshot; a per-PID query only for a PID the snapshot holds; one count line for the gone ones; its seconds.
4. `results_scrub.py`: the rule starts at a word's start (`(?<!\w)\w+://...`): linear, same matches (8 MB of hex: 1 s). `scrub_tree` names a
   file that takes over 5 s.
5. `RUNNER_AFTER_SCRIPT_TIMEOUT` stated on the three parents: `15m` on `.appci_linux` and `app-test-macos`; `app-test-windows` KEEPS the `"45m"`
   that #2114 (r19-winmisc) set, as headroom. It is not replaced: a larger limit costs nothing once the steps under it are bounded, and
   `selftest_r19_winmisc.py` and its docs section name it.

A passing run keeps exactly what it kept: the artifact paths are unchanged and the job script's pass has no budget.

## Rejected

- Only raising the after_script limit: it leaves the evidence behind a limit that other steps share, and the per-PID loop grows with the run.
- Only fixing the loop: the slim of a 7 h run's screenshots can itself take minutes; it would be the next step to hit the limit.
- Dropping the after_script pass: a cancelled job's script is killed and never reaches its own pass.
- Slimming inside `run.py` after each case: touches the harness's hot path and every platform's report step for no gain over the exit path.

## Risk

- Not run anywhere: the PowerShell changes were parsed (pwsh 7) and the loop was exercised with stubbed `Get-CimInstance` / `taskkill.exe`;
  Windows PowerShell 5.1 on the VM parses them first in the job's `parse_ps1.ps1` gate.
- The job script's pass adds its time (scrub + slim, minutes on a long run) to the job, inside `deadline.py`'s 5 min reserve; if the job
  timeout cuts it short the net finishes it within its budget. `RESERVE_S` was left alone (its self-test pins the numbers).
- A job GitLab kills at the job timeout may get no `after_script`; that is as before.
- `RUNNER_AFTER_SCRIPT_TIMEOUT` is read by the runner (19.5.1 on the VM per the trace); if a runner ignores it the net still fits in 5 min for
  a job whose script finished (it then only re-scrubs a handful of files).
