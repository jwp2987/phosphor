# Testing Phosphor

One document for ALL testing, on Linux, macOS and Windows: what to run before you push, every automated
layer, every real-app harness, the diagnostics, the release checklist and how to file findings.
Sources are cited by path. Where something is not merged yet it says so ("being added, round 18").
`docs/STATE.md` (generated) is authoritative for current counts and guard status; this file is the map.

## 1. Quick start

**Where things run.**

| What | Where | How |
|---|---|---|
| `cargo` anything (check, build, nextest, bench) | the build box (`little-thing`, linux amd64, 12c/32 GB), through the GitLab runner | push the branch as `build/<name>`, read the pipeline with `glab` (section 2.5) |
| macOS build / tests / real-app | Mac test box (M1 Pro, 16 GB, dedicated to this project) | manual CI jobs with tag `Mac`, run by `gitrunner` inside its own logged-in desktop session (3.3); hand-run GUI work over ssh |
| Windows build / real-app | Windows 10 VM (also the GitLab Windows runner) | manual CI jobs with tag `windows`; the runner is on the console desktop, so CI jobs drive the GUI directly; hand-run GUI work through `indesk.ps1` (3.4) |
| non-cargo gates, shell-script harnesses | anywhere with bash/python3 | `script/precheck --fast`, `script/test_*` |

Hard rules:

- **No cargo on the laptop.** Fix agents write code + tests, run `rustfmt --check` on the files they touched
  and commit, reporting "not compiled". The coordinator runs ONE combined build and test pass on the box
  (`docs/FLEET-ROUND.md`, `AGENTS.md` 5.6/5.8). On the box every cargo call goes through
  `script/agent-cargo <agent-name> <cargo args>` (slot and memory governor, per-agent `CARGO_TARGET_DIR`);
  never bare `cargo`, never two cargo invocations at once.
- Every defect gets an issue first (section 6). A red test is fixed, never deferred, never weakened
  (`AGENTS.md` 5.6); a test carried over from Warp that fails after a port means suspect the code first.
- Do not run a Mac CI job while a Mac latency/GUI measurement is running (the runner user `gitrunner`
  compiles in parallel and contaminates the numbers).

**Before pushing:**

1. `script/precheck --fast` (seconds, no compile): toolchain pin, rustfmt on changed files, every guard
   in 2.3, the shell-script tests in 2.4. `--fast` does NOT authorise a push.
2. `script/precheck` (no flags) on the box: also `cargo check`, the integration suite under Xvfb, the pwsh
   scenarios, `warp`+`warp_tui` and `warpui_core` suites via `check_test_failures`, the rest of the
   workspace (40 packages), `docs/STATE.md` freshness, vendored genai. Unknown flags stop rather than
   start a full build. Header of `script/precheck` explains every step.
3. Push `build/<name>`; `guards` and `check-linux` run automatically, the heavy jobs are manual.
4. `script/presubmit` is the older upstream script (fmt, clippy `-D warnings`, clang-format, wgslfmt); run
   it on the box if you touched C/ObjC or `.wgsl`.

## 2. Automated layers

### 2.1 Unit and in-process tests (per crate)

- Convention: `${file}_tests.rs` or `mod_test.rs`, referenced from the source with
  `#[cfg(test)] #[path = "x_tests.rs"] mod tests;` (`AGENTS.md` 5.6).
