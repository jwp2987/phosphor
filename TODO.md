# TODO — Phosphor: Warp parity ledger (#11) + code-review debt

**Checkbox key:** `- [ ]` open · `- [>]` **IN FLIGHT, agent assigned** · `- [~]` partial · `- [x]` done.
Added 2026-08-10 after a status report listed four in-flight items as unstarted:
the assignment lived in the operator's head and not in this file. **Record the
assignment here when you start work, not when you finish it.**

## ACTIVE WORK QUEUE (2026-08-08) — read this first

**Process, agreed with the maintainer:**
- ONE sonnet agent at a time, ONE issue at a time.
- All work happens on branch `working`. The agent touches nothing else.
- After each issue: run the build check, and if green, merge `working` into
  local `main` and move to the next.
- Work the tiers in order: trivial -> small -> medium -> large.
- Update this section as each issue lands, so it survives context compaction.
- **TICK ITEMS WHEN YOU CLOSE THE ISSUE, not at end of session.** Added 2026-08-09
  after a reconciliation found **19 closed issues still sitting as unchecked `- [ ]`
  work** — the file claimed ~19 more items of work than existed. Both directions of
  drift matter: untracked-open makes work invisible, unticked-closed makes the
  backlog look worse than it is and wastes the next reader's time.
  Check with: open issues vs `- [ ]` lines, both ways.
- **EVERY NEW ISSUE GETS TIERED AT FILING TIME.** Agreed 2026-08-09. An issue that
  exists only on GitHub and not in a tier here is invisible to the plan — the
  2026-08-08 reconciliation found 8 open issues untracked by any tier, 5 of them
  already done. File it, tier it, in the same action.

**Verification rule learned the hard way (2026-08-08):** when a signature in
`app/src` changes, verify the DEPENDENTS too, not just the crate edited.
`warp_tui` consumes `Block::{is_visible,height}` via `warp::tui_export` and was
broken by #423 because the verification run covered only the edited crates.
Build check that catches this: `-p warp -p warp_tui -p remote_server -p repo_metadata`.

**This rule bit a SECOND time the same day, in a place that check does not
reach.** `crates/integration` also consumed the old `Block::is_empty` signature
and did not compile. Nothing caught it for hours because `warp` alone does not
enable the `integration_tests` feature — `crates/integration` only compiles when
`warp` and `integration` are checked in the SAME cargo invocation, so
`cargo check -p warp` is structurally blind to it and the suite was 5614/5614
green throughout. **Only `script/precheck` runs the feature-unified check. Run
`script/precheck`, not a hand-rolled `cargo check`, before calling anything
done.** Fixed in `0aee67c11`.

**Corollary — never `| tail -N` a gate script.** The first precheck run of this
work was piped to `tail -30`, which discarded every step above the test summary.
It printed `precheck: FAILED` with the failing step scrolled off, and the visible
tail was all green. Redirect the whole log to a file and grep it.

**Third rule — WIRE WHAT YOU PORT.** If a symbol is implemented but has no
production call site, that is a defect, not a done item. Fix it in the same
change, or say so loudly with file:line evidence. Never silently accept "ported
but never wired". #207 tracks this class (12 known instances). #334 is the
worked example: `reset_pane_sizes` landed in PR #515 with passing tests and
nothing ever called it, so the issue read as done while the feature did not
exist for a user. It was closed on that basis and had to be reopened.
Every agent brief must carry this rule.

