# TODO — Warp parity restore ledger (#11 reconciled) + outstanding work

Reconciled 2026-08-04 against the actual code state. `[x]` items in issue #11 =
"keep/restore" (maintainer wants them in the fork). This file is the live tracker:
**mark an item `- [x]` the moment it's verified done.**

## Rules (apply to every item — same as the whole project)
- **`warp/master` is the behavioral oracle.** Port faithfully; adapt only for
  BYOP/local (no cloud) — never silently simplify away Warp behavior (AGENTS §5.10).
- **Tests-first, never defer.** Port Warp's oracle tests with each feature; a red
  test gets fixed now, never parked (AGENTS §5.6). Never weaken an assertion to go green.
- **flock-serialize all cargo:** `ulimit -n 8192; flock
  /home/winters/.claude/jobs/d323e5af/tmp/zap-cargo.lock -c '<cargo>'`. Never run
  cargo concurrently with another agent.
- **English only** (code, comments, tests, docs). Exception: `app/i18n/zh-CN|ja/*.ftl`.
- **Central verification:** the owner re-runs the suite before marking done — don't
  trust an agent's self-report.
- **No CI builds as a discovery loop.** Local `cargo`/user's `script/run --release`
  is the verification; a release build happens once at the end to confirm.

---

## ✅ Done — 25 of the `[x]` keeps already in the code (verified by symbol)

