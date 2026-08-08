# `#[allow(dead_code)]` audit (#207)

This is the full inventory #207 asked for: every `#![allow(dead_code)]` /
`#[allow(dead_code)]` site in the tree at the time of this audit, classified
as **LEGITIMATE** (genuinely unused on purpose), **HIDES-UNWIRED-PORT** (real
code with zero callers — the defect class #207 exists to surface), or
**STALE** (the allow is no longer accurate; the item now has real callers, or
the allow is redundant with a still-present crate-wide blanket).

This is an audit, not a wiring pass (AGENTS.md §5.6 — every defect gets an
issue first, fixed separately). Per #207's own instructions, three buckets
came out of this:

1. **Stale allows** — removed in this PR (4 sites; see below).
2. **Legitimately-unused API surface** — left alone; many already carried a
   reason, several did not and would benefit from one in a follow-up.
3. **Genuine defects (ported, never wired)** — 44 sites. The clearest,
   most consolidated ones got a new issue each (10 total, listed below); the
   rest are recorded in the table with their evidence so they're not lost,
   even without individual issues.

## How this was produced

`rg` for every `allow(dead_code)` occurrence in the tree (460 total, minus
`/target/` and `.worktrees/`), split into:

- **232 `cfg_attr`-conditional** sites (`cfg_attr(target_family = "wasm", ...)`,
  `cfg_attr(not(feature = "local_fs"), ...)`, `cfg_attr(not(windows), ...)`,
  etc.) — compiled out under most feature/platform combinations, so by
  construction they're not "ported but unreachable," they're "unreachable on
  *this* target/feature set." Spot-checked a sample (test-conditional ones,
  `AgentEventConsumerControlFlow::Stop`, `create_log_bundle_zip`,
  `debounce_highlighting_tx`) and all held up as genuinely conditional.
  Bucketed as **LEGITIMATE** rather than exhaustively tabulated below —
  460 rows of "wasm-only" would not be a useful table.
- **8 file/module-level blanket `#![allow(dead_code)]`** — 2 crate-root
  (`app/src/lib.rs`, `crates/warpui_core/src/lib.rs`, the ones #207 names
  directly) and 6 narrower module-level ones. Each investigated by hand
  (below).
- **218 item-level `#[allow(dead_code)]`** attributes not inside a
  `cfg_attr` — the real audit surface. Each one read in context, then
  `rg -n -w <name>` across the tree (excluding its own definition and, where
  relevant, its own test module) to determine reachability. Full table below.

Tally over the 218 item-level sites: **81 LEGITIMATE / 44
HIDES-UNWIRED-PORT / 93 STALE**.

## The two crate-wide blankets #207 names

