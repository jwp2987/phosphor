# Round 12: four owner decisions of 2026-11-10 (task r12-policy)

Branch `fix/r12-policy`, from `integ/r12`. Nothing was compiled or run on a build: `rustfmt --check`,
the guards, the script tests and the self-tests only. Everything below is "worked, unproven".

## 1. The timer rule: the allowlist says what the code does (#1996)

Decision: keep the 9 Nov ruling. A give-up net may write the keys it held to the pty in typed order;
it never discards them.

Evidence. Three texts still stated the 8 Nov rule ("a timer never writes to the pty"): the header of
`script/input_path_timers.allow` (lines 4-6), the rule in `script/check_input_path_timers` (the
docstring and `RULE`, which is printed on every failure), and the `check_input_path_timers` row of
`docs/testing.md`. Each allowlist line was then read against its code:

| net | what the code does | line was |
|---|---|---|
| `SSH_TYPEAHEAD_PRECMD_WAIT_CAP` (`view/ssh_typeahead.rs:32`) | `view.rs` first-Precmd branch calls `ssh_hold_give_up_to_pty` | wrong: "hands back as text, writes nothing" |
| `REPORT_TAB_GIVE_UP` (`view.rs`) | `report_tab_gave_up` -> `keep_deferred_enter_text_hand_back` -> `give_up_deferred_enter_keys`: line and keys stay in the editor; a bare Enter and its keys go to the pty | incomplete: "hands the line back" |
| `YOUNG_DETECTION_CAP`, `YOUNG_DETECTION_BACKSTOP`, `arm_young_timer` | `finish_young_detection_wait` -> the same `keep_deferred_enter_text_hand_back` | wrong for the bare-Enter case: "never submits or writes" |
| `INTERRUPT_GIVE_UP` (`view.rs`) | `interrupt_give_up_fired` clears the wait and logs; "the pty keeps input" | wrong: "hands input back via the hand-off" |
| the 15 other writers (`UNREADY_ENTER_GIVE_UP`, `ENTER_GATE_LEAK_NET`, both `arm_give_up`, `YOUNG_HOLD_GIVE_UP`, `EVENT_HOLD_GIVE_UP`, `AFTER_ENTER_REPLAY_PTY_WAIT`, `settle_enter_queue`, `CLI_SUBMIT_LEAK_NET`, `BOOTSTRAP_PROGRESS_GRACE`, `spawn_bootstrap_timer`, the three `SSH_CONNECT_*`, `schedule_ssh_typeahead_give_up`) and the three bare-Enter ones (`TYPEAHEAD_REPORT_PATIENCE`, `UNCOVERED_REPORT_PATIENCE`, `HANDOFF_GATE_LEAK_NET`) | as their lines say (`on_paste_queue_give_up`, `give_up_young_hold_keys_to_pty`, `give_up_enter_route_wait`, `give_up_unready_enter`, `finish_ssh_typeahead`, `hand_held_ssh_typeahead_to_unbootstrapped_remote`) | right |
| the editor-only nets (verdict / Tab holds of `input.rs`, the setup nets, the PowerShell nets, the ConPTY prompt hold, `hand_back_gate_keys_inner`) | hand back or keep held (`KeysAfterEnter::WithLine`, `give_up_setup_gates_to_editor`, `keep_overdue_prompt_hold`) | right |

The task named four wrong lines; reading every line found six lines plus the header (the three young
ones share one cause). All are corrected.

Fix: text only. The header and the guard's rule now state the amended rule; the six lines and the
`give-up-net:` comments beside the code say what the timer does. No code path changed, and
`input.rs`, `enter_queue.rs`, `give_up_to_pty.rs` are untouched.

Rejected: making the guard check the text (it cannot know what a function does); moving the
writing nets to their own allowlist section with a `writes-to-pty:` tag (a guard change with a new
comment tag on 16 timers in files other agents edit; worth doing, not in a text correction).

Risk: none at run time.

## 2. `bash_init_subshell.sh` reports `wsl_name`

Cause (r11-win.md, "Not covered"): the #2025 rule sources a bash subshell's bootstrap only below a
Git Bash pane, because a bash InitShell said nothing about WSL and a WSL bash must not be told to
source a Windows path. From a PowerShell pane `bash` may be Git Bash or the WSL launcher.

Fix.
- Script: `"wsl_name": "<WSL_DISTRO_NAME>"` in the payload, as the zsh and fish scripts have. The
  value keeps only `A-Za-z0-9._-` (what a distribution name may hold), so no value can break the
  JSON and no second escaping pass is typed: the setup stays two chunk lines (1392 of 1400 bytes
  with a 20-digit session id; the escaped form was 1425, a third typed line on every platform).
- App: `bootstrap::pane_shell_runs_windows_bash` (MSYS2 as before; `Executable` launch data on
  Windows that is not WSL) replaces `session.is_msys2()` in
  `PtyController::spawned_from_msys2_through_plain_bash`; a plain bash on the way that reports WSL
  ends the walk. `msys2_bash_subshell_sources_file` already refused a subshell with a `wsl_name`.

Rejected: zsh's raw interpolation (a quote in the variable would make the payload invalid and the
subshell would never be recognised); a rule on `$OSTYPE` (a second new field for the same fact).

