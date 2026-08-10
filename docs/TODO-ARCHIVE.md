# TODO archive — Phosphor

Completed, superseded and historical sections moved out of `TODO.md` on
2026-08-10. **Nothing here has an open checkbox** — that was the selection rule,
asserted mechanically at the time of the move, not judged by eye.

Kept rather than deleted because a large share of it is the record of *how a
wrong answer was corrected*, which is the most reusable thing in the file: six
separate entries here once stated the opposite of the code. `TODO.md` now holds
only sections with live work; look here for why something was decided, and there
for what is left to do.

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
- [x] **Claude harness cannot receive MCP servers.** **[DONE 28d21e520 — ported
      the pin's approach unchanged. `serialize_claude_mcp_config` emits Claude's
      `{"mcpServers": {name: entry}}` shape (types byte-identical to the pin);
      `--mcp-config` added to `claude_command`; `resolved_mcp_servers` threaded
      through `ThirdPartyHarness::build_runner` and `ClaudeHarnessRunner::new`;
      `write_temp_file_with_suffix` is the new suffix-taking form, with the old
      `.txt` signature kept as a wrapper. Codex/Gemini take the parameter and
      ignore it, as at the pin. The pin's 4 serialization tests are ported plus
      2 for the flag itself, which closes 4 of the pin-test gaps that
      `claude_code_tests.rs` enumerated. 5421/5421 green.]** The pin stages them as a
      temp JSON passed with `--mcp-config` from `build_runner`. That flag,
      `serialize_claude_mcp_config`, and a suffix parameter on `write_temp_file`
      are all absent here. A capability port, not trait plumbing — deliberately
      not invented during the trait work. When it lands, `build_runner` will also
      need `resolved_mcp_servers`; the doc comment on `claude_code.rs` records
      this. Gemini needs nothing — it ignores both at the pin too.