- Runner is **nextest**, not `cargo test` (the suite was stabilised under process-per-test isolation).
  Config: `.config/nextest.toml`. Default profile: slow-timeout 30 s, terminate after 2 periods;
  `package(integration)` counts twice against parallelism; `package(warp_tui)` retries 2; the 3 MB word layout
  test (#940) and the pwsh real-shell attack-list test (#1050) get reserved CPUs. Profile `ci`: fail-fast
  off, junit at `target/nextest/ci/junit.xml`, integration retries 2, Windows `update_manager` retries 2.
  Always pass `--profile ci` for the integration package, it is what applies the retries.
- Crate layout. App code and views: `app/src/**/*_tests.rs` (package `warp`, needs `--features gui`
  for the GUI paths). TUI: `crates/warp_tui` (`usage_smoke_tests` module, view tests, two benches).
  UI core: `crates/warpui_core` (`--features tui` is mandatory, otherwise ~259 `elements/tui` tests do not
  exist and the run is a false green). The remaining ~40 library crates under `crates/` have their own
  unit tests; the list is the `test-workspace-rest` job in `.gitlab-ci.yml`.
- **Code-editor memory (#1340)**. `crates/editor/src/render/model/parked.rs` (unit tests: a chunk expands into
  exactly the paragraphs it stood for, its summary equals the sum of the per-line summaries bit for bit, split / merge
  round trips, which paragraphs are parkable) and `parked_tests.rs` (randomized sessions, `StdRng` seeds 0..60 x 40 steps
  and 1000..1006 x 250 steps, no extra dependency): a plain `Vec<String>` model, a render state whose dehydrated runs are
  parked into chunks and an "eager" render state with one tree item per line receive the same random edits (replace N lines
  with M across chunk boundaries, deletes, 8-700 line pastes, replacing the whole document as a line-ending change does),
  hydration passes, chunk expansion, eviction and re-parking. After every step the rendered text equals the model, the
  chunked tree's summary (length, height, width, lines) equals the eager tree's exactly, every line's cached metrics equal a
  fresh layout, `y_offset_at_line` / `vertical_offset_at_render_location` / `character_bounds` y / soft-wrap rows equal the
  eager tree's, and each chunk keeps its invariants (1..=256 lines, runs tile their line, offsets monotonic). Targeted tests
  cover an edit inside one chunk, deleting almost every line, expand / re-park round trips, a temporary (diff-hunk) block
  inside a chunk and a split at a non-line boundary. Run them like any `warp_editor` test:
  `script/agent-cargo <name> nextest run -p warp_editor parked`.
- **warp_tui tests need the warp package in the same invocation** (feature unification of `test-util`):

      script/agent-cargo <name> nextest run -p warp -p warp_tui --lib --features warp/gui [filter]

  `-p warp_tui` alone fails to compile (`debug_text_content is not a member of trait Element`).
- Full workspace: `nextest run --no-fail-fast --workspace`; in practice use
  `script/check_test_failures` (below) so the known-failure contract is applied.
- **Known failures** (`script/known_test_failures.txt`): currently EMPTY (header only). Contract of
  `script/check_test_failures`: a failing test not on the list is a regression (exit 1); a listed test that
  passes is a stale entry (exit 1, delete the line in the same change). BOTH `warp` and `warp_tui` must be
  in one invocation, otherwise a listed failure that did not run counts as fixed. Adding a line needs an
  issue and maintainer sign-off.

### 2.2 Integration crates

- `crates/integration`: ~300 GUI-driving scenarios (`src/test/*.rs`: agent_mode, ssh, subshell, typeahead,
  bootstrapping, session_restoration, osc8_hyperlinks, settings_*, workflows, ...). Support code lives in
  `app/src/integration_testing/` (feature `integration_tests`). There is no headless winit backend on
  Linux, so it needs a display:

      xvfb-run -a -s "-screen 0 1280x1024x24" script/agent-cargo <name> nextest run -p integration --profile ci

  Environment: `xvfb`, a software Vulkan driver (`mesa-vulkan-drivers`), `libxkbcommon-x11-0`
  (`script/linux/install_test_deps`); `precheck` reports a missing piece as its own failure. Baseline is
  empty: any failure is a regression. Shard in CI with `--partition count:N/3`.
  Every scenario in `integration_tests!` must be registered in the binary's `register_tests()`
  (guard `check_integration_test_registry`). `PHOSPHOR_TEST_SHELL=pwsh` pins the shell for the two
  PowerShell-only scenarios (`test_input_reporting_powershell`, `test_pwsh_vi_edit_mode_does_not_corrupt_commands`,
  #796); without pwsh they silently never run, `precheck` prints a warning. Debug aids:
  `WARP_INTEGRATION_TEST_VIDEO[_DIR]`, `WARP_INTEGRATION_TEST_ARTIFACTS_DIR`.
- `crates/usage_suite` (`script/usage-test`): NDJSON smoke/acceptance runner over the integration scenarios
  (GUI) and `usage_tui_*` tests in `warp_tui` (TUI). Manifest `crates/usage_suite/src/manifest.rs`; tags
  `reliable-here`, `needs-real-shell`, `needs-byop-provider`. `--surface gui|tui|both`, `--only a,b`,
  `--include-flaky`, `--include-byop`, `--json`. Docs: `specs/usage-test-suite/README.md`.
- Real-shell tests inside `warp` (`real_shells::*`, bootstrap tests in `app/src/terminal/bootstrap_test.rs`)
  start actual bash/zsh/fish/pwsh/nu where installed; they skip a shell that is missing.

### 2.3 Guards (`script/check_*`, run by `guards` in CI and by `precheck`)

| script | fails when |
|---|---|
| `check_cloud_boundary` (+ `cloud_boundary_allowlist.txt`, `_cloud_boundary_scan.py`) | code outside cloud modules imports dropped-cloud modules |
| `check_stub_coverage` | a test targets a function the fork gutted to a no-op stub (needs the oracle pin fetched) |
| `check_declined_collisions` | the tree collides with a `DECLINED.md` decision |
| `check_dead_links` | shipped Rust code links the dead ancestor repo (#632) |
| `check_brand_strings` | a user-visible string says Zap / Oz / Warp instead of Phosphor |
| `check_settings_registry` | a settings group registered in prod but not in the test harness; duplicate `toml_path` / `storage_key` |
| `check_channel_command_names` | packaging scripts drift from `Channel::cli_command_name()` / `warpctrl_command_name()` |
| `check_no_autoupdate` | autoupdate is re-enabled in a shipped build (#630) |
| `check_generator_wrapper_names` | bootstrap scripts drift from the in-band generator wrapper names |
| `check_sweep_ledger` | `docs/sweep-verdict-ledger.tsv` malformed or cites a struck `DECLINED.md` row |
| `check_integration_test_registry` | a name in `integration_tests!` is not in `register_tests()` |
| `check_large_deletions` (+ `large_deletion_allowlist.txt`) | bulk deletion with no `Large-deletion:` trailer or DECLINED citation (CI passes `--require-base`) |
| `check_queue_access` | the pane input queue is reached other than through `PaneQueue` (#1353) |
| `check_pty_doors` | a raw `write_to_pty(` / `clear_line_editor_and_write_to_pty(` / `write_end_of_transmission_char` / `emit(Event::WriteBytesToPty` outside the reasoned allowlist in the script (#1429-#1436). Allowlisted doors, each with its reason in the script: FOCUS_IN / FOCUS_OUT reports (protocol answers), the raw emit and its wrapper (definitions), the sink `write_user_bytes_to_pty_after_paste_stage` (what the pane queue delivers into), `release_held_keys` (keys the hand-off held, released in typed order), the OSC 52 clipboard-read reply, the subshell bootstrap writers (`write_init_subshell_bytes_to_pty`, `write_subshell_bootstrap_line`: the setup that owns the SubshellSetup gate), the PowerShell StopProfiles ^C (`ctrl_c_during_powershell_setup`), the `PtyIntent` dispatcher and the pty controller. Door tests: the `*_door`-style cases at the end of `terminal/view_test.rs` (aws login is a command, synced input honours gates, agent writes refused during setup, editor Ctrl-D through the queue, CLI-agent text held by a gate, ssh warpify payload paced, programmatic insert waits for a waiting Enter, local-child harness command waits as a pending command). `check_queue_access` also runs in the CI `guards` job (#1448). Enter/paste coverage from the Mac run (#1452, #1453): `an_inline_editor_paste_then_an_immediate_enter_are_queued_in_order` (`terminal/view_test.rs`: the editor's inline-read paste is a pane-queue entry via `offer_answered_paste`, with the Enter behind it), `test_enter_behind_an_unhandled_edit_does_not_accept_the_stale_as_you_type_menu` and `test_enter_with_an_as_you_type_menu_and_no_selection_submits` (`terminal/input_test.rs`: Enter never accepts an as-you-type menu that answers older text or has nothing selected; it submits). #1459: in terminal mode the slash menu never swallows Enter unless it lists current rows for the typed text (`test_terminal_mode_slash_path_enter_runs_the_line_1459`; control `test_agent_mode_slash_menu_still_takes_enter_1459`, `terminal/input_test.rs`). appci keeps one `phosphor.<n>.log` per app instance in `logs/<mode>/` (#1457), and `ctrl_c_d` sends a Ctrl-D only after proving the cat / nested bash it ends is running (#1458). Pending-command coverage (`terminal/input_test.rs`): text typed into a pending command is handed back with `TypeaheadHandoff` off too (#1451); `is_pending_aws_login` is set when the login starts, not when requested (#1450, `aws_login_is_a_command_that_waits_for_the_running_program`); the prompt look never takes the blocking model lock on event/timer paths (#1447, `prompt_look_outside_key_path_does_not_block_on_a_busy_model`); the ssh held-back Enter guard only holds the handed-back line (#1396, `ssh_handover_enter_guard_*`). Agent writes during an ssh connect's remote-setup window / subshell / PowerShell setup are REFUSED (logged), not held: the answer belongs to the prompt it was computed for |
| `check_outbound_redaction` | a model-provider wire call (`exec_chat`, `exec_chat_stream`, embeddings/Exa `.post(`) appears outside the files that route through `agent_providers/outbound_redaction.rs`, or one of them stops calling it (#1469). Tests: `agent_providers/outbound_redaction_tests.rs` (every content kind, on and off), `chat_stream` request-builder tests, `oneshot` and embeddings tests |
| `check_dangling_modules` | `mod foo;` with no file (untracked file lost in a patch) |
| `check_pwsh_parse` | a bundled `.ps1` does not parse in real PowerShell (#1161); skipped without pwsh, which `precheck` shows as a warning |
| `check_license_config_sync` | `deny.toml` and `about.toml` license lists differ (job `licenses`) |
| `check_test_failures` | see 2.1 |
| `check_workspace_clean` | NOT wired anywhere (recorded gap): refuses to build a checkout holding another author's uncommitted work |

### 2.4 Shell-script harnesses (`script/test_*`, no cargo; run by `precheck`)

They run the REAL bundled bootstrap text (cut out of `app/assets/bundled/bootstrap/*`) in real shells and
pin protocol behaviour. Skips are reported as SKIPPED, never silent.

| script | checks |
|---|---|
| `test_bash_bootstrap_nounset`, `test_zsh_bootstrap_strict_options` | hooks survive `set -u` / `err_return` / failing user precmd (#825, #836, #844) |
| `test_fish_bootstrap` | fish hooks fire end to end in a pty (#829, #879) |
| `test_bootstrap_hook_equivalence` (bash/zsh/fish), `test_nu_bootstrap_hook_equivalence`, `test_pwsh_hook_equivalence` (+ `testdata/pwsh_hooks_golden.json`) | fork-free hook helpers emit byte-identical DCS payloads to the old forking code (#1170, #1233, #1231) |
| `test_bash_bootstrap_builtins` | fork-free `sort -V`/os-release/`shopt` replacements match the commands (#1164) |
| `test_bash_preexec_large_command` | Preexec escaping of a 3 MB command is linear (#1255) |
| `test_init_shell_escaping` | InitShell JSON round-trips hostile user/host values (#879) |
| `test_hijack_bootstrap` | user `echo/printf/cd/read/pwd` functions never run inside the bootstrap (#929) |
| `test_line_editor_markers` | `LineEditorReading` / `ContinuationPrompt` OSC markers and `seq` per Precmd (#1390-#1392) |
| `test_local_counted_bootstrap` | counted-read bootstrap behaves identically to the here-doc form (#1226) |
| `test_ssh_keepalive`, `test_ssh_probe_cache`, `test_ssh_wrapper_forks`, `test_ssh_raw_bootstrap_hook` | ssh wrapper options, probe cache, fork budget, `raw_bootstrap` hook (#1171, #1136, #1172, #1243) |
| `test_pwsh_git_head.ps1` | pwsh in-process git head equals `git` (#1163) |
| `test_warpctrl_early_dispatch` | the app binary honours `--warpctrl` before GUI startup (needs a built binary, else SKIPPED) |
| `macos/test_create_warpctrl_wrapper` | macOS wrapper packaging |
| `script/repro_subshell_relay_deadlock.py` | repro for a subshell relay deadlock (manual) |

### 2.5 CI jobs (`.gitlab-ci.yml`)

Pipelines run only on protected refs: `main`, `build/*` branches, `v*` tags. Every job needs a runner tag:
`little-thing` (linux), `Mac` (darwin arm64, user `gitrunner`, `CARGO_BUILD_JOBS=4`), `windows` (Win10 VM,
PowerShell 5.1). Project `winters-phosphor/phosphor` (id 88085499), use `glab`; run `glab api user` before
`git push` if auth fails. Jobs never install anything on the boxes; a missing prerequisite fails with a message.

Play-time job variables (`TEST_FILTER`, `APPCI_TIER`, `APPCI_CASES`, ...): the project setting "Minimum role to use
pipeline variables" is **Maintainer** (changed 2026-11-02; it was "No one allowed", which made every play with variables
fail with 403). There are no project-level CI variables: personal values never go into GitLab.

Machine-local test settings (owner-provided, NOT in the repo or GitLab), per runner user, mode 600, in
`~/.config/phosphor-ci/` (Windows: `%USERPROFILE%\.config\phosphor-ci\`): `ci.env` (`APPCI_OLLAMA_URL`,
`APPCI_OLLAMA_MODEL`; KEY=VALUE) and `owner-settings.toml` (an exact copy of the owner's Phosphor `settings.toml`: layout,
vertical tabs, theme, fonts, footer, agent and ssh settings). Present for `gitlab-runner` on the box, `gitrunner` on the
Mac and the console user on the Windows VM. The harness starts its profile from them (2.8); their contents are never
printed or uploaded. Re-seed all three when the owner changes their config.

Build-box GUI stack for `app-test-linux-*` (installed by the owner 2026-11-01): `sway`, `wtype`, `grim`, `wl-clipboard`,
`xwayland`, `xterm` (reference terminal); `gitlab-runner` is in the `video` and `render` groups for `/dev/dri`.

Box target-dir pruning: the persistent cargo target dirs under `/home/gitlab-runner/.phosphor-build/targets/<label>`
are shared by every `build/*` branch and used to grow unbounded (2026-11: `ci-test` 219 GB, `ci-check-linux`
119 GB, disk full, jobs failed). `script/ci/prune_target.sh` runs at the start of `.box_job` (all box jobs) and
`check-linux`; it never fails a job and logs `[prune_target] <label>: before -> after`. Per label it (1) always
deletes `incremental/` of release/LTO profiles, drops nextest/junit output older than a day; (2) if the filesystem
has < 80 GB free or the dir exceeds its cap (`ci-test` 120 GB, `ci-check-linux` 60 GB, others 60 GB) deletes all
`incremental/`, then files unmodified and unaccessed for 7 days, and if still over, wipes the dir. Plain
`find`/`du`/`df`, no cargo-sweep; the cargo registry is untouched. Tunables: `PRUNE_MIN_FREE_GB`,
`PRUNE_STALE_DAYS`, `PRUNE_CAP_GB_<LABEL>`, `PRUNE_DRY_RUN=1`.

| job | stage | trigger | what |
|---|---|---|---|
| `guards` | test | automatic | oracle pin fetch + all guards of 2.3 + `macos/test_create_warpctrl_wrapper` + `test_warpctrl_early_dispatch` |
| `check-linux` | test | automatic (~10 min warm) | `cargo check -p warp --features gui --lib --tests`, feature-unified multi-package check, `--features tui`, `-p warp_tui` |
| `test-linux` | test | manual (auto on tags) | `check_test_failures -p warp -p warp_tui --lib --features warp/gui`, plus `-E $TEST_FILTER` when that variable is set (fix tier; a filtered run skips the stale-known-failure rule) |
| `test-warpui-core` | test | manual (auto on tags) | `check_test_failures -p warpui_core --features tui` (honours `$TEST_FILTER`) |
| `test-workspace-rest` | test | manual (auto on tags) | the 40 other packages in one invocation (honours `$TEST_FILTER`) |
| `integration-linux` | test | manual, allow_failure | `crates/integration` under xvfb, 3 shards, `--profile ci` |
| `integration-linux-pwsh` | test | manual | the two pwsh scenarios with `PHOSPHOR_TEST_SHELL=pwsh` |
| `usage-test-linux` / `usage-test-macos` | test | manual | `script/usage-test --surface both` (Linux, xvfb) / `--surface tui` (Mac has no WindowServer for the daemon) |
| `licenses`, `advisories` | test | on box | `cargo deny check licenses sources`, `check advisories` |
| `check-macos`, `check-windows` | test | manual | cargo check of OS-only paths (`script/windows/bundle.ps1 -CHECK_ONLY`) |
| `diag-mac`, `diag-windows` | test | manual | read-only environment reports |
| `build-linux-x86_64`, `build-macos-arm64`, `build-windows-x86_64` | build | manual | AppImage+deb (launches `--version`), ad-hoc signed DMG, unsigned Inno installer; `dist/` artifacts, 3 days |
| `build-{linux,macos,windows}-profiling` | build | manual | unstripped `profiling` profile binary (`docs/profiling.md`) |
| `app-test-linux-wayland`, `app-test-linux-x11`, `app-test-macos`, `app-test-windows` | apptest | manual, allow_failure, played by `ci_seq.py` after each build | real-app matrix on the built artifact, section 2.8 |
| `app-test-{linux-wayland,linux-x11,macos,windows}-reuse` | apptest | manual, allow_failure, played by `ci_seq.py` (fix tier) only | the same job on an EARLIER pipeline's build of an app-identical commit (no `needs`; uses the build cached on the same machine, GitLab download only as a fallback), section 2.9 |
| `release-*`, `cli-*`, `release-publish` | build/publish | `v*` tag (manual dry run on main) | release pipeline, never run end to end on all platforms |

Kick off a build: `git push origin <branch>:build/<name>`, then `glab ci status -b build/<name>` or
`glab api "projects/88085499/pipelines?ref=build/<name>&per_page=1"`; play a manual job with
`glab api -X POST projects/88085499/jobs/<id>/play`. `script/ci/ci_seq.py build/<name> --tier fix|candidate|release`
plays the jobs of a test tier (section 2.9), then the `app-test-*` jobs of every build that succeeded. Heavy suites are
long: ask first. Cancelling a running build can orphan cargo processes under `gitlab-runner`; a cancelled app-test job
can leave the test app, its sshd and `caffeinate` running (seen on the Mac 2026-11-02): check `ps` and stop them by exact PID.

### 2.6 Environment switches (kill switches and test knobs)

All are read once at startup unless noted. "=0" switches turn a new behaviour OFF to A/B a failure; "=1"
switches restore the old behaviour. Use one at a time to bisect (the README of any failing real-app trial
should say which switch flipped it). The `PHOSPHOR_INPUT_QUEUE_{PASTE,SUBSHELL,ENTER,SSH,HANDOFF,SHARED}`
switches no longer exist: the queue is one per pane and the pane key sink replaced the per-element
claims (#1353, #1404); only a doc comment in `app/src/terminal/input_queue/mod.rs` and a reference in
`view.rs` still name them.

| variable | effect |
|---|---|
| `PHOSPHOR_PANE_KEY` | `0`/`off`: pane key sink off (elements take keys as before); `shadow`: sink decides and logs `kh_pane_key_shadow`, elements deliver (#1404) |
| `PHOSPHOR_PROMPT_SEQ=0` | hand-off decides young Enter / Ctrl-C hand-back / stale line counts by the old timing rules (#1353 phase 6) |
| `PHOSPHOR_PTY_WRITE_QUEUE=0` | back to timer-chained pty writes instead of the writer-owned job queue (#1384) |
| `PHOSPHOR_PTY_COALESCE=0` | no key-write coalescing in the pty event loop |
| `PHOSPHOR_HANDOFF_FACTS=0` | user-key path takes the model lock again instead of published facts (#1403) |
| `PHOSPHOR_RENDER_LOCK_LEGACY=1` | previous model-lock scoping in render/layout/wakeup (round 18 R1) |
| `PHOSPHOR_BLOCK_LIST_PAINT_UNDER_LOCK=1`, `PHOSPHOR_ALT_SCREEN_PAINT_UNDER_LOCK=1` | paint from the locked model instead of snapshots |
| `PHOSPHOR_ALT_SCREEN_FULL_RENDER=1`, `PHOSPHOR_BLOCK_LIST_FULL_RENDER=1`, `PHOSPHOR_BLINK_FULL_RENDER=1` | full re-render on every wakeup / blink instead of paint-only frames |
| `PHOSPHOR_BOOTSTRAP_TIMED=1` | timer-based subshell setup instead of the event-driven one (#1354) |
| `PHOSPHOR_SSH_COUNTED_BOOTSTRAP=0`, `PHOSPHOR_LOCAL_COUNTED_BOOTSTRAP=0` | here-document bootstrap instead of the counted raw read (#1226) |
| `PHOSPHOR_SSH_SPECULATIVE_CONNECT=0`, `PHOSPHOR_SSH_EARLY_BOOTSTRAP=0` | old probe-then-connect / bootstrap-after-connect order (#1266) |
| `PHOSPHOR_SSH_CHUNKED_RESPONSES=0` (client), `PHOSPHOR_REMOTE_SERVER_CHUNKED_RESPONSES` (daemon) | whole-frame responses as from an older daemon |
| `PHOSPHOR_SSH_LEGACY_MAX_INFLIGHT`, `PHOSPHOR_SSH_LEGACY_COMMAND_TIMEOUT_SECS`, `PHOSPHOR_SSH_HISTORY_MAX_LINES` | tuning of the legacy (no remote extension) ssh executor and history fetch |
| `PHOSPHOR_ROUTING_V1=1` | pre-#1234 agent/terminal routing rule set, to rule the redesign in or out |
| `PHOSPHOR_SYNC_REFILTER=1` | always filter completion menus inline |
| `PHOSPHOR_NO_ALIAS_LOOKUP_CACHE=1`, `PHOSPHOR_NO_BRANCH_LIST_CACHE=1` | disable the alias-lookup / git-branch-list caches |
| `PHOSPHOR_GIT_REFRESH_ALWAYS=1` | git status refresh after every block |
| `PHOSPHOR_COPY_INLINE=1` | selection copy on the UI thread instead of off-thread (#1304) |
| `PHOSPHOR_SYNC_CLIPBOARD_WRITE=1` | synchronous clipboard writes instead of the serial owner thread (#1307) |
| `PHOSPHOR_CLIPBOARD_TARGETS_QUERY=0` | no X11 targets-based availability query (#1306) |
| `PHOSPHOR_PASTE_BACKSTOP_MS=<n>` | how long held keys wait for a clipboard read (default 3000; `0` = wait forever, the old behaviour) (#1331) |
| `PHOSPHOR_ATTACHMENT_READ_GIVE_UP_MS` | attachment-read timeout for a submit (#1370) |
| `PHOSPHOR_CONPTY_PROMPT_HOLD=0` | Windows: no ConPTY prompt hold (#1162) |
| `PHOSPHOR_EDITOR_EAGER_LAYOUT=1`, `PHOSPHOR_EDITOR_FULL_LAYOUT=1`, `PHOSPHOR_EDITOR_LAZY_STRICT=1`, `PHOSPHOR_EDITOR_MAX_FILE_MB` | editor: disable lazy layout / windowed layout; panic on geometry of a dehydrated paragraph (always on in tests); file-size guard (`0`/`off` disables) |
| `PHOSPHOR_EDITOR_P5=0` | editor: no parked line chunks (#1340 P5); every dehydrated line is an individual tree item again, the P4 representation |
| `PHOSPHOR_CODE_PANE_KEEP_ON_CLOSE=1`, `PHOSPHOR_FILE_PANE_KEEP_ON_CLOSE=1` | keep closed code/file pane editors alive through the undo-close grace (pre-#1340) |
| `PHOSPHOR_LAZY_RESTORE=0`, `PHOSPHOR_LAZY_REFLOW=0` | eager restore / reflow of blocks (#1303) |
| `PHOSPHOR_ALT_FILE_LINKS=0` | alt-screen TUIs treated as editors (no file hover scan) |
| `PHOSPHOR_DB_PREWARM_READ=0`, `PHOSPHOR_SECURE_STORAGE_GIVE_UP_SECS` | DB read-ahead off; secure-storage wait bound |
| `PHOSPHOR_WATCHER_LEGACY_DEBOUNCER=1` | polling `notify_debouncer_full` instead of the event-driven loop |
| `PHOSPHOR_TIMER_RESOLUTION=0` | Windows default timer tick instead of 1 ms (#1260) |
| `PHOSPHOR_GLYPH_RASTER_BUDGET_MS=<n>` (`0` = off), `PHOSPHOR_WGPU_PER_FRAME_BUFFERS`, `PHOSPHOR_WGPU_CLEAR_TO_BACKGROUND`, `PHOSPHOR_RECT_FAST_PATH=0`, `PHOSPHOR_PRESENT_MODE`, `PHOSPHOR_FRAME_LATENCY` | renderer A/B and latency experiments (#1279) |
| `PHOSPHOR_GPU_TIMING=1`, `PHOSPHOR_IMG_TIMING`, `WARP_STARTUP_TRACE` | extra timing diagnostics (section 4) |
| `PHOSPHOR_BYOP_STREAM_IDLE_TIMEOUT_SECS`, `PHOSPHOR_BYOP_STREAM_FIRST_EVENT_TIMEOUT_SECS` | BYOP stream timeouts (#853) |
| `PHOSPHOR_TEST_SHELL`, `PHOSPHOR_TEST_MIGRATION_RACE_DB`, `PHOSPHOR_PARSE_FILE` | test-only: pin the shell for integration scenarios; DB migration race test; bootstrap parse test |
| `WARP_INPUT_ORDER_LOG_TEXT=1` | log typed text in `warp::input_order` (section 4); synthetic text only |
| `WARP_SSH_KEEPALIVE=0`, `WARP_SSH_OBSCURE_KEYSTROKE_TIMING=0`, `WARP_USE_SSH_WRAPPER`, `WARP_HONOR_PS1` | ssh wrapper / prompt behaviour read by the bootstraps |

**Input-path hygiene tests (#1440-#1444).** Named so a failure points at the guarantee:

| guarantee | test |
|---|---|
| no blocking model lock on the per-key path: scroll position, input-box probe, ssh prompt check, give-up scheduling and cursor-key sequences read the key snapshot / `try_lock` and defer, in order, what needs the model (`view.rs` `deferred_scroll_updates`, flushed on the output wakeup) | `key_path_helpers_do_not_wait_for_the_parser`, `input_box_visibility_from_the_snapshot_matches_the_model` (`view_test.rs`; the model lock is held by another thread for up to 3 s) |
| a pane closed with queued/held input or a paste in flight logs counts (never content) | `a_pane_closed_with_held_input_has_a_drop_summary` (`view/shared_queue_tests.rs`) |
| every input buffer has a cap, a logged overflow and a toast: pane `InputQueue` (`MAX_QUEUED_ENTRIES` 64 Ki entries, newest dropped, ^C never), `ModelInbox` (`MAX_INBOX_COMMANDS` 256 Ki, data dropped, `UserWrote` flags merged), pty `write_list` (`MAX_QUEUED_WRITES` 64 Ki, signals never dropped), tmux control-mode pending buffer (`MAX_TMUX_PENDING_BYTES` 64 KiB, whole writes). Each cap is far above 180 s of 60 Hz key repeat (asserted) | `a_full_queue_*`, `the_overflow_episode_resets_*` (`input_queue/tests.rs`), `a_full_inbox_*` (`model/key_facts_tests.rs`), `a_full_write_list_*` (`local_tty/event_loop_tests.rs`), `tmux_pending_input_is_bounded_*` (`writeable_pty/pty_controller_lifecycle_tests.rs`) |
| tracked modifiers are cleared on window focus loss and regain (winit backends; macOS derives modifiers from each native event and tracks none) | `focus_modifiers_tests.rs` (`crates/warpui/src/windowing/winit/event_loop/`) |
| control-sequence bytes are not logged unless `WARP_INPUT_ORDER_LOG_TEXT=1` | `raw_bytes_are_described_by_count_not_content` (`crates/warpui_core/src/input_order.rs`) |

### 2.7 Dev test tooling (merged in round 18)

All dev-only (nothing ships). Exact commands, flags and caveats: **`docs/dev-tooling.md`**. Summary:

| tool | where | what it covers | when it runs |
|---|---|---|---|
| proptest | `app/src/terminal/input_queue/tests/prop_model.rs` (`ops.rs` shared with fuzz) | random op sequences against a model of the pane input queue | every test run (in `test-linux`) |
| unit (hand-off pipeline, #1420/#1422/#1423/#1439) | `input_queue/tests.rs` (`retag_gate_with_a_multi_byte_...`, `editor_keys_pass_only_setup_gates_...`), `typeahead_handoff_tests.rs` (`a_key_reaching_the_hand_off_under_a_live_*_hold_is_held_not_written`, `the_editor_takeover_window_*`), `editor/view/mod_test.rs` (`test_queue_editor_keys_behind_a_setup_gate_*`) | a `\r\n`-led or multi-byte Enter write keeps its tail when retagged; a key under a live queued hold is held (never written unheld); editor keys behind only a subshell/PowerShell setup gate apply at once (Enter/Tab still wait; CliSubmit and hand-off gates hold); the takeover window ends on focus/click, with a 10 s logged give-up | every test run |
| unit (busy queue = deferred operation, #1416-#1419/#1426-#1428) | `terminal/view_test.rs` (`editor_keys_taken_out_of_a_prompt_hold_*`, `a_prompt_hold_cap_that_fires_while_the_queue_is_busy_*`, `a_held_key_that_finds_the_queue_busy_*`, `the_cli_submit_gate_survives_a_busy_queue`, `a_pane_drain_wake_that_finds_the_queue_busy_is_kept`), `input_queue/pane.rs` (`deferred_operations_keep_their_order_*`) | an operation that finds the pane queue busy is recorded (`PaneQueue::defer`) and applied in order when free: no dropped / as-typed key, no consumed one-shot cap, editor keys taken from a gate are applied. The proptest model has no busy op: the queue core has no busy concept (the mailbox lives in `PaneQueue`/the view), so the model cannot express it | every test run |
| unit (busy queue, operations that RETURN bytes, #1446) | `terminal/view_test.rs` (`a_powershell_take_under_a_busy_queue_*`, `a_deferred_enter_gate_take_under_a_busy_queue_*`, `an_ssh_gate_drop_under_a_busy_queue_is_not_lost`) | the take (deferred Enter gate keys, PowerShell held bytes, ssh gate drop, void Enter text) and the caller's continuation are one `Deferred` variant: nothing returns empty when busy, the ssh gate is not left set | every test run |
| insta | `.snap` files under `app/src/terminal/snapshots/`, `crates/warpui_core/src/snapshots/` | per-shell bootstrap lines, pane-key routing tables, input_order trace shapes | every test run; review diffs with `cargo insta review` on the box |
| expectrl | `app/tests/real_shell_markers.rs` | real bash through a pty: marker order (Preexec, Precmd, LineEditorReading with the same seq) | every test run of `-p warp` (skips without bash) |
| shuttle / loom | `warpui_core::sync_shim`; `model_tests::*` in `clipboard_model_tests.rs`, `key_facts_model_tests.rs`, `typeahead_handoff_model_tests.rs` | thread interleavings of the clipboard write queue, KeyFacts, PaneHandoff, the reader cap | `test-shuttle-loom` job (release tier) |
| criterion | `app/benches/key_path_bench.rs`, `ansi_bench.rs` (`--features dev-support`) | per-key cost (idle, behind a gate), routing, hand-off, 1 MB ANSI parse | `bench` job (release tier) |
| cargo-fuzz | `fuzz/` (own workspace; targets `ansi_parser`, `hook_parsers`, `input_queue`) | crashes on hostile terminal output, hook frames, queue op sequences | `fuzz-smoke` job (release tier) |
| cargo-mutants | `.cargo/mutants.toml`; jobs `mutants` (pane_key, key_facts, input_queue gates/destination) and `mutants-warpui-core` (clipboard) | lines the tests do not actually check | release tier only; widen with `MUTANTS_FILES` / shard with `MUTANTS_SHARD` |

### 2.8 Real-app tests in CI (`app-test-*`, stage `apptest`)

The release-test matrix of 3.5 is automated against the BUILT artifact, per platform, in BOTH session modes (terminal and
agent). Code: `script/realapp/appci/` (Python: `run.py` driver, `cases.py`, `backend.py`, `profile.py`, `app.py`, `sshd.py`,
`report.py`), entry points `script/realapp/linux/wayland_session.sh`, `mac/ci/app_test.sh`, `windows/ci/app_test.ps1`.
Each also has a `-reuse` twin that tests an earlier app-identical build (2.9). Heavy and GUI-driving: all four jobs are **manual, allow_failure**, need the platform's build job (`needs:` + artifacts) and
`script/ci/ci_seq.py` plays them after each successful build, so every numbered build runs them. The play-time variable
`APPCI_TIER` selects the size: `smoke` (fix tier) or `full`/empty (candidate, release; everything below). Smoke = the cases
`mode_proof`, `cadence`, `fast_typing`, `paste_instant_enter`, `tab_enter`, `ctrl_c_d`, `ssh_connect_typing`, `latency_enter`, `latency_key_log`, `md_viewer` in both
modes (`SMOKE` in `script/realapp/appci/cases.py`, applied by `run.py`); on Linux only the `wayland` run (the `xwayland` pass and
`app-test-linux-x11` exit at once); on Windows `fast_typing`, `paste_instant_enter`, `ctrl_c`, `latency_enter` (its names differ; `tab_enter`
and `ssh_connect_typing` have no Windows equivalent). Target <= 10 min per platform; UNMEASURED so far (the full matrix has not been timed
either).

| job | runner | session | input / screenshots |
|---|---|---|---|
| `app-test-linux-wayland` | box (`little-thing`, resource_group `little-thing-gui`) | headless sway, `WLR_BACKENDS=headless`, gles2 on `/dev/dri/renderD128` (pixman fallback), 1600x1000; full matrix as a native Wayland client, then a reduced set under XWayland (`--mode xwayland`, app forced to X11) | `wtype` (`-d N` = 10-25 ms/key, text + Enter in one call), `grim`, `wl-copy`/`wl-paste` |
| `app-test-linux-x11` | box | private Xvfb (software GL) | `xdotool`, `xsel`, `import`/`ffmpeg` |
| `app-test-macos` | `Mac` (user `gitrunner`) | must be the logged-in Aqua session | `cliclick` + `osascript`, `screencapture`, `pbcopy`; frontmost-app guard before every burst; `caffeinate` |
| `app-test-windows` | `windows` (console desktop, the runner is NOT a session-0 service) | the interactive desktop | `SendKeys` (foreground-guarded), `CopyFromScreen`; side-by-side install; backup + md5-verified restore of app data, uninstall key, PATH, shortcuts, `phosphor://` handler; `WINPUSH.lock`; exact-PID stop |

Session handling (Linux): own short `XDG_RUNTIME_DIR` (falls back to `/tmp/pa-*` when the job dir is too long for the 108-byte socket
limit), private `HOME` per mode with an owner-like scratch config and **no credentials**, app under its own `dbus-run-session`, a local ssh
server on 127.0.0.1 (system `sshd` as the CI user with a `ForceCommand` into a scratch remote home; else `fakesshd.py` when paramiko
exists; else the ssh cases SKIP). Every process is recorded in `pids` and torn down by exact PID / process group, never by name.
Scratch config: `general.default_session_mode` = `terminal` or `agent`; agent mode adds autodetect + natural-language detection; a
provider is configured only when the endpoint is known (then `approval_card_pause` runs; otherwise it skips).

**Machine-local CI inputs (owner-provided, per machine, never in the repo or in GitLab variables).** Directory `~/.config/phosphor-ci/`
(Windows `%USERPROFILE%\.config\phosphor-ci\`), files mode 600, seeded by the owner on the box (`gitlab-runner`), the Mac (`gitrunner`)
and the Windows VM:
- `ci.env`: `KEY=VALUE` lines, `#` comments. Keys `APPCI_OLLAMA_URL` (the owner's Ollama endpoint) and `APPCI_OLLAMA_MODEL` (default
  `gpt-oss:20b`). The environment wins over the file. `appci/ci_env.py` (and `app_test.ps1`) read it; the job log says only `ollama:
  configured` or `not configured - agent provider cases skip`, the app log copied into artifacts has the endpoint redacted, and the value
  is never written to results.json/summary.md.
- `owner-settings.toml`: an exact copy of the owner's `settings.toml` (layout, vertical tabs, theme, fonts, footer bar, agent/autodetect,
  ssh). `profile.py` copies it into the job's private profile (the seed is never modified) and edits ONLY these keys textually
  (`appci/toml_text.py`; the file's multi-line inline tables defeat `tomllib`): `general.default_session_mode` (per pass),
  `general.restore_session=false`, `warpify.ssh.ssh_extension_install_mode="never_install"`, removal of `session.startup_shell_override` /
  `new_session_shell_override`, autodetect + NLD on in the agent pass, and, when Ollama is configured, `agents.warp_agent.providers` (the CI
  Ollama provider replaces the owner's list, which names a laptop-only proxy) and `agents.byop.last_used_model_id =
  "byop:appci-ollama:<model>"` so the Ollama model is the active one. Without the file the minimal bare profile is used and
  results.json/summary.md say `owner_settings=not found (bare profile)`; with it they say `owner_settings=sha256 <first 12>` (contents
  are never printed or uploaded).

**macOS specifics (`app-test-macos`).** Typing is `htype2` (CGEvent posting, exact cadence), key to paint is `kl3` (ScreenCaptureKit),
OCR is `ocr` (Apple Vision); all three are compiled from `script/realapp/mac/tools/*.swift` by `swiftc` at job start and cached by
source hash in `~/.cache/phosphor-appci-tools` (no binaries in git). cliclick is no longer used. The app is launched with a PRIVATE HOME
(`open --env HOME=<job>/run/home`): on macOS the settings file is `$HOME/.phosphor/settings.toml` (`warp_core::paths::config_local_dir`,
OSS channel) and the log is `$HOME/Library/Logs/phosphor.log`, so the runner user's real profile is never touched and nothing needs
restoring. Before every injection the guard checks the frontmost app name AND that its PID is the Phosphor PID the harness started; it
refuses Cmd-Q/W/Tab/Space/H/M, Ctrl+Cmd chords and Globe. The job cleans up by exact, command-line-verified PID (Phosphor, sshd,
caffeinate, python runner) in a trap that also fires on SIGTERM from a cancelled job. Mode proof: `shots/<mode>_proof_prekey.png` before
any key and `<mode>_proof.png` after Escape, OCR-checked: agent = header `New Phosphor Agent conversation` or placeholder `Ask anything`
(and, with Ollama configured, the `gpt-oss` model chip); the terminal hint `new /agent conversation` is NOT an agent marker.

Machine-checkable output (`appci-out/<mode>/run/`): `results.json` (summary + one object per case: name, mode, status
pass/fail/skip, detail, metrics, shots), `junit.xml` (GitLab test report), `summary.md`, `shots/*.png`, `logs/<mode>/` (app log with
`warp::input_order` at debug and synthetic text only, `rec.log` = the exact command line the shell received per prompt, sshd log,
sway/GPU info). A case passes only if the effect file the typed command writes AND the shell's own `rec.log` match the typed commands
exactly; an empty `rec.log` is noted and the effect file alone decides.

Cases (each in both modes unless noted): `mode_proof` (screenshot + OCR when tesseract exists), `cadence` (real key spacing measured at
the pty), `fast_typing` (10-25 ms/key, Enter in the same call, gaps 0 and 250 ms), `rollover` (next key down before the previous is
up), `expert_fuzz` (seeded typos fixed with BackSpace / Left+insert / Ctrl-W), `paste_instant_enter`, `arrows_behind_paste` (#1389),
`big_paste` (`APPCI_BIG_MB`, default 1, byte-exact), `tab_enter` (gaps 0/50/150 ms, no stray uncompleted file), `ctrl_c_d` (Ctrl-C
delivery latency, Ctrl-D in `cat` and in a nested bash), `read_x`, `subshells` (bash twice incl. typing during setup, zsh, fish, nu where
installed), `alt_screen` (less, vim), `ssh_connect_typing` (0/100/200/300 ms after Enter, `APPCI_SSH_TRIALS` per gap, exact remote
command, nothing local), `ssh_enter_then_immediate_command` (#1415: `ssh fake` Enter and the next command with no pause), `ssh_nested_typeahead` (#1421: a nested ssh and the line behind it, typed during the first connect; runs once, remotely), `ssh_failing_not_local`, `idle_cpu`, `latency_enter` (vs a reference terminal in the same session),
`latency_key_log` (key to first frame from the app's own `input_order` log), `approval_card_pause` (agent, needs a provider),
`tabs_two_windows` (`APPCI_TABS`, default 12 streaming tabs, a second window, typing and paste). Full tier, module `cases_caseagent.py` (#1463; Linux/macOS backends; the AI ones skip without `ci.env`, the OCR sub-checks are notes without tesseract): `agent_stream_cancel` (agent; a long reply streams, text typed at 10-25 ms/key lands in the input with no new request and nothing in rec.log, Ctrl-C stops the stream: the screen stops changing; request/stream-open times are metrics), `agent_approval` (agent; nothing runs before approval, the first Enter on the empty input only focuses the card, the second approves: the command is in rec.log and its effect file exactly once; a follow-up question in the same conversation opens a new model request; a rejected command never reaches rec.log), `routing_table` (both modes; 10 installed-command lines must reach the shell with the exact line in rec.log, 6 prose lines go to the agent (terminal mode per `nld_in_terminal_enabled`), 3 installed-head English lines follow the documented rule by the surface the `phosphor::routing` enter-stage log reports; misroutes both ways fail, the table is in results.json). Windows runs the PowerShell subset
`tabs_two_windows` (`APPCI_TABS`, default 12 streaming tabs, a second window, typing and paste).
Remote and per-shell cases (`cases_caseremote.py`, issue #1464; all FULL tier, none in SMOKE; order 60-70):
`remote_session_legacy` (ssh with `never_install`, the legacy warpify path: 12 lines typed at 10-25 ms inside the remote shell arrive exactly and
ONLY there (remote `rec.log` exact, local file and local `rec.log` empty); nested remote subshells `bash`, `bash` typed with no wait, `sh` and
`env -i ... bash --noprofile --norc -i` as the stand-in for sudo/su/ksu (none works non-interactively on CI), the nesting proven by pid (inner `$PPID` ==
outer `$$`, outer pid again after `exit`); vim over ssh (`ihello ...`, Esc, `:wq`, file content exact); `less` (space, G, q, no key leaks into the
shell); Ctrl-C of a remote `sleep 60` (a `trap` writes `INT`, then `AFTER`); `exit` and the next LOCAL command runs locally and never remotely; a second
connect and a remote command runs remotely and never locally), `remote_session_ext` (the same steps with `always_install`: the app is relaunched with the
setting and relaunched back with `never_install` afterwards; the CI host has no route to the extension download, so the app may fall back, and the
case asserts exact input and records `connect_s` and the app-log lines naming the remote server / extension), `ssh_hanging_host_not_local` (a refused
port and a TCP listener that never speaks, `ConnectTimeout=4`: the line typed behind the ssh Enter never runs locally nor reaches the local `rec.log`;
the local shell is usable afterwards), `ssh_password_prompt` (the owner's rule: host `fakepw` is a passphrase-protected key the local sshd accepts,
so ssh stops at `Enter passphrase for key` on the tty, `NumberOfPasswordPrompts 1`. A junk burst typed BEFORE the prompt must be discarded and not
reach any shell; the passphrase typed AFTER it must reach the prompt and log in; neither appears in any `rec.log`; a wrong passphrase returns to the
local shell where the next command runs locally. Needs the system OpenSSH sshd; fakesshd.py cannot prompt, so it skips there),
`shell_zsh`, `shell_fish`, `shell_nu`, `shell_pwsh` (`shell_fish` and `shell_nu` are also in SMOKE; binaries are looked up on PATH then ~/.local/bin, /home/gitlab-runner/.local/bin, /opt/homebrew/bin and added to the pane PATH; fish adds autosuggestion-does-not-steal-keys, `fish -c` and a `fish_postexec` log, nu adds `nu -c` and a nested nu; each shell as a subshell of the pane's bash: a probe line proves readiness, 12 typed lines, pasted
lines + instant Enter, Ctrl-C of a 30 s sleep then the next line, `exit` and the pane's bash runs the next command). A missing shell SKIPS with the
package: `apt install zsh`, `apt install fish`, PowerShell 7 from the Microsoft apt repo (`packages-microsoft-prod.deb`, then `apt install powershell`),
nushell is not in the Ubuntu repos (release tarball from github.com/nushell/nushell or `cargo install nu --locked`). The pane's own primary shell is
still bash (the harness rc files log its `rec.log`); a zsh/fish/nu/pwsh PRIMARY shell would need per-shell logging rc files in `profile.py`.
Windows adds `gitbash_subshell` (Git for Windows `bash.exe` started from the pane's PowerShell: 20 fast-typed lines, 4 pasted lines of 10-5000 bytes
with instant Enter, 3 Ctrl-C of a trapped `sleep 30`, `exit` back to PowerShell; skips when Git for Windows is absent). Windows runs the PowerShell subset
`editor_memory_7mb` (terminal pass only, #1340 P5: generates a 7 MB plain-text and a 7 MB Rust file, opens each in the code editor through the command palette with the absolute path, records the app's RSS before / after open and after closing the pane and waiting out the 60 s undo-close grace; fails when open costs more than `APPCI_EDITOR_MEM_TXT_MAX_MB` = 100 / `APPCI_EDITOR_MEM_RS_MAX_MB` = 150 MB over the pre-open baseline or more than `APPCI_EDITOR_MEM_CLOSE_MAX_MB` = 60 MB is still held after close; metrics `txt_rss_*_mb`, `rs_rss_*_mb` in `results.json`; `APPCI_KEY_PALETTE` / `APPCI_KEY_CLOSE` override the keys, `APPCI_EDITOR_MEM_STRICT=1` fails instead of skipping when the file never opens, `APPCI_EDITOR_MEM_KINDS=txt` limits it; needs `ps` RSS, so it skips on Windows, and on macOS the close phase is skipped because the frontmost-app guard refuses Cmd-W),
`tabs_two_windows` (`APPCI_TABS`, default 12 streaming tabs, a second window, typing and paste). Windows runs the PowerShell subset
(`fast_typing`, `paste_instant_enter`, `ctrl_c`, `latency_enter`, `idle_cpu`, `tabs_streaming`).

**UI cases (`script/realapp/appci/cases_ui.py`, #1462; all full-tier, none in SMOKE, Linux + macOS python harness only).**
Keys: Linux ctrl-shift-X, macOS cmd-X (`app/src/util/bindings.rs`). Every case verifies the shell's own `rec.log` and an effect file.
- `ui_tab_menu`: Tab on an ambiguous path opens the menu; plain accept and Down+accept each make the shell receive exactly one of the two candidates' full command, token in that file only, no file named like the uncompleted prefix.
- `ui_ghost_accept`: type a prefix of an earlier command, Right (default binding) + instant Enter, fast and slow: the shell gets the full earlier command. End is probed and recorded (`end_key_accepts_ghost`), not failed.
- `ui_ctrl_r`: Ctrl-R, a unique fragment, accept: the shell receives exactly the earlier command (one extra Enter allowed, both flows handled), effect file shows it ran twice.
- `ui_palette`: palette opens ('Search for a command'), nonsense filter shows 'No results', 'split pane right' + Enter makes a new pane (new shell prompt in `prompt.log`), typing there is exact, `exit` closes it.
- `ui_mouse_select_copy`: print a token absent from the command line, find it by tesseract word box, double-click then copy: the PRIVATE clipboard (wl-paste / xsel of the job's display, pre-set to a sentinel) holds exactly the token; a drag selection contains it. Skips on macOS (no pointer tool) and without tesseract.
- `ui_right_click_paste` (terminal mode only): right-click in the input, click 'Paste' (OCR), Enter: the shell receives exactly the clipboard command.
- `ui_drag_drop_file` (terminal only): file name with a space dropped onto the terminal; the shell must see ONE argument equal to the path. Skips unless X11 with `dragon-drop` installed (no tool synthesizes a drag source on Wayland/macOS).
- `ui_resize_typing`: window small, large, back (xdotool windowsize / sway floating resize / System Events); typing exact at each size, a wrapping 150-char command byte-exact; skips if the screenshot does not change (resize ignored).
- `ui_split_panes`: split right and down by key, new shell per pane, focus moved with next/prev pane keys, typing exact in both panes, `exit` closes the new pane.
- `ui_startup_time`: launch to first prompt (metrics `cold_s` with state dirs wiped, `warm_s` three relaunches; OS page cache is not dropped, that needs root); fails at no prompt in 90 s or warm median over `APPCI_STARTUP_MAX` (20 s); typing exact after each launch.
- `ui_settings_live`: theme, font size, `appearance.vertical_tabs.enabled` changed by writing the settings file (hot reload); each must visibly change the screen (Pillow diff, else hash) and typing stays exact; the original file is restored. The settings page is not clicked.
Packages the box needs for the full set: `tesseract-ocr` (positions), `python3-pil` (optional, stronger diff), `dragon-drop` (DnD only). The panes/palette/mouse/settings cases are not run in the sim backend.
Robustness cases (`appci/cases_robust.py`, #1468; all FULL tier, none in SMOKE; both modes; Linux unless noted). The quit and file-open ones
SKIP on macOS/Windows (Cmd-Q is forbidden by the Mac safety rules; the editor open uses the Linux D-Bus URL forward):
- `session_restore` (order 85): restarts the app with `general.restore_session=true` (the profile default is false), opens `APPCI_SR_TABS`
  (5) tabs each with its own cwd and one effect command, quits through the quit key (`ctrl+shift+q`, `APPCI_KEY_QUIT`; the confirm
  dialog is answered by Return / Tab+Return), relaunches on the same private profile, and proves: the tabs come back in order with their
  cwds (`ctrl+1..N`, `ctrl+9` = last tab, each answers `pwd`), the effect file is unchanged and rec.log has NO command from the restore
  (the scratch rc no longer logs a shell's loaded history tail at its first prompt, so any command in rec.log after a relaunch is real).
  Leaves a fresh restore-off app behind.
- `quit_24_tabs` (86, #1410): `APPCI_QUIT_TABS` (24) streaming tabs + a second window, quit key; the app must be gone within
  `APPCI_QUIT_BOUND_S` (10) s of the last confirm key, the app log must not contain `Failed to send Shutdown`, and no process whose cwd is
  under the profile may outlive the app (orphan check). Repeated `APPCI_QUIT_REPEAT` (2) times (the original failure was 1 in 3).
- `flood_typed_line` (60): `yes | head -n 2e6` and `seq 1 5e6` (scaled by `APPCI_SCALE`) with a line typed at 10-25 ms/key + Enter while
  it runs: the line runs exactly once, only after the flood (shell-side timestamps), rec.log shows exactly [flood, typed line]; fails if the
  flood ended before the typing did (then it tested nothing).
- `flood_typing_latency` (61): tab A floods (`seq 1 8e6`), tab B reads raw keys at the pty (`ts_reader.py`): 80 chars at 15 ms/key arrive
  complete and in order; gap p50/p95/max are metrics, a stall above `APPCI_FLOOD_STALL_MS` (1000) fails. No reference terminal is measured, so
  no latency verdict against one is made.
- `flood_scrollback_find` (62): a random uppercase marker is the first output line of a 300k-line flood; control: it is NOT on screen after
  the flood; `ctrl+shift+f` + the marker + Enter must bring it on screen (OCR). SKIPs without tesseract (package `tesseract-ocr`).
- `editor_edit_save` (70): opens a file in the code editor (`<bin> file://PATH` forwarded over the app's D-Bus session; the opener has no
  display in its env and is killed after 30 s), types an edit at 10-25 ms/key, `ctrl+s`, byte-exact on disk; undo xN + save = original, redo
  xN + save = edited; closes the pane, changes the file outside, reopens, edits again: the disk must hold the second edit on top of the
  external content (a stale reopened buffer fails); nothing may reach the shell.
- `editor_paste` (71, #1409): `ctrl+v` and `ctrl+shift+v` paste into the code editor; the saved file is byte-exact.
- `soak` (89): `APPCI_SOAK_MIN` (60) minutes of mixed use (8 typed commands, an instant-Enter paste, a tab opened/used/closed per minute;
  a 300k-line flood every 5th minute; in agent mode with a CI Ollama an agent prompt in a throw-away tab every 10th minute). Every minute
  the RSS and CPU of every process running the app's executable (main + terminal-server) is sampled to `soak_samples.csv`. Rules: every
  typed/pasted command arrived exactly; RSS FAILS when the series (first max(3, 10%) samples dropped, medians of windows of 5) never
  decreases by more than 2% between windows AND grows by more than max(`APPCI_SOAK_GROW_MB` 150, 25% of the first window) MB, or any
  sample exceeds `APPCI_SOAK_RSS_MAX_MB` (4096); fewer than 3 windows = noted, no verdict; idle CPU over 30 s at the end must be <=
  max(`APPCI_SOAK_IDLE_CPU_PCT` 5, start + 3) %.

#### Latency budgets and regression checks (`latency_enter`, `latency_key_log`, `ctrl_c_d`; Windows `latency_enter`)

The latency cases are pass/fail on the owner's budgets (3.6, `perf-budgets`), not CI-sized bounds. Every record is in
`results.json` under `metrics.lat` and in the `summary.md` latency table (case, Phosphor median/p95, reference median/p95, overhead,
budget, previous build, baseline, verdict, n).

- **Budgets.** Phosphor *overhead* = Phosphor median minus the reference terminal's median, measured in the same session with the
  same keys, cadence and timestamp command: Enter to command start <= 2 ms (`APPCI_ENTER_BUDGET_MS`), key to paint <= 2 ms
  (`APPCI_KEY_BUDGET_MS`). Ctrl-C <= 20 ms after subtracting the input-tool floor (`APPCI_CTRLC_MAX_MS`); the CI case is a plain
  `sleep`, not the output flood of 3.6. UI work: the app's slow-op log (#1146) only emits >= 8 ms, so `PASS` means no item >= 8 ms
  while typing and `WARN` means >= 16 ms (a dropped frame); the 4 ms line itself cannot be seen from the log.
- **Verdicts.** `PASS`, `FAIL`, `WARN`, or `NO-REFERENCE` when no reference could be measured the same way (never shown as pass).
  `latency_key_log` is `NO-REFERENCE` everywhere today: it reads the app's own `input_order` clock (keydown to first frame) and no
  reference terminal exposes a comparable one; absolute numbers and the old CI sanity bounds (p50 <= 20 ms, p99 <= 60 ms) still apply.
- **Reference terminal** (same session, Phosphor measured first, reference second, same rc and cwd): Linux Wayland = `foot` if installed,
  else `xterm` through XWayland (recorded in the `reference` name); Linux X11 = `xterm`; macOS = Terminal.app, driven through the same
  cliclick/osascript path under the frontmost-app guard (the guard is retargeted to Terminal while its window is driven, no
  Cmd-Q/W/Tab/Space, the window is closed by id and focus returned to Phosphor); Windows = `powershell.exe` in conhost, same SendKeys path.
- **Samples.** n >= 30 per measurement (`ctx.count`, so `APPCI_SCALE` smoke runs are marked "indicative only"), the first
  `APPCI_WARMUP` (3) samples discarded, median and p95 reported.
- **Regression rule.** The Phosphor median is compared with (a) the previous successful run of the same job on a `build/*` ref
  (fetched by `appci/prev.py` / `app_test.ps1` from the GitLab jobs API with `CI_JOB_TOKEN`, artifact `appci-out/<x>/run/results.json`) and
  (b) the committed `script/realapp/appci/baselines/<platform>.json` (`linux-wayland`, `linux-x11`, `macos`, `windows`). `FAIL` when the
  median is worse by more than `APPCI_REGRESS_PCT` (5 %) **and** `APPCI_REGRESS_MS` (0.5 ms); `WARN` between `APPCI_WARN_PCT` (2 %) and
  5 %. A suspected regression is re-measured once; if the second measurement is still regressed it stays `FAIL`, if it is clean it
  becomes `WARN`; both measurements are always kept in the record and the table. A missing previous run or a `pending` baseline is
  shown as such and checks nothing.
- **Promote a baseline** (coordinator, from a run on the build the owner judged good):
  `python3 script/realapp/appci/baseline.py promote <results.json> <platform>` then commit the file; `baseline.py compare <old> <new>`
  prints a delta table for two results/baseline files. The seeded files are `"status": "pending"`.
- **Caveat.** Headless-GPU (llvmpipe, software D3D12) numbers and the input-tool spawn cost make absolute values
  incomparable with the hand-measured baselines in `latency-baselines-r18`; the CI numbers compare builds to each other (and to the
  reference terminal in the same session), not to those baselines.

Other CI bounds: idle CPU <= `APPCI_IDLE_CPU_MAX` (15 %) of one core; with no reference terminal Enter to start must be <= 80 ms. The
owner budgets still need the real-GPU runs of 3.2-3.4.

Not covered by these jobs (still manual, section 3): the owner's real-config copy (CI has none, by design), reference-terminal
comparison beyond Enter latency, mouse selection/copy cases, Install-vs-"continue without installing" ssh flows, 24-tab soak, kill-switch A/B.
`APPCI_SCALE` (e.g. 0.3) shortens counts for a smoke run; `run.py --backend sim` runs the case logic against a bash in a pty (no GUI) to
test the harness itself.

Key-loss control (`key_loss_control`, terminal mode, #1414): in the first Wayland CI run (build/r18bn) the first character of
every wtype-typed line was missing (`echo` arrived as `cho`) in both modes. The app log showed no keydown for that key at all
(consecutive `seq`, nothing between), i.e. it never reached Phosphor's winit layer, which points at the injector/compositor: every
`wtype` call is a new virtual keyboard with its own keymap. Per the "no bad result without a control" rule the case types the same
sequences into Phosphor AND a reference terminal (foot, else xterm via XWayland, a different input path) in the same sway
session: per-line calls with the harness priming off, per-line calls with it on (the default), and ONE call for all lines.
`results.json` `metrics.key_loss.variants` and `summary.md` ("Key-loss control") carry lost-first / other-bad / missing counts
per target and injection. A Phosphor variant fails only when it loses more than the reference with the SAME injection; the
default (primed, per-line) injection must lose nothing in Phosphor. The Wayland backend leads every `wtype` call with a Shift_L
tap and a 20 ms sleep (`APPCI_WL_PRIME_MS`, 0 = off; it adds that much to input-tool spawn cost, which cancels in the
Phosphor-vs-reference latency overhead). Keep the control permanently.

**Reusing a build (`app-test-*-reuse`).** When a fix round touches nothing that goes into the app, rebuilding (Mac ~20 min, Windows ~38,
Linux ~30) tests the same bytes. The `-reuse` jobs extend the plain ones with `needs: []`; their `before_script`
(`script/ci/fetch_build.sh` / `.ps1`) uses the build **already on the same machine**. Each platform's app test runs on the machine that built it
(box runner, Mac runner, Windows VM runner), so every successful `build-*` job ends with `script/ci/cache_build.sh <platform>` /
`cache_build.ps1`, which copies `dist/` (with `SHA256SUMS`) plus a `meta.json` (job, pipeline, sha, ref, created) into
`$HOME/.cache/phosphor-ci/builds/<linux|macos>/<CI_JOB_ID>/` (Windows: `%USERPROFILE%\.cache\phosphor-ci\builds\windows\<CI_JOB_ID>\`), owned by the
runner user and outside the job's build dir (so the `git clean` / "Removing dist/" never touches it). The copy is atomic (tmp dir, then
rename) and only the newest 3 builds per platform are kept. The reuse job copies that directory to `dist/`, verifies `SHA256SUMS` and records
`"fetched": "local cache (<path>)"`; nothing is downloaded. **Only if the local copy is missing** (pruned, or built on another machine) does it log a
WARNING and fall back to downloading the GitLab artifacts: `GET $CI_API_V4_URL/projects/$CI_PROJECT_ID/jobs/$APPCI_BUILD_JOB_ID/artifacts` with the
`CI_JOB_TOKEN` header (then the by-ref endpoint, `APPCI_BUILD_REF` + `APPCI_BUILD_JOB_NAME`, on Linux/Mac). Play-time variables: `APPCI_BUILD_JOB_ID`
(required; the job fails with a clear message without it, or, on the fallback, on 404 = artifacts expired after 3 days, 401/403 = token
refused, or a zip without `dist/`), plus `APPCI_BUILD_PIPELINE_ID`, `APPCI_BUILD_SHA`, `APPCI_BUILD_JOB_NAME` (recorded). On the fallback, a masked CI
variable `APPCI_API_TOKEN` (read_api) is used as `PRIVATE-TOKEN` if the job token is refused. The tested build is written to
`appci-out/reused_build.json`, to `reused_build` in every `results.json` and to the
first line of `summary.md` (`record_reuse.py` / `fetch_build.ps1 -Record`, as `after_script`). The job-token fetch itself could not be
exercised outside a running job when this was written: the first real reuse run is its test.

### 2.9 Test tiers (what a push costs)

One driver, three sizes: `python3 script/ci/ci_seq.py build/<ref> --tier fix|candidate|release [--platforms linux,macos,windows]
[--tests '<nextest filterset>'] [--skip job ...] [--no-reuse] [--dry-run]` (default tier `fix`; bare job names after the ref still mean `--skip`).
It plays only manual jobs that have not run yet (never a re-run), passes per-job variables (`TEST_FILTER`, `APPCI_TIER`) through the
play API, and ends with a table of job, status and minutes. Heavy jobs stay manual: the script is how you start them, after asking.

| tier | runs | target | when |
|---|---|---|---|
| `fix` | `check-linux`; `test-linux` with a nextest filter; `test-warpui-core` / `test-workspace-rest` only if their crates changed; builds for the chosen platforms only; app tests in SMOKE mode on those platforms | ~1 h | every push of a fix |
| `candidate` | `check-linux`; all three suites unfiltered; all three builds; the FULL app matrix on every platform (both Linux passes) | ~2.5-3 h | a batch of fixes is ready; nothing is "done" until a candidate passes |
| `release` | candidate + `test-shuttle-loom`, `fuzz-smoke`, `bench`, `mutants`, `mutants-warpui-core` | ~8 h | before a numbered release (then also the manual checklist of section 5) |

Measured durations (box and Macs, warm caches): `check-linux` 7-9 min, `test-linux` ~50 min, `test-warpui-core` ~2, `test-workspace-rest` ~4,
`build-macos-arm64` ~21, `build-windows-x86_64` ~38. Linux build, app tests (smoke and full), shuttle/loom, fuzz, bench and mutants are not
timed yet; the tier targets above are estimates built from the numbers that are. Builds and the box suites overlap (the macOS and
Windows runners are separate machines), so a fix push is bounded by `check-linux` + the filtered `test-linux`/builds + smoke.

**Build reuse in the fix tier.** Before playing `build-X`, `ci_seq.py` looks at the newest `build/*` pipelines (at most 6 with a succeeded
`build-X`) and reuses one when its `build-X` job succeeded (its pipeline may still be running; the build is taken from the test machine's local cache, 2.8) and `git diff --name-only <its commit> HEAD` is empty over the app-affecting
path set AND the `build-X` job block (plus `.shim_github_env` / `.windows_job`) in `.gitlab-ci.yml` is textually identical. App-affecting
is everything EXCEPT the list `APP_NONAFFECTING` in `script/ci/ci_seq.py`: `docs/`, `*.md`, `.gitlab*`, `specs/`, `fuzz/`, `script/ci/`,
`script/realapp/`, `script/check_*`, `script/test_*`, `precheck`/`presubmit`/`state`, agent/editor files. So `app/`, `crates/`, `lib/`,
`Cargo.*`, toolchain, `.cargo/`, `assets/`, `resources/`, `themes/`, `script/{bundle,linux,macos,windows,install_*,
prepare_*}` and any new unlisted path rebuild (fail safe). Each platform prints its decision (`macos: reusing build job N from pipeline P
(commit C), app unchanged` or `macos: building, app changed in: <paths>`); `--no-reuse` forces builds. `candidate` and `release` NEVER reuse.
Reused platforms start their app test right after `check-linux` (no build to wait for); the smoke tier does not play `app-test-linux-x11-reuse` (it
would only exit).

**Fix-tier filter derivation.** Without `--tests`, `ci_seq.py` runs `git diff --name-only <base>..<ref>` (base = merge-base with the
newest other `build/*` ref, else `origin/main`; `--base` overrides) and maps paths: `crates/<name>/` -> `package(<name>)` (`warpui_core`
also runs its own suite and counts as a change to all of `warp`); `crates/warp_tui` -> `package(warp_tui)`; `app/src/<a>/<b>/x.rs` ->
`package(warp) & test(/<a>::<b>/)` (the last two path segments, `_test(s)` suffixes and `tests/` directories stripped, `mod.rs` = its
directory); non-Rust app files -> all of `warp`; workspace `Cargo.toml`/`Cargo.lock`/toolchain/`.cargo/config` -> nothing filtered, all
suites. Docs-only and CI-only diffs run nothing but `check-linux`. The derived filter, the reasons and the platforms are printed
before anything is played (`--dry-run` stops there): read it, and override with `--tests` when the mapping is too narrow (a change in
shared code that other modules' tests exercise) or too wide. Dependents of a changed non-app crate inside `warp` are not added
automatically. Platforms: a diff of only Windows (or macOS, Linux) paths builds only that platform; any shared code builds all three;
`--platforms` overrides. A filtered `check_test_failures` run still fails on any new failure but does not judge listed known failures
that did not run (that stale-entry rule needs a full run, which candidate does).

**Scoped mutation testing (release tier).** `.cargo/mutants.toml` + the `mutants` job: `pane_key.rs`, `model/key_facts.rs`,
`input_queue/gates.rs`, `input_queue/destination.rs` (about 900 lines, an estimated 80-110 mutants) with the tests limited to those modules
(`MUTANTS_TEST_FILTER`), each mutant costing one incremental `warp` rebuild: estimated 3-6 h, UNMEASURED (the first run records the
real number in `mutants.out/outcomes.json`). `mutants-warpui-core` mutates the clipboard (`clipboard.rs`, `clipboard_utils.rs`) against
warpui_core's tests only: estimated 25-40 mutants, under an hour. Widen by playing `mutants` with `MUTANTS_FILES` (space separated) and
`MUTANTS_TEST_FILTER` set, and split a wide run with `MUTANTS_SHARD=k/n` over several jobs; `cargo mutants --list --file ...` counts
mutants without running them.

### 2.10 Markdown and Mermaid readers (#1460)

What is covered, where, and how to update it. Nothing here runs cargo on the laptop; the box does (section 1).

| layer | where | what it checks |
|---|---|---|
| parser unit tests (pre-existing, ~142) | `crates/markdown_parser/src/*_test.rs` | individual constructs, incremental tail parse |
| markdown corpus + insta snapshots | `crates/markdown_parser/tests/corpus.rs`, fixtures `tests/fixtures/*.md`, snapshots `tests/snapshots/` | every fixture (headings, nested lists, task lists, tables, fenced code, inline, links/images, block quotes, HTML blocks, footnotes, unicode/RTL, CRLF, malformed, control chars, empty) parsed with and without GFM tables; the `FormattedText` document model is the snapshot. Also: no panic, deterministic, CRLF reads like LF, 600 KB single lines, deep nesting, streamed tail parse equals full parse. Tables: `parse_markdown_with_gfm_tables` is the runtime switch (no cargo feature) |
| mermaid corpus | `crates/editor/tests/mermaid_corpus.rs`, fixtures `tests/fixtures/mermaid/*.mmd`, snapshots `tests/snapshots/` | 14 valid diagrams (flowchart TD/LR, sequence, class, state, ER, gantt, pie, mindmap, journey, timeline, frontmatter theme, unicode labels, CRLF) rendered through `mermaid_asset_source`'s fetch, the exact app path: expected labels present, deterministic, normalized summary snapshot (rounded size, element counts, text labels; never the raw SVG). 9 `invalid_*` files: must return the app's error (which becomes `FailedToLoad` and the source fallback in `render_mermaid_diagram_section`) or a non-blank SVG, never panic. Hostile sources for every diagram kind, 300-node flowchart and 400-message sequence bound. The app's fetch is bounded (#1477): sources over 256 KiB or with a line over 4000 characters are refused up front (an unbroken 100k-character token made the renderer spin for minutes), and a render runs on a worker thread with an 8 s budget, ending as the error/source-fallback path; unit tests in `mermaid_diagram.rs` (`render_limit_tests`) |
| render-time bench | `crates/editor/benches/mermaid_render_bench.rs` | criterion; budgets 50 ms per small diagram, 2 s for 300 nodes / 400 messages; `MERMAID_BENCH_ENFORCE=1` makes a blown budget fail |
| cargo-fuzz | `fuzz/fuzz_targets/markdown_parser.rs`, `mermaid_render.rs`; seeds `fuzz/corpus/{markdown_parser,mermaid_render}` | run by `fuzz-smoke` (180 s each) |
| real-app CI cases | `script/realapp/appci/cases.py`: `md_viewer` (smoke + full, both modes), `md_viewer_big` (full only), `md_viewer_ai_answer` (agent, needs the CI Ollama model) | opens a `.md` with a table and a mermaid flowchart in the running app through a `file://` URL (Linux: second invocation forwarded over the app's D-Bus; macOS: `open -a <bundle>`), screenshot, OCR (tesseract on Linux, Apple Vision on Mac; no engine = screenshot only and a note) of the table text and the diagram node labels; 2 MB file: time to first heading and RSS growth (first-guess budgets 90 s / 3000 MB, tighten after the first numbers). The AI answer part is a note/metric only (model output is nondeterministic); no fake-model path exists. Windows: not covered (`app_test.ps1` has no OCR) |

**Updating snapshots** (first run after adding a fixture, or an intended parser/renderer change). On the box through the runner or
`script/agent-cargo <name>`:

```
INSTA_UPDATE=always script/agent-cargo <name> nextest run -p markdown_parser --test corpus
INSTA_UPDATE=always script/agent-cargo <name> nextest run -p warp_editor --test mermaid_corpus
cargo insta review          # or just read `git diff -- '*.snap'`; commit the .snap files
```

In CI: play the manual `insta-snapshots` job (`.gitlab-ci.yml`) on a `build/*` pipeline. It runs every package with insta
snapshot tests under `INSTA_UPDATE=always` and uploads every `snapshots/*.snap` and `*.snap.new` as an artifact; review and
commit them, then the normal test jobs compare against them.

A missing snapshot FAILS the test in CI (insta never writes there); the first box run of a new fixture therefore needs the
`INSTA_UPDATE=always` pass above. Review every diff: a snapshot change is a changed reading of the document, not noise. Do not
accept a diff to make a red test green without understanding it (`AGENTS.md` 5.6).

**Fuzzing**: `cd fuzz && cp ../Cargo.lock Cargo.lock && cargo +nightly fuzz run markdown_parser -- -max_total_time=120` (same for
`mermaid_render`); `docs/dev-tooling.md` has the rest. A crash artifact goes to `fuzz/artifacts/`; file an issue, add the input as a
fixture/seed, fix the code.

**Not covered yet**: pixel-level rendering of the diagram block (`crates/editor/src/render/element/mermaid.rs`), the notebook
`mermaid_render_offsets`, `render_mermaid_diagram_section` and the `ai_document` view beyond the real-app OCR check above.

## 3. Real-app testing

Automated tests do not catch lag, lost keys or broken flows; the owner feels those. Every release candidate
and every input/latency/ssh/bootstrap change gets a real-app pass on **Linux, macOS and Windows**.
Scripts are in `script/realapp/` (index: `script/realapp/README.md`); the latency tools are in
`script/keylat/`. Real hosts, users and paths are NOT in the repo: they are in the owner's local config
(`script/realapp/config.example.sh` shows the variables). Prefer real-app tests on a build over
code-refuter agents.

### 3.1 Rules that apply to every real-app run

- **Setup = the owner's, not a bare profile.** Test HOMEs are scratch COPIES of the owner's real config
  (`~/.config/phosphor` incl. vertical tabs, footer, themes, statusline, providers; a snapshot of
  `~/.local/state/phosphor` with the sqlite DB; bash rc/history, fish config), never the originals, deleted
  afterwards, secrets never printed. **Agent mode default**, autodetect + natural-language detection on,
  LAN Ollama provider (`gpt-oss:20b`; not the laptop-only Claude proxy on 127.0.0.1:8787 on other machines).
  Verify by screenshot before typing and after every relaunch (header "New Phosphor Agent conversation",
  input placeholder "Ask anything ...", model chip; the hint "/agent to start conversation" means terminal
  session = wrong). Restored tabs from the copied state must not be touched (one is the Windows VM).
  Redaction is OFF in the owner's config; ask before assuming. Always ASK the owner about setup instead of
  inferring.
- **Test BOTH modes**: agent mode (type commands into the agent input, autodetect routes them) and plain
  terminal mode, screenshot proof of each, results side by side, on every path (local, ssh with and
  without the remote extension, subshells, vim/less, every shell, every platform). One mode = not tested.
  Never type shell commands into an agent conversation that is busy replying/awaiting approval and call
  the result a routing bug (text typed during a reply is steering, #851/#1299).
- **Fast typing**: 10-25 ms per key in bursts with rollover (next keydown before previous keyup), Enter within
  ~15 ms, next command immediately; Tab / right-arrow / menu accepts 10-25 ms after ghost text or menu appears
  and sometimes before; plus the seeded expert-typist fuzzer (typos fixed with Backspace/Ctrl-W/Ctrl-U/left,
  Home/End edits, overtyping ghost text, accept-then-delete) 5+ min per context. 10-25 ms is a deliberate
  stress rate, not the owner's measured speed. Verify the EXACT command the shell received (file/log written
  by the command), not that "something ran".
- **Conflicts**: collide overlays (completion menu/ghost text with toast/card/tooltip/banner/palette/Find),
  typing under chip/agent/output/ssh updates, tab/pane switching with overlays up; check every key's destination
  and no stale overlays.
- **Validity checks before measuring** (discard and say so when they fail): ssh cases really run on the remote
  (prompt shows the remote host); seed a realistic history (>=2k varied commands, colliding prefixes) locally and
  remotely and confirm ghost text and the Tab menu appear; log every key the harness sends and replay the identical
  sequence into the reference terminal to attribute mangled input; include a routing corpus (>=200 commands +
  >=50 prose) reporting misroutes both ways.
- **Never dismiss a bad number**: prove harness overhead with a control and subtract it; an outlier is reported
  with its cause or "unexplained"; what the owner reports feeling is a bug until disproved (record it, pull the
  app log, do not argue). Measure agent-mode paths first (Enter in agent input to command start, typing while
  streaming, slash/@/model pickers, approval cards, ssh from agent input).
- **Reference terminal in the same session** for every latency case (Konsole/xterm, Terminal.app/iTerm2,
  conhost/Windows Terminal), one app at a time, Phosphor part first.
- **Safety** (all platforms): never kill by name or pattern (`pkill -f`, `pgrep -f` with a pattern that appears
  in your own command line kills your shell; read PIDs in one command, `kill <numbers>` in the next; kill only
  PIDs you started). Never touch the owner's installed Phosphor, config, DB, clipboard or tabs. Test builds run
  under their own `dbus-run-session` (single-instance forwards over the session bus; a leftover test instance on
  :99 stole a user launch once) and are killed when the round ends. Back up before, restore after, with
  checksums. Never edit a bash script while a run is executing it (bash reads incrementally). Chained box jobs
  fail closed: `git fetch && git checkout || { echo CHECKOUT_FAILED; exit 1; }`, log `git rev-parse HEAD`, compare
  binary md5s, never trust a 0-byte copy (`md5sum` != d41d8cd98f00...). Do not run CI jobs on a box while it is measuring.
- **Time boxes / watchdogs**: auto-approve agent commands (Ctrl-Shift-I) before measuring, clear queued rows,
  fresh conversation per case group, 60 s no-progress watchdog that screenshots and resolves cards, update the
  report after every case group (a Mac agent once sat 2 h on an approval card).
- **Results**: pass/fail table with repro counts (>=3 tries per failure), numbers vs the baselines in 3.6,
  evidence paths, issues filed. Do not close issues; post notes on the ones verified.

### 3.2 Linux

- Where: laptop or the build box, private Xvfb display (e.g. `:75`, `:98`/`:99`; two agents can use two
  displays), release `.deb` extracted to scratch (never `/opt/phosphor`), isolated `HOME`, `dbus-run-session`.
  Software rendering (llvmpipe): compare ratios and trends, not absolutes; use a real GPU session for absolute
  numbers; Wayland is covered by the CI harness (2.8: headless sway + wtype/grim); XTEST only reaches X11.
- Launch gotchas (each cost a failed run): under `dbus-run-session` gnome-keyring defers to the user's daemon
  and the app hangs after "Running app with windowing system: X11". Use a private `XDG_RUNTIME_DIR`, unset
  `GNOME_KEYRING_CONTROL` and `SSH_AUTH_SOCK`, and start `printf "" | gnome-keyring-daemon --unlock --components=secrets`
  inside the session before `exec`ing the app. Keep the runtime dir SHORT (unix socket path limit 108 bytes).
  Isolate HOME (the login shell reads `~/.bash_profile`). Global MCP config is `~/.phosphor/.mcp.json`.
  A fresh profile opens a "new agent conversation" overlay that swallows scripted clicks (Escape, click terminal,
  verify). View-only: `ffplay -f x11grab -framerate 15 -video_size 1600x1000 -i :99`.
- Harness `script/realapp/linux/` (consolidated from `linux-r18bi`): `env.sh` (display, run dir, binary),
  `launch.sh` (Xvfb + app under `dbus-run-session`, `RUST_LOG=warp::input_order=debug,...model_lock_trace=debug,
  warpui::frame_cause=debug,phosphor::routing=debug`), `h.sh` (xdotool `t`/`k`/`ent`, screenshots, log tail helpers
  `lgm`/`lgs`), `f.sh` (trial functions: `fastrun` fast typing exact-match, `tabrun` Tab then Enter at a gap,
  `pasterun` paste + arrow edits md5, `pe` paste + instant Enter, `ccrun` Ctrl-C delivery latency, `bigcat`
  multi-MB paste). `bin/`: `lat.py` key/enter-to-paint (phos vs xterm), `fuzz.py` expert-typist fuzzer, `rollover.py`,
  `fakesshd.py` (fake sshd on localhost for ssh-connect typing at 0/100/200/300 ms after Enter; the run's
  `fakessh/` keys are generated locally, not in the repo), `handoff*.sh` + `xowner.py`/`xmgr.py`/`xget.py` (clipboard
  owner / manager handoff, #1307: copy then quit then paste elsewhere; stalled owner leaves UI responsive), `cc.py`
  (Ctrl-C under flood), `chip*.py`, `altk.py`, `tabm.py`, `st*.py`/`cpu*.sh` (idle CPU), `cmds.py`/`cmdchk.py` (DB
  audit of what ran), `waitbox.sh`/`waitlock.sh`. `torture/`: `kseq.c` XTest sequencer with ms timing, `rec.c`
  recorder, `fuzz.py`, `lib.sh` (scratch profile from the owner copy), `run.sh`/`all.sh`/`scen.py` (same seed against
  Phosphor and a reference terminal).
- Earlier tab-scaling harness (idle CPU, main-thread CPU, RSS and typing cost per tab count with `PROMPT_COMMAND`
  markers, `BUSY=1` background ticker, `perf` at max tabs) is described in `docs/profiling.md` and the tab-lag
  notes (#805); under Xvfb `idle_main_thread_pct` is the number that transfers to a GPU box.
- Remote target: the build box over ssh, scratch dir `~/tmp-<name>`, **lock `~/perf-remote.lock`** (mkdir-style
  lock dir: wait for it, take it, release it), back up and restore `~/.phosphor` byte-identical, delete the
  scratch dir after, no environment/package changes on the box without the owner's OK. Agent provider: LAN Ollama,
  one request in flight, about one per 20 s.

### 3.3 macOS

- Box: M1 Pro, macOS 26, 16 GB, passwordless ssh, Homebrew at `/opt/homebrew` (`PATH=/opt/homebrew/bin:$PATH` in
  ssh commands). Dedicated to this project (not the owner's laptop) but the owner may be watching the screen live:
  keep visible activity purposeful. Owner's Warp.app and iTerm2 are theirs: never touch. Sshd has Accessibility
  and Screen Recording so ssh sessions can type (`cliclick`; Return via `osascript ... key code 36`, `cliclick kp:return`
  does not deliver) and screenshot (`screencapture -x`; 3024x1964 px = 1512x982 pt).
- Run in `~/phosphor-profile/<run>/`, launch with `open -n --env ZDOTDIR=<run>/zdot <App>`; `zdot/.zshrc`
  (`script/realapp/mac/r18bi/zshrc`) logs preexec/precmd into `ctl/log`. Never use disowned background runners in the rc.
- Safety (each from an incident): `caffeinate -dimsu` for the whole run (record PID, kill at end) and a screenshot
  check before typing, because a locked screen sends keys into the password field; **frontmost-app AND window-title
  guard before EVERY key burst, for reference apps too** (`h.sh` `guard()`); never generate modifier chords from
  random input (no Ctrl-Cmd-*, Globe/fn, Cmd-Q/W/Tab/Space, Ctrl-D; one run opened the emoji picker and typed into it);
  back up and restore `~/Library/Preferences/dev.phosphor.*` plists with sha256; do not modify `authorized_keys`;
  do not run CI on the box during a measurement (check `ps` for gitrunner rustc/cargo first); your own typing helper
  crashing looks to the owner like "Phosphor closed", so guard bursts with a PID check. Never `npm install -g create-dmg`.
- Harness `script/realapp/mac/r18bi/` (latest wave): `h.sh` (source: paths, `guard`, `front`, `apppid`), `fast.sh`
  (fast typing 10-25 ms with Enter after 15 ms; needs the `htype2` typing tool, built outside the repo), `lat.py`
  (key and Enter latency, Phosphor vs Terminal.app, JSON lines; includes a `cc.sh` timing control for harness overhead),
  `paste.sh` (paste + instant Enter, paste + Left x3 + type, Tab + Enter), `ssh2.sh`/`ssh_an.py`/`ksan.py` (ssh connect
  typing at offsets, expected exact lines in the remote log), `fail*.sh` (failing ssh plus continuous line + Enter must
  NOT run locally; expect the "Enter was held back" notice), `pw.sh` (wrong ssh password, no leak into the log),
  `sub.sh`/`nu.sh`/`nuexit.sh` (subshells incl. nushell, typing during setup, exit), `big.sh` (Ctrl-C mid command),
  `idle.sh`, `alt.sh`, `ks.sh` (kill-switch A/B: relaunch with `VAR=0`, 40 ssh trials). `mac/earlier/`: cold start,
  flood, mixed, UI, tab batches, Terminal.app comparison. Key to photon: `script/keylat/mac/keylat.swift`
  (ScreenCaptureKit, 120 Hz; resolution about 8 ms; fixed settle interval, so wrap it for random gaps).
- Build under test comes from the manual `build-macos-arm64` job (ad-hoc signed DMG artifact); install to a scratch
  location (`~/phosphor-profile/apps/<name>/Phosphor.app`), never over the owner's app.
- **CI runner (since 2026-11-02).** The GitLab runner runs as the standard user `gitrunner` (created by
  `setup-mac-runner.sh` to keep CI away from the owner's files) as a **per-user LaunchAgent**
  (`/Users/gitrunner/Library/LaunchAgents/com.gitlab.runner.gitrunner.plist`, Aqua session only). Mac CI jobs therefore run
  **only while `gitrunner` is logged in at the console** (fast user switching is fine; the screen must not lock: set its Lock
  Screen options to "Never"). After a reboot the owner unlocks FileVault, then logs `gitrunner` in. Accessibility and
  Screen Recording are granted to `/usr/local/bin/gitlab-runner` in that session. The old system LaunchDaemon is kept as
  `/Library/LaunchDaemons/com.gitlab.runner.gitrunner.plist.disabled`. `script/ci/mac/fix-gitrunner-login.sh`
  made the account loggable and moved the runner (`--apply --gui-runner`); `--apply --daemon-runner` reverts.
  Isolation still holds: `gitrunner` cannot read the owner's home and has no sudo (the script re-checks).
  Lesson: macOS shows another user's `AuthenticationAuthority` only to root and that user, so test a password with
  `dscl . -authonly <user>`, never by reading the attribute. Each Mac build's DMG step briefly opens a Finder window on
  that desktop (the bundler lays out the installer window); that is expected.

- **Local Network prompt and the CI harness (macharness2, 2026-11-02).** A fresh ad-hoc-signed build asks "Allow "Phosphor" to
  find devices on local networks?" the first time it talks to a LAN address (CI Ollama). The prompt is owned by a system
  process, is NOT the frontmost app (the menu bar still says Phosphor, `lsappinfo front` cannot see it), covers the window
  and eats keys. **The owner clicked Allow once in gitrunner's session on 2026-11-02**, but every new ad-hoc-signed build
  may re-trigger it (the grant is keyed to the code signature). The harness therefore checks, before the first key and in
  `MacBackend.guard()` before every burst, for a system dialog (`winrect list`: an on-screen window of
  UserNotificationCenter / CoreServicesUIAgent / universalAccessAuthWarn / SecurityAgent; plus a Vision OCR for
  "Don't Allow" + "local network" once before the first key). On a hit it STOPS: exit 3 and `NEEDS_OWNER.txt` (in the artifacts
  root and in `run/`) naming the dialog; the owner answers it once in gitrunner's session (Screen Sharing / console), then the
  job is re-run. The harness never clicks it. Idea, NOT implemented: a loopback forwarder (a tiny tcp relay started by the job on
  127.0.0.1:PORT forwarding to the Ollama host, with the profile's endpoint pointing at it) so the app only talks to 127.0.0.1,
  which needs no Local Network permission and would make the prompt impossible; the relay process (not the app) would then
  own the LAN connection, and it is a plain CLI child of the runner, which is not prompt-gated.
- **Mac harness PID/ref notes.** The app PID is resolved by the exact private bundle path inside the job dir (never by name),
  kept while verified (`kill -0` + command prefix), re-resolved on demand; "APP DIED" is only claimed when that PID is gone AND
  no process of the bundle exists, otherwise "PID lookup failed". One instance per pass is enforced. The window rect comes
  from `winrect` (CGWindowList; no System Events). The latency_enter reference is Terminal.app launched with
  `open -a Terminal x.command` (no Apple Events, no Automation grant), closed by typing `exit`; if it cannot be reached the
  verdict is "no-reference" with the reason. mode_proof reads the PRE-key screenshot (Escape leaves a fresh agent view).

### 3.4 Windows

- VM: Windows 10 22H2 x64, standard user (installs go through a separate admin account), also the GitLab Windows
  runner (if the runner hangs: Stop-Process, then `schtasks /run` as the standard user). Tools: Git for Windows (bash/vim/less),
  Win32-OpenSSH, Windows PowerShell 5.1; NO pwsh, no WSL. Shells offered by the app: PowerShell (default) and Git Bash.
  The VM has no ssh key to the box or the Mac, so only unauthenticated ssh rows can be tested; ask the owner if
  authenticated remote tests from Windows are needed. Never use `cmd` one-liners with `|`; write `.ps1` files.
  Never say "we have no Windows box".
- GUI runs execute inside the interactive desktop session: `indesk.ps1 <script> <timeout>` (schtasks /IT via a .cmd
  runner), `shot.ps1` for screenshots. `script/realapp/windows/lib/run.sh` uploads a step file and runs it that way.
- Safety: **lock file `C:\Users\<user>\WINPUSH.lock`** (agent name + time; if younger than 2 h wait/poll; delete when
  done) - only ONE agent drives the GUI/app at a time; headless PowerShell measurements that do not launch Phosphor
  need no lock. Back up the app data dir (md5), uninstall key, PATH, shortcuts and the `phosphor://` handler before
  installing a test build and restore over plain ssh (`lib/bak*.ps1`, `fin*.ps1`, `restore_sys.ps1`); the installed
  release (v0.1.15 at the time) must still report its version afterwards. Leave the GitLab runner alone.
- Harness `script/realapp/windows/`: `lib/lib.ps1` (foreground-guarded `Keys`/`Cmd` - `IsPho` refuses to type when
  Phosphor is not foreground - `Click`, `Drag`, `Wheel`, `Shot`, `Launch`, `QuitApp`, `KC` combos), `lib2.ps1`,
  `mouse.ps1`, `lat.ps1` (BitBlt region hash, +-16 ms capture resolution), `inst.ps1`/`launch.ps1`, backup/restore set;
  `cases-r18ba/` and `cases-r18bj/` (per-wave cases: typing, paste, ssh, approval cards, scene analysis
  `an_scene.py`); `hooks/` (PowerShell bootstrap hook profiling `prof*.ps1`/`hk.ps1`, ConPTY driver `cpty.ps1`; measures per-prompt hook cost,
  Warp-Precmd was 30-120 ms, #1163); `agentkey/` (agent-mode key delivery variants); `pwshmarker/` (line-end / ready
  marker experiments for the typeahead hand-off, #1162); `bashstall/` (Git Bash startup stall bisection, `rc_*`
  variants from `gen.sh`). Windows scripts contain `@WINUSER@`; render with `script/realapp/shared/render.sh`.
  Key to photon: `script/keylat/windows/lat.ps1` (`keybd_event`, needs the interactive session).
- Build under test: manual `build-windows-x86_64` (unsigned Inno installer artifact) or `build-windows-profiling`.
- CI runner: runs on the console (interactive desktop), not as a session-0 service, so `app-test-windows` launches the app,
  sends keys and takes screenshots from the job itself; the `indesk.ps1` indirection is only for hand-run work over ssh.
  `app_test.ps1` (#1413): the backup lives in `%USERPROFILE%\appci-bak\<CI_JOB_ID>` (OUTSIDE the job dir so a later git clean
  cannot delete it; newest 3 kept; `meta.json`, `md5_pre.txt`, data dirs, `uninstall.reg`, `handler.reg`, `path_pre.txt`,
  shortcuts). The finally block calls `windows/ci/restore.ps1`, whose steps (data dirs, uninstall key, handler key, user PATH,
  shortcuts, md5 + version verify) each run in their own try/catch with `ErrorActionPreference=Continue`; native `reg.exe` is
  judged by `$LASTEXITCODE`, never by stderr (a "successful" stderr line aborted the first run's restore). Results, junit and
  summary are written regardless. If a job's restore is marked failed: `powershell -File script\realapp\windows\ci\restore.ps1
  [-Bak <dir>]` finishes it by hand (default newest backup; log in `<Bak>\restore.log`). Before any case, a **key-delivery
  proof** types a marker command (absolute path) and checks it arrived, trying `SendKeys`, then `SendInput` unicode, then
  `SendInput` virtual keys; if none lands the mode stops with `keys not delivered: <reason>` (job/explorer/app session ids,
  foreground window) and runs no further cases. Latency verdicts need >= 24 samples (Phosphor and reference); fewer gives
  `no-data` / `no-reference`, never a computed overhead.

### 3.5 Case matrix (run on each platform; EVERY case in both modes)

1. **Fast typing / lost keys**: 10-25 ms/key incl. rollover, Enter within 15 ms, 50-command bursts, exact command
   verified; typing at 25 ms with Tab then Enter (Tab+Enter must run the COMPLETED line, local only, ~150 ms wait;
   Tab on a word with no completions then Enter submits in ~2 ms, #1246); arrow keys behind a paste (#1389);
   paste + instant Enter; 1 MB and 3 MB pastes (single word #955/#940); Ctrl-C/Ctrl-D at prompt and mid-command.
2. **Shells**: bash (3.2 on Mac and 5), zsh, fish, pwsh 7 and Windows PowerShell 5.1, nushell - as the tab default and as a
   subshell; hostile rc (errexit, nounset, PRINT_EXIT_VALUE, slow rc, vi mode, traps); unmanaged shells (`bash --norc`,
   `zsh -f`, `sh`, `fish --no-config`, `env -i bash`, tmux/screen); `read x` at a prompt; vim/less/htop entry + resize.
3. **Subshells**: `sudo -i`, `sudo -s`, `su`, `ksu`, `docker exec -it`, `poetry shell`, nested 3-4 deep; typing during
   setup (event-driven markers, #1354/#1372); Phosphorize via Ctrl-I / footer / Cmd-Enter (#976); exit back out; root
   shells read-only commands; record leftover processes.
4. **ssh**: connect typing at 0/100/200/300 ms after Enter, 30+ trials, `kh_handover` hops in the log, exact remote
   command (typeahead is HELD and delivered to the remote shell, never dropped or split, owner decision; keep the #983
   must-not-swallow cases: password prompts, host-key yes, failing ssh, command ssh, Ctrl-C); wrong password; failing
   ssh with continuous line + Enter must not run locally; with and without the remote extension; Install vs "Continue
   without installing"; remote subshells in every shell; tmux; dropped connection and reconnect; remote `less`/`vim`/`htop`;
   history, Up/Ctrl-R; leftover processes after disconnect/quit (box and client).
5. **Agent**: prompt -> streamed answer -> approval card -> run -> result; `/agent`, Cmd/Ctrl-Enter; autodetect + Enter;
   queued prompts; Stop/Ctrl-C mid-stream; approval card with a pause >1 s (Enter must not approve, #1350/#1362);
   attachments + Ctrl-C; agent `cd`/`export` persist; `exit`/`exec` must not kill the shell; pagers, `read`, redirects/
   here-docs byte-exact; unmanaged and root shells; routing corpus (section 3.1); slash/@/model pickers.
6. **Prediction**: Tab path completion (spaces, quotes, unicode, symlinks, 5k-10k entry dirs), autosuggest accept,
   history suggestions, per context; accepting never runs or misroutes anything; speed.
7. **Long-running and plumbing**: `sleep 600` + Ctrl-C, jobs control, redirects/here-docs/process substitution,
   `| head` SIGPIPE, `exec >file`; files byte-exact (md5/od).
8. **Copy/paste**: drag/double-click/shortcut/right-click, block copy, while output streams, in ksu, under mouse
   reporting, CLIPBOARD vs PRIMARY, OSC 52 from a remote program (must be blocked), clipboard handoff after quit,
   rapid double copy (second wins), image paste in agent mode; verify bytes with `pbpaste|od -c` / `xsel`.
9. **Terminal core**: 60 rapid resizes during output; 24-50 tabs and splits, close/undo-close; many tabs streaming
   (24) plus two windows with typing/paste; hostile escapes (OSC 52, title spam, DECRQSS, huge OSC 8); Unicode/emoji/RTL;
   Find; persistence (quit/relaunch, kill -9 restore, corrupted and locked DB: back up the data dir first).
10. **Performance**: key-to-paint and Enter latency vs budgets; idle CPU (1 tab, 21/50 tabs, vim with the mouse parked,
    quiet prompt); memory per tab; startup cold/warm x5; throughput (`seq 1 2000000`, 80 MB long lines, ANSI,
    `yes | head`); 30-min mixed soak; `leaks` (Mac) / heaptrack (Linux); Ctrl-C under flood; new tab to prompt;
    raw-input latency vs running-block size (fresh, 10k, 100k, 1M lines) local and remote.
11. **Kill switches**: on any failure run the case once with the relevant switch from 2.6 for A/B and record the result.

#### 3.5.1 International input cases (`appci/cases_caseintl.py`, `windows/ci/cases_caseintl.ps1`, #1466)

Full tier only (not in `SMOKE`), both session modes unless noted. Non-ASCII expectations are `\u` escapes in the source; every check
reads bytes (effect files opened in binary, the shell's own `rec.log`; for the marked-text rule a raw-tty reader,
`appci/bytes_reader.py`, logs every chunk that reaches the pty with its timestamp).

| case | platforms | proves |
|---|---|---|
| `intl_wide_type` | Linux, Mac (sim) | CJK, accented Latin, emoji and a ZWJ family typed at 10-25 ms/key + instant Enter reach the shell as exact UTF-8; screenshot of the rendered wide line |
| `intl_wide_paste` | Linux, Mac (sim) | the same strings pasted inside a typed command and as a whole pasted command + instant Enter; the harness clipboard round trip is checked first so a mangling clipboard tool is not blamed on the app |
| `intl_wide_edit` | Linux, Mac (sim) | Left / BackSpace over double-width and astral characters move by whole characters (checked on the received command) |
| `intl_wide_program` | Linux, Mac | `head -n 1 > f` (a running program, no Ctrl-D) receives typed and pasted wide text exactly |
| `intl_compose_linux` | Linux wayland (wtype) / x11 (xdotool) | xkb dead keys (`dead_acute`+e, `dead_diaeresis`+u, `dead_tilde`+n) and `Multi_key` sequences: NOTHING reaches the pty before the final key, then exactly the UTF-8 of the composed character; also in the input editor. Skips with the reason when the job locale is not UTF-8 |
| `intl_ime_japanese` | Mac | Japanese (Romaji, Hiragana) input source selected with `mac/tools/tis.swift` (TISSelectInputSource): `nihon`, `kyou`, `konnnichiha` + Return commit the exact kana in the editor and in a running raw reader; marked text does not leak romaji to the pty; Space conversion yields only non-ASCII text |
| `intl_ime_pinyin` | Mac | Pinyin - Simplified: `nihao` + Space commits exactly the UTF-8 of the first candidate in a raw reader and in the editor; no letters leak while composing |
| `intl_deadkey_option` | Mac | ABC layout: Option+e/u/n/i then the letter (e acute, u diaeresis, n tilde, i circumflex, capital with Shift), dead key + non-combining letter and + Space, Option+s/o/c direct characters; nothing reaches the pty before the commit |
| `intl_wide` | Windows | the same four strings typed (KEYEVENTF_UNICODE) and pasted; PowerShell writes them to a file, read back byte-exact |
| `intl_altgr_ps`, `intl_altgr_bash` | Windows | German layout (KLID 00000407) loaded for the Phosphor window only: AltGr+Q @, E euro, 7 `{`, 8 `[`, 9 `]`, 0 `}`, sharp s `\`, `<` `|`, `+` `~`, 2/3/M (superscripts, micro) and a plain Y key (must be z) typed into a command that writes them out; the received bytes are read from the file in PowerShell and in Git Bash (started from the pane, `LC_ALL=C.UTF-8`; skips when Git is not installed) |

Input-source rules (Mac): the harness only SELECTS among sources already enabled for `gitrunner`; it never enables or installs one. A
missing source skips the case with a `NEEDS_OWNER:` message and writes `NEEDS_OWNER_ime.txt` (run dir and artifact root) with the exact
clicks (System Settings > Keyboard > Text Input > Input Sources > Edit... > +, in the gitrunner session: Japanese - Romaji, Chinese
Simplified - Pinyin, English - ABC). ASCII (the `echo`, the redirect, the reader command) is always typed on the ABC layout, because
the injection tool posts unicode events that an active IME would compose; the IME is selected only around the composition, and the
original source is restored in a `finally` block. Windows: only the Phosphor window's layout changes (`WM_INPUTLANGCHANGEREQUEST`,
nothing system-wide); `Intl-Snapshot` saves the language list + `HKCU\Keyboard Layout\Preload` to `<backup>\kbd_pre.json` and the
`keyboard-layout` step of `restore.ps1` verifies and, if a load persisted, repairs them.

### 3.6 Latency: method, budgets, baselines

Method and protocol live in `script/keylat/README.md` (>=200 keystrokes per case, random 80-180 ms gaps plus the
10-25 ms stress profile and a background flood, p50/p90/p99/max with N, reference terminal in the same session,
calibration, record build/OS/GPU/refresh/config; at stress speed p90/max are backlog, not latency). Profiling:
`docs/profiling.md` (samply, Tracy, `profiling` build jobs).

**Budgets (owner-tightened; pass/fail):** key to paint next refresh, Phosphor's own work <=2 ms median / <=4 ms p99
(FAIL: >1 ms worse than the reference median, or any key > 1 refresh + 4 ms); Enter to command written <=2 ms (FAIL
>5 ms); ssh keystroke over LAN = plain ssh in the reference terminal +-1 ms; any single UI-thread item <=4 ms (log slow
>=8 ms, WARN >=16 ms); popup/menu/chip/toast/ghost text next refresh, no missed frame or layout jump while typing;
completion local <=20 ms, remote <= RTT + 20 ms, never on stale text; tab switch/split/resize next refresh; Ctrl-C under
flood <=20 ms; new tab to typeable prompt <=50 ms + the shell's rc time; **lost, duplicated or reordered keys, or a wrong
command executed: never (blocker)**. The practical target is slightly better than the reference terminal on the same
machine; the budgets are the ceiling. Mac Enter to command start of 6-7 ms is accepted.

**Baselines (round 18, v0.1.16; before-picture, median/p95/max ms):** Linux (Xvfb + llvmpipe, ratios only): Phosphor
local managed 11.7/14.8/20.4, `cat` 11.1/14.7/17.2, `bash --norc` 10.5/11.7/18.4; Konsole 11.1/13.5/15.7, Konsole over
ssh 23.4/31.2/33.6, xterm 3.4/4.8/6.0; owner config at 25 ms/key median 13-23. macOS (M1 Pro 120 Hz): local managed zsh
37-38/~42/42.5, `cat` 36-38/44-50/52, remote managed 37.5-38.5/~42/51 (reference apps to be measured with the same
tool); first key after ~1 s idle ~100 ms vs ~33 ms. Windows VM (software D3D12, +-16 ms): Git Bash managed 48/64/78,
PowerShell 61/77/93, PowerShell `echo hi` Enter to last paint 368/432/441. Shell hooks (box, Enter to next prompt
for `true`): plain bash 0.6 / zsh 0.3 / fish 1.0 ms vs Phosphor 35 / 27 / 25 ms before the hook work (#1164, #1170,
#1172). Startup: launch to first prompt 0.84 s cold / 0.89 s warm (Linux). Re-measure the fix build with the same method
and report deltas; a change passes when it is at or slightly below the reference terminal and never worse than v0.1.16.

## 4. Diagnostics

Logging is `RUST_LOG` (env_logger syntax). Release builds do not print the target, so each input-order line begins with
its own tag. Keep the input-order target on for any input/routing test (cheap) and save the app log with the results.
Log file: Linux `~/.local/state/phosphor/phosphor.log`, Mac `~/Library/Logs/phosphor.log`.

| target / switch | gives you |
|---|---|
| `warp::input_order=debug` | every key received, inserted, submitted; pty write (`pty_write` bytes + time), first output read after it (`pty_read` with gap), first frame after (`frame`); `kh_*` key-hop lines tracing a key through the pane: `kh_element_key_down`, `kh_pane_key` / `kh_pane_key_shadow`, `kh_terminal_typed_entry/routed/inserted`, `kh_editor_action`, `kh_editor_held`, `kh_editor_elem_typed`, `kh_focus_editor/input/terminal_view`, queue `kh_iq_*` (hold, queue, release, deliver, discard), hand-over `kh_hold_entry/exit`, `kh_holding_set`, `kh_handover`, `kh_handover_waits`, `kh_finish_begin/end`. Names align with the Tracy zones in `docs/profiling.md`. Source: `crates/warpui_core/src/input_order.rs` |
| `WARP_INPUT_ORDER_LOG_TEXT=1` | adds the typed text to those lines. Keystrokes can be passwords: use it ONLY with synthetic text, never with real input; default is length only |
| `warpui::frame_cause=debug` (and `warpui::notify_cause`) | names what asked for each frame (find the view/handler that dominates frame time, e.g. #1137) |
| `warpui::scene_stats=debug` | per-frame scene size (primitives, glyphs) |
| `warpui::gpu_done=debug` with `PHOSPHOR_GPU_TIMING=1` | GPU completion time per frame (off by default; the wait never runs on the UI thread) |
| `warp::terminal::model_lock_trace=debug` | holds of the terminal model lock >=2 ms (debug) and >8 ms (warn, rate limited 1/s) by named scope; the pty reader's `try_lock` fails while the UI holds it. `PHOSPHOR_RENDER_LOCK_LEGACY=1` compares |
| `phosphor::routing=debug` | agent/terminal routing decision per line (`PHOSPHOR_ROUTING_V1=1` for the old rules) |
| `PHOSPHOR_BOOTSTRAP_TIMED`, `bootstrap` timing | bootstrap InitShell to Bootstrapped timing |
| `PHOSPHOR_IMG_TIMING`, `WARP_STARTUP_TRACE` | image render and startup traces |

Slow-operation logging follows the budgets (summary >=8 ms, WARN >=16 ms). Samply/Tracy CPU profiles: `docs/profiling.md`.

## 5. Release test checklist

Run on a numbered release candidate, once per platform with a new agent per platform, against the previous release for
comparison.

1. Gates green on the candidate commit: `script/precheck` (box), `guards` + `check-linux` CI, `test-linux`,
   `test-warpui-core`, `test-workspace-rest` (play them; automatic only on tags), `integration-linux` (read, non-blocking),
   `integration-linux-pwsh`, `usage-test-linux` / `usage-test-macos`, `licenses`, `advisories`, `docs/STATE.md` current.
2. Builds: `build-linux-x86_64` (AppImage launches `--version`), `build-macos-arm64`, `build-windows-x86_64`; checksums
   recorded. Release pipeline on the tag only (`v*` matching `app/Cargo.toml`); never tag or push a release without asking.
3. Real-app pass per platform (section 3): the whole case matrix 3.5 in both modes, with the latency tables vs reference
   terminals and baselines 3.6, idle CPU, memory, startup, soak; ssh cases against the box; Windows against the VM.
4. Install paths: fresh install, upgrade over previous, uninstall; no autoupdate present (`check_no_autoupdate`);
   `phosphor://` handler; Windows PATH/shortcuts.
5. Per failure: >=3 repro tries, kill-switch A/B, app log attached. Compare with the previous release; new regressions block.
6. Cleanup/verification: test instances killed by PID, box lock released, `~/.phosphor` byte-identical, Mac plists/caffeinate
   restored, Windows backups restored and WINPUSH.lock deleted.
7. Update `docs/STATE.md` (generated by `script/state`, never hand-edited) and record known issues in the release notes.

## 6. Filing findings

- **Issue first, on GitLab** (`glab issue create`, project `winters-phosphor/phosphor`), before any fix and before reporting
  (`AGENTS.md` 5.11). No code changes in a test task. Issue numbers are kept across the GitHub-to-GitLab move.
- Contents: build/commit and platform, mode (agent or terminal) and shell, exact steps with timing, expected vs actual with
  numbers (N, p50/p90/max, the baseline and the budget), repro count, whether a kill switch (2.6) changes it, evidence:
  screenshot, the app log excerpt with `warp::input_order` hops (synthetic text only), harness script path and the control
  measurement. If the owner reported it, record it and pull the log, do not argue.
- Duplicate check against open issues and `DECLINED.md` (an `IMPROVED` entry is expected divergence). Known false positives
  are listed there. The oracle (`ORACLE.md`) is evidence ("is this our bug or upstream's"), not a specification.
- After a fix: a test that fails on the old code (Warp-mirrored where one exists), a kill switch for risky changes, then
  verify on a real build and post a pass/fail note with numbers on the issue; do not close it yourself.

## 7. Known gaps (no automated coverage today)

- The real-app layer is entirely manual/agent-driven; none of `script/realapp/` runs in CI, and the Mac typing tool
  (`htype2`) and Linux `kseq`/`rec` binaries are not built by any job.
- No automated key-latency regression gate on any platform (keylat is manual, not built by CI).
- `integration-linux` has no required-status gate in CI (it is a hard gate only in `precheck`); macOS and Windows have no
  nextest job (only `cargo check` and, for the Mac, `usage-test --surface tui`).
- Wayland input is untested; X11 only. GPU-backed rendering is untested on CI (llvmpipe only).
- The release pipeline has never run end to end; Windows and macOS Intel/Linux aarch64 are partial or dropped.
- `check_workspace_clean` is not wired anywhere.
- Windows and Mac real shells beyond what `real_shells` covers (nushell, pwsh 7, Windows PowerShell 5.1) are covered only by the
  bootstrap equivalence scripts, which skip when the shell is absent.
- Authenticated ssh from the Windows VM, and agent-mode tests against a cloud provider, are not covered.
- The "being added" tools in 2.7 are unverified until merged.

## 8. Test results history

Owner rule (2026-11-02): every test result is kept in the repo for regression review. `test-results/` is that archive; it is
append-only history, **retained forever** (it is small by design, see the size rules).

**Collect.** After a tier run, `python3 script/ci/collect_results.py <pipeline_id | build/ref> [--out test-results]` pulls every
finished job of that pipeline through `glab api`: the nextest jobs (Summary line, every FAIL/TIMEOUT/SIGSEGV with test names, the
known-failure notes, duration), `check-linux` (compile ok/fail, warnings by crate and kind), the guard jobs, bench/fuzz/mutants/shuttle
numbers when they ran, and the `app-test-*` / `*-reuse` artifacts (`results.json`, `summary.md`, `junit.xml`, `reused_build.json`,
latency tables, soak CSVs). `script/ci/ci_seq.py` ends every run by printing the exact collect-and-commit command; `--collect` runs
it for you. CI never pushes: the coordinator commits (`git add test-results && git commit`). Re-running for the same pipeline
replaces only that run's directory, so collect again once a still-running pipeline finishes.

**Layout.**

    test-results/INDEX.md                          one row per run + "vs previous run" sections (generated)
    test-results/<date>_<ref>_<pipeline>/
        manifest.json                              pipeline, commit, ref, tier, per-job status/duration/runner, reuse info, all app cases
        README.md                                  human summary + diff against earlier runs (generated from the manifests)
        unit/<job>.json                            nextest summary + failures; check-linux.json; guards; bench/fuzz/mutants numbers
        app/<platform>/                            results.json, summary.md, junit.xml, reused_build.json, latency/soak files,
                                                   shots/ (FAILING cases only, JPEG <= 1280 px wide; skipped with a note if PIL is missing)

Where a platform job dies before writing `results.json` (Windows when `report.py` never runs) the case table is rebuilt from the job trace
and the manifest says `from_trace`.

**Read INDEX.md.** Columns: date, ref, commit, tier (inferred from which jobs ran), compile (`check-linux`, warning count), unit
pass/fail/skip (summed over the nextest suites that ran), app pass/fail/skip per platform, headline latency medians with overhead against
the owner budget (`t:` terminal mode, `a:` agent mode), soak verdict. The "vs previous run" sections list NEW failures, FIXED failures
and latency regressions (> 5 % and > 0.5 ms on the Phosphor median, the `appci/lat.py` rule). Each unit job and each app platform is
compared with the most recent EARLIER run that has that same job/platform, not just the previous row, so a Mac-only reuse run does not
"fix" the Linux failures. A fix-tier run that executed fewer tests than its baseline never reports FIXED for the tests it did not run.

**Size and privacy rules.** App logs are never copied wholesale (only the failure excerpts inside `results.json`); **no screenshots or other images, ever**
(the repo is public; owner 2026-11-02): failing cases list their screenshot names, which live in the CI job artifacts (7 days).
A run directory over 5 MB is flagged; `script/check_test_results` (in `script/precheck`
and the CI `guards` job) fails above 10 MB per run and on anything that looks like an endpoint or secret. The collector scrubs
(`script/ci/results_scrub.py`) the values of `APPCI_OLLAMA_URL`, `PHOSPHOR_BOX_HOST/SSH`, `PHOSPHOR_MAC_SSH`, `PHOSPHOR_WIN_SSH`, `WINUSER`
(from the environment, `~/.config/phosphor-ci/ci.env` and `~/.config/phosphor-realapp.sh`), every non-loopback IPv4 address, home
directory user names, and token/key patterns, so collect on a machine that has that config. Archive files are text except the JPEGs
(`.gitattributes` marks `test-results/**/*.png` and `*.jpg` binary).

