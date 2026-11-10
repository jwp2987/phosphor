# Round 12: every Windows ssh case was a quiet SKIP (#2043)

Nothing was run on Windows when this was written. Python selftests only.

## Evidence

- Round 11 (build/r11a c61b6a3b8), job app-test-windows: 22 rows SKIP, all six passes, reason
  `fakesshd_win.py did not listen on 127.0.0.1:<port> (see ssh/fakesshd.stderr)`.
- `run_slim/logs/<pass>/fakesshd.stderr`, identical in all six passes, and identical in round 10 (build/r10a 5dd50d49c):

      File "...\script\realapp\appci\fakesshd_win.py", line 26, in <module>
          import ptyhost
      ModuleNotFoundError: No module named 'ptyhost'

- Round 9 (build/r18w9 41340e2fd): the server ran (`ssh_connect_typing` PASS x4; its stderr holds paramiko banner noise, not a start failure).
- `git merge-base --is-ancestor 1766c6a2b`: not in r18w9, in r10a and r11a. 1766c6a2b (#2007) added `import ptyhost` at
  `script/realapp/appci/fakesshd_win.py:26` (now line 30). `ptyhost.py` is tracked and sits next to it.
- `script/ci/nodes/windows.ps1:44`: the Windows machine's Python is the python.org embeddable layout (`python312.zip` + `._pth`). With a `._pth`
  file `sys.path` is exactly the file's entries; the folder of the script being run is not added. `run.py:13` and every case module insert
  their own folder, which is why the harness itself and `sshd.py`'s own `import ptyhost` (inside the `run.py` process) work.
  `fakesshd_win.py` is started as its own process (`sshd.py`, `subprocess.Popen([sys.executable, .../fakesshd_win.py, ...])`) and had no such line.

## Verdict

TEST fault (harness). Not APP. Not ENV: paramiko imports (the traceback is past `import paramiko`), no port or firewall question, no timeout
(the process exits at once and the listen loop sees `poll()`), and nothing has to be installed or changed on the Windows machine.

Consequence beyond the six cases: the #2007 repair has never executed on Windows. Its first real evidence is the next run.

The same fault, latent: `lab.py` has a `__main__` entry (`lab.py stop <run dir>`, called by `windows/ci/after_job.ps1` with the same Python)
and imports `ci_env` at line 11 with no path line. No Windows lab job has been played, so it has not fired.

## Fix

1. `fakesshd_win.py`, `lab.py`: `sys.path.insert(0, <own folder>)` before the sibling import.
2. Selftest `appci_programs_find_their_siblings`: every appci file run as a program (shebang or `__main__`) that imports a sibling has a
   `sys.path.insert` before the first such import (AST scan, line order), and every program `sshd.py` starts is in the scan. A static rule
   because `fakesshd_win.py` cannot be imported off Windows (`msvcrt`, `ctypes.wintypes` calls).
3. The reason text quotes the stderr tail (`sshd.stderr_tail`), so the row carries the cause.
4. Loudness. `sshd.required()` = `CI` set or `APPCI_REQUIRE_SSH=1`. In a non-sim pass that requires the host and has none (start failed or the
   `ssh fake true` probe failed): one FAIL row `ssh_test_host` (exit code, release gate, trend failing list), and every ssh case of the pass is
   NOREF through `cases.need_ssh` -> `cases.HostMissing(Skip)` -> `run.skip_status`. `results_trend.noref_kind` files `TEST HOST MISSING` under
   `other`, so the trend report names each lost case.

## Rejected

- Making each ssh case a FAIL: 22 red rows that read as app failures for one harness cause. One FAIL row plus named NOREF rows says the same
  with the cause in one place.
- NOREF only, no FAIL: NOREF does not fail the job or the release gate (`check_release_gate.py`), so the area could stay lost for rounds again.
- Changing `skip_status` so any skip after a `no_reference` note becomes NOREF: changes rows of unrelated cases. The new status is tied to one
  exception type.
- Requiring the host everywhere, local runs included: a developer machine without sshd is a legitimate skip (`APPCI_REQUIRE_SSH=1` opts in).
- An opt-out variable for CI: no CI machine is meant to lack the host; an opt-out is how the area would get lost again.
- Fixing it on the machine (adding the folder to `python312._pth`, or `PYTHONPATH` in the job): an environment change for a repo fault, and the
  next machine would have it again.
- `python -m` / package imports for appci: a larger change to every entry point for no gain here.

## Risk

- A CI app job on a machine with no working sshd now goes red where it was quietly green. That is the intent; rounds 9 to 11 show the Linux
  and macOS jobs always had a host (no such skip in their results).
- The probe (`ssh fake true`, 15 s) failing in CI is now a FAIL row too. Same reasoning: without it no ssh case of the pass is judged.
- Nothing here was executed on Windows. With the import repaired, the #2007 code (msys pty host, arm measurement) runs there for the first
  time; new failures in the ssh cases are then findings about #2007 or the app, with the server's stderr in the row if the server is the cause.
- Not addressed (seen in the same results, not this task): `ssh_connect_typing_wrapper` / `ssh_nested_typeahead_wrapper` skip in the Git Bash
  pane in rounds 9 to 11 because the job does not set `APPCI_SSH_WRAPPER=1`.
