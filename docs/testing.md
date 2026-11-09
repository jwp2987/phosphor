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
- Unit tests must run on the Mac runner's older git and macOS userland (first Mac unit run, 24 of 10,956 failed, none a
  product bug but one probe line): create fixture repos with `git init` + `git symbolic-ref HEAD refs/heads/<branch>`, never
  `git init -b` (needs git 2.28); a test that spawns a real shell must pin what macOS's `/etc/zshrc` and fish's `path_helper`
  override (HISTFILE, PATH); an Option-as-Meta key is `meta-<key>` on macOS, `alt-<key>` elsewhere. The shell-agnostic probe
  (`unknown_init_subshell.sh`) prints ESC as `\033`, because macOS `/bin/dash` printf does not know `\e`.
  `fuzz-smoke` on macOS passes `--target <rustc host>` to cargo-fuzz (its macOS release is x86_64 and would default to that).

**Before pushing:**

1. `script/precheck --fast` (seconds, no compile): toolchain pin, rustfmt (changed files parse; whole tree formatted via `check_rustfmt`), every guard
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
- **Code-editor memory accounting (#1607)**. `crates/editor/tests/editor_memory.rs` (its own test binary with a counting global allocator) loads a generated
  7 MB file with `Buffer::replace_all` and asserts the buffer retains nothing but its content tree (`Buffer::memory_stats`), no undo entry, a bounded transient peak
  and nothing after the drop; it prints the breakdown. `script/agent-cargo <name> nextest run -p warp_editor --test editor_memory --no-capture`.
- **CLI-agent plugin source (GitLab move)**. `app/src/terminal/cli_agent_sessions/plugin_manager/{claude,codex,gemini,opencode,mod}_tests.rs`
  pin the install/update commands to the `gitlab.com/winters-phosphor` URLs and cover the migration of installs registered
  from `warpdotdev/` GitHub: fixtures written in each CLI's real on-disk shape (Claude `settings.json`
  `extraKnownMarketplaces` and `plugins/known_marketplaces.json`, Codex `[marketplaces.codex-warp]` in `config.toml`, Gemini
  `.gemini-extension-install.json`, OpenCode `opencode.json(c)` `plugin` arrays with comments, tuples and other plugins,
  rewritten byte-for-byte except the one entry), local-path overrides never reported, and `is_warp_plugin_source` over every spelling
  the CLIs record (and the mirror/`file://` look-alikes it must reject). `install()` / `update()` are driven through
  `FakeRunner` (`plugin_manager/mod_tests.rs`, a scripted `PluginCommandRunner` that records each command line and its
  extra env, fails chosen prefixes and applies a command's on-disk effect via a hook): `update()` never moves a
  Warp-registered install, the move (`move_to_phosphor_source`) changes nothing on a failed pre-flight, restores the
  previous source on a failed add/install, serialises concurrent moves, and is not re-offered after a completed attempt;
  OpenCode's rewrite is atomic (read-only dir leaves the file untouched; a symlink keeps its link) and keeps comments.
  `driver_tests.rs` and `local_harness_launch_tests.rs` assert the background paths never call the move;
  `agent_input_footer/plugin_chip_tests.rs` pins the independent update/move dismissals; `shell_command_line` quoting
  is tested in `mod_tests.rs`. Pure file
  fixtures in temp dirs; no agent CLI or `git` is run.
  `script/agent-cargo <name> nextest run -p warp --features gui plugin_manager`.
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
- **Dead code is no longer blanket-allowed (#1467).** The crate-level `#![allow(dead_code)]` in
  `app/src/lib.rs` and `crates/warpui_core/src/lib.rs` is gone, so a new unused item warns. Items kept on
  purpose carry a narrow `#[allow(dead_code)] // reason` (used only by tests, by `warp_tui` under the `tui`
  feature, by other platforms, or persisted). The same change deleted test helpers nothing called
  (`check_cloud_folders`, `check_cloud_workflows`, `check_cloud_notebooks`, `assert_sorting_timestamp` in
  `cloud_object/model/model_test.rs`, `ensure_saved` in `notebooks/notebook_tests.rs`,
  `mock_workspace_with_shared_session` in `workspace/view_test.rs`, `build_test_http_client` in
  `ai/agent_sdk/test_support.rs`, `void_enter_gate_for_test` in `terminal/input/enter_queue.rs`); no test was removed.

### 2.2 Integration crates

- `crates/integration`: ~300 GUI-driving scenarios (`src/test/*.rs`: agent_mode, ssh, subshell, typeahead,
  bootstrapping, session_restoration, osc8_hyperlinks, settings_*, workflows, ...). Support code lives in
  `app/src/integration_testing/` (feature `integration_tests`). There is no headless winit backend on
  Linux, so it needs a display:

      xvfb-run -a -s "-screen 0 1280x1024x24" script/agent-cargo <name> nextest run -p integration --profile ci

  Environment: `xvfb`, a software Vulkan driver (`mesa-vulkan-drivers`), `libxkbcommon-x11-0`
  (`script/linux/install_test_deps`); `precheck` reports a missing piece as its own failure. Baseline is
  empty: any failure is a regression. Shard in CI with `--partition count:N/3`.
  The ssh scenarios (`src/test/ssh.rs`: `test_legacy_ssh_into_bash/zsh`, `test_ssh_into_sh/ash`,
  `test_ssh_with_shell_override`, `test_ssh_remote_bash_bootstrapped_hook`,
  `test_ssh_typeahead_during_connect_runs_remotely_once`; #1506) used to need Warp's private GCP instance via
  `gcloud` and therefore never ran. They now connect to a LOCAL OpenSSH `sshd` on 127.0.0.1 that the scenario
  starts and stops itself (`app/src/integration_testing/subshell/util.rs`, same recipe as
  `script/realapp/appci/sshd.py`): the current user, no privileges (`UsePAM no`, pubkey only), one sshd per
  remote login shell, a `ForceCommand` that sets `HOME` (scratch remote home) and `SHELL`, runs commands as
  `$SHELL -c` and otherwise prints a MotD and starts `$SHELL -l -i`; the remote shell has
  `PHOSPHOR_TEST_SSHD=remote` and `WARP_PROFILE_LOADED=true` exported. Two throw-away client keys: one without a
  passphrase (no prompt: used by the bootstrap and typeahead scenarios, so keys typed during the connect are real
  typeahead) and one with a passphrase (the stand-in for the old password prompt: `Enter passphrase for key`).
  The remote user and host are this machine's (`whoami`, `hostname`). Without `sshd` / `ssh-keygen` / the remote
  shell the scenario SKIPS with the reason on stderr; `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` (set by the
  `integration-linux` job) turns a missing or broken sshd into a failure (a missing remote shell, e.g. `ash`, still
  skips). The tmux-wrapper variants (`test_tmux_ssh_into_*`) need tmux on the remote
  and a package install and are not listed in `integration_tests!`; `test_ssh_into_fish` stays off (fish is not a
  supported remote shell). A scenario killed by a timeout can leave its sshd running (`ps`, stop by exact PID).
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

### 2.3 Guards (`script/check_*`)

Every guard in this table runs in `precheck`. In CI the `guards` job runs all of them except those the row says otherwise:
`check_license_config_sync` runs in `licenses`, `check_test_failures` in the test jobs, and `check_workspace_clean` nowhere.

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
| `check_rustfmt` | a tracked `.rs` file (outside `lib/` and test data) is not formatted under `.rustfmt.toml`, or does not parse. The tree was formatted in one commit listed in `.git-blame-ignore-revs`; fix with `rustfmt --config-path .rustfmt.toml <file>`. Also in the CI `guards` job |
| `check_queue_access` | the pane input queue is reached other than through `PaneQueue` (#1353) |
| `check_input_path_timers` | a timer on the input path is not a give-up net (owner rule 2026-11-08: a timer may only hand text back to the editor or keep it held; it never writes to the pty, releases held keys, routes, commits or re-asks). Scans `INPUT_PATH_FILES` (the list at the top of the script: `writeable_pty/`, `view.rs`, `view/paste_queue.rs`, `view/subshell_ready.rs`, `view/ssh_*.rs`, `input.rs`, `input/enter_queue.rs`, `input/paste_queue.rs`, `input_queue/`, `model/typeahead_handoff.rs`, `line_editor_status.rs`, `model/block/interaction_mode.rs`, and the bundled shell bootstrap/ssh scripts) for Rust `Timer::after` / `spawn_after` / `sleep` / `timeout` calls and Duration constants named `*_GIVE_UP`, `*_DELAY`, `*_WINDOW`, `*_QUIET`, `*_SETTLE`, `*_BUDGET`, `*_RESET`, `*_NET`, `*_CEILING`, `*_TIMEOUT`, `*_GRACE` and similar, and for shell `read -t`, `sleep`, `zselect -t`, `$SECONDS` / `$EPOCHREALTIME` arithmetic, `TMOUT`, `Start-Sleep`, `Wait-Event -Timeout`. Each must sit within 5 lines of a `give-up-net: <what it hands back / keeps held>` comment AND be listed in `script/input_path_timers.allow` (`path<TAB>identifier<TAB>what it does`); stale allowlist lines fail. The allowlist's last section, `# PENDING-CONVERSION`, holds the timers known to break the rule and being converted to events: the default run WARNS for each, `--strict` FAILS on any (CI job `input-timers-strict`, played by `ci_seq.py` in the release tier). Scope (widened 2026-11-09): `INPUT_PATH_FILES` are scanned in full; `INPUT_PATH_LISTED_FILES` (mostly-non-input files such as the @-menu, agent controller, clipboard, TUI views, `terminal_probe.rs`, the pty event loop) enforce only the identifiers listed for them in the allowlist, so UI/network noise stays out. A timer that never touches input stays in neither list. Three more owner-exempt classes (2026-11-09), each with its own section, its own required comment tag and no `--strict` count: `# UI-CONFIRM` (`ui-confirm:`; double-press confirmation windows and question auto-advance: `ENTER_OR_EXIT_CONFIRMATION_WINDOW`, `CTRL_C_EXIT_WINDOW`, `ASK_USER_QUESTION_AUTO_ADVANCE_DELAY`, `AUTO_ADVANCE_DELAY`), `# SAMPLING` (`sampling:`; timers that only schedule a probe sample, the probe or an event decides: `POLL_INTERVAL`, `judge`, `observe`; a timer that decides alone stays PENDING) and `# REPLY-PACING` (`reply-pacing:`; our own terminal-query replies, `REPLY_GATE_POLL*`, `REPLY_GATE_SLOW_AFTER`; also fails on a user-key API in its path). Still PENDING: `ENTER_VERDICT_BUDGET`, `ENTER_VERDICT_WALL_CEILING`, `PROBE_DEADLINE`, `DA1_DRAIN_GRACE` (being converted) and `wakeup_after_delay` (latches on elapsed time alone where there is no probe). A third section, `# PACING` (owner-exempt 2026-11-09), holds the timers that pace the app's OWN bootstrap bytes into a pty relay, container proxy or cooked tty whose consumption is unobservable (#960, #1384: `RELAY_BOOTSTRAP_PACING`, `CONTAINER_BOOTSTRAP_PACING`, `MAC_TTY_INPUT_QUEUE_PACING`, `write_next_paced_chunk`). They need a `pacing: <why>` comment instead of `give-up-net:`, are not pending under `--strict`, and the guard FAILS if a function that references a pacing constant (or is a pacing function) mentions a user-key API (`write_user_bytes*`, `user_bytes*`, `UserInput*`, `held_keys*`, `release_held_keys`, `typeahead*`, `PtyIntent`; a name-based check, extend `USER_KEY_RE` for a new entry point). `--self-test` proves the `pacing:` rule and also proves it catches an unannotated timer, a missing allowlist entry, a stale entry and the pending/strict split (it runs in `precheck` and the CI `guards` job). To add a net: put the `give-up-net:` comment next to it and one allowlist line; if you cannot honestly write the comment, the timer is not allowed. |
| `check_pty_doors` | a raw `write_to_pty(` / `clear_line_editor_and_write_to_pty(` / `write_end_of_transmission_char` / `emit(Event::WriteBytesToPty` outside the reasoned allowlist in the script (#1429-#1436). Allowlisted doors, each with its reason in the script: FOCUS_IN / FOCUS_OUT reports (protocol answers), the raw emit and its wrapper (definitions), the sink `write_user_bytes_to_pty_after_paste_stage` (what the pane queue delivers into), `release_held_keys` (keys the hand-off held, released in typed order), the OSC 52 clipboard-read reply, the subshell bootstrap writers (`write_init_subshell_bytes_to_pty`, `write_subshell_bootstrap_line`: the setup that owns the SubshellSetup gate), the PowerShell StopProfiles ^C (`ctrl_c_during_powershell_setup`), the ssh bootstrap-failure `stty sane` setup line ahead of the held typeahead (`hand_held_ssh_typeahead_to_unbootstrapped_remote`, #1494), the `PtyIntent` dispatcher and the pty controller. Door tests: the `*_door`-style cases at the end of `terminal/view_test.rs` (aws login is a command, synced input honours gates, agent writes refused during setup, editor Ctrl-D through the queue, CLI-agent text held by a gate, ssh warpify payload paced, programmatic insert waits for a waiting Enter, local-child harness command waits as a pending command). `check_queue_access` also runs in the CI `guards` job (#1448). Enter/paste coverage from the Mac run (#1452, #1453): `an_inline_editor_paste_then_an_immediate_enter_are_queued_in_order` (`terminal/view_test.rs`: the editor's inline-read paste is a pane-queue entry via `offer_answered_paste`, with the Enter behind it), `test_enter_behind_an_unhandled_edit_does_not_accept_the_stale_as_you_type_menu` and `test_enter_with_an_as_you_type_menu_and_no_selection_submits` (`terminal/input_test.rs`: Enter never accepts an as-you-type menu that answers older text or has nothing selected; it submits). #1459: in terminal mode the slash menu never swallows Enter unless it lists current rows for the typed text (`test_terminal_mode_slash_path_enter_runs_the_line_1459`; control `test_agent_mode_slash_menu_still_takes_enter_1459`, `terminal/input_test.rs`). appci keeps one `phosphor.<n>.log` per app instance in `logs/<mode>/` (#1457), and `ctrl_c_d` sends a Ctrl-D only after proving the cat / nested bash it ends is running (#1458). Pending-command coverage (`terminal/input_test.rs`): a deferred Enter (verdict / Tab-completion gate) refused because the shell has not processed its first Precmd is held as the pending command and runs once, exactly as typed, when the shell can take it; text typed during the wait never merges into it (#1558, `test_deferred_enter_on_unready_shell_is_held_and_runs_once_as_typed`, `test_text_typed_during_the_verdict_wait_never_merges_into_the_held_enter`, legacy and table); #1564: a DIRECT Enter refused only because the shell is not ready (NotBootstrapped / HistoryNotAppendable) is held the same way (`hold_refused_unready_enter`; the "still starting" toast stays as a notice; a running program and a shared-session participant still refuse): `test_direct_enter_on_unready_shell_is_held_and_runs_once_as_typed`, `test_text_typed_after_a_held_direct_enter_never_merges_into_it` (legacy and table), control `test_refused_enter_while_a_program_runs_is_not_held`. In an appci log, `enter_declined deferred=false held_as_pending=false` on a not-ready shell is the old bug. #1565 (fixed): keys typed into the focused INPUT EDITOR (agent view) during an ssh connect used to never reach `hold_input_during_ssh_connect` (r18cz: every key `ssh_hold=false target=Editor`, no `kh_hold_entry`) because `key_target` read only the already-active hold; the pane's focus facts now count an armed connect gate as the hold, and `legacy` mode arms the gate at the PreInteractiveSSHSession event like `table` (`ssh_keys_typed_in_the_agent_view_editor_during_connect_are_held_and_handed_over_once`, Legacy and Table, `terminal/view_test.rs`); in an appci log, `kh_pane_key ... target=Editor ... ssh_hold=false` during a connect is the old bug; text typed into a pending command is handed back with `TypeaheadHandoff` off too (#1451); `is_pending_aws_login` is set when the login starts, not when requested (#1450, `aws_login_is_a_command_that_waits_for_the_running_program`); the prompt look never takes the blocking model lock on event/timer paths (#1447, `prompt_look_outside_key_path_does_not_block_on_a_busy_model`); the ssh held-back Enter guard only holds the handed-back line (#1396, `ssh_handover_enter_guard_*`). Agent writes during an ssh connect's remote-setup window / subshell / PowerShell setup are REFUSED (logged), not held: the answer belongs to the prompt it was computed for #1561 / #1611 / #1612 / #1621 (owner decision 2026-11-04, `DECLINED.md`): the ssh connect reads NOTHING from the screen any more. The keys held since the submit go to ssh at the event that says ssh OWNS the tty (`CommandBecameInteractive` for the window's block, from the `tcgetpgrp` / `tcgetattr` probe; `TerminalView::ssh_on_tty_owned`), once and in order, and every later key passes straight through; the remote side decides (a shell reads them, a password read's `TCSAFLUSH` discards them). The one guard is a termios read at that event: canonical with echo off (a secret being read) discards what is held, as the tty would (`TerminalModel::tty_reading_secret`, `terminal_attributes::tty_is_reading_secret`). The prompt latch (`ssh_prompt_has_arrived`, the shell-vs-credential shape test, the table's H4 row and `SshPhase::PromptUp`) is deleted; the tests that encoded it were changed to drive the ownership event (`ssh_tty_owned_for_test`, `set_tty_reading_secret_for_test`): `ssh_shell_prompt_of_an_unbootstrapped_inner_host_receives_the_held_keys`, `ssh_secret_read_at_ownership_still_discards_the_held_keys_in_both_modes`, `ssh_ownership_delivers_the_held_keys_and_answers_pass`, `ssh_ownership_ends_the_hold_a_secret_read_discards_a_cooked_read_gets_the_keys`, `ssh_output_does_not_end_the_hold_ownership_does`, `ssh_output_of_any_shape_does_not_open_the_gate_ownership_does`, `ssh_keys_after_ownership_reach_ssh_with_no_burst_window` (Legacy and Table where the scenario has both, `terminal/view_test.rs`); every discard logs its input count (never the bytes). In an appci log, `ssh_held_delivery source=tty_owned` is the release and `kh_finish_begin tty_owned=true secret_read=true` the guard's discard; a `kh_table_drop` after a delivery means keys were lost (no row drops a held ssh input any more). #1641 (owner option (b)): under the ssh wrapper (`PreInteractiveSSHSession` seen for the window) the ownership event is deferred, not a release: the keys stay held until the wrapper's `ManagedSSHSessionStarting` hook (the managed hold delivers them at the remote shell's first prompt, after the bootstrap) or its `PlainSSHFallback` hook (the ownership applies then), and a block end without either hands them back as text; a secret read (canonical, echo off) under that hold discards what is held and passes the password to ssh (`managed_ssh_keys_are_held_through_the_master_login_and_delivered_after_the_bootstrap`, `managed_ssh_plain_fallback_releases_the_held_keys_at_ownership`, `managed_ssh_block_end_without_a_verdict_releases_the_keys_to_the_local_shell` (owner, after r18u16: a managed ssh that fails or ends with no verdict before its remote shell announced itself writes the held keys to the pty once, in order, for the local shell, like plain ssh in iTerm2; exact pty bytes asserted, editor empty, nothing kept), `managed_ssh_failing_without_a_remote_shell_releases_the_keys_locally_for_each_exit` (managed verdict then 255, no verdict with 1 and 0), `managed_ssh_password_typed_during_the_master_login_reaches_ssh`, control `unannounced_ssh_still_releases_held_keys_at_ownership`; all Legacy and Table, `terminal/view_test.rs`). In an appci log, `ssh_wrapper announced`, `ssh_wrapper tty_owned_deferred` and `ssh_wrapper verdict managed=...` are the new hops; `ssh_held_delivery source=tty_owned` before `verdict managed=false` under the wrapper is the old bug. #1641 review round: the wait for the verdict has no ceiling (`past_ceiling`) where the tty-read probe works (Linux `/proc`, `tty_read_probe_works`); without it (macOS) the 15 s ceiling's give-up hands the held keys back as text and releases the window (`managed_ssh_without_a_tty_read_probe_keeps_the_ceiling_and_hands_back`, Legacy and Table), only the first verdict from the announcing session counts, an announcement before the window arms is remembered, one after the ownership event takes the window back, and an echoing question during the master login is answered while the OS says ssh is blocked reading the tty (`terminal_attributes::foreground_reads_tty`, Linux `/proc` wchan/stat/syscall; unit tests `wchan_names_only_the_tty_read`, `a_blocked_read_fd_is_parsed_from_the_syscall_line`, `only_terminal_paths_are_ttys`, `the_tty_read_decision_needs_the_line_discipline_or_a_sleeping_tty_read` in `local_tty/terminal_attributes_tests.rs`; `a_reclaimed_tty_holds_again` in `input_queue/ssh_phase_tests.rs`; view tests `managed_ssh_verdict_before_ownership_is_honoured_for_each_verdict`, `managed_ssh_ownership_before_the_announcement_holds_the_later_keys`, `managed_ssh_announcement_before_the_window_arms_is_applied_when_it_arms`, `managed_ssh_ctrl_c_while_awaiting_the_verdict_interrupts_and_flushes`, `managed_ssh_a_second_verdict_is_ignored`, `managed_ssh_verdict_after_the_old_ceiling_still_delivers_once`, `managed_ssh_an_echoing_question_during_the_master_login_gets_the_answer`, all Legacy and Table, the last with a fake probe). The appci cases `ssh_connect_typing`, `ssh_enter_then_immediate_command` and `ssh_nested_typeahead` already assert the rule (each line exactly once, remotely, never locally) and are unchanged. #1571: an Enter refused only because the shell is not ready is a GATE (`Input::gate_unready_enter`, an `EnterVerdict` gate keyed with `UNREADY_ENTER_KEY_FLAG`, no queue cap): what is typed after it (text, Enters, and editing keys in their `Held` form) waits behind it in order, the editor never merges it into the held line, a second Enter is its own entry, the readiness event (`SessionBootstrapped` / the first Precmd, `retry_held_unready_enter`) runs the held line once and releases the rest like any Enter that ran; a deferred Enter's verdict gate is retagged in place; timers never run it (`execute_pending_command_after_timer`); its 60 s net (`UNREADY_ENTER_GIVE_UP`, 15 s until the owner decision of 2026-11-05) leaves the line in the editor, unrun, and DISCARDS what followed it with a toast that gives the count (#1661, owner decision 2026-11-05); ^C voids it (`test_unready_direct_enter_then_a_second_line_and_enter_run_as_two_commands`, `test_unready_deferred_enter_keeps_both_later_lines_behind_it_in_order`, `test_unready_enter_give_up_keeps_the_line_discards_the_rest_and_runs_nothing`, `terminal/input_test.rs`; `nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell`, agent mode, `terminal/view_test.rs`; all Legacy and Table). In an appci log: `enter_held_gate` at the refusal, `enter_queued_behind_held` for later Enters, `enter_held_retry` / `enter_held_ran` at readiness; `enter_held_give_up` is the net (the line stays, what followed is discarded, nothing run). #1572: ONE readiness predicate, `Input::shell_unready_for_submit` (not bootstrapped, history not appendable, no Precmd, no session id; a running program is not "unready"), decides both the held Enter and the ssh hand-over (`deliver_ssh_typeahead_if_ready` waits on it whether or not keys are held yet), so a managed remote hands over at `SessionBootstrapped`, not at its first Precmd; an unmanaged inner shell keeps the #1561 rule. #1579: a connect window armed at preexec (an ssh the submitted text did not show, e.g. an alias) inherits the submit's origin (`note_block_started_for_ssh`), so `legacy` mode holds the agent-view editor's keys too (`ssh_window_armed_at_preexec_for_a_users_submit_holds_the_agent_view_editor_keys`, Legacy and Table; control `ssh_submit_origin_is_consumed_by_the_block_it_started`). #1571 review fixes: (superseded by #1661: an unready Enter's give-up and a deferred `VoidEnterGate` now DISCARD what was typed after the Enter, pastes and Enters included, and `text_of_entries_typed_after` is deleted; the line alone keeps its guard); the ssh hand-back never discards (a running block: kept on the input and restored, guard armed, when a block completes; a non-empty editor: appended on its own line); the first-Precmd wait cap hands the held keys back instead of writing them; the submit origin is consumed before the in-band early return (`test_unready_enter_give_up_discards_a_paste_typed_behind_it`, `test_blind_enter_after_an_unready_give_up_is_held_by_the_guard`, `test_ctrl_c_voids_an_unready_enter_and_its_gate`, `terminal/input_test.rs`; `ssh_first_precmd_wait_cap_hands_the_keys_back_and_writes_nothing`, `ssh_origin_of_an_in_band_submit_does_not_leak_to_a_later_raw_block`, updated `ssh_held_line_is_never_glued_to_existing_editor_text`, `terminal/view_test.rs`; all Legacy and Table). |
| route parity A (#1852, #1854, #1855, #1856, #1857) | `terminal/view/route_parity_a_tests.rs`, Legacy and Table each: `suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does` (^Z / ^\ leave a waiting Enter and the keys behind it alone, ^C voids it), `a_delivered_key_cancels_a_written_setup_when_a_secret_is_read` (R8 check at delivery), `synced_input_does_not_take_over_an_agent_driven_command` (synced input is passive in both modes), `ctrl_c_before_the_connect_gate_is_armed_is_sigint` (P1e). #1857 is log-only: table mode now logs `kh_terminal_typed_entry`, `kh_hold_entry`, `kh_hold_exit` (with `table=1`) and `kh_terminal_typed_routed` like legacy; check in an appci log, not in a unit test. |
| `check_network_gate` | production code builds an HTTP client outside the network gate (`reqwest::Client::{new,builder,default}`, `reqwest::ClientBuilder`, `reqwest::blocking::Client`, `reqwest::get`), calls the unclassified `http_client::Client::new()`, builds a genai `Client` without `redirect_guard`, or the gate / its `http_client` wiring is gone. Tests and test support are exempt. The gate's switch and event log are covered too. Tests: `crates/network_gate/src/lib_tests.rs`, `app/src/network_gate_log_tests.rs`, `app/src/network_gate_notice_tests.rs`, `app/src/settings_view/network_page_tests.rs` |
| `check_outbound_redaction` | a model-provider wire call (`exec_chat`, `exec_chat_stream`, embeddings/Exa `.post(`) appears outside the files that route through `agent_providers/outbound_redaction.rs`, or one of them stops calling it (#1469). Tests: `agent_providers/outbound_redaction_tests.rs` (every content kind, on and off), `chat_stream` request-builder tests, `oneshot` and embeddings tests |
| `check_dangling_modules` | `mod foo;` with no file (untracked file lost in a patch); also in the CI `guards` job (#1521). Skips `lib/` (vendored crates; some declare modules their own build.rs generates) |
| `check_pwsh_parse` | a bundled `.ps1` does not parse in real PowerShell (#1161); skipped without pwsh, which `precheck` shows as a warning; also in the CI `guards` job (#1521), where a runner without pwsh logs `SKIPPED` and passes |
| `check_license_config_sync` | `deny.toml` and `about.toml` license lists differ (job `licenses`) |
| `check_test_failures` | see 2.1 |
| `check_workspace_clean` | NOT wired anywhere, deliberately: it expects a hand-managed shared checkout (branch `main`/`working`, no uncommitted work), which a CI clone on a `build/*` branch is not. It refuses to build a checkout holding another author's uncommitted work |

### 2.4 Shell-script harnesses (`script/test_*`, no cargo; run by `precheck`)

They run the REAL bundled bootstrap text (cut out of `app/assets/bundled/bootstrap/*`) in real shells and
pin protocol behaviour. Skips are reported as SKIPPED, never silent. A Skip raised AFTER a failed check keeps the FAIL (noted "ALSO SKIPPED"); `python3 script/realapp/appci/run.py --self-test` covers that rule.

| script | checks |
|---|---|
| `test_bash_bootstrap_nounset`, `test_zsh_bootstrap_strict_options` | hooks survive `set -u` / `err_return` / failing user precmd (#825, #836, #844); a detached HEAD under strict options reports git's hash every prompt (#1616). Option guard (every hook entry saves the user's options, turns them off for its own body and gives them back exactly): bash also covers `set -E` + an ERR trap (never fires inside a hook, once for the user's own `false`), `set -a` (no hook scratch variable is exported), `set -x` (no hook body is traced), and `$?` / `PIPESTATUS` / `$BP_PIPESTATUS` as the user's PROMPT_COMMAND sees them, plain and under `set -u -o pipefail`; zsh covers `pipe_fail` + `all_export` + `xtrace` the same way. fish has no errexit; its `$status` capture is the first line of `warp_precmd` |
| `test_fish_bootstrap` | fish hooks fire end to end in a pty (#829, #879) |
| `test_hook_forks` | processes the prompt hooks start per prompt, in the REAL assembled bash, zsh and fish bootstraps (the Shell class of `test_cwd_listing`), local shell, in a plain directory and in a git repository whose directory and branch names need JSON escaping, with the prompt honored (`WARP_HONOR_PS1=1`) and with the rendered PS1 carried in the Precmd: every shell runs under `strace -f -e trace=execve,clone,clone3,fork,vfork`, `true` is typed ten times, and the successful execs (named) and non-thread forks between the first Enter and the last Precmd are counted per prompt. Budget: zero of each, every shell (fish gets a plain prompt: its stock one runs git, which is the user's prompt, not a hook). Each Precmd must be valid JSON and the odd branch/dir names must arrive intact. Also prints the median Enter-to-Precmd of `true` (untraced). `--root DIR` measures another checkout's bootstraps (a worktree of an older commit gives the "before" numbers), `--keep DIR` saves the traced segments, `--no-budget` only prints. Without `strace` the counts are SKIPPED and only the timing runs |
| `test_bootstrap_hook_equivalence` (bash/zsh/fish), `test_nu_bootstrap_hook_equivalence`, `test_pwsh_hook_equivalence` (+ `testdata/pwsh_hooks_golden.json`) | fork-free hook helpers emit byte-identical DCS payloads to the old forking code (#1170, #1233, #1231); a detached HEAD runs git once per directory and HEAD commit, a moved HEAD is seen at the next prompt, `PHOSPHOR_GIT_HEAD_CACHE=0` restores git per prompt (#1616); fish's in-process hex encoder (ASCII, at most 2 KiB, switched on by a probe the helper file runs too) and its `string replace` JSON escape are compared with `od` / the sed pipeline over every byte value, random strings and the 511..2049 byte boundaries |
| `test_nu_agent_pager_env` (#1813) | REAL nushell (0.116.1 checked; skipped cleanly when `nu` is absent, `--nu PATH` to force) in a pty with the real `nu.nu` bootstrap, a throw-away HOME/XDG under `$TMPDIR` and a `config.nu` holding the user's own `pre_prompt`/`pre_execution` hooks. An agent command is typed as Nu's wrapper form (` <command> # __phosphor_ap_pagers`, `NU_PAGER_MARKER`): `cd` and `$env.X =` persist; `PAGER`/`LESS`/... are set during it (nushell and an external child); the user's own `PAGER` is back, and variables the user never had are gone, after a success, after an external command exiting 3 and after a nushell error; an unmarked line is untouched; the user's hooks ran and ours are appended; no agent line is in the history (a control line without the leading space is). In `precheck`. Unit twins in `agent_shell_wrap_tests.rs`: `nushell_commands_are_not_wrapped_in_a_block` (owner-decision expectation change: the Nu line now ends in the marker comment), `nu_bootstrap_matches_the_overrides_and_the_marker` | the script's own checks |
| `test_bash_bootstrap_builtins` | fork-free `sort -V`/os-release/`shopt` replacements match the commands (#1164) |
| `test_bash_preexec_large_command` | Preexec escaping of a 3 MB command is linear (#1255) |
| `test_init_shell_escaping` | InitShell JSON round-trips hostile user/host values (#879) |
| `test_hijack_bootstrap` | user `echo/printf/cd/read/pwd` functions never run inside the bootstrap (#929) |
| `test_line_editor_markers` | `LineEditorReading` / `ContinuationPrompt` OSC markers and `seq` per Precmd (#1390-#1392) |
| `test_local_counted_bootstrap` | counted-read bootstrap behaves identically to the here-doc form (#1226) |
| `test_ssh_keepalive`, `test_ssh_probe_cache`, `test_ssh_wrapper_forks`, `test_ssh_raw_bootstrap_hook` | ssh wrapper options, probe cache, fork budget, `raw_bootstrap` hook (#1171, #1136, #1172, #1243) |
| `test_ssh_auth_once` | a failed ssh login (master step 255, no control socket) or a Ctrl-C at its prompt runs ssh ONCE and returns its status, so no second password prompt eats the next command (#1626); a ssh that authenticated but cannot multiplex (socket created, then 255) still falls back to the user's plain ssh once (#1551); r18u13: the same, plus a successful login, with a `$HOME` that is not the passwd home and the absolute passwd-home SSH_SOCKET_DIR the app now exports there (the fake ssh expands `~` from `$PWHOME`, as ssh does from the passwd entry; the old literal `~/.ssh` export made the wrapper miss its own master and log in twice): the wrapper logs in ONCE and multiplexes the session over its master; #1641: the hooks the wrapper prints are logged in order with the ssh calls: `ManagedSSHSessionStarting` exactly once after the master and right before the managed session, `PlainSSHFallback` right before each plain ssh (mux failure, a RemoteCommand, a non-POSIX remote, MSYS2 in bash), nothing on an auth failure or Ctrl-C; #1641 host-key retry: under the default `stricthostkeychecking ask` the master carries `StrictHostKeyChecking=yes`, an unknown host (real `ssh-keygen` known_hosts lookup) gets ONE plain ssh retry with the fallback hook before it, a known host's auth failure makes exactly one call, a user's `accept-new` gets no override (these cases need ssh-keygen and are SKIPPED without it); bash/zsh/fish |
| `test_native_completions` | the in-band native-completions exchange (#1801) in the REAL assembled bash and zsh bootstraps (their own bind lines): the Bootstrapped hook advertises `!phosphor_nc_frames_v1`; candidates (`cd pro`, the WHOLE token `dir/file1`, the `a;` lead; bash `complete -F`/`-W` specs, COMP_WORDS/COMP_CWORD, the `--opt=` part); a command behind the frame (same write, 5 ms later) runs; the app's real command bytes (^P + bracketed paste) and a bracketed paste alone (ESC first) right after ^Y run whole; the cancel frame and a stale-token frame give no candidates and the next command runs; a frame landing at the prompt is dropped; a frame's start alone at the prompt stops at the next real key; a frame's tail after a 5 s stall is never run and the real command after it runs whole; ^Y + cancel + command in one batch; a DA reply after ^Y leaves the line clean; a bare RS then a word + Enter runs the word; a SIGWINCH during the hook loses no key; US (^_) is still the line editor's undo; vi mode (bash `set -o vi`, zsh `bindkey -v`); zsh without zsh/datetime. bash below 4.0 SKIPPED by design; fish not exercised (off the path). A missing shell is SKIPPED |
| `test_cwd_listing` | the bounded cwd listing a REMOTE shell adds to its Precmd (#1801), in the REAL assembled bash, zsh and fish bootstraps (fish is covered here; a missing shell is SKIPPED): the first prompt after the bootstrap carries the listing once, exactly the real entries (hidden included, a symlink to a directory `d`, a broken link `f`); an unchanged directory sends none; a created file or a `cd` sends the new one; awkward names (space, quote, backslash, `%`, newline, tab, ESC, `;`, `$`, `\|`, leading `-`, non-ASCII, non-UTF-8) never break the JSON and decode to the real names; 600 entries give `P` with exactly 500, long names stay within 32 KB, 2500 give `T` and the directory is not globbed again until a `cd`; a LOCAL shell (`WARP_IS_LOCAL_SHELL_SESSION=1`) and `PHOSPHOR_CWD_LISTING=0` send none; prints the shell-side cost per call at 10/500/2000/10000 entries and the Enter-to-Precmd time in a 10000-entry vs an empty directory (budgets fail a regression to a fork per entry) |
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
fail with 403). Project-level CI variables: only the masked `APPCI_API_TOKEN` (read_api, used by the reuse fetch when the job token is refused, section 2.8); personal values never go into GitLab. Signing credentials do not exist yet (`docs/release-signing.md`).

Machine-local test settings (owner-provided, NOT in the repo or GitLab), per runner user, mode 600, in
`~/.config/phosphor-ci/` (Windows: `%USERPROFILE%\.config\phosphor-ci\`): `ci.env` (`APPCI_OLLAMA_URL`,
`APPCI_OLLAMA_MODEL`; KEY=VALUE) and `owner-settings.toml` (an exact copy of the owner's Phosphor `settings.toml`: layout,
vertical tabs, theme, fonts, footer, agent and ssh settings). Present for `gitlab-runner` on the box, `gitrunner` on the
Mac and the console user on the Windows VM. The harness starts its profile from them (2.8); their contents are never
printed or uploaded. Re-seed all three when the owner changes their config.

Rebuilding a lost CI node (box, Mac or Windows VM) from scratch: [`script/ci/nodes/README.md`](../script/ci/nodes/README.md)
(dry-run-by-default bootstrap scripts with the versions observed on each machine, runner registration and tags, the
machine-local files to restore, and the jobs that prove a rebuilt node).

Build-box GUI stack for `app-test-linux-*` (installed by the owner 2026-11-01): `sway`, `wtype`, `grim`, `wl-clipboard`,
`xwayland`, `xterm` (reference terminal); `gitlab-runner` is in the `video` and `render` groups for `/dev/dri`.

Box target-dir pruning: the persistent cargo target dirs under `/home/gitlab-runner/.phosphor-build/targets/<label>`
are shared by every `build/*` branch and used to grow unbounded (2026-11: `ci-test` 219 GB, `ci-check-linux`
119 GB, disk full, jobs failed). `script/ci/prune_target.sh` runs at the start of `.box_job` (all box jobs) and
`check-linux`; it never fails a job and logs `[prune_target] <label>: before -> after`. Per label it (1) always
deletes `incremental/` of release/LTO profiles, drops nextest/junit output older than a day; (2) if the filesystem
has < 80 GB free or the dir exceeds its cap (`ci-test` 120 GB, `ci-check-linux` 60 GB, others 60 GB) deletes all
`incremental/`, then files unmodified and unaccessed for 7 days, and if still over, wipes the dir. Plain
`find`/`du`/`df`, no cargo-sweep; the cargo registry is untouched. `script/agent-cargo` writes an in-use marker (`<root>/.inuse/<label>.<pid>`) while a cargo process compiles into a label; prune skips a label with a live marker and re-checks before the stale pass and before the whole-dir wipe (#1523). The few ms between the last check and `rm -rf` remain; the `resource_group`s serialise the box jobs. Tunables: `PRUNE_MIN_FREE_GB`,
`PRUNE_STALE_DAYS`, `PRUNE_CAP_GB_<LABEL>`, `PRUNE_DRY_RUN=1`.

| job | stage | trigger | what |
|---|---|---|---|
| `guards` | test | automatic | oracle pin fetch + all guards of 2.3 + `macos/test_create_warpctrl_wrapper` + `test_warpctrl_early_dispatch` |
| `check-linux` | test | automatic (~10 min warm) | `cargo check -p warp --features gui --lib --tests`, feature-unified multi-package check, `--features tui`, `-p warp_tui` |
| `test-linux` | test | manual (auto on tags only, owner 2026-11-02 #1503: pushes to `main` do not auto-run suites; `AUTO_DERIVE` derives the filter from the pushed diff with `ci_seq.py --derive-env` when set) | `check_test_failures -p warp -p warp_tui --lib --features warp/gui`, plus `-E $TEST_FILTER` when that variable is set (fix tier; a filtered run skips the stale-known-failure rule, and FAILS when one of its `test(/.../)` terms selected no test that ran: `script/ci/check_filter_terms.py`, #1504) |
| `test-warpui-core` | test | manual (auto on tags only, #1503) | `check_test_failures -p warpui_core --features tui` (honours `$TEST_FILTER`) |
| `test-workspace-rest` | test | manual (auto on tags) | the 40 other packages in one invocation (honours `$TEST_FILTER`) |
| `integration-linux` | test | manual, allow_failure (gating in the candidate/release tier verdict, owner 2026-11-09) | `crates/integration` under xvfb, 3 shards, `--profile ci`; sets `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` and checks `sshd` / `ssh-keygen` first (the ssh scenarios use a local sshd, 2.2) |
| `integration-linux-pwsh` | test | manual | the two pwsh scenarios with `PHOSPHOR_TEST_SHELL=pwsh` |
| `usage-test-linux` / `usage-test-macos` | test | manual | `script/usage-test --surface both` (Linux, xvfb) / `--surface tui` (Mac has no WindowServer for the daemon) |
| `licenses`, `advisories` | test | on box | `script/ci/cargo_deny licenses|advisories` (cargo-deny pinned, installed only when missing; a failed advisory-DB fetch falls back to a cached DB under 7 days old, with a WARNING) = `cargo deny check licenses sources`, `check advisories`; `retry: max 2` on script/runner-system failure (network-dependent: ci_seq reports a fetch outage as NETWORK-FAIL, separate from code failures; a real `error[...]` finding stays FAIL) |
| `check-macos`, `check-windows` | test | manual | cargo check of OS-only paths (`script/windows/bundle.ps1 -CHECK_ONLY`) |
| `test-macos`, `test-warpui-core-macos`, `test-workspace-rest-macos`, `test-shuttle-loom-macos`, `fuzz-smoke-macos`, `bench-macos`, `mutants-macos`, `mutants-warpui-core-macos` | test | manual, allow_failure | the box suite of the same name on the Mac runner (section 2.9, "Every suite on every platform"); command in `script/ci/run_suite.sh macos <suite>` |
| `test-windows`, `test-warpui-core-windows`, `test-workspace-rest-windows`, `test-shuttle-loom-windows`, `fuzz-smoke-windows`, `bench-windows`, `mutants-windows`, `mutants-warpui-core-windows` | test | manual, allow_failure | the same on the Windows runner (`bash.exe` of Git for Windows runs `run_suite.sh windows <suite>`); `fuzz-smoke-windows` fails by design (cargo-fuzz is Unix-only) |
| (build-windows ISCC watchdog) | n/a | n/a | `script/windows/bundle.ps1` runs ISCC under a watchdog (`PHOSPHOR_ISCC_TIMEOUT_MIN`, default 25 min; a good run is ~85 s). On a hang it prints ISCC child processes, free disk and `Output\` file times, kills the process tree and fails the job (#1602) |
| `diag-mac`, `diag-windows` | test | manual | read-only environment reports |
| `build-linux-x86_64`, `build-macos-arm64`, `build-windows-x86_64` | build | manual | AppImage+deb (launches `--version`), ad-hoc signed DMG, unsigned Inno installer; `dist/` artifacts, 3 days |
| `build-{linux,macos,windows}-profiling` | build | manual | unstripped `profiling` profile binary (`docs/profiling.md`) |
| `app-test-linux-wayland`, `app-test-linux-x11`, `app-test-macos`, `app-test-windows` | apptest | manual, allow_failure, played by `ci_seq.py` after each build | real-app matrix on the built artifact, section 2.8 |
| `app-test-{linux-wayland,linux-x11,macos,windows}-reuse` | apptest | manual, allow_failure, played by `ci_seq.py` (fix tier) only | the same job on an EARLIER pipeline's build of an app-identical commit (no `needs`; uses the build cached on the same machine, GitLab download only as a fallback), section 2.9 |
| `release-gate` | test | `v*` tag only | refuses the tag unless the commit has a green release-tier run (section 2.9 "Release gate") |
| `release-*`, `cli-*`, `release-publish` | build/publish | `v*` tag (manual dry run on main) | release pipeline, never run end to end on all platforms; `release-meta` and `release-publish` `needs:` every guard, check, suite and `release-gate` |

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
| `PHOSPHOR_SSH_COUNTED_BOOTSTRAP=0`, `PHOSPHOR_LOCAL_COUNTED_BOOTSTRAP=0` | here-document bootstrap instead of the counted raw read (#1226) |
| `PHOSPHOR_SSH_SPECULATIVE_CONNECT=0`, `PHOSPHOR_SSH_EARLY_BOOTSTRAP=0` | old probe-then-connect / bootstrap-after-connect order (#1266) |
| `PHOSPHOR_REMOTE_NATIVE_COMPLETIONS=0` | Tab in a Phosphorized remote session without the helper goes back to the completer first (ControlMaster side commands) instead of asking the remote shell in-band (#1801) |
| `PHOSPHOR_SSH_EARLY_FLUSH_KNOWN=0` | stash the bootstrap until `check_binary` returns even for a host already known to have the extension (#1172) |
| `WARP_SSH_PROBE_TTL=<seconds>` | lifetime of the cached non-POSIX probe verdict (default 600; 0 disables the cache); the cache file is shared by bash, zsh and fish tabs (#1172) |
| `APPCI_LAB=off\|local\|fixed\|remote` | harness, fake ssh lab (2.15): `local` runs labctl on this machine (default when podman exists, i.e. the box); `fixed` (default on a podman-less machine when `SSHLAB_BOX` and `SSHLAB_TOKEN` are set) attaches to this platform's persistent slot on the box over HTTP; `remote` is the LEGACY forced-command key mode; `off` makes the `lab_*` cases SKIP with the reason. `APPCI_LAB_SLOT` / `APPCI_LAB_SLOTS` choose the slot / the slot table |
| `SSHLAB_BOX`, `SSHLAB_TOKEN` | harness, fixed lab: the box address and the control-API bearer token (protected project variables or machine-local `ci.env`; the token is never printed or written) |
| `APPCI_LAB_BOX`, `APPCI_LAB_KEY` (`SSHLAB_BOX` is the CI variable that feeds the first) | harness, LEGACY remote lab: `user@host` of the box account whose `authorized_keys` carries the forced-command key, and the path of that machine-local private key. Never in the repo |
| `APPCI_LAB_MATRIX=quick\|full\|<spec>` | which arms of a lab case run: `quick` = {bash, zsh, fish, nu} x {LAN, 150 ms} x {nohelper, helper} (nu is the pinned 0.116.1 of `versions.env`); `full` = all 8 shells x {LAN, 150, 300} x {nohelper, helper} + the fault arms (bash/zsh) + the scenario users; or `shells=bash,zsh;lat=lan,150;helper=nohelper;faults=1;scenarios=0` (a typo is an error, not an empty selection) |
| `APPCI_LAB_HELPER_BIN` | path of a built `phosphor-remote` (`cargo build -p remote_server --profile release-remote-helper --no-default-features --features daemon --bin phosphor-remote`, built from the SAME tree as the app under test so both compute the same `phosphor-remote-<version>` name). The `helper` arm pre-installs it as `~/.phosphor/remote-server/phosphor-remote-<version>`; `upload`, `upgrade` and `never` arms hand it to the app as its bundled asset (`PHOSPHOR_REMOTE_HELPER_ASSET_DIR` = `<run>/helper-asset/phosphor-remote-linux-<arch>.gz`). The lab blocks outbound network. Without it every non-`nohelper` arm is UNVERIFIABLE |
| `PHOSPHOR_REMOTE_HELPER_ASSET_DIR` | app: a directory that directly holds `phosphor-remote-<os>-<arch>.gz` and replaces the bundled resources directory as the helper's source (set by the lab arms; usable for manual runs) |
| `PHOSPHOR_REMOTE_HELPER_DIR` | helper: base directory for the daemon sockets and pid files instead of the directory of the executable (unit tests, manual runs) |
| `PHOSPHOR_REMOTE_LOG=error\|warn\|info\|debug\|trace\|off` | helper: level of `{daemon dir}/helper.log` (default info; truncated at 1 MiB at start) |
| `APPCI_LAB_CTL` | override of the labctl.py path (default: the checkout's `script/realapp/sshlab/labctl.py`) |
| `WARP_SSH_VERIFY_MASTER=1` | run the `-O check` after the owned master starts instead of trusting the socket test (#1172) |
| `PHOSPHOR_SSH_CHUNKED_RESPONSES=0` (client), `PHOSPHOR_REMOTE_SERVER_CHUNKED_RESPONSES` (daemon) | whole-frame responses as from an older daemon |
| `PHOSPHOR_SSH_LEGACY_MAX_INFLIGHT`, `PHOSPHOR_SSH_LEGACY_COMMAND_TIMEOUT_SECS`, `PHOSPHOR_SSH_HISTORY_MAX_LINES` | tuning of the legacy (no remote extension) ssh executor and history fetch |
| `PHOSPHOR_ROUTING_V1=1` | pre-#1234 agent/terminal routing rule set, to rule the redesign in or out |
| `PHOSPHOR_SYNC_REFILTER=1` | always filter completion menus inline |
| `PHOSPHOR_NO_ALIAS_LOOKUP_CACHE=1`, `PHOSPHOR_NO_BRANCH_LIST_CACHE=1` | disable the alias-lookup / git-branch-list caches |
| `PHOSPHOR_GIT_REFRESH_ALWAYS=1` | git status refresh after every block |
| `PHOSPHOR_GIT_HEAD_CACHE=0` | shell bootstrap (bash/zsh/fish; read by the shell at every prompt, so it can be exported in a running session): a detached HEAD asks git twice on every prompt instead of once per directory and HEAD commit (#1616) |
| `PHOSPHOR_COPY_INLINE=1` | selection copy on the UI thread instead of off-thread (#1304) |
| `PHOSPHOR_SYNC_CLIPBOARD_WRITE=1` | synchronous clipboard writes instead of the serial owner thread (#1307) |
| `PHOSPHOR_CLIPBOARD_TARGETS_QUERY=0` | no X11 targets-based availability query (#1306) |
| `PHOSPHOR_PASTE_BACKSTOP_MS=<n>` | how long held keys wait for a clipboard read (default 3000; `0` = wait forever, the old behaviour) (#1331) |
| `PHOSPHOR_ATTACHMENT_READ_GIVE_UP_MS` | attachment-read timeout for a submit (#1370) |
| `PHOSPHOR_CONPTY_PROMPT_HOLD=0` | Windows: no ConPTY prompt hold (#1162) |
| `PHOSPHOR_EDITOR_EAGER_LAYOUT=1`, `PHOSPHOR_EDITOR_FULL_LAYOUT=1`, `PHOSPHOR_EDITOR_LAZY_STRICT=1`, `PHOSPHOR_EDITOR_MAX_FILE_MB` | editor: disable lazy layout / windowed layout; panic on geometry of a dehydrated paragraph (always on in tests); file-size guard (`0`/`off` disables) |
| `PHOSPHOR_EDITOR_P5=0` | editor: no parked line chunks (#1340 P5); every dehydrated line is an individual tree item again, the P4 representation |
| `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1` | editor (#1607): OPT-IN heap trim after a file-sized layout (off by default: a trim takes the allocator's locks and its UI cost is unmeasured; `editor_memory_7mb` A/Bs it with `APPCI_EDITOR_MEM_TRIM_AB=1`) |
| `PHOSPHOR_HEAP_TRIM=0` | no heap trim at all (glibc `malloc_trim`, macOS `malloc_zone_pressure_relief`, Windows `HeapCompact` on the process heap; the close-time trims included) |
| `PHOSPHOR_HEAP_TRIM_ON_CLOSE=0` | editor (#1607, #1407): no heap trim when a closed large (>= 1M chars) editor's buffer is really dropped. Default on: the drop event starts one trim on a background worker (a burst of drops is folded into at most one more pass, no timer); the app logs `editor_memory buffer_dropped ... trim=true|coalesced|off|false` and, when the trim ends, `editor_memory heap_trim why=close ms=<ms> freed_estimate=<bytes>` (footprint before minus after; also on the `input_order hop=heap_trim` line) |
| `PHOSPHOR_AGENT_CODE_EDITOR_RECYCLE=0` | agent answers (#1267): keep every fenced code block's editor for the life of its block (default: an AI block more than 6 rich-content entries from the visible ones drops the editors that hold no user state and renders those snippets as plain code blocks; within 2 entries they are rebuilt from the kept text; log `editor_memory agent_code_recycled` / `agent_code_rebuilt` at debug) |
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
| an Enter pressed while a pty paste is being read, swept with that paste into a prompt hold's gate, goes out once, in its place, when the hold is released (#1628: the table's `Enter` / `Text` entries taken out of a hand-off gate were handed to the input, which delivers neither, and two pasted commands merged into one line). In an appci log, `the input was handed an entry it does not deliver: enter` is the old bug | `an_enter_held_behind_a_paste_read_is_released_with_the_prompt_hold_legacy` / `_table`, `an_enter_entry_in_a_prompt_hold_is_released_as_its_byte_in_both_modes`; review: the cap counts the table's swept `Enter` / `Text` / `Held` key as an Enter as `legacy`'s `\r` bytes are, so both modes keep the segment past the cap (`a_swept_enter_keeps_the_prompt_hold_past_its_cap_in_both_modes`), and `Held` keys / sequences (Backspace) swept in are released in place, never lost (`held_keys_behind_a_paste_read_are_released_in_place_in_both_modes`). The shared setup registers the local session, as a shell that shows a prompt has one (without it `legacy` routes typed text to the editor before bootstrap), and checks the editor stays empty: every key reaches the pty, once (`terminal/view_test.rs`) |
| keys typed behind an Enter that ran a shell command reach the pty after that command: the Enter's own replay request (handled in the flush that carries the command to the pty controller, ahead of it) does not settle the gate; the controller's event does, so the keys follow a command parked behind the line editor (the PowerShell pipe path, #1630) | `keys_behind_a_run_enter_follow_its_command_legacy` / `_table`; with no event the deferred settle alone releases them (`keys_behind_a_run_enter_are_released_by_the_deferred_settle_alone_in_both_modes`); a pty that never takes the command: the 300 ms cap hands them back to the editor, nothing written (`keys_behind_a_run_enter_go_back_to_the_editor_at_the_cap_in_both_modes`) (`terminal/view_test.rs`) |
| no size above which input is NOT classified (#1604; the 4096-byte prefilter cap, the parse-cap abandon and the 16 KiB #968 cap sent every over-size shell command to the agent in agent mode): a 5400-byte shell line, a 5000-byte 40-line script and a 20 KB word paste each route exactly as their first line does. Text over `MAX_SYNC_ROUTING_BYTES` (4 KiB) is routed by the same linear rules in a background job (`start_detection_for_unparsed_buffer`; started from the edit path up to `MAX_EAGER_ROUTING_BYTES` 256 KiB, else by the Enter), the Enter waits for that verdict with a compute allowance of 1 ms per 4 KiB (`large_text_compute_allowance`; the backstops grow the same way), the give-up hands the text back (#1577) and never routes it, and for an over-cap buffer keeps the request and its job so the NEXT Enter waits for or submits on the verdict (`abandon_pending_detection_after_give_up`); a buffer under the parse cap never gets a parse-less job (its parse delivers the verdict) and a parse-less snapshot's head is read from a bounded window of the text (`head_word_from_text`); the model classifier sees at most `MAX_MODEL_INPUT_BYTES` (4 KiB) of the text; the history close-match is ruled out by length before the matcher indexes the buffer. These route tests disarm the held Enter's wall-clock give-up nets (`Input::disarm_hold_give_ups_for_test`: an idle compute clock and a new watch epoch) and wait for the verdict event, so a loaded box cannot hand the text back first (r18u19: a 21 KB paste's job ran 122 ms against the 100 ms floor and was rightly handed back, nothing routed); the give-up itself is `test_second_enter_after_a_give_up_...`. The same helper keeps `test_enter_queue_keys_typed_behind_a_verdict_held_enter_keep_order_at_10ms_per_key`'s backstop (250 ms then, 2 s since #1662) from firing mid-typing on Windows' ~15.6 ms timer tick. The completer parse (quadratic) stays capped at `MAX_BACKGROUND_JOB_BUFFER_CHARS`: over it there is no syntax decoration, alias expansion or completer autosuggestion, and that is all | `test_ready_paste_then_immediate_enter_classifies_the_pasted_line` (the three over-size cases), `test_enter_on_oversized_buffer_waits_for_its_background_verdict_and_runs_the_shell_line`, `test_enter_on_20k_char_paste_is_classified_without_a_parse_and_never_swallowed`, `test_paste_of_a_large_buffer_and_the_keystroke_after_it_are_linear_in_its_size`, `test_second_enter_after_a_give_up_on_an_over_cap_shell_line_runs_it_in_the_shell` (`terminal/input_test.rs`); `huge_buffer_is_classified_like_a_short_one_in_the_background`, `verdict_handback_cap_is_the_cpu_cap_and_grows_with_the_text`, `history_close_match_is_ruled_out_by_length_before_the_matcher_is_built`, `a_short_line_without_a_parse_is_classified_from_its_text` (`ai/blocklist/input_model_tests.rs`); `test_oversized_buffer_keeps_detection_pending_and_is_classified_in_the_background`, `test_short_buffer_gets_no_parse_less_detection_job` (`input/decorations_tests.rs`); `long_lines_route_like_their_short_forms` (`routing_corpus_tests.rs`), `test_routing_rules_are_linear_in_the_text` (1.28 MB vs 128 KiB ratio <= 30x, `util_tests.rs`), `test_gate_bounds_what_the_inner_classifier_sees` (`safety_gate_tests.rs`); the DEBUG decision line (which rule or model decided, the verdict, the model's confidence; never the typed text) is pinned by `the_decision_log_line_has_the_source_and_not_the_input` and `a_model_decision_line_carries_the_confidence` (`safety_gate_tests.rs`); release-build bound: `routing/*` benches, gated at the allowance |
| the Enter's shell-or-chat verdict cap `VERDICT_HANDBACK_CPU_CAP` is 1 s (coordinator 2026-11-09; was the 50 ms `ENTER_VERDICT_BUDGET` that SUBMITTED the Enter, with the 100 ms large-paste floor, both deleted; the 1 ms per 4 KiB allowance stays on top) of the classifier's own CPU time, not wall time (#1651), and it is a give-up net only (owner ruling 2026-11-09, a timer never commits): the Enter submits from EVENTS (arrival with a decided type -- locked or autodetection off -- the verdict landing, or a lock / setting change observed by `release_verdict_wait_if_settled`; none of these arms a cap), and the cap and the 2 s backstop only hand the line back (`hand_back_slow_verdict`: toast `VERDICT_GIVE_UP_TOAST`, keys typed behind the Enter dropped with a count #1661, a counted WARN line `Enter verdict too slow ... (total N this run)` so the rate can be measured). `EnterVerdictWait` has two outcomes, `Submit` and `Wait`; `SubmitWithoutVerdict`, `ENTER_VERDICT_BUDGET`, `ENTER_VERDICT_WALL_CEILING` and `enter_verdict_cap_*` are gone (a job waiting off the CPU is bounded by the wall-clock backstop; the young line's wall ceiling went too, `YOUNG_DETECTION_CAP` is a literal 250 ms hand-back cap). Measured: the detection request's `ComputeClock::thread_cpu` times each poll of its jobs on the polling thread's CPU clock (`ai/blocklist/thread_cpu.rs`: `CLOCK_THREAD_CPUTIME_ID` on Linux/macOS, `GetThreadTimes` on Windows; a running poll is read from the main thread through `pthread_getcpuclockid` / `thread_info` / an opened thread handle), so time waiting for a CPU on a loaded machine or queued between polls does not count; with no thread clock a poll is timed on wall time (logged once). The CPU clock does not charge a stage waiting off the CPU (a local directory listing on a slow filesystem, `$PATH` probes, the model still loading, the model's helper threads), so every clock also keeps its stages' wall time (`ComputeClock::stage_wall`) (logged only). A poll is booked from the same probe a reader saw it on, so `spent()` does not go backwards at the poll's end. Both backstops are clamped at `HOLD_BACKSTOP_LIMIT` (9 s, under the Enter-gate leak net, 20 s since the #1661 review), so for a paste over ~32 MiB the leak net never fires first. The wall-clock hold backstops are lost-job nets only (#1662/#1664, busy-machine audit): the verdict hold's is 2 s + the allowance (was 250 ms; >= cap + 150 ms at every size) and the young line's `YOUNG_DETECTION_BACKSTOP` is 2 s + the allowance (was 500 ms), so on a loaded machine the CPU cap, not a wall timer, decides; their expiry, like every give-up, only hands the text back (#1577), never routes. The Tab-completion hold's 500 ms backstop is unchanged (owned by the Tab work, H6/H7); the compute-cap timers (editor Enter and young line) re-arm no sooner than 5 ms. The CPU-clock tests are pure and need no app; tests that hand a wait an idle clock or call `disarm_hold_give_ups_for_test` are unchanged; appci `ssh_nested_typeahead`'s give-up note names the new cap (text only, no expectation changed) | `busy_work_counts_on_the_thread_clock`, `sleeping_does_not_count_on_the_thread_clock`, `probe_reads_another_threads_busy_work`, `probe_of_a_sleeping_thread_barely_moves` (`ai/blocklist/thread_cpu_tests.rs`); `thread_cpu_clock_counts_busy_work_not_time_off_cpu`, `thread_cpu_clock_does_not_count_time_between_polls`, `thread_cpu_clock_reads_a_running_poll_from_another_thread`, `wall_clock_measure_counts_the_stage_wall_time`, `a_stage_waiting_off_the_cpu_never_reaches_the_cpu_cap`, `a_cpu_starved_stage_keeps_its_cpu_cap`, `spent_rises_while_the_poll_is_still_spinning` (`ai/blocklist/compute_clock_tests.rs`); `probe_rises_while_its_thread_is_still_spinning` (`thread_cpu_tests.rs`); `enter_verdict_wait_is_submit_or_wait_never_a_clock_submit`, `verdict_handback_cap_is_the_cpu_cap_and_grows_with_the_text` (`ai/blocklist/input_model_tests.rs`); `test_slow_verdict_cap_hands_the_line_back_and_drops_keys_behind_the_enter` (cap = hand-back: text stays, keys behind dropped with the count toast, nothing submitted; a clock wake with compute 5x the cap submits nothing), `test_decided_input_type_submits_at_once_with_no_cap_armed`, `test_lock_change_while_the_enter_waits_submits_it_in_order` (`terminal/input_test.rs`; the verdict_wait/gate is the same for Legacy and Table, the young line's two paths are `view_test::a_decided_input_type_submits_the_young_enter_at_once_without_a_timer`); `test_stall_between_enter_and_verdict_still_routes_by_the_verdict` (stall = 3 caps), `test_verdict_give_up_cap_counts_only_classifier_compute`, `test_verdict_hold_backstop_stays_clearly_above_the_compute_cap` (backstop >= cap + 150 ms up to 10 MB; under the leak net at any size), `test_verdict_cpu_cap_ignores_a_job_waiting_off_the_cpu` (`terminal/input_test.rs`); `a_young_line_is_submitted_only_on_an_actual_verdict` (young cap = 5 budgets, now 250 ms; `terminal/model/typeahead_handoff_tests.rs`) |
| in agent mode a lone word nothing on the session answers to (not installed, no shell syntax, not a curated command word or a path) goes to the agent, whether the session knows it is absent or cannot say, at the keystroke and at Enter, in a fresh conversation and after an agent turn, in every shell family; installed, the same word runs in the shell, an explicit path runs, and clear shell structure around an unknown head still runs (#1290, owner decision #1560); outside a conversation the classifier decides | `lone_unknown_word_in_a_conversation_goes_to_the_agent` and the #1290 corpus cases (`XTOOLS`, `frobxone`; `routing_corpus.rs`), run by `corpus_against_the_routing_procedure` (`routing_corpus_tests.rs`) |
| in an agent conversation a command-headed line with an English contraction (`it's`, `don't`, `I'm`; `'` or `’`) is a sentence and goes to the agent (#1624: `what is in the file that I just mentioned T169_6rjko, it's short` with macOS `/usr/bin/what` ran in the shell, because the `'` made the quoting unsafe and the evidence / sentence tests read that as "no verdict" / "a quoted argument"); shell syntax still wins (#1560: `what -s file.c` runs), one-off keyword and free-text heads keep their route (`echo it's done`), an apostrophe inside double quotes is literal (`mv "the team's notes.md" and the backup` runs), a typographic `’` counts only with another prose word (`ls Josh’s` runs), and the plain terminal is unchanged (installed head runs) | the #1624 corpus cases (`routing_corpus.rs`), run by `corpus_against_the_routing_procedure` and, with their `v1_xfail`, `corpus_against_the_current_procedure` (`routing_corpus_tests.rs`) |
| an agent-run command is never handed to the user, and no key is written to a pager, on a timer (owner decision 2026-11-09, #1811/#1812/#1813; refs #1796 #1734 #1735 #963 #1365 #1431): the prompt detector asks the tty-read probe at every observation of a prompt-shaped line (explicit marker or bare `?`/`:`) and a proven `Reading` hands over at that observation with no quiet floor; `NotReading` shows nothing and arms no timer (the 1 Hz poll re-asks); `Unknown` shows the 'may be waiting for input -- Take over' notice once (3 s for a marker, `INTERACTIVE_PROMPT_NOTICE_EXPLICIT`; 90 s for a bare `?`/`:`) and never hands over. A full-screen program unchanged for `STALL_NOTICE_AFTER` (5 s) gets the same notice once per screen, whether or not it is a pager; the `q` write, the 8 s hand-over, the pager-name logic and the 'pager closed' note to the model are gone. Agent commands run with `LESS=-FRX`, `SYSTEMD_LESS=FRXMK`, `PERLDOC_PAGER`, `MYSQL_PAGER` on top of the existing pager variables, restored by the bash/zsh precmd backstops (Nu stays unwrapped: a block would swallow the agent's `cd`). Expectation changes: `prompt_detector_fires_after_the_prompt_is_quiet_for_its_duration_not_after_n_polls` -> `a_proven_tty_read_hands_over_at_once_for_either_shape`; `an_explicit_marker_is_still_handed_over_without_a_tty_read_probe` -> `an_explicit_marker_with_a_foreground_not_reading_the_tty_shows_nothing`; `a_new_candidate_after_awaiting_a_tty_read_arms_the_quiet_timer_again` -> `a_new_candidate_restarts_the_episode_and_its_notice_timer`; `a_real_prompt_is_handed_over_within_the_window_plus_three_seconds` -> `a_real_prompt_is_handed_over_at_the_first_observation`; `fullscreen_detector_closes_a_known_pager_once_and_never_before_the_threshold` and `..._next_wake_follows_a_closed_pager_to_the_handover` -> `fullscreen_detector_gives_one_notice_at_the_threshold_and_never_before`; the pager-name tests and `pager_closed_note_is_appended_to_finished_output_only` are deleted with the code; appci `agent_lrc_no_false_takeover` part 2 accepts the notice log line where the OS cannot tell (selftest `agent_lrc_takeover_log_parse`) | `a_proven_tty_read_hands_over_at_once_for_either_shape`, `an_explicit_marker_with_an_unknown_tty_read_gets_a_notice_not_a_hand_over`, `an_explicit_marker_with_a_foreground_not_reading_the_tty_shows_nothing`, `a_trailing_colon_with_an_unknown_tty_read_gets_a_notice_not_a_hand_over`, `fullscreen_detector_is_driven_by_output_events_and_one_quiet_timer` (`terminal/model/block_test.rs`); `a_stalled_fullscreen_program_gets_the_notice_and_the_agent_keeps_the_command` (Legacy and Table, nothing written to the pty; `terminal/view_test.rs`); `bootstrap_backstops_restore_every_pager_variable`, `extra_pager_variables_are_set_for_every_wrapped_shell` (`agent_shell_wrap_tests.rs`) |
| in the agent view Ctrl-C goes to the pty whenever the pty owns the keys (a long-running command, an agent command before its preexec, or a hand-off that routes keys to the pty such as a submitted line zsh holds at `quote>`); at an idle prompt the first Ctrl-C still arms "^C again to exit" (#1625) | `agent_view_ctrl_c_reaches_a_shell_reading_a_submitted_line`, `agent_view_ctrl_c_at_an_idle_prompt_still_arms_the_exit` (`terminal/view_test.rs`, Legacy and Table), `agent_view_ctrl_c_reaches_an_agent_command_in_flight` (#850 window) |
| every input buffer has a cap, a logged overflow and a toast: pane `InputQueue` (`MAX_QUEUED_ENTRIES` 64 Ki entries, newest dropped, ^C never), `ModelInbox` (`MAX_INBOX_COMMANDS` 256 Ki, data dropped, `UserWrote` flags merged), pty `write_list` (`MAX_QUEUED_WRITES` 64 Ki, signals never dropped), tmux control-mode pending buffer (`MAX_TMUX_PENDING_BYTES` 64 KiB, whole writes). Each cap is far above 180 s of 60 Hz key repeat (asserted) | `a_full_queue_*`, `the_overflow_episode_resets_*` (`input_queue/tests.rs`), `a_full_inbox_*` (`model/key_facts_tests.rs`), `a_full_write_list_*` (`local_tty/event_loop_tests.rs`), `tmux_pending_input_is_bounded_*` (`writeable_pty/pty_controller_lifecycle_tests.rs`) |
| tracked modifiers are cleared on window focus loss and regain (winit backends; macOS derives modifiers from each native event and tracks none) | `focus_modifiers_tests.rs` (`crates/warpui/src/windowing/winit/event_loop/`) |
| control-sequence bytes are not logged unless `WARP_INPUT_ORDER_LOG_TEXT=1` | `raw_bytes_are_described_by_count_not_content` (`crates/warpui_core/src/input_order.rs`) |

### 2.7 Dev test tooling (merged in round 18)

All dev-only (nothing ships). Exact commands, flags and caveats: **`docs/dev-tooling.md`**. Summary:

| tool | where | what it covers | when it runs |
|---|---|---|---|
| proptest | `app/src/terminal/input_queue/tests/prop_model.rs` (`ops.rs` shared with fuzz) | random op sequences against a model of the pane input queue | every test run (in `test-linux`) |
| unit (hand-off pipeline, #1420/#1422/#1423/#1439) | `input_queue/tests.rs` (`retag_gate_with_a_multi_byte_...`, `editor_keys_pass_only_setup_gates_...`), `typeahead_handoff_tests.rs` (`a_key_reaching_the_hand_off_under_a_live_*_hold_is_held_not_written`, `the_editor_takeover_window_*`), `editor/view/mod_test.rs` (`test_queue_editor_keys_behind_a_setup_gate_*`) | a `\r\n`-led or multi-byte Enter write keeps its tail when retagged; a key under a live queued hold is held (never written unheld); editor keys behind only a subshell/PowerShell setup gate apply at once (Enter/Tab still wait; CliSubmit and hand-off gates hold); the takeover window ends on focus/click, with a 10 s logged give-up | every test run |
| unit (remote young Enter, measurement only, #1615) | `model/typeahead_handoff_tests.rs` (`an_enter_delivered_past_an_unsampled_group_is_followed_through_the_next_hooks`) | an Enter delivered because a group the platform does not sample (a warpified ssh's remote shell, an inner shell) is in front while a line is unanswered is followed through the next Precmd and Preexec and settled by them or by the editor's next command; an empty line, a sampled shell and an unsampled platform are not followed; no decision changes. In an appci log: `hop=unsampled_enter_delivered` at the Enter, `unsampled_enter_then_precmd delivered_to_precmd_ms=`, then exactly one `unsampled_enter_settled` with `outcome=ran_as_shell_command` (after that Precmd the shell itself started the delivered line as a command, unclassified: the remote #933 race when `delivered_to_precmd_ms` is within `rtt_p90_ms`), `outcome=app_command` (after that Precmd the next Preexec was a command the app wrote), `outcome=no_command` (a second Precmd with no command between: the running command took the Enter), `outcome=editor_command` (the editor wrote the next command first) or `outcome=new_session` (a new shell took the pty over, e.g. the remote shell of the ssh the Enter was typed into) | every test run |
| unit (busy queue = deferred operation, #1416-#1419/#1426-#1428) | `terminal/view_test.rs` (`editor_keys_taken_out_of_a_prompt_hold_*`, `a_prompt_hold_cap_that_fires_while_the_queue_is_busy_*`, `a_held_key_that_finds_the_queue_busy_*`, `the_cli_submit_gate_survives_a_busy_queue`, `a_pane_drain_wake_that_finds_the_queue_busy_is_kept`), `input_queue/pane.rs` (`deferred_operations_keep_their_order_*`) | an operation that finds the pane queue busy is recorded (`PaneQueue::defer`) and applied in order when free: no dropped / as-typed key, no consumed one-shot cap, editor keys taken from a gate are applied. The proptest model has no busy op: the queue core has no busy concept (the mailbox lives in `PaneQueue`/the view), so the model cannot express it | every test run |
| unit (destination table, step 1: facts plumbing, #1424) | `model/key_facts_tests.rs` (`the_gen_is_stored_with_the_bits_*`, `a_reader_takes_the_pair_only_when_*`, `a_pair_caught_between_the_two_stores_*`, `every_publication_bumps_the_gen_*`, `visible_text_is_sticky_*`, `unterminated_since_is_the_gen_*`), `input_queue/tests.rs` (`an_entry_keeps_the_stamp_it_was_offered_with_*`, `a_follow_up_takes_the_stamp_*`, `a_guard_stamps_arrivals_*`), `input_queue/facts.rs` (mode parse, `GateMask`, stamp order), `terminal/view_test.rs` (`route_facts_build_takes_no_blocking_model_lock_*`, `the_panes_stamp_follows_the_models_publication_gen`, `route_facts_derive_the_phases_*`) | the model publishes a gen in BOTH cells (`KeyFactsCell` word + `KeyModelSnapshot`); a reader retries until they agree and a torn pair reports the older gen; a main-thread refresh never changes the gen; the sticky visible-text / unterminated-since bits; a key keeps the `Stamp` it got at the sink through the mailbox and follow-ups; `RouteFacts::build` takes no blocking model lock inside or outside a key. NOTHING DECIDES on any of it yet (`PHOSPHOR_QUEUE_DESTINATION` defaults to `legacy`; `shadow`/`table` are read by nothing) | every test run |
| unit + proptest (destination table, step 2: the pure table, #1424) | `input_queue/table_tests.rs` (one test per row: `h1_*` .. `h10_*`, `d1_d2_*`, `d2b_*`, `d2c_*`, `d1_prime_and_d11_*`, `d3_d12_*`, `d4_*`, `d5_*`, `d6_*`, `d7_d8_*`, `d9_*`, `d10_*`, `d13_*`, `p1a_od5_*` .. `p2_and_d6_*`, `synced_*`, `viewer_*`, `cli_submit_*`, `the_four_predicates_differ_*`, `the_gate_by_signal_plan_matrix`, `h8_every_gate_holds_every_class_*`, `b12_*`; properties `decide_is_total_and_deterministic`, `every_drop_has_a_reason_*`, `nothing_reaches_the_pty_while_the_agent_owns_it_*`, `a_hidden_box_never_drops_text_the_pty_does_not_take`, `signals_never_wait_behind_a_gate`, `pty_writes_consult_the_oracle_*`, `origin_exemptions_hold`, `the_matrix_is_what_holds_content`), `model/typeahead_handoff_tests.rs` (`peeking_the_route_mutates_nothing`, `a_peek_predicts_the_commit`) | every row of design revisions 2+3 (incl. OD-1 signal classes, OD-5 held signal, OD-2/3/4; OD-7's H4 row is gone with the ssh prompt latch, #1611/#1612/#1621) as a table case; the 14 gates x 4 signals plan; the H8 gate x class matrix with #1423's editor exemption and `only_ssh_gates_open`; the four legacy pty predicates and where they differ (B11/B13); totality, determinism, drop-has-reason, never-pty-when-the-agent-owns-it (except aborts), the password rule, hidden box never drops for `!pty_typed`; step 5 adds the enumerated totality test `enumerated_facts_make_decide_total_and_reach_every_row_and_outcome` (every combination of the decisive facts x every class and origin: total, every row a plan can carry reached, every outcome has a row) and a `decide` op in the shared `Op` model (`input_queue/tests/ops.rs`: the table agrees with the queue about what holds, for the proptest AND the cargo-fuzz target). `decide` is called since steps 3-6 (rows below) | every test run |
| view_test (ssh bootstrap timeout, #1494) | `view_test.rs` (`ssh_bootstrap_failure_hands_held_typeahead_back_without_stty_sane` Legacy+Table, `ssh_bootstrap_failure_with_a_dead_shell_discards_the_held_typeahead`) | when the ssh bootstrap timer fires with typing held and the remote shell alive, NOTHING is written (no `stty sane`, no keys): the held text is handed back, nothing runs, with a toast naming `stty sane`; a dead shell discards the held text with a log. The `PHOSPHOR_SSH_BOOTSTRAP_GIVEUP_DELIVERS` switch that wrote them is deleted (#1815), with its three tests `ssh_bootstrap_failure_under_the_kill_switch_delivers_held_typeahead_after_stty_sane`, `ssh_bootstrap_timer_second_expiry_under_the_kill_switch_still_delivers_after_stty_sane` and `the_ssh_bootstrap_failure_delivery_is_marked_final` | every test run |
| view_test (bootstrap timer vs progressing session, #1566) | `view_test.rs` (`ssh_bootstrap_timer_racing_the_bootstrap_precmd_does_not_fail_the_session` Legacy+Table, `ssh_bootstrap_timer_second_expiry_after_the_grace_still_fails_the_session`) | the bootstrap give-up timer defers once (3 s, logged) when the pending session already showed its bootstrap `Precmd`; the held line then reaches the pty exactly once at the first remote prompt, editor empty, no `stty sane`; a second expiry still fails the session and hands the held text back (nothing is written). In an appci log, `Bootstrapping failed` shortly after a remote `precmd_seen` and before `Received Bootstrapped hook` is the old bug | every test run |
| unit + view_test (destination table, step 3: the shadow comparison, #1424) | `input_queue/shadow_tests.rs` (`agreement_reports_nothing`, `a_mismatch_reports_row_both_classes_and_flags`, `suspend_and_quit_differences_are_tagged_od1`, `a_signal_held_in_the_ssh_setup_window_is_tagged_od5`, `a_silent_drop_of_hidden_input_is_tagged_b3`, `the_log_line_has_no_content_of_the_input`, `every_outcome_has_a_class`, `the_leaf_is_the_first_one_noted_*`, `the_test_override_nests_and_restores`), `terminal/view_test.rs` (`shadow_mode_changes_no_destination_*`, `shadow_reports_a_legacy_table_disagreement_*`) | `PHOSPHOR_QUEUE_DESTINATION=shadow` compares the table with the destination class the legacy chain reached at its leaf (typed text, control keys, signals, escape sequences, the replayed Enter / held key) and logs `kh_dest_disagree row=.. legacy=.. table=.. flags=..` (`input_order`, debug; counts and flags, never text) with `expected=OD1/OD5/OD7/B3/B7/B13` tags for known causes; agreement logs nothing; shadow changes no behaviour. Tests force the mode on their own thread with `set_destination_mode_for_test` (never `std::env::set_var`) | every test run |
| view_test (destination table, step 4: delivery through the table, the #1415 fix, #1424) | `terminal/view_test.rs` (`legacy_mode_delivers_replayed_text_through_the_chain`, `shadow_mode_delivers_replayed_text_through_the_chain`, `table_mode_delivers_replayed_text_without_the_legacy_chain`, `table_mode_text_replayed_after_an_ssh_recognised_at_preexec_only_is_held_not_run_locally` (the mixed-mode case: lazy arming of the connect gate by the delivery), `table_mode_text_replayed_into_a_new_ssh_connect_is_held_then_delivered_once` (also asserts no key-path model lock), `table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover`, `a_held_signal_is_discarded_when_the_connect_ends_without_a_remote_shell`, `table_mode_facts_never_wait_on_the_handoff_lock`), `input_queue/facts.rs` (`mode_parses_like_the_pane_key_switch_and_defaults_to_table`) | `PHOSPHOR_QUEUE_DESTINATION=table` (forced per test thread by `set_destination_mode_for_test`): replayed Text / Enter and every ssh-held input (`EntryKind::Held`, five classes) are decided ONCE by `table::decide` and executed with the leaf writers; the legacy chain is not called (`dest_table::legacy_chain_deliveries` is the discriminator: it moves in legacy and shadow, not in table); an ssh recognised at preexec only is held for the connect, never run locally; a `HELD_SIGNAL` entry is delivered to the pty once after the keys before it and discarded (not shown, not run) when the connect ends without a remote shell; the H4 drop is executed; facts never wait for the hand-off lock. The default mode is `table` (owner, 2026-11-02); `PHOSPHOR_QUEUE_DESTINATION=legacy` is the kill switch | every test run |
| unit + view_test (destination table, steps 5 and 6: arrival of terminal keys, the H8 editor column, the origin rows, paste routing, #1424) | `input_queue/tests.rs` (`the_h8_matrix_agrees_with_the_editors_hold_decision_for_every_gate_set`: all 16384 gate sets x editor entry queued or not), `input_queue/table_tests.rs` (the H8 matrix has a class column: an editor key that submits or completes is never exempt), `editor/view/paste_hold_tests.rs` (`the_matrix_class_of_an_editor_key_is_the_legacy_submit_or_complete_split`), `input_queue/shadow_tests.rs` (`the_editor_matrix_comparison_reports_only_a_difference`, `editor_text_past_a_gate_is_the_tagged_difference_b14`), `terminal/view_test.rs` (`table_mode_arrivals_reach_the_same_destinations_as_legacy` runs five scenarios in `legacy` and `table` and compares every pty write, the editor text and the PowerShell gate's bytes; `only_table_mode_decides_live_keys_with_the_table`; the #1179 agent-driven typing and the live Enter (OD-2) in each mode; `table_mode_typing_after_the_user_takes_over_an_agent_command_reaches_the_pty_once`; `table_mode_text_the_pty_does_not_take_is_kept_in_the_editor_even_when_the_box_is_hidden` (OD-4); OD-1: `table_mode_ctrl_z_cancels_a_paste_being_read_and_reaches_the_pty_once` against `legacy_mode_ctrl_z_waits_behind_a_paste_being_read` and `queue_signal_classification_follows_the_mode`; OD-5: `table_mode_a_ctrl_c_in_the_ssh_setup_window_is_held_and_reaches_the_remote_shell_once` against `legacy_mode_a_ctrl_c_in_the_ssh_setup_window_is_dropped_with_the_notice`; H8 strict FIFO: `table_mode_text_typed_behind_a_gate_waits_as_a_neutral_entry_and_is_routed_when_it_opens`, `table_mode_a_control_key_behind_a_gate_is_delivered_at_the_gates_event`, `table_mode_a_key_that_waited_in_the_mailbox_keeps_the_stamp_it_had_when_typed`; the origin rows: `table_mode_the_synced_origin_row_refuses_a_pane_with_no_running_command`, `table_mode_synced_input_is_written_or_held_as_legacy_does`, `table_mode_keys_typed_during_a_paced_cli_agent_submit_follow_the_submit_in_order`, `{legacy,table}_mode_a_cli_agent_submit_during_a_powershell_setup_is_held_and_delivered_once` (#1601: an open PowerShell setup holds the submit as legacy does; released once, in order, at the prompt after Bootstrapped), then #1601b: the O-cli-submit row states what the legacy writer does with the bytes -- a PACED submit (its `CliSubmit` gate is open) is held by nothing but the PowerShell setup, an UNPACED one waits behind the Enter-verdict / tab-completion gates (H5), the subshell setup and bootstrap write (H7), any other open gate but an ssh-only set (H8) and queued entries (H8-queue), and both consult the typeahead hand-off (`CONSULT_ORACLE`; the shadow counts a hand-off hold as `Pty`): `table_tests.rs` (`a_paced_cli_submit_is_exempt_*`, `an_unpaced_cli_submit_*`, `origin_exemptions_hold`), `table_enum_tests.rs` and `fuzz/fuzz_targets/route_decide.rs` state the same rule, and `view_test.rs` runs the submit paced and unpaced behind an Enter-verdict gate, a subshell setup and a paste being read: `a_cli_submit_behind_a_gate_is_held_unpaced_and_written_at_once_paced_in_both_modes` (delivered bytes and order in `legacy` and `table`) and `shadow_mode_reports_zero_untagged_disagreements_for_a_cli_submit_behind_a_gate` (single and chained, two orders), `table_mode_the_agents_write_is_refused_during_a_setup_and_goes_out_otherwise` (row D13 at the agent's writer); paste: `table_mode_paste_routing_agrees_with_the_legacy_routing`, `table_mode_paste_routing_sends_a_paste_to_the_agents_input_while_it_drives_the_command`; the equivalence tests `shadow_mode_reports_no_untagged_disagreement_over_the_arrival_scenarios` (nine scenarios: running command, a block started but not yet long running, idle prompt, ssh connect and interrupt, PowerShell setup, the other origins' writers, the agent's writes, the navigation keys, text behind a gate) and `shadow_mode_agrees_on_typing_during_an_agent_driven_command` drive the real view chain and fail on any `kh_dest_disagree` line that is not tagged `expected=`) | in `table` mode a live key (text, keydown, control sequence) is decided by the table with the stamp taken first and the queue's own facts; the idle path is `Pty` executed at once; a held key is a destination-neutral queue entry routed again when its gate opens; ^Z / ^\ jump the queue; a signal in the ssh setup window is held and delivered remotely; the writers of other origins and the paste route consult the table; `shadow` proves the same destinations with zero untagged disagreements. Forced per thread with `set_destination_mode_for_test`, never `std::env::set_var` | every test run |
| unit + view_test (destination table, step 7: the edge-driven ssh phase, #1348, #1349, #1490, #1424) | `input_queue/ssh_phase_tests.rs` (every edge: submit, connect start, ssh owning the tty (`TtyOwned`, #1611/#1612/#1621: the pre-ownership hold ends there and never on output), observed facts, block completed, bootstrap failed, swallow give-up, hold started/ended, released by an event or a named give-up), `terminal/view_test.rs` (`shadow_mode_the_ssh_phase_machine_agrees_with_the_derivation_through_a_connect`, `table_mode_decides_on_the_machines_phase_not_the_legacy_fields`, `table_mode_the_remote_prompt_event_hands_over_with_no_timer_and_no_give_up` (asserts `ssh_give_ups == 0`), `table_mode_a_gate_past_its_ceiling_is_released_by_a_logged_give_up`, `table_mode_the_give_up_ends_a_window_nothing_is_held_behind`, `table_mode_a_secret_read_at_ownership_discards_the_held_keys_and_the_next_key_reaches_it`, `a_non_ssh_command_starting_releases_the_window_by_event`, `legacy_mode_does_not_feed_the_ssh_phase_machine`), `model/typeahead_handoff_tests.rs` (the ConPTY enter window: `keys_typed_between_the_enter_and_the_next_ready_signal_are_held_and_released_in_order`, `a_command_that_becomes_interactive_gets_the_typeahead_in_order`, `a_running_programs_own_enter_never_opens_the_window`, `a_ready_signal_before_the_precmd_does_not_end_the_window`, `the_enter_window_gives_up_for_its_own_keys_and_the_hold_stays_on` (#1573), the kill switch and platform tests), `input_queue/tests.rs` (`an_owner_change_edge_clears_the_line_pin_at_the_next_lock`) | the ssh phase is fed by the events that are its edges and `table` mode (the default) carries it in the facts with no ssh refresher on the key path; `shadow` reports `kh_ssh_phase_disagree`; the hand-over runs inside the remote prompt's own handler and every timer left is a counted, logged give-up (`kh_ssh_give_up`); on ConPTY keys typed from an Enter until the next line-editor-ready signal are held and released in order (typeahead for an interactive program is released to it by the latch event); `PHOSPHOR_CONPTY_ENTER_HOLD=0` is the kill switch. The focus epoch and the pin clear follow focus and owner-change edges; `EditorFocusPending` and `PtyRouteSettling` (no opener) are deleted | every test run |
| unit + appci (Windows secrets and hand-over on ConPTY, #1882 #1883 #1885 #1891; audit `docs/design/windows-input-parity-audit.md` findings 1, 2, 4, 10) | `model/typeahead_handoff_tests.rs` (`a_hold_in_a_shell_that_has_signalled_is_kept_never_handed_to_the_editor` (#1883: the prompt-hold give-up hands keys to the editor only for a shell that never signalled; a silent shell that signalled before is reading a secret), `a_powershell_marker_stamped_with_the_answered_prompt_proves_nothing` (#1891: a `LineEditorReading` marker whose seq is not past the prompt the last counted Enter answered is ignored), `an_announced_tty_read_keeps_the_pty_the_owner_until_it_ends` (#1882)), `model/terminal_model_test.rs` (`tty_read_markers_are_events_and_the_secret_fact_without_termios`: the `TtyRead`/`TtyReadEnd` markers become a `TtyRead` event and the model's secret-read probe, ended by `TtyReadEnd` or the next Precmd/Preexec), `terminal/view/prompt_notice_tests.rs` (#1882, the pure take-over NOTICE watch the Windows view runs on output: explicit prompt noticed once after its quiet window, never a hand-over; changed prompt restarts the clock; stale timer ignored; new block starts over; bare `:` waits longer), `input_queue/ssh_phase_tests.rs` (`an_owned_window_passes_keys_like_a_plain_terminal_secret_read_or_not`, #1885), appci `ps_read_host_secure` (PowerShell pane: `Read-Host -AsSecureString` typed after the marker and typed at once behind the Enter; length and character sum recorded, the secret in no app-log line / PSReadLine history / received command), `gitbash_precmd_secret` (Git Bash pane: a precmd function runs `read -s`; the PIN typed in two halves with a pause past the 1.5 s prompt-hold give-up reaches the read whole), `agent_interactive_password` (now also the PowerShell pane with `Read-Host -AsSecureString`, handed over by the marker) and `agent_lrc_no_false_takeover` (now both Windows panes: the printed prompt gets the notice, no hand-over on silence); selftest `windows_handover_secret_cases_pure_parts`; `script/check_pwsh_parse` parses the bootstrap wrappers | ConPTY has no termios, so a command that reads the tty has to announce itself: the PowerShell bootstrap shadows `Read-Host` / `Get-Credential` with functions that write a `TtyRead` OSC (`secret` for `-AsSecureString`/`-MaskInput`/Get-Credential) and a `TtyReadEnd`; the app hands a waited agent command to the user at once (`hop=tty_read_marker`, then `hop=handover ... reason=blocked_on_input`), `tty_reading_secret()` is true for a secret read (so the ssh secret pass-through and the subshell setup's secret cancel see it), and the hand-off is never re-baselined into the editor meanwhile. `PHOSPHOR_TTY_READ_MARKERS=0` leaves the cmdlets alone. `$Host.UI.ReadLineAsSecureString` called directly cannot be wrapped (a .NET method), and a REMOTE login script's `read -s` over ssh announces nothing: that is the plain-terminal pass-through (#1885, documented, tested at the phase machine). The notice watch (`win_prompt_notice_observe`) is the #1796 path for prompts nothing announces: notice only, never a hand-over, one give-up-net timer | every test run (Rust, selftest); the appci cases are in the fix tier (`cases.PROVING`) |
| unit (busy queue, operations that RETURN bytes, #1446) | `terminal/view_test.rs` (`a_powershell_take_under_a_busy_queue_*`, `a_deferred_enter_gate_take_under_a_busy_queue_*`, `an_ssh_gate_drop_under_a_busy_queue_is_not_lost`) | the take (deferred Enter gate keys, PowerShell held bytes, ssh gate drop, void Enter text) and the caller's continuation are one `Deferred` variant: nothing returns empty when busy, the ssh gate is not left set | every test run |
| insta | `.snap` files under `app/src/terminal/snapshots/`, `crates/warpui_core/src/snapshots/` | per-shell bootstrap lines, pane-key routing tables, input_order trace shapes | every test run; review diffs with `cargo insta review` on the box |
| expectrl | `app/tests/real_shell_markers.rs` | real bash through a pty: marker order (Preexec, Precmd, LineEditorReading with the same seq) | every test run of `-p warp` (skips without bash) |
| shuttle / loom | `warpui_core::sync_shim`; the job's filter is `model_tests::model_` (a model test lives in a module named `model_tests` or `*_model_tests` and is named `model_*`; `model_tests::` alone also matched the ordinary `file_search_model_tests::test_*`, which crashes under loom outside a model, r18u22); `model_tests::model_*` in `clipboard_model_tests.rs`, `key_facts_model_tests.rs`, `typeahead_handoff_model_tests.rs` | thread interleavings of the clipboard write queue, KeyFacts, PaneHandoff, the reader cap; the two-cell publication gen (`model_the_reader_retry_never_combines_gens`, `model_the_gen_is_monotonic_across_loads`, `model_a_stamp_taken_after_seeing_the_prompt_is_not_before_it`: a consistent read never mixes publications, a stamp is never from the future, a stamp taken after the prompt was seen is never classed before it) | `test-shuttle-loom` job (release tier) |
| shuttle / loom (input pipeline, #1512 #1531 #1424) | `app/src/terminal/model/pipeline_model_tests.rs` (wired from `key_facts.rs`; the module name ends in `model_tests` and every test is named `model_*`, so the job's `model_tests::model_` filter selects it): `model_pane_queue_drain_never_loses_reorders_or_duplicates_a_key`, `model_ssh_hand_over_keeps_every_held_key_in_order_and_delivers_it_once`, `model_handoff_holds_start_and_end_once_and_never_strand_a_key`, `model_a_refresh_racing_a_publication_never_moves_the_gen`, `model_the_decision_is_never_older_than_the_stamp_it_follows` | `PaneQueue`/`InputQueue` are `Rc` (main thread only), so the two queue models are faithful minimal copies of the protocol in which every point another operation can run at (inside a drain, between the drain's end and the mailbox flush, between the ssh gate closing and its drain finishing) is a lock release; each names the code it mirrors (`pane.rs` `draining`/`defer`/`try_lock_ordered`/`begin_deferred_flush`, `ssh_queue.rs` hold/hand-over/drop). They assert the owner's invariants: every key is delivered exactly once, in arrival order, to one destination (or, in the ssh model, discarded because the connect was dropped while it was held), nothing is left waiting, nothing is delivered behind an open gate. The hand-off model runs the REAL `TypeaheadHandoff` behind `PaneHandoff` with a model thread, an editor submit and the key path: a `Held` answer always has a live hold, a signal is never held, every hold the key path saw start has an end recorded for the view. The gen models: a main-thread refresh never moves the gen (r18u22 loom: with a plain `store` as the publication, loom's partial modification order let a refresh CAS of the pre-publication word win and the cell ended on gen 0; `store_with_gen` is a `swap` so every write to the word is an RMW), and a decision read is never older than the stamp taken before it. Loom bodies are sized down with `cfg!(loom)` | `test-shuttle-loom` job (release tier) |
| unit (enumerated totality, #1531 item 3) | `input_queue/table_enum_tests.rs` (wired from `input_queue/facts.rs`): `decide_is_total_and_consistent_over_the_enumerated_facts`, `every_row_of_the_table_is_reachable`, `row_names_are_unique_and_the_list_is_complete` | ~860 000 samples: the booleans the rows branch on x ssh x subshell phases x every class x origins, every gate kind alone and next to the ssh gate, and a fixed-seed sweep over all of `RouteFacts`/`Intent`/pin. `decide` never panics and is deterministic; every drop names a row that can give it; a refused pty write drops only a typed write; signals never wait; the oracle is consulted exactly for entries not released from a hand-off gate; the editor never reaches the pty; a PACED CLI-agent submit is held only by the PowerShell setup, an unpaced one by the gates a key waits behind (#1601b); every `Row` is decided by some sample (a row nothing reaches is dead) and the `ALL_ROWS` list is the whole enum (an exhaustive match). If a new `Row` is added the match fails to compile until it is listed | every test run |
| criterion | `app/benches/key_path_bench.rs`, `ansi_bench.rs`, `routing_bench.rs` (`--features dev-support`) | per-key cost (idle, behind a gate), routing, hand-off, 1 MB ANSI parse, input-type routing of 1 MB / 10 MB text (#1604; limits at the Enter's compute allowance) | `bench` job (release tier) |
| cargo-fuzz | `fuzz/` (own workspace; targets `ansi_parser`, `hook_parsers`, `input_queue`, `route_decide`, `key_facts_cell`, `typeahead_handoff`) | crashes on hostile terminal output, hook frames, queue op sequences; `route_decide`: `table::decide` over arbitrary `RouteFacts`/`Intent`/pin never panics and every plan keeps the table's properties (determinism, drop reasons name their rows, signals never wait, oracle consulted, editor never reaches the pty, CLI submit never held); `key_facts_cell`: `KeyFactsCell`/`KeyModelCell` operation sequences against a reference model, the reader reports a consistent pair exactly when the gens agree and a torn pair one publication behind; `typeahead_handoff`: arbitrary hook/key/editor event sequences on the hand-off never panic, a `Held` answer has a live hold, a signal is never held except behind a deferred Enter's `HandoffReport` gate, where it waits in typed order and is reported for that gate (r18u23: the model's unconditional "never held" was wrong; seed `regression_r18u23_signal_behind_deferred_enter`, unit test `a_signal_behind_a_deferred_enter_waits_in_order_r18u23_fuzz`), a hold that starts is reported and a hold that ends leaves an end for its gate; `hook_parsers` r18u27: `line_to_string` looked up row `0 - 1` for a leading wide-char spacer in row 0 (seed `regression_r18u27_row0_wide_spacer`, unit tests `row_zero_leading_wide_spacer_does_not_underflow_r18u27` and `spacer_in_column_zero_does_not_underflow_r18u27`); `markdown_parser` r18u27: a delimiter row's last cell took the NEXT line's `|` as its closing pipe, so a cut after the row streamed text where the full parse had a table (seed `regression_r18u27_tab_delimiter_table`, unit test `a_delimiter_row_never_reads_its_closing_pipe_from_the_next_line_r18u27`) | `fuzz-smoke` job (release tier) |
| cargo-mutants | `.cargo/mutants.toml`; jobs `mutants` (pane_key, key_facts, the #1424 destination table: input_queue gates/table/facts/mod/pane/shadow and view/dest_table.rs) and `mutants-warpui-core` (clipboard) | lines the tests do not actually check | release tier only (`mutants-warpui-core` timeout 4h: 250 mutants at ~43 s; 90m timed out in r18u23); widen with `MUTANTS_FILES` / shard with `MUTANTS_SHARD` |

### 2.8 Real-app tests in CI (`app-test-*`, stage `apptest`)

The release-test matrix of 3.5 is automated against the BUILT artifact, per platform, in BOTH session modes (terminal and
agent). Code: `script/realapp/appci/` (Python: `run.py` driver, `cases.py`, `backend.py`, `profile.py`, `app.py`, `sshd.py`,
`report.py`, `winapi.py`, `winmap.py`, `fakesshd_win.py`), entry points `script/realapp/linux/wayland_session.sh`, `mac/ci/app_test.sh`, `windows/ci/app_test.ps1`.
**One harness, three platforms (#1488): Linux, macOS and Windows run the same `appci/run.py` and the same case list.** Windows is no longer a separate PowerShell
case list: `windows/ci/app_test.ps1` is a thin VM wrapper (lock, side-by-side install, md5-verified backup, `restore.ps1`, report regeneration) that runs
`& %LOCALAPPDATA%\phosphor-ci\python312\python.exe script\realapp\appci\run.py --backend windows --platform windows --bin <test build exe> ...`.
That Python (3.12.10 + paramiko + Pillow, installed by hand, NOT on PATH, always called by full path) is part of the VM setup; the harness uses ctypes for everything else.
By hand on the VM: `$py = "$env:LOCALAPPDATA\phosphor-ci\python312\python.exe"; & $py script\realapp\appci\run.py --run C:\tmp\run --bin <...>\phosphor-oss.exe --backend windows --platform windows --cases fast_typing`
(take WINPUSH.lock and a backup first, 3.4; `app_test.ps1` does all of that).

| | Linux (wayland / x11) | macOS | Windows |
|---|---|---|---|
| backend class (`backend.py`) | `WaylandBackend` / `X11Backend` | `MacBackend` | `WindowsBackend` |
| keys | `wtype` / `xdotool` | `htype2` (CGEvent, real key codes) | `SendInput`: real VK + scancode events (static US-layout table for printable ASCII), `KEYEVENTF_UNICODE` for everything else |
| focus guard | X11: before every injection, the X input focus must be a Phosphor window, else the newest visible one is focused and verified (r18u28) | frontmost app NAME and PID == the started PID, refuses Cmd-Q/W/Tab/Space/H/M | `GetForegroundWindow` + `GetWindowThreadProcessId` == the started PID before every burst (and every key of a long one); refuses Win-key / Alt-F4 / Alt-Tab chords |
| screenshots | `grim` / `import` / `ffmpeg` | `screencapture` | `PIL.ImageGrab` |
| clipboard | `wl-copy` / `xsel` | `pbcopy` | user32 `CF_UNICODETEXT` |
| injector record (#1482, #1509) | Wayland/X11: one record per `wtype`/`xdotool` call (`backend.parse_injector`) | `htype2` per-call K lines | per call: events asked / accepted by `SendInput`, wall time of each posted Return; `<run>/inject.log` |
| reference terminal | foot / xterm (own rc `.refrc` and own `refctl/` log dir, never the app pane's `ctl/rec.log`; keys are refused unless the reference window has the focus: sway `get_tree` focused pid/app_id on Wayland, X input focus on the PID's windows on X11, checked before every timed burst) | Terminal.app (`.command`) | the same shell in a plain console (conhost: `powershell.exe -NoLogo` or Git Bash), `APPCI_WIN_REF=wt` for Windows Terminal; guard accepts only that window |
| key -> paint (`latency_paint`) | skip (no read-back) | `kl3` ScreenCaptureKit | skip (no read-back) |
| app launch | `dbus-run-session` | `open -n` bundle, private HOME | the side-by-side test build started by the harness; data under the known folders `%LOCALAPPDATA%\phosphor`, `%APPDATA%\phosphor` (wiped per pass; backed up and restored by `app_test.ps1`) |
| local ssh server | system `sshd` / `fakesshd.py` | system `sshd` | `fakesshd_win.py` (paramiko) on 127.0.0.1; remote shell = Git Bash in a real ConPTY; no OpenSSH Server service |
| ssh client | `ssh` + `-F` wrapper | `ssh` + `-F` wrapper | PowerShell pane: built-in `System32\OpenSSH\ssh.exe` through `<home>\bin\ssh.cmd`; Git Bash pane: Git for Windows' own `usr\bin\ssh.exe` through `<home>\bin\ssh` (sh) (Win32-OpenSSH cannot do the ControlMaster Phosphor's bash ssh wrapper adds: `getsockname failed: Not a socket`, r18cj); both add `-F <home>/.ssh/config`; the remote home gets the same config + wrapper for nested ssh; the PowerShell pane has no rc, so the harness types a `function ssh { & '<System32 ssh.exe>' -F '<home>/.ssh/config' @args }` into it once per launch (r18co: plain `ssh fake` could not resolve the host). The Git Bash setup probe also starts a ControlMaster with the wrapper's options; when Git's ssh.exe cannot (r18co: `Failed to connect to new control master`) the ssh cases still RUN and their failure text carries the probe result as a `PROBE:` lead, because the app's bash wrapper does the same on every ssh, so a real Git Bash user gets that failure (candidate app finding, not a skip) |

**Windows triage rules (r18cj, the first unified Windows run, #1488).** A failure is an APP bug only with proof: a control in the same session (conhost running the same shell, same injector, same cadence) that keeps what Phosphor lost, or the app's own log showing the key arrived and was then lost inside the app. Concretely the harness now: (1) judges `fast_typing`, `ctrl_c_d` and `cadence` lines on the EXACT command the shell recorded and prints, per corrupted line, what changed (one character lost / extra / reordered), whether the app's `typed_characters` log shows the line arriving in order, and what the reference did (`enter_loss_control` counts exactly-received lines too); (2) compares `md5sum` output by digest (Git for Windows prints `<hash> *-`) after checking the pasted lines reached the shell byte-exact; (3) after a failing case probes the pane with a marker command and, when it is wedged, reports `pane_wedged_after_<case>` and relaunches the app, so one wedged pane no longer fails every later case; (4) saves the failing case's app-log lines (`logs/<label>/case_<case>.log`, the app log rotates) plus `ssh/sshd.log` and `fakesshd.stderr`, and `inject.log` is a CI artifact; (5) writes `results.json` / `summary.md` without a BOM and passes `--backend` on the report-only call so the wrapper's `restore` row lands in them.


**Windows pane shells.** Each session mode runs once per pane shell (`APPCI_WIN_SHELLS`, default `powershell,gitbash`). The `powershell` pass is the app default; the app starts PowerShell
with `-NoProfile`, so there is no rc hook: no `prompt.log` / `rec.log`, cases use effect files written in PowerShell syntax (`Add-Content`) and fixed settles instead of prompt waits,
and Ctrl-D halves are not run. The `gitbash` pass sets `session.startup_shell_override` to Git for Windows' `bash.exe` with `HOME` = the scratch home, so `~/.bashrc` logs exactly as on Linux
and the POSIX cases run unchanged; its result rows carry the mode label `terminal@gitbash` / `agent@gitbash` (and its latency ids and previous-build comparisons are separate). **`winmap.py`
classifies every case per pane** (`BOTH`, `GITBASH_ONLY`, `NOT_WINDOWS` with the reason); a case a pane cannot run is a SKIP row with that reason, never an omission, and
an unclassified case is reported as such. `PS_ONLY` cases (#1882: `ps_read_host_secure`, `agent_interactive_password`) run in the PowerShell pane and skip with their reason in the Git Bash pane. After `mode_proof` (whose pre-key screenshot must come first) a key-delivery proof types a marker command and confirms it arrived (VK events,
then Unicode packets; `key_delivery` row); if neither lands the pass stops with `keys not delivered: <reason>` and the session / foreground diagnostics.
Each also has a `-reuse` twin that tests an earlier app-identical build (2.9). Heavy and GUI-driving: all four jobs are **manual, allow_failure**, need the platform's build job (`needs:` + artifacts) and
`script/ci/ci_seq.py` plays them after each successful build, so every numbered build runs them. The play-time variable
`APPCI_TIER` selects the size: `smoke` (fix tier) or `full`/empty/`release` (candidate = `full`, release = `release`, which the release gate requires; everything below). Smoke = the cases
`mode_proof`, `cadence`, `fast_typing`, `paste_instant_enter`, `tab_enter`, `ctrl_c_d`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_nested_typeahead`, `latency_enter`, `latency_key_log`, `md_viewer` in both
modes (`SMOKE` in `script/realapp/appci/cases.py`, applied by `run.py`); on Linux only the `wayland` run (the `xwayland` pass and
`app-test-linux-x11` exit at once); the SAME list on Windows, where a case that cannot run in a pane shell is a SKIP with its reason
(winmap.py); it includes `ssh_connect_typing`, `ssh_enter_then_immediate_command` and `ssh_nested_typeahead` on every platform, the hand-over cases behind
the ConPTY release blocker #1349. Target <= 10 min per platform; UNMEASURED so far (the full matrix has not been timed
either).

**Destination mode in the real app (#1532, #1528).** The app runs in the table's default mode (`PHOSPHOR_QUEUE_DESTINATION` unset = `table`)
unless the run sets the variable; the harness passes it through (Linux and Windows inherit the environment, macOS via `open --env`,
`app.passthrough_env`; `APPCI_APP_ENV="K=V,K2=V2"` sets more app variables for one run). Every `mode_proof` row records
`metrics.queue_destination` (`table` | `legacy` | `shadow`, parsed like `facts.rs`), and `run.py` prints the mode at the start. To exercise
the kill switch in the real app, play the fix tier a second time with `PHOSPHOR_QUEUE_DESTINATION=legacy` (set it as a play-time variable or
export it): with `APPCI_TIER=smoke` the case list narrows to `winmap.KILL_SWITCH_SMOKE` (`mode_proof`, `paste_instant_enter`, `tab_enter`,
`ctrl_c_d`, `ssh_connect_typing`, `ssh_enter_then_immediate_command`, `routing_table`: the cases whose routing the table decides), about a
third of the smoke time, so both modes are covered without doubling the tier. It is NOT part of the default fix tier (that would double
its runtime); run it per candidate. `PHOSPHOR_QUEUE_DESTINATION=shadow` runs the whole smoke list and adds a `dest_disagree` row per
pass that FAILS on any `kh_dest_disagree` line in the app log that carries no `expected=` tag (`metrics.dest_disagree`, `dest_disagree_untagged`).
Unit side of the same checks: `shadow::take_untagged` (a `cfg(test)` counter), `shadow_mode_reports_zero_untagged_disagreements_over_chained_key_sequences`
(`terminal/view_test.rs`) and the exhaustive `exhaustive_enumeration_*` tests (`input_queue/table_enum_tests.rs`, #1530: all gate masks, the phase
product and every pair of fact dimensions). `python3 script/realapp/appci/selftest.py` checks the metric and the list.

| job | runner | session | input / screenshots |
|---|---|---|---|
| `app-test-linux-wayland` | box (`little-thing`, resource_group `little-thing-gui`) | headless sway, `WLR_BACKENDS=headless`, gles2 on `/dev/dri/renderD128` (pixman fallback), 1600x1000; full matrix as a native Wayland client, then a reduced set under XWayland (`--mode xwayland`, app forced to X11) | `wtype` (`-d N` = 10-25 ms/key, text + Enter in one call), `grim`, `wl-copy`/`wl-paste` |

**Wayland pointer / OCR cases (owner 2026-11-05 NOREF; real pointer since #1789).** Headless sway has no input device (`WLR_LIBINPUT_NO_DEVICES=1`), so its seat advertised no pointer capability and no client ever created a `wl_pointer`: `swaymsg seat - cursor ...` moved sway's own cursor and nothing else (the same trap as the lost first key, #1414). `script/realapp/appci/wlptr.py` fixes that with no install: plain Python speaking `zwlr_virtual_pointer_v1` (sway/wlroots) over the Wayland socket. `wayland_session.sh` starts ONE holder (`wlptr.py serve --ctl $RT/wlptr.sock`, recorded in `pids`, `APPCI_WL_PTR_HOLD=0` skips it) before the app, so the app binds `wl_pointer` once, and exports `APPCI_WL_PTR`; `cases_ui.Mouse` sends `abs X Y W H` / `btn CODE STATE` lines to it (falls back to the sway seat commands when there is no holder). `cases_ui.wl_pointer_real(ctx)` is true only when the holder answers a ping. Gate rules (`cases_ui.wl_check`): with a live holder a pointer-ONLY claim (`what == "pointer"`: the clipboard kept the sentinel, nothing pasted, no SGR event reached the program) is a plain check, i.e. a FAIL of the app; claims that depend on OCR (`"OCR"`, `"pointer/OCR"`) stay NOREF ("Wayland harness cannot ... read this text by OCR reliably; ...") until the app hook below exists; without a holder everything stays NOREF as before. Positions without OCR: `alt_screen_mouse_reporting` on Wayland takes the window rectangle from sway (`sway_window_rect`, exact) and sweeps six clicks down its left side (only a click inside the program's block is reported to it); `ui_palette` treats a log that HAS frame lines but none rendering `CommandPaletteView` as a real failure (the log is OCR-free evidence); `ui_mouse_select_copy` and `ui_right_click_paste` first ask `cases_ui.hook_find`, which reads `appci_find token=T x0= y0= x1= y1=` lines from the app log (window-content pixels, converted with the sway window origin) for tokens the harness lists in `$PHOSPHOR_APPCI_FIND_FILE` (`app.py` sets it; `register_find` appends). **The app hook exists** (`crates/warpui_core/src/appci_find.rs`, `Line::find_hits`, `app/src/terminal/grid_renderer.rs`): when `PHOSPHOR_APPCI_FIND_FILE` is set, the app reads the token list (one per line; re-read when the file changes, at most every 200 ms, which is why `register_find` sleeps 0.35 s) and, whenever it DRAWS text containing a listed token (terminal grid rows in both renderers; every laid-out text line: the editor input, menus, labels), logs at info on target `warpui::appci_find` the line `appci_find token=T x0= y0= x1= y1=` in device pixels of the window (`app.py` adds `warpui::appci_find=info` to RUST_LOG). Cost with the variable unset: one cached `OnceLock` read per layout and per grid render, no string work, `Line::find_hits` stays an empty Vec. Privacy: only the listed tokens and their boxes are ever logged, never other screen text. A token is logged once per position change (an older position coming back is re-logged at most once a second). Known limits: a token wrapped across two lines is not reported; a line clipped at its start is not reported; text laid out before the token was registered carries no hit (register first, then draw). `ui_mouse_select_copy` and `ui_right_click_paste` (the probe, and the context menu's `Paste` entry) ask the hook first on X11 (window origin from `xdotool`) and Wayland (from sway) with a log `since` offset, and fall back to the old OCR method with a case note ("app find hook gave no position ...") when the line is missing. Self-test `app_find_hook_harness_side_1789`; Rust unit tests in `crates/warpui_core/src/appci_find_test.rs` (token matching, char offsets, glyph extents, once-per-position, line format). The Wayland job needs nothing installed: python3 only. Self-tests `wlptr_virtual_pointer_wire_and_holder` (wire layout against a fake compositor), `wayland_pointer_gate_follows_the_holder_1789`, `wayland_pointer_ocr_cases_noref_x11_judged`.
| `app-test-linux-x11` | box | private Xvfb (software GL) | `xdotool`, `xsel`, `import`/`ffmpeg` |

**X11 injector (r18u28).** The release-candidate X11 pass (3d6d6ed21) failed rollover, keydown_count, intl_wide_type/edit/program, latency_first_key_after_idle, ui_startup_time, cb_settings_migration, remote_session_ext and editor_memory_7mb where Wayland/Mac/Windows passed; the app's own `input_order keydown` log showed it received exactly what X delivered in every one, so the causes were the harness. `X11Backend` now (1) checks the X input focus before EVERY injection and focuses the newest visible Phosphor window when it is elsewhere (Xvfb has no window manager: after an in-case relaunch nothing focused the new window and every key went nowhere), refusing to type when there is no Phosphor window; (2) types rollover the way a physical US keyboard can (`backend.x11_roll_argv`): a Shift-typed character, or a repeat of the key still held, is a plain tap after the held key is released (the old chain held `_` over `8` and got `*`, and re-pressed a key that was still down, which X does not deliver); (3) binds every non-ASCII character of a typed text to a spare keycode of its own once per run with `xmodmap` and waits `PREMAP_SETTLE_S` before typing (`xdotool type` rebinds one scratch keycode per character and an X client re-reads the keymap only after the next rebinding: `日本語のテスト` arrived as `本本テト`); `APPCI_X11_PREMAP=0` turns this off; (4) maps lower-case key names to X keysyms (`escape` -> `Escape`; xdotool ignored `escape` with exit 0). `keydown_audit` now parses the space bar's `key=" "` (a missed space made `echo o...` look like a doubled `o`), `latency_first_key_after_idle` waits (<= 2 s) for both keys' frames before reading a sample (the read ran before the second frame was logged: 0 of 30 pairs), and on X11 only OCR also reads a 2x grey upscale of the shot (ImageMagick `convert`) and adds its words (`cases.ocr_upscaled`, `APPCI_OCR_UPSCALE=0` off). Self-tests: `x11_injector_rollover_is_physical_and_keys_are_named`, `x11_injector_premaps_wide_characters`, `x11_injector_focuses_phosphor_before_every_call`, `idle_first_key_waits_for_both_frames`, `x11_ocr_upscaled_words_scale_back`, `keydown_audit_counts_plain_keys_and_flags_doubles`.
| `app-test-macos` | `Mac` (user `gitrunner`) | must be the logged-in Aqua session | `cliclick` + `osascript`, `screencapture`, `pbcopy`; frontmost-app guard before every burst; `caffeinate` |
| `app-test-windows` | `windows` (console desktop, the runner is NOT a session-0 service) | the interactive desktop | `SendInput` (foreground-PID-guarded), `PIL.ImageGrab`, via the shared Python harness; side-by-side install; backup + md5-verified restore of app data, uninstall key, PATH, shortcuts, `phosphor://` handler; `WINPUSH.lock`; exact-PID stop |

Session handling (Linux): own short `XDG_RUNTIME_DIR` (falls back to `/tmp/pa-*` when the job dir is too long for the 108-byte socket
limit), private `HOME` per mode with an owner-like scratch config and **no credentials**, app under its own `dbus-run-session`, a local ssh
server on 127.0.0.1 (system `sshd` as the CI user with a `ForceCommand` into a scratch remote home; else `fakesshd.py` when paramiko
exists; else the ssh cases SKIP). OpenSSH reads `~/.ssh/config` from the passwd home, not `$HOME` (#1481), so `profile.py` writes `ssh`/`scp`/`sftp` wrappers into `<home>/bin` (first on PATH in the rc files) that exec the real binary (absolute path resolved at setup) with `-F $HOME/.ssh/config`; after the profile is built `run.py` probes `ssh fake true` in that environment and, if it fails or hangs, every ssh case SKIPs with ssh's stderr as the reason. The ssh cases type `exit` only after the token reached the REMOTE file (`Ctx.leave_remote`; it also fails fast when the count of shells under the app, from `ps`, drops, since the app logs nothing when the local shell exits); otherwise they record the miss, reset the pane and stop their trials, so a missed connect can never exit the local shell. In a pane with no local prompt hook (Windows PowerShell) `leave_remote` settles 2 s after `exit` and then `wait_ssh_gone` waits (<= 8 s) for the app's own `hop=ssh_pty_owner ssh=false`, because fakesshd-win can hold the channel open ~2.5 s after the remote shell's output ends and keys typed by the next trial reach the dying ssh.exe (r18w5, agent-mode PowerShell `ssh_connect_typing`; the app log shows plain pass-through, no hold, nothing dropped). Every process is recorded in `pids` and torn down by exact PID / process group, never by name.
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

Run lifecycle (r18d8, Linux CI; `job 17686762542` was killed at the 90 min timeout with nothing but the runner log):
- **Crash-safe results.** `run.py` regenerates `results.json`, `summary.md` and `junit.xml` from `results.jsonl` after EVERY case (meta
  `partial=1` until the final write, which is unchanged and decides the exit code). A killed job leaves every finished row.
- **Artifacts.** `.appci_linux` uploads with `when: always`, so a timed-out or cancelled job still ships results, `shots/`, `logs/` and
  `results.jsonl`; finished cases' screenshots and logs are already under `run/` when the job dies.
- **Timeout.** `.appci_linux` has `timeout: 4h` (GitLab's is static per job). The full tier of `app-test-linux-wayland` runs two modes
  (terminal ~75 min alone, `soak` ~70 min per mode at `APPCI_SOAK_MIN=60`) plus the xwayland pass; the fix tier (smoke) ends by itself well
  under 90 min and is not lengthened. The per-case limit `APPCI_CASE_TIMEOUT` (1500 s) is unchanged EXCEPT `soak`, which gets
  `APPCI_SOAK_MIN * 60 + 600` s (it can never finish a 60 min soak inside 1500 s: it hit CaseTimeout in r18d8). `quit_24_tabs` + `soak`
  are the dominant cases; nothing else looked mis-sized, and no coverage was shrunk.
- **SIGTERM/INT/HUP.** `wayland_session.sh` traps them, signals run.py's process group (run.py is started under `setsid`; its pgid is
  recorded in `pids` as `runpy`), waits up to 30 s for it to flush and stop the app, then exits (the EXIT trap tears down every recorded
  PID). `run.py`'s own handler writes the partial report first, then unwinds (`app.stop`, `sshd.stop`).
- **Stale processes.** At job start `wayland_session.sh` runs `teardown.sh <this out dir>` and `teardown.sh --stale`. The latter reaps
  every process of this user whose `/proc/PID/environ` carries `APPCI_SESSION=<session pid>:<job id>:<mode>` (exported by the session
  script; all its sway/app/foot/wtype-holder/sshd children inherit it) when that session pid is dead. Exact PIDs, group-leader
  by group, never by name; a live concurrent session is untouched; what it reaped is logged as `[session] stale: ...`.
- **Self-deadline (r18e0, all platforms).** `appci/deadline.py`: `run.py` reads the job's budget (`APPCI_DEADLINE_S`, else GitLab's
  `CI_JOB_TIMEOUT`, seconds) and the job start (`CI_JOB_STARTED_AT`, else its own start) and logs `[deadline] ...` at start. Before EACH case
  (and each pass, so a late pass obeys it too) it requires `time left >= max(10 min, that case's own timeout) + 5 min` (report, teardown,
  upload). Otherwise the case is not started: it gets a `status=skip` row `not run: job time budget (N min left, M min needed ...)`, the
  decision is logged once as `[deadline] ... STOP starting cases`, and a pass with no budget left does not even launch the app. The run
  then finishes normally (report, artifacts). No budget variable (a local run): never stops itself. Caveat: if the runner enforces a
  `maximum_timeout` below the job's `timeout:`, `CI_JOB_TIMEOUT` may not reflect it; export `APPCI_DEADLINE_S` then.
  `app-test-macos` and `app-test-windows` (and their `-reuse` jobs, which extend them) have `timeout: 4h` like Linux; the fix tier still
  ends early on its own.
- **Per-row evidence in the job log.** Under a row, `    evidence:` lines: `mode_proof` prints `log_view_mode`, `ocr_agent_markers`,
  `ocr_terminal_hint`, `queue_destination`; any case whose app log shows Enters prints `enter_route: N Enter(s) ...: Shell=a AI=b [sequence]`
  (from `hop=enter_begin input_type=`, this case's log window only); every latency row prints `host load after the case: loadavg1=.. cpus=..`
  (also `metrics.host_load`). The `summary.md` table prints the first detail in full (rest capped at 160) and launch-error lines are not cut.
- **Cleanup evidence (all platforms).** Per pass `run.py` prints `teardown[<label>]:` lines: app pid stopped/gone, test sshd, scratch dirs
  (`home`, `ctl`, `rctl`, `refctl`, `rhome`) removed, and a leftover-process audit (processes whose command line names the run dir). macOS
  adds `teardown: Terminal.app reference pid N quit ...` (refterm_mac) and `cleanup:` lines from `mac/ci/app_test.sh` (run.py/app/caffeinate/
  DMG, then the audit); `teardown.sh` (after_script) prints per-pid outcomes, survivors and other processes naming the out dir;
  Windows `after_job.ps1` prints recorded pids stopped/gone/not-ours, leftover processes and removed scratch dirs.
- **Windows app runs in a Job Object** (#1642). `WindowsApp.start` creates the app suspended (`CREATE_SUSPENDED`), assigns it to a job with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`winapi.job_*`, ctypes only), then resumes it, so conhost and any shell it spawns are members from
  birth. `stop()` records the members as `jobmember <pid> <creation FILETIME>` lines in `run\pids`, runs `taskkill /T`, then closes the job
  handle (which kills every remaining member); if the harness dies the OS closes it. `app.stdout` is opened non-inheritable and `Popen` hands the
  child only its std handles. `after_job.ps1` is the fallback: a recorded `jobmember` is killed only when its creation time still matches (never
  by name, never an unrecorded PID). If the job cannot be made or assigned the app starts plain, as before. Selftest
  `windows_app_runs_under_a_kill_on_close_job` drives the ordering with a mocked kernel32/Popen on Linux.
- **Job log.** Each row's FIRST failing message (or a `noref` row's `NO-REFERENCE` reason) is printed in full; the rest of the detail
  stays capped at 200 characters; the Ollama endpoint is redacted as before.

Cases (each in both modes unless noted): `mode_proof` (screenshot + OCR when tesseract exists), `cadence` (real key spacing measured at
the pty), `fast_typing` (10-25 ms/key, Enter in the same call, gaps 0 and 250 ms), `rollover` (next key down before the previous is
up), `expert_fuzz` (seeded typos fixed with BackSpace / Left+insert / Ctrl-W), `paste_instant_enter`, `arrows_behind_paste` (#1389),
`big_paste` (`APPCI_BIG_MB`, default 1, byte-exact), `tab_enter` (gaps 0/50/150 ms, no stray uncompleted file), `ctrl_c_d` (Ctrl-C
delivery latency, Ctrl-D in `cat` and in a nested bash), `read_x`, `subshells` (bash twice incl. typing during setup, zsh, fish, nu where
installed), `subshell_sh_probe` (#1710: dash and busybox ash, plus `su -s /bin/sh $APPCI_SU_SH_USER` when that variable names a passwordless-su user; Ctrl-I at the banner writes the marker-paced sh probe, the line typed right behind it runs in the inner shell < 2 s after its Enter, then 4 exact lines, `exit`, the pane's bash; the app log (read from the trial's own offset) must show `subshell_setup_unsupported` and no marker give-up fallback; dash is cooked and editor-less, so the judge writes only at an idle prompt: the case pauses after the first line until that event (a continuous typist starves the judge by design) and reads the parent's comm from `/proc` first because busybox `ps` has no `-p`; the profile adds `^dash$` and `^busybox ash$` to `warpify.subshells.added_subshell_commands`, merged with the owner's list), the subshell READINESS lab (`appci/cases_subshell_ready.py`, #1679 step 3/4; every case Phosphorizes a real shell with Ctrl-I and judges from the effect files and the app log's hops `subshell_setup_armed` / `subshell_ready_judged` / `subshell_ready_line_end` / `subshell_setup_ready_write` / `subshell_written_judged` / `subshell_setup_cancelled`; a missing facility SKIPS with its reason, never fails): `subshell_ready_rc_questions` (two `read -p` answers are ReArms, the one write comes after both, the rc recorded `me ops`), `subshell_ready_vim_typeahead` (`vim f⏎` typed into the sleeping rc, AltScreen / Unknown waits, vim's file is exactly `hello`, written after `:wq`), `subshell_ready_unsupported_tcsh` (`why=Unsupported`, nothing written into tcsh, the line runs in tcsh), `subshell_ready_secret_after_prompt` (a one-shot `read -s` in PROMPT_COMMAND before the second prompt: `why=Secret dest=Editor`, the password reaches the read, the setup is written again, the handed-back line runs only on the user's Enter), `subshell_ready_relay_shape` (passwordless `sudo -s`: written behind the relay, the line runs as uid 0), `subshell_ready_exec_fish_rc` (`exec fish` in the rc: `decision=Write` on `comm="fish"`, the line runs in fish), `subshell_ready_docker_t_without_i` (a local alpine image only, never pulled: RelayCooked, nothing written, the line runs once in the parent), `subshell_ready_zsh_newuser_install` (an empty HOME: the menu footer is an InputPrompt, no `~/.zshrc` appears, written after `q`), `subshell_ready_rc_clear` (an rc that runs `clear`: written on the OS proof, first inner line < 2 s); the profile adds `^bash --rcfile .*`, `^tcsh -f$` and the docker command to the subshell regexes), `alt_screen` (less, vim), `ssh_connect_typing` (0/100/200/300 ms after Enter, `APPCI_SSH_TRIALS` per gap (default 3 = 12 trials) or `run.py --ssh-trials N` / `APPCI_SSH_TOTAL_TRIALS` for N trials in TOTAL spread over the four gaps, exact remote
command, nothing local; #1127: the result carries `metrics.loss` and a note with the observed loss rate and its one-sided 95 % Clopper-Pearson upper bound -- 0 lost of 40 only bounds the rate to 7.2 %, excluding the original 1/40 (2.5 %) needs 119 clean trials), `ssh_enter_then_immediate_command` (#1415: `ssh fake` Enter and the next command with no pause), `ssh_nested_typeahead` (#1421: a nested ssh and the line behind it, typed during the first connect; runs once, remotely), `ssh_failing_not_local`, `idle_cpu`, `latency_enter` (vs a reference terminal in the same session),
`latency_key_log` (key to first frame from the app's own `input_order` log; also, informational only, #1176: `key_to_pty_write_ms` (keydown to the event loop's `pty_write` line, target <= 1 ms median), `typing_ui_items` (`hop=ui_item` >= 4 ms) and `typing_lock_holds` (model lock held >= 2 ms) over the same typing), `keydown_count` (#1159, full tier only, both modes, every platform incl. both Windows panes: `ctx.count(20)` lines of `echo <12 lowercase letters> >> kd.txt` at 12-30 ms/key, alternating plain typing and `be.roll` (overlapping key-down/key-up on macOS htype2 `r:`; Wayland/X11/Windows use their own `roll`); asserts the app log's plain `input_order keydown` count (ctrl/alt/cmd presses and bare modifiers excluded) EQUALS the keys sent (characters + Enter), no same-key repeat < 2 ms apart (the #1159 signature was an extra keydown 0.4 ms after the real one), and the received text exact (effect file, plus rec.log where the pane has one)), `latency_first_key_after_idle` (#1160, full tier only, not in SMOKE: the FIRST key after 1, 2 and 5 s of no input, `ctx.count_lat(30)` (>= 30) kept samples per gap plus discarded warm-up. macOS: kl3 key->paint for Phosphor AND Terminal.app, A/B in blocks of 10 samples with the first side alternating per gap (not per sample: a focus switch between the two windows is itself an activation), verdict per gap on the overhead against `APPCI_IDLE_KEY_BUDGET_MS` (default 5 ms, owner 2026-11-03) with the SE-aware PASS/FAIL/INCONCLUSIVE of `lat.py`; Linux and the Windows Git Bash pane: key->frame from the app log with the SAME session's next key (0.3 s later, not idle) as the control, so the verdict is the idle penalty; Windows PowerShell pane skips), `approval_card_pause` (agent, needs a provider),
`tabs_two_windows` (`APPCI_TABS`, default 12 streaming tabs, a second window, typing and paste). Full tier, module `cases_caseagent.py` (#1463; Linux/macOS backends; the AI ones skip without `ci.env`, the OCR sub-checks are notes without tesseract): `agent_stream_cancel` (agent; a long reply streams, text typed at 10-25 ms/key lands in the input with no new request and nothing in rec.log, Ctrl-C stops the stream: the screen stops changing; request/stream-open times are metrics), `agent_approval` (agent; nothing runs before approval, the first Enter on the empty input only focuses the card, the second approves: the command is in rec.log and its effect file exactly once; a follow-up question in the same conversation opens a new model request; a rejected command never reaches rec.log), `agent_lrc_no_false_takeover` (agent, #1796, needs the CI model; Linux/macOS, not Windows: the take-over detector is unix-only): part 1 asks the agent to run and wait for `printf 'Waiting:'; sleep 20; echo <tok> >> lrc_wait.txt` and fails when the app log says `handing the command to the user` (a slow, silent command after a trailing `:` handed over as "Command needs your input") or the effect file never gets the token; the detector's `leaving the command with the agent` line is a note (it is only logged when the model's tool call waited for completion; the app logs `agent_command_wait awaited=<bool>` for every agent command, read by the case); part 2 runs `printf 'Continue? [y/N] '; read ans; echo <tok> >> lrc_yn.txt` and expects the hand-over on the explicit marker, then answers `y` + Enter, which must reach the command (its effect file: a check); a model that answered the prompt itself is a note, a command still stuck at the prompt after 60 s with no hand-over is a note when the log says `awaited=false` (the detector was never armed) and fails otherwise; any `Failed to enter inline agent view for LRC handoff` line (the false ERROR of a hand-back from the full-screen agent view) fails; self-test `agent_lrc_takeover_log_parse` pins the log needles; unit tests: `a_silent_command_after_a_trailing_colon_is_not_handed_to_the_user`, `a_trailing_colon_is_handed_over_once_the_foreground_starts_reading_the_tty`, `an_explicit_marker_is_still_handed_over_without_a_tty_read_probe` and the updated `prompt_detector_fires_after_the_prompt_is_quiet_for_its_duration_not_after_n_polls` in `terminal/model/block_test.rs` (a bare trailing `?`/`:` needs the OS tty-read probe as well as 15 s of quiet; a probe that cannot tell (`Unknown`: no probe on macOS, an unreadable `/proc/<pid>/syscall`) NEVER hands over on a timer (owner rule 2026-11-08: timers never decide routing): after `INTERACTIVE_PROMPT_QUIET_TRAILING_UNKNOWN_READ` = 90 s only `PromptDetection::MayBeWaiting` is returned, once, and the view shows the non-modal notice "This command may be waiting for input — Take over" (click = the existing take-over, `TerminalAction::ToggleLongRunningCommandControl`); a later `Reading` probe still hands over; `NotReading` shows nothing. The 30-minute `MAX_UNTIL_COMPLETION_DURATION` backstop only resolves the agent's tool call with its preempted snapshot (it never moved terminal control) and now also emits `ShellCommandExecutorEvent::BackstopReached`, which shows the same notice; explicit markers and the password shape never ask it), `a_trailing_colon_with_an_unknown_tty_read_gets_a_notice_not_a_hand_over`, `a_new_candidate_after_awaiting_a_tty_read_arms_the_quiet_timer_again`, `a_task_tty_read_is_unknown_when_its_syscall_cannot_be_read` and `any_reading_task_makes_the_group_read_the_tty` in `local_tty/terminal_attributes_tests.rs` (`terminal_attributes::foreground_group_tty_read` asks every thread of every member of the foreground process group, so a prompt under `make`, `npm run`, a script's child or a Go/Java reader thread counts; the #1641 per-key ssh probe `foreground_reads_tty` stays on the leader), `lrc_handoff_opens_the_inline_agent_view_only_when_no_agent_view_is_active` in `ai/blocklist/block/cli_controller_tests.rs`), `routing_table` (both modes; 10 installed-command lines must reach the shell with the exact line in rec.log, 6 prose lines go to the agent (terminal mode per `nld_in_terminal_enabled`), 3 installed-head English lines follow the documented rule by the surface the `phosphor::routing` enter-stage log reports; misroutes both ways fail, the table is in results.json; #1593: a line whose shell-or-chat verdict missed its budget stays in the editor with the "press Enter again" toast (`enter_verdict_give_up ... verdict_cap` / `young_verdict_cap`, `kh_enter_gate_give_up by=verdict_cap`): its row carries `verdict_give_up`, the check names the A5 toast instead of "LOST", and the `verdict_give_ups` metric counts them, so the agent-mode round shows whether the budget is too tight). Windows runs the PowerShell subset
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
the local shell is usable afterwards), `latency_enter_ssh` (#1532: Enter -> command start inside `ssh fake`, Phosphor vs plain `ssh fake` in the
reference terminal, both typing into the same remote shell; overhead verdict against `APPCI_SSH_ENTER_BUDGET_MS`, default 1 ms = the owner's
"plain ssh +-1 ms"; sequential; Windows: Git Bash pane only), `ssh_password_prompt` (#1535 part 2: runs BOTH secret prompts per ssh test host: `fakepw` and a REAL `pwuser@127.0.0.1's password:` on host `fakepass`; the password user is accepted only by the harness's paramiko fake sshd (`FAKE_PASSWORD` / `FAKE_PW_USER`, constants `sshd.PW_USER` / `PW_PASSWORD`: `fakesshd.py` as a second server beside the system sshd, or itself as the fallback server; `fakesshd_win.py` for the Windows ssh clients, Git Bash pane), no account or PAM needed; a missing prompt kind is a note, not a silent pass). The original text, passphrase half: host `fakepw` is a passphrase-protected key the local sshd accepts,
so ssh stops at `Enter passphrase for key` on the tty, `NumberOfPasswordPrompts 1`. A junk burst typed BEFORE the prompt must be discarded and not
reach any shell (since #1611/#1612/#1621 by the tty, not by Phosphor reading the prompt: the burst is given to ssh once it owns the tty and ssh's
passphrase read flushes it, or, if ssh is already reading the passphrase at that event, the termios guard discards it); the passphrase typed AFTER it must reach the prompt and log in; neither appears in any `rec.log`; a wrong passphrase returns to the
local shell where the next command runs locally. Needs the system OpenSSH sshd; fakesshd.py cannot prompt, so it skips there),
`shell_zsh`, `shell_fish`, `shell_nu`, `shell_pwsh` (`shell_fish` and `shell_nu` are also in SMOKE; binaries are looked up on PATH then ~/.local/bin, /home/gitlab-runner/.local/bin, /opt/homebrew/bin and added to the pane PATH; fish adds autosuggestion-does-not-steal-keys, `fish -c` and a `fish_postexec` log, nu adds `nu -c` and a nested nu; each shell as a subshell of the pane's bash: a probe line proves readiness, 12 typed lines, pasted
lines + instant Enter, Ctrl-C of a 30 s sleep then the next line, `exit` and the pane's bash runs the next command). A missing shell SKIPS with the
package: `apt install zsh`, `apt install fish`, PowerShell 7 from the Microsoft apt repo (`packages-microsoft-prod.deb`, then `apt install powershell`),
nushell is not in the Ubuntu repos (release tarball from github.com/nushell/nushell or `cargo install nu --locked`). The pane's own primary shell is
still bash (the harness rc files log its `rec.log`); a zsh/fish/nu/pwsh PRIMARY shell would need per-shell logging rc files in `profile.py`.
`gitbash_subshell` (Windows, PowerShell pane; Git for Windows `bash.exe` started from the pane's PowerShell: 20 fast-typed lines, 4 pasted lines of 10-5000 bytes
with instant Enter, 3 Ctrl-C of a trapped `sleep 30`, `exit` back to PowerShell; skips when Git for Windows is absent, and in the Git Bash pane) and `tabs_streaming` (every platform: 12 tabs each streaming a line every 20 ms, then 10 fast-typed commands in one more tab, exact, app alive) run in the shared harness. Windows runs the shell-neutral subset in the PowerShell pane and the POSIX cases in the Git Bash pane (3 above)
`agent_code_recycle` (both modes, #1267, needs the CI model: one answer with 12 fenced `echo RCY<tok>_NN` code blocks, then 9 one-word answers push it more than 6 rich-content entries away from the nearest visible one, so the app logs `editor_memory agent_code_recycled` (r18u22: when several short answers fit on the screen 9 are not enough, so the case adds more, one at a time, until the line comes, cap `APPCI_RECYCLE_MAX_FILLERS` = 24; metric `fillers_used`); block navigation (ctrl+up, mac cmd+up) goes back until `agent_code_rebuilt`; fails when no recycle or no rebuild happens, when the snippets visible on screen (OCR; without OCR the screenshot hash) change across the rebuild, or when copying the selected answer (ctrl+shift+c, mac cmd+c) loses a snippet line; a model that does not write the snippets is a SKIP; the per-snippet Run / Copy buttons need a mouse and are not driven), `editor_memory_7mb` (terminal pass only, #1340 P5 / #1607 / #1407: generates a 7 MB plain-text and a 7 MB Rust file, opens each in the code editor through the command palette with the absolute path, records memory before / after open, right after closing the pane and after the 60 s undo-close grace, and repeats open/close `APPCI_EDITOR_MEM_CYCLES` = 3 times for the plain-text file; fails when open costs more than `APPCI_EDITOR_MEM_TXT_MAX_MB` = 100 / `APPCI_EDITOR_MEM_RS_MAX_MB` = 150 MB over the pre-open baseline (**ACCEPTED_FOR_RELEASE, #1607, v0.1.17 only**: the owner accepted the open cost for v0.1.17 on 2026-11-04, so while app/Cargo.toml is at or below 0.1.17 an over-budget open is reported as `KNOWN: ... [ACCEPTED_FOR_RELEASE #1607 v0.1.17 ...]` with its numbers instead of failing; past 0.1.17 it fails again and the offline self-test fails until the acceptance is re-decided — `ACCEPTED_FOR_RELEASE` in cases.py; an unreadable version fails closed), more than `APPCI_EDITOR_MEM_CLOSE_MAX_MB` = 60 MB is still held after close, or the post-close figure grows more than `APPCI_EDITOR_MEM_CYCLE_GROWTH_MAX_MB` = 30 MB from the first cycle to the last. Judged figures: RSS (Windows: working set) everywhere and, on macOS, ALSO `phys_footprint` from libproc for the open cost (owner 2026-11-03: either one over a budget fails; both reported as `*_mb` / `*_phys_footprint_mb`). **The after-close "memory given back" check is judged on `phys_footprint` only on macOS** (owner 2026-11-05, `mem_judge_closed`): macOS marks freed pages MADV_FREE_REUSABLE, which leave `phys_footprint` at once but stay in RSS until memory pressure (r18u23: RSS +170 MB while `phys_footprint` was 290 MB below baseline), so RSS is reported there as a note with the reusable share and no longer fails the case; Linux and Windows keep their figures. **The default close trim's one-off UI pause on Linux is accepted** (owner 2026-11-05, ACCEPTED_FOR_RELEASE `editor_close_trim_ui`, #1607, v0.1.17, platforms linux: `malloc_trim` puts a ~7-16 ms UI item on screen after a large file closes, over the 4 ms budget): the trim A/B's default-vs-`PHOSPHOR_HEAP_TRIM=0` close verdict (`trim_ui_verdict`) is `KNOWN` with its numbers on Linux, and FAILs again past v0.1.17 and on other platforms; the opt-in OPEN trim A/B check is not covered and still fails. The app's own breakdown lines (log target `editor_memory`, debug: buffer tree, render tree, parked chunks, hydrated frames, buffer drop) are attached as `*_app_breakdown_open` / `*_app_buffer_dropped`; a close through the command palette that is not followed by a `buffer_dropped` line fails (Return ran a different palette item, or the editor is still referenced). #1607: a `buffer_dropped` with `trim=false` fails (the closed 7 MB editor asked for no heap trim), and one with `trim=true` / `coalesced` must be followed within 15 s by `editor_memory heap_trim why=close` (attached as `*_app_heap_trim_close`, `*_app_heap_trim_freed_mb`; on macOS that line also carries `released` (what `malloc_zone_pressure_relief` reported) and `resident` / `reusable` bytes, and the case notes the reusable share as `*_app_reusable_mb`: freed malloc pages marked MADV_FREE_REUSABLE are out of `phys_footprint` but still in RSS until the kernel reclaims them; a note, never a verdict); `trim=off` (a kill switch) is a note. A reopen that grows memory by less than 8 MB still counts as opened when the app logged its `editor_memory open` line (it reused the heap the previous close freed; before, the case pressed Escape and left that editor open under the next measurements; the app only logged that line on the lazy layout path, never for the code pane's file editor, until the #1607 fix in `CodeEditorModel`'s `LayoutUpdated` handler). A `buffer_dropped` with `registered` > 0 adds a note: another editor buffer is still open, so that cycle's held-after-close figure includes it (#1607: every run that blamed the .rs path had `registered=1` from a 7 MB txt reopen misjudged as "did not open"). Then a heap-trim UI A/B by relaunch (`APPCI_EDITOR_MEM_TRIM_AB=0` skips it): default, `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1` and `PHOSPHOR_HEAP_TRIM=0`, one open/close each, the max `input_order hop=ui_item` (main-thread frame / event / action of >= 4 ms, `log_ui_item_if_over_budget`) in the 5 s after the load's layout frame and after the close key, against the 4 ms budget; a trim variant fails only when it is over the budget and more than 1 ms over its control (metric `trim_ab`). The load's own layout frame (`ui_load_frame`: the largest item after the open) spawns the opt-in open trim, so it ends inside that trim's window; it is left out of the open trim's window and judged instead against the default variant's load frame by the same rule (r18u23 Mac: the '4.1 ms UI item' of the open trim was that frame, `load_frame_ms` 4.07, the trim itself 0.01 ms). `APPCI_KEY_PALETTE` / `APPCI_KEY_CLOSE` override the keys; where the close key is empty (macOS: the guard refuses cmd-shift-W) the pane is closed through the command palette, `APPCI_KEY_CMD_PALETTE` (mac `cmd+p`; `cmd+shift+p` is the navigation palette) + `APPCI_CLOSE_PALETTE_TEXT` (`Close Current Session`); `APPCI_EDITOR_MEM_STRICT=1` fails instead of skipping when the file never opens, `APPCI_EDITOR_MEM_KINDS=txt` limits it), **#1340 measurement fixes (Windows):** the case now judges Windows on PRIVATE bytes (`PROCESS_MEMORY_COUNTERS_EX.PrivateUsage`, `winapi.process_times_private`, sample key `private`, `judged_sample`), not on `WorkingSetSize`; the working set stays in the metrics (`*_rss_*`) and in notes `rss: +N MB over the baseline (reported, not judged on Windows ...)`, the way macOS reports RSS. The baselines are WARM: before the kinds loop `editor_warm_up` opens and closes a 64 KB file in the same app run (metric `warm_baseline`; `APPCI_EDITOR_MEM_WARM=0` off, `=1` forces it on other platforms; default on only for Windows), so first-open one-off costs are not booked to the 7 MB file. The thresholds and their env overrides (`APPCI_EDITOR_MEM_TXT_MAX_MB` / `_RS_MAX_MB` / `_CLOSE_MAX_MB` / `_CYCLE_GROWTH_MAX_MB`) are unchanged; Linux and macOS judge exactly as before. The app logs `editor_memory load_stage stage=<name> rss_kb=.. [ws_kb=.. private_kb=..|phys_footprint_kb=..] t=<epoch ms>` (debug, target `editor_memory`, loads of 1 MB or more only; stages: `file_read_done`, `start`, `formatted_text`, `edit_applied`, `undo_reset`, `reset_content_done`, `layout_start`, `layout_tasks_created`, `layout_chunk_done start=N` per parallel chunk, `layout_chunks_done`, `parker_finished`, `measure_widest_start/done`), attached for the first cycle as metric `<kind>_load_stages` (`load_stages`; the per-chunk lines are thinned to the first and last). The `input_order hop=heap_trim` and `editor_memory heap_trim` lines now carry `start=<epoch ms>` beside the end `t=`; `ui_trim_max` leaves out any UI item that began before the trim started (the load's own layout frame, which spawns the open trim, is one) and falls back to the old `[end - ms, end + 0.25 s]` window when a build's line lacks `start=`. The load-frame lookup for the open trim has no `t_close` clip and spans the whole open phase (a slow load puts its layout frame past 5 s). Self-test fixtures cover both the old and the new log formats.
**#1787 (Windows PowerShell agent pass never recycled, 24 fillers).** Recycling runs only when `block_list_element.rs report_visible_rich_content` sends a CHANGED visible
rich-content set, and `ai_code_recycle_plan` measures distance in entries from the nearest visible one, so there are two silent ways to never recycle: the visible set
never changes (the viewport stays where it was while answers append below it), or the snippet answer is still on screen / within 6 entries of a visible one (then
keeping it is CORRECT). The logs of r18u27/r18u28 cannot tell them apart, so this is not claimed as an app bug or a harness bug yet. The app now writes one debug line per
changed visible set, `editor_memory agent_code_plan rich=<n> visible=[<positions>] plan=[<position><R|B|K>,...]` (`ai_code_recycle_plan_summary`, `app/src/terminal/view.rs`,
unit-tested in `block_tests.rs`), and when the recycle never comes `agent_code_recycle` fails with that evidence: the number of plan lines and the last one (no `R` =
the snippet answer stayed near a visible entry), `NO agent_code_plan line` (the visible set never changed: a scroll/follow-the-bottom fault in the PowerShell agent view,
to be filed against the app), and whether the snippets are still on screen (OCR, metric `snippets_visible_without_recycle`). Decision rule for the next Windows run:
plan lines with no `R` and the snippets on screen = the case must scroll to the newest answer before counting (harness); no plan line, or a plan with `R` that did not
recycle = app bug. Unresolved until that run.
`tabs_two_windows` (`APPCI_TABS`, default 12 streaming tabs, a second window, typing and paste).
Windows `ctrl_c_d` / `latency_enter` (#1487 history: the PowerShell harness read a one-line timestamp file through a function that returned a bare string, so every sample was dropped;
the Python harness reads files with `Ctx.lines`/`stamps`, which cannot have that bug): PowerShell stamps UTC milliseconds (`[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()`),
bash stamps `date +%s.%N`; both are judged on OVERHEAD over the same typing into the reference console, exactly like Linux/macOS (budget `APPCI_CTRLC_MAX_MS`, 20 ms;
no reference = `no reference: absolute only`). A Ctrl-C miss in the PowerShell pane is followed by two more Ctrl-C so the next sample is not queued behind a 30 s sleep.
The old "try the other injectors" diagnostic is now the delivery proof's job (VK then Unicode).
**Linux reference terminal and `/tmp/.X11-unix` (#1567).** XWayland refuses to start when `/tmp/.X11-unix` is not owned by root, so the session has no `DISPLAY`, no xterm reference, and every Linux latency row is NOREF. `wayland_session.sh` logs a WARNING with the owner fix, and `ref_terminal` puts the same reason in `ctx.ref_reason` (`x11_dir_problem`). The harness never changes the box: the owner runs `sudo chown root: /tmp/.X11-unix && sudo chmod 1777 /tmp/.X11-unix`, or installs `foot`.

**UI cases (`script/realapp/appci/cases_ui.py`, #1462; all full-tier, none in SMOKE, Linux + macOS python harness only).**
Keys: Linux ctrl-shift-X, macOS cmd-X (`app/src/util/bindings.rs`). Every case verifies the shell's own `rec.log` and an effect file.
- `ui_tab_menu`: Tab on an ambiguous path completes the common prefix and opens the menu with nothing highlighted (#1452; owner decision on #1610, keep as built). Tab then Return runs exactly the typed line once (token in `tc/menuzz_` only); Tab, Down, Return accepts one candidate without running, the next Return runs exactly that candidate's full command (one trailing space allowed), token in that file only, no prefix-named file.
- `ui_ghost_accept`: type a prefix of an earlier command, Right (default binding) + instant Enter, fast and slow: the shell gets the full earlier command. End is probed and recorded (`end_key_accepts_ghost`), not failed.
- `ui_ctrl_r`: Ctrl-R, a unique fragment, accept: the shell receives exactly the earlier command (one extra Enter allowed, both flows handled), effect file shows it ran twice.
- `ui_palette`: palette opens ('Search for a command', >= 3 filter chips, or a `CommandPaletteView` frame in the app log), nonsense filter shows 'No results', 'split pane right' + Enter makes a new pane (new shell prompt in `prompt.log`), typing there is exact, `exit` closes it.
- `ui_mouse_select_copy`: print a token absent from the command line, find it by tesseract word box, double-click then copy: the PRIVATE clipboard (wl-paste / xsel of the job's display, pre-set to a sentinel) holds exactly the token; a drag selection contains it. Skips on macOS (no pointer tool) and without tesseract.
- `ui_right_click_paste` (terminal mode only): right-click in the input, click 'Paste' (OCR), Enter: the shell receives exactly the clipboard command.
- `ui_drag_drop_file` (terminal only): file name with a space dropped onto the terminal; the shell must see ONE argument equal to the path. Skips unless X11 with `dragon-drop` installed (no tool synthesizes a drag source on Wayland/macOS).
- `ctrl_c_no_esc_i_local` (#1798/#1672, `appci/cases_shells.py`, both modes, local; skips on Windows): `sleep 30`, Ctrl-C x1/x2/x3 at gaps 0/50/200 ms, then a typed echo. The effect file is exact, rec.log shows exactly [sleep 30, the echo] (no ESC-i or `^[i` ran as text), the app log never says "Session abandoned before bootstrap", and an OCR finds no visible `^[i`. The ssh variant is the lab case `lab_ctrl_c_no_esc_i` (its oracle is unchanged and matches the fix: the request is only written to a line editor the shell's ready signal proved reading). Unit tests: `pty_controller_tests::an_unproven_line_editor_after_ctrl_c_gets_no_request` / `a_proven_line_editor_after_ctrl_c_still_gets_the_request` (no ESC i while the editor is only fallback-active, typed order kept, Legacy and Table share this controller) and `line_editor_status_tests::only_the_ready_signal_proves_the_line_editor_reads`.
- `ui_resize_typing`: window small, large, back (xdotool windowsize / sway floating resize / System Events); typing exact at each size, a wrapping 150-char command byte-exact; skips if the screenshot does not change (resize ignored).
- `ui_split_panes`: split right and down by key, new shell per pane, focus moved with next/prev pane keys, typing exact in both panes, `exit` closes the new pane.
- `ui_startup_time`: launch to first prompt (metrics `cold_s` with state dirs wiped, `warm_s` three relaunches; OS page cache is not dropped, that needs root); fails at no prompt in 90 s or warm median over `APPCI_STARTUP_MAX` (20 s); typing exact after each launch.
- `keyring_unavailable_startup` (#1805, Linux only; Windows/macOS skip with the reason): three relaunches on private D-Bus sessions of the Linux launcher (`app.py` `keyring_dbus_files` / `launcher_argv`, `App.keyring`): `absent` (a bus config with no service directories and no gnome-keyring-daemon: nothing owns `org.freedesktop.secrets`), `hang` (a `.service` file whose `Exec` never claims the name, so a call waits out the bus's 120 s `service_start_timeout`, the window the waste audit saw block startup), then the normal `unlocked` keyring (control; the launcher seeds an unencrypted `login` keyring under the run's XDG data dir, because gnome-keyring cannot create a default collection without a prompt and every launch otherwise logged `Failed to acquire default Secret Service collection` and showed the notice). `network_gate_switch` also checks settings.toml still holds its origin before the keyboard Remove. Each launch: the `startup timing: ... total=Nms` first frame within `APPCI_KEYRING_FIRST_FRAME_MAX_MS` (2000; not judged for `unlocked`), typing exact right after the first prompt (no settle wait), the keyring notice (`Showing the keyring-unavailable notice`, logged only once a workspace has put the toast on screen) exactly once without a keyring and never with one, at most one `Failed to acquire default Secret Service collection` per run, no secure-storage worker give-up for `absent`, no `slow model update` >= 100 ms of a secure-storage model. Each launch's log is kept (redacted) as `logs/<mode>/phosphor.keyring_<variant>.log`. Self-test `keyring_unavailable_startup_pure_pieces`.
- `ui_settings_live`: theme (with `system_theme = false` written first), font size, `appearance.vertical_tabs.enabled` changed by writing the settings file (hot reload); each must visibly change the screen (Pillow diff, else hash) and typing stays exact; the original file is restored. The settings page is not clicked.
Packages the box needs for the full set: `tesseract-ocr` (positions), `python3-pil` (optional, stronger diff), `dragon-drop` (DnD only). The panes/palette/mouse/settings cases are not run in the sim backend.
Coverage-gap cases (`appci/cases_coverage_a.py`, #1534-#1541; all FULL tier, none in SMOKE; registered by the last import line of `cases.py`; every one
types at 10-25 ms/key with an instant Enter, judges the EXACT line/bytes the program received, saves screenshots at each decisive step, and ends with a
typed line proven exact in the LOCAL shell). A program the case needs that is not installed makes the case `noref` (UNVERIFIABLE, with the package), never a
quiet pass or a substitute (#1537 "silently uses less when vim is missing"):
- `paste_multiline_prompt` (terminal): a 3-line paste (short and 3000-char lines) into the prompt, Enter, each line runs exactly once and in order; metric `*_lines_run_before_enter` records whether the editor ran them at paste time. BOTH Windows panes.
- `paste_bracketed_off` (terminal): `printf '\033[?2004l'; cat > f`, multi-line paste with tabs and no final newline arrives byte for byte, no `ESC[200~` markers. Git Bash pane.
- `paste_agent_input` (agent): multi-line prose pasted into the agent input; the app's `hop=enter_begin` line holds it exactly, input type AI, the shell got nothing. Git Bash pane.
- `alt_screen_programs`: vim (typed text saved), less (`!cmd` inside less writes a token), htop (draws, `q`), tmux (inner shell receives the exact line; its rec.log is checked). Git Bash pane.
- `alt_screen_mouse_reporting` (terminal, Linux X11/Wayland): a python3 program turns on SGR mouse reporting; a real pointer click must arrive as `ESC[<0;x;yM` + release and the key typed after it last. Click position by OCR of the program banner (tesseract) else the window centre (Pillow); on Wayland with the virtual pointer, a click sweep inside the sway window rectangle (no OCR, #1789). Skips on macOS/Windows (no pointer tool).
- `sudo_su_real_shell` (terminal): the real privilege boundary the nested-bash stand-in only imitates. sudo needs passwordless `sudo -n`; su needs the runner to be root (su as `nobody`, writes into the case dir, which is chmod 777); ksu needs `ksu` + a ticket. Each part that cannot run is UNVERIFIABLE with the reason; the case SKIPS when none can. Proof is `id -u` typed into the inner shell, then exact lines, `exit`, and `id -u` of the local shell again.
- `ssh_drop_reconnect` (terminal, needs the local sshd): loses a live ssh by `<Enter>~.` (after a command; and, #1793, at the first remote prompt right after connect, with and without the Enter) and by `kill -9 $$` on the remote; after each the local shell works and is exact, a new `ssh fake` gives a remote prompt, remote lines are exact, `exit` returns local. Linux/Mac only (not yet verified against the msys/Win32 ssh clients).
- `crash_kill9_restore` (Linux, macOS, Windows; #1540/#1541 part 2): `restore_session=true`, N tabs with commands, kill of the app pid (SIGKILL; Windows `taskkill /PID /T /F`; `cases_robust.hard_kill`), relaunch through the platform's `App.start()` (macOS `open -n` of the bundle, Windows the exe, then run.py's key proof again via `ctx.reprove`): a shell comes up, nothing re-runs (effect file and rec.log), pane exact; restored shell count is a metric (a hard kill may lose the last session-state debounce, never replay a command).
- `crash_corrupt_db` (Linux, macOS, Windows): `warp.sqlite` (found under the scratch home, or on Windows under `%LOCALAPPDATA%\phosphor` / `%APPDATA%\phosphor`, `cases_robust.data_roots`) overwritten with garbage while the app is down; the relaunch must come up, move it aside as `warp.sqlite.corrupt-*` (#860) and leave an exact pane.
Still NOT covered (owner/env decisions, not code): Linux ibus/fcitx and Windows IME (needs an input-method daemon installed on the box/VM).
Windows pane shells beyond PowerShell 5.1 / Git Bash (#1534 part 2): `APPCI_WIN_SHELLS=pwsh7,cmd` (never in the default list; play-time variable of `app-test-windows`) adds startup_shell_override passes: the harness writes `session.startup_shell_override = "<pwsh.exe or cmd.exe>"` (and `new_session_shell_override`) into the scratch settings (`profile.gitbash_toml`, variant `pwsh7` / `cmd`), result rows are labelled `terminal@pwsh7` / `terminal@cmd`, and only `winmap.VARIANT_CASES` run (pwsh7: `mode_proof`, `startup_shell_pane`, `fast_typing`, `paste_instant_enter`, `tab_enter`, `ctrl_c_d` under the PowerShell family's helpers; cmd: `mode_proof`, `startup_shell_pane`), no skip rows for the rest. `startup_shell_pane` (`appci/cases_shells.py`, order 72) proves the shell by an effect (`PS7_Core` / `CMDOK_cmd.exe`), types N exact lines at 10-25 ms/key, interrupts a long command with Ctrl-C and runs the next line. cmd.exe's effect syntax is `Ctx.append_cmd('cmd')` (`echo tok>>file`), its line clear is Escape (no Ctrl-U), its pane state is `cd /d`. A machine without pwsh 7 / cmd.exe gets one skip row for the pass. Whether the app bootstraps cmd.exe at all is unverified until the first run.
Nextest on Windows (#1534 part 2, superseded 2026-11-03): `test-windows` now runs the same command as `test-linux` (see "Every suite on every platform" in 2.9); `WIN_TEST_FILTER` / `WIN_TEST_FULL` and `--profile ci` are gone, use `TEST_FILTER` (a nextest filterset) as on Linux.
Remote-server size and latency (#1157 part 2): `script/ci/remote_server_bench.py` drives `<bin> remote-server-proxy --identity-key K` over stdio with hand-encoded length-prefixed protobuf (`Initialize`, then `ResolvePath` / `ListDirectory` requests) in a scratch HOME and reports, per binary, the binary and gzip -9 size (plus a given tarball's size), startup to `InitializeResponse` (includes the daemon the proxy forks), round-trip p50/p95/p99/max of N small requests (`rtt_ms`), the same for a 3000-entry directory (`list_ms`, the malloc-sensitive path) and back-to-back throughput (`burst_ms_per_request`), with a delta table against the first binary. `script/ci/remote_server_before_after` builds `phosphor-oss` for the musl target twice, BEFORE = `release-cli` without mimalloc, AFTER = `release-remote` + `mimalloc` (what `script/linux/bundle --artifact cli` builds), and runs it; the manual `bench-remote-server` job (box runner, 5 h, nothing installed: zig, cargo-zigbuild and the musl target must already be there from `cli-linux`) uploads `bench-out/remote-bench.json` and both tarballs. Expected from #1157: tarball about +20-30 %, p99 and burst better with mimalloc; the first run records the real numbers. The measurement loop is unit-tested against a fake proxy in the appci self-test.

One version on the host (Wave H step 1): after a verified install the host keeps exactly one `phosphor-*` binary. `crates/remote_server/src/one_version_cleanup.sh` is one shell fragment run by two callers, the tail of `install_remote_server.sh` (only once the new binary answers `--version`) and `setup::one_version_cleanup_command` (the client runs it after `verify_installed_binary` and after every `check_binary` that finds the right version). `setup_tests.rs` covers it with real `sh`: `one_version_cleanup_keeps_only_the_named_binary`, `..._keeps_the_unversioned_dev_binary_when_it_is_the_one_kept`, `..._removes_dead_daemon_files_of_other_versions_but_not_live_ones` (a live daemon's pid file is this test process), `..._with_no_hash_sweeps_no_daemon_files`, the single-quote guard on the fragment, and `install_script_leaves_exactly_one_version_installed` (pre-seeded old versions plus the legacy unversioned binary, one left after the real install script). These are unit tests, no ssh and no lab.

Crate features (Wave H step 2, #1804): `remote_server` has `client` (default; warpui, warp_core, warp_util, repo_metadata: the manager, transport, client, setup) and `daemon` (the `phosphor-remote` helper core; no warpui). The always-on modules (`proto`, `protocol`, `frames`, `host_response`, `ssh`, `auth`, `codebase_index_proto`, `rt`) compile without `client`. `rt::FutureExt`, `BoxFuture` and `mark!` are warpui's own under `client` and small `async-io` stand-ins without it, so the app behaves as before. Unit tests for the stand-ins (`rt::tests`) only exist in the no-client build: `cargo test -p remote_server --no-default-features --features daemon --lib`. The protocol and frame tests run in both builds; the guards CI job checks `cargo tree -p remote_server --no-default-features --features daemon -e normal` has no warpui (job added in step 5).

The helper's daemon core (Wave H step 3, #1804), `crates/remote_server/src/daemon/` (feature `daemon`, unix; run with `cargo nextest run -p remote_server --no-default-features --features daemon --lib`): `server.rs` is the single-owner core driven by an mpsc of connection events (`server_tests.rs` drives it in-process through `handle_message`: handshake with capabilities, `UNSUPPORTED` for everything outside the list, list-directory, a failing git request, `RunCommand` before and after `SessionBootstrapped`, abort, the idle-exit grace period and that a connection prevents it); `conn.rs` (reader and writer threads, `conn_tests.rs` is the app daemon's disconnect-classification tests); `proxy.rs` (the old flock + fork + exec + stdio bridge, `proxy_tests.rs` ported: versioned socket and pid names, stale unversioned cleanup, the readiness pipe); `fs_ops_tests.rs` (listing, symlink-aware resolve, nested create, chunked write/read with eof, whole-file write and delete); `git_tests.rs` (the app daemon's response-builder tests plus branch-list and numstat parsing and a real `git init` repository, skipped when `git` is missing); `ripgrep_tests.rs` (validation, line and submatch offsets, the long-line case, case folding, the cap, a bad regex is an error, cancel); `exec_tests.rs` (shell names, per-shell no-config flags, output/exit code/cwd/env, spawn failure, wire-size truncation, abort kills the process group; bash-dependent tests return early without bash); `jobs_tests.rs` (reply routing and that an aborted job sends nothing). `capability.rs` tests pin the rule: an EMPTY capability list is a legacy full daemon (supports everything), a non-empty list is the helper. `naming.rs` holds the hash and file-name helpers shared with `setup.rs` (the pinned-hash test stays in `setup_tests.rs`). Not covered by any unit test: the accept loop and `run_daemon` themselves; the lab `upload` arms (step 5) are the first end-to-end run of the bin.

The helper binary and its profile (Wave H step 4, #1804): `[[bin]] phosphor-remote` (`crates/remote_server/src/bin/phosphor-remote.rs`, `required-features = ["daemon"]`, so a normal workspace build skips it). Build it with `cargo build -p remote_server --profile release-remote-helper --no-default-features --features daemon --bin phosphor-remote`. The profile (`release-remote-helper` in the root `Cargo.toml`: opt-level "s", fat LTO, one codegen unit, strip, panic abort) is NOT the existing `release-remote`, which is the full CLI tarball's speed profile (#1157) and stays. Arguments are `--version` (prints `phosphor-remote <version> protocol <n>`, plus a `build <id>` line when `PHOSPHOR_REMOTE_BUILD_ID` was set at build time), `proxy <key>` and `daemon <key>`. `naming::tests::version_line_has_the_shape_the_client_probe_expects` pins the first line the client compares in step 5. Manual smoke on a build: `PHOSPHOR_REMOTE_HELPER_DIR=$(mktemp -d -p $TMPDIR) ./phosphor-remote --version`, then pipe a hand-encoded `Initialize` into `proxy k` (the same technique as `script/ci/remote_server_bench.py`).

Parity P0: shell mode foundations (Wave H parity, `docs/design/remote-helper-parity.md`). All unit tests; nothing here needs ssh, a lab, cargo on the laptop or the network. `crates/remote_server/src/shellcmd/` (no feature, no I/O, no warpui) has golden tests beside each file: `frame_tests.rs` (the exact POSIX script for two sections, `sh -c` wrapping for fish and nushell, `-EncodedCommand` for PowerShell, parsing: two sections with exit codes, the framing newline removed exactly once, CRLF, a marker with the wrong nonce or mid-line is plain output, binary bytes survive, NoFrame / Truncated / Malformed / TooLarge are distinct errors, the 10 MB cap is an error not a truncation), `base64_tests.rs` (RFC 4648 vectors, all 256 byte values, 64- and 76-column wrapping, `od -tx1` hex, the probe command's tool order and the probe-output map), `shell_kind_tests.rs` (shell name classification, fish `\\` and `\'` escaping, the nushell raw-string hash count, UTF-16LE encoded command), `predicates_tests.rs` (`command_may_change_head/_cwd/_git_state`; the app's `util/git.rs` and `git_status_update.rs` delegate to them, so their existing tests in `util/git_tests.rs` and `git_status_update_tests.rs` still cover the same bodies). `crates/remote_server/src/backend/` (feature `client`): `mod_tests.rs` (every Family has a distinct capability that round-trips, every capability the crate names belongs to a Family, error classification into unsupported / connection lost), `router_tests.rs` (the router decision: no helper = shell; an EMPTY capability list = legacy daemon = every family on the helper; a partial list serves only what it lists; a lost connection = shell; UNSUPPORTED moves one family to the shell for that connection only, other families and other hosts are untouched; a reconnect clears it; the per-family always-shell override and its `PHOSPHOR_REMOTE_ALWAYS_SHELL` parser; out-of-band channels are per session and a session without one makes the shell backend unavailable), `refresh_tests.rs` (RefreshCoordinator: one in flight, any number of events meanwhile collapse into exactly one rerun, EveryPrompt vs GitRelevant policies, an unknown last command counts as relevant, one refresh per new cwd; there is no timer anywhere in it), `contract_tests.rs` (ONE set of assertions run against every backend: the shell backend over a local `sh` executor always, the helper backend over an in-process daemon only with `--features daemon` on unix: `cargo nextest run -p remote_server --features daemon --lib`; P0's single family-independent assertion is that a request no backend implements is `Unsupported`; plus a framed script round-trip through real `sh`, the base64 probe and a binary file round trip, `Unavailable` without a channel, and that an UNSUPPORTED answer from a helper is served by the shell and the helper is then not asked again). Each family package adds its assertions to `assert_contract`, which binds both backends. `daemon/server_tests.rs::every_reserved_family_request_is_unsupported_and_named` pins the reserved daemon modules, `capability.rs` pins the `agent_context` and `codebase_chunks` names (the old `skills` constant was never used and is gone). The chip wording is now "shell mode" (`ssh_no_helper_chip_tests::label_is_the_agreed_wording`, owner-approved in the design; `lab_no_helper_chip` OCRs the new text); the explanation test that it promises nothing the helper lacks is unchanged.

Parity out-of-band adapter (Wave H parity, P1 commit 0). `ControlMasterChannel` (`app/src/terminal/model/session/command_executor/remote_command_executor.rs`) implements `OutOfBandExecutor` over a fresh mux client on the session's ssh ControlMaster, and `command_executor::register_shell_mode_channel` registers it with the router for a legacy ssh session that has a ControlMaster socket (not tmux, not in-band, not local); `Sessions` holds the guard that unregisters it when the terminal goes away, and `RemoteServerManager::backend_for_session` is how call sites that know a session but have no helper host id get a backend. Unit tests in `remote_command_executor_tests.rs`: `channel_output_is_cut_at_the_cap_and_marked`, `one_master_has_one_inflight_limit` (the generator executor and the shell channel share ONE limit of 4 per master, so a burst queues instead of failing or opening more sessions than sshd allows), `login_shell_types_map_to_command_builder_kinds`. The ControlMaster itself is exercised end to end by the P1/P2 lab cases below.

Parity P1+P2: file reads and writes in shell mode (`docs/design/remote-helper-parity.md` family a and the `apply_file_diffs` write half). Shared, pure, std-only: `shellcmd/read.rs` (per-file and batch byte caps, line ranges, the truncation `line_range_*` marker, CRLF normalisation, binary detection, `failed_files` wording: ONE `assemble` that the helper's `daemon/file_context.rs` and the shell arm's `backend/shell/read.rs` both feed, so they cannot differ; the POSIX and PowerShell section builders and the parser) and `shellcmd/write.rs` (temp file in the target's directory + `mv`, base64 chunks of at most 64 KB, mode kept via `stat -c %a` / `stat -f %Lp`, symlink targets written through, optional size+mtime `Expect`, PowerShell `WriteAllBytes` + `Move-Item`; the PowerShell scripts set `$LASTEXITCODE` and `return` because `exit` inside the framed block would end the host). Unit tests: `shellcmd/read_tests.rs` (caps, ranges, CRLF, empty file, first line over the cap is a named failure not an empty file, batch budget, binary and invalid UTF-8, a prefix of a big file keeps the whole file's line count, a prefix cut mid-character is not binary, a prefix the cap never cut is refused, plans, section parsing, quoting and `~` expansion of both builders), `shellcmd/write_tests.rs` (chunking, first/middle/last command shapes, the guarded check sits before the rename, quoting, no `exit` in PowerShell, failure wording), `daemon/file_context_tests.rs` (real files: truncation, deep ranges, missing/dir/binary, cancel, the gather bound), `daemon/server_tests.rs` (the helper advertises `file_context` and serves it), `base64_tests.rs` (decoder probe), `router_tests.rs` (the `hop=backend_route` log line format and reasons; refresh listeners get `AfterOwnWrite`), `manager_tests.rs` (the no-helper stand-in host id round trip). `backend/contract_tests.rs` gained `assert_read_file_context` and `assert_file_write`: literal expectations (whole files with apostrophes/spaces/non-ASCII names, CRLF, empty, cap truncation with the range marker, ranges deep in a 3000-line file, missing/dir/binary, batch budget; writes into new nested directories, tricky text byte for byte, a 180 KB multi-chunk write, mode 0751 kept, no temp file left, delete, delete of a missing file is an error) run against the shell backend over a local `sh` always and the helper over an in-process daemon with `--features daemon` (`cargo nextest run -p remote_server --features daemon --lib`). Lab (`appci/cases_lab_remote.py`, `lab_read_files_remote` and `lab_diff_apply_remote`; shells bash dash ash nu pwsh, `nohelper` and `upload`, agent mode, needs the lab and `APPCI_FAKE_MODEL=1`): the scripted fake model now also answers "Please read exactly this file and nothing else: P", "Please create exactly this file: P with this content: T" and "Please delete exactly this file and nothing else: P" with `read_files` / `apply_file_diffs` calls (`fake_model_replies` self-test); the oracles are the file on the host and the app log's `hop=backend_route family=<capability> backend=<helper|shell|unavailable> reason=<caps|override|unsupported_fallback|connection_lost|no_channel>` line (nohelper must say shell, upload helper), and that nothing the feature did was typed into the remote shell (rec.log). No base64 decoder on the host: the raw chunk bytes ride stdin to `cat` (since #1864; `write_tests.rs::without_a_decoder_the_raw_bytes_travel_on_stdin_and_cat_copies_them`), and `contract_tests::the_shell_backend_writes_on_a_host_with_no_base64_decoder` runs the whole write assertion set on a local `sh` whose PATH has `base64` and `openssl` removed (reads use `od` there). The lab coverage guard (`cases_lab_helper.py`) now sees `file_context` and `file_write` as covered through `cases_lab_remote.declare_coverage()`, and only counts a family as stubbed while a method is still `=> unimplemented`. `notebooks/file/mod.rs` reads through the router. Not run yet: nothing here has been compiled or run (no cargo on the laptop, no lab).

Write correctness (remote parity audit findings 2, 3, 4, 5, 13, 14, 16; #1865-#1868, #1876, #1877, #1879). One decided semantics, held by both arms: a whole-file write is a hidden temp file `.<name>.phosphor-tmp-*` beside the target plus a rename, so the INODE IS REPLACED on both (a hard link keeps the old bytes, ACLs/xattrs are not carried; mode is kept, owner and group are kept as far as the user may via best-effort `chown`/`chgrp`); a symlink (dangling too) is written through, never replaced, and the shell follows it with plain `readlink` (no `readlink -f`); a dangling link is PRESENT for `Expect::Absent` on both; a directory at the path fails (shell exit 7 `EXIT_ISDIR`, helper `IsADirectory`) with nothing created; a leading `~`/`~/` expands on the host in writes, deletes and the shell's git `cd`. The shell arm sends a cleanup command (`posix_cleanup_command`/`pwsh_cleanup_command`) after any non-OK exit, refusal or transport error between chunks; a write future DROPPED mid-flight cannot (the hidden name keeps that leftover out of trees and `git status`). Buffers: `RoutedBackend::open_buffer` (an inherent method that shadows the trait one) remembers the SHA-256 of what a helper-served open returned, so a save that falls back to the shell arm is guarded; a shell save with no remembered base is refused ("Reload the file"), never written. Tests: `shellcmd/write_tests.rs` (hidden temp name, ISDIR before and after the chunks, no `readlink -f`, dangling check, chown/chgrp before the move, cleanup names the same temp file), `shellcmd/mod_tests.rs::sh_quote_path_*`, and in `backend/contract_tests.rs` `assert_write_edge_cases` (directory, symlink, dangling link, hard link, `~`, no leftovers) run on the shell over local `sh` and on the helper over an in-process daemon (`--features daemon`), `a_multi_chunk_write_that_fails_part_way_leaves_no_temp_file`, `a_buffer_save_with_no_record_of_the_loaded_file_is_refused_not_written` and the router test `a_buffer_opened_through_the_helper_is_guarded_when_the_save_falls_back_to_the_shell`. The tilde case writes under the real `$HOME` in a uniquely named directory it removes afterwards. Not compiled or run yet (no cargo on the laptop).

Parity P3: the remote file tree and the helper's watch infrastructure (Wave H parity families b and i). Unit tests need no ssh and no lab. `shellcmd/tree_tests.rs` (pure): child ordering (directories first), host path joining and containment, `tree_from_paths` / `snapshot_entries` (ancestors loaded, ignored directories listed unloaded, breadth-first entries), `diff_dir` and `apply_refresh` (removed and retyped entries, an unlisted cached directory is left alone, a second identical refresh says nothing), the NUL-record list parser (symlinked directories and special files dropped), the ignored-name and `.git` rule, path lists in NUL or newline form with a cut final record dropped, quoting of the builders for POSIX and PowerShell, and the navigation answer parser. `daemon/watch_tests.rs`: the refcounted `WatchSet` over a recording backend (the OS watch lives while any consumer holds the path, the cap refuses new paths but not new references, capacity vs missing-path refusals are told apart, a disabled set reports unavailable), event classification (`Access` is not forwarded), and a REAL `notify` write that must arrive as `Event::FsChange` with no debounce (skipped with a message when the machine has no watcher). `daemon/repo_meta_tests.rs`: `.git` walk-up (directory and file), gitignore-aware listing, `.git` flagged only inside a repo, everything inside an ignored directory ignored, symlinked directories omitted, breadth-first walk vs the one-level walk outside git. `daemon/server_tests.rs` (P3 section): `NavigatedToDirectory` replies and then pushes a request-id-less `RepoMetadataSnapshot` (one level outside git; the whole ignore-aware tree inside), the Invalid path / repo_path / dir_path / not-a-descendant / not-indexed errors of `LoadRepoMetadataDirectory`, and, with a real watcher, `a_watched_repo_pushes_an_update_when_a_file_appears_or_goes_with_no_timer` and `a_repo_nobody_follows_any_more_releases_its_watches`; the capability pin now asserts the exact list including `repo_metadata`. `backend/contract_tests.rs`: `assert_repo_metadata` runs the SAME file-tree assertions against the shell backend (local `sh`) and the helper (in-process daemon), and `the_shell_backend_queues_the_snapshot_the_helper_pushes` compares the two snapshots node by node; `a_shell_refresh_queues_only_what_changed_and_never_before_it_is_asked` proves the shell backend sees an outside edit only when `refresh_repo_tree` is called. These need `git`; they return early with a message when it is missing. Lab: `appci/cases_lab_remote.py` (new file, one case per family, self-test `lab_file_tree_remote_pieces`) registers `lab_file_tree_remote.<bash|zsh>.<lan|150>.<nohelper|upload>`: in a git repo on the server the user `cd`s into a subdirectory (the app logs `hop=file_tree event=snapshot source=helper|shell`), a file is then created over a SEPARATE ssh connection; the `upload` arm must log the `update` with no prompt (live), the `nohelper` arm must log nothing while no prompt returns and the update after the next one, and the removal follows the same rule. The log carries counts and the FNV-1a hash of each added leaf name only. On the `nohelper` arm the case is UNVERIFIABLE (never a pass) when no snapshot is logged, which is what a build without the session's out-of-band channel adapter does.

Parity P8: the remote agent context, family h (the host's skills and global rules; bundled skills come from the app, never from the host, ruling R3). Pure and shared: `shellcmd/agent_context.rs` (the ten home skill roots, the rule file, the size/byte/count caps, `assemble` = UTF-8 only + sorted + capped, `same_context`, the POSIX record commands and their parser, `is_relevant_change`, `command_may_change_agent_context`, and the project-skill standing-query helpers) with `shellcmd/agent_context_tests.rs` (record round trip incl. NUL and newline content, a record cut short ends the list, filtering and caps, relevant vs irrelevant paths, commands for posix/fish/nu and none for PowerShell, project skills one level under a provider directory, the standing delta). `daemon/agent_context_tests.rs`: the `std::fs` scan (provider dirs, depth, hidden dirs, the watch set) and, with a real watcher, initialize -> `InitializeResponse` then a `RemoteAgentContextSnapshot` (revision 1), a skill written afterwards -> a higher revision pushed without a request, a second connection is handed the current snapshot with no new revision. `daemon/server_tests.rs`: the capability pin lists `agent_context`. `backend/contract_tests.rs`: `assert_agent_context` runs the SAME assertions against the shell backend (local `sh` with `HOME` set to a fixture, `HomeShExecutor`) and the helper (in-process daemon with the fixture as its home, `spawn_in_process_with_home_for_tests`): skills in `~/.agents/skills` and `~/.claude/skills`, left out when not UTF-8 / over 256 KiB / two levels deep / hidden, source Home only, the rules file, a skill added -> higher revision with all three, the rules file removed -> another; plus an unchanged shell read queues nothing and revisions only grow, no channel is Unavailable, and the section commands render for posix, fish and nu. `app/src/ai/remote_agent_context_tests.rs` pins the host-side roots to `SKILL_PROVIDER_DEFINITIONS`; `terminal/model/session/active_session.rs` tests that a no-helper session with an out-of-band channel is `Remote` under `shell_mode_host_id`. Project skills ride with the file tree (`standing_results` in the snapshot and updates, both arms). Lab (`appci/cases_lab_skills.py`, `lab_skills_remote.<bash|zsh|fish|nu>.lan.<nohelper|upload>`, terminal mode, self-test `lab_skills_remote_pieces`): a skill and `~/.agents/AGENTS.md` made on the server over a separate ssh connection reach the app (`hop=agent_context source=helper|shell revision=N skills=S rules=R names_hash=[...]`, counts and name hashes only); a second skill added behind the app's back arrives live with the helper and, without it, only after a command that names a skill location (`ls $HOME/.agents/skills`), never on a timer; the feature types nothing into the remote pty. UNVERIFIABLE (never a pass) on the nohelper arm when the app logs no `hop=agent_context` line. Not run yet: nothing here has been compiled or run (no cargo on the laptop, no lab).

Parity P4: remote buffer editing (Wave H parity family c). Helper arm, `daemon/buffers.rs`: `buffers_tests.rs` (pure) pins 1-indexed CHARACTER offsets applied in sequence and clamped (non-ASCII text, an end before its start inserts, offset 0 is the start) and `diff_edit` (the single differing middle between common prefix and suffix, in characters, applied to the old text gives the new text for empty/unicode/overlapping cases). `daemon/server_tests.rs` (P4 section) drives the core: open + edit + save with a temp+rename write and no temp file left, a stale `expected_server_version` dropped with no response, a disk change with no client edits pushed as ONE `BufferUpdatedPush` (version bumped, `expected_client_version` carried) and the same change again silent, a disk change over unsaved edits pushed as `BufferConflictDetected` (not applied), our own save echoing through the watcher without a push, `ResolveConflict` refused when the acknowledged server version is stale, a `force_reload` open telling the OTHER connection the whole new text, the last close/disconnect freeing the buffer, and a missing or non-UTF-8 file refusing to open; the capability pin now lists `buffers` and the reserved-family test no longer lists the three buffer requests. Shell arm (ruling R4: no server-side buffer; conflicts are found at save): `shellcmd/buffer_tests.rs` (state-command parsers and the tools they use), `shellcmd/write_tests.rs` (`HexDigest`, the SHA-256 guard of a write before the rename, POSIX and PowerShell). `backend/contract_tests.rs`: `assert_buffers` runs the SAME open / edit + save / outside change / discard / keep-mine / error assertions against the shell arm over a local `sh` and the helper over an in-process daemon, with the one documented difference (the helper PUSHES the conflict, the shell arm reports `BufferSaveOutcome::Conflict` at save and writes nothing); `the_shell_arm_finds_a_same_size_same_second_change_with_the_hash` proves the hash sees what a size+mtime stamp cannot and that a `touch` of unchanged bytes is not a conflict. App: `global_buffer_model_tests.rs` `save_remote_buffer_fails_when_the_host_has_no_helper_and_no_channel`. Lab: `appci/cases_lab_buffers.py` registers `lab_remote_edit.<bash|zsh|fish|nu>.lan.<nohelper|upload>` (self-test `lab_remote_edit_pieces`); the harness has no pointer click, so the code-editor layer (open, edit, save, banner) is UNVERIFIABLE there, never a pass, and the case checks only the staged file and the pty (and the `hop=backend_route family=buffers` line when `APPCI_LAB_EDIT_OPENED=1`); the protocol is held by the contract tests. Needs the build box: `cargo test -p remote_server --features daemon --lib` (not run on the laptop).

Parity P6: diff state, git status and GitHub info, families d and e (helper and shell arm). Shared, pure: `shellcmd/git/diff.rs` (the code-review snapshot, metadata, per-file deltas and the `content_at_base` wire budget computed over a `GitRunner` straight into the wire messages; hunk parsing with the no-newline marker, the render-size limits that mirror `diff_size_limits.rs`, autogenerated detection, git calls run four at a time), `shellcmd/git/status.rs` (the chip's metadata: branch, main branch, HEAD stats, upstream ahead/behind/rebased) and `shellcmd/git/gh.rs` (gh JSON reading moved out of the app; host parsed from the URL; `get_pr_for_branch`/`get_repository_info` over a runner). Unit tests: `diff_tests.rs` (real git in temp repos: modified/added/deleted/untracked/renamed, staged vs unstaged vs partial, binary, branch modes with the merge-base content, per-file deltas, NotInRepository), `status_tests.rs` (chip stats, upstream counts, detached HEAD), `gh_tests.rs` (JSON, host parsing, no PR / signed out / detached / outside a work tree never run gh wrongly). Helper (`daemon/diff_state.rs`, `git_status.rs`, `github.rs`, `repo_watch.rs`; capabilities `diff_state`, `git_status`, `github`): `diff_state_tests.rs` (wire path echoed verbatim, a bad path is a `DiffStateError`, requests for one key join one computation, an edit is a `DiffStateFileDelta` with fresh metadata and an ignored file is not news, a commit pushes a whole snapshot, unsubscribe and disconnect stop pushes), `git_status_tests.rs`, `github_tests.rs` (a fake `gh` on PATH: PR and repository info, a gh that cannot run or is signed out is the absent shape, the PR is looked up again on a branch change and NOT on an edit or commit), `repo_watch_tests.rs` (what inside `.git` matters, `index.lock` ignored, many files become a whole recompute, the watch plan). The tests drive a real core with real filesystem watching (`live_tests_support.rs`). `server_tests.rs`: the capability pin lists the three. Shell arm (`backend/shell/diff.rs`, `status.rs`; `RoutedBackend::request_git_status/request_pr_info/request_repository_info`): `backend/contract_tests.rs` `assert_diff_state` runs the SAME literal assertions against the shell arm over a local `sh` and the helper over an in-process daemon, and `the_shell_arm_serves_the_chip_pr_and_repository_values` / `the_helper_serves_the_same_chip_pr_and_repository_values` hold both to the same chip, PR and repository values with a fake `gh` (also: gh missing or signed out is absent; the PR is not looked up again while the branch is unchanged unless forced). The app models (`diff_state_remote.rs`, `git_status_update_remote.rs`, `github_repo_model/remote.rs`) go through the backend and re-read on `RemoteServerManagerEvent::ShellRefresh` (prompt, cwd, user refresh, the app's own write) with a `RefreshCoordinator`, never a timer; they log `hop=diff_state`, `hop=git_chip`, `hop=gh_chip` (counts and hashes only). Lab (`appci/cases_lab_diff.py`, `lab_diff_panel_remote` and `lab_gh_chip_remote`, bash zsh fish nu, `nohelper` and `upload`, terminal mode, self-test `lab_diff_panel_and_gh_chip_pieces`): an edit made over a second ssh connection is pushed live with the helper and seen only at the next prompt without it; the fake gh lives in the lab home (`~/bin`, `~/.local/bin`) and the PR chip shows PR 42; the code-review panel has no keyboard path, so the panel layer is UNVERIFIABLE (never a pass) when no `hop=diff_state` line appears. Not run yet: nothing here has been compiled or run (no cargo on the laptop, no lab).

Parity P5: shared git logic (Wave H parity). `crates/remote_server/src/shellcmd/git/` (no I/O; the `GitRunner` trait is the only door out) holds the argv builders, parsers and git algorithms the local app, the helper and the shell backend now share. `parse_tests.rs` is golden: porcelain v2 (changed with the staged column, rename and copy with the old-path token, unmerged, untracked, ignored, malformed), `# branch.*` headers (tracking with ahead/behind, detached HEAD, unborn branch, gone upstream, garbage `ab`), shortstat, tracking counts, `numstat -z` with a rename, numstat text forms, name-status `-z`, the commit log, the branch list, the gh/git error classifiers, the argv builders (`apply --cached [--reverse] [--recount]`, stage/unstage, `rm --cached`, stash) and the operation-probe script. `ops_tests.rs` drives the algorithms against a scripted runner that records argv: main-branch detection order, detached/unborn current branch, the commit chain per mode and that it stops at the first failure, the pre-first-commit un-stage fallback, patch via stdin, discard with and without stash (rename handling, restore-and-clean fallbacks incl. the fs-removal hook), the exit-status policy of `git_text`/`gh_text`. Runner tests: `backend/shell_git_runner_tests.rs` (the exact script text, base64 stdin, and real `git` through a local-`sh` executor incl. a hunk over stdin and a commit chain), `daemon/git_runner_tests.rs` (`--features daemon`: the shared algorithms on the helper runner, spawn failure is an error not an exit code); both skip when `git` is missing. Behaviour parity with the old app code is pinned by the EXISTING tests, unchanged: `app/src/util/git_tests.rs` (detached HEAD raw/display, `run_pull` fast-forward and diverged, `commit_chain_commit_only_creates_a_commit_and_reports_delta`, `compute_unpushed_state_tracks_upstream_and_unpushed_commits`, `git_operation_in_progress_*`, the gh/PR tests with a fake `gh`, `child_env`, `wsl_unc_git`), `app/src/code_review/diff_state_tests.rs` (`parse_git_status*`, name-status, `sum_numstat_z`, stage/discard) and `git_status_update_tests.rs`. The app keeps thin wrappers (`LocalGitRunner` in `util/git.rs` runs the same `git -c diff.autoRefreshIndex=false ...` with the same env, WSL routing and error text).

Parity P7: git writes on both arms (Wave H parity, family f; capability `git_write`, now in `HELPER_CAPABILITIES`). The commit chain, push, pull, create PR, discard and stage requests are ONE implementation, `shellcmd/git/write.rs`, run over a `GitRunner`: the helper (`daemon/git_write.rs` on worker threads, `HelperGitRunner`) and the shell backend (`backend/shell_git_write.rs`, `ShellGitRunner` over the out-of-band executor; the in-progress guard is `operation_probe_script` run on the host; discard leftovers are deleted host-side with `rm -f`). `shellcmd/git/write_tests.rs` (real `git`, temp repos with a bare `origin`, skipped without git): commit-and-push reaches the origin and the delta is empty, push and fast-forward pull between two clones (a diverged pull is refused with git's words), stage takes exactly one of paths|patch and rejects absolute paths with the old server's messages, paths and a hunk stage and un-stage (`--reverse`), discard validates (empty list, relative path) then restores a modified file and removes an untracked one, every write is refused while `MERGE_HEAD` exists and nothing is committed, absolute-path detection in either host's encoding, `git_ssh_command`. Credentials (owner ruling R2, never prompt): both runners remove `GIT_ASKPASS`/`SSH_ASKPASS`, set `SSH_ASKPASS_REQUIRE=never` and `GIT_TERMINAL_PROMPT=0`, and set `GIT_SSH_COMMAND` to the user's own command (or `ssh`) plus `-o BatchMode=yes`; `gh` gets `GH_PROMPT_DISABLED=1`. `daemon/git_runner_tests.rs::the_helper_runner_never_lets_git_prompt` and `backend/shell_git_runner_tests.rs::the_shell_runner_never_lets_git_prompt` prove it with a git alias that runs `env` (the second with `GIT_ASKPASS`, `SSH_ASKPASS` and `GIT_SSH_COMMAND` offered by the caller's environment); the pinned script text in `script_quotes_cwd_args_and_sets_the_shared_env` changed for this ruling. `daemon/server_tests.rs` pins `HELPER_CAPABILITIES` (now including `git_write`), drops the six requests from the reserved-UNSUPPORTED set, and `git_write_requests_answer_with_their_own_error_responses` checks the old contract (a bad `repo_path`, an empty discard and a malformed stage come back as the response's error). `backend/contract_tests.rs::assert_git_write_contract` runs against BOTH the shell backend (local `sh`) and the in-process helper daemon: commit chain with push, every stage validation and both stage granularities, discard validation and effect, push, a fast-forward pull, the `MERGE_HEAD` guard on stage and push, a push to an HTTP remote that answers 401 (must fail with "terminal prompts disabled"/"could not read Username" and not hang), and a create-PR in a throwaway repo (an error response, never a prompt). The old "unimplemented is Unsupported" checks moved from `git_stage` to `read_file_context`. App side: `git_dialog/mod_tests.rs::credential_failures_are_recognised_whichever_words_git_or_ssh_use` and `the_run_in_terminal_command_is_one_pasteable_line_for_any_shell` (the toast link puts `git -C <repo> push --set-upstream origin <branch>` and friends into the input editor via `WorkspaceAction::InsertInInput`, NOT run). Lab: `script/realapp/appci/cases_lab_git.py`, case `lab_git_write_remote.<bash|zsh|fish|nu>.lan.<nohelper|upload>` (terminal mode; a bare repo in the lab is the remote). The host layer runs the exact script shape of the shell backend under the user's login shell wrapped in `sh -c '...'` (commit chain and push, ff pull and a refused diverged pull, ONE of two hunks staged from a base64 patch on stdin and taken back out with `--reverse`, discard, the `operation_probe_script` sentinel, and an unreachable ssh remote with an askpass that would leave a marker: fails fast, git's words, no marker). The app layer reads `hop=backend_route family=git_write backend=<shell|helper>` (logged by `RoutedBackend` for every call) and is UNVERIFIABLE until the harness can drive the code-review git dialog; `lab_git_write_remote_mirrors_the_rust_scripts` (self-test) pins the script text and the log line to the Rust source. NOT run: the lab arms need the box lab (`nohelper` is the shell backend, the primary design; `upload` needs `APPCI_LAB_HELPER_BIN`).

Robustness cases (`appci/cases_robust.py`, #1468; all FULL tier, none in SMOKE; both modes; Linux unless noted). `session_restore` also runs on macOS and
in the Windows Git Bash pane (#1540): there the app is stopped BY PID (`stop_for_restart`: SIGTERM then SIGKILL / taskkill) after a 3 s session-writer settle, because
no quit key is usable, so it proves the restore of a hard stop and says so in a note; tab keys are Cmd/Ctrl+N (`tab_key`), cwds are read with `pwd -W` on Windows. The quit and file-open ones
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
- `flood_typed_line` (60): an endless `yes | head` / `seq` producer loop (r18e4: it runs until the case touches a stop file after the typing, and at least 2e6 / 5e6 lines (scaled by `APPCI_SCALE`) and 4 s, so it outlasts the typing on any machine) with a line typed at 10-25 ms/key + Enter while
  it runs: the line runs exactly once, only after the flood (shell-side timestamps), rec.log shows exactly [flood, typed line]; fails if the
  flood ended before the typing did (then it tested nothing).
- `flood_typing_latency` (61): tab A floods (endless `seq` loop until the case touches a stop file after the typing), tab B reads raw keys at the pty (`ts_reader.py`): 80 chars at 15 ms/key arrive
  complete and in order; gap p50/p95/max are metrics, a stall above `APPCI_FLOOD_STALL_MS` (1000) fails. No reference terminal is measured, so
  no latency verdict against one is made.
- `flood_scrollback_find` (62): (r18u23: the marker is the output of its OWN command, printed and proven to have run before the flood; the earlier same-block form lost it to the 50,000-row block cap, see the r18u23 notes) a random uppercase marker is printed before a flood of at least 300k lines AND 3 s (r18e4: a fast macOS finished 300k lines before the screen scrolled); control: it is NOT on screen after
  the flood; `ctrl+shift+f` + the marker + Enter must bring it on screen (OCR) and move the view off the flood's tail (an OCR line of just
  499999/500000, never the flood command's `seq 1 500000`: see the r18u27 note). On a miss it keeps looking for 12 s (diagnosis only) and quotes
  the app's `hop=find_scroll` lines, with the app's own placement of the match (r18u29 note). Agent mode SKIPs when Find matched no block
  at all (every hop `result=no_match`: the marker's block is gone from the pane; owner 2026-11-05). SKIPs without tesseract (package `tesseract-ocr`).
- `editor_edit_save` (70; also Windows, #1544: `phosphor.exe file:///C:/...` is forwarded to the running instance over its single-instance named pipe, `cases.open_file_in_windows_app`; the second process is killed by exact pid after 30 s if it does not hand over): opens a file in the code editor (`<bin> file://PATH` forwarded over the app's D-Bus session; the opener has no
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
  **#1780 (r18u23 Wayland terminal soak: "295/660 commands arrived exactly", RSS 365 -> 157 MB).** The 295 was a harness artefact: the check compared the
  effect file position by position, so ONE lost command (minute 27's throw-away-tab token; 26 minutes x 11 + 8 typed + 1 pasted = 295) shifted all 365 later ones.
  The case now reports `soak_diff` (arrived as a multiset, the MISSING commands by name, unexpected lines, whether order held). The RSS drop is the part that
  could be an app fault (a restarted `terminal-server` helper looks exactly like it) and the evidence cannot tell: the job timed out and uploaded no logs. So each
  minute `soak` now records the app's process set (a change is a note and the `soak_pid_changes` metric), scans that minute's app-log lines for panic / SIGSEGV /
  crash / helper-exit / "Session abandoned" (`soak_alerts.log` in the run directory, metric `soak_log_alert_minutes`), waits up to 10 s for the throw-away tab's shell
  prompt before typing (typing still happens either way) and, when its token is missing, notes whether the prompt was up (`soak_lost_tab_tokens`) and keeps a
  screenshot. The next Wayland soak decides it: a pid change plus a log line = an app restart bug to file; a missing token with the prompt up = a lost key in a new
  tab (owner rule: never), with no pid change = the harness was not at fault and the app is. Unresolved until then: the cause of the single lost token and of the RSS drop.

#### r18e0 macOS validation follow-up: test-accuracy rules

- **Close key (`APPCI_KEY_CLOSE`).** Phosphor's close-pane/tab shortcut on macOS is `cmd-shift-W` (`app/src/util/bindings.rs`, `CloseCurrentSession`; it is also bound to
  CloseWindow), and the Mac backend refuses every Cmd+W chord by policy, so `mac/ci/app_test.sh` keeps the value EMPTY. A case must never pass the empty value to
  `be.key()` (that raised `no key code for ''` in `soak` / `flood_typing_latency`): `soak` SKIPs with the reason, `flood_typing_latency` ends its throw-away tab with
  Ctrl-D and says so (unverified that the tab goes), `editor_memory_7mb` notes "release on close NOT MEASURED" (the open-cost half is still judged).
- **Reference measured in blocks, never after.** `ctrl_c_d` (Ctrl-C under flood), `latency_enter_ssh` (both windows hold an ssh session) and
  `latency_first_key_after_idle` (block A/B per gap since #1160; now labelled `interleaved`) use the A/B/A/B block design of `latency_enter` / `latency_paint`
  (`cases.run_interleaved`, #1514). Where the reference cannot be kept open next to Phosphor the row is NOREF with the reason (`metrics.interleave_fallback`),
  never a verdict against a reference measured after Phosphor. Every latency record now carries `per_side` (n and the bootstrap SE of each side's median) next
  to `overhead_se_ms`. `latency_first_key_after_idle` passes `implausible_neg_ms` (`APPCI_IMPLAUSIBLE_NEG_MS`, default 5): Phosphor more than that many ms FASTER than
  Terminal.app beyond 2 SE (r18e0: -23.7 / -36.8 ms) is a mis-measured reference, reported NOREF (`implausible: true`), never PASS.
- **A pass rests on the claimed evidence.** `Result.unverifiable()` (status `noref`) is used when the evidence is missing. (`ssh_failing_not_local` no longer needs a
  `kh_hold_entry`: since 2026-11-04 a key typed after ssh owns the tty is not held.) The trace itself is enabled by
  `RUST_LOG=warp::input_order=debug` + `WARP_INPUT_ORDER_LOG_TEXT=1`, set by `app.py` (`MacApp.start` passes them through `open --env`, `WindowsApp.env`, Linux `App`).
  `ssh_hanging_host_not_local` polls the ssh client by `-p <port>` and logs ssh first-seen / exit time next to the first and last MUSTNOT key: a line typed after the ssh
  already exited (a refused host) is UNVERIFIABLE (running locally is correct), a line typed while it was alive is judged, and no `kh_hold_*` line after it is UNVERIFIABLE too.
- **International text.** `intl_wide_*` messages print the UTF-8 hex beside the repr: `repr()` shows a real U+200D as the text `\u200d`, which in r18e0 looked like
  a typed backslash sequence (and `intl_wide_paste` never touches the injector). `htype2` `t:` sends one key event per Unicode scalar (not per grapheme cluster), so the
  ZWJ scalars of a family emoji are sent as themselves. The shell's line is compared as exact text and the file as exact bytes. Unverified on a Mac until the next run.
- **macOS coverage gaps, precisely.** `editor_edit_save` / `editor_paste` need the Linux D-Bus URL forward (a second `phosphor <path>` process) and the Linux key set
  (macOS: Cmd-S/Cmd-V, close = cmd-shift-W); `quit_24_tabs` needs the quit key and /proc (Cmd-Q is refused). `session_restore`, `crash_kill9_restore`, `crash_corrupt_db` relaunch by pid (#1540/#1541). They SKIP with those reasons (`cases_robust.SKIP_EDITOR_WHY`, `SKIP_QUIT_WHY`); they are gaps, not "not applicable".

#### Latency budgets and regression checks (`latency_enter`, `latency_key_log`, `ctrl_c_d`; Windows `latency_enter`)

The latency cases are pass/fail on the owner's budgets (3.6, `perf-budgets`), not CI-sized bounds. Every record is in
`results.json` under `metrics.lat` and in the `summary.md` latency table (case, Phosphor median/p95, reference median/p95, overhead,
budget, previous build, baseline, verdict, n).

- **Budgets.** Phosphor *overhead* = Phosphor median minus the reference terminal's median, measured in the same session with the
  same keys, cadence and timestamp command: Enter to command start <= 2 ms (`APPCI_ENTER_BUDGET_MS`), key to paint <= 2 ms
  (`APPCI_KEY_BUDGET_MS`), Ctrl-C <= 20 ms (`APPCI_CTRLC_MAX_MS`; `ctrl_c` is a plain `sleep`, the control; `ctrl_c_flood` is the output flood of 3.6, #1514). **v0.1.17 exception (owner 2026-11-04, #1606):** `ctrl_c_flood` is judged at 45 ms (`APPCI_CTRLC_FLOOD_MAX_MS`, constant `CTRLC_FLOOD_RELEASE_MS` marked ACCEPTED_FOR_RELEASE in cases.py) for this release only; it returns to 20 ms after v0.1.17. **macOS Enter overhead is REPORT-ONLY (owner 2026-11-05, #1606 closed as macOS-bound: Phosphor beats Warp on the Mac, key->screen 33.3 vs 36.7 ms, Enter->first paint 36.6 vs 76.5 ms):** on macOS `latency_enter` and `latency_enter_ssh` still print the overhead, the OS split and the app share, but an over-budget overhead is a `REPORT-ONLY` note (reason `macOS-bound; #1606 closed by owner 2026-11-05`), never a FAIL, and not time-limited. Guards that still FAIL on macOS: the app's own share (`enter_begin` -> `pty_write`) over 2 ms, and the absolute Enter->start median over the 80 ms sanity bound. Linux keeps the judging below. **v0.1.17 KNOWN exceptions (owner 2026-11-04; all `ACCEPTED_FOR_RELEASE` in cases.py, same machinery and version window as the #1607 editor open cost: KNOWN while app/Cargo.toml is at or below 0.1.17, FAIL again after, and the offline self-test fails until re-decided):** (1) `latency_enter` `enter_to_start` overhead over the 2 ms budget, both modes, Linux (Wayland/X11) only (macOS is report-only, above), #1606; (2) `ctrl_c_d` `ctrl_c_flood` overhead over the 45 ms flood budget, both modes, #1653 / #1606. Each is reported as `KNOWN: ... [ACCEPTED_FOR_RELEASE #N v0.1.17 ...]` with every number, note and metric still recorded. Everything else in those cases still fails normally (lost keys, sample counts, injector accounting, the 80 ms Enter sanity bound, regressions vs the previous/baseline build, the plain-sleep `ctrl_c` control at 20 ms, the M3 VOID logic); `latency_paint`, `latency_enter_ssh`, `latency_first_key_after_idle` and all other cases are unchanged.
  **Also KNOWN at v0.1.17 (owner 2026-11-04, #1658, `ACCEPTED_FOR_RELEASE['tabs_streaming_reorder_1658']`, Windows only):** the `N/M exact lines` verdict of `tabs_streaming` and `tabs_two_windows` (`streaming_exact_lines`) is `KNOWN` with its numbers (lines exact, lines missing) when the young-hold reorder puts lines out of order, but ONLY if the app log since the typing started has no `typed after Enter were discarded` / `was discarded` line (a discard is a real loss and FAILs, as does an unreadable log). Any other platform and any release after 0.1.17 FAIL; design pass after the release.
  Every one of the three is judged on overhead over the reference terminal (#1486): `ctrl_c_d` types the identical trap line in both
  terminals (macOS: zsh `$EPOCHREALTIME`, so no `python3` start inside the measurement; the old `~/bin/now` helper cost ~30 ms and
  was in every absolute number; Linux: `date +%s.%N`), `latency_paint` runs the same kl3 post/capture against Terminal.app's window
  (macOS only; elsewhere the case skips, there is no read-back to compare). The absolute Phosphor number stays in `metrics`
  (`ctrl_c_ms`, `key_to_paint_ms`) and includes the input-tool spawn. With no reference the case notes `no reference: absolute only`
  and the verdict is `NO-REFERENCE`, never pass: the CASE status is `noref` (its own count in `results.json`/`summary.md`/the collected INDEX as `+N no-ref`, a JUnit `<skipped message="NO-REFERENCE ...">`, never a pass; it does not fail the job, but no latency budget was judged). The CI sanity bounds (Enter 80 ms, paint 120 ms) still FAIL independently. A real failed check wins over `noref`. UI work: the app's slow-op log (#1146) only emits >= 8 ms, so `PASS` means no item >= 8 ms
  while typing and `WARN` means >= 16 ms (a dropped frame); the 4 ms line itself cannot be seen from the log.
- **Overhead verdicts are noise-aware (#1583).** The record carries `overhead_ms` and `overhead_se_ms`, the bootstrap standard error of the median
  difference (fixed seed). `FAIL` only when `overhead - 2*SE > budget`; `PASS` only when `overhead + 2*SE <= budget`; anything between is
  `INCONCLUSIVE` (reported with the numbers; the case status is `noref`, never a pass; take more samples). n=30 medians with a 12 ms spread
  have a ~2.3 ms SE, larger than the 2 ms Enter budget, so `latency_enter` now takes 60 samples (`ctx.count(60)`). Pane and reference shells
  are recorded as case metrics (`pane_shell`, `reference_shell`) and in the latency record (`pane_shell`, `reference_shell`); when they differ the verdict is `NO-COMPARE` (never PASS/FAIL).
  macOS `latency_enter` stamps with the pane shell's builtin when it is zsh (`zmodload zsh/datetime; print -r -- $EPOCHREALTIME`), the same line in
  Terminal.app; only bash 3.2 keeps the perl stamp.
- **Thin evidence is never a pass (#1514).** A budget `PASS` is withheld (the verdict becomes `INCONCLUSIVE`, case status `noref`) when the Phosphor series has
  fewer than `APPCI_MIN_SAMPLES` (30) samples, fewer than 90 % of the samples the case asked for, a time quantum above half the budget (`latency_paint`: kl3
  samples at 240 Hz, a 4.17 ms quantum against the 2 ms budget, so it is `resolution-limited` and can only FAIL or be INCONCLUSIVE until the tool samples at
  >= 1 kHz), or a half-to-half drift of either series (first half vs second half median, beyond 2 SE and half the budget; Phosphor and reference are
  measured one after the other, not interleaved A/B/A/B, so slow drift is detected, not cancelled; the record carries `drift_ms`). A FAIL is never softened.
  The reasons are in the record (`insufficient`, `notes`). Latency cases use `ctx.count_lat(n)`: the scale (`APPCI_SCALE`) can no longer take them below
  `lat.min_n()`; `latency_enter` still hard-fails below 80 % captured. `ctrl_c_d` also runs `ctrl_c_flood` (unix panes, not the fix tier): the same trap line but the
  interrupted command is `yes` writing to the terminal, Phosphor vs the reference, against the same 20 ms budget; the plain-sleep `ctrl_c` is its control.
  The Windows reference runs `powershell.exe -NoLogo -NoProfile`, like the pane. The PowerShell pane has no `rec.log`: `fast_typing`
  compares the lines PSReadLine's history file recorded with the typed ones (exact, FAIL on a mismatch); when the history has a different line count (file
  missing, PSReadLine filters lines containing password/token/secret) it only notes `command text NOT verified` and the metric `command_text_unverified_<tag>`.
- **Baselines and build-to-build (#1513).** `prev.py` takes the previous FINISHED job (success or failed) on a `build/*` ref that has a `results.json`. A previous
  record with n < `APPCI_MIN_SAMPLES` is ignored (`prev: ignored`), since a crashed run is not a representative reference. `baselines/*.json` stay `pending`
  (the baseline column says so; nothing is checked against it) until a run is accepted: `python3 script/realapp/appci/baseline.py check RESULTS.json` lists
  why a run is not promotable, `... promote RESULTS.json PLATFORM` refuses unless the run is recorded as full tier (`meta.tier`, `meta.scale` = 1), has no
  failed case, every latency record n >= 30 and no FAIL / INCONCLUSIVE / NO-COMPARE / NO-REFERENCE verdict (`--force` overrides). The owner reviews the numbers in
  `summary.md`; the coordinator promotes and commits. `summary.md` ends with "Changed since previous build" (`APPCI_PREV_RESULTS`): cases that went
  pass->fail (NEW-FAILURE), fail->pass (FIXED), vanished (MISSING), appeared (NEW-CASE) or changed otherwise. Informational, no exit-code change.
- **Where the Enter time goes (#1583 W3).** From the app's own `input_order` log `latency_enter` records `metrics.app_segments_ms`
  (per platform): `enter_to_write_ms` (`hop=enter_begin` to the `pty_write` of the line), `write_to_first_read_ms` (that write to the first
  `pty_read`, the app's `since_write_ms`), and `write_to_preexec_ms` only if the log carries a `hop=preexec_received` line. The app does not
  log one today (Preexec is only stamped into an atomic), so that segment is absent and the case says an app log hop is needed.
- **Burst reference (Windows, #1583 W1).** After the per-key reference samples, `latency_enter` measures the conhost reference again with the stamp
  line delivered as one 0 ms burst then Enter (`reference_burst_enter_to_start_ms`, `reference_burst_minus_per_key_ms`). A burst median more than 5 ms above the
  per-key one is noted as "the gap is shell ingestion of a burst" (Phosphor also writes the line in one pty write). It does not change the verdict.
- **Bootstrap-sourced reference (Windows Git Bash pane, #1583 G2).** After the plain reference, `latency_enter` opens a SECOND conhost whose bash sources the plain
  reference rc and then Phosphor's own bash bootstrap (`bash.sh` with `bash_body.sh` included and its placeholders filled in, `profile.compose_bash_bootstrap`,
  written as `<home>/.refboot.sh` + `.refrc_boot`) and measures the same stamp line (`reference_bootstrap_enter_to_start_ms`,
  `reference_bootstrap_minus_plain_ms`): the cost of the pane's shell hooks alone, outside the app. Never a verdict input; a note when the assets are not next to the harness.
- **Bracketed-paste cost (macOS zsh pane, terminal mode, #1583 M3).** After the Phosphor samples the case types `unset zle_bracketed_paste` (confirmed by a state
  file), measures a second batch of the same stamp line (`unset_bracketed_paste_enter_to_start_ms`, `unset_bracketed_paste_minus_default_ms`) and puts the parameter back
  (`zle_bracketed_paste=( $'\e[?2004h' $'\e[?2004l' )`, confirmed). Not a verdict input. **#1606 rediag:** until this fix the interleaved
  run ended on a reference block and left the injector aimed at Terminal.app, so every M3 key went to the reference (r18u8/r18u13: ktap
  counted 111 "elsewhere" Returns = 75 reference + 35 M3 + `exit`), and the reported "-15.8 ms" was the reference's own median. The case now
  switches back to Phosphor after interleaving, refuses to run M3 while the guard is aimed at the reference, and counts Phosphor's
  `hop=enter_begin` lines during the batch (`unset_bracketed_paste_app_enters`); fewer than the samples marks it `unset_bracketed_paste_void`
  and no difference is reported.
- **Where a macOS sample starts (#1606 rediag).** The Mac backend runs its frontmost-window guard inside `key()`, and for Phosphor the guard
  verifies the PID with a `ps` spawn that the reference's guard (a name compare) never pays: ~3-6 ms of harness work inside Phosphor's samples
  only (`enter_split_ms.*.t0_to_inj_ms` shows it per side). A sample now starts when the injector is spawned, after the guard, on both sides
  (`metrics.enter_sample_start` = `injector_spawn`); `APPCI_ENTER_T0=harness` restores the old start (`t0`). Absolute Mac numbers drop by the
  guard time on both sides, so a build-to-build compare across this change moves for that reason alone. Linux and Windows are unchanged.
- **OS split of the Return (#1606 rediag, macOS).** Every Mac sample keeps the injector's post time on the OS uptime clock (htype2's K line)
  and its clock pair; `latency_enter` joins it with the run's `ktap.log` Return (the OS event timestamp and the tap's callback time) and, for
  Phosphor, its `app_send_event` dequeue, as `metrics.enter_os_split_ms` {phosphor, reference, delta}: `post_to_evts` (injector post to the
  OS's event timestamp: the window server taking the event in), `evts_to_tap`, `evts_to_app` (Phosphor only) and `evts_to_lf` (event to
  zle-line-finish: all the terminal and the shell do). r18u8-r18u16 showed ~50 ms post -> zle-line-finish on BOTH sides and a Phosphor
  excess the app's own log cannot see; this says whether it is before or after the OS stamps the event. Self-test `g11_enter_rediag_1606`.
- **Ctrl-C under flood on macOS (#1838): new app hops, no harness change.** With `RUST_LOG=warp::input_order=debug` the app now logs, for a Control key-down, `hop=mac_ctrl stage=key_down_impl_entry|after_interpret_key_events|after_handle_view_event since_send_ms=` (time since `-[NSApplication sendEvent:]`; a `drawable_wait` between two stages places a frame inside the key handling); after the `^C` reaches the pty, `hop=ctrl_c_frame since_write_ms= since_precmd_ms= presented= after_precmd=` for each presented frame up to the first one after the prompt hook; and `hop=frame_skip_run skipped= held_ms=` when the blocked-frame gate (kill switch `PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES=0`) skipped frames. Unit tests: `frame_latency_tests.rs` (gate decision, skip run) and `input_order.rs` (ctrl-c frame watch). Not yet parsed by appci; read them from the log of the next Mac `ctrl_c_d` run.
- **Ctrl-C under flood, gate on vs off (#1838 owner decision): `ctrl_c_flood_ab`.** `appci/cases_ctrlc_ab.py`, macOS only (SKIP elsewhere; `winmap.NOT_WINDOWS`), in `cases.PROVING` so it runs in the fix tier. The blocked-frame gate reads `PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES` once at startup (`OnceLock`), so the arms cannot switch inside a running app and two simultaneous instances are not supported (`MacApp` refuses a second launch, one window/ctl dir/guard); the case therefore RELAUNCHES the app on the same profile before every block with the variable set to `0` (off arm, via `MacApp.extra_env` -> `open --env`) or unset (on, the default), in the A/B/A/B block order of `lat.block_schedule` (`APPCI_LAT_BLOCK`, same n and warm-up per arm, first sample of each block a discarded lead), using the `ctrl_c_d` flood sampler (`cases.ctrlc_samples(flood=True)`). Metrics `ctrl_c_flood_gate_on` / `ctrl_c_flood_gate_off`: n, p50, p95, max, `frame_skip_runs` (count of `hop=frame_skip_run`), `held_ms_max`, `held_ge_100ms`, `drawable_wait_ge_2ms`, `os_key_event_to_ctrl_c_keydown_p50` (ctrl=true `os_key_event` to the next `hop=ctrl_c_keydown`), `flood_bytes_per_s` (sum of `pty_read bytes=` in runs of >= 10 reads less than 250 ms apart, over the runs' span; both arms measured identically, so the ratio is what matters); plus `ctrl_c_flood_gate_delta_p50_ms` and the note `gate on vs off: Δp50 = ...` (on minus off, negative = the gate helps). WARN notes: ON throughput more than 10 % below OFF, any `frame_skip_run held_ms >= 100` (the watchdog fired), and an OFF arm that still logged `frame_skip_run` (the switch did not reach the app). Fails only on harness problems (a relaunch with no prompt, fewer kept samples than asked). The ON arm against the budget / Terminal.app reference is still judged by `ctrl_c_d` / `ctrl_c_flood`, unchanged. Self-test `ctrl_c_flood_ab_pure_parts_1838` covers the log parsing, merge, verdict and block order. Unverified until the first Mac run (no cargo/Mac here).
- **Shell-side split for bash/zsh panes (#1492), opt-in.** The appci bash rc appends to `PS0` (`PS0+=`, so a system PS0 is kept) a
  no-fork record of when readline accepted the line (`$__pa_lf`, integer microseconds, bash >= 5.0; the zsh rc already records
  zle-line-finish). With `APPCI_ENTER_SPLIT=1`, in a Git Bash pane or a Linux pane identified as bash/zsh, the stamp line becomes
  `date +%s.%N > lat_N;echo $__pa_lf>>lat_N`, and `metrics.enter_split_ms` gains `t0_to_lf_ms` (harness Return to line accepted: delivery,
  terminal, pty write, line ingest) and `lf_to_stamp_ms` (line accepted to the stamp: the preexec hooks and the `date` start) for Phosphor,
  the reference, and their `delta`; it decides whether the Windows Git Bash gap is burst ingest or Phosphor's hooks. It is OFF by default
  because the line is 23 characters longer and Phosphor writes the whole line at Enter, so the extra ingest lands inside Phosphor's
  Enter->start samples: split runs are not comparable with default runs or baselines. The two fields are separate writes, so each sample
  waits up to 1 s for the second; samples without it keep their stamp and are counted in `metrics.lf_missing`. `metrics.stamp_line`
  records the line used.
- **Verdicts.** `PASS`, `FAIL`, `WARN`, or `NO-REFERENCE` when no reference could be measured the same way (never shown as pass).
  `latency_key_log` is `NO-REFERENCE` everywhere today: it reads the app's own `input_order` clock (keydown to first frame) and no
  reference terminal exposes a comparable one; absolute numbers and the old CI sanity bounds (p50 <= 20 ms, p99 <= 60 ms) still apply.
  The app logs a `frame` line only for the first frame after pty output that followed a pty write, so keys typed at a prompt (they
  go to the input editor, not the pty) never produce one (#1486: why the case once found 0 pairs among thousands of `keydown` lines).
  The case types into a running `cat > /dev/null` instead, pairs each plain `keydown` with the first following `frame`
  (`key_frame_pairs`), and on failure reports the keydown/frame/pty_write/pty_read counts of its own window with a diagnosis.
- **Reference terminal** (same session, Phosphor measured first, reference second, same rc and cwd): Linux Wayland = `foot` if installed,
  else `xterm` through XWayland (recorded in the `reference` name); Linux X11 = `xterm`; macOS = Terminal.app, driven through the same
  cliclick/osascript path under the frontmost-app guard (the guard is retargeted to Terminal while its window is driven, no
  Cmd-Q/W/Tab/Space, focus returned to Phosphor). The reference is a `.command` file opened with `open -a Terminal` (no Apple Events, no Terminal preference changes). At harness start the harness records whether Terminal.app was already running (exact PID from `ps -A -o pid=,comm=`, `appci/refterm_mac.py`); if not, the Terminal.app the run launched is recorded as `terminal <pid>` in `<run>/pids` and quit at the end of the run by that PID (SIGTERM, then SIGKILL), which closes every reference window (#1485). #1826 (round 6): the reference `.command` is opened ONLY by `refterm_mac.launch` (`open -a Terminal <cmd> --args -ApplePersistenceIgnoreState YES`: a per-launch argument, no preference change), because Terminal restores the windows it saved when SIGTERM'd and each relaunch brought back all earlier reference windows (one Terminal owned 276 `login` shells; measured on the Mac: 137-161 logins after a plain relaunch, 1 with the flag; an already-running Terminal ignores the arguments and is never ours). After EVERY case on macOS `run.py` calls `refterm_mac.case_guard`: a harness-started Terminal.app still running with `login` children is quit by PID and the case row carries `reference Terminal.app leaked by this case` (that names the leaking case). Self-tests `refterm_single_launch_path` (greps the harness for any other launch path) and `refterm_mac_bookkeeping`. The same quit also runs at the end of EACH reference block (`RefTerm.close()` -> `refterm_mac.quit_launched`, after the measurement and the shell's `exit`; Terminal keeps the window of an exited shell, so one window per probe used to pile up to ~250 per run), so the next block launches a fresh Terminal.app; the end-of-run quit prints `teardown: Terminal.app reference windows left open by this run: NONE` (or `FAIL, pid N still running`), and `teardown.sh` prints the same audit line. Which PID is ours is the pure `refterm_mac.pick_ours` (single Terminal started after the harness, never the one running at harness start; self-test `refterm_mac_bookkeeping`). `teardown.sh` does the same for a cancelled job, only if that PID started after the run dir was created. An already-running Terminal.app is never quit: the case result carries a note "reference windows left open: Terminal was already running (pid N)"; Windows = `powershell.exe` in conhost, same SendKeys path.
- **Samples.** n >= 30 per measurement (`ctx.count`, so `APPCI_SCALE` smoke runs are marked "indicative only"), the first
  `APPCI_WARMUP` (3) samples discarded, median and p95 reported.
- **Regression rule.** The Phosphor median is compared with (a) the previous finished run (success or failed, #1513) of the same job on a `build/*` ref
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

**Wayland key injection: persistent keyboard (#1414, wire-trace 2026-11-03).** The lost first character (and the lost whole key in key-only
calls such as ctrl+c or a bare Return) is an *injector* property: each `wtype` process creates a new virtual keyboard, so the seat's keyboard
capability flips 0 -> 2 -> 0 per call, and wtype's first key press is sent before any client has bound the new `wl_keyboard`. The old
per-call-keyboard behaviour is therefore a harness property, not a Phosphor result. `wayland_session.sh` now starts ONE long-lived
`wtype -s ...` (sleep only; it keeps its virtual keyboard for ~11 h) right after sway is up, records it in `pids` as `kbd-holder` and kills it
by exact PID in teardown. Clients bind `wl_keyboard` once, before any test key, for Phosphor and for the reference terminal alike (same
session). The session exports `APPCI_WL_INJECTOR=persistent|percall` (percall = holder failed to start, or `APPCI_WL_KBD_HOLD=0` forces the
old behaviour for reproducing the artifact); `key_loss_control` reports it as `metrics.key_loss.injector_mode` and a note. With `persistent`,
the "perline-unprimed" variant measures Phosphor with a stable keyboard and no priming; with `percall` it reproduces the artifact.
The per-call race had a Phosphor-side part too: foot kept every first character with the same per-call stream (r18d2: foot 0/12,
Phosphor 12/12), and the wire trace shows why -- winit queued `get_keyboard` and then built the keyboard's xkb context and Compose table
(~2 ms) before the dispatch flushed it. The vendored winit now flushes right after `get_keyboard` (`lib/winit/CHANGES-PHOSPHOR.md`). To
check it, run the Wayland job once with `APPCI_WL_KBD_HOLD=0 APPCI_CASES=key_loss_control`: "perline-unprimed" must lose no more first
characters in Phosphor than in foot (the case's own Phosphor-vs-reference check); the default persistent session cannot show it.

**Injector accounting, all keys (#1509).** Every case on Linux (wayland/x11) and macOS now ends with an informational note `injector accounting: injector sent N;
app saw M; lost-before-app K (Enter: sent a, app saw b)` and a `metrics.injector` record. `sent` is the backend's own record of what it posted
(`<run>/inject.log`: Linux one line per logical `wtype`/`xdotool` call with the typed-character COUNT and every named key; the wtype priming Shift tap and
modifier presses are not keys; a call that errored posted nothing); `saw` is the number of distinct physical keys in the app's `input_order keydown` /
`typed_characters` lines (shared `seq`) in the case's wall window (`appci/injacct.py`). `lost-before-app` is therefore lost between the injector and
the app, never inside Phosphor's queue; keys aimed at a reference terminal are excluded. It only counts, so it cannot see a reorder or a changed
character (that is `cadence`/`fast_typing`'s exact-line check); on `key_loss_control`/`enter_loss_control`/`fast_typing`/`ssh_connect_typing`/`cadence` a non-zero
loss is prefixed `WARN loss case` so it is read next to the case's own verdict. On macOS `ktap` now also logs every non-Return key-down (`K down ...`,
key codes only) and `enter_route.py` prints `ALL KEYS: injector sent N; macOS delivered M to Phosphor, K to other processes; never in macOS L`.
Windows keeps its stronger `SendInput` accepted-count record.

**Accounting accuracy (r18d8 validation).** (1) An Enter the input editor handles logs NO `keydown key=enter`; its app-side evidence is `hop=enter_begin
deferred=false` (`terminal/input.rs`), so `app saw` and `Enter: ... app saw` now add those hops (a `deferred=true` re-run is not a new key; an Enter that
went to the pty logs the keydown instead, the two are disjoint). Before this every row read "Enter: app saw 0" and `lost-before-app` was inflated by the Enter
count. (2) A paste chord (ctrl+v, ctrl+shift+v, shift+Insert) posts no per-character keys: it is not counted in `sent`, and a paste-only call reads `injector
posted N paste chord(s) ...: per-key accounting n/a` instead of "saw 0". (3) `keydown_count` counts what the app logs per injected key (`cases.key_event_audit`):
a `keydown`, or a `typed_characters` with no keydown (wtype-typed symbols), or an editor Enter's `enter_begin` hop; the failure text splits the three. It still
fails on a real loss or on an extra/doubled key. (4) Injector health (Wayland): `wtype` stderr is kept in `<run>/logs/injector-stderr.log` and quoted in the
`key injector failed: wtype exit N (stderr: ...)` error; before every case `WaylandBackend.check_injector` verifies the persistent keyboard holder (#1414) is
still there (sway's keyboard-input count; the holder's own log `<out>/kbd-holder.log`), and if not it restarts one (stderr in `logs/kbd-holder-restart.log`) and
adds the note `injector died: ... (key loss in this case is the INJECTOR's, not the app's)` to the case, so a dead injector is never read as app key loss.

**`ssh_failing_not_local` (r18d8; expectation changed 2026-11-04).** Since the plain-terminal ssh pass-through (DECLINED.md, "A FAILING ssh behaves like a plain terminal too") the case asserts the iTerm2 / plain-terminal result: `MUSTNOT` typed behind the failing ssh runs locally EXACTLY ONCE, exactly as typed, and only after the ssh failed (file mtime >= the failure); no hold hop is expected. Before, it asserted the line never ran locally. The target is a local TCP listener that accepts and never speaks (`cases.HangingTcpServer`), so the ssh is provably still
CONNECTING (TCP accepted, connection open; ConnectTimeout 6 s) when `MUSTNOT` is typed and fails only afterwards (`Connection timed out during banner
exchange`). r18u13: the listener READS (and drops) what ssh sends; it used to `MSG_PEEK`, which kept seeing ssh's unread banner after ssh had closed the
connection, so the failure time was the wait's 14 s timeout and a line that ran after ssh exited was reported as running ~9 s BEFORE it. The old `.invalid` host could fail in tens of ms, before the first typed key, when running the line locally is correct. The case fails as a HARNESS
problem if the ssh was not connecting at that moment, and notes the app's `kh_hold_entry` line for the first MUSTNOT key (`holding=...`, `gate_drops=...`), the
first `kh_hold_exit` / `kh_iq_release Q7_ssh_typeahead` after it and the time the ssh failed. The enforced claim: text typed while an ssh connects and then
fails is NEVER run locally (handed back unexecuted, #983/#1292).

**ssh cases with no remote shell (r18d8).** `ssh_connect_typing`, `ssh_nested_typeahead`, `remote_session_*` and `latency_enter_ssh` quote `ctx.ssh_diag()` on a
missing remote shell: the last 8 `sshd.log` lines (the test sshd now runs `LogLevel DEBUG1`, so connection/auth/session lines exist; an empty log means ssh never
reached it), whether the test sshd was listening on its port (and its exit code if not), a fresh `ssh -vv fake true` client probe in the app's environment (exit
status and stderr tail; full text in `logs/ssh-client-probe.txt`), and the app's ssh hold / hand-over hops of the case (`kh_iq_hold/release Q7_ssh_typeahead`,
`kh_hold_entry/exit`, `ssh hold:` lines). Pass criteria are unchanged.

**Where a Return went (Mac, `ktap` + `enter_route.py`, #1482).** For the whole Mac run, `script/realapp/mac/tools/ktap.swift` (compiled at job
start like htype2) sits at the *annotated* session event tap, where macOS has already chosen the target process, and logs each Return
keyDown/keyUp (key code, the event's own timestamp, target and source PID; never characters), passing every event through unchanged. After the
run `script/realapp/mac/ci/enter_route.py` joins that with Phosphor's `input_order hop=mac_enter stage=app_send_event ts=` lines by timestamp and
writes `run/logs/enter_route.txt`: Returns targeted at Phosphor that its NSApplication never saw (lost between macOS and the app), Returns targeted
at another process (by name), and a join check. ktap uses the Accessibility grant the job already has; if the tap cannot be created the run goes on
without it and says so.

Enter-loss control (`enter_loss_control`, terminal mode, SMOKE right after `key_loss_control`, #1482): on the Mac `fast_typing` once
merged two lines (`...r_ft0.txtecho t28...`): the app log has no `kind=enter` for line 27, which cannot say whether the Enter reached the
app. Two additions make the harness able to tell. (1) The Mac backend keeps every htype2 call's K-lines (htype2 overwrites its outfile per
call): `<run>/inject.log` gets one line per call (counter, wall-clock span, typed-character COUNT, ENTER/KEY/BS lines with wall times; typed
characters are never logged, they may be passwords), and `fast_typing`'s failure message cites for the first bad line whether the injector
posted its ENTER and when. (2) The control types the exact `fast_typing` burst (30 lines, 10-25 ms/key, instant Enter, gaps 0 and 0.25 s)
into the reference terminal's shell (Terminal.app / foot / xterm) in the same session and counts lines missing from the file
(`metrics.enter_control`, summary.md "Enter-loss control"). It is its own case, not a `key_loss_control` variant, because it needs the
30-line fast_typing cadence and measures Enter, not first-character, loss. Verdict rule: a `fast_typing` run that ends with merged lines
fails as an APP bug only if the reference kept every Enter at the same gap; if the reference also lost lines it is reported as "injector/OS
lost Enter (reference too)" (a note plus `metrics.injector_lost_enter_*`, no failure); with no reference terminal it fails as before,
labelled unattributable. `fast_typing` runs the control lazily when it fails and no cached result exists. The control itself fails only if
the injector log says it did not post an Enter. Linux backends keep no injector record yet.

**Where every app test gets its build: this machine, never a download.** The plain `app-test-*` jobs `need` their build job with
`artifacts: false` (ordering only) and their `before_script` is `script/ci/fetch_build.sh --this-pipeline` / `fetch_build.ps1 -ThisPipeline`:
it reads the `pipeline-<CI_PIPELINE_ID>` pointer (`pipeline-<platform>-<id>` since #1524; the lookup and the writer share one lock) that `cache_build.sh` / `.ps1` leaves in the local build cache (below), copies that build to
`dist/` and checks `SHA256SUMS`. A missing pointer or copy fails the job loudly (exit 2); it never downloads and never tests another build.
(Until 2026-11-02 the plain jobs still pulled GitLab artifacts; only the `-reuse` jobs used the cache.)

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
first line of `summary.md` (`record_reuse.py` / `fetch_build.ps1 -Record`, as `after_script`).
**Identity is verified before anything is stamped (#1501).** A cached copy's `meta.json` (job id, commit, and for `--this-pipeline` also the pipeline) must equal what `ci_seq.py` asked for (`APPCI_BUILD_JOB_ID` / `APPCI_BUILD_SHA`), else exit 2. A downloaded build is checked against GitLab's own record of the job (commit, name, status `success`; the by-ref fallback must also be the latest successful pipeline's job of that name) via `script/ci/verify_build_identity.py` / the `fetch_build.ps1` equivalent; that needs `read_api` (`CI_JOB_TOKEN` or `APPCI_API_TOKEN`), and a build that cannot be verified is refused, never tested. `APPCI_BUILD_SHA` is required. The job-token fetch itself could not be
exercised outside a running job when this was written: the first real reuse run is its test.

### 2.9 Test tiers (what a push costs)

One driver, three sizes: `python3 script/ci/ci_seq.py build/<ref> --tier fix|candidate|release [--platforms linux,macos,windows]
[--tests '<nextest filterset>'] [--skip job ...] [--no-reuse] [--dry-run]` (default tier `fix`; bare job names after the ref still mean `--skip`).
It plays only manual jobs that have not run yet (never a re-run), passes per-job variables (`TEST_FILTER`, `APPCI_TIER`) through the
play API, and ends with a table of job, status, VERDICT (PASS / FAIL / SKIPPED / INFO) and minutes, plus an `overall:` line. The exit code is non-zero when ANY
played job fails, app tests included (they are `allow_failure` in GitLab, so the pipeline is green regardless; the ci_seq verdict is the one that is not). An app
test that never ran because its build failed counts as FAIL. `skipped by tier` (smoke tier x11) is the only neutral status; the x11 job itself exits 77
(allowed failure code) in smoke so it can never be recorded as a success that tested nothing. bench/mutants/fuzz are INFO. Heavy jobs stay manual: the script is how you start them, after asking.

| tier | runs | target | when |
|---|---|---|---|
| `fix` | `check-linux`; `test-linux` with a nextest filter; `test-warpui-core` only if its crate changed; `test-workspace-rest` on every code change (#1496); builds for the chosen platforms only; app tests in SMOKE + PROVING mode on those platforms (the PROVING cases, incl. the `agent@fake` pass; on Linux also `app-test-lab-linux` with the PROVING lab cases) | ~1 h before PROVING; about 2.5 h on Linux while the PROVING list is long (section 2.9 'New tests run in the fix tier first') | every push of a fix |
| `candidate` | `check-linux`; all three suites unfiltered; all three builds; the FULL app matrix on every platform (both Linux passes); on macOS and Windows also the three unit suites (`test-<plat>`, `test-warpui-core-<plat>`, `test-workspace-rest-<plat>`), played BEFORE that platform's app tests, one at a time on the same machine (owner 2026-11-05: unit tests first); on the Linux box the Wayland/X11 app tests likewise wait for the box's three unit suites, and in the release tier every machine's heavy suites (loom, fuzz, bench, mutants) follow its app tests, so compile and mutation load never overlaps the latency and typing cases (r18u31) | ~6 h (Mac/Windows suites unmeasured) | a batch of fixes is ready; nothing is "done" until a candidate passes |
| `release` | candidate + `test-shuttle-loom`, `fuzz-smoke`, `bench`, `mutants`, `mutants-warpui-core` on the box, and the same five with the `-macos` and `-windows` suffix after each platform's unit suites (`fuzz-smoke-windows` is reported `unsupported on platform`, never played) | ~16 h (estimate; the first run records it) | before a numbered release (then also the manual checklist of section 5) |

**New tests run in the fix tier first (owner rule, standing, 2026-11-09).** Every NEW test runs in the fix tier until it has passed; only then does it graduate to the candidate / full tier.

- `PROVING` (`script/realapp/appci/cases.py`, next to `SMOKE`) is that list. A new app case is added to `PROVING`, in the same commit that registers it, and NOT to `SMOKE`. The fix tier (`APPCI_TIER=smoke`) runs `SMOKE` + `PROVING`; the candidate and release tiers run every registered case as before, so a PROVING case also runs there (the list only guarantees it is in the fix tier too). `PROVING` and `SMOKE` are disjoint, every name is a registered case, and `run.py --self-test` pins that (`proving_in_the_fix_tier`). The legacy kill-switch run (`PHOSPHOR_QUEUE_DESTINATION=legacy`) keeps its narrowed `KILL_SWITCH_SMOKE` list and runs no PROVING case.
- **Graduation is never automatic, and no code does it.** After each fix-tier run the coordinator reads the section **"PROVING cases (owner decides graduation)"** that `report.py` writes into every run's `summary.md` (case, mode, status, per platform) and that `collect_results.py` repeats in the archived README (case, platform, mode, status), and asks the owner which of the passed cases move. The owner decides; the coordinator then removes the names from `PROVING` (a case that belongs in every fix push goes to `SMOKE` instead). A skipped, failed or no-reference case stays in `PROVING`. A case that never passes stays in the fix tier until it does or is withdrawn.
- **Windows input-parity cases (docs/design/windows-input-parity-audit.md; #1884, #1886, #1887, #1888, #1889, #1892).** `appci/cases_win_input.py`, all in `PROVING`, each skipping itself off its Windows pane (`winmap.py`): `ssh_connect_typing_wrapper` and `ssh_nested_typeahead_wrapper` (Git Bash pane; run the existing ssh case bodies with the managed-ssh wrapper pinned on by `APPCI_SSH_WRAPPER=1`, which makes `profile.py` write `warpify.ssh.enable_ssh_warpification` and `enable_legacy_ssh_wrapper` explicitly, and additionally require the app log to show `hop=ssh_wrapper announced` and no `verdict_ignored`: the born-owned ConPTY window must accept the wrapper announcement); `ps_ctrl_break` (PowerShell pane; a Python probe with SIGBREAK/SIGINT handlers must get SIGBREAK, three runs, from `ctrl+break`, which `backend.WindowsBackend.NAMED` now maps to VK_CANCEL); `ps_fast_command_young_enter` (PowerShell pane; 12 pairs of `Add-Content` lines typed with no pause, each exactly once and in order). `subshell_ready_rc_questions` and `subshell_ready_rc_clear` moved from `NOT_WINDOWS` to `GITBASH_ONLY` (no tty probe on ConPTY: the readiness judge decides from the screen facts). `ctrl_c_d`'s Ctrl-C-under-flood half (`ctrlc_flood`) now also runs in the Git Bash pane on Windows (the PowerShell pane still does not). All unverified until the first Windows run; none can be run on the Linux laptop.
- **Fixed lists are not enumerated here:** read `PROVING`, `PROVING_FAKE` and `PROVING_LAB` in `cases.py`. At the rule's introduction `PROVING` holds the agent file-edit, interactive-program, latency / steer, MCP-fault, multi-tab, network-gate, fault-injection and restart cases, the `subshell_ready_*` / `subshell_sh_probe` cases and the other cases registered since 2026-11-08. `upgrade_from_v0_1_17` is deliberately not in it: it fetches a previous release and stays a candidate / release case (section 2.18).
- **Windows.** `winmap.tier_plan(..., proving)` adds every PROVING case to every pass's plan; one that cannot run in a pane SKIPs there with its `winmap` reason (a skip row, never an absent case). `FIX_TIER`'s agent subset trims only SMOKE.
- **The fake-model pass (`agent@fake`).** Most agent PROVING cases need the scripted provider (`fake_model.py`: its request log, faults, slow streams), but the CI machines carry the owner's machine-local Ollama settings and `run.py` used to start the fake only when no provider was configured, so those cases would SKIP. In the fix tier `run.py` therefore runs the `PROVING_FAKE` cases in a second, separate app launch with its own profile: label `agent@fake` (Windows: `agent@<pane>+fake`), provider = the local fake model, `APPCI_FAKE_MODEL_ACTIVE=1` only during that pass. The ordinary passes keep the configured provider and run everything else (SMOKE agent cases such as `agent_stream_cancel`, and the PROVING cases that need no model), so nothing in them changes. Nothing is ever sent to a real provider by a PROVING case; the configured endpoint is still redacted from every artifact. No fake pass is made when the run already is the fake (`APPCI_FAKE_MODEL=1`), with an explicit `--cases`, without the agent mode, or when the selection has no PROVING_FAKE case. `APPCI_FAKE_MODEL=1` now FORCES the fake even when a provider is configured (it used to defer to it), which is what the lab and replay jobs always meant. **The fake answers every purpose the app uses (round 6).** The app appends an `<environment_context>` user message after the prompt (or inside it), so every trigger, fault `match`, request-log row and follow-up check reads `user_prompt()` / `clean_messages()` (the block removed), never the raw last message (Windows agent@fake: faults hit the title request and the fake answered `OK </environment_context>`). The purpose table lives in `fake_model.py`'s header; `classify_purpose(path, body)` decides from the endpoint and the system prompt: `chat` (offers tools, or no system message; the only purpose that takes triggers and faults unless `inject(..., purpose=P)` names another), `title`, `commit_message`, `explain_command`, `compaction`, `next_command`, `nld_predict`, `prompt_suggestions`, `relevant_files`, `workflow_metadata` (each a fixed, valid, purpose-shaped answer; `"stream": false` requests get one JSON object), `show`, `tags`. Anything else (`/api/generate`, `/api/embed*`, an unmatched system prompt without tools) is `UNKNOWN_PURPOSE`: HTTP 501, a stderr line, `fake.unknown`, never answered as chat. `fake.requests` counts chat requests only; `fake.by_purpose` counts all. The app uses no embeddings or `/api/generate` on this path. Self-test: `fake_model_ignores_environment_context`.
- **The lab in the fix tier.** The `PROVING_LAB` globs (`lab_agent_remote_*`, `lab_fam_agent_*`) need the fake ssh lab (section 2.15). `ci_seq.py --tier fix` therefore also plays `app-test-lab-linux` (`-reuse` when the Linux build is reused), Linux only, after the Linux app tests (same resource group), with play-time `APPCI_CASES` = the globs, `APPCI_FAKE_MODEL=1`, `APPCI_LAB_MATRIX=quick` (27 arms at introduction: 16 terminal, 11 agent). `SSHLAB_BOX` / `SSHLAB_TOKEN` are protected project variables with scope `*` and `build/*` is a protected branch (checked 2026-11-09), so the job has them on the refs ci_seq drives. The Mac and Windows lab jobs are not played in the fix tier. A red lab job fails the tier like any app job. **The lab image is played first (2026-11-09):** the image tag hashes the lab inputs, so an edit under `script/realapp/sshlab/` invalidates it and every lab case skips with "image ... is not built". `ci_seq.py` therefore plays `sshlab-image` and waits for it before ANY lab job (`ensure_lab_image`, once per run; `labctl.py build` skips a tag that already exists, so it is cheap when nothing changed). If `sshlab-image` is not green the lab job is not played and the timeline shows `lab image not built` (FAIL); a lab job whose `results.json` has a skip row reason `image ... is not built` is also turned into that FAIL by `check_app_results`, never a green-ish skip. `--self-test` pins both.
- **Expected extra fix-tier time (estimates from per-case budgets; not yet measured, the first run records them):** Linux wayland about +85 min (fake pass ~60, other PROVING cases ~25) plus the lab job about +55 min on the same box; macOS about +50 min (the Linux-only cases skip); Windows about +65 min across both panes (fake pass in each pane ~25 min, the rest ~15). The fix tier is no longer ~10 min per platform until cases graduate out of `PROVING`; that is the point of the owner's graduation step.

**Which manual suites a tier plays (#1502).** Before this, `integration-linux`, `integration-linux-pwsh`, `usage-test-*`, `insta-snapshots`, `check-macos` and
`check-windows` were manual in every pipeline and no tier played them, so the ~300 GUI scenarios (typeahead, ssh, subshell, bootstrapping, ctrl_d, session
restoration) never ran. Now `candidate` and `release` play, on the box and after its three unit suites (and before the Wayland/X11 app tests, which wait for
them): `integration-linux: [1]`..`[3]` (the matrix shards, one after the other), `integration-linux-pwsh`, `usage-test-linux`; and on the Mac, with its unit suites
and before its app tests, `usage-test-macos`. They stay `manual` + `allow_failure` in GitLab (owner rule: heavy suites are manual or tag-only, no schedules):
`ci_seq.py` plays them, and its verdict counts them (FAIL on a red job). **Owner 2026-11-09: they GATE the candidate and release tiers from the first run** (no baseline-first period, no "known red" exemption): a red or timed-out shard, pwsh run or usage suite fails the tier verdict like `test-linux` does; a red scenario is a bug to fix or an owner-accepted entry, never noise. Being `allow_failure` in GitLab only keeps the pipeline itself green, because the jobs are played by hand. Deliberately in NO tier: `insta-snapshots` (it writes
snapshot files for a human to review and commit), `check-macos` / `check-windows` (the build jobs compile the same code under the stricter installer profile;
`guards-windows` already runs as the first step of every Windows app test). Shuttle/loom, fuzz, bench and the two mutants jobs were already `release` (above).
`ci_seq.py --self-test` pins the suite lists, that every one of these suites yields a FAIL verdict when red, and that a matrix job name resolves to its template's `timeout:`.

**Every suite on every platform (2026-11-03, owner: "the tests need to be the same on all boxes").** Each box suite has a `-macos` and
a `-windows` twin (names in the table in 2.7; `test-macos` / `test-windows` are the `test-linux` lib suite). All are manual and
`allow_failure`, never automatic on a push. They share ONE definition: `script/ci/run_suite.sh <macos|windows> <suite>` runs the same command
as the box job (same packages, `check_test_failures`, `TEST_FILTER` + `check_filter_terms.py`, default nextest profile, same `MUTANTS_*` scope via the
`.mutants_scope` template). Differences that are not choices: no slot governor (`PHOSPHOR_GOVERNOR=0` in `script/agent-cargo`; one job per machine,
the job/thread caps stay: 4 on the 16 GB Mac, 8 on Windows); failures are judged against `script/known_test_failures_macos.txt` /
`_windows.txt` (`PHOSPHOR_KNOWN_FAILURES`; empty at first, so the first run reports every platform failure as a regression to be
triaged); the jobs are in `resource_group` `mac-gui` / `windows-gui`, the same as the app tests, so a suite never overlaps an app run on that machine;
the log is the artifact (`nextest-<plat>.log`, `nextest-warpui-core-<plat>.log`, `nextest-rest-<plat>.log`; no junit: the box jobs have none and the default
profile has no junit section), plus `fuzz/artifacts/` and `fuzz/logs/`, `mutants.out/`, `bench-results/`. `check_test_failures` and `agent-cargo` were made BSD/Git-Bash
portable for this (`sed -i`, `\x1b` and `\s` replaced). `collect_results.py` archives the new jobs like the box ones (`unit/<job>.json`, per-job history).

**Stale mutants scratch (#1764).** cargo-mutants copies the tree to `<temp>/cargo-mutants-<name>-*.tmp` and removes it only on a clean exit; a cancelled
job left an 18.65 GB one that filled the Windows VM disk. Every mutants job (`mutants`, `mutants-warpui-core` and their `-macos` / `-windows` twins) now
starts with `script/ci/clean_mutants_tmp.sh`: it deletes `cargo-mutants-*` directories in `$TMPDIR` / `$TEMP` / `/tmp` (Git Bash maps `$TEMP` on Windows),
only when no `cargo-mutants` process runs for this user, and logs what it removed and the free space before and after. On Windows, check free space
on the runner's system drive before a release-tier run; the script's log line is the preflight.

Tools are installed by the job into the CI user's OWN `~/.cargo/bin` when missing (pinned `script/install_cargo_binstall`, then `cargo binstall`
of `cargo-nextest`, `cargo-mutants`, `cargo-fuzz`), never system-wide, never with sudo. The runner must already have cargo/rustc (the pinned toolchain
installs itself through rustup on first use), protoc (Mac: on PATH; Windows: `$env:PROTOC`), perl and cmake (Windows), Git for Windows' bash and the VM
Python (`%LOCALAPPDATA%\phosphor-ci\python312`); a missing one fails the job with a message. Platform limits found:

- `fuzz-smoke-windows` CANNOT run: cargo-fuzz (libFuzzer plus sanitizers) "works only on x86-64 and Aarch64, and only on Unix-like systems (not Windows)" (rust-fuzz/cargo-fuzz README).
  The job exits 1 with that reason (it never skips silently); `ci_seq.py` does not play it and reports `unsupported on platform` (neutral). Linux and macOS cover the same eight targets.
- `fuzz-smoke-macos` mirrors the box: `RUSTC_BOOTSTRAP=1` on the pinned stable toolchain, so no nightly is installed anywhere (needs the Xcode command line tools' C++ compiler). It never linked before r18: Apple's new `ld` (ld-1267, Xcode 26 CLT 26.4) fails the ASan build with `initializer pointer has no target in libai-*.rlib`. `run_suite.sh` now adds `-C link-arg=-Wl,-ld_classic` to `RUSTFLAGS` when `ld -ld_classic -v` works (the Mac has ld-classic, ld64-957.1), and otherwise passes `--sanitizer none` with a log line saying ASan is off on macOS. Linux is unchanged. Follow-up: `phosphor-fuzz` pulls `ai` only through its `warp` (app) dependency, so avoiding the link would need the targets split from `warp`.
- `fuzz-smoke` (box) and `fuzz-smoke-macos` run every target through `script/ci/fuzz_smoke.sh`: each target's full output (cargo build and libFuzzer, stdout
  and stderr) goes to `fuzz/logs/<target>.log`, kept as an artifact next to `fuzz/artifacts/`, and the trace gets one start line, the final stats
  (`-print_final_stats=1`) and, for a failing target, its crash text (the `panicked at` line and the message after it, libFuzzer's ERROR/SUMMARY lines,
  the crash input's path) plus the log's last 40 lines. Before this, libFuzzer's progress lines for eight 180 s targets overran GitLab's 4 MB trace limit
  and cut off the r18u23 crash's panic message (job 17761810047). Every target now runs even after one fails; the script exits 1 naming the failed ones.
- `test-shuttle-loom-*`: `RUSTFLAGS="--cfg shuttle|loom"` replaces `.cargo/config.toml` flags (on Windows that drops `target-cpu=x86-64-v3`, on macOS the header-pad link arg), each cfg in its own `target/ci-shuttle` / `target/ci-loom`. Pure Rust, expected to work on both; unmeasured.
- Unverified until the first run: whether the whole `warp` lib test suite passes on macOS and Windows at all (the Windows run was never completed before, the old `test-windows` ran only a name filter), the run times, and `mutants-*` copy-tree cost on Windows.

Measured durations (box and Macs, warm caches): `check-linux` 7-9 min, `test-linux` ~50 min, `test-warpui-core` ~2, `test-workspace-rest` ~4,
`build-macos-arm64` ~21, `build-windows-x86_64` ~38. Linux build, app tests (smoke and full), shuttle/loom, fuzz, bench and mutants are not
timed yet; the tier targets above are estimates built from the numbers that are. Builds and the box suites overlap (the macOS and
Windows runners are separate machines), so a fix push is bounded by `check-linux` + the filtered `test-linux`/builds + smoke.

**Release gate (#1495).** A `v*` tag pipeline cannot publish unless the tagged commit passed the release tier first. Two mechanisms, both in
`.gitlab-ci.yml`:

1. *In the tag pipeline.* `release-meta` (the root of the release DAG, so no CLI tarball or installer is built or uploaded to the package
   registry before this is green) and `release-publish` (the only job that creates the GitLab release) both `needs:` one list, written once as
   `&release_quality_needs`: `release-gate`, `guards`, `check-linux`, `licenses`, `advisories`, `test-linux`, `test-warpui-core`,
   `test-workspace-rest`. The suites run automatically on a tag (`.heavy_rules`) and are `allow_failure: false` there.
2. *Recorded evidence for the rest of the release tier.* `release-gate` runs `script/ci/check_release_gate.py`, which reads the pipelines of the
   tagged sha through the API and requires on that exact sha: a `success` for `guards`, `check-linux`, `licenses`, `advisories` and for every
   job the release tier gates on, **taken from `ci_seq.release_gate_jobs()` (the tier's own lists, not a hand list)**: the box unit suites,
   `integration-linux` (3 shards) and `-pwsh`, `usage-test-linux`, `input-timers-strict` (the `--strict` timer guard), `test-shuttle-loom`,
   `fuzz-smoke`, and the Mac / Windows unit and heavy suites (bench and mutants are informational in the tier verdict and not required; a suite
   the pipeline does not define, e.g. `fuzz-smoke-windows`, cannot be required); and a `success` for each plain `app-test-*` job (never `-reuse`)
   whose `results.json` (Wayland: both the wayland and xwayland passes) says `tier=release` (`ci_seq.py --tier release` plays the app tests with
   `APPCI_TIER=release`; candidate stays `full`, fix `smoke`), `sha=<the tagged commit>` (`run.py` records the full `CI_COMMIT_SHA`), no failed
   case and at least one pass. The verdict is the appci one: a NOREF case (no reference terminal) and a KNOWN note (an `ACCEPTED_FOR_RELEASE`
   shortfall, recorded on a pass row) do not fail a run, so they do not fail the gate. Missing API access, expired artifacts (7 days), a smoke/full-tier
   run, or a run of another commit all FAIL the gate.

Procedure to release: bump the version and regenerate `docs/STATE.md` FIRST, merge, run `python3 script/ci/ci_seq.py build/<name> --tier release` on
that exact commit, then tag that commit within 7 days. (v0.1.17 was tagged two commits after the last tested commit, a version bump and a STATE.md
regeneration, so under this gate it would have been refused: the evidence must be on the tagged sha.)

**Known gap (documented, not redesigned).** The app tests run on the `build-*` binaries of the `build/*` pipeline, not on the tag pipeline's
`release-*` binaries (those have the CLI digests compiled in and are built after the gate). Same commit, different bytes.

**Dry run (read-only, nothing is started):** `python3 script/ci/check_release_gate.py --sha <commit> [--pipeline <id> ...]` reads through `glab api`
and prints ok / FAIL lines; exit 1 = it would refuse. Offline fixtures (pass, failed job, wrong sha, wrong tier, KNOWN, NOREF, missing platform,
expired artifact, reuse variant): `python3 script/ci/test_check_release_gate.py`, run by the `guards` job and `script/precheck`.

**OWNER STEPS (GitLab project settings; YAML cannot do these, so the owner does them, not CI or an agent):**

- [ ] Settings > Repository > Protected tags: protect `v*`, allowed to create = **Maintainers** (not Developers). Keep it protected (the Mac and Windows runners are `ref_protected`).
- [ ] Settings > Repository > Protected branches: `main` push restricted to the owner, force push off (optional, recommended).
- [ ] Settings > CI/CD > Variables: a masked, protected `APPCI_API_TOKEN` with `read_api`, in case `CI_JOB_TOKEN` is refused for other pipelines' job artifacts (the first tag will show it: the gate prints "cannot read ... set a masked APPCI_API_TOKEN").
- [ ] After the first gated tag, confirm `release-gate` read the evidence rather than failing on access.


**Build reuse in the fix tier.** Before playing `build-X`, `ci_seq.py` looks at the newest `build/*` pipelines (at most 6 with a succeeded
`build-X`) and reuses one when its `build-X` job succeeded (its pipeline may still be running; the build is taken from the test machine's local cache, 2.8) and `git diff --name-only <its commit> HEAD` is empty over the app-affecting
path set AND the `build-X` job block (plus `.shim_github_env` / `.windows_job`) in `.gitlab-ci.yml` is textually identical. App-affecting
is everything EXCEPT the list `APP_NONAFFECTING` in `script/ci/ci_seq.py`: `docs/`, `*.md`, `.gitlab*`, `specs/`, `fuzz/`, `script/ci/`,
`script/realapp/`, `script/check_*`, `script/test_*`, `precheck`/`presubmit`/`state`, agent/editor files. So `app/`, `crates/`, `lib/`,
`Cargo.*`, toolchain, `.cargo/`, `assets/`, `resources/`, `themes/`, `script/{bundle,linux,macos,windows,install_*,
prepare_*}` and any new unlisted path rebuild (fail safe). Each platform prints its decision (`macos: reusing build job N from pipeline P
(commit C), app unchanged` or `macos: building, app changed in: <paths>`); `--no-reuse` forces builds. `candidate` and `release` NEVER reuse.
Reused platforms start their app test right after `check-linux` (no build to wait for); the smoke tier does not play `app-test-linux-x11-reuse` (it
would only exit; the summary shows it as `skipped by tier`).

**Fix-tier filter derivation.** Without `--tests`, `ci_seq.py` runs `git diff --name-only <base>..<ref>` (base = merge-base with the
newest other `build/*` ref, else `origin/main`; `--base` overrides) and maps paths: `crates/<name>/` -> `package(<name>)` (`warpui_core`
also runs its own suite and counts as a change to all of `warp`); `crates/warp_tui` -> `package(warp_tui)`; `app/src/<a>/<b>/x.rs` ->
`package(warp) & test(/<a>::<b>/)` (the last two path segments, `_test(s)` suffixes and `tests/` directories stripped, `mod.rs` = its
directory); non-Rust app files -> all of `warp`; workspace `Cargo.toml`/`Cargo.lock`/toolchain/`.cargo/config` -> nothing filtered, all
suites. Docs-only and CI-only diffs run nothing but `check-linux`. The derived filter, the reasons and the platforms are printed
before anything is played (`--dry-run` stops there): read it, and override with `--tests` when the mapping is too narrow (a change in
shared code that other modules' tests exercise) or too wide. Dependents of a changed non-app crate inside `warp` are not added
automatically. Platforms: a diff of only Windows (or macOS, Linux) paths builds only that platform; any shared code builds all three;
`--platforms` overrides. `ci_seq.py --self-test` checks the filter mapping (`--dry-run --files a,b` prints the filter for a fake file list); a job pending with no runner for `--pending-timeout` minutes (default 20) is reported RUNNER-OFFLINE, the rest continue and the exit status is non-zero. A filtered `check_test_failures` run still fails on any new failure but does not judge listed known failures
that did not run (that stale-entry rule needs a full run, which candidate does).

**Scoped mutation testing (release tier).** `.cargo/mutants.toml` + the `mutants` job: `pane_key.rs`, `model/key_facts.rs`,
`input_queue/{gates,table,facts,mod,pane,shadow}.rs` and `view/dest_table.rs` (the #1424 destination table and its plumbing, about 4,000 lines;
the legacy `input_queue/destination.rs` is deliberately out of scope, it is the function the table replaces) with the tests limited to those
modules and the table-mode view tests (`MUTANTS_TEST_FILTER`), each mutant costing one incremental `warp` rebuild: several hundred mutants,
so shard it (`MUTANTS_SHARD=k/n`); time UNMEASURED (the first run records the
real number in `mutants.out/outcomes.json`). `mutants-warpui-core` mutates the clipboard (`clipboard.rs`, `clipboard_utils.rs`) against
warpui_core's tests only: estimated 25-40 mutants, under an hour. Widen by playing `mutants` with `MUTANTS_FILES` (space separated) and
`MUTANTS_TEST_FILTER` set, and split a wide run with `MUTANTS_SHARD=k/n` over several jobs; `cargo mutants --list --file ...` counts
mutants without running them.

**Clipboard mutants (#1778).** The first `mutants-warpui-core` run left 51 clipboard mutants surviving; the tests added to
`crates/warpui_core/src/clipboard_tests.rs` kill them, none touching the real system clipboard (`InMemoryClipboard`, stubs, local
`PendingWrites` / `WriteOrder`, pure functions, so no nextest group): the deferred-read gate (`read_gate_*`, `deferral_is_active_until_released`,
`in_memory_reads_inline_unless_deferred`, `deferred_in_memory_read_answers_only_after_release`; every wait is bounded by a 2 s `within` or channel
timeout, so a regression fails instead of hanging), `should_insert_text_on_paste_truth_table`, the `ClipboardContent` accessors
(`text_bytes_*`, `has_image_data_*`, `num_paths_*`, `has_non_image_filepaths_*`), `PendingWrites::wait_until_idle` (`idle_wait_*`),
`WriteOrder` (`generations_start_at_one_*`, `a_write_of_the_same_generation_*`), the legacy readers and trait defaults, `paste_cap_is_64_mib`,
`abandon_pending_reads_bumps_the_global_epoch`, and the global-static ones (`global_pending_write_*`, `menu_entry_follows_*`,
`availability_events_and_own_writes_*`; they own one target each and retry, so plain `cargo test` threads cannot flake them). Five mutants stay
unkillable by design: the `>` in `abandon_pending_reads` and the `!=` / `>=` in `tracked_reader_with` (lines 233, 312, 321) only change a
`log::warn!`, and `warpui_core` cannot install a capture logger (`src/test.rs` `#[ctor]`, see `report_error.rs`).

**Clipboard mutants, what is left (#1778 follow-up).** The remaining survivors, with the reason each is equivalent (cargo-mutants cannot tell a changed log line or an
unreachable OS failure from a changed behaviour): the `>` in `abandon_pending_reads` (`blocked > 0` only chooses whether a `warn!` is written), the `!=` in
`tracked_reader_with` (the abandon-epoch check only writes a `warn!`; the result is handed back either way), the `>=` in `tracked_reader_with` (the slow-read `warn!`),
and the failure arms of `std::thread::Builder::spawn` in `start_reader`, `write_in_background` and `serial_writer` (`!spawned`, the `Err(err)` log): a thread spawn cannot be
made to fail in a unit test. Killed since: `ReadKind::target` (`a_read_kind_targets_the_clipboard_it_reads`), `ReaderTracker::default` / `new` / the epoch bump
(`a_new_tracker_*`, `abandoning_bumps_the_epoch_once_per_call`) and the default `InMemoryClipboard` (`the_default_in_memory_clipboard_*`). The job's mutant list itself was not re-run
(no cargo here): the next `mutants-warpui-core` run (release tier) is the measurement.

### 2.10 Markdown and Mermaid readers (#1460)

What is covered, where, and how to update it. Nothing here runs cargo on the laptop; the box does (section 1).

| layer | where | what it checks |
|---|---|---|
| parser unit tests (pre-existing, ~142) | `crates/markdown_parser/src/*_test.rs` | individual constructs, incremental tail parse |
| markdown corpus + insta snapshots | `crates/markdown_parser/tests/corpus.rs`, fixtures `tests/fixtures/*.md`, snapshots `tests/snapshots/` | every fixture (headings, nested lists, task lists, tables, fenced code, inline, links/images, block quotes, HTML blocks, footnotes, unicode/RTL, CRLF, malformed, control chars, empty) parsed with and without GFM tables; the `FormattedText` document model is the snapshot. Also: no panic, deterministic, CRLF reads like LF, 600 KB single lines, deep nesting, streamed tail parse equals full parse. Tables: `parse_markdown_with_gfm_tables` is the runtime switch (no cargo feature) |
| mermaid corpus | `crates/editor/tests/mermaid_corpus.rs`, fixtures `tests/fixtures/mermaid/*.mmd`, snapshots `tests/snapshots/` | 14 valid diagrams (flowchart TD/LR, sequence, class, state, ER, gantt, pie, mindmap, journey, timeline, frontmatter theme, unicode labels, CRLF) rendered through `mermaid_asset_source`'s fetch, the exact app path: expected labels present, deterministic, normalized summary snapshot (rounded size, element counts, text labels; never the raw SVG). 9 `invalid_*` files: must return the app's error (which becomes `FailedToLoad` and the source fallback in `render_mermaid_diagram_section`) or a non-blank SVG, never panic. Hostile sources for every diagram kind (including #1622's subgraph ids reused inside themselves and #1639's title-only `subgraph [Title]` blocks), 300-node flowchart and 400-message sequence bound; `issue_1622_unclosed_subgraph_reuse.mmd` (the fuzz timeout input, also seed `fuzz/corpus/mermaid_render/ref_issue_1622`) must return the app's named self-nesting error in under 2 s, and `issue_1613_cycle_two_sources.mmd` (#1613: two source nodes reaching one cycle, also seed `ref_issue_1613`) must give byte-identical SVG over 24 same-thread and 8 fresh-thread renders (the fork's back-edge search used to follow `HashMap` order; the fork pins the fix, this test keeps it pinned), `issue_1639_title_only_subgraph.mmd` (the fuzz stack-overflow input with its literal 0x04 byte, also seed `ref_issue_1639`) must render with its labels in under 2 s, well inside the 8 s worker budget, so a regression fails as a timeout (or an abort) instead of passing as the budget's error. The app's fetch is bounded (#1477): sources over 256 KiB or with a line over 4000 characters are refused up front (an unbroken 100k-character token made the renderer spin for minutes), and a render runs on a worker thread with an 8 s budget, ending as the error/source-fallback path; unit tests in `mermaid_diagram.rs` (`render_limit_tests`) |
| render-time bench | `crates/editor/benches/mermaid_render_bench.rs` | criterion; budgets 50 ms per small diagram, 2 s for 300 nodes / 400 messages; `MERMAID_BENCH_ENFORCE=1` makes a blown budget fail |
| cargo-fuzz | `fuzz/fuzz_targets/markdown_parser.rs`, `mermaid_render.rs`; seeds `fuzz/corpus/{markdown_parser,mermaid_render}` | run by `fuzz-smoke` (180 s each) |
| real-app CI cases | `script/realapp/appci/cases.py`: `md_viewer` (smoke + full, both modes), `md_viewer_big` (full only), `md_viewer_ai_answer` (agent, needs the CI Ollama model) | opens a `.md` with a table and a mermaid flowchart in the running app through a `file://` URL (Linux: second invocation forwarded over the app's D-Bus; macOS: `open -a <bundle>`), screenshot, OCR (tesseract on Linux, Apple Vision on Mac; no engine = screenshot only and a note) of the table text and the diagram node labels; 2 MB file: time to first heading and RSS growth (first-guess budgets 90 s / 3000 MB, tighten after the first numbers). The AI answer part is a note/metric only (model output is nondeterministic); no fake-model path exists. Windows: SKIPs with the reason (no verified way to hand a `file://` URL to the RUNNING Windows instance; no OCR engine on the VM) |

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

**Blockquotes and tilde fences (#1554, #1555).** `> text` parses to `FormattedTextLine::BlockQuote { depth, line }` (one per quoted line; lists, code and nested quotes inside a quote are supported; a lazy continuation line is normalized to a quoted line). `~~~` fences are recognised like backtick fences (same character and at least as long to close). Two consequences to know when reading results: (1) any line starting with `>` in agent output or a README now renders as a quote with the marker removed, including shell-prompt-looking `> cmd` lines; (2) the rich-text editor parses with `parse_markdown_without_blockquotes`, so a quote stays literal `> ` text there and open then save is byte-identical (the editor does not render quote bars; read-only views in `FormattedTextElement` and the TUI do). The agent-answer splitter in `app/src/ai/agent/util.rs` still only knows backtick fences.

### 2.11 ConPTY held keys across the prompt (#1569)

The enter window (#1490, on with the table mode) holds every key typed while a command runs, from its Enter to the next prompt's
line-editor signal (about 0.5 s, a burst of dozens of keys with whole lines and their Enters). The hand-off keeps no bytes for those
keys, so they are not counted until the signal releases them. `try_commit_at_prompt` (the precmd) used to see an empty shell line and
hand input to the editor: keys typed next went to the editor while the held ones were written raw to the pty, the shell's report then
merged another line into the editor, and the wrong command ran (Git Bash fast_typing 19/30; also tab_enter, ctrl_c_d line integrity).
Fixed: no commit while a prompt hold is queued (the release asks for the decision again with true counts), and a hand-off report that
replaces the editor's front but does not extend it is refused and logged (`hop=handoff_insert_refused`).

| layer | test | asserts |
|---|---|---|
| model | `typeahead_handoff_tests.rs`: `the_prompt_does_not_commit_to_the_editor_over_keys_held_for_the_shell`, `the_prompt_commits_to_the_editor_when_no_keys_are_held` | precmd with a queued hold leaves the pty owning input; release counts the keys; no `Legacy`/`Committed` decision over an unreported line |
| view, Legacy and Table | `view_test.rs`: `keys_held_across_the_prompt_all_go_to_the_pty_once_and_the_editor_stays_empty` | a full line + Enter + the next line's start held in the enter window, precmd before the signal, one more key, signal: the pty gets `echo A\recx` once, byte-exact, one write per key, and the editor buffer stays empty throughout |
| editor | `input_test.rs`: `a_handoff_report_that_does_not_extend_the_editors_typeahead_is_refused` | a report replacing 9 characters with another line's text (or a shorter one) leaves the buffer alone; a real extension still replaces |
| real app | Windows `fast_typing` (instant Enter), `tab_enter`, `ctrl_c_d` line integrity, Git Bash pane, terminal and agent mode | the shell's `rec.log` has every typed line exactly once; the log has no `handoff_insert replaced_prefix_chars>0` against a different line and no `handoff_insert_refused` in a passing run |

Evidence to look for in a Windows app log: `kh_iq_release ... chunks=N` with N in the dozens is the enter window working as designed;
it is correct only if `hop=commit_deferred_prompt_hold` appears when a `precmd_seen` lands while the gate is open, and no `kh_pane_key
... target=Editor` follows that precmd until the release. `PHOSPHOR_CONPTY_ENTER_HOLD=0` and `PHOSPHOR_QUEUE_DESTINATION=legacy` are the
kill switches to compare against.

### 2.12 ConPTY give-up is per prompt, and reports the hand-off cannot account for (#1573, #1574)

**#1573.** The enter window and the precmd's wait for the line editor share one `LineEditorActive` gate and its 1.5 s cap
(`CONPTY_PROMPT_HOLD_GIVE_UP`). A command that ran longer than that with typeahead (`sleep 3`, a build) fired the cap: the keys were
written raw to ConPTY mid-command and the prompt hold was switched off for the rest of the session, so every later prompt wrote its keys
raw again (#1162/#1569). Fixed: the cap asks the hand-off (`TypeaheadHandoff::prompt_hold_cap_fired`) whether anything is overdue. A
running command (its preexec seen since the Enter) is not: the cap is armed again and nothing is written; the window ends by its events
(precmd, the interactive latch, a continuation prompt -- now an event too -- or a signal key). The precmd that follows gets a full cap of
its own. A give-up releases the keys in typed order through the counted release (the signal's path), and switches the hold off for the
session only when the shell has never answered a precmd's wait with a signal (#1349's documented reason). `hop=prompt_hold_cap_not_due`,
`hop=prompt_hold_expired precmd_wait=.. enter_window=.. session_off=..`, `hop=enter_window_continuation`.

**#1574.** The shell's report handler clears its own line (bash `READLINE_LINE=""`, zsh `BUFFER=""`), so a report must never be
refused once asked for: refusing it loses the line. Instead: a line this pane delivered to the shell as final (`TypeaheadHandoff::note_committed_delivery`; the #1494 `stty sane`
delivery that set it is deleted, #1815) is never asked for -- `on_line_editor_active` answers `Drain`, the pty keeps input
and the shell keeps the line where the toast said it went (`hop=handoff_request_skipped`); the mark ends with a user Enter, a written
command or a commit to the editor. A report reaching an input whose mode is AI and stays AI (locked, or no autodetection) is inserted and
the input is switched to shell, locked, visibly (`hop=handoff_insert_as_shell`, counts only); a deferred Enter runs it as a shell command
exactly as before. An unlocked AI input with autodetection on keeps its mode: the typeahead provenance (#886) decides. Only the #1569
not-my-text refusal remains, and a deferred Enter whose report it refused is given up, never run on the fragment.

| layer | test | asserts |
|---|---|---|
| model | `typeahead_handoff_tests.rs`: `a_command_running_past_the_give_up_cap_keeps_its_keys_held_and_the_hold_on`, `a_key_typed_after_the_cap_into_a_running_command_is_still_held`, `the_enter_window_gives_up_for_its_own_keys_and_the_hold_stays_on`, `a_give_up_by_a_shell_that_signals_is_for_that_prompt_only`, `a_continuation_prompt_ends_the_enter_window_and_releases_its_keys`, `a_cap_firing_while_an_event_release_is_in_flight_is_not_a_give_up` | Git Bash-like `sleep 3` with keys in its first second: the cap is `NotYet` while it runs and after its precmd, every key released once in order by the signal, the next prompt held again; no clock on the window; a give-up never switches off a shell that signals |
| model | `typeahead_handoff_tests.rs`: `a_committed_delivery_is_never_asked_for_and_stays_in_the_shell`, `the_committed_delivery_mark_ends_when_its_line_cannot_hold_it` | with the mark no request is decided (`Drain`), the pty keeps input; the mark ends with a user Enter, a written command or a commit to the editor |
| view, Legacy and Table | `view_test.rs`: `a_command_running_past_the_prompt_hold_cap_keeps_its_keys_held_and_the_hold_on` | the cap firing mid-command and right after the precmd writes nothing; the signal writes `echo\r` once, one write per key; the next prompt opens a hold again |
| view, Legacy and Table | `view_test.rs`: `a_line_delivered_to_the_shell_as_final_stays_in_the_shell`, `a_report_into_an_ai_mode_input_is_kept_as_shell_text_and_runs_once` | the delivered line stays in the shell (no request, editor empty, the next keys and Enter reach the pty once); a report into a locked AI input is inserted as shell text, and a deferred Enter runs it as a shell command exactly once; the `note_committed_delivery` mark itself is covered by `typeahead_handoff_tests.rs` (its only production caller, the #1494 delivery, is deleted, #1815) |
| editor | `input_test.rs`: `a_handoff_report_into_an_ai_mode_input_is_kept_as_shell_text` | the report is inserted (never dropped) and the input is switched to shell, locked; a shell-mode input inserts as before |
| real app | Windows Git Bash and PowerShell, terminal and agent mode: `sleep 3` + a line typed in its first second, then a fast line at the next prompt | the shell's `rec.log` has both lines once, in order; no `session_off=true` in the log for Git Bash; agent mode ssh with a slow bootstrap: after `stty sane` the delivered line stays on the remote (no `handoff_insert` of it, `handoff_request_skipped` logged), nothing typed is lost |

### 2.13 The prompt hold's give-up never writes to the pty (#1584)

The 1.5 s cap of the ConPTY prompt hold is a give-up, and a timer never writes to the pty. At a prompt whose line holds nothing typed
(the precmd's wait), held keys with no Enter go back to the editor as shell-routed text (`system_insert` + the next Enter as a shell
command), the editor takes input over, and the log says so with counts (`hop=prompt_hold_handed_back`). A segment with an Enter, or one
that cannot go back in order (an enter window whose Enter the shell never acted on), stays held: the cap is armed again, a toast and one
log line (`hop=prompt_hold_kept_overdue`) say input is waiting for the shell, and the events release it (the signal, a command becoming
interactive, a continuation prompt, ^C or another signal key). A shell that has never signalled and has missed
`CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF` (3) prompts in a row (#1697: one slow first prompt is not enough) is not held for at later
prompts (its keys go by the live path) until a signal shows it does. Such a shell's segment at a clean prompt is handed back even with an Enter (as a
newline, never run): its signal is not coming. An interrupt (^C, ^Z, ^\\) on held keys VOIDS them (the tty flushes typeahead on it;
`hop=prompt_hold_voided` with counts) -- only the signal reaches the pty; a lone ^D still follows them. The hand-back keeps typed order
across editor and pty entries, and a stale gate's keys are shell-routed too. A preexec that clears a hold's precmd wait turns it into
an enter window, released by the latch.

| layer | test | asserts |
|---|---|---|
| model | `typeahead_handoff_tests.rs`: `only_a_hold_at_a_clean_prompt_can_go_back_to_the_editor`, `an_overdue_hold_kept_for_the_events_is_still_released_by_the_signal` | hand-back only at a clean prompt; a kept hold keeps its wait and the signal releases it |
| view, Legacy and Table | `view_test.rs`: `a_prompt_hold_past_its_cap_hands_its_keys_back_to_the_editor_and_writes_nothing`, `a_held_segment_with_an_enter_past_the_cap_stays_held_until_the_signal` | no signal past the cap: `ls -l` in the editor, editor owns input, zero pty writes; `ls\rx` past the cap (twice): still held, nothing written, then the signal writes it once, in order |
| view, Legacy and Table | `view_test.rs`: `an_interrupt_voids_a_kept_overdue_segment_and_only_the_signal_reaches_the_pty`, `a_hand_back_keeps_typed_order_across_editor_and_pty_entries`, `a_stale_prompt_hold_gate_hands_back_shell_routed_text`, `a_never_signalling_shell_hands_back_a_segment_with_an_enter`, `a_hold_whose_wait_a_preexec_cleared_is_released_by_the_latch`; model `a_preexec_turns_a_waiting_prompt_hold_into_an_enter_window` | ^C on a kept `ls\r`: the pty gets only `\x03`; `l`,`s`,editor `x`,` -a` → `lsx -a`; a stale gate's `ls` is shell-submitted; a never-signalling shell's `ls\rx` → editor `ls\nx`, nothing written; a preexec-cleared hold is released by the latch, once, in order |
| view | `view_test.rs`: `a_signal_takes_the_keys_held_for_the_prompt_in_front_of_it` (^C voids, ^D follows), `the_queued_prompt_hold_gives_up_in_order_and_switches_itself_off`, `a_prompt_hold_cap_that_fires_while_the_queue_is_busy_still_releases_the_keys` | the give-up hands back to the editor (also from a busy queue), never writes; the never-signalling shell's next key goes by the live path |

**Keys held behind an unsubmitted deferred Enter (#1189).** `release_held_keys` (a deferred Enter's `HandoffReport` gate closed and
the Enter wrote no command) was the last release that wrote its keys to ConPTY as ONE write with the Enter(s) inside it -- the
shape #1189 lost lines on. It now gets the keys one chunk per key entry and, on a ConPTY session, writes each as its own write, like
every other release there (`TypeaheadHandoff::held_key_release_writes`; elsewhere still one write). In an app log the release is
`hop=held_keys_release writes=N bytes=M`: on Windows N equals the number of keys handed to the event loop, which may still merge
adjacent plain-text keys (every byte >= 0x20, `is_plain_key_text` in `local_tty/event_loop.rs`) into fewer pty writes
(`hop=pty_write_coalesced`); a CR or an ESC sequence always ends a write. Tests: `typeahead_handoff_tests.rs`
`held_keys_released_after_an_unsubmitted_enter_keep_their_key_boundaries_on_conpty` (ConPTY: one write per key, empty chunks
dropped; elsewhere: one write, typed order) and `view_test.rs` `a_deferred_enter_gate_release_on_conpty_writes_one_key_per_write`
(`ls`, `\r`, `x` held behind the gate and released: ConPTY writes `ls` / `\r` / `x`, otherwise one `ls\rx`).

**App-test artifacts are slimmed before upload (r18u8).** The macOS run's 467 PNG shots + 102 logs were rejected by GitLab
(`413 Payload Too Large`) and the run's evidence was lost. Every app-test job's `after_script` now runs `script/ci/slim_appci_run.py`
on its run dir, which writes `run_slim/` (shots as JPEG q60 at most 1600 px via Pillow or macOS `sips`, logs over 16 MiB gzipped,
results/summary/junit as is), and the artifacts are taken from `run_slim/`. When an upload still fails, play `collect-macos-run` /
`collect-windows-run` (manual, `GIT_STRATEGY: none`, same resource group) on that runner BEFORE any other job checks out there: it
re-slims the run left on disk and uploads it.

### 2.14 Coverage-gap cases (`appci/cases_coverage_b.py`, #1542-#1548, #1460, #1463)

Full tier only (none is in `SMOKE`). Same rules as 3.1: exact bytes from `rec.log` / the app log's `hop=enter_begin` line, 10-25 ms/key, instant Enter, a screenshot per step.

| case | gap | what it proves |
|---|---|---|
| `cb_agent_pickers` (agent) | #1542 slash/@ pickers | `/` and `@` change the screen (Pillow diff), Escape closes them, nothing reaches the shell, a prose line afterwards arrives in the agent input byte for byte |
| `cb_agent_file_mention` (agent) | #1542 attachments | `@<file prefix>` lists a real file (OCR when present); Enter accepts the row without submitting to the agent or the shell (an AI `hop=enter_begin` counts as a submit only when no `hop=enter_menu_accept` follows it before the next Enter, #1633; unit: `test_enter_on_the_at_picker_in_agent_mode_never_submits` hands the picker the typed query as the app does and checks the Enter waits in the picker, `terminal/input_test.rs`; the editor re-reporting the unchanged filter after that Enter must not cancel it, `query_gate_tests.rs` `the_same_query_again_*`); the next prose line is exact |
| `cb_settings_ui_open` (both) | #1545 settings UI | Ctrl-, / Cmd-, opens the page (screen change, OCR), Escape closes it, typing in the pane behind is exact. Windows: not mapped |
| `cb_settings_migration` (both) | #1545 migration | a settings.toml with unknown comment/key/table, then a truncated unparsable one: the app starts, types exactly, logs no panic and no migration/settings ERROR (on the truncated file the app's own `Settings file could not be parsed` notice is required, not counted as a failure), and leaves the truncated file byte for byte (a check since r18u22); `legacy_entries_kept` is a metric. Before each arm's burst one line records where the relaunched shell started (`pwd` into `cb_settings_<arm>_cwd.txt`, metric `<arm>_start_cwd`, a note when it is not the case dir) and `cd`s to the case dir: on the truncated file the app runs on defaults, `restore_session = true` restores the previous tab in its saved cwd, and the rc only moves a `$HOME` or `/` start (r18u23 Mac agent pass: rec.log had the 4 lines exactly, the effect file in the case dir 0/4). The original file is restored |
| `cb_state_relaunch` (terminal) | #1546 DB migration path | three relaunches on the same state dirs: prompt, exact typing, no migration/database ERROR in the app log, database files neither vanish nor halve |
| `cb_no_autoupdate` (terminal) | #1547 | the whole run's app log has no updater line (the GUI updater is declined: DECLINED.md `FeatureFlag::Autoupdate`, `script/check_no_autoupdate`) |
| `cb_ui_action_latency` (terminal) | #1548 | local Tab completion (`tab_completion_s`: a unique directory prefix + Tab; the completed line must also RUN, `test -d <completed> && echo tok >> f`; POSIX-shell panes), new tab and tab switch seen from screenshots: bound `APPCI_UI_ACTION_MAX_S` (1.5 s, first guess), numbers as metrics. An UPPER bound on the app's time (each screenshot costs 50-300 ms), not a 4 ms proof |
| `cb_editor_mac` (mac) | #1544 | edit, Cmd-S, undo, Cmd-S on a file handed over by `open -a`; disk byte-exact; nothing at the shell. Runs last (no keyboard pane-close on macOS; run.py relaunches if the pane is not the shell) |

Pure helpers (`legacy_settings`, `corrupt_settings`, `startup_log_problems`, `autoupdate_lines`, `picker_verdict`) are covered by `selftest.py` (`coverage_b_pure_helpers`).

**Already covered, so not duplicated here.** #1460 (markdown/mermaid): sections 2.10 (parser corpus + snapshots, mermaid corpus, bench, fuzz targets) and the `md_viewer*` cases. #1463: `agent_stream_cancel`, `agent_approval`, `routing_table`, `agent_input_exact`, `agent_mode_switch` in `cases_caseagent.py`. #1543 is the same gap as #1460 (pixel-level diagram rendering stays the documented gap in 2.10). #1544 Linux and Windows: `editor_edit_save`, `editor_paste`, `editor_memory_7mb` (Windows hand-over: the app's named-pipe single-instance forward); macOS: `cb_editor_mac`.

**Recorded gaps (need an owner decision or a fixture, not code):** uninstall and a `phosphor://` URL end to end (#1546): need a clean-VM step, today manual (5.4). Upgrade over a previous build's DATA (#1546 option a) is a case now, `upgrade_over_previous` (and, for v0.1.17 specifically with every changed setting asserted, `upgrade_from_v0_1_17`, section 2.18) (order 90, full tier, `cases_coverage_b.py`): the new build starts on the previous release's data directory; the data comes from `APPCI_PREV_FIXTURE=<state tarball>` or from `APPCI_PREV_BIN=<previous release binary>` (macOS also `APPCI_PREV_BUNDLE=<.app>`), which the case launches on the scratch profile, uses, stops and packs to `<run>/fixtures/prev-state.tar.gz` (keep it as a CI artifact; not on Windows, where the state is in the known folders). Verdict: the new build reaches a prompt, types exactly, logs no failed start/migration, keeps every database file (not moved aside, not shrunk), and has not fewer applied `__diesel_schema_migrations` than the old build left (metric `migrations_before_after`). Without either variable it SKIPS with that reason: where CI gets the previous release's artifact (the generic package registry has the CLI tarballs and the installers per version) and which Windows/Mac machine may keep it is the owner's decision. The installer-over-installer step itself stays manual. Auto-update against a real release (#1547): the GUI updater is off by decision; its `SHA256SUMS` verification is unit-tested in `app/src/autoupdate/gitlab_tests.rs` and cannot meet a real release until the release pipeline has run end to end. Key-to-paint on Linux/Windows (#1548): still no pixel read-back tool there (Linux needs a 240 Hz capture; Windows needs Desktop Duplication through an installed tool: an environment decision), so a pixel-level key-to-paint verdict is NOT-FIXED; the app-side key to first frame (`latency_key_log`, both platforms) remains the proxy. The 4 ms UI-work line IS covered now: the app's `hop=ui_item` line (`input_order::log_ui_item_if_over_budget`, floor 4 ms, #1191) is read by `latency_key_log` (`metrics.ui_items_over_4ms`, `lat.ui_work`: event/action items over 4 ms beyond max(2, 5 % of the typed keys) FAIL; `frame` items WARN because software GL rasterises over 4 ms; an item >= 16 ms is a WARN note) and by `cb_ui_action_latency` (`new_tab_ui_work`, `tab_switch_ui_work`).

**#1617 (Windows session-end gate).** Every handled shell exit asks the gate once (`exited_shell_keeps_pane`), a shell that never
bootstrapped included, so its counted session-end exit is consumed and a later ordinary `exit` closes its pane.

| layer | test | asserts |
|---|---|---|
| pane | `terminal_pane.rs` `exited_shell_tests`: `an_unbootstrapped_exit_consumes_its_session_end_count`, `a_counted_exit_of_a_bootstrapped_shell_keeps_its_pane_once` | the gate is asked for an unbootstrapped exit and its count is gone; the next ordinary exit closes |

**#1438 (PowerShell abort settle).** After a Ctrl-C skips a PowerShell setup, the shell's read marker is the only release that writes
the held keys. A hold with no marker token (the timed setup and its `PHOSPHOR_BOOTSTRAP_TIMED` switch were deleted, #1814) has no event, so
`POWERSHELL_ABORT_SETTLE` is a give-up that hands the held keys back to the editor (`abort_settle_give_up`), never writes them.
The Windows real-app cases `cases-r18bj/S2.ps1` and `LS2.ps1` no longer set `PHOSPHOR_BOOTSTRAP_TIMED=1` around `FreshApp2` (the switch is deleted, #1814); they run the marker-paced setup.

| layer | test | asserts |
|---|---|---|
| view, Legacy and Table | `group2_timer_tests.rs`: `powershell_abort_settle_without_a_marker_hands_the_held_keys_back` | no token: the held `ls` is in the editor with the Enter guard armed, nothing written; a stale generation does nothing |
| view, Legacy and Table | `group2_timer_tests.rs`: `ctrl_c_on_a_timed_powershell_setup_hands_the_held_and_later_keys_back_after_the_settle` | the real path: `ls`, Ctrl-C, `x`, the spawned 500 ms timer: the editor holds `lsx`, the pty got only `\x03` |
| view | `view_test.rs`: `powershell_abort_settle_ends_on_the_read_marker_not_the_clock` (existing) | the marker after the Ctrl-C writes the held keys; time and line markers do not |

**#1710 step 1 (the sh probe is marker-paced; an unsupported shell ends the setup by an event).** The shell-agnostic probe
(`shell_type == None`: sudo -s, su, ksu, docker exec, a nested bash from the banner) used to write its two lines 30 ms apart, and a
probe that named a shell with no bootstrap script (dash, ash, ksh) sent NO event, so the setup stayed announced and the keys stayed
held until a net fired (#1679). Now the prelude ends in a plain-`printf` `\033]7771;<token>;0\a` marker (no `builtin`, no `\e`: macOS
dash) that releases the probe line, the probe's own `InitSubshell` is its consumed event, and an unknown shell sends
`ModelEvent::SubshellNotPhosphorizable { shell_name }` (not on the tmux-ssh path, which keeps `RemoteWarpificationIsUnavailable`):
the view clears the announcement, closes the gates, delivers the held keys to the shell in order and shows a plain-shell toast.
`TerminalModel::unregister_session_id` forgets the probe's ID at `InitSubshell` (review H9) and nothing else. The marker survives
tmux control mode: `%output` carries the pane's raw bytes, like the DCS hooks (review H8). (Step 1 kept the 5 s marker give-up as
the fallback; step 2 below removes it.)

| layer | test | asserts |
|---|---|---|
| bootstrap | `bootstrap_snapshot_tests.rs`: `bootstrap_setup_lines_sh_probe` (snapshot `snapshots/bootstrap_setup_lines_sh_probe.snap`) | the prelude line unmarked and marked (token 4242) |
| bootstrap | `bootstrap_test.rs`: `test_sh_probe_is_marker_paced_on_its_prelude_line` | marked: line 0 is the prelude + `printf '\033]7771;4242;0\a'`, no `builtin`/`\e`/ESC byte, under the canonical line limit; line 1 is the unchanged probe with no marker; unmarked lines unchanged |
| model | `terminal_model_test.rs`: `init_subshell_with_an_unknown_shell_reports_not_phosphorizable`, `unregister_session_id_forgets_only_that_id` | `dash` sends exactly `SubshellNotPhosphorizable("dash")` and no `RemoteWarpificationIsUnavailable`; `zsh` still sends `InitSubshell`; unregister removes one ID, idempotent, others untouched |
| view | `view_test.rs`: `the_sh_probe_is_marker_paced_not_timed` | `write_init_subshell_bytes_to_pty(None)`: one line out, the prelude carries the marker; 300 ms and output release nothing; marker 0 writes the probe line and ends the in-flight write; the probe's ID is remembered |
| view, Legacy and Table | `view_test.rs`: `an_unsupported_subshell_ends_the_setup_and_delivers_the_held_keys_in_order` | `ec`,`ho`,Enter typed during the probe are held through the marker, and on the event reach the pty once as `echo\r` right behind the probe; announcement, in-flight, probe ID and held counts cleared; editor empty |
| view | `view_test.rs`: `the_probe_id_is_unregistered_when_init_subshell_names_the_shell` | after `InitSubshell(bash)` the probe's ID is unregistered, a registered parent ID is untouched, stage 2 is in flight with its own registered ID |
| appci | `subshell_sh_probe` (see the cases list) | real dash / busybox ash: first inner line < 2 s, exact lines, exit, log hops |

**#1711 / #1712 step 2 (the one progress-based setup net; owner decision 2026-11-08: hand back, 30 s / 120 s / notice at ~10 s).**
A subshell setup has ONE give-up net, `SubshellSetupNet` in `view.rs`: armed when the first setup line is written, re-armed ONLY by
setup progress (`SubshellSetupProgress`: a setup-line marker of this setup's token, the probe's `InitSubshell`, `InitShell`, a
stage-2 chunk written, `Bootstrapped`; review H4: output bytes never re-arm it), with a 120 s absolute ceiling that stage 2 shares
with stage 1 and a persistent waiting notice after 10 s without progress (withdrawn by the next progress). Firing HANDS BACK: the
keys behind all three setup gates (`SubshellSetup`, `PowershellSetup`, `SubshellBootstrapWrite`) come back to the editor as text in
order with the Enter guard and a toast that gives their count (never their text), the unwritten setup lines are dropped, stashed
abort keys are dropped with a logged count (a timer never sends them), a pending subshell session is abandoned so the pane leaves
"Starting shell..." (the block is unhidden) and the session ID STAYS registered so a late `Bootstrapped` is accepted (review H6 (a)).
It replaces: `SUBSHELL_BOOTSTRAP_STALL_CEILING`'s key forwarding (`subshell_bootstrap_key_forwarded` forwards abort keys only;
`RouteFacts::subshell_bootstrap_stalled` is always false until step 3 removes the row condition), `SUBSHELL_SETUP_TYPED_HOLD_LIMIT`
(the generic hold and the PowerShell hold hold for as long as the setup is announced), the gates' own caps (`default_cap` is `None`
for the three setup gates; `POWERSHELL_GATE_LEAK_NET` is gone), and `SUBSHELL_SETUP_MARKER_GIVE_UP`'s timed writes (a marker that
never comes is no progress; nothing is written on a timer). `BOOTSTRAP_FAILED_DURATION` (7 s) is re-armed on every progress, so a
slow but healthy marker-paced setup does not trip it. The #1032 deferral and its 20 s notice timer (`SUBSHELL_SETUP_DEFER_NOTICE`) are
deleted (#1814); the judge's Wait and the setup net's notice replace them. "Stalled" for the PowerShell Ctrl-C route (#1051) is the net's notice having fired. Expectation changes, each
by the owner decision: typing is held however old the announcement (`subshell_announcement_applies_to_the_bootstrapped_parent_block_before_initshell`,
`subshell_bootstrap_announcement_survives_the_initshell_block_change`), a stalled bootstrap never forwards typing
(`a_stalled_subshell_bootstrap_still_holds_typing_the_net_hands_it_back` replaces `stalled_subshell_bootstrap_stops_holding_input`),
the stale-timer rule moved to the net (`a_stale_net_timer_does_not_release_a_later_setups_keys` replaces
`a_stale_hold_limit_timer_does_not_release_a_later_setups_keys`), the gate caps are gone
(`setup_gates_have_no_cap_of_their_own_the_net_is_the_one_net` replaces `bootstrap_gate_cap_outlasts_every_line_waiting_out_the_marker_give_up`),
and `stall_powershell_setup` (test helper) marks the net noticed instead of backdating the announcement. In an appci log:
`subshell_setup_net_armed`, `subshell_setup_progress kind=...`, `subshell_setup_given_up why=quiet_net|ceiling chunks=N` are the hops;
"writing the rest of the subshell setup on timers" no longer exists.

Review fixes (Opus, FIX-FIRST on the two commits): **F1** a Ctrl-C / Ctrl-D typed while the setup lines are in flight is the EVENT
that ends the setup (`abort_in_flight_subshell_setup`: unwritten lines dropped, the signal written at once as the user's event, the
setup over, the held keys behind the signal in order; it used to be stashed until the last line, which a stuck shell never wrote,
while the notice said "Ctrl-C skips setup"). Re-review: only the ABORT keys (^C / ^D, `is_abort_key`) end the setup; ^Z / ^\ (queue signals in `table` mode, Table row P1d) wait in order for the last line as in `legacy` (`SubshellPrep::HeldInOrder`); the abort also unregisters the probe ID; on a PowerShell setup the ^C turns the hold into `AfterAbort` and ONLY the bootstrap-write gate settles, so the held keys meet that hold and wait for the read marker (never written raw behind the ^C, #1372/#1438); `pending_aborts` / `send_pending_subshell_aborts` are deleted (nothing stashes an abort any more).
**F2** `setup_live` uses the APPLYING announcement (`subshell_bootstrap_announced_age`) and `subshell_bootstrap_pending`, the
post-Bootstrapped Precmd clears the net gate or no gate (`clear_subshell_setup_net_if_setup_over`), and a block completion with the
announced block gone drops the unwritten lines and clears the net (`end_subshell_setup_if_block_gone`). **F3** a subshell `InitShell`
with no net armed arms one (`on_subshell_init_shell`). **F4** the generation is the view's monotonic counter
(`subshell_setup_net_generation`), and the notice timer re-checks its generation AND that the quiet stretch is due. **N1** a PowerShell
held line (`LineRunning`) arms the net too; coordinator decision: a quietly running command must not have the typeahead bounced at 30 s (output is not progress), so `LineRunning` shows the notice at 10 s and hands back ONLY at the 120 s ceiling (`held_line`, its own notice: the setup succeeded, the command is slow); the prompt's `Precmd` ends the hold as before.
**N2** `give_up_setup_gates_to_editor` hands the gates back in the queue's order. **N3** `end_subshell_setup_unsupported` acts only on
a live attempt, a give-up unregisters the probe ID, and a late `InitSubshell` / `SubshellNotPhosphorizable` with no probe outstanding
is ignored. **N5** once the bootstrap give-up failed a setup, progress no longer re-arms it. **N4 (known, documented):** a given-up
setup leaves line 0 (the prelude) in the shell's tty / history: when the shell eventually reads it, `HISTCONTROL=ignorespace:...`
(bash) or `hist_ignore_space` on / `print_exit_value` off (zsh) stay changed for that shell, because the restore is part of the
probe line / the known-shell stage's last line that was never written. The user sees it only on a shell that then becomes usable
without Phosphor; `exit` ends it. Expectation change (F1, review decision): `ctrl_c_during_the_subshell_bootstrap_waits_for_its_last_line`
-> `ctrl_c_during_the_subshell_bootstrap_ends_it_and_goes_out_first` (`^C a b` whether or not announced; Legacy and Table);
`stall_ceiling_hand_back_defers_to_the_abandon_notice` (group2) -> `setup_net_hand_back_defers_to_its_own_counted_notice`.

| layer | test | asserts |
|---|---|---|
| view (pure) | `view_test.rs`: `subshell_setup_net_verdict_is_quiet_at_30_s_and_ceiling_at_120_s` | the truth table: Waiting / Quiet at 30 s quiet / Ceiling at 120 s even with progress 1 s ago |
| view, Legacy and Table | `view_test.rs`: `ctrl_c_during_a_stuck_marker_paced_setup_goes_out_at_once_and_ends_it` (REAL key path: `typed_characters_on_terminal`, `keydown_on_terminal(^C)`) | F1: `ec`,`ho` held, no marker, the notice up; ^C: the pty gets `\x03echo` at once, the probe line is never written, in-flight / announcement / net / notice / held counts cleared, editor empty, the probe ID unregistered |
| view, Legacy and Table | `view_test.rs`: `ctrl_c_during_the_subshell_bootstrap_ends_it_and_goes_out_first` (real key path) | `^C a b` announced or not |
| view, Table | `view_test.rs`: `table_mode_ctrl_z_during_the_bootstrap_lines_waits_in_order_and_does_not_end_the_setup` | F-a: ^Z (a queue signal in Table) during the in-flight write leaves the setup live and waits behind the keys; after the last line `a\x1a` follow it in order |
| view, Legacy and Table | `view_test.rs`: `a_ctrl_c_on_a_stuck_powershell_setup_keeps_the_held_keys_for_the_read_marker` | B-1: marker-paced PowerShell setup, noticed, `ls` held, ^C through the key path: the pty gets `\x03` only, the hold is `AfterAbort`, `ls` is still held (not written raw), the in-flight write is over |
| view | `view_test.rs`: `a_finished_setup_does_not_trip_the_net_later`, `a_subshell_that_exits_during_the_setup_ends_the_net` | F2: Bootstrapped + Precmd clears the net with a key still held (the key is delivered, stale timers do nothing, nothing written); a block change with nothing pending drops the lines and clears the net |
| view | `view_test.rs`: `an_init_shell_with_no_net_armed_arms_one` | F3: no net -> armed and progressed (seq 1); a net -> progress only |
| view | `view_test.rs`: `a_notice_timer_of_an_earlier_setup_does_not_toast_during_a_later_one` | F4: B's generation > A's and never 1 again; A's notice timer during B toasts nothing; B's not-yet-due timer toasts nothing; B's due timer toasts |
| view, Legacy and Table | `view_test.rs`: `a_powershell_held_line_is_watched_by_the_net_and_handed_back_only_at_the_ceiling` | N1: `LineRunning` + net; `Get-`,`Date` held; the quiet net leaves them held (hold stays `LineRunning`); the ceiling hands `Get-Date` back, hold Off, nothing written |
| view | `view_test.rs`: `setup_gates_are_handed_back_in_queue_order` | N2: `a` behind the older bootstrap-write gate, `b` behind the newer setup gate -> editor `ab` |
| view | `view_test.rs`: `a_give_up_unregisters_the_probe_id_and_a_late_probe_answer_is_ignored` | N3: after the quiet net the probe ID is unregistered; a late InitSubshell writes no stage 2; a late NotPhosphorizable writes nothing |
| view | `view_test.rs`: `the_bootstrap_give_up_fires_at_most_once_per_setup` | N5: after `on_bootstrap_failed_timer_complete` the net's `bootstrap_failed_after` is None and progress bumps no timer generation |
| view | `group2_timer_tests.rs`: `setup_net_hand_back_defers_to_its_own_counted_notice` | the `quiet_net` / `ceiling` whys show no generic toast; the counted notice's wording for 0 / 1 / n keys and `held_line` |
| view | `view_test.rs`: `the_setup_net_re_arms_only_on_progress_not_on_output` | output, a prompt shape and another setup's marker leave (gen, seq) and `last_progress` alone; the notice fires and progress withdraws it; our marker is progress once; `InitSubshell` + stage 2 is a new generation with the same start; `InitShell` and `Bootstrapped` count; output after them does not |
| view, Legacy and Table | `view_test.rs`: `the_setup_net_hands_the_held_keys_back_at_30_s_quiet_and_at_the_120_s_ceiling` | `ls`,` -la`,`x` held through five chatty outputs; the quiet net (backdated) and the ceiling (backdated, with a fresh marker) each hand back `ls -lax` with the guard armed, write nothing, clear announcement / in-flight / net / held counts / notice; late timers do nothing |
| view | `view_test.rs`: `a_stale_net_timer_does_not_release_a_later_setups_keys` | a timer of an earlier quiet stretch or an earlier generation does nothing; the current pair hands back once; nothing written |
| view | `view_test.rs`: `a_late_bootstrapped_after_the_net_is_accepted` | after `InitShell` the net abandons the pending session (kept as abandoned, block list bootstrapped), the ID stays registered, `bootstrapped()` consumes the abandoned session; the held key came back |
| view | `view_test.rs`: `a_healthy_marker_paced_setup_re_arms_the_bootstrap_give_up` | via `trigger_subshell_bootstrap_now` the net records 7 s; a marker bumps `bootstrap_timer_generation`, output does not; a setup written without the trigger re-arms nothing |
| view, Legacy and Table | `view_test.rs`: `powershell_setup_holds_keys_however_old_the_announcement_is_until_the_net` | every `PowerShellSetupHold` state holds with a 1000 s old announcement; the net hands `Get-Date` back, hold `Off`, nothing delivered |
| view | `view_test.rs`: `setup_gates_have_no_cap_of_their_own_the_net_is_the_one_net` | `default_cap` is `None` for the three setup gates; notice < quiet < ceiling |
| view (existing, kept green) | `subshell_setup_lines_follow_the_shells_markers_not_the_clock`, `powershell_setup_lines_follow_the_read_markers_not_the_clock`, `ctrl_c_during_the_subshell_bootstrap_waits_for_its_last_line`, `subshell_gate_give_ups_hand_back_without_losing_or_reordering`, `text_typed_during_a_stalled_subshell_setup_is_held_then_delivered_in_order`, the PowerShell Ctrl-C route tests and `group2_timer_tests.rs` | ordering, stall hold and Ctrl-C behaviour unchanged |

**#1679 / #1711 step 3, C3a (the readiness facts and the pure judge; owner-approved 2026-11-08, the Opus review R1-R10
wins where it and the redesign differ). ADDITIVE: nothing decides on it until C3b switches the setup to it.**
`model/subshell_tty.rs` holds the platform-free part: `SubshellTtySample { kind: Secret | Relay | LineReader | Cooked |
Unknown, leader: Shell | Relay | Unsupported | Other | Parent | Unreadable, leader_comm, canonical, queue_empty }`, the leader
lists (`SHELL_LEADERS`, `RELAY_LEADERS`, `UNSUPPORTED_LEADERS` (csh/tcsh/xonsh/elvish...), `EDITOR_LESS_SHELL_LEADERS`
(dash/sh/ash/busybox/ksh/mksh, R3)), `classify_leader` (login `-`, case and `.exe` normalized), `leader_has_line_editor`,
`shell_type_of_leader` (R5: `exec zsh` in a `.bashrc`) and `comm_from_proc_stat` (between the FIRST `(` and the LAST `)`).
`local_tty/terminal_attributes.rs` holds the OS probes: `subshell_tty_fact(fd, parent_pid)` (one `tcgetattr`, one
`tcgetpgrp`, at most ONE `/proc/<pgid>/stat` read or `proc_pidpath`; Secret > Relay/Unknown > LineReader > Cooked; the parent's
own group is `Unknown`/`Parent`; R10: the ioctls first, `/proc` only for a candidate, one sample per wakeup),
`slave_input_queue_empty(fd)` (R1, the consumption proof: Linux opens the slave peer with `TIOCGPTPEER` (fallback
`ptsname_r`+`open`) `O_NOCTTY|O_NONBLOCK`, `poll(0)`, `FIONREAD`, close; macOS `FIONREAD` on the master -- TO BE VERIFIED on
the Mac box) and `flush_slave_input(fd)` (R9, `TCIFLUSH` on the peer / the master). The session-reported parent pid: every
`InitShell` payload now carries `"pid"` (`$$`, `$fish_pid`, `$nu.pid`, `$PID`; `InitShellValue.pid: Option<u32>`, serde
default, the MSYS2 key-value form sends none), the model keeps `session_shell_pids` by session ID plus the spawn pid
(`set_local_shell_pid`), and `subshell_parent_pid()` is the active session's reported pid else the spawn pid (R5: never a
`tcgetpgrp` sample at a Precmd). The terminal manager installs `set_subshell_tty_probe(probe, flush)`; a test installs a
closure. `key_facts::output_tail_is_input_prompt` gains the owner's Q10 narrow choice group (`line_ends_in_choice_group`: a
trailing `[..]`/`(..)` of digits with `/` `-`, or 2-5 single letters with `/`; never a bare `)`/`]` or a bracketed word) and
the zsh-newuser-install footer (R6). `view/subshell_ready.rs` is the pure judge: `subshell_ready(sample, ledger, trigger)`
-> `Write(Option<ShellType>) | Wait(why) | Cancel(UserDrove | TooLong | Gone | NotAShell | Unsupported)` with
`PRE_READY_CEILING` 60 s (owner Q7); `on_user_line_ended` (R2: ReArm only for an empty line, a Secret, a cooked shell keeping
up with its reads or an InputPrompt tail; anything else is UserDrove); `LineTracker` (R2: "empty" = only the terminator;
ESC sequences, Tab, ^R, ^P, ^Y, history recall and a Backspace are content; bracketed paste understood across chunks, a
newline inside it ends no line; ^U and ^C end the line empty); `judge_written` (R7 stream-ordered E-UNSUPPORTED: echo ended,
then a fresh prompt-shaped line, no token-matched marker between; local confirms by the queue proof, a relay by a second
prompt; E-SECRET and E-GONE first, progress clears every suspicion, E-QUESTION needs an InputPrompt drawn since the write);
`held_keys_destination` (R8: Secret/Question/net/Gone -> editor; Unsupported -> pty only with a fresh prompt proven AND the
line consumed); `cancel_flushes_unread_setup` (R9). Consumption (R1 + owner Q8): a line editor's proof is the empty queue
(plus `!line_dirty`, which the preconditions already require); a cooked shell's or a relay's proof is the empty queue AND a
prompt drawn since the user's last line end -- "line ended + new prompt" never counts alone. An Enter never writes.

| layer | test | asserts |
|---|---|---|
| model (pure) | `model/subshell_tty_tests.rs`: `every_listed_leader_classifies_as_its_list_and_the_lists_do_not_overlap`, `login_shells_case_and_exe_suffixes_are_normalized`, `programs_that_are_neither_are_other`, `only_editor_less_shells_may_be_written_at_a_cooked_prompt`, `the_leaders_shell_type_names_the_scripted_shells_only`, `the_comm_is_between_the_first_open_and_the_last_close_parenthesis` | the lists, the normalization, the comm parse with nested parentheses |
| local_tty (real pty, Linux) | `terminal_attributes_tests.rs`: `the_slave_input_queue_proof_and_the_flush_read_a_real_pty` | on a pty pair the test owns: empty, a byte written by the master waits (`Some(false)`), the flush empties it unread, a byte the slave read is consumed; the sample of a pty with no foreground group is `Unknown` |
| model (pure) | `key_facts_tests.rs`: `choice_groups_and_the_zsh_newuser_footer_are_input_prompts` | `[1-3]`, `[1/2/3]`, `(y/n/a)`, `(a/b/c/d/e)`, the newuser footer are prompts; `(venv) ... $`, `[me@host ~]$`, `[main]`, `done)`, `(abcdef/ghij)`, six letters, `(a)`, `[]`, `[/]` are not |
| view (pure) | `view/subshell_ready_tests.rs`: `row_1_secret_waits` .. `row_15_alt_screen_waits`, `r3_*`, `r4_*`, `r5_*`, `the_preconditions_apply_in_order`, `exhaustive_a_write_needs_every_precondition_and_a_proof` (every kind x leader x canonical x queue x block x alt x dirty x tail x moved x moved-since-end x age x three ledgers x two triggers), `r2_only_an_empty_or_answering_line_re_arms`, `the_line_tracker_counts_only_the_terminator_as_empty`, `the_line_tracker_understands_bracketed_paste_across_chunks`, `judge_written_rows`, `r7_unsupported_is_stream_ordered_and_confirmed_by_the_queue_or_a_second_prompt`, `r8_held_keys_go_to_the_editor_except_a_proven_unsupported_prompt`, `r9_a_cancel_flushes_only_provably_unread_setup_bytes` | every row of the redesign's table with the review's corrections; the invariant that a Write needs every precondition and a proof and that a young attempt is never cancelled on a fact alone; the ceiling names NotAShell for an unknown leader and TooLong otherwise |
| view (#1847) | `terminal/view_test.rs`: `an_idle_dash_prompt_drawn_around_the_users_enter_is_judged_moved` (Legacy and Table; dash armed at col 0, the user's Enter, then the prompt as one burst, as two bursts, or already drawn before the key path runs; plus a control with no prompt that stays armed) | a ReArm takes its baselines from the position of the LAST look (`ArmedSubshellSetup::last_position`), not from a `try_lock` or a deferred re-take at the next look (`rebaseline_pending` is gone), so the output wakeup that carries a fresh `$ ` is judged `moved` and an idle cooked shell is written to (`source=output`); `subshell_sh_probe` is unchanged (it already waits for the event after the first line) |
| view + pure (#1847 option 2, OWNER DECISION 2026-11-09) | `subshell_ready_tests.rs`: `idle_untouched_editor_less_prompt_is_written_without_a_moved_prompt` (each condition false restores the wait) and the widened `exhaustive_a_write_needs_every_precondition_and_a_proof`; `terminal/view_test.rs`: `an_idle_dash_prompt_with_nothing_typed_is_written_on_the_look` (Legacy and Table; a typed byte keeps today's rule) | a Cooked editor-less shell with `tail == ShellPrompt`, `leader == Shell`, `queue_empty == Some(true)` and `bytes_since_arm == 0` gets `Write(None)` without `prompt_moved_since_baseline`; nothing else changed (relays, editor shells, any typed byte keep the old rules) |

**#1679 / #1711 step 3, C3b (the switch to the readiness judge; owner-approved 2026-11-08). There is NO rollback: owner decision 2026-11-09,
`PHOSPHOR_SUBSHELL_READY=0` and `PHOSPHOR_BOOTSTRAP_TIMED` are deleted with the first-prompt-shape wait (`AwaitingSubshellPrompt`,
its 5 s give-up that wrote the setup) and the timed bootstrap, because a rollback built on timers that write defeats the rule
that timers never write on the input path (#1814).**
`SubshellSetupState { Idle, Armed(ArmedSubshellSetup), Written(WrittenSubshellSetup) }` in `view.rs`. Arming (auto-warpify at
preexec, the rc-file snippet, a Phosphorize click: `arm_subshell_setup`, `arm_subshell_setup_from_banner`) holds NOTHING: keys
pass through, counted in the ledger (R1), and every line end is judged by R2 (`note_user_bytes_for_subshell_readiness` on the
sink: an empty or answering line re-arms the shape baseline, taken at once with `try_lock` or at the next look; any other
non-empty line is UserDrove and cancels on the key path). Judging runs on EVENTS only: every output wakeup
(`judge_subshell_setup_on_event("output")`), the hand-off's change (`"handoff"`) and the click, ONE model lock per look
(`take_subshell_pane_look`: block id, cursor position, the tail classified None / ShellPrompt / InputPrompt, alt screen, the tty
sample through the model's probe, echo state, marker count; R10). A Write runs the trigger's write NOW
(`write_armed_subshell_setup` -> `trigger_subshell_bootstrap_now`, which no longer has the old deferral checks, deleted in #1814: the judge
makes them) and the attempt becomes Written with the minted ID / token (`note_subshell_setup_written`; a stage-2 write after
`InitSubshell` refreshes the same attempt with a fresh token and fresh counts). Written: `judge_written_subshell_setup` at every
look: E-GONE, E-SECRET, E-QUESTION, and the stream-ordered E-UNSUPPORTED (our echo ended per the echo filter's new
`lines_ended`, then a fresh prompt-shaped line counted in `fresh_prompts_after_echo`, no token-matched marker between;
local attempts confirm by the queue proof with NO timer, a relay by a SECOND fresh prompt-shaped line with no marker between; the 2 s `UNSUPPORTED_CONFIRMATION` timer is gone (owner rule: timers never decide), an unconfirmed relay suspicion waits for that event or the one `SUBSHELL_SETUP_QUIET_NET` progress net, which only hands back; pinned by `subshell_ready_tests::r7_unsupported_is_stream_ordered_and_confirmed_by_the_queue_or_a_second_prompt`). R8 on the key path: `secret_read_on_key_path_cancels_written_setup` samples the termios probe at
each key while written and hands back before the key is offered to the gates, so it goes to the password. The ONE cancel,
`cancel_subshell_setup(why, proof)`: Armed -> Idle (auto-warpify aborted; TooLong/NotAShell toast "click Phosphorize",
Unsupported names the shell); Written -> the pending lines cut (`CancelPacedSetupWrites`), this attempt's ID unregistered (a late
hook of it is refused; the net's give-up keeps its ID for a late `Bootstrapped`, this does not), echo filter and marker cleared,
R9 flush of provably unread setup bytes (`cancel_flushes_unread_setup`: queue holds bytes AND no echo started), the held keys to
R8's destination (`held_keys_destination`: the editor as plain text with the Enter guard and a count-only toast
`subshell_setup_handed_back_toast`, never auto-submitted, never history; or the pty in order for a proven unsupported prompt),
the block unhidden; E-SECRET re-arms the same trigger. The base's review F1 removed abort stashing (an abort ends an in-flight
setup at once), so nothing is stashed for a cancel to send and the step-2 "Ctrl-C was not sent" toast has nothing to say.
Owner Q11: `start_tmux_control_mode_setup` keeps the direct write for tmux control mode but waits (`tmux_setup_pending`) for
the look that finds the alt screen off and the line clean. In an appci log the hops are `subshell_setup_armed trigger=`,
`subshell_ready_judged source= decision= kind= leader= tail= queue_empty=`, `subshell_ready_line_end verdict=`,
`subshell_setup_ready_write`, `subshell_written_judged verdict=`, `subshell_setup_cancelled why= dest=`; a
`subshell_setup_ready_write` with `decision=Write` on a `kind=Unknown` sample would be the bug.

| layer | test | asserts |
|---|---|---|
| view, Legacy and Table | `view_test.rs`: `subshell_setup_is_written_on_the_shells_proof_not_on_the_prompt_shape` | a `bash-5.1$ ` with an Unknown sample writes nothing; a LineReader sample with the queue empty writes once |
| view, Legacy and Table | `view_test.rs`: `rc_questions_re_arm_the_setup_and_the_answers_never_write_it` | two `read -p` questions through a cooked bash keeping up: each answer passes through exactly (`me\r`, `ops\r`), nothing held, the attempt stays armed with the ledger counting; written at the prompt once the line editor reads |
| view | `view_test.rs`: `a_users_own_command_cancels_the_armed_setup_and_nothing_is_written` | `ls\r` with an Unknown leader cancels (UserDrove); a later proof does not revive it; the pty got only `ls\r` |
| view | `view_test.rs`: `vim_typeahead_waits_through_the_alt_screen_and_the_setup_follows_it` | `vim f\r` into a cooked reading bash re-arms; vim (Unknown, alt screen) waits; after the alt screen closes and readline reads: written |
| view, Legacy and Table (parity (a)) | `view_test.rs`: `e_secret_hands_the_held_keys_back_in_the_same_wakeup_and_re_arms` | relay `sudo -s` at `# ` writes the probe; `sec`+`ret` held; a Secret sample at the next wakeup: ZERO further pty writes, editor `secret` with the guard, announcement / in-flight / net cleared, re-armed; `pw` typed next passes straight through |
| view, Legacy and Table (parity (b)) | `view_test.rs`: `e_question_cancels_and_later_keys_reach_the_pty_in_order_in_both_modes` | `Continue? [y/N] ` drawn after the write with no marker: Idle, `held` back in the editor, nothing written; `y`,`\r`,`ls` then reach the pty in order; the byte lists of both modes are equal |
| view | `view_test.rs`: `e_unsupported_is_stream_ordered_and_delivers_the_held_keys_at_the_fresh_prompt` | a prompt shape BEFORE the echo ended is no verdict; the echo through its LF then an error and a fresh `bash-5.1$ ` with the queue empty: Idle, the held `ls` delivered to the pty in order, editor empty, the attempt's ID unregistered |
| view | `view_test.rs`: `a_marker_between_the_echo_and_the_prompt_keeps_the_written_attempt` | our token's marker before the prompt is progress; another token's marker is not ours |
| view | `view_test.rs`: `the_banner_click_arms_and_the_pre_ready_ceiling_cancels_without_writing` | a click arms and judges; a second click resets `armed_at`; at 60 s with vim in front: Idle (NotAShell), nothing written before or after |
| view | `view_test.rs`: `a_relay_writes_on_its_first_prompt_only_with_a_raw_local_tty` | `/ # ` behind a relay with a COOKED local tty waits (R4); with a raw one the sh probe's prelude goes out |
| view | `view_test.rs`: `an_rc_file_trigger_writes_for_its_shell_once_ready` | a cooked bash (the rc running) waits; zle reading writes the rc trigger's zsh setup, Written with `shell_type` zsh |
| view | `view_test.rs`: `tmux_control_mode_setup_waits_for_the_alt_screen_to_close` | alt screen on: `tmux_setup_pending`, nothing written; the look after it closes writes directly, no judged attempt |
| view, Legacy and Table | `view_test.rs`: `a_password_prompt_gets_none_of_the_setup_and_the_prompt_after_it_does` | a `Password: ` prompt with a Secret fact writes nothing; the typed answer reaches the pty untouched; the answer and a Ctrl-C keep the attempt armed; the setup is written at the prompt that follows when the line editor reads (replaces the deleted #1032 deferral tests `subshell_setup_waits_behind_a_password_prompt_until_it_is_answered`, `ctrl_c_skips_a_deferred_subshell_setup`, `deferred_subshell_setup_is_released_by_the_output_after_the_answer`, and the deleted `the_rollback_switch_restores_the_deferral_and_the_first_prompt_path`) |
| view | `view_test.rs`: `the_readiness_look_reads_only_the_tail_of_the_output`, `input_prompt_detection` | a 5000-line output ending in `Password: ` is classified `InputPrompt` from the last `OUTPUT_TAIL_ROWS` rows; the prompt shapes (replace `deferred_subshell_setup_reads_only_the_tail_of_the_output` and `input_prompt_detection_and_deferred_setup_readiness`, whose `subshell_setup_defer_ready` asserts went with the 600 ms quiet clock) |
| view, Legacy and Table | `view/group4_prompt_tests.rs`: `subshell_bootstrap_waits_for_a_line_holding_delivered_keys` | an armed setup on a dirty line (Enter or ^U ends it) is a Wait, writes nothing, then writes once on the hand-off change event; ported from the deleted deferral (`subshell_setup_deferred`) form of the same test |
| view | `view_test.rs`: `subshell_bootstrap_waits_for_the_shells_proof_however_long_it_takes` | an auto-warpify attempt waits past the old 1 s with nothing written, a finished banner line is no proof, the proof writes exactly once (replaces `subshell_bootstrap_waits_for_the_first_prompt_however_long_it_takes`) |
| view | `view_test.rs`: `drain_model_startup_events` (helper) | the model events queued at window creation (the bootstrap block's `AfterBlockCompleted` / `Precmd`) release held setup keys and end an in-flight setup whose announcement does not apply; a subshell-setup test that crosses an update or an await drains them first, by a marker through the same FIFO channel (used by `the_sh_probe_is_marker_paced_not_timed`, `paste_during_the_subshell_bootstrap_then_keys_arrive_in_order_after_it`, the ctrl_c-during-setup tests, `table_mode_ctrl_z_during_the_bootstrap_lines_waits_in_order_and_does_not_end_the_setup`, `an_unsupported_subshell_ends_the_setup_and_delivers_the_held_keys_in_order`, `the_setup_net_re_arms_only_on_progress_not_on_output`) |
| view | run-4 test corrections | `readiness_toasts_carry_counts_never_text`: the 0-count toast says nothing was typed, so only n>0 carries "not run"; `the_setup_net_re_arms...`: the notice is judged by the quiet rule, so `last_progress` is backdated; `an_unsupported...`: the probe line contains the word `echo`, so the check is for the typed `echo` + Enter; `table_mode_ctrl_z...`: the keys stay behind the setup gate after the last line and follow when the setup ends (design: InitSubshell / Bootstrapped), not at the last line |
| view, shadow | `view_test.rs`: `shadow_mode_reports_no_untagged_disagreement_over_the_arrival_scenarios` (scenario `announced subshell setup asking for an answer`) | the table facts read an announced setup that governs input (`announced_age` Some, PowerShell hold off) as `SubshellPhase::SetupHeld`, because the legacy writer opens the setup gate with the first non-signal key (`prepare_subshell_gates`); read as `Announced` no row held an arrow (D6 `legacy=held table=pty`) after the Bypass removal (#1679 step 4). Table mode itself opens the gate before it reads its facts, so only the shadow comparison changed |
| local_tty | `terminal_attributes_tests.rs`: `the_slave_input_queue_proof_and_the_flush_read_a_real_pty` | the slave is made raw first: FIONREAD on a canonical tty counts only complete lines (the bytes a `read` would get), which is the kernel's behaviour, not the box's; the proof is for a raw line editor |
| view (pure) | `view_test.rs`: `readiness_toasts_carry_counts_never_text` | the hand-back toast names a count and "not run", never the text |
| appci | `cases_subshell_ready.py`: `subshell_ready_rc_questions`, `subshell_ready_vim_typeahead`, `subshell_ready_unsupported_tcsh`, `subshell_ready_secret_after_prompt`, `subshell_ready_relay_shape`, `subshell_ready_exec_fish_rc`, `subshell_ready_docker_t_without_i`, `subshell_ready_zsh_newuser_install`, `subshell_ready_rc_clear` (see the cases list) | real shells on a real pty: the write happens only on the proof, after the answers / vim / the menu, never on a shape; what was typed runs once in the right shell; the unsupported and relay-cooked cases write nothing |
| model (pure) | `setup_echo_filter_tests.rs`: `the_echo_counts_say_when_our_line_was_taken` | `lines_ended` counts lines echoed through `\n`, `started` flips at the first matched character or the whole prefix, a stand-down keeps the counts, `reset_counts` forgets them |

**#1679 / #1711 step 4, C4 (the Bypass removed in Legacy AND Table; owner decision 2026-11-08).** An announced subshell
setup whose output "awaits an answer" let typing straight through the setup gates (`SubshellPrep::Bypass` in
`prepare_subshell_gates`, the table's row `H7b` / `RouteFacts::subshell_awaiting_answer`): on macOS the setup's echo stops
mid-line while the shell is not reading and keys let through on its strength ran inside the hidden setup block. Both copies are
gone, in one commit: the paste-queue branch and the `Bypass` variant, `view.rs`'s arm, table.rs's `H7b` row and the bypass in
`setup_and_queue_holds` (a setup gate holds; the matrix / H8 and H8-queue decide the rest), `RouteFacts::subshell_awaiting_answer`
and the dead `subshell_bootstrap_stalled` (always false since #1711; `pty_typed` loses the term), `route_facts.rs`'s two field
inits (`awaits_an_answer` at :243 STAYS: the ssh ^D rule reads it), `dest_table.rs`'s comment, the fuzz target
`route_decide.rs`, `table_tests` / `table_enum_tests` (`EFFECT_ONLY_ROWS` is three rows; `ALL_ROWS` 34; 40 boolean facts).
`active_output_awaits_an_answer` stays for `ssh_connect_holds_eot`. What replaces the bypass: the readiness judge never writes
at an input prompt (step 3), and a question or secret read AFTER the first line is E-QUESTION / E-SECRET, which cancel the setup
and hand the held keys back in the same wakeup, so the answer typed next passes through with no gate open. The bypass is not coming back (the rollback switch is deleted, #1814).

| layer | test | asserts |
|---|---|---|
| view, Legacy and Table (expectation changed by the owner decision) | `view_test.rs`: `typed_input_during_an_announced_setup_is_held_whatever_the_output_shows` (replaces `typed_input_during_stalled_setup_passes_through_to_a_visible_prompt`, whose expectation was the bypass) | `secret` typed during an announced setup with `Password: `, `Continue? [y/N] ` or silent output is held (1 chunk, 6 bytes), nothing written; the setup's end delivers it once |
| view, Legacy and Table (parity) | `view_test.rs`: `e_secret_hands_the_held_keys_back_in_the_same_wakeup_and_re_arms`, `e_question_cancels_and_later_keys_reach_the_pty_in_order_in_both_modes` (step 3) | the parity tests (a) and (b) of the redesign: zero pty writes at the hand-back, the editor holds the text, equal byte lists after |
| view (shadow) | `view_test.rs`: `shadow_mode_reports_no_untagged_disagreement_over_the_arrival_scenarios`, new scenario `scenario_announced_subshell_setup_asking` | an announced setup at `Password: `: typed text, Enter and a control sequence are held by Legacy and by the table alike, zero untagged disagreements |
| table | `table_tests.rs`: `h7_*` (the H7b block replaced by "a setup gate with another gate open is H8"), `an_unpaced_cli_submit_is_held_behind_a_subshell_setup_like_a_key` (the asked case gone), `the_four_predicates_differ_*` (case (d) is the abort forwarding only), `origin_exemptions_hold`, `the_matrix_is_what_holds_content` (no bypass exemption) | no fact lets a key past the setup gates |
| table (enum, fuzz) | `table_enum_tests.rs` (`ALL_ROWS` 34, `EFFECT_ONLY_ROWS` 3, 40 boolean dims, `every_row_of_the_table_is_reachable`), `fuzz/fuzz_targets/route_decide.rs` | every remaining row reachable; the effect-only rows never decide |

**#1666 / #1667 (Tab completion give-up, busy-machine audit H6/H7).** Tab then a fast Enter holds the Enter for the local
completion (`TAB_COMPLETION_ENTER_BUDGET`, 150 ms of completer wall time, unchanged; backstop 500 ms, unchanged) and runs the
COMPLETED line when it lands. When either net fires first, the timer runs NOTHING (#1577 rule; it used to submit the visible
line, `cd proj` instead of `cd project-x/`): the wait is voided, the Enter is dropped, the late result is dropped, and:

- nothing was typed during the hold: the uncompleted line stays in the editor with the toast "Completion took too long, so the
  line was not run. Press Tab to complete it, or Enter to run it as shown." and a GLUE guard (`EnterGuardKind::Glue`) over the
  line. An Enter on the unchanged line runs it at once; an Enter after text was typed onto its end (`ls` → `cd projls`) is held
  once, runs nothing, keeps the buffer, with "Text was typed onto a line that was not run; check it and press Enter again"; a
  completion inserted into the line (the toast's Tab), an accepted menu item (completion or enum) or an accepted
  autosuggestion drops the guard, and an Enter that only accepts a highlighted menu item is neither held nor consumed by it;
  Ctrl-C, a clear, or an edit that leaves the buffer not starting with the line (or empty) drops it as stale; finishing the
  word by hand costs exactly one extra Enter, with the glue toast (pinned). Only this give-up arms the glue guard on the line
  alone (`GiveUp::line_guard`); the verdict give-up could opt in later (owner: deferred to the design pass);
- keys were typed during the hold: #1661 (owner decision 2026-11-05) they are DISCARDED, never handed back: the line stays with
  the same glue guard and the one toast is the completion toast followed by the count ("... — 4 keys typed after Enter were
  discarded"). A busy queue discards the same keys with the same toast when its deferred take runs.

Every held-back-Enter guard now carries its own toast, set when it is armed (`Input::arm_enter_guard`): ssh hand-backs keep the
ssh text, an unready shell's give-up and the other hand-backs say their own reason (the guard used to say "the SSH session ended"
for all of them). In an appci log the give-up is `enter_completion_give_up editor completion_cap` / `completion_backstop` (plus
`kh_enter_gate_give_up by=completion_cap` with a gate) and a held Enter is `enter_guard_held kind=..`. The appci `tab_enter` case
detects a give-up from the app log (`completion_give_ups`), reports it as a GIVE-UP note and the `completion_give_ups` metric,
clears the handed-back line with Ctrl-C so the next trial is not typed onto it, and FAILS the case on any give-up (each trial's
give-up is read from its own log window, its start to the next trial's start, so a late backstop give-up is not blamed on the
next trial); "ran the
UNCOMPLETED line" is now reserved for a stray file (self-test `tab_enter_completion_give_up_is_detected`).

| layer | test | asserts |
|---|---|---|
| input, Legacy and Table | `input_test.rs`: `test_tab_then_enter_timeout_hands_the_line_back_and_drops_late_result` (was `..._submits_visible_text_...`, whose expectation was the defect), `test_tab_completion_cap_counts_only_completer_compute` (expectation changed the same way) | cap spent: nothing executed, wait and gate gone, `git checkout fe` in the editor, glue guard armed, exactly one toast with the exact text; the late `feature` result neither completes nor runs it; the user's next Enter on the unchanged line runs `git checkout fe` |
| input, Legacy and Table | `input_test.rs`: `test_tab_then_enter_backstop_hands_the_line_back_and_runs_nothing`, `test_tab_then_enter_cap_then_backstop_on_one_wait_shows_one_toast` | the 500 ms backstop gives up the same way; cap then backstop on one wait: one hand-back, one toast |
| input, Legacy and Table | `input_test.rs`: `test_tab_completion_give_up_then_typing_onto_the_line_does_not_run_it_glued`, `test_tab_completion_give_up_then_tab_completes_and_enter_runs_it` | `ls` + Enter onto the handed-back line: nothing runs, buffer `git checkout fels` kept, the glue toast; the next Enter runs it. A Tab whose single match is inserted drops the guard and Enter runs `git checkout feature` |
| input, Legacy and Table | `input_test.rs`: `test_glue_guard_lets_an_enter_accept_a_completion_menu_item`, `test_glue_guard_is_dropped_by_an_accepted_autosuggestion` | `a` typed onto the line, a classic menu with a highlighted item: Enter accepts it (nothing runs, guard dropped), the next Enter runs the accepted line; an accepted autosuggestion drops the guard and Enter runs `git checkout feature` at once |
| input, Legacy and Table | `input_test.rs`: `test_glue_guard_judges_the_menu_after_a_pending_refilter_empties_it` | a Tab menu over a large result set (background refilter) whose first item is `fe` itself (classic completions write the item they cycle to into the line; cycling to `fea0` made it `git checkout fea0zz`, r18u23), `zz` typed onto the line with that item highlighted on the old list, Enter: the stale-menu drop and refilter flush run before the glue check, the emptied menu closes, the guard holds the Enter, nothing runs, `git checkout fezz` stays |
| input, Legacy and Table | `input_test.rs`: `test_glue_guard_is_dropped_when_the_line_is_cleared_or_edited_away`, `test_tab_completion_give_up_then_finishing_the_word_by_hand_costs_one_enter` | after a clear, `git checkout feature/foo` + Enter runs at once; after a Backspace (`git checkout f`) the guard is gone and `eature` + Enter runs; `ature` typed by hand: one Enter held with the glue toast, the second runs `git checkout feature` |
| input, Legacy and Table | `input_test.rs`: `test_tab_then_enter_completion_in_time_runs_the_completed_line_in_both_modes` | a completer within its cap is waited for and `git checkout feature` runs once |
| input, Legacy and Table | `input_test.rs`: `test_tab_then_enter_give_up_discards_the_keys_typed_after_it` (was `..._keeps_the_keys_typed_after_it_in_order`; expectation changed by the owner decision of 2026-11-05, #1661) | `l`,`s`,Enter,`pwd` typed behind the held Enter, then the cap: editor `git checkout fe`, glue guard, nothing executed or released, one toast with the count 4; text typed onto the line next holds the Enter with the glue toast |

**Busy-machine timer batch 1, report and @-menu nets (#1701, #1702, #1728; audit `docs/audits/2026-11-05-busy-machine-timers.md`).**
The report patiences behind a deferred Enter (`UNCOVERED_REPORT_PATIENCE`, `TYPEAHEAD_REPORT_PATIENCE`, `terminal/view.rs`) are the
10 s net (`YOUNG_HOLD_GIVE_UP`, was 3 s): since #1661 their give-up discards the keys typed after the Enter, and a report that comes
first releases the Enter as before. A deferred @-menu Enter waits 10 s (`SELECT_WAIT_GIVE_UP`, was 3 s) for its results; the give-up
applies nothing, keeps the typed `@...` text and shows the toast "Enter not applied: ..." instead of dropping the Enter with a log
line (#1728).

| layer | test | asserts |
|---|---|---|
| view, Legacy and Table | `group2_timer_tests.rs`: `a_report_several_seconds_late_still_releases_the_deferred_enter` (real 3.5 s wait) | both patiences >= 10 s; 3.5 s after the Enter, behind the hand-off's own report and behind an uncovered report, the Enter is still deferred (not given up), `ho 47` intact, the key typed after it still queued, nothing written; the late report then releases the Enter |
| input, Legacy and Table | `input_test.rs`: `test_at_picker_enter_give_up_keeps_the_text_and_says_so` | the @-menu wait is 10 s; at the give-up nothing is inserted, `@cbmention_x` stays, nothing runs, one "Enter not applied" toast; a second firing changes nothing |

**Busy-machine timer batch 1, ConPTY prompt hold (#1697, #1698).** The hold is switched off for the session only after
`CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF` (3) unsignalled precmd-wait misses in a row, counted once per hold, reset by any signal or a
new session (#1697; the 1.5 s cap stays: a miss only hands keys back, and a longer cap would slow every prompt of a shell that truly
never signals). A hold whose release an event already decided (signal, interactive latch, continuation prompt), or whose hand-off
writes are still in flight, is not given up at twice the cap while that release waits behind a busy queue; it is bounded by the 10 s
net from the decision (#1698). Hop `prompt_hold_expired` now logs `unsignalled_misses=N`. Changed expectations (the old ones encoded
#1697 / #1698): a single unsignalled miss no longer switches the hold off in `a_never_signalling_shell_hands_back_a_segment_with_an_enter`,
`an_overdue_hold_kept_for_the_events_is_still_released_by_the_signal`, `the_queued_hold_give_up_is_id_guarded_and_switches_the_hold_off`
and `a_hold_that_gave_up_is_off_for_the_session_until_a_new_shell` (they now reach the third miss);
`a_cap_firing_while_an_event_release_is_in_flight_is_not_a_give_up` no longer gives up at twice the cap.

| layer | test | asserts |
|---|---|---|
| model | `typeahead_handoff_tests.rs`: `the_first_unsignalled_conpty_miss_does_not_switch_the_hold_off`, `a_signal_resets_the_unsignalled_miss_count`, `one_overdue_hold_counts_as_one_unsignalled_miss` | misses 1 and 2 leave the hold on and the next prompt held; the 3rd switches it off; a signal resets the count; a hold kept overdue several times then expired counts once |
| view, Legacy and Table | `view_test.rs`: `the_queued_prompt_hold_gives_up_in_order_and_switches_itself_off` (now three prompts) | each miss hands `ab` back, nothing written; off only after the 3rd; then `c` goes by the live path |
| model | `typeahead_handoff_tests.rs`: `a_cap_firing_while_an_event_release_is_in_flight_is_not_a_give_up`, `a_hold_with_no_wait_and_no_release_keeps_its_bound_unless_writes_are_in_flight` | a decided release is `NotYet` at 2x cap and at 9 s, `GiveUp` at the decision + 10 s; with no release and no writes in flight the 2x-cap bound stands, writes in flight extend it to 10 s |

**Busy-machine timer batch 1, interrupt ^C (#1774).** Every interrupt path that writes its own ^C (Stop of the user's command, the ssh
warpify and tmux-install blocks' Interrupt, a rewind) cancels a deferred Enter first (`cancel_deferred_enter_before_interrupt`), as
`ctrl_c` and `interrupt_agent_command` do, so the ^C is not held behind its gate.

| layer | test | asserts |
|---|---|---|
| view, Legacy and Table | `view_test.rs`: `an_interrupt_ctrl_c_is_not_held_behind_a_deferred_enter` (Stop of a user command, ssh warpify Interrupt, tmux-install Interrupt, rewind) | with an Enter deferred and `x` typed behind it, each path writes exactly one ^C, `x` is never written, the deferred Enter is cancelled |

**#1661 (owner decision 2026-11-05): a give-up DROPS the keys typed after an Enter.** Every give-up of an Enter gate
(the route-settle net, the gate's leak net, an orphan gate's leak net, the verdict / completion give-ups, an unready shell's
15 s net and its gate-gone path, a void of the waiting Enter, the hand-off's report give-ups) closes the gate and DISCARDS what
waited behind it in every form (editor text, `PtyBytes`, pastes, Enters), with one toast: the give-up's reason, then
"N keys typed after Enter were discarded" (`typed_after_enter_discarded_toast`). Nothing is converted into editor text
(`text_of_entries_typed_after`, the busy hand-back's line break and its bookkeeping are deleted) and nothing is routed by a
timer. The line typed BEFORE an Enter that did not run stays in the editor, unrun, with its own guard (verdict: none, or the
glue guard when keys were discarded; completion: glue; unready shell: the hand-back guard). A busy queue gives the free
queue's result: the deferred `VoidEnterGate` carries the report (`EnterDiscard`) and discards the same keys with the same
toast (#1768: `cancel_unready_enter` and `cancel_multiline_dialog` now discard on a busy queue too). The route-settle net
(`AFTER_ENTER_REPLAY_PTY_WAIT`) is 10 s (was 300 ms) and the keys are released by events: the hand-off engaging or not
engaging, the shell's Preexec, the command's block starting, and the command's own block completing (its block id is
captured where the view starts the command, `note_command_started_for_enter_gate`). Tests that wait for the net itself
shorten it with `set_enter_route_wait_net_for_test`. In an appci log the discard is `enter_gate_typed_after_discarded keys=N`.
Round 2 (owner decisions A/B of 2026-11-05 and the Opus review): the young-Enter hold's cap (`PromptSeen`) and the paced
CLI-agent submit's cap (`CliSubmit`) discard the same way (`discard_gate_keys_after_enter`, hop
`gate_typed_after_enter_discarded`); a line typed before the young Enter in the held bytes stays in the editor, unrun, guarded.
The young hold's cap is a 10 s net (`YOUNG_HOLD_GIVE_UP`, `young_hold_give_up`; `EVENT_HOLD_GIVE_UP` 2 s -> 10 s), released by
its events. `UNREADY_ENTER_GIVE_UP` is 60 s. `ENTER_GATE_LEAK_NET` is 20 s (it runs from the Enter, so the 10 s route net,
from the submit, is the one that fires); `HOLD_BACKSTOP_LIMIT` stays 9 s. An editing key typed while keys are already held
behind a waiting Enter queues behind the gate in its `Held` form instead of voiding the Enter
(`InputQueue::enter_gate_holds_typed_keys`). The busy path's glue guard is decided by the take (`glue_if_discarded`), the
orphan-gate takes are ordered, discarded pty pastes leave the view's paste maps (`Event::DiscardedPtyPastes`), and an in-band
block is never captured as the Enter's command block. The `LineEditorActive` prompt hold is NOT changed here.
Round 3 (owner decision 2026-11-05: some caps are a hold's NORMAL end): the fish no-barrier young hold (`PrecmdMaybeInFlight`,
`EVENT_HOLD_GIVE_UP` 2 s) (the `TimingWindow` hold and its `YOUNG_ENTER_WINDOW` are gone, 2026-11-09) keep their short cap and the old hand-back to the editor (`young_hold_cap_is_normal_end`,
`young_hold_give_up`; the class is fixed when the hold starts, `young_hold_normal_end`, so a sequence-numbered Precmd parsed
while it waits does not turn its hand-back into a drop: `a_young_holds_normal_end_class_is_fixed_when_it_starts`); every other
young hold gives up at the 10 s net and drops (a hold whose record is gone, `young_hold_gone`, is a defect path and drops). A busy give-up of a young / CliSubmit gate is
ONE mailbox operation (`Deferred::DiscardGateKeysAfterEnter`, FIFO), so a key typed after it is not discarded. Discarding takes
also take the `PasteReading` gates opened inside what the Enter gate held (`InputQueue::close_gate_discarding`), reporting each
unread paste `PasteCancelled`, so nothing typed after the Enter drains when the read ends. A `Held` Tab (`\t`) released on an
editor route (the hand-off's editor-takeover window in legacy, the table's `EditorAction` plan) runs the editor's Tab
(completion), as typing would, through `apply_key_to_editor_after_handoff` (it was dropped); with no editor route it is dropped
like any sequence, and every earlier route (pty, approval card, agent-owned pty, setup, table re-holds) is unchanged; editing keys
with no tty form (Undo, Cut) still act at once and void a verdict / Tab wait (documented in `editor/view/mod.rs`). Known gap
#1658: a line kept by a young give-up while the hand-off stays pty-owned until the request decision.

| layer | test | asserts |
|---|---|---|
| input, Legacy and Table | `input_test.rs`: `test_route_wait_give_up_discards_the_keys_behind_a_submitted_enter` (was `..._hands_the_keys_..._to_the_editor`) | buffer empty, no guard, gate gone, nothing executed or released, toast with count 1 |
| input, Legacy and Table | `input_test.rs`: `test_enter_queue_keys_typed_behind_a_verdict_held_enter_keep_order_at_10ms_per_key` (expectation changed) | verdict cap with `echo hi`, Enter, `ls` behind: line `is it true` stays, glue guard, toast with count 10, nothing released |
| input, Legacy and Table | `input_test.rs`: `test_enter_queue_voided_enter_discards_the_keys_typed_behind_it` (was `..._keeps_the_typed_text_in_the_editor`) | Backspace on the waiting line voids the Enter: buffer `git checkout f`, `ls` discarded, toast "The Enter was cancelled — 2 keys ..." |
| input, Legacy and Table | `input_test.rs`: `test_unready_enter_give_up_keeps_the_line_discards_the_rest_and_runs_nothing`, `test_unready_enter_give_up_discards_a_paste_typed_behind_it`, `test_blind_enter_after_an_unready_give_up_is_held_by_the_guard` (expectations changed) | buffer `ssh fake`, toast with count 7, the paste discarded, the guard still holds the first Enter, nothing runs |
| input, Legacy and Table, free and busy queue | `input_test.rs`: `test_completion_give_up_on_a_busy_queue_discards_like_on_a_free_one`, `test_unready_enter_cancel_on_a_busy_queue_discards_like_on_a_free_one`, `multiline_dialog_cancel_on_a_busy_queue_discards_like_on_a_free_one` | a peek holds the queue so the take is deferred: the same buffer, guard and toast (none for ^C on an unready Enter, the dialog's own discard toast for the dialog) as on a free queue |
| view, Legacy and Table | `view_test.rs`: `keys_behind_a_run_enter_are_discarded_at_the_net_in_both_modes` (was `..._go_back_to_the_editor_at_the_cap_...`), `keys_behind_a_run_enter_follow_a_command_taken_after_two_seconds_in_both_modes`, `keys_behind_a_run_enter_are_released_by_its_block_completing_in_both_modes` | the net (shortened to 300 ms) writes nothing and leaves the editor empty; a command taken 2 s later (inside the real 10 s net) gets `ls\r` after it; the command's block completing releases `ls` to the editor with no timer |
| view, Legacy and Table | `group2_timer_tests.rs`: `enter_gate_leak_net_discards_the_keys_behind_the_enter` (was `..._hands_..._back`, now with a `PtyBytes` key too), `orphan_enter_gate_leak_net_discards_the_keys_behind_it`, `uncovered_report_patience_keeps_the_text_instead_of_running_the_enter` (expectation changed) | nothing in the editor, no guard, nothing written, the queue idle; the report give-up keeps `ho 47` and drops `ls` |
| input, Legacy and Table | `input_test.rs`: `test_backspace_behind_keys_typed_after_a_waiting_enter_does_not_void_it` (new); `test_enter_queue_voided_enter_discards_the_keys_typed_behind_it` (the void now comes from an edit of the line outside the editor's keys) | `git checkout fe⏎ls⌫`: the Enter still waits, `l`,`s`,`\x7f` held in order, the completed line runs once, no toast |
| view, Legacy and Table | `group2_timer_tests.rs`: `young_hold_cap_hands_the_enter_and_keys_back_with_the_guard_armed` (no-sequence window hold: normal end, original expectation restored in round 3), `young_hold_fish_no_barrier_cap_hands_the_keys_back`, `young_hold_cap_discards_the_enter_and_the_keys_after_it` and `young_hold_cap_leaves_the_line_typed_before_the_enter_in_the_shell` (sequence-path holds, `RequestDue`; r18u27: a young hold engages only on a write that starts with the Enter, so the line before it is in the shell's own line, never in the hold), `a_key_typed_after_a_busy_young_hold_give_up_is_not_discarded`, `cli_submit_cap_discards_the_keys_behind_it` (was `..._hands_..._back`); `view_test.rs`: `a_young_enter_and_its_keys_are_handed_back_when_no_prompt_comes` (normal end, unchanged from before #1661), `keys_behind_a_run_enter_are_released_by_the_real_block_completed_event_in_both_modes` (real `ExecuteCommand` capture and the model's own `BlockCompleted`); `typeahead_handoff_tests.rs`: `only_the_two_normal_end_young_holds_keep_their_short_cap`, `a_young_holds_normal_end_class_is_fixed_when_it_starts`; `input_test.rs`: `test_a_tab_held_behind_an_enter_runs_the_editors_tab_only_on_an_editor_route` (with and without the takeover window) | normal-end holds hand `\nls` back, guarded; sequence-path holds drop (nothing back, nothing written; `ls` typed before stays in the shell's line); a key typed after a busy give-up survives; caps 2 s / window / 10 s; a released Tab reaches `input_tab` once |
| view, Legacy and Table | `followup_tests.rs`: `deferred_enter_hand_back_guards_only_multi_line_buffers` (reworked); `view_test.rs`: `typeahead_handoff_holds_an_unfinished_line_behind_the_enter_and_its_give_up_discards_it` (was `..._returns_an_unfinished_held_line_to_the_editor_...`) (r18u27: the line typed after the deferred Enter used to reach the unready gate as a deferred programmatic insert that the give-up applied; `release_held_keys` now queues it behind that gate as typed text, so the give-up discards it and the readiness event releases it in order); inside the mailbox flush (a deferred `TakeGateKeys`) it offers in mailbox order (`queue_behind_enter_in_mailbox_flush`), so a later key already in the mailbox stays behind it (`keys_released_by_a_deferred_take_keep_typed_order_behind_a_waiting_enter`, Legacy and Table); never behind the multi-line dialog's gate; the `has_enter` branch's direct pty write past an unready gate is #1783 | the hand-off's deferred-Enter give-up keeps the line and its guard rule, discards the keys; the unready give-up leaves the editor empty |

**#1793 (a request never answered held every later Enter, so `<Enter>~.` in a managed ssh session was lost).** The
hand-off's `requests_outstanding` is decremented only by a report, so a request counted but never answered kept it above
zero for good. Reports are now counted where the model hands them to the view as a `Typeahead { shell_report: true }`
event (`EarlyOutput::reports_delivered`, so a report dropped on the alt screen or outside `ShellReported` mode is not
counted); typeahead matched from echo (`InputMatching`, `shell_report: false`) is neither counted there nor taken off the
request count by the view (`TypeaheadHandoff::on_input_matched_typeahead`). The written command's own Preexec, told by
its text (`PtyController` names it, `TerminalModel::preexec` passes the Preexec's), drops requests written before it that
were never answered (`hop=handoff_requests_eaten`, an INFO line too); a shell-started line, a Preexec naming only part of
the command (bash's preexec stops at the first `|`/`&&`: such a command settles nothing), a command sent by the PowerShell
insert chord (#1581, its text goes on the pipe), an empty line, or a command written while lines typed ahead are still
queued in the tty in front of it (`hop=handoff_settle_skipped why=lines_queued_ahead`: the first of those lines' Preexec
may carry the same text) settles nothing. Eaten requests leave the RTT window too. The uncovered-report hop and its
give-up warning print the request accounting; `hop=handoff_untracked_line_bytes` says when the line first got untracked
bytes. A bare Enter is NOT a pass-through of its own: a round-2 shortcut that wrote a raw `\r` for an empty Enter in a
managed ssh session was removed (owner, 2026-11-05) and gets a proper design after the release. Known gap until then:
`<Enter>~.` typed right after ssh connects, while the first prompt's request is still on its way, waits behind that
report like any Enter (#1104), and `~.` waits behind it.

| layer | test | asserts |
|---|---|---|
| unit | `typeahead_handoff_tests.rs`: `a_request_never_answered_is_settled_by_the_next_commands_own_preexec`, `answered_requests_are_not_settled_again_at_the_preexec`, `a_request_written_after_the_command_still_defers_the_enter`, `only_the_commands_own_preexec_settles_requests`, `a_command_written_behind_lines_queued_ahead_settles_nothing`, `input_matched_typeahead_neither_answers_nor_counts_as_a_handled_report` | settled at the command's own Preexec; a delivered report on its way to the view is not settled twice; a later request still defers (#1104); a shell-started line, a line queued ahead (other text), an empty line, a command with no named text or one sent by the insert chord settles nothing; typeahead `make\r` queued ahead, a request, then the editor's `make`: neither Preexec settles the request, its report still does; input-matched typeahead leaves the request count and the handled count alone |
| unit | `early_output_tests.rs`: `input_matched_typeahead_is_not_a_delivered_report` | an echo match sends `Typeahead { shell_report: false }` and leaves `reports_delivered` at 0; a shell report sends `shell_report: true` and counts one |
| model + controller | `pty_controller_tests.rs`: `requests_are_settled_only_by_the_written_commands_own_preexec_in_stream_order` | real `TerminalModel` and `PtyController`, hooks in stream order: never answered → settled; answered → not; a line ahead and a PS2 head → nothing; typeahead `make\r` → request → editor `make`: the typeahead line's Preexec and the command's own settle nothing; a report dropped outside `ShellReported` or on the alt screen is not counted (the request is settled); unsolicited reports are counted and eat nothing |
| input, Legacy and Table | `input_test.rs`: `test_bare_enter_then_ssh_escape_after_an_unanswered_request_in_a_managed_ssh_session` (round 1's test, restored with the round-2 shortcut removed) | managed ssh after a settled unanswered request: the bare Enter is not held, its command write (captured from the view's `ExecuteCommand`, the pty controller's input) comes first, then `~.`, once, in order, editor empty; with the prompt's request on its way the Enter waits and nothing (neither the Enter nor `~.`) reaches the pty |
| input, Legacy and Table | `input_test.rs`: `test_a_settled_unanswered_request_does_not_hold_a_typed_line_in_a_managed_ssh_session` | `ls` + Enter is not held after a settled unanswered request, and is held while the prompt's request is on its way |

**#1793 round 3 (`~.` after a prompt reached the remote shell, not ssh; owner decision 2026-11-06: the small fix, confined
to the escape path).** ssh takes `~X` only right after a `\r`/`\n` in its own input, and after every remote prompt the
last bytes a managed session's ssh read are the hand-off's legacy report request (`ESC i`). When the #1638 pass-through
sends an escape on an empty editor line, the view now writes `\r` immediately before it, in the same write and the same
pane queue, where the remote line is proven empty: the hand-off is `EditorOwned`, has no untracked line bytes, no request
outstanding, every parsed report handled, the last report emptied the line, and `PtyController`'s write count
(`PtyWriteCount`, bumped for every input message it hands the event loop) is still that request's own write (no key,
command, agent or setup write since; any Precmd, Preexec or new session voids the proof); the pane queue holds no keys;
no full-screen program, agent control or running command. Otherwise the escape goes as typed. The hop
`ssh_escape_cr` logs `added`, `skipped=<reason>`, or `skipped=after_own_escape` (ssh's last input was an escape we sent at
its line start: it stays there, so `~?` then `~.` needs no second `\r`). The `\r` runs an empty remote line, counted as a
bare Enter's: the hand-off goes `PtyOwned` and the routed `\r~X` counts its Enter, so the remote Precmd answers it and the
next request hands input back. When and whether the report request is written is unchanged.

| layer | test | asserts |
|---|---|---|
| unit | `typeahead_handoff_tests.rs`: `an_ssh_escape_on_a_proven_empty_line_gets_a_carriage_return_counted_as_a_bare_enter`, `an_ssh_escape_gets_no_carriage_return_unless_the_remote_line_is_proven_empty`, `the_pty_write_count_survives_a_reset_on_exit` | after `echo hi` and an answered empty report: `AddCr`, then `PtyOwned` with one Enter; a second escape with nothing written since needs none; the empty line's Precmd → `SendRequest` → report → `EditorOwned`, and the next escape gets its own `\r`; untracked bytes, an outstanding request, a report on its way, a write since, a Preexec, a new prompt, `PtyOwned`, or no report ever: skipped with the reason |
| input, Legacy and Table | `input_test.rs`: `test_ssh_escape_on_a_proven_empty_remote_line_goes_out_behind_a_carriage_return`, `test_ssh_escape_gets_no_carriage_return_unless_the_remote_line_is_proven_empty`, `test_enter_then_ssh_escape_right_after_connect_adds_no_second_carriage_return`, `test_ssh_escape_carriage_return_leaves_a_local_session_alone` | managed ssh after `echo hi` (and at the first prompt after connect) with an answered empty report: `~.` gives the one pty write `\r~.`, editor empty; untracked bytes / an outstanding request / a write since: `~.` alone; `<Enter>~.` at the first prompt: `\r` then `~.`, no second `\r`; a local session: `~.` stays editor text, nothing written |
| appci | `cases_coverage_a.py` `ssh_drop_reconnect`: arms `escape_connect_enter` (`<Enter>~.` at once at the first remote prompt) and `escape_connect_bare` (`~.` with no Enter), next to `escape` and `kill` | a local prompt within 20 s and an exact local line; on failure the `ssh_escape` hops (incl. `ssh_escape_cr`) are quoted; these arms are their own reconnect |

**#1391 (PS2 continued line).** bash and zsh send `ContinuationPrompt` (PS2 head / zle `CONTEXT=cont`) and the hand-off re-baselines
its counts on it; on Unix a stale count is also corrected from the sampled line-editor state (#1614). fish and PowerShell continue a
line inside their editor with no hook, so on ConPTY the 5 s logged give-up is still what resolves it. Evidence to look for in a log:
`hop=handoff_stale_counts_reset by=give_up shell_activity=.. markers_live=..` -- `Reading` with `markers_live=true` is a line continued
inside the editor (pwsh on ConPTY); `Unknown` is a bootstrap without markers.

### 2.14b Wave 1.5 small UI features: unit tests and cases

- `completions_while_typing_effective` (`settings/input.rs`): the as-you-type menu default is on for LOCAL sessions only; an explicit setting always wins. Real-app case `ui_completions_as_you_type_local` (terminal mode, skipped when the profile writes the option): typing a shared prefix without Tab shows both candidates (OCR).
- `no_helper_chip_visible` (`settings/input.rs`) and `ssh_no_helper_chip_tests.rs`: the chip shows unless the `terminal.remote.show_no_helper_chip` setting is off or the host is in `terminal.remote.no_helper_chip_dismissed_hosts` (case-insensitive). Lab case `lab_no_helper_chip` (2.15).
- `auth/local_identity_tests.rs` (#1806): display name (configured, then OS user, never "Test User"), stable default emoji, emoji/image-path/missing-file resolution. No real-app avatar case yet: the avatar only renders beside an agent query block, which needs a model scenario (open item).

### 2.14c "Report a problem" diagnostics bundle (Wave 1.5 #1)

A palette / Help-menu action (`workspace:report_problem`, `WorkspaceAction::ReportProblem`) writes ONE local zip (`$PHOSPHOR_DIAG_BUNDLE_DIR`, else Downloads, else the home directory) and reveals it: `README.txt`, `manifest.txt` (version, OS, build profile, enabled feature flags), `app-log-window.txt` (the last 15 minutes of the app log, scrubbed) and `settings-scrubbed.toml`. **Never uploaded: no network call and no send button** (owner rule); the toast says so. The older Settings > About "Export logs" zip is also local-only and unchanged (it is raw, not scrubbed).

| layer | where | what it proves |
|---|---|---|
| scrubber + builder unit tests | `crates/warp_logging/src/report_bundle_tests.rs` | home / user / host of the machine, other absolute paths, e-mail, `user@host`, IPs, URLs, tokens / API keys / Authorization / Bearer, typed text, command lines and quoted prompts are removed; `hop=` / `ms=` / `t=` timing lines and ssh lines survive byte for byte; scrubbing is idempotent; settings keep only bool / number / short enum values; the window keeps only the last 15 minutes (wrapped lines follow their parent); the zip has exactly the four entries and none of the planted leaks; the app's secret detector runs as a second pass |
| no network path | `report_bundle_tests.rs` `no_network_client_is_reachable_from_the_report_code`, `app/src/workspace/problem_report_tests.rs`, `script/check_outbound_redaction` section 4 | neither the collector nor the builder names a network client, URL opener or mail hook |
| real app | `appci/cases_coverage_b.py` `cb_report_problem` (terminal mode, full tier; Windows classified NOT_WINDOWS, winmap.py) | palette action writes the zip, it has the four entries and log lines, and contains neither the profile's home path nor the machine's host / user name; selftest `report_problem_zip_verdict` |

### 2.15 Fake ssh lab (`script/realapp/sshlab`, `appci/lab.py`, `appci/cases_lab.py`; Wave 0.5b)

The harness's ssh server used to be the same machine (`sshd.py`: `ctx.lines(remote=True)` reads `prof.rhome` directly), so latency, a second
login shell, a missing helper, a read-only home, MFA, a banner or a dropped link could not be tested. The lab is a podman container that is a
fake REMOTE: two sshds, a user per shell, scenario users, a network fault shaper and an HTTP control port. Everything is code that runs later
in CI; nothing in it is pinned to a machine, and no key, hostname or IP is in the repo.

Setup, standalone use, CI wiring, rebuild and troubleshooting are written up in `script/realapp/sshlab/README.md`.

**Pieces.**

| piece | what it does |
|---|---|
| `sshlab/Containerfile`, `versions.env` | debian trixie-slim by digest, apt from a snapshot.debian.org timestamp, nu and pwsh tarballs by sha256. Targets `full` (bash, zsh, fish, nu, pwsh, dash, busybox ash, rbash, git, bash-completion, sudo, iproute2, nftables, libpam-google-authenticator) and `bare` (bash + coreutils minus od/tr/base64; no scp/sftp/curl/wget; read-only rootfs; noexec /tmp). `versions.env` pins the image (`name:tag@sha256:digest`), the snapshot timestamp and the nu/pwsh tarballs (URL + sha256); **`labctl.py build` refuses any value that is missing, still says PLACEHOLDER or is not a digest/sha256/date**. `full` has the sftp subsystem (so the installer's scp/sftp fallback is testable); `bare` and `p_noinstall` remove it |
| `sshlab/rootfs/usr/local/lib/sshlab/users.tsv` + `usersdb.py` | `user | shell | profile | sshd Match fragment`. Drives `useradd`, `/etc/shells`, the `Match User` blocks and labd. Per-shell users `u_bash u_zsh u_fish u_nu u_pwsh u_dash u_ash u_rbash`; scenario users `p_banner p_rcerr p_rcprompt p_slowrc(4 s) p_colon(PS1 'lab:') p_question(PS1 'ok? ') p_sete p_exec(bash_profile execs zsh -l) p_rohome p_nohome p_tinypath p_password p_kbdint p_mfa(publickey + TOTP) p_slowauth(PAM sleeps 5 s) p_force p_nopty p_onesess(MaxSessions 1) p_noinstall(read-only ~/.phosphor, no outbound, no scp/sftp) p_sudo` |
| `sshlab/rootfs/usr/local/sbin/labd.py` | the daemon (run under `podman --init`): sshd :22 (host key A) and :2222 (host key B, the changed-host-key case), an echo probe on :7, and a stdlib HTTP server on :7000 with a per-start bearer token. `GET /health`, `POST /lease` (keys + the shell hook templates; resets every home; returns host keys A/B, the password, the TOTP secret), `POST/DELETE /fault`, `POST /drop {rst\|blackhole\|stall}`, `GET/DELETE /file` and `GET /ctl` (path-jailed to /work, /home, the ctl dir), `PUT /tree` (a directory or git fixture), `POST/DELETE /helper`, `POST /complete` (the SHELL's own candidates: bash-completion, zsh zpty, fish `complete -C`, pwsh `TabExpansion2`, nu `--ide-complete`), `GET /sshdlog?since=` (LogLevel VERBOSE: counts exec channels). 8 h TTL |
| `sshlab/rootfs/usr/local/lib/sshlab/netem.py` | the fault commands as pure data (unit-tested in selftest). Egress: `prio` qdisc + a u32 filter on the source ports 22/2222/7 into a `netem` band; ingress: `mirred` to `ifb0` with netem; the RTT is split half and half; delay, jitter, loss, rate. nftables refuses NEW outbound connections of the lab users. **Port 7000 is never shaped** |
| `sshlab/labctl.py` | the ONLY caller of podman. `build` (tag = sha256 of the sshlab tree, skipped if present, 3 tags kept, pruned by label), `start --run --flavor --ttl` (gc first; netavark bridge network `sshlab`; cap-drop all + NET_ADMIN SETUID SETGID CHOWN DAC_OVERRIDE FOWNER SYS_CHROOT AUDIT_WRITE KILL NET_BIND_SERVICE; pids/memory limits; labels incl. `expires`; random published ports), `stop --id <exact cid>` (must carry the sshlab label), `gc` (expired label, exact ids), `list`, `selftest`, and `remote` (the forced-command verb: only start/stop/list/gc, strictly shaped, the client's sshlab tree arrives as a tar on stdin) |
| `appci/lab.py` | the client: `Lab.start(local\|remote)`, lease/fault/drop/file/ctl/tree/helper/complete/sshd_since/stop, `write_ssh()` (the `Host lab-<user>` entries, forward slashes so the Windows clients read them, plus `lab-changedkey`), `PlainSsh` (the pty control), and the pure parts: matrix expansion, TOTP, the RTT gate, the paired verdict, sshd channel counts, `LocalRemote`/`LabRemote` |

**Loopback shaping (found on the box 2026-11-09).** podman's rootlessport forwards a published port by connecting to the lab ports over the container's LOOPBACK,
not through eth0, so a fault on eth0/ifb alone measured 0.2 ms at a requested 150 ms. `netem.py` therefore also shapes `lo` (a `prio` + the same u32 port filters on
sport and dport, half the RTT per crossing; a connection uses lo or eth0, never both). The `bare` flavor (read-only root) cannot change `/etc/shadow`, so its lease
reports `password_users: []`; the password cases use `full`. `u_pwsh` needs ICU (`libicu76` in the `full` image; the env switch is not inherited through sshd).
`onesess_refuses` takes ssh's "Session open refused by peer" on stderr as the evidence, because ssh then retries on a fresh connection that MaxSessions does not limit.
`prune_plan` dedupes rows (podman lists a multi-tag image once per tag, which used to prune a live tag).

**Why a bridge network and an RTT gate.** Rootless pasta can splice loopback traffic AROUND the container's tc, so a "150 ms" arm could silently run at
0 ms. The container sits on a netavark bridge, and before every arm the harness shapes the network, MEASURES the echo RTT (median of 15, port 7) and
compares it with the target +-15 ms (LAN: <= 15 ms). A mismatch makes the arm UNVERIFIABLE (status `noref`, never a pass).

**Fixed-port slots (Mac and Windows, since 2026-11-09).** Instead of a forced-command ssh to the box, the box runs PERSISTENT lab containers, one per client
platform, on fixed published ports, started and kept by `labctl.py serve` (a `systemd --user` service, `sshlab/sshlab-serve.service`, for the gitlab-runner
account; `sshlab-image` installs it). Scheme for a slot `name:SSH:CTL`: ssh `SSH`, host-key-B sshd `SSH+10`, echo probe `CTL+700`, control API `CTL`; defaults
`mac 22202/22212/7702/7002`, `windows 22203/22213/7703/7003` (all in 1024..32767, so they never collide with the random ports of local mode). The container
carries the label `sshlab.slot=<name>`, gets the token from the box file `~/.phosphor-sshlab/token` (mode 600) mounted read-only (never an argument, never in
`podman inspect`), and runs with `SSHLAB_EXCLUSIVE=1`: ONE lease owner at a time. `serve` reconciles on podman die/remove events plus a 5-minute safety re-check
(a supervisor, not the input path): it creates a missing slot, replaces it when the image tag (recorded by `build`) changes, its 24 h TTL expires or it died, and
defers while a lease is held. `serve --once` reconciles and exits (the `sshlab-image` job runs it after `build && selftest`). The client (`lab.py`, mode `fixed`)
reads `SSHLAB_BOX`/`SSHLAB_TOKEN` through `ci_env.get`, checks `/health`, leases with a per-client owner id (the lease returns host keys A and B, pinned in a
per-run known_hosts; never trust on first use) and releases in `finally` (`POST /release`: clears the fault, kills the lab users' sessions, empties the
authorized keys). A lease on a busy slot gets 409 and the lab cases SKIP with that reason; a lease also expires by itself after 3 h. No ssh to the box account is
involved. `SSHLAB_BOX` and `SSHLAB_TOKEN` are protected project variables, so only jobs on protected refs see them; elsewhere the cases SKIP with
"SSHLAB_TOKEN is not set". The token is never printed, logged or written to an artifact (selftest `lab_fixed_slots_ports_mode_and_no_token_leak`). The control API is plain HTTP on the LAN: keep the ports firewalled to it. Full setup and operation: `script/realapp/sshlab/README.md`.

**Safety.** The control token is passed to podman through the environment (never on the command line); at most 3 lab containers run at once; `stop --id` takes 12-64 hex characters and only touches containers carrying the sshlab label; the image tag hashes a normalised tree (LF line endings, a `#!` flag instead of file modes) so a Windows client's tar selects the same image. Loss in a `/fault` is applied per direction at half the given rate (the rate you give is the round-trip figure). Drops use one nft table each (`sshlab_drop_<n>`, chains `drop_in`/`drop_out`); connections are matched to a user through the sshd log's `Accepted ... port N` lines because sshd's processes are not readable inside the container. CI collection files lab job results under `lab-<platform>` so they never replace the ordinary app-test rows.

**Remote reads.** `ctx.remote` is `LocalRemote` (today: `prof.rhome`, `prof.rctl`) unless a lab arm swaps in `LabRemote` (HTTP: `/work/<user>` effect
files with a fallback to the home, the ctl dir's `rec.log`/`prompt.log`/`fish_rec.log`). `ctx.lines(remote=True)`, `prof.read(remote=True)` and
`CR.rm_remote` go through it; every existing `remote=True` case is untouched because `LocalRemote` is the default.

**The cases** (`appci/cases_lab.py`, `@lab_case(name, shells=, lat=, helper=, faults=, scenarios=, flavor=, modes=)` expands one body into one case per
arm, named `lab_<case>.<shell|scenario user>.<lan|150|300>.<nohelper|helper>[.<fault>]`, ordered helper-major so the app relaunches only when the
helper mode changes). They run ONLY when `--cases` names them (`--cases` now takes comma-separated fnmatch globs; the app-test-lab-* jobs pass
`'lab_*'`) and only the arms `APPCI_LAB_MATRIX` selects. `nohelper` is always the default and the first arm; the `helper` arm pre-installs
`APPCI_LAB_HELPER_BIN`. **[hook]** marks a check resting on an app log line the Wave 0 code adds (`hop=tab_insert`, `hop=tab_menu items=[..]`,
`hop=path_detect source=pushed ..` and `hop=cwd_listing ..`, #1801: ids and counts only); until the app logs it the check is UNVERIFIABLE, never a pass.

| case | arms | what it proves |
|---|---|---|
| `lab_tab_remote_dir` | bash zsh fish x LAN/150/300 x helper + 5 scenario users | a unique prefix + Tab completes a REMOTE directory (`&& pwd >> /work/u/tab.txt` writes the exact directory); an ambiguous prefix: the lab's own `/complete` candidates contain both, the app's menu [hook] equals them; Tab -> insert <= 1 RTT + 100 ms [hook]; Enter 30 ms after Tab runs the COMPLETED line |
| `lab_path_detect` | bash zsh fish x LAN/150/300 x helper + p_exec, p_tinypath | `cd` into a fixture directory (2 entries), then a subdirectory's name typed without Tab or Enter: the shell's pushed listing arrived (`hop=cwd_listing kind=L entries=2` [hook]) and detection used it (`hop=path_detect source=pushed entries=2 complete=true` [hook], no names logged); typing it opens ZERO new exec channels (sshd log) |
| `lab_chips` | bash zsh x LAN/150 x helper | the branch chip is on screen (OCR); at most one batched side call per prompt (exec channels per prompt in the sshd log; with no helper at least one) |
| `lab_enter_start` | 7 shells x LAN/150/300 x helper + p_sete, p_colon | 20 lines at 10-25 ms/key with an instant Enter, interleaved with a plain `ssh -tt` session of the same user: median(app) <= median(plain) + max(15 ms, 10 %) (owner 2026-11-08); the remote `rec.log` equals the typed lines exactly and the effect file has every line once in order. Time = the shell's own record (remote clock mapped by `/health`'s clock) minus the Enter. fish (whole-second stamps), dash and ash (no hook) are exactness-only |
| `lab_ssh_drop_reconnect` (terminal) | bash zsh x LAN/150 x helper, + `rst` and `stall10` arms | the `ssh_drop_reconnect` body (now `ssh_drop_body(ctx, r, host, variants)`) against `lab-<user>`; `rst`: the lab resets the connection and the pane returns to a local prompt, a reconnect works; `stall10`: the server session is SIGSTOPped for 10 s, the pane stays remote and the lines typed meanwhile arrive exactly, in order |
| `lab_ctrl_c_no_esc_i` | bash zsh fish x LAN/150 x helper | #1798/#1672: `sleep 30`, then Ctrl-C x1/x2/x3 at gaps 0/50/200 ms, then a typed echo: effect file exact; the shell received exactly [sleep 30, echo]; prompt.log grew one per command (bash exact); no ESC-i in rec.log; no "Session abandoned before bootstrap" in the app log; no visible `^[i` (OCR) |
| `lab_no_helper_chip` (terminal) | bash zsh x LAN x nohelper | the "shell mode" chip is on screen (OCR) in a no-helper ssh session, is not an ERROR banner, and a command typed after connecting runs exactly once |
| `lab_password_prompt` (#1535) | p_password, p_kbdint x LAN/150 x nohelper (scenario arms: `full` only) | the `ssh_password_prompt` body (`secret_prompt`) against a REAL sshd that asks for the unix password (password auth, and keyboard-interactive via PAM): nothing typed before the prompt, the secret typed after it logs in, the secret is in no rec.log and not on screen, a wrong secret ends the ssh and the next command runs in the LOCAL shell. The password is the throw-away one labd leased for the run |
| `lab_nu_agent_pager_env` (#1813) | u_nu x LAN x nohelper (terminal mode) | the same sequence as `test_nu_agent_pager_env` against a REAL remote nushell in the app's ssh session: marked lines are typed exactly as the Nu wrapper types an agent command; `cd`/`$env` persist, pagers set during, restored after success, after `exit 3` and after a nushell error. `selftest` `nu_pager_marker_matches_the_app` keeps the marker equal to `NU_PAGER_MARKER` and `nu.nu` |

The quick matrix is {bash, zsh, fish, nu} x {LAN, 150} x {nohelper, helper} (owner 2026-11-09; nu = the lab's pinned nushell 0.116.1, `versions.env`; P10 added the `fam_*`, `helper_*` and `nohelper_*` cases below: 252 of their 727 arms are in quick); `full` is 180 arms and is for a candidate/release run, not a fix round (docs 2.9).

**Helper and no-helper coverage (P10, `appci/cases_lab_helper.py`).** Owner 2026-11-09: the small helper `phosphor-remote` AND the no-helper (shell) arm are tested with the same
rigour. Cases: `lab_fam_<capability>` (one per backend `Family`; arms `nohelper` = shell backend over the out-of-band ControlMaster channel, `helper` = helper present), the lifecycle
cases `lab_helper_*`, the routing cases, and the no-helper gaps `lab_nohelper_*`. **The Rust source is the single source of truth**: the harness parses
`crates/remote_server/src/capability.rs` (`HELPER_CAPABILITIES`), the `Family` -> capability map in `backend/mod.rs`, and `backend/features.rs` (a family still in the
`remote_features!` stub list has no shell arm yet). A `helper` arm whose capability is not in `HELPER_CAPABILITIES` SKIPs with "family <x> not in HELPER_CAPABILITIES yet"; a `nohelper`
arm whose family has no shell arm SKIPs with "shell arm for <x> not implemented yet". Both flip to running by themselves when the Rust lands. `codebase_index` (legacy full daemon only)
has no case by design. The fam cases run the no-helper arm on ALL eight shells in the full matrix (quick: bash zsh fish nu), at LAN/150 (full adds 300). Checks per arm: helper arm: ZERO new
sshd exec channels while the feature runs and the app log route line says `helper`; no-helper arm: 1..prompts exec channels and route `shell`; both: the shell's own record holds
only lines the case typed (R1, never typing into the pty). **[hook]** `hop=backend_route family=<cap> backend=<helper|shell>` is a line the app does not log yet: until it does the route
check is UNVERIFIABLE (never a pass). Only `run_command`, `git_read` and `fs_basic` have a scripted driver (`DRIVERS`: cd into a git fixture, branch chip, `ls`); the other capabilities
report UNVERIFIABLE "no scripted app driver" until one is added. Mac/Windows use the same lab API (`Lab.ssh/file/tree/helper/sshd_since/drop`) and so run these through the fixed-port lab.

**Coverage guard (appci self-test `lab_helper_coverage_guard`).** Fails when a capability in `HELPER_CAPABILITIES` has no registered lab case with a `helper` arm, or a `Family`
(but `codebase_index`) has none with a `nohelper` arm. Cases in `cases_lab_remote.py` (P1/P3) satisfy it by calling `cases_lab_helper.declare_external(cap, helper_case=, nohelper_case=)`.

| capability / feature | helper arm | nohelper (shell) arm | state today |
|---|---|---|---|
| `run_command` | `lab_fam_run_command` | `lab_fam_run_command` (+ `lab_chips`, `lab_path_detect`, `lab_tab_remote_dir`) | runs; route line [hook] |
| `fs_basic` | `lab_fam_fs_basic` | `lab_fam_fs_basic` | helper arm runs; shell arm SKIP until implemented |
| `git_read` | `lab_fam_git_read` (+ `lab_helper_killed`, `lab_helper_route_unsupported`) | `lab_fam_git_read` | helper arm runs; shell SKIP |
| `file_write`, `ripgrep_search` | `lab_fam_<cap>` | `lab_fam_<cap>` | no scripted driver yet (UNVERIFIABLE); shell SKIP |
| `file_context`, `repo_metadata`, `buffers`, `diff_state`, `git_status`, `github`, `git_write`, `agent_context`, `codebase_chunks` | `lab_fam_<cap>` | `lab_fam_<cap>` | helper arm SKIP "not in HELPER_CAPABILITIES yet"; shell arm SKIP "not implemented yet" |
| helper lifecycle | `lab_helper_install` (upload/upgrade/never/p_noscp), `lab_helper_quiet_fallback` (p_rohome, p_noinstall), `lab_helper_digest_mismatch` (needs a build with pinned digests, `APPCI_APP_PINNED_DIGESTS=1`), `lab_helper_missing_asset`, `lab_helper_one_version` (live other-version process survives), `lab_helper_policy` (allowed list; "Don't ask me again" has no scripted click: UNVERIFIABLE), `lab_helper_idle_exit` (11 min, `APPCI_LAB_SLOW=1`), `lab_helper_reconnect` (rst, one daemon), `lab_helper_scp_fallback` (UNVERIFIABLE: no lab scenario fails only the in-band upload) | n/a | not run |
| routing | `lab_helper_route_unsupported` (needs `APPCI_LAB_UNSUPPORTED_HELPER_BIN`, a test helper answering UNSUPPORTED), `lab_helper_killed` (read falls back to shell; the write-not-retried half is UNVERIFIABLE until a scripted app write exists) | | not run |
| no-helper features | | `lab_tab_remote_dir` (Tab = labd /complete, Tab-Enter runs the completed line), `lab_nohelper_tab_hands_back` (stalled link: the typed line arrives once), `lab_path_detect` + `lab_chips` (zero extra exec channels), `lab_nohelper_listing_single_flight` ([hook] `hop=listing_inflight`), `lab_no_helper_chip` (all 8 shells), `lab_enter_start` (paired with plain ssh), `lab_nohelper_unavailable` (subshell inside ssh: "unavailable" message, remote rec.log holds only typed lines) | not run |

**Adding a case for a new helper capability.** (1) Add the capability to `HELPER_CAPABILITIES` and the `Family` in the Rust; `lab_fam_<cap>` is registered and un-SKIPped automatically
(the self-test fails until it is, if you add a Family it cannot see). (2) Add a driver to `DRIVERS` in `cases_lab_helper.py` that makes the REAL app use the feature in the lab
(type the command / open the UI; return prompts and what it saw); without it the case reports UNVERIFIABLE. (3) Make the app log `hop=backend_route family=<cap> backend=..` for
the call. (4) A case living in another file calls `declare_external`. (5) Add the row above.

**ssh coverage gap map (#1535).** The gap list was: (1) no plain-ssh latency control: `lab_enter_start` interleaves a plain `ssh -tt` session of the same lab user
and judges the app against it (and `latency_enter_ssh` is the same-machine control); (2) no disconnect/reconnect: `lab_ssh_drop_reconnect` (escape, kill -9, RST,
10 s stall) and `ssh_drop_reconnect`; (3) no password prompt: `ssh_password_prompt` (fake sshds) and `lab_password_prompt` (real sshd, password and PAM
keyboard-interactive); (4) no Windows OpenSSH-server path; and "integration `ssh.rs` never runs", which the candidate tier now plays (2.9, #1502). STILL OPEN: the
Windows OpenSSH-SERVER path (the lab and `fakesshd_win.py` give the Windows CLIENT a server, not Windows `sshd` itself: it needs the OpenSSH Server capability on the VM,
an environment decision), `ssh_drop_reconnect` on Windows, a TOTP (`p_mfa`) lab case (the code must be computed at typing time), and the passphrase prompt in the lab.

**Latency coverage gap map (#1548).** The gap list was: key -> paint on Linux/Windows (none); UI work <= 4 ms (invisible under the 8 ms log floor); completion,
tab-switch and new-tab budgets (none). Now: UI work is judged from the app's own `hop=ui_item` lines by `latency_key_log` (FAIL above max(2, 5 %) of keys at >= 4 ms; 16 ms
warns) and recorded for new tab / tab switch as `new_tab_ui_work` / `tab_switch_ui_work`; new tab, tab switch and the LOCAL Tab completion have screenshot-observed
bounds in `cb_ui_action_latency` (upper bounds, `APPCI_UI_ACTION_MAX_S`); the REMOTE Tab -> insert budget (<= 1 RTT + 100 ms) is `lab_tab_remote_dir`; Enter -> command
start over ssh against a paired plain `ssh -tt` is `lab_enter_start` (and `latency_enter_ssh` on the same-machine sshd); Ctrl-C and Enter are `ctrl_c_d` / `latency_enter`. STILL OPEN:
pixel-level key -> paint on Linux and Windows (needs a 240 Hz grab / Desktop Duplication capture tool; the owner declined it for now, 2026-11-05), and a hard
budget for completion/tab/new-tab (the screenshot bounds are loose first guesses until a few runs give a baseline).

**Jobs** (all manual, never part of the ordinary app-test jobs): `sshlab-image` (box runner, `resource_group: little-thing-sshlab`; play it after a change under
`script/realapp/sshlab/**`) runs `labctl.py build && labctl.py selftest` and then installs `labctl.py` into
`~gitlab-runner/.phosphor-sshlab/`; `app-test-lab-linux`, `app-test-lab-macos`, `app-test-lab-windows` run `--cases 'lab_*'` and share their platform's GUI
resource_group. Linux uses `APPCI_LAB=local`; Mac and Windows use `APPCI_LAB=remote`. The jobs select the cases with the existing `APPCI_CASES` variable (`mac/ci/app_test.sh` and `wayland_session.sh` already passed it; `windows/ci/app_test.ps1` now does too, and an unset `APPCI_CASES` changes nothing there). The run directory records `lab.json` (container id, run, expiry; no token, no
address); `run.py`'s `finally`, `teardown.sh` and `after_job.ps1` call `labctl stop --id` from it, and the `expires` label plus `labctl gc` (run at every
`start`) are the net behind that. `ssh_diag()` appends the lab's `/health` and the tail of its sshd log. Win32-OpenSSH has no ControlMaster, so
`winmap.py` skips `p_onesess` arms on Windows; the lab cases run in the Git Bash pane only.

`labctl.py selftest` (in `sshlab-image`): per user a login and shell identity, the RTT gate at 150 ms, the MFA login (SSH_ASKPASS gives the TOTP) and the
`p_onesess` refusal of a second session, then it asserts ZERO lab containers are left. Offline, `run.py --self-test` covers the pure pieces: matrix expansion
and selection, the `users.tsv` parse, tc/ifb/nft command generation, the TOTP vector, the RTT-gate and paired-verdict maths, the pin check, the forced-command
whitelist, the tar jail, the container argv, expiry/prune planning and labd's helpers.

**Owner setup checklist** (nothing here is done by the harness or by an agent):

1. [x] **Pins.** Done in `versions.env` (re-check them when the image is rebuilt; `labctl.py build` refuses a missing, placeholder or malformed value). Moving a pin changes the tree hash, so the next `sshlab-image` rebuilds.
2. [x] ~~**The forced-command key.**~~ NO LONGER NEEDED for Mac/Windows (fixed slots, above); kept for the legacy `APPCI_LAB=remote` mode only. Old text: On a Mac/Windows runner create a machine-local key (`ssh-keygen -t ed25519`, no passphrase, mode 600, outside any repo). In `~gitlab-runner/.ssh/authorized_keys` on the box add one line: `restrict,command="python3 /home/gitlab-runner/.phosphor-sshlab/labctl.py remote" ssh-ed25519 AAAA...` (the forced command reads the allowed verb from `SSH_ORIGINAL_COMMAND`: only start, stop, list, gc). Put the private key's path in that runner's environment, or in its machine-local `~/.config/phosphor-ci/ci.env`, as `APPCI_LAB_KEY` (with `APPCI_LAB=remote`).
3. [x] ~~**The box's host key file.**~~ NO LONGER NEEDED for the fixed mode (the host keys come from the authenticated lease). Legacy remote mode only. Old text: Put the box's ssh host key line (`box.example ssh-ed25519 AAAA...`) in a machine-local file on each Mac/Windows runner and set its path as `APPCI_LAB_KNOWN_HOSTS` (environment or `ci.env`). Remote mode fails closed (the lab cases SKIP with that reason) when it is unset: the harness never trusts a host key on first use.
3b. [x] **The `SSHLAB_BOX` and `SSHLAB_TOKEN` CI variables** (done 2026-11-09: both protected, the token masked; the box also has the same token in `~gitlab-runner/.phosphor-sshlab/token`, mode 600; no `user@` part in `SSHLAB_BOX` now). Old text for the box variable (the box account, `user@host`, as the Mac/Windows jobs reach it). Allowed by the owner as a project variable; it is never written in the repo. The jobs map it to `APPCI_LAB_BOX`. Alternatively set `APPCI_LAB_BOX` in each runner's machine-local `ci.env` (what the 2026-11-09 setup did; no project variable needed). The box is the build box (it runs podman).
4. [x] **Kernel modules** (DONE 2026-11-09: `/etc/modules-load.d/sshlab.conf` holds `sch_netem sch_prio cls_u32 sch_ingress act_mirred ifb sch_tbf`, one per line). Old text, only if the first `sshlab-image` selftest says `tc/ifb unavailable`.** labd's startup probe and the RTT gate show it in the job log. The modules (`sch_netem sch_prio cls_u32 sch_ingress act_mirred ifb sch_tbf`) exist but were not loaded on the box; a rootless container normally gets them auto-loaded. If it does not, add one file `/etc/modules-load.d/sshlab.conf` listing those modules (needs sudo; **not done by anyone but the owner**).
5. [ ] Run `sshlab-image` once by hand (manual job; it also installs and starts the `sshlab-serve` user service and the two slots), then `app-test-lab-linux` with `APPCI_LAB_MATRIX=quick`.
6. [ ] Helper arms: set `APPCI_LAB_HELPER_BIN` on the runner to a built remote-server binary (otherwise those arms report UNVERIFIABLE).
7. [ ] ksu / a KDC are phase 2 and are not in this lab.

### 2.16 Real-use replay (`appci/replay.py`, `appci/cases_replay.py`, `appci/fake_model.py`; Wave 1.5 #3)

One long, seeded "day of use" per session mode, registered as `lab_replay.bash.150.nohelper` (fake ssh lab at 150 ms netem, the no-helper default, bash). It
is MANUAL ONLY: job `app-test-replay-linux` (extends `app-test-lab-linux`; `APPCI_CASES=lab_replay*`, `APPCI_FAKE_MODEL=1`), never part of a `ci_seq.py` tier, no
schedule. The `lab_*` glob of the ordinary lab jobs does NOT sweep it in (run.py runs `lab_replay*` only when `--cases` names `lab_replay`). Knobs:
`APPCI_REPLAY_MINUTES` (default 15; the nightly plays it with 60; the case timeout follows it), the run's `--seed` / `APPCI_SEED` (same seed, minutes and mode =
the same steps), `APPCI_REPLAY_ALLOW` (extra comma-separated regexes for the ERROR allowlist). No lab, no fake model, or a bad config is a SKIP with the reason.

* **Terminal pass** (weighted random steps until the estimated time is used): a local command; `cd` (absolute, `..`, `/tmp`); Tab completion of a unique
  directory prefix; an ssh session to the lab (2-5 inner steps: remote command, `cd`, Tab, Ctrl-C at a random delay, `seq` big output); a local Ctrl-C at a random
  delay in `sleep 30; echo ...`; `seq 1 50000|200000`; `seq > file; cat file` (a large cat); a bash / dash subshell; `sudo -n` (skipped with a note where it does
  not work). Typing 10-25 ms/key, instant Enter.
* **Agent pass** (`fake_model.py` as the Ollama-protocol provider; `run.py` starts it when `APPCI_FAKE_MODEL=1` and no real provider is configured): prose; an
  approved command (card, focus Enter, approve Enter); a rejected command (card, focus Enter, Ctrl-C); Ctrl-C mid-stream on a `SLOW` turn (3 s hold).
* **Oracles:** (1) every effect file holds its token exactly once (a Ctrl-C'd or rejected step leaves nothing): no lost, doubled or misrouted key; (2) the exact typed
  lines arrive, in order, in the local and the remote `rec.log` (the Tab-completed line is compared with the trailing `/` normalised); (3) no app-log ERROR or
  panic outside `replay.ERROR_ALLOW_DEFAULT` + `APPCI_REPLAY_ALLOW`; (4) agent invariants from the `hop=` lines (`replay.judge_agent`): A1 every ask Enter is
  `hop=enter_begin input_type=AI` carrying its token and never `Shell`; A2 `hop=view_mode` never leaves `agent`; A3 at least one `[byop] opening stream` per ask;
  (5) latency per step kind (n, p50, p95, max) is RECORDED as the `replay_latency_s` metric and never gates.
* **Self-test:** `python3 script/realapp/appci/run.py --self-test` pins the scenario generator (determinism, length, coverage of every kind), the effect and
  ERROR/hop oracles, the line comparison, the config parser, the fake model's replies and its HTTP server, and the registry (the replay is not swept by `lab_*`).
* **Unverified until a box run:** the case has never been driven against a build; the first run may show harness faults (subshell readiness, the approval-card
  keys, the exact Tab-completed line) before it shows app bugs.

### 2.17 No network at startup, bundled agent plugins (`appci/cases_netless.py`, `appci/fakeclaude.py`, `fakecodex.py`, `fakegemini.py`; Wave N)

Owner rule: a default Phosphor makes no outbound connection by itself. What the app does for each source:

| source | behaviour |
|---|---|
| models.dev catalog | built-in catalog (gzip in the binary); `agents.warp_agent.models_dev_fetch` defaults to `false`; only the Providers page's "Refresh model list" button downloads, from `models_dev_url` (default the repo copy `https://gitlab.com/winters-phosphor/phosphor/-/raw/main/assets/models_dev/api.json`). The load starts in `on_page_selected`, not when the widget is built. |
| Claude Code plugin | shipped in `resources/bundled/claude_plugin` and registered as a local-folder marketplace; no GitHub or GitLab fetch |
| Codex, Gemini CLI, OpenCode plugins | shipped in `resources/bundled/{codex,gemini,opencode}_plugin` (owner decision 2026-11-09). Codex: `codex plugin marketplace add <folder>` + `codex plugin add warp@codex-warp`; Gemini: `gemini extensions install <folder> --consent` (an update is `uninstall` + `install`); OpenCode: a `file://<folder>` entry in the global `opencode.json(c)` `plugin` array (written by the move chip, shown in the install/update dialogs). No `git ls-remote` pre-flight, no `marketplace upgrade`, no `extensions update`, no npm/git spec |
| remote helper download | only after the user opens an ssh session (`RemoteServerController::on_session_connected`), and by default only after the modal prompt (`ssh_extension_install_mode = AlwaysAsk`); nothing at launch |
| MCP servers | only servers in a config file the user (or their tools) wrote; see `docs/manual/06-mcp-skills-and-rules.md` |

Unit tests: `models_dev_bundle_tests.rs` (Refresh fetches from the one URL whatever `models_dev_fetch` says; startup path never fetches when off; this replaces
`disabled_refresh_path_never_fetches_even_with_a_custom_url`, an expectation the owner changed in Wave N), `plugin_manager/claude_tests.rs` (every command Phosphor
runs is local, the shipped plugin has no oz part / Warp metadata / network call and its version equals `MINIMUM_PLUGIN_VERSION`, `truncate_chars` cuts by characters
under `LC_ALL=C`), `model/ansi/mod_test.rs` (`osc_777_*`: a `;` in the body, a non-UTF-8 byte is replaced and the param kept), `cli_agent_sessions/mod_tests.rs`
(the WARN for a malformed event carries sizes only). The same style of test covers the other three agents (`plugin_manager/codex_tests.rs`, `gemini_tests.rs`, `opencode_tests.rs`):
every command is local (`assert_all_local`: no http/github/gitlab/git@/ls-remote/upgrade/` update `), install/update/move without the bundled folder fails with "bundled ... not found"
and changes nothing, a remote registration (Warp's GitHub, Phosphor's old GitLab copy, an `@warp-dot-dev/opencode-warp` or `git+https` entry) and a bundle folder at an old install
location are replaced by the bundled folder, a developer's own local checkout/link is never touched, a failed add restores a previous *local* source and never re-fetches a remote one,
the shipped version equals `MINIMUM_PLUGIN_VERSION` (Codex also the version in `on-session-start.sh`), and the shipped folders contain no orchestration/oz part, Warp URL/metadata,
`http(s)://`, curl/wget or similar. These replace the tests of the GitLab URL, the `git ls-remote` pre-flight, `marketplace upgrade`, `extensions update` and the `git+https` spec
(an expectation the owner changed on 2026-11-09). The bundled Codex/Gemini hook scripts also cut long text by characters through `truncate_chars` (jq), not by bash byte substrings.

App cases (Linux only, `--cases 'zero_network_startup,claude_plugin_offline,agent_plugins_offline,network_gate_switch,network_gate_notice'`, terminal mode):

- **`zero_network_startup`**: relaunches the app under `strace -f -e trace=network` (`App.wrap`), idles `APPCI_ZNS_IDLE_S` seconds (default 120), and fails on any
  `connect`/`sendto`/`sendmsg` to a non-loopback inet address (`outbound_attempts`). The harness profile has no cloud account or provider key. SKIPS (not passes)
  when `strace -f` does not work on the runner (ptrace denied); a `unshare -rn` launch was rejected because the app's D-Bus and X11 sockets are abstract and
  a new network namespace cannot reach them.
  The scripted session proves the shell is in front after Escape (probe line; Escape does not close the settings tab) and closes the tab with the platform's close when it is not, so the ssh connect reaches the shell.
- **`claude_plugin_offline`**: inside `unshare -rn` (network blocked; without it the case is `noref`, not a pass), `fakeclaude.py` registers the bundled folder and
  installs `warp@claude-code-warp` (it refuses a non-local source and checks the hook scripts exist and are executable), then the plugin's real hook scripts run on
  synthetic Claude Code inputs (a prompt with `;`, multi-byte characters and 300+ characters; tool; permission request; stop). A fake `claude` in a pane prints the
  sequences; the app log must show one `CLI agent event received: agent=Claude event=<Type> body_bytes=<n>` line per event, no `Dropped a CLI agent event` line, and
  none of the prompt text. Needs `bash` and `jq`. The binary's `resources/bundled/claude_plugin` is used (else the checkout's; `APPCI_PLUGIN_DIR` overrides).

- **`network_gate_switch`** (round 7: the case, not the app, lost the allowed origin: its gate_enabled edits were `open(p, "w").write(set_key(open(p).read(), ...))`, which truncates the file before the read, so settings.toml became just the one key; fixed with `toml_text.edit_file`, which reads first, and the self-test `toml_edit_file_keeps_the_other_keys` also refuses that shape in any case file; the same bug sat in `cases_lab_helper.py`): relaunches the app on a rewritten `settings.toml` three times. With `network.gate_enabled` absent the app log has no `network gate: turned OFF`
  line (absent means on, the v0.1.17 behaviour); with `gate_enabled = false` it has exactly one; with `true` again it has none. The log must not contain the host of the
  `network.allowed_origins` entry the case writes (the log carries a host hash at most). The Settings > Network page's origin rows are driven by KEYBOARD through the page's focus ring (palette entry "Settings: Network: focus allowed origins", Tab to the origin's Remove, Delete; then the origin typed into the editor + Enter) and judged by the app's `hop=network_gate_ui action=remove_origin` / `add_origin` lines (host hash only), `settings.toml`, and the log never naming the host (#1830 follow-up; same helpers as `agent_gate_deny_allow`). Only the denial toast, its link and the log row's Allow button stay `UNVERIFIABLE` (status `noref`, never a pass) here, because no setting can make the gate refuse a request it allows; `agent_gate_deny_allow` and `network_gate_notice` trigger a real denial and judge them. Linux only (winmap lists the reason).
- **`network_gate_notice`** (agent mode only, needs `APPCI_FAKE_MODEL=1`, tesseract and the pointer): the fake model answers `Please show this image: <url>` with a markdown image of the TEST-NET host `192.0.2.1:8099`. Remote images load through `asset_cache::fetch_file_to_memory`, which calls the gate with purpose `remote-asset` (allowlist only), so the gate refuses before any connection: the app log must show `network gate: denied "remote-asset" (host hash <fnv>)` and no host or URL. The case finds the toast's "Allow" by OCR and clicks it, then `settings.toml` must hold `192.0.2.1:8099` exactly once and a second image from that origin must not be refused; finally `gate_enabled=false` is written, the app relaunched, and a third image must not be refused (log `network gate: turned OFF`). Without OCR or a pointer the click, write and retry steps are reported `UNVERIFIABLE`. Terminal mode is not run: the toast belongs to the workspace, not the input mode.

- **`agent_plugins_offline`** (`--cases agent_plugins_offline`): the same for Codex, Gemini CLI and OpenCode. Inside `unshare -rn`, `fakecodex.py` (`plugin marketplace add <folder>`,
  `plugin add warp@codex-warp`; it records `source_type = "local"` and refuses a remote source) and `fakegemini.py` (`extensions install <folder>`; copies the folder, records
  `type: local`, refuses a remote source) install the bundled folders, then each plugin's real hook scripts run under a pty (they write to `/dev/tty`) on synthetic inputs
  (session start, a prompt with `;`, multi-byte characters and 300+ characters, tool, permission request, stop) and must write exactly one `warp://cli-agent` sequence each.
  OpenCode has no install command, so its bundled `dist/notify.js` + `dist/payload.js` are run with `node` (skipped as `noref` when node is missing). A fake `codex` and a fake `opencode`
  in a pane then print the recorded sequences; the app log must show one `CLI agent event received: agent=<Codex|OpenCode> event=<Type>` line per event, no `Dropped a CLI agent event`
  and none of the prompt text. Gemini's events are not fed to the app: `GeminiNotifications` is an opt-in flag. Needs `bash`, `jq`. `APPCI_<AGENT>_PLUGIN_DIR` overrides a folder.


#### Network gate (`crates/network_gate`)

Beyond "nothing at startup", every app-initiated request goes through one runtime gate: `network_gate::check_url(url, purpose)` plus the one client builder
(`network_gate::builder`, used by `http_client::Client::for_purpose`). Rules: a host passes when it is a loopback IP literal, a configured origin (BYOP providers
incl. Ollama and the local proxy, the models.dev URL, a custom proxy) or in `network.allowed_origins`, or a host the purpose's user action names; a name that is
loopback by name (`localhost`) or RESOLVES to loopback is refused unless configured; every redirect hop is re-checked; system proxy env vars are ignored (only a proxy
set in Phosphor settings is used, so proxy mode `system` behaves like `off`); a denial logs the purpose and a host hash, never the URL. Default mode: background /
unclassified requests are allowlist-only; the user-action purposes (models.dev Refresh, update check, network test, LSP install) add their fixed hosts; the agent's
web tools may reach any public host; MCP servers the user configured use their own URL. Unit tests (`crates/network_gate/src/lib_tests.rs`): redirect to a
non-allowed host denied, proxy applier used only when configured, name to loopback refused, configured endpoints pass (port-exact), user actions reach only their hosts
and subdomains, public web refuses private/loopback literals, a denial leaks neither URL nor host. Guard: `script/check_network_gate`.

The user can turn the gate off (Settings > Network > Network gate, `network.gate_enabled`, default true, never synced). Off: `check_url`, `check_redirect` and the resolver's
loopback refusal return Ok (the redirect-hop cap stays); proxy rules are untouched. The switch change is logged once (`network gate: turned OFF/ON`); passes while off are
logged at debug with a host hash only. Every denial, every gate-off pass and every allowed non-loopback request is also a `GateEvent` (purpose, host for the local UI only,
host hash, outcome) delivered to `set_event_listener`; its `Debug`/`Display` never print the host. The app keeps the newest 200 in `NetworkGateLog`
(`app/src/network_gate_log.rs`, memory only, never serialised or uploaded), shows them on the Network page with an "Allow" button on denied rows, and `NetworkGateNotice`
(`app/src/network_gate_notice.rs`) raises a toast with an "Allow this origin" link (`WorkspaceAction::AllowNetworkOrigin`); a repeat denial replaces the toast (same object id,
no clock). Unit tests: `crates/network_gate/src/lib_tests.rs` (switch off allows an unlisted host, a loopback name and a redirect; back on denies; the event records denials,
gate-off passes and non-loopback allows only; event text never has the host; the redirect-cap event has no host), `app/src/network_gate_log_tests.rs` (cap 200, order, clear,
`allow_text`), `app/src/network_gate_notice_tests.rs` (repeat denial = same toast id; no Allow link without a host), `app/src/settings/network_tests.rs` (`gate_enabled`
defaults true, absent = true), `app/src/settings_view/network_page_tests.rs` (origin add/remove normalisation, add twice adds once, the stored text parses as the gate parses it).
Upgrade row U21a covers the new key.

##### Agent network gate cases (`cases_agent_netgate.py`, #1830)

Agent mode, Linux only (live edits of the Linux profile's `settings.toml`, pointer + OCR for the toast; winmap lists the reason; Mac/Windows not applicable until the same
paths exist). They need `APPCI_FAKE_MODEL=1`. The trick that makes the gate refuse something in the real app: a provider URL is always a configured origin, but a loopback NAME
(`localhost`) is refused unless configured, so the fake model's markdown image `http://localhost:<port>/...` (purpose `remote-asset`, allowlist only) is denied while a LOCAL
LISTENER (`Capture`: 127.0.0.1 and ::1 on one port, records method/path/headers/body) shows whether anything left the app. Every image uses a fresh token (asset cache).
- **`agent_gate_deny_allow`**: denied -> app log `network gate: denied "remote-asset" (host hash <fnv>)`, no host/port/URL on any gate line, listener got nothing; the toast's Allow
  (OCR + click) -> `settings.toml` holds the origin once and the retry reaches the listener; then `allowed_origins` edited in `settings.toml` with the app RUNNING: removed ->
  refused again and the listener silent, added -> reaches. No restart anywhere. The Settings > Network page is driven by KEYBOARD (no noref): the command palette entry
  "Settings: Network: focus allowed origins" puts the page's focus ring on the origin editor; Tab / Shift-Tab walk editor -> each origin's Remove -> each denied log
  row's Allow (the focused row is marked `> `); Enter in the editor adds the typed origin, Enter/Space on Allow/Remove acts, Delete/Backspace on Remove removes, Escape returns to the settings
  search box. Every action logs `hop=network_gate_ui action=add_origin|remove_origin|allow_from_log|toggle_gate origin_hash=<fnv of the host>` (hashes only), judged together with settings.toml
  and the listener. The file-edit variant stays as an extra live-reload check, and a POINTER pass repeats the steps by OCR + click (toast Allow, page Remove, log Allow, editor + Add, and in
  `agent_gate_off_live` the gate switch, whose x offset is found by trying a few and reading the `toggle_gate` hop). A control the OCR cannot find is `UNVERIFIABLE`, never a pass.
- **`agent_gate_off_live`**: `network.gate_enabled=false` written live -> one `network gate: turned OFF`, the request reaches the listener, no denial; `true` -> `turned ON` and refused again, listener silent.
- **`agent_gate_key_never_leaves`**: the fake-model provider carries an `X-Appci-Key` extra header as the stand-in provider key (the key store is not scriptable). Precondition: the
  fake model (`fake_model.ACTIVE.headers`) received it, else `noref`. Then images from a denied origin, a live-allowed origin, an allowed `127.0.0.1` origin that 302-redirects to a denied
  `localhost` origin (the target must get nothing), and a gate-off request: no listener may see the sentinel (header, path, body) or any Authorization/x-api-key header; the app log has
  no sentinel and no gate line names a host, port or key.
Whole-case network oracle (all three cases, `NetOracle`): the app is relaunched under `strace -f -qq -e trace=connect,sendto,sendmsg -o <run>/netgate_<case>.strace`; at the end every
destination must be loopback (127.*, ::1, ::ffff:127.*) or a unix/netlink socket, anything else fails naming the syscall and address (`non_local_destinations`). Without a working strace the
step is `UNVERIFIABLE` (noref), never a pass. `zero_network_startup` still covers the idle app. Self-test rows: `agent_netgate_*` (incl. `agent_netgate_strace_destination_parser`), `fake_model_records_request_headers`.
Unit tests for the page's keyboard ring (`app/src/settings_view/network_page_tests.rs`): ring order and wrap, Enter/Delete mapping per stop, origin hash = gate host hash, add/remove/allow text.

`zero_network_startup` also drives a scripted session after the idle (`scripted_session`): the settings page is opened with its shortcut and walked with the keyboard
(`APPCI_ZNS_PAGES` Down presses, default 12, the AI page among them; best effort, screenshots `zns_settings_*`), then an ssh connect to the fake host and `exit`. The
trace must still contain no inet destination beyond loopback + the fake ssh host address (+ `APPCI_ZNS_ALLOW`, comma separated). The fake ssh host is on loopback
unless the lab supplies another address.

`run.py --self-test` also runs `wn_agent_plugins_offline` (the fake CLIs refuse a remote source; all three bundles install and emit five events each without a namespace).

`run.py --self-test` runs `wn_zero_network_trace_and_offline_plugin`: the strace parser on sample lines (and its allowlist argument), `fakeclaude.py` refusing a remote source, and the same
install + hook run (without a network namespace) against the checkout's plugin folder.

### 2.18 Upgrade from v0.1.17 (`appci/cases_upgrade.py`, `upgrade_table.py`, `prev_release.py`, `fixtures/upgrade_v0_1_17/`; #1546)

App case `upgrade_from_v0_1_17` (terminal mode, **full tier**: played by the candidate and release tiers' full app matrix on Linux, not in SMOKE/fix; no CI
schedule was added; `ci_seq.py` needs no entry because the tiers play every non-SMOKE case). Linux only for now: the seed paths (`~/.config/phosphor`,
`~/.local/share/phosphor`, `~/.claude`), the `secret-tool` keyring helper and the AppImage fetch are the Linux ones; macOS and Windows skip with that
reason (`winmap.py`), and the table and fixture are platform-neutral so the port is the seeding only.

**What it does.** It stops the app, wipes the profile's state directories, writes the fixture (`fixtures/upgrade_v0_1_17/`: `settings.toml` in the v0.1.17 shape
and owner-like (agent mode default, autodetect + natural-language detection on, a local model provider plus a local Claude proxy, vertical tabs, bash, a custom
proxy, redaction patterns set but redaction OFF, a keybinding, a user theme, the removed tmux key, an explicit `models_dev_fetch = true`), a Claude Code plugin
registered from Warp's GitHub, and two keyring items), then:

1. **Old profile.** The real v0.1.17 build is launched on it so it writes its own state (SQLite, config) and is stopped. The build is `APPCI_PREV_RELEASE_BIN`
   (an executable), else the cache `APPCI_PREV_RELEASE_DIR` (default `~/.cache/phosphor-ci/prev-release/v0.1.17/`, an extracted AppImage's `squashfs-root/AppRun`),
   else a download of the release's `Phosphor-x86_64.AppImage` from the GitLab generic package registry, checked against the release's `SHA256SUMS` and
   extracted with `--appimage-extract` (no FUSE, no `/tmp`; `APPCI_PREV_RELEASE_FETCH=0` turns the download off). Without a build the case runs from the fixture alone
   (metric `old_profile_source` says which) and SKIPS only when the fixture directory is missing. Rows are judged against what the OLD build left in `settings.toml`
   (a key v0.1.17 itself dropped is the metric `seed_keys_dropped_by_v0_1_17_itself`, not an upgrade failure).
2. **Launch 1 (current build).** Reaches a prompt, takes 4 exact typed lines, the app log has no panic / migration / database / settings / keybinding / theme /
   plugin / keyring / provider ERROR (`upgrade_log_problems`; a loopback model server that is not running is not one), and every `kept`, `unknown_kept`,
   `absent_or_default`, `file_same` and `db_migrations` row below holds. The keyring items are read back with `secret-tool` after the stop.
3. **Launch 2** with `PHOSPHOR_BOOTSTRAP_TIMED=1`, `PHOSPHOR_SSH_BOOTSTRAP_GIVEUP_DELIVERS=1`, `PHOSPHOR_SUBSHELL_READY=0` set (rows U70-U72): it starts, logs no
   failure, and **changes nothing**: `settings.toml` byte for byte, every file under the config dir and `~/.claude`, the keyring items.
4. **Downgrade note.** With the real v0.1.17 build: v0.1.17 is opened on the migrated profile and what it did is RECORDED (metric
   `downgrade_v0_1_17_on_migrated_profile`: reached a prompt, seeded keys it lost, problem lines; a `DOWNGRADE (recorded)` note). Never a failure.

The case leaves the harness profile as it found it (settings.toml restored, `~/.claude` put back, state directories wiped like `upgrade_over_previous` does,
a fresh app started). New environment variables: `APPCI_PREV_RELEASE_BIN`, `APPCI_PREV_RELEASE_DIR`, `APPCI_PREV_RELEASE_FETCH`, `APPCI_KEYRING_SERVICE`
(default `dev.phosphor.Phosphor`, the `service` attribute the app's Secret Service items carry). Without `secret-tool` + `dbus-run-session` +
`gnome-keyring-daemon` the keyring rows are a note, not a pass.

**The table.** One list, `upgrade_table.py` `ROWS` (generated here with `python3 script/realapp/appci/upgrade_table.py --markdown`); selftest
`upgrade_table_matches_fixture_and_docs` fails when a row, the fixture, this table or the tree's `toml_path`s drift apart. Every key between the tag
`v0.1.17` and merge/w0 that was added, removed, or whose default or meaning changed is here; unchanged owner-shape keys (U01-U11) are there too, as the "no
settings reset" half. "upgrade must" is `keep` (survives byte for byte), `drop silently` (the app stops reading it, the line stays: unknown keys are preserved,
#1545), or `new key` (the upgrade must not invent a value). No row needs `migrate` or `drop with a notice`: the only real migration code between the tags is the
Claude/Codex/Gemini/OpenCode plugin source move, which is a user action offered by the plugin manager, not a launch-time rewrite (U83).

| id | area | key | change | v0.1.17 | now | upgrade must | check |
|---|---|---|---|---|---|---|---|
| U01 | general | `general.default_session_mode` | unchanged | "agent" | same | **keep** | kept |
| U02 | agent input | `agents.warp_agent.input.ai_auto_detection_enabled / nld_in_terminal_enabled` | unchanged | true / true | same | **keep** | kept |
| U03 | agent providers | `agents.warp_agent.providers` | unchanged shape; now also feeds the network gate | two inline-table providers (local model, local Claude proxy) | same list; each provider's base_url is a configured origin of the gate | **keep** | kept |
| U04 | agent providers | `agents.byop.last_used_model_id` | unchanged | "byop:upg-ollama:probe-model" | same | **keep** | kept |
| U05 | appearance | `appearance.vertical_tabs.enabled` | unchanged | true | same | **keep** | kept |
| U06 | appearance | `appearance.themes.theme / appearance.text.font_size` | unchanged | "tokyo_night" / 14.0 | same | **keep** | kept |
| U07 | shell | `session.startup_shell_override` | unchanged | "<bash>" | same | **keep** | kept |
| U08 | privacy | `privacy.custom_secret_regex_list` | unchanged | two patterns (one named) | same | **keep** | kept |
| U09 | privacy | `privacy.secret_redaction.enabled` | unchanged | false | same | **keep** | kept |
| U10 | privacy | `privacy.telemetry_enabled` | unchanged | false | same | **keep** | kept |
| U11 | general | `general.restore_session` | unchanged | false | same | **keep** | kept |
| U20 | network | `network.proxy_mode / proxy_url / proxy_username / proxy_no_proxy` | meaning changed for system and off | "custom" + http://127.0.0.1:3128 + user + no_proxy list (the default "system" followed OS/env proxies) | custom is used as before; system and off now both mean NO proxy (the gate ignores system and environment proxies) | **keep** | kept |
| U21 | network | `network.allowed_origins` | new key | (absent) | default "" (loopback + configured AI endpoints always allowed) | **new key** | absent_or_default |
| U21a | network | `network.gate_enabled` | new key | (absent) | default true (the gate stays ON, as in v0.1.17) | **new key** | absent_or_default |
| U22 | keyring | `ProxyPassword (keyring item)` | unchanged key; read moved off the UI thread (#1805) | stored in the OS keyring | same item, read on the secure-storage worker | **keep** | secret_same |
| U23 | keyring | `AgentProviderSecrets (keyring item: per-provider API keys, JSON)` | unchanged | stored in the OS keyring | same item | **keep** | secret_same |
| U30 | agent providers | `agents.warp_agent.models_dev_fetch` | default changed true -> false | true (explicit in the seed) | explicit true is honoured (auto-download when the Providers page opens); UNSET is now false (built-in catalog, Refresh button only) | **keep** | kept |
| U31 | agent providers | `agents.warp_agent.models_dev_url` | meaning changed (a configured origin of the gate) | a mirror URL (loopback) | same URL, added to the gate's configured origins | **keep** | kept |
| U40 | ssh | `warpify.ssh.use_ssh_tmux_wrapper` | REMOVED (the tmux option is deleted, DECLINED.md) | true (explicit) | no setting by that name; the key is unknown | **drop silently** | unknown_kept |
| U41 | ssh | `warpify.ssh.ssh_extension_install_mode` | meaning changed (the app-wide default for a host with no own decision) | "always_install" (what "don't ask again" + Install wrote in v0.1.17) | same value; a host in neither list still follows it, so the old global choice keeps applying to every host | **keep** | kept |
| U42 | ssh | `warpify.ssh.remote_helper_allowed_hosts` | new key | (absent) | default [] | **new key** | absent_or_default |
| U43 | ssh | `warpify.ssh.remote_helper_denied_hosts` | new key | (absent) | default [] | **new key** | absent_or_default |
| U44 | ssh | `terminal.remote.show_no_helper_chip / no_helper_chip_dismissed_hosts` | new key | (absent) | default true / [] | **new key** | absent_or_default |
| U50 | input | `terminal.input.completions_open_while_typing` | default changed false -> true (local sessions only unless set) | false (explicit in the seed) | explicit false wins everywhere; UNSET is now on for local sessions, off for remote | **keep** | kept |
| U51 | profile | `appearance.profile.display_name / appearance.profile.avatar` | new key | (absent) | default "" / "" | **new key** | absent_or_default |
| U60 | agent permissions | `agents.warp_agent.permissions.safe_command_allowlist_enabled / safe_command_allowlist` | new key | (absent) | default false / the built-in read-only list (no git) | **new key** | absent_or_default |
| U70 | env switch | `PHOSPHOR_BOOTSTRAP_TIMED` | REMOVED (the events-not-timers rework, #1354/#1815) | =1 selected the old fixed-delay ssh bootstrap | ignored | **drop silently** | env_inert |
| U71 | env switch | `PHOSPHOR_SSH_BOOTSTRAP_GIVEUP_DELIVERS` | REMOVED (#1815) | =1 delivered typed text after a give-up | ignored (a give-up drops the keys with a count toast, #1661) | **drop silently** | env_inert |
| U72 | env switch | `PHOSPHOR_SUBSHELL_READY` | never in a release (added and removed between the tags) | (not in v0.1.17) | ignored | **drop silently** | env_inert |
| U80 | state | `warp.sqlite (and every other SQLite file in the data/state dirs)` | schema migrations run at open | the v0.1.17 schema | same file, migrations applied in place | **keep** | db_migrations |
| U81 | state | `keybindings.yaml` | unchanged | one custom binding | same file | **keep** | file_same |
| U82 | state | `themes/<user theme>.yaml` | unchanged | one user theme | same file | **keep** | file_same |
| U83 | state | `~/.claude/plugins/known_marketplaces.json + installed_plugins.json (Claude Code plugin registered from Warp's GitHub)` | bundled plugin replaces remote registrations (Wave N) | marketplace registered from a remote repository | NOT touched at launch: the app offers a 'move to the bundled copy' action (needs_source_migration) and never fetches the plugin | **keep** | file_same |

**Verified only on the box (not by the case, not by anything runnable on the laptop).** (a) The case itself: nothing here has run (no app, no display on the laptop;
`selftest.py` runs the pure parts offline). (b) The settings UI is not read (no stable pointer path, the same gap `ui_settings_live` records); the rows are judged on
the settings FILE the UI writes, and on behaviour only where the table says so. (c) U22/U23 need `secret-tool` on the runner. (d) Behaviour behind a kept value
(the gate really admitting the seeded loopback providers, an unset `completions_open_while_typing` completing in a local pane, `models_dev_fetch` really off when
unset, the plugin manager really offering the move for U83) is covered by `zero_network_startup`, `claude_plugin_offline` and the Rust unit tests, not by this case.
(e) The removed env switches are proven inert only as far as "the app starts and logs no failure". (f) The Rust tests below were written without a compiler.

**Unit tests added with this case** (Rust, beside the code; not run on the laptop): `models_dev_fetch_defaults_off_and_a_v0_1_17_provider_list_still_parses`
(`app/src/settings/ai_tests.rs`), `allowed_origins_defaults_empty_and_v0_1_17_proxy_modes_still_map` (`app/src/settings/network_tests.rs`),
`a_v0_1_17_profile_keeps_its_app_wide_choice_until_a_host_decides` (`crates/remote_server/src/policy_tests.rs`), and for the shared plugin-source helpers that had none:
`a_recorded_folder_is_the_bundle_only_when_it_is_that_folder`, `a_path_looks_like_a_bundle_at_this_or_an_older_install_location`,
`the_bundled_plugin_folder_needs_its_marker_file`, `a_missing_bundle_says_phosphor_does_not_download_it`
(`app/src/terminal/cli_agent_sessions/plugin_manager/mod_tests.rs`). Python: selftest `upgrade_table_matches_fixture_and_docs`, `upgrade_case_pure_helpers`.

**Findings from writing it.** #1816: the network-mode description string and the manual still describe the removed behaviour (a user upgrading with the default
`system` proxy mode is told it follows the OS proxy; it does not). Owner decision recorded, not a defect: a v0.1.17 `ssh_extension_install_mode = "always_install"`
(what "don't ask again" + Install wrote) keeps applying to EVERY host that has no own decision; no per-host list is filled from it because a v0.1.17 profile holds no
host names.

### 2.19 Agent resilience: model failures and restarts (`appci/cases_agent_resilience.py`, `appci/fake_model.py`; #1831, #1832)

Twelve agent-mode cases against the scripted fake model (`APPCI_FAKE_MODEL=1`; without it they SKIP with the reason, a real provider is never used). Not run yet:
they first run in round 6, so expect harness faults before app findings. Run: `python3 script/realapp/appci/run.py --cases 'agent_fault_*,agent_restart_*'`.

**Fake model fault injection** (`fake_model.py`): `fake.inject(kind, times, match, **kw)` makes the next `times` matching `/api/chat` requests (`match` = substring of
the last user text) take a fault: `status` (code, optional `Retry-After`), `disconnect` (cut mid-body after N lines), `delay` (nothing for N s, hang-up counted in
`fake.aborted`), `drip` (a long reply, one line per interval). `fake.go_down()` / `come_up()` close and rebind the listening socket (connection refused).
Oracles: `fake.requests`, `fake.fresh` (each prompt the app really sent), `fake.fired`, `fake.aborted`.

**#1831 model failures** (all platforms; the fake runs on the host): `agent_fault_429` (Retry-After 1 on every try), `agent_fault_500`, `agent_fault_disconnect`
(no verbatim retry once output was seen; the app may RESUME once, so `success` after `recovery=resume` and 2 requests also pass; `hop=turn` logs changes only and a new conversation starts InProgress, so `agent_fault_slow_cancel` reads no line as in progress; 429/5xx `ErrorStatus` and the exhausted-retry wrapper log the final note at WARN, a 4xx stays ERROR), `agent_fault_down` (connection refused), `agent_fault_slow_cancel` (30 s first-token delay, Ctrl-C within 5 s, the connection is
hung up), `agent_fault_queued_rows` (two `/queue` rows behind a failing reply). Oracles: `hop=turn` ends in `error` / `cancelled` / `success`, never `in_progress`; no
`hop=agent_action` left open; the app log has the failure line (`[byop] open stream failed | stream chunk error`) and ERROR lines stay inside the fuzzer's allowance
(`oracle.rs error_allowance`: 2 per failing chain, only the `[byop]` failure kinds); no request on its own after the turn (no endless retry); the re-sent prompt
reaches the model exactly once and succeeds; OCR of the block is checked for error wording when an engine exists. Queued rows: each token is sent once XOR still held
(queued_prompts / input), never lost, never twice, and the `fired` hops equal the prompts the model got (`judge_rows`).

**#1832 restarts** (`agent_restart_{quit,kill}_{stream,queued,blocked}`; kill: Linux, macOS, Windows; quit: Linux only, the other harnesses have no quit key and the
case SKIPs): `restore_session` on, one completed turn first, then a streaming reply / two `/queue` rows behind it / an approval card blocked on a command with a queued
follow-up, then a clean quit or SIGKILL, then a relaunch on the same profile (`cases_robust` helpers). Oracles: the completed turn is in `warp.sqlite`
(`agent_conversations` / `ai_queries`, read read-only with sqlite3); each queued row is in `queued_prompts` (or on the screen if the app popped it back to the input) and
reached the model zero times (restored rows are suspended, #1027); the fake got no request on its own; no `hop=agent_action status=running` and no
`hop=turn state=in_progress` in the relaunch log; the blocked command left no effect file, no rec.log line and no pty write; a new prompt then works.

**Overlap with the in-process fuzzer** (`app/src/ai/blocklist/agent_fuzz`): the fuzzer replaces the provider seam (events, not HTTP) and checks state invariants under
`status` / `disconnect` / `stall` / `delay` / `never_end` chaos; it never touches the wire, restarts the app or reads the sqlite store. These cases cover the real HTTP
client (Retry-After, refused connection, a cut socket), the real error block and cancel key, and persistence across quit / kill -9. Self-test rows:
`fake_model_fault_injection`, `agent_resilience_pure_parts`.

### 2.20 The question card never hangs (`appci/cases_agent_question.py`, `appci/fake_model.py`, fuzzer corpus `hang1851_*`; #1851)

An agent turn hung on "Preparing question..." (the ask_user_question card, never answerable, the turn never ended). Eight agent-mode PROVING cases against the
scripted fake model (`APPCI_FAKE_MODEL=1`), never run yet: `agent_question_answer` (whole call: card blocked, Enter answers, the model gets the answer, the turn
succeeds), `agent_question_cut` / `_nodone` / `_trunc` / `_empty` / `_slow` (the call cut right after it, held open with no `done` then closed, arguments cut
mid-JSON, `{}` arguments, 6 s of silence before the call), `agent_question_ctrl_c` (Ctrl-C while the call sits unfinished: `cancelled` within 5 s and the client hangs
up), `agent_question_steer` (#851: a new prompt typed meanwhile reaches the model once), `agent_question_after_command` (#1858: one reply `[sleep 8, ask]`; OCR must not
see "Preparing question..." while the command runs, then the card answers) and `agent_question_steer_at_end` (#1861: a steer typed as the reply ends discards the ask; the steer
prompt is answered, nothing stays open). Fake model trigger "Please run a slow command then ask me a question <TAG>". Run: `python3 script/realapp/appci/run.py --cases 'agent_question_*'`.
Pass rule for every fault variant: an answerable card (`hop=agent_action kind=ask_user_question status=blocked`, then answered) or a terminal turn (`error` /
`cancelled` / `success`) with no ask action left open, within 150 s; then a plain prompt reaches the model exactly once and succeeds, and the ERROR lines stay inside
the `[byop]` failure allowance. A turn still `in_progress`, or terminal with an ask action open, fails as the hang. Fake model additions: the user turn
"Please ask me a question <TAG>" -> one ask_user_question call (options alpha / beta); faults `ask_cut`, `ask_nodone` (secs), `ask_trunc`, `ask_empty`, `ask_slow`
(secs). Fuzzer corpus (`app/src/ai/blocklist/agent_fuzz/corpus/hang1851_*.json`, replayed by `corpus_replay_tests`): truncated and empty arguments, a stream with no End
or cut after the call, a cut then a resume that asks again, a stall after the call (needs the short idle timeouts), a missing / empty / repeated call id, a question
next to a truncated one, a steer on the blocked card, byte-split arguments. Unit: `only_content_bearing_events_restart_the_idle_window` (chat_stream.rs): empty
keep-alive chunks no longer restart the stream idle window, so a provider that holds the response open after its last call is ended by the idle give-up.

Candidate fixes (#1858 loading-text precedence, #1859 preprocess abandonment, #1860 AfterStreamFinished fall-through, #1861 follow-up discard results), none built or run yet:
unit tests `preparing_question_tests` (common.rs), `abandon_pending_unparks_later_batches` / `abandoned_batch_late_result_is_dropped` / `abandon_all_returns_parked_actions`
(preprocess_tests.rs). Fuzzer oracle I1 gained `PendingStage::Parked` (a finished preprocess batch with no Pending batch ahead that never became pending/blocked) and corpus
`hang1851c1_multi_tool_slow_command_trailing_ask`, `hang1851c2_hung_command_then_ask_stop` (the hung-diff stand-in: the fake DSL has no apply_file_diffs, a never-ending command
plays the hung earlier action), `hang1851c4_steer_during_ask_placeholder`.
Empty call id (#1851): the End event's captured copy of an empty-id call is re-keyed to the id the stream invented for its chunk (`assign_empty_call_ids`, unit `empty_ids_take_the_ids_invented_for_the_chunks_in_order`); corpus `hang1851_ask_empty_id` now sees one call and one result.
Owner decision 2026-11-09 (#1859): earlier-turn actions that finished preprocessing but were parked behind an abandoned batch are CANCELLED at the next turn's `queue_actions`
(a `cancelled` result each, no execution, no FinishedAction event); unit `stale_parked_command_is_released_for_cancel_not_queued_with_new_turn`. The Parked oracle and the
corpus did not assume release, so they are unchanged.

Pre-build review round 8 fixes (#1893-#1898; unit tests only, none built or run yet):
`synced_ness_at_delivery_is_the_entrys_not_the_enclosing_arrivals` (view_test.rs; the one predicate `input_is_synced` both the legacy writer and the table executor read, so it
covers Legacy and Table: a drain inside a synced arrival keeps the user's own keys taking over, a held synced entry stays passive; #1893/#1862),
`late_result_of_an_abandoned_or_missing_batch_hands_back_its_actions_to_cancel` / `live_batch_result_has_no_stale_actions` (preprocess_tests.rs; #1894),
`release_without_event_hands_over_before_the_next_paste` (group1_handover_tests.rs, Table mode: the paste lands behind the held keys; #1895),
`discarded_actions_of_a_follow_up_send_no_request_of_their_own` (controller_tests.rs: a batch of only the cancelled results of discarded actions plans no follow-up request, so a
steer sends exactly one; #1896), `exit_shell_removes_the_sessions_typeahead_mode` (terminal_model_test.rs; #1897) and `an_empty_first_chunk_keeps_the_first_event_window`
(chat_stream.rs; #1898).

## 3. Real-app testing

Automated tests do not catch lag, lost keys or broken flows; the owner feels those. Every release candidate
and every input/latency/ssh/bootstrap change gets a real-app pass on **Linux, macOS and Windows**.
Scripts are in `script/realapp/` (index: `script/realapp/README.md`); the latency tools are in
`script/keylat/`. Real hosts, users and paths are NOT in the repo: they are in the owner's local config
(`script/realapp/config.example.sh` shows the variables). Prefer real-app tests on a build over
code-refuter agents.

### 3.0 View-mode signal (#1568)

The app logs `input_order hop=view_mode mode=agent|terminal pane=<terminal view id> t=<ms>` (debug level, the `input_order`
target) when a pane's agent-view controller is created (once per pane) and on every agent-view enter/exit
(`app/src/ai/blocklist/agent_view/controller.rs`). It carries ids and a fixed word only, never user text. The appci harness
(`Ctx.log_view_mode`, `Ctx.mode_now`, `mode_proof`) reads the latest such line from the app log (rotated
`phosphor.log.in_session.N` chunks included), so the agent-vs-terminal proof and the per-case "still in agent view" check (#1563)
work on Linux, macOS and Windows without OCR. `mode_proof` FAILS in the agent pass if the log says `terminal`. OCR is only the
fallback for builds that predate the line.

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
  no process of the bundle exists, otherwise "PID lookup failed". The harness cannot `wait()` an `open`-launched app, so on an
  APP DIED it reads the exit status from the unified log (#1782, `run.mac_exit_evidence`, read only, bounded): launchd's
  `exited due to SIG... | sent by ...` line for that PID (a fault says `sent by exc handler[<pid>]`; a kill names the sender)
  and the app's last system-log lines; a signal there counts as crash evidence. The app itself writes one
  `[ERROR] fatal signal SIG... code= addr= thread='...'` line to its log on SIGBUS/SIGSEGV/SIGILL/SIGFPE/SIGABRT
  (`warp_logging::fatal_signal`, all Unix; kill switch `PHOSPHOR_FATAL_SIGNAL_LOG=0`), which the death classifier quotes
  first. One instance per pass is enforced. The window rect comes
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
- Real-app CI harness: ONE Python harness shared with Linux and macOS (`script/realapp/appci/`, 2.8) run by the private Python `%LOCALAPPDATA%\phosphor-ci\python312\python.exe`
  (3.12.10, paramiko, Pillow; not on PATH). The hand-run scripts below are the older exploratory tooling.
- Harness `script/realapp/windows/`: `lib/lib.ps1` (foreground-guarded `Keys`/`Cmd` - `IsPho` refuses to type when
  Phosphor is not foreground - `Click`, `Drag`, `Wheel`, `Shot`, `Launch`, `QuitApp`, `KC` combos), `lib2.ps1`,
  `mouse.ps1`, `lat.ps1` (BitBlt region hash, +-16 ms capture resolution), `inst.ps1`/`launch.ps1`, backup/restore set;
  `cases-r18ba/` and `cases-r18bj/` (per-wave cases: typing, paste, ssh, approval cards, scene analysis
  `an_scene.py`); `hooks/` (PowerShell bootstrap hook profiling `prof*.ps1`/`hk.ps1`, ConPTY driver `cpty.ps1`; measures per-prompt hook cost,
  Warp-Precmd was 30-120 ms, #1163); `agentkey/` (agent-mode key delivery variants; `B2_ab_lat2.ps1` is the owner-approved #1225/#1279 key->paint A/B, `PHOSPHOR_FRAME_LATENCY=2` against the DX12 default 1, interleaved def/lat2 x2 relaunches, PowerShell and agent tab each, read with `dist.py`); `pwshmarker/` (line-end / ready
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
  proof** types a marker command (absolute path) and checks it arrived, trying `SendInput` virtual keys, then `SendInput` unicode packets (`SendKeys` is gone: the Python harness has no WinForms); if none lands the mode stops with `keys not delivered: <reason>` (job/explorer/app session ids,
  foreground window) and runs no further cases. Latency records follow `lat.py` like every platform: fewer than 30 samples is flagged
  "indicative only", and no reference console gives `no-reference`, never a computed overhead.

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
   history, Up/Ctrl-R; leftover processes after disconnect/quit (box and client); without the helper, Tab completes a
   remote directory in-band (`remote_tab_no_helper`, #1801; repeat at netem 150 ms, see "#1801" below).
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
12. **Fake ssh lab** (box and, via the fixed-port slots, Mac and Windows; separate manual jobs `app-test-lab-*`, 2.15): Tab on a remote directory, path detection with zero
    exec channels, git chips, Enter -> start paired with plain ssh at 150/300 ms, ssh drop/reset/stall, Ctrl-C bursts - per shell, with and without the remote helper.

#### 3.5.1 International input cases (`appci/cases_caseintl.py`, #1466; the Windows ones moved from `cases_caseintl.ps1` into the shared harness in #1488)

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
| `intl_wide_type`, `intl_wide_paste`, `intl_wide_edit` | Windows (both panes) | the strings typed (KEYEVENTF_UNICODE) and pasted; the PowerShell pane writes them with `[IO.File]::AppendAllText(..., UTF8Encoding($false))`, the Git Bash pane with `echo`; read back byte-exact (was `intl_wide`) |
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
| `warp::input_order=debug`, Windows only (#1490) | `hop=pty_write_bytes` (the exact bytes each ConPTY write accepted, with `since_{precmd,preexec,ready,request,report}_ms` of the shell-ready signals the hand-over acts on) and `hop=hand_route` (why a user write was held or let through, with the hand-off state). Bytes show only with `WARP_INPUT_ORDER_LOG_TEXT=1`. Join with the shell's own `rec.log` to place a lost or moved character |
| `WARP_INPUT_ORDER_LOG_TEXT=1` | adds the typed text to those lines. Keystrokes can be passwords: use it ONLY with synthetic text, never with real input; default is length only |
| `warpui::frame_cause=debug` (and `warpui::notify_cause`) | names what asked for each frame (find the view/handler that dominates frame time, e.g. #1137) |
| `warpui::scene_stats=debug` | per-frame scene size (primitives, glyphs) |
| `warpui::gpu_done=debug` with `PHOSPHOR_GPU_TIMING=1` | GPU completion time per frame (off by default; the wait never runs on the UI thread) |
| `warp::terminal::model_lock_trace=debug` | holds of the terminal model lock >=2 ms (debug) and >8 ms (warn, rate limited 1/s) by named scope; the pty reader's `try_lock` fails while the UI holds it. `PHOSPHOR_RENDER_LOCK_LEGACY=1` compares |
| `phosphor::routing=debug` | agent/terminal routing decision per line (`PHOSPHOR_ROUTING_V1=1` for the old rules) |
| `bootstrap` timing | bootstrap InitShell to Bootstrapped timing |
| `PHOSPHOR_IMG_TIMING`, `WARP_STARTUP_TRACE` | image render and startup traces |

Slow-operation logging follows the budgets (summary >=8 ms, WARN >=16 ms). Samply/Tracy CPU profiles: `docs/profiling.md`.

## 5. Release test checklist

Run on a numbered release candidate, once per platform with a new agent per platform, against the previous release for
comparison.

1. Gates green on the candidate commit: `script/precheck` (box), `guards` + `check-linux` CI, `test-linux`,
   `test-warpui-core`, `test-workspace-rest` (play them; automatic only on tags), `integration-linux` (gating since 2026-11-09),
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

## 7. Known gaps

What is automated today (details in 2.8 and 2.9): the real-app layer runs in CI through ONE Python harness
(`script/realapp/appci/run.py`) on Linux (`app-test-linux-wayland`, `app-test-linux-x11`), macOS (`app-test-macos`) and Windows
(`app-test-windows`), each in both session modes, plus a `-reuse` twin of each (`app-test-*-reuse`) that tests an earlier
app-identical build. They are manual jobs played by `script/ci/ci_seq.py` in the fix (smoke), candidate and release (full) tiers.
Latency is judged on overhead over a reference terminal in the same session (foot/xterm, Terminal.app, conhost/Windows Terminal), and
`key_loss_control` is kept as a permanent control. On the Mac `ktap` + `enter_route.py` record where every Return went (#1482); the
Mac injection and latency tools (`htype2`, `kl3`, `ktap`, ...) are compiled from `script/realapp/mac/tools/*.swift` at job start.

Real gaps that remain (each checked against `.gitlab-ci.yml` and `script/realapp/`):

- Every `app-test-*` and heavy job is `when: manual`; nothing runs the real-app layer on a plain push or on a schedule, so a
  regression is found only when someone plays a tier. The Linux `kseq`/`rec` torture tools (`script/realapp/linux/torture/`) are
  built by hand and by no CI job.
- `latency_paint` (key to paint) needs read-back and is skipped on Linux and Windows (Mac only, via `kl3`). The latency budgets
  are judged on overhead over the reference; when no reference can be measured the verdict is `NO-REFERENCE`, not pass.
- `integration-linux` is manual and `allow_failure: true` in CI, but a hard gate in `precheck` and, since 2026-11-09, in the candidate and release tier verdicts; macOS and Windows have no
  nextest job (`check-macos`/`check-windows` are `cargo check`; the Mac also runs `usage-test-macos`).
- Wayland input is tested with a headless sway only (X11 and XWayland as reduced passes); GPU-backed rendering is untested on CI
  (llvmpipe/pixman only, or the box's render node).
- The release pipeline has never run end to end; macOS Intel and Linux aarch64 are partial or dropped.
- `check_workspace_clean` is deliberately not wired anywhere (see the comment in `script/precheck`).
- Windows and Mac shells beyond those the harness drives (bash, zsh, fish, nushell, pwsh 7, Windows PowerShell 5.1) are covered
  only by the bootstrap equivalence scripts, which skip when the shell is absent.
- Authenticated ssh from the Windows VM (the harness uses `fakesshd_win.py`), and agent-mode tests against a cloud provider, are
  not covered.
- Durations of the Linux build, app tests (smoke and full), shuttle/loom, fuzz, bench and mutants are not timed yet (2.9).

## 8. Test results history

Owner rule (2026-11-02): every test result is kept in the repo for regression review. `test-results/` is that archive; it is
append-only history, **retained forever** (it is small by design, see the size rules).

**Launch failures name their cause.** When the pane's shell never shows a prompt, the `launch` row's detail quotes the app log's own error lines (`Bootstrapping failed`, panics, `ERROR`, keyring), redacted, newest last, plus a platform hint (macOS has no `app.stdout`; read `logs/<mode>/phosphor.log`). r18d8's macOS run failed with only "see logs/app.stdout; GPU/Wayland init?" while the real cause, a zsh bootstrap line over macOS's 1024-byte canonical limit, was in phosphor.log.

**Mac and Windows builds start at once.** `ci_seq.py` plays `build-macos-arm64` and `build-windows-x86_64` (both `needs: []`, own machines) before it waits for `check-linux`; only the Linux build and the box suites wait for the box. In r18e4 the Mac and the Windows VM sat idle for over an hour behind a `check-linux` queued on the busy box.

**GitLab API rate limits.** `ci_seq.py` and `collect_results.py` call GitLab only through `script/ci/glapi.py` (`glab api -i`). This account is on GitLab.com's per-user Free-plan burst limit (observed 2026-11-03: `Ratelimit-Name: burst_traffic_per_user_free_plan`, `Ratelimit-Limit: 100` per minute). glapi waits exactly as long as a 429's `Retry-After` says (else until `Ratelimit-Reset`), backs off exponentially when throttled again, pauses before the window is spent when `Ratelimit-Remaining` drops under 10%, and never reports a throttled call as "no such job" or "no results". Do not add other pollers alongside a running `ci_seq`; a hand-written wait loop polls no faster than every 2 minutes. `python3 script/ci/glapi.py --self-test` checks the header parsing.

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
and the CI `guards` job) fails above 10 MB per run and on anything that looks like an endpoint or secret. It also fails (#1496) when the
NEWEST run (dated 2026-11-03 or later) has a unit failure that the run's `triage.md` does not own: write one line per failing test,
naming the test and its issue (`terminal::view::tests::foo #1234`), when you commit the run. The collector scrubs
(`script/ci/results_scrub.py`) the values of `APPCI_OLLAMA_URL`, `PHOSPHOR_BOX_HOST/SSH`, `PHOSPHOR_MAC_SSH`, `PHOSPHOR_WIN_SSH`, `WINUSER`
(from the environment, `~/.config/phosphor-ci/ci.env` and `~/.config/phosphor-realapp.sh`), every non-loopback IPv4 address, home
directory user names, and token/key patterns, so collect on a machine that has that config. Archive files are text except the JPEGs
(`.gitattributes` marks `test-results/**/*.png` and `*.jpg` binary).

**Windows triage rules, r18co additions (#1488).** (6) The wedge probe judges by EFFECT, never by a prompt (the PowerShell pane has no prompt hook): a marker command appending to `alive_<tok>.txt` (looked up in the case dir and, in the PowerShell pane, the real home); only no effect after two tries (the second after Ctrl-C x3) is `pane_wedged_after_<case>`; an effect only after the retry is the passing row `pane_needed_ctrl_c_after_<case>` (Git Bash pane after `cadence`: the app's log shows typed keys written to the pty and the Enter taken by the empty input editor, no `kh_focus_input` after the long-running `ts_reader` exited; candidate app finding). The probe waits for its own line in rec.log so it cannot land in the next case's `cmds_since`. It also runs after `cadence`. (7) After the key proof the PowerShell pane is `Set-Location`'d into the case dir and verified by a relative marker file, retried 3 times (r18co: Ctrl-U typed while the proof's command was still returning became `^USet-Location`, the pane stayed in the real home and every PowerShell effect file landed outside the case dir). (8) `session.new_session_shell_override` is an enum: its TOML is `{ executable = "<path>" }`, not a bare string (r18co: 'Invalid value for new_session_shell_override').

**Windows fix tier (`APPCI_TIER=smoke`), as data (`winmap.FIX_TIER` / `tier_plan`).** r18co took 70 min. Terminal mode runs the shared SMOKE list (the reference measurements: conhost enter control, Ctrl-C reference + remeasure, `latency_enter`, `key_loss_control` in the Git Bash pane only). Agent mode runs only `mode_proof`, `fast_typing` (gap 0 only, no enter control), `paste_instant_enter`, `tab_enter`, a short `ctrl_c_d` (delivery and line integrity; no reference, no remeasure, the overhead budget is judged in terminal mode) and the three ssh hand-over cases, once per pane. Cases per pass that actually run (`run.py --self-test` prints them): terminal/powershell 13, terminal/gitbash 17, agent/powershell 8, agent/gitbash 8. Expected duration, estimated from r18co per-case times (not yet measured): about 10 + 11 + 4.5 + 4.5 = about 30 min including launches and probes. The full tier (candidate, release) and an explicit `--cases` list are unchanged.

## r18e4 harness accuracy fixes (macOS live run)

- **Floods** (`flood_typed_line`, `flood_typing_latency`, `flood_scrollback_find`): fixed line counts finished before the typing on macOS. The producer is now an
  endless loop (`cases_robust.endless_flood`) that runs until the case touches a stop file (after the typing) and at least a floor of lines/seconds; the "flood
  ended before the typing finished" self-checks are kept.
- **ZWJ "typed as literal text"** (`intl_wide_type` / `intl_wide_paste`) was a recorder bug, not the app: the zsh rc read the line with `fc -ln -1`, which prints a
  non-printing character as the text `\u200d` (bytes 5c 75 32 30 30 64), while the effect file had the real U+200D. The rc now reads the raw `$history` entry.
- **`ssh_hanging_host_not_local` refused arm**: a refused port exits in milliseconds, before the first key, so the ssh cannot be kept alive; the arm is UNVERIFIABLE
  (NOREF) in that case and no longer fails the "usable afterwards" check because the line correctly ran locally. The hanging-host arm is the real hold test.
- **`routing_table`**: `what is 5 > 3` carries shell syntax (`>`; `what` is an installed head on macOS), so it runs in the shell (#1183 / #1560). It moved from the
  prose list to the ambiguous list (judged by the logged rule); a syntax-free prose line (`is five bigger than three`) took its place.

## r18u8 harness fixes (macOS and Windows live runs, 443965c77)

Every item below is a harness change; none touches app code. Anything not listed was judged an APP finding in the two triage files.

- **ssh pass-through expectations** (owner 2026-11-04, DECLINED.md): `ssh_hanging_host_not_local` now asserts what `ssh_failing_not_local` asserts: the line typed behind a
  connecting ssh runs LOCALLY exactly once, exactly as typed, and only after the ssh exited (file mtime >= the exit the poller saw); the local shell receives it once;
  the next probe line then runs once. The refused arm (ssh already gone before the first key) is an ordinary local command and must also run exactly once; it is no
  longer UNVERIFIABLE. `ps_has_port` reads Windows command lines through CIM (there is no POSIX `ps` in the Windows harness PATH).
- **`ssh_password_prompt`** types NOTHING before ssh's prompt is on screen (OCR polled for the prompt word up to 20 s; with no OCR a fixed 3 s wait, said in a note), then
  the secret; the wrong-secret arm waits for the ssh client to exit (a check, 20 s) before any local command is typed. The "junk typed before the prompt is discarded"
  check is gone: under the plain-terminal pass-through that junk is ssh's (ssh.exe takes it as the one password attempt).
- **Pillow**: `cb_agent_pickers`, `cb_settings_ui_open` and `cb_ui_action_latency` SKIP with "Pillow not installed on this runner (owner: install?)" instead of passing on a note
  (`cases_ui.need_pillow`).
- **Agent cases**: `agent_stream_cancel` no longer assumes the first Ctrl-C clears typed text (while a reply streams it cancels the stream; the second clears the input): it
  presses at most twice, measures both facts after each press and requires the end state (stream stopped, typed text gone). `agent_stream_cancel` and `agent_approval` start a
  FRESH agent conversation (`cases_caseagent.fresh_conversation`: Escape, `/agent`, proven by `hop=view_mode`; a skip if it cannot be proven): the model had seen earlier
  cases' prompts and proposed the stale `echo APPROVAL_TOKEN >> ap.txt`. `agent_code_recycle` owns its own Escape: it re-enters the agent view with `/agent` instead of
  being reported as `left_agent_mode_after_agent_code_recycle`. Frame stability (`frames_settled`) compares screenshots by changed-pixel fraction when Pillow is there
  (a blinking caret is ~0.00002 of a screen, a streaming reply far more; threshold 0.0008) and by the old "<= 2 distinct hashes" otherwise (never looser).
- **Ctrl-C spacing in agent mode** (H16): `pane_responsive` and `run.set_ps_pane_state` space their Ctrl-C presses `EXIT_WINDOW_S + 0.4` s apart in agent mode (0.8 s apart
  exited an empty agent conversation). r18u22: `pane_responsive`'s FIRST Ctrl-C also waits out the window after the harness's previous Ctrl-C
  (`Ctx.wait_out_exit_window`); the probe after `cadence` / `ctrl_c_d` pressed it 0.8 s after that case's own `reset()` and exited the agent view, which the runner
  then reported as `left_agent_mode_after_<next case>`.
- **`paste_instant_enter` rec.log** (r18u22): the Git Bash pane's `rec.log` is written by `PROMPT_COMMAND` after each command ran, so the case waits (5 s) for
  every pasted line to be logged before it compares the shell's record; it compared as soon as `pe.txt` had six digests and saw five.
- **`cb_settings_ui_open`**: the settings tab is closed with the platform's close (chord, else the command palette's `Close Current Session`) when a probe line shows the
  shell is not in front (terminal mode), instead of only on a screenshot diff and only with a chord that is empty on macOS. **`cb_no_autoupdate`**: the startup line's
  `autoupdate_configured: false` is blanked before matching (`: true` still matches). `cb_agent_file_mention`: the Mac failure is an APP finding (the `@` picker is open in
  the screenshot, Enter submits); not changed.
- **`subshells`**: nu appends with `| save --append` (`>>` is a nushell parse error). **`remote_session_*`**: the clean-env subshell quotes `PATH="$PATH"` (a Mac PATH with
  spaces split the words); on Windows that step is NOT RUN (noref): `env -i` under msys ends the fake sshd session (H6).
- **`flood_scrollback_find`**: Find is Cmd-Shift-F on macOS (`bindings.rs CustomAction::Find`); the marker is typed split by an empty string (`ABC""DEF`) because Phosphor
  keeps the running command's text as the block header, which kept the control "marker not on screen" from ever holding. (Both superseded in r18u13 below: Cmd-F, reversed marker.)
- **`remote_heredoc_paste`**: a remote shell without bracketed paste makes the app ask `Run N lines one at a time?`; the case answers it with Run (OCR-detected, noted).
  Without OCR nothing is pressed and the note says a timeout may be that dialog.
- **`session_restore` / `crash_kill9_restore`**: the app's own bootstrap line (`command -p stty -echo; WARP_BOOTSTRAP_VAR=...`) that a restored zsh pane logs to rec.log is not a
  replayed user command (`Ctx.APP_BOOTSTRAP_RE`, excluded from `cmds_since` and the setup comparison). The restored-cwd mismatch (all tabs `~`) is NOT changed here. On the
  PowerShell pane (no prompt hook) the "a shell started" prompt-count check is replaced by a note (H10).
- **`ssh_drop_reconnect`**: since #1638 (owner, 2026-11-04) a managed session passes `~X` typed at the start of an empty line to ssh, so the `<Enter>~.` escape variant runs
  by default, next to the `kill -9 $$` variant. `APPCI_SSH_ESCAPE_THROUGH=0` opts out (a build from before #1638): the escape arm is then UNVERIFIABLE (noref).
  Unit tests: `input_test.rs` `test_ssh_escape_*` / `test_tilde_and_a_non_escape_key_*` (Legacy and Table; shell, autodetected-agent and locked-agent input).
- **Logs**: `run.save_old_rotations` also saves every `<log>.old.N` (plain or .gz, redacted, decompressed) next to `phosphor.log` / `phosphor.<n>.log`; the `.in_session.N` chunks were
  already folded in.
- **Injector accounting**: the macOS and Windows injector records carry `ref=True` for keys aimed at the reference terminal (as Linux did), so interleaved latency cases stop
  reporting "sent 16008, saw 6450 / Enter sent 3, saw 75".
- **Windows**: `leave_remote` retries a probe that went nowhere up to 3 times with a longer wait before refusing to type `exit` (ssh_nested_typeahead outer level, H3);
  `tabs_streaming` / `tabs_two_windows` name their effect file by absolute path (agent-mode new tabs start in `~`, H9); `intl_wide_edit` uses a bareword PowerShell command (no quote in
  the edited region, H11) and a per-edit lead word so the End key finds no autosuggestion to accept (H12); `ts_reader.py` writes its file by rename (the cadence case read it while it
  was still empty, H13); `alt_screen_programs` waits for vim/less to leave before typing (H14); `editor_edit_save` retries a sharing violation and closes the editor on a harness
  exception (H15).
- **Latency against ConPTY (U1)**: the Windows reference defaults to Windows Terminal when `wt.exe` is installed (`APPCI_WIN_REF=auto|wt|conhost`). A failed overhead budget against
  a bare conhost window is reported `NO-CONPTY-REFERENCE` (status noref, numbers kept), because the pane runs through ConPTY and conhost does not: the gap mixes the ConPTY layer with
  Phosphor's cost. Against Windows Terminal the budget verdict is unchanged.
- **`big_paste` under ConPTY (U2)**: when the Windows pane gets less than the whole paste, the same paste into `cat` in a conhost Git Bash window is the control. Same shortfall
  -> noref (platform, with the byte counts); conhost gets every byte -> the failure stands; no usable control (no reference window, or it received nothing) -> noref with the reason.
- **`big_paste` leaves the pane at a prompt (r18u22)**: under ConPTY Ctrl-D is not EOF for msys `cat`, so the case ends with `cat` still reading and reset()'s Ctrl-C
  reached it only after the paste backlog drained (3.7 s in r18u22, 68 ms in r18u19). `tab_enter`, next in order, typed its first line into the dying `cat`; the line
  split across the shell hand-back, ran as `T180_x >> tc/zzcompl_alpha_u`, and that stray file made every later Tab ambiguous (0/12). `big_paste` now waits for the
  shell's own prompt (`prompt.log`) after the Ctrl-C, presses Ctrl-C again (up to 3 more, agent-mode spacing) and FAILS by name if no prompt comes; panes without the
  rc hook do not run the case.
- **`big_paste` macOS paste-read budget (#1782, owner decision 2026-11-05)**: macOS reads the pasteboard on the main thread again (a worker's
  NSPasteboard read crashed the app with SIGBUS; DECLINED.md). The app logs `input_order hop=pasteboard_read kind= ms= bytes=`; `big_paste` judges
  the slowest one against `cases.PASTE_READ_UI_BUDGET_MS` (macOS 250 ms) and fails over it. Every other platform and every other UI budget stays
  at 4 ms (`paste_read_budget_ms`; Linux and Windows log no such read). A macOS run with no `pasteboard_read` line gets a note that the
  budget was NOT judged. Self-test `i1782_macos_paste_read_budget_is_raised_only_for_the_pasteboard_read`.
- **Not changed, by design**: H7 (fake sshd ConPTY heredoc stall) and H8 (Git Bash pass opens PowerShell tabs/restored tabs) need run evidence the artifacts do not contain;
  `profile.gitbash_toml` already writes `startup_shell_override` and `new_session_shell_override` for that pass.

## r18u13 harness fixes (macOS live run, 5e9dfbb71)

Harness changes, plus one app LOG line (no behaviour change). `run.py --self-test` covers each (`r18u13_mac_harness_fixes`, `keydown_audit_counts_plain_keys_and_flags_doubles`).

- **`flood_scrollback_find`**: the terminal Find on macOS is Cmd-F (`terminal:find` = `cmd_or_ctrl_shift("f")`, which is `cmd-f` there). The r18u8 entry above was wrong:
  Cmd-Shift-F is the workspace's global search (`OpenGlobalSearch`); the marker went into it, the panel kept the focus after Escape, and the next case's `ssh fake` + Enter
  were typed into that search field (`remote_heredoc_paste` "no remote shell prompt" in both modes, and its `pane_not_local_after_...` cascade). The marker is now typed
  REVERSED and printed through `rev`: OCR dropped the `""` of the split marker in the block header and read the joined marker there (the control failed).
- **zsh rec.log hook** (`profile.ZSHRC`): a line the history drops (`_warp_zshaddhistory` returns 1 for the agent-command wrapper; `hist_ignore_space`) lingers until the next
  line and that next line reuses its event number, so a `$HISTCMD`-only test never logged it (agent `routing_table`: `ls -la` right after an agent command, "LOST or
  unproven" although the app wrote it and zsh ran it). The rc's preexec hook now sets `__pa_ran`; a prompt logs the line when the number changed OR a command ran.
- **`crash_kill9_restore` / `crash_corrupt_db`**: the post-restore exactness line names its effect file by absolute path (`local_cmd_exact(..., absolute=True)`): a restored
  pane starts in its saved cwd, so the relative file landed in `ck_dir_N`.
- **`keydown_count`**: a same-key pair under 2 ms on a key the typed text has doubled (`>>`), with exactly as many presses of that key logged as sent, is a `burst` (note +
  metric `same_key_bursts`), not a dup: macOS handed both `>` to the app 0.97 ms apart (OS stamps 0.23 ms apart, both ~5 ms old) and the buffer went `>` then `>>`. An extra
  press of that key, or a fast pair on a key never typed doubled, is still a dup.
- **`ui_palette`**: two more proofs the palette opened: >= 3 of its filter chips (workflows, prompts, notebooks, ...) by OCR, or an app-log frame that rendered
  `CommandPaletteView` after the reopen key (`cases_ui.palette_rendered`). The dim placeholder did not OCR on the Mac agent pass while the palette was open.
- **`ui_settings_live`**: the owner's settings follow the system theme (`appearance.themes.system_theme = true`), so writing `theme` changed nothing (diff 0.001 / 0.0 with
  Pillow; the hash-only terminal pass before Pillow "passed" on the caret). The theme step first writes `system_theme = false` and lets the screen settle; the first `theme`
  write is always a change from the current value. The original file is restored as before.
- **`agent_stream_cancel`**: the app now logs where every ^C went (`input_order hop=ctrl_c_route route=<name>`, `TerminalView::ctrl_c_to_active_block` and
  `maybe_handle_ctrl_c_in_rich_content_block`; names only). The case fails if a ^C while the reply streams went to the pty, or if the first press that logged a route did not
  cancel the conversation (#851). Without Pillow a "still changing" screen is UNVERIFIABLE (noref), not a fail: md5 can prove a still screen, never a moving one (r18u13 Mac:
  3 distinct hashes from the caret and the menu-bar capture indicator, while the app log showed the 120 fps stream redraw stopping at the first ^C).

## r18u13 harness fixes (Windows live run, 5e9dfbb71)

- **Git Bash pane cwd** (regression from 70dd2f253): the scratch rc moves a shell to the work dir only when it starts in `$HOME` or `/`, so restored tabs keep
  their cwd. On Windows the app starts Git Bash in the OS profile folder (`%USERPROFILE%`, `/c/Users/<user>`), not in the scratch `$HOME`, so the Git Bash pass
  stayed there: every relative-path effect file (`cad.txt`, `alive_*.txt`, `ie_*.txt`, ...) landed in `C:\Users\<user>`, and from `cadence` on each case reported
  0 lines and a `pane_wedged_after_*` row although the pane worked (screenshot: block header `/c/Users/Winters`; r18u8's showed `~/w`). The rc now also treats
  `cygpath -u "$USERPROFILE"` as a start in home (only when `USERPROFILE` is set and `cygpath` exists, so Linux/macOS run no extra command).
- **winit trace in the Windows app log** (#1629): `WindowsApp.env` adds `winit=info` to `RUST_LOG`. The vendored winit's keyboard-layout trace (WM_INPUTLANGCHANGE
  enter/exit, `LAYOUT_CACHE` takes, layout-build timing; `lib/winit/CHANGES-PHOSPHOR.md`) logs at info, and under the bare `warn` base the r18u13 AltGr hang run
  carried none of it. winit logs nothing else through `log`, so the line count does not change outside a layout switch.
- **#1629 root cause and unit tests**: the AltGr hang is the UI thread blocked in `DefWindowProc(WM_INPUTLANGCHANGEREQUEST)` on wgpu-hal's
  never-pumped WGL instance thread (hidden top-level window). The app no longer initializes the GL backend on Windows by default
  (`crates/warpui/src/rendering/wgpu/mod.rs` `default_backends`, kill switch `PHOSPHOR_WGPU_WINDOWS_GL=1`); `rendering/wgpu/mod_tests.rs` pins the
  default set per platform. `intl_altgr_ps` / `intl_altgr_bash` are the real-app check.
- **`intl_altgr_*` final key**: the unshifted layout probe after the AltGr string is VK_OEM_1 (`ü` on German, `;` on US English). It was VK_Y expecting `z`;
  VK_Y types `y` on both layouts (the "Y key types z" rule is the physical key, which KBDGR reports as VK_Z), so a run that got every AltGr character
  right failed on the last byte. Verified on the VM: with GL left out (`WGPU_BACKEND=dx12,vulkan` on the r18u13 build) both cases PASS.

## r18u16 Windows unit-suite fixes (first complete `test-windows` run, job 17737862347)

The first `test-windows` run to finish had 51 failures in 10795 tests. Each one is either a test that assumed a Unix
host (now portable, or gated with the reason) or a Windows app defect (fixed, test kept):

- **App fixes.** Skill publishing (`skill_dirs_publish.rs`) falls back to a directory junction when Windows refuses a
  symlink (`ERROR_PRIVILEGE_NOT_HELD`, no Developer Mode), so `WARP_SKILL_DIRS` publishes for Claude Code / Codex there;
  `create_junction_*` tests cover the junction directly (Windows only). A remote whose OS is not reported yet follows the
  shape of its working directory / home (`PathHost::for_remote_session`, `crates/ai/src/paths.rs`), so a Windows client
  no longer turns `~/a.txt` on a Linux remote into `\home\u\a.txt`. `FileStatusInfo` decoding (`remote_server/diff_state_proto.rs`)
  checks absoluteness in the path's own encoding, so a Windows client accepts a Linux remote's `/repo/...`. Generated
  worktree paths are written with `/` on Windows (`tab_configs/tab_config.rs`), so a `'` in the user or repo name no
  longer makes every worktree command unquotable; saved configs with the old `\` form are still upgraded.
- **Test isolation.** The input tests' bootstrapped terminal points its history file at a path that does not exist
  (the shell's default history file is the real user's: PSReadLine's on the VM held 3016 lines). `TestCommandExecutor`
  uses `powershell` when `pwsh` is not installed. A remote path in a test is built with `StandardizedPath::try_new`,
  never `from_local_absolute_unchecked`. Paths are joined per component where the code under test rebuilds them.
- **Gated, with the reason in the test.** The relative-symlink publish case is skipped at run time where only a
  junction can be made; the #911 abandoned-connection test and two `repo_metadata` symlink tests are `#[cfg(unix)]`
  (Windows cannot delete a file SQLite holds open; the symlink tests need a real symlink).

## r18u18 Windows workspace-rest fixes (job 17744882175)

The four Windows-only `test-workspace-rest-windows` failures were tests that assumed a unix host; the app was right in each:
`is_unsafe_watch_root` rejects home's own root (`C:\` on Windows, where a bare `/` is not an ancestor of `C:\Users\...`); a remote
repo root in `repositories_tests.rs` is parsed with `StandardizedPath::try_new`; nushell double quotes process `\` escapes, so the
quoted Windows home is expected with doubled backslashes (`path_test.rs`); the reveal URI test uses a Windows absolute path on Windows.

**#1650 (sqlite setup lock budget on Windows).** Pinned on the VM with Python's sqlite3 (SQLite 3.49): SQLite's `busy_timeout`
overshoots on Windows (3000 ms gave up after 4.6-4.8 s), so one wait broke the 3 s budget. Setup now hands SQLite 25 ms slices and
retries against its own `Instant` deadline, the migration transaction included (`BUSY_WAIT_SLICE`, `sqlite.rs`). New tests:
`setup_busy_waits_are_short_slices_of_the_budget`, `setup_waits_out_a_lock_released_inside_the_budget`.

Follow-up (RC r18u22 macOS, `terminate_flushes_queued_events_once_the_lock_is_gone` failed at its own `BEGIN EXCLUSIVE`). Not the
slices: the writer's connection already went back to `busy_timeout = 1000` after setup. The race was the writer thread's startup
orphan-block GC (#1155): its `DELETE` takes the write lock even when it matches nothing, and the test's locker (a bare diesel
connection, `busy_timeout = 0`) failed at once if it landed in that window. The GC now reads first and deletes only when something
is due. The one setup path that could leave a 25 ms slice in force (a lost migration race that someone else then finished) now
restores the normal timeout too. New tests: `setup_hands_the_writer_the_normal_busy_timeout`,
`startup_gc_with_nothing_due_takes_no_write_lock`.

## r18u19 macOS app-run fixes (pipeline 3028768440, 9eb5c3f50)

Harness changes plus two small app changes. `run.py --self-test` covers the harness ones (`r18d2_routing_expectation_ocr_fuzz_and_ctrlc_window`,
`r18u19_heredoc_dialog_is_read_from_the_app_log`, `r18u19_settings_probe_and_theme_diff`).

- **`routing_table` (terminal)**: `cat the error log and tell me what is wrong` was typed in the PLAIN terminal for the first time (r18u8/r18u13
  had it inside a conversation) and went to the agent by `request-shape (#834/#965)`. That is the documented route on both surfaces (the corpus
  pins `sudo rm the old logs for me` "#1291 (plain terminal unchanged)" and `find the process that is eating my memory` #834 as Agent with an
  installed head; the owner's #964 installed-head rule covers English-looking arguments, not a sentence addressed to someone). The case's
  plain-terminal expectation was unpinned and fell back to "shell"; it is now pinned `("agent", "agent")`, and the routing corpus has the exact
  line on both surfaces (`routing_corpus.rs`). No routing code changed.
- **`remote_heredoc_paste`**: the `Run N lines one at a time?` dialog is read from the app log (the dialog's view in a `frame ... updated=[...]`
  line; a `try_execute_done` ends the look) for `6 s + 1 s per 50 KB` (at most 40 s), OCR only without a log; r18u19's 600 KB dialog was drawn
  just after the old 6 s OCR look ended, sat for 180 s and was then confirmed by the zsh step's Enter. A dialog still open after a failed step
  is cancelled with Escape. While the case waits for a remote file it samples `ps` every 2 s and records the peak %cpu of the app, bash, sshd
  and ssh (`hd_*_peak_cpu`), so the next run says whether the remote line editor or the app is the ~2 KB/s bottleneck (#1129). App side: the
  dialog is no longer shown for a bash command over the base64 wrapper size (it reaches the shell as one line, so "runs each of the N lines
  as a separate command" was untrue): `multiline_submit_needs_confirmation`, test
  `multiline_submit_does_not_ask_when_bash_gets_the_wrapper_line` (`terminal/input_test.rs`).
- **App log**: `describe_text` / `describe_bytes*` log text over 64 KiB by its count even with `WARP_INPUT_ORDER_LOG_TEXT=1` (the form the
  harness already parses): every key of the 600 KB here-doc logged the whole buffer, ~50 MB in 13 s, and the rotation deleted the minutes that
  showed the agent-mode failure (`opted_in_text_over_the_cap_is_logged_by_count`, `crates/warpui_core/src/input_order.rs`).
- **`cb_settings_ui_open` (terminal)**: each probe is judged by its own line. Probe 1 went to the still-open settings page, probe 2 ran in the
  shell after the palette close (the shot shows it), and the old `>= n lines` count read that as "did not take typing".
- **`ui_settings_live`**: the theme and font steps use `img_diff(color=True)` (largest channel difference > 8). The shots show both changes
  applied; the grey-level test (> 24) saw 0.3 % of a dark-to-dark theme recolour and 1.5 % of the font change, the channel test 18-67 % and
  3.3-3.6 %, identical shots 0.03 %. Thresholds unchanged; vertical tabs keep the grey-level test.
- **`key_loss_control`**: the macOS reference is ready when its own shell has drawn a prompt (`refctl/prompt.log`, up to 30 s), not when
  Terminal.app is frontmost: Terminal.app had restored windows from an earlier session, so it was frontmost at once and the keys went to a
  restored window while the `.command` shell was still starting.

## r18u19 Windows app-run fixes (job 17754316946, 9eb5c3f50)

77 pass, 12 fail. One app defect (H8 of r18u8, now pinned) and harness fixes; `run.py --self-test` covers the harness ones
(`r18u19_windows_where_probe_ignores_earlier_runs`, `r18u19_windows_relaunch_heredoc_io_and_back_line`).

- **App: new tabs in the Git Bash pass opened PowerShell (H8).** The harness writes `new_session_shell_override =
  { executable = "C:/Program Files/Git/bin/bash.exe" }`. The background shell scan lists Git Bash only as the MSYS2
  `Git\usr\bin\bash.exe`, so once the scan had finished the saved path matched nothing and every new tab and window fell back
  to the system default (PowerShell); the first tab, resolved before the scan finished (#1310), started bash. Evidence: the new
  tabs' commands went through `hop=pwsh_command_via_pipe`, their prompts are `C:\...\w` in the tabs_streaming shot, and the
  bash `while :; do` lines went to the agent (AI=12). A finished scan that does not list the saved shell now resolves the saved
  path directly, as the unfinished scan does (`AvailableShells::shell_for_preference`, test
  `saved_shell_the_finished_scan_does_not_list_is_still_used`). This was also the cause of `tabs_two_windows`, `tabs_streaming`,
  their `pane_wedged_after_*`, `session_restore` (the `cd X && echo` setup lines ran in PowerShell 5.1, where `&&` is a parse
  error) and `crash_kill9_restore` (the restored session's active tab was one of those PowerShell tabs, so the bash key proof failed).
- **`ssh_nested_typeahead` (PowerShell pane, H3): harness.** After the inner `exit` the inner Windows ssh client, still tearing down
  inside the fake sshd's pseudo console (`client_input_channel_req: channel 0: unknown channel`), swallowed the first 18 characters of
  the outer probe (`y01 >> ~/.appci_where` ran in the remote bash). The retry's probe was then reported "ran in the LOCAL shell" because
  the probe tokens are seeded (seed 1, the same in every run) and the PowerShell pane's `~/.appci_where` is in the real home, which no run
  clears: an earlier run's copy of that token was found. `exit` was not typed, the pane stayed remote, and the wedge probe's
  `Add-Content` lines ran in the remote bash (remote rec.log). `Ctx.local_where` now reads only what was appended to each
  `.appci_where` after the pass started (`Ctx.where_skip`, `where_tokens`).
- **`ssh_nested_typeahead` (agent mode, PowerShell pane, round 7): harness race, not the app.** The outer probe's chars were typed while the inner ssh client was
  still tearing down (the app log shows plain pass-through, `ssh_hold` routing unheld, every key written to the pty in order). The remote rec.log shows
  the outer shell ran `tac >> ~/.appci_where` (the probe's last `t` plus `ac >> ...`: the rest was swallowed by the dying client), and `tac` then read the next two
  probes from stdin. Cause: `leave_remote(level=True)` sampled `before = prompts(remote)` the moment the probe's effect file appeared, a few ms before the
  rc hook logged the probe's own prompt, so that prompt counted as the `exit`'s, the wait ended at once and the 0.5 s settle was shorter than the
  Windows teardown (~0.7 s). `leave_remote` now waits for the probe's own prompt (up to 5 s, level only) before sampling `before`
  (selftest `leave_remote_level_waits_for_the_probes_own_prompt_before_exit`).
- **`remote_session_legacy` (Git Bash pane): harness race.** The local command after `exit` ran exactly (effect file right) and is in
  the local rec.log; the check read rec.log once, but the rc hook writes the line at the NEXT prompt (`PROMPT_COMMAND` forks
  `history 1`, slow under msys). The check now waits up to 5 s for the exact line, as `local_cmd_exact` does. The clean-env step stays
  NOT RUN on Windows (H6, noref).
- **`remote_heredoc_paste` (Git Bash pane, H7): harness/platform, not pinned.** The app wrote the whole 142 KB paste to its pty in one
  write; the fake sshd's session had sent 68 bytes in total (the pseudo console's start-up output and the `~$` prompt) and sent nothing
  more until the case ended 5.5 minutes later: not one byte of the paste was echoed. The fake sshd now accounts the input side
  (`client input so far` every 64 KB, logged before the write; `conpty input: writing ... took` for a write over 2 s; `client input
  ended` per session; `conpty output: sending ... took` for a client that stops taking output), and a failing heredoc step adds those
  lines to its detail, so the next run says whether the paste reached the test sshd and whether its pseudo console stopped reading it.
- **`editor_memory_7mb` trim A/B (PowerShell pane): harness.** `editor_mem_relaunch` waited for `prompts()` to grow, which never happens
  in the PowerShell pane (no rc hook), so every relaunch read as "reached no prompt". On Windows it now waits as `run.py` and
  `cases_robust.start_app` do: the main window, a prompt (Git Bash) or the fixed settle (PowerShell), then the key proof (which also
  puts a PowerShell pane back in the case dir).

## r18u22 `remote_heredoc_paste` (#1129): plain-ssh control, verdicts, the pane left local

Harness only; `run.py --self-test` covers it (`r18u22_heredoc_wrapper_mirror_and_control_verdicts`).

- **What r18u22 showed (macOS, both modes).** The 2,000-line here-doc (142 KB, sent by the app as its 190 KB base64 wrapper line, since
  the remote `/bin/bash` 3.2 has no bracketed paste) took 86.4 s with bash at 100 % cpu and the app at 52-53 %; the 10 KB single line took
  0.54 s (bash 8-12 %). 10 KB to 190 KB on one line is 19x the size for 160x the time (exponent ~1.7): the remote readline is
  super-linear in the line length and single-threaded at 100 %, while the app had headroom. The 600 KB step (an ~800 KB wrapper line;
  extrapolated 6-17 min) had no file after 180 s (bash 100 %, the app 290 % drawing the echo; bash was still the side at its limit). It then
  cascaded: `zsh -f`, `exit` and run.py's where-probe queued behind the busy line, hence `pane_not_local_after_remote_heredoc_paste`.
- **Control.** When a step fails, or the 2k step takes over 20 s, the case runs the same bytes through plain ssh with no Phosphor
  (`plain_ssh_send`: `ssh -tt fake` in a pty that the harness drains faster than any terminal does, the same sshd and remote rc). Two
  variants, cached per run because neither involves the app: **W**, the app's own wrapper line for the 2k here-doc
  (`wrapper_line`, a byte-for-byte mirror of `posix_long_command.rs` that the self-test checks against the Rust source), and **H**, the
  here-doc itself as a terminal paste sends it (2,000 short lines). On Windows the control is **H** in the reference console (Git Bash
  + the same msys ssh + the same fake sshd, `conhost_ssh_send`), as big_paste's control is.
- **Verdicts** (`classify_timeout`, `judge_step`). Wrong bytes always FAIL. A step with no file in its budget is **ENV** when plain ssh
  with the wrapper line, scaled linearly to the step's line (a lower bound), plus the 1.5x + 10 s tolerance could not have finished in the
  time the app had. ENV is reported as `KNOWN` with the numbers while `ACCEPTED_FOR_RELEASE["remote_heredoc_remote_editor"]` (#1129,
  v0.1.17) is in force, and FAILS after that release. If plain ssh was clearly faster, the step FAILS as APP. A step the control does not cover
  FAILS: no control, a step that was not provably wrapped (bash with bracketed paste and under 512 KB), zsh, fish, or the 10 KB line. A 2k
  step that finished but took over 1.5x + 10 s of the plain-ssh time also FAILS. When H is much faster than W, the text says so: on that
  remote bash the app's one-line wrapper, not the paste, is the slow path, and that design question stays open on #1129. On Windows, a
  reference console that fails the same here-doc makes the step NOREF (PLATFORM) and one that delivers it makes the step FAIL as APP. The
  fake sshd's received-byte count (`hd_*_sshd_got`) is part of the text either way.
- **The pane is left local.** The remote shell's pid is recorded on connect (`echo $$ $(cat /proc/$$/winpid) > ~/.appci_rpid`). When a step
  times out, the harness ends that shell and its descendants (POSIX: SIGKILL to the descendants, then SIGHUP and SIGKILL to the shell;
  Windows: `taskkill /T /F` of the msys shell's Windows pid). It then proves the pane LOCAL and reconnects for the next step. The 600 KB step
  now runs LAST, after zsh and fish. The 2k step keeps its 90 s verdict budget but waits up to 150 s more (`hd_2k_txt_s_after_budget`), so
  a run that is just over the budget keeps a usable session.

## #1652: keys typed while the multi-line submit dialog is open

The `Run N lines one at a time?` dialog takes focus; a key its bindings did not take was dropped unseen
(`kh_editor_elem_typed ... live_focus=no accepted=false`) and the next Enter answered it. Owner rule: anything typed
there that is not part of the approval is thrown away (`DECLINED.md`, "Keys typed while the multi-line submit dialog
is open are discarded"). The asking Enter is a gate on the pane queue, so no key edits or submits the waiting buffer
while the dialog is open; the dialog is a pane key sink that hands each key to the asking input's editor (typed text
as typed text, a key down dispatched on the editor's responder chain, `AppContext::responder_chain_of`), where it
queues; the answer discards them all with a log line and a toast. A test window has no workspace, so
`multiline_submit_fixture_with_terminal` marks the dialog as shown (`Input::set_multiline_dialog_shown_for_test`);
`TerminalView::settle_enter_queue_for_test` drives the view's after-Enter settle. `multiline_dialog_fixture` also
registers the editor's key bindings (`crate::editor::init`), and `enter_in_editor` asserts its Enter was taken: without
them that Enter matched no binding, never reached the input, and the cancel test's `[Text("ls"), Enter, Text("pwd")]`
came out as `[Text("lspwd")]` (r18u23; the confirm test's Enter was silently lost the same way). Log hops: `enter_dialog_gate`,
`enter_dialog_answered`, `enter_dialog_typeahead_discarded`, `enter_dialog_cancelled`, `enter_queued_behind_dialog`,
`kh_multiline_dialog_key`.

| guarantee | test |
|---|---|
| keys on the dialog (text; Backspace is swallowed there) and an Enter in the editor (the editor's real binding) wait behind the asking Enter in typed order without touching the buffer; Cancel discards them: buffer exactly as it was, nothing runs or reaches the pty, a toast (both `DestinationMode`s) | `multiline_dialog_cancel_discards_the_held_keys` (`terminal/input_test.rs`) |
| VimBackspace, a file drop, Ctrl-U (real dispatch), Escape are held, never applied, then discarded; the clipboard is untouched (both modes) | `multiline_dialog_holds_unmapped_keys_then_discards_them` |
| Confirm runs exactly the buffer, once; the held keys (text, Enter, ctrl-w, a paste) are discarded: nothing reaches the pty or the editor, even after the view settles; a toast (hand-off on, both modes) | `multiline_dialog_confirm_discards_the_held_keys`, `dialog_typeahead_discarded_toast_text` |
| ^C / ^D void the waiting Enter and what followed it; a signal reaching the queue directly keeps the gate | `ctrl_c_voids_the_enter_waiting_for_the_multiline_dialog`, `ctrl_d_voids_the_enter_waiting_for_the_multiline_dialog`, `a_signal_at_the_queue_keeps_the_multiline_dialog_gate` |
| no dialog shown, nothing held (F9) | `an_unshown_multiline_dialog_holds_nothing` |
| the answer goes to the asking input only, to nobody when it is gone (F6) | `the_multiline_dialog_answers_only_the_input_that_asked` |
| the dialog's Enter confirms and Escape cancels (F7) | `multiline_dialog_enter_confirms_and_escape_cancels` |
| a second ask is refused; only the asker's ^C dismisses (F5) | `a_second_multiline_dialog_ask_is_refused_and_only_the_asker_dismisses_it` (`workspace/view_test.rs`) |

## r18u23: the two Mac cases that stayed red (r18u22)

- **`flood_scrollback_find` (terminal, Mac): HARNESS, and the case was vacuous.** The marker was the first output line of the same block as the 500,000-line flood, and a block keeps at most `terminal.maximum_grid_size` rows (default 50,000, `terminal/settings.rs`), so the marker was evicted before Find ran. The r18u19 "pass" was OCR reading the marker in the Find FIELD (`shots/terminal_find_after.png` of r18u19: the output still shows 499967..500000, nothing scrolled); r18u22 failed because OCR missed that field text behind a macOS notification banner. The injector's "Enter: sent 2, app saw 1" is not a loss: the second Enter is the Find field's (`mac_enter app_send_event` + `core_enter ... handled` in the app log, no `enter_begin`), and the agent pass shows the same 210/205. Now the marker is its own command (its own block), proven to have run in the shell (rec.log; an Enter lost before the app is retyped, up to 3 tries, each noted), the flood is the next block, and the check needs the marker AND a view that scrolled away from the flood's tail (499999/500000 on screen before the Find, gone after).
- **`ssh_nested_typeahead` (terminal, Mac): APP-side designed give-up under load, not a connect failure and not the `local_where` change.** In the app log the first ssh connected and the hand-over released the 5 held keys; the second `ssh fake` Enter arrived 38 ms later, the shell-or-chat verdict had spent 20.4 ms of its 20 ms compute cap, and by #1577 the clock never chooses: `kh_enter_gate_give_up by=verdict_cap submitted=false`, the line stayed in the editor and `ssh fakeecho T..` ran at the first remote. Nothing ran in a wrong place. The case now notices a verdict give-up in the app log, recovers (probe + proven `exit`) and runs the trial again, up to 3 trials, every give-up noted and in the metric `verdict_give_up_retries`; the third give-up fails it.

## r18u24: `flood_scrollback_find` is APP (#1775)

- **The r18u23 failure (Mac both modes, Wayland) was the app, not the case.** With the default async find
  (`async_find` is a default cargo feature, so `FeatureFlag::AsyncFind` forces it on) `TerminalView::scroll_to_match`
  read the focused match from the sync `block_list_find_run` only, which the async path never fills: Find found and
  highlighted the marker in its older block but never moved the view (`view scrolled away from the flood's tail=False`).
  The async port (e2226e40a) missed the pin's `focused_block_list_match`, and e12a0f2cb removed that helper as
  zero-caller. It is restored and used; a find or Enter that runs before the background scan has reported leaves the
  scroll pending until the find model's next `RanFind` (cleared on scroll, on a scan that ends with no match, and on
  find-bar close). The injector's "Enter: sent 3, app saw 2" is the Find field's Enter, as in the r18u23 note: not a loss.
- Unit tests (`app/src/terminal/view_test.rs`): `find_scrolls_to_older_block_above_{huge,normal}_block_async` (+
  `_inverted_blocklist`), `find_scrolls_to_older_block_above_huge_block_sync`,
  `find_scrolls_to_older_block_above_normal_block_sync_inverted_blocklist`: a marker block, then a flood block (capped
  grid, truncated, or a normal 1,000-line block); control: the marker is off screen; Find + an immediate Enter must put
  it on screen and leave nothing pending. `async_find_without_match_leaves_no_pending_scroll`: a match-less scan clears
  the pending scroll, so later streaming output cannot pull the view.
- r18u26 (test-linux): the first version of these tests did everything in ONE update. The input mode's subscription
  (`set_is_inverted`) and the commands' model events (block completed, wakeups, the first layout) then ran after the
  Find instead of before it, unlike the app: the inverted controls failed (the list was not inverted yet, and an
  inverted list follows the newest block's bottom, which borders the marker), and the async scans reported 0 matches.
  Now each step is its own update with the app settling in between; the inverted cases first show the flood's head
  (its far end from the marker), so the control means the same in both layouts; the async wait needs 100 ms of
  steady `Complete`; and the precondition messages print the async find's per-block state.

## r18u27: `flood_scrollback_find` still red after the #1775 fix (r18u26, Mac + Wayland) — the scroll check

- **HARNESS: the "scrolled away from the flood's tail" check could never pass.** It was a substring test for
  `500000`/`499999` on the whole OCR text. The flood's own command (`... do seq 1 500000; __i=$((__i+500000)); done ...`)
  contains `500000`, the block header keeps that command on screen, and a Find that does scroll to the marker block puts
  the match 5 lines above the viewport's bottom (`block_list_viewport.rs` `scroll_to_match_if_not_visible`, `SCROLL_OFFSET`)
  with the flood block's command header right below it. So `scrolled` read False whether or not the view moved (r18u26
  Mac: `marker read=True, scrolled=False`). The tail is now an OCR line of just `499999`/`500000`
  (`cases_robust.flood_tail_on_screen`, self-tested against the command text).
- On a miss the case now keeps taking screenshots for up to 12 s after the Enter (`shots/<mode>_find_late.png`) and says
  whether the view got there later (a slow scan of the 50,000-row block) or never; the 1.5 s verdict is unchanged.
- The app logs each Find scroll (`input_order hop=find_scroll result=scroll block=N moved=<bool> from=<pos> to=<pos>`, or
  `result=no_match pending=<bool>`; `TerminalView::scroll_to_match`), and the case quotes them, so a red run says whether
  the app scrolled. Wayland r18u26 read no marker at all (also not in the Find field, as in r18u22): the next run's
  `find_scroll` lines decide whether that is OCR or the app.

## r18u29: `flood_scrollback_find` red in agent mode only (r18u28, Mac RC 3d6d6ed21)

- r18u28 agent pass: `marker read=False`, the flood's tail gone (`tail_visible_before=True`, `after=False`), still no marker 12 s
  later; the app log: the Find matched the marker in block 16 and scrolled (`moved=true`, `FollowsBottomOfMostRecentBlock` ->
  `ScrollTop(30.45)`), the same shape as the terminal pass (block 17, `ScrollTop(31.9)`). So the marker's block exists in the
  agent pane (the owner's "scrollback wiped" hypothesis is ruled out for this run: Find only matches blocks of non-zero height
  in the current transcript, `find/model/block_list.rs run_find_on_sumtree`). r18u26 (commit 479fc797d) shows the same agent
  pass putting the highlighted marker on screen (`shots/agent_find_after.jpg`), and no find/scroll/agent-view code changed
  between 479fc797d and 3d6d6ed21 except the hop line. OCR read the marker nowhere, not even in the Find field it was typed
  into, as in Wayland r18u22/r18u26; the r18u28 screenshots were not readable from the laptop. Not pinned to the app.
- LOGGING: the `find_scroll` hop now carries the app's placement of the match in the DRAWN viewport
  (`TerminalView::find_match_geometry`: `viewport_lines= rendered_mode= scroll_mode= agent_fullscreen= match_line= block_visible=`).
  The case parses it (`cases_robust.find_scroll_geometry`, self-tested) and notes "the app placed the match ON screen ... OCR read
  the marker nowhere" (look at the screenshot) or "OFF screen by the app's own geometry" (an app bug), and any disagreement
  between the mode the scroll was computed with and the mode drawn. The verdict is unchanged.
- Agent mode: when every hop is `result=no_match` the marker's block is not in the pane and the case SKIPs with that reason
  (owner 2026-11-05); a check that failed before it stays FAIL (`run.skip_status`).
- Unit tests (`view_test.rs`): `find_scrolls_to_older_block_above_huge_block_async_in_agent_view`,
  `find_scrolls_to_older_block_above_normal_block_sync_in_agent_view`: the #1775 marker + flood test with both commands run in a
  full-screen agent view (their blocks belong to its conversation), judged in the viewport that view draws.
- Found on the way (latent, not this run's cause: the owner profile is the default `PinnedToBottom`): the full-screen agent
  view DRAWS `PinnedToBottom` (`current_input_mode`) but `apply_scroll_update` computes every scroll, the Find scroll included,
  with the `input_mode` SETTING. With `PinnedToTop` (or a Waterfall gap) the two disagree in the agent view.

## Busy-machine data-loss timers: pane-close hang-up grace and the quit-time clipboard hand-off (#1685, #1692, #1693)

From the busy-machine timer audit (`docs/audits/2026-11-05-busy-machine-timers.md`, H25, H32, H33). Owner
principle: the app must not need a quiet machine. Each timer is a give-up net that ends early on its event, so
raising it costs a normal close or quit nothing.

- **#1685 `HANGUP_GRACE` (300 ms → 5 s, `app/src/terminal/local_tty/process_tree.rs`).** On a pane close the
  shell's own group and the pty's foreground group get SIGHUP, and whatever is still alive after the grace is
  SIGKILLed; it used to be killed mid-save after 300 ms. The shell's own group gets the same grace as the foreground
  job (review F2): `exec vim`, a launch command run as the pty child, a shell without job control, zsh saving its
  HISTFILE. The shell is probed with `try_wait` (reaped once it exits, so its zombie no longer counts) and its group
  with `killpg(g, 0)`; the shell group is SIGKILLed only if the shell still runs or other members remain. An idle
  shell exits in ms, so the wait ends at once. The grace runs on the "PTY child reaper" thread, so the close is
  instant; the inline fallback when that thread cannot be spawned uses no grace (it runs on a pty or server loop).
  At app quit nothing waits on a reaper: if the process exits inside a grace the SIGKILL is never sent; the programs
  have had their SIGHUP, and one that ignores it keeps running, as after closing a plain terminal emulator.
  Tests (`process_tree_tests.rs`): fake-clock policy (`wait_for_tree_to_exit`)
  `program_that_exits_within_the_grace_is_never_killed`, `program_that_ignores_sighup_is_killed_after_the_grace`,
  `only_the_groups_alive_at_the_grace_are_killed`,
  `program_in_the_shell_group_that_exits_within_the_grace_is_never_killed`,
  `shell_group_is_killed_only_if_something_in_it_outlived_the_grace`, `idle_shell_close_ends_the_wait_at_once`,
  `hangup_grace_survives_a_loaded_machine`; real processes on a pty (Linux): `idle_shell_close_is_fast`,
  `close_returns_at_once_while_the_reaper_sits_in_the_grace` (1 s grace via `terminate_and_reap_with_grace`),
  `foreground_cleanup_after_sighup_is_not_killed` and `shell_group_cleanup_after_sighup_is_not_killed` (a 1 s
  cleanup finishes, in the foreground job and in the shell's own group after `exec`), and
  `foreground_program_ignoring_sighup_is_killed_after_the_grace` (1 s grace; a second thread sees the program alive
  200 ms before the grace ends, or overslept past the grace). A shell that is not a session leader leads no group
  and its `/proc` tpgid is the inherited terminal's (under nextest, possibly the runner's own group), so it is hung
  up alone with `kill`, no group is targeted or probed (`shell_that_leads_no_group_is_never_group_probed`). The
  real-pty bash spawns set `HISTFILE=/dev/null` so a run never writes the developer's history.
- **#1692 / #1693 Linux clipboard at quit (`crates/warpui/src/windowing/winit/linux/clipboard.rs`).**
  `EXIT_PENDING_WRITE_WAIT` 300 ms → 2 s (woken by the serial writer's Condvar), `EXIT_HANDOFF_BUDGET` 600 ms → 3.5 s
  (woken by the helper thread's done message, `run_with_budget`), and arboard's clipboard-manager handover wait
  100 ms → 1 s (`lib/arboard/src/platform/linux/x11.rs`, woken by `handover_cv` when the manager has fetched the
  data). The handover is skipped when nothing owns `CLIPBOARD_MANAGER` (review F1): without a manager the X server
  answers with a None-property `SelectionNotify` that arboard rightly does not take as "done" (KDE 5.18 sends one and
  then requests anyway), so every quit after a copy would otherwise wait out the whole handover. `EXIT_HANDOFF_BUDGET`
  is derived: pending wait + handover (`arboard::MANAGER_HANDOVER_WAIT`) + 500 ms margin = 3.5 s, under the 5 s signal
  `SHUTDOWN_DEADLINE`. Not capped by the time left on that deadline: when the watchdog fires the process exits,
  and the clipboard handoff is the last quit step. Tests (`clipboard_tests.rs`,
  `bounded_wait_tests`): `exit_waits_survive_a_loaded_machine_within_the_shutdown_deadline`,
  `exit_handoff_wait_ends_when_the_work_does`, `exit_handoff_wait_gives_up_at_its_budget`, with the existing
  `exit_wait_is_woken_by_the_writer_finishing_not_polled`.

## r18u22 Wayland app-run triage (job 17757564474, 2ecf74940)

- **`cb_settings_migration`.** The truncated arm failed in both modes on the app's own `[ERROR] Settings file could not be parsed: ...` line, which is
  its notice of the broken file (it started in 0.9 s, kept the file byte for byte, showed the banner). `startup_log_problems(text, broken_settings=True)`
  no longer counts that notice on the truncated arm (every other ERROR, and any panic, still counts); the arm now requires the notice
  (`settings_notices`) and checks `truncated_file_unchanged` instead of only recording it. The agent pass's 0/4 typed lines on the truncated file
  stays a failure; the arm now notes the view before/after and the burst's agent submits (`ai_submits`), because on defaults
  (`restore_session = true`, no per-pass overrides) where a typed line goes is the open question. **macOS:** `MacApp._install_settings` copied the
  profile file over `~/.phosphor/settings.toml` on EVERY start, so the Mac pass never tested the legacy or truncated file (it passed with
  `truncated_file_unchanged False`, `legacy_entries_kept 0`). It now copies only when the profile file changed (a new pass) or the app's file is
  missing. App side: the log said `unknown error` for every syntax error; `user_preferences::Error::Syntax` now says `invalid TOML at line L,
  column C: <parser message>` without quoting the file, and a syntax error found by a RELOAD (the user saved a broken file while the app ran)
  inhibits writes as the startup one does, so the next setting change in the app can no longer overwrite the user's edit
  (`toml_backed_tests.rs` `test_syntax_error_names_line_and_column_without_quoting_the_file`,
  `test_runtime_syntax_error_inhibits_writes_until_reload_succeeds`).
- **`ssh_drop_reconnect`.** On Wayland the `<Enter>~.` escape left the session up, and the rest of both arms ran inside it (the reconnect nested a
  second ssh, the kill arm's `kill -9 $$` returned to the first remote shell): one failure read as nine. When the escape does not drop the session
  the arm now quotes the app's `ssh_escape*` hops from the log, clears the line, leaves with `exit` and skips to the kill arm, which is then judged
  on its own. The app logs `hop=ssh_escape_refused char=. why=not_insert_char|ai_locked|no_active_session|not_managed_ssh|shared_viewer` whenever
  the editor holds exactly `~X` and keeps it (`terminal/input.rs ssh_escape_typed_at_line_start`), so the next run says why.
- **`session_restore`** (Wayland, intermittent, both modes): every tab key answered from one middle tab. The case records the app windows before the
  quit and after the restore (`windows_before_quit` / `windows_after_restore`, sway titles) and, on a mismatch, the `hop=tab_activate index= count=
  active= window=` lines the app now logs for every tab activation (`workspace/view.rs activate_tab`): no line = the key never reached the
  workspace; `count=1` = the focused window had one tab (a restore that split or dropped tabs).
- **`shell_nu`** (agent): the Linux `App` now puts `USER_BIN_DIRS` (`~/.local/bin`, `/home/gitlab-runner/.local/bin`, ...) on the app's PATH
  (`app.with_user_bin_dirs`). The r18cz fix for this lived only in `WindowsApp.env` under `os.name != "nt"`, i.e. never ran, so on the box nu
  was not on the app's PATH and the agent view sent a bare `nu` to the agent.
- **`ui_mouse_select_copy` / `ui_right_click_paste`** (Wayland): the terminal pass failed three runs running on the same token (`mselt409jy2dwo`,
  seed 1: tesseract misreads glyphs of the terminal font). Tokens the case must find on screen are drawn from `OCR_SAFE` glyphs (`ocr_safe`,
  the seeded rng sequence unchanged) and `find_token` accepts one misread glyph. `ui_right_click_paste` looked for the prompt text `~/w$`, which a
  Phosphor-drawn prompt does not show; it now types an OCR-safe probe into the input, finds it, clears it with Ctrl-U and right-clicks where it
  began. The agent pass's copy (clipboard kept the sentinel) is unresolved: sway `seat - cursor` pointer synthesis had never got this far on
  Wayland; the case now records `dblclick_screen_diff` (0 = no selection drawn) and notes it when the sentinel survives.


## r18u23 Windows app-run fixes (job 17761810079, 0a37554a5)

Harness only; `run.py --self-test` covers it (`r18u23_paste_backlog_and_bash_agent_command_count`).

- **`big_paste`'s control logs to its own files.** The Git Bash reference console sourced the pane's `~/.bashrc`, so its prompts and
  `cat > big_ref.out` went to the pane's `prompt.log` / `rec.log`. `big_paste_back_at_prompt` (r18u22) counted those prompts as the
  pane's and returned while the pane's `cat` was still running. It ran for another 14-19 s, the time ConPTY took to reach the Ctrl-C
  behind the 1 MB paste. `tab_enter` then typed all 12 trials into it (0/12 in both modes, `completion_give_ups 0`, no Enter reached the
  editor). The control now uses `prof.ref_rc`, which logs to `refctl`, as the Linux and macOS references do (#1500, #1550).
- **A paste backlog relaunches the app.** Under ConPTY, the part of a paste that the receiving program (`cat`, an ssh client) never read
  stays queued as console input. Once that program ends, Git Bash reads the rest line by line as commands, one fork per line.
  `rec.log` showed this twice. After `big_paste`, the payload lines ran into `tab_enter` and the wedge probe
  (`pane_wedged_after_tab_enter`). After `remote_heredoc_paste` ended its stuck remote shell, ssh had read only 284 KB of 600 KB, and the
  local shell then ran 541 payload lines over 150 s (`pane_not_local_after_remote_heredoc_paste`).
  `paste_backlog_count` detects this: it counts the payload lines in `rec.log` since the step's mark, waiting up to 5 s. If it finds
  any, `relaunch_after_paste_backlog` notes the count and relaunches the app (`cases_robust.start_app`: prompt, key proof, pane state).
  Windows only; `big_paste` checks after its prompt wait, and `end_stuck_session` checks after it kills the remote shell. The backlog
  is the platform's, and the conhost control queues the paste the same way, so it is not a verdict on the app.
- **`agent_approval` in a bash pane.** The app keeps the agent's command wrapper out of the shell history by design (#904: HISTIGNORE on
  `__phosphor_ap_`), and the bash rc hook builds `rec.log` from `history 1`. A bash pane's `rec.log` therefore never shows an approved
  command; the zsh hook logs from preexec, so macOS is unaffected. In a bash pane (Windows Git Bash, Linux), if `rec.log` does not show
  the command, it must instead appear in exactly one of the app's `hop=pty_write_bytes class=UserCommand` writes, **but that hop is
  Windows-only** (`event_loop.rs`: `if cfg!(windows)`), so a Linux log has none (the case reported "0 pty writes" although the command ran).
  `approved_once_verdict` therefore counts the hop only when the log carries it; otherwise the effect file (`echo tok >> ap_ok.txt`, one
  line per run) is the count: exactly `[tok]` passes, no line or two lines fail (selftest `r18u23_...`). The rejected command's hop check
  likewise runs only where the hop exists; the empty `ap_rej.txt` check covers the rest.
- **Triaged, not harness:** `tabs_streaming` (Git Bash, both modes, 8/10) and `tabs_two_windows` (PowerShell, 15/16) are the #1658
  mechanism. (Historic: a young Enter hold on ConPTY, then `TimingWindow`, reached `young_hold_cap` before the Preexec that would have released it.
  It hands the Enter and the keys behind it back to the editor, while later keys still go to the pty, so the line is reordered or
  left in the editor. A1 (bdf6f4fac) did fix the second window: it opened this time.

## r18u28 Windows app-run fixes (ssh/schtasks run, 3d6d6ed21)

Harness only; `run.py --self-test` covers both (`app_log_readers`, `mac_close_key_empty_never_crashes_and_latency_guards`).

- **`Ctx.log_size` / `Ctx.log_since` survive the app's log rotation.** The app renames its log to `<log>.in_session.0` at 10 MB (#1187)
  and opens a fresh active file. `log_since(off)` used to seek into the NEW file and returned `""` until it grew past the old offset, so
  every log-read check after a rotation saw nothing. `agent@gitbash agent_approval` counted 0 `pty_write_bytes` of an approved command
  whose effect file showed it ran exactly once, and reported "the follow-up question did not open a model request within 60 s" (r18u27:
  1.05 s); its case row also lost the `pty writes` / `enter_route` evidence. `log_size()` now returns a `LogOffset`, an int that also
  carries the identity of the file it measured (inode / NTFS file index, else its first 256 bytes). `log_since` resolves it to that
  file wherever the rotation moved it and reads every newer chunk after it. A plain int still reads the active file as before.
- **`intl_wide_edit`'s lead word is unique per run, not just per seed.** The lead (H12) came from the seeded token stream, so every
  seed-1 run typed the same `Write-Output k2t241_86giBq | ForEach-Object { ...('<run dir>\ie_2.txt'...) }`. PowerShell writes every
  accepted line to the user's real PSReadLine history (`%APPDATA%\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt`; the
  harness wipes only `%APPDATA%\phosphor`), and the app reads that file for autosuggestions (`ShellType::history_files`). In edit 2 the
  End key lands at the end of the line, so it accepted the earlier run's line, path included. CI runs share one build directory, so the
  doubled pipeline still wrote the right file. The ssh/schtasks run used another directory, so the accepted text wrote to the OLD path
  and the new file never appeared (`edit 2 (BackSpace): file bytes None`, PowerShell pane, both modes; Git Bash keeps its history in the
  run's HOME). The lead now ends in `EDIT_LEAD_NONCE` (6 hex digits from `os.urandom`, not the seeded rng, so no other case's tokens
  change). The case notes when the history already holds a line with the seed's lead, which confirms the cause on the next PowerShell run.

## models.dev option (#1791)

Unit tests in `app/src/ai/agent_providers/models_dev_bundle_tests.rs` cover `models_dev_fetch`/`models_dev_url` on both entry points (startup `ensure_loaded`, Refresh `refresh_catalog`), the cache -> bundled fallback, response validation, and that the bundled gzip matches `app/resources/models_dev/PROVENANCE.txt` and `assets/models_dev/api.json`. They share one catalog singleton and serialize on a mutex. Wave N changed the defaults (`models_dev_fetch = false`, Refresh always fetches); see 2.17.

## Stuck agent actions: a pending or running action with no usable UI (#1797, #1799)

Invariant under test: **no agent action is pending without a visible card, and nothing waits on an answer no UI can give.** Unit and
in-process tests, no app test yet (the app cases are Wave 0.5's, below).

| test | file | what it proves |
|---|---|---|
| `answers_staged_before_execute_are_its_result` | `action_model/execute/ask_user_question_tests.rs` | #1797: the question card stages its answers per action, then executes; `execute` returns them synchronously. The old card wrote them to the shared decision channel, which `execute` drains on entry (#1278), so the answer was dropped and the turn hung on "Preparing question..." |
| `staged_answers_belong_to_their_action` | same | an answer staged for one question is not taken by another (the #1278 concern without a drain) and is dropped with its action (`discard_action_state`). The cards stage only while the question is blocked and discard anything the execution did not take (a cancel or a second submit leaves nothing behind) |
| `a_question_list_with_nothing_to_show_returns_an_error_at_once` | same | `convert_from` keeps only multiple-choice questions; a call left with none runs without approval (even under `AlwaysAsk`) and returns an `Error` result, never a pending action |
| `user_accept_of_a_question_needs_its_staged_answer`, `question_answer_guard_leaves_other_actions_alone` | `action_model_tests.rs` | `execute_action` (every by-id accept: the generic Run / Enter `ExecuteRequestedAction`, TUI views) refuses a question with no staged answer; other actions and an empty question list are unaffected |
| `only_a_user_reject_cancels_the_rest_of_the_turn` | same | #1799: only `ManuallyCancelled` (Reject, the status bar's Ctrl-C / Stop, the TUI reject) cancels the later actions of the turn |
| `transfer_asks_for_approval_only_over_a_running_command`, `transfer_with_no_running_command_resolves_without_approval` | `action_model/execute/shell_command_tests.rs` | #1799 invariant: `transfer_shell_command_control_to_user`, whose only card is the CLI subagent view over a running command, waits for approval only while a command runs; with none it runs at once and returns `BlockNotFound` |
| `cancelled_transfer_reason_reaches_the_model` | `agent_providers/tools/markers.rs` | a cancelled transfer has no API form (`convert.rs` ignores it), so `serialize_action_result` gives nothing and the model reads `display_fallback_result` (the live `chat_stream` request and the persisted message both use it): the result's `Display` carries the reason (`TRANSFER_CANCELLED_REASON`) |
| `byop_message_for_a_cancelled_transfer_says_why` | `blocklist/controller_tests.rs` | the persisted BYOP tool-result message (`byop_action_result_message`) for a cancelled transfer has no structured result and its `server_message_data` carries the reason |
| `transfer_with_nothing_running_says_why` | `agent_providers/tools/markers.rs` | the `BlockNotFound` result (it has an API form) carries a `detail` saying nothing was handed over |
| `rejecting_a_command_cancels_the_rest_of_its_turn` | `terminal/view_test.rs` | the #1799 turn (`run_shell_command` held for approval + `transfer_...` behind it, a follow-up queued `PendingApprovalFollowUp`): Reject cancels both, results in tool-call order, nothing left pending, conversation `Cancelled`, queued row unlocked, no request sent on the user's behalf |
| `ctrl_c_cancels_a_turn_blocked_on_pending_actions` | same | Ctrl-C on a `Blocked` turn with no unfinished AI block (it matched no Ctrl-C route before and did nothing) cancels the whole turn, same end state |
| `ctrl_c_on_a_blocked_turn_in_the_agent_view_releases_the_queued_follow_up` | same | in the agent view the queued follow-up comes back into the input after the Ctrl-C (one Enter sends it). A cancelled turn never auto-sends a queued row (#690, #1026), so "sent" means released, not fired |
| `a_question_finishes_only_with_its_cards_answer`, `a_question_with_nothing_to_show_resolves_with_an_error` | same | through the action model: an accept with no answer leaves the question blocked (the card stays up), the card's submit finishes it with its answers; an empty question list resolves itself with an error |

**Owner decision 2026-11-08: a question has no approval step.** Its card shows as soon as it is queued and blocks; answering it IS the
approval. The executor decides a question by `AskUserQuestionExecutor::waits_for_answer` alone (answer staged or not), not by the generic
confirmation policy (initiator, autonomy mode, the conversation's auto-execute override). A skip (the card's "Skip all") or a cancel
(Reject / Stop / Ctrl-C) resolves it with one result; nothing else does.

| test | file | what it proves |
|---|---|---|
| `a_question_waits_for_its_answer_and_only_for_its_answer` | `action_model/execute/ask_user_question_tests.rs` | with auto-approve on, a question waits until answers are staged for it, then runs; another question's answer does not release it |
| `a_question_with_nothing_to_ask_does_not_wait` | same | an empty question list and a `Never ask` profile do not wait (error result / skipped result), so nothing is left pending without a card |
| `a_question_shows_its_card_with_no_approval_step_and_a_skip_resolves_it_once` | `terminal/view_test.rs` | queued through the real pipeline, the question blocks with no accept and its `AskUserQuestionView` renders at once (no generic Run drawn); the card's Skip all gives one `Success` result, every question skipped |
| `cancelling_a_question_resolves_it_once` | same | a `ManuallyCancelled` cancel gives exactly one `Cancelled` result; a late accept finds nothing to run |
| `autonomy_max_still_waits_for_the_answer` | same | run-to-completion plus an always-allow profile (questions "unless auto-approve"): the agent drain, the accept by id and the plain accept leave it blocked with no result; the staged answer completes it |
| `a_plain_accept_does_not_complete_a_question_its_card_stays_up` | `crates/warp_tui/src/tui_ask_question_view_tests.rs` | TUI parity: the TUI card is up on a blocked question; the accept by id and the plain accept leave it blocked and awaiting answers |
| `a_blocked_question_awaits_an_answer_not_an_approval` | `crates/warp_tui/src/tool_call_labels_tests.rs` | the TUI transcript label of a blocked question says "(awaiting your answer)", never "(awaiting approval)"; other tools keep "(awaiting approval)" |

### Wave 0.5 fake-model scenarios for these issues (to build; scripted BYOP stand-in)

Each scenario runs in agent mode and terminal mode, on the owner setup, with a screenshot per step and the exact text the model receives
read from the stub server's request log. Fail on any pending action without a card (screen) or an `action_model` pending entry after the
turn ends (app log), on any turn that does not end within 10 s of the last user step, and on a queued follow-up that stays queued.

| scenario | script (tool calls the stub returns) | user steps | expected |
|---|---|---|---|
| `aq_valid` | `ask_user_question` with one multiple-choice question | answer on the card | card shows at once; the next request carries `{"status":"ok","answers":[...]}` |
| `aq_run_without_answer` | same | press Enter / click any generic Run before answering | the card stays, focus moves to it; nothing is sent; then answer: as `aq_valid` |
| `aq_empty` | `ask_user_question` whose only question is not multiple choice (dropped by `convert_from`) | none | no card, no "Preparing question..."; the next request carries the `Error` result |
| `aq_malformed` | `ask_user_question` with arguments that do not parse | none | the existing invalid-arguments path: a visible error and a retry, never a hang |
| `aq_ctrl_c` | `ask_user_question` | Ctrl-C while the card is up | turn cancelled, card shows Cancelled, a typed follow-up sends |
| `reject_then_transfer` (#1799) | `run_shell_command` (held), `transfer_shell_command_control_to_user` | Reject the command | both cancelled, no hidden pending action, queued follow-up released; the next request carries both cancelled results; the transfer's text (its `Display` fallback, `{"result": ...}`) says it was cancelled because an earlier call of the turn was rejected |
| `reject_middle_of_three` | `read_files`, `run_shell_command` (held), `grep` | Reject the command | `read_files` result kept, `grep` cancelled with the turn |
| `approve_then_transfer` | `run_shell_command` (a real password prompt), `transfer_...` | approve | the CLI subagent card for the transfer appears over the running command; approve hands over |
| `transfer_after_fast_command` | `run_shell_command` that finishes at once, `transfer_...` | approve the command | the transfer resolves on its own with `block_not_found` (no card, no wait) |
| `ctrl_c_blocked` | `run_shell_command` (held), `transfer_...` | type a follow-up (queued), then Ctrl-C | turn cancelled, follow-up released into the input (agent view) or unlocked in the queue |
| `stop_button_blocked` | same | Stop button instead of Ctrl-C | same as `ctrl_c_blocked` |

Fuzzer oracle (the Wave 0.5 grammar generator): after every generated user step, each `pending_actions` entry is the front action with a
rendered card or is behind one; nothing is `RunningAsync` for an `ask_user_question`; a turn whose actions are all finished leaves the
conversation not `Blocked`. #1797 and #1799 are the acceptance cases: the fuzzer must find both from scratch on the pre-fix build.

## Agent tests: several agents across tabs, and misbehaving MCP servers (#1833, #1834)

Test-only cases for round 6; nothing here has been run (no cargo and no app on the laptop). Both need the scripted fake model (`APPCI_FAKE_MODEL=1`) and run in the **agent** pass on **Linux and macOS**; Windows is classified `NOT_WINDOWS` in `winmap.py` with the reason. Without the fake model, an app log, or the preconditions below they SKIP with the reason; they never pass on an unproven premise. Run: `python3 script/realapp/appci/run.py --cases agent_multitab_streaming,agent_mcp_faults` with `APPCI_FAKE_MODEL=1`.

Fake model additions (`appci/fake_model.py`, self-test `agent_multitab_pure_pieces`): `Please stream long reply <TAG> chunks <N> gap <MS>` streams N tagged chunks `[<TAG>:0001] ...` then `[<TAG>:END]` on an open close-delimited stream, one per gap, and records it in `fake.streams[TAG]` (chunks written, finished, broken = the app dropped the connection). `Please follow up check <TAG>` answers `CHECKED <TAG> <verdict>` where `history_check()` read the conversation the app sent back (own chunks 1..N each once, END, no marker of another tag). `Please call mcp tool <TOOL> on <SERVER> [with <json>]` emits the function call `mcp__<SERVER>__<TOOL>` when the app offered it in the request's `tools` (else `NO-MCP-TOOL`), and every tool-role message the app sends back is kept in `fake.tool_results`. `fake_model.ACTIVE` is the running instance (the cases read its records; `run.py` is unchanged).

- **`agent_multitab_streaming`** (#1833). Tab 1 is the typing tab; `APPCI_MT_TABS` (6) more tabs each start a long reply (`APPCI_MT_STREAM_S` 75 s, `APPCI_MT_GAP_MS` 100), so every other tab is hidden while it streams (the #805 situation). The typing tab does the same two phases WITHOUT streams (baseline) and WITH them: (a) agent input, prose at 10-25 ms/key with instant Enter, exact text from `hop=enter_begin` (input_type AI), main-thread `hop=ui_item` logic items no more than baseline + 2; (b) terminal view (Escape), key -> first frame after the pty echo (typing into a running `cat`, the pairs of `latency_key_log`) and a 12-command append burst that must arrive exactly (effect file and received lines, `exact_burst`). The latency verdict is the load median against the same run's baseline median: overhead <= `APPCI_MT_BUDGET_MS` (2) or twice the bootstrap SE of the difference, plus p95 within +16 ms. Completion: every stream must be fully written and never dropped (`fake.streams`), exactly one request may carry each tab's stream prompt (a key or Enter misrouted into a streaming tab adds one), and a follow-up typed in each streaming tab makes the app send the tab's conversation back, which must hold the tab's own chunks and END and nothing from another tab or from the typing tab's prose (a verdict `empty`, the app sending no assistant text back, is `noref`, not a pass). #805 hops: frames per second (`warpui::frame_cause` lines) over a 6 s idle window with and without the hidden streams are a metric and a note only; there is no fixed budget to judge them by. Self-test: `agent_multitab_pure_pieces`.
- **`agent_mcp_faults`** (#1834). `appci/fakemcp.py` is a stdlib stdio MCP server (newline-delimited JSON-RPC, one tool `ping`) with modes `ok`, `crash_start` (exits before `initialize`), `crash_call` (exits inside the first `tools/call`), `hang` (never answers `ping`), `garbage` (malformed JSON, a reply with a foreign id, non-objects, then the valid reply), `big` (5 MB text result), `flaky` (answers once as `LAUNCH-1` and exits; a restart answers `LAUNCH-2`). The case writes them into the profile's `~/.warp/.mcp.json` (Zap-native global config: always spawned; `tool_timeout_secs` 20 s ends the hang; `app/src/ai/mcp/file_based_manager.rs`, `tool_call.rs`), relaunches the app once, proves the precondition with a healthy `fxok` control (its tool offered to the model and `PONG:<token>` returned, else SKIP), then per fault: a fresh conversation, the call request typed at 10-25 ms/key (Enter exact), and the oracles from the wire and disk: the tool result the model received (`fake.tool_results`: a clear error, or for `garbage`/`big` the payload or a clear error, never a hang past the timeout), a follow-up prose turn typed afterwards reaching the model (the turn completed and the conversation takes input), the server's state file (it really ran; `flaky` needs >= 2 starts and a later call answered `LAUNCH-n`, n >= 2), no `panicked at` in the app log, and UI responsiveness during the call: no main-thread `hop=ui_item` of `APPCI_MCP_UI_BLOCK_MS` (250) or more, and tab-switch probes (screenshot diff, needs Pillow, bound `APPCI_UI_ACTION_MAX_S`) while the call runs. Approval cards, if the app raises one for the MCP call, are approved with Return, Return like `agent_approval`. A tool the app does not offer after `crash_start` is the expected outcome and is noted; for the mid-call stages a tool that is not offered is noted as "not exercised", not passed. The config is removed and the app relaunched at the end. Self-test: `agent_mcp_pure_pieces`.

## Agent fuzzer and fake model

A fake AI model plus a fuzzer that drives whole agent conversations. Three layers, one scenario schema (`"v": 1`, `crates/fake_llm/src/scenario.rs`); a failing case from any layer is a JSON file any other layer can replay. Nothing here ships in a release build: `fake_llm` is a dev-dependency of `app` only, and the app-side seam is `cfg(any(test, feature = "test-util"))`.

| layer | what it drives | where | state |
|---|---|---|---|
| **L0 wire** | the real genai client (`lib/rust-genai`, the BYOP decoder) against llmposter, a scripted raw server and a byte-level chaos proxy | `crates/fake_llm/tests/genai_decode.rs` | in this build |
| **L1 in-process** | the real controller / action model / queue in `App::test`, fed scripted turns through the stream seam, an actor stepping the user's side, an oracle checking I1-I7 after every step | `app/src/ai/blocklist/agent_fuzz/` | in this build (see below) |
| **L2 real app** | appci + a scripted fake provider, real PTY | `script/realapp/appci` (`fake_model.py`, a stdlib Ollama-protocol server, `APPCI_FAKE_MODEL=1`; first user: the real-use replay, 2.16; llmposter itself is not used at L2) | in the replay |

**llmposter** (crates.io, AGPL-3.0-or-later, test-only; Phosphor is AGPL-3.0-only so the licences are compatible, and `AGPL-3.0-or-later` is on the `deny.toml` / `about.toml` lists for it) supplies the HTTP mock: OpenAI SSE, character-chunked text, HTTP error statuses, frame truncation. `crates/fake_llm` writes only the gaps: the in-process seam, byte-level cuts (inside UTF-8, a JSON string, between `data:` and `\n\n`), tool-call arguments streamed as many deltas, malformed arguments, chosen call ids, and the scenario schema.

### L0 (`genai_decode*`)

| test | what it proves |
|---|---|
| `genai_decode_llmposter_text_reassembles` (proptest) | text of any shape (multi-byte, quotes, newlines) chunked any way reassembles byte-exact |
| `genai_decode_llmposter_tool_call_arguments_parse` | every known tool's valid arguments arrive as the call's name and parsed arguments; the chunk the app accumulates by id carries the raw text |
| `genai_decode_llmposter_turns_follow_request_order` | the nth request gets the nth turn; past the script the fake answers `END` |
| `genai_decode_proxy_cuts_keep_text_exact` (proptest) | the stream cut at arbitrary bytes by `ChaosProxy` still decodes exactly |
| `genai_decode_wire_split_arguments_are_byte_exact` (proptest) | arguments streamed as 1-11 deltas, frames cut at arbitrary bytes, accumulate byte-exact |
| `genai_decode_wire_invalid_json_arguments_arrive_as_the_raw_text` | malformed / empty arguments reach the app as the exact text (the app parses and rejects them) |
| `genai_decode_wire_two_calls_keep_their_ids_and_order` | two calls keep ids and order |
| `genai_decode_disconnect_mid_stream_is_an_error_not_a_short_reply` | a connection dropped mid-stream is an error |
| `genai_decode_clean_close_before_done_is_a_visibly_missing_end` | a clean close before `[DONE]` is a plain end with no End event; the only defence is the app's "ended without a terminal End event" guard (`chat_stream.rs`) |
| `genai_decode_stall_is_catchable_with_a_timeout` | a silent connection can be timed out by the caller (the app has its idle timeouts) |
| `genai_decode_llmposter_truncation_never_looks_complete` | llmposter's frame truncation never yields an End |
| `genai_decode_llmposter_http_statuses_reach_the_caller`, `genai_decode_wire_html_error_page_is_an_error_with_its_status`, `genai_decode_llmposter_context_overflow_keeps_the_providers_wording`, `genai_decode_in_stream_error_event_is_an_error` | 429 / 529 / 503 / 500, an HTML 502, a context-overflow 400 and an in-stream error event reach the caller with the status or the provider's wording the app's classifiers match on |

### The seam and the test-only probes (L1 plumbing)

All `cfg(any(test, feature = "test-util"))`; none of it is in a release build.

| piece | where | what it is |
|---|---|---|
| stream seam | `app/src/ai/agent_providers/fake_stream_seam.rs`, hook `open_sdk_stream` in `chat_stream.rs` | `install(model_id, script)` registers scripted turns per model id; `generate_byop_output` asks the seam just before `exec_chat_stream` and, if a script exists, reads its `ChatStreamEvent`s instead of the network. Records every request (`RecordedRequest`: tool-message ids, tool-call ids, last user text, offered tools) for I3/I4. Open errors, mid-stream `Http` / `Transport` errors, `Stall`, `Wait` and a stream that stops with no `End` are all scriptable |
| `pending_snapshot_for_test` | `BlocklistAIActionModel` | every pending / preprocessing / running action with its stage (`PendingStage`) and kind; **panics on an action kind `fuzz_action_kind` does not classify**, so a new kind fails the fuzz run until it is given a verdict |
| `actionable_card_ids_for_test`, `card_ids_for_test` | `AIBlock`; `TerminalView::actionable_card_ids_for_test` aggregates | the actions a block shows an actionable card for (command / MCP / button cards while the action is blocked; the question card while it takes answers) |
| `complete_command_for_test`, `is_waiting_on_command_for_test` | `ShellCommandExecutor` | `MockTerminalManager` has no pty, so no block ever exists for a requested command; this finishes the waiting action as if the command exited, printing a chosen output |

Seam unit tests: `fake_stream_seam_tests.rs` (unregistered ids fall through to the network, requests are recorded with their tool messages, dropping the handle unregisters, tool-call chunks and the End content match genai's, an open error carries its status, a duplicate id panics).

Request recovery (`agent_providers/stream_recovery.rs`, driven from `generate_byop_output`): before the first streamed byte a 429 / 408 / 5xx / transport failure is retried after 2, 4, 8 s (a `Retry-After` replaces that wait); after the first real event nothing is retried and the error is surfaced; a context-overflow answer (or a prompt over the known window, checked before the send) drops the oldest turns from that one request, whole tool rounds together, keeping system messages and the latest user message on, and retries once. Unit tests: `stream_recovery_tests.rs` (run against the seam, no sleeping: 429 with `Retry-After: 7` waits 7 s then succeeds; 503 x3 waits 2/4/8 s; schedule exhausted gives the note; 401 not retried; a drop before output is retried; a drop after output is not (one request, the error reaches the stream); overflow trims and retries once with the latest user message kept and no orphaned tool result (I4); a second overflow and an overflow with nothing older both surface; proactive trim before the first send; classification table). `SeamError::HttpRetryAfter` scripts a status with a `Retry-After` header. Corpus: `recovery_429_then_success`, `recovery_overflow_nothing_to_trim`, `recovery_midstream_disconnect_no_retry` (written without a build; confirm they pass on the box before they stay, per the corpus rule below). In unit-test builds the retry waits are divided by 1000. An error that used up the 2/4/8 s schedule (or stayed too long after trimming) is wrapped in `PreOutputRecoveryExhausted`, which `AIApiError::is_retryable` treats as terminal, so the outer layer adds no requests (a dead provider makes exactly 4; `a_dead_provider_makes_four_requests_and_the_error_is_not_retried_again`, `exhausted_in_stream_recovery_is_terminal_but_a_mid_stream_drop_still_resumes` in `response_stream_tests.rs`, which also pins that a mid-stream drop is still retryable and resumes once actions have run). The read-ahead's wait for the first event is deducted from the send loop's first-event window.

### The L1 harness (`app/src/ai/blocklist/agent_fuzz/`)

`cfg(test)` only. A case is one `App::test`: a real `TerminalView`, `BlocklistAIController`, action model and queue, the BYOP provider wired (`wire_mock_provider_config_with_model`) to a per-case model id whose replies come from the seam, and the active model set through `LLMPreferences`.

| file | role |
|---|---|
| `player.rs` | scenario -> `SeamTurn` per request (`Match`: nth request / last tool result / user token / default; an unmatched request gets `END`); records the call ids each reply emitted. `Missing` id = the id genai assigns (`call_<index>`), `DuplicateOf` = a repeated id, chaos = open error / mid-stream error / stall / no `End` |
| `rig.rs` | setup, `observe` (conversation status, stream in flight, action snapshot, actionable cards, queue rows), `drive_commands` (finishes `Quick`/`SetE` commands through `complete_command_for_test`; reports a never-ending one to the agent after a short wait, as the executor's stall snapshot does), `settle_until` |
| `actor.rs` | the scripted user, through the UI's entry points (table in the file header): Ctrl-C is the view's real key handling (`press_ctrl_c_for_test`), a follow-up is typed into the input and `maybe_queue_input_for_in_progress_conversation` decides queue-or-submit (`type_and_try_queue_for_test`), Stop is the status bar's controller call. `AnswerQuestion` acts only on a question that is Blocked and has a card up, stages the answers, executes by id and then discards what was not taken (as `ask_user_question_view.rs` does), so it never completes a cardless question and works whether or not `ask_user_question` needs an approval step first |
| `oracle.rs` | I1-I4, I6, I7 (I5 is `prompt_tests.rs`) |
| `runner.rs` | one case end to end; a panic becomes an I6 violation; a failure prints the scenario as schema JSON |
| `log_capture.rs` | ERROR-log tap on the unit-test logger (one tap per process: run under nextest) |
| `smoke_tests.rs` | a plain reply (status `Success`, one request, no tool message), and a command that waits for approval then runs (the step was performed on a `run_shell_command` card, two requests, exactly one tool message, `Success`): the smallest cases, so read their failure first; nothing orders them before the others |

Invariants (checked after every step; delta = 2 s, all waits scale with `AGENT_FUZZ_TIME_SCALE`):

| id | invariant |
|---|---|
| I1 | every blocked action has an actionable card within delta; nothing pending sits neither blocked-with-card, runnable nor running longer than delta (`transfer_shell_command_control_to_user` counts as carded only while the rig has a live never-ending command: its card is the CLI subagent view) |
| I2a | script used up and every card answered -> conversation terminal within 5 s (+4 s per failing turn: retry backoff) |
| I2b | Ctrl-C / Stop -> terminal within 2 s, nothing pending or running, no stream in flight |
| I3 | when terminal, locked queued rows unlock within 2 s; after a success, unlocked rows are sent within 3 s. After a cancel rows are restored to the input, not sent (#690 / #1026, kept by #1799) |
| I4 | each request carries exactly one tool message per emitted call id, in order, none extra, none orphaned; duplicates renamed `id#2` |
| I6 | no panic on any thread (a hook records panics in background futures), no ERROR log beyond the allowance per chaos kind and count: `status` / `context_overflow` / `disconnect` / `stall` 2 each, `never_end` 1, `split` / `delay` 0 (`oracle::error_allowance`). Log lines are not filtered by thread, so ERRORs from spawned futures count (run under nextest: one process per test) |
| I7 | no card takes accept keys for an action the model no longer holds (read from the card's own key routing, `AIBlock::key_routed_card_action_for_test` over `pending_command_card_view_id`, not from the action status) |

I5 (`prompt_tests.rs`): the rig has no pty, so I5 drives the real `PromptDetector::observe` (now, termios shape, screen candidate and the #1796 `foreground_tty_read` probe) at the executor's 1 Hz poll with synthetic observations built from each command behaviour, plus the real hand-over on a `TerminalModel` block (`take_over_control_for_user` / `handoff_control_to_agent`, the calls `CLISubagentController` wraps and logs ERROR on failure).

| test | what it proves |
|---|---|
| `a_prompt_lookalike_is_never_handed_over_while_the_foreground_is_not_reading` | `Silent` / `TrailingColon` / `TrailingQuestion` with probe `NotReading`: never handed over in 30 simulated minutes (#1796) |
| `a_real_prompt_is_handed_over_within_the_window_plus_three_seconds` | `RealPrompt` + `Reading`: not before 15 s, not after 18 s |
| `a_secret_prompt_is_handed_over_at_once` | `read -s` (echo off, canonical) needs no probe |
| `with_no_probe_a_bare_question_gets_a_notice_after_ninety_seconds_and_no_hand_over` | probe `Unknown`: never handed over (owner rule: timers never decide routing); exactly one `MayBeWaiting` notice, not before 90 s |
| `handover_only_for_commands_that_read_the_tty` (proptest over every behaviour) | a hand-over happens only for a command that reads the tty (a secret prompt, echo off, aside) |
| `handing_over_and_back_logs_no_error_and_leaves_the_agent_in_control` | the real block hand-over round trip: no ERROR logged, agent in control afterwards |
| `a_second_hand_back_is_refused_without_a_panic` | a repeated hand-back is an `Err` (the controller logs it at ERROR): the executor must hand back once per take-over |

Determinism: the default run uses a FIXED proptest seed and 12 cases (`AGENT_FUZZ_SEED=<0-255>` picks another, `AGENT_FUZZ_SEED=random` a fresh one), so an unrelated change is never gated by unseeded randomness. Deep runs are `script/agent_fuzz` with `AGENT_FUZZ_SEED=random PROPTEST_CASES=200` and/or `AGENT_FUZZ_MINUTES`. `.config/nextest.toml` gives `test(agent_fuzz)` a 120 s slow period (terminate after 5) because one test is many whole conversations with real waits.

Environment knobs: `AGENT_FUZZ_TIME_SCALE` (default 1.0), `AGENT_FUZZ_HUMAN_SCALE` (default 0.05, scales the actor's 400-2500 ms "human" delays). `stall` chaos needs `PHOSPHOR_BYOP_STREAM_IDLE_TIMEOUT_SECS` and `PHOSPHOR_BYOP_STREAM_FIRST_EVENT_TIMEOUT_SECS` at 3 or less (`script/agent_fuzz` sets them to 1).

### Corpus, known failures, coverage, entry point

* `agent_fuzz/corpus/*.json` is replayed on every run by `corpus_replay_tests.rs`. Each file is a schema-v1 scenario named by its stem. The four Wave 0 scenarios are there as ordinary entries (they pass on this branch, where #1796, #1797 and #1799 are fixed): `wave0_1799_reject_then_transfer` (a held command and a hand-over behind it, a follow-up queued while blocked, then Reject), `wave0_1797_ask_empty_args` (`questions: []`), `wave0_1797_ask_valid` (a real question, answered), `wave0_1796_silent_then_handoff` (a silent, prompt-shaped command; L1 has no pty, so its detector half is `prompt_tests.rs`). Also `dup_call_ids`, `unknown_and_not_offered_tools`, `ctrl_c_during_never_ending_command`.
* Adding a case: copy the JSON a failure prints (or the file `AGENT_FUZZ_SAVE=<dir>` wrote) into `corpus/`. Never put a scenario in the repo that you have not seen pass or listed.
* `known_failures.toml` maps a corpus scenario to the issue that tracks it. A listed scenario must still fail with the same invariant; it fails the run the day it passes (remove the entry) or fails with another invariant (file that one). Empty today.
* `coverage.rs` counts (tool x op x timing) for performed user steps and (chaos x phase) for the model's turns. `WAVE0_FLOOR` lists cells the corpus must reach on its own (reject / approve / answer / follow-up on a card, Ctrl-C), checked in the corpus replay (nextest runs each test in its own process, so the floor cannot live in a separate test). `AGENT_FUZZ_COVERAGE=1` prints the table.
* `script/agent_fuzz` runs L0 + L1 through `script/check_test_failures -p fake_llm -p warp --lib --features warp/gui -E 'test(agent_fuzz) | test(genai_decode)'`, with `PHOSPHOR_BYOP_STREAM_IDLE_TIMEOUT_SECS` / `..._FIRST_EVENT_TIMEOUT_SECS` at 1 so a `stall` turn ends in test time. Knobs: `PROPTEST_CASES` (12), `AGENT_FUZZ_SEED`, `AGENT_FUZZ_MINUTES` (time-boxed fresh rounds), `AGENT_FUZZ_SAVE`, `AGENT_FUZZ_TIME_SCALE`, `AGENT_FUZZ_COVERAGE`. Proptest never writes `proptest-regressions` into the tree (the corpus is the memory), and shrinks to a minimal (script, user) pair printed as schema JSON.
* CI jobs: none yet (manual, on the build box).
* **Cargo.lock:** `crates/fake_llm` adds llmposter (and its axum 0.8 tree). The first box build updates `Cargo.lock`; the coordinator commits it (do not build with `--locked` before that).
* **cargo-about:** `crates/fake_llm` is a workspace member, so `cargo about` lists llmposter in the third-party notices even though it is dev-only and never in a release binary. Over-attribution; acceptable.

#### Acceptance on the pre-fix base (to run on the box, not done)

On `ffdaa4536` (before #1796 / #1797 / #1799) the seeded `wave0_1799_reject_then_transfer`, `wave0_1797_ask_empty_args` and `wave0_1797_ask_valid` must FAIL with I1 / I2a, and unseeded `AGENT_FUZZ_MINUTES=10` must find each class. Method: branch from `ffdaa4536`, cherry-pick this branch's commits (the seam and probes touch `chat_stream.rs`, `action_model.rs`, `block.rs`, `terminal/view.rs`, `shell_command.rs`, `mock_provider`; expect trivial conflicts only in `action_model.rs`), drop `mod prompt_tests;` from `agent_fuzz/mod.rs` (it needs the #1796 `observe` probe argument and does not compile on the base, which is itself the signal for #1796), run `script/agent_fuzz`. On this branch all of them pass.

Question scenarios tolerate either `ask_user_question` flow: the card appearing without an approval step (an owner decision in flight in another change), or after one. The actor answers whatever question is at the front by staging the answers and executing by id; `OnCard` triggers on any actionable card.

#### Approvals suite and the safe read-only allowlist (Wave 1.5 #2)

Nothing here has been compiled or run yet (written without cargo; the box run is the first).

**The matrix** (`agent_fuzz/matrix.rs`): a deterministic, exhaustive sweep next to the proptest. Every cell is a scenario built in code, run through the same rig and oracle (I1-I7 after every step), once with the safe allowlist OFF (`off`, the gate) and once ON (`on`, a separate test so an ON failure never hides an OFF result). One module per row, so nextest runs rows in parallel processes: `agent_fuzz::matrix::<row>::{off,on}`.

| sweep | rows | cells per mode |
|---|---|---|
| `kinds_*`: every action kind (command, listed command, failing, slow, never-ending, read_files, grep, file_glob, question, transfer, write-to-command, read-command-output, websearch, webfetch) x every response at the card (approve, reject, edit, Ctrl-C, stop, close the tab, switch mode, escape, follow-up, answer, skip, never) | 14 | 168 |
| `timings_*`: command / never-ending command / question x approve, reject, Ctrl-C, stop, close the tab, follow-up x early (during the stream), fast (10-25 ms), late, after the end, twice, burst of three | 3 | 108 |
| `pairs_*`: every ordered PAIR of six kinds in one turn x seven user policies (approve all, reject first, reject last, Ctrl-C / stop / close the tab / follow-up after the first) | 6 | 252 |
| `triples_*`: a seeded sample (12 per first kind, xorshift, no dependency) of ordered triples x three policies | 4 | 144 |
| `malformed_*`: every tool x every malformed-argument shape (empty, `{}`, truncated, wrong types, extra fields; four more for `ask_user_question`) x approve / reject / Ctrl-C | 10 | 162 |
| `command_behaviours`: quick, failing, slow, never ends, silent, pager, alt screen, secret prompt x approve / reject / Ctrl-C / stop / follow-up | 1 | 40 |

874 cells per mode, 1,748 with both. `the_matrix_sizes_are_what_the_docs_say` pins the counts and `the_matrix_names_every_tool_and_every_response` fails when a tool is added to `KNOWN_TOOLS` without a matrix kind.

Also: `paced_random_sequences_hold_the_invariants` (+ `_with_the_allowlist_on`): random conversations whose user steps are all `Timing::Fast` (10-25 ms apart), 3-9 steps; `random_agent_conversations_hold_the_invariants_with_the_allowlist_on`; `corpus_replays_with_the_allowlist_on`. 29 seeded corpus scenarios (`corpus/appr_*.json`, 36 files in all) cover each combination class: orders (6), timings (6), slow / failing commands (4), malformed arguments (6), kinds x responses spot checks (5), the allowlist (2). New schema piece: `CmdBehaviour::Slow { ms, out }` (finishes by itself after `ms`; `env.rs` `CompletesAfter`).

**The allowlist** (`ai/blocklist/safe_commands.rs`, ships OFF): `AISettings::safe_command_allowlist_enabled` (default `false`, `agents.warp_agent.permissions.safe_command_allowlist_enabled`) and `safe_command_allowlist` (entries `program` or `program subcommand`; default `ls cat head tail wc grep rg pwd whoami id uname df du stat file which basename dirname realpath tree uptime free`, and NO git subcommand: git runs helpers a repository's config names, so a user must add `git status` etc. deliberately). `BlocklistAIPermissions::can_autoexecute_command` consults it AFTER the denylist and the command-word analysis (a denylisted or unresolved line never reaches it) and before the profile policy; reason `SafeReadOnlyAllowlisted`. The matcher accepts one program with plain words only: it rejects any shell metacharacter (pipe, ampersand, semicolon, redirects, parentheses, braces, backtick, dollar, backslash, glob characters, quotes, `#`, `!`, `~`, `=`, `%`, `^`, `@`, `,`, `:`), newline or non-ASCII, a program that is not spelled exactly (no `/`), flags that write / run / never end (`sort -o`, `tail -f`, `rg --pre`, `date -s`, `git diff --output`), and never vouches for `find env xargs sudo sh sed tee cp rm curl less vim ...` even if listed. Residual risk for a git entry a user adds: git runs helpers a repository's config names (`core.fsmonitor`, `diff.external`, textconv); that is why the defaults hold none. Questions (`ask_user_question`) already need no approval and are unchanged. UI: Settings > AI > Permissions, under the command allowlist (switch plus an editable list, entries validated by `is_valid_entry`).

* `safe_commands_tests.rs` (pure matcher), `permissions_test.rs::safe_command_allowlist` (off by default and `ls` asks; on: listed lines allowed, pipes / redirects / `;` / `&&` / substitution / globs / `rm` / `git push` / `find` / `tail -f` asked; denylist still wins), `settings/ai_tests.rs` (ships off).
* `allowlist_tests.rs`: `allowlist_removes_the_approval_for_a_listed_line_and_only_that` (ON: `echo hi` runs with no card; `fake-cmd` lines still ask), and the differential `allowlist_on_and_off_reach_the_same_terminal_state` (proptest; a user who only approves / answers ends in the same status, request count and tool messages with it ON or OFF; the only permitted difference is the skipped approvals).
* The rig has no pty and so no shell type; `shell_command.rs::escape_char_without_a_session` (test / `test-util` builds only) assumes POSIX when the allowlist is ON, so the ON runs exercise the real decision. A release build and every OFF run keep "no session -> ask".
* Agent-mode vs terminal-mode coverage of the approval flows comes from the real-app replay case (`lab_replay`) and the agent appci cases, not from the in-process rig (which has no mode concept).
* **Owner rule:** the allowlist ships OFF. It is enabled by the owner only after this suite is green with it OFF (the `off` rows, corpus replay, both proptests). Do not flip the default in code.

#### `hop=` diagnostic lines (always on, owner-approved)

Info-level, written only when a state CHANGES, ids / kinds / states only, never content (no command text, prompt text, arguments, error text or the agent's hand-over reason). The later L2 (appci) reads them from the app log to see which hop of a conversation stalled.

| line | written by | states |
|---|---|---|
| `hop=agent_action id= kind= status=` | `BlocklistAIActionModel::hop` | `queued`, `blocked` (front of the queue, needs the user), `running`, `done`, `failed`, `cancelled` |
| `hop=action_card id= kind= shown=true` | `AIBlock` (`log_card_shown`) | a card came up: `run_shell_command`, `call_mcp_tool`, `action_buttons`, `ask_user_question` |
| `hop=handover action= reason=` | `CLISubagentController::switch_control_to_user` | `manual`, `stop`, `transfer_from_agent`, `blocked_on_input` (`action=-` when the block has no requested command) |
| `hop=queued_row id= state=` | `QueuedQueryModel` | `queued_locked`, `queued`, `unlocked`, `popped` (restored to the input), `fired`, `deleted` |
| `hop=turn conv= state=` | `AIConversation::update_status_with_error_message` | `in_progress`, `success`, `error`, `transient_error`, `cancelled`, `blocked`, `waiting_for_events` |

Run: `script/agent_fuzz` (L0 + L1). Nothing in this section has been compiled yet.

## #1801: remote Tab and path detection without the helper

A Phosphorized ssh session where the remote helper is absent, declined, failed or lost is the owner's PRIMARY remote
design. Its side commands run over the ControlMaster (`RemoteCommandExecutor`), which a VPN makes slow or breaks.

What changed and what pins it:

- **Tab asks the remote shell in-band.** An explicit Tab in such a session (`Session::is_remote_without_helper`)
  sends the `^Y` native-completions handshake to the session's own shell first and uses the completer only when the
  shell offers nothing (`app/src/terminal/input/native_completions.rs`, decision + span unit tests in
  `native_completions_tests.rs`; terminal mode and a shell command in agent mode take the same path, agent text never
  does; completions-while-typing never does). bash 4.0+ gained the handshake (programmable completion with COMP_*
  emulated, bash-completion's `-D` loader, `-C` commands, compgen actions, `-o` options); bash 3.2 gets no `^Y` binding
  and the app sends it none (`native_shell_completions_need_bash_4`). fish is off the path and its bootstrap has no
  handler: its read would need external commands, around which fish switches bracketed paste and keypad modes.
  **The exchange is self-identifying, and nothing is ever held behind it** (design comment in `pty_controller.rs`,
  "The in-band native-completions exchange"; third #1801 review). Wire: the app writes `^Y` only to a shell whose
  Bootstrapped hook advertised `!phosphor_nc_frames_v1` in its shell options (bash 4.0+, zsh); the hook names the
  exchange with a fresh token (OSC `9280;P;<token>`; a reused or missing token is cancelled); the app always ends it
  with ONE frame, `RS <token> : <%HH text> US` (RS = 0x1e, US = 0x1f), or the cancel frame `RS - : US`. The hook and
  the RS widget at the prompt consume nothing outside a frame: the first byte must be RS, then the frame's exact shape
  (`(<digits>x<digits>x<digits> | -) : (%HH)* US`), judged by CONTENT only with no inter-byte timing (a frame may
  arrive in pieces with any pauses; the only clock is the overall read deadline `PHOSPHOR_NC_DEADLINE`, a give-up net
  that hands back what was read and writes nothing); on the first byte that cannot continue a frame everything read goes back to the line editor, an Enter
  included (bash: ^Y and RS are macros that run the function and then a key it rebinds to accept-line), and a
  terminal reply (a DA report) is dropped whole. US is not bound (it stays the line editor's undo). App: an exchange
  starts only when the write queue is empty, no command is executing, the line editor is active and no other write
  path is engaged (including a `SetupChunked` job the writer still holds: `SetupLive` counts it until the writer
  drops it); while it waits
  for its prompt, ANY write (a command, typed or pasted bytes, agent bytes, ^C) first writes the cancel frame, then
  goes out, in order. The 2 s prompt timeout is a give-up that WRITES NOTHING: it closes the requester's channel (the
  completer answers) and marks the cancel frame owed; the owed cancel goes in front of the next real write, or when the
  shell's own late prompt arrives, exactly once (`the_cancel_owed_after_a_timeout_precedes_the_next_write_exactly_once`,
  `a_late_prompt_after_the_timeout_gets_the_cancel_once`). Nothing is queued behind `^Y`, so there is nothing to hold, hand back or reorder; the shell
  leaving first just ends the exchange. Unit tests (`pty_controller_tests.rs`, through the real write APIs):
  `native_completions_frames_are_self_identifying_and_never_end_a_line`,
  `an_exchange_starts_only_when_nothing_is_queued_or_running`,
  `writes_during_an_exchange_go_out_in_order_after_the_cancel_frame` (an editor command and agent bytes; the queue
  stays empty), `a_direct_byte_write_during_an_exchange_is_preceded_by_the_cancel_frame` (bytes, paste, ^C),
  `the_prompt_is_answered_with_one_frame_and_later_writes_follow_it`, `a_reused_or_missing_prompt_token_is_cancelled`,
  `prompt_timeout_cancels_without_a_toast_for_a_remote_first_request` (now: the timeout writes nothing),
  `prompt_timeout_of_a_fallback_request_asks_for_the_toast`,
  `keys_after_the_prompt_timeout_and_a_late_prompt_lose_nothing` (#1674/H14: `^Y`, timeout cancel, keys, command, then a
  late prompt: the tty order is kept and the late prompt writes nothing; same for Legacy and Table, the pty layer is
  shared), `a_second_request_while_one_is_in_flight_is_refused`,
  `the_shell_leaving_ends_the_exchange_without_a_frame`,
  `an_exchange_does_not_start_while_the_writer_holds_a_setup_job`,
  `a_setup_write_during_an_exchange_is_preceded_by_the_cancel_frame`; terminal-query replies and a resize do not pass
  through the cancel-first rule (why each is safe: the design comment); `native_shell_completions_need_the_advertised_protocol_and_bash_4`
  (`warp_terminal`); `completions_prompt_token_is_letters_and_digits_only` (`ansi/mod_test.rs`). Tab then a fast
  Enter runs the COMPLETED line remotely too: the editor holds the Enter for the in-band answer with NO time budget
  (`REMOTE_NATIVE_TAB_ENTER_BUDGET` is gone; owner decision: no timer decides what Enter runs). The answer is an
  event: candidates run the completed line however late; an answer of nothing (the shell found none, or the app
  cancelled the exchange: the cancel sends an empty result) runs the typed line and the completer answers the menu;
  The remote as-you-type completer listing has no debounce timer any more (`REMOTE_AS_YOU_TYPE_DEBOUNCE` is gone; it
  only ever delayed the ssh side-command listing, never the `^Y`): it is single flight, latest wins (`RemoteListingFlight`:
  a keystroke while a listing runs holds only the newest request, replayed when the listing ends if still current; an
  explicit Tab supersedes without a replay; `test_remote_as_you_type_listing_is_single_flight_latest_wins`, pure and
  mode-independent). A hung listing is freed by EVENTS, no timer: the next Precmd, ExitShell/Exit, `SSHControlMasterError`, `PreInteractiveSSHSession` or a cwd change aborts it and drops the held request (`end_remote_listing_on_event`; `test_remote_listing_event_frees_a_hung_flight`); each flight step logs `hop=listing_inflight event=<start|held|replay|drop|abort> gen=<n>` through the input-order log (`log_hop`; generation only, never buffer text; format pinned by `test_listing_inflight_hop_detail_format`; the lab case `nohelper_listing_single_flight` reads it); the cancel closure and the result callback both go through `remote_listing_finished`, so the replay does not depend on which runs first.
  The shell's Precmd/ExitShell/Exit ends an exchange awaiting its RESULTS too (an event beats the 15 s net, which only
  drops state and writes/releases nothing; `the_shell_leaving_during_the_results_wait_ends_the_exchange_by_event`,
  `the_results_net_drops_the_exchange_and_writes_nothing`);
  a second Tab for the same text while an exchange is in flight attaches to it (Tab, Tab, Enter runs the completed line;
  a cancelled first exchange runs the typed line; `a_second_tab_for_the_same_text_gets_the_first_exchanges_answer`,
  `an_attached_tab_follows_the_cancel_and_the_give_up_of_the_exchange`, pty layer shared by Legacy and Table); a different
  text is refused;
  NO answer at all (the pty's 2 s give-up closes the channel, or the shell left) hands the line back and runs nothing
  (`give_up_unanswered_remote_hold`, #1577/#1666), keys typed meanwhile held in order. Only the 9 s hold backstop
  remains as a lost-signal net with the same hand-back. Tab-path timers left: pty 2 s prompt give-up (writes nothing),
  hook read deadline 20 s (writes nothing), results timeout 15 s (writes nothing), 9 s hold backstop (hands back),
  and the local Tab's 150 ms cap / 500 ms backstop (hand back, never run) (`input_test.rs`, Legacy and Table):
  `test_tab_then_enter_remote_in_band_answer_runs_the_completed_line_in_both_modes`,
  `test_tab_then_enter_remote_in_band_give_up_runs_nothing_in_both_modes`,
  `test_tab_then_enter_remote_in_band_keys_typed_during_hold_follow_the_completed_line`,
  `test_tab_then_enter_remote_native_miss_submits_the_visible_line_at_once_in_both_modes`,
  `test_remote_native_miss_without_enter_makes_the_next_enter_not_wait_in_both_modes`. Known limits: a quoted word
  with spaces is not one token to the app; a backslash-escaped word is not dequoted before bash's compgen; a byte
  outside a frame that is not text, ^P or a bracketed paste cannot be handed back by bash (only a writer outside the
  app could put one there: the app sends the cancel frame before any write).
  Every shell writes candidates as the whole whitespace-delimited token (zsh now adds its
  IPREFIX and -P/-p prefixes, so `cd dir/fo` no longer offers `foo/`). The OSC parser rejoins a payload split at `;`
  (`completions_osc_payload_rejoins_a_payload_split_at_semicolons`). Shell side: `script/test_native_completions` (also: a frame split with 0.5 s and 3 s pauses is consumed whole with
  no junk, RS + `ls` + Enter and RS + letters + Ctrl-C hand back by content, a stale-token frame is consumed; ESC after RS is classified by the NEXT byte, not by time: DA / OSC / DCS XTVERSION replies are dropped whole, an arrow, Alt-x and a lone ESC + Enter are handed back).
  Kill switch: `PHOSPHOR_REMOTE_NATIVE_COMPLETIONS=0`.
- **Chips re-run when the session registers.** A remote session's blocks carry its id before its bootstrap completes
  (the ssh setup holds the bootstrap while it checks for the helper), so chips refreshed then failed with "could not
  prepare execution context" and the refresh after the bootstrap, with an unchanged block and cwd, was skipped by the
  #1167 dedupe. The dedupe key now includes whether the session is registered
  (`test_chips_rerun_when_the_blocks_session_registers` in `current_prompt_test.rs`).

App case `remote_tab_no_helper` (`appci/cases_caseremote.py`, full tier, both modes; Git Bash pane on Windows): the
harness default `ssh_extension_install_mode = never_install` IS the helper-missing setup. Creates `~/phxtab_<tok>/` on
the remote side only (and `phxtab_local_only/` locally), `ssh fake`, types `cd phxtab_<counter>_<1 char>` (`tok_prefix`: unique per trial; the old 3-char prefix `t12` was shared by t126-t128 and made Tab ambiguous, r18w6), Tab, waits 1.5 s
(3 s with a delay applied), Enter. Pass: the REMOTE rec.log's command is `cd phxtab_<tok>/`, the next remote `pwd` ends in
`/phxtab_<tok>`, nothing reached the local shell, and the app log has `candidates from the remote shell in N ms`
(metric `remote_tab_in_band_ms`). `remote_tab_enter_no_helper` (same setup, ctx.count(3) trials): Tab, Enter 30 ms later;
the remote rec.log must hold the COMPLETED `cd phxte_<tok>/`, never the typed prefix. Both skip on macOS, whose test
host's remote shell is /bin/bash 3.2 (no in-band Tab there, by design). No fish remote case: fish is off the path.
Both read the remote rec.log through `remote_cmds_after`, which drops the harness's own `~/.appci_where` probes: the precondition probe's
line can reach rec.log AFTER the mark is taken (round 7: both cases failed with only the probe 'received' while the completed `cd` ran).

Lab spec, netem 150 ms (manual; needs root, the harness never applies it): on the Linux box,
`sudo tc qdisc add dev lo root netem delay 75ms` (150 ms round trip on loopback, where the fake sshd listens), run
`remote_tab_no_helper` with `PHOSPHOR_APPCI_NETEM_NOTE="lo netem 75ms"` (recorded as a note, longer settle), then
`sudo tc qdisc del dev lo root`. Pass as above, with `remote_tab_in_band_ms` below ~2 round trips plus the shell's own
completion time; typing during the Tab must not lag (the editor never waits on the handshake). The helper-missing
"install refused" variant is the same case with `ssh_extension_install_mode = always_ask` and "Continue without
installing" chosen.

**Path detection without the helper (a pushed cwd listing).** On a remote session path detection reads directory listings
from a cache only (#1295, never a command per keystroke), so without a recent Tab a typed directory (`cd somedir`) was not
recognised. The remote bootstrap now sends the listing itself: `__phosphor_cwd_listing` (`bash_body.sh`, `zsh_body.sh`,
`fish.sh`) appends `"cwd_listing"` to the Precmd JSON when the cwd or its contents changed since the last one it sent, and
only when `WARP_IS_LOCAL_SHELL_SESSION` is not `1` (a local listing is already fast; kill switch `PHOSPHOR_CWD_LISTING=0`).
Entries only, with a dir flag (`d` follows symlinks), hidden files included, names `%HH`-encoded outside
`[A-Za-z0-9._+@,=~-]` so no filename can break the JSON or the DCS. Bounds: `P` (cut) at 500 entries or 32 KB, `T` (too
large, no entries, and the directory is not globbed again until a `cd`) above 2000. No fork anywhere: the glob, the
`-d` test and the encoding are builtins (fish: `string escape --style=url`). Freshness is the listing itself: it is rebuilt
at each prompt of a listable directory and sent only when it differs from the last one sent, so a file a command creates is
in the next prompt's payload; a file created outside the session shows at the next prompt. The app parses it
(`app/src/completer/pushed_listing.rs`, unit tests `pushed_listing_tests.rs`) into a per-session slot keyed to the pwd it
came with; `SessionContext::list_directory_entries` answers from it before the 2 s cache, the failed-listing memory or any
remote command: a complete listing answers every consumer, a truncated one only the per-keystroke ones (highlighting,
suggestions), never an explicit Tab (`test_pushed_cwd_listing_answers_background_contexts_without_a_remote_command`,
`test_truncated_pushed_listing_is_not_used_for_explicit_listing` in `completer/test.rs`). The app logs
`hop=cwd_listing session=<id> kind=<L|P|T> entries=<n>` per listing and `hop=path_detect source=pushed session=<id>
entries=<n> complete=<bool>` (at most every 250 ms): ids and counts only. Shell side: `script/test_cwd_listing`
(timings printed). App side: `lab_path_detect` (above). Measured on the dev box, per call of `__phosphor_cwd_listing`
with a rebuild forced (steady state is the same glob with a string compare), 10 / 500 / 2000 / 10000 entries: bash 0.3 / 4.9 / 6.9 / 7.4 ms, zsh 0.1 / 3.2 / 10 / 31 ms, fish 1.3 / 21 / 25 / 17 ms (fish's own prompt is ~46 ms either way). The 10000-entry directory is globbed once per `cd` (`T`), not per prompt: a `true` there costs the same as in an empty directory (bash 14 vs 12 ms, zsh 4.0 vs 4.1, fish 46 vs 47).



## Remote helper client switch (Wave H step 5, #1804)

What changed for tests and CI when the GitLab download of the full app binary was removed and the bundled `phosphor-remote` helper became the ssh remote server. Nothing the app installs is fetched: the helper is gzip-compressed inside the app bundle (`<bundled_resources>/remote/phosphor-remote-<os>-<arch>.gz`), uploaded IN-BAND over the existing ControlMaster (`ssh::run_ssh_script_with_payload`: `bash -s` reads the install script, which ends in one call to `main`, so `gunzip -c` inside it receives the gzip bytes that follow the script on the same stdin; scp is the fallback), verified against a SHA-256 compiled into the client, started once (`--version`) before anything older is removed, and linked as `current`.

- `crates/remote_server/src/setup_tests.rs`: the install script is driven with real bash and a gzip payload on stdin. `in_band_upload_installs_verifies_and_links_the_helper` (mode 0755, `current` link, a created dir is 0700, no `.up*` leftovers), `staged_upload_installs_and_removes_the_staged_file` (the scp-fallback shape), `a_helper_failing_the_pinned_digest_is_rejected_with_the_integrity_code` (exit 6, nothing installed), `no_pinned_digest_is_refused_before_anything_is_read` (exit 4), `unusable_digest_tool_reports_no_digest_tool_never_a_mismatch` (exit 5, three broken tools shadowed on PATH), `an_empty_or_broken_stream_is_an_upload_failure_not_a_verdict` (exit 7, and a failed run removes a directory it created), `a_helper_that_does_not_start_is_removed_and_old_versions_are_kept` (exit 9), `install_leaves_exactly_one_version_and_removes_the_legacy_app_binary`, the exit-code reserved-status and uniqueness checks, the tilde-expansion test (the probe now closes `main`), `nothing_in_the_install_path_can_reach_the_network`, `digest_decision_fails_closed_unless_a_source_build_computed_its_own`, `install_script_renders_every_placeholder`, `the_script_ends_in_a_single_call_so_the_payload_stays_unread`. Removed with the download (owner decision, #1804): the GitLab URL test, the curl-shim download tests and the tarball install tests. Changed: `binary_name()` is `phosphor-remote`, the staged upload is `.upload.gz`, the digest-mismatch headline says the install was not retried.
- `app/src/remote_server/ssh_transport.rs` tests: `classify_install_failure` now has exit 7 as the only script-level transport failure (plus 255 and `Other`), exit 3 (no gunzip/gzip) is fatal, `exit_codes_in_install_script_are_all_classified` follows the script's `EXIT_*` table, `remote_proxy_command_quotes_identity_key` expects `<versioned helper path> proxy <key>`, and `gzip_round_trip_hashes_the_decompressed_bytes`, `a_non_gzip_bundle_is_an_error_not_a_hash`, `unavailable_helper_message_names_the_platform_and_the_fallback` cover the local preparation.
- `crates/remote_server/src/policy_tests.rs`: per-host consent. Default `ask` follows the app-wide `ssh_extension_install_mode`; a host's own `allow`/`never` beats it; only `never` stops an existing helper being updated; `never` wins if a host is in both settings lists (`warpify.ssh.remote_helper_allowed_hosts`, `warpify.ssh.remote_helper_denied_hosts`, keyed `user@host`). The choice block's *Don't ask me again for this host* (en; ja and zh-CN translated) now stores that for the host instead of changing the app-wide mode.
- Capabilities: `capability.rs` tests, and the client gates the automatic repo-metadata requests (`NavigatedToDirectory`, `LoadRepoMetadataDirectory`) on `RemoteServerClient::capabilities()`; every other request outside the helper's list gets `ErrorCode::UNSUPPORTED` from the helper.
- Lab arms (`script/realapp/appci/cases_lab.py`, `lab.py`): new helper modes `upload`, `upgrade`, `never` after `nohelper`/`helper`, and case `lab_helper_install` (terminal mode, bash, LAN and 150 ms, plus scenario user `p_noscp` whose ForceCommand refuses scp and sftp but whose `~/.phosphor` is writable). `upload`: nothing on the server, `always_install`, the lab blocks outbound, so exactly one `phosphor-remote-<version>` appearing proves the in-band upload; it must run, `current` must point at it, and on `p_noscp` that proves no scp is needed. `upgrade`: `phosphor-remote-v0.0.1`, `-v0.0.2`, `phosphor-oss` and `phosphor-oss-v0.0.3` are pre-seeded and exactly the new version must remain. `never`: `warpify.ssh.remote_helper_denied_hosts` holds `u_bash@lab-u_bash`, nothing named `phosphor-*` may appear, the app log says `remote helper install skipped`, and the shell still works. Self-test: `lab_helper_arm_pieces` (file-name derivation from `--version`, the policy key, the arm order). NOT run: these arms need the box lab and a built helper; the first run may need the label format fixed if `connection_label_for_session_info` renders differently.
- CI (`.gitlab-ci.yml`, written, not run): `build-remote-helper` (stage cli; `cargo zigbuild` for x86_64 and aarch64 musl with the `release-remote-helper` profile, a 20 MB size gate per binary, `gzip -9`, a `.sha256` of each decompressed binary), `guards-helper-deps` (runs `script/check_remote_helper_deps`: `cargo tree -p remote_server --no-default-features --features daemon -e normal` must contain none of warpui, warp_core, warp_util, repo_metadata, ai, tokio, reqwest, rustls, openssl, hyper, git2), and `.pin_helper_digests` replacing `.pin_cli_digests` in the three GUI release jobs (`PHOSPHOR_REMOTE_SHA256_*`, `PHOSPHOR_REMOTE_HELPER_DIR`). `script/prepare_bundled_resources` copies the `.gz` files into `remote/` and re-checks each against its pinned digest, so a client can never ship next to a helper it would refuse. The Windows installer does not bundle helpers yet (documented in the job).

Missing helper falls back quietly (#1804): when the app has no helper to upload (no bundled asset for the host's platform, or an unreadable one) the install error starts with `setup::HELPER_UNAVAILABLE_PREFIX`; `RemoteServerManager::install_binary` then emits `Unsupported { HelperUnavailable }` instead of `Failed`, the controller logs at info (not error) and releases the bootstrap, and `unsupported_shows_banner` keeps the terminal view from drawing any banner. A digest mismatch, a damaged-checksum bundle or a failed script stay `Failed` with their banner. Tests: `setup_tests::only_helper_unavailable_messages_take_the_quiet_fallback`, `ssh_transport` `unavailable_helper_message_names_the_platform_and_the_fallback`, `ssh_remote_server_failed_banner_tests::a_missing_helper_gets_no_banner_but_incompatible_hosts_still_do`.

## #1772: the startup writable probe is bounded by the setup deadline

`probe_writable` shares the setup's `LockDeadline` and waits on a held write lock for at most one busy slice (a busy
result already proves the connection is not read-only), then restores the normal 1 s busy timeout (a failed restore is an
error, not a warning). Tests (`persistence/sqlite_tests.rs`):
`writable_probe_is_bounded_by_the_setup_deadline_and_restores_the_timeout` (a held `BEGIN IMMEDIATE` and a 1 s writer
timeout: the probe returns in under 500 ms and `busy_timeout` reads 1000 afterwards), plus the existing
`writable_probe_tolerates_a_held_write_lock`, `read_only_connection_fails_the_writable_probe` and
`deliberately_read_only_file_is_not_failed_or_retried`.

## #1129: the typed-input wrapper is folded (no 190 KB readline line)

A bash WITHOUT bracketed paste (macOS 3.2 over ssh) gets a large command as the base64 wrapper, typed key by key. One
190 KB line took 86 s (~2 KB/s): readline redisplays the whole line per key, so the cost is the square of the line's
length. `posix_long_command::wrapper_lines_for_typed_input` folds the base64 onto lines of at most 960 characters inside
the same open single quote (continuation lines; nothing runs until the closing quote; `base64 -d`/`-D` ignore the
newlines), under macOS's 1024-byte `MAX_CANON` too. With bracketed paste the one-line `wrapper_line` is kept (measured
fast, #1255). Unit tests (`writeable_pty/posix_long_command_tests.rs`):
`the_typed_input_wrapper_folds_the_base64_and_carries_the_command_byte_for_byte`,
`the_typed_input_wrapper_has_the_same_decode_and_cleanup_shell_as_the_one_line_form`,
`the_typed_input_wrapper_redisplay_work_scales_linearly` (sum of squared line lengths: 100 KB to 1 MB grows ~10x folded,
~100x as one line). The real-app proof is the existing `remote_heredoc_paste` (byte-exact 2,000-line here-doc, 10 KB
line, ~600 KB here-doc over the `fake` ssh host); on a bash 3.2 remote its 2k here-doc step's time is the metric to
compare with the 86 s of r18u22. Unverified until the box runs it: the block header's display override is keyed by the
exact wrapper text the shell's preexec reports, which now contains newlines (bash keeps them inside an open quote).

## #1191: large-paste scaling tests (100 KB vs 1 MB)

The causes found by reading the path were already fixed on this base, each by a commit named in the issue's notes: the 1.3-1.8 s
Enter-to-shell of a 1 MB SINGLE line was the base64 wrapper (db7c0fb4a: a single line is never wrapped under bracketed paste); the
multi-second UI freeze was per-row layout of the whole buffer (e7fd2738e, be8886829: only the viewport, the ends and the cursor
rows are laid out, with a self-check and the `PHOSPHOR_EDITOR_FULL_LAYOUT=1` switch) plus per-edit copies and parses (e23bc8304,
e462b84f4, #1604). What was missing was a test that would catch a quadratic step coming back on the multi-line and Enter side:
`test_paste_of_a_large_multi_line_buffer_and_the_keystroke_after_it_are_linear_in_its_size` (`terminal/input_test.rs`, the
single-line twin's body with ~13,000 lines; paste and the keystroke after it at 1 MB cost at most 40x their 100 KB counterparts,
a quadratic step would be ~100x) and `a_huge_single_line_command_is_written_whole_and_in_linear_time`
(`writeable_pty/pty_controller_command_bytes_tests.rs`: 100 KB, 1 MB and 3 MB single lines are not wrapped, the written bytes are the
text exactly inside one bracketed paste, and the 1 MB write is within 40x the 100 KB one). The appci proof stays `big_paste`
(byte-exact 1 MB / 3 MB, Enter-to-shell and key-to-paint metrics against the 2 ms / 4 ms budgets); a run on the box is what
confirms the freeze is gone, since these unit tests cannot render.

#1777 (ConPTY Ctrl-C and queued lines): a Ctrl-C does not flush the lines written behind a running command under ConPTY with an msys program, so on a ConPTY session the hand-off keeps its Enter count across the interrupt (`interrupt_enters_written`) and the pty owns input until a prompt has answered each queued line (the stale-count reset covers lines that were in fact discarded); POSIX keeps the flush rule. Unit tests in `terminal/model/typeahead_handoff_tests.rs`: `conpty_ctrl_c_keeps_the_queued_lines_counted_until_their_prompts_arrive`, `the_interrupt_enter_count_is_kept_only_on_conpty` (pure function; the existing POSIX cases `ctrl_c_during_a_command_hands_back_at_its_prompt` and `ctrl_c_at_the_prompt_discards_the_line` are the controls). No appci case yet: the Windows `big_paste` / `tab_enter` cases already relaunch the app around a paste backlog (r18u23), so they do not exercise this path; a case that types a line behind `cat`, sends Ctrl-C, and checks each line ran once and in order needs a Windows run.

## Round g2: input and editor fixes (#1635, #1643, #1645, #1659, #1703)

- #1635: `editor/view/mod_test.rs` `test_queue_editor_host_gates_every_map_reading_key_not_only_layout_ones` (the editor's
  own queue host opens the display-map gate for a cursor key and a typed closer, as the input's host does; it used to
  gate only layout-reading keys and applied the rest against the pre-paste map).
- #1643: no unit test (GlobalSearchView has no view-test harness; building one needs the full workspace singletons).
  Covered by the existing macOS appci cases `flood_scrollback_find` and `remote_heredoc_paste`, which press Cmd-Shift-F,
  Escape, then type a command: before the fix the command was searched, now Escape emits `Event::Close`, the left panel
  closes and the active tab takes focus. Check on the Mac box in the next candidate run.
- #1645: `search/ai_context_menu/query_gate_tests.rs` `an_enter_typed_after_a_pending_edit_survives_that_edits_query`,
  `a_different_query_still_cancels_an_enter_waiting_for_a_pending_edit`, `no_edit_is_pending_for_the_newest_query_or_a_fresh_gate`.
  The Enter carries the filter the buffer holds (`Input::accept_ai_context_menu_item`); an edit event handled after it
  is that Enter's own query and keeps it. Both input modes share this path (the picker is above the queue).
- #1659: `terminal/model/typeahead_handoff_tests.rs` `a_report_is_pending_before_editor_text_until_it_lands`. A Tab with
  the shell's line report pending is held (`Input::defer_tab_for_report`, hop `tab_deferred_for_report`) and runs on the
  merged line when the report lands (hop `tab_resumed_after_report`); a deferred Enter behind the report waits for that
  completion (`Event::ReportTabSettled`, hop `enter_waits_for_report_tab`, give-up net `REPORT_TAB_GIVE_UP` that hands the
  line back and never writes). Run `script/test_native_completions` after this change (the remote Tab goes through
  `open_completion_suggestions`, which now returns early for a pending report only). The Windows `tab_enter` appci cascade
  (r18u22) is the real-app check: Tab on the fragment while the report is in flight must complete the whole line and
  Enter must run the completed line. No view-level unit test: the report/Tab interleave needs the pty-backed
  `TerminalView` fixtures that only run on the box.
- #1703: `terminal/view_test.rs` `typeahead_handoff_ctrl_c_give_up_does_not_commit` (both the zsh barrier and the
  prompt-cycle wait, in Legacy and Table). The 2 s `INTERRUPT_GIVE_UP` now ONLY clears the wait for that one signal (owner
  rule 2026-11-08): it emits no `RetryTypeaheadHandoffRequest` and triggers no write; the re-ask happens on the next real
  event (the Precmd, the line editor's ready signal, a report). The test asserts 0 retries from the timer (1 for zsh: the
  interrupt's own event-driven retry). The editor takes the line on the shell's own line-editor-active evidence (or the late
  Precmd/report), never on the timer. The older settle path for bootstraps without prompt sequence numbers (`INTERRUPT_SETTLE`, 250 ms) is not
  part of M10 and is unchanged.

## Wave P performance (code only; measured at the test run)

Every item has a kill switch (env var read once at start) and a numbers-only log line (`RUST_LOG=debug`). A test run
must collect the "before" with the switch at `0` and the "after" with it unset, in the same build.

### P1 echo path (`PHOSPHOR_PERF_ECHO`, `0` = old behaviour)
- A keypress no longer clears (or announces the clearing of) an EMPTY block-list / alt-screen selection
  (`blocks/selection.rs` and `alt_screen.rs` `clear_selection`).
- The wakeup throttle's echo floor is 1 ms instead of 4 ms (`wakeup_bypass_floor`).
- Collect: `hop=perf_echo_wakeup urgent=1 floor_ms=N since_prev_us=U` (the gap between delivered wakeups; with the
  switch on, urgent echo wakeups should cluster near 1000 us instead of 4000 us) and key-to-paint p50/p95 from the
  existing latency case (terminal mode, typing at 10-25 ms/key). A flood (`yes`) must still show `since_prev_us` near
  16700 for non-urgent wakeups.
- Unit: `wave_p_switch_values` (model_lock_trace.rs).

### P2 agent-mode key path under an output flood (`PHOSPHOR_PERF_AGENT_KEY`, `0` = old behaviour)
- Found: every typed key in agent mode ran `typed_on_settled_ai_target`, which built a Vec of the view ids of every AI
  block and every settled action card (O(blocks) allocations and card walks per key). It now asks the non-allocating
  `focused_settled_ai_target`, the same answer.
- Collect: `hop=perf_agent_key_probe settled_target_us=U ai_blocks=N on_target=B` (probe cost against block count, with
  the switch on and `0`), and key / Ctrl-C to pty-write p50/p95 in agent mode under `yes` / a flood, against terminal
  mode, with a conversation of 50+ AI blocks. The remaining agent-mode gap, if any, is main-thread time spent in
  flood-driven re-layout (P3); read `model lock held` lines for `BlockListElement::layout` at the same time.

### P3 long agent conversations: block list layout (`PHOSPHOR_PERF_BLOCKLIST_WINDOW`, `0` = old behaviour)
- `BlockListElement::layout` already lays out only the viewport plus a 20-line margin. What it did twice was the
  visible items: the viewport pass laid every visible AI block out, and the visible-items pass laid out again each one
  whose height had not moved by a pixel, with the model lock held. A view laid out in the first pass is not laid out
  again in the second.
- Collect: `hop=perf_blocklist_layout hold_us=U layouts=N reused=M switch=S` on a conversation of 100+ AI blocks, with
  and without the switch (`hold_us` is the lock hold of the visible pass; `reused` is the layouts saved), plus the
  existing `model lock held ... BlockListElement::layout` warn lines. Pass: hold_us falls with `reused>0`; scrolling
  through the conversation and typing in it show no stale or mis-sized block (screenshots at the top, middle, bottom).

### P4 PowerShell hooks (#1280; `PHOSPHOR_PWSH_HANDJSON`, `0` = ConvertTo-Json as before)
- `Warp-Precmd` built its CommandFinished and Precmd messages with `ConvertTo-Json` (the most expensive statement
  there on PS 5.1, per #1582); they are now built by hand like Preexec's (`Warp-Json-Str`, `Warp-Send-RawJson` in
  pwsh.ps1). The decoded messages are unchanged: `script/test_pwsh_hook_equivalence` passes every scenario (its
  `InitShell` row already differs on a `pid` field before this change, unrelated). Unicode and quote/backslash/control
  characters in the cwd and git fields are covered by the `unicode` / `cd-*` scenarios.
- Collect (Windows VM, PS 5.1, warp diag stamps `<tmp>.on` next to the diag file): new stamps `c` (Precmd start) and
  `r` (Precmd end) beside the existing `p` / `q` (Preexec); Precmd cost = (r - c) / QPC frequency, median and p95 over
  30 Enters, with `PHOSPHOR_PWSH_HANDJSON=0` and unset. Compare Enter -> prompt as in the #1280 harness.

### P5 code editor memory (#1340): measurement only this wave
- Earlier rounds already added parked line chunks (P5), the 5 MB syntax cap, no per-edit copy without a diff base and
  the close / opt-in open trims; the last recorded txt open still cost ~217 MB of RSS over a ~75 MB retained tree
  (buffer ~54 MB, parked lines ~10 MB), so ~140 MB is load transients the allocator keeps. No blind behaviour change was
  made. `Buffer::replace_all` now logs, for texts of 1 MB or more, `editor_memory load_stage stage=<start|formatted_text|
  edit_applied|undo_reset> rss_kb=N` (log target `editor_memory`, debug; Linux reads `/proc/self/statm` assuming 4 KiB
  pages, other platforms print -1).
- Collect: open the 7 MB txt and rs files from `editor_memory_7mb` with `RUST_LOG=editor_memory=debug` and report the
  rss_kb step between stages (the stage with the largest jump is the next thing to cut), together with the existing
  `editor_memory open ...` breakdown line and the after-open figure with `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1`.

## AWS credential resolver for Bedrock (`app/src/ai/aws_creds`)

The app resolves Bedrock credentials without the AWS SDK (design: slim the SDK, no loss of access). Unit tests, all in
`cargo nextest -p warp --lib --features warp/gui -E 'test(aws_cred)'`; none touch the process environment or the real `~/.aws`
(the resolver takes an injected `Env`, a temp home and a frozen clock from `aws_creds/test_support.rs`, so no serial guard is needed).
Endpoints are redirected with `AWS_ENDPOINT_URL_<SERVICE>` / the ECS and IMDS variables to `fake_llm::aws_stub::HttpStub`.

| area | tests | covers |
|---|---|---|
| INI parser | `aws_creds::ini_tests` | headers, comments, inline comments, nested blocks, BOM, credentials-over-config precedence, `~` and file-location variables |
| SigV4 | `aws_creds::sigv4_tests` | the AWS SigV4 test-suite vectors (get-vanilla, session token, post form), double-encoded path |
| chain / env / profile | `aws_creds::mod_tests`, `profile_tests` | env, `AWS_PROFILE`, explicit profile, missing profile, order env > profile > web identity > container > IMDS, static keys, session tokens, source_profile chains and loops, external_id, duration, credential_source, MFA, credential_process |
| SSO | `aws_creds::sso_tests` | valid cache, legacy start-url key, expired + refresh rewrites the cache, expired without refresh token, spent refresh, still-valid fallback, portal 401 |
| STS | `aws_creds::sts_tests` | unsigned web identity (JSON + XML), error code/message only, SigV4-verified AssumeRole, OidcManaged path |
| ECS / IMDS | `aws_creds::imds_tests` | full-URI auth token/file, IMDSv2 token, v1 fallback, disabled, unreachable, no role |
| credential_process | `aws_creds::process_tests` | ok, non-zero exit (stderr never reported), version, bad JSON, expired (unix only for the spawning cases) |
| `aws login` | `aws_creds::login_tests` | DPoP proof shape and ES256 signature, cache path, refresh with proof bound to POST+URL+time, cache rewrite, rejected refresh |
| Bedrock signed request | `aws_creds::mod_tests::*converse*` | resolved (profile and assumed-role) credentials sign `POST /model/{id}/converse`; `fake_llm::aws_stub::BedrockStub` recomputes SigV4 independently (scope, `x-amz-security-token`, content sha256, frozen time) and also refuses wrong region/secret/token |
| error messages | `aws_credentials_tests` | user-facing text per error kind; under `aws-sdk-login` the SDK error mapping |

`crates/fake_llm/src/aws_stub.rs` has its own tests (`aws_stub_tests.rs`) including the suite vectors against the verifier.
The `aws-sdk-login` feature is default ON; run the lib tests once with it off (`--no-default-features` plus the usual feature set, minus `aws-sdk-login`) to prove the build graph is SDK-free.
Not covered by unit tests: a real `aws login` / SSO / IMDS endpoint. **Owner verification before turning `aws-sdk-login` off:** a Bedrock request from a real `aws login` profile, with the cache close to expiry so the DPoP refresh runs.
**Cargo.lock:** adds `hmac 0.12`, `sha1 0.10`, `p256 0.14` to `app`, and `hmac`/`sha2` to `fake_llm`; the first box build updates it.

## Line-editor activation is event-driven (#1672/#1798, owner rule: timers never route)
`LINE_EDITOR_ACTIVATION_DELAY` (50 ms) is gone. A shell that has never sent the ready signal (bracketed-paste enable, PowerShell `LineEditorReading`) opens the write gate on the Precmd/EndPrompt event itself, unproven; a signalling shell opens it only on the signal. `LINE_EDITOR_SIGNAL_GIVE_UP` (1 s, or the ConPTY hold's wait) is only a net: it emits `LineEditorStatusEvent::SignalGaveUp`, the controller emits `PtyControllerEvent::LineEditorSignalGaveUp`, and the view runs the prompt hold's existing give-up (`on_line_editor_signal_gave_up`: keys back to the editor, or kept with the waiting toast); it never opens the gate or writes. A signal that could not open the gate (unanswered typeahead line, released keys not counted yet) is remembered and the next hand-off retry (`retry_typeahead_handoff_request`) opens it. Tests: `line_editor_status_tests::{a_shell_that_never_signals_opens_the_gate_on_the_prompt_event, a_signalling_shell_opens_the_gate_only_on_its_signal, the_give_up_net_hands_back_and_never_opens_the_gate, a_remembered_signal_opens_the_gate_only_when_asked, only_the_ready_signal_proves_the_line_editor_reads}`, `pty_controller_tests::the_signal_give_up_net_writes_nothing_and_leaves_the_gate_shut`, and (Legacy and Table) `view_test::the_line_editor_signal_give_up_hands_the_held_keys_to_the_editor_and_writes_nothing`. Behaviour change to know: an editor command queued for the gate waits for the signal for a signalling shell (it is no longer released by a timer); the first prompt of a session opens unproven, so it gets no ESC i request.

## Remote ssh holds are event-driven (#1808, #1809, #1810; owner rule: timers never route)

### #1808: the bootstrap hold has no 15 s clock
`SSH_BOOTSTRAP_SWALLOW_CEILING` is gone. Keys typed while the remote bootstrap is pending are held
(never dropped) until `Bootstrapped`/`InitShell` ends the session or the bootstrap-failed give-up
(a hand-back net, already compliant) marks it failed. Tests in `terminal/view_test.rs`:
`ssh_bootstrap_swallow_is_ended_by_events_only` (replaces `ssh_bootstrap_swallow_is_bounded`),
`ssh_bootstrap_interrupt_route_holds_then_forwards` (only `failed` flips Hold to Forward),
`ssh_keys_typed_during_a_pending_bootstrap_stay_held_in_order_and_unwritten` (byte trace, Legacy and
Table: held bytes exact, nothing written, nothing in the editor); `input_queue/ssh_phase_tests.rs`
`the_bootstrap_hold_has_no_clock` replaces `the_swallow_give_up_ends_the_swallow`.

### #1809: the ssh connect window arms and ends on events
`SSH_CONNECT_SILENCE_CEILING` is replaced. The window no longer arms on `block age < 15 s` (a first
key typed late still finds the window open); it arms on the preexec/submit event, and not at all
for a block whose ownership event (`CommandBecameInteractive`) arrived before any window existed (an
established session is a plain terminal). Where the tty-read probe works there is no clock; on
probe-less platforms `SSH_CONNECT_NO_PROBE_GIVE_UP` (15 s) remains as a hand-back-only net (compliant
section of `script/input_path_timers.allow`). Tests (`terminal/view_test.rs`):
`ssh_gate_with_a_tty_read_probe_has_no_clock` (new); the existing ceiling cases
(`ssh_no_prompt_ever_gives_the_text_back_to_the_input`, the table-mode give-up cases,
`managed_ssh_without_a_tty_read_probe_keeps_the_ceiling_and_hands_back`,
`managed_ssh_verdict_after_the_old_ceiling_still_delivers_once`) now name the renamed constant and
run unchanged. Not covered at view level: arming with an old block start (needs a clock-injected
model); the gate-level test covers the clockless release.

### #1810: no timer declares the ssh login done
`SSH_LOGIN_RECHECK_GIVE_UP` (3 s of quiet output) is deleted. It only ever fired the warpify
footer/prompt (`ReadyToWarpify`, re-verified: the handler shows UI and writes nothing to the pty;
the bootstrap is written only after the user accepts). Login-done is now decided by events alone: a
recognised prompt / last-login line in a later output chunk, or the remote shell's own bootstrap DCS
(`note_remote_shell_bootstrap_signal`); a re-prompt withdraws the recheck. A host showing neither
(`.hushlogin` with an unrecognised prompt) gets no offer and stays a plain terminal. The model-level
recheck cases in `terminal/model/terminal_model_test.rs` (`check_for_end_of_ssh_login`) are unchanged;
no test depended on the view timer.

## Stale hand-off counts: events only (owner rule 2026-11-08)
`TYPEAHEAD_HANDOFF_STALE_COUNT_RESET` (250 ms, non-seq path) is deleted; `TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP` (5 s) now only logs `handoff_stale_counts_kept` (no reset, no retry, no toast, no write): the pty keeps input until an event. Events that re-baseline: the sampled terminal owner at decision time, `counts_provably_stale` (unix probe + stream barrier), `ContinuationPrompt`, the Precmd, and on ConPTY (no probe, no barrier) PowerShell's `LineEditorReading` marker parsed after the last counted Enter (`TypeaheadHandoff::powershell_marker_proves_reading`; set per session by the controller for PowerShell under `PHOSPHOR_LINE_EDITOR_READING_ACTIVATES`; `has_unanswered_lines_for_activation` lets that marker activate the editor so the decision re-baselines). Residual risk: a marker emitted before an Enter but parsed after it (ConPTY has no barrier). Tests: `typeahead_handoff_tests::a_powershell_marker_after_the_last_enter_proves_stale_counts_without_a_probe`, `pty_controller_tests::typeahead_handoff::{a_consumed_enter_is_released_by_the_line_editor_marker_not_a_quiet_period, an_in_band_command_is_released_by_the_line_editor_marker_not_a_quiet_period}` (replace the two quiet-period tests; the hand-off and controller are shared by Legacy and Table). The non-seq bootstrap (`PHOSPHOR_PROMPT_SEQ=0`) now waits for the same events; dash/sh with no hooks and a stuck PS2 outside PowerShell stay pty-owned until the next Precmd.

## Ctrl-C settle without prompt sequence numbers: an event, not a timer (owner rule 2026-11-08)
`INTERRUPT_SETTLE` (250 ms, `TerminalView::handle_typeahead_handoff_interrupt_for_shell`) no longer commits the editor. On a session without prompt sequence numbers the hand-off sets `interrupt_settle_pending`; the line editor's ready signal while it is already active (`activate_line_editor_on_ready_signal`: zle/readline restart the line after ^C, no Precmd, so no `Active`) runs the controller's decision, and the next Precmd clears it. A shell that never signals and runs no prompt cycle (dash/sh) keeps the pty as input until its next Precmd. Tests: `view_test::typeahead_handoff_ctrl_c_without_seq_numbers_does_not_commit_on_a_timer` (Legacy and Table), `pty_controller_tests::typeahead_handoff::a_ctrl_c_without_seq_numbers_is_settled_by_the_ready_signal_not_a_timer`.

## ConPTY mid-line guard: state, not a quiet period (owner rule 2026-11-08)
`CONPTY_MIDLINE_QUIET` (120 ms), `last_line_bytes_at` and the lock-free `QueuedLineBytesStamp` the view wrote for keys queued behind a hold are deleted. `TypeaheadHandoff::typing_a_line_on_conpty()` (ConPTY, no prompt sequence numbers) is true while a line is in progress: `wrote_user_bytes` (keys in the shell's line without their Enter), a live young hold or a queued prompt hold. The request decision answers `Wait` meanwhile and is decided again at the next boundary event: the flushed Enter (`write_handoff_user_bytes_inner`), the Precmd, a hold's end. A user who types a partial line and pauses no longer gets the request after 120 ms; the line stays in the shell (plain terminal) until one of those events (owner approved). Tests (`typeahead_handoff_tests.rs`; hand-off state is shared by Legacy and Table): `conpty_writes_no_request_while_a_line_is_still_being_typed` (no pause releases it; the line's Enter + Precmd commit), `conpty_midline_guard_counts_held_keys`, `the_midline_guard_sees_keys_held_behind_a_prompt_hold` (was `..._keys_the_view_queued`; the sequence path never waits).

## Young Enter: state and the stream barrier, no window (owner ruling 2026-11-09)
`YOUNG_ENTER_WINDOW` (600 ms), `YOUNG_ENTER_MIN_HOLD`, `YOUNG_ENTER_FLOOR`, `RTT_HOLD_FACTOR`, the `TimingWindow` evidence and the round-trip-sized hold wait are deleted. Where no sample says who owns the tty (ConPTY, a session without prompt sequence numbers) `TypeaheadHandoff::unsettled_evidence` decides an Enter typed while a line is unanswered at the moment of the Enter: the stream barrier says output is unparsed -> held (`PrecmdInFlight`), released by `on_stream_applied` (a Precmd in the output: the request is due; none: typeahead for the running command); the stream fully applied -> NOT young: either a command is running (delivered to it, plain terminal) or the prompt is SETTLED (counts say it is back and nothing is unparsed) and the Enter follows the normal non-young path exactly as before the window was removed (round 2, 2026-11-09: a settled prompt is not unsettled; #1574, a committed delivery is never asked for its line); no barrier or a failed look -> delivered (#933's cost accepted). A hold that waited on the barrier and finds a Precmd in the output it waited for makes the request due (`on_stream_applied`, non-seq sessions). The nets are unchanged: `EVENT_HOLD_GIVE_UP` 2 s for the no-barrier (fish) hold, `YOUNG_HOLD_GIVE_UP` 10 s (#1661) otherwise. The seq-path `window()` uses (`own_preexec_pending`, an unsampled command running) became `PrecmdMaybeInFlight` / the same barrier rule.
ConPTY barrier (Windows only, wired in `terminal_manager.rs`): the unread half is `windows::conpty_output_has_unread` (`PeekNamedPipe` on the ConPTY output pipe; feature `Win32_System_Pipes`) behind the existing reader-side counters. If the peek fails the barrier answers "cannot tell" (`with_stream_drained_or_unknown`), the hand-off delivers as a plain terminal, and `ConPTY output peek unavailable` is logged once per session. Caveat, to verify on the Windows VM after the build: the peek sees bytes in the pipe only; bytes the async pipe wrapper completed into its own buffer but the event loop has not read yet, and bytes conhost has not written yet, are invisible. Unit tests model "activity just now" (a command written, a Precmd or Preexec seen: every place `last_activity_at` is set) as an undrained stream (`note_fresh_activity_for_test` installs one unless the test installed a probe) and `age_last_activity_for_test` as SETTLED (a fully applied stream, until new activity reopens it). Tests: `an_enter_is_held_by_the_stream_barrier_not_by_the_age_of_the_last_activity`, `the_young_hold_wait_is_the_event_net_whatever_the_round_trip`, `one_stalled_sample_ages_out_of_the_round_trip_window`, `a_command_write_to_preexec_round_trip_is_sampled_before_any_report`, `only_the_no_barrier_young_hold_keeps_its_short_cap`, `a_young_holds_normal_end_class_is_fixed_when_it_starts`, the rewritten property `every_hold_names_a_release_that_is_satisfiable_when_it_engages` (adds an unsampled-with-barrier probe, drops the window dimension), all in `typeahead_handoff_tests.rs` (the hand-off is shared by Legacy and Table; the view tests in `group2_timer_tests.rs` that hold a young Enter run both modes and use the undrained-stream setup).
What the ~50 ms "became interactive" latch (`CommandBecameInteractive`) still decides after this: `view.rs` calls `note_command_interactive`, which on a platform that samples nothing ends a live young hold as typeahead (writes its keys); `young_evidence_sampled` returns "deliver" when the latch is set and nothing is sampled; `release_enter_window_for_program` releases the ConPTY enter window's keys to the program; `ssh_on_tty_owned` releases the ssh-held keys. On unix the sample overrides the latch for the young Enter. Log-only uses: `log_ssh_pty_owner`, `young_hold_handed_back`.

Capability gating in the app (#1804): every feature family that looked a client up with `RemoteServerManager::client_for_host` now uses `client_for_host_with(host, capability)` (or `host_supports`), so with the limited helper connected it takes its no-helper path exactly as with no server: `FILE_CONTEXT` (agent `read_files`, `apply_file_diffs`, `read_skill`, remote context files, and `SessionContext::is_remote_without_file_tools`, which withdraws the tools), `BUFFERS` (global buffer model), `DIFF_STATE` (diff-state subscription, remote diff model creation), `GIT_WRITE` (discard, stage, git dialog), `GIT_READ` (branch list), `GIT_STATUS`, `GITHUB`, `CODEBASE_INDEX` (index mutations and remote retrieval), `REPO_METADATA`, plus `RIPGREP_SEARCH` and `FS_BASIC` for the helper's own families. An empty capability list (legacy full daemon) supports everything. Tests: `client_tests::initialize_records_the_servers_capabilities` (empty, limited without and with `FILE_CONTEXT`), `controller_tests::file_tools_are_withdrawn_for_a_server_without_file_context_and_offered_for_a_legacy_daemon`, `capability::tests` (the helper claims none of the legacy-only families, now including `GIT_STATUS`, `GITHUB`, `SKILLS`), and `ssh_no_helper_chip_tests::explanation_does_not_promise_features_the_helper_lacks`. The no-helper chip (added on merge/w0) was merged into this branch; it was never deleted here, the branch had only predated it.

## Young-line verdict wait: decided type goes at once, the timers only hand back (owner ruling 2026-11-09)
`TerminalView::finish_young_detection_wait`: when the input type is already decided (`input_type_is_decided`: locked, or autodetection off) and a classifier can run, the young line's deferred Enter is submitted immediately (the detection in flight is abandoned, `give_up_pending_detection_for_submit`) -- no verdict can change the route, so there is nothing to wait for and no timer is armed. A lock or setting change while the Enter waits releases it through the existing `ctx.observe(&ai_input_model)` -> `release_deferred_enter_if_verdict_settled`, which now also treats a decided type as settled. The three timers of `arm_young_timer` (compute cap, wall ceiling, 2 s backstop) are give-up nets only: the `SubmitWithoutVerdict` outcome always drops the late verdict and hands the line back (`keep_deferred_enter_text_hand_back("young_verdict_cap")`, #1661 semantics for the keys behind it unchanged); the timer callback also returns without acting when the type has become decided (the notification owns that release). `young_submit_decision_for` has exactly four outcomes (`Submit`, `Wait`, `KeepInEditor`, `SubmitWithoutVerdict`). `script/check_input_path_timers --strict` passes with zero pending. Tests: `view_test::a_decided_input_type_submits_the_young_enter_at_once_without_a_timer` (Legacy and Table), `typeahead_handoff_tests::the_young_submit_decision_has_exactly_four_outcomes`.

## Remote codebase index: scan, chunk, hash (P9a)

`phosphor-remote index-scan <repo> [--known <file>]` is the helper's scan half of remote indexing, runnable by hand before any
app wiring exists (P9b adds the store, the proto and the app driver). It walks `<repo>` with `ignore` (`.gitignore`, plus
`.warpindexingignore` / `.cursorignore` / `.cursorindexingignore` / `.codeiumignore`; no global or parent ignore files, `.git`
never), skips binary (a NUL in the first 8000 bytes, or not UTF-8), empty and 3,000,000-byte-or-larger files, chunks with the
app's NAIVE chunker (`shellcmd/chunk.rs`: 200 lines or 12,000 bytes per fragment, 0-based inclusive lines, half-open byte range)
and prints JSON: `manifest` `[{path, file_hash}]`, `chunks` `[{content_hash, path, start_line, end_line, byte_start, byte_end}]`
and `root_hash`. All hashes are SHA-256 hex (`file_hash` of the file bytes, `content_hash` of the fragment text, `root_hash` of
the sorted `path\0file_hash\n` lines). With `--known <previous output>` it prints only the diff: `unchanged`, `added_files`,
`changed_files`, `removed_files` and `new_content_hashes` (the fragments that need text and an embedding). Known difference from
the app's local index: the app tries a tree-sitter split first for languages it recognises; the helper cannot carry grammars, so
for those files its fragment boundaries (and hashes) differ from the app's. Helper and shell path agree with each other.

Owner test (build on the box: `cargo build -p remote_server --profile release-remote-helper --no-default-features --features daemon --bin phosphor-remote`; use any repo, a scratch copy is safest; keep scratch under the job tmp dir, never `/tmp`):

```
H=./target/release-remote-helper/phosphor-remote
$H index-scan "$REPO" > "$T/one.json"
$H index-scan "$REPO" > "$T/two.json" && cmp "$T/one.json" "$T/two.json"    # identical
echo "// touched" >> "$REPO/<one file>"
$H index-scan "$REPO" --known "$T/one.json"      # changed_files = that file; new_content_hashes = its new fragments only
```

Unit tests (`cargo nextest run -p remote_server --features daemon --lib chunk`): `shellcmd/chunk_tests.rs` (golden spans: the
app's `naive_tests.rs` cases and a 450-line file) and `daemon/chunk_tests.rs` (temp dirs: determinism, hash values, one file
changed, `.gitignore` honoured without a git repo, binary / oversize / empty skipped, rename, delete, 200-line boundaries).

`script/fake_embeddings.py [--port N] [--dims N]` (standard library only, loopback, no outgoing connection) answers
OpenAI-style `POST /v1/embeddings` (`input` string or list, optional `dimensions`) with a deterministic unit vector per text
(SHA-256 derived), `GET /health` with `ok`; the chosen port is its first stdout line. It exists for the P9b end-to-end case.

## Remote codebase indexing, host scans / app embeds (P9b)

Capability `codebase_chunks`; the legacy host-embedding `codebase_index` stays legacy-only. Owner rulings: NO HTTP/TLS in the helper; the APP embeds through
`crates/network_gate` with the user's own BYOP `/embeddings` provider, so the key never leaves the user's machine (`EmbeddingProviderConfig` is no longer sent;
`Initialize.embedding_provider` / `UpdatePreferences.embedding_provider` stay occupied in the proto and are always empty, `ClientPreferences` has no such field);
the host keeps a flat-file store; search is brute-force cosine. Off by default: nothing scans, embeds or reads a key unless `should_use_codebase_indexing` (the
`FullSourceCodeEmbedding` flag AND the codebase-context setting) is true.

Wire (additive): `ScanCodebase` -> manifest + fragment spans + root hash (a big reply is split into FrameChunks like any large frame), `GetChunks` -> text only
for fragments without a vector under the storage key, `StoreVectors`, `QueryVectors` (host-scoped tags 25-28, responses 44-47).

Store (`crates/remote_server/src/index/store.rs`; helper: `~/.phosphor/index/<hash of repo path>/`, override `PHOSPHOR_REMOTE_INDEX_DIR` for tests; no-helper arm:
`<state dir>/remote-index/<hash of user@host + repo>/`, so it survives reconnects): `meta.json` (format 1: manifest, spans, root hash; atomic rename) and one
`vectors-<hash of storage key>.bin` per embedding space (header `PHVEC001`, key, dims; records = 32 raw hash bytes + dims f32 LE; appended, torn tail ignored and
cut by the next append; last record of a hash wins). A storage-key change leaves the old space unused and `StoreVectors` removes it, so the repository re-embeds.
Fragments whose vectors are dead (edited/deleted files) are compacted away on a scan once the file holds 1.5x the live set + 64 records.

No-helper arm (`shellcmd/index.rs`, `backend/shell/index.rs`): `git ls-files -z --cached --others --exclude-standard` (find fallback pruning .git, node_modules,
target, __pycache__, .venv) piped through `find -maxdepth 0 -type f -size -3000000c` into `sha256sum` / `shasum -a 256` / `sha256 -r` (hashes equal the helper's;
`git ls-files -s` was NOT used because it hashes the index and misses unstaged edits). Only files whose hash is new are read (base64, 32 per command, halved when
the output cap is hit) and chunked with the shared chunker; binary/empty files are remembered by hash and not read again. Known differences from the helper:
the shell arm does not read `.warpindexingignore`/`.cursorignore`/`.codeiumignore`, prunes the five directory names in a non-git tree, and is not offered on
PowerShell hosts (Unsupported). Both arms use the naive chunker, not the app's tree-sitter split (fragment hashes differ from the app's local index).

App (`app/src/remote_server/remote_index_driver.rs`, `remote_server::backend::index_sync`): the algorithm is written once over `RemoteFeatures` (scan -> get_chunks
windows of 256 -> embed in the provider's batch size -> store_vectors; rescans up to twice when a file changed between scan and fetch). `RemoteIndexDriver` starts
a sync on events only: `NavigatedToDirectory` into a git repo (auto indexing setting), the router's `AfterOwnWrite`, the prompt after a HEAD-changing command
(`command_may_change_head`), `user_resync`, and the setting being turned on. One sync per repo at a time, one more queued if events arrive meanwhile. No timer.
`CodebaseRetrievalHandle::RemoteIndex` embeds the query in the app and asks the host to rank. Logs: `hop=remote_index_sync start|done|failed|skipped`,
`hop=remote_index_embed texts=N via=network_gate purpose=ai_endpoint` (counts only).

Tests (compile on the box, not run on the laptop): `cargo nextest run -p remote_server --features daemon --lib index` -- `index::store_tests` (round trip, torn tail,
dimension change, storage-key change, ranking and tie order, compaction), `daemon::index_store_tests` (scan -> chunks -> store -> query, incremental, a file edited
after the scan is reported missing), `shellcmd::index_tests` (command builder, parser), `backend::contract_tests::assert_codebase_index` (the SAME literal assertions
against the in-process helper and the shell arm over a local sh) plus shell-only tests (cold text cache re-reads and verifies hashes; only new hashes are read;
no storage configured = Unavailable), `backend::index_sync_tests` (first/second/edit/key-change/batching/failure-resume/search on the shell arm with a fake
embedder, and the helper arm in-process); app: `remote_index_driver_tests` (model choice keeps the repository's last storage key). Removed on purpose (owner ruling):
`byop_key_gate_tests` and `remote_indexing_extra_headers_warning_tests` in `ai/codebase_embeddings.rs`, which pinned the key being SENT to the host.

Lab: `lab_codebase_index_remote` (`cases_lab_index.py`; bash zsh fish nu, lan and 150 ms, nohelper + upload, terminal mode). Needs `APPCI_FAKE_EMBEDDINGS=1`
(run.py starts `script/fake_embeddings.py --log` on loopback; profile.py configures it as the BYOP embeddings provider `text-embedding-3-small`, with a sentinel
`X-Appci-Key` header as its key, turns on codebase context + auto indexing, and sets `ZAP_UNSTABLE_FEATURES=full_source_code_embedding`); skips with a reason
otherwise. It asserts: first sync embeds every fragment, routing line says helper/shell, the gate line is logged, the fake provider saw the sentinel hash, the
sentinel is nowhere on the host (remote shell commands, `~/.phosphor`, the repo), and after an outside edit + `git checkout -b` exactly the edited fragment is embedded.

## Editor Enter and host-colour probe: the clock never commits, the probe never eats a key (owner ruling 2026-11-09)
Editor Enter (`Input::wait_for_verdict_then_enter`): a decided input type (locked, or autodetection off) gives up the pending detection and submits at the Enter, and from the model notification (`release_verdict_wait_if_settled`), with no cap armed; `EnterVerdictWait` is `Submit | Wait`. `VERDICT_HANDBACK_CPU_CAP` (1 s of classifier CPU + the size allowance) and the 2 s backstop only hand the line back (`hand_back_slow_verdict`: toast, keys behind the Enter dropped with a count #1661, counted WARN `Enter verdict too slow ... (total N this run)`); an event that decided the Enter meanwhile wins. Tests: see the verdict-cap row above (`test_slow_verdict_cap_hands_the_line_back_and_drops_keys_behind_the_enter`, `test_decided_input_type_submits_at_once_with_no_cap_armed`, `test_lock_change_while_the_enter_waits_submits_it_in_order`, `test_verdict_cpu_cap_ignores_a_job_waiting_off_the_cpu`).

TUI host-colour probe (`crates/warpui_core/src/runtime/terminal_probe.rs`, `run_tui_input_reader`): the probe no longer reads stdin behind crossterm's back (it used to discard every key typed in its 100 ms / 50 ms window and let a late reply leak as typed text, #580 #1748 #1749). It runs on the driver's reader thread -- once at startup AFTER the first frame (the colours apply when they land; the first frame uses `COLORFGBG`, else dark) and on focus -- reading the same crossterm events as the keyboard. `ReplyFilter` swallows only an OUTSTANDING `ESC ] 10|11 ; rgb:|rgba:... (BEL|ST)` (crossterm shows it as Alt-`]` + characters + Ctrl-G / Alt-`\`), by content and once, for the probe window plus `LATE_REPLY_WINDOW` (2 s); every other event passes in order, a withheld Esc/Alt-`]` is handed on after `REPLY_HOLD_NET` (50 ms). It stops the moment OSC 11 completes (`DA1_DRAIN_GRACE` and the DA1 sentinel are gone: crossterm consumes a DA1 reply internally, so it cannot leak). Tests (`crates/warpui_core/src/runtime/terminal_probe_tests.rs`, pure, no tty): `user_bytes_before_between_and_after_replies_keep_their_order`, `a_reply_split_across_reads_still_matches_bel_and_st`, `alt_bracket_then_text_is_not_eaten`, `an_unfinished_match_is_released_in_order_by_the_hold_net`, `a_late_reply_is_swallowed_once_and_only_while_outstanding`, `only_the_asked_for_osc_code_is_swallowed`, `an_event_that_is_not_a_key_breaks_a_partial_match_and_nothing_is_lost`, `key_events_map_to_the_bytes_crossterm_parsed_them_from`, `the_window_stops_when_the_reply_completes_and_forwards_keys_typed_inside_it`, `a_missed_window_gives_up_without_losing_keys_and_the_reply_stays_outstanding`. The old `classify_probe_read` / `contains_da1_reply` tests (the drain-window rule) are deleted with the code they pinned. Real terminals are not exercised by these tests; a real-tty check (type during startup, focus away and back on a slow transport) belongs to the next TUI app round. `script/check_input_path_timers --strict` has no A/B entries left.

## Thread census, the rayon cap and the shared filesystem watcher (perf, measured-only)

Two changes cut idle threads; each has a kill switch and neither is a latency change.

| switch | effect |
|---|---|
| `PHOSPHOR_RAYON_THREADS=<n>` | caps rayon's global pool (`warp-rayon-N` threads). Unset = 8 (clamped to the CPU count), `0` = rayon's own default (one per CPU). Applied once at the start of `run()`. |
| `PHOSPHOR_SHARED_WATCHER=0` | each `BulkFilesystemWatcher` gets its own three threads again. Default (Linux only): all handles share one platform watcher, a `Shared FS Commands` thread and a `Shared FS Dispatch` thread. |

The shared watcher keeps every subscriber's own debounce window (no timers added). A root that
conflicts with an existing one (same directory, or inside a recursive root) is served by a private
`Bulk Filesystem Watcher` thread set and logged at info level, so a few of those in the census are
expected (the home directory is non-recursive and does not conflict with repos below it).

How to measure (build box, Linux), before/after:

1. Start the app, let it idle 30 s, then `script/thread_census.sh <pid>`.
2. Open a file in the editor, wait 10 s, run the census again.
3. Repeat with `PHOSPHOR_RAYON_THREADS=0 PHOSPHOR_SHARED_WATCHER=0` for the baseline.

Expect `Bulk Filesystem Watcher`/`Filesystem Debouncer`/inotify threads to fall from 3 per handle
to 2 shared plus the conflicts, and `warp-rayon-N` to stop at the cap. Re-run the perf scripts
(`script/bench_gate.py`, the editor open/memory baseline in `script/editor_mem_baseline`) with the
rayon cap, since a smaller pool can slow a parallel edit or hydration; if it does, raise the default
rather than drop the cap. Unit tests: `cargo nextest run -p watcher` and the `rayon_pool` tests in
`-p warp --lib`.

## Keys follow the editor's kept line while the pty still owns input (#1658)
`TypeaheadHandoff::keys_follow_editor` (model/typeahead_handoff.rs): when an Enter gate's give-up keeps the line typed before the Enter (`finish_discard_gate_keys_after_enter`, `kept_line`), or hands keys back as text (`hand_back_gate_keys_inner`, `inserted`), or a deferred Enter's give-up leaves text in the editor, and the hand-off is still `PtyOwned`, the fact is set (`note_keys_follow_editor`; a no-op in any other state). While set, `routes_keys_to_pty()` and `wants_terminal_focus()` are false and `editor_takeover_open` / `editor_takeover_would_be_open` are true, so printable and control keys reach the editor in order after the line (legacy: `should_write_typed_chars_to_pty` returns false; table: `RouteFacts::keys_follow_editor` makes `pty_typed` false and `handoff_routes` false). It is cleared by EVENTS only: a state change (request written, commit), `on_command_written`, `on_preexec`, `note_command_interactive` (a program, ssh or a password prompt owns the tty, via `CommandBecameInteractive`), alt-screen swap, a password prompt, a signal key (Ctrl-C/D/Z/\), a click/selection on the terminal (`end_editor_takeover_window`), the editor emptying (`InputEvent::EditorUpdated` with an empty buffer), `reset_on_exit`. `EDITOR_TAKEOVER_GIVE_UP` (10 s) and the focus event do NOT clear it. No timer is added (`script/check_input_path_timers --strict` unchanged). Kill switch: `PHOSPHOR_KEYS_FOLLOW_EDITOR=0` restores the old routing. Tests: `typeahead_handoff_tests.rs`: `a_kept_line_makes_keys_follow_the_editor_not_the_pty`, `keys_follow_the_editor_only_where_the_pty_owns_input_and_the_switch_is_on` (kill switch via `note_keys_follow_editor_if(false)`), `the_ten_second_takeover_give_up_never_clears_keys_follow_editor`, `every_event_returns_the_keys_to_the_pty`, `a_state_change_ends_keys_follow_editor_and_a_new_pty_owned_state_does_not_inherit_it`; `view/keys_follow_editor_tests.rs` (Legacy and Table): `keys_typed_after_a_kept_line_follow_it_into_the_editor_while_the_report_is_late`, `a_program_or_an_empty_editor_returns_keys_to_the_pty`, `an_interactive_program_and_the_alt_screen_end_keys_follow_editor`, `a_signal_key_ends_keys_follow_editor`. Not yet covered by an automated test: the ssh `ssh_on_tty_owned` path with a live gate (it reaches the fact through `CommandBecameInteractive`, tested), and a real PowerShell tab (Windows VM, r18u22 `tabs_streaming`).

Run-4 follow-up on the young-Enter path (2026-11-09): `cap_is_normal_end(evidence, prompt_seq_mode)` is back to two normal-end classes -- the no-barrier hold (`PrecmdMaybeInFlight`) and the barrier hold (`PrecmdInFlight`) of a session WITHOUT prompt sequence numbers, which is what the window hold of such a session was (cap 2 s, hand-back; #1661). A sequence-path barrier hold stays a give-up (10 s, drop). The ctrl-C settle tests (`typeahead_handoff_ctrl_c_without_seq_numbers_does_not_commit_on_a_timer`, `a_ctrl_c_without_seq_numbers_is_settled_by_the_ready_signal_not_a_timer`) type text for the next prompt first, because with nothing typed the hand-off is already editor-owned at the prompt and there is nothing to settle. The property test treats `Probe::None` as carrying the test-default undrained barrier.

Run-4b (2026-11-09): `input_reporting_sequence_is_written_before_an_already_queued_in_band_command` pins the ORDER of the two messages (request, then the in-band command); the handler drains the queue in one pass, so the mid-test `pending_writes.len() == 1` that batch 4 added was wrong. `an_unproven_line_editor_after_ctrl_c_gets_no_request` types "echo hi" BEFORE the unproven activation ("the keys typed meanwhile"): an unproven `Active` with nothing queued hands the editor the line (no request binding), so keys typed after it are no longer the pty's (the helper asserts `WriteCounted`); the test was written blind on 2026-11-08 and never passed.

Run-5 (2026-11-09), tests only: `keys_follow_editor_tests::a_program_or_an_empty_editor_returns_keys_to_the_pty` registers a local session first (Legacy writes typed text to the pty only for a session that is bootstrapped or pending, since #1711 no longer opens that door by age; Table decides from its facts, which is why only Legacy failed). `typeahead_handoff_ctrl_c_without_seq_numbers_does_not_commit_on_a_timer` lets the pane's own start-up events land before it sets the hand-off up (a bootstrap Precmd event arriving during its 600 ms wait commits the hand-off at the prompt through `commit_typeahead_handoff_at_prompt`; that is not the timer the test is about).

## Line-editor ready signal is learned per session (#1823)
`LineEditorStatus::ready_signal_seen` was per pane and never reset, so after the local shell signalled, the session an `ssh`/`su` started (the fake or remote shell, which may never enable bracketed paste) was treated as a signalling shell: its prompt armed the wait, the editor's command stayed parked behind the closed gate and the 1 s net only reported `SignalGaveUp` (`holds=0`). Now the learned set is keyed by the ACTIVE SESSION id (`active_session`): a session that has not signalled opens the gate on its Precmd/EndPrompt event (unproven), returning to the local shell keeps what it learned, and a session whose wait gave up is demoted (its next prompt opens on the event; a late signal learns it again). The timer still never writes or opens the gate. A command already parked at the moment of a give-up waits for the next event of that session (a late signal or the next prompt hook), not for a clock. Tests: `line_editor_status_tests::{the_ready_signal_is_learned_per_session, a_session_whose_signal_wait_gave_up_opens_on_its_next_prompt}`, `pty_controller_tests::typeahead_handoff::a_nested_session_that_never_signalled_gets_the_parked_command_at_its_prompt` (controller and status are shared by Legacy and Table); the app case `ssh_nested_typeahead` is the real-app check.

## Young Enter: "own Preexec pending" needs a session that emits Preexec (round 3, 2026-11-09)
Mac terminal-mode run (`latency_key_log` family, zsh): 23-32 young holds per run reached their cap (`PrecmdMaybeInFlight`, `young_hold_cap`) because the session never emitted a Preexec, so the hold waited for an event that cannot come and the 2 s cap handed the Enter back to the editor instead of delivering it to the running program. `TypeaheadHandoff::session_emits_preexec` is learned per session (set by `on_preexec`, cleared by `on_new_session`, like the line editor's ready signal, #1823); the two `own_preexec_pending` evidence rows (`LineEditorReading` with a drained barrier, `ShellCooked`) only hold for a proven session, an unproven session's Enter is delivered at once (plain typeahead, nothing handed back). The `PromptSeen` gate in that log is the young hold's own gate, not a separate prompt-hold assumption; the ConPTY `LineEditorActive` prompt hold is unaffected. Tests: `an_unproven_session_gets_its_enter_at_once_not_a_hold_for_a_preexec_that_cannot_come` (the hand-off is shared by Legacy and Table); the property test and the ShellCooked pending test mark the session proven.

## The prompt event always opens the write gate (#1825; supersedes the signal-gated gate of #1672/#1798/#1823)
`unset zle_bracketed_paste` (the latency_enter M3 step, a legitimate user setting) stopped zsh's ready signal; the session had signalled before, so the next prompt closed the gate until the signal, the 1 s net only reported, and the editor's next command was parked behind the closed gate for good (no Preexec, so no next prompt for the #1823 demotion to act on): the pane wedged and every later case failed. Now `LineEditorStatus::on_prompt_hook` opens the write gate on the Precmd/EndPrompt EVENT for every session. A session not yet known to signal also decides at the event (`LineEditorStatusEvent::Active`, unproven: no ESC i, #1798). A session known to signal (learned per session, #1823) emits `GateOpened` (the controller runs its queued writes, nothing else) and its hand-off request decision waits for the signal (`awaiting_signal`, `decision_awaits_signal`): the signal proves the reader and emits `Active`; `PtyController::retry_typeahead_handoff_request` does not decide while that is pending. A prompt cycle that ends (a command starts) with the decision still waiting records `signal_missed`, and the next prompt event demotes the session (and clears the ConPTY `signal_wait`) so it decides at the event; a late signal learns it again. The 1 s give-up net, `LINE_EDITOR_SIGNAL_GIVE_UP`, `LineEditorStatusEvent::SignalGaveUp`, `PtyControllerEvent::LineEditorSignalGaveUp` and the view's `on_line_editor_signal_gave_up` are deleted; nothing on this path holds keys or uses a timer. ConPTY (Windows): `signal_wait` now only defers the request decision like a learned session; the write gate opens at the prompt, so an editor command can be written before the PowerShell marker (the tty buffers it), while the typed-key prompt hold in the hand-off is unchanged and keeps its own cap. Tests: `line_editor_status_tests::{the_prompt_event_opens_the_gate_of_a_shell_that_never_signalled, a_session_that_stops_signalling_still_has_its_gate_opened_by_the_prompt, the_ready_signal_proves_the_reader_and_starts_the_decision, a_remembered_signal_starts_the_decision_only_when_asked, the_conpty_signal_wait_defers_only_the_decision, the_ready_signal_is_learned_per_session}`, `pty_controller_tests::typeahead_handoff::{a_command_submitted_to_a_session_that_stopped_signalling_is_written_at_its_prompt, a_nested_session_that_never_signalled_gets_the_parked_command_at_its_prompt}` (controller and status are shared by Legacy and Table; the app cases latency_enter and ssh_nested_typeahead are the real-app checks). Behaviour change: for a session that signals, a command queued while the line editor was inactive is written at the prompt event instead of after the signal (a few ms earlier), and ahead of that prompt's request decision.

## Agent-run interactive commands: hand over and hand back (#1829)

Module `script/realapp/appci/cases_agent_interactive.py`, agent mode only, needs `APPCI_FAKE_MODEL=1` (the cases skip with the reason otherwise). `fake_model.py` now writes a request log (`APPCI_FAKE_MODEL_LOG`, default `<run>/fake_model_requests.jsonl`, set by `run.py`): per `/api/chat` request the last tool result and every user message. That log is the oracle for "the model's NEXT request carried the command's output" and "nothing typed into the program reached the conversation". Typing is at 10-25 ms/key with Enter in the same call.

| case | the fake model asks to run | oracles |
|---|---|---|
| `agent_interactive_password` | `bash -c 'read -s -p "pw: " x; echo; echo got:$x'` (a real tty read with echo off, not sudo) | the app log shows the hand-over (`hop=handover action=<id> reason=blocked_on_input`, logged by `switch_control_to_user`: the one line every system hand-over writes, including the echo-off password path that logs no "handing the command to the user" line; that line is also accepted); the next model request's tool result contains `got:<secret>` byte-exact; the secret is in no user message and not in the app log |
| `agent_interactive_vim` | `vim -u NONE -N ai_vim.txt` | vim's swap file proves it is up; after `i<text>`, Esc, `:wq` the file is exactly the text; text in no user message |
| `agent_interactive_ssh` | `ssh -tt fake '<remote read -s ...>'` against the local test sshd (skips with `ssh_why` when there is none) | the REMOTE file holds exactly `got:<text>`, no local file does; the model's next request has `got:<text>` |
| `agent_interactive_less` | `less ai_long.txt` (600 lines) | `G g jjjjj /line 450`, `!echo <tok> >> ai_less.txt`, Enter, `q`: the effect file is exactly the token; token in no user message |
| `agent_noninteractive_no_takeover` | `printf 'Continue? '; sleep 6; echo done:<tok>` | no `handing the command to the user` line; `done:<tok>` in the model's next request |

Every case ends with the same tail (`after_checks`): the model got a tool result within 40 s (control returned to the agent), no `Failed to enter inline agent view for LRC handoff`, and prose typed afterwards reaches the model exactly as typed (the agent input is alive and clean). A missing vim/less is unverifiable + skip. vim and less are full-screen programs: the app logs `hop=alt_screen_enter` when one takes the screen (less waits for it instead of a fixed sleep; no hop in 30 s = skip) and `hop=fullscreen_handover agent_in_control=0` when the user's first key into it takes over a command the agent holds in control (`terminal/view.rs`). An agent command waiting for completion has no control state (a Manual take-over is refused by design, #1796/#1811), so that hop never fires for it (Round 7 Linux, fake model); both cases then accept a pty-owned `hop=hand_route ... command_interactive=true` after `alt_screen_enter` (the user's keys reached the program; both input modes log it) and still FAIL with neither, besides the file oracle. Platforms: Linux and macOS; Windows is classified NOT_WINDOWS in `winmap.py` (POSIX programs, unix-only tty probe). The terminal-mode counterparts are `alt_screen_programs`, `sudo_su`, the `ssh_*` cases (an agent-run command exists only in agent mode). Run: `APPCI_FAKE_MODEL=1 python3 script/realapp/appci/run.py --cases 'agent_interactive_*,agent_noninteractive_no_takeover'`. Self-test: `agent_interactive_oracles` (includes matcher rows for the hand-over hop and the lines that must not match, such as `reason=stop`). The hand-over oracle (`handed`) was the prompt-shape line only until Round 7, which a `read -s` hand-over never writes.

## #1827: the agent driving a remote host in the ssh lab (`appci/cases_agent_remote.py`)

What: three lab cases where the AGENT works on an ssh pane end to end, each with `nohelper` (shell mode over the ControlMaster) and `upload` (the helper), agent mode only, `--cases lab_agent_remote_*` (needs the lab and `APPCI_FAKE_MODEL=1`; without them `lab_prepare` / `need_model` SKIP with the reason).
- `lab_agent_remote_flow` (shells bash dash ash nu pwsh): ONE agent turn of scripted tool calls: `read_files` -> `grep` -> `file_glob` -> `apply_file_diffs` edit of an EXISTING file -> `run_shell_command` `cat` of the edited file -> `search_codebase` (only when `APPCI_FAKE_EMBEDDINGS=1` gives the host an embeddings provider; otherwise a note says it was not run). The fake model's new plan turn: `Please carry out these steps in order: read P ;; grep Q DIR ;; glob PAT DIR ;; edit P :: SEARCH :: REPLACE ;; run CMD ;; codebase Q` answers one tool call per already-answered step, then `Done.` (`fake_model.parse_plan` / `plan_call` / `turn_state`, self-test `fake_model_plan_and_agent_remote_pure_parts`).
- `lab_agent_remote_errors` (same shells): a read of a missing file, an edit whose search text is not in the file, a grep and a glob with no hit. The model must be told the truth (`failed_files`/error, no applied edit, empty hit lists) and the host file stays exactly as it was.
- `lab_agent_remote_drop` (bash dash nu): the lab server RESETS the connection (`lab.drop(user, "rst")`) while the agent's `run_shell_command` (`sleep 300 && echo fin_<tok>`) runs on the host. Oracle: within 60 s the model receives a tool result that is not `completed` with exit 0, not `running`, and has no final `fin_<tok>` output line (no hang); the pane is local; a new prompt is answered. If the app sends the model nothing, the case FAILS with "the tool call hangs" (that is the defect the case exists for).

Oracles (no screenshot alone): the file on the host (exact lines after the edit; unchanged after a refused edit); `hop=backend_route family=<capability> backend=...` per family, ALL required (`file_context`, `file_write`, `ripgrep_search`, `fs_basic`, and `codebase_chunks` when the index leg runs) with nohelper -> shell and upload -> helper. The agent's grep and glob run as session commands, not RoutedBackend calls, so `execute.rs::log_remote_tool_route` logs them with `remote_server::backend::executor_route_line` (grep -> `ripgrep_search`, glob -> `fs_basic`; backend = whether the session's executor is the helper's) for remote sessions only; the agent's `run_shell_command` is typed into the pane, not routed, and logs nothing; the lab shell's `rec.log` (the tokens only the read/grep/edit tools carry are never typed into the remote shell; the one approved `cat` is typed exactly once); and the tool results the MODEL received, read from the fake model (`FakeModel.tool_results_for(marker)`, `fake_model.ACTIVE`, the harness and cases share one process): parsed JSON, status ok, the right paths, lines and the NEW file content.

Run: `APPCI_FAKE_MODEL=1 python3 script/realapp/appci/run.py --cases 'lab_agent_remote_*'` (narrow with `APPCI_LAB_MATRIX`). Platforms: Linux lab host and Linux/Mac client; the drop case skips Windows panes (not ported); the others have no Windows-specific gate but have not been run there. Not run yet: nothing here has been run against a build.

## Agent file edits (#1835): accept, reject, conflict at save, line endings

`script/realapp/appci/cases_agent_file_edits.py`. The scripted fake model (`fake_model.py`, `APPCI_FAKE_MODEL=1`) gained one reply: `Please edit exactly this file: <path> with these edits: <search>=><replace> ;; <search>=><replace>` becomes one `apply_file_diffs` call with one `edit` op per pair (a multi-hunk diff; a literal backslash-n is a newline). The cases answer the diff card with the owner's keys (Enter focuses the card, Enter accepts, Ctrl-C rejects) and read the FILE BYTES back; the app log is the second oracle. The fake model's request log no longer raises on a tool call without a `command` argument.

| case | proves | oracle |
|---|---|---|
| `agent_edit_accept` | a three-hunk diff (lines 2, 5 and the last line; each search is a WHOLE line, because the app's matcher works on whole lines and a prefix-only search failed to match in round 7: `Failure(s) applying diff: 1 unmatched`, action `failed`, no card to accept) accepted writes exactly the expected bytes, once | file bytes, re-read after 2 s (no duplicate write) |
| `agent_edit_reject` | rejected: byte-identical, no later change; a fresh proposal afterwards still accepts | file bytes |
| `agent_edit_conflict` | the file changes on disk after the proposal and before the accept: the save is refused, the user's bytes are intact, the conflict is reported | file bytes == the user's bytes; app log `Failed to save file for accepted AgentMode diffs` (or `changed on disk`) |
| `agent_edit_endings` | CRLF, LF without a final newline, CRLF without one: line endings and the final newline survive an accepted edit that touches the last line | byte-for-byte against the expected bytes |
| `lab_fam_agent_edit_conflict.*` | accept + conflict on a remote host; arms `nohelper` (shell backend; the primary design) and `upload` (the helper); both guard the write with the SHA-256 of the bytes the accept read (#1837: `Expect::Sha256` in `shellcmd/write.rs` for the shell script, `WriteFile.expect` + capability `file_write_expect` for the helper; a helper without the capability falls back to the shell script, never an unguarded write); shells bash, dash, nu, pwsh | the host's file via the lab |
| `lab_fam_agent_delete_conflict.*` | the agent proposes DELETING a file and the user edits it on the host before the accept (#1837 follow-up); arms `nohelper` and `upload`; shells bash, dash, pwsh | the file is still on the host with exactly the user's bytes, and the refusal is in the app log (`changed on the remote host` / `was not deleted`) |
| `lab_fam_agent_edit_endings.*` | the endings cases on the remote host; shells bash, pwsh | the host's file via the lab |

`agent_edit_touch_only` (local) and `lab_fam_agent_edit_touch_only` (nohelper, upload): the mtime moves, the bytes do not, then accept: it must succeed with no conflict report (local: `warp_files::save_if_unchanged` compares content; remote nohelper: the size+mtime `Expect` may give a false conflict until branch `fix/1837-remote-edit-conflict` lands, a FAIL there is that defect). The lab edit cases (`conflict`, `endings`, `touch_only`) now require `hop=backend_route family=file_write` with nohelper -> shell, upload -> helper: `RoutedBackend::save_buffer_content` and `write_file_expecting` log it (the buffer-protocol save used to log none). Per-hunk accept stays unbuilt (all-or-nothing UI; deliberate note, no case).

Unit/contract tests for the #1837 guard (run on the box with the usual suites): `backend/contract_tests.rs` `the_shell_backend_refuses_a_stale_guarded_write` and `the_helper_refuses_a_stale_guarded_write` (same assertions on both backends: matching hash and `Absent` write, a user edit / appeared / deleted file writes nothing and leaves the user's bytes and no temp file); `backend/router_tests.rs` `a_guarded_write_with_no_way_to_check_is_refused_and_leaves_the_file`; `daemon/fs_ops_tests.rs` `write_file_with_an_expectation_writes_only_when_it_holds`; `capability.rs` `write_expect_is_never_assumed_for_a_legacy_daemon`; `warp_files` `remote_write_expectation_names_the_exact_bytes_that_were_read`; `code_diff_view_tests.rs` `toast_message_carries_the_remote_host_conflict` (the accept surfaces the host's refusal in the toast and the `Failed to save file for accepted AgentMode diffs` log line).

#1837 follow-ups. (1) Remote DELETE carries the same expectation: `DeleteFile.expect` (optional `WriteExpect`, the SHA-256 `delete_file_if_unchanged`'s read approved, via `remote_write_expectation`), capability `file_delete_expect`, `RoutedBackend::delete_file_expecting` (helper that lists the capability, else the guarded shell script `posix_guarded_delete_command` / `pwsh_guarded_delete_command`, else REFUSED, never an unguarded delete), `warp_files` refuses when it has no exact hash to send. Tests: `contract_tests.rs` `the_shell_backend_refuses_a_stale_guarded_delete` and `the_helper_refuses_a_stale_guarded_delete` (same assertions: user edit -> nothing removed and the message says `not deleted`; matching hash deletes; already gone is an error); `router_tests.rs` `a_guarded_delete_with_no_way_to_check_is_refused_and_leaves_the_file`; `daemon/fs_ops_tests.rs` `delete_file_with_an_expectation_deletes_only_when_it_holds`; `capability.rs` (same test as for the write capability); `shellcmd/write_tests.rs` `a_guarded_delete_checks_before_it_removes_and_the_plain_one_stays_plain`; lab case `lab_fam_agent_delete_conflict` (above). (2) A host with no `sha256sum`, `shasum` or `Get-FileHash`: the guarded script exits `EXIT_NO_HASH` (6, output `NOHASH`), distinct from the conflict exit 5, and the message is `could not check that the file on the host is unchanged (no sha256 tool), so nothing was written` (or `deleted`), never `changed on the host`; still fail closed. Tests: `write_tests.rs` `a_hash_guarded_script_reports_a_missing_hash_tool_apart_from_a_conflict` (script outcome and message), `contract_tests.rs` `a_host_with_no_sha256_tool_refuses_guarded_changes_and_says_why` (a local `sh` whose PATH has base64 but no hash tool: write and delete leave the file, no temp file, truthful message; an `Absent` guard still works). `file_delete_expect` is registered as `fam_file_delete_expect` in `cases_lab_helper.py` like `file_write_expect`. Self-test: `agent_file_edit_pieces` also covers the deletion prompt and report matcher.

The lab coverage guard (`lab_helper_coverage_guard`) needs every helper capability to have a lab case with a helper arm; `file_write_expect` is registered as `fam_file_write_expect` in `cases_lab_helper.py`, which always skips and names `lab_fam_agent_edit_conflict` as the real proof.

Partial accept: the diff card has no per-hunk accept (Up/Down only navigate hunks), so accept is all or none; each case records that as a note. A case that cannot reach its precondition (no action card, or the first Enter already applied the edit when a reject was wanted) is UNVERIFIABLE, never a pass. A clobber of the user's change is a FAIL on every arm, including the helper arm.

Run: `APPCI_FAKE_MODEL=1 python3 script/realapp/appci/run.py --cases 'agent_edit_*'` (agent mode); the lab pair with `--cases 'lab_fam_agent_edit_*'` (needs the lab; `upload` arms need `APPCI_LAB_HELPER_BIN`). Platforms: Linux and Mac; not Windows (`winmap.NOT_WINDOWS`). Self-test: `agent_file_edit_pieces` (fake-model reply, expected-bytes builder, judges).

## Agent-mode latency and the fast steer (#1828)
Three appci cases in `script/realapp/appci/cases_agent_latency.py`, agent pass only (`modes=("agent",)`), all `gui=True`. They need the scripted fake model (`APPCI_FAKE_MODEL=1`, `fake_model.py`) and skip with the reason without it. The fake model gained a **slow steady stream**: any conversation whose user turns contain `STREAMLONG` is answered by 240 six-character pieces 50 ms apart (about 12 s, flushed as produced, sticky for the rest of that conversation) and every `/api/chat` request body is kept in `fake_model.FakeModel.ACTIVE.reqs` (`{"t", "messages"}`), which is the steer oracle.
- **`agent_latency_key`**: key -> first frame built, from the app's own log. A key that never reaches the pty (the agent input) has no echo, so the old `frame` line never fires for it; the app now also logs `input_order hop=key_frame layout=<bool> t=<ms>` for the first frame after any key-down (tiny hook in `crates/warpui_core/src/input_order.rs`, separate flag from the pty-echo `frame` line, which is unchanged). Samples: agent input idle, and agent input while a fake reply streams (only keys typed 0.3-11 s after that request count; the stream is restarted every 30 keys and cancelled with Ctrl-C). Reference: the same measure typed into the **terminal-mode input editor** of the same session (half before, half after the agent phases, so drift shows in `lat.evaluate`'s block-drift rule). Verdict: overhead (agent median minus terminal-editor median) against `APPCI_KEY_BUDGET_MS` (2 ms) by `lat.evaluate` (2 SE; thin evidence is `noref`, never a pass). Also the UI-work rule of `latency_key_log` (`hop=ui_item` event/action items >= 4 ms while typing; frame items WARN). Records: `lat.agent_latency_key/idle`, `.../streaming`. Skips (not passes) on a build without the hook.
- **Render-latency step A: measurement hops, metrics only (`docs/design/render-latency.md`, #1848 #1843 #1838).** No behaviour change; every new field is an `input_order` hop, parsed by `appci/render_hops.py` (`render_hop_metrics`, `record`; self-test `render_latency_step_a_hops_parse`). macOS: `hop=key_presented submit_to_presented_ms= key_to_presented_ms= sync=<0|1> wait_ms=` for each presented KEY frame (submit = the moment `warp_frame_track_drawable` runs, a little before encode/commit, so it slightly over-counts by the encode time; `sync` = `CAMetalLayer.displaySyncEnabled` at submit); `hop=key_frame_sched ms=` = `request_redraw`/`render_scene` of a key-driven frame -> `warp_update_layer` entry (the dispatch_async turn + CA commit hop); `hop=frame_skip_run ... key_frame_skipped=<n>` = how many of the run's skipped frames were key-driven (the #1838 gate is not key-aware). The key tag for the last two is a measurement-only twin of `warpNextFrameFromKey` in `host_view.m` (set by `keyDownImpl`, consumed by `displayLayer:` even when display-sync-off-while-typing is disabled). Linux/Windows: `hop=frame_drawn ... acquire_ms= present_ms=` (`present_ms` = time inside `surface_texture.present()`, 0 when the frame did not present; `acquire_ms` already existed). `latency_paint`, `ctrl_c_flood_ab` (per gate arm, prefix `ctrl_c_flood_gate_<on|off>_`) and `agent_latency_key` (prefixes `agent_idle_` / `agent_streaming_`) record p50/p95/max of each field as metrics (`key_submit_to_presented_ms`, `key_to_presented_ms_hop`, `key_presented_wait_ms`, `key_presented_sync_on/off`, `key_frame_sched_ms`, `key_frames_skipped`, `frame_present_ms`, `frame_acquire_ms`); a note says when a build logs none of them. No new verdicts. Decision rule from the design: submit->presented about one composite period -> option E; key_frame_sched >= 1 ms -> B; Linux present_ms >= 8 under streaming -> F default change. Unverified until the first Mac/Linux run (no cargo here); the `acquire_ms=\S+ present_ms=` shape of `frame_drawn` changed, `cases_agent_latency.FRAME_DRAWN_RE` and `windows/agentkey/an2.py` accept both.
- **`agent_latency_key` present-mode A/B (design option F, Linux only, metrics + notes).** After the judged default run the case relaunches the app per block (`App.extra_env`, new for the Linux launcher; `render_hops.arm_schedule`: two rounds of default / `PHOSPHOR_PRESENT_MODE=mailbox` / `PHOSPHOR_FRAME_LATENCY=1`, first `APPCI_LAT_BLOCK_LEAD` samples of each block discarded) and types keys while a fake reply streams (`stream_keys`). Metrics `present_mode_ab_<arm>` (streaming raw and slot-adjusted key->frame stats, `present_ms`, `acquire_ms`) and `present_mode_ab_deltas` (arm minus default, p50, raw and slot-adjusted; negative = faster), plus one note per arm. A failed relaunch or skipped block is a WARN note (`ArmR`), never a verdict; the final relaunch with the default environment is a real check. `APPCI_RENDER_AB=0` turns the A/B off. macOS and Windows skip it (no such switch / launcher).
- **Round-6 corrections (Linux Wayland agent@fake pass)**: (1) streaming key -> frame is still JUDGED on the raw delta (coordinator review, owner rule: never dismiss bad numbers -- a key that waits for the next streaming frame slot is a wait the user sees, #1843). `paced_key_frame_deltas` (slot wait = previous frame start + median frame spacing, from `hop=frame_drawn`) is reported as the metric `key_to_frame_agent_streaming_slot_adjusted_ms` only, to show how much of the overhead is that wait: in round 6 the raw 10-14 ms was mostly the ~17 ms frame pacing, 2.3 ms median after it (design: `docs/design/agent-input-latency-streaming.md`). (2) `enter_shares` starts at `hop=core_enter stage=handle_window_event` (an Enter an editor binding takes, Linux and Windows, logs no `input_order keydown` line), so the pair was never found ("no Enter keydown -> enter_begin pair"). (3) The steer/Enter judges read the request through `fake_model.clean_messages` (the app appends an `<environment_context>` user message after the prompt). (4) `agent_fault_queued_rows` skips with the reason when the first prompt is not `in_progress` when the rows are typed: `/queue` sends at once with no reply running (no `hop=queued_row`). (5) App: the final-failure note for an unreachable provider (`PreOutputRecoveryExhausted`) is a WARN, not an ERROR (`agent_fault_down`).
- **`agent_latency_enter`**: the app's own share of Enter (Enter key-down -> `hop=enter_begin`) for a prose line typed at 10-25 ms/key with Enter in the same call, idle and while a reply streams (each line typed over the previous line's running reply, i.e. a steer; conversation restarted every 20 lines), against a shell line (`true # tok`) in the terminal-mode editor; budget `APPCI_ENTER_BUDGET_MS` (2 ms). Enter -> the request arriving at the fake model is a metric with a 500 ms sanity bound. Records `lat.agent_latency_enter/idle`, `.../streaming`.
- **`agent_steer_fast`** (#851, by design: input while a reply runs is a steer): three times in a row, while the stream runs, a 200+ character line with shell-looking text (`ls -la | grep x`, `$HOME`, `&&`, a redirect, `rm -rf` as prose) at 10-25 ms/key with instant Enter. Oracles: (1) the fake model's next request has the steer as its LAST user turn, equal byte for byte, in exactly one request, occurring once in it (`steer_verdict`); (2) after the third, the last request holds all three once each, in typed order (`steers_in_order`); (3) the app's `hop=enter_begin` buffer is the steer routed `AI`; (4) no `input_order pty_write` line and no shell command (rec.log) from the first steer on. If the stream had already ended or aged past 4 s when the keys began, the case reports `noref` (precondition not provable), never a pass.
- **Platforms**: Linux, macOS, Windows Git Bash pane (`winmap.GITBASH_ONLY`: the terminal-editor reference and rec.log are POSIX; PowerShell pane skips with the reason). There is no terminal-mode twin of the steer (it exists only in the agent view); the terminal editor is the reference of the other two. Run: `APPCI_FAKE_MODEL=1 python3 script/realapp/appci/run.py --cases agent_latency_key,agent_latency_enter,agent_steer_fast` (agent mode). Pure parts (`key_frame_deltas`, `enter_shares`, `steer_verdict`, `steers_in_order`, the stream reply) are in `run.py --self-test` (`agent_latency_pure_parts`).

### appci shell-depth check (subshell leak, Mac fix tier round 6)

A `subshell_ready_*` case could leave the pane inside a LOCAL bash subshell: its "back in the parent" proof was `echo tok >> file`, which runs just as well in the
inner shell when the `exit` was eaten, and run.py's `where_now()` cannot see a local nested shell (same HOME). Every later case then typed into the inner bash
(`latency_enter`: `print: command not found`, 0/60 timings; `routing_table` lines ran in bash). Fixed in two layers:

- `cases_subshell_ready.py`: every case is wrapped by `@guarded` (leaves its inner shell in a `finally` on pass, fail or skip) and leaves through
  `leave_inner`, which proves the ROOT shell by PID (`Ctx.leave_nested`), typing `exit` only while a probe shows a different shell in front (max 3).
- `run.py`: after every case for which `cases.wants_depth_check` is true (all PROVING cases plus names starting `ssh_`, `remote_`, `subshell`, `lab_`) the shell in front
  must be the pane's root shell. The root PID is recorded (`sh -c 'echo tok:$PPID >> ~/.appci_pid'`, shell-agnostic: fish has no `$$`, PowerShell `Add-Content ~/.appci_pid tok:$PID`) at the first checked case after
  each (re)launch; cmd.exe panes have no PID probe and skip the check. Otherwise `exit` is typed while the PID differs (max 3, never at the root); still nested or no answer
  emits `pane_not_local_after_<case>` and relaunches the app, as the ssh block does. A case that relaunched the app itself skips the check (fresh pane). The probe runs in
  whatever view the pane is in, exactly like `where_now`. Cost: one typed line plus a file poll, about 0.7-1 s per checked case and once per relaunch.
- Self-test: `shell_depth_probe_pure_parts` (probe syntax per pane, parser, which cases are checked, exits only while nested, never at the root).

### Mac round 7 terminal pass: subshell exit keys, vim typeahead, fish probe, zsh newuser (fix/subshell-exit-keys)

- `subshell_ready_rc_questions` / `subshell_ready_vim_typeahead` (leave): after the inner bash exited into the root shell, the typeahead
  mode stayed the inner bash 3.2's `InputMatching` (the mode was set only by a `Bootstrapped` hook, one per app). A young Enter then
  opened a `HandoffReport` gate, the command was written as "unengaged", and nothing ended the gate: every key after it was held for the
  60 s net. APP bugs, fixed: the terminal model remembers each session's mode and puts it back at that session's prompt
  (`typeahead_mode_by_session`), and an unengaged user command now lets the held keys follow it (`on_unengaged_user_command_written`;
  unit tests in `typeahead_handoff_tests.rs`).
- `subshell_ready_vim_typeahead`: the Enter of vim's `:wq` was judged a user line in the shell and cancelled the armed setup
  (`why=UserDrove`). APP bug, fixed: a line ended while a program (leader `Other`) owns the tty re-arms (`on_user_line_ended`,
  `r2_a_line_typed_into_a_program_that_owns_the_tty_re_arms`).
- The PID probe is now `sh -c 'echo tok:$PPID >> ~/.appci_pid'`: fish has no `$$`, so `leave_nested` got "none" in an exec'd fish.
  The self-test runs the probe in every installed sh/bash/zsh/fish/dash.
- `subshell_ready_zsh_newuser_install` skips where an empty-HOME zsh shows no newuser menu (Apple's zsh); the check is made on a pty
  before the case types anything (the case typed `q` at a plain prompt: `qecho ...: command not found`). TEST fix.
- `subshell_ready_secret_after_prompt`: APP bug, fixed: the R8 key-path secret check ran only on the legacy route
  (`write_user_bytes_to_pty_inner`); in table mode the keys were held by row H7 before it, so the typed password was held and later ran
  as part of a command line. `dest_table.rs` now runs the same check before the subshell gates are prepared.
- `subshell_sh_probe` (dash): the failing check is the EXPECTED outcome missing (the case passes when `subshell_setup_unsupported`
  is logged). The app judged dash once (`Wait(CookedNoPrompt)`, tail none, right after the first line's Enter) and never again for
  6 s, so it never wrote the probe at the idle prompt. Open app issue; not fixed here (needs a live tty).
- `subshell_ready_secret_after_prompt` on Linux (round 7): not an app bug. The log shows `Secret` judged 5 ms after the setup write (0 held chunks), then 41 `Pty(Typed)` keys straight to the `read -s`: the typed line (no Enter, by design) and `hunter2` reached the read in typed order as one line. The rc now sleeps 1 s before the `read -s` so the line is typed while the setup is still out and is held, then handed back.

## Paste routing in table mode (#1853)

In `table` mode `paste()` is decided by the table's plan alone (`paste_plan_via_table`); the legacy ssh probe is not consulted, so one phase source (the ssh edge machine) judges the paste and the keys around it. Unit tests in `view_test.rs` (Legacy and Table via `set_destination_mode_for_test`): `ssh_paste_between_held_keys_keeps_typed_order_in_both_modes` (keys, paste, key held together; delivered once, in order, after ownership), `paste_during_a_subshell_setup_is_held_behind_earlier_keys_in_both_modes`, `paste_during_a_powershell_setup_is_held_behind_earlier_keys_in_both_modes`. Written without a compiler; first run is on the build box.

- **Round-7 restart/queue corrections (Linux Wayland agent@fake)**: `agent_restart_quit_*` failed on "unexpected ERROR line: error creating connection to Xorg server" from the RELAUNCH log; this was an APP defect, not the harness (every Wayland launch logged it at line 4, the first launch too, and the relaunch uses the same environment): `windowing/winit/window.rs` now logs it at debug when `WAYLAND_DISPLAY` is set and `DISPLAY` is not. `agent_fault_queued_rows` treats a missing `hop=turn` as the implicit InProgress of a new conversation (`implicit_in_progress`, self-test row `agent_resilience_pure_parts`).

Remote no-helper payloads on stdin (#1864, #1869, #1870; `docs/design/remote-backend-parity-audit.md` findings 1, 6, 7). File chunks, git patches and commit messages no longer ride in the ssh argv or the host's `sh -c` argument (visible in `ps`, kept by audit logs, 128 KiB E2BIG, 32 767 characters on Windows): `ExecRequest.stdin` carries them (`posix_write_payload` base64 for `base64 -d >> "$tmp"`, raw bytes for the `cat` no-decoder fallback, `pwsh_write_payload` read with `[Console]::OpenStandardInput()`; `ShellGitRunner` passes the patch as stdin and `run_commit` uses `git commit -F -`, which also changes the helper's git call). Unit tests: `shellcmd/write_tests.rs::no_content_is_in_any_write_command_and_the_pwsh_one_fits_a_windows_command_line` (the pwsh `-EncodedCommand` line stays under 6 KiB for a 200 KiB file), `backend/shell_git_runner_tests.rs::a_200_kib_patch_stages_and_never_appears_in_the_command_line` and `a_commit_message_is_stdin_not_argv` (real git through a recording `sh`), `backend/contract_tests.rs::a_shell_write_keeps_the_file_content_out_of_every_command_line` (200 KiB write, byte-exact, no command line holds raw or base64 content). `ops_tests.rs` now expects `commit -F -`. Lab: `lab_diff_apply_remote` starts a detached process sampler on the host (patterns in a file, plus a control process so a blind sampler is caught) and fails if a command line ever held the written content raw or base64; skipped (unverifiable) on a pwsh login shell. `run_over_master` now feeds stdin concurrently with collecting output. Not run yet: nothing here has been compiled or run.
