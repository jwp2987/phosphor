# SCOPE — Local usage / acceptance smoke suite (GUI + TUI)

Status: design only. No implementation code here beyond illustrative snippets.
Branch context: `edition-2024`. GUI binary = `warp` crate (`app/`) built with
`--features gui`; TUI binary = `zap-tui-oss` (`crates/warp_tui`) built with
`--features tui`. App id = `Zap` (shared config/secrets/BYOP for both surfaces).

## 0. Goal (restated)

A **higher-level-than-unit** smoke suite that an AI agent (Claude) can run on
demand, **headlessly, from one command**, to answer "does the running app
actually do the thing" for BOTH GUI and TUI. It must:

- launch the real app (or its real view subtree), drive realistic usage flows,
  assert the outcome, and print a **machine-readable pass/fail summary** with a
  single aggregate exit code;
- run with **no human clicking and no desktop** (mock display by default);
- degrade gracefully around the sandbox's known limits (the shell-preexec race,
  no cloud/BYOP provider, no real GPU display).

## 1. What already exists (reuse, do not reinvent)

The repo already has **two** in-process "launch the real thing and drive it"
harnesses. The usage suite is a thin **curation + orchestration + reporting**
layer on top of them, not a new driver.

### 1.1 GUI: the `integration` harness (`crates/integration`)

- A **standalone binary** `integration` (`crates/integration/src/bin/integration.rs`)
  boots the **full GUI app** in the `Channel::Integration` channel (app id
  `WarpIntegration`, hermetic `$HOME` under `CARGO_TARGET_TMPDIR`) with a **mock
  display by default** — a real window only opens when
  `WARPUI_USE_REAL_DISPLAY_IN_INTEGRATION_TESTS=1` is set. So it is **headless by
  default and needs no X/Wayland/GPU**.
- One invocation runs exactly **one** scenario and **exits with 0 = pass**,
  nonzero = fail, or `RERUN_EXIT_CODE` = "flaked, retry":
  `cargo run --bin integration -- <scenario_name>`.
- Scenarios are `fn() -> Builder` registered in `register_tests()`
  (`crates/integration/src/bin/integration.rs`). The `Builder`
  (`crates/integration/src/builder.rs`, wrapping
  `warpui::integration::Builder` in `crates/warpui_core/src/integration/driver.rs`)
  composes `TestStep`s: `.with_setup`, `.with_step`, `.with_keystrokes`,
  `.with_event_fn`, `.with_action`, `.add_named_assertion`, timeouts.
- Rich, ready-made step + assertion library lives in
  `app/src/integration_testing/` (`terminal/step.rs`, `terminal/assertion.rs`,
  `input/`, `tab/`, `command_palette/`, `settings*`, `agent_mode/`, plus
  `view_getters` that read live view/model state). Assertions are async
  (`async_assert!`, retried until timeout) or immediate (`integration_assert!`).
- The existing `#[test]` wrappers (`crates/integration/tests/`) simply
  **spawn this same binary as a subprocess** (`common/mod.rs`,
  `CARGO_BIN_EXE_integration`) with a 10x rerun loop on `RERUN_EXIT_CODE`. That
  is exactly the orchestration pattern the usage runner will reuse.

### 1.2 TUI: the `warp_tui` in-process view harness (`crates/warp_tui`)

- In-crate `#[cfg(test)]` modules (`crates/warp_tui/src/*_tests.rs`) use
  `App::test((), |app| async { … })` to build a real app, register the BYOP
  session singletons via `register_tui_session_view_test_singletons`
  (`app/src/tui_test_support.rs`), pump the single-thread executor with
  `test_fixtures::settle().await`, then **render a view to a text buffer and
  snapshot it**: `TuiPresenter::new().present_element(…).buffer.to_lines()`
  (see `tui_permission_prompt_tests.rs::render_lines`). Focus/selection/model
  state is asserted directly with `app.read(|ctx| …)`.
- Fully **in-process, no PTY, no shell, no provider, deterministic**. Runs via
  `cargo test -p warp_tui` / `cargo nextest run -p warp_tui`.
- The `zap-tui-oss` binary itself is a crossterm/real-terminal app; we do **not**
  drive the binary — the view harness is the reliable, headless surface.

### 1.3 CI today