| location | verdict | notes |
|---|---|---|
| `app/src/lib.rs:4` | **HIDES-UNWIRED-PORT** (by design) | This is the finding, not a single item: its own comment reads *"Orphaned code left over from upstream Zap trimming is temporarily kept; dead_code warnings are suppressed globally."* It admits to masking an unknown quantity of dead code — exactly #207's defect class. Not removed here (see "Why the blanket stays" below). |
| `crates/warpui_core/src/lib.rs:1` | **HIDES-UNWIRED-PORT** (by design) | Same shape as above. `crates/warpui_core/src/image_cache.rs:835` (`ImageCache::evict_size`, tracked by an existing internal `TODO(APP-3877)`) and `crates/warpui_core/src/integration/capture_recorder.rs` (orphaned duplicate of `video_recorder.rs`, new issue #551) are two concrete items this blanket is currently hiding. Not removed here for the same reason as `app/src/lib.rs`. |

### Why the blankets stay

Both `app` and `warpui_core` are large enough, and this audit session has no
working `cargo build` (the box OOMs with ~25 concurrent agents; STEP 4 of
this task explicitly forbids it), that removing either blanket cannot be
verified before pushing — the resulting warning list would be unmeasured and
the PR could not be trusted. Per #207's own "expected shape of the work,"
removing these needs a quiet branch with real build capacity, not this
audit. This PR is the inventory that makes that follow-up tractable: every
item under both crates that's already known-dead is listed below with
evidence, so the actual removal PR does not have to re-derive it.

## The 6 narrower module-level blankets

| location | verdict | action |
|---|---|---|
| `app/src/code/file_tree/snapshot.rs:1` | **HIDES-UNWIRED-PORT** | Kept. A 368-line SumTree-based file-tree data model (`FileEntry`, `FileTreeSnapshot`) that `app/src/code/file_tree/view.rs` — the actual rendered file tree — never references; `view.rs` uses a completely different `repo_metadata::file_tree_store` model instead. Only consumer is the module's own tests. **Filed #536.** |
| `app/src/terminal/model/secrets.rs:1` | STALE | **Removed.** Redundant with the `app` crate-root blanket, and every exported type (`SecretsRegex`, `ObfuscateSecrets`, `SecretLevel`, `RichContentSecretTooltipInfo`, ...) has real external callers across `ai/blocklist`, `terminal/model/*`, `settings_view`, `env_vars`. |
| `app/src/remote_server/diff_state_proto.rs:19` | STALE | **Removed**, plus the stale comment ("non-test consumers land in following diff-state increments") — that future already landed (diff-state-over-SSH is complete per PRs #61-71). One function, `build_diff_state_file_delta`, is still genuinely test-only (the daemon's debounced per-file push path is tracked by #324); converted its coverage from the module blanket to a precise `#[allow(dead_code)]` with a reason, on that one function only. |
| `crates/warp_completer/src/parsers/errors.rs:1` | **HIDES-UNWIRED-PORT** (uncertain) | Kept, with an added comment. `ParseError::unexpected_eof` / `::extra_tokens` / `::internal_error` and `ArgumentError::MissingMandatoryFlag` are genuinely never constructed anywhere in the tree — only `::mismatch` / `::argument_error` are live. Initially removed the blanket on the (correct-for-most-lib-crates) assumption that `pub` items in a lib crate are dead-code-exempt regardless of internal callers; reverted once the parallel per-item audit (chunk 03) surfaced the specific unconstructed variants, because that exemption's exact boundary for enum variants isn't something this session can verify without a compiler. Left as a documented blanket rather than 4 individual allows for the same reason. |
| `crates/onboarding/src/bin/main.rs:1` | LEGITIMATE | Left alone. Standalone dev-preview binary (separate `bin` target, not shipped) for iterating on onboarding slides visually — matches the dev/test-only-helper bucket, and as a `bin` (not `lib`) target its dead-code surface is a normal by-product of a preview tool. |
| `crates/warpui_core/src/elements/new_scrollable/mod.rs:1` | STALE | **Removed.** Redundant with the `warpui_core` crate-root blanket, and `NewScrollableElement`/`ScrollableAppearance`/`SingleAxisConfig`/`DualAxisConfig` all have dozens of real external consumers across `app/src/**`. |

## Stale allows removed in this PR

4 sites, all verified safe without a compiler two ways: either (a) redundant
with a crate-root blanket that stays in place (zero behavior change either
way), or (b) confirmed to have no `deny(warnings)` anywhere in the repo or
CI config, so even a residual warning cannot fail a build.

1. `app/src/remote_server/diff_state_proto.rs:19` (+ stale comment)
2. `app/src/terminal/model/secrets.rs:1`
3. `crates/warpui_core/src/elements/new_scrollable/mod.rs:1`
4. `crates/warp_completer/src/parsers/errors.rs:1` — attempted, **reverted**
   (see table above); net change on this file is a documentation comment,
   not a removal.

## New issues filed for HIDES-UNWIRED-PORT findings

44 item-level sites came back HIDES-UNWIRED-PORT. Rather than one issue per
line (several are near-duplicates of the same root cause, and several are a
single unused accessor not worth its own tracker entry), the clearest,
best-evidenced, and most consolidated findings got a new issue each. The
rest are recorded with full evidence in the table below so nothing is lost,
just not all individually tracked.

| issue | covers |
|---|---|
| #536 | `code/file_tree/snapshot.rs` — SumTree file-tree model never wired into the view |
| #547 | `view_components`: `ActionButton.callout`, `AlertConfig::success`, `Dropdown::Naked`/`set_style` |
| #548 | `workspace/view/launch_modal`: `CTAButton`/`Slide` framework, zero live `Slide` implementors |
| #549 | Duplicate dead test-fixture helpers (`WarpDirs`/`Dirs::git_repository_fixture`, `Zap::executable`, `Zap::fixtures`) in both `app/src/test_util/virtual_fs.rs` and `crates/virtual_fs/src/lib.rs` |
| #550 | `warpui_core::integration::capture_recorder` — orphaned duplicate of `video_recorder` |
| #551 | `drive/items/space.rs`: `WarpDriveSpace::new` never constructed |
| #552 | `search/ai_context_menu`: `render_search_bar` never placed in the render tree |
| #553 | `terminal/writeable_pty/remote_server_controller.rs`: reinstall-vs-fresh-install messaging (`for_update`) never differentiated |
| #554 | `code/editor_management.rs`: `CodeManagerEvent::EditCompleted` emitted, no subscriber |
| #555 | `prompt/editor_modal.rs`: same-line-prompt toggle UI never wired despite a fully working backing state machine |

All ten reference #207.

## Full table: 218 item-level `#[allow(dead_code)]` sites

Legend: **L** = LEGITIMATE, **H** = HIDES-UNWIRED-PORT, **S** = STALE.

| location | item | V | evidence |
|---|---|---|---|
| app/src/menu.rs:2279 | `Menu<A>::add_item` | S | Called in `app/src/notebooks/editor/block_insertion_menu.rs:97,108,125,128`. |
| app/src/warp_managed_paths_watcher.rs:202 | `WarpManagedPathsWatcherEvent` (wasm cfg, 0 variants) | L | Wasm-only stub pairing with the wasm-only watcher; the non-wasm variant of the same enum has real callers. |
| app/src/editor/view/model/display_map/mod.rs:330 | `DisplayMap::anchor_after` | S | Called at `editor/view/model/mod.rs:2114`. |
| app/src/editor/view/model/display_map/mod.rs:364 | `DisplayPoint::zero` | S | Called at `display_map/mod.rs:265`, `editor/view/mod_test.rs:27`. |
| app/src/editor/view/model/mod.rs:139 | `UpdateBufferOption::SkipUndoRedoRecord` | S | Matched at `mod.rs:152`, constructed in `mod_test.rs:122,134`. |
| app/src/themes/theme_chooser.rs:405 | `ThemeChooser::themes` | L | Called at `integration_testing/settings/step.rs:65`, matching its own comment. |
| app/src/lib.rs:4 | crate-wide blanket | H | See "the two crate-wide blankets" above. |
| app/src/lib.rs:25 | `mod context_chips;` | S | `crate::context_chips::` used in 40+ files. |
| app/src/lib.rs:69 | `mod remote_server;` | S | `crate::remote_server::` used across many files, called directly at `lib.rs:756,761`. |
| app/src/lib.rs:420 | `LaunchMode::add_url` | S | Called at `lib.rs:930`. |
| app/src/view_components/compactible_split_action_button.rs:21 | `CompactibleSplitActionButton` struct | S | Used in `ai/blocklist/block/cli.rs:450`, `inline_action/requested_command.rs:230`, `inline_action/code_diff_view.rs:488`. |
| app/src/view_components/compactible_split_action_button.rs:31 | `CompactibleSplitActionButton::new` | S | Called at `cli.rs:546`, `requested_command.rs:299`, `code_diff_view.rs:855`. |
| app/src/view_components/dismissible_toast.rs:252 | `ToastLink::with_href` | S | Called in `code_review/git_dialog/pr.rs:266`, `workspace/view.rs:7543,12457`. |
| app/src/view_components/action_button.rs:328 | `ActionButton::with_callout` | H | Zero callers; `callout` field is actively rendered (`maybe_render_callout`, line 807). **#547.** |
| app/src/view_components/action_button.rs:371 | `ActionButton::with_width` | S | Chained off `ActionButton::new` in several views. |
| app/src/view_components/action_button.rs:387 | `ActionButton::with_height` | S | Chained off `ActionButton::new` in several views. |
| app/src/view_components/alert.rs:164 | `AlertConfig::success` | H | Zero call sites for `.success(`; `AlertFlavor::Success` has live render logic but is never constructed. **#547.** |
| app/src/view_components/dropdown.rs:37 | `DropdownStyle::Naked` | H | Never constructed; only in exhaustive match arms. **#547.** |
| app/src/view_components/dropdown.rs:229 | `Dropdown::set_style` | H | Zero callers. **#547.** |
| app/src/autoupdate/mod.rs:794 | `UpdateReady` enum | S | Constructed/matched throughout `autoupdate/mod.rs`, `mod_test.rs`, `root_view.rs:1617`. |
| app/src/experiments/mod.rs:37 | `INVALID_GROUP_ASSIGNMENT_ERR` | S | Used at `experiments/mod.rs:293,295`. |
| app/src/experiments/mod.rs:40 | `INVALID_USER_OVERRIDE_ERR` | S | Used at `experiments/mod.rs:310`. |
| app/src/experiments/mod.rs:43 | `NO_LAYER_FOUND_ERR` | S | Used at `experiments/mod.rs:226,228`. |
| app/src/experiments/mod.rs:100 | `BucketRange::new` | S | Called from `login_layer.rs`, `block_onboarding_layer.rs`, `improved_palette_search_layer.rs`, `mod_tests.rs`. |
| app/src/experiments/mod.rs:138 | `Layer` struct | S | Constructed via struct literal in several layer files and `mod_tests.rs`. |
| app/src/experiments/mod.rs:157 | `impl Layer` block | S | Methods called from `Experiment` trait's default methods in the same file. |
| app/src/experiments/mod.rs:352 | `Experiment::set_override` (trait default method) | H | Zero callers anywhere — no settings/UI/debug path calls it. Not independently filed (single trait method, low signal); refs #207. |
| app/src/workflows/categories.rs:290 | `WorkflowsFocusState` enum | S | Constructed/matched at `categories.rs:416,648,891,999,1084,1263`. |
| app/src/ui_components/buttons.rs:16 | `ButtonMode::Accent` | S | Produced via `accent_icon_button()`, called from `details_bar.rs:128`, `workflow_view.rs:1931`. |
| app/src/antivirus/mod.rs:34 | `AntivirusInfoEvent::ScannedComplete` | S | Emitted in `antivirus/windows.rs:66`, matched in `crash_reporting/mod.rs:183`. |
| app/src/drive/workflows/modal.rs:423 | `populate` (private method) | H | Only caller is `modal_test.rs:579` (`#[cfg(test)]`); no production "edit existing workflow" call site. Not independently filed; refs #207. |
| app/src/drive/items/space.rs:18 | `WarpDriveSpace::new` | H | Zero callers; sibling `WarpDriveFolder::new` is constructed in `drive/folders/mod.rs:94`, no analog exists for `Space`. **#551.** |
| app/src/drive/sharing/mod.rs:98 | `Subject::PendingUser` | S | Matched/constructed extensively in `drive/sharing/mod.rs`, `persistence/cloud_objects.rs`, `cloud_object_tests.rs`. |
| app/src/cloud_object/update_manager.rs:825 | `create_ai_execution_profile` | S | Called from `ai/execution_profiles/profiles.rs:302,1334`. |
| app/src/cloud_object/update_manager.rs:847 | `update_ai_execution_profile` | S | Called from `ai/execution_profiles/profiles.rs:1386`. |
| app/src/cloud_object/model/actions.rs:192 | `ObjectActions::object_actions_by_id` field | S | Read/written throughout `actions.rs`. |
| app/src/quit_warning/mod.rs:32 | `QuitScope::EditorTab` | S | Matched/constructed at 9 sites in `quit_warning/mod.rs`. |
| app/src/quit_warning/mod.rs:218 | `UnsavedStateSummary::for_editor_tab` | S | Called from `code/view.rs:1370,1642`, `code_review/code_review_view.rs:7508`. |
| app/src/quit_warning/mod.rs:375 | `on_save_changes` builder | S | Called from `code/view.rs:1402,1661`, `code_review_view.rs:7540`. |
| app/src/quit_warning/mod.rs:382 | `on_discard_changes` builder | S | Called from `code/view.rs:1403,1662`, `code_review_view.rs:7541`. |
| app/src/terminal/mock_terminal_manager.rs:22 | `MockTerminalManager.view` field | L | Doc comment: kept alive for the manager's lifetime — write-only Drop-retention field. |
| app/src/terminal/mod.rs:482 | `ClipboardType` enum | S | Constructed/matched in `terminal/model/grid/ansi_handler.rs:1158-1175`, `terminal/event.rs`. |
| app/src/terminal/view.rs:1092 | `InlineBannersState::last_banner_id` | H | Zero callers anywhere. Not independently filed (single field, low signal); refs #207. |
| app/src/terminal/input.rs:1554 | `ai_follow_up_icon_mouse_state` field | L | Write-only lifetime-retention field, doc-commented. |
| app/src/terminal/input.rs:12422 | `queued_prompts_panel` method | S | Comment says "follow-up increment," but already called at `terminal/view.rs:4491`. |
| app/src/terminal/input.rs:12428 | `is_queued_prompt_inline_editor_focused` | S | Same stale "follow-up" comment; called at `terminal/view.rs:9680`. |
| app/src/terminal/warpify/settings.rs:274 | `WarpifySettings::new_with_defaults` | L | `#[cfg(any(test, feature = "integration_tests"))]`; matches sibling pattern, currently zero call sites even in test code — flagged for follow-up scrutiny. |
| app/src/terminal/local_tty/terminal_manager.rs:100 | `terminal_attributes_poller` field (unix) | L | Write-only lifetime-retention field, doc-commented. |
| app/src/terminal/local_tty/terminal_manager.rs:105 | `pty_controller` field | S | Read at lines 585, 594. |
| app/src/terminal/local_tty/terminal_manager.rs:109 | `remote_server_controller` field | S | Borrowed at lines 283-284. |
| app/src/terminal/local_tty/terminal_manager.rs:121 | `inactive_pty_reads_rx` field | S | `.close()` called at line 579. |
| app/src/terminal/model/secrets.rs:1 | module blanket | S | Removed — see above. |
| app/src/terminal/model/block.rs:306 | `background_executor` field | S | Read at `block.rs:2519`. |
| app/src/terminal/model/ansi/mod.rs:104 | `xparse_color` fn | S | Called at `ansi/mod.rs:873,948`. |
| app/src/terminal/writeable_pty/remote_server_controller.rs:55 | `SshInitState::AwaitingInstall.for_update` field | H | Constructed with real values but every consumer discards it with `..`. **#553.** |
| app/src/terminal/find/mod.rs:1 | `pub mod model;` | S | Re-exports have real external callers (`block_list_element.rs`, `ai/blocklist/block/find.rs`). |
| app/src/terminal/cli_agent_sessions/mod.rs:238 | `CLIAgentSessionsModelEvent` enum | L | Documented rationale in the comment; `agent` field always destructured with `..` by every consumer, matching intent. |
| app/src/terminal/cli_agent_sessions/event/mod.rs:29 | `CLIAgentEventPayload` struct | S | Fields read at `cli_agent_sessions/mod.rs:178-205`, `view.rs:11829,11838`. |
| app/src/terminal/cli_agent_sessions/event/mod.rs:42 | `CLIAgentEvent` struct | S | Constructed/consumed pervasively across `event/v1.rs`, `listener/mod.rs`, `view.rs`. |
| app/src/terminal/find/model.rs:4 | `mod rich_content;` (private) | S | Exported items used throughout `block_list.rs`, `async_find.rs`, re-exported via `model.rs:18-19`. |
| app/src/terminal/view/shell_terminated_banner.rs:126 | `TerminationType::Normal` variant | L | Only match arms, never constructed; adjacent TODO documents it's reserved pending a styling change. |
| app/src/terminal/view/docker_sandbox/mod.rs:43 | `sbx_path` param | L | Branch-conditional inside a `cfg_if!` — unused on some arms only. |
| app/src/terminal/remote_tty/terminal_manager.rs:38 | `view` field | L | Write-only lifetime-retention field, doc-commented. |
| app/src/env_vars/mod.rs:126 | `EnvVarCollection::new` | S | Called at `env_vars/view/env_var_collection.rs:832`. |
| app/src/code/local_code_editor_wasm.rs:27-65 (17 variants) | `LocalCodeEditorEvent` (wasm build only) | L | `code/mod.rs:15-17` `cfg_attr`-swaps this file in only under `target_family = "wasm"`; zero `ctx.emit` calls in the wasm file (only the sibling non-wasm `local_code_editor.rs` emits these) — platform-conditional stub, same shape as the `cfg_attr` bucket, implemented via `path` swap instead. |
| app/src/code/editor/scroll.rs:6 | `ScrollWheelBehavior::OnlyHandleOnFocus` | H | Never constructed, unlike siblings `AlwaysHandle`/`NeverHandle`. Not independently filed; refs #207. |
| app/src/code/editor/scroll.rs:8 | `ScrollWheelBehavior::AlwaysHandle` | S | Constructed at `diff_viewer.rs:56`, default at `editor/view.rs:382`. |
| app/src/code/editor/scroll.rs:10 | `ScrollWheelBehavior::NeverHandle` | S | Constructed at `diff_viewer.rs:55`. |
| app/src/code/editor/scroll.rs:15 | `ScrollWheelBehavior::should_handle` | S | Called at `editor/view.rs:2138`. |
| app/src/code/editor_management.rs:244 | `CodeManagerEvent::EditCompleted` | H | Emitted at `editor_management.rs:311`, matched nowhere. **#554.** |
| app/src/code/editor_management.rs:301 | `CodeManager::complete_pending_diffs` | S | Called from `code/view.rs:775,780`. |
| app/src/code/editor/view.rs:120 | `CodeEditorEvent::DiffHunkContextAdded.line_range` | S | Read at `code_review_view.rs:5713`. |
| app/src/code/editor/view.rs:1496 | `CodeEditorView::line_at_vertical_offset` | H | Zero callers; the underlying model method is called directly instead. Not independently filed; refs #207. |
| app/src/code/editor/element/gutter_button.rs:115 | `CommentButton` enum | S | Matched/constructed in `editor/element.rs`, `editor/view.rs:437`. |
| app/src/code/file_tree/snapshot.rs:1 | module blanket | H | See above. **#536.** |
| app/src/search/ai_context_menu/notebooks/data_source.rs:22 | `NotebookDataSource::new` | S | Called from `search/ai_context_menu/view.rs:957,971,1136,1142`. |
| app/src/search/ai_context_menu/view.rs:1655 | `render_search_bar` | H | Never called from `render()`/`render_main_menu`/etc. **#552.** |
| app/src/search/ai_context_menu/workflows/data_source.rs:19 | `WorkflowDataSource::new` | S | Called from `view.rs:943,1130`. |
| app/src/search/ai_context_menu/rules/data_source.rs:18 | `RulesDataSource::new` | S | Called from `view.rs:985,1148`. |
| app/src/search/ai_context_menu/commands/data_source.rs:15 | `CommandDataSource::new` | S | Called from `view.rs:893,1104`. |
| app/src/workspace/close_session_confirmation_dialog.rs:54 | `impl CloseSessionConfirmationDialog` | S | `::new()`/`set_open_confirmation_source` called from `workspace/view.rs:1641,15613`. |
| app/src/workspace/view/launch_modal/cta_button.rs:13 | `CTAButton.telemetry_event` field | H | No live `Slide` implementor exists. **#548.** |
| app/src/workspace/view/launch_modal/cta_button.rs:35 | `CTAButton::open_url` | H | Zero callers. **#548.** |
| app/src/workspace/view/launch_modal/cta_button.rs:55 | `CTAButton::with_telemetry` | H | Zero callers. **#548.** |
| app/src/workspace/view/launch_modal/cta_button.rs:65 | `CTAButtonAction::OpenUrl` variant | H | Matched but never constructed (constructor is dead). **#548.** |
| app/src/workspace/view/global_search/view.rs:1603 | `get_or_create_directory_entry` | L | Comment: "Will be used in later PRs"; zero callers. |
| app/src/default_terminal/mod.rs:12 | `mod non_mac { .. }` | S | `is_warp_default_terminal` etc. called within the same file on non-macOS. |
| app/src/remote_server/diff_state_proto.rs:19 | module blanket | S | Removed — see above. |
| app/src/integration_testing/agent_mode/assertions.rs:995 | `exchange_with_expected_action_result` | L | Called by `pub fn assert_exchange_action_result`; both are unreferenced elsewhere, consistent with a test-assertion API library surface. |
| app/src/ai/mcp/gallery.rs:14 | `GalleryMCPServer.version` field | S | Read via accessor, called externally at `templatable_manager/native.rs:1154,1163`. |
| app/src/ai/mcp/gallery.rs:16 | `GalleryMCPServer.instructions_in_markdown` field | S | Read via accessor, called externally at `settings_view/mcp_servers/list_page.rs:894`. |
| app/src/ai/mcp/templatable_manager.rs:111 | `is_authenticated_transport` field | L | TODO: "provide a 'log out' button" — written but genuinely never read, documented future-UI field. |
| app/src/ai/mcp/templatable_manager.rs:352 | `ServerInstallationAdded` variant | S | Emitted at `native.rs:1017`; matched at 4 external sites. |
| app/src/ai/mcp/templatable_manager.rs:354 | `ServerInstallationDeleted` variant | S | Emitted at `native.rs:1101`; matched at 4 external sites. |
| app/src/ai/mcp/templatable_manager/wasm.rs:104 | `spawn_cli_ephemeral_server` (wasm stub) | S | Generic call routes here on wasm builds — real caller, no-op body. |
| app/src/ai/ambient_agents/mod.rs:96 | `AmbientConversationStatus::Cancelled` | S | Constructed at `mod.rs:127`, matched at `agent_sdk/driver.rs:1294`. |
| app/src/ai/ambient_agents/mod.rs:100 | `AmbientConversationStatus::Blocked` | S | Constructed at `mod.rs:118`, matched at `agent_sdk/driver.rs:1300`. |
| app/src/ai/outline/native.rs:119 | `RepoOutlines::new_for_test` | S | Called from `test_util/terminal.rs`, `terminal/input_test.rs`, `workspace/view_test.rs`. |
| app/src/ai/agent/conversation.rs:112 | `AddedExchange.task_id` field | S | Destructured/used at multiple sites in `conversation.rs`. |
| app/src/ai/agent/conversation.rs:3401 | `total_request_cost` | S | Called at `integration_testing/agent_mode/mod.rs:188`. |
| app/src/ai/agent_sdk/driver/terminal.rs:326 | `current_directory` | H | Zero callers repo-wide. Not independently filed (single accessor); refs #207. |
| app/src/ai/blocklist/queued_query.rs:226 | `QueuedQueryEvent::EditCancelled.query_id` field | H (uncertain) | Constructed with real data but both consumers ignore it with `..`, unlike sibling variants whose `query_id` IS read. Not independently filed; refs #207. |
| app/src/ai/blocklist/mod.rs:26 | `pub(crate) mod queued_query;` | S | Comment says "follow-up increment," but `QueuedQueryModel`/`Event` are now used pervasively. |
| app/src/ai/blocklist/action_model/execute/call_mcp_tool.rs:24 | `terminal_view_id` field | L | Used on native, `cfg_attr(wasm, allow(dead_code))`-equivalent unused only on wasm. |
| app/src/ai/blocklist/passive_suggestions/legacy.rs:593 | `build_prompt_suggestions_request` (cloud request builder) | L | Superseded by the BYOP equivalent `build_prompt_suggestions_byop_request` (used); old cloud-era codepath left after migration. |
| app/src/ai/skills/skill_manager.rs:102 | `skill_watcher` field | L | Comment: "Can't remove this or it'll get cleaned up after new()" — Drop-lifetime field. |
| app/src/ai/execution_profiles/profiles.rs:89 | `DefaultProfileState::Cli` variant | S | Constructed at `profiles.rs:181`, read at 6 sites. |
| app/src/ai/agent_providers/openai_compatible.rs:17 | `OpenAiCompatibleModel` struct | L | `id` used; `owned_by` doc-commented as "used mainly for UI display," genuinely never read yet. |
| app/src/ai/agent_providers/wire_inspector.rs:568 | `WireInspectorModal.view` field | L | Keep-alive pattern, same shape as `skill_watcher`. |
| app/src/code_review/git_status_update.rs:58 | `impl GitStatusUpdateModel` (non-`local_fs`) | L | `#[cfg(not(feature = "local_fs"))]` stub constructor used by `lib.rs:1672`. |
| app/src/code_review/code_review_view_tests.rs:104 | `create_editor_with_diff` | L | Zero callers anywhere including its own test file; unused test-only helper. |
| app/src/code_review/code_review_view_tests.rs:265 | `TestContext.window_id` field | L | Unused test-struct field, test-only code. |
| app/src/code_review/code_review_view.rs:320 | `InitButtons` enum | L | `OpenRepository` is used at 3 sites; only `None` is unconstructed inside an otherwise clearly-used, wasm-conditional enum. |
| app/src/workspaces/user_profiles.rs:34 | `photo_url` field | S | Read at `drive/sharing/mod.rs:306`. |
| app/src/prompt/editor_modal.rs:591 | `render_same_line_prompt_section` | H | Backing state machine fully wired; only the render call is missing. **#555.** |
| app/src/banner/view.rs:55 | `BannerTextContent::plain_text` | S | Called at `settings_view/mcp_servers/edit_page.rs:181`. |
| app/src/context_chips/context_chip.rs:363 | `RefreshConfig::Periodically` | S | Constructed at `context_chips/mod.rs:122,127,134`, matched in `current_prompt.rs`. |
| app/src/context_chips/context_chip.rs:365 | `RefreshConfig::OnFileChanges` | L | Matched but never constructed; the match arm itself logs "Unimplemented" — self-documented stub. |
| app/src/settings_view/mcp_servers/edit_page.rs:126 | `database_connection` field | H | Opens a real sqlite RO connection but is never read afterward, unlike the sibling field of the same name/type in `templatable_manager/native.rs` which IS read. Not independently filed; refs #207. |
| app/src/settings_view/appearance_page.rs:531 | `thin_strokes_dropdown` field | S | Read/constructed at 4 sites in `appearance_page.rs`. |
| app/src/app_services/mac/single_instance_manager.rs:14 | `LockFileHandle(File)` | L | Comment: `File` kept alive only so the OS releases the `flock` on drop. |
| app/src/test_util/virtual_fs.rs:5 | `WarpDirs::git_repository_fixture` | H | Module not re-exported from `test_util/mod.rs`, unreachable even in principle. **#549.** |
| app/src/test_util/virtual_fs.rs:16 | `Zap::executable()` | H | Zero call sites anywhere. **#549.** |
| app/src/test_util/virtual_fs.rs:35 | `Zap::fixtures()` | H | Zero external callers. **#549.** |
| crates/editor/src/content/text.rs:893 | `BufferBlockStyle::Table.cache` field | S | Read via `content/buffer.rs:2390`, `content/edit.rs:1094`. |
| crates/warp_tui/src/telemetry.rs:21 | `TuiAutoupdateTelemetryEvent` enum | L | Write-only payload for the no-op `send_telemetry_from_ctx!` macro — documented pattern. |
| crates/warp_tui/src/terminal_session_view.rs:492 | `ToggleResponseSummaryVisibility` variant | L | Doc comment: "deliberately kept" pending a future keybinding, citing AGENTS §5.10. |
| crates/warp_tui/src/slash_commands.rs:31 | `TuiSlashCommandRow` struct | S | Used extensively, constructed at line 528, plus tests. |
| crates/warp_tui/src/slash_commands.rs:68 | `TuiSlashCommandState` enum | S | Driven throughout the file. |
| crates/warp_tui/src/report_error.rs:10 | `ReportErrorLogMode` enum | L | Doc comment: "kept for source compatibility with warp's macro." |
| crates/warp_tui/src/inline_menu.rs:48 | `TuiInlineMenuRowStyle` enum | S | Used pervasively across every menu file. |
| crates/warp_completer/src/parsers/errors.rs:1 | module blanket | H (uncertain) | See "narrower blankets" above — kept, documented. |
| crates/warp_completer/src/completer/suggest/test.rs:40 | `TEST_ROOT_DIR` const | S | Used at line 1578. |
| crates/warp_completer/src/signatures/testing/v2.rs:50 | `create_hidden_argument_suggestion` | L | TODO: documented future-API reason. |
| crates/settings/src/schema_tests.rs:194,233,251,257 | test-only derive fixture types | L | Used only via `JsonSchema` derive reflection inside their own tests. |
| crates/settings/src/macros.rs:716,727,737,778,816 | macro-generated per-setting items | L | Genuinely called per-group but rustc's per-expansion analysis can't universally verify it. |
| crates/repo_metadata/src/telemetry.rs:1 | `BuildTreeFailed` variant | L | Write-only payload for the no-op telemetry macro, same pattern as `warp_tui/telemetry.rs`. |
| crates/repo_metadata/src/entry.rs:883 | `is_common_git_config` | L | Doc comment: "reserved for the watcher tracking-state logic ... out of scope for this local port." |
| crates/persistence/src/model.rs:500,502,510,512 | `PaneBranch`/`PaneNode` fields | L | Diesel `Queryable` column-layout requirement. |
| crates/sum_tree/src/lib.rs:87 | `SumTree::first()` | L | Trivial symmetric counterpart to used `last()` in a general-purpose library crate. |
| crates/sum_tree/src/lib.rs:381 | `SumTree::insert()` | L | Sole `KeyedItem` impl (the dead `file_tree/snapshot.rs`) uses `.edit()` instead — unused generic-library API. |
| crates/sum_tree/src/lib.rs:502 | `Edit<T>` enum | S | Constructed at `file_tree/snapshot.rs:231,237`. |
| crates/sum_tree/src/cursor.rs:226 | `Cursor::prev()` | S | Called at `editor/src/render/model/mod.rs:3360`, `content/cursor.rs:102`. |
| crates/integration/src/user_defaults.rs:6 | `input_mode()` | S | Called at `integration/src/test.rs` (6 sites). |
| crates/onboarding/src/bin/main.rs:1 | crate-wide blanket | L | Standalone dev-preview binary — see above. |
| crates/managed_secrets/src/envelope.rs:38 | `UploadKey.public_key` field | L | Read only in `#[cfg(test)]`. |
| crates/virtual_fs/src/lib.rs:22 | `Dirs::git_repository_fixture()` | H | Zero external callers. **#549.** |
| crates/virtual_fs/src/lib.rs:29 | `Stub::FileWithContent` variant | S | Used in dozens of test files across several crates. |
| crates/virtual_fs/src/lib.rs:162 | `Zap::executable()` | H | Zero callers, duplicate of the app-side copy. **#549.** |
| crates/virtual_fs/src/lib.rs:181 | `Zap::fixtures()` | H | Zero external callers. **#549.** |
| crates/ai/src/skills/parser.rs:11,30,37 | `ParsedMarkdown`/`parse_markdown_file`/`parse_markdown_content` | S | All called from `parse_skill.rs`. |
| crates/warp_core/src/operating_system_info.rs:126 | `OperatingSystemCategory::Windows` | S | Constructed unconditionally in `::new()`. |
| crates/warp_core/src/operating_system_info.rs:164 | `UnsupportedPlatform` variant | L | Never constructed; reserved/never-triggered discriminant. |
| crates/warp_core/src/ui/theme/color.rs:493 | `accent_pressed()` | S | Called from `ui_components/buttons.rs:68` via re-export. |
| crates/warp_terminal/src/model/escape_sequences.rs:20 | `mod C0` | S | Dozens of external callers. |
| crates/warp_terminal/src/model/grid/dimensions.rs:7 | `Dimensions::visible_rows()` | S | Heavy production use across grid/terminal files. |
| crates/warp_terminal/src/model/grid/cell.rs:336 | `Cell::flags()` | S | Non-test callers in several grid files. |
| crates/warp_terminal/src/model/grid/cell.rs:343 | `Cell::flags_mut()` | S | Non-test callers in several grid files. |
| crates/warp_terminal/src/model/ansi/control_sequence_parameters.rs:470 | `StandardCharset::map()` | S | Called from `ansi_handler.rs:494,1590`. |
| crates/warpui/src/platform/mod.rs:66 | `trait AsInnerMut<Inner>` | S | Implemented for `AppBuilder`, used in platform mod files. |
| crates/warpui/src/platform/mac/rendering/renderer.rs:19 | `Device::Metal` variant | S | Constructed and matched at several sites. |
| crates/warpui/src/platform/mac/rendering/metal/frame_capture.rs:80 | `create_capture_texture()` | L | Doc comment: "kept for future headless capture or visual regression testing support." |
| crates/warpui/src/windowing/winit/app.rs:64 | `CustomEvent::Clipboard` variant | L | Constructed only in wasm's paste listener — platform-conditional. |
| crates/warpui/src/windowing/winit/app.rs:113 | `ClipboardEvent::Paste` variant | L | Same wasm-only wiring as above. |
| crates/warpui/src/windowing/winit/fonts.rs:484 | `try_read_face_source()` | S | Called at `fonts.rs:272`. |
| crates/warpui/src/windowing/winit/fonts/font_handle.rs:7 | module cfg_attr blanket | L | `validate_font_data()` confirmed Linux/freebsd-only. |
| crates/warpui/src/windowing/winit/fonts/font_handle.rs:97 | `FontHandle::data()` | H | Zero call sites anywhere. Not independently filed (single accessor); refs #207. |
| crates/warpui/src/windowing/winit/linux/window_manager.rs:63 | `get_wayland_compositor_from_socket()` | L | Only call site is commented out, tracked by `TODO(CORE-3034)`. |
| crates/warpui_core/src/lib.rs:1 | crate-wide blanket | H | See "the two crate-wide blankets" above. |
| crates/warpui_core/src/image_cache.rs:835 | `ImageCache::evict_size()` | H | Only caller is its own test; tracked by an existing `TODO(APP-3877)`, not re-filed. |
| crates/warpui_core/src/elements/formatted_text_element.rs:330 | `with_alignment()` | S | ~15 production call sites. |
| crates/warpui_core/src/elements/formatted_text_element.rs:1339 | `LaidOutTextFrame::contains` | H | Zero callers anywhere, including tests; a sibling hit-test path is used instead. Not independently filed (single accessor); refs #207. |
| crates/warpui_core/src/elements/new_scrollable/mod.rs:1 | module blanket | S | Removed — see above. |
| crates/warpui_core/src/ui_components/radio_buttons.rs:86 | `RadioButtonState::new()` | H | Radio buttons ARE used elsewhere now, but always via `RadioButtonStateHandle::default()`/struct literal — this constructor specifically still has zero callers. Not independently filed; refs #207. |
| crates/warpui_core/src/integration/capture_recorder.rs:31,37,61 | `TimestampedFrame`/`SharedState`/`CaptureLoopState` | H | Entire module orphaned, superseded by `video_recorder.rs`. **#550.** |
| crates/warpui_core/src/async/mod.rs:82 | `SpawnedLocalStream::future` field | L | Read only via `#[cfg(test)] into_future()`; production call sites drop the handle. |
| crates/warpui_core/src/platform/app.rs:114,308,335 | mac-only `AppCallbackDispatcher` impls | L | `cfg_attr`-gated with `TODO(CORE-2322)`/`TODO(CORE-2691)`/`TODO(CORE-2323)` tracking native support elsewhere. |
| crates/warpui_core/src/core/mod_test.rs:2269 | `struct Action(String)` | L | Test-only `Debug`-output helper. |
| crates/warpui_core/src/platform/menu.rs:17,36 | `Menu`/`MenuBar` fields | L | Read only from `crates/warpui/src/platform/mac/menus.rs` (mac-only). |
| crates/warpui_core/src/platform/test/app.rs:8,17 | `test::App::new/run` | L | Unreachable fallback stub for unsupported OSes; `unimplemented!()` body. |
| crates/warpui_core/src/windowing/mod.rs:109 | `dispatch_standard_action()` | L | `cfg_attr`-gated, mac-only caller, `TODO(CORE-2691)`. |
| crates/warpui_core/src/core/app.rs:686 | `spawned_futures` field | L | `#[cfg(feature = "test-util")]`-only usage. |
| crates/warp_features/src/lib.rs:839 | `FeatureFlag::set_enabled()` | S | Extensive production callers across `app/src/server/experiments`, `lib.rs`, `warp_core/features.rs`. |

## Not independently tabulated

10 comment-only mentions of `allow(dead_code)` (in prose, not attributes) and
the 232 `cfg_attr`-conditional sites, per the "how this was produced" section
above.