- [x] Theme syncability scope (`is_custom_theme_reference_syncable`)
- [x] Editor relative line numbers (`CodeEditorLineNumberMode`)
- [x] Mermaid diagram config (`mermaid_diagram_config`)
- [x] OSC-52 clipboard access-control (`osc52_clipboard_access`) [#22]
- [x] Box-drawing glyphs (`grid_renderer` box_drawing)
- [x] OSC-8 clickable hyperlinks (`Hyperlink`)
- [x] Terminal hyperlink registry (`HyperlinkRegistry`)
- [x] tmux DCS passthrough
- [x] File-link trailing-punctuation strip (`path_without_trailing_sentence_punctuation`)
- [x] CJK link-boundary via `unicode-general-category`
- [x] warp_tui terminal-background live re-probe
- [x] Block-lifecycle coordinator (`LifecyclePhase`)
- [x] Code-symbol AI context source (`ai_context_menu/code`)
- [x] Configurable context window (`LLMContextWindow` / `configurable_context_window`)
- [x] `AGENT_FOLLOW_UP_INPUTS` "approve"
- [x] Jupyter-notebook detection (`is_jupyter_notebook_file`)
- [x] `file_uri_drive_path_to_windows`
- [x] `sync::Condition`
- [x] NLD heuristic flags (`nld_heuristic`)
- [x] CDPATH-aware `cd` completion (`sorted_cd_directories`)
- [x] Size-based log rotation (`simple_logger` + `warp_logging`)
- [x] context-chips git-branch tracking (`GitBranchTrackingStatus`)
- [x] `SettingSurfaces` / `SettingsMode`
- [x] Browser URL-scheme allowlist (`safe_browser_open_url`) [#25]
- [x] get_relevant_files BYOP tool (bonus — PR #52)

---

## 🔨 Remaining — 31 of the `[x]` keeps still missing

### Small / local — good next builds
- [x] **Banner-immune PATH capture** — `__ZAP_PATH_CAPTURE_*` markers + `extract_captured_path`;
  6 oracle tests 6/0 (verified). Commit `c2404b5eb`. (markers rebranded __WARP_→__ZAP_, identity-only)
- [x] Async (background-thread) find — full subsystem: grid `find_dirty_rows_range` substrate +
  `async_find` module + `is_scanning` trait + `AsyncFindEnabled`/`FeatureFlag::AsyncFind` +
  `BlockFindRenderData` render path; 21 tests; full warp 3871/0 (no regression). Commit `e2226e40a`.
  (additive — gated behind `experimental.async_find_enabled`, off by default)
- [ ] Queued-prompts-while-busy panel — `view/queued_prompts_panel.rs`. ⛔ BLOCKED / DECISION:
  the panel is a thin layer over Warp's `QueuedQueryModel` subsystem (845+1066 lines), which the fork
  DELIBERATELY dropped for a simpler one-shot `/queue` (`settings/ai.rs:407`). Restoring needs porting
  that subsystem + `drain_queued_prompts` AND a maintainer call (keep one-shot `/queue` vs restore
  Warp's persistent auto-queue). Not a simple restore — hold for decision.
- [x] `TuiStack` element — `warpui_core/elements/tui/stack.rs`; 15 oracle tests; full warpui_core
  521/0 (no regression). Commit `9901ff460`. (also ported the opaque-region prereq into container.rs)
- [x] Content-version-aware asset invalidation — `warpui_core` `LocalFileContentVersion`; 4 oracle
  tests; verified across all 4 rippled crates (warpui_core 529/0, warp 3814/0, warp_editor 437/0,
  warp_core 86/0). Commit `29ebd2222`. (AssetSource::LocalFile gained `content_version`; mechanical
  constructor updates across editor/warp_core/app)
- [x] Image load-failure/timeout fallback — `warpui_core` `Image`; 4 oracle tests; full warpui_core
  525/0 (no regression). Commit `f21c31f44`. (fork's Image is at `elements/image.rs`)
- [x] Soft-wrap row bounds — `FrameLayouts::soft_wrapped_row_bounds` + `DisplayMap` wrapper;
  oracle tests; full warp lib 3810/0. Commit `b475f6d60`.
- [x] Home/End on soft-wrapped lines — `EditorAction::MoveToVisualLineStart`/`End` (renamed from
  Home/End) + keybinding rewire; same commit `b475f6d60`. (macOS bindings ported compile-only)
- [x] Cross-window tab-drag placeholder collapse — `collapsed_source_placeholder_index`; 4 oracle
  tests; full warp lib 3814/0. Commit `c3f4b667a`. (adapted to fork's diverged drag-state)
- [x] Editable bindings — `toggle_maximize_pane` ALREADY present (action+handler+`CustomAction`
  registered `bindings.rs:95`, `pane_group/mod.rs:426`); ledger was stale. `orchestration_cycle` is a
  multi-agent-orchestration binding → KEEP-DROPPED (RunAgents/orchestration is dropped cloud per #11).
- [x] Oversized data-URI image handling — `replace_oversized_data_uri_images` + `MAX_DATA_URI_PAYLOAD_BYTES`
  + `IMAGE_TOO_LARGE_PLACEHOLDER`; 2 oracle tests (asset_cache 1/0, warp_editor 438/0). Commit `c26ba8b5b`.
  (did not port the separate `data_uri_source` decode feature — out of scope)
- [x] `remote_server_controller` connection-label helpers — `connection_label_from_session_hosts`;
  3 helpers + 3 oracle tests 3/0. Commit `b6fc0cab1`. ⚠️ *follow-up:* helpers restored + tested but
  not yet wired into the connect_session display flow (fork's `connect_session` signature diverged)
- [x] Autoupdate per-channel repo + exit-code parsing — `repo_name` (adapted to fork's single-repo
  channels) + Windows `parse_forcekill_exit_code`/`parse_minidump_cleanup_exit_code`; warp autoupdate
  13/0. Commit `66884f3cb`. (Windows parsers compile-only here → run on Windows CI)
- [x] `external_control_master` signal plumbing (#37 refinement) — DCS hook -> session -> controller;
  `owns_control_master = !external_control_master`; 2 DCS tests; warp 3825/0. Commit `aaa436aad`.

### Build non-cloud half (per 2026-08-02 BYOP decisions on #11)
- [x] `history_model` reconciliation — PARTIAL: built optimistic rename, event-sequence persistence,
  child-index cleanup (9 tests; warp 3825/0, commit `d472f292d`). NOW ALSO built (commit `81e2e20cc`, warp 3876/0):
  `ConversationStatus::{TransientError,WaitingForEvents}` + `TransientNetworkError`/`TransientNetworkErrorKind`
  + recovery/auto-resume `recovery_pending` wiring (6+10 match sites). STILL deferred (separate features):
  wider-arity `start_new_conversation`/`prompt_history_candidates` (constructor arity), the `WaitForEvents`
  action subsystem (so `WaitingForEvents` never fires yet). Dropped cloud-merge/remote-child.
- [x] AI bundled skills — ALREADY DONE: ported inline in `skill_manager.rs` (BundledSkill,
  load_bundled_skills, activation) on Warp's older flat-HashMap design. Warp's newer `bundled.rs` is a
  remote-catalog rewrite whose net-new content is all the cloud/daemon arm. Ledger was stale.
- [ ] AI global skills (`global_skills.rs`) — ⛔ DECISION → recommend KEEP-DROPPED: cloud-dominant
  (`resolve_skill_repos` → amputated `cloud_environments::GithubRepo`); the only non-cloud fn
  (`filter_skills_by_spec`) has NO consumer in the fork = dead code, and rests on `LocalOrRemotePath`
  (fork uses `PathBuf`) — a faithful port is a workspace-wide type migration, not a skills-dir port.
- [x] Persistence: pinned tabs / tab groups / conversation summary + backfill — 3 migrations +
  schema/model/query + `AgentConversationSummary`; persistence 15/0. Commit `5fff5db83`. (dropped
  `add_team_uid_to_windows` + `total_provider_cost_in_cents` cloud-billing; tab-group/pinned is
  storage-only, GUI round-trip is a separate follow-up)

### Platform-gated (Windows/macOS — NOT verifiable on Linux; port compile-only + flag)
- [ ] WSLENV passthrough vars — `wsl_env_allowlist()`
- [ ] WSL program translation (`git`/`gh`) — `command`'s `wsl.rs` `translate_program_for_spawn`
- [ ] Windows PATHEXT exec-resolution — `util/path.rs` fallback
- [ ] Launch-at-login — `app/src/login_item/` (macOS/Windows)

### Large subsystems — each needs a dedicated scope
- [x] `local_control` / `warpctrl` IPC — CRATE done: `crates/local_control` (protocol/catalog/auth/
  discovery/client, renamed zap/`zapctrl`); 40/0. Commit `baffca6c6`. ⛔ APP-SIDE deferred (prereqs
  out of scope): `FeatureFlag::{WarpControlCli,AgentManagementView}`, `WorkspaceAction::Open*`
  variants (fork has only Toggle*), and the fork has NO Agent-Management view subsystem. Follow-up.
- [x] `repo_metadata` lazy/budget file-tree + `standing_queries` — async budget `build_tree`
  (StopAndLazyLoad + force-included + standing-query hooks) + `standing_queries.rs`; repo_metadata
  87/0, warp compiles. Commit `57e1b624b`. (skipped watcher-rewrite + remote/cloud incremental path;
  standing-query results queryable but app skill-watcher wiring is a follow-up)
- [~] Code review over SSH — IN PROGRESS (branch `parity-remote-getbranches`). Needs ~8 remote-server
  RPCs + a `DiffStateModel` Local/Remote backend refactor + branch-picker/diff wiring. Building the
  request/response RPCs first (foundational, self-contained), then the DiffState subscription
  (snapshot/metadata/delta — the harder subscription model) + consumer refactor.
  DONE (branch, req/resp RPCs — the "what changed" queries):
  • **GetBranches** (`af78b3185`) — proto tag 24, client `get_branches`, daemon reuses
    `DiffStateModel::get_all_branches` (`git for-each-ref`), 3 tests.
  • **GetCommittedBranchFiles** (`1cd53973c`) — proto tag 25, client `get_committed_branch_files`,
    daemon reuses new `DiffStateModel::get_committed_branch_file_entries` (merge-base + `diff --numstat`),
    3 tests.
  NEXT (larger): DiffState snapshot/metadata/delta (the diff CONTENT — a subscription model, harder) +
  the `DiffStateModel` Local/Remote backend refactor + branch-picker/diff wiring (makes it user-visible),
  then git ops (commit/push; BYOP-gate GenerateCommitMessage, keep CreatePr).
  Fork transport: direct ClientMessage oneof entries (no oracle `host_scoped_request`).
- [x] Remote/SSH global search — **DONE** on branch `parity-ripgrep-rpc` (5 commits). Layers:
  (1) host-scoped ripgrep RPC `a580c03de` — proto `RipgrepSearch` req/resp (+Success/Error/Match/Submatch,
  oneof tag 23), `RemoteServerClient::ripgrep_search`, server `ripgrep_search` module (validate/run via
  `warp_ripgrep::search_streaming`, 5000-match+8MB caps) + `server_model` async handler, 2 tests.
  (2) `GlobalSearchMatch` substrate `0d80e7618`. (3) view+model `LocalOrRemotePath` migration `14fa9a59d`
  — directory/match model rekeyed, per-host remote dispatch via `client_for_host().ripgrep_search()`
  (fork per-host-client idiom, NO oracle `PendingRipgrepSearch` machinery), `remote_matches_to_global`,
  `ActiveSearch` multi-source aggregation, richer `Started{remote_host_count}`/`Completed{capped,
  local/remote failures}`, remote-open via `OpenRemoteFile{line_col}` → `open_remote_file_with_target`,
  pre-trim `column_num`, `buffer_location` core↔util `HostId` bridges. (4) seam wiring `bac2561c2` —
  remote root bound through `set_server_file_browser_root` (local `DirectoriesChanged` is local-only since
  `active_session_path_if_local` is None for remote); view merges local + server roots in `recompute_roots`.
  Verified: warp lib 3878/0 (33 ignored); 2 pre-existing doctest fails in untouched modules. Manual QA
  (real SSH remote round-trip) + wasm-target build still pending. Also unblocks code-review-over-SSH
  (consumes the client method directly, no UI migration).
- [x] URI local deep-links — Session/TabConfig/settings-widget/OpenFileEditor; 21 tests; warp 3850/0. Commit `f1c9dbaa1`. (cloud/team variants + fork-absent custom_router skipped)
- [~] Skill remote-path resolution — `get_scope_for_path` **DONE** (branch `parity-skill-scope`,
  commit `28dd8faf5`): restored Warp's `SkillScope::Home` variant + `get_scope_for_path(&Path)`;
  `parse_skill` now derives scope instead of hardcoding `Project` (real regression — home skills wrongly
  showed the picker's "Project Skill" badge); `conversion.rs` maps Home↔API both ways. crates/ai 33/0,
  warp app skills 57/0. REMAINING: the `LocalOrRemotePath` half — migrate `get_provider_for_path` +
  `ParsedSkill.path` + skill discovery to `LocalOrRemotePath` for remote-repo skill discovery. That is
  prep-without-consumer in the fork (no remote skill-discovery pipeline; local file-watchers only), so
  DEFER like [[global_skills]] until a remote-skill consumer exists.
- [x] `ModelEventDispatcher` SSH gate — `SshRemoteServerSupport::{Enabled,Disabled}`; per-instance (GUI=Enabled/TUI=Disabled); 4 tests; warp 3850/0. Commit `1c1fff909`.
- [x] Managed-secrets BYO-endpoint APIs — `seal_with_context` + `ByoFirstPartyPayload`/`ByoEndpointPayload` + `validate_field_sizes`; warp_managed_secrets 25/0, wasm 5/0. Commit `e2c5ecfc9`.
- [ ] Pending-edit-batch conflict-discard — ⛔ ASSESS/DESIGN: `PendingEditBatch` lives in the fork's
  `code/global_buffer_model.rs`, which uses a DIFFERENT (working) buffer-sync design. It reads as
  SSH-remote (not cloud-collab: helpers are `seed_remote_buffer_for_test`/`sync_clock_for_remote_test`),
  so buildable per the maintainer note — BUT restoring Warp's design means re-architecting the fork's
  working buffer model (§5.10 risk). Needs a dedicated assess: additive vs rewrite. Not a quick build.

---

## Other outstanding (non-#11)
- [x] ⭐ **SMOKE TESTS** — on merged main (2026-08-05, after the 6 diff-state PRs #59-#64):
  `./script/usage-test --surface both` = **12 pass / 0 fail / 7 skip** (skips are
  needs-real-shell / needs-byop / needs-desktop — environmental), EXIT 0. Full warp lib
  sanity: `cargo test -p warp --lib` = **3910 pass / 0 fail / 33 ignored**. App boots and
  behaves with all parity + diff-state-over-SSH changes in.
- [ ] **Edition-2024 cross-platform build** — mac/wasm/windows `unsafe` syntax fixed on
  branch `fix/edition-2024-native-targets`; awaiting local macOS `script/run --release`
  verification (no CI-discovery builds). May surface more latent mac errors.
- [ ] **#4 warp_tui suite** — plain `cargo test -p warp_tui --lib` STILL DEADLOCKS at
  `tui_generic_tool_call_view::accepting_new_conversation_suggestion_completes_the_executor`
  (reconfirmed 2026-08-04). The #4 fix may only hold under nextest. Re-investigate;
  do NOT force-green. Also its listed real bugs (diff ghost-blocks, transcript-clear).
  UPDATE 2026-08-05: `cargo nextest run -p warp_tui --no-fail-fast` = **579 pass / 18 fail**
  (589 → 597 total). PRE-EXISTING (confirmed: `git log dc885a802..HEAD -- crates/warp_tui`
  is EMPTY — this session's diff-state PRs #61-65 never touched warp_tui). Failures span
  input::view, root_view, session_registry, terminal_session_view, transcript_view,
  tui_diff_storage (×4), tui_file_edits_view, tui_permission_prompt (×2),
  tui_shell_command_view (×4). Spot-check: `input::view::move_up_through_empty_line_positions_cursor`
  PASSES in isolation (nextest parallel-ordering artifact — shared global-state pollution,
  same class as the i18n-isolation item below); `session_registry::focus_drives_events`
  FAILS in isolation (real). So the 18 are a MIX of real bugs + isolation-order artifacts.
  Dedicated #4 effort — not a diff-state regression. (usage-suite's warp_tui-nextest subset
  is green, which is why smoke passed.)
- [ ] **#2 sweep** — the 2 missing GUI auto-resume oracle tests
  (`completed_user_controlled_lrc_{resumes_when_not_suppressed,skips_resume_when_suppressed}`)
  are now PORTED to `terminal/view_test.rs` (2/0; the resumes case needed a
  `GlobalResourceHandlesProvider` mock for the subagent-sidecar persist path; the fork's
  teardown method is `set_user_control_with_stop_reason`, Warp's is `set_user_control_for_teardown`).
  Broader 379-module sweep still ongoing. (Anchor Stop/auto-resume regression already code-fixed.)
- [ ] **#5 deferred low-sev** — 5 latent items, all still present; low priority.
- [ ] **warp-suite i18n test-isolation** (found 2026-08-04) — `drive::export::test_export_untitled_notebook`
  (and likely others) PASS in the full suite but FAIL in isolation: the export default name is a
  localized `t!()` string, and `App::test` never globally inits i18n, so it only resolves when an
  earlier test happens to trigger init. Same class as #4's `slash_commands` note. A test-binary-global
  i18n init would fix both. Pre-existing; NOT a parity-work regression.
- [ ] get_relevant_files: live end-to-end smoke against a real BYOP provider (unit + lib green).

## Issue reconciliation status
- **#37** SSH ControlMaster guard — DONE (verify → close). Refinement `external_control_master` still open (above).
- **#4** — NOT done (deadlock reproduces; see above).
- **#2/#5/#11** — tracking issues; stay open. #11 items tracked here.