`.github/workflows/pr-check.yml` runs `cargo check` for `-p warp --features gui`,
`-p warp --features tui`, and `-p warp_tui`. **No E2E/usage run in CI.** The usage
suite adds an opt-in workflow (Chunk E).

## 2. Recommended architecture

**Build ON the two existing harnesses via a thin orchestrating runner crate.**
Rejected alternatives and why:

- *Pure bash script* — can shell out fine, but aggregating per-scenario
  pass/fail into machine-readable JSON with tags/durations/skip-reasons is ugly
  and fragile in bash.
- *One big new GUI-driving harness* — pointless duplication; the `integration`
  binary already launches the full app headlessly and the TUI harness already
  renders views deterministically.
- *Make the runner depend on `warp`/`warpui`* — would pull the whole heavy app
  graph into the runner, slow its build, and fight the cargo lock. Avoid.

### Recommended: `crates/usage_suite` (bin `usage-suite`) — a light orchestrator

A **new, dependency-light crate** (`clap` + `serde_json` + `std::process` only;
**no** workspace app deps) that:

1. Holds a **scenario manifest** (surface, name, tags, skip-by-default flags).
2. For each selected **GUI** scenario: spawns `cargo run -q -p integration --bin
   integration -- <name>` (reusing 1.1 verbatim, including the mock display and
   the `RERUN_EXIT_CODE` retry loop), captures exit status + duration.
3. For selected **TUI** scenarios: runs `cargo nextest run -p warp_tui -E
   'test(/(^|::)usage_tui_/)' --message-format libtest-json` (or `cargo test -p
   warp_tui usage_tui_ -- --format json -Z unstable-options` as fallback) and
   parses per-test outcomes.
4. Emits a **unified report**: NDJSON per scenario on stdout + a
   `target/usage-report.json` summary + a human-readable table on stderr, and a
   single aggregate **exit code** (0 = all selected passed/skipped, nonzero =
   ≥1 failure).

Because `crates/*` is a glob workspace member, the crate is auto-included with
**no root `Cargo.toml` edit**; like `integration`, it is simply left out of
`default-members` so normal builds ignore it.

**Where GUI and TUI share vs differ:**

| Concern | GUI | TUI | Shared |
|---|---|---|---|
| App launch | full app, mock display (`integration` bin) | `App::test` + session singletons | both are in-process, headless |
| Drive input | `TestStep` keystrokes/events | `dispatch_focused_key`, model updates | scenario = "act then assert" |
| Assert | view/model getters, block state | render-buffer snapshot + focus reads | outcome-based, not pixel-based |
| Runner call | spawn `integration` bin per scenario | one `nextest`/`test` filter run | `usage-suite` aggregates both |
| AI/agent | synthetic injection / mockito (Chunk D) | mock models (`TerminalModel::mock`) | no real cloud/provider |

### 2.1 Why the runner shells out instead of linking

Both harnesses are **test-context** constructs (`#[cfg(feature=integration_tests)]`
/ `#[cfg(test)]`) that cannot be called as a plain library from a bin. Spawning
the already-supported entrypoints (the `integration` bin; `nextest` for
warp_tui) is the lowest-risk, zero-duplication path and keeps the runner's own
compile surface tiny so it does not serialize hard against other cargo jobs.

## 3. How Claude runs it

Single entrypoint wrapper `script/usage-test` (holds the shared cargo flock so it
obeys AGENTS §5.8), which builds the `integration` bin once then invokes the
runner:

```
./script/usage-test                 # run all default (non-flaky, non-BYOP) scenarios, both surfaces
./script/usage-test --surface gui   # GUI only
./script/usage-test --surface tui   # TUI only
./script/usage-test --only usage_launch_bootstrap,usage_run_command
./script/usage-test --include-flaky # also run real-shell scenarios (preexec race; auto-retried)
./script/usage-test --include-byop  # also run scenarios needing a real provider (requires key+network)
./script/usage-test --json          # NDJSON only (suppress the human table)
```

Under the hood (illustrative): `flock … cargo run -q -p usage_suite -- <args>`.

**Pass/fail output format.** One NDJSON object per scenario, then a summary
object, then a human table on stderr:

