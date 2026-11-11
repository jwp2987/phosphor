# Round 16: harness and CI decisions (owner 2026-11-11, "Fine I agree", item 21)

Harness and CI only. Nothing was compiled, no pipeline was played and no app was run: the three offline self-tests
(`run.py --self-test`, `ci_seq.py --self-test`, `collect_results.py --self-test`) are the only things executed. No app code changed.
Every item below is a decision already taken; this note records where each lands, what was rejected, and the risk.

## 1. `latency_key_log` judged on the key -> pty write share (Linux, Windows)

- Cause of the NOREF: `c_lat_key` calls `lat.evaluate(..., ref=None)` (no reference terminal exists for the app's own keydown -> frame clock),
  and `lat_checks(share_judges=True)` judges the row only when `rec["app_share"]` is usable; only macOS built one
  (`key_frame_regress_basis`, #2037). The key -> pty write figure was already measured everywhere (`key_write_pairs`, #1176) and only noted.
- Chosen: `cases.key_pty_share_gate(rec, kw, n)` attaches the figure to the record as `app_share` with `gate_only`, budget
  `APPCI_KEY_PTY_SHARE_MAX_MS` (1), usability by `lat.app_share_usable` (n >= 30, >= 90 % of the keys). `lat_checks` then does what it already
  does for macOS: over budget FAIL, usable and within JUDGED, otherwise NOREF with the reason. macOS returns early, untouched.
- Rejected: passing the figure as `lat.evaluate(app_share=...)`. That also moves the previous-build / baseline regression verdicts from the
  key -> frame median to the share (`regress_basis = app_share`), which nobody decided, and no Linux / Windows baseline carries the figure.
- Rejected: a re-measure before a share FAIL. The decision names a target; the hard bound on macOS has no re-measure either.
- Risk: a FAIL on a loaded machine. r12a medians are 0.33 to 0.69 ms, so the margin is about 0.3 ms at worst. `baseline.py` copies any
  `app_share` into a promoted baseline; it is read back only on the app-share basis, so a Linux baseline carrying it changes nothing.

- Found while testing the gate: `lat_checks` asks "is the record's verdict `no-reference`" before it reports NOREF. A regression WARN makes the
  verdict `warn`, so a row with no reference and no usable share would pass. Off macOS the question is now asked of the budget verdict
  (`share_judges` only, i.e. this case only). macOS is left exactly as it was, as the task says. The same hole exists there (a WARN on the
  median basis with no share figure passes the row): reported, not changed.

## 2. `sudo_su_real_shell`

- Before: an arm counted as "ran" when it started; the row's verdict was only the recorded checks; the zero-arm skip text began with prose and
  had `NEEDS:` in the middle.
- Chosen: `priv_arms_verdict` (pure). An arm passes when `nested_root_lines` returned True (the inner `id -u`) and it added no failed check.
  Zero arms: SKIP, text starts with `NEEDS:`. Arms ran, none passed: one more failed check. Metric `boundaries_passed`.
- In practice the old code could not PASS with zero passing arms either (a failed arm fails the row); the rule is now explicit and tested, and
  the inner-shell proof is part of "passed".

## 3 to 5, 8. `.gitlab-ci.yml` and `ci_seq.py`

- `APPCI_FAKE_EMBEDDINGS: "1"` as a JOB variable on the three lab jobs (a hand play gets it too), `"0"` on the replay, which extends the Linux
  lab job. Rejected: a play-time variable from `ci_seq.py` only (a hand play of the lab job would still skip the index case).
- `lab_tier_vars(plat, tier)`: Mac / Windows in `LAB_PLAT_PROVING_ONLY_TIERS = ("candidate",)` get `APPCI_CASES` = the PROVING_LAB globs.
  The release tier is not named by the decision and is left as it was; it is one word in that constant. An empty glob list passes no
  `APPCI_CASES` (an empty play-time value would replace the job's `lab_*` with nothing).
- `APPCI_SSH_WRAPPER: "1"` on `app-test-macos`. The two `*_wrapper` cases already run in any pane but Windows PowerShell; no case code changed.
- Guards: `selftest.py --require <names>` and `ci_seq.py --self-test`. Both checks already ran inside `selftest.py`; what was missing is
  anything that fails when the tail lines registering their files are lost in a merge, which is how #2086 happened.

## 6. The macOS close key

Read `docs/design/r10-r15-w4mac.md` ("Setting `APPCI_KEY_CLOSE=cmd+w` globally now" was rejected there until a passing run).
- The condition is a case verdict inside `run.py`; `app_test.sh` is a shell script that has exported its environment before the case runs.
  So the job names the key (`APPCI_KEY_CLOSE_WHEN_PROVEN`) and the proof case gives it (`cases_robust.proven_close_key`).
- Given only when the whole case has no failed check AND `app_chords_proven is True`. Taken back at the start of each proof, because the
  backend object and `os.environ` live across the two session-mode passes.
- Rejected: a separate `run.py` invocation for the proof before the main one (two app launches, two result directories, and the tier tools
  read one). Rejected: giving the key on `app_chords_proven` alone (the task says the case passed).
- Risk: about a dozen macOS cases change path at once after a passing proof, on a mechanism with no run yet. A case that presses the close
  key while a reference terminal is the injection target gets a `RuntimeError` from `app_chord` (refused by design), not a stray Cmd-W.

## 7. MOVE-AFTER-ONE-RUN

Marker comment and tuple `cases.LAB_MOVE_AFTER_ONE_RUN`; nothing moved. The self-test proves the candidate matrix already selects every arm
the entries select today, so the later move loses nothing.

## Not done / for the owner

- Whether the release tier's Mac and Windows lab jobs are also PROVING_LAB only.
- Which MOVE-AFTER-ONE-RUN families leave the fix tier after their run (asked then, per the no-graduation rule).
