# AGENTS.md

> This file is a navigation document for AI/automation agents working in this repository. It summarizes the repo's overall architecture, the responsibilities of each crate in the Cargo workspace, the boundaries between submodules under the `app/` main binary, and the engineering conventions that must be followed before making changes.
>
> It pairs with `CLAUDE.md`: `CLAUDE.md` is the engineer's handbook (commands, style, process), and this file is the **code map**. Read `CLAUDE.md` first, then use this file to locate the right crate / module.

---

## 1. Repository overview

Warp is a mostly-Rust **agentic terminal / development environment**: on top of an in-house UI framework (WarpUI), it integrates terminal emulation, an AI Agent, cloud sync (Drive), code review, completion, Notebook, settings, IPC, and more.

Top-level directories:

| Directory | Purpose |
|------|------|
| `app/` | The main binary crate (`warp`), assembling all subsystems, UI, database migrations, and the platform glue layer |
| `crates/` | 67 workspace members, library crates split by responsibility |
| `command-signatures-v2/` | An independent sub-project (`--exclude`d when running nextest) |
| `script/` | Cross-platform bootstrap, build, and presubmit scripts |
| `resources/` | Fonts, icons, shell integration scripts, shaders, and other runtime resources |
| `docker/` | Containerized build related |
| `specs/` | Product/technical spec documents |
| `.agents/skills`, `.claude/skills` | Skill descriptions for agent workflows (creating PRs, fixing bugs, feature rollout, etc.) |
| `.warp/`, `.config/`, `.cargo/`, `.vscode/` | Various tool configs |

Build system: Cargo workspace, `resolver = "2"`; `default-members` is deliberately narrowed to the subset that's compiled/tested often (see `Cargo.toml`). `serve-wasm` and `integration` are not in `default-members` by default.

License split:
- `crates/warpui` and `crates/warpui_core` → MIT
- Everything else → AGPL-3.0-only

---

## 2. Top-level architecture layers

Roughly 4 layers from bottom to top. When adding code or locating a bug, first determine which layer the change belongs to — **don't create upside-down cross-layer dependencies**.

```
app/  (main binary: assembly, entry point, platform glue, persistence migrations, UI view root)
  ↑
Product-domain crates: ai / computer_use / vim / onboarding /
              warp_completer / lsp / languages / code-review …
  ↑
Framework crates: warpui / warpui_core / warpui_extras / editor /
            ui_components / sum_tree / syntax_tree
  ↑
Infrastructure crates: warp_core / warp_util / http_client /
                websocket / ipc / jsonrpc / persistence / graphql /
                managed_secrets / virtual_fs / watcher / asset_cache …
```

Key architectural patterns (see `CLAUDE.md` for details):

1. **Entity-Handle system**: `App` globally owns all view/model entities; Views reference each other via `ViewHandle<T>` rather than owning directly.
2. **Element / Action**: the UI is composed of a declarative Element tree + an Action event system (Flutter-style).
3. **Cross-platform**: native macOS / Windows / Linux implementations plus a WASM target; platform code is isolated with `#[cfg(...)]`.
4. **AI integration**: Agent Mode and context indexing, with code concentrated in `app/src/ai` (389 files) and `crates/ai`.
5. **Cloud sync**: `Drive` syncs objects across devices; see `app/src/drive` and `crates/warp_files`.
6. **Feature flags**: runtime gradual rollout takes priority over `#[cfg]`; the enum is defined in `crates/warp_core/src/features.rs`.

---

## 3. `crates/` overview

The table below lists all 67 crates grouped by topic. Each row is **one sentence of responsibility**; for implementation details, open the corresponding `crates/<name>/src/lib.rs` directly (many crates have `//!` module docs at the top of `lib.rs`).

### 3.1 UI framework / view layer

| Crate | Responsibility |
|-------|------|
| `warpui_core` | WarpUI framework core (MIT): infrastructure such as `App` / `Entity` / `ViewHandle` / `AppContext` |
| `warpui` | WarpUI's upper-level components, Element tree, layout, render pipeline (MIT) |
| `warpui_extras` | WarpUI's optional extensions; not all features are enabled by default |
| `ui_components` | High-level component library reused across views (buttons, inputs, lists, modals, etc.) |
| `editor` (`warp_editor`) | Text editor: buffer, selection, cursor, key mapping, undo stack |
| `sum_tree` | Persistent balanced B-tree; the core data structure for the editor / Notebook / large lists |
| `syntax_tree` | Tree-sitter wrapper and syntax highlighting support |
| `markdown_parser` | Markdown parsing (used for AI messages, doc views, Notebook, etc.) |
| `vim` | Vim-mode keybindings and operation semantics |
| `voice_input` | Voice input support |