```jsonl
{"surface":"gui","scenario":"usage_launch_bootstrap","status":"pass","duration_ms":1840,"tags":["reliable-here"]}
{"surface":"gui","scenario":"usage_open_close_settings","status":"pass","duration_ms":610,"tags":["reliable-here"]}
{"surface":"gui","scenario":"usage_run_command_output_block","status":"pass","duration_ms":2570,"tags":["needs-real-shell"],"retries":1}
{"surface":"tui","scenario":"usage_tui_transcript_render","status":"pass","duration_ms":90,"tags":["reliable-here"]}
{"surface":"gui","scenario":"usage_agent_roundtrip","status":"skip","reason":"needs-byop-provider (no --include-byop)","tags":["needs-byop-provider"]}
{"summary":{"total":12,"passed":10,"failed":0,"skipped":2,"surfaces":{"gui":8,"tui":4}}}
```

```
SURFACE  SCENARIO                          STATUS  MS
gui      usage_launch_bootstrap            PASS    1840
gui      usage_open_close_settings         PASS     610
...
tui      usage_tui_transcript_render       PASS      90
-----------------------------------------------------
12 total | 10 passed | 0 failed | 2 skipped   → EXIT 0
```

Exit code: `0` if `failed == 0`, else `1`. Skips never fail the run. Claude reads
the final `summary` line (and any `"status":"fail"` lines, which include the
captured stderr tail) to report result.

## 4. Scenario catalog

Tags: **reliable-here** (in-process, no shell/provider — trustworthy in this
sandbox); **needs-real-shell** (drives a real PTY shell to command completion —
subject to the preexec race, run only with `--include-flaky`, auto-retried);
**needs-desktop** (wants a real GPU window/pixel result — skip here);
**needs-byop-provider** (real agent round-trip — skip unless `--include-byop`).

### 4.1 GUI scenarios (`integration` binary)

| Scenario | Flow | Tag | Reuses |
|---|---|---|---|
| `usage_launch_bootstrap` | launch → single pane bootstraps, input focused | reliable-here | `wait_until_bootstrapped_single_pane_for_tab`, `input_editor_is_focused` |
| `usage_open_close_settings` | open settings pane, assert open, close | reliable-here | pattern from `test_open_and_close_settings` |
| `usage_open_command_palette` | open palette, assert visible, run an entry (e.g. create folder) | reliable-here | `command_palette/` steps |
| `usage_tabs_add_switch_close` | new tab (cmd/ctrl-shift-t), switch, close; assert tab count/active | reliable-here | `tab/` steps, session steps |
| `usage_theme_creator_modal` | open/close theme creator modal | reliable-here | `test_open_and_close_theme_creator_modal` |
| `usage_block_navigation_select` | select last block via keybinding, assert selection | reliable-here | `assert_selected_block_index_is_last_renderable` |
| `usage_find_in_block` | open find bar, type query, assert autoselect/match | reliable-here | `find/` steps |
| `usage_agent_block_render` | inject synthetic AI block (`insert_dummy_ai_block`), assert title/body render + markdown | reliable-here | `agent_mode.rs` pattern (no provider) |
| `usage_secret_redaction` | run/inject output containing a secret, assert obfuscation on copy | reliable-here | `secrets.rs` steps |
| `usage_run_command_output_block` | type `echo hello`+enter → block reaches `DoneWithExecution`, output == `hello` | needs-real-shell | `execute_command_for_single_terminal_in_tab` |
| `usage_run_command_exit_code` | run failing command, assert non-zero exit block state | needs-real-shell | `execute_command` + `ExpectedExitStatus` |
| `usage_agent_roundtrip` | real agent prompt → tool call → result | needs-byop-provider | mock provider (Chunk D) preferred; real only with `--include-byop` |
| `usage_font_size_window_resize` | assert re-layout produces expected pixel geometry | needs-desktop | (documented, skipped here) |

### 4.2 TUI scenarios (`warp_tui` `usage_tui_*` tests)

All in-process render-snapshot/focus assertions → **reliable-here**; none need a
shell or provider.

| Scenario | Flow | Reuses |
|---|---|---|
| `usage_tui_zero_state_render` | fresh session view renders zero-state prompt | `zero_state_tests` pattern |
| `usage_tui_transcript_render` | seed a mock conversation/agent block, render buffer, assert transcript lines | `agent_block_tests`, `TerminalModel::mock` |
| `usage_tui_permission_prompt` | queue a blocking action, render prompt, assert options + default focus | `tui_permission_prompt_tests` |
| `usage_tui_completions_menu` | open completions, assert rendered entries + selection | `completions_menu_tests` |
| `usage_tui_conversation_menu` | open conversation/exchange picker, navigate, assert highlight | `conversation_menu_tests`, `exchange_menu_tests` |
| `usage_tui_slash_command_palette` | open slash-command menu, assert the supported non-cloud commands render | slash-command parity (18 cmds) |

