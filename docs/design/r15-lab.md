# Round 15: the lab's remaining run gaps that are code (#2062, #2063, #2005)

Base: integ/r14 `75847d84c`. Nothing was compiled; no lab, app or CI job was run. The appci self-test (215 checks) and
`lab_helper.py --self-test` pass. Source of the gaps: `docs/design/r14-test-gaps.md` G6, F8, 7.0 d and e.

## 1. #2062: the helper arms are NOREF every run

**Cause.** A helper arm needs a static musl `phosphor-remote` named after the app's version. Round 12 built the recipe
(`script/ci/lab_helper.py`, manual job `lab-helper`, a cache in the runner user's home on the box). Two things were left:

- The box cannot run the recipe: #2062 says zig is not installed there (`missing_tools`, `lab_helper.py:245` at the base; the job
  installs nothing by decision; the box was not looked at for this note). The arm's reason said "play the manual `lab-helper` job"
  (`lab_helper.py:144` at the base), which is not what is missing.
- The Mac and the Windows VM have no cache and cannot build a Linux binary. `docs/design/r12-macssh.md` 4 proposed
  `needs: lab-helper`; `.gitlab-ci.yml` still said "carried by hand".

**Fix.**

- `lab_helper.py`: `build` also writes `export.json` (pipeline, job) into `helper-artifacts/`. New `artifact_entry()`: the artifact in
  a checkout is accepted only as that checkout's helper (`meta.json` key == the key computed there; where no key can be computed,
  `export.json` must name the job's pipeline). New command `artifact`.