### 3.2 Terminal

| Crate | Responsibility |
|-------|------|
| `warp_terminal` | Terminal emulation core: PTY management, ANSI/VT parsing, grid, scrolling, shell integration hooks |
| `input_classifier` | Terminal input intent classification (plain command / natural language / AI prompt) |
| `natural_language_detection` | Natural language detection (paired with `input_classifier`) |

### 3.3 AI / Agent

| Crate | Responsibility |
|-------|------|
| `ai` | AI model client, prompt orchestration, Agent protocol, tool-calling framework |
| `computer_use` | Rust-side implementation of "Computer Use" tool capabilities (screenshot, click, type, etc.) |
| `command-signatures-v2` | Command signatures v2 (command classification metadata for AI use); an independent project, not part of the main workspace test set |
| `onboarding` | New-user onboarding flow data/state |

### 3.4 Networking / protocol / IPC

| Crate | Responsibility |
|-------|------|
| `http_client` | Workspace-wide unified HTTP client wrapper |
| `http_server` | Embedded HTTP server (local RPC, login callbacks, etc.) |
| `websocket` | WebSocket abstraction shared between native and WASM, adapted for `graphql_ws_client` |
| `ipc` | Generic typed IPC request/response protocol (inter-process) |
| `jsonrpc` | JSON-RPC implementation |
| `lsp` | Language Server Protocol client implementation |
| `remote_server` | Server-side logic for the remote sshd mode |
| `serve-wasm` | Helper server that hosts WASM build output (not part of compilation by default) |
| `firebase` | Firebase client tooling (crash/analytics channels, etc.) |

### 3.5 Persistence / files / resources

| Crate | Responsibility |
|-------|------|
| `persistence` | Diesel + SQLite persistence layer foundation; **migrations live in `app/migrations/`, schema in `app/src/persistence/schema.rs`** |
| `warp_files` | Syncable file objects such as Drive files, Workflow, Notebook |
| `virtual_fs` | Abstract file system (test mock and production real FS share the same interface) |
| `repo_metadata` | Repo metadata: file tree construction, `.gitignore` handling, file system watching |
| `watcher` | File system watcher (wraps `notify`) |
| `asset_cache` | Disk/memory cache for resources |
| `asset_macro` | Resource-reference macros such as `bundled!` / `theme!` |
| `managed_secrets` / `managed_secrets_wasm` | Keychain / DPAPI / Linux Keyring abstraction + WASM proxy |

### 3.6 Configuration / settings

| Crate | Responsibility |
|-------|------|
| `settings` | Settings storage and change dispatch |
| `settings_value` | `SettingsValue` trait: controls TOML serialization semantics |
| `settings_value_derive` | `#[derive(SettingsValue)]` proc macro (e.g. converting enum variants to snake_case) |
| `warp_features` | High-level feature-flag API (consumer side) |
| `channel_versions` | Release channels (stable/preview/dogfood) and version comparison |

### 3.7 Commands / completion / languages

| Crate | Responsibility |
|-------|------|
| `command` | Safe wrapper for cross-platform process spawning, **with special handling for Windows' `no_window` flag**; all new subprocess spawns should go through here |
| `warp_completer` | Completion engine (supports `--features v2`) |
| `languages` | Language/extension/Tree-sitter grammar registration |
| `warp_ripgrep` | Thin ripgrep wrapper used by `warp_cli` |
| `warp_cli` | CLI subcommand parsing inside the binary (`warp <subcmd>`) |
| `fuzzy_match` | Fuzzy matching + glob-style wildcards, used for path search and the command palette |

### 3.8 Platform / system services

