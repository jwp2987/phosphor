# Warp OSS sync scope

Scoping doc for pulling value from current Warp OSS (`warpdotdev/warp` `master`)
into the Zap BYOP fork **without** re-adopting the cloud stack.

- Fork point (merge-base): `c325d146` (2026-04-28)
- Warp OSS reference at time of scoping: `warp/master` `af891a4e` (2026-07-23)
- Divergence: Warp is **1683 commits** ahead of the fork point; Zap is 800 ahead.
- Remote already wired: `warp` -> `https://github.com/warpdotdev/warp.git`.

Two independent workstreams:

- **A. Easy cherry-pick fixes** — self-contained bug/perf fixes in shared crates.
- **B. TUI port** — a scoped port of `crates/warp_tui`, dropping cloud orchestration.

---

## Workstream A — easy cherry-pick fixes

Target crates were chosen because Warp changed them meaningfully since the fork
while Zap barely touched them (low collision) and they still exist in Zap:

| Zone | Warp churn | Zap churn | Clean commits |
|---|---|---|---|
| `crates/repo_metadata` | 11,233 | 236 | 8 |
| `app/src/context_chips` | 4,232 | 158 | 11 |
| `crates/editor` | 6,719 | 213 | 4 |

"Clean" = the commit's entire diff is confined to the one zone. That makes it a
candidate; it is **not** a guarantee it applies without conflict (an intervening
non-clean commit may have changed the crate's base). Apply in the listed
(chronological) order and resolve per commit.

Workflow per commit:
`git cherry-pick -x <sha>` -> resolve -> `cargo build -p <crate> --features gui`
-> `cargo test -p <crate>` where tests exist -> next.

### repo_metadata (apply in this order) — highest value

Real file-tree correctness/perf fixes; no cloud coupling.

- `424e3335` Fall back to lazy index when repo exceeds max file limit (#10234)
- `9f459842` Fix symlinked gitignored paths in code review (#11856)
- `a856c95d` remove eager descendent logic (#12207)
- `43828a6d` Avoid cloning whole file tree on view update & flatten entries (#12221)
- `e8024b5a` Honor force-included paths in lazy repo metadata (#12235)
- `42642758` Register non-recursive watchers for Linux (#12176)
- `03ad9ea9` Do not eagerly expand subtrees on lazy loaded repo update (#12211)
- `2aa06b13` Fix watcher rebuild storm: skip re-indexing gitignored directories (#13151)

### editor

- `d7ecfac5` Fix header horizontal alignment in markdown viewer (#12371)
- `bf14cbec` Fix editor find/replace not matching non-ASCII text (#12547) — relevant to CJK users
- `be547674` refresh changed local Markdown images (#13764)
- `7867010a` make render test logging initialization idempotent (#13889) — test-only

### context_chips (verify each; a couple touch adjacent concerns)

- `38f8d5b9` stop GitDiffStats flicker from shell fallback (#9244)
- `1175e82f` Fix race condition in git branch/diff-stats chip initialization (#10265)
- `59e802ea` Fix linked-worktree branch checkout (#9905)
- `df4c8d2a` Skip periodic chip refresh when fingerprint matches (#10307)
- `9eadcf93` use branch_name (missed PR comment) (#10851)
- `b9a17537` Add "Create new branch" option to branch switcher (#10610) — feature, not a fix
- `ad32ee26` Support diff stats chip for remote sessions (#11214) — **RISK**: remote sessions may be stripped in Zap; verify before taking
- `2fe9d43c` Fix stale git diff chip and code review button (#11242) — touches code_review UI; verify
- `262a6696` Abort DirectoryFetcher futures (#12355)
- `24b585eb` Avoid cloning display menu items on every render (#12357)
- `8ee42169` Align git branch status chip styling with the built-in branch chip (#13349)

**Effort:** ~23 commits, mostly small. Budget ~0.5–1 day including conflict
resolution and builds. This is a recurring chore (re-run the enumeration each
sync; `-x` provenance lets future sweeps skip what is already taken).

---

## Workstream B — TUI port scope

`crates/warp_tui` is 143 files, **entirely new since the fork** (not something Zap
deleted). It is not a standalone cherry-pick; it fails at three layers.

### Layer 1 — the crate (additive)

Drop-in, no conflict with existing Zap code. But see layers 2–3 before it compiles
or does anything useful.

### Layer 2 — feature hooks threaded into the app/core crates

`warp_tui` depends on `warp` and `warpui_core` built with a `"tui"` feature. That
feature is wired into 35 source files (2 spec `.md` files ignored). Split by
whether the host file still exists in Zap:

**Re-thread in place (host EXISTS in Zap) — ~23 files:**
```
app/src/ai/agent_conversations_model.rs
app/src/ai/blocklist/action_model.rs
app/src/ai/blocklist/mod.rs
app/src/ai/document/ai_document_model.rs
app/src/ai/mcp/file_based_manager.rs
app/src/ai/mcp/file_mcp_watcher.rs
app/src/ai/mcp/templatable_manager.rs
app/src/cloud_object/model/persistence.rs
app/src/lib.rs
app/src/settings/init.rs
app/src/terminal/local_tty/mod.rs
app/src/terminal/local_tty/terminal_manager.rs
app/src/terminal/mod.rs
app/src/user_config/mod.rs
app/src/warp_managed_paths_watcher.rs
app/src/workspaces/user_workspaces.rs
crates/warpui_core/src/core/app.rs
crates/warpui_core/src/core/mod.rs
crates/warpui_core/src/core/view/context.rs
crates/warpui_core/src/core/view/mod.rs
crates/warpui_core/src/core/window.rs
crates/warpui_core/src/lib.rs
crates/warpui_core/src/presenter.rs
```

**Host ABSENT in Zap (cloud/orchestration/auth/server — hook has nothing to attach
to; the dependent TUI code must be dropped, not ported) — ~10 files:**
```
app/src/ai/blocklist/orchestration_event_streamer.rs
app/src/ai/blocklist/orchestration_topology.rs
app/src/ai/orchestration/mod.rs
app/src/ai/orchestration/remote_child.rs
app/src/ai/orchestration/snapshots.rs
app/src/ai/orchestration/validation.rs
app/src/auth/auth_manager.rs
app/src/server/server_api.rs
app/src/server/sync_queue.rs
app/src/tui/mcp.rs
crates/warpui_core/src/elements/gui/hoverable.rs
crates/warpui_core/src/elements.rs
```

### Layer 3 — cloud coupling inside warp_tui

Of 80 non-test source files, **56 have zero cloud/orchestration coupling
(portable as-is)** and **24 need rewire or removal**. The coupled files are the
orchestration / cloud-run surfaces — features Zap does not want:

```
terminal_session_view.rs        cloud_run_view.rs
orchestration_block.rs          orchestration_model.rs
session_registry.rs             orchestration_tab_bar.rs
cloud_run.rs                    orchestration_block/configuration.rs
agent_block.rs                  orchestration_block/render.rs
agent_message.rs                tui_builder.rs
input/view.rs                   conversation_menu.rs
...
```

### Recommended TUI scope: "core TUI, no orchestration"

Drop the orchestration/cloud-run feature entirely and keep the core terminal +
agent front-end. Concretely:

1. Import the 56 zero-coupling files as-is.
2. Exclude `orchestration_*`, `cloud_run*`, `session_registry`, and their tests.
3. Rewire the remaining coupled files (`agent_block`, `agent_message`,
   `input/view`, `tui_builder`, `conversation_menu`, `terminal_session_view`) from
   Warp's cloud inference/agent calls to the Zap BYOP path
   (`ai::agent_providers::chat_stream` / `oneshot` / the prompt-override system).
4. Re-thread the ~23 EXISTS `"tui"` hooks into Zap's versions of those files.
5. Skip the ~10 ABSENT hooks; delete the TUI code that needed them.
6. Add the `tui` feature + `warp_tui` to the workspace and an entry bin.

**Effort:** multi-day port, not a cherry-pick. The bulk is step 3 (BYOP rewiring of
the agent-facing views) and step 4 (hook re-threading across diverged app-crate
files). Steps 1–2 are mechanical.

**Note on MCP:** Zap already has its own MCP plumbing (`app/src/ai/mcp/*`,
`.mcp.json`), so Warp's TUI MCP surfaces likely map onto existing Zap code rather
than requiring new infrastructure.

---

## Common mechanics & risks

- Work on a dedicated `sync/warp-oss-<date>` branch, never on a feature branch.
- Use `git cherry-pick -x` for provenance so future sweeps skip taken commits.
- "Fully self-contained" is a candidate signal, not a clean-apply guarantee;
  every pick needs its own build + test.
- Warp moves daily; treat this as a periodic sync, not a one-time task.
- BYOP is unaffected by Workstream A (no `app/src/ai` changes). Workstream B
  deliberately routes the TUI onto the BYOP layer instead of Warp's cloud stack.

## Suggested sequencing

1. Workstream A first (fast, low risk, immediate value).
2. Decide whether the core TUI is worth the multi-day port in Workstream B.
3. If yes, do the mechanical import (steps 1–2), then BYOP rewiring (step 3),
   then hooks (step 4), building incrementally.

---

## Progress log

### Sync-source finding (important)

Two upstreams, split by purpose:

- **General fixes -> `zerx-lab/zap` (upstream/*), not Warp OSS.** Their feature
  branches share our exact lineage and are BYOP-native, so they cherry-pick
  cleanly (~2 files each). Warp OSS, by contrast, is 1,683 commits ahead with
  architecture rewrites and yielded ~50% with heavy conflicts. Prefer zerx-lab
  as the fix source. Candidates: `fix/issue-260-home-dir-file-tree`,
  `fix/issue-276-ai-suggestions-language`, `fix/issue-145-models-dev-loading-stuck`,
  `fix/issue-193-multimodal-caps-override`. (`issue/116-rules-agent-context` is
  already in our tree.)
- **TUI -> only Warp OSS has it.** zerx-lab/zap has no `warp_tui`/`ratatui` in any
  branch, so there is no BYOP-native TUI to adopt. The port + cloud->BYOP rewire
  is the only path.

### Workstream A cherry-pick results (from warp/master)

Landed 12 of 23 candidates on `josh/warp-oss-sync`:

- `repo_metadata`: 2/8. Rest skipped — tails of an upstream `entry.rs`
  NodeBuilder rewrite and a `watcher.rs` git-routing refactor Zap never took.
- `warp_editor`: 1/4. `bf14cbec` (non-ASCII find/replace) intentionally dropped:
  Zap already implements the same reverse-DFA fix.
- `context_chips`: 9/11. Plus one adaptation commit — a pick referenced
  `FeatureFlag::RemoteCodeReview` (a cloud feature Zap stripped); remote sessions
  now return false for code-review support.

Verified with `cargo check` on `repo_metadata`, `warp_editor`, and
`warp --features gui`. Tests not yet run.

### Workstream B TUI port — phase plan and status

- **Phase 0 (done): stage the crate.** `crates/warp_tui` imported from
  `warp/master` (143 files) and added to workspace `exclude` so the build is
  unaffected while porting proceeds.
- **Phase 1 (next): graft the `tui` feature into `warpui_core`.** `warpui_core`
  itself diverged heavily between the forks (e.g. `core/app.rs` is ~404+/723-
  different), so the `#[cfg(feature = "tui")]` hooks must be extracted hunk-by-hunk
  from warp/master and grafted onto Zap's versions of the 7 present files; the 2
  absent files (`elements.rs`, `elements/gui/hoverable.rs`) need Zap equivalents.
  Goal: `cargo check -p warpui_core --features tui` green.
- **Phase 2: graft the `tui` feature into the `warp` app crate** (`tui =
  ["warpui_core/tui"]` plus ~23 present hook sites; skip the ~10 absent
  cloud/orchestration ones).
- **Phase 3: bring `warp_tui` into the build**, dropping orchestration/cloud-run
  files and rewiring the agent surfaces to the BYOP layer.
- **Phase 4: entry bins + workspace wiring + full build.**

### Status update — Phases 1 & 2 DONE

- **Phase 1 (done):** `cargo check -p warpui_core --features tui` compiles; GUI
  build verified unaffected. Key moves: edition 2021 -> 2024, imported ratatui
  runtime + element system (gated), relaxed the view read/update trait path
  `View` -> `Entity` (the central change), and — instead of the upstream
  `StoredView` enum (~83 GUI hot-path sites) — isolated TUI views in a separate
  gated `Window.tui_views` map (zero GUI blast radius).
- **Phase 2 (done):** `tui = ["warpui/tui"]` on the app crate (routed through the
  warpui umbrella crate, since Zap's app depends on warpui not warpui_core
  directly). `cargo check -p warp --features tui` and `--features gui,tui` both
  green. The ~40 app-crate hook sites were NOT needed to compile; they are only
  required where warp_tui calls specific APIs, so graft on demand in Phase 3.

### Phase 3 wall — crate-topology mismatch (needs a decision)

warp_tui is built on warp's crate topology, which Zap never adopted. Three crates
it depends on are missing from Zap:

- **`warp_channel_config`** (~113 lines, 1 file): used only by the 5 per-channel
  bins via `load_config!`. Small — import it, or ship only the `oss` bin (which
  uses the runtime generator, not the macro).
- **`warp_errors`** (~606 lines): used for `report_error!` / `ReportErrorLogMode`
  in ~4-6 sites (some in cloud-drop files). Adapt away to `log::error!` as already
  done in warpui_core; don't import.
- **`warp_search_core`** (~4256 lines, 11 files): THE WALL. warp extracted the
  `inline_menu` system into this shared crate so the GUI app and TUI could share
  it. **Zap kept its inline_menu inside the app crate**
  (`app/src/terminal/input/inline_menu/`). warp_tui's slash-command / inline-menu
  / mcp-menu / option-selector (4 files) import `warp_search_core::inline_menu::*`.
  Reconciling this is a real decision, not mechanical:
  - (a) extract Zap's inline_menu into a shared `warp_search_core` crate (large
    app-crate refactor), or
  - (b) import warp's `warp_search_core` (a second, parallel ~4000-line menu
    implementation, likely incompatible with Zap's), or
  - (c) rewire warp_tui's 4 menu files onto Zap's app-crate inline_menu (if the
    types are pub-accessible through the `warp` crate), or
  - (d) stub the TUI menu system for a first cut (no slash commands / inline menus)
    to reach a running skeleton, then revisit.

Then, still remaining after the crates are resolved: drop the ~11
orchestration/cloud_run files, and rewire the agent surfaces (agent_block,
agent_message, input/view, terminal_session_view, conversation_menu, tui_builder)
from warp's cloud inference/agent calls to the Zap BYOP layer.

**Assessment:** Phase 3 is multi-session and gated on the search_core topology
decision above. Phases 1-2 (the framework foundation) are complete and GUI-safe.

### Phase 3a-3b done + 3c error map

- **3a (done):** imported the trimmed `warp_search_core` (inline_menu subset, no
  Tantivy), vendored `debounce` locally. Compiles clean.
- **3b (done):** `warp_tui` manifest now resolves and the crate is a workspace
  member (NOT in `default-members`, so the default build is unaffected even while
  its lib is uncompiled). Manifest work: dropped the 5 per-channel bins (need
  `warp_channel_config`, deferred to phase 4), dropped `warp_errors` +
  `warp_channel_config` deps, fixed `fs4`/`unicode-segmentation` to direct
  versions, added `warp_search_core` to `[workspace.dependencies]`.
- **3c (mapped, not done):** `cargo check -p warp_tui --lib` = **95 errors**,
  almost all unresolved imports. Breakdown:
  - **49 = `warp::tui_export`** — THE seam. warp's app crate exposes a
    `tui_export` facade module (276 lines, 79 `pub use` re-exports) that hands
    types to warp_tui. Zap lacks it. Grafting is per-export triage: ~60% are
    types Zap has (keep, fix path), **~40% re-export cloud/orchestration types Zap
    stripped** (`orchestration_config`, `ambient_agents`, `RunAgentsRequest`,
    `Harness`, `orchestration_event_streamer`, `connected_self_hosted_workers`,
    …) — drop those, and drop the warp_tui modules that consume them.
  - 3 = `warp_errors` (adapt to `log::error!` as elsewhere)
  - ~10 = smaller API-path gaps (`warp::editor::CodeEditorModel`,
    `warp_util::local_or_remote_path`, `warp::settings::Tui*`, a few `ai::agent`
    types)
  - Plus: 7 cloud/orchestration modules (`cloud_run*`, `orchestration_*`,
    `session_registry`, `orchestrated_agent_identity_styling`) woven into 3-6
    other modules each — excision touches ~15 interconnected modules.

**Remaining Phase 3 = graft a Zap-adapted `tui_export` (cloud re-exports removed)
+ excise the cloud/orchestration modules + rewire the agent surfaces to BYOP.
Multi-session.** The default GUI build stays green throughout (warp_tui is a
non-default workspace member).

### Phase 3c probe result — tui_export needs ~half its surface BUILT, not just cloud dropped

Imported warp's `tui_export` (gated) and built `warp --features tui`: **55 errors
across 33 re-export groups**. The probe was reverted to keep `warp --features tui`
green; the finding is the deliverable. The facade splits ~50/50:

- **RESOLVES in Zap (≈half — keep in the eventual Zap facade):** terminal
  model/grid/blocks, themes, throttle, `tui` MCP types, `util::image`,
  repo_detection, appearance, llms, conversation core, and the core agent
  action/context/output types. So the terminal/rendering/MCP surface warp_tui
  needs is largely available.
- **MISSING — cloud (drop):** `orchestration*`, `RunAgents*`, `StartAgent*`,
  `server_api`, `connected_self_hosted_workers`, `ambient_agents`, `harness*`,
  oz child-launch.
- **MISSING — Zap app-crate diverged (must repath/adapt or build):**
  slash-command mixer/model, skills selection, model picker, `git_repo_model`
  (repath: exists in Zap under different names — see below),
  `conversation_selection`/`conversation_restoration`, `diff_storage` (adapt onto
  Zap's different edit-persistence model). These are NOT cloud — they are
  app-crate features warp added that Zap never built or structured differently.

**Reframed scope:** the warp_tui port is multi-session not because of one seam but
because warp_tui depends on ~half of warp's app-crate feature surface Zap lacks
(cloud + diverged app infra).

**Direction (per the "match Warp minus cloud" north star — see
`docs/DESIGN-ZAP-FORK.md`): BUILD/PORT the non-cloud missing features for parity;
drop ONLY cloud.** So the earlier "minimal v0, stub/drop slash+skills" idea is
superseded. Per-feature plan for the MISSING groups:

- **Already in Zap at diverged paths → port warp's newer types in (adapt, not
  build):**
  - slash commands → `app/src/search/slash_command_menu` +
    `app/src/terminal/input/slash_command_model.rs` (add `SlashCommandMixer`,
    `TuiSlashCommandDataSource`, `SlashCommandKind/Surfaces`, the record_* fns, …)
  - skills selection → `app/src/terminal/input/skills` (add `SelectableSkill`,
    `query_selectable_skills`, …)
  - model picker → `app/src/terminal/input/models` (add `ModelPickerChoice`,
    `query_model_picker_choices`)
  - `git_repo_model` → **exists in Zap, just renamed.** `GitRepoModels` ↔ Zap's
    `GitStatusUpdateModel` (same `.subscribe(&repo_path, ctx)` shape),
    `GitRepoStatusModel`/`GitStatusMetadata` ↔ same names, all in
    `app/src/code_review/git_status_update.rs`; `detect_possible_git_repo` in
    `crates/repo_metadata`. Facade re-export + a `GitRepoModels = GitStatusUpdateModel`
    alias — mechanical, no build.
- **Adapt onto Zap's different architecture:**
  - `diff_storage` → warp_tui's `TuiDiffStorage` implements warp's surface-agnostic
    `DiffStorage` trait (persist `RequestFileEdits` diffs; the TUI applies deltas to
    base content and writes through `FileModel` since it has no editor buffers).
    Zap has the pieces (`FileDiff`, `DiffSessionType`, `ai::diff_validation`) but
    **not** the `DiffStorage` trait — it persists edits via `ApplyDiffModel::apply_diffs`
    (`app/src/ai/blocklist/action_model/execute/request_file_edits/`). Either port the
    trait or re-target `TuiDiffStorage` onto `ApplyDiffModel`.
- **Genuinely absent in Zap → build the local equivalent for parity (or confirm
  cloud-adjacent, then drop):** `conversation_selection`, `conversation_restoration`.
- **Cloud → drop (and drop the warp_tui modules that consume them):**
  orchestration*, RunAgents*, StartAgent*, server_api, connected_self_hosted_workers,
  ambient_agents, harness*, oz child-launch.

Then rewire warp_tui's agent surfaces to the BYOP layer. The default GUI build
stays green throughout (warp_tui is a non-default workspace member).

### Phase 3c progress — facade + report_error shim landed; warp_tui 95 -> 84 (categorized)

Done this session:
- **Zap-adapted `tui_export` facade** (`app/src/tui_export.rs`, gated) compiles;
  `warp --features tui` + GUI both green.
- **Local `report_error!` shim** in warp_tui (Zap has no warp_errors crate). This
  unblocked a compilation-cascade that was masking the real error surface.

`cargo check -p warp_tui --lib` is now **84 real errors**, cleanly categorized:
- **43 = CLOUD `tui_export` items** (AmbientAgent, CloudAgent*, Orchestration*,
  RunAgents*/StartAgent*, remote-child/host/env snapshots, auth-secret picker, oz
  run url, self-hosted workers). These vanish when warp_tui's cloud modules
  (cloud_run*, orchestration_*, session_registry, orchestrated_agent_identity)
  are dropped. **Next step — biggest single unlock, and it's the drop-cloud side
  of the north star.**
- **22 = non-cloud STAGED-OUT items** — Zap has them but they are pub(crate)/
  private (blocklist controller/input/action/context types, view_util,
  alt_screen, blocklist_filter, failed-output presentation). Promote
  pub(crate)->pub + re-add to the facade.
- **~15 = API-path gaps** — Zap named/pathed differently or lacks: `CodeEditorModel(+Event)`,
  `TuiAutoupdateSettings`, `TuiUsageDisplayMode`, mime helpers
  (MIME_SNIFF_BYTES/infer_mime_type), `format_elapsed_seconds`,
  `prompt_history_for_terminal_view`, model-picker + slash-command fns,
  `warp_util::local_or_remote_path`, a couple `ai::agent` types. Repath or port.

**Next session order:** (1) drop warp_tui cloud modules (+ fix the ~15
interconnected referencing modules) — clears ~43; (2) promote visibility for the
22 non-cloud staged items; (3) repath/port the ~15 API gaps; (4) rewire agent
surfaces to BYOP; (5) phase 4 bins.

### Phase 3c excision — COMPLETE

The cloud/orchestration excision cascade is done. All of the following landed
(each a separate commit, default GUI build green throughout):

- Deleted the pure-cloud warp_tui modules (cloud_run, cloud_run_view,
  orchestration_block +dir, orchestration_model, orchestration_tab_bar,
  orchestrated_agent_identity_styling); cloud-free `session.rs`/`keybindings.rs`.
- `agent_message.rs` dropped; `agent_block.rs` stripped of the AgentMessage
  section, the RunAgents/OrchestrationBlock tool-call view + card logic, and the
  TuiBlockingChild::Orchestration variant. MessagesReceivedFromAgents /
  EventsFromAgents output variants are now transcript no-ops.
- `tui_builder.rs` stripped of CloudRunMarkStyles + the orchestration_* /
  cloud_run styles + agent_identity_palette.
- `session_registry.rs` reduced to the terminal-only path (dropped the
  TuiSessionView::Cloud variant, create_cloud_run_session,
  create_remote_child_session, register_cloud_session, wire_orchestration,
  RemoteChildSession, and the orchestration teardown); root_view Cloud arm gone.
- `terminal_session_view.rs` (deepest): orchestration tab bar + StartAgent
  executor + orchestrated-child + tab-focus actions removed; focus state machine
  collapsed; ctrl-c->Interrupt binding preserved directly. Orchestration test
  cluster removed, terminal-only tests kept.

**Result:** `cargo check -p warp_tui` now surfaces **~74 errors, all
facade-population / API-path gaps — zero orchestration errors.** The excision
achieved its goal: the real remaining surface is now visible.

### Phase 3d — facade population + subsystem ports (IN PROGRESS)

**CURRENT STATE: `cargo check -p warp_tui` = 32 errors** (down from 84), GUI gate
green at every commit, all pushed to `origin/josh/warp-oss-sync` (tip commit
`document_action_presentation`). Build the TUI check with plain
`cargo check -p warp_tui` (the crate has no `tui` feature of its own); the app
crate carries it via `cargo check -p warp --features tui`, and the guardrail is
`cargo check -p warp --features gui` after any shared-crate change.

**Two reusable techniques (use these):**
1. **git-extract** a self-contained warp/master file straight into Zap's tree at
   the same relative path so its `super::`/`crate::` imports resolve unchanged:
   `git show warp/master:PATH | sed '/#\[cfg(test)\]/,/mod tests;/d' > destPATH`
   (drops the trailing test module). Then add the `pub mod` + facade re-export.
2. **wildcard-adapt** — warp's big `match AIAgentActionType {…}` blocks enumerate
   variants Zap lacks (`UploadArtifact`, `SearchCodebase`, `UseComputer`,
   `RequestComputerUse`, `StartRecording`/`StopRecording`, `FetchConversation`,
   `StartAgent`, `SendMessageToAgent`, `RunAgents`, `WaitForEvents`, …). Replace the
   catch-all enumeration with `_` so the match stays exhaustive over Zap's subset.

**Workflow that works:** for each still-erroring file, read its *full* `use
warp::tui_export::{…}` set; port only files whose entire missing set is satisfiable
now; GUI-gate; commit; push. Files clear one at a time; the error count is a rough
proxy (some groups share types).

**✅ DONE (foundational + this run):**
- `warp::editor::CodeEditorModel(+Event)` — `#[cfg(feature="tui")]` re-export on the
  public `editor` path (Zap keeps the code editor under private `code::editor`).
- **blocklist model cluster** — promoted `action_model`/`context_model`/`controller`/
  `input_model`/`view_util`/`block::view_impl::common` to `pub mod` (the `pub mod
  block` pattern) + facade re-exports (BlocklistAIActionModel/Event, AIActionStatus,
  ShellCommandExecutor(+Event), NewConversationDecision, BlocklistAIContextModel/Event,
  AttachmentType, block_context_from_terminal_model, PendingQueryState,
  BlocklistAIController, BlocklistAIInputModel, InputConfig, InputType, format_credits,
  format_elapsed_seconds).
- **BYOP `app/src/tui/` module** — always-`LoggedIn` `TuiLoginModel`/`Phase`/`Event`
  + no-op `log_out_tui`, no auth/cloud; crate-root re-exports + facade `log_out_tui`.
- **`run_tui` runtime spine** — `ExecutionMode::Tui` (warp_core),
  `warp_cli::is_worker_invocation`, `LaunchMode::Tui { mount, api_key }` + `TuiMountFn`
  + Tui arms through all 10 `LaunchMode` methods + execution-profiles/`launch()`/api_key
  sites, shared `run_worker_command`, `run_tui`/`run_tui_worker_if_requested`, and the
  `run_internal` dispatch to `crate::tui::init(mount, ctx)`. (`SettingsMode::Tui` NOT
  needed — Zap's `LaunchMode` has no `settings_mode()`.)
- **inert telemetry** — stripped warp_tui `telemetry.rs` to plain event types (Zap's
  `warp_core::telemetry` is no-op shims); `session.rs` fully unblocked.
- **workspace-crate ports:** `LocalOrRemotePath`/`RemotePath`/`HostId` → warp_util;
  `alt_screen_scroll_to_pty_bytes` → warp_terminal.
- **facade + promotions:** `should_intercept_mouse/scroll` (promoted
  `terminal::alt_screen` to `pub(crate)`), `SlashCommandDataSource`/
  `AcceptSlashCommandOrSavedPrompt`/`UpdatedActiveCommands` (via `slash_commands`'
  `pub use data_source::*`), `FileDiff`/`DiffSessionType`.
- **type ports:** `Option*` plain-data types → new `app/src/ai/option_snapshot.rs`
  (lifted out of warp's cloud `orchestration/snapshots.rs`; builders left behind);
  `CLISubagentTarget` → cli_controller; `prompt_history_for_terminal_view` →
  `terminal/history/up_arrow` (promoted `up_arrow` to `pub(crate)`);
  **`AskUserQuestionSession`** + **`document_action_presentation`** git-extracted into
  `crates/ai/src/agent/` (the latter wildcard-adapted; added
  `DEFAULT_PLANNING_DOCUMENT_TITLE` to `ai/document.rs`).
- **Files cleared:** terminal_content_element, input_detection, tui_file_edits_view,
  tui_permission_prompt, option_selector, tui_cli_subagent_view, prompt_history_menu,
  tui_ask_question_view, tui_plan_view (plus session.rs, root_view, etc. earlier).

**REMAINING — the big-subsystem ports + cloud-adjacent items.** Isolated facade
re-exports no longer reduce the count: every still-erroring file mixes present with
genuinely-absent types, so a file clears only when its whole subsystem lands. Port
each from `warp/master` (remote `warpdotdev/warp`; app-side TUI types mostly in
`app/src/tui/`, `app/src/terminal/input/slash_commands/data_source/`,
`app/src/ai/orchestration/snapshots.rs`, `app/src/ai/blocklist/`):

1. **MCP menu data source** (files: mcp_menu, zero_state, inline_menu, input/view,
   terminal_session_view) — `TuiMcpManager(+Event)`, `TuiMcpAction`, `TuiMcpConfigState`,
   `TuiMcpServerStatus`, `TuiMcpTransport`, `TuiMcpSnapshot`. Source: warp
   `app/src/tui/mcp.rs`. **Cascade:** Zap's `FileBasedMCPManager` diverged — adapt
   `global_warp_servers`→`file_based_servers`, and it lacks `config_diagnostic` /
   `global_warp_installation_by_hash` (adapt via `get_hash_by_uuid`/`get_installation_by_uuid`)
   + needs the `MCPServerExt` trait (warp `ai/mcp/mod.rs`) + `active_mcp_config_file_path`
   (warp `warp_managed_paths_watcher.rs`, which uses `SettingsMode` — adapt to the TUI
   config path only). Register `TuiMcpManager` in `crate::tui::init` once ported.
2. **Slash-command TUI mixer** (files: slash_commands, skills_menu) —
   `TuiSlashCommandDataSource(+Args)`, `SlashCommandMixer`, `ParsedSlashCommandInput`,
   `SlashCommandKind`, `SlashCommandSelectionBehavior`, `build_slash_command_mixer`,
   `slash_command_query`, `should_close_slash_command_menu_for_exact_match`,
   `slash_command_selection_behavior`, plus the `record_*`/`saved_prompt_text_for_id`
   helpers and skills (`AcceptSkill`, `query_selectable_skills`). Zap's
   `search/slash_command_menu` + `terminal/input/slash_commands` diverged — expect
   adaptation. (`slash_commands.rs` also has an internal `AUTO_APPROVE` scope error.)
3. **Conversation selection/management** (files: conversation_selection,
   conversation_menu, input_mode_policy, inline_menu, input/view, terminal_session_view)
   — `ConversationSelection(+Event/Handle)`, `AgentConversationEntry(+Id)`,
   `AgentConversationListEntryState`, `AgentConversationListPolicy`,
   `query_conversation_entries`, `InputModePolicy`, `PolicyConfigUpdate`,
   `InputTypeAutoDetectionSource`, `PendingAttachmentSummary`. Source: warp
   `app/src/ai/blocklist/conversation_selection.rs` + `agent_conversations_model/`.
   **⚠ NOT a clean git-extract (investigated):** the app-side
   `conversation_selection.rs` (191 lines) is fairly clean — for the non-test build it
   needs only `AgentConversationListPolicy` beyond present types — BUT the warp_tui-side
   `conversation_selection.rs` also needs `AgentConversationEntry`/
   `AgentConversationListEntryState`/`AgentRunDisplayStatus`, which live in
   `agent_conversations_model` (warp's `AgentConversationEntry` is a **690-line**
   cloud-agent-run type in `agent_conversations_model/entry.rs`). **Zap's
   `agent_conversations_model` is deeply diverged** — it already defines its OWN
   `HarnessFilter`/`AgentManagementFilters` + a different filter model (SessionStatus/
   StatusFilter/SourceFilter/CreatorFilter/…), so a faithful port would CONFLICT.
   This is cloud-adjacent (the agent-run list panel). Plan: reconcile with Zap's
   existing model (add just the entry/policy types the TUI needs onto Zap's version,
   or make the TUI conversation menu BYOP-local), not a straight extract.

   **BYOP-local build plan (chosen approach; investigated in depth):** Zap has NONE
   of warp's list types (`AgentConversationEntry`/`AgentConversationEntryId`/
   `AgentConversationListEntryState`/`query_conversation_entries`); its
   `agent_conversations_model` (1040 lines) models conversations as
   `AgentConversationsModel`/`ConversationMetadata` instead. warp_tui's
   `conversation_menu.rs` uses only `entry.id` + `entry.display.title`, `classify_entry`,
   `query_conversation_entries`, plus cloud `agent_conversations_cloud_metadata_load_failed`
   (DROP). So build a small BYOP-local module (e.g. `app/src/ai/conversation_entry.rs`):
   `AgentConversationEntryId` (wrap `AIConversationId`), `AgentConversationEntry { id,
   display: { title } }`, `AgentConversationListEntryState` enum (Selected/OpenElsewhere/
   Available/Unavailable — verbatim from warp), `AgentConversationListPolicy` trait
   (`classify_entry`), `query_conversation_entries` (fuzzy filter — port warp's fn),
   backed by `BlocklistAIHistoryModel` local conversations. Then git-extract the app
   `conversation_selection.rs` (191 lines: `ConversationSelection` trait +
   `ConversationSelectionHandle`/`ConversationSelectionEvent`/`PendingQueryState`
   +Noop impl — note it re-defines `PendingQueryState`, may collide with the
   context_model one already faceted; alias or reconcile). Rewrite warp_tui
   `conversation_menu.rs`/`conversation_selection.rs` to source entries from this local
   model, dropping the cloud metadata load. `InputModePolicy`/`PolicyConfigUpdate`
   (input_mode_policy.rs) and `InputTypeAutoDetectionSource`/`PendingAttachmentSummary`
   (attachment) are SEPARATE from the menu — split them out. This is a multi-commit
   build, not an extract; do it in a dedicated session.
4. **Model picker** (file: model_menu) — `query_model_picker_choices`/`ModelPickerChoice`.
   Zap's `terminal/input/models/data_source.rs` diverged — port/adapt.
5. **Zero-state data source** (file: zero_state) — `TuiZeroStateDataSource` (warp
   `terminal/input/slash_commands/data_source/zero_state.rs`).
6. **Conversation restoration** (file: terminal_session_view) — `CloudConversationData`
   (cloud — likely DROP/adapt), `ConversationBlockRestorationPlan`,
   `prepare_conversation_block_restoration`, `ConversationFileExport`,
   `export_conversation_markdown`, `maybe_build_ai_query_upsert_event`,
   `GetRelevantFilesController`, `TranscriptScope`, `PtyIntent(+Event)`,
   `TerminalSurface(+Init/Result)`, `RepoDetectionSessionType`,
   `LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE`.
7. **Diff storage adapter** (file: tui_diff_storage) —
   `DiffStorage`/`DiffStorageHelper`/`RegisteredDiffStorage`/`SaveFuture`/
   `UpdatedFileState`/`changed_lines_from_op`/`FileSnapshot`: adapt `TuiDiffStorage`
   onto Zap's `ApplyDiffModel` (Zap has `FileDiff`/`DiffSessionType`/`ai::diff_validation`
   but not the `DiffStorage` trait; persists via
   `blocklist/action_model/execute/request_file_edits/apply_diff_model.rs`).
   **⚠ NOT a clean git-extract (investigated):** warp `diff_storage.rs` (280 lines,
   the DiffStorage/Helper/RegisteredDiffStorage traits + FileSnapshot/UpdatedFileState/
   SaveFuture) imports `crate::code::editor::compute_unified_diff` which is **absent
   from Zap** (would cascade into porting the unified-diff algorithm), and
   `crate::ai::blocklist::diff_types` (Zap keeps FileDiff/DiffSessionType in
   `inline_action/code_diff_view` instead). `changed_lines_from_op` IS portable (small
   fn over Zap's `diff_validation::DiffType`). Plan: either port `compute_unified_diff`
   too, or rewrite `TuiDiffStorage` to persist through Zap's `ApplyDiffModel` directly
   (drop the DiffStorage trait dependency on the warp_tui side).
8. **Editor char-cell rendering** (files: editor_element, editor_interaction) —
   `warp_editor::render::model::{DisplayLattice, DisplayRow, DisplayRowKind,
   CharCellState, CharCellTemporaryBlock}` (warp `crates/editor/src/render/model/
   char_cell_display.rs` + `mod.rs`). Zap's editor crate lacks char-cell rendering —
   deeper editor port.
9. **tool_call_labels drop-adaptation** (file: tool_call_labels) — big
   `match AIAgentActionType` with absent `SearchCodebase`/`RunAgents`/`StartAgentExecutionMode`
   arms + `RunAgents*`/`SearchCodebase*` result types. Remove the absent-variant arms
   (wildcard-adapt) + drop the cloud result-type imports.
10. **Cloud-adjacent (DESIGN CALL — decide keep-BYOP-inert vs port):**
    `usage.rs` (`ConversationUsageTotals`, `TuiUsageDisplayMode` = credits/cost billing;
    BYOP has no billing), `agent_block.rs` `FailedOutputPresentation` family (cascades
    into `QuotaLimit`/`OutOfCredits`/`AIRequestUsageModel`), `autoupdate.rs`
    `TuiAutoupdateSettings`, `attachment_bar/image_processing` `infer_mime_type`/
    `MIME_SNIFF_BYTES` (+ `warpui::platform::create_system_clipboard`),
    `transcript_view` `BlockSpacing`/`should_show_task_in_blocklist`,
    `tui_cli_subagent_view`/`terminal_session_view` `CLISubagentTarget` (now present —
    facade when group unblocks), `session_registry` `TerminalSurfaceResult`.

`terminal_session_view.rs` is the last file to clear (imports ~35 symbols across
subsystems 1/3/6 + 4 internal `builder` scope errors) — expect it to fall only after
MCP + conversation-selection + restoration land.

**Still-present-in-Zap items to facade when their group unblocks** (mechanical, no
build; VERIFIED present, but each shares a `use {…}` with absent types so none
clears a file alone yet): `GitRepoStatusModel`/`GitStatusMetadata`
(`code_review/git_status_update.rs`; `code_review` is a private `mod` — promote) +
`detect_possible_git_repo` (`repo_metadata`) + a `GitRepoModels`=`GitStatusUpdateModel`
alias; `AgentManagementFilters`/`AgentRunDisplayStatus`/`HarnessFilter`
(`agent_conversations_model`, already facade-reachable); `AcceptSkill`
(`terminal/input/skills/data_source`); `Harness` (`warp_cli`); `CLISubagentTarget`
(now ported). These belong to subsystems 2/3/6 above.

**Reminder — no isolated quick wins remain.** Every still-erroring file mixes
present with genuinely-absent types, so a file clears only when its whole numbered
subsystem lands. Keep GUI green (`cargo check -p warp --features gui`) after every
shared-crate change; warp_tui stays a non-default workspace member so the default
build is unaffected regardless. After all subsystems compile: Phase 4 = entry bins +
workspace wiring, then wire the agent surfaces to BYOP end-to-end.