## 5. Assertion mechanism per surface

- **GUI**: outcome-based reads of live views/models via `view_getters` +
  `app/src/integration_testing/**/assertion*.rs`. Command completion checked with
  block state (`BlockState::DoneWithExecution` / `DoneWithNoExecution`), terminal
  grid text (`contents_to_string` / `output_with_secrets_unobfuscated`), input
  focus/empty, tab title/count, panel open state. Assertions are `async_assert!`
  (retried to timeout) so they tolerate the event-loop latency. **No pixel/scene
  assertions** in the default suite (that is the `needs-desktop` bucket).
- **TUI**: `TuiPresenter` render → `buffer.to_lines()` **text snapshot** compared
  to expected lines (trim/empty-filter as in `render_lines`), plus direct
  `app.read` assertions on `highlighted_index`, `is_focused`, keymap context, and
  model/pending-action state. `settle().await` is pumped before every assert.

## 6. Phased, partitioned build plan

Five chunks that **do not touch the same files** (one narrow append-coordination
note per shared file). Chunk A is the only prerequisite; B/C/D/E can then proceed
in parallel. Any agent running cargo must hold the shared flock (AGENTS §5.8);
**do not run cargo concurrently** across chunks.

### Chunk A — Runner scaffolding + docs  *(prerequisite; owns new files only)*
Owns:
- `crates/usage_suite/Cargo.toml` (deps: `clap`, `serde`, `serde_json`, `anyhow`
  only — **no** `warp`/`warpui`/`integration` deps)
- `crates/usage_suite/src/main.rs` (arg parsing, orchestration, exit code)
- `crates/usage_suite/src/manifest.rs` (the scenario table: surface, name, tags,
  default-skip flags — the single source of truth B/C register against by name)
- `crates/usage_suite/src/report.rs` (NDJSON + summary + table emitter)
- `script/usage-test` (flock wrapper; `chmod +x`)
- `specs/usage-test-suite/README.md` (how-to for humans)

No root `Cargo.toml` change needed (`crates/*` glob). Deliver with a **stub
manifest** (a couple of already-existing integration test names, e.g.
`test_open_and_close_settings`, and a placeholder TUI filter) so the runner is
end-to-end runnable before B/C land.

### Chunk B — GUI usage scenarios  *(owns 1 new file; 2 append-only edits)*
Owns:
- `crates/integration/src/test/usage.rs` (new — the `usage_*` `fn() -> Builder`
  scenarios from §4.1, composed from existing steps/getters; no new assertion
  infra)

Append-only coordination (documented insert points, no logic touched):
- `crates/integration/src/test.rs` — add `pub mod usage;` + re-export
- `crates/integration/src/bin/integration.rs` — add `register_test!(usage_*)`
  lines in `register_tests()`

Then add the real GUI scenario names to `crates/usage_suite/src/manifest.rs`
(the manifest is A's file; B only appends rows — coordinate via a clearly marked
`// GUI scenarios` region).

### Chunk C — TUI usage scenarios  *(owns 1 new file; 1 append-only edit)*
Owns:
- `crates/warp_tui/src/usage_tests.rs` (new — `usage_tui_*` `#[test]`s from §4.2
  using `App::test` + `register_tui_session_view_test_singletons` +
  `TuiPresenter` snapshots)

Append-only coordination:
- `crates/warp_tui/src/lib.rs` — add `#[cfg(test)] mod usage_tests;` next to the
  other test-module declarations

Naming: prefix every test `usage_tui_` so the runner's nextest filter
`test(/(^|::)usage_tui_/)` selects exactly this set (the anchor matches the
prefix at the crate root or inside a module such as `usage_smoke_tests`).
Append the TUI names to the manifest's `// TUI scenarios` region.

### Chunk D — Provider / AI mock  *(owns new module; 1 append-only edit)*
Owns:
- `app/src/integration_testing/mock_provider/mod.rs` (new — canned streamed agent
  response via `mockito` on `localhost`, plus a helper to point `AISettings`/
  `ApiKeyManager` at the local mock so `usage_agent_roundtrip` can run **without
  cloud or a real key**)

