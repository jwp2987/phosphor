# TODO — Warp parity restore ledger (#11 reconciled) + outstanding work

Reconciled 2026-08-04; **#11 section re-verified against code on `main` 2026-08-06.**
`[x]` items in issue #11 = "keep/restore" (maintainer wants them in the fork). This
file is the live tracker: **mark an item `- [x]` the moment it's verified done.**

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

## #11 status — verified against code on `main` 2026-08-06

Of issue #11's **56 checked `[x]` items: 44 built + 2 resolved 2026-08-06 eve
(pending-edit-batch core merged via #105; history_model remainder keep-dropped
per #107) = 46. 10 remain**, of which 3 are decisions/holds (global-skills,
skill-remote-path, local_control app-side) and **7 are buildable — now in-flight
on the overnight agent fleet**. (The 13 unchecked `[ ]` items are all
keep-dropped/cloud — OTEL, VoiceInputLifecycle, semantic-search, RunAgents
orchestration, computer-use recording, cloud-mode-v2, product-analytics
telemetry, `IsCloudConversationStorageEnabled`, etc. — not work, by decision.)

### Merged this session
- [x] **Pending-edit-batch conflict-discard** — CORE MERGED (PR #105, `Fixes #101`):
  `PendingEditBatch` 200 ms debounce + push-conflict-discard + save-flush; 3 oracle
  tests green in isolation. Deferred sub-part `BufferConflictDetected` server→client
  push tracked as **#102** (blocks `handle_buffer_conflict_detected` + its 4th test);
  now being built by the fleet. Assessment: `specs/pending-edit-batch/ASSESS.md`.

### Not started — true gaps
- [ ] **WSLENV passthrough vars** — `wsl_env_allowlist` absent (0 hits). Windows-only;
  not verifiable on Linux (compile-only port + flag).
- [ ] **Launch-at-login** — `app/src/login_item/` does not exist. macOS/Windows;
  not verifiable on Linux.
- [ ] **AI global skills** (global arm only) — no `global_skills`/`filter_skills_by_spec`
  in `app/src/ai/skills/`. Bundled arm done; remote arm = cloud (dropped). RECOMMEND
  KEEP-DROPPED (needs maintainer sign-off): the one non-cloud fn has no consumer and
  rests on a `LocalOrRemotePath` type migration.

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
- [ ] **Skill remote-path** — `get_scope_for_path` done (`#59`); but
  `get_provider_for_path(path: &Path)` (`crates/ai/src/skills/skill_provider.rs:174`)
  is still `&Path`, not `LocalOrRemotePath`. DEFER: no remote-skill consumer exists.
- [ ] **`remote_server_controller` connection-label helpers** — defined
  (`remote_server_controller.rs:564`) + tested, but referenced ONLY in its own tests;
  not wired into the `connect_session` display flow.
- [ ] **`local_control` / `warpctrl` app-side** — crate `crates/local_control` exists;
  `app/src/local_control/` is absent. Blocked on `FeatureFlag::{WarpControlCli,
  AgentManagementView}` + a missing Agent-Management view subsystem.
- [ ] **Persistence pinned-tabs / tab-groups GUI round-trip** — migrations + schema on
  main (`crates/persistence/migrations/2026-08-05-*`, storage done); no
  `toggle_pin`/`is_pinned`/`PinTab` in `app/src/workspace` → GUI wiring not done.
- [ ] **repo_metadata standing-queries wiring** — `standing_queries.rs` on main;
  the app skill-watcher wiring that drives it is the follow-up.
- [ ] **Log-rotation deferred wiring** — machinery built (`simple_logger` + `warp_logging`
  rotation); `register_with_rotation` is NOT called at the MCP logger site
  (`app/src/ai/mcp/templatable_manager/native.rs`), and `frontend`/`max_file_size_bytes`
  aren't threaded into `LogConfig`.
- [ ] **code_review over SSH — git write-ops** — diff-state read/view landed (PRs #59–71);
  but NO `CommitFiles`/`PushBranch`/`GenerateCommitMessage`/`CreatePr` in
  `crates/remote_server/proto/remote_server.proto` → remote git actions not done.

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
- [x] **warp-suite i18n test-isolation** (found 2026-08-04) — the 3 deterministically-red
  tests (`drive::export::test_export_untitled_notebook`, `search::…::test_directory_search_support`,
  `workspace::…::terminal_primary_line_falls_back_to_new_session`) were the localized-`t!()`
  case: `App::test` never globally inits i18n, so the key only resolved when an earlier test
  triggered init. FIXED per-test on branch `fix/issue-98-red-tests` (commit `3150a17b9`,
  `Fixes #98`) — all 3 green in isolation, no assertions changed. NOTE: the same class likely
  still affects #4's `slash_commands` tests; a test-binary-global i18n init would close those too.
- [ ] get_relevant_files: live end-to-end smoke against a real BYOP provider (unit + lib green).
- [x] **Vertex provider bugs** — empty-project silent-drop (`#99`) + 8-field payload struct
  (`#100`) FIXED on branch `fix/vertex-provider-bugs` (commit `a08b52777`); `cargo check` clean,
  21 tests pass. → review + merge.

## Issue reconciliation status
- **#37** SSH ControlMaster guard — DONE (verify → close). Refinement `external_control_master` still open (above).
- **#4** — NOT done (deadlock reproduces; see above).
- **#98/#99/#100** — fixed on branches this session (unmerged); **#101** pending-edit-batch core (branch, unmerged); **#102** filed (deferred BufferConflictDetected push).
- **#2/#5/#11** — tracking issues; stay open. #11 items tracked here.
