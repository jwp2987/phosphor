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
- [ ] **Banner-immune PATH capture** ⚠️ *functional risk* — `__WARP_PATH_CAPTURE_START__/__END__`
  markers + `extract_captured_path`; `app/src/terminal/local_shell/mod.rs:244`
- [ ] Async (background-thread) find — `find/model/async_find.rs`
- [ ] Queued-prompts-while-busy panel — `view/queued_prompts_panel.rs`
- [ ] `TuiStack` element — `warpui_core/elements/tui/`
- [ ] Content-version-aware asset invalidation — `warpui_core` `LocalFileContentVersion`
- [ ] Image load-failure/timeout fallback — `warpui_core` `Image` `on_load_failure`/`on_load_timeout`
- [ ] Soft-wrap row bounds — `FrameLayouts::soft_wrapped_row_bounds` (`app/src/editor`)
- [ ] Home/End on soft-wrapped lines — `EditorAction::MoveToVisualLineStart`/`End`
- [ ] Cross-window tab-drag placeholder collapse — `collapsed_source_placeholder_index`
- [ ] Editable bindings `orchestration_cycle` / `toggle_maximize_pane` — `util/bindings.rs`
- [ ] Oversized data-URI image handling — `replace_oversized_data_uri_images`
- [ ] `remote_server_controller` connection-label helpers — `connection_label_from_session_hosts`
- [ ] Autoupdate per-channel repo + exit-code parsing — `repo_name`/`parse_forcekill_exit_code`
- [ ] `external_control_master` signal plumbing (the #37 refinement; only comments today)

### Build non-cloud half (per 2026-08-02 BYOP decisions on #11)
- [ ] `history_model` reconciliation (rename / event-seq / fork-arity / `transient_network_error`;
  DROP cloud-merge/remote-child/canonical-id)
- [ ] AI bundled + global skills (`ai/skills/{bundled,global_skills}.rs`; DROP the `remote` daemon arm)
- [ ] Persistence: pinned tabs / tab groups / conversation summary + backfill
  (DROP `add_team_uid_to_windows`)

### Platform-gated (Windows/macOS — NOT verifiable on Linux; port compile-only + flag)
- [ ] WSLENV passthrough vars — `wsl_env_allowlist()`
- [ ] WSL program translation (`git`/`gh`) — `command`'s `wsl.rs` `translate_program_for_spawn`
- [ ] Windows PATHEXT exec-resolution — `util/path.rs` fallback
- [ ] Launch-at-login — `app/src/login_item/` (macOS/Windows)

### Large subsystems — each needs a dedicated scope
- [ ] `local_control` / `warpctrl` scripting IPC (~3,000 lines)
- [ ] `repo_metadata` lazy/budget file-tree + `standing_queries`
- [ ] Code review over SSH (`diff_state/{local,remote}`, `git_repo_model`)
- [ ] Remote/SSH global search (`remote_matches_to_global`)
- [ ] URI local deep-links (`UriHost::Session`, `find_terminal_pane_by_session_uuid`)
- [ ] Skill remote-path resolution (`get_scope_for_path`, `LocalOrRemotePath`)
- [ ] `ModelEventDispatcher` SSH gate (`SshRemoteServerSupport::should_use_remote_server`)
- [ ] Managed-secrets BYO-endpoint APIs (`seal_with_context`, `ByoEndpointPayload`)
- [ ] Pending-edit-batch conflict-discard (verify SSH-remote vs cloud-collab FIRST)

---

## Other outstanding (non-#11)
- [ ] **Edition-2024 cross-platform build** — mac/wasm/windows `unsafe` syntax fixed on
  branch `fix/edition-2024-native-targets`; awaiting local macOS `script/run --release`
  verification (no CI-discovery builds). May surface more latent mac errors.
- [ ] **#4 warp_tui suite** — plain `cargo test -p warp_tui --lib` STILL DEADLOCKS at
  `tui_generic_tool_call_view::accepting_new_conversation_suggestion_completes_the_executor`
  (reconfirmed 2026-08-04). The #4 fix may only hold under nextest. Re-investigate;
  do NOT force-green. Also its listed real bugs (diff ghost-blocks, transcript-clear).
- [ ] **#2 sweep** — port the 2 missing GUI auto-resume oracle tests
  (`completed_user_controlled_lrc_{resumes_when_not_suppressed,skips_resume_when_suppressed}`);
  broader 379-module sweep ongoing. (Anchor Stop/auto-resume regression already code-fixed.)
- [ ] **#5 deferred low-sev** — 5 latent items, all still present; low priority.
- [ ] get_relevant_files: live end-to-end smoke against a real BYOP provider (unit + lib green).

## Issue reconciliation status
- **#37** SSH ControlMaster guard — DONE (verify → close). Refinement `external_control_master` still open (above).
- **#4** — NOT done (deadlock reproduces; see above).
- **#2/#5/#11** — tracking issues; stay open. #11 items tracked here.
