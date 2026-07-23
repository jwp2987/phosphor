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