| Crate | Responsibility |
|-------|------|
| `app-installation-detection` | Detects apps already installed on the system (used for launcher integration) |
| `prevent_sleep` | Suppresses sleep (during long tasks/AI Agent runs) |
| `isolation_platform` | Compatibility layer for running inside sandboxes such as Docker / GitHub Actions |
| `node_runtime` | Automatically installs/manages Node.js and npm (macOS/Linux/Windows × multiple architectures) |
| `warp_js` | Helper abstraction for operating on JavaScript values/functions from the Rust side |

### 3.9 General utilities / communication

| Crate | Responsibility |
|-------|------|
| `warp_core` | The lowest-level "core" in the workspace: platform abstraction, the `FeatureFlag` enum in `features.rs`, and `DOGFOOD/PREVIEW/RELEASE_FLAGS` |
| `warp_util` | General-purpose utility functions reused across multiple crates |
| `warp_logging` | Unified entry point for log configuration |
| `simple_logger` | Simple async file logging for stderr-only processes such as `remote_server` |
| `warp_web_event_bus` | Web-side event bus (for the embedded web view) |
| `field_mask` | gRPC/Proto-style FieldMask utility |
| `string-offset` | Base offset types (byte/char/utf16) |
| `handlebars` | Handlebars template engine wrapper |
| `integration` | Integration test framework, used only for testing |

> Naming gotchas: `crates/editor`'s package name is `warp_editor`; `crates/isolation_platform` is `warp_isolation_platform`; `crates/managed_secrets` is `warp_managed_secrets`; `crates/virtual_fs` is `virtual-fs` (hyphenated); `crates/string-offset` is `string-offset` (hyphenated).

---

## 4. `app/` submodule navigation

`app/src/` has 60+ product-domain directories laid out flat, each roughly corresponding to one product feature line. Grouped by topic below; the number in parentheses is the approximate `.rs` file count, for estimating module size:

### 4.1 Startup / assembly / global
- `bin/` (7) — multiple binary entry points (main program, side tools).
- `lib.rs` / `app_state.rs` / `app_state_tests.rs` — application state root.
- `app_menus.rs`, `app_services/`, `app_id_test.rs`
- `appearance.rs`, `gpu_state.rs`, `font_fallback.rs`, `global_resource_handles.rs`
- `dynamic_libraries.rs`, `alloc.rs`, `tracing.rs`, `profiling.rs`
- `crash_recovery.rs`, `crash_reporting/` (4)
- `features.rs` — `app/`'s consumption of `warp_core::FeatureFlag`; adding a new flag usually needs updating both places.
- `channel.rs`, `download_method.rs`, `autoupdate/` (8)

### 4.2 Terminal
- `terminal/` (427) — the main body: shell process, PTY, grid, blocks, shell integration, command execution, I/O pipeline.
- `default_terminal/` (2) — default terminal startup logic.
- `shell_indicator.rs`, `prefix.rs` / `prefix_test.rs` (command prefix parsing), `vim_registers.rs`

### 4.3 AI / Agent
- `ai/` (389) — includes Agent UI, conversation model, Agent management, tools/MCP, Cloud Agent, Plan/Diff views, artifacts, blocklist, execution profiles, etc. **This is the largest subtree in the repo**; before making changes, grep the specific subtopic within this directory first (`agent_*`, `conversation_*`, `cloud_agent_*`, `mcp`, `tool_*`).
- `ai_assistant/` (9) — legacy AI-assist entry point/adapter.
- `chip_configurator/`, `context_chips/` (22) — Agent context chip selection/construction.
- `coding_entrypoints/` (5), `coding_panel_enablement_state.rs`
- `prompt/` (2), `tips/` (3), `voice/` (2), `completer/` (3)

### 4.4 Editor / code / review
- `editor/` (38) — main editor integration.
- `code/` (52) — code view, diff, navigation.
- `code_review/` (36) — code review flow.
- `notebooks/` (30), `workflows/` (22)

### 4.5 Search
- `search/` (172) — multi-target search (files, commands, Agent history, etc.).
- `search_bar.rs`

### 4.6 Server communication / Drive / sync
- `server/` (55) — HTTP/WS interaction with the warp backend (corresponds to the local dev mode `with_local_server`).
- `drive/` (45) — cloud object sync entry point.
- `cloud_object/` (12) — cloud object abstraction layer (workflow, notebook, etc.).
- `remote_server/` (5) — client-side glue for connecting to remote-mode sshd.

