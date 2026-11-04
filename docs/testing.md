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
  skips). The tmux-wrapper variants (`test_tmux_ssh_into_*`, `test_install_tmux_ssh_into_*`) need tmux on the remote
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
| `check_pty_doors` | a raw `write_to_pty(` / `clear_line_editor_and_write_to_pty(` / `write_end_of_transmission_char` / `emit(Event::WriteBytesToPty` outside the reasoned allowlist in the script (#1429-#1436). Allowlisted doors, each with its reason in the script: FOCUS_IN / FOCUS_OUT reports (protocol answers), the raw emit and its wrapper (definitions), the sink `write_user_bytes_to_pty_after_paste_stage` (what the pane queue delivers into), `release_held_keys` (keys the hand-off held, released in typed order), the OSC 52 clipboard-read reply, the subshell bootstrap writers (`write_init_subshell_bytes_to_pty`, `write_subshell_bootstrap_line`: the setup that owns the SubshellSetup gate), the PowerShell StopProfiles ^C (`ctrl_c_during_powershell_setup`), the ssh bootstrap-failure `stty sane` setup line ahead of the held typeahead (`hand_held_ssh_typeahead_to_unbootstrapped_remote`, #1494), the `PtyIntent` dispatcher and the pty controller. Door tests: the `*_door`-style cases at the end of `terminal/view_test.rs` (aws login is a command, synced input honours gates, agent writes refused during setup, editor Ctrl-D through the queue, CLI-agent text held by a gate, ssh warpify payload paced, programmatic insert waits for a waiting Enter, local-child harness command waits as a pending command). `check_queue_access` also runs in the CI `guards` job (#1448). Enter/paste coverage from the Mac run (#1452, #1453): `an_inline_editor_paste_then_an_immediate_enter_are_queued_in_order` (`terminal/view_test.rs`: the editor's inline-read paste is a pane-queue entry via `offer_answered_paste`, with the Enter behind it), `test_enter_behind_an_unhandled_edit_does_not_accept_the_stale_as_you_type_menu` and `test_enter_with_an_as_you_type_menu_and_no_selection_submits` (`terminal/input_test.rs`: Enter never accepts an as-you-type menu that answers older text or has nothing selected; it submits). #1459: in terminal mode the slash menu never swallows Enter unless it lists current rows for the typed text (`test_terminal_mode_slash_path_enter_runs_the_line_1459`; control `test_agent_mode_slash_menu_still_takes_enter_1459`, `terminal/input_test.rs`). appci keeps one `phosphor.<n>.log` per app instance in `logs/<mode>/` (#1457), and `ctrl_c_d` sends a Ctrl-D only after proving the cat / nested bash it ends is running (#1458). Pending-command coverage (`terminal/input_test.rs`): a deferred Enter (verdict / Tab-completion gate) refused because the shell has not processed its first Precmd is held as the pending command and runs once, exactly as typed, when the shell can take it; text typed during the wait never merges into it (#1558, `test_deferred_enter_on_unready_shell_is_held_and_runs_once_as_typed`, `test_text_typed_during_the_verdict_wait_never_merges_into_the_held_enter`, legacy and table); #1564: a DIRECT Enter refused only because the shell is not ready (NotBootstrapped / HistoryNotAppendable) is held the same way (`hold_refused_unready_enter`; the "still starting" toast stays as a notice; a running program and a shared-session participant still refuse): `test_direct_enter_on_unready_shell_is_held_and_runs_once_as_typed`, `test_text_typed_after_a_held_direct_enter_never_merges_into_it` (legacy and table), control `test_refused_enter_while_a_program_runs_is_not_held`. In an appci log, `enter_declined deferred=false held_as_pending=false` on a not-ready shell is the old bug. #1565 (fixed): keys typed into the focused INPUT EDITOR (agent view) during an ssh connect used to never reach `hold_input_during_ssh_connect` (r18cz: every key `ssh_hold=false target=Editor`, no `kh_hold_entry`) because `key_target` read only the already-active hold; the pane's focus facts now count an armed connect gate as the hold, and `legacy` mode arms the gate at the PreInteractiveSSHSession event like `table` (`ssh_keys_typed_in_the_agent_view_editor_during_connect_are_held_and_handed_over_once`, Legacy and Table, `terminal/view_test.rs`); in an appci log, `kh_pane_key ... target=Editor ... ssh_hold=false` during a connect is the old bug; text typed into a pending command is handed back with `TypeaheadHandoff` off too (#1451); `is_pending_aws_login` is set when the login starts, not when requested (#1450, `aws_login_is_a_command_that_waits_for_the_running_program`); the prompt look never takes the blocking model lock on event/timer paths (#1447, `prompt_look_outside_key_path_does_not_block_on_a_busy_model`); the ssh held-back Enter guard only holds the handed-back line (#1396, `ssh_handover_enter_guard_*`). Agent writes during an ssh connect's remote-setup window / subshell / PowerShell setup are REFUSED (logged), not held: the answer belongs to the prompt it was computed for #1561 / #1611 / #1612 / #1621 (owner decision 2026-11-04, `DECLINED.md`): the ssh connect reads NOTHING from the screen any more. The keys held since the submit go to ssh at the event that says ssh OWNS the tty (`CommandBecameInteractive` for the window's block, from the `tcgetpgrp` / `tcgetattr` probe; `TerminalView::ssh_on_tty_owned`), once and in order, and every later key passes straight through; the remote side decides (a shell reads them, a password read's `TCSAFLUSH` discards them). The one guard is a termios read at that event: canonical with echo off (a secret being read) discards what is held, as the tty would (`TerminalModel::tty_reading_secret`, `terminal_attributes::tty_is_reading_secret`). The prompt latch (`ssh_prompt_has_arrived`, the shell-vs-credential shape test, the table's H4 row and `SshPhase::PromptUp`) is deleted; the tests that encoded it were changed to drive the ownership event (`ssh_tty_owned_for_test`, `set_tty_reading_secret_for_test`): `ssh_shell_prompt_of_an_unbootstrapped_inner_host_receives_the_held_keys`, `ssh_secret_read_at_ownership_still_discards_the_held_keys_in_both_modes`, `ssh_ownership_delivers_the_held_keys_and_answers_pass`, `ssh_ownership_ends_the_hold_a_secret_read_discards_a_cooked_read_gets_the_keys`, `ssh_output_does_not_end_the_hold_ownership_does`, `ssh_output_of_any_shape_does_not_open_the_gate_ownership_does`, `ssh_keys_after_ownership_reach_ssh_with_no_burst_window` (Legacy and Table where the scenario has both, `terminal/view_test.rs`); every discard logs its input count (never the bytes). In an appci log, `ssh_held_delivery source=tty_owned` is the release and `kh_finish_begin tty_owned=true secret_read=true` the guard's discard; a `kh_table_drop` after a delivery means keys were lost (no row drops a held ssh input any more). #1571: an Enter refused only because the shell is not ready is a GATE (`Input::gate_unready_enter`, an `EnterVerdict` gate keyed with `UNREADY_ENTER_KEY_FLAG`, no queue cap): what is typed after it (text, Enters, and editing keys in their `Held` form) waits behind it in order, the editor never merges it into the held line, a second Enter is its own entry, the readiness event (`SessionBootstrapped` / the first Precmd, `retry_held_unready_enter`) runs the held line once and releases the rest like any Enter that ran; a deferred Enter's verdict gate is retagged in place; timers never run it (`execute_pending_command_after_timer`); its 15 s net (`UNREADY_ENTER_GIVE_UP`) hands the line and what followed back to the editor as text with a toast, never runs it; ^C voids it (`test_unready_direct_enter_then_a_second_line_and_enter_run_as_two_commands`, `test_unready_deferred_enter_keeps_both_later_lines_behind_it_in_order`, `test_unready_enter_give_up_hands_the_lines_back_and_runs_nothing`, `terminal/input_test.rs`; `nested_ssh_with_late_readiness_runs_each_line_once_in_order_in_the_right_shell`, agent mode, `terminal/view_test.rs`; all Legacy and Table). In an appci log: `enter_held_gate` at the refusal, `enter_queued_behind_held` for later Enters, `enter_held_retry` / `enter_held_ran` at readiness; `enter_held_give_up` is the net (lines handed back, nothing run). #1572: ONE readiness predicate, `Input::shell_unready_for_submit` (not bootstrapped, history not appendable, no Precmd, no session id; a running program is not "unready"), decides both the held Enter and the ssh hand-over (`deliver_ssh_typeahead_if_ready` waits on it whether or not keys are held yet), so a managed remote hands over at `SessionBootstrapped`, not at its first Precmd; an unmanaged inner shell keeps the #1561 rule. #1579: a connect window armed at preexec (an ssh the submitted text did not show, e.g. an alias) inherits the submit's origin (`note_block_started_for_ssh`), so `legacy` mode holds the agent-view editor's keys too (`ssh_window_armed_at_preexec_for_a_users_submit_holds_the_agent_view_editor_keys`, Legacy and Table; control `ssh_submit_origin_is_consumed_by_the_block_it_started`). #1571 review fixes: the hand-back text of an unready Enter's give-up and of a deferred `VoidEnterGate` keeps pastes, Enters (as line breaks) and the editing keys' effect (`text_of_entries_typed_after`), and a multi-line hand-back arms the ssh hand-back Enter guard (one Enter would submit it as ONE command); the ssh hand-back never discards (a running block: kept on the input and restored, guard armed, when a block completes; a non-empty editor: appended on its own line); the first-Precmd wait cap hands the held keys back instead of writing them; the submit origin is consumed before the in-band early return (`test_unready_enter_give_up_keeps_a_paste_typed_behind_it`, `test_blind_enter_after_an_unready_give_up_is_held_by_the_guard`, `test_ctrl_c_voids_an_unready_enter_and_its_gate`, `terminal/input_test.rs`; `ssh_first_precmd_wait_cap_hands_the_keys_back_and_writes_nothing`, `ssh_origin_of_an_in_band_submit_does_not_leak_to_a_later_raw_block`, updated `ssh_held_line_is_never_glued_to_existing_editor_text`, `terminal/view_test.rs`; all Legacy and Table). |
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
| `test_bash_bootstrap_nounset`, `test_zsh_bootstrap_strict_options` | hooks survive `set -u` / `err_return` / failing user precmd (#825, #836, #844); a detached HEAD under strict options reports git's hash every prompt (#1616) |
| `test_fish_bootstrap` | fish hooks fire end to end in a pty (#829, #879) |
| `test_bootstrap_hook_equivalence` (bash/zsh/fish), `test_nu_bootstrap_hook_equivalence`, `test_pwsh_hook_equivalence` (+ `testdata/pwsh_hooks_golden.json`) | fork-free hook helpers emit byte-identical DCS payloads to the old forking code (#1170, #1233, #1231); a detached HEAD runs git once per directory and HEAD commit, a moved HEAD is seen at the next prompt, `PHOSPHOR_GIT_HEAD_CACHE=0` restores git per prompt (#1616) |
| `test_bash_bootstrap_builtins` | fork-free `sort -V`/os-release/`shopt` replacements match the commands (#1164) |
| `test_bash_preexec_large_command` | Preexec escaping of a 3 MB command is linear (#1255) |
| `test_init_shell_escaping` | InitShell JSON round-trips hostile user/host values (#879) |
| `test_hijack_bootstrap` | user `echo/printf/cd/read/pwd` functions never run inside the bootstrap (#929) |
| `test_line_editor_markers` | `LineEditorReading` / `ContinuationPrompt` OSC markers and `seq` per Precmd (#1390-#1392) |
| `test_local_counted_bootstrap` | counted-read bootstrap behaves identically to the here-doc form (#1226) |
| `test_ssh_keepalive`, `test_ssh_probe_cache`, `test_ssh_wrapper_forks`, `test_ssh_raw_bootstrap_hook` | ssh wrapper options, probe cache, fork budget, `raw_bootstrap` hook (#1171, #1136, #1172, #1243) |
| `test_ssh_auth_once` | a failed ssh login (master step 255, no control socket) or a Ctrl-C at its prompt runs ssh ONCE and returns its status, so no second password prompt eats the next command (#1626); a ssh that authenticated but cannot multiplex (socket created, then 255) still falls back to the user's plain ssh once (#1551); bash/zsh/fish |
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
| `integration-linux` | test | manual, allow_failure | `crates/integration` under xvfb, 3 shards, `--profile ci`; sets `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` and checks `sshd` / `ssh-keygen` first (the ssh scenarios use a local sshd, 2.2) |
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
| `PHOSPHOR_SSH_EARLY_FLUSH_KNOWN=0` | stash the bootstrap until `check_binary` returns even for a host already known to have the extension (#1172) |
| `WARP_SSH_PROBE_TTL=<seconds>` | lifetime of the cached non-POSIX probe verdict (default 600; 0 disables the cache); the cache file is shared by bash, zsh and fish tabs (#1172) |
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
| `PHOSPHOR_HEAP_TRIM=0` | no heap trim at all (glibc `malloc_trim`, macOS `malloc_zone_pressure_relief`; the close-time trims included) |
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
| no size above which input is NOT classified (#1604; the 4096-byte prefilter cap, the parse-cap abandon and the 16 KiB #968 cap sent every over-size shell command to the agent in agent mode): a 5400-byte shell line, a 5000-byte 40-line script and a 20 KB word paste each route exactly as their first line does. Text over `MAX_SYNC_ROUTING_BYTES` (4 KiB) is routed by the same linear rules in a background job (`start_detection_for_unparsed_buffer`; started from the edit path up to `MAX_EAGER_ROUTING_BYTES` 256 KiB, else by the Enter), the Enter waits for that verdict with a compute allowance of 1 ms per 4 KiB (`large_text_compute_allowance`; the backstops grow the same way), the give-up hands the text back (#1577) and never routes it, and for an over-cap buffer keeps the request and its job so the NEXT Enter waits for or submits on the verdict (`abandon_pending_detection_after_give_up`); a buffer under the parse cap never gets a parse-less job (its parse delivers the verdict) and a parse-less snapshot's head is read from a bounded window of the text (`head_word_from_text`); the model classifier sees at most `MAX_MODEL_INPUT_BYTES` (4 KiB) of the text; the history close-match is ruled out by length before the matcher indexes the buffer. The completer parse (quadratic) stays capped at `MAX_BACKGROUND_JOB_BUFFER_CHARS`: over it there is no syntax decoration, alias expansion or completer autosuggestion, and that is all | `test_ready_paste_then_immediate_enter_classifies_the_pasted_line` (the three over-size cases), `test_enter_on_oversized_buffer_waits_for_its_background_verdict_and_runs_the_shell_line`, `test_enter_on_20k_char_paste_is_classified_without_a_parse_and_never_swallowed`, `test_paste_of_a_large_buffer_and_the_keystroke_after_it_are_linear_in_its_size`, `test_second_enter_after_a_give_up_on_an_over_cap_shell_line_runs_it_in_the_shell` (`terminal/input_test.rs`); `huge_buffer_is_classified_like_a_short_one_in_the_background`, `enter_verdict_budget_grows_with_the_text`, `history_close_match_is_ruled_out_by_length_before_the_matcher_is_built`, `a_short_line_without_a_parse_is_classified_from_its_text` (`ai/blocklist/input_model_tests.rs`); `test_oversized_buffer_keeps_detection_pending_and_is_classified_in_the_background`, `test_short_buffer_gets_no_parse_less_detection_job` (`input/decorations_tests.rs`); `long_lines_route_like_their_short_forms` (`routing_corpus_tests.rs`), `test_routing_rules_are_linear_in_the_text` (1.28 MB vs 128 KiB ratio <= 30x, `util_tests.rs`), `test_gate_bounds_what_the_inner_classifier_sees` (`safety_gate_tests.rs`); release-build bound: `routing/*` benches, gated at the allowance |
| in agent mode a lone word nothing on the session answers to (not installed, no shell syntax, not a curated command word or a path) goes to the agent, whether the session knows it is absent or cannot say, at the keystroke and at Enter, in a fresh conversation and after an agent turn, in every shell family; installed, the same word runs in the shell, an explicit path runs, and clear shell structure around an unknown head still runs (#1290, owner decision #1560); outside a conversation the classifier decides | `lone_unknown_word_in_a_conversation_goes_to_the_agent` and the #1290 corpus cases (`XTOOLS`, `frobxone`; `routing_corpus.rs`), run by `corpus_against_the_routing_procedure` (`routing_corpus_tests.rs`) |
| in an agent conversation a command-headed line with an English contraction (`it's`, `don't`, `I'm`; `'` or `’`) is a sentence and goes to the agent (#1624: `what is in the file that I just mentioned T169_6rjko, it's short` with macOS `/usr/bin/what` ran in the shell, because the `'` made the quoting unsafe and the evidence / sentence tests read that as "no verdict" / "a quoted argument"); shell syntax still wins (#1560: `what -s file.c` runs), one-off keyword and free-text heads keep their route (`echo it's done`), an apostrophe inside double quotes is literal (`mv "the team's notes.md" and the backup` runs), a typographic `’` counts only with another prose word (`ls Josh’s` runs), and the plain terminal is unchanged (installed head runs) | the #1624 corpus cases (`routing_corpus.rs`), run by `corpus_against_the_routing_procedure` and, with their `v1_xfail`, `corpus_against_the_current_procedure` (`routing_corpus_tests.rs`) |
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
| view_test (ssh bootstrap timeout, #1494) | `view_test.rs` (`ssh_bootstrap_failure_delivers_held_typeahead_after_stty_sane`, `ssh_bootstrap_failure_with_a_dead_shell_discards_the_held_typeahead`) | when the ssh bootstrap timer fires with typing held and the remote shell alive, a space-led `stty sane` line goes to the pty, then the held text through the normal hand-over, once, in order, nothing local, with an accurate toast; a dead shell discards the held text with a log | every test run |
| view_test (bootstrap timer vs progressing session, #1566) | `view_test.rs` (`ssh_bootstrap_timer_racing_the_bootstrap_precmd_does_not_fail_the_session` Legacy+Table, `ssh_bootstrap_timer_second_expiry_after_the_grace_still_fails_the_session`) | the bootstrap give-up timer defers once (3 s, logged) when the pending session already showed its bootstrap `Precmd`; the held line then reaches the pty exactly once at the first remote prompt, editor empty, no `stty sane`; a second expiry still runs the #1494 path. In an appci log, `Bootstrapping failed` shortly after a remote `precmd_seen` and before `Received Bootstrapped hook` is the old bug | every test run |
| unit + view_test (destination table, step 3: the shadow comparison, #1424) | `input_queue/shadow_tests.rs` (`agreement_reports_nothing`, `a_mismatch_reports_row_both_classes_and_flags`, `suspend_and_quit_differences_are_tagged_od1`, `a_signal_held_in_the_ssh_setup_window_is_tagged_od5`, `a_silent_drop_of_hidden_input_is_tagged_b3`, `the_log_line_has_no_content_of_the_input`, `every_outcome_has_a_class`, `the_leaf_is_the_first_one_noted_*`, `the_test_override_nests_and_restores`), `terminal/view_test.rs` (`shadow_mode_changes_no_destination_*`, `shadow_reports_a_legacy_table_disagreement_*`) | `PHOSPHOR_QUEUE_DESTINATION=shadow` compares the table with the destination class the legacy chain reached at its leaf (typed text, control keys, signals, escape sequences, the replayed Enter / held key) and logs `kh_dest_disagree row=.. legacy=.. table=.. flags=..` (`input_order`, debug; counts and flags, never text) with `expected=OD1/OD5/OD7/B3/B7/B13` tags for known causes; agreement logs nothing; shadow changes no behaviour. Tests force the mode on their own thread with `set_destination_mode_for_test` (never `std::env::set_var`) | every test run |
| view_test (destination table, step 4: delivery through the table, the #1415 fix, #1424) | `terminal/view_test.rs` (`legacy_mode_delivers_replayed_text_through_the_chain`, `shadow_mode_delivers_replayed_text_through_the_chain`, `table_mode_delivers_replayed_text_without_the_legacy_chain`, `table_mode_text_replayed_after_an_ssh_recognised_at_preexec_only_is_held_not_run_locally` (the mixed-mode case: lazy arming of the connect gate by the delivery), `table_mode_text_replayed_into_a_new_ssh_connect_is_held_then_delivered_once` (also asserts no key-path model lock), `table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover`, `a_held_signal_is_discarded_when_the_connect_ends_without_a_remote_shell`, `table_mode_facts_never_wait_on_the_handoff_lock`), `input_queue/facts.rs` (`mode_parses_like_the_pane_key_switch_and_defaults_to_table`) | `PHOSPHOR_QUEUE_DESTINATION=table` (forced per test thread by `set_destination_mode_for_test`): replayed Text / Enter and every ssh-held input (`EntryKind::Held`, five classes) are decided ONCE by `table::decide` and executed with the leaf writers; the legacy chain is not called (`dest_table::legacy_chain_deliveries` is the discriminator: it moves in legacy and shadow, not in table); an ssh recognised at preexec only is held for the connect, never run locally; a `HELD_SIGNAL` entry is delivered to the pty once after the keys before it and discarded (not shown, not run) when the connect ends without a remote shell; the H4 drop is executed; facts never wait for the hand-off lock. The default mode is `table` (owner, 2026-11-02); `PHOSPHOR_QUEUE_DESTINATION=legacy` is the kill switch | every test run |
| unit + view_test (destination table, steps 5 and 6: arrival of terminal keys, the H8 editor column, the origin rows, paste routing, #1424) | `input_queue/tests.rs` (`the_h8_matrix_agrees_with_the_editors_hold_decision_for_every_gate_set`: all 16384 gate sets x editor entry queued or not), `input_queue/table_tests.rs` (the H8 matrix has a class column: an editor key that submits or completes is never exempt), `editor/view/paste_hold_tests.rs` (`the_matrix_class_of_an_editor_key_is_the_legacy_submit_or_complete_split`), `input_queue/shadow_tests.rs` (`the_editor_matrix_comparison_reports_only_a_difference`, `editor_text_past_a_gate_is_the_tagged_difference_b14`), `terminal/view_test.rs` (`table_mode_arrivals_reach_the_same_destinations_as_legacy` runs five scenarios in `legacy` and `table` and compares every pty write, the editor text and the PowerShell gate's bytes; `only_table_mode_decides_live_keys_with_the_table`; the #1179 agent-driven typing and the live Enter (OD-2) in each mode; `table_mode_typing_after_the_user_takes_over_an_agent_command_reaches_the_pty_once`; `table_mode_text_the_pty_does_not_take_is_kept_in_the_editor_even_when_the_box_is_hidden` (OD-4); OD-1: `table_mode_ctrl_z_cancels_a_paste_being_read_and_reaches_the_pty_once` against `legacy_mode_ctrl_z_waits_behind_a_paste_being_read` and `queue_signal_classification_follows_the_mode`; OD-5: `table_mode_a_ctrl_c_in_the_ssh_setup_window_is_held_and_reaches_the_remote_shell_once` against `legacy_mode_a_ctrl_c_in_the_ssh_setup_window_is_dropped_with_the_notice`; H8 strict FIFO: `table_mode_text_typed_behind_a_gate_waits_as_a_neutral_entry_and_is_routed_when_it_opens`, `table_mode_a_control_key_behind_a_gate_is_delivered_at_the_gates_event`, `table_mode_a_key_that_waited_in_the_mailbox_keeps_the_stamp_it_had_when_typed`; the origin rows: `table_mode_the_synced_origin_row_refuses_a_pane_with_no_running_command`, `table_mode_synced_input_is_written_or_held_as_legacy_does`, `table_mode_keys_typed_during_a_paced_cli_agent_submit_follow_the_submit_in_order`, `{legacy,table}_mode_a_cli_agent_submit_during_a_powershell_setup_is_held_and_delivered_once` (#1601: an open PowerShell setup holds the submit as legacy does; released once, in order, at the prompt after Bootstrapped), then #1601b: the O-cli-submit row states what the legacy writer does with the bytes -- a PACED submit (its `CliSubmit` gate is open) is held by nothing but the PowerShell setup, an UNPACED one waits behind the Enter-verdict / tab-completion gates (H5), the subshell setup and bootstrap write (H7), any other open gate but an ssh-only set (H8) and queued entries (H8-queue), and both consult the typeahead hand-off (`CONSULT_ORACLE`; the shadow counts a hand-off hold as `Pty`): `table_tests.rs` (`a_paced_cli_submit_is_exempt_*`, `an_unpaced_cli_submit_*`, `origin_exemptions_hold`), `table_enum_tests.rs` and `fuzz/fuzz_targets/route_decide.rs` state the same rule, and `view_test.rs` runs the submit paced and unpaced behind an Enter-verdict gate, a subshell setup and a paste being read: `a_cli_submit_behind_a_gate_is_held_unpaced_and_written_at_once_paced_in_both_modes` (delivered bytes and order in `legacy` and `table`) and `shadow_mode_reports_zero_untagged_disagreements_for_a_cli_submit_behind_a_gate` (single and chained, two orders), `table_mode_the_agents_write_is_refused_during_a_setup_and_goes_out_otherwise` (row D13 at the agent's writer); paste: `table_mode_paste_routing_agrees_with_the_legacy_routing`, `table_mode_paste_routing_sends_a_paste_to_the_agents_input_while_it_drives_the_command`; the equivalence tests `shadow_mode_reports_no_untagged_disagreement_over_the_arrival_scenarios` (nine scenarios: running command, a block started but not yet long running, idle prompt, ssh connect and interrupt, PowerShell setup, the other origins' writers, the agent's writes, the navigation keys, text behind a gate) and `shadow_mode_agrees_on_typing_during_an_agent_driven_command` drive the real view chain and fail on any `kh_dest_disagree` line that is not tagged `expected=`) | in `table` mode a live key (text, keydown, control sequence) is decided by the table with the stamp taken first and the queue's own facts; the idle path is `Pty` executed at once; a held key is a destination-neutral queue entry routed again when its gate opens; ^Z / ^\ jump the queue; a signal in the ssh setup window is held and delivered remotely; the writers of other origins and the paste route consult the table; `shadow` proves the same destinations with zero untagged disagreements. Forced per thread with `set_destination_mode_for_test`, never `std::env::set_var` | every test run |
| unit + view_test (destination table, step 7: the edge-driven ssh phase, #1348, #1349, #1490, #1424) | `input_queue/ssh_phase_tests.rs` (every edge: submit, connect start, ssh owning the tty (`TtyOwned`, #1611/#1612/#1621: the pre-ownership hold ends there and never on output), observed facts, block completed, bootstrap failed, swallow give-up, hold started/ended, released by an event or a named give-up), `terminal/view_test.rs` (`shadow_mode_the_ssh_phase_machine_agrees_with_the_derivation_through_a_connect`, `table_mode_decides_on_the_machines_phase_not_the_legacy_fields`, `table_mode_the_remote_prompt_event_hands_over_with_no_timer_and_no_give_up` (asserts `ssh_give_ups == 0`), `table_mode_a_gate_past_its_ceiling_is_released_by_a_logged_give_up`, `table_mode_the_give_up_ends_a_window_nothing_is_held_behind`, `table_mode_a_secret_read_at_ownership_discards_the_held_keys_and_the_next_key_reaches_it`, `a_non_ssh_command_starting_releases_the_window_by_event`, `legacy_mode_does_not_feed_the_ssh_phase_machine`), `model/typeahead_handoff_tests.rs` (the ConPTY enter window: `keys_typed_between_the_enter_and_the_next_ready_signal_are_held_and_released_in_order`, `a_command_that_becomes_interactive_gets_the_typeahead_in_order`, `a_running_programs_own_enter_never_opens_the_window`, `a_ready_signal_before_the_precmd_does_not_end_the_window`, `the_enter_window_gives_up_for_its_own_keys_and_the_hold_stays_on` (#1573), the kill switch and platform tests), `input_queue/tests.rs` (`an_owner_change_edge_clears_the_line_pin_at_the_next_lock`) | the ssh phase is fed by the events that are its edges and `table` mode (the default) carries it in the facts with no ssh refresher on the key path; `shadow` reports `kh_ssh_phase_disagree`; the hand-over runs inside the remote prompt's own handler and every timer left is a counted, logged give-up (`kh_ssh_give_up`); on ConPTY keys typed from an Enter until the next line-editor-ready signal are held and released in order (typeahead for an interactive program is released to it by the latch event); `PHOSPHOR_CONPTY_ENTER_HOLD=0` is the kill switch. The focus epoch and the pin clear follow focus and owner-change edges; `EditorFocusPending` and `PtyRouteSettling` (no opener) are deleted | every test run |
| unit (busy queue, operations that RETURN bytes, #1446) | `terminal/view_test.rs` (`a_powershell_take_under_a_busy_queue_*`, `a_deferred_enter_gate_take_under_a_busy_queue_*`, `an_ssh_gate_drop_under_a_busy_queue_is_not_lost`) | the take (deferred Enter gate keys, PowerShell held bytes, ssh gate drop, void Enter text) and the caller's continuation are one `Deferred` variant: nothing returns empty when busy, the ssh gate is not left set | every test run |
| insta | `.snap` files under `app/src/terminal/snapshots/`, `crates/warpui_core/src/snapshots/` | per-shell bootstrap lines, pane-key routing tables, input_order trace shapes | every test run; review diffs with `cargo insta review` on the box |
| expectrl | `app/tests/real_shell_markers.rs` | real bash through a pty: marker order (Preexec, Precmd, LineEditorReading with the same seq) | every test run of `-p warp` (skips without bash) |
| shuttle / loom | `warpui_core::sync_shim`; `model_tests::*` in `clipboard_model_tests.rs`, `key_facts_model_tests.rs`, `typeahead_handoff_model_tests.rs` | thread interleavings of the clipboard write queue, KeyFacts, PaneHandoff, the reader cap; the two-cell publication gen (`model_the_reader_retry_never_combines_gens`, `model_the_gen_is_monotonic_across_loads`, `model_a_stamp_taken_after_seeing_the_prompt_is_not_before_it`: a consistent read never mixes publications, a stamp is never from the future, a stamp taken after the prompt was seen is never classed before it) | `test-shuttle-loom` job (release tier) |
| shuttle / loom (input pipeline, #1512 #1531 #1424) | `app/src/terminal/model/pipeline_model_tests.rs` (wired from `key_facts.rs`; the module name ends in `model_tests`, so the job's `model_tests::` filter selects it): `model_pane_queue_drain_never_loses_reorders_or_duplicates_a_key`, `model_ssh_hand_over_keeps_every_held_key_in_order_and_delivers_it_once`, `model_handoff_holds_start_and_end_once_and_never_strand_a_key`, `model_a_refresh_racing_a_publication_never_moves_the_gen`, `model_the_decision_is_never_older_than_the_stamp_it_follows` | `PaneQueue`/`InputQueue` are `Rc` (main thread only), so the two queue models are faithful minimal copies of the protocol in which every point another operation can run at (inside a drain, between the drain's end and the mailbox flush, between the ssh gate closing and its drain finishing) is a lock release; each names the code it mirrors (`pane.rs` `draining`/`defer`/`try_lock_ordered`/`begin_deferred_flush`, `ssh_queue.rs` hold/hand-over/drop). They assert the owner's invariants: every key is delivered exactly once, in arrival order, to one destination (or, in the ssh model, discarded because the connect was dropped while it was held), nothing is left waiting, nothing is delivered behind an open gate. The hand-off model runs the REAL `TypeaheadHandoff` behind `PaneHandoff` with a model thread, an editor submit and the key path: a `Held` answer always has a live hold, a signal is never held, every hold the key path saw start has an end recorded for the view. The gen models: a main-thread refresh never moves the gen, and a decision read is never older than the stamp taken before it. Loom bodies are sized down with `cfg!(loom)` | `test-shuttle-loom` job (release tier) |
| unit (enumerated totality, #1531 item 3) | `input_queue/table_enum_tests.rs` (wired from `input_queue/facts.rs`): `decide_is_total_and_consistent_over_the_enumerated_facts`, `every_row_of_the_table_is_reachable`, `row_names_are_unique_and_the_list_is_complete` | ~860 000 samples: the booleans the rows branch on x ssh x subshell phases x every class x origins, every gate kind alone and next to the ssh gate, and a fixed-seed sweep over all of `RouteFacts`/`Intent`/pin. `decide` never panics and is deterministic; every drop names a row that can give it; a refused pty write drops only a typed write; signals never wait; the oracle is consulted exactly for entries not released from a hand-off gate; the editor never reaches the pty; a PACED CLI-agent submit is held only by the PowerShell setup, an unpaced one by the gates a key waits behind (#1601b); every `Row` is decided by some sample (a row nothing reaches is dead) and the `ALL_ROWS` list is the whole enum (an exhaustive match). If a new `Row` is added the match fails to compile until it is listed | every test run |
| criterion | `app/benches/key_path_bench.rs`, `ansi_bench.rs`, `routing_bench.rs` (`--features dev-support`) | per-key cost (idle, behind a gate), routing, hand-off, 1 MB ANSI parse, input-type routing of 1 MB / 10 MB text (#1604; limits at the Enter's compute allowance) | `bench` job (release tier) |
| cargo-fuzz | `fuzz/` (own workspace; targets `ansi_parser`, `hook_parsers`, `input_queue`, `route_decide`, `key_facts_cell`, `typeahead_handoff`) | crashes on hostile terminal output, hook frames, queue op sequences; `route_decide`: `table::decide` over arbitrary `RouteFacts`/`Intent`/pin never panics and every plan keeps the table's properties (determinism, drop reasons name their rows, signals never wait, oracle consulted, editor never reaches the pty, CLI submit never held); `key_facts_cell`: `KeyFactsCell`/`KeyModelCell` operation sequences against a reference model, the reader reports a consistent pair exactly when the gens agree and a torn pair one publication behind; `typeahead_handoff`: arbitrary hook/key/editor event sequences on the hand-off never panic, a `Held` answer has a live hold, a signal is never held, a hold that starts is reported and a hold that ends leaves an end for its gate | `fuzz-smoke` job (release tier) |
| cargo-mutants | `.cargo/mutants.toml`; jobs `mutants` (pane_key, key_facts, the #1424 destination table: input_queue gates/table/facts/mod/pane/shadow and view/dest_table.rs) and `mutants-warpui-core` (clipboard) | lines the tests do not actually check | release tier only; widen with `MUTANTS_FILES` / shard with `MUTANTS_SHARD` |

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
| focus guard | focus the newest Phosphor window (X11) | frontmost app NAME and PID == the started PID, refuses Cmd-Q/W/Tab/Space/H/M | `GetForegroundWindow` + `GetWindowThreadProcessId` == the started PID before every burst (and every key of a long one); refuses Win-key / Alt-F4 / Alt-Tab chords |
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
an unclassified case is reported as such. After `mode_proof` (whose pre-key screenshot must come first) a key-delivery proof types a marker command and confirms it arrived (VK events,
then Unicode packets; `key_delivery` row); if neither lands the pass stops with `keys not delivered: <reason>` and the session / foreground diagnostics.
Each also has a `-reuse` twin that tests an earlier app-identical build (2.9). Heavy and GUI-driving: all four jobs are **manual, allow_failure**, need the platform's build job (`needs:` + artifacts) and
`script/ci/ci_seq.py` plays them after each successful build, so every numbered build runs them. The play-time variable
`APPCI_TIER` selects the size: `smoke` (fix tier) or `full`/empty (candidate, release; everything below). Smoke = the cases
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
| `app-test-linux-x11` | box | private Xvfb (software GL) | `xdotool`, `xsel`, `import`/`ffmpeg` |
| `app-test-macos` | `Mac` (user `gitrunner`) | must be the logged-in Aqua session | `cliclick` + `osascript`, `screencapture`, `pbcopy`; frontmost-app guard before every burst; `caffeinate` |
| `app-test-windows` | `windows` (console desktop, the runner is NOT a session-0 service) | the interactive desktop | `SendInput` (foreground-PID-guarded), `PIL.ImageGrab`, via the shared Python harness; side-by-side install; backup + md5-verified restore of app data, uninstall key, PATH, shortcuts, `phosphor://` handler; `WINPUSH.lock`; exact-PID stop |

Session handling (Linux): own short `XDG_RUNTIME_DIR` (falls back to `/tmp/pa-*` when the job dir is too long for the 108-byte socket
limit), private `HOME` per mode with an owner-like scratch config and **no credentials**, app under its own `dbus-run-session`, a local ssh
server on 127.0.0.1 (system `sshd` as the CI user with a `ForceCommand` into a scratch remote home; else `fakesshd.py` when paramiko
exists; else the ssh cases SKIP). OpenSSH reads `~/.ssh/config` from the passwd home, not `$HOME` (#1481), so `profile.py` writes `ssh`/`scp`/`sftp` wrappers into `<home>/bin` (first on PATH in the rc files) that exec the real binary (absolute path resolved at setup) with `-F $HOME/.ssh/config`; after the profile is built `run.py` probes `ssh fake true` in that environment and, if it fails or hangs, every ssh case SKIPs with ssh's stderr as the reason. The ssh cases type `exit` only after the token reached the REMOTE file (`Ctx.leave_remote`; it also fails fast when the count of shells under the app, from `ps`, drops, since the app logs nothing when the local shell exits); otherwise they record the miss, reset the pane and stop their trials, so a missed connect can never exit the local shell. Every process is recorded in `pids` and torn down by exact PID / process group, never by name.
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
- **Job log.** Each row's FIRST failing message (or a `noref` row's `NO-REFERENCE` reason) is printed in full; the rest of the detail
  stays capped at 200 characters; the Ollama endpoint is redacted as before.

Cases (each in both modes unless noted): `mode_proof` (screenshot + OCR when tesseract exists), `cadence` (real key spacing measured at
the pty), `fast_typing` (10-25 ms/key, Enter in the same call, gaps 0 and 250 ms), `rollover` (next key down before the previous is
up), `expert_fuzz` (seeded typos fixed with BackSpace / Left+insert / Ctrl-W), `paste_instant_enter`, `arrows_behind_paste` (#1389),
`big_paste` (`APPCI_BIG_MB`, default 1, byte-exact), `tab_enter` (gaps 0/50/150 ms, no stray uncompleted file), `ctrl_c_d` (Ctrl-C
delivery latency, Ctrl-D in `cat` and in a nested bash), `read_x`, `subshells` (bash twice incl. typing during setup, zsh, fish, nu where
installed), `alt_screen` (less, vim), `ssh_connect_typing` (0/100/200/300 ms after Enter, `APPCI_SSH_TRIALS` per gap (default 3 = 12 trials) or `run.py --ssh-trials N` / `APPCI_SSH_TOTAL_TRIALS` for N trials in TOTAL spread over the four gaps, exact remote
command, nothing local; #1127: the result carries `metrics.loss` and a note with the observed loss rate and its one-sided 95 % Clopper-Pearson upper bound -- 0 lost of 40 only bounds the rate to 7.2 %, excluding the original 1/40 (2.5 %) needs 119 clean trials), `ssh_enter_then_immediate_command` (#1415: `ssh fake` Enter and the next command with no pause), `ssh_nested_typeahead` (#1421: a nested ssh and the line behind it, typed during the first connect; runs once, remotely), `ssh_failing_not_local`, `idle_cpu`, `latency_enter` (vs a reference terminal in the same session),
`latency_key_log` (key to first frame from the app's own `input_order` log; also, informational only, #1176: `key_to_pty_write_ms` (keydown to the event loop's `pty_write` line, target <= 1 ms median), `typing_ui_items` (`hop=ui_item` >= 4 ms) and `typing_lock_holds` (model lock held >= 2 ms) over the same typing), `keydown_count` (#1159, full tier only, both modes, every platform incl. both Windows panes: `ctx.count(20)` lines of `echo <12 lowercase letters> >> kd.txt` at 12-30 ms/key, alternating plain typing and `be.roll` (overlapping key-down/key-up on macOS htype2 `r:`; Wayland/X11/Windows use their own `roll`); asserts the app log's plain `input_order keydown` count (ctrl/alt/cmd presses and bare modifiers excluded) EQUALS the keys sent (characters + Enter), no same-key repeat < 2 ms apart (the #1159 signature was an extra keydown 0.4 ms after the real one), and the received text exact (effect file, plus rec.log where the pane has one)), `latency_first_key_after_idle` (#1160, full tier only, not in SMOKE: the FIRST key after 1, 2 and 5 s of no input, `ctx.count_lat(30)` (>= 30) kept samples per gap plus discarded warm-up. macOS: kl3 key->paint for Phosphor AND Terminal.app, A/B in blocks of 10 samples with the first side alternating per gap (not per sample: a focus switch between the two windows is itself an activation), verdict per gap on the overhead against `APPCI_IDLE_KEY_BUDGET_MS` (default 5 ms, owner 2026-11-03) with the SE-aware PASS/FAIL/INCONCLUSIVE of `lat.py`; Linux and the Windows Git Bash pane: key->frame from the app log with the SAME session's next key (0.3 s later, not idle) as the control, so the verdict is the idle penalty; Windows PowerShell pane skips), `approval_card_pause` (agent, needs a provider),
`tabs_two_windows` (`APPCI_TABS`, default 12 streaming tabs, a second window, typing and paste). Full tier, module `cases_caseagent.py` (#1463; Linux/macOS backends; the AI ones skip without `ci.env`, the OCR sub-checks are notes without tesseract): `agent_stream_cancel` (agent; a long reply streams, text typed at 10-25 ms/key lands in the input with no new request and nothing in rec.log, Ctrl-C stops the stream: the screen stops changing; request/stream-open times are metrics), `agent_approval` (agent; nothing runs before approval, the first Enter on the empty input only focuses the card, the second approves: the command is in rec.log and its effect file exactly once; a follow-up question in the same conversation opens a new model request; a rejected command never reaches rec.log), `routing_table` (both modes; 10 installed-command lines must reach the shell with the exact line in rec.log, 6 prose lines go to the agent (terminal mode per `nld_in_terminal_enabled`), 3 installed-head English lines follow the documented rule by the surface the `phosphor::routing` enter-stage log reports; misroutes both ways fail, the table is in results.json; #1593: a line whose shell-or-chat verdict missed its budget stays in the editor with the "press Enter again" toast (`enter_verdict_give_up ... verdict_cap` / `young_verdict_cap`, `kh_enter_gate_give_up by=verdict_cap`): its row carries `verdict_give_up`, the check names the A5 toast instead of "LOST", and the `verdict_give_ups` metric counts them, so the agent-mode round shows whether the budget is too tight). Windows runs the PowerShell subset
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
`agent_code_recycle` (both modes, #1267, needs the CI model: one answer with 12 fenced `echo RCY<tok>_NN` code blocks, then 9 one-word answers push it more than 6 rich-content entries away, so the app logs `editor_memory agent_code_recycled`; block navigation (ctrl+up, mac cmd+up) goes back until `agent_code_rebuilt`; fails when no recycle or no rebuild happens, when the snippets visible on screen (OCR; without OCR the screenshot hash) change across the rebuild, or when copying the selected answer (ctrl+shift+c, mac cmd+c) loses a snippet line; a model that does not write the snippets is a SKIP; the per-snippet Run / Copy buttons need a mouse and are not driven), `editor_memory_7mb` (terminal pass only, #1340 P5 / #1607 / #1407: generates a 7 MB plain-text and a 7 MB Rust file, opens each in the code editor through the command palette with the absolute path, records memory before / after open, right after closing the pane and after the 60 s undo-close grace, and repeats open/close `APPCI_EDITOR_MEM_CYCLES` = 3 times for the plain-text file; fails when open costs more than `APPCI_EDITOR_MEM_TXT_MAX_MB` = 100 / `APPCI_EDITOR_MEM_RS_MAX_MB` = 150 MB over the pre-open baseline, more than `APPCI_EDITOR_MEM_CLOSE_MAX_MB` = 60 MB is still held after close, or the post-close figure grows more than `APPCI_EDITOR_MEM_CYCLE_GROWTH_MAX_MB` = 30 MB from the first cycle to the last. Judged figures: RSS (Windows: working set) everywhere and, on macOS, ALSO `phys_footprint` from libproc (owner 2026-11-03: either one over a budget fails; both reported as `*_mb` / `*_phys_footprint_mb`). The app's own breakdown lines (log target `editor_memory`, debug: buffer tree, render tree, parked chunks, hydrated frames, buffer drop) are attached as `*_app_breakdown_open` / `*_app_buffer_dropped`; a close through the command palette that is not followed by a `buffer_dropped` line fails (Return ran a different palette item, or the editor is still referenced). Then a heap-trim UI A/B by relaunch (`APPCI_EDITOR_MEM_TRIM_AB=0` skips it): default, `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1` and `PHOSPHOR_HEAP_TRIM=0`, one open/close each, the max `input_order hop=ui_item` (main-thread frame / event / action of >= 4 ms, `log_ui_item_if_over_budget`) in the 5 s after the load's layout frame and after the close key, against the 4 ms budget; a trim variant fails only when it is over the budget and more than 1 ms over its control (metric `trim_ab`). `APPCI_KEY_PALETTE` / `APPCI_KEY_CLOSE` override the keys; where the close key is empty (macOS: the guard refuses cmd-shift-W) the pane is closed through the command palette, `APPCI_KEY_CMD_PALETTE` (mac `cmd+p`; `cmd+shift+p` is the navigation palette) + `APPCI_CLOSE_PALETTE_TEXT` (`Close Current Session`); `APPCI_EDITOR_MEM_STRICT=1` fails instead of skipping when the file never opens, `APPCI_EDITOR_MEM_KINDS=txt` limits it),
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
- `ui_palette`: palette opens ('Search for a command'), nonsense filter shows 'No results', 'split pane right' + Enter makes a new pane (new shell prompt in `prompt.log`), typing there is exact, `exit` closes it.
- `ui_mouse_select_copy`: print a token absent from the command line, find it by tesseract word box, double-click then copy: the PRIVATE clipboard (wl-paste / xsel of the job's display, pre-set to a sentinel) holds exactly the token; a drag selection contains it. Skips on macOS (no pointer tool) and without tesseract.
- `ui_right_click_paste` (terminal mode only): right-click in the input, click 'Paste' (OCR), Enter: the shell receives exactly the clipboard command.
- `ui_drag_drop_file` (terminal only): file name with a space dropped onto the terminal; the shell must see ONE argument equal to the path. Skips unless X11 with `dragon-drop` installed (no tool synthesizes a drag source on Wayland/macOS).
- `ui_resize_typing`: window small, large, back (xdotool windowsize / sway floating resize / System Events); typing exact at each size, a wrapping 150-char command byte-exact; skips if the screenshot does not change (resize ignored).
- `ui_split_panes`: split right and down by key, new shell per pane, focus moved with next/prev pane keys, typing exact in both panes, `exit` closes the new pane.
- `ui_startup_time`: launch to first prompt (metrics `cold_s` with state dirs wiped, `warm_s` three relaunches; OS page cache is not dropped, that needs root); fails at no prompt in 90 s or warm median over `APPCI_STARTUP_MAX` (20 s); typing exact after each launch.
- `ui_settings_live`: theme, font size, `appearance.vertical_tabs.enabled` changed by writing the settings file (hot reload); each must visibly change the screen (Pillow diff, else hash) and typing stays exact; the original file is restored. The settings page is not clicked.
Packages the box needs for the full set: `tesseract-ocr` (positions), `python3-pil` (optional, stronger diff), `dragon-drop` (DnD only). The panes/palette/mouse/settings cases are not run in the sim backend.
Coverage-gap cases (`appci/cases_coverage_a.py`, #1534-#1541; all FULL tier, none in SMOKE; registered by the last import line of `cases.py`; every one
types at 10-25 ms/key with an instant Enter, judges the EXACT line/bytes the program received, saves screenshots at each decisive step, and ends with a
typed line proven exact in the LOCAL shell). A program the case needs that is not installed makes the case `noref` (UNVERIFIABLE, with the package), never a
quiet pass or a substitute (#1537 "silently uses less when vim is missing"):
- `paste_multiline_prompt` (terminal): a 3-line paste (short and 3000-char lines) into the prompt, Enter, each line runs exactly once and in order; metric `*_lines_run_before_enter` records whether the editor ran them at paste time. BOTH Windows panes.
- `paste_bracketed_off` (terminal): `printf '\033[?2004l'; cat > f`, multi-line paste with tabs and no final newline arrives byte for byte, no `ESC[200~` markers. Git Bash pane.
- `paste_agent_input` (agent): multi-line prose pasted into the agent input; the app's `hop=enter_begin` line holds it exactly, input type AI, the shell got nothing. Git Bash pane.
- `alt_screen_programs`: vim (typed text saved), less (`!cmd` inside less writes a token), htop (draws, `q`), tmux (inner shell receives the exact line; its rec.log is checked). Git Bash pane.
- `alt_screen_mouse_reporting` (terminal, Linux X11/Wayland): a python3 program turns on SGR mouse reporting; a real pointer click must arrive as `ESC[<0;x;yM` + release and the key typed after it last. Click position by OCR of the program banner (tesseract) else the window centre (Pillow). Skips on macOS/Windows (no pointer tool).
- `sudo_su_real_shell` (terminal): the real privilege boundary the nested-bash stand-in only imitates. sudo needs passwordless `sudo -n`; su needs the runner to be root (su as `nobody`, writes into the case dir, which is chmod 777); ksu needs `ksu` + a ticket. Each part that cannot run is UNVERIFIABLE with the reason; the case SKIPS when none can. Proof is `id -u` typed into the inner shell, then exact lines, `exit`, and `id -u` of the local shell again.
- `ssh_drop_reconnect` (terminal, needs the local sshd): loses a live ssh by `<Enter>~.` and by `kill -9 $$` on the remote; after each the local shell works and is exact, a new `ssh fake` gives a remote prompt, remote lines are exact, `exit` returns local. Linux/Mac only (not yet verified against the msys/Win32 ssh clients).
- `crash_kill9_restore` (Linux, macOS, Windows; #1540/#1541 part 2): `restore_session=true`, N tabs with commands, kill of the app pid (SIGKILL; Windows `taskkill /PID /T /F`; `cases_robust.hard_kill`), relaunch through the platform's `App.start()` (macOS `open -n` of the bundle, Windows the exe, then run.py's key proof again via `ctx.reprove`): a shell comes up, nothing re-runs (effect file and rec.log), pane exact; restored shell count is a metric (a hard kill may lose the last session-state debounce, never replay a command).
- `crash_corrupt_db` (Linux, macOS, Windows): `warp.sqlite` (found under the scratch home, or on Windows under `%LOCALAPPDATA%\phosphor` / `%APPDATA%\phosphor`, `cases_robust.data_roots`) overwritten with garbage while the app is down; the relaunch must come up, move it aside as `warp.sqlite.corrupt-*` (#860) and leave an exact pane.
Still NOT covered (owner/env decisions, not code): Linux ibus/fcitx and Windows IME (needs an input-method daemon installed on the box/VM).
Windows pane shells beyond PowerShell 5.1 / Git Bash (#1534 part 2): `APPCI_WIN_SHELLS=pwsh7,cmd` (never in the default list; play-time variable of `app-test-windows`) adds startup_shell_override passes: the harness writes `session.startup_shell_override = "<pwsh.exe or cmd.exe>"` (and `new_session_shell_override`) into the scratch settings (`profile.gitbash_toml`, variant `pwsh7` / `cmd`), result rows are labelled `terminal@pwsh7` / `terminal@cmd`, and only `winmap.VARIANT_CASES` run (pwsh7: `mode_proof`, `startup_shell_pane`, `fast_typing`, `paste_instant_enter`, `tab_enter`, `ctrl_c_d` under the PowerShell family's helpers; cmd: `mode_proof`, `startup_shell_pane`), no skip rows for the rest. `startup_shell_pane` (`appci/cases_shells.py`, order 72) proves the shell by an effect (`PS7_Core` / `CMDOK_cmd.exe`), types N exact lines at 10-25 ms/key, interrupts a long command with Ctrl-C and runs the next line. cmd.exe's effect syntax is `Ctx.append_cmd('cmd')` (`echo tok>>file`), its line clear is Escape (no Ctrl-U), its pane state is `cd /d`. A machine without pwsh 7 / cmd.exe gets one skip row for the pass. Whether the app bootstraps cmd.exe at all is unverified until the first run.
Nextest on Windows (#1534 part 2, superseded 2026-11-03): `test-windows` now runs the same command as `test-linux` (see "Every suite on every platform" in 2.9); `WIN_TEST_FILTER` / `WIN_TEST_FULL` and `--profile ci` are gone, use `TEST_FILTER` (a nextest filterset) as on Linux.
Remote-server size and latency (#1157 part 2): `script/ci/remote_server_bench.py` drives `<bin> remote-server-proxy --identity-key K` over stdio with hand-encoded length-prefixed protobuf (`Initialize`, then `ResolvePath` / `ListDirectory` requests) in a scratch HOME and reports, per binary, the binary and gzip -9 size (plus a given tarball's size), startup to `InitializeResponse` (includes the daemon the proxy forks), round-trip p50/p95/p99/max of N small requests (`rtt_ms`), the same for a 3000-entry directory (`list_ms`, the malloc-sensitive path) and back-to-back throughput (`burst_ms_per_request`), with a delta table against the first binary. `script/ci/remote_server_before_after` builds `phosphor-oss` for the musl target twice, BEFORE = `release-cli` without mimalloc, AFTER = `release-remote` + `mimalloc` (what `script/linux/bundle --artifact cli` builds), and runs it; the manual `bench-remote-server` job (box runner, 5 h, nothing installed: zig, cargo-zigbuild and the musl target must already be there from `cli-linux`) uploads `bench-out/remote-bench.json` and both tarballs. Expected from #1157: tarball about +20-30 %, p99 and burst better with mimalloc; the first run records the real numbers. The measurement loop is unit-tested against a fake proxy in the appci self-test.

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
- `flood_scrollback_find` (62): a random uppercase marker is the first output line of a flood of at least 300k lines AND 3 s (r18e4: a fast macOS finished 300k lines before the screen scrolled); control: it is NOT on screen after
  the flood; `ctrl+shift+f` + the marker + Enter must bring it on screen (OCR). SKIPs without tesseract (package `tesseract-ocr`).
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
  (`APPCI_KEY_BUDGET_MS`), Ctrl-C <= 20 ms (`APPCI_CTRLC_MAX_MS`; `ctrl_c` is a plain `sleep`, the control; `ctrl_c_flood` is the output flood of 3.6, #1514).
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
  (`zle_bracketed_paste=( $'\e[?2004h' $'\e[?2004l' )`, confirmed). Not a verdict input.
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
  Cmd-Q/W/Tab/Space, focus returned to Phosphor). The reference is a `.command` file opened with `open -a Terminal` (no Apple Events, no Terminal preference changes). At harness start the harness records whether Terminal.app was already running (exact PID from `ps -A -o pid=,comm=`, `appci/refterm_mac.py`); if not, the Terminal.app the run launched is recorded as `terminal <pid>` in `<run>/pids` and quit at the end of the run by that PID (SIGTERM, then SIGKILL), which closes every reference window (#1485); `teardown.sh` does the same for a cancelled job, only if that PID started after the run dir was created. An already-running Terminal.app is never quit: the case result carries a note "reference windows left open: Terminal was already running (pid N)"; Windows = `powershell.exe` in conhost, same SendKeys path.
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
exchange`). The old `.invalid` host could fail in tens of ms, before the first typed key, when running the line locally is correct. The case fails as a HARNESS
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
| `fix` | `check-linux`; `test-linux` with a nextest filter; `test-warpui-core` only if its crate changed; `test-workspace-rest` on every code change (#1496); builds for the chosen platforms only; app tests in SMOKE mode on those platforms | ~1 h | every push of a fix |
| `candidate` | `check-linux`; all three suites unfiltered; all three builds; the FULL app matrix on every platform (both Linux passes); on macOS and Windows also the three unit suites (`test-<plat>`, `test-warpui-core-<plat>`, `test-workspace-rest-<plat>`), played after that platform's app tests | ~6 h (Mac/Windows suites unmeasured) | a batch of fixes is ready; nothing is "done" until a candidate passes |
| `release` | candidate + `test-shuttle-loom`, `fuzz-smoke`, `bench`, `mutants`, `mutants-warpui-core` on the box, and the same five with the `-macos` and `-windows` suffix after each platform's unit suites (`fuzz-smoke-windows` is reported `unsupported on platform`, never played) | ~16 h (estimate; the first run records it) | before a numbered release (then also the manual checklist of section 5) |

**Every suite on every platform (2026-11-03, owner: "the tests need to be the same on all boxes").** Each box suite has a `-macos` and
a `-windows` twin (names in the table in 2.7; `test-macos` / `test-windows` are the `test-linux` lib suite). All are manual and
`allow_failure`, never automatic on a push. They share ONE definition: `script/ci/run_suite.sh <macos|windows> <suite>` runs the same command
as the box job (same packages, `check_test_failures`, `TEST_FILTER` + `check_filter_terms.py`, default nextest profile, same `MUTANTS_*` scope via the
`.mutants_scope` template). Differences that are not choices: no slot governor (`PHOSPHOR_GOVERNOR=0` in `script/agent-cargo`; one job per machine,
the job/thread caps stay: 4 on the 16 GB Mac, 8 on Windows); failures are judged against `script/known_test_failures_macos.txt` /
`_windows.txt` (`PHOSPHOR_KNOWN_FAILURES`; empty at first, so the first run reports every platform failure as a regression to be
triaged); the jobs are in `resource_group` `mac-gui` / `windows-gui`, the same as the app tests, so a suite never overlaps an app run on that machine;
the log is the artifact (`nextest-<plat>.log`, `nextest-warpui-core-<plat>.log`, `nextest-rest-<plat>.log`; no junit: the box jobs have none and the default
profile has no junit section), plus `fuzz/artifacts/`, `mutants.out/`, `bench-results/`. `check_test_failures` and `agent-cargo` were made BSD/Git-Bash
portable for this (`sed -i`, `\x1b` and `\s` replaced). `collect_results.py` archives the new jobs like the box ones (`unit/<job>.json`, per-job history).

Tools are installed by the job into the CI user's OWN `~/.cargo/bin` when missing (pinned `script/install_cargo_binstall`, then `cargo binstall`
of `cargo-nextest`, `cargo-mutants`, `cargo-fuzz`), never system-wide, never with sudo. The runner must already have cargo/rustc (the pinned toolchain
installs itself through rustup on first use), protoc (Mac: on PATH; Windows: `$env:PROTOC`), perl and cmake (Windows), Git for Windows' bash and the VM
Python (`%LOCALAPPDATA%\phosphor-ci\python312`); a missing one fails the job with a message. Platform limits found:

- `fuzz-smoke-windows` CANNOT run: cargo-fuzz (libFuzzer plus sanitizers) "works only on x86-64 and Aarch64, and only on Unix-like systems (not Windows)" (rust-fuzz/cargo-fuzz README).
  The job exits 1 with that reason (it never skips silently); `ci_seq.py` does not play it and reports `unsupported on platform` (neutral). Linux and macOS cover the same eight targets.
- `fuzz-smoke-macos` mirrors the box: `RUSTC_BOOTSTRAP=1` on the pinned stable toolchain, so no nightly is installed anywhere (needs the Xcode command line tools' C++ compiler).
- `test-shuttle-loom-*`: `RUSTFLAGS="--cfg shuttle|loom"` replaces `.cargo/config.toml` flags (on Windows that drops `target-cpu=x86-64-v3`, on macOS the header-pad link arg), each cfg in its own `target/ci-shuttle` / `target/ci-loom`. Pure Rust, expected to work on both; unmeasured.
- Unverified until the first run: whether the whole `warp` lib test suite passes on macOS and Windows at all (the Windows run was never completed before, the old `test-windows` ran only a name filter), the run times, and `mutants-*` copy-tree cost on Windows.

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

### 2.10 Markdown and Mermaid readers (#1460)

What is covered, where, and how to update it. Nothing here runs cargo on the laptop; the box does (section 1).

| layer | where | what it checks |
|---|---|---|
| parser unit tests (pre-existing, ~142) | `crates/markdown_parser/src/*_test.rs` | individual constructs, incremental tail parse |
| markdown corpus + insta snapshots | `crates/markdown_parser/tests/corpus.rs`, fixtures `tests/fixtures/*.md`, snapshots `tests/snapshots/` | every fixture (headings, nested lists, task lists, tables, fenced code, inline, links/images, block quotes, HTML blocks, footnotes, unicode/RTL, CRLF, malformed, control chars, empty) parsed with and without GFM tables; the `FormattedText` document model is the snapshot. Also: no panic, deterministic, CRLF reads like LF, 600 KB single lines, deep nesting, streamed tail parse equals full parse. Tables: `parse_markdown_with_gfm_tables` is the runtime switch (no cargo feature) |
| mermaid corpus | `crates/editor/tests/mermaid_corpus.rs`, fixtures `tests/fixtures/mermaid/*.mmd`, snapshots `tests/snapshots/` | 14 valid diagrams (flowchart TD/LR, sequence, class, state, ER, gantt, pie, mindmap, journey, timeline, frontmatter theme, unicode labels, CRLF) rendered through `mermaid_asset_source`'s fetch, the exact app path: expected labels present, deterministic, normalized summary snapshot (rounded size, element counts, text labels; never the raw SVG). 9 `invalid_*` files: must return the app's error (which becomes `FailedToLoad` and the source fallback in `render_mermaid_diagram_section`) or a non-blank SVG, never panic. Hostile sources for every diagram kind (including #1622's subgraph ids reused inside themselves), 300-node flowchart and 400-message sequence bound; `issue_1622_unclosed_subgraph_reuse.mmd` (the fuzz timeout input, also seed `fuzz/corpus/mermaid_render/ref_issue_1622`) must render with its labels in under 2 s, well inside the 8 s worker budget, so a regression fails as a timeout instead of passing as the budget's error. The app's fetch is bounded (#1477): sources over 256 KiB or with a line over 4000 characters are refused up front (an unbroken 100k-character token made the renderer spin for minutes), and a render runs on a worker thread with an 8 s budget, ending as the error/source-fallback path; unit tests in `mermaid_diagram.rs` (`render_limit_tests`) |
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
refused once asked for: refusing it loses the line. Instead: a line this pane delivered to the shell as final (the #1494 `stty sane`
path, `TypeaheadHandoff::note_committed_delivery`) is never asked for -- `on_line_editor_active` answers `Drain`, the pty keeps input
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
| view, Legacy and Table | `view_test.rs`: `a_line_delivered_to_the_shell_as_final_stays_in_the_shell`, `a_report_into_an_ai_mode_input_is_kept_as_shell_text_and_runs_once`, `the_ssh_bootstrap_failure_delivery_is_marked_final` | the delivered line stays in the shell (no request, editor empty, the next keys and Enter reach the pty once); a report into a locked AI input is inserted as shell text, and a deferred Enter runs it as a shell command exactly once; the #1494 path sets the mark |
| editor | `input_test.rs`: `a_handoff_report_into_an_ai_mode_input_is_kept_as_shell_text` | the report is inserted (never dropped) and the input is switched to shell, locked; a shell-mode input inserts as before |
| real app | Windows Git Bash and PowerShell, terminal and agent mode: `sleep 3` + a line typed in its first second, then a fast line at the next prompt | the shell's `rec.log` has both lines once, in order; no `session_off=true` in the log for Git Bash; agent mode ssh with a slow bootstrap: after `stty sane` the delivered line stays on the remote (no `handoff_insert` of it, `handoff_request_skipped` logged), nothing typed is lost |

### 2.13 The prompt hold's give-up never writes to the pty (#1584)

The 1.5 s cap of the ConPTY prompt hold is a give-up, and a timer never writes to the pty. At a prompt whose line holds nothing typed
(the precmd's wait), held keys with no Enter go back to the editor as shell-routed text (`system_insert` + the next Enter as a shell
command), the editor takes input over, and the log says so with counts (`hop=prompt_hold_handed_back`). A segment with an Enter, or one
that cannot go back in order (an enter window whose Enter the shell never acted on), stays held: the cap is armed again, a toast and one
log line (`hop=prompt_hold_kept_overdue`) say input is waiting for the shell, and the events release it (the signal, a command becoming
interactive, a continuation prompt, ^C or another signal key). A shell that has never signalled is not held for at later prompts (its
keys go by the live path) until a signal shows it does. Such a shell's segment at a clean prompt is handed back even with an Enter (as a
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
| `cb_agent_file_mention` (agent) | #1542 attachments | `@<file prefix>` lists a real file (OCR when present); Enter accepts the row without submitting to the agent or the shell (an AI `hop=enter_begin` counts as a submit only when no `hop=enter_menu_accept` follows it before the next Enter, #1633; unit: `test_enter_on_the_at_picker_in_agent_mode_never_submits` hands the picker the typed query as the app does and checks the Enter waits in the picker, `terminal/input_test.rs`); the next prose line is exact |
| `cb_settings_ui_open` (both) | #1545 settings UI | Ctrl-, / Cmd-, opens the page (screen change, OCR), Escape closes it, typing in the pane behind is exact. Windows: not mapped |
| `cb_settings_migration` (both) | #1545 migration | a settings.toml with unknown comment/key/table, then a truncated unparsable one: the app starts, types exactly, logs no panic and no migration/settings ERROR; `legacy_entries_kept` is a metric. The original file is restored |
| `cb_state_relaunch` (terminal) | #1546 DB migration path | three relaunches on the same state dirs: prompt, exact typing, no migration/database ERROR in the app log, database files neither vanish nor halve |
| `cb_no_autoupdate` (terminal) | #1547 | the whole run's app log has no updater line (the GUI updater is declined: DECLINED.md `FeatureFlag::Autoupdate`, `script/check_no_autoupdate`) |
| `cb_ui_action_latency` (terminal) | #1548 | new tab and tab switch seen from screenshots: bound `APPCI_UI_ACTION_MAX_S` (1.5 s, first guess), numbers as metrics. An UPPER bound on the app's time (each screenshot costs 50-300 ms), not a 4 ms proof |
| `cb_editor_mac` (mac) | #1544 | edit, Cmd-S, undo, Cmd-S on a file handed over by `open -a`; disk byte-exact; nothing at the shell. Runs last (no keyboard pane-close on macOS; run.py relaunches if the pane is not the shell) |

Pure helpers (`legacy_settings`, `corrupt_settings`, `startup_log_problems`, `autoupdate_lines`, `picker_verdict`) are covered by `selftest.py` (`coverage_b_pure_helpers`).

**Already covered, so not duplicated here.** #1460 (markdown/mermaid): sections 2.10 (parser corpus + snapshots, mermaid corpus, bench, fuzz targets) and the `md_viewer*` cases. #1463: `agent_stream_cancel`, `agent_approval`, `routing_table`, `agent_input_exact`, `agent_mode_switch` in `cases_caseagent.py`. #1543 is the same gap as #1460 (pixel-level diagram rendering stays the documented gap in 2.10). #1544 Linux and Windows: `editor_edit_save`, `editor_paste`, `editor_memory_7mb` (Windows hand-over: the app's named-pipe single-instance forward); macOS: `cb_editor_mac`.

**Recorded gaps (need an owner decision or a fixture, not code):** uninstall and a `phosphor://` URL end to end (#1546): need a clean-VM step, today manual (5.4). Upgrade over a previous build's DATA (#1546 option a) is a case now, `upgrade_over_previous` (order 90, full tier, `cases_coverage_b.py`): the new build starts on the previous release's data directory; the data comes from `APPCI_PREV_FIXTURE=<state tarball>` or from `APPCI_PREV_BIN=<previous release binary>` (macOS also `APPCI_PREV_BUNDLE=<.app>`), which the case launches on the scratch profile, uses, stops and packs to `<run>/fixtures/prev-state.tar.gz` (keep it as a CI artifact; not on Windows, where the state is in the known folders). Verdict: the new build reaches a prompt, types exactly, logs no failed start/migration, keeps every database file (not moved aside, not shrunk), and has not fewer applied `__diesel_schema_migrations` than the old build left (metric `migrations_before_after`). Without either variable it SKIPS with that reason: where CI gets the previous release's artifact (the generic package registry has the CLI tarballs and the installers per version) and which Windows/Mac machine may keep it is the owner's decision. The installer-over-installer step itself stays manual. Auto-update against a real release (#1547): the GUI updater is off by decision; its `SHA256SUMS` verification is unit-tested in `app/src/autoupdate/gitlab_tests.rs` and cannot meet a real release until the release pipeline has run end to end. Key-to-paint on Linux/Windows (#1548): still no pixel read-back tool there (Linux needs a 240 Hz capture; Windows needs Desktop Duplication through an installed tool: an environment decision), so a pixel-level key-to-paint verdict is NOT-FIXED; the app-side key to first frame (`latency_key_log`, both platforms) remains the proxy. The 4 ms UI-work line IS covered now: the app's `hop=ui_item` line (`input_order::log_ui_item_if_over_budget`, floor 4 ms, #1191) is read by `latency_key_log` (`metrics.ui_items_over_4ms`, `lat.ui_work`: event/action items over 4 ms beyond max(2, 5 % of the typed keys) FAIL; `frame` items WARN because software GL rasterises over 4 ms; an item >= 16 ms is a WARN note) and by `cb_ui_action_latency` (`new_tab_ui_work`, `tab_switch_ui_work`).

**#1617 (Windows session-end gate).** Every handled shell exit asks the gate once (`exited_shell_keeps_pane`), a shell that never
bootstrapped included, so its counted session-end exit is consumed and a later ordinary `exit` closes its pane.

| layer | test | asserts |
|---|---|---|
| pane | `terminal_pane.rs` `exited_shell_tests`: `an_unbootstrapped_exit_consumes_its_session_end_count`, `a_counted_exit_of_a_bootstrapped_shell_keeps_its_pane_once` | the gate is asked for an unbootstrapped exit and its count is gone; the next ordinary exit closes |

**#1438 (PowerShell abort settle).** After a Ctrl-C skips a PowerShell setup, the shell's read marker is the only release that writes
the held keys. A setup with no marker token (`PHOSPHOR_BOOTSTRAP_TIMED=1`, or markers that never came) has no event, so
`POWERSHELL_ABORT_SETTLE` is a give-up that hands the held keys back to the editor (`abort_settle_give_up`), never writes them.

| layer | test | asserts |
|---|---|---|
| view, Legacy and Table | `group2_timer_tests.rs`: `powershell_abort_settle_without_a_marker_hands_the_held_keys_back` | no token: the held `ls` is in the editor with the Enter guard armed, nothing written; a stale generation does nothing |
| view, Legacy and Table | `group2_timer_tests.rs`: `ctrl_c_on_a_timed_powershell_setup_hands_the_held_and_later_keys_back_after_the_settle` | the real path: `ls`, Ctrl-C, `x`, the spawned 500 ms timer: the editor holds `lsx`, the pty got only `\x03` |
| view | `view_test.rs`: `powershell_abort_settle_ends_on_the_read_marker_not_the_clock` (existing) | the marker after the Ctrl-C writes the held keys; time and line markers do not |

**#1391 (PS2 continued line).** bash and zsh send `ContinuationPrompt` (PS2 head / zle `CONTEXT=cont`) and the hand-off re-baselines
its counts on it; on Unix a stale count is also corrected from the sampled line-editor state (#1614). fish and PowerShell continue a
line inside their editor with no hook, so on ConPTY the 5 s logged give-up is still what resolves it. Evidence to look for in a log:
`hop=handoff_stale_counts_reset by=give_up shell_activity=.. markers_live=..` -- `Reading` with `markers_live=true` is a line continued
inside the editor (pwsh on ConPTY); `Unknown` is a bootstrap without markers.

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
- `integration-linux` is manual and `allow_failure: true` in CI (a hard gate only in `precheck`); macOS and Windows have no
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
  exited an empty agent conversation).
- **`cb_settings_ui_open`**: the settings tab is closed with the platform's close (chord, else the command palette's `Close Current Session`) when a probe line shows the
  shell is not in front (terminal mode), instead of only on a screenshot diff and only with a chord that is empty on macOS. **`cb_no_autoupdate`**: the startup line's
  `autoupdate_configured: false` is blanked before matching (`: true` still matches). `cb_agent_file_mention`: the Mac failure is an APP finding (the `@` picker is open in
  the screenshot, Enter submits); not changed.
- **`subshells`**: nu appends with `| save --append` (`>>` is a nushell parse error). **`remote_session_*`**: the clean-env subshell quotes `PATH="$PATH"` (a Mac PATH with
  spaces split the words); on Windows that step is NOT RUN (noref): `env -i` under msys ends the fake sshd session (H6).
- **`flood_scrollback_find`**: Find is Cmd-Shift-F on macOS (`bindings.rs CustomAction::Find`); the marker is typed split by an empty string (`ABC""DEF`) because Phosphor
  keeps the running command's text as the block header, which kept the control "marker not on screen" from ever holding.
- **`remote_heredoc_paste`**: a remote shell without bracketed paste makes the app ask `Run N lines one at a time?`; the case answers it with Run (OCR-detected, noted).
  Without OCR nothing is pressed and the note says a timeout may be that dialog.
- **`session_restore` / `crash_kill9_restore`**: the app's own bootstrap line (`command -p stty -echo; WARP_BOOTSTRAP_VAR=...`) that a restored zsh pane logs to rec.log is not a
  replayed user command (`Ctx.APP_BOOTSTRAP_RE`, excluded from `cmds_since` and the setup comparison). The restored-cwd mismatch (all tabs `~`) is NOT changed here. On the
  PowerShell pane (no prompt hook) the "a shell started" prompt-count check is replaced by a note (H10).
- **`ssh_drop_reconnect`**: the `<Enter>~.` escape variant is UNVERIFIABLE (noref) until the owner decides whether a managed session's editor should pass the ssh escape through
  (`APPCI_SSH_ESCAPE_THROUGH=1` runs it); the `kill -9 $$` variant runs.
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
- **Not changed, by design**: H7 (fake sshd ConPTY heredoc stall) and H8 (Git Bash pass opens PowerShell tabs/restored tabs) need run evidence the artifacts do not contain;
  `profile.gitbash_toml` already writes `startup_shell_override` and `new_session_shell_override` for that pass.

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