Risk: low on unix (one more JSON field, parsed since the zsh script sends it). Medium on Windows:
a `bash` in a PowerShell pane that is neither Git Bash nor WSL (Cygwin) is now sourced a Windows
path, which Cygwin's bash opens; if a bash cannot, it prints one error and the quiet net abandons the
setup as today. Switch `PHOSPHOR_MSYS2_SUBSHELL_SOURCE=0`.

Test fault found: `script/test_init_shell_escaping` had not run since the ssh wrappers' rcfile
snippet became `_hostname=${HOSTNAME:-$(...)}` (its search raised `StopIteration`; noted as failing
on the base in round 10). Behind that, 48 expectations were stale: the zsh first-stage line and the
zsh one-liner now remove control characters on purpose (macOS MAX_CANON, 62a20a056, bafcee901).
Repaired: the search, the removed one-liner of `zsh_body.sh`, and the expectation for the scripts
that strip. 330 of 330 pass.

## 3. The lab's helper: a cached CI artifact, built once on the box

Need: the lab's `.helper` arms pre-install a real `phosphor-remote` on the lab host. It must be a
static musl binary and must carry the app's version name (`naming::helper_version` =
`GIT_RELEASE_TAG`; the dev builds export `v<app version>-dev`).

Design (the `sshlab-image` pattern: a manual box job that is a no-op when its product exists).
- `script/ci/lab_helper.py build`, played as the manual job `lab-helper`: cargo-zigbuild with the
  release job's profile and features, the dev tag, the cli-linux pkg-config isolation; then checks
  (no ELF interpreter, the version line names the tag, under 20 MB) and stores the binary, its
  `.gz`, its SHA-256, the version line and `meta.json` in
  `$HOME/.cache/phosphor-ci/helper/<key>/` (beside `cache_build.sh`'s builds). The same files are
  the job's artifact (30 days).
- Key: tag, target, profile, features, recipe revision and the git ids of `crates/remote_server`,
  `crates/command`, `lib/notify`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`,
  `.cargo/config.toml`. Computed without cargo. `build` proves the crate list with `cargo tree`
  before each build and fails on a local crate outside it. Entries are immutable; three are kept.
- Lab cases: `cases_lab.resolve_helper_bin` (explicit `APPCI_LAB_HELPER_BIN`, else the cache entry
  of this checkout, else UNVERIFIABLE naming the job). `app-test-lab-linux` prints the path.

Finding that limits it: CI's dev build is a bundled build with no pinned helper digest, which
refuses every helper upload (`setup::decide_helper_digest`, `INSTALL_REFUSED_NO_PINNED_DIGEST`;
`.gitlab-ci.yml` "Build jobs" says so). With a helper present the `upload` and `upgrade` arms could
only fail and `never` could only pass. `app-test-lab-linux` therefore sets
`APPCI_APP_PINNED_DIGESTS=0` and those three arm kinds are UNVERIFIABLE with that reason. The
pre-installed `.helper` arms, the ones the decision names, run.

Rejected:
- Building in the dev build job (the decision).
- `WARP_REMOTE_SERVER_FROM_LOCAL=1` on the app so it accepts an unpinned asset: with the asset
  hidden (`helper_missing_asset`) the app would start a cargo cross build in the CI checkout.
- A key over all of `crates/` and `lib/`: a rebuild on nearly every round.
- Playing `lab-helper` from `ci_seq.py`: it shares `little-thing-cargo` with the builds, so a
  seconds-long no-op would wait behind them (the #1921 starvation); without the resource group a
  real build would run beside another cargo job.

Needs a decision or a box change (not done):
1. Tools on the box. `lab-helper` installs nothing. It needs the `x86_64-unknown-linux-musl` rust
   target, zig and cargo-zigbuild for the runner user, which `cli-linux` leaves there. If one is
   missing the job fails naming it.
2. The cache directory `$HOME/.cache/phosphor-ci/helper/` is created by the job in the runner
   user's home, as `cache_build.sh` creates `builds/`. If that counts as a box change, say so.
3. Upload arms in CI: pin the cached helper's digest into the dev build (export
   `PHOSPHOR_REMOTE_SHA256_LINUX_X86_64` and `PHOSPHOR_REMOTE_HELPER_DIR` from the cache entry before
   `script/bundle`; seconds, no helper build). It changes what a dev build is: it would upload the
   helper to real ssh hosts where it now fails closed. Owner question.
4. Mac and Windows lab jobs: the file must reach those runners (carry `helper-artifacts/` by hand
   and set `APPCI_LAB_HELPER_BIN`, or automate a fetch). `stage_helper_asset` also names the asset
   after the RUNNER's architecture, not the lab host's (an arm64 Mac stages `linux-aarch64` for an
   x86_64 lab): a test fault to fix with that work.
5. Whether `ci_seq.py` should play `lab-helper`.

Risk: the recipe has never run. The first play is its first compile.

## 4. `big_paste` budgets

Decision: 1500 ms from the end of the paste to the Enter's effect, 250 ms for the largest single
UI item. Round 10 already measured and judged both as proposals (`judge_big_paste_time`). Now:
the constants are the owner's, the override variables follow `latency_paint`'s naming
(`APPCI_BIGPASTE_ENTER_BUDGET_MS`, `APPCI_BIGPASTE_UI_ITEM_BUDGET_MS`; the round 10 names still
read), an override that is not a positive number is ignored instead of voiding the budget, and the
run records `big_paste_budgets`. Self-test `big_paste_time_budgets_2004`; two of its assertions
name the variable in the failure message and changed with the rename.

Risk: the case can now fail on time where it passed. That is the decision.