- `cases_lab.resolve_helper_bin`: explicit `APPCI_LAB_HELPER_BIN`, then the artifact, then the cache.
- `.gitlab-ci.yml`: `app-test-lab-macos` and `app-test-lab-windows` get `needs: [build, {job: lab-helper, artifacts: true, optional:
  true}]` and `APPCI_APP_PINNED_DIGESTS: "0"` (r12-macssh items 1 and 3; item 2, the bundle's name, was #2042). `lab-helper` uploads its
  artifact `when: always`.
- `NEEDS:`. A build that cannot start for missing tools records one line, `NEEDS: zig, cargo-zigbuild installed for the CI runner user
  on the build box (owner: machine change; ...)`, in `helper-artifacts/NEEDS.txt` and `<cache>/NEEDS.txt`. The helper arms quote it. On
  Linux, with no record, the resolver adds the line for zig when zig is not where the build looks. The first successful build removes
  the record, so the line goes away when the owner installs the tools and not before.

**Deviation from the task's wording, on purpose.** The task says the lab jobs take the artifact through `needs: lab-helper`. The Linux
lab job does NOT get that `needs`:

- It runs on the box, where the cache is. The artifact is a copy of the cache entry; it adds nothing there.
- GitLab does not offer a job whose `needs` names a manual job that has not been played (the documented behaviour of `needs` on an
  optional manual job; not tried here). `ci_seq.py` plays `app-test-lab-linux` in every fix tier and does not play `lab-helper`
  (`r12-policy.md` 3: rejected for the #1921 starvation, and listed as open item 5). With the `needs`, the fix tier's lab job would stop
  being playable the day this merges.

The Linux job still uses a `helper-artifacts/` that is in its checkout, first. The Mac and Windows jobs have no other source, are in
no tier, and are played by hand: there "play `lab-helper` first" is the order anyway, and a failed `lab-helper` (it is
`allow_failure`) does not block them.

**Rejected.**

- Fetching the artifact through the jobs API in `before_script`. `CI_JOB_TOKEN` can ask for "the latest successful job of a ref", not
  for this pipeline's job; that is the stale-helper case the key check exists for, reached by a longer road.
- Probing cargo and the rust target from the lab job for the NEEDS line. They depend on `CARGO_HOME` / `RUSTUP_HOME`, which only the
  `lab-helper` job sets; the probe would report tools missing that are there. zig is looked up in the runner user's home, which both
  jobs share.
- SKIP instead of UNVERIFIABLE for a helper arm without a helper. Round 12 decided UNVERIFIABLE (noref); both are "not a pass", and
  `results_trend.py` lists both as never judged. Only the text changed.
- Playing `lab-helper` from `ci_seq.py`: open item 5 of `r12-policy.md` 3. Not decided here.

**Risk.** `needs` on a job that is manual and `allow_failure`: stated from GitLab's documentation, never played. If GitLab offers the
Mac job without `lab-helper` having run, the job runs with no artifact and the helper arms say so. `artifacts: when: always` on a failed
`needs` job: if GitLab does not hand a failed job's artifact on, the Mac arms give the cache reason instead of the NEEDS line; still
UNVERIFIABLE.

## 2. #2063: lab families that never played

**Finding 1: `PROVING_LAB` was assigned twice** (`cases.py:130` and `:132` at the base). Two round 14 branches each rewrote the tuple
and the merge kept both statements. Python used the second (no `lab_agent_nld_predict_ssh`); `ci_seq.proving_lab_globs` returned the
first `Assign` it met (no `lab_raw_key_arrival`). So the fix tier's lab job has never been given `lab_raw_key_arrival`, and the
self-test that checks the globs checked the other list. Fix: one assignment, one `PROVING_LAB += (...)` line per later entry (a new
entry is a new line, so two branches cannot collide in one statement again), and `ci_seq.fold_tuple_assignments` reads what Python
computes. That edit to `ci_seq.py` is the reader only; no tier plan changed.

**Finding 2: no rule put a lab family in a tier.** Of the 46 registered lab families, 36 have no row in any committed result (10 do:
the three `lab_agent_remote_*`, four `lab_fam_agent_*`, `lab_enter_start`, `lab_enter_no_network_wait`, `lab_ctrl_c_no_esc_i`). 28 of the
36 are now in `PROVING_LAB` (list in `docs/testing.md`, round 15 section), by the owner rule that a case that has never passed runs in the fix tier. Scenario users and
network faults are outside the quick matrix; six such arms are named exactly, which `run.py` runs whatever the matrix says.
`cases.LAB_NOT_IN_FIX_TIER` holds the eight families that job does not play, with reasons, and the self-test
`r15_every_lab_family_is_in_a_tier` fails for a family that is in neither.

Cost (registry count, not measured): 76 rows before, 331 now at the quick matrix; 159 of the new rows are no-helper rows, about 80
minutes at the 30 s a row of the one timed run. This is a real change to the fix tier's length. If it is too much, the move is one
line per family; the tier plan and the owner decide.

**Finding 3: three helper-free families had no driver for their app layer.** Each ended UNVERIFIABLE by construction.

| case | what it said | what is true | driver now |
|---|---|---|---|
| `lab_diff_panel_remote` | "the harness has no keyboard path to open the code-review panel" | `workspace:toggle_right_panel` is an editable binding (`workspace/mod.rs:642`) and so a command-palette entry, "Toggle code review" | the palette; the app confirms with a trace line |
| `lab_remote_edit` | "no pointer click on the file tree" | the palette's "Toggle project explorer" opens the tree AND focuses it (`workspace/view.rs` `open_left_panel_view`, `focus_active_view_on_entry`); the tree has Up / Down / Enter (`code/file_tree/view.rs:151-173`); Down clamps at the last row (`:3213`) | one-file repository, palette, Down x4, Enter; the app's `Opening remote file` line is the proof |
| `lab_git_write_remote` (app layer) | "no keyboard path to the code-review git dialog" | true for the dialog (`git_dialog/mod.rs:77` binds Escape only), not for the panel | panel by palette, `Commit` and `Confirm` by pointer (Linux) |

App change, trace only: `hop=right_panel open=<bool> repo_context=<bool>` from `Workspace::update_right_panel_open_state`
(`right_panel::right_panel_trace_line`, unit test `right_panel_trace_line_format_is_pinned`). Flags only. No kill switch: it changes no
behaviour. Without it "the panel opened and showed nothing" and "the palette ran something else" cannot be told apart, and the first is
a FAIL while the second is not.

Design points that keep a wrong result from passing or a harness miss from failing:

- Diff panel: after the panel opens nothing is typed into the app, because the focus may be in the panel. The no-helper arm needs a
  prompt to return; the foreground is held by `sh -c 'until [ -e <gate> ]; do sleep 0.2; done'` typed BEFORE the panel opens, and the
  harness creates the gate over its own ssh connection. Panel confirmed open and no snapshot: FAIL. No confirmation: UNVERIFIABLE.
- Remote edit: nothing is typed unless the app logged that it opened that file. The conflict oracle does not read the screen: after a
  change made on the host and a second save, the host's change must still be in the file.
- Git write: the verdict is a pure function of (how far the driver got, the route lines, the host's last commit). A call on the wrong
  backend fails however the driver fared; clicks with no git call and no commit are UNVERIFIABLE, not a failure of the app.
- fish / nu login shells have a plain session by decision (#2019, `DECLINED.md`): the three cases report that limitation, read from the
  app log of the connect, instead of failing on a feature that session does not have.

**Rejected.** A FAIL whenever the diff snapshot is missing (the old text said UNVERIFIABLE; without the trace line a palette miss would
fail the app). Placing the pointer on the file tree by OCR for the editor (a keyboard path exists and needs no OCR). Driving push /
pull / stage / discard through the app too (four more OCR-placed dialogs for one routing line each; the commit proves the route).

**Risk.** The drivers have never met the app. Likely first-run faults, each of which ends UNVERIFIABLE or as a labelled check, not as a
pass: the palette's best match for the typed name is another entry; the file tree's first root is not the remote repository; the editor
pane does not close because the save was refused and the buffer still counts as changed after the undo (then `finish_remote`'s check
fails with "exit did not return": a harness fault to read as such); OCR does not find `Commit` on Wayland.

## 3. #2005: "app logged N more than were injected", macOS TAINTED rows

Checked against the committed runs `2026-11-10_build_r11a_3049602519` and `..._r12a_3050758880` (both after 15093ac33, the fix the issue
describes; r12a also after #2041):

| run | platform | rows | TAINTED | Enter sent != seen | surplus note |
|---|---|---|---|---|---|
| r11a, r12a | lab, Wayland, Windows | 100 to 336 | 0 | 0 | 0 |
| r11a | macOS | 158 | 0 | 0 | 6 |
| r12a | macOS | 160 | 0 | 0 | 6 |

The issue's two symptoms are gone. The six rows left are `latency_paint`, `latency_paint_ab`, `latency_key_views_ab` in both modes,
with 39, 78 and 78 surplus keys, the same numbers in both runs. Those are exactly the cases that post keys through `kl3`
(`cases.paint_samples`, `backend.MacBackend.kl`), and `kl()` kept no injector record. Verdict: TEST (harness), a second cause under the
same note. Fix: `kl()` records each command (`kl_keys`, `kl_posted`). Not changed: the note's wording and every check of `injacct`; a
kl3 key the app never logged now shows as lost, which is new information.

## 4. Owner questions this work ran into (not decided)

1. zig, cargo-zigbuild and the `x86_64-unknown-linux-musl` target for the runner user on the box (`r12-policy.md` 3, item 1). Until
   then every helper arm is `NEEDS: zig ...`.
2. Whether `ci_seq.py` plays `lab-helper` (`r12-policy.md` 3, item 5). It decides whether the Linux lab job can take a `needs`.
3. Upload / upgrade / never arms: pin the cached helper's digest into the dev build (`r12-policy.md` 3, item 3). Seven `lab_helper_*`
   families wait on it.
4. Whether the fix tier carries about 80 more lab minutes, or the never-played families move to another tier.
5. When the Mac and Windows lab jobs are first played (`r12-macssh.md` 4: after one Linux lab run with no FAIL).
6. `APPCI_FAKE_EMBEDDINGS=1` for the lab job: without it `lab_codebase_index_remote` SKIPs (8 rows).