Append-only coordination:
- `app/src/integration_testing/mod.rs` — add `pub mod mock_provider;`

Consumed by: `usage_agent_roundtrip` (B) and, if a streamed transcript is wanted,
`usage_tui_transcript_render` (C). Until D lands, those two scenarios stay tagged
`needs-byop-provider` / use pure synthetic injection (`insert_dummy_ai_block`,
`TerminalModel::mock`) so B and C are not blocked on D.

### Chunk E — CI wiring  *(owns new file; no Rust)*
Owns:
- `.github/workflows/usage-test.yml` (new — `workflow_dispatch` + nightly
  `schedule`; runs `./script/usage-test --surface tui` always and `--surface gui`
  for the reliable-here set; **no xvfb needed** — GUI integration uses the mock
  display by default). Optionally a `--include-flaky` matrix leg allowed to
  soft-fail while the preexec race is unresolved.

Docs touch (append-only): a short "Usage suite" note in `AGENTS.md` §5 test
discipline referencing `./script/usage-test`.

**Parallelization summary:** A first (blocks nothing else once its stub manifest
runs). Then B, C, D, E run concurrently — disjoint file ownership; the only
shared files (`integration/src/test.rs`, `bin/integration.rs`, `warp_tui/src/lib.rs`,
`integration_testing/mod.rs`, and `usage_suite/src/manifest.rs`) are touched by
**exactly one** chunk each except the manifest, which B/C/D append to in
separately-marked regions.

## 7. Risks + what will not work in this sandbox

- **The shell-preexec race (primary risk).** `execute_command*` types a command
  and waits for the shell's bash-preexec `Preexec`/`Precmd` DCS messages
  (`assert_active_block_received_precmd`; the "racy" note at
  `app/src/integration_testing/terminal/assertion.rs:503`). In this sandbox the
  hook fires unreliably → blocks stick at `DoneWithNoExecution` and command/output
  merge. **Mitigation:** (a) keep the *default* GUI suite on `reliable-here`
  flows that assert view/model/injection state, **not** real command completion;
  (b) gate real-shell scenarios behind `--include-flaky` and lean on the existing
  `RERUN_EXIT_CODE` 10x retry loop; (c) pin `WARP_SHELL_PATH=/bin/bash` and write
  hermetic rc files (the harness already does) to minimize variance. Real command
  round-trips are **reliable on a real desktop/CI**, flaky here — reported as
  `skip`/`retry`, never a hard fail of the suite by default.
- **No cloud / BYOP.** There is no cloud agent and no bundled provider. A genuine
  agent round-trip needs a real key + network → `needs-byop-provider`, **skipped
  by default**. The suite exercises agent *UI/behavior* deterministically via
  synthetic injection (`insert_dummy_ai_block`, restored
  `warp_multi_agent_api::Message` snapshots, `TerminalModel::mock`) and, once
  Chunk D lands, a local `mockito` canned-response provider — covering the
  transcript/tool-call/permission UX without any real LLM.
- **No real GPU display.** GUI integration runs on the **mock display** by
  default (good — truly headless), so scene/pixel/font-geometry assertions are
  out of scope here; those flows are tagged `needs-desktop` and skipped. Setting
  `WARPUI_USE_REAL_DISPLAY_IN_INTEGRATION_TESTS=1` needs an X/Wayland server and
  is a real-desktop/CI-with-xvfb concern, not a sandbox default.
- **cargo contention.** The runner shells to cargo; it (via `script/usage-test`)
  must hold the shared flock, and usage-suite agents must never run cargo
  concurrently (AGENTS §5.8). The runner crate deliberately has a tiny dep set so
  its own build is cheap.
- **Per-scenario process cost (GUI).** Each GUI scenario is a separate
  `integration` process launch. Acceptable for a smoke suite (seconds each); the
  binary is built once and cached, so only first build is heavy.
- **TUI binary itself is untested.** We assert the TUI **view subtree**, not the
  `zap-tui-oss` crossterm event loop end-to-end. Driving the real TUI binary
  headlessly (pty + input script + screen scrape) is possible but out of scope
  and lower-value than the deterministic render-snapshot harness; noted as a
  future extension if event-loop-level coverage is ever needed.