### 4.7 Settings / user config / themes / onboarding
- `settings/` (46), `settings_view/` (63)
- `user_config/` (6), `themes/` (11), `appearance.rs`
- `experiments/` (7), `tab_configs/` (15), `launch_configs/` (4)
- `tips/`, `banner/` (3), `quit_warning/` (1), `wasm_nux_dialog.rs`, `referral_theme_status.rs`

### 4.8 Auth / billing / usage
- `auth/` (22) — login, tokens, SSO.
- `billing/` (3), `pricing/` (1), `usage/` (1), `reward_view.rs`

### 4.9 Persistence
- `persistence/` (9) — Diesel migration assembly, `schema.rs` (Diesel-generated), migration runner.
- Migration files live in the repo's top-level `migrations/` directory (managed by the Diesel CLI).

### 4.10 Platform / system integration
- `platform/` (2), `system/` (3) / `system.rs`
- `login_item/` (3), `antivirus/` (3), `network.rs`
- `external_secrets/` (1), `env_vars/` (14)
- `keyboard.rs` / `keyboard_test.rs`, `safe_triangle.rs` / `safe_triangle_tests.rs` (menu-hover safe triangle)

### 4.11 View root / panes / general UI
- `root_view.rs` / `root_view_tests.rs`
- `pane_group/` (35) — split-pane layout.
- `tab.rs`, `command_palette.rs`, `modal.rs`, `menu.rs` / `menu_test.rs`
- `palette.rs`, `notification.rs`, `resource_center/` (10)
- `view_components/` (20), `ui_components/` (14)
- `workspace/` (54), `workspaces/` (10), `voltron.rs` (multi-window/multi-workspace coordination)
- `session_management.rs`, `undo_close/` (3), `word_block_editor.rs`
- `suggestions/` (2), `input_suggestions.rs` / `input_suggestions_test.rs`
- `plugin/` (21) — plugin system integration.
- `uri/` (7) — `warp://` URL handling.
- `debug_dump.rs`, `debounce.rs`, `interval_timer.rs`, `throttle.rs`
- `linear.rs`, `resource_limits.rs`, `warp_managed_paths_watcher.rs`
- `preview_config_migration.rs` / `preview_config_migration_tests.rs`
- `window_settings.rs`, `projects.rs`

### 4.12 Test infrastructure
- `integration_testing/` (79) — end-to-end integration test support.
- `test_util/` (6) — common unit-test utilities.

---

## 5. Engineering discipline (hard constraints for agents)

> Compiled from `CLAUDE.md` and the project's custom rules; this file's verification requirement for agents is `cargo check`.

### 5.1 Must-read conventions
- **Write all code comments and docs in English** (project convention; superseded the earlier Simplified Chinese convention this fork inherited from Warp).
- For searching/grepping within the git index, use the `fff` tool or `rg -n "<keyword>" <path>`; `read_file` is only for images/binaries.
- Before opening a PR / pushing a new commit, **only** needs to pass: `cargo check`.
- Changes must be precise: **every changed line should be traceable to the user's request**; don't casually "improve" unrelated code, comments, or formatting along the way.
- Prefer simplicity: don't introduce abstractions, config, error handling, or extra features for a single use site.
- Explain multiple options and surface uncertainty rather than silently deciding for the user.
- worktree path: .worktrees/<worktree_name>/

### 5.2 Rust style (from `CLAUDE.md`)
- Don't write redundant type annotations on closure parameters.
- Use unified `use` statements at the top; don't write long fully-qualified paths — except inside `#[cfg]` branches.
- Name the context parameter `ctx` and put it last; if there's also a closure parameter, the closure goes last.
- **Delete** unused parameters outright rather than prefixing with `_`, and update call sites accordingly.
- Use inline format args for macros like `println!` / `format!` (`"{x}"` rather than `"{}", x`) to satisfy `uninlined_format_args`.
- **Never use a `_` wildcard** in `match` statements (unless truly needed); keep matches exhaustive.

### 5.2.1 Comments
Comments have a cost. They carry a maintenance burden, because they must be kept in sync
with the code they describe. It is tempting to assume that more comments is always better,
but be judicious about when a comment is actually necessary because the code cannot speak
for itself.
- **Minimalist Comments**: Assume the reader is a Senior Software Engineer. Never comment
  to explain WHAT or HOW code works if self-documenting names accomplish that.