- [x] **Guard the shell-to-Rust name agreement.** **[DONE 8eca0b2eb — `script/check_channel_command_names` compares the bundle scripts' channel maps against `crates/warp_core/src/channel/mod.rs`. Wired into both `script/precheck` and `.github/workflows/pr-check.yml`, so it fails locally before CI.]** The warpctrl defect was a
      silent mismatch between a bundle script's channel map and
      `crates/warp_core/src/channel/mod.rs:50`, caught only because the install
      button failed at runtime. There is now a **second** pair of the same shape
      (`oz`/`zap-oss`). A grep-based CI gate comparing the bundle scripts' maps
      against `channel/mod.rs` would prevent recurrence. Deliberately not added
      during the fix: gate wiring overlaps the `ci/clearer-test-gate` work.
- [x] **`script/test_warpctrl_early_dispatch` not ported.** **[DONE 8eca0b2eb — ported, plus a one-character fix to `windows-installer.iss:243`. Wired into precheck and pr-check.]** The pin has it; it
      needs a built binary. This is the missing half of the coverage — the new
      bash test proves the wrapper forwards `--warpctrl`, but nothing proves the
      binary still honours it.
- [x] **`tui-migrate-setup` skill — NEEDS MAINTAINER SIGN-OFF (AGENTS §5.10).** **[RESOLVED 4b4e87a38 — maintainer chose option 2 (2026-08-10): rather than port a skill whose premise two DECLINED decisions contradict, ship a `tui-settings` orientation skill that explains the shared-config reality instead of pretending GUI and TUI have separate files. `resources/bundled/skills/tui-settings/SKILL.md`.]**
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
- ~~**Reranking is worse.**~~ **FIXED** — see the D2d row below.
- ~~Search is **exact rather than approximate**, so latency grows linearly with
  repo size.~~ **FIXED** — see the D2d row below. Still exact; no longer linear.
- ~~`populate_merkle_tree_cache` is the one genuine no-op.~~ It now builds the
  local search index, which is the local equivalent of what it warmed remotely.
  Its caller still treats the result as advisory, which stays correct: without an
  index a search is slow, never wrong.
- **Nothing degrades to a silent empty answer** — a missing provider raises
  `Error::NoEmbeddingProvider` naming the model, because an empty result set is
  indistinguishable from an empty store and would re-embed the repo every sync.

### D2d — the two D2 regressions, closed

- [x] **Latency.** The merkle tree now doubles as a ball tree in cosine space:
      each node carries a `NodeSummary` (centroid, leaf count, angular radius) in
      a new `codebase_index_node_summaries` table, and retrieval descends
      best-first, skipping any subtree whose *best possible* score cannot reach
      the current k-th best. **Still exact** — same fragments, same order as
      scoring every leaf, asserted directly against an independent exhaustive
      implementation. Measured on synthetic corpora with directory locality: a
      query reads ~88 leaf vectors at 512 fragments and ~72 at 8,192, i.e. flat
      where it used to be all of them. With *no* locality it degrades toward the
      full scan (416 of 512) and stays correct; that case is tested too.
      Summaries are keyed by node hash, so they can be absent but never stale.
- [x] **Reranking.** `RerankProvider` calls the user's own `/rerank` model where
      their provider has one (`SUPPORTED_RERANK_MODELS`, Voyage and Cohere
      shapes) — a real cross-encoder, the pin's design bought rather than
      approximated. Where they have none, reranking fuses the bi-encoder with
      BM25 over the same fragments (code-aware tokenizer: identifiers emitted
      whole *and* split on case/underscore), combined by reciprocal rank fusion.
      Measured on a fixture with known answers: MRR 0.625 bi-encoder only,
      0.8125 lexical only, **1.0 fused**.
- Answers the old "open question for the maintainer" below: both options it
  listed were taken, in the order it suggested — provider-side where available,
  no local model dependency added.

- [x] **DONE 2026-08-10** (`623937230`) — remote-daemon indexing, settings UI and all 39 daemon tests landed; `LaunchMode::supports_indexing()` ported properly and `daemon_codebase_index_data_dir` lives next to the daemon (this fork's `RemoteServerDaemon` never reaches `initialize_app`). Originally: remote-daemon indexing (the pin's
      `LaunchMode::supports_indexing()` gate has no fork equivalent;
      `FeatureFlag::RemoteCodebaseIndexing` stands in), settings-page UI for the
      two new `CodeSettings` toggles and for picking an embedding provider (both
      reachable via `settings.toml` today), and
      `app/src/remote_server/codebase_index_model_tests.rs` (39 tests).
- [x] **Answered (D2d above):** reranking quality. Provider-side rerank endpoint
      where the user has one, hybrid vector+BM25 where they do not. A local
      cross-encoder was considered and declined: `crates/input_classifier` proves
      the fork *can* run local inference (candle/ort behind `onnx_*` features),
      but it would add a model download and inference cost to a path that a
      provider already serves for a fraction of a cent.

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

- [x] **Should the GUI's Settings key editors also notify the TUI? — TAKEN
      2026-08-10.** Answer: yes, and the reverse direction needed the same fix.
      The stamp moved out of the single `zap-tui` CLI call site into the two
      secret stores' write choke points (`ApiKeyManager::write_keys_to_secure_storage`
      and `AgentProviderSecrets::persist`), so every GUI *and* TUI mutation
      notifies. Two corrections to the original entry: the *subscriber* was
      already ungated on `LaunchMode::Tui` in this fork, so the GUI already
      reloaded on a TUI-side write — only the writer side was missing; and the
      store this fork's GUI key editor actually writes is `AgentProviderSecrets`,
      not `ApiKeyManager`, which had no cross-process reload at all in either
      direction. See `crates/ai/src/secret_revision.rs` for the mechanism.
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
- [x] **VERIFIED FALSE POSITIVE 2026-08-10** — the fork's `completions_menu.rs` IS the pin's `completion_menu.rs`, renamed and extended (same list state, gating, inline-menu wiring, completer engine). Annotated in `SCOPE-TERMINAL.md`; -2 from the debt count. One genuinely untested guard got a test. Originally: TUI completion menu (fork's
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
