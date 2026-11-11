# r19-targeted: a targeted tier for `ci_seq.py`

Owner decision 2026-11-11: fixes coming out of a round are proven with a TARGETED run first (only the cases that failed, on the OS where they failed,
plus the cases that touch the changed code), and only then the whole harness. Harness and CI only; no app code. Nothing was compiled and no pipeline
was played: the self-tests and `--dry-run` are the evidence. Worked, unproven.

## What exists (evidence)

- `script/ci/ci_seq.py` has three tiers; none can be told "these cases only". The smallest app run is the fix tier (SMOKE + PROVING on every platform).
- The harness can already narrow a run at play time: `wayland_session.sh:28` reads `APPCI_MODES` / `APPCI_CASES`, `mac/ci/app_test.sh:113` the same,
  `windows/ci/app_test.ps1:88` reads `APPCI_CASES`, `run.py:184` reads `APPCI_WIN_SHELLS`. Windows ignored `APPCI_MODES` (`app_test.ps1:17`, a fixed default).
- `run.py:255-270` refuses an unknown case name and an empty selection (#1508), so a wrong name is a FAIL row, not a green empty run.
- `run.py:252,280` and `cases_robust.py:980` change behaviour only for `APPCI_TIER == "smoke"`; any other value is the full-strength case.
- `.gitlab-ci.yml` `app-test-linux-wayland` runs a second, fixed-list xwayland pass unless `APPCI_TIER=smoke`.
- `collect_results.py` `diff()` takes the newest earlier run with the platform as the app baseline.

## Chosen design

1. `--tier targeted --cases-file FILE`. The file is the whole plan (platform -> cases and arm filters; machine -> suites; optional filters). It is
   validated in full before anything is played; every problem is named; exit 2.
2. Pure functions `targeted_spec` (validate, normalise), `targeted_vars`, `targeted_plan` (per machine, in play order, plus the jobs NOT played), and a
   driver `run_targeted(plan, ops)` that takes the pipeline operations as callables. `main()` hands it its own `play` / `wait` / `play_wait` /
   `ensure_lab_image` / `run_lab_helper`; the self-test hands it a recording fake. So the self-test runs the real driver, not a description of it.
3. Per machine one job at a time: (box only: lab image, lab helper when any lab job is planned), build (only with an app job), suites, app jobs with
   cases (wayland, x11, lab). Mac / Windows builds start before check-linux and are waited for before that machine's suites (a build and a suite never
   share those machines); the box build runs beside the box suites as in every tier.
4. `APPCI_TIER=targeted` for the app jobs: recorded in `meta.tier`, full-strength cases. The Wayland job's xwayland pass is skipped for it (one yml line).
5. A job the file does not name gets the timeline status `not in this targeted run`, verdict SKIPPED (neutral, like `unsupported on platform`).
6. `collect_results.py --tier targeted --cases-file FILE`: `tier: targeted` and the file under `targeted` in the manifest, a README line; a targeted run
   is never an app-diff baseline.
7. `script/ci/failed_cases.py` writes the file from a collected run or a directory of saved traces.

## Rejected

- **Reusing an earlier build** (the fix tier's `-reuse` jobs). The owner's text says it builds; a proof run of fixes is by definition a new app.
- **`APPCI_TIER=smoke` for targeted jobs.** A case that failed in a candidate run would be re-run at the smoke settings (10 minute soak, Windows
  `FIX_TIER` options) and "pass" something else.
- **Checking case names in `ci_seq.py`.** It imports no harness code by design, and a static scan of `@case("...")` misses generated names (10 of 245
  non-lab names: the `agent_restart_*` and `shell_*` families). `run.py` already refuses unknown names; `failed_cases.py` (which may import the registry) filters what it writes.
- **Per-case mode filters.** The jobs take one `APPCI_MODES` / `APPCI_WIN_SHELLS`; the file carries the union for the platform.
- **Playing a job twice** (once plain, once with the fake model). `ci_seq.py` never re-plays a job. Instead `run.py` plans the `agent@fake` pass of a
  targeted run from the selected names (`APPCI_TIER=targeted`), so a `PROVING_FAKE` case named in the file gets the scripted model in the same job.
- **Mapping a changed file to every case in it** (`--changed-since`). `cases.py` changed, so that would be all 34 of its cases. The helper compares
  each `@case` function, and the top-level helpers and tables of the same file it uses, between the two commits (AST, so comments do not count).
  It does not follow imports: a change in `winmap.py` / `backend.py` / `run.py` is listed as not mapped, never guessed.
- **Keeping rows that are not cases** (`needs_owner`, `startup_shell_pass`, `preflight`). `run.py` refuses a `--cases` list with an unknown name, so
  one such name would fail the whole job; they are listed under `info.dropped` instead.
- **Targeting the xwayland pass or the replay.** Each has its own fixed list / job; listed as not targeted.

## Risk

- Play-time variables override the job's own `variables:` (GitLab precedence; the lab jobs already rely on it for `APPCI_CASES`). Unproven for
  `APPCI_WIN_SHELLS` on `app-test-windows` until a run shows the pass list in its log.
- `app_test.ps1`'s `param` default is now an expression reading `APPCI_MODES`; not parsed on Windows here (`script/ci/parse_ps1.ps1` runs in CI).
- A follow-on row adds the case it names (`left_agent_mode_after_soak` adds `soak`: two hours on Linux). The file is a proposal; read it before playing.
- A targeted PASS can be mistaken for a round's result. The summary line, the README line and the manifest tier say what it is.