- **Strictly "Why" Only**: Reserve inline comments strictly for non-obvious business
  rationale, workarounds for third-party bugs, complex algorithms, unidiomatic code, or
  unexpected edge cases.
- **No Line-by-Line Narrations**: Never add comments restating the syntax (e.g., omit
  `// Initialize array`, `// Loop over users`).
- **Clean Docstrings**: Keep doc comments concise. Document public APIs, arguments, types,
  and returns. Do not narrate the method's internal implementation steps.
- **Single-source of documentation**: For items/members that have a doc comment explaining
  their purpose, you do not need to repeat that explanation anywhere else. A good example
  is a float const specifying an amount of spacing. You may use a doc comment on the
  declaration if necessary, but do not repeat that where the const is *referenced*. Another
  example is function call sites. Function doc comments explain what they do. Do not repeat
  the explanation at the call site.
- **Don't enumerate function call sites in doc comments**: Function doc comments should
  document their behavior and NOT their callers, e.g. it should never say things like,
  "this is used by [certain callers]" or "this is used when...".
- **No "transformation comments"**: Do not add comments that explain *your edits*. Comments
  only need explain the *current state* of the code. Explanations of edits belong in pull
  request comments instead. You shouldn't add comments with phrases like, "this used to do
  so-and-so".
- Don't delete/change existing comments for unrelated changes.
- **These rules never override §5.10.** A comment that justifies a deliberate divergence
  from Warp — *why* the behavior change is acceptable — is required by §5.10 and is exactly
  the "why" a comment is for. Likewise a comment recording that a port was adapted to this
  fork's API shape. Neither is noise; do not delete either one under "Minimalist Comments".
- **Wrap comments at 100 columns, by hand.** `.rustfmt.toml` here sets only
  `edition = "2024"`, so rustfmt's default `max_width = 100` applies. Fill that width rather
  than wrapping early at a narrower column, so comments span as few lines as possible.
  Nothing will do this for you: rustfmt does not reflow comments (`wrap_comments` is
  nightly-only and unset), this repo has never been rustfmt-clean, and `script/precheck`
  runs `rustfmt --check` on changed files only as a *parse* check — it reports nothing about
  line width.

### 5.3 Terminal model lock (high priority!)
- Calling `TerminalModel::lock()` is extremely prone to deadlock (manifests as a frozen UI / beachball on macOS).
- Before adding a new `model.lock()`, confirm that no caller further up the stack already holds the lock; prefer passing an already-locked reference down the call stack rather than locking again.
- Minimize the scope during which the lock is held, and don't call functions that might lock again while holding it.

### 5.4 Feature flags
- Adding one: add a variant to the `FeatureFlag` enum in `crates/warp_core/src/features.rs`; add it to `DOGFOOD_FLAGS` / `PREVIEW_FLAGS` / `RELEASE_FLAGS` as needed.
- Usage: **prefer** the runtime `FeatureFlag::Xxx.is_enabled()` over `#[cfg(...)]`; only use `cfg` when it's impossible to compile without it (platform/optional dependency).
- Wrap the whole product feature, not every call site individually; **clean up the flag and dead branches** once it's stable in production.
- The UI entry point and the code path should use the same flag.

### 5.5 Database
- ORM: Diesel + SQLite.
- Any schema addition/change must go through a migration: add a new directory under `migrations/` (`up.sql` / `down.sql`); don't hand-edit `app/src/persistence/schema.rs` (generated by `diesel print-schema`).

### 5.6 Testing
- Use `cargo nextest run --no-fail-fast --workspace --exclude command-signatures-v2`.
- Put unit tests in `${filename}_tests.rs` or `mod_test.rs`, referenced at the end of the original file with:

  ```rust
  #[cfg(test)]
  #[path = "filename_tests.rs"]
  mod tests;
  ```

- Integration tests use the framework in `crates/integration`; examples are in `app/src/integration_testing/`.
- **Failing tests MUST be fixed, never deferred.** A red test — whether it
  fails, hangs/deadlocks, or is flaky/order-dependent — is a defect to fix
  now, not a footnote to log in `todo.md` and move past. "It fails on clean
  `main` too" / "it's a pre-existing harness issue" / "`cargo build` still
  passes" are explanations of *cause*, never licenses to leave it red. This
  includes test-isolation failures (a test that only passes when another
  module runs first is not hermetic — make its setup explicit per-test) and
  deadlocks that make the suite un-runnable end-to-end. Do not mark a task or
  change complete while it leaves any test in the touched crate red; if a
  green suite is genuinely blocked on something out of scope, stop and raise
  it, don't silently ship around it.

