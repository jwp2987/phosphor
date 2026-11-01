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
| macOS build / tests / real-app | Mac test box (M1 Pro, 16 GB, dedicated to this project) | manual CI jobs with tag `Mac`; GUI runs over ssh (section 3.3) |
| Windows build / real-app | Windows 10 VM (also the GitLab Windows runner) | manual CI jobs with tag `windows`; GUI runs through `indesk.ps1` (section 3.4) |
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
  `command-signatures-v2` is excluded (build script needs yarn, 0 tests).
- **warp_tui tests need the warp package in the same invocation** (feature unification of `test-util`):

      script/agent-cargo <name> nextest run -p warp -p warp_tui --lib --features warp/gui [filter]

  `-p warp_tui` alone fails to compile (`debug_text_content is not a member of trait Element`).
- Full workspace: `nextest run --no-fail-fast --workspace --exclude command-signatures-v2`; in practice use
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

| job | stage | trigger | what |
|---|---|---|---|
| `guards` | test | automatic | oracle pin fetch + all guards of 2.3 + `macos/test_create_warpctrl_wrapper` + `test_warpctrl_early_dispatch` |
| `check-linux` | test | automatic (~10 min warm) | `cargo check -p warp --features gui --lib --tests`, feature-unified multi-package check, `--features tui`, `-p warp_tui` |
| `test-linux` | test | manual (auto on tags) | `check_test_failures -p warp -p warp_tui --lib --features warp/gui` |
| `test-warpui-core` | test | manual (auto on tags) | `check_test_failures -p warpui_core --features tui` |
| `test-workspace-rest` | test | manual (auto on tags) | the 40 other packages in one invocation |
| `integration-linux` | test | manual, allow_failure | `crates/integration` under xvfb, 3 shards, `--profile ci` |
| `integration-linux-pwsh` | test | manual | the two pwsh scenarios with `PHOSPHOR_TEST_SHELL=pwsh` |
| `usage-test-linux` / `usage-test-macos` | test | manual | `script/usage-test --surface both` (Linux, xvfb) / `--surface tui` (Mac has no WindowServer for the daemon) |
| `licenses`, `advisories` | test | on box | `cargo deny check licenses sources`, `check advisories` |
| `check-macos`, `check-windows` | test | manual | cargo check of OS-only paths (`script/windows/bundle.ps1 -CHECK_ONLY`) |
| `diag-mac`, `diag-windows` | test | manual | read-only environment reports |
| `build-linux-x86_64`, `build-macos-arm64`, `build-windows-x86_64` | build | manual | AppImage+deb (launches `--version`), ad-hoc signed DMG, unsigned Inno installer; `dist/` artifacts, 3 days |
| `build-{linux,macos,windows}-profiling` | build | manual | unstripped `profiling` profile binary (`docs/profiling.md`) |
| `release-*`, `cli-*`, `release-publish` | build/publish | `v*` tag (manual dry run on main) | release pipeline, never run end to end on all platforms |

Kick off a build: `git push origin <branch>:build/<name>`, then `glab ci status -b build/<name>` or
`glab api "projects/88085499/pipelines?ref=build/<name>&per_page=1"`; play a manual job with
`glab api -X POST projects/88085499/jobs/<id>/play`. `script/ci/ci_seq.py build/<name> [skip...]` plays
check-linux, then the macOS/Windows builds, the Linux tests and the Linux build in order. Heavy suites are
long: ask first. Cancelling a running build can orphan cargo processes under `gitlab-runner`.

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

### 2.7 Tooling being added (round 18, agent `testlibs`, branch `fix/testlibs`) - TODO verify commands once merged

Not on `integ/r18` yet; the commands below are the intended invocations and MUST be re-checked against the
merged `Cargo.toml` / `.gitlab-ci.yml` before relying on them.

| tool | purpose | intended run |
|---|---|---|
| shuttle / loom (`warpui_core::sync_shim`, `clipboard_model_tests.rs`) | exhaustive/randomised interleavings of the concurrency models (clipboard owner thread, queue gates) | `nextest run -p warpui_core --features <shuttle\|loom feature>` |
| insta snapshots | golden output for TUI/render text and bootstrap payloads | `cargo insta test` / `cargo insta review` on the box only |
| criterion benches | repeatable microbenchmarks (existing: `warp_tui` `transcript_bench`, `zero_state_bench` with `--features test-util`; editor `buffer_bench`) | `cargo bench -p <crate> --features test-util` on the box |
| expectrl real-shell tests | pty-driven bash/zsh/fish tests in Rust instead of python scripts | `nextest run -p warp --lib --features warp/gui real_shells` |
| cargo-fuzz targets | parser/protocol fuzzing (escape sequences, OSC, DCS hooks) | `cargo +nightly fuzz run <target>` on the box, never in CI by default |
| cargo-mutants | do the tests catch a mutated line | `cargo mutants -p <crate>` on the box (slow, manual) |
| manual CI jobs | the above as `when: manual` jobs | play from the pipeline page; heavy ones ask first |

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
  numbers; Wayland is not covered by XTEST.
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