**Second rule:** VERIFY EVERY ISSUE'S PREMISE BEFORE ESTIMATING OR IMPLEMENTING.
Of 11 issues examined closely on 2026-08-08, four stated the opposite of the code
(#437, #418, #532, #548-partly) and three more were already partly done. Estimating
from titles here is unreliable.

## LANDED 2026-08-09 early — tier 2 batch verified and merged

Local main = `e1df34e3a`. `precheck: ok` — **5620 + 565 + 2178 = 8,363 tests, 0 failures.**
20 commits. Closed on evidence: #545, #396, #403, #300, #299, #205 (dup of #299).

Two real bugs the FIRST build of this batch caught, both invisible until compiled:
- `crates/editor` called the pin's `AssetCache::as_ref(app)` without the pin's
  `SingletonEntity` trait import — the #300/#403 mermaid port, uncompiled for hours.
- `convert_conversation.rs` still called the removed `ParsedSkill::try_from` inside
  an `if let Ok(..)`, so it failed **silently**: restored conversations dropped their
  skill invocations and returned an empty input list. **Grep for other `if let Ok`
  wrappers around migrated conversions — same shape, same silent-failure risk.**

This is the argument for the batch-build rule paying off, not against it: both bugs
were found in one pass, and neither would have been caught by review.

### STILL OPEN from this batch (next agent)
- `BundledSkills` multi-host router + `remote_home_directories`
- `skill_manager.rs` **merge, not port** — the fork has a fork-original
  `list_skill_inventory`/`SkillInventoryItem`/`SkillInventoryDuplicate` feature
  (consumed by `app/src/skill_manager/panel.rs`) that the pin does NOT have.
  **Enumerate its call sites BEFORE editing** — a "match the pin" rewrite deletes it.
- `parsed_skill_for_common_locations` + 2 pinned tests
- `remote_agent_context.rs`, `skill_watcher.rs` remote branch
- #353 daemon producer, #388's three sub-items
- #440 — or the above ships degraded (skills empty; `home_dir`/`global_rules` still work)

## LANDED 2026-08-09 — #147 and #289 (parallel worktree)

Local main = `682ea7eca`. `precheck: ok` — **8,384 tests, 0 failures** (+21 from these ports).

- **#147 CLOSED** — `/theme` was the only remaining sub-claim; ported TUI-only per the
  pin, reusing the fork's existing `TuiTheme` settings type. 3 tests.
- **#289 CLOSED with two limitations recorded, not hidden:**
  - `harness_output_monitor.rs` — ported AND wired into `AgentDriver::run_harness`,
    with a real non-empty `runtime_error_patterns()` for `ClaudeHarness` rather than
    a stub default (the fake-coverage trap the issue itself warns about). 8 tests.
  - `claude_transcript.rs` — ported (10 tests) but **has NO production call site**.
    The pin's remaining functions rehydrate an envelope downloaded from Warp's
    server for cloud resume; the fork has no resume feature to hook them into, so
    wiring would mean inventing one. **DECIDED 2026-08-09: KEEP.** Not
    the #207 dead-code class after all -- removal would delete 10 passing tests AND
    re-block a pinned one (`claude_code_tests.rs:562` says
    `write_session_index_entry_creates_expected_entry` needs this module). It is a
    tested primitive whose consumer does not exist yet, not dead weight.
  - `codex_transcript.rs` — not portable: the fork's `Harness` enum has **no `Codex`
    variant at all**, so nothing to wire it to at the type level. That is #183.

**Operational note:** this worktree suffered an unexplained mid-task
`reset: moving to HEAD` that discarded uncommitted work (visible in `git reflog`,
not performed by the agent). The agent redid both issues and committed
immediately rather than batching. Verified intact afterwards: 1,339 lines across
two commits. **Lesson: commit early even under a batch-build rule — the batch rule
defers the BUILD, not the commit.**

## LANDED 2026-08-09 — tier 2 batch COMPLETE (#353, #388, #487 SSH arm)

Local main = `a1f121ad6`. `precheck: ok` — **5660 + 565 + 2178 = 8,403 tests, 0 failures.**
Six pieces + 5 compile fixes. Closed: **#353, #388**. #487's SSH arm delivered.

Landed: `BundledSkills` (per-host catalogs), the `skill_manager.rs` MERGE with the
fork-original inventory feature preserved, `parsed_skill_for_common_locations` with
mixed-host refusal, `RemoteAgentContext` client reconciliation, the #353 daemon
producer + `ai/skills/remote.rs`, and #388's three real sub-items.

**Two severe bugs found only by doing the work:**
- `handle_skills_added` silently dropped EVERY remote skill via a `to_local_path()`
  guard — the remote path would have been built then discarded at the last step.
- The daemon never registered `SkillManager`/`WarpManagedPathsWatcher`/
  `HomeDirectoryWatcher` at all, so the #353 producer would have **panicked the
  daemon on startup**. Not testable without running the daemon; found by reasoning
  through the registration chain.

**The dual-HostId trap bit again** (`warp_core::HostId` vs `warp_util::host_id::HostId`
— distinct here, same type in the pin). Second time tonight in this subsystem. Only
the compiler catches it; bridge with `code::buffer_location::core_host_id_to_util`.

**#353 ships DEGRADED until #440 + `remote_context_files`:** the daemon's catalog is
empty without `daemon_bundled_resources_dir()`, so the snapshot carries `home_dir`
but no skills and no global rules. Both are in the next batch.

**Build infrastructure note:** two prechecks died mid-`cargo check` with 37G free and
`/tmp` at 12% — NOT resource pressure. Cause is the harness's 600s cap killing the
background process group. Fix: launch detached (`nohup setsid ... & disown`) and poll
the log with an `until` loop. A killed run can also report "exit code 0" — **read the
log, never trust the exit code.**

## LANDED 2026-08-10 (late) — five merges, ALL UNBUILT

**Nothing below has been compiled.** Building was frozen by the maintainer
part-way through, and every agent was correctly barred from cargo. Local `main`
is 26 commits ahead of origin; the last build-verified commit is `26b04309f`.
Treat all of this as staged, not validated.

- [x] **Licence compliance** (`b5fea7a86`, six commits A-F). Alacritty
      attribution restored — **18 files, not 16**: two upstream files were
      *renamed* not deleted (`grid_handler_tests.rs` → `grid_handler_test.rs`,
      `cell_tests.rs` → `cell_test.rs`), matched on contents since the rename
      predates our history. Headers copied per-file, not from a template —
      `control_sequence_parameters.rs` credits **the vte crate**, not
      alacritty_terminal. Also README licensing, About-page source offer
      (AGPL §13), licence CI job, libgit2/winit/genai notices, asset
      attribution, and trademark de-branding (separable, commit F).
      **The libgit2 `[licenses.exceptions]` entry was correctly REFUSED**: my
      brief's premise was wrong — `libgit2-sys` declares MIT so cargo-deny never
      sees the vendored GPL, upstream ran the same check green, and SPDX has no
      identifier for libgit2's bespoke linking exception.
- [x] **`getpwuid_r` panic** (`951be89c4`). Three tiers restored. **Found a
      SECOND panic the brief missed**: `shell.rs` called
      `User::from_uid(..).expect(..).expect(..)` directly rather than going
      through `unix.rs`, so fixing only `unix.rs` would have left the crash.
- [x] **`all_working_directories` single home** (`9fb1900fd`). Pre-emptive; the
      duplicate only appears when D2c lands. Named
      `ai/terminal_working_directories.rs` because
      `pane_group/working_directories.rs` already exists and is NOT a duplicate.
- [x] **Scripting page + warpctrl skill** (`242e84af6`). Wired to the EXISTING
      `LocalControlSettings` group. `FromStr` now accepts `"Scripting"`, so the
      already-shipped `surface.settings.open --page Scripting` action reaches it.
- [x] **`crates/lsp` + initial wiring** (`f4e99118a`). Step 1 + part of step 2.
      **This is not working LSP.**

### Corrections to the parity audit, found by doing the work
- [x] ~~External-editor Warp-bundle guard absent~~ — **FALSE POSITIVE.** The
      guard exists as `is_zap_bundle` (`external_editor/mac.rs:367`), is wired at
      the pin's call site, and both tests were already present. The audit's
      evidence (`git grep -c is_warp_bundle` → 0) was true but not evidence of
      absence — the port renamed it. It *did* surface a real bug there: the fork
      asserted `dev.warp.Zap`, which is not a real bundle id anywhere. Fixed.
- **Lesson worth keeping:** a zero grep count for a pin identifier proves the
  *name* is absent, never the *behaviour*. Two audit findings this session were
  wrong in exactly this way (this one, and `all_working_directories` "exists
  twice"). Grep for behaviour before filing.

### New, from doing the work — not previously tracked
- [x] **`warpctrl` wrapper is never bundled.** **[DONE 2026-08-10 — ported the pin's script/macos/create_warpctrl_wrapper into the .app assembly (before codesign) and the local dev bundle. Windows/Linux correctly need nothing: cli_install is macOS-gated. Ported bash test passes; all 5 channels simulated against a fake .app.]** `cli_install::warpctrl_bundle_source_path()`
      expects `Contents/Resources/bin/<warpctrl_command_name>`, and
      `grep -rn warpctrl script/` returns **nothing**. So the macOS install button
      errors with "does not contain the expected wrapper" on a locally-built
      Phosphor, and `{{warpctrl_wrapper_path}}` in the skill points at a missing
      file. `warpctrl` mode itself still works via the app binary's `--warpctrl`
      flag. Bundling scripts were off-limits to the agent.
- [ ] **Claude harness cannot receive MCP servers.** The pin stages them as a
      temp JSON passed with `--mcp-config` from `build_runner`. That flag,
      `serialize_claude_mcp_config`, and a suffix parameter on `write_temp_file`
      are all absent here. A capability port, not trait plumbing — deliberately
      not invented during the trait work. When it lands, `build_runner` will also
      need `resolved_mcp_servers`; the doc comment on `claude_code.rs` records
      this. Gemini needs nothing — it ignores both at the pin too.
- [ ] **Guard the shell-to-Rust name agreement.** The warpctrl defect was a
      silent mismatch between a bundle script's channel map and
      `crates/warp_core/src/channel/mod.rs:50`, caught only because the install
      button failed at runtime. There is now a **second** pair of the same shape
      (`oz`/`zap-oss`). A grep-based CI gate comparing the bundle scripts' maps
      against `channel/mod.rs` would prevent recurrence. Deliberately not added
      during the fix: gate wiring overlaps the `ci/clearer-test-gate` work.
- [ ] **`script/test_warpctrl_early_dispatch` not ported.** The pin has it; it
      needs a built binary. This is the missing half of the coverage — the new
      bash test proves the wrapper forwards `--warpctrl`, but nothing proves the
      binary still honours it.
- [ ] **`tui-migrate-setup` skill — NEEDS MAINTAINER SIGN-OFF (AGENTS §5.10).**
      Not merely unported: two *existing* DECLINED decisions make it
      unshippable. It resolves `gui_settings_file_path` against
      `tui_settings_file_path`, but this fork shares one app id and one
      `config_local_dir()` between GUI and TUI, so both sides of every pair
      resolve to the same file — and `warp_core::paths` has no
      `gui_config_local_dir`/`tui_config_local_dir` at all. It also treats the
      schema's `x-warp-surfaces` annotation as authoritative, and this fork
      dropped `SettingSurfaces`, so the generator emits no such annotation.
- [x] **Localized `Display` + English-literal `FromStr` on `SettingsSection`** **[DONE 35baf6e4a — issue #578. persistence_key() returns the variant name; Display stays localized. Legacy values upgrade on READ, not by migration, because the localized vocabulary is unbounded, save_app_state rewrites settings_panes wholesale so the legacy path drains itself, and an older build can still write legacy rows post-migration. Two residual cases stated not hidden: cross-locale first read still falls back to default (no regression), and zh-CN renders MCPServers and AgentMCPServers identically. surface.settings.open now takes stable keys + English names but NOT localized ones -- an agent-facing contract that only resolves in the caller's UI language is not a contract.]** **[IN FLIGHT 2026-08-10]**
      breaks settings-pane persistence round-trips if any `settings-section-*`
      key is ever translated (`persistence/sqlite.rs:2667`). Pre-existing for
      every section; the `"Network" | "网络"` arm in `FromStr` suggests someone
      already hit this once.
- [x] **LSP: the `ON DELETE CASCADE` guard arm.** **[DONE 5f2f5d103 — verified: CASCADE in the migration AND clean_up_expired_metadata's third arm restored. Both halves covered; they are not interchangeable.]** The LSP
      agent chose CASCADE (verified `PRAGMA foreign_keys = ON` is per-connection)
      but argued correctly that **CASCADE does not close the guard out** — they
      fix different halves. CASCADE makes the orphan state unrepresentable;
      the pin's guard preserves the *user's choice* by keeping the workspace row
      alive, which CASCADE deletes. Both are needed for parity.

### Immediate follow-ups when building resumes
1. **Regenerate `Cargo.lock`** — `lsp-types 0.97.0` and `fluent-uri 0.1.4` are
   new and unlocked. `--locked` builds fail until an unlocked build runs.
2. **Expect `cargo deny` to reject those two deps.** The licence job is new and
   has never run; `deny.toml` was off-limits to the LSP agent.
3. Highest residual compile risks, per the agents' own ranking: the About-page
   element-tree code (licence B, new and untypechecked), the macOS-only
   `install_warpctrl` widget, and `app_menus.rs` in D1 (macOS-gated, will not
   compile on this host or in Linux CI).

## LSP TRACK — **[>] document lifecycle CODE-COMPLETE, UNBUILT** (verdict 2026-08-10: RESTORE)

**Status: the functional gap is closed in code and has never been compiled.**
Everything above the document lifecycle was already merged and wired — the crate,
the driver (install/spawn/detect), the shutdown scan, the real `footer.rs`,
`try_connect_lsp_server` on buffer load, `format_and_save` on save, hover,
goto-definition, find-references, the context menu, vim `gd`/`gr`/`K`,
diagnostics *rendering*, and log routing to a terminal.

Nothing ever sent `didOpen` or `didChange`: the server started, the editor
connected, `refresh_diagnostics` ran, and the server held no document so it
published nothing. Hover / goto-def / find-refs returned empty for the same
reason — position queries against a document the server was never given. That is
the "looks finished, silently dead" failure mode, and it is what step 5 closes.
**Until someone builds and runs it, "functional" is a code claim, not a
measurement.**

- [x] **Step 5 — `global_buffer_model.rs`, hand-integrated** (branch
      `lsp/document-lifecycle`). `initial_content_version` on
      `BufferSource::{Local,ServerLocal}` and `latest_buffer_version` on
      `InternalBufferState`; the local `ContentChanged` subscription
      reconstructed in both `create_new_buffer` and `register_buffer_for_path`;
      `lsp_server_for_path`, `log_lsp_sync_debug`,
      `open_or_sync_document_with_lsp`, `close_document_with_lsp`,
      `handle_lsp_manager_events`, `notify_lsp_of_content_change`; didClose on
      `cleanup_file_id` / `remove_deallocated_buffers` / `rename`. The
      remote-buffer layer was untouched — the two paths are genuinely additive,
      as predicted.
- [x] **Step 6a — the wasm build break.** `code_pane.rs` was ported verbatim
      from the pin including its `#[cfg(target_family = "wasm")]`
      `CodeViewEvent::OpenLspLogs` no-op arm, but `code/wasm.rs` never declared
      the variant. Declared it, matching the pin.
- [x] **Step 6b — the `code_page.rs` LSP settings subpage.** **[DONE 2026-08-10 — hand-integrated (+931), 207 lines of tests, 16 i18n keys. Subset property confirmed FALSE as predicted. Used the pin's per-workspace shape, NOT efcaa42b8's own pre-removal version, whose global `enabled_lsp_servers` model no longer has a state layer. Also restored FormatOnSaveToggleWidget — the setting came back with LSP during the build repair but its only UI control did not, leaving code.editor.format_on_save unreachable. Deliberate divergences: no 'View logs' button (footer covers it), and rows are SORTED where the pin walks a HashMap and shuffles between frames.]**
- **`BufferState` was never divergent.** Both fork and pin carry exactly
  `file_id` + `buffer`. The two extra fields live on `InternalBufferState`
  (`latest_buffer_version`) and on `BufferSource::{Local,ServerLocal}`
  (`initial_content_version`). The handover conflated the two structs.
- **Model-event closures take 3 arguments here, not the pin's 4.**
  `ModelContext::subscribe_to_model` is
  `FnMut(&mut T, &S::Event, &mut ModelContext<T>)`; only `ViewContext` takes the
  4-arg form. So `handle_lsp_manager_events` drops the pin's `ModelHandle`
  parameter. Pasting the pin's signature compiles nowhere.
- **The pin's `new()` shape would crash the remote-server daemon.** The daemon
  registers `GlobalBufferModel` (`app/src/remote_server/mod.rs`) and never calls
  `lsp::init`; `LspManagerModel::handle` panics on an unregistered singleton.
  Resolved by gating on `has_singleton_model::<LspManagerModel>()` in `new()`
  and returning `None` from `lsp_server_for_path` — one guard on the single
  resolver every LSP entry point funnels through. This also leaves
  `buffer_location_tests` and `global_buffer_model_tests` working unchanged,
  with no test relaxed. Same hazard class as the one already documented on
  `subscribe_to_remote_server_manager`.
- **`local_code_editor_wasm.rs` needs nothing.** The handover expected
  `language_server_enabled` / `add_footer` / `with_find_references_provider` to
  be missing there; the **pin's own wasm stub has none of the three**, because
  every call site is excluded on wasm (`view.rs` → `wasm.rs`,
  `language_server_shutdown_manager` → `#[cfg(feature = "local_fs")]`, which
  wasm does not enable). The fork's stub also diverges from the pin in
  fork-favouring ways; do not sync it as part of this track.

### Adjudication verdicts (evidence-based, do not re-derive)
- 8 `LocalCodeEditorView` fields LSP-caused → restored; 3 explicitly NOT
  (`has_remote_conflict`, `auto_save_debounce_tx`, `auto_save_in_flight` — real
  parity gaps, but a general pin-sync, not this track); 0 unclear.
- The `Hoverable` + `on_right_click` render wrapper: **category (a), LSP
  fallout** — restore was correct. **Method worth reusing:**
  `git log -S base_with_handler` was *useless*, because `efcaa42b8` kept the
  binding and only changed its value, so the string count never moved. Probing
  the wrapper's *contents* (`-S on_right_click`) gave the answer. Generalisable:
  `-S` on a binding NAME proves nothing when a commit rebinds rather than
  removes.
- **Source rule:** `persisted_workspace.rs` follows the **pin**;
  `local_code_editor.rs` call sites adapt to the pin's shape. The fork base has
  `LspTask::Spawn { file_path, server_type }`, the pin has `{ file_path }` —
  pasting from the `efcaa42b8` removal diff compiles nowhere.
- **Subset rule:** wholesale replacement is only safe when the fork's file is a
  strict subset of the pin's. True for `footer.rs` (verified before copying),
  **false** for `global_buffer_model.rs`.

### NEW — an untracked product decision this surfaced, needs a maintainer entry
- [ ] **`lsp_server_selector.rs` is NOT an LSP-track item.** It was not removed
      by `efcaa42b8`. `app/src/terminal/view/init_project/` was deleted five days
      earlier by **`b0b1faef9`** — a separate decision, rationale *"the
      InitProject wizard is Warp cloud agent mode's first-run onboarding; openWarp
      BYOP has no cloud onboarding need"*. The selector is a leaf of a 1,901-line
      wizard (`mod.rs` 1,303 + `model.rs` 598) that would have to be reversed
      first. **That decision is in neither `DECLINED.md` nor `TODO.md`** — a third
      deliberate removal recorded nowhere, after LSP itself and the
      PersistedWorkspace/indexing retirement. Per §5.10 the rationale also
      deserves a second look: `/init` is a **local** flow, so "cloud onboarding"
      may be the wrong frame. Needs a maintainer verdict either way.

### Done
- [x] `crates/lsp` restored verbatim, 22 tests unweakened (`f4e99118a`)
- [x] initial app wiring — deps, `lsp::init`, `FeatureFlag::LSPAsATool`,
      terminate hook, `lsp_logs.rs`, `lsp_telemetry.rs` catalog, 3 editor helpers
- [x] `workspace_language_server` migration, re-applied onto current main (`5f2f5d103`)
- [x] PersistedWorkspace LSP **state** layer — `EnablementState`,
      `language_servers`, the seven enable/disable/query methods, `ModelEvent`
      dispatch, both sqlite functions
- [x] **the `ON DELETE CASCADE` guard arm** — both halves of the hazard now
      covered, with the reason they are not interchangeable recorded in code

### Remaining — ~2,500 lines of surgery into a diverged host, NOT started
`language_server_extension.rs`, `find_references_view.rs` and
`language_server_shutdown_manager.rs` all gate on the same blocker:
`LocalCodeEditorView` state the fork does not have. The agent stopped here
deliberately rather than shipping a large blind edit, and it was right to.

**The host is the problem, not the three files.** Pin `LocalCodeEditorView` has
~25 fields, the fork has 15 — and **the absences are NOT all LSP-caused**. The
file diverged in both directions, so every field needs individual adjudication
rather than a bulk restore. On top of that: ~20 methods / ~800 lines in
`local_code_editor.rs`, new `LocalCodeEditorAction` variants and dispatch, the
`CodeEditorEvent::MouseHovered` arm (an explicit no-op today at
`local_code_editor.rs:227`), render changes for the hover tooltip and
find-references card, `editor/element.rs` (+88 LSP lines), and `code/mod.rs`'s
`ShowFindReferencesCardProvider` trait. Then the two 600-700 line files land on
top of that.

Also absent and needed by the shutdown manager:
`TerminalView::canonical_session_pwd_if_local`. Restorable — its inputs
(`active_session_path_if_local`, `repo_metadata::CanonicalizedPath`) both exist
— but it needs a new `canonical_session_pwd_cache` field on `TerminalView`.

**Do not assign this as one unit.** Adjudicating the diverged host is its own
piece of work and should land before any of the three files are attempted.

**Status 2026-08-10:** step 1 + part of step 2 MERGED (`f4e99118a`). The original
agent was **resumed** and is continuing with `language_server_extension.rs`,
`find_references_view.rs`, the shutdown manager, the server selector, the
`code_page.rs` section, the persistence half (unblocked now D1 landed), and the
`ON DELETE CASCADE` guard arm. **This is not working LSP yet.**

Was the largest item with no home — removed deliberately by `efcaa42b8` and
recorded in neither `DECLINED.md` nor this file. Maintainer decided 2026-08-10
to restore it, so it is tracked work now, not an open question.

**What it is.** Language Server Protocol support: the standard that lets the
editor talk to per-language backends and get code intelligence back. Without it
this fork ships a code editor and file tree with **no** diagnostics,
go-to-definition, hover docs, find-references or formatting —
`git grep -l 'language_server\|lsp_types\|LSPServerType'` matches nothing but
yarn cache zips and the migration that dropped it.

**SCOPE CORRECTION 2026-08-10.** The figure below (~6,600) counts what the pin
*has*. What `efcaa42b8` *deleted* is **14,611 lines** — it took `code/footer.rs`
(1,910), `local_code_editor.rs` (1,365) and `settings_view/code_page.rs` (1,055)
with it. Restoring LSP means restoring those too, or establishing that the fork's
current editor works without them. Scope this before committing to an estimate.
**Also honour the `ON DELETE CASCADE` trap recorded in the D1 section** — the
`workspace_language_server` FK has no cascade and the startup join silently drops
orphans, so enabled servers read as disabled.

**Scope, measured at the pin (~6,600 lines).**
- `crates/lsp/` — 20 files / 4,891 lines. `service.rs` exposes `definition`,
  `hover`, `references`, `format`, `did_open`, `did_change`.
  `supported_servers.rs:40` lists 5 servers: rust-analyzer, gopls, pyright,
  tsserver, clangd. Tests: `config_tests.rs`, 22 test names, all absent here.
- `app/src/code/language_server_extension.rs` (625)
- `app/src/code/find_references_view.rs` (696)
- `app/src/code/language_server_shutdown_manager.rs` (152)
- `app/src/code/lsp_telemetry.rs` (203), `lsp_logs.rs` (33)
- `app/src/terminal/view/init_project/lsp_server_selector.rs`
- the `code_page.rs` settings section
- two persistence tables, dropped by
  `crates/persistence/migrations/2026-05-11-000000_drop_lsp_workspace_tables/up.sql`

**Known couplings — do not discover these late.**
- **`node_runtime` dependency.** pyright and tsserver are node-based; the pin
  installs and runs them through it. Confirm whether this fork still has
  `node_runtime` before assuming the install path works.
- **Persistence.** The tables were dropped by migration. Restoring needs a NEW
  forward migration — do not edit or revert the existing one.
- **Delta D1.** `PersistedWorkspace` owns per-workspace LSP enable/disable
  state. With this verdict, D1's LSP half is now real work rather than a stub.
- **Telemetry.** `lsp_telemetry.rs` targets a telemetry channel this fork
  disabled (`ChannelState::is_telemetry_available()` is hard-`false`). Port the
  call sites, not the transport.

**Sequencing.** `crates/lsp/` first (self-contained, has the only tests), then
the app-side wiring, then settings + persistence, then the D1 join. Not a
single-agent single-pass job.

## D1 WORKSPACE LANDING 2026-08-10 — code complete, NOT YET BUILT

Branch `feat/restore-persisted-workspace`. **Nothing here has been compiled** —
the agent was correctly barred from running cargo, so every claim below is
read-verified, not build-verified. Treat as unproven until the operator builds.

**Branch needs rebuilding before merge.** The operator's `git add -A` for #577
ran while the shared tree was on this branch and swept ~19 of its files into
`8a76a0807`. `main` has since been rebuilt with #577 as only its own 6 files
(`2c9e23a61`), so this branch's history now duplicates that work. Rebuild the
branch onto current `main` — take its 19 files plus `7e4fab4f7`, drop the rest.
Root cause: the agent was not worktree-isolated. All later agents are.

### Landed
`app/src/ai/persisted_workspace.rs` (517) + tests · `crates/ai/src/workspace.rs`
(`WorkspaceMetadata`/`WorkspaceMetadataEvent`, restored verbatim) ·
`workspace_metadata` table + migration `2026-08-09-000100_restore_workspace_metadata` ·
`app/src/persistence/{mod,sqlite}.rs` (`PersistedData.codebase_indices`, two new
`ModelEvent` variants, save/get/delete, startup read) · `app/src/lib.rs` wiring ·
`repo_metadata::index_local_directory_path` · `workspace/view.rs` sidecar repo list ·
`terminal/view.rs:11680` `navigated_to_path` on cd (**this is what makes the list
grow**) · un-stubbed `repo_picker.rs`, `directory_color_add_picker.rs`,
`new_worktree_modal.rs`, `terminal/input/repos/data_source.rs` ·
`search/command_palette/repos/` restored and wired to `QueryFilter::Repos`, which
was **dead** (audit finding 9, fixed here) · `app_menus.rs` File ▸ Open Recent.

**All 6 acceptance tests restored and un-ignored.** Zero `unimplemented!()` and
zero "retired PersistedWorkspace" strings remain. Three adaptations, none
weakening an assertion — notably `MenuAction::HoverSubmenuLeafNode` gained a
`select: bool`; passing `select: false` reproduces the pin (the pin's variant
only recorded the hover and let the workspace's `ItemHovered` handler move the
selection), whereas `select: true` would reach the same assertion by bypassing
the very mechanism the test is named after.

### Briefing corrections (my scoping was wrong on three counts)
- **The LSP leg is untenable, not merely stubbed.** I briefed "port per-workspace
  LSP state". `crates/lsp` does not exist: `efcaa42b8` deleted **14,611 lines** —
  the crate (24 files), five `app/src/code/` LSP files, `code/footer.rs` (1,910),
  `local_code_editor.rs` (1,365), `find_references_view.rs` (699),
  `settings_view/code_page.rs` (1,055), plus the SQLite table. There is no partial
  LSP restoration that type-checks. Cut and documented as `LSP SEAM`. **None of
  the 6 acceptance tests need LSP.**
- `WorkspaceMetadata`/`WorkspaceMetadataEvent` are gone from `crates/ai` — I
  assumed they survived. Restored verbatim from the pin.
- `read_project_rule_contents` does not exist here; this fork inlined it into
  `index_and_store_rules(root_path, ctx)`.
- The pin's `persisted_workspace_tests.rs` is a **zero-byte file**. The fork's own
  pre-removal tests were recovered instead (translated to English per
  `CLAUDE.local.md`).

### Two findings worth acting on independently
- [x] **Nothing prunes the recent-repos list.** **[DONE fca2bedb2 — verified: RemoveExpiredIndexMetadata present in both persisted_workspace.rs and the index manager. Pruning returned with the index manager, as predicted.]** Expiry is the index manager's job,
      and indexing is absent — so the list grows without bound until D2c lands.
- [x] **`all_working_directories` already exists as a private copy** **[DONE 9fb1900fd — now ai/terminal_working_directories.rs.]** in
      `app/src/ai/outline/native.rs`. Reunify when indexing returns; do not add a third.
- [x] **LSP-restoration trap** **[DONE 5f2f5d103 — verified: CASCADE in the migration AND the guard arm in clean_up_expired_metadata. Both halves covered.]** (documented at the `clean_up_expired_metadata` seam):
      `workspace_language_server` foreign-keys `workspace_metadata` **without
      `ON DELETE CASCADE`**, and the startup `inner_join` silently drops orphans —
      making enabled servers look disabled. Restoring that table without the guard
      arm is a live data bug. **The LSP track must honour this.**

### Cloud boundary
Only three cloud contacts, all `ServerApiProvider::get_http_client()`, **all inside
the LSP leg** (server download / install / availability probing). No cloud call
survives into the restored file. `script/check_cloud_boundary` and
`script/check_stub_coverage` both pass.

### Unverified — ranked by the agent's own confidence (it could not build)
1. **`app_menus.rs` is `#[cfg(target_os = "macos")]`** — the Open Recent
   restoration will not compile on this Linux host or in Linux CI, only the macOS
   job. **Highest risk; drop this hunk first if the branch needs de-risking.**
2. `RepoDataSource::top_n` returns `impl Iterator<..> + use<>` (pin's edition-2024
   form). The reasoning that no borrow escapes is the agent's, unverified.
3. `ModelEvent` gained two variants — only one `handle_model_event` match was
   found; a second exhaustive match anywhere would now be non-exhaustive.
4. The `lib.rs` 17-tuple destructuring (counted three times, but a mismatch here
   produces a confusing error).
5. `use ai::workspace::...` resolution against the `mod ai;` shadow in lib.rs.
6. Diesel derives on the restored types — copied from `efcaa42b8^`, so diesel
   version drift since would surface here.
7. `index_local_directory_path` delegates to a method never seen called through
   the wrapper.

## DELTA TRACK (opened 2026-08-10, maintainer) — workspace + indexing

One named track for the two bodies of work that were scoped out of other tasks
because they kept turning into "a different task wearing the same name". Both
are **local, not cloud**, and therefore parity-legit under the drop-only-cloud
principle. Neither is in DECLINED.md and neither belonged in a tier before now.

**Why they are one track.** `PersistedWorkspace` is the pin's integration point
for codebase indexing — it owns the workspace metadata that indexing keys off,
and its `CodebaseIndexManager` seams are where indexing attaches. Porting one
without the other leaves either dangling seams or an indexer with nothing to
hang it on.

### D1 — PersistedWorkspace (**MERGED 2026-08-10** `ec227975d`, UNBUILT)

Rebuilt onto current `main` rather than merged from its branch: the original
carried the polluted #577 commit and would have duplicated work `main` already
had. Unblocks the LSP persistence half, which foreign-keys `workspace_metadata`.
~1,289 lines (`git show 02b53fcd8:app/src/ai/persisted_workspace.rs`).
Local content: recent repositories / workspace metadata, per-workspace LSP
enable-disable state, project-context and project-rules wiring,
`DetectedRepositories` integration, persistence via `crate::persistence::ModelEvent`.
- It was NOT removed for being cloud. It went out attached to the indexing
  retirement (`d84dd8e4d`), taking local features with it.
- Acceptance: the **6 tests** in `app/src/workspace/view_test.rs` whose bodies
  are `unimplemented!("PersistedWorkspace retired; ...")` behind
  `#[ignore = "depends on the retired PersistedWorkspace"]` are restored from
  the pin and pass, with the `#[ignore]` removed. Do not weaken them (§5.6/§5.11).
- Its LSP-state half is coupled to **the LSP verdict** in the parity-audit
  section above. If LSP stays out, that half is stubbed; if LSP comes back, this
  is where its per-workspace state lives.
- Note: the pin's command-palette recent-repos data source (audit finding 9) is
  backed by `PersistedWorkspace`, so it lands naturally with this.

### D2 — Codebase indexing subsystem — **[x] ALL THREE STAGES LANDED `fca2bedb2`, UNBUILT**

~15.4k lines in one pass. D2a restored all 31 files with 96 pin tests
unweakened; D2b is new construction (`LocalStoreClient` over an
`EmbeddingProvider` + `VectorStore` pair, an `HttpEmbeddingProvider` posting to
the user's `/embeddings`, a `SqliteVectorStore` on the app DB via a new forward
migration); D2c registered the manager, filled every `INDEXING SEAM`, and
restored recent-repos pruning. Configuration reuses `AISettings::agent_providers`
— no new settings mechanism. `all_working_directories` is CALLED, not duplicated.

**Honest quality delta vs the pin, stated not buried:**
- **Reranking is worse.** Pin used a cross-encoder (query + fragment scored
  together); this is a bi-encoder cosine over independently-encoded vectors.
  Recall unchanged, top-few ordering measurably worse. Biggest regression.
- Search is **exact rather than approximate** — more accurate, but latency grows
  linearly with repo size where the pin's was roughly flat.
- `populate_merkle_tree_cache` is the one genuine no-op (it warmed a *remote*
  cache for other clients; writing nodes locally already did that work).
  Documented, and its caller treats the result as advisory.
- **Nothing degrades to a silent empty answer** — a missing provider raises
  `Error::NoEmbeddingProvider` naming the model, because an empty result set is
  indistinguishable from an empty store and would re-embed the repo every sync.

- [ ] **Not done, carried forward:** remote-daemon indexing (the pin's
      `LaunchMode::supports_indexing()` gate has no fork equivalent;
      `FeatureFlag::RemoteCodebaseIndexing` stands in), settings-page UI for the
      two new `CodeSettings` toggles and for picking an embedding provider (both
      reachable via `settings.toml` today), and
      `app/src/remote_server/codebase_index_model_tests.rs` (39 tests).
- [ ] **Open question for the maintainer:** reranking quality. If the bi-encoder
      ordering proves too weak in practice, the options are a local cross-encoder
      model or a provider-side rerank endpoint. Worth deciding only after someone
      uses it.

**Status:** a single agent is building all three stages. I advised against one
pass for ~12.4k lines across two crates plus a subsystem with no pin to copy;
the maintainer reaffirmed, so it is running with instructions to commit per
stage and report honestly rather than bluff completion.
32 files / **12,316 lines** at `crates/ai/src/index/full_source_code_embedding/`,
plus `app/src/ai/codebase_auto_indexing.rs` (82). The fork's
`crates/ai/src/index/` now holds only `file_outline`, `locations.rs`, `mod.rs`.

**This subsystem is MIXED, and the split is the whole design problem.** Verified
against the pin, not assumed:
- **Local core — port as-is.** `chunker/{naive,semantic}.rs`, `changed_files.rs`,
  `codebase_index.rs`, `fragment_metadata.rs`, merkle-tree logic, `manager.rs`.
  Zero cloud markers: `grep -E 'server_api|ServerApiProvider|warp_graphql|warp_server_client|crate::server'`
  over `manager.rs` and `codebase_index.rs` returns nothing.
- **Cloud seam — needs a BYOP substitute, do NOT port as-is.**
  `store_client.rs:14` defines `trait StoreClient` with `generate_embeddings`,
  `rerank_fragments`, `get_relevant_fragments`, `sync_merkle_tree`,
  `populate_merkle_tree_cache`, `update_intermediate_nodes`,
  `codebase_context_config`. Its **only non-mock impl at the pin is
  `impl StoreClient for ServerApi`** (`app/src/server/server_api/ai.rs:3332`).
  `mod.rs:25,134-166` also carries `warp_graphql` type conversions for embedding
  configs (OpenAI text-small-3-256, Voyage code-3-512, Voyage-3.5, Voyage-4).

**Correcting the 2026-08-10 audit on this point.** The audit classified semantic
codebase search as "genuinely cloud-bound rather than merely unported" and
recommended a DECLINED.md row. That is right about the code as written and wrong
about the conclusion: the cloud boundary is a **single-implementation trait**,
which is exactly the seam BYOP exists to replace — the same shape already solved
for LLM providers. And the embedding configs the pin enumerates (OpenAI, Voyage)
are ordinary third-party provider APIs a user can bring themselves. Do not file
the DECLINED.md row; the work is a local `StoreClient`, not an omission.

- D2a: port the local core (chunking, merkle, changed-file detection, index).
- D2b: implement a BYOP `StoreClient` — embeddings through the user's configured
      provider, vector storage local (sqlite, alongside existing persistence),
      rerank local or provider-side. **The fork today has no embeddings plumbing
      at all** (`grep -rli embedding app/src/ai crates/ai/src` matches only
      `persisted_workspace.rs`, `request_usage_model.rs`, `orchestration_events.rs`,
      `telemetry.rs` — none of them an embeddings client), so this is new
      construction, not a port.
- D2c: re-wire `codebase_auto_indexing` + the `CodebaseIndexManager` seams that
      D1 is leaving live and documented.
- Unblocks: **#11 code-symbol source** and **SearchCodebase** (see the
  fork-retired-outline-indexing note), plus ~90 indexing tests and
  `app/src/remote_server/codebase_index_model_tests.rs` (39).

### Sequencing
D1 → D2a → D2b → D2c. D2b is the only piece with no pin to copy from and is the
one to design before building. **D2 is ~12.4k lines and must not be handed to a
single agent in one pass** — it is a subsystem port across two crates, not a
feature.

## LANDED 2026-08-10 — test gate + #577

- [x] **Test gate was reporting green on a failed test BUILD** (`dc4853aa8`).
      `script/check_test_failures` piped nextest through `tee`, discarding the
      exit status, then judged the run purely by counting `FAIL` lines. A build
      failure emits none, so "the test binary did not compile" and "everything
      passed" were byte-identical inputs. `warp_tui` stopped compiling, ~5,700
      tests never ran, and the gate printed `ok  no change`. Now checks the
      nextest exit code (0 / 100 only) AND requires a `Summary [...]` line as
      proof a suite ran.
- [x] **The 20 test failures that gate was hiding** (`26b04309f` and four
      predecessors). 11 were code defects, 9 were wrong tests. Notable code
      defects: the TUI's `?`/`/status` sheet could not be closed with Escape
      from a real keypress (the keymap flag was never set, so the binding never
      matched); an agent-controlled alternate screen took the whole pane, leaving
      no way to reach the agent mid-command; `pane_count()` counted hidden child
      agent panes, so closing your last visible pane left an empty tab alive; two
      CLI-agent sites used `listener.is_some()` where the pin uses
      `supports_rich_status()`, so Codex status changes stole the keyboard.
- [x] **#577 remote_server granular diff-state deltas** (`8a76a0807`). The daemon
      pushed a full repo snapshot on every change; it now pushes one
      `DiffStateFileDelta` per changed file. `classify_repository_update` is
      extracted from `LocalDiffStateModel` and shared so the daemon cannot drift
      from the GUI's rules. Whole-snapshot push is retained as the fallback for
      repos with no watchable `Repository`. **Note the issue's sizing was wrong**:
      it called for replacing ~400 lines of `server_model.rs` with a persistent
      per-key model manager. That was unnecessary — the granularity was available
      from the git watcher directly, and the GUI relocation the issue described
      as prerequisite was not needed at all. Close #577.

## A BREAK I INTRODUCED, AND HOW IT WAS CAUGHT

- [x] **`main` did not compile for several hours and no one knew.** The D1
      rebuild (`a2a10c0f1`) took `app/src/ai/mod.rs` **wholesale** from a branch
      that predated `terminal_working_directories`, silently deleting the module
      declaration while leaving the file and `ai/outline/native.rs:19`'s import
      of it in place. Restored in D2c, verified in `fca2bedb2`.
      **Cause:** rebuilding a branch by `git checkout <branch> -- <files>` takes
      whole files, so any line added to those files *on main since the branch
      point* is reverted without a conflict. Merging would have conflicted;
      checkout does not.
      **Rule:** when reconstructing a branch onto a moved `main`, treat shared
      module-declaration files (`mod.rs`, `lib.rs`) as merge targets, never as
      checkout targets — or diff them against `main` afterwards.
      **Only found because an agent read the tree rather than trusting it.** With
      building frozen there was no other signal; it would have surfaced as a
      confusing error in a 66-commit build.

## OPEN QUESTIONS FROM TONIGHT'S WORK — small, need a maintainer answer

- [ ] **Should the GUI's Settings key editors also notify the TUI?** The API-key
      hot-reload hook landed matching the pin: only the `zap-tui` key CLI writes
      the revision file. But the pin gates on `LaunchMode::Tui` because upstream's
      GUI has a **separate** keyring, whereas this fork shares one — so a running
      GUI goes stale on a TUI-side change identically. Making the GUI editors
      notify too is a behaviour change with no pin to port from, so it was flagged
      rather than taken.
- [x] **Issue #578 — CLOSED 2026-08-10** with the fix referenced. The rule conflict it exposed is settled going forward: every agent brief now says do not file issues; the operator routes findings. Original text: It accurately describes
      the `SettingsSection` persistence bug (now fixed, `35baf6e4a`). The
      maintainer had asked that issues not be filed without checking first;
      AGENTS §5.11 requires an issue per defect. The two rules conflict — needs a
      ruling on which wins for agent-filed defects. All later briefs now say do
      not file.
- [x] **MCP servers and model config reach the Codex harness only as empty
      arguments.** `write_codex_mcp_servers` and `set_codex_model` /
      `set_codex_model_reasoning_effort` are fully ported and tested, but this
      fork's `ThirdPartyHarness` trait has nowhere to pass them, so they are
      reachable only with `&HashMap::new()` and `None`. One-argument change each
      once the trait carries them. Unported upstream parity debt, previously
      untracked.

## INHERITED SUBSYSTEM REMOVALS (from the Zap/OpenWarp lineage, NOT this fork)

**Provenance correction 2026-08-10.** All four removals below are authored by
`zero <1603852@qq.com>` — the upstream Zap/OpenWarp author — between 2026-04-30
and 2026-05-10. **This fork's own history starts 2026-07-18.** They are
*inherited* decisions, not undocumented decisions of this project.

That corrects how they were first written up here. `DECLINED.md` and `TODO.md`
document *this* project's calls, so they were never going to contain zero's, and
describing these as "recorded nowhere" implied a bookkeeping failure that did
not happen. The real situation is narrower and more useful: **we inherited four
large local-subsystem removals whose rationales we have not audited**, and three
of the four turned out to be worth reversing.

| commit | date | author | scale | subsystem | status |
|---|---|---|---|---|---|
| `efcaa42b8` | 05-10 | zero | −14,891 / 92 files | **LSP** | restored 2026-08-10 through the document lifecycle |
| `d84dd8e4d` | 05-10 | zero | −2,858 / 39 files | PersistedWorkspace + indexing | restored (D1 + D2) |
| `b0b1faef9` | 05-05 | zero | −2,794 / 41 files | InitProject wizard | **under review** — rationale never verified |
| `9765692e1` | 04-30 | zero | −936 / 17 files | computer-use dispatch | **being restored** |

      **[DONE 2026-08-10 — both flow through `prepare_environment_config`. Found two REAL bugs beyond scope: `--harness codex --model X` was REJECTED as an unknown Zap model, and `--harness claude --model X` silently ignored the model because `harness_model_env_vars` was never called. Both fixed. `context` not ported (would be permanently None here). Claude MCP staging NOT wired — needs `--mcp-config` + `serialize_claude_mcp_config`, a capability port; see below.]**
- [~] **Computer-use dispatch — RESTORED 2026-08-10, still not reachable.** 1,332 lines across 22 files merged; `check_cloud_boundary` green. The `DECLINED.md` contradiction is resolved (recording stays declined; targeting and dispatch are not). **But an agent still cannot drive it — two blockers OUTSIDE `9765692e1`, both found during the restore:** (see the two rows below). Originally: `crates/computer_use`
      is fully restored and green, but `create_actor()` has exactly one caller
      (the dev CLI) because the dispatch path is gone, so no agent can drive it.
      **Also resolving a live contradiction**: `execute.rs:377` says *"Computer
      Use is out of scope for this fork (see `DECLINED.md`)"* while
      `DECLINED.md:137` lists `crates/computer_use` as **not** declined and
      `:125` says **"#349 is NOT covered"**. The `DECLINED.md` rows are right;
      the code comment is wrong. Recording *is* declined (#350/#367) and stays so.
- [ ] **BLOCKER 1 — `FeatureFlag::AgentModeComputerUse` is hard-coded `false`**
      (`crates/warp_features/src/lib.rs:865`, short-circuited alongside
      `ForceLogin`, `AvatarInTabBar`, `HOARemoteControl`). Added by `5013248be`
      (zero, 2026-04-29) **one day before** the dispatch removal — same inherited
      family, and it means `app/src/ai/agent/api.rs:424` computes
      `computer_use_enabled` as always false. **It is not a cloud flag** — it
      gates a client capability. **One-line change, but it also controls
      settings-page visibility, so it is a maintainer call.**
- [ ] **BLOCKER 2 — the BYOP tool registry has no computer-use tool.**
      `app/src/ai/agent_providers/tools/REGISTRY` lists ~20 `OpenAiTool`
      descriptors with no `use_computer` / `request_computer_use` entry, so no
      model is ever offered the tool. Relatedly `AIRequestInput::computer_use_enabled`
      is **set and never read** — at the pin it travels to Warp's server, which
      owns tool selection; BYOP builds the tool list locally instead. Closing this
      means writing JSON schemas for the full action set plus `result_to_json`.
      **Genuinely new work with no pin reference** (the pin's schema is
      server-side), not a restore. This is the larger of the two.
- [>] **InitProject** — review agent assigned 2026-08-10, read-only.
      The "cloud agent mode's first-run onboarding" rationale came from zero's
      commit message and **has been repeated through several handovers without
      anyone reading the code**. `/init` is a local flow, so the framing is
      suspect. The review will answer what it does, whether it is cloud or local,
      its relationship to `/init`, and whether to restore, partly restore, or
      formally decline it. `lsp_server_selector.rs` went with it.

- [ ] **Still worth a guard, but scoped honestly.** A CI check flagging large
      non-cloud deletions without a `DECLINED.md` row or issue would not have
      caught any of the four — they predate this fork. It would prevent *future*
      ones, and it is cheap. Lower priority than first framed.



Four deliberate removals of **local** subsystems surfaced on 2026-08-10, every
one found by an agent doing unrelated work, and **every one recorded in neither
`DECLINED.md` nor `TODO.md`**. The audit did not catch them because it keys on
pin tests, and these carry few or none.

- [x] **SUPERSEDED — see the in-flight entry above.** `9765692e1` (2026-04-30) — client-side computer-use dispatch, 17 files,
      −936 lines. VERIFIED, and it carries an active documentation
      contradiction.** Removed both executors
      (`execute/{use_computer,request_computer_use}.rs`), the `crates/ai` action
      and action_result variants (`UseComputer`, `RequestComputerUse`,
      `UseComputerResult`, `RequestComputerUseResult`, `ScreenDimensions`), their
      protobuf conversions, the `block.rs` ViewScreenshot lightbox, the render
      and persistence paths, and gutted `conversation.use_computer_action_ids()`
      to `std::iter::empty()`. Inbound `Tool::UseComputer` now returns
      `UnexpectedTool`.
      **Not cloud** — the executors call `computer_use::create_actor()` locally
      and the pin's versions run entirely client-side.
      **Consequence:** #349's port is complete and the feature still cannot work.
      `create_actor()` has exactly one caller, the `use_computer` dev CLI.
      **The contradiction:** `app/src/ai/blocklist/action_model/execute.rs:377`
      says *"Computer Use is out of scope for this fork (see `DECLINED.md`)"* —
      but `DECLINED.md:137` lists `crates/computer_use` under **"Not declined —
      common false positives"**, and `DECLINED.md:125` states outright
      **"#349 is NOT covered"**. The code cites a decision the decision file
      explicitly contradicts. **Maintainer ruling needed:** either record the
      dispatch removal as declined and fix `DECLINED.md`, or file it as debt and
      fix the comment. It cannot stay as-is.
- [x] **SUPERSEDED — under review, see the in-flight entry above.** `b0b1faef9` — InitProject wizard, 1,901 lines. Rationale given was
      "cloud agent mode's first-run onboarding", but `/init` is a **local** flow,
      so per §5.10 the framing deserves a second look. Takes
      `lsp_server_selector.rs` with it.
- [x] **`efcaa42b8` — LSP, 14,611 lines. RESTORED 2026-08-10** through the document lifecycle; builds and passes. (maintainer verdict
      2026-08-10), but the removal itself was never recorded.
- [x] **`d84dd8e4d` — PersistedWorkspace + codebase indexing. RESTORED 2026-08-10** (D1 + D2, both merged and green). D1 restored the
      workspace half; D2 is restoring indexing.

- [x] **SUPERSEDED by the scoped guard entry above** (it would not have caught these — they predate the fork). Original: Four in one day is not four oversights. Nothing in
      this project forces a removal to be recorded, and the parity audit cannot
      see them (no pin tests). Proposal: a CI guard in the spirit of
      `check_cloud_boundary` that flags a commit deleting more than N lines of
      non-cloud source unless it cites a `DECLINED.md` row or a `TODO.md` issue.
      Cheaper than any of the four restorations it would have prevented.

## LICENCE COMPLIANCE 2026-08-10 — one BLOCKING item

Read-only review against pin `02b53fcd8`. Reviewer is not a lawyer; these are
located concerns with evidence, not a legal opinion.

**Headline: the MIT question, asked about Warp, is a PASS. The failure is
against Alacritty under Apache-2.0.** `LICENSE-MIT` is byte-identical to the
pin, and upstream Warp uses no per-file copyright or SPDX headers at all
(`git grep -l 'SPDX-License-Identifier' 02b53fcd8` → 0), so nothing could have
been stripped from Warp's own code. AGPL is substantially compliant: correctly
declared `AGPL-3.0-only`, public repo, all 65 workspace members inherit it, and
the single AGPL dependency (`warp_multi_agent_api`) is compatible. No GPL-3.0 /
LGPL / BUSL / SSPL / Elastic / Commons Clause / CC-BY-NC anywhere in the graph.

- [x] **BLOCKING — restore Alacritty's Apache-2.0 attribution.** **[DONE b5fea7a86 — 18 files, not 16.]** The licence
      file `crates/warp_terminal/src/model/LICENSE-ALACRITTY` exists upstream and
      is absent from this repo *and its entire history* (stripped in the
      Zap/OpenWarp ancestor, before our history begins). The 2-line attribution
      header is gone from **16 shipping source files**; for
      `crates/warp_terminal/src/model/mode.rs` the header removal is the ONLY
      difference from the pin. Both bundling scripts had the entry deleted
      (`script/prepare_bundled_resources:107-114`, `script/windows/prepare_bundled_resources.ps1:147-154`),
      so the `THIRD_PARTY_LICENSES.txt` in every shipped release never mentions
      Alacritty. Apache-2.0 §4(a) (licence copy), §4(b) (change notices) and
      §4(c) (retain attribution) are all live and all unmet, in distributed
      artifacts. Our own `docs/DESIGN-PHOSPHOR-FORK.md:127` states the rule the
      code breaks. Mechanical fix: restore the licence file, restore 16 headers,
      re-add 2 manifest entries.
- [x] **AGPL §13 — no source offer in the shipped product.** **[DONE b5fea7a86 — README + About page. Third-party-licences VIEWER still outstanding.]** We ship a daemon
      users interact with over a network (`app/src/remote_server/`,
      `crates/remote_server/`, reached over SSH) and neither it nor the About
      page offers Corresponding Source. `README.md` has **zero** hits for
      "licen"/"AGPL"/"MIT" across 148 lines — upstream's `## Licensing` section
      (`02b53fcd8:README.md:54-58`) was dropped. About page shows only
      `Copyright 2026 Phosphor`. One fix discharges both this and the
      MIT-notice-communication problem: restore the README licensing section and
      add a source URL + third-party-licence link to the About page.
- [x] **Licence CI was dropped; the allowlists enforce nothing.** **[DONE b5fea7a86 — licenses job added; has never run, expect first-run surprises.]** `deny.toml:18`
      and `about.toml:3` both claim "CI enforces this via
      `script/check_license_config_sync`" — that script is referenced nowhere in
      `.github/` or `script/precheck`. Upstream ran `cargo deny -L error check
      licenses` AND the sync check (`02b53fcd8:.github/workflows/ci.yml:665-671`).
      Nothing now stops a GPL/BUSL/SSPL/unknown crate entering on a dep bump.
      This is why the next two items exist. Needs a cargo invocation → belongs in
      CI, not `precheck`.
- [x] **`libgit2` vendored statically, GPL-2.0 notice not emitted.** **[DONE b5fea7a86 — LICENSE-LIBGIT2 committed. The deny.toml exception was correctly REFUSED; see the merge note.]**
      `app/Cargo.toml:273-275` uses `vendored-libgit2`. Not a conflict — the
      linking exception resolves compatibility with AGPL — but `cargo about`
      reads `libgit2-sys`'s declared MIT and never emits the GPL-2.0 text that
      governs the bundled C source.
- [ ] **`winit` from a personal fork our own policy forbids — DECISION OPEN.**
      `Cargo.toml:405` pins `github.com/chenx-dust/winit` rev `7ef914a4`;
      `deny.toml:52-58` allows git sources only from `servo/core-foundation-rs`
      and the `warpdotdev` org. Licence is fine (Apache-2.0); this is
      supply-chain + policy drift. A personal fork can be force-pushed or
      deleted. `cargo deny check sources` would fail on it — except licence CI
      was silently dropped, so nothing checks. The allowlist was not wrong; the
      dependency drifted out of it unnoticed.

      **Investigated 2026-08-10 — the delta is exactly one commit.**
      `chenx-dust/winit`'s parent IS `warpdotdev/winit` (its default branch is
      literally `warpdotdev/v0.30.x`). Comparing Warp's pinned rev to ours,
      `a4e0ecb5...7ef914a4` = **ahead_by 1, behind_by 0**. That commit is
      `fix(windows): use registry value to detect dark mode`, touching only
      `Cargo.toml` and `src/platform_impl/windows/dark_mode.rs`. Nothing else
      differs. Upstream Warp pins `warpdotdev/winit@a4e0ecb5`.

      **Upstream has NOT fixed it.** The same change is
      `rust-windowing/winit#4453` (author `Slinetrac`, identical title), **open
      since 2025-12-27**, last touched 2026-05-30, 1 comment / 0 review comments,
      not draft, not rejected — simply unreviewed for ~7.5 months. So "wait for
      upstream" is not a strategy, and dropping the fix means shipping a known
      Windows dark-mode bug with no upstream fix to inherit later. **We do ship
      Windows builds** (`script/windows/prepare_bundled_resources.ps1`, MSVC
      redistributables in the bundle), so this is user-visible.

      Options: (1) fork to an org we control carrying that one commit — keeps the
      fix, kills the availability risk, lets `deny.toml` name an org we own;
      (2) vendor the one-file patch via `[patch]` against `warpdotdev/winit` — no
      third-party account in the graph at all, slightly more work per winit bump;
      (3) move to `warpdotdev/winit@a4e0ecb5` and drop the fix — simplest,
      policy-clean immediately, ships the bug. Helping review #4453 retires the
      question permanently.

      **Stray artifact to clean up:** a `jwp2987/winit` repo was created
      2026-08-10 while exploring option 1, then abandoned on maintainer
      instruction. It is a fork of `chenx-dust/winit` with **contents unchanged**
      and **nothing pushed to it**. Delete it, or keep it if option 1 is chosen.
      Deleting needs a gh scope not currently granted
      (`gh auth refresh -h github.com -s delete_repo`).

      **Interaction with the licence work:** the in-flight licence agent is adding
      `chenx-dust` to `deny.toml`'s `allow-git` so licence CI can be re-enabled
      without instantly failing. That is a CI stopgap and does NOT address
      availability. If option 1 or 2 is chosen, that entry should name the new
      source instead.
- [x] **Trademark — "Warp" branding retained across the user-facing surface.** **[DONE b5fea7a86 commit F (separable). Warpify -> Phosphorize. warpctrl BINARY rename still open; stale Zap/Zapping branding now inconsistent.]**
      AGPL §7 explicitly declines to license trademarks, so this is not covered
      by either licence. 46 occurrences in `app/i18n/en/warp.ftl` (45 ja, 47
      zh-CN): "Install the **Warp plugin**", `settings-warpify-page-title =
      Warpify`, "Install **Warp Control** CLI". Worse, `script/update_plist:261-266`
      ships macOS permission dialogs reading *"A program in Warp wants to use
      your camera / microphone / contacts / calendar / location."* Also
      Warp-branded marketing PNGs under `app/assets/async/png/`.
      `docs/DESIGN-PHOSPHOR-FORK.md:126-127` already forbids exactly this.
      Nominative use ("a fork of Warp") is fine and should stay.
- [x] **Bundled assets with no attribution or licence.** **[DONE b5fea7a86 where determinable; password.ttf / ~356-icon set / Figma recorded in docs/licensing-open-questions.md.]**
      `app/assets/bundled/fonts/password.ttf` (no licence, no provenance, present
      since Warp's first public commit); 17 file-type SVGs marked "Uploaded to:
      SVG Repo" (per-icon terms vary, several are trademarked vendor logos);
      ~29 vendor product logos; MSVC redistributable DLLs
      (`app/assets/windows/*/`) covered by neither `LICENSE-DXC` nor
      `LICENSE-WINDOWS-TERMINAL`; `resources/bundled/mcp_skills/figma/` (contrast
      the Anthropic skills, which do ship `LICENSE.txt`).
- [ ] Informational, no action forced: `lib/rust-genai` is vendored correctly with
      both licence files but is skipped by `about.toml` as a path dep, so its
      attribution never reaches the generated notice; `warpui`/`warpui_core`
      declare MIT while depending on AGPL `markdown_parser`/`sum_tree`
      (inherited from upstream, verified identical at the pin).

**Reviewer could not determine (8 items) — do not read the above as exhaustive:**
provenance of `password.ttf`; identity/licence of the ~359-icon set (naming
suggests Untitled UI, no marker in any file); per-icon SVG Repo licences;
whether the ~29 vendor logos were redistributed with permission; whether the
generated `THIRD_PARTY_LICENSES.txt` is correct in practice (could not run
cargo, so all claims about its output are derived from config, not observed);
`warpdotdev/jemallocator` + `warpdotdev/rmcp` (GitHub reports NOASSERTION);
xdotool's licence for the ported logic; and whether 2 further upstream files
carrying the Alacritty header were deleted or renamed — if renamed, the
stripped-header count rises from 16 to 18.

## PARITY AUDIT 2026-08-10 — gaps NOT in any tier (needs tiering)

Audit compared the fork against pin `02b53fcd8` using test coverage as the
signal. **Headline methodological result: the largest gaps carry 0-3 pin tests
each and are invisible to a test-count burndown** — they were found by diffing
source basenames, enum variants and crate lists, not test names. Do not treat a
green suite or a shrinking test gap as evidence of parity.

- [x] ~~LSP needs a maintainer verdict~~ — **VERDICT 2026-08-10: RESTORE.**
      Promoted out of this section into its own track below.
### MIS-TICKED / MIS-SCOPED — existing entries that misstate reality
- [x] **#323 is ticked LANDED but the Codex SDK harness driver is absent.** **[DONE 75e5dc30c — harness_kind returns ThirdParty(CodexHarness); 46 tests. Audit said 7 tests needed adapting; real number was 3.]** **[IN FLIGHT 2026-08-10 — agent porting codex.rs + codex_transcript.rs + 47 tests]**
      TODO.md:524 marks it done. What actually landed is the *local child-pane
      launch* (`app/src/pane_group/pane/local_harness_launch.rs:148
      build_local_codex_child_command`). The SDK driver was explicitly excluded
      from that work and nothing tracks the remainder, so the ledger reads
      "Codex done" while `app/src/ai/agent_sdk/driver/harness/mod.rs:134` still
      returns `HarnessKind::Unsupported(Harness::Codex)` — i.e. `oz agent run
      --harness codex` does not work. Pin: `harness/codex.rs` (943) +
      `codex_transcript.rs` (247); tests `codex_tests.rs` (38) +
      `codex_transcript_tests.rs` (9), all absent. Templates already exist in
      the fork: `claude_code.rs`, `gemini.rs`. **Untick #323 or split out the
      remainder as its own issue — do not leave it reading as complete.**
- [x] **#349's parking rationale is mis-scoped.** **[PORTED 64b6e03c6 — ~3,100 lines, not ~1,430. BLOCKED from working, see below.]** **[IN FLIGHT 2026-08-10 — agent doing platform-neutral API, then Linux X11, then macOS; will return a corrected scope]** Parked as "macOS-only, cannot
      verify on this host", but that covers neither `linux/x11/{seat,windows}.rs`
      (buildable here) nor the platform-neutral `Target`/`TargetedAction`/
      `enumerate_windows` API every caller must thread.
### PORTED BUT NEVER WIRED — the class this file's own rules call a defect
- [x] **Settings > Scripting page absent** **[DONE 242e84af6 — wired to the existing LocalControlSettings group.]** (302 lines). The fork ships the whole
      `local_control` stack, and `app/src/settings/local_control.rs:53` says
      users "must opt in through Settings > Scripting" — a page that does not
      exist. On public channels local control cannot be enabled by any
      user-reachable path. Ported-but-never-wired.
- [x] **Ctrl+Tab cycle-most-recent-TAB missing** **[DONE deea5d7ce — 19 files, not the ~187 lines estimated. features_page.rs:2800 is a vec! not a match: would have compiled clean with the mode unreachable from Settings.]** **[IN FLIGHT 2026-08-10]** (sessions-only today).
      `CtrlTabBehavior` has 2 variants, no `CycleMostRecentTab`; `QueryFilter`
      has no `Tabs`. ~187 lines.
### DEFECT-SHAPED — these are bugs, not missing features
- [x] **`getpwuid_r` panics with no fallback** **[DONE 951be89c4 — also fixed a second panic in shell.rs.]** (`terminal/local_tty/unix.rs:132-143`).
      The pin degrades getpwuid_r -> `getent passwd` -> parse `/etc/passwd`; the
      fork aborts. Breaks LDAP/SSSD hosts and some containers. ~80 lines. Defect-shaped.
- [x] **TUI API-key hot-reload hook absent** **[DONE deea5d7ce — tui_config_local_dir() NEVER existed (git log -S finds only the commits that added comments recording its absence), so this was a never-present gap, not a removal. Shared app id is properly recorded at DECLINED.md:106. Revision file lives at config_local_dir(), already a recursive watch root. No second config dir invented.]** **[IN FLIGHT 2026-08-10 — agent warned the shared GUI/TUI config dir may block it, same trap that killed tui-migrate-setup]** (46 lines). Matters more here than
      upstream because GUI and TUI share one app id and keychain namespace.
- [x] **Command-palette recent-repos data source not wired** **[DONE ec227975d — landed with D1.]** (~220 lines).
      `QueryFilter::Repos` exists with no producer behind it in the palette.
- [~] **Bundled skills `warpctrl` / `change-keybinding` / `tui-migrate-setup`** **[PARTIAL 242e84af6 — warpctrl + change-keybinding landed; tui-migrate-setup needs maintainer sign-off, see the 2026-08-10 landing section]**
      (#370 — cited in fork source, absent from this file). The fork has the
      entire local_control stack and no skill telling the agent it exists.
- [x] **External-editor Warp-bundle guard absent** **[FALSE POSITIVE — the guard exists as is_zap_bundle; the port renamed it. Fixed a real bug there instead (dev.warp.Zap is not a real bundle id).]** — "open in external editor"
      can resolve back to the app itself, so the user's editor never opens.
      Pin has `is_warp_bundle`; `git grep -c is_warp_bundle` → 0. <50 lines.
      Tests `is_warp_bundle_recognises_warp_channels`,
      `is_warp_bundle_rejects_other_apps` absent.
- [x] **remote_server client log tail absent** **[DONE deea5d7ce — 8 tests added; the pin has none.]** **[IN FLIGHT 2026-08-10]** (54 lines).
- [ ] Low confidence, verify before acting: TUI completion menu (fork's
      `completions_menu.rs` may cover it under different names);
      warpui_core telemetry ring buffer (probably belongs in DECLINED.md — the
      fork removed the telemetry channel, so an event store has no consumer).

**Also needs a DECLINED.md row rather than TODO prose:** semantic codebase
search. Verified genuinely cloud-bound, not merely unported — `StoreClient`'s
only non-mock impl at the pin is `impl StoreClient for ServerApi`, and
`full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments`.

**SCOPE-*.md is stale**: measured against fork `4f33fcf9c`, now 560 commits
behind. 9 AI-slice and 12 REST-slice verdict-D rows have since landed.
Re-verify any SCOPE row before acting on it.

## AGREED QUEUE 2026-08-09 (maintainer)

Order: **#440**, then **#381 → #382 → #236**. One sonnet agent per batch, coordinator
builds once per batch and merges on green. `TODO.md` updated at each landing.

- [x] **#440** remote_server bundled resources — unblocks the #487/#353 chain from  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      shipping degraded. Rust side small; the PACKAGING half (artifact must ship
      `bundled_resources/`) touches the release pipeline — coordinator to report
      rather than change packaging unilaterally.
- [x] **#381 — TIER 4** (maintainer, 2026-08-09). Its code rode along with the #440 batch; the ISSUE is tracked in tier 4. Scoped against `working` 2026-08-09:  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      real remaining work is **2 modules / 9 tests**, not six modules / 81.
      `remote_agent_context.rs` (4) is DONE (built under #438/#487);
      `orchestration/` (39) moved to #310/#304 when local orchestration was
      reopened; `agent_management/` (19) + `active_agent_views_model.rs` (10) stay
      DECLINED (the latter is permanently deleted; substitute pattern at
      `app/src/notifications/model.rs:275`).
      **What is left, both verified portable — every dependency present on `working`:**
      - `local_harness_setup.rs` — 98 pin lines, imports only `warp_cli::agent::Harness`,
        `FeatureFlag`, `util::path::resolve_executable`. Purely local CLI-harness
        setup, the BYOP-relevant path. Cheapest item in any tier.
      - `remote_context_files.rs` — 108 pin lines, imports `remote_server::proto`,
        `HostId`, `LocalOrRemotePath`, `RemoteServerManager`.
      **Why folded into #440 rather than worked alone:** the pin's
      `remote_agent_context.rs` consumes `RemoteContextFileProto` (`:204`) — that is
      the `global_rules` half the #353 port deliberately skipped (client
      `ProjectContextModel` has no per-host storage). #440 makes the daemon ship
      SKILLS; `remote_context_files` makes GLOBAL RULES arrive. Same files, same
      feature, one batch. Together they take remote agent context from degraded to
      complete.
- [x] **#382** — scoped against `working` 2026-08-09: **~19 real tests**, four  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      unrelated items. `prune_unreachable_subtasks` is ALREADY LANDED (`8d3f9d9ba`).
      Remaining:
      - `exchange_by_id` indexed lookup (~6 tests). Fork has only `exchange_mut`
        (linear scan) — functionally equivalent, so this is a PERFORMANCE gap, not a
        correctness one. Weigh accordingly.
      - `AmbientAgentTask::display_name`. **TRAP:** the fork HAS a `display_name` at
        `ambient_agents/task.rs:176`, but it is on `AgentSource` — a homonym. The
        pin's is on `AmbientAgentTask` (snapshot name -> title -> `"Agent"`). A grep
        alone says "already done"; it is not.
      - `file_mcp_watcher` diagnostics — zero `diagnostic` refs in the fork's file.
      - `skills/file_watchers/utils` — pin 23 tests, fork 20. Only 3 missing.
- [x] **#236** — scoped against `working` 2026-08-09: **~14 real tests**, 74% already  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      ported (pin `local_model_tests.rs` 54, fork `local_model_test.rs` 40 — note the
      fork's rename drops the plural, which has caused false "absent" readings).
      Remaining: `load_directory_with_completion` coalescing (5 pin tests, and
      `local_model.rs:231` still carries the "not yet ported" marker), plus ~6 of the
      7 symlink/lexical tests — `added_external_target_skill_symlink_routes_to_lexical_repository`
      is already ported and wired via `find_repository_for_watcher_entry_path`.

**Queue total after scoping: ~42 real tests across #381/#382/#236, not the ~113 their
titles sum to.** Consistent with every audit this session: filed counts run 2-4x high.

Deferred by explicit decision, do NOT pick up without asking:
- Tier 3.5 orchestration (6 issues, needs a new forward migration)
- #324 (live file collision with `integration/round4*` branches)
- #210 (re-file first — counts wrong in both directions, 2 rows already done)
- #405 Jupyter (whole feature), #349 (macOS half unverifiable on this host)

## RE-PIN AUTOMATION -- build during catch-up, pays off at pin N+1

Decided 2026-08-08. The catch-up against `02b53fcd8` is the FIRST pass and is
expensive by nature. Moving the pin later repeats tonight's motions, and most of
it can be mechanised -- but only if the inputs are recorded WHILE the first pass
happens. Retrofitting them afterwards costs as much as the pass itself.

**Mechanisable, worth building:**
- [ ] **Identical-to-pin manifest.** Per fork file, record whether it is
      byte-identical to the pin. Files that are identical can be fast-forwarded
      at the next re-pin with zero judgment. This single number tells us how
      cheap re-pinning actually gets. Cheap to generate as a one-off measurement
      (same method as the 2026-08-08 coverage measurement).
- [ ] **Re-pin work queue generator.** `git diff <pin N> <pin N+1>` over
      test-bearing files, bucketed by the existing `SCOPE-*.md` verdicts and
      `script/check_cloud_boundary`, so cloud-touching changes drop out
      automatically and what remains is a triaged list.
- [ ] **Divergence-collision guard.** THE ONE TONIGHT PROVED WE NEED. Flag when
      an incoming pin test collides with a deliberate fork divergence. Requires
      `DECLINED.md` entries to carry machine-checkable markers (symbol names or
      file paths), not just prose. Change how entries are written NOW, while they
      are being created anyway.
- [x] **Gates that actually run.** DONE 2026-08-08: `script/precheck` now covers
      8,342 tests across 43 packages, up from 6,181 across 3.

**Deliberately NOT automatable -- do not try:**
- Cloud-vs-local calls on ambiguous subsystems. `CLAUDE.md` already warns that
  `SCOPE-AI.md`'s verdict A is overstated (MIXED files collapse to their majority
  bucket), so a script reading those verdicts will confidently mis-bucket.
- Product divergences (e.g. the 2026-08-08 double-click decision). Maintainer's
  call, every time.
- This fork's own seams. Both focus bugs fixed tonight came from the GUI/TUI
  storage split THIS fork introduced; the skills-path issues come from cloud
  removal. Warp will never fix those, and they are where bugs concentrate.

**The discipline that keeps re-pinning cheap:** record every intentional
divergence in `DECLINED.md` the day you make it. Tonight's double-click
collision -- a July divergence contradicted by an August parity port, discovered
in neither -- cost real time purely because nobody wrote it down. `DECLINED.md`
already existed; it was not the tooling that failed.

### DECISION 2026-08-08 late — the SSH half of `ai/skills/remote.rs` is UN-DROPPED

**This partially reverses the #11 maintainer decision of 2026-08-02** ("AI skills:
build `bundled` + `global` (local); DROP the `remote` daemon-sync / cloud-repo
arm"). That verdict put two unrelated things under one label.

What the file actually is: `02b53fcd8:app/src/ai/skills/remote.rs` is **59 lines,
two functions** (`mcp_integration_wire_id`, `bundled_skill_snapshot_protos`), with
**zero cloud imports** — no `warp_graphql`, no `server_api`, no
`ServerApiProvider`, no `warp_server_client`. Its only external import is
`remote_server::proto`, which is **Phosphor's own daemon**. Its doc comment
describes the SSH path outright: *"Serializes a daemon-side bundled catalog for
the aggregate remote Agent Mode snapshot — the daemon owns the files."*

There is no cloud-repo resolution arm in that file.

**It is also a hard dependency of #353**, approved for build the same evening: the
pin's `app/src/remote_server/server_model.rs:91` imports
`bundled_skill_snapshot_protos` and calls it at `:348` to build the snapshot that
`refresh_remote_agent_context_snapshot` broadcasts at `:692`.

**CORRECTED AGAIN, later the same evening — `BundledSkills` IS needed, build it.**
I issued three different boundaries before getting this right. The final one:

| verdict | shape |
|---|---|
| **BUILD** | anything keyed by `HostId` that stores or routes context for hosts we are SSH-connected to |
| **DROPPED** | genuine cloud only — in this whole surface that is just `is_cloud_environment` |

**The proof was in the FORK, not the pin**, which is why reasoning from #11's label
kept failing. `crates/remote_server/src/manager.rs` already has, landed under #438:
- `:619` `remote_agent_context_snapshots: HashMap<HostId, RemoteAgentContextSnapshot>`
  — one snapshot PER HOST, with revision-based conflict resolution at `:2073`
- `:588` `host_to_sessions: HashMap<HostId, HashSet<SessionId>>` — multiple
  simultaneous hosts, each with multiple sessions

and `crates/remote_server/proto/remote_server.proto:294` defines
`RemoteAgentContextSnapshot { revision, home_dir, repeated RemoteSkillProto skills,
repeated RemoteContextFileProto global_rules }`.

So once #353's producer fills those snapshots, the client holds N hosts' skill
catalogs at once. `BundledSkills { local, remote_by_host: HashMap<HostId,
BundledSkill> }` is exactly the structure to hold and route them; without it a
conversation on host B resolves against host A's catalog. `remote_home_directories:
HashMap<HostId, LocalOrRemotePath>` is what proto field 2 (`home_dir`) is for, by
the same argument. Singular `BundledSkill` survives as the per-host inner type.

**The reusable lesson, which cost three reversals:** I reasoned downward from a
decision's *label* ("drop the remote arm") instead of checking what the codebase
already models. Every correction came from reading the fork's own structures. When
a scope decision seems to say "we do not support X", check whether the fork
already has data structures for X — if it does, the decision was about something
narrower than its wording.

**How this was nearly missed, which is the reusable lesson.** #487 raised exactly
this — *"the cloud-repo resolution arm is likely out of scope; the SSH/daemon arm
may not be... needs classification per-arm rather than a blanket verdict"* — and
was then closed citing #11's blanket verdict without performing the split. A
correct doubt was recorded and then overridden by a broader statement that had
never been checked against the code. **When a decision names two things with a
slash, check whether the file actually contains both.**

Porting notes for `remote.rs` (easy to lose in a port): `TuiOnly` skills are
omitted (a daemon cannot expose client-local migration behaviour); `RequiresFile`
and `RequiresFeature` are evaluated DAEMON-SIDE so the client only ever receives
`Always` or `RequiresMcp`; results are sorted by skill path so pushes are
deterministic across daemon restarts. `BundledSkill` needs an `iter_definitions()`
yielding `(id, skill, activation)` — the fork's current `iter()` drops activation.

### RECONCILIATION 2026-08-08 late — every open issue is tiered

Checked both directions programmatically (`gh issue list --state open` against
the tier lists): **0 untracked, 0 listed-but-closed.**

| bucket | count |
|---|---|
| tier 2 (in flight) | 2 — #205, #299 |
| absorbed into tier 2 | 2 — #353, #388 |
| tier 3 | 14 |
| tier 4 | 10 |
| maintainer decision, not code | 7 |
| **open total** | **35** |

Started the day at 63. The drop is **not** mostly fixes: roughly half were closed
because the premise did not hold — six were symbols the pin does not call either
(#552, #555, #547, #554, #536, #553), and several were records of completed work
that nobody closed (#523, #4, #208, #338).

**Re-run this reconciliation after any closing spree.** Eight issues were found
untracked by the tiers on 2026-08-08 — five already done and simply never closed,
#405 never tiered at all, and #4/#208 stale-open. A tier list nobody reconciles
drifts silently, and the drift always reads as "more work remaining than there is".

### RECOVERED WORK from closed-unmerged PRs (2026-08-08)

Nine PRs were closed without merging. When the workflow switched away from PRs
that morning, the in-flight ones were never triaged -- so real work sat on local
branches while the same ground was covered again. All branches survive locally.

| PR | branch | issue | status |
|---|---|---|---|
| #480 | feat/wire-local-control-cli | #216 | RECOVERED, landed `974cb9cc4` |
| #529 | ci/208-run-integration-tests | #208 | RECOVERED, landed `974cb9cc4` |
| #538 | fix/422-419-grid-clear-and-dcs | #422,#419 | RECOVERED -- real bug fixed (`reset_invalid_trailing_wide_char` now preserves `bg`, matching the oracle) |
| #546 | feat/394-411-288-cli-agent-variants | #411 | RECOVERED -- semantic conflict resolved (parse accepts `Harness::Codex`; local launch still rejects it, test relocated to prove both) |
| #565 | test/418-399-terminal-view | #418 | SUPERSEDED (my port covers it) |
| #566 | ci/multi-package-feature-check | -- | SUPERSEDED (precheck has it) |
| #489 | fix/373-ask-user-question-auto-approve | #373 | maintainer chose to leave as-is |
| #198 | chore/governor-disk-hygiene | -- | stale docs, 335 commits behind |
| #1 | review/oss-sync-shared | -- | review-only, never for merge |

**Every one of these predates the compiler-in-the-loop policy.** PR #480's own
body says "No compiler has touched this diff". They merge cleanly and compile,
but running the tests found two real defects nobody had ever seen:
- `FullGridClearBehavior` loses cell attributes across a shrink-resize (#538's
  OWN test caught it)
- `Harness::Codex` now parses where an existing test asserts it must not (#546
  vs the local-child-harness contract)

**Corrections this sweep forced, all one root cause -- treating `main` as the
only reality and never looking at the branches:**
- #208 was closed on faulty analysis (wrong directory: `src/test/` is the bin's
  scenarios; `tests/` is the real cargo test target). REOPENED.
- #532's premise was called false; it was actually written against #538's work,
  which was closed rather than merged.
- #401's "blocked by in-flight PR #480" was stale -- #480 was already closed.
- The "5 cloud_boundary_allowlist entries" figure was mine and wrong: it is ONE
  entry (4 of the lines were comments), and it is justified-local.

### Tier 1 — trivial (< 1h each)
- [x] #334 pane divider double-click -- DONE 2026-08-08 (`315cfbb57`). Data layer was
      already in (PR #515); the gesture was never wired. Ported the pin's
      `divider_mouse_down_action` into both divider variants + `PaneGroupAction::ResetPaneSizes`.
- [x] #401 warpctrl symlink installer -- DONE 2026-08-08 (`693046e02`). Also had to add
      `Channel::warpctrl_command_name()`, which the issue did not mention.
      NOTE the distinction for the 'wire what you port' rule: #334 was unwired with NO
      blocker (fix it); #401 was unwired with a DOCUMENTED blocker owned elsewhere (accept
      and record it). The rule must not force collisions.
      **The #401 blocker is now GONE** and its note above was stale in two ways: PR #480
      was closed, not in flight, and `FeatureFlag::WarpControlCli` has since arrived on
      main by another route. Both stale comments corrected in `d16d7261b`, which also
      swapped the `read_skill_tests` stand-in flag back to the real one, matching the pin
      exactly. If #401 still wants palette wiring, nothing blocks it now.

**Premises for all of tier 1 were verified against the pin on 2026-08-08 (all 8 real).**
- [x] #342 port `repository_gated_command_{drops_when_leaving,stays_within}_repository`.
      Blocker removed: `simulate_directory_for_completion` exists at `app/src/terminal/input_test.rs:515`.
      Pin source: `app/src/terminal/input/slash_command_model_tests.rs:556,627`.
      NB the issue title garbles the first test name.
- [x] #410 util/bindings: two editable-binding regressions vs the pin.
      Verified: fork declares AND registers `TOGGLE_MAXIMIZE_PANE_BINDING_NAME`
      (`pane_group/mod.rs:184,434`) but never uses it at the pin's second site,
      `terminal/view/pane_impl.rs:692`.
- [x] #436 warpui_core TuiViewportedList: no trimmed-selection-line-ends option.
      Verified absent; pin has `trim_selection_line_ends` + `trimmed_selection_row_end`
      in `crates/warpui_core/src/elements/tui/viewported_list.rs:21,168,438`.
- [x] #498 file tree: `show_hidden_files` has no Settings toggle / palette action.
      Verified: setting IS read (`code/file_tree/view.rs:357,418,726,1704`), no UI entry.
- [x] #549 duplicate dead test-fixture helpers. Verified: `app/src/test_util/virtual_fs.rs`
      and `crates/virtual_fs/src/lib.rs` both define `git_repository_fixture`/`executable`/
      `fixtures`; the ONLY callers are each file's own `git_repository_fixture` calling its
      own `fixtures()`. Trap: delete inner-first or you break the self-reference.
- [x] #547 view_components: ActionButton.callout / AlertConfig::success / Dropdown::Naked unwired.
      `AlertConfig::success` verified at zero uses; confirm the other two individually.
- [x] #552 search/ai_context_menu: `render_search_bar` never called. Verified: defined at
      `app/src/search/ai_context_menu/view.rs:1656`, no call site. (The same-named methods in
      command_palette/welcome_palette/theme_chooser ARE called — do not confuse them.)
- [x] #555 prompt/editor_modal: same-line-prompt toggle UI missing. Verified:
      `render_same_line_prompt_section` defined once at `app/src/prompt/editor_modal.rs:592`,
      never called.
- [x] #532 CLOSED 2026-08-08: #419 has now landed (recovered from PR #538) and
      `requires_registered_session`, `is_registered_session`, and
      `should_validate_dcs_hook_session_id` are present in
      `app/src/terminal/model/ansi/{dcs_hooks,mod}.rs` and `terminal_model.rs`. The
      original premise ("its premise is false, #419 hasn't landed") is now moot.

### Tier 2 — small (~half a day each)
- [x] #523 cmd-k: `try_clear_buffer_in_agent_view` still checks only `is_agent_monitoring`
      (`clear_buffer` was fixed; this one guard remains)
- [x] #545 CLI-agent image paste: keystroke is still agent-agnostic. Pin sends `ESC v`
      ONLY for `CLIAgent::Claude` on Windows; fork sends it for every agent, in BOTH
      `cli_agent_paste_keystroke_bytes` and `TerminalView::paste`.
- [x] #205 skill path classification uses client home dir, misclassifies remote skills
- [x] #299 SkillReference lacks remote/SSH path support
- [x] #300 Mermaid code block does not defer to code-block rendering while loading/failed
- [x] #313 BlocklistAIInputModel does not take an injected InputModePolicy
- [x] #342 cannot port repository_gated_command_* without simulate_directory_for_completion
- [x] #396 forking a conversation starts the new pane in the wrong working directory
- [x] #403 notebooks/editor: mermaid asset-load relayout tracking missing
- [x] #411 warp_cli: Harness has no Codex variant -- DONE 2026-08-08. Recovered from
      closed PR #546 (`feat/394-411-288-cli-agent-variants`); `Harness::Codex` parses
      everywhere including local-child-harness normalization, but local launch still
      returns "Local Codex child harness support is not yet implemented." (that gap
      is #323's scope, not #411's). Test relocated in
      `local_harness_launch_tests.rs` to assert both halves of the contract.
- [x] #422 terminal/grid: FullGridClearBehavior missing -- DONE 2026-08-08. Recovered
      from closed PR #538 (`fix/422-419-grid-clear-and-dcs`); fixed a real bug the
      port's own test caught (shrink-resize was losing cell `bg` via
      `Cell::default()` instead of preserving it -- see
      `reset_invalid_trailing_wide_char` in
      `app/src/terminal/model/grid/grid_storage/resize.rs`, matching the oracle).
- [x] #552 search/ai_context_menu: render_search_bar never called
- [x] #554 code/editor_management: CodeManagerEvent::EditCompleted has no subscriber

### Tier 3 — medium (1-3 days each)

**Fully re-audited against the pin 2026-08-08.** All 20 prior entries were
verified with file:line evidence; none came back uncertain. Result: 4 closed, 2
absorbed into the tier-2 batch, 14 remain — and most of those are NARROWER than
their titles claim. Read the issue's latest comment, not its title.

CLOSED 2026-08-08 on evidence:
- #536, #553 — dead code AT THE PIN TOO (`snapshot.rs`, `for_update`). Not gaps.
- #548 — the only `impl Slide for` in all of Warp is `oz_launch.rs`, pure cloud
  marketing. Scaffolding is faithfully ported; its one implementor is declined.
- #338 — composite; every sub-item already done, declined, or never a pin feature.

ABSORBED into the tier-2 batch (maintainer decision 2026-08-08):
- #353 — the skills full-parity work includes `remote_agent_context.rs` and the
  daemon-side producer, which IS #353's scope.
- #388 — its 3 real sub-items touch the same proto/daemon files, so folded in to
  avoid a second proto-regeneration cycle. (Sub-item 3, `GetCommittedBranchFiles`,
  is NOT a gap — the fork uses direct RPC, functionally equivalent.)

**REOPENED 2026-08-08 late — #440, and it is a HARD DEPENDENCY of in-flight work:**
- [x] #440 remote_server: bundled global skills/resources install mechanism.  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      **Reopened not because the decline was mislabelled** (a full 30-row
      `DECLINED.md` audit confirms it never claimed cloud — it is an honest
      packaging decision) **but because it became incoherent with the #487 SSH
      un-drop made the same evening.** The pin's daemon
      (`02b53fcd8:app/src/remote_server/server_model.rs:724`) gates its whole
      bundled-skill catalog on `daemon_bundled_resources_dir()`; with #440
      declined it takes the `else` branch forever, so `bundled_skill_snapshot_protos`
      — un-dropped tonight — serializes an empty catalog and #353's broadcast
      carries no skills. We would ship the entire chain inert, knowingly.
      Scope: `BUNDLED_RESOURCES_DIR_NAME` / `remote_server_bundled_resources_dir()`
      / `remote_server_removal_command()` in `crates/remote_server/src/setup.rs`,
      `daemon_bundled_resources_dir()` + the spawn in `server_model.rs`, removal
      wiring in `ssh_transport.rs:289`, **plus the packaging half** — the
      remote-server artifact must actually ship a `bundled_resources/` tree, which
      touches the release pipeline, not just Rust. Tier 3 because of that packaging
      half; the Rust side alone is small. **Do it with or before #353 ships**, or
      #353 ships degraded (it still carries `home_dir` and `global_rules`, which
      have separate sources — so degraded, not dead).

      **Reusable lesson:** the audit that cleared this row answered *"is the stated
      reason true?"* — and it was. It did not answer *"is this still consistent with
      what we decided since?"* **A decline can be individually sound and
      collectively wrong.** Re-check declines against decisions made after them.

**FILED 2026-08-09 — tiered at filing per the rule above:**

**REAL as filed:**
- [x] #284 no `received_rich_notification` latch on `CLIAgentSession`; fork derives  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      rich-status statically per agent type (`listener/mod.rs:36-38`) vs the pin's
      per-event latch (`cli_agent_sessions/mod.rs:153,412,441`). 3 pinned tests.
      **Touches the same struct as tier-2 #545** — adjacent, low risk.
- [x] #343 `BlocklistAIContextModel` has no `try_start_new_conversation` for TUI;  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      fork hard-codes the GUI path and always errors on TUI (`context_model.rs:1184`).
      **BLOCKED on #316** — needs a real `AgentViewConversationSelection` to inject.
- [x] #316 `AgentViewConversationSelection` never ported. Delegation half is real,  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      portable debt (the `AgentViewController` it needs already exists at
      `agent_view/controller.rs:778`). **The `classify_entry` half is entangled with
      the #418 DECLINED decision** — it calls `ActiveAgentViewsModel`, permanently
      deleted here; needs a `BlocklistAIHistoryModel`-based substitute, not a port.
- [x] #256 no persisted prompt-history snapshot / `prompt_history_candidates`  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      (pin `history_model.rs:331-333,2370`). Items 1/3/4 of the original issue are
      superseded by #336/#337/#331; only item 2 remains.
- [x] #431 no lazy metadata-only conversation read + summary backfill. Fork reads  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      eagerly on every startup path (`sqlite.rs:3347`). 4 pinned tests. Real perf
      AND correctness gap.
- [x] #217 CLOSED 2026-08-09 by maintainer decision. Verified REAL first (361 `"Zap"`
      literals on main; every cited example still present), so this is a deliberate
      leave-it, not a false premise. Renaming touches persisted keybinding names and
      settings keys, where a wrong move silently breaks existing users' configs, and
      the `zapctrl` vs `warpctrl` naming decision is still open. If revisited: 19 of
      the 361 are user-facing, the rest internal — that subset is the low-risk cut.
- [x] #254 NARROWED to two items: `Input::unfreeze_agent_input` (pin  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      `input.rs:7625`) and `CommandExecutionSource::SharedSession`'s `preserve_input`
      field. Items b/c are already ported (`input.rs:2037,2064`) via #399.
- [x] #323 NARROWED: `Harness::Codex` now exists (landed under #411), but local  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      Codex launch still returns "not yet implemented" (`local_harness_launch.rs:145-148`),
      and `ANTHROPIC_MODEL` merge, `normalize_orchestrator_agent_name`, and the
      OZ_CLI *prompt-text* augmentation (`local_claude_child_prompt`) are all absent.

**PARTLY REAL — scope narrowed, see each issue's re-scope comment:**
- [x] #147 ONLY `/theme` remains. `/clear`+`/set-tab-color` done; `/rename-conversation`
      is genuinely cloud-coupled; `/reset-statusline`+`/copy-debugging-id` never existed
      at the pin — **that issue cited `warp/master`, the exact ORACLE.md trap.**
- [x] #341 prompt-attachment plumbing DONE (`29049f4f8`); `register_mock_stream_for_test`  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      exists. Remaining: `schedule_auto_resume_after_error`, `fail_conversation_due_to_shell_exit`,
      `emit_response_event_for_test`.
- [x] #389 voice half DECLINED. Menu half is **ported but NOT WIRED** — `TuiReadOnlyMenuKind`  **LANDED 2026-08-09.**
      has zero call sites. Also: `status_menu.rs` landed at the WRONG PATH (top-level
      instead of nested under `terminal_session_view/`); move it, do not re-port.
- [x] #390 `state.rs` done. Remaining: `completions.rs`, `shortcuts.rs`, the  **LANDED 2026-08-09.**
      attach/detach running-command API, and `terminal_use.rs`'s missing 6th param
      `agent_owns_alt_screen_input`. **`completions.rs` is BLOCKED on #395's
      completion-menu API.**
- [x] #395 footer wording FIXED. Remaining: ask-question multiselect, blocked-action  **LANDED 2026-08-09.**
      presentation, completion-menu API shape. File-edits expand/collapse: API landed
      but the DEFAULT still diverges (fork collapses, pin expands).
- [x] #397 error tone FIXED. Remaining: statusline datetime/footer grouping  **LANDED 2026-08-09.**
      (`format_statusline_*`, `render_statusline_datetime`,
      `TuiUiBuilder::shell_command_accent_style` — all absent).

**SEQUENCING — the warp_tui cluster is NOT parallelisable.** #389/#390/#395/#397
all touch `crates/warp_tui/src/terminal_session_view.rs`; #390 depends on #395's
completion-menu API; #390 and #397 both need `TuiUiBuilder::shell_command_accent_style`.
Work them as one ordered sequence. Likewise #343 is blocked on #316 — one pair.

### Tier 3.5 — LOCAL multi-agent orchestration (reopened 2026-08-08 late)

Reversed from `DECLINED.md` on the maintainer's product call. The original
decline was correct that the code is **non-cloud** (`SCOPE-AI.md` verdict D);
what changed is that we want the feature. Reason it changed: the fork already
ships the substrate — `app/src/pane_group/pane/local_harness_launch.rs` launches
local child agents, and `agent_sdk/driver/harness/mod.rs:191-204` already stamps
`OZ_RUN_ID`/`OZ_PARENT_RUN_ID`/`OZ_CLI`, so parent-child identity is tracked
today. A 2026-08-08 audit also found **unwired, already-tested local scaffolding
sitting dead in the tree**: `children_by_parent`, `ChildAgentStatusCard`, a
de-cloud'ified `local_harness_launch.rs`. We were building the foundations while
declining the feature.

Sizing: ~72 of ~305 orchestration-adjacent pin tests are import-clean of cloud.

**Build order — these have a real dependency chain, do not parallelise:**
- [x] #310 topology + events modules (the non-cloud core, 36 pinned tests) — FIRST
- [x] #376 `AgentConversationData` fields the view reads. **Verify each field
      individually**: the issue's claim that `is_remote_child` is missing is
      FALSE, it is already present.
- [x] #304 the orchestrator/child-agent view **[DONE — issue CLOSED; orchestration_pill_bar.rs + _model.rs + avatar + conversation_links all present with tests]** (pill bar, avatar, conversation
      links, block view-impl, inline controls). Folds in **#410's second half**
      (`cycle_next/previous_orchestration_child_agent` bindings) — #410 was
      closed citing the orchestration decline, and that citation is now stale.
- [x] #325 run-agents child prompt composition **[DONE — issue CLOSED]** — **LOCAL arm only.**
- [x] #329 collapsible defaults **[DONE — issue CLOSED]** — LAST, it configures presentation of the above.
- [x] #309 topology half only. **The credit-rollup half stays declined** — Warp
      credits are a billing concept with no BYOP equivalent.

**Still declined, and this boundary matters:** the cloud-runner half. #290
(RunAgents — children executing on Warp's servers) stays out. Children run as
**local processes on this machine**. `is_remote_child` will be permanently
`false`; the pin defines it as a placeholder for a child on a remote worker.

**Warp never built "spawn an agent on your own SSH host."** That would be
fork-original work, not parity — and Phosphor has better foundations for it than
Warp does, since `remote_server` is a real daemon on the host. Do not confuse it
with `is_remote_child`.

**Persistence needs a NEW forward migration.**
`crates/persistence/migrations/2026-03-23-180000_remove_orchestration_persistence`
deleted orchestration storage deliberately; this is not a revert.

### Tier 3.5 remaining — AGREED SEQUENCE 2026-08-09

**ONE agent at a time. ONE build at a time. Each step lands green and merges before
the next starts.** Coordinator builds and merges; agents never merge.

- [x] **Step 1a** **[DONE — orchestration avatar helpers extracted]** — extract the avatar helpers into a new shared module
      `agent_view/avatar_disc.rs`. Six items, ALL pure rendering with **zero**
      pill-bar state (verified: `render_avatar_disc` has 0 references to telemetry,
      `self`, or `PillBarModel`):
      `render_orchestrator_avatar_disc` (pin pill_bar:127, 11 lines),
      `render_agent_avatar_disc` (:143, 13 lines), `pill_avatar_color` (:109),
      `pill_initial` (:117), `AvatarGlyph` (:196), `render_avatar_disc` (:2125).
      ~60-90 lines total. The pin already exposes them `pub(crate)`, so Step 2's
      pill bar imports them from here instead of defining them.
- [x] **Step 1b** **[DONE — orchestration_avatar.rs present with tests]** — `orchestration_avatar.rs` (41 lines) + `block/view_impl/orchestration.rs`
      (656). The latter uses `OrchestrationAvatar` 7x, so these go together.
      `CollapsibleExpansionState` already exists generically in `block.rs` — not
      gated on #329.
- [x] **Step 1c** **[DONE — orchestration_conversation_links.rs present]** — `orchestration_conversation_links.rs` (299). **Independent of
      1a/1b** — uses `OrchestrationAvatar` 0 times. Needs
      `TerminalAction::OpenChildAgentInNewPane` (0 in fork; note
      `RevealChildAgent` already exists and is wired, so #410's second half is
      partly done) and `AgentConversationsModel::resolve_open_action` /
      `AgentConversationNavigationSubject` (0 in fork).

      **CORRECTION 2026-08-09:** an earlier version of this plan said "the avatar
      cannot land alone" and had Step 1 reach into Step 2's 2,539-line file. That
      was wrong — the six helpers are self-contained, so 1a makes the split clean
      and no structural deviation from the pin is needed.
- [x] **Step 2** **[DONE — orchestration_pill_bar.rs present with tests]** — `orchestration_pill_bar.rs` (2,539). Port the
      `blocklist::telemetry` module FIRST (`BlocklistOrchestrationTelemetryEvent`:
      6 pin files, **0 in fork**), then the pill bar, then the new variants on
      `PaneHeaderAction`/`MenuEvent`/`WorkspaceAction`/`TerminalAction`. Own session.
- [x] **Step 3** **[DONE — #325 CLOSED]** — #325. Add `AIAgentActionType::RunAgents` (16 pin sites) and let the
      compiler walk the **59 files** matching that enum. Also needs
      `StartAgentExecutionMode`/`RunAgentsExecutionMode`/`RunAgentsAgentRunConfig`
      (all 0 in fork). LOCAL arm only. One deliberate compiler-checked pass.
- [x] **Step 4** **[DONE — #329 CLOSED]** — #329, collapsible defaults in `block.rs`. Small, and genuinely last:
      it configures presentation of steps 1-2.
- [x] **NOT IN THIS TIER** — `inline_action/orchestration_controls.rs` (~1,336) is
      **cloud**: `orchestration_controls.rs:48` imports `crate::server::experiments`.
      `DECLINED.md` covers it under the RunAgents entry; its one non-cloud caveat is
      **#11's** scope. Do not port it here.

**Deviating from this order requires asking first.** Recorded because the coordinator
changed an agreed order twice on 2026-08-09 (#381 folded into the #440 batch against
"after 440"; #405 re-tiered unasked) and both were wrong.

### Landed 2026-08-09 — untracked features (no issue filed, maintainer directed)

These shipped to `main` on 2026-08-09 without GitHub issues, by explicit maintainer
decision. Recorded here so the next port sweep finds a decision rather than
apparent debt.

- [x] **Remote file-viewer routing.** Every file opened from the remote (SSH) file
      tree used to land in the code editor: the remote branch asked one question
      (`is_supported_image_file`) and its own comment said "everything else opens via
      the buffer-sync protocol". `FileTreeEvent::OpenRemoteFile` carried no
      `FileTarget`, so no viewer choice could be expressed. **Remote markdown never
      rendered.** Fixed by threading `target` through `OpenRemoteFile` ->
      `LeftPanelEvent` -> `Workspace`, adding `SourceFile::Remote`,
      `FileNotebookView::open_remote` (over the existing `ReadFileContextRequest` RPC)
      and `RemoteServerManager::host_request_handle`. Root cause of the class:
      the pin unified local/remote behind one `LocalOrRemotePath`; this fork split
      them into two events over two `RemotePath` families.
- [x] **Remote notebook Raw-mode toggle.** `open_as_code`/`ToggleMarkdownDisplayMode(Raw)`
      were gated on `local_path()`, always `None` for remote, so Raw was a silent
      no-op. `PaneEvent::ReplaceWithCodePane.path` widened `PathBuf` ->
      `BufferLocation`. Only 4 reference sites across 3 files. Deliberately used the
      fork-native `BufferLocation` rather than renaming to the pin's
      `LocalOrRemotePath`; `ReplaceWithFilePane` left as `PathBuf` (its callers are
      local-only by design — remote panes toggle rendered/raw inline in `CodeView`).
- [x] **TUI orchestration tab bar.** The pin's TUI imported `crate::orchestration_tab_bar`,
      absent fork-wide, blocking 9 tests. Ported the module plus a **local-only**
      `TuiOrchestrationModel` fed by `orchestration_topology.rs`, dropping the pin's
      `StartAgentExecutionMode::Remote` branch (cloud-runner, declined under #290).
      `crate::tab_bar` turned out to be already ported byte-identical — the generic
      tab machinery was never the gap. All 9 tests ported unweakened.
- [x] **Per-host skills and global rules reaching agent context.** This fork had built
      per-host storage TWICE (`BundledSkills::remote_by_host` under #487/#353,
      `remote_global_rules` under #575) and wired consumption NEITHER time.
      `SkillManager` was already remote-aware and tested; the bug was call sites
      hardcoding `LocalOrRemotePath::Local(...)` regardless of session type — four of
      them. Upstream cause: `ActiveSession::current_working_directory_location`
      carried a doc comment claiming "BYOP sessions are local, so this is always a
      Local path", false since this fork tracks `SessionType::WarpifiedRemote`.
      A wrong comment propagated a wrong assumption into every consumer.
- [x] **`format_todo_progress` + statusline wiring.** Previously declined as
      "not small" because the bare function would be dead without the
      `TuiStatuslineItem`/`FooterSegment` plumbing. Ported whole. No settings
      migration needed — new items append disabled via `TuiStatuslineConfig::normalized()`,
      same as #397's Date/Time variants. **Also fixed two tests #397 left stale on
      `main`**: `ai_tests.rs` and `statusline_config_view_tests.rs` hardcoded a 7-item
      `TuiStatuslineItem::ALL` order against what is now 12 entries.

### Tier 4 — large (a week+)
- [x] #576 (replaces **#210**, closed 2026-08-09) · #382 · #236 · #324 · #405  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
- [x] **#349 PARKED 2026-08-09 (maintainer).** `computer_use` per-window activation
      (`mac/activation.rs`, `mac/window.rs`, `mac/post.rs`, `linux/x11/seat.rs`,
      `linux/x11/windows.rs`). Parked, not declined: the macOS half cannot be built or
      verified on this host, so porting it would ship code no one here can test.
      Revisit only if a macOS build host becomes available.
- [x] #575 `RemoteAgentContextSnapshot.global_rules` is always empty. Split out of the  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      #440 batch after a scope correction: I had assumed `remote_context_files.rs`
      supplied it — **it does not.** `global_rules` arrives pre-serialized in the
      snapshot from daemon-side `ProjectContextModel::global_rules()`;
      `remote_context_files.rs`'s real consumers (`metadata_project_rules.rs`,
      `skill_watcher.rs::read_project_skill_contents`) are unrelated and absent here.
      **Real scope:** daemon `ProjectContextModel::global_rules()` + client
      `set_remote_global_rules`/`remove_remote_global_rules`/`remote_global_rules`
      storage. **Blocker:** this fork's `ProjectContextModel`
      (`crates/ai/src/project_context/model.rs`) is a flat local-only `path_to_rules`
      map with no per-host scaffolding — comparable in size to the per-host skills
      work that landed under #487/#353, not a wiring change. **MOVED TO TIER 4 2026-08-09 by maintainer** — sized like the #487/#353
      per-host skills work, not like the rest of tier 3, and it was the only item
      holding tier 3 open. Verified before the move: `project_context/` has **zero**
      `HostId` references (model.rs 0, mod.rs 0, model_tests.rs 0), no `global_rules()`
      accessor exists at all, and `app/src/remote_server/server_model.rs:262` hardcodes
      `global_rules: Vec::new()`. The protocol half is already correct —
      `protocol_tests.rs` round-trips the field.
- [x] #312 NLD prompt-history match — **moved here from the maintainer-decision bucket  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      2026-08-09; it was never a decision, it is ordinary local work.** Warp's
      natural-language detection consults TWO history sources (shell command history +
      agent prompt history) and breaks ties by recency, so retyping a previous agent
      prompt locks the input to AI mode and retyping a previous shell command locks it
      to Shell. The fork consults command history only, so a previously-sent prompt is
      re-classified from scratch every time. Entirely local (both sources on disk),
      9 pinned tests blocked.
      **The issue's claim that none of the symbols exist is WRONG** — 4 of 5 are partly
      present: `HistoryMatch` 2 fork/6 pin, `InputTypeAutoDetectionSource` 5/16,
      `NldPromptHistoryMatch` 2/5, `prompt_history_candidates` 2/3. The genuinely
      absent one is **`resolve_history_match` (0 fork / 2 pin)** — the tie-break itself.
      **SEQUENCING: blocked on #256** (tier 3, in flight) — `prompt_history_candidates`
      is its prompt-side source. Once #256 lands this is `resolve_history_match` plus
      porting 9 tests.
      (#252, #289, #142 CLOSED 2026-08-08/09)
- [x] #381 — **work DONE, issue still open.** Its two real modules  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      (`local_harness_setup.rs` 5 tests, `remote_context_files.rs` 4 tests) are
      committed on `working` awaiting the tier-3 merge; the other four modules it
      named are either done (`remote_agent_context.rs`), moved to tier 3.5
      (`orchestration/`), or declined (`agent_management/`,
      `active_agent_views_model.rs`). **Close it when the tier-3 batch merges.**

**#210 was re-filed as #576 after re-measuring all ten rows against `main`.** Its
figures were wrong in BOTH directions: pin counts undercounted 2-4x on 6 of 10 rows
(`input_tests.rs` 149 not 54, `view_tests.rs` 142 not 37); three rows listed as
absent actually exist under fork-renamed paths at 78-94% ported (`input_test.rs`,
`view_test.rs`, `local_model_test.rs`) — the exact filename-not-content error #210's
own rules warned against; two rows were already closed (#142, #252); and
`pane_group/mod_tests.rs` is majority-cloud (21 marker lines: `CodebaseIndexManager`,
`IapManager`, `CloudConversationData`), not clean debt.
**~521 claimed -> ~214 genuinely portable non-cloud tests.**
- [x] #405 Jupyter (`.ipynb`) rendering. **STAYS IN TIER 4** (maintainer, 2026-08-09).  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      Scoped 2026-08-09: verdict REAL, zero cloud,
      but **~3-4x smaller than the tier-4 framing**: ~500-700 net-new lines across
      ~12 files, ~30 tests, 1-3 days. The only genuinely new code is
      `crates/ipynb_parser` (401 lines + 24 tests, self-contained nbformat-v4 JSON ->
      `FormattedText`); everything else is 2-13 line hooks into files that already
      exist, because ~90% of the scaffolding is already here (the whole
      `app/src/notebooks/` subsystem, the `FeatureFlag` mechanism, `ContentFormat`,
      `markdown_parser`, and `is_jupyter_notebook_file()` with its 6 tests already
      passing). No blocking dependencies. See the issue for the 5-step order.

**Being audited against the pin (2026-08-08), same treatment tier 3 got.** For
this tier the TEST COUNTS are the main claim -- five of these issues assert a
number of blocked tests (~733 total between #210/#381/#252/#289/#382), and that
number is what sets their priority. Verify claimed-vs-actual before acting.
Suspected double-counting: #252 vs #289 (both agent_sdk) and #381 vs #382 (both
app/src/ai). Tier 3's audit found 4 of 20 closeable and 8 more narrower than
filed, so treat these numbers as unverified until the audit lands.

Audit landed 2026-08-08 late. Results below.

- **#142 — CLOSEABLE, already done. My earlier "pull it forward, BYOP is
  untested" note here was WRONG and is retracted.** I saw `api_key` in two
  absent pin filenames and assumed BYOP. They are absent, but they are Warp's
  *cloud team API-key management* (`agent_sdk/api_key.rs` imports
  `warp_graphql::mutations::{expire_api_key,generate_api_key}` and
  `ServerApiProvider`) — programmatic tokens for Warp's own cloud API, a
  different concept from BYOP provider keys. PR #189/#227 already reconciled the
  real file: 12 ported / 3 blocked on pin-side dead code / 16 superseded by
  `AgentProviderSecrets` (the fork's actual BYOP store, 19 fork-original tests) /
  36 cloud. The "7 of 82" figure conflated four differently-scoped
  `api_key`-named files. **Lesson: a filename is not a scope.**
- **#324 overlaps work in flight.** Its `diff_state_tracker.rs`
  (`RemoteDiffStateManager`, ~472 lines) sits beside the `diff_state_remote.rs` /
  `diff_state_proto.rs` / proto files the current tier-2 batch is editing for
  #388/#353. Cheaper to do while that area is open.
- **#349 is macOS-only** — cannot be built or verified on this host regardless of
  verdict.

### Needs a maintainer decision, not code
- [x] #149 · #150 · #203 (design decision) · #206 · #207 · #279 · #312  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**

### Landed 2026-08-08
#191, #251, #253, #570, #437, #438, #439, #399, #418, #423, #208, #537 — plus
`main` going from 65 failing tests to 0, and three CI gates repaired
(`check_test_failures` was blind to `TRY n FAIL`; `precheck` compiled nothing and
ignored uncommitted work).


Reconciled 2026-08-04; **#11 section re-verified against code on `main` 2026-08-06.**
**Reconciled again 2026-08-07 (#148) — `main` = `2e7d6eb2f` (194 commits past the
`af79b705d` HANDOFF.md snapshot).** Every checkbox and issue reference below was
re-verified against `origin/main`, `gh issue view`, and `DECLINED.md` on that date;
see the commit that made this edit for the full classification.
`[x]` items in issue #11 = "keep/restore" (maintainer wants them in the fork). This
file is the live tracker: **mark an item `- [x]` the moment it's verified done.**

> **Issue #11 itself is now CLOSED (2026-08-07, `COMPLETED`).** It was fully
> reconciled: 44 of its 56 ticked items were already implemented on `main` (verified
> symbol-by-symbol), and the 10 genuinely absent are each now tracked by a specific
> issue. See the "#11 status" section below for the current table — the old
> "10 remain / 7 buildable" framing is superseded.
>
> **The old "`main` is red" story (PRs #140/#181, issue #171) is also resolved** —
> #171 is closed; PR #224 repaired the underlying regressions. `main`'s current
> red-test state is a different, smaller, fully-attributed set; see
> [Red on main](#red-on-main--2026-08-06) below, which now points at `HANDOFF.md`
> for the live count instead of embedding a number that will go stale again.

## Rules (apply to every item — same as the whole project)
- **`warp/master` is the behavioral oracle.** Port faithfully; adapt only for
  BYOP/local (no cloud) — never silently simplify away Warp behavior (AGENTS §5.10).
- **Tests-first, never defer.** Port Warp's oracle tests with each feature; a red
  test gets fixed now, never parked (AGENTS §5.6). Never weaken an assertion to go green.
- **Run all cargo through the governor:** `script/agent-cargo <agent-name> <cargo-args>`.
  It bounds how many compiles run at once and gives each agent its own target dir.
  Never invoke cargo bare while another agent is running (AGENTS §5.8).
- **English only** (code, comments, tests, docs). Exception: `app/i18n/zh-CN|ja/*.ftl`.
- **Central verification:** the owner re-runs the suite before marking done — don't
  trust an agent's self-report.
- **No CI builds as a discovery loop.** Local `cargo`/user's `script/run --release`
  is the verification; a release build happens once at the end to confirm.

---

## What's in this file

Two separate concerns, kept distinguishable:

- **[Part 1 — Warp parity restore ledger (#11)](#part-1--warp-parity-restore-ledger-11)** —
  the issue #11 keep/restore ledger plus the other outstanding parity work.
- **[Part 2 — Code-review debt](#part-2--code-review-debt)** — actionable findings from the
  code reviews and the security/performance audit, grouped by review.

Part 2 was consolidated in from a separate lowercase `todo.md` on 2026-08-06: two tracked
files differing only by case collide on case-insensitive filesystems (macOS/Windows), so
only this `TODO.md` remains. Nothing was dropped — items since verified as landed on `main`
are marked `- [x]` with the evidence inline rather than deleted.

---

# Part 1 — Warp parity restore ledger (#11)

## #11 status — CLOSED 2026-08-07 (full reconciliation)

Issue #11 was reconciled at *definition level* (symbol-by-symbol against
`origin/main`, excluding comments/binaries — a first pass with loose greps
produced false positives and was discarded) and closed. **44 of its 56 ticked
items were already implemented on `main`**; nothing was unticked (the ticks stand
as the historical record of intent, per maintainer instruction). The **10
genuinely absent items are now each tracked by a specific issue**, superseding
the old "3 decisions/holds + 7 buildable" framing:

| item | tracked by |
|---|---|
| Size-based / `warp_logging` rotation | DONE — see "Log-rotation" below |
| `SettingSurfaces` / `SettingsMode` | declined, `DECLINED.md` |
| CJK link-boundary mechanism | #223 (open) |
| `local_control` / `warpctrl` | #216 (open, comprehensive); #401 (installer sliver, open); #184/#200/#183 closed as subsets |
| Banner-immune PATH capture | #481 (closed — done) |
| TUI live background re-probe | #482 (open) |
| CDPATH-aware `cd` completion | #483 (closed — done) |
| Launch-at-login | #484 (closed — done, see "Requires macOS/Windows" below) |
| NLD heuristic feature flags | #485 (closed — done) |
| AI bundled + global skills | #487 (open — this ledger's old "AI global skills" entry below was **wrong**; see correction) |

(The 13 unchecked `[ ]` items are all keep-dropped/cloud — OTEL, VoiceInputLifecycle,
semantic-search, RunAgents orchestration, computer-use recording, cloud-mode-v2,
product-analytics telemetry, `IsCloudConversationStorageEnabled`, etc. — not work,
by decision. One open question remains outside the 10/13 split: theme syncability
*portable-path* machinery, since "syncable" there means syncable to Warp's cloud —
computes a correct answer to a question nothing currently asks unless local theme
portability is wanted.)

### Merged this session
- [x] **Pending-edit-batch conflict-discard** — CORE MERGED (targeted issue #101):
  `PendingEditBatch` 200 ms debounce + push-conflict-discard + save-flush; 3 oracle
  tests green in isolation. Deferred sub-part `BufferConflictDetected` server→client
  push (**#102**, blocked `handle_buffer_conflict_detected` + its 4th test) is now
  **DONE too** — fixed by commit `78b66b6b2` ("feat(remote_server): BufferConflictDetected
  push + git write-op RPC surface"), issue closed 2026-08-07. Assessment:
  `specs/pending-edit-batch/ASSESS.md`.

### Requires macOS / Windows — cannot be built or verified on this host
Not deferred for lack of intent: this box is Linux and these cannot be compiled or
exercised here at all. They need a macOS or Windows machine (or CI) to progress.
**Do not mark any of these done from a Linux build.**

- [ ] **WSLENV passthrough vars** *(Windows)* — **STALE: this claimed absent, but it
  is DONE.** `wsl_env_allowlist` exists at
  `app/src/terminal/local_tty/windows/environment.rs:202` (commit `17ee390a2`, PR #119,
  targeted issue #117). Compile-only port, per the commit's own note — still not
  runtime-verified on an actual WSL/Windows host, which is the real remaining item.
- [ ] **Launch-at-login** *(macOS + Windows)* — **STALE: this claimed absent, but it
  is DONE.** `app/src/login_item/` exists (`mod.rs`, `macos.rs`, `windows.rs`,
  `windows_tests.rs`; commit `17ee390a2`, PR #119, targeted issue #118). Same caveat:
  compile-only on this Linux host, not runtime-verified on macOS/Windows.
- [ ] **Edition-2024 release verification** *(macOS)* — the **code work is done and on
  `main`** (commit `48bc21cb9`, PR #53). Only a macOS release build remains unverified.
- [ ] **pwsh `-EncodedCommand` at 2 call sites** *(Windows)* — the fix is ported to
  `local_command_executor.rs:55` and `msys2_command_executor.rs:67`, matching the
  already-verified `shell.rs` site (commit `5365c62a`). Needs a Windows run to confirm.

### STALE-WRONG — corrected 2026-08-07
- [ ] **AI global skills** — **this entry previously said "WON'T DO (maintainer,
  2026-08-06)" and stated the opposite of the actual decision.** #11's 2026-08-07
  closing comment quotes the ledger's own "Maintainer BYOP decisions — 2026-08-02"
  section, settled before the WON'T-DO note was ever written: *"AI skills: build
  `bundled` + `global` (local); DROP the `remote` daemon-sync / cloud-repo arm."*
  Verified against `origin/main` today: `app/src/ai/skills/` is missing exactly
  `bundled.rs`, `bundled_tests.rs`, `global_skills.rs`, `global_skills_tests.rs`
  (plus `remote.rs`/`remote_tests.rs`, which stay dropped per the decision above).
  This is real open work, tracked at **#487** (open) — do not treat it as done or
  declined.

### Not started — true gaps
- [x] **Skill remote-path** — now **#205**. Promoted out of this ledger after finding a
  real correctness bug rather than a missing feature: `get_provider_for_path` **and**
  `get_scope_for_path` both resolve `home_skills_path` against the *client's* home, so
  a remote skill under a same-named home dir is silently misclassified as local.
  Latent only because #170 means no remote path reaches them yet — **fix with or
  before #170.** Note this ledger previously claimed `get_scope_for_path` was migrated
  by #59; it was not (still `&Path`). Related but distinct from **#487** (AI global
  skills, above): #205 is the path-*typing* half of remote skills, #487 is the
  missing-*modules* half.

### Keep-dropped (decided this session)
- [x] **history_model reconciliation** — non-cloud parts DONE (optimistic rename /
  event-sequence / child-index cleanup + `TransientError` recovery). Remaining
  `WaitForEvents`/orchestration part is **KEEP-DROPPED (maintainer 2026-08-06)**: it
  is cloud orchestration (only a Warp-server tool call triggers it; RunAgents /
  OrchestrationEventStreamer are the dropped cloud surface). The BYOP recovery
  equivalent (`recovery_pending`→`TransientError`) already covers the local case, so
  `WaitingForEvents` never firing is correct, not a bug. The constructor-arity bits
  (`start_new_conversation`/`prompt_history_candidates`) have no consumer (tie to the
  undecided NLD-flags item). Recorded on #11; tracking issue #107 closed.

### Core landed — sub-part / wiring still outstanding
- [x] **`remote_server_controller` connection-label helpers** — DONE. This entry was
  **false**: `connection_label_for_session_info` is called in production at
  `remote_server_controller.rs:290` and `:526`, not only from its own tests.
  Re-verified against `main` `8c1841a94` on 2026-08-06.
- [x] **`local_control` / `warpctrl` app-side** — **#200 is now CLOSED**, as a subset
  of **#216** (open), the comprehensive tracking issue: app-side module (23+2+2+1
  tests) + CLI-side module (19 tests) + settings group (6 tests, already landed via
  PR #472) = 53 tests. `crates/local_control` exists (14 source files);
  `app/src/local_control/` and `crates/warp_cli/src/local_control/` are still absent
  — PR #480 is open, wiring the app-side surface. The `install_warpctrl`/
  `uninstall_warpctrl` installer sliver in `app/src/workspace/cli_install.rs` is
  NOT covered by #216 and stays tracked separately at **#401** (open).
- [x] **Pinned-tabs / tab-groups remaining GUI surfaces** — **DONE, #146 closed
  2026-08-07.** Fixing commit `ababc7f07` ("feat(tabs): move-to-group submenu,
  multi-tab menu and modifier selection") ported the vertical-tabs group-header
  row, tab-group right-click menu, inline group-rename editor, and group-aware
  drag-and-drop reordering — the four items this entry previously listed as
  outstanding. Verified: `git merge-base --is-ancestor ababc7f07 origin/main`.
- [x] **repo_metadata standing-queries wiring** — **DONE, #201 closed 2026-08-07.**
  Wired by commit `0d345486f` (PR #121): `app/src/ai/skills/file_watchers/skill_watcher.rs`
  subscribes to `RepoMetadataEvent::StandingQueryResultsUpdated`, and `app/src/lib.rs`
  calls `set_project_skill_provider_paths`/`register_force_included_paths` at
  startup. (The old repro — `grep -rn standing_queries app/src` — returns zero hits
  because the driving symbols were renamed; search for the concept, not the name.)
  The **remote** half is genuinely missing, now tracked at **#296** (PR #526 open).
- [x] **Log-rotation deferred wiring** — **DONE, #202 closed 2026-08-07 — premise was
  already false when filed.** `crates/warp_logging`'s `LogConfig` already carries
  both `frontend` and `max_file_size_bytes`, and `app/src/lib.rs::init_common`
  already threads `launch_mode.log_frontend()` through — landed in the same commit
  `0d345486f` (PR #121) as the standing-queries wiring above. `max_file_size_bytes`
  staying `None` at call sites is not a fork gap either: the pin does the identical
  `..Default::default()` at every one of its own call sites.
- [x] **code_review over SSH — git write-ops** — DONE, merged 2026-08-06 (PR #125,
  issue #116). Commit / push / create-PR RPCs over SSH, plus a
  `git_operation_in_progress` guard on all three mutating handlers. Verified
  109/109 on `code_review` before merge. **Remaining sub-part:** AI commit-message
  autogen is still local-only and calls `generate_for_local_repo` with no
  `is_remote()` check — see #126.

### Done — 44 of 56 (present on `main`)
Verified by spot-check (all present): `is_jupyter_notebook_file`, `sorted_cd_directories`,
`LLMContextWindow`, `safe_browser_open_url`, `remote_matches_to_global`,
`GitBranchTrackingStatus`, `seal_with_context`, `SshRemoteServerSupport`,
`soft_wrapped_row_bounds`. The remainder are the earlier ~24 keeps (theme
syncability, relative line numbers, mermaid config, OSC-52/OSC-8, hyperlink
registry, tmux DCS, link-punct strip, CJK boundary, box-drawing, block-lifecycle,
code-symbol source, `approve` keyword, `sync::Condition`, `file_uri_drive_path`,
NLD flags, CDPATH, SettingSurfaces, browser allowlist, content-version assets,
image fallback, `TuiStack`, soft-wrap Home/End, tab-drag collapse, oversized
data-URI, editable bindings, autoupdate per-channel, `external_control_master`,
async find, banner-immune PATH capture, terminal-background reprobe, managed-secrets
BYO, `ModelEventDispatcher` SSH gate, URI deep-links) plus the PRs #58–97 items
(queued-prompts panel, remote/SSH global search, diff-state-over-SSH read path,
skill-scope `Home`, WSL program translation, Windows PATHEXT, `nld_heuristic_v2`,
mermaid fallback, focus-URL env, `standing_queries`, pinned-tabs storage).
(Sampled, not each of 44 exhaustively grepped.)

---

## Other outstanding (non-#11)
- [x] ⭐ **SMOKE TESTS** — on merged main (2026-08-05, after the 6 diff-state PRs #59-#64):
  `./script/usage-test --surface both` = **12 pass / 0 fail / 7 skip** (skips are
  needs-real-shell / needs-byop / needs-desktop — environmental), EXIT 0. Full warp lib
  sanity: `cargo test -p warp --lib` = **3910 pass / 0 fail / 33 ignored**. App boots and
  behaves with all parity + diff-state-over-SSH changes in.
- [ ] **Edition-2024 cross-platform build — macOS release verification only.** The code work
  is DONE and on `main`: the mac/wasm/windows `unsafe`-syntax fixes from branch
  `fix/edition-2024-native-targets` are merged (commit `48bc21cb9`, via PR #53) — verified
  2026-08-06 with `git merge-base --is-ancestor 48bc21cb9 main`, and the remote branch has
  been deleted. **All that remains is a local macOS `script/run --release` run**, which
  cannot be done on this Linux host — it needs a Mac (no CI-discovery builds). That run may
  still surface further latent mac-only errors.
- [ ] **#4 warp_tui suite** — **STALE, corrected 2026-08-07.** The deadlock this
  entry describes (`tui_generic_tool_call_view::…_completes_the_executor`) is FIXED
  — see Part 2 below, PR #124, commit `87d06d179` — do not re-investigate it. #4
  itself stays open, but its scope moved: CI now gates the `warp_tui` crate at all
  (it previously didn't — issue #465 covered that gap; PR #469 addressed it), and the remaining gap
  is understood as `warp_tui` trailing the pin by a generation, tracked with a full
  root-cause map at **#456**, with #384/#387/#389/#390/#392/#395 as siblings tracing
  to the same cause. Treat the old 18-failure nextest breakdown above as historical
  context for how this was first noticed, not as the current state.
- [ ] **#2 sweep** — the 2 missing GUI auto-resume oracle tests
  (`completed_user_controlled_lrc_{resumes_when_not_suppressed,skips_resume_when_suppressed}`)
  are now PORTED to `terminal/view_test.rs` (2/0; the resumes case needed a
  `GlobalResourceHandlesProvider` mock for the subagent-sidecar persist path; the fork's
  teardown method is `set_user_control_with_stop_reason`, Warp's is `set_user_control_for_teardown`).
  Broader 379-module sweep still ongoing. (Anchor Stop/auto-resume regression already code-fixed.)
- [x] **#5 deferred low-sev** — **STALE-WRONG, corrected: #5 is CLOSED (2026-08-05),
  not "all still present."** All 5 findings were dispositioned: mouse-wheel scroll
  reuse was FIXED (#78); the other 4 (multi-cursor selection span, footer statusline
  recompute, `first_rendered_line_width` paint-to-measure, `vim_visual_selection_ranges`
  duplication) were explicitly won't-fixed as either feature-gated, negligible, or
  folded into `specs/tui-render-perf/SCOPE.md`. Nothing actionable remains here.
- [x] **warp-suite i18n test-isolation** (found 2026-08-04) — the 3 deterministically-red
  tests (`drive::export::test_export_untitled_notebook`, `search::…::test_directory_search_support`,
  `workspace::…::terminal_primary_line_falls_back_to_new_session`) were the localized-`t!()`
  case: `App::test` never globally inits i18n, so the key only resolved when an earlier test
  triggered init. FIXED per-test and **LANDED ON `main` via PR #103** (commit `3150a17b9`) —
  verified 2026-08-06 with `git merge-base --is-ancestor 3150a17b9 main`; issue #98 is closed.
  All 3 green in isolation, no assertions changed. NOTE: the same class likely
  still affects #4's `slash_commands` tests; a test-binary-global i18n init would close those too.
- [x] **get_relevant_files live smoke** — now **#206**. Unit + lib green (4 tests in  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
  `get_relevant_files_tests.rs`, 4 in `get_relevant_files_runtime_tests.rs`), but never
  run against a real BYOP provider. Matters because the tool is intercepted by name and
  bypasses the protobuf executor, so no other integration coverage touches its path.
  Manual verification item — needs provider credentials.
- [x] **Vertex provider bugs** — DONE, on `main`. Empty-project silent-drop (`#99`) +
  8-field payload struct (`#100`), fixed on `fix/vertex-provider-bugs` (commit `a08b52777`)
  and **merged via PR #104** — verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`. `AgentProvider::validation_error()` and
  `ProviderEditFields` are present on `main`; issues #99/#100 are closed. Nothing to review
  or merge.

## Issue reconciliation status
- **#37** SSH ControlMaster guard — DONE and CLOSED. **Correction:** the previous
  line here said the `external_control_master` refinement was "still open" — it is
  not; it is plumbed DCS hook → session → controller and covered by tests
  (`owns_control_master_accessor_reflects_constructor`,
  `parse_dcs_ssh_with_external_control_master`). Both halves closed together.
- **#4** — still OPEN, but **PR #124 already repaired the deadlock** this entry
  describes (see Part 2 below) — do not read "#4 open" as "deadlock reproduces". The remaining
  scope is now understood as one symptom of a broader gap: `warp_tui` trails the
  pin by a generation, with #384/#387/#389/#390/#392/#395 tracing to the same root
  cause. Full map: **#456**.
- **#98/#99/#100/#101/#102** — all MERGED and CLOSED. (#102, the deferred
  `BufferConflictDetected` push, is now also done — see "Merged this session" above.)
- **#2** — tracking issue for the broader Warp-test-parity sweep; stays OPEN (this is
  now the umbrella for the much larger `SCOPE-*.md`-driven fleet effort — see
  `HANDOFF.md`, not this file, for its live numbers).
- **#5** — **CLOSED 2026-08-05, not open.** All 5 deferred low-severity findings were
  triaged: 1 fixed (mouse-wheel scroll reuse, in #78), 4 explicitly won't-fixed
  (deferred to `specs/tui-render-perf/SCOPE.md` or judged not worth doing standalone).
  The "Other outstanding" entry below previously said "5 latent items, all still
  present" — that was stale; see the correction there.
- **#11** — **CLOSED 2026-08-07.** Fully reconciled; see "#11 status" above.

### Closed 2026-08-06 late
#129 (mermaid flake) · #131 (MCP redaction gate) · #135 (PR lookup) · #137 (empty
branch dropdown over SSH) · #138 (watch filter) · #143 (Privacy page) · #145 (editor
parity) · #152 (`/usage` + `/cost`) · #156 (`PrInfo` fields) · #157 (gh-auth) ·
#185 (WSL paths) · #196 (WCAG chip labels).

### Deliberately left open — partially resolved, remainder is real
- **#126** — still OPEN, still real. BYOP commit-message gen: local path shipped
  (PR #130); #125 landed and the wiring still is not done —
  `maybe_start_commit_message_autogen` (`app/src/code_review/git_dialog/commit.rs:295`)
  calls `generate_for_local_repo` with **no `is_remote()` check**, so on an SSH repo
  it runs `git` against a path that does not exist locally and silently produces no
  draft (no toast — the empty editor is the only symptom). This is the same defect
  class as #188 (local diff-state model used on a path that may be remote); #126 is
  reportedly instance twelve of that class.
- **#136** — **DONE, closed 2026-08-07.** Fixed by PR #468, commit `5b83a8ee8`. Both
  halves verified: the local path's `local_read_files_result` now returns
  `Success { files, failed_files }` instead of discarding successful reads on any
  failure; the remote path threads `failed_files` through instead of flattening
  every failure into one hardcoded string. (The proto-field premise in the old
  entry was also wrong the other way — `AnyFilesSuccess.failed_reads` was already on
  the wire; `convert.rs` was just populating it with an empty vec.)
- **#142** — still OPEN (left to a maintainer to close/retitle), but nothing further
  to port: PR #189 and PR #227 are merged, `api_keys.rs`/`api_keys_tests.rs` carry
  the full ported/blocked/superseded/cloud breakdown, and `git grep CustomEndpoint`
  across the tree is empty. The "superseded by `AgentProviderSecrets`" decision is
  now recorded in `DECLINED.md` (PR #486) — see that file's "Divergences where the
  fork deliberately differs" section (`CustomEndpoint` row) rather than treating
  this as open parity work.
- **#146** — **DONE, closed 2026-08-07** (commit `ababc7f07`); see the remaining GUI
  surfaces item above, now also marked done.

### New issues filed 2026-08-06 late
#183, #184 (`warp_cli` gaps) — **both now closed as subsets**: #183 into #411
(`Harness::Codex` variant; the `config_name`/`from_config_name` half of #183 was
separately done, PR-added additively), #184 into #216 (see `local_control` above)
· #188 (3 more local-model-on-remote-path sites — still OPEN) ·
#191 (`.rustfmt.toml` pins edition 2018 while all 64 crates are 2024 — still OPEN) ·
#194 (BYOP token accounting was dead, which disabled auto-compaction — **now
CLOSED/fixed**) · #196 (closed).

---

## Red on main — 2026-08-06 (superseded, see correction)

`main` carried knowingly-failing tests as of 2026-08-06, per a maintainer decision to
consolidate all work onto one branch and fix afterwards. **That specific episode is
now resolved:**

- [x] **#171 — 9 ported Warp terminal tests fail** — **CLOSED; PR #224 repaired it**
  ("fix(terminal): repair the 9 ported Warp terminal regressions"), including
  both security-relevant fixes (the OSC 1337 parser panic and the unquoted
  `cat {history_file}`).
- [x] **`warpui` / `warpui_core` suite** — no longer a distinct red item. PR #181,
  which introduced it, was itself a test-porting pass that found and ported real
  gaps (word/punctuation selection expansion, hyperlink click-handling, one
  macOS-only font-identity file); nothing in the current comprehensive test count
  (see below) attributes failures to `warpui`/`warpui_core`.
- [x] **Baseline established** — but do not trust a number written here; it goes
  stale within a day at the current merge rate (194 commits landed between
  `HANDOFF.md`'s last snapshot and this reconciliation). **`HANDOFF.md` is the live
  source for `main`'s current red-test count** — as of its last rewrite: 4946 tests
  batch-run, 4935 passed, 11 failed, all attributed (5 deliberate — PR #259 pinning
  real `history_model` rewind/fork divergences #251/#253 rather than shipping
  unverified fixes; 6 from two PRs' uncompiled hunks in `vim_handler_tests.rs` and
  `app/src/terminal/input.rs`, not deliberate, need fixing). Re-read `HANDOFF.md`
  rather than updating a count here.

**Method note (still valid):** always state which commit a test count belongs to —
a branch number was once mistaken for `main`'s and made a clean proto re-pin look
like a 22-test loss.

---

# Part 2 — Code-review debt

Actionable items from the code reviews run on 2026-07-26 (and later). Grouped by review.
Each item notes `file:line`, the problem, and the suggested fix.

Consolidated here from the former lowercase `todo.md` on 2026-08-06. Every item was carried
over. Items re-verified against `main` during the consolidation and found already landed were
flipped to `- [x]` with the evidence inline — none were deleted. Note that several `file:line`
references below predate later refactors (e.g. `app/src/settings_view/about_page.rs` is now the
`about_page/` module); the original paths are kept as written so the findings stay traceable.

## warp_tui test suite health (found 2026-07-29, commits `5b2d600f`/`eaabdc36`)

Discovered while verifying the #328 fix + TUI allow/reject keybindings.
Confirmed via `git stash` that both issues below reproduce identically on
clean HEAD — pre-existing, unrelated to either of those changes. Not fixed
here to keep those changes scoped; `cargo build`/`cargo check` (the actual
release gates) are unaffected either way.

- [x] **`cargo test -p warp_tui --lib` deadlocks partway through a full serial run**
  — RESOLVED on `main` (PR #124, commit `87d06d179`, "fix(warp_tui): stop cargo test
  --lib deadlock"; verified 2026-08-06 with
  `git merge-base --is-ancestor 87d06d179 main`), which reworked
  `tui_generic_tool_call_view_tests.rs` and `test_fixtures.rs`.
  Hung at `tui_generic_tool_call_view::tests::accepting_new_conversation_suggestion_completes_the_executor`
  — 39 threads, all blocked in `futex_do_wait`, zero CPU progress for 20+
  minutes. Reproduced twice. Scoping the test filter away from this module
  avoided it (used for verification of the allow/reject work), so the full
  crate suite may simply have never been run to completion before.
  **NOTE:** only the *deadlock* is closed out here. The remaining `warp_tui`
  suite work — the 18 `nextest` failures — is tracked in Part 1 under
  **`#4 warp_tui suite`**, whose text predates PR #124.

- [x] **3 tests in `terminal_session_view_tests.rs` fail even run alone/serially**
  — RESOLVED on `main` (PR #73, commit `b7c6012ce`, "test(tui): drive footer/zero-state
  trio into AI input mode"; verified 2026-08-06 with
  `git merge-base --is-ancestor b7c6012ce main`), which made the implicit setup explicit
  per-test exactly as the fix below proposed. All three tests still exist in
  `crates/warp_tui/src/terminal_session_view_tests.rs`.
  `agent_hint_tracks_transcript_emptiness_without_input_invalidation`,
  `footer_conversations_callout_no_longer_renders`,
  `footer_model_label_is_a_bounded_click_target` — all failed with a
  default/empty-looking footer ("shell mode", "No custom provider
  configured" not found) even filtered down to just this one test module
  with `--test-threads=1`, meaning they depended on setup that normally
  happened as a side effect of some other test module running first in the
  full suite — not truly hermetic.
  **Fix (applied):** find whatever global/singleton setup they implicitly depend on
  and make it explicit per-test (matching the pattern already used to fix
  `warp`'s own historical test-isolation issues — see settings.toml
  hermetic-path fix, ssh-onekey singleton, etc.).

---

## Follow-up code-review fixes (2026-07-29, commit `fddc193a`)

Dev machine is Linux; nothing below has been run against a real Windows
`pwsh.exe`. Verified only via `cargo check`/`cargo test` (static + unit-level).

- [ ] **NEEDS WINDOWS VERIFICATION: pwsh `-EncodedCommand` at 2 more call sites**
  — `app/src/terminal/model/session/command_executor/local_command_executor.rs:55`,
  `app/src/terminal/model/session/command_executor/msys2_command_executor.rs:67`
  Ported the same fix as the interactive-session-launch site (`shell.rs`,
  commit `5365c62a`) to `LocalCommandExecutor`'s generator/login-shell command
  path and `MSYS2CommandExecutor`'s Windows-native-shell path, both of which
  built `pwsh ... -c <command>` as a plain string — open to the same PS 7.6
  `-Command` quoting-parser crash on any command containing a `"`. Shared the
  encode logic into `util::encode_pwsh_command`.
  Regression tests (`encode_pwsh_command_round_trips_without_trailing_nul` in
  `util/mod.rs`, plus the existing `shell_tests.rs` one) only check the
  base64/UTF-16LE encoding itself round-trips correctly — they don't spawn a
  real `pwsh.exe` and confirm it accepts the argv or that a generator command
  containing a quote actually executes.
  **To verify:** on a Windows box with PowerShell 7.6, run a generator/BYOP
  local command whose text contains a `"` (e.g. a quoted path) through both
  executors and confirm it executes instead of erroring; also sanity-check a
  plain no-quotes command still works end-to-end (stdout/exit code correct).

---

## Security / performance audit — non-Warp code (2026-07-26)

Parallel audit (6 agents) of the fork's own code (fork-specific additions + newer work;
boundary = Warp merge-base `c325d146`). Upstream Warp treated as trusted/out of
scope. Ranked most-actionable first. No CRITICAL/HIGH security issues; **crash
sweep found zero reachable panics** (BYOP/AI stack is well-hardened). Duplicates
across agents have been merged.

> **Scope note (2026-08-06):** Track 3 (merge `fad390189`, on `main`) deleted
> `app/src/ssh_manager/`, `crates/warp_ssh_manager`, `crates/zap_sync` and
> `crates/zap_sftp` outright. Every finding below that names one of those paths is
> therefore moot as live code — the fixes are recorded for history, and the one
> remaining follow-up they carried (decoupling the cloud-sync DEK from the PAT) has
> no code left to apply to.

### Security

- [x] **[MED] SSH-sync payload integrity → RCE-on-connect** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  Now seals the entire `SshSyncData` in a single AES-GCM envelope (`seal_payload`
  / `unseal_payload`, v2 format), so every field — `host`, `key_path`,
  `startup_command`, `notes`, node structure — is covered by the GCM auth tag.
  Tampered payloads fail authentication and are rejected; legacy v1
  (unauthenticated) payloads are refused with a "re-upload to upgrade" message.
  Tests: `seal_roundtrip_*`, `tampered_sealed_payload_is_rejected`,
  `legacy_unauthenticated_payload_is_rejected`.
  *(original location note: sync_provider.rs:174,332)*
  On download, only the encrypted secret fields were authenticated; `host`,
  `key_path`, and `startup_command` came from the gist JSON integrity-unprotected,
  and `startup_command` was written verbatim to the PTY on connect. A tampered gist
  (writable with a `gist`-scoped or leaked token that can't read the encrypted
  blob) → command execution on connect, or connection/key redirect.
  **Fix:** authenticate the whole payload (HMAC/sign all fields, or wrap the entire
  JSON in the AES-GCM envelope), and confirm changes pulled from sync on apply.

- [x] **[MED] SSH destination argument injection (leading-dash host → local RCE)** — FIXED
  — `crates/warp_ssh_manager/src/ssh_command.rs` *(crate since removed by Track 3)*
  Added a `--` option terminator before the destination in all three argv paths
  (`build_ssh_args`, `test_key_auth`, `build_password_auth_cmd_args`) via a shared
  `push_destination` helper, so a `-o…` host/username can't be parsed as an ssh
  option. Regression tests: `build_ssh_args_guards_leading_dash_host`,
  `password_auth_args_guard_leading_dash_host`.
  *(original location note: ssh_command.rs:50-55)*
  (also `test_key_auth:118`, password path `:307-309`, PTY `build_ssh_command_line:59-65`)
  The `host` / `user@host` target was appended as the final `ssh` argv with no `--`
  separator. A host beginning with `-` (e.g. `-oProxyCommand=touch /tmp/pwned`)
  is parsed as an option → local command execution before any connection.
  `shell_escape` does NOT neutralize a leading-dash flag. Self-inflicted today
  (own config), but reachable if a host was ever imported/synced from `~/.ssh/config`
  or a shared profile.
  **Fix:** insert a literal `--` before the target in all four paths; reject
  host/username values starting with `-`.

- [x] **[MED] Cloud-sync key is unsalted, token-coupled, not a real KDF** — PARTIALLY FIXED,
  remainder now MOOT (crate removed by Track 3)
  — `crates/zap_sync/src/crypto.rs` *(crate since removed by Track 3)*
  Replaced `SHA256(SHA256(token))` with **Argon2id** over a random 16-byte
  per-message salt (embedded in the blob as `salt || nonce || ciphertext`). This
  closed the "not a real KDF / unsalted / brute-forceable low-entropy token"
  weakness; API unchanged so all callers were untouched. It remained **token-derived**
  (not decoupled from gist access) — full decoupling would have needed an independent
  user passphrase (larger UX change), left as a follow-up. That follow-up is now moot:
  there is no `zap_sync` crate on `main`.
  The AES-256-GCM key was `SHA256(SHA256(PAT))` — derived from the same GitHub/Gitee
  token that also fetched the ciphertext gist, with no salt/work factor/domain
  separation. Token compromise yielded both ciphertext and key; low-entropy
  (self-hosted/Gitee/custom) tokens became brute-forceable against the public gist.
  **Fix:** derive the DEK from an independent user passphrase (or a random per-user
  key kept only in the OS keychain, never uploaded) via Argon2id + stored random
  salt. **Availability footgun:** rotating the PAT silently made all synced data
  undecryptable — document it.

- [x] **[LOW] `http://` provider base_url sends the API key as cleartext Bearer**
  — RESOLVED on `main` (PR #114, commit `74e365635`; verified 2026-08-06 with
  `git merge-base --is-ancestor 74e365635 main`)
  — `app/src/ai/agent_providers/openai_compatible.rs:61` (and `chat_stream.rs`
  `normalize_endpoint_url:3344`)
  `http://` was permitted and `Authorization: Bearer <key>` was attached anyway.
  Intended for local Ollama, but a plaintext/MITM'd provider leaked the key.
  **Fix (applied):** `is_loopback_host` / the cleartext-risk check in
  `app/src/ai/agent_providers/mod.rs` now gate the bearer to `https://` or a
  loopback `http://` endpoint; `chat_stream.rs` strips the key and warns otherwise.
  The gate matches on the literal host (`localhost`, `127.0.0.0/8`, `::1`) rather
  than on DNS resolution, since a name that merely resolves to loopback today can
  be repointed tomorrow.

- [x] **[LOW] Unbounded response/stream reads (DoS)** — ACCEPTED RISK (stock upstream, unfixed upstream; documented in SECURITY.md)
  — `lib/rust-genai/src/webc/web_client.rs:113,128`, `models_dev.rs:254`
  (`res.text()`/`bytes()` with no cap) and `web_stream.rs:~168` (SSE
  `partial_message` grows unbounded if the delimiter never arrives).
  A malicious/compromised provider endpoint can OOM the client. (gzip is off, so
  not a decompression bomb — just raw size.)
  **Fix:** size-limited streamed reads; cap the SSE buffer and error past a limit.

- [x] **[LOW] SSH sync uploads structural fields to the gist in plaintext** — RESOLVED (by the v2 seal)
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  Mooted by the payload-integrity fix: the whole `SshSyncData` (host, username,
  port, startup_command, notes, key_path, node tree) went inside the v2 AES-GCM
  seal, so nothing structural was on the wire in plaintext anymore.

- [x] **[LOW] Bearer token forwarded to `raw_url` taken from response JSON** — FIXED
  — `crates/zap_sync/src/gist_client.rs` *(crate since removed by Track 3)*
  The truncated-gist path only attached the `Authorization` header when
  `raw_url_is_trusted(platform, raw_url)` — HTTPS + a per-platform content-host
  allowlist (`gist.githubusercontent.com` etc. for GitHub, `*.gitee.com` for
  Gitee). A tampered `raw_url` was fetched without credentials, so the token
  couldn't be exfiltrated. Tests: `raw_url_trusted_*`, `raw_url_rejected_*`.

- [x] **[LOW] Decrypted secrets held in non-zeroized `String`** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  `PendingSecret.value` became `Zeroizing<String>` and both per-field decrypts were
  wrapped in `Zeroizing::new(...)`, so decrypted passwords/passphrases were zeroed
  on drop after being written to the keychain — consistent with
  `WrittenSecret.prior_value`.

- [x] **[LOW] SSRF IPv4-compatible IPv6 gap** — FIXED (to_ipv4 covers ::a.b.c.d); WASM DNS-filter gap noted (cloud target only)
  — `app/src/ai/agent_providers/tools/web_runtime.rs:110-155`
  `is_blocked_ip` handled `::ffff:a.b.c.d` but not the deprecated `::a.b.c.d`
  form; `SsrfSafeResolver` is `cfg(not(wasm32))` so the WASM build only checks IP
  literals. Marginal on desktop; noted for completeness.
  **Fix:** also reject embedded-IPv4 IPv6 / `v6.to_ipv4()`; document the WASM gap.

- [x] **[LOW] Defense-in-depth: unvalidated inputs to sensitive sinks**
  — RESOLVED on `main`, all three sub-parts:
  - `vertex_auth.rs:89` — gcloud `--impersonate-service-account` SA email was only
    checked non-empty (argv-safe, no injection, but wanted an email format check).
    **Done:** `is_plausible_service_account_email` now gates the flag (PR #114,
    commit `74e365635`), with unit tests.
  - `app/src/ssh_manager/su_password_injector.rs` + `secret_injector.rs:107` — raw
    secret + `\n` written to PTY, so an embedded newline injected trailing bytes as
    commands. **Moot:** the whole `app/src/ssh_manager/` directory was deleted by
    Track 3 (merge `fad390189`, on `main`); there are no injectors left in the tree.
  - prompt custom-file loader `prompt_renderer.rs:278` — blocked `..`/absolute but
    followed symlinks out of the dir. **Done:** `canonicalize` + `starts_with`
    containment on both loader paths (PR #114), with a regression test.

### Performance (new TUI rendering — all HIGH, same trigger: per-streamed-chunk / per-frame)

- [x] **[HIGH] `sync_code_block_views` reclones every code block each streamed chunk** — FIXED
  — `crates/warp_tui/src/agent_block.rs`
  The reconciler now compares the borrowed `&str` against the retained view's
  content (`TuiCodeBlockView::matches`) and only clones new/changed sections (in
  practice just the streaming block). `sync()` already no-ops on an equal payload,
  so this elides only redundant allocation. Verified: builds; code_block (8) +
  agent_block (51) tests pass.

- [x] **[HIGH] `sync_action_views` re-clones actions each chunk** — FIXED (matches-skip for shell+plan; plan re-resolves presentation to catch model state). Commit e77659f7
  — `crates/warp_tui/src/agent_block.rs:498-541`
  Same trigger; cloned every plan/shell/generic action every chunk.
  **Analysis:** *Shell* is safe to skip-when-unchanged — `update_action` is a pure
  function of `(action, output_streaming)` and shell action payloads are small
  (just the command string; live output is reactive from `terminal_model`), so the
  payoff is small. *Plan* (`CreateDocuments`/`EditDocuments`, the larger payloads)
  is NOT safe to skip: `sync_action` → `sync_documents` re-resolves per-document
  state from `action_model`, which changes independently of the action. A correct
  plan fix needs to fold that model-derived state into the change key.
  **Recommend:** do plan properly with a running-TUI check + a streaming snapshot
  test; shell-only is low value.

- [x] **[HIGH] Full-document rebuild on every layout pass, not viewport-gated** — now **#203**. — NEEDS REFACTOR (deferred)  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
  — `crates/warp_tui/src/editor_element.rs:351-401` (`build`) +
  `crates/editor/src/render/model/char_cell_display.rs:257-334` (`display_rows`)
  `layout()` unconditionally rebuilds: `text.chars().collect()` + a full-buffer
  `display_lattice` walk even when `with_viewport_rows` is set; any animated
  element (shimmer, ~10 Hz) re-layouts the whole retained tree.
  **Analysis:** `build()` can't be memoized wholesale — it has essential
  per-layout side effects (`try_layout_pending_edits`, scroll clamp/follow_cursor,
  `set_terminal_width`); skipping it breaks editing/scroll. The real fix is to
  separate the pure projection from the side effects and/or make `display_lattice`
  viewport-windowed in shared `crates/editor` code — an intricate change that
  **must** be verified in a running TUI. Deferred to a focused, harness-backed
  session rather than shipped blind.

### INFO / noted (not action items)

- Linux `secure_storage` fallback uses a hardcoded embedded key
  (`secure_storage/linux.rs:95-113`) → fallback files are effectively plaintext.
  This is **upstream Warp** code, but the fork now routes far more sensitive
  secrets through it (BYOP API keys, proxy password) on
  headless-Linux/WSL/no-Secret-Service boxes, amplifying blast radius. Escalate
  upstream or override in the fork. (The cloud-sync PAT and SSH passwords that
  originally widened this blast radius are gone with Track 3.)
- genai logs full response bodies at `tracing::trace` (no secrets/`Authorization`).
- LLM file tools (`tools/files.rs`, `edit.rs`) add no extra sandboxing beyond
  upstream's executor + block-UI approval.
- **Crash sweep: 0 findings.** BYOP/AI stack uses checked slicing, `saturating_sub`,
  `.get()`, `from_utf8_lossy`, `to_ascii_lowercase`, division-by-zero guards
  throughout; one `crates/editor` diff is itself a panic fix.

---

## About page + Phosphor theme (commits `41a77348`, `472a339b`)

- [x] **Search terms advertise now-hidden autoupdate controls** — FIXED (trimmed)
  — `app/src/settings_view/about_page.rs:138` (now `about_page/mod.rs:152`)
  `search_terms` still listed "automatic updates auto update check for updates
  new version", but `SHOW_AUTOUPDATE_UI = false` hides those controls. Settings
  search for "automatic updates" led to the About page with no such control.
  **Fix:** trim the autoupdate vocabulary from `search_terms` while the UI is
  hidden.

- [x] **JPEG logo: opaque background + baked-in text, illegible at ~100px** — FIXED, **#204**.
  — `app/src/settings_view/about_page.rs:187` (now `about_page/mod.rs:167`)
  The 1024×1024 badge was downscaled to ~100px (its "PHOSPHOR TERMLNK / CRT
  TERMINAL" lettering became noise), and being an opaque JPEG it rendered as a
  dark box on a light-themed About page.
  **Fix:** point the About page at the existing vector Phosphor mark
  (`app/channels/oss/icon/AppIcon.icon/Assets/logo.svg`, already the source of
  truth for the app icon) copied to `bundled/svg/phosphor-logo.svg` — no baked
  text, alpha background, crisp at any size. The old
  `bundled/jpg/phosphor-logo.jpeg` (unused elsewhere) was removed; the
  README/marketing badge at repo-root `assets/phosphor-logo.jpeg` is untouched.

- [x] **Autoupdate observer now gated** — FIXED (subscribe only when SHOW_AUTOUPDATE_UI)
  — `app/src/settings_view/about_page.rs:61` (now `about_page/mod.rs:72`)
  `new()` still subscribed to `AutoupdateState` (`ctx.observe(... ctx.notify())`)
  and all autoupdate `handle_action` arms remained. While disabled, any autoupdate
  stage change re-rendered the About page for no visible effect; the controller
  half was left half-wired.
  **Fix:** gate the subscription alongside the render (ideally derive the flag
  from real release-channel availability).

- [x] **~200 lines reachable only via the const-false branch** — RESOLVED via the
  "extract so it's clearly parked" option
  — `app/src/settings_view/about_page.rs:303`
  `render_update_status` + `UpdateAction` + `format_bytes` +
  `format_download_progress` were only reachable through
  `SHOW_AUTOUPDATE_UI` (compile-time `false`). Deliberate/reversible, but the
  dead branch would bit-rot and was untested while disabled.
  **Fix (applied):** they now live in their own module,
  `app/src/settings_view/about_page/autoupdate_ui.rs`, whose header documents it as
  "**Parked, not wired up.**" and states that re-enabling means flipping
  `SHOW_AUTOUPDATE_UI` back to `true`; `autoupdate_ui_tests.rs` covers `format_bytes`
  and `format_download_progress` so the parked code no longer rots untested.

- [x] **Amber theme duplicated in Rust const + yaml, hand-synced** — RESOLVED via the
  "add a test asserting the two stay in sync" option
  — `themes/phosphor_amber.yaml:24`
  Phosphor Amber is defined twice — the bundled Rust `AnsiColors` const (the
  actual default) and this copy-in yaml — with no shared source. The change that
  raised this had to edit identical blue/cyan values in both, and nothing prevented
  future drift.
  **Fix (applied):** `app/src/themes/default_themes_tests.rs` now has
  `phosphor_amber_yaml_matches_builtin_theme` (and a green counterpart) asserting the
  YAML round-trips to exactly the built-in `WarpTheme`, with a failure message telling
  you to re-sync. The two are still hand-synced duplicates by design — the test is the
  guard rail.

---

## Vertex AI provider (merge `fae32e14`)

- [x] **Empty project builds a malformed URL + silent picker drop** — RESOLVED on `main`
  (issue `#99`, commit `a08b52777`, PR #104; verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`)
  — `app/src/settings/ai.rs:924`
  There was no save-time validation, so a Vertex provider could be saved with an empty
  project. `build_byop_llm_infos` (`mod.rs:83`) then silently skipped it (models
  never appeared, no feedback), and `vertex_endpoint_url("", "global")` yielded
  `.../projects//locations/global/` if any path resolved it.
  **Fix (applied):** `AgentProvider::validation_error()` rejects a Vertex provider with
  an empty `vertex_project` at save time and `save_agent_provider_edits` surfaces it as
  an error toast.

- [x] **Vertex location not case-normalized** — FIXED (vertex_endpoint_url lowercases location)
  — `app/src/settings/ai.rs:927`
  The `location == "global"` check was case-sensitive and the raw location was
  interpolated into the hostname, so "Global" → `Global-aiplatform...` and
  "US-EAST5" → `US-EAST5-aiplatform...` — both invalid hosts.
  **Fix:** `location.to_ascii_lowercase()` before the global check and host
  interpolation.

- [x] **Cold-start token mint has no in-flight coalescing** — FIXED (MINT_LOCK single-flight)
  — `app/src/ai/agent_providers/vertex_auth.rs:47`
  On a cold cache, concurrent first requests (main stream + title gen +
  active-AI) each missed and spawned their own `gcloud auth print-access-token`
  subprocess.
  **Fix:** single-flight the mint per credential (per-credential async lock or
  in-flight map) so only one `gcloud` runs.

- [x] **8-field positional provider-edit payload duplicated ~4×** — RESOLVED on `main`
  (issue `#100`, commit `a08b52777`, PR #104; verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`)
  — `app/src/settings_view/ai_page.rs:2425`
  `SaveAgentProviderEdits` / `SaveAgentProviderEditsThen` / the
  `to_save_action_with` closure type / `save_agent_provider_edits` all carried the
  same 8 positional fields, kept in lockstep by hand (needing
  `#[allow(clippy::too_many_arguments)]`). A mismatched order silently swapped
  values.
  **Fix (applied):** collapsed into a single `ProviderEditFields` struct passed by
  value (now in `app/src/settings_view/ai_page.rs` +
  `app/src/settings_view/agent_providers_widget.rs`).

- [x] **Vertex family routing duplicated** — FIXED (shared vertex_model_family())
  — `app/src/ai/agent_providers/reasoning.rs:100`
  (and `app/src/ai/agent_providers/attachment_caps.rs:225`)
  The `contains("claude") ? Anthropic : Gemini` dispatch was implemented verbatim
  in both; a change to the heuristic had to touch both or the surfaces disagreed.
  **Fix:** extract `fn vertex_model_family(model_id: &str) -> AgentProviderApiType`
  and call it from both.

---

## warp-oss-sync / TUI port (range `ab207e20..7accb626`)

Scale: ~150 commits, 20k+ lines across 207 shared files (plus the isolated
`warp_tui` crate + test churn). Too large for a faithful inline line-by-line
pass — run `/code-review ultra josh/warp-oss-sync` for full coverage.

A **focused GUI-regression review of the two biggest GUI-facing keystones** was
done inline and both came back **clean**:

- [x] **View→Entity relaxation + `tui_views` routing** (`core/view/context.rs`,
  `core/view/handle.rs`) — GUI-safe. All `T: View` → `T: Entity` changes are
  widenings (`View: Entity`), method bodies unchanged; the `tui_views` fallback
  in `WeakViewHandle::upgrade` (and view/try_view/update_view) is
  `#[cfg(feature = "tui")]`-gated, so GUI builds behave identically. The change
  also fixes a latent bug where weak handles to TUI views failed to upgrade.
- [x] **`TerminalManager<S>` genericization** (`terminal/local_tty/terminal_manager.rs`)
  — structurally sound. GUI wiring stays in a concrete `impl
  TerminalManager<TerminalView>`; the generic `impl<S>` path is additive; GUI
  downcast site (`pane_group/mod.rs:2314`) is consistent. Full line-by-line of
  the 1079-line body extraction was not done (defer to ultra); the green test
  suite covers terminal behavior.

Reviewed 2026-07-26 (the three previously-unreviewed files) — all CLEAN:
- [x] `crates/warpui_core/src/core/app.rs` — GUI-safe. Same shape as the cleared
  keystones: View→Entity widenings, `tui_views` fallbacks all `#[cfg(feature =
  "tui")]`-gated (compiled out of GUI builds; the GUI `views` map is always
  checked first with unchanged behavior), and the `&mut dyn Any` downcast
  refactor is consistent throughout. No regression.
- [x] `crates/editor/src/render/model/mod.rs` — char-cell render model, no
  reachable panic: `opportunities` is sized `count+1` (never empty), row/char
  indexing relies on sentinel invariants that are `debug_assert`-checked and
  maintained by `rebuild`, byte-offset math is `.min()`/`.get()`/`saturating_sub`
  clamped. Internal-invariant-guarded, not untrusted-triggerable.
- [x] `app/src/ai/agent_providers/prompt_renderer.rs` — no SSTI: templates are
  pre-registered by name; LLM/user values flow in only as context DATA
  (`Value::from_serialize`), never compiled as templates. minijinja is sandboxed
  (no eval/shell/fs from templates), no `render_str`, no command exec.
  `custom_prompt_raw` blocks absolute/`..` paths (input is user config, not LLM).
  Only residual: symlink-follow — since fixed, see the defense-in-depth item above.