### 5.7 Cross-process commands
- Don't call `std::process::Command::new(...)` directly (it pops up a window on Windows, in particular); always go through `crates/command`.

### 5.8 Subagents / multi-agent
- Split large tasks into subtasks with **non-overlapping write domains** and dispatch them in parallel; information-gathering tasks can be parallelized.
- Do simple tasks directly; don't over-split them.
- **In a fleet round, agents do NOT run the full suite.** See `docs/FLEET-ROUND.md`.
  An agent's gate is `rustfmt --check --edition 2024 <changed files>` (about a
  second, and a real parser, so it catches the truncated-function damage that
  brace-counting misses), plus at most a `cargo check` on its own small crate.
  Full-suite verification is **batched by the coordinator**: merge every finished
  branch into one integration branch and run the suite once for all agents.
  Rationale: a cold `nextest run -p warp --lib --features gui` is 10-50 minutes
  and there are only 3 slots, so per-agent verification made the queue the
  bottleneck — agents waited over an hour to land while the same ~1,000 crates
  were recompiled per agent. Per-agent runs never tested agent *interaction*
  anyway; each built its own branch in isolation. Batching defers exactly the
  thing a single integration run is designed to catch.
- **`cargo build`/`check`/`test` MUST go through `script/agent-cargo`, on every agent, no exceptions.** This workspace is large enough that *unbounded* concurrent heavy compiles exhaust memory and can crash the whole session — this has happened more than once. The governor exists so that concurrency is bounded to what the host can actually take, and it is the only sanctioned way to invoke cargo when more than one agent is running.

  ```
  script/agent-cargo <agent-name> check -p <crate> --lib
  ```

  - **Use your own distinct `<agent-name>`.** It selects a per-agent `CARGO_TARGET_DIR`, which is not an optimization — it is what prevents *shared-target contamination*: agents on different branches sharing one target dir leak each other's cached codegen, producing phantom compile errors that reference symbols the agent never touched. (This burned a previous session and produced a bogus issue. If you hit an inexplicable "missing symbol" / "non-exhaustive match" error for code you did not touch, suspect contamination, `touch` the relevant `.proto` to force regen, and do **not** file an issue for it.)
  - **Do not hand-tune around the governor**: no bare `cargo`, no ad-hoc `flock`, no `CARGO_BUILD_JOBS` override, no "just this once, unlocked, because the queue seemed slow." The slot count and job count are set centrally (`PHOSPHOR_BUILD_SLOTS`, `PHOSPHOR_BUILD_JOBS`) and are sized to the host's RAM; overriding them per-agent re-creates the OOM the governor prevents.
  - **Size slots against `nextest`/`--all-targets`, never against `cargo check`.** A check peaks ~1-2 GB over baseline; building and *linking* the test binaries costs several GB **per parallel job**, and mold deliberately trades memory for link speed. This host has been OOM-reaped once by slots sized from a check measurement — every running agent died at once. A queued build costs minutes; an OOM costs the whole fleet. When in doubt, lower it.
  - An agent must never run a second cargo invocation (governed or not) while it already has one in flight — one at a time, full stop, even for itself.
  - If a governed command hits a tool call's timeout because the queue is long, that is expected and fine — use a longer timeout or a proper watch, not a second ungoverned attempt.
  - **Build slots are scarce; agents are not.** Many agents may run at once, but only `PHOSPHOR_BUILD_SLOTS` of them compile at any moment. Waiting for a slot is the normal, expected state — it is never a reason to bypass the governor.
  - **Batch your work around the build, not the other way round.** A blocked `agent-cargo` call blocks that agent entirely, so every extra invocation costs a fresh trip through the queue. Do all the reading, analysis, and editing you can *before* the first build; then build once, fix the whole batch of errors together, and build again. Treat "compile after each small edit" as a bug in your workflow — that pattern is what makes the queue the bottleneck.
  - Periodically verify compliance directly (`ps aux | grep cargo`, check each hit's cwd against `/proc/<pid>/cwd` to attribute it to an agent) rather than trusting every agent's self-report — agents have claimed to be serializing while a bare process was demonstrably running at the same time.
  - **Host sizing is a deliberate decision, not a default.** The slot count encodes how many concurrent compiles the current host survives. Re-tune it (and only it) when moving hosts; do not assume a number that worked elsewhere transfers.

### 5.9 TUI / GUI slash-command parity (strong constraint)
- **TUI and GUI slash commands MUST stay at feature parity**: every **non-cloud** slash command that works in the GUI should also work in the TUI. Only genuinely cloud-only / GUI-only commands (cloud/orchestration, or commands that open a GUI editor / settings panel) may be absent from the TUI, and the reason must be noted in code.
- Gate points: `StaticCommand::supports_tui()` (`app/src/search/slash_command_menu/static_commands/mod.rs`) decides which commands are executable in the TUI AND whether they appear in the TUI slash menu (the menu is filtered by `supports_tui` — see `SlashCommandDataSource::recompute_active_commands`). Execution is dispatched in `TuiTerminalSessionView::execute_tui_slash_command` (`crates/warp_tui/src/terminal_session_view.rs`); unsupported kinds fall into the GUI-only `debug_assert(false)` catch-all.
- **Steps to port a command**: (1) add a TUI handler in `execute_tui_slash_command` (menu commands: follow `model_menu` / `profile_menu`; prompt commands: follow `Compact | Plan`); (2) add the command name to `supports_tui()`; (3) the menu surfaces it automatically.
- Gotcha: `slash_command_selection_behavior` — a command with an argument whose `should_execute_on_selection == false` (e.g. `/mcp`, `/plan`) **inserts `/cmd ` text on selection instead of executing**; submitting `/cmd` then routes through `handle_submitted_input`. Porting these requires handling the select/submit routing, not just adding `supports_tui`.
- Current state and gap list: see memory `tui-slash-command-parity`.

### 5.10 Warp behavioral parity — silent regressions are NOT acceptable
A user who knows Warp MUST get the same observable behavior in this fork. The
**only** sanctioned divergences are cloud/collab removal and BYOP additions
(see the parity principle). Everything else must match Warp.

- **"Simplifying" away a Warp mechanism that changes observable behavior is a
  REGRESSION, not a simplification — and it is not acceptable.** When a Warp
  sync/port drops a flag, enum-variant payload, struct field, or code branch
  "to simplify," you MUST prove the drop does not change what the user sees. If
  it does, keep the mechanism. Collapsing a data-carrying variant into a unit
  variant (or replacing a per-instance flag with a hard-coded constant) is the
  classic trap — it silently makes two previously-distinct cases behave the
  same.
- **Canonical example (do not repeat):** Warp's `UserTakeOverReason::Stop`
  carried a resume flag so that a Ctrl-C takeover of an agent's long-running
  command *resumes* the conversation on completion, while only genuine teardown
  suppresses resume (Warp PR #12738). The port collapsed `Stop` to a unit
  variant with `should_auto_resume => false`, reintroducing the exact
  stuck-"Warping…" regression Warp had fixed. The porting comment ("unlike
  Warp… never auto-resumes") *documented the regression as if it were a design
  choice* — a comment describing a divergence is not the same as justifying it.
- **When you diverge from Warp on purpose**, the reason must be cloud/BYOP (or
  an explicitly-approved product decision recorded in `docs/DESIGN-PHOSPHOR-FORK.md`),
  and the code comment must state *why the behavior change is acceptable*, not
  merely *that* it differs.
- **Test corollary (hard rule):** a test carried over from Warp encodes Warp's
  intended behavior. If such a test fails after a sync/port, **suspect the code
  (a regression) FIRST** — do not assume the test is stale. **Never change or
  weaken a test's assertions to match a simplified fork behavior**; that hides
  the regression instead of fixing it. Changing *what a test asserts* is only
  allowed when the behavior change is itself sanctioned (cloud/BYOP/approved),
  and the reason must be recorded.
- **Mandatory Warp-mirrored coverage (hard rule).** Any behavior the fork ships
  — new, changed, or carried over — MUST have test coverage that mirrors Warp's
  for that behavior. Port Warp's tests (`warp/master`) rather than writing
  thinner fork-specific substitutes. LLM-interaction behavior is covered by
  running Warp's tests against the local/BYOP provider, NOT by dropping them.
  "It's cloud" is never self-justifying — check for a local/BYOP equivalent
  first; most of Warp's cloud-organized AI behavior (CLI-agent harnesses, the
  agent loop, tool-calling, streaming, context/prompt building) has one.
- **Any deviation requires maintainer sign-off + tracking (hard rule).**
  Deviating from Warp behavior, OR from Warp's test coverage (dropping,
  weakening, or not-porting a Warp test), requires **explicit sign-off from the
  maintainer (josh)** AND a **GitHub tracking issue** recording exactly what
  deviates and why. No silent deviations. No un-tracked coverage gaps. Do not
  drop or weaken a Warp-derived test on your own judgment.
- **Reference for comparisons: the pinned Warp stable release, NOT `warp/master`.**
  See `ORACLE.md` for the current pin, and use that commit in place of
  `warp/master` in every diff, grep and coverage measurement. `warp/master` is
  unreleased trunk that moves 50-80 tests/day; comparing against it makes the
  parity gap unmeasurable and every burndown look flat. The pin tracks the
  **latest stable** release and moves only by a recorded update to `ORACLE.md` —
  never implicitly by fetching. Porting something newer than the pin is fine when
  there's a reason; note it on the issue, and do not move the pin for it.

### 5.11 Issue → fix → PR → merge workflow (hard rule)
Every defect, regression, or Warp divergence follows this workflow — no
exceptions, and nothing merges without it:
1. **Log it as a GitHub issue first** (`gh issue create` on `jwp2987/phosphor`)
   the moment it's found, *before* fixing — so it's never lost. Include the Warp
   comparison / correct behavior where known.
2. **Fix it on a branch**, together with the Warp-mirrored test(s) that prove
   the fix (a red test that goes green, whose assertions come from Warp).
3. **Open a PR that links the issue** (`Fixes #N` / `Closes #N` in the body).
4. **The PR must be attached/linked to its issue before the merge happens** — a
   merge without a linked issue and its accompanying test coverage is not
   allowed.

---

## 6. Common entry-point quick reference

| What you want to do | Starting point |
|---------|------|
| Change terminal grid / shell integration | `crates/warp_terminal/src/`, in tandem with `app/src/terminal/` |
| Change Agent UI / conversation | grep `app/src/ai/` by topic: `agent_*` / `conversation_*` |
| Change command completion | `crates/warp_completer/` (note `--features v2`) |
| Change AI model / tool-calling protocol | `crates/ai/` |
| Add a new setting | `crates/settings_value*`, `crates/settings`; UI in `app/src/settings_view/` |
| Add a feature flag | `crates/warp_core/src/features.rs` + the call sites |
| Change cloud sync objects | `crates/warp_files` + `app/src/drive/` + `app/src/cloud_object/` |
| Change persistence structure | add a migration under `migrations/` + `crates/persistence` |
| Add a new binary tool | `app/src/bin/` |
| Platform-specific code | use `#[cfg(target_os = "...")]`; UI platform glue is in `app/src/platform/` |
| Vim mode | `crates/vim` + `app/src/vim_registers.rs` |
| Notebook / Workflow | `app/src/notebooks/`, `app/src/workflows/`, `crates/warp_files` |
| Cross-platform process spawning | `crates/command` |
| File search / watching | `crates/repo_metadata`, `crates/watcher`, `crates/warp_ripgrep` |

---

## 7. Pre-change checklist

Before touching the keyboard to change code, ask yourself once:

1. Which layer / crate / `app/src/<submodule>` does this belong to? Would the change cross a layer boundary?
2. Does it need a new dependency? If an existing workspace dependency can be reused, prefer reusing `Cargo.toml`'s `[workspace.dependencies]`.
3. Is this a product feature? Does it need to be wrapped in a feature flag?
4. Does it touch the terminal model? Does the current call stack already hold the `TerminalModel` lock?
5. Does it touch a subprocess? Does it go through `crates/command`?
6. Does it touch persistence? Does it need a migration?
7. Has the corresponding `${file}_tests.rs` been written?
8. Is `cargo check` green?
9. Does every changed line trace back to the user's request? Should any incidental "small refactor" be reverted?

Go through all 9 items above before delivering.
