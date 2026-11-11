# Round 15: tier reach (#2056, #2057, #2058, #2063, #2064)

Harness and CI only. No app code. Nothing was compiled or run on a test machine when this was written; only the two
self-tests ran (`ci_seq.py --self-test`, `appci/run.py --self-test`, `collect_results.py --self-test`).

Source: `docs/design/r14-test-gaps.md` 5.1 (G1, G2, G6, G7) and 7.0 a to f. Owner 2026-11-11: every gap is in the harness.

## What was wrong (evidence)

| issue | cause |
|---|---|
| #2057 | `cases.PROVING` held only cases registered since 2026-11-08. The 13 daily-use cases of 7.0 b are registered, have `winmap` entries, and are in neither `SMOKE` nor `PROVING` (`cases.py:20`, `:33`), so the fix tier (`run.py:246`: `SMOKE + PROVING`) never played them. |
| #2058 | `ci_seq.py` marked `app-test-linux-x11` "skipped by tier" in the fix tier (old lines 810 and 855) and the job itself left with exit 77 when `APPCI_TIER=smoke` (`.gitlab-ci.yml`, old line 1260). `wayland_session.sh --mode x11` already runs both modes. |
| #2063 | `ci_seq.py` played a lab job only in the fix tier and only on Linux (`LAB_FIX_JOB`); no tier played `lab-helper`, `app-test-replay-linux`, `app-test-lab-macos` or `app-test-lab-windows`. The quick lab matrix leaves out 8 of the 46 lab families (`helper_install`, `helper_policy`, `helper_digest_mismatch`, `helper_missing_asset`, `helper_one_version`, `helper_quiet_fallback`, `helper_scp_fallback`, `password_prompt`: upload / upgrade / never arms and scenario users). A helper arm with no helper file was a NOREF row whose text did not say what to change. |
| #2064 | The candidate tier has played `integration-linux` (3 shards), `-pwsh`, `usage-test-linux` and `usage-test-macos` since #1502, and the collector parses the integration traces, but `collect_results.py` had no branch for `usage-test-*`: the job got a row in the job table and no result. The collector also named a run "candidate" when x11 ran over 2 minutes, which the fix tier now does. |
| #2056 | follows from the three above: `--tier candidate` did not reach the lab, the replay or the helper. |
| found on the way | `cases.py` assigns `PROVING_LAB` twice (lines 130 and 132, a merge of two branches). Python keeps the second, `ci_seq.proving_lab_globs` read the first: the fix tier played `lab_agent_nld_predict_ssh.*` and not `lab_raw_key_arrival.*`. |

## Chosen

1. **Fix tier slice (#2057).** The 13 names are appended to `PROVING` (their own lines). None needs the scripted model
   (`agent_approval` uses the configured provider and skips with its reason without one).
2. **X11 in the fix tier (#2058).** The x11 job no longer leaves in smoke; `ci_seq` plays it with `APPCI_MODES=terminal,agent`.
   On the box the app jobs are now played ONE AT A TIME by the driver, in a fixed order, each waited for before the next:
   wayland, x11, lab (fix: the `PROVING_LAB` globs; candidate / release: every family), replay (candidate / release). The
   resource group only kept them from overlapping; it did not order them. The reuse path follows the same order and now also
   waits for the box unit suites (it used to start at once, beside them).
3. **The lab in candidate and release (#2063).** New order on the box: `sshlab-image` and `lab-helper` right after
   `check-linux` (both are no-ops of seconds when nothing changed), the unit and integration suites, then the app sequence
   above. The Linux lab job gets a matrix that reaches every family once per shell of the quick set:
   `shells=bash,zsh,fish,nu;lat=lan,150;helper=nohelper,helper,upload,upgrade,never;faults=1;scenarios=1` (372 arms, 509 rows;
   the full matrix is 626 arms, 831 rows, about 10 h at the 43 s a row the committed lab runs took, over the job's 6 h).
   `lab_*` never sweeps in the replay (`run.py`), so the replay runs once, in its own job. macOS and Windows play their lab job
   after their app tests with the quick matrix and `APPCI_LAB=fixed` (the job's own variable); the two jobs now `needs` the
   `lab-helper` artifact (r12-macssh.md 4, item 1) and say `APPCI_APP_PINNED_DIGESTS=0` like the Linux job (item 3).
4. **Helper arms without a helper.** `lab_prepare` now SKIPs such an arm with `NEEDS: ...` naming the machine change (zig,
   cargo-zigbuild and the musl target for the runner user on the box, then the `lab-helper` job; #2062). This replaces the
   NOREF row: the task rule for a missing machine capability is a SKIP that names the change. A helper file that exists and
   is not a helper stays UNVERIFIABLE. `lab-helper` failing for a missing tool gets the tier verdict `NEEDS` (listed, not a
   pass, does not fail the tier); any other `lab-helper` failure is a FAIL.
5. **Collector (#2064).** `parse_usage` reads the suite's NDJSON rows from the trace into `unit/<job>.json` in the shape of
   a nextest suite (so totals, NEW / FIXED and the failure list work unchanged), keeps every skip with its reason, and the
   README lists them. `lab-helper` and `sshlab-image` keep their ERROR lines. The tier comes from `--tier` (ci_seq passes
   it), else from the app runs' own `meta.tier`, else from which suites ran (integration ran = candidate).
6. **A job the plan names that cannot be played** (absent, or still `created` because a need did not run) is a FAIL row with
   the reason. `play()` used to return quietly for anything that was not `manual`.
7. `proving_lab_globs` returns the union of every literal `PROVING_LAB` assignment. `cases.py` is not edited for it.

## Rejected

- The full lab matrix in the candidate tier: does not fit the job; the deadline would turn the tail into skip rows every run.
- Playing `lab-helper` after the unit suites: the Mac and Windows lab jobs need its artifact, and they can be ready first.
- Fetching the helper on the Mac / Windows through the API by job id: new code on two platforms for what `needs` does.
- Adding the lab jobs to `release_gate_jobs()` / `ci_seq.APP`: that changes what a tag requires (owner decision, see below).
- A new `SMOKE` entry for the 13: they are not input-path cases for every push after they graduate; `PROVING` is the rule.

## Risk

- GitLab: a job that `needs` a manual, `allow_failure` job that failed or was never played. The existing app-test jobs
  already need the manual build jobs, and a failed allowed job lets dependants run; if GitLab holds the Mac / Windows lab job
  in `created`, the driver reports it as a FAIL row ("not playable") instead of waiting forever. To check on the first play.
- Time. Fix tier on the box: the x11 pass is a second full fix-tier run (about +100 min measured for Wayland in r12a) and the
  13 cases add to each pass. Candidate: the lab adds an estimated 3 to 6 h on the box and 2.5 to 4 h on each of macOS and
  Windows. `app-test-lab-macos` gets `timeout: 6h` (it inherited 4 h).
- The reuse path of the fix tier now waits for the unit suites: slower start, same rule as a fresh build.

## Owner decisions left open

- Whether the lab jobs, the replay and x11-in-smoke become part of the release gate (`check_release_gate.py`).
- Which matrix the Mac and Windows lab jobs play in the candidate tier (quick here; r12-macssh.md recommended PROVING_LAB only).
- Whether the fix tier keeps the x11 pass once it has a clean run (it roughly doubles the Linux app time).
