# App-AI sweep adjudication — `app/src/ai/**`
Oracle: pin `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. Source
list: `docs/SWEEP-INVENTORY.md`'s `app/src/ai/*` sections (98 pin test files, 917
absent tests total — the file-level counts there are trustworthy; the *bucket*
on each test, where it ends in `?`, is a mechanical guess this document
replaces with a hand-traced verdict).

Scope: `app/src/ai/**` only (not `crates/ai/**`, not `app/src/remote_server/**`
— those are other agents' areas).

## Method

For every file, I read the pin source's `use` imports at `02b53fcd8` and, for
files the fork ships, the fork's current implementation, then decided per
test (or per coherent group of tests within a file) using seven buckets:

- **PORTABLE** — ported in this pass (see the `PORTED` bucket below for what
  landed; nothing new was left in a bare "PORTABLE, not yet ported" state —
  everything I confirmed portable, I ported).
- **CLOUD** — needs `warp_graphql`, `server_api`/`ServerApiProvider`,
  `crate::server::`, or Warp-account credentials/`cloud_object`.
- **COVERED-ELSEWHERE** — the fork already tests this behaviour under a
  different name; the covering fork test is cited.
- **DECLINED** — an existing `DECLINED.md` row covers it; the row is quoted.
- **DIVERGENT** — the fork's API differs enough that the pin test has no
  equivalent to run against — including three real *code defects* this sweep
  found (see the dedicated section below), which are DIVERGENT rather than
  DECLINED because nobody decided the fork should behave this way.
- **MISSING-SUBSYSTEM** — real, non-cloud parity debt: a module, field, or
  helper the fix would need does not exist in the fork at all. This is the
  most valuable bucket; every entry says what's missing.
- **PORTABLE-OUT-OF-AREA** — genuinely portable test debt, but the fork's
  implementation of the feature lives outside `app/src/ai/**` (one file,
  `app/src/notifications/model.rs`) — outside my write boundary; flagged for
  whoever owns that area.

**Corroboration**: two background research agents independently re-verified
imports for the 56 largest `CLOUD?`-bucketed files (using the same
`git show 02b53fcd8:<path>` method, told nothing about each other's
findings); their reports are folded in below and materially changed roughly
a third of those files' verdicts — see "Where the inventory was wrong."

## Bucket totals — all 917 absent tests adjudicated
| bucket | count | meaning |
|---|---:|---|
| PORTED | 15 | ported to the fork in this pass (3 new + 12 already on `main`, verified) |
| CLOUD | 552 | needs Warp's dropped cloud backend |
| DECLINED | 160 | covered by an existing `DECLINED.md` decision |
| MISSING-SUBSYSTEM | 155 | real non-cloud parity debt; module/field/helper genuinely absent |
| DIVERGENT | 14 | fork's API differs; includes 3 real code defects found this pass |
| COVERED-ELSEWHERE | 6 | fork already tests the same behaviour under another name |
| PORTABLE-OUT-OF-AREA | 15 | portable, but the fork's code lives outside app/src/ai/** |
| **total** | **917** | |

---

## Code defects found — the highest-value output of this sweep

Three real defects, all "the fork has everything the fix needs but never
wired it" — porting the test would have compiled and then failed, which is
exactly the trap `docs/SWEEP-INVENTORY.md`'s caveats warn about. None fixed in
this pass (each needs either a constructor-signature change touching several
existing passing tests, or new UI state, with no compiler available to verify
against) — reported here per AGENTS.md §5.11 for follow-up with build access.

1. **`ReadSkillExecutor` ignores the active session's host, so a remote (SSH)
   session reads the wrong bundled-skill catalog.**
   `app/src/ai/blocklist/action_model/execute/read_skill.rs`. The pin's
   executor holds a `ModelHandle<ActiveSession>` and calls
   `SkillManager::active_skill_by_reference_with_origin` with the session's
   resolved `skill_path_origin()`. The fork's `ReadSkillExecutor::new()` takes
   no session parameter and calls the origin-agnostic
   `active_skill_by_reference` unconditionally. The fork already has
   `active_skill_by_reference_with_origin` (`skill_manager.rs:435`),
   `SessionContext::skill_path_origin` (`blocklist/controller.rs:158`), and
   `ActiveSession` — every piece the fix needs already exists, it's just not
   connected. User impact: reading a skill by bundled ID from an SSH session
   can silently return the client's local catalog entry instead of the
   remote host's. See `read_skill_tests.rs`'s DIVERGENT entry below for the
   full trace.
2. **`ConversationUsageView::handle_action` is a literal no-op — "View
   details"/"Show N more" do nothing, and no test catches it.**
   `app/src/ai/blocklist/usage/conversation_usage_view.rs:502`:
   `fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {}`.
   The struct has no `details_expanded`/`show_all_clicked` fields at all. The
   pin test file's own doc comment describes an *earlier*, already-fixed bug
   in this fork (the view is correctly created via `add_typed_action_view`
   now) — but the handler it registers discards every action, so the affordance
   is still fully inert. This is real dead UI, invisible to any existing
   test. See `conversation_usage_view_tests.rs`'s DIVERGENT entry.
3. **`classify_gui_list_entry` can never return `Unavailable`, even though the
   enum variant it needs already exists.**
   `app/src/ai/blocklist/agent_view/conversation_selection.rs:99`. The pin's
   4-state classifier (`Selected`/`OpenElsewhere`/`Available`/`Unavailable`)
   takes an extra availability-predicate closure the fork's 3-state,
   4-parameter version dropped. `AgentConversationListEntryState::Unavailable`
   is defined at `app/src/ai/conversation_entry.rs:93` and simply never
   constructed. Single call site, low blast radius — but I could not
   determine what should make a GUI entry "unavailable" without deeper
   tracing than time allowed, and declined to guess at the business logic for
   an unverifiable fix. See `conversation_selection_tests.rs`'s DIVERGENT
   entry.

## MISSING-SUBSYSTEM highlights — real, non-cloud parity debt

155 tests across roughly 30 files. The largest and most actionable:

- **CORRECTED 2026-08-11 (maintainer): orchestration IS built in this fork.**
  The wording below describes the pin's *path* and reads as a claim about the
  subsystem — that is wrong. The fork ships `blocklist/orchestration_topology.rs`
  (26 tests), `blocklist/orchestration_events.rs` (10), the four
  `agent_view/orchestration_*` modules, `block/view_impl/orchestration.rs`, and
  `warp_tui/src/orchestration_{model,tab_bar}.rs`. What is genuinely absent is
  only the pin's **config-picker layer** (`config_state`, `edit_state`,
  `providers`, `remote_child`, `snapshots`, `validation`) — the UI for *choosing*
  harness / model / environment / host, not orchestration itself. Read the
  original text below with that correction applied.

- **The pin's `app/src/ai/orchestration/` config-picker layer** (39 tests across
  `snapshots_tests.rs`, `edit_state_tests.rs`, `config_state_tests.rs`,
  `validation_tests.rs`, plus 13 of `snapshots_tests.rs`'s own tests are pure
  local data with no cloud symbol). This is the shared harness/model/
  environment/host "option snapshot" + edit-state layer that would back an
  interactive picker for `/orchestrate`'s local children. DECLINED.md's
  2026-08-08 reversal put local orchestration back in scope, and the fork
  already has the substrate (`orchestration_topology.rs`, the pill bar), but
  this config-picker layer specifically was never built — `/orchestrate`
  currently spawns local children through a simpler path with no equivalent
  interactive picker under `app/src/ai/**`.
- **`app/src/ai/blocklist/usage/rollup.rs` is unusually close to portable.**
  Its own doc comment: "Pure function — no I/O, no GraphQL." Its sole real
  dependency, `descendant_conversation_ids_in_spawn_order`, **already exists**
  in the fork at `orchestration_topology.rs:164` with its own tests. This
  8-test file (per-agent credit-breakdown rollup for the footer "View
  details" list) is the single highest-value MISSING-SUBSYSTEM item found —
  not ported this pass only because the exact `credits_spent` bookkeeping on
  `AIConversation` and the consuming footer UI weren't independently
  verified in the time available.
- **`app/src/ai/blocklist/history_model_tests.rs`: 19 of its 27 CLOUD?-bucketed
  tests are actually local-orchestration debt, not cloud.** All of them call
  fork methods that already exist (`assign_run_id_for_conversation`,
  `mark_conversation_as_remote_child`, `fork_conversation`,
  `set_server_conversation_token_for_conversation`) — "cloud"/"server" in the
  names is legacy narrative naming the pin never updated, not a network
  dependency. Two need one missing test helper
  (`upgrade_optimistic_root_to_server_task_for_test`).
- **`app/src/ai/skills/file_watchers/skill_watcher.rs` lacks the whole
  remote-project-skill refresh/fallback layer** (13 tests) —
  `parse_project_skill_contents`, `refresh_project_skills_for_repo`,
  `local_project_fallback_*` don't exist anywhere in the fork; it only does
  direct local-filesystem repo scanning.
- Several single-test MISSING-SUBSYSTEM findings are pure, small, and would
  be one-file, low-risk ports for an agent with build access:
  `blocklist/handoff/touched_repos_tests.rs::find_git_root_walks_up_to_dot_git`,
  `get_relevant_files/remote_search/native_tests.rs::file_contents_from_response_keeps_only_whole_text_files`,
  `agent_sdk/driver/harness/claude_code/wake_driver_tests.rs::local_wake_task_state_ready_allows_success_and_stale_in_progress`.

## Where `SCOPE-AI.md` / the inventory turned out to be wrong

- **The mechanical `CLOUD?` bucket itself was wrong for roughly a third of
  the 56 largest files it covers**, in both directions. Undercounted (mixed
  files where more tests were cloud than the split implied, e.g.
  `blocklist/block_tests.rs`'s `local_arm_*`/`remote_arm_*`/`compose_child_prompt_*`,
  15 of 20, all built on the cloud-typed `RunAgentsExecutionMode` even on
  their "local" branch). Overcounted more often: `agent_events/driver_tests.rs`,
  `agent_sdk/driver/harness/mod_tests.rs`, `blocklist/controller_tests.rs`,
  `execution_profiles/config_tests.rs`, `agent_sdk/common_tests.rs`,
  `blocklist/handoff/touched_repos_tests.rs`, and several others were pure
  local logic mis-flagged cloud because the *file* also contains cloud
  imports the specific test never touches.
- **`llms_tests.rs`'s 20 mechanically-CLOUD? tests are the same declined
  surface as its 3 already-DECLINED? tests** (DECLINED.md #142/#347,
  `CustomEndpoint`) — one row, not two different buckets.
- **`active_agent_views_model_tests.rs`'s DECLINED.md entry does not cover
  what its own 10 pin tests test.** The entry (rightly) says a *conversation-
  transfer* substitute exists (`BlocklistAIHistoryModel::terminal_view_id_for_conversation`);
  these 10 tests are a *different* mechanism — per-`WindowId` last-focused-
  view tracking — that the substitute does not provide. Corrects the entry's
  practical scope without disputing its (narrower) original claim.
- **`agent_management_model_tests.rs` (15 tests) was bucketed DIVERGENT? for
  "fork does not ship the pin's source module,"** but the fork DOES ship an
  equivalent — relocated to `app/src/notifications/model.rs`, outside
  `app/src/ai/**`, which is why the mechanical pass (scoped to filenames
  under the pin's path) couldn't see it. That file has zero test coverage.
- **Three `PORTABLE?`-bucketed clusters were the opposite of portable on
  inspection**: `agent_sdk/config_file_tests.rs` + `mcp_config_tests.rs` (5
  tests, need server-side well-known-MCP-id resolution), `agent/api/convert_conversation_tests.rs`
  (3 tests, need a `convert_conversation_data_to_ai_conversation` function
  that restores a *cloud*-fetched conversation), and every
  `format_upload_artifact_text_*`/`converts_upload_artifact_tool_call_to_action`-
  shaped test across the sweep (upload-artifact needs a cloud upload target,
  full stop — there's no local place to upload an artifact *to*).
- **`skill_watcher_tests.rs`'s 13 `PORTABLE?` tests were the file's worst
  overstatement**: none needed a symbol that exists in the fork; the whole
  remote-project-skill-refresh layer is absent.

## Per-file adjudication

Ordered by absent-test count, matching `docs/SWEEP-INVENTORY.md`. Within a file, tests are grouped by final bucket; the evidence line covers every test listed under it unless a test has its own note.

### `app/src/ai/blocklist/orchestration_event_streamer_tests.rs` — 55 absent

pin 55 · fork 0 · source `app/src/ai/blocklist/orchestration_event_streamer.rs` · fork ships source: NO

- **CLOUD** — orchestration_event_streamer.rs imports `crate::server::server_api::{ServerApi, ServerApiProvider}` and `crate::server::server_api::ai::{AIClient, AgentRunEvent, TaskListFilter}` -- SSE streaming of remote-child run events from Warp's server (`get_ambient_agent_task`, `?ancestor_run_id=` REST seed). This is the cloud-runner half explicitly carved out as still-declined in DECLINED.md's orchestration-reversal row (#290).
  - `ai_conversation_new_restored_preserves_last_event_sequence`
  - `build_pending_events_preserves_message_payload`
  - `child_only_conversation_opens_self_run_id_filter`
  - `convert_lifecycle_events_filters_self_run_blocked`
  - `convert_lifecycle_events_includes_run_blocked`
  - `convert_lifecycle_events_maps_run_restarted`
  - `dormant_claude_wake_consumer_stops_on_first_target_event`
  - `dormant_local_claude_child_skips_generic_sse_but_allows_wake_listener`
  - `dormant_local_claude_child_uses_task_harness_when_server_metadata_missing`
  - `finish_ancestor_seed_fetch_emits_child_spawned_for_each_seeded_child`
  - `finish_restore_fetch_err_does_not_resurrect_deleted_conversation`
  - `finish_restore_fetch_no_ops_when_conversation_deleted_mid_flight`
  - `finish_restore_fetch_reconnects_sse_when_children_added_to_open_connection`
  - `finish_restore_fetch_uses_server_cursor_when_sqlite_is_absent`
  - `handle_event_batch_drops_events_for_killed_run_ids_after_persisting_cursor`
  - `handle_event_batch_persists_max_seq_to_history_model`
  - `is_known_child_dedupes_per_parent_after_first_observation`
  - `is_known_child_isolated_per_parent`
  - `is_remote_run_view_excludes_remote_child`
  - `is_remote_run_view_excludes_shared_session_viewer`
  - `killed_run_ids_are_bounded`
  - `lifecycle_event_type_blocked_maps_to_blocked_with_empty_action`
  - `lifecycle_event_type_cancelled_maps_to_cancelled`
  - `lifecycle_event_type_errored_maps_to_error`
  - `lifecycle_event_type_failed_maps_to_error`
  - `lifecycle_event_type_in_progress_maps_to_in_progress`
  - `lifecycle_event_type_legacy_idle_maps_to_success`
  - `lifecycle_event_type_legacy_restarted_maps_to_in_progress`
  - `lifecycle_event_type_legacy_started_maps_to_in_progress`
  - `lifecycle_event_type_succeeded_maps_to_success`
  - `on_conversation_removed_prunes_killed_child_run_id_from_parent_but_keeps_tombstone`
  - `on_conversation_removed_prunes_stale_child_run_id_from_parent`
  - `parent_with_many_children_opens_one_ancestor_include_self_stream`
  - `persist_event_cursor_keeps_the_max_sequence_and_updates_history_model`
  - `reevaluate_eligibility_does_not_reconnect_when_watched_run_ids_unchanged`
  - `register_parent_on_wait_already_parent_is_idempotent`
  - `register_parent_on_wait_child_short_circuits`
  - `register_parent_on_wait_flag_off_is_noop`
  - `register_parent_on_wait_without_self_run_id_is_noop`
  - `register_viewer_mode_consumer_replays_known_children_for_later_panes`
  - `registering_additional_child_does_not_reconnect_parent_family_stream`
  - `restored_child_without_children_opens_self_run_id_stream`
  - `restored_conversations_initialize_v2_streaming_state`
  - `restored_parent_with_children_opens_ancestor_include_self_stream`
  - `sse_backoff_escalates_then_caps`
  - `sse_backoff_zero_failures_uses_first_step`
  - `sse_forwarding_consumer_skips_message_hydration_when_disabled`
  - `threshold_exceeded_at_and_above_limit`
  - `threshold_not_exceeded_below_limit`
  - `unknown_lifecycle_maps_to_error`
  - `viewer_mode_consumer_refcount_handles_multiple_panes_and_double_unregister`
  - `wait_registration_fetch_error_does_not_register`
  - `wait_registration_root_with_children_opens_ancestor_include_self_stream`
  - `wait_registration_root_without_children_does_not_register`
  - `wake_ready_does_not_advance_cursor_before_wake_preparation`

### `app/src/ai/agent_conversations_model_tests.rs` — 41 absent

pin 59 · fork 18 · source `app/src/ai/agent_conversations_model.rs` · fork ships source: yes

- **CLOUD** — Fork's own get_entries() doc comment: "there are no ambient cloud runs, so this emits one entry per local conversation"; needs the fork's cloud-stripped tasks:HashMap<AmbientAgentTaskId,AmbientAgentTask> / AgentSource::{WebApp,GitHubAction,ScheduledAgent} / ambient_agents::spawn::spawn_task, which yields Err("Agent spawning is disabled in Zap").
  - `cloud_conversation_metadata_reports_failed_load`
  - `local_conversation_sync_finishes_initial_load_without_starting_cloud_load`
  - `rtc_task_refresh_pending_timestamp_keeps_earliest_timestamp`
  - `rtc_task_refresh_pending_timestamp_records_first_timestamp`
  - `rtc_task_refresh_pending_timestamp_replaces_later_timestamp`
  - `test_conversation_metadata_child_predicate_matches_conversation`
  - `test_display_status_uses_active_execution_over_previous_conversation_status`
  - `test_eviction_caps_each_group_independently`
  - `test_eviction_noop_when_under_cap`
  - `test_eviction_protects_personal_from_team_overflow`
  - `test_eviction_removes_oldest_within_group`
  - `test_get_entries_excludes_child_agent_task`
  - `test_get_entries_excludes_conversation_shadowed_by_child_task`
  - `test_get_entries_includes_cloud_metadata_only_entry`
  - `test_get_entries_includes_task_only_entry`
  - `test_get_entries_keeps_unrelated_task_and_conversation_entries`
  - `test_get_entries_keeps_unrelated_tasks_and_conversations`
  - `test_get_entries_merges_task_and_local_conversation_by_run_id`
  - `test_get_entries_merges_task_and_local_conversation_by_server_token`
  - `test_get_entries_prefers_task_when_server_token_matches`
  - `test_get_entries_prefers_task_when_task_id_matches_conversation_run_id`
  - `test_get_or_async_fetch_task_data_returns_cached_task_without_fetching`
  - `test_get_or_async_fetch_task_data_skips_when_in_flight`
  - `test_get_or_async_fetch_task_data_skips_when_permanently_failed`
  - `test_get_or_async_fetch_task_data_skips_within_transient_cooldown`
  - `test_has_items_ignores_child_agent_tasks`
  - `test_resolve_copy_link_prefers_active_session_link`
  - `test_resolve_copy_link_uses_attached_synced_conversation_for_task_without_token`
  - `test_resolve_copy_link_uses_cloud_conversation_link_for_inactive_task`
  - `test_resolve_open_action_handles_server_token_subject_without_entry`
  - `test_resolve_open_action_opens_active_ambient_session`
  - `test_resolve_open_action_opens_active_ambient_session_from_link`
  - `test_resolve_open_action_opens_completed_cloud_task_by_server_token`
  - `test_resolve_open_action_opens_metadata_only_cloud_conversation_by_server_token`
  - `test_resolve_open_action_prefers_active_ambient_terminal`
  - `test_resolve_open_action_reopens_ambient_session_after_terminal_unregister`
  - `test_resolve_open_action_returns_none_for_active_unattachable_session`
  - `test_server_token_assignment_updates_copy_link_resolution`
  - `test_task_fetch_error_extracts_access_denied_http_status`
- **DIVERGENT** — Not cloud, but resolve_copy_link's local-only branch does not exist yet in the fork -- a real local gap, not cloud debt.
  - `test_resolve_copy_link_returns_none_for_local_only_unsynced_conversation`
- **COVERED-ELSEWHERE** — Near-duplicate of the fork's existing test_get_entries_includes_local_conversation.
  - `test_get_entries_includes_local_only_entry`

### `app/src/ai/blocklist/local_agent_task_sync_model_tests.rs` — 36 absent

pin 36 · fork 0 · source `app/src/ai/blocklist/local_agent_task_sync_model.rs` · fork ships source: NO

- **CLOUD** — use warp_graphql::ai::{AgentTaskState, PlatformErrorCode}; use crate::server::server_api::ServerApiProvider. Syncs local state to a server ai_tasks row.
  - `agent_exited_shell_is_failed_with_invalid_request`
  - `aws_bedrock_credentials_is_failed_with_auth_required`
  - `cli_blocked_maps_correctly`
  - `cli_blocked_without_message`
  - `cli_in_progress_maps_correctly`
  - `cli_success_maps_correctly`
  - `cli_task_mapping_survives_cli_session_end`
  - `context_window_exceeded_is_failed`
  - `conversation_server_token_assigned_fires_update_with_conversation_id`
  - `conversation_server_token_assigned_skips_remote_child_conversations`
  - `conversation_server_token_assigned_skips_viewer_conversations`
  - `conversation_server_token_assigned_skips_without_task_id`
  - `gemini_enterprise_credentials_is_failed_with_auth_required`
  - `internal_warp_error_is_error`
  - `invalid_api_key_is_failed_with_auth_required`
  - `map_conversation_status_error_classifies_agent_exited_shell`
  - `map_conversation_status_error_classifies_exchange_error`
  - `map_conversation_status_error_classifies_status_error`
  - `map_conversation_status_error_classifies_status_error_other_as_error`
  - `map_conversation_status_error_classifies_status_error_via_setter`
  - `map_conversation_status_error_ignores_will_attempt_resume`
  - `map_conversation_status_error_without_exchange_error_is_generic`
  - `map_conversation_status_in_progress_reports_in_progress_with_no_message`
  - `map_conversation_status_waiting_for_events_reports_in_progress_with_no_message`
  - `other_error_is_error_with_internal`
  - `other_user_error_is_failed_with_invalid_request`
  - `quota_limit_is_failed_with_insufficient_credits`
  - `server_overloaded_is_error_with_resource_unavailable`
  - `shared_session_link_fires_update_agent_task_with_session_id`
  - `shared_session_link_skips_remote_child_conversations`
  - `shared_session_link_skips_unknown_conversation`
  - `shared_session_link_skips_viewer_conversations`
  - `shared_session_link_skips_when_task_id_missing`
  - `shared_session_link_uses_correct_argument_order`
  - `transient_error_status_maps_to_in_progress_with_no_message`
  - `transient_network_error_is_error_with_internal_and_debug_details`

### `app/src/ai/agent_sdk/driver/snapshot_tests.rs` — 35 absent

pin 35 · fork 0 · source `app/src/ai/agent_sdk/driver/snapshot.rs` · fork ships source: NO

- **CLOUD** — Module entirely absent from the fork. Source imports crate::server::server_api::ai::{...}, crate::server::server_api::harness_support::{...}; e2e_dirty_repo_uploads_patch_and_manifest_reports_success uses an HTTP Matcher for the upload. The parse_declarations_*/resolve_declarations_path_* sub-tests are pure parsing but exist only to feed the cloud upload pipeline, with no independent local caller.
  - `build_repo_patch_preserves_non_utf8_untracked_paths`
  - `declarations_writer_appends_unique_absolute_paths_once`
  - `declarations_writer_continues_after_per_path_write_failures`
  - `declarations_writer_preempts_paths_inside_existing_repo`
  - `declarations_writer_resolves_relative_paths_against_working_dir`
  - `drop_files_covered_by_repos_drops_file_inside_repo_keeps_file_outside`
  - `drop_files_covered_by_repos_handles_nested_repo_paths`
  - `drop_files_covered_by_repos_keeps_everything_when_no_repos_declared`
  - `e2e_clean_repo_uploads_only_manifest`
  - `e2e_dirty_repo_uploads_patch_and_manifest_reports_success`
  - `e2e_file_is_uploaded_with_correct_body`
  - `e2e_gather_failed_entry_captured_in_manifest`
  - `e2e_get_snapshot_upload_targets_failure_returns_none`
  - `e2e_manifest_reflects_mixed_outcomes`
  - `e2e_manifest_upload_failure_produces_partial_outcome`
  - `e2e_multi_repo_mixed_statuses_roundtrip_to_manifest`
  - `e2e_per_run_cap_drops_excess_blobs_as_skipped`
  - `e2e_permanent_4xx_fails_fast_without_retries`
  - `e2e_read_failed_for_missing_file_continues_pipeline`
  - `e2e_repo_plus_inside_and_outside_files_filters_overlap`
  - `e2e_retry_exhaustion_marks_entry_failed_and_records_in_manifest`
  - `e2e_short_response_leaves_trailing_file_without_target`
  - `e2e_transient_5xx_is_retried_then_succeeds`
  - `parse_declarations_deduplicates_kind_path_pairs`
  - `parse_declarations_ignores_blank_lines`
  - `parse_declarations_skips_lines_with_empty_path`
  - `parse_declarations_skips_malformed_lines_without_aborting`
  - `parse_declarations_skips_missing_or_unsupported_versions`
  - `parse_declarations_tolerates_crlf_line_endings`
  - `resolve_declarations_path_defaults_without_override_or_task_id`
  - `resolve_declarations_path_respects_override`
  - `resolve_declarations_path_uses_task_id_when_provided`
  - `upload_skipped_when_declarations_file_empty`
  - `upload_skipped_when_declarations_file_has_no_valid_jsonl_entries`
  - `upload_skipped_when_declarations_file_missing`

### `app/src/ai/blocklist/inline_action/run_agents_card_view_tests.rs` — 32 absent

pin 32 · fork 0 · source `app/src/ai/blocklist/inline_action/run_agents_card_view.rs` · fork ships source: NO

- **DECLINED** — Confirmation-card view for the model-invoked `RunAgents` action. Imports `warp_graphql::queries::get_runners::RunnerSortBy` directly. DECLINED.md #325 ("Agent-invoked agent spawning"): `AIAgentActionType` has no RunAgents variant in the fork, so this card's `AIAgentActionId`-keyed `ChildView` never has a caller. DECLINED.md's own caveat ("a few cases exercise a Local execution-mode variant... tracked under #11") was checked: the `local_*`/`overrides_local_to_remote` tests still operate on `RunAgentsAgentRunConfig`, a cloud-typed struct from `ai::agent::action` with no fork equivalent -- there is no separable local-only slice to port.
  - `all_failed_uses_failure_status_not_mixed`
  - `approved_local_disabled_harness_reports_disabled_reason_after_override`
  - `cancelled_uses_cancelled_status`
  - `cloud_to_local_drops_environment`
  - `cloud_with_env_and_non_opencode_harness_allows_accept`
  - `cloud_with_opencode_disables_accept`
  - `cloud_without_env_no_longer_disables_accept`
  - `denied_with_reason_appends_reason`
  - `denied_without_reason_uses_short_label`
  - `does_not_carry_computer_use_from_local_to_remote`
  - `failure_with_empty_error_uses_short_label`
  - `failure_with_error_includes_error_text`
  - `from_request_sanitizes_disabled_local_harness_to_oz`
  - `launched_partial_uses_x_of_y_label_and_mixed_status`
  - `launched_plural_uses_plural_label`
  - `launched_singular_uses_singular_label`
  - `local_to_cloud_idempotent_when_already_remote`
  - `local_to_cloud_initializes_remote_with_empty_environment`
  - `local_to_cloud_resets_opencode_to_oz`
  - `local_with_any_harness_does_not_disable_accept`
  - `local_with_disabled_codex_disables_accept`
  - `overrides_even_when_request_has_values`
  - `overrides_local_to_remote`
  - `overrides_model_and_harness_unconditionally`
  - `overrides_remote_to_local`
  - `preserves_computer_use_when_both_remote`
  - `set_environment_id_no_op_in_local_mode`
  - `set_environment_id_updates_remote`
  - `set_runner_id_no_op_in_local_mode`
  - `set_runner_id_updates_remote_and_round_trips`
  - `single_failed_uses_singular_failure_label`
  - `to_request_round_trips_request_fields`

### `app/src/ai/request_usage_model_tests.rs` — 30 absent

pin 30 · fork 0 · source `app/src/ai/request_usage_model.rs` · fork ships source: yes

- **CLOUD** — ORACLE.md itself documents this exact file: "crates/ai/src/api_keys_tests.rs yields zero straight debt... request_usage_model_tests.rs is out of scope despite the fork shipping the source -- that source is a 260-line no-op stub." Confirmed: app/src/ai/request_usage_model.rs is a stub; every test needs Firebase-anonymous-user checks, workspace overage/auto-reload policy, or bonus-credit balances -- all cloud billing state this fork has none of. Not a DECLINED.md row (no product decision needed) -- it's a stub-coverage exclusion, correctly excluded by script/check_stub_coverage's logic.
  - `refresh_request_usage_returns_no_fresh_limit_when_logged_out`
  - `test_ambient_credits_banner_dismissal_is_persisted`
  - `test_ambient_credits_banner_dismissal_loads_from_preferences`
  - `test_buy_credits_banner_hidden_with_non_ambient_bonus_credits`
  - `test_buy_credits_banner_shows_when_non_ambient_bonus_credits_are_depleted`
  - `test_buy_credits_banner_shows_with_only_ambient_bonus_credits`
  - `test_byo_api_key_disabled_for_anonymous_firebase_user`
  - `test_has_any_ai_remaining_false_both_payg_and_autoreload_disabled`
  - `test_has_any_ai_remaining_false_when_no_requests_or_bonus`
  - `test_has_any_ai_remaining_false_with_add_on_credits_policy_when_purchase_would_exceed_limit`
  - `test_has_any_ai_remaining_false_with_byok_enabled_but_no_key`
  - `test_has_any_ai_remaining_false_with_enterprise_auto_reload_policy_on_non_enterprise`
  - `test_has_any_ai_remaining_false_with_grok_subscription_but_byo_disabled`
  - `test_has_any_ai_remaining_false_with_only_ambient_bonus_credits`
  - `test_has_any_ai_remaining_false_with_workspace_no_pricing_no_overages_no_credits`
  - `test_has_any_ai_remaining_true_with_byo_key_and_no_workspace`
  - `test_has_any_ai_remaining_true_with_byok_enabled_and_key_provided`
  - `test_has_any_ai_remaining_true_with_enterprise_auto_reload`
  - `test_has_any_ai_remaining_true_with_grok_subscription_connected`
  - `test_has_any_ai_remaining_true_with_payg_enabled`
  - `test_has_any_ai_remaining_true_with_remaining_requests`
  - `test_has_any_ai_remaining_true_with_self_serve_auto_reload`
  - `test_has_any_ai_remaining_true_with_self_serve_auto_reload_and_billing_v2_disabled`
  - `test_has_any_ai_remaining_true_with_user_bonus_credits`
  - `test_has_any_ai_remaining_true_with_workspace_bonus_credits`
  - `test_has_any_ai_remaining_true_with_workspace_overages`
  - `test_request_limit_info`
  - `test_request_limit_info_is_unlimited_true`
  - `test_request_limit_info_past_refresh_time`
  - `test_request_limit_info_with_limit`

### `app/src/ai/blocklist/history_model_tests.rs` — 27 absent

pin 71 · fork 44 · source `app/src/ai/blocklist/history_model.rs` · fork ships source: yes

- **CLOUD** — merge_cloud_conversation_metadata needs credits_spent/billing fields (cloud). hydrate_remote_child_placeholder_* constructs is_remote_child:true, a state the fork's AgentConversationData::is_remote_child doc says Phosphor only ever writes false.
  - `hydrate_remote_child_placeholder_with_cloud_transcript_preserves_placeholder_identity`
  - `test_find_by_token_after_merge_cloud_metadata`
  - `test_merge_cloud_conversation_metadata`
  - `test_merge_cloud_metadata_refreshes_stale_restored_conversation_metadata`
  - `test_merge_cloud_metadata_removes_stale_duplicate_metadata_ids_for_token`
  - `test_merge_cloud_metadata_reuses_restored_conversation_id_for_token`
  - `test_merge_cloud_metadata_updates_already_restored_conversations`
  - `test_reserved_canonical_conversation_id_reused_by_later_metadata_merge`
- **MISSING-SUBSYSTEM** — Mis-bucketed by the mechanical pass. All call EXISTING local fork methods (assign_run_id_for_conversation at history_model.rs:1344; mark_conversation_as_remote_child at line 644, kept explicitly 'so the shape matches the pin'; fork_conversation; set_server_conversation_token_for_conversation; find_conversation_id_by_server_token). 'cloud'/'server' in the names is legacy/narrative naming carried over from the pin, not a network dependency -- crates/persistence/src/model.rs:1289's AgentConversationData already has pinned/is_remote_child/orchestration_harness_type/root_task_is_optimistic fields (DECLINED's #376 persistence-fields row is stale on this point). Two (test_truncate_from_exchange_to_empty..., test_two_restart_cycles...) additionally need a missing test helper upgrade_optimistic_root_to_server_task_for_test -- a scaffolding gap, not cloud. This is now genuinely portable local-orchestration debt; not ported in this pass for lack of time to build/verify the missing test helper.
  - `prompt_history_candidates_seeds_from_snapshot_then_appends_session_prompts`
  - `start_new_child_conversation_persists_harness_metadata`
  - `test_assign_run_id_for_conversation_persists_updated_conversation_state`
  - `test_find_by_token_after_mark_conversations_historical_for_terminal_surface`
  - `test_fork_conversation_title_override_replaces_prefix`
  - `test_fork_then_bind_handoff_token_persists_to_restored_conversation`
  - `test_fork_then_bind_handoff_token_resolves_to_forked_conversation`
  - `test_fork_then_bind_handoff_token_updates_cached_metadata_and_emits_refresh_events`
  - `test_initialize_historical_conversations_eagerly_hydrates_orchestration_children`
  - `test_initialize_historical_conversations_resolves_parent_agent_id_children_via_seeded_run_ids`
  - `test_initialize_output_for_response_stream_persists_updated_conversation_state`
  - `test_mark_conversation_as_remote_child_persists_updated_conversation_state`
  - `test_optimistic_root_restore_round_trip_yields_in_progress_optimistic_root`
  - `test_optimistic_root_upgrade_then_persist_emits_event_with_single_server_task_row`
  - `test_persist_with_optimistic_root_emits_event_with_no_task_rows`
  - `test_restore_conversations_indexes_child_by_parent_agent_id`
  - `test_start_new_child_conversation_persists_child_metadata_for_restore`
  - `test_truncate_from_exchange_to_empty_persist_event_has_empty_updated_tasks`
  - `test_two_restart_cycles_keep_exactly_one_server_root_task_row`

### `app/src/ai/agent_sdk/agent_management_tests.rs` — 23 absent

pin 23 · fork 0 · source `app/src/ai/agent_sdk/agent_management.rs` · fork ships source: NO

- **CLOUD** — use crate::server::server_api::ServerApiProvider; CreateAgentRequest/UpdateAgentRequest from server_api::ai -- commands to manage named agents via Warp's public cloud API, not the oz agent local CLI.
  - `agent_response_defaults_prompt_to_none_when_absent`
  - `agent_response_deserializes_null_prompt_as_none`
  - `agent_response_deserializes_prompt`
  - `apply_secret_deltas_uses_secret_names`
  - `apply_string_deltas_removes_and_appends_without_duplicates`
  - `build_create_request_forwards_prompt`
  - `build_create_request_omits_prompt_when_unset`
  - `build_update_request_leaves_prompt_unchanged_when_neither_flag_set`
  - `build_update_request_remove_prompt_clears_via_empty_string`
  - `build_update_request_replaces_prompt`
  - `create_agent_request_omits_prompt_when_none_and_serializes_when_set`
  - `rejects_sort_for_json_output`
  - `request_is_empty_clears_prompt_still_counts_as_an_update`
  - `request_is_empty_treats_prompt_as_an_update`
  - `sort_agents_defaults_created_at_to_descending`
  - `sort_agents_defaults_to_name_ascending`
  - `sort_agents_respects_explicit_sort_order_without_sort_field`
  - `table_format_does_not_include_available_column`
  - `table_format_preserves_short_prompt_and_flattens_newlines`
  - `table_format_truncates_prompt_to_sixty_characters`
  - `update_request_omits_unset_fields_and_serializes_clears`
  - `update_request_serializes_prompt_clear_as_empty_string_and_omits_none`
  - `visible_agents_and_hidden_count_filters_disabled_agents`

### `app/src/ai/execution_profiles/profiles_tests.rs` — 23 absent

pin 25 · fork 2 · source `app/src/ai/execution_profiles/profiles.rs` · fork ships source: yes

- **CLOUD** — Pin imports crate::cloud_object, CloudAIExecutionProfileModel, crate::server::cloud_objects::update_manager::UpdateManager, AuthStateProvider -- a cloud-sync migration state machine. The fork ships its own cloud_object-based (but locally-scoped) profiles.rs/profiles_tests.rs; the remaining unlisted pin tests need the pin's specific cloud migration retry semantics.
  - `auth_completion_waits_for_cloud_initial_load_before_migrating`
  - `cloud_initial_load_retries_pending_migration`
  - `completed_migration_is_not_reapplied_and_legacy_ids_restore_after_restart`
  - `explicit_local_collection_is_preserved_from_onboarding`
  - `feature_disabled_keeps_legacy_backend_behavior`
  - `filters_non_owned_non_default_profile_from_list`
  - `gui_default_execute_commands_remains_always_ask`
  - `ignores_shared_default_profile_after_initial_load`
  - `ignores_shared_default_profile_created_from_cloud`
  - `malformed_cloud_collection_falls_back_to_legacy_import`
  - `malformed_cloud_collection_without_legacy_profiles_materializes_default`
  - `materialized_pending_profile_is_rekeyed_after_server_id_arrives`
  - `migration_imports_owned_legacy_profiles_with_deterministic_keys`
  - `migration_retries_after_auth_completes`
  - `migration_retries_after_pending_legacy_profile_receives_server_id`
  - `pending_migration_keeps_legacy_default_model_until_import_succeeds`
  - `profile_sources_preserve_state_across_migration_and_rollout`
  - `reset_without_explicit_collection_reimports_the_next_accounts_legacy_profile`
  - `settings_sync_disabled_imports_legacy_profiles`
  - `tui_default_denylist_overrides_agent_decides_command_execution`
  - `tui_explicit_collection_preserves_execute_commands`
  - `tui_missing_collection_seeds_agent_decides_for_execute_commands`
- **DECLINED** — DECLINED.md: Account-first onboarding, billing, paid tiers (#11): "account_class, is_paid, has_team, upgrade flows. No BYOP equivalent."
  - `pre_login_edit_materializes_the_pending_collection`

### `app/src/ai/llms_tests.rs` — 23 absent

pin 27 · fork 4 · source `app/src/ai/llms.rs` · fork ships source: yes

- **DECLINED** — Maps exactly onto DECLINED.md's #142/#347 CustomEndpoint row -- the mechanical pass's CLOUD? cluster (should_clear_preference_*, host_icon_visibility_requires_enabled_credentials_and_model_host, custom_llm_infos_*, removing_*, reconcile_preserves_custom_* -- 20 tests) plus the already-DECLINED? group (active_models_fall_back_to_usable_choice_or_custom_endpoint_when_default_disabled, custom_endpoint_usage_display_label_resolves_alias_name_and_generic_fallback, reconcile_preserves_custom_endpoint_models_not_configured_locally -- 3 tests) are the SAME declined surface. Fork's current llms.rs has zero server_api/warp_graphql imports -- already fully local via AgentProviderSecrets. Reclassified from the mechanical CLOUD? to DECLINED since it's not fresh debt, it's an existing, already-argued decision.
  - `active_models_fall_back_to_usable_choice_or_custom_endpoint_when_default_disabled`
  - `active_models_use_default_when_usable`
  - `custom_endpoint_usage_display_label_resolves_alias_name_and_generic_fallback`
  - `custom_llm_display_name_falls_back_to_name_when_alias_missing`
  - `custom_llm_display_name_uses_alias_when_present`
  - `custom_llm_infos_built_from_endpoints`
  - `custom_llm_infos_skip_endpoints_with_empty_api_key`
  - `custom_llm_infos_skip_models_without_config_key`
  - `explicit_child_model_pin_preserves_gui_behavior_and_only_emits_for_effective_changes`
  - `host_icon_visibility_requires_enabled_credentials_and_model_host`
  - `is_cloud_runnable_oz_model_id_classifies_ids`
  - `reconcile_preserves_custom_endpoint_models_not_configured_locally`
  - `reconcile_preserves_custom_models_saved_on_execution_profile`
  - `removing_endpoint_purges_all_its_models_from_custom_llms`
  - `removing_model_row_purges_from_custom_llms`
  - `shared_model_picker_query_orders_filters_and_marks_disabled_choices`
  - `should_clear_preference_admin_disabled`
  - `should_clear_preference_requires_upgrade_without_byok`
  - `should_clear_preference_unavailable`
  - `should_not_clear_preference_out_of_requests`
  - `should_not_clear_preference_provider_outage`
  - `should_not_clear_preference_requires_upgrade_with_byok`
  - `updating_active_profile_base_model_persists_and_updates_resolution`

### `app/src/ai/agent_sdk/driver/error_classification_tests.rs` — 22 absent

pin 22 · fork 0 · source `app/src/ai/agent_sdk/driver/error_classification.rs` · fork ships source: NO

- **CLOUD** — use warp_graphql::ai::{AgentTaskState, PlatformErrorCode}; crate::server::server_api::ai::TaskStatusUpdate; depends on local_agent_task_sync_model.rs (confirmed cloud above).
  - `bootstrap_internal_error_is_error_with_internal`
  - `bootstrap_pty_spawn_failed_with_reason_includes_reason_in_message`
  - `bootstrap_pty_spawn_failed_without_reason_is_generic`
  - `bootstrap_timed_out_is_error_with_internal`
  - `conversation_blocked_is_blocked`
  - `conversation_cancelled_is_cancelled`
  - `conversation_harness_mismatch_is_failed_with_env_setup`
  - `conversation_resume_state_missing_is_failed_with_resource_not_found`
  - `environment_not_found_is_failed_with_resource_not_found`
  - `environment_setup_failed_is_failed`
  - `harness_auth_check_failed_is_failed_with_auth_required`
  - `harness_runtime_failure_detected_is_failed_with_auth_required`
  - `managed_mcp_resolution_failed_is_failed_with_env_setup`
  - `mcp_server_not_found_is_failed_with_env_setup`
  - `mcp_startup_failed_is_failed_with_env_setup_and_per_server_details`
  - `not_logged_in_is_error_with_auth_required`
  - `profile_error_is_failed_with_resource_not_found`
  - `share_session_disabled_gets_feature_not_available`
  - `share_session_failed_includes_reason`
  - `share_session_timeout_gets_internal_error`
  - `terminal_unavailable_is_error_with_internal`
  - `warp_drive_sync_failed_is_error`

### `app/src/ai/geap_credentials_tests.rs` — 21 absent

pin 21 · fork 0 · source `app/src/ai/geap_credentials.rs` · fork ships source: NO

- **CLOUD** — warp_managed_secrets::{ManagedSecretManager, client::{IdentityTokenOptions, TaskIdentityToken}}; crate::auth::AuthStateProvider; UserWorkspaces -- mints GCP tokens via Warp's own account-bound managed-secrets service.
  - `impersonation_expiry_parses_rfc3339`
  - `impersonation_expiry_rejects_invalid_timestamps`
  - `impersonation_response_parses_camel_case`
  - `mint_binding_from_parts_requires_an_audience`
  - `mint_binding_from_parts_trims_and_normalizes`
  - `mint_binding_from_parts_uses_direct_wif_without_sa`
  - `mint_completion_discards_stale_binding_result_and_remints`
  - `mint_completion_failure_restores_servable_previous`
  - `mint_completion_failure_with_unservable_previous_fails`
  - `refresh_disables_and_drops_tokens_when_gate_is_off`
  - `refresh_noops_while_a_mint_is_in_flight`
  - `refresh_remints_on_binding_mismatch`
  - `refresh_remints_when_token_needs_refresh`
  - `refresh_rests_at_unconfigured_when_enabled_but_unconfigured`
  - `refresh_skips_when_token_is_fresh_and_binding_matches`
  - `safety_net_is_a_pure_noop_when_gate_is_off`
  - `safety_net_noops_on_fresh_token_and_rearms_parked_chain`
  - `sts_expires_at_prefers_expires_in_and_falls_back_to_jwt_expiry`
  - `sts_response_parses_with_and_without_expires_in`
  - `timer_delay_clamps_to_floor_when_near_or_past_expiry`
  - `timer_delay_fires_lead_time_before_expiry`

### `app/src/ai/ambient_agents/spawn_tests.rs` — 20 absent

pin 20 · fork 0 · source `app/src/ai/ambient_agents/spawn.rs` · fork ships source: yes

- **CLOUD** — Fork's ambient_agents/spawn.rs::spawn_task is a stub that always yields Err(anyhow!("Agent spawning is disabled in Zap")). Pin's spawn.rs needs crate::server::server_api::ai::{...}.
  - `followup_api_error_does_not_poll`
  - `followup_bounded_skip_for_server_stall`
  - `followup_cancelled_state_breaks_skip_loop`
  - `followup_skips_prior_terminal_state_until_working_then_attaches`
  - `followup_skips_prior_terminal_then_surfaces_real_failure`
  - `followup_submits_before_polling_and_ignores_previous_session_id`
  - `followup_terminal_failure_surfaces_status_message`
  - `followup_without_previous_session_id_accepts_joinable_session`
  - `followup_without_previous_session_id_errors_if_run_finishes_before_session`
  - `monitor_spawned_task_does_not_spawn_again`
  - `poll_fails_on_permanent_http_error`
  - `poll_for_session_join_info_waits_until_link_is_available`
  - `poll_gives_up_after_max_transient_retries`
  - `poll_retries_transient_429_errors`
  - `poll_stops_on_terminal_failure_like_state`
  - `session_join_info_constructs_link_from_session_id_when_link_missing`
  - `session_join_info_falls_back_to_session_id`
  - `session_join_info_ignores_empty_link_and_invalid_session_id`
  - `session_join_info_prefers_server_session_link_when_session_id_is_present`
  - `session_join_info_requires_session_id`

### `app/src/ai/blocklist/block_tests.rs` — 20 absent

pin 35 · fork 15 · source `app/src/ai/blocklist/block.rs` · fork ships source: yes

- **CLOUD** — recording_artifact_view_url_* builds {ChannelState::oz_root_url()}/runs/... (Warp's hosted recording viewer); user_avatar_info_* needs UserProfileWithUID{firebase_uid,...} (cloud account profile).
  - `recording_artifact_view_url_requires_task_id`
  - `recording_artifact_view_url_uses_configured_oz_origin`
  - `user_avatar_info_prefers_conversation_creator_profile`
  - `user_avatar_info_uses_cached_profile_for_creator_uid`
- **DECLINED** — All built on RunAgentsExecutionMode, the type #325's decline covers end to end -- including its Local arm, since the containing action (model-invoked RunAgents) never reaches these constructors in the fork.
  - `compose_child_prompt_concatenates_when_both_non_empty`
  - `compose_child_prompt_returns_empty_when_both_empty`
  - `compose_child_prompt_treats_whitespace_only_base_as_empty`
  - `compose_child_prompt_uses_base_only_when_per_agent_empty`
  - `compose_child_prompt_uses_per_agent_only_when_base_empty`
  - `local_arm_allows_claude`
  - `local_arm_ignores_auth_secret_name`
  - `local_arm_rejects_agent_identity_uid`
  - `local_arm_rejects_disabled_codex`
  - `remote_arm_filters_whitespace_auth_secret_name_to_none`
  - `remote_arm_propagates_agent_identity_uid`
  - `remote_arm_propagates_claude_auth_secret_into_mode`
  - `remote_arm_propagates_skills_into_skill_references`
  - `remote_arm_rejects_opencode`
  - `remote_arm_with_empty_skills_propagates_empty_vec`
- **COVERED-ELSEWHERE** — Pure string-prefix helper; received_message_collapsible_id already exists in fork's block.rs:837, just missing this one unit test -- genuinely portable, not ported this pass (single small file, low priority vs. the rest of the queue).
  - `received_message_collapsible_id_prefixes_row_ids`

### `app/src/ai/agent/conversation_tests.rs` — 18 absent

pin 36 · fork 18 · source `app/src/ai/agent/conversation.rs` · fork ships source: yes

- **CLOUD** — Body comments "The server's usage metadata is cumulative per conversation" and asserts on credits_spent from credits_usage_metadata(...).
  - `usage_totals_reads_gui_credits_and_accumulates_provider_cost`
- **DECLINED** — Exercises start_recording_tool_call/stop_recording_tool_call message sequences -- the computer_use session-recording feature DECLINED.md marks out of scope under #350.
  - `recording_span_clears_when_stop_errors`
  - `recording_span_closes_on_matching_stop_result`
  - `recording_span_ignores_failed_start`
  - `recording_span_ignores_mismatched_stop_id`
  - `recording_span_stays_open_without_stop_result`
- **DECLINED** — DECLINED.md #142/#347 CustomEndpoint row.
  - `footer_model_token_usage_keeps_custom_endpoint_usage_distinct_from_same_labeled_models`
  - `footer_model_token_usage_preserves_unresolved_custom_endpoint_usage_with_fallback_label`
  - `update_cost_and_usage_resolves_custom_endpoint_alias_for_footer_usage`
  - `update_cost_and_usage_uses_fallback_label_for_unknown_custom_endpoint`
- **MISSING-SUBSYSTEM** — Pure local AIConversation/root-task/memory-list logic, no network. Mis-bucketed CLOUD? by the mechanical pass; genuinely missing local test coverage, not ported this pass.
  - `cli_agent_transcript_vehicle_is_excluded_from_navigation`
  - `fetched_memories_dedupes_keeping_first_position_and_latest_data`
  - `fetched_memories_is_empty_when_no_message_has_memories`
  - `fetched_memories_preserves_order_across_and_within_messages`
  - `reassign_exchange_ids_keeps_exchange_lookup_consistent`
  - `restored_conversation_ignores_legacy_root_task_is_optimistic_flag_with_empty_tasks`
  - `restored_conversation_ignores_legacy_root_task_is_optimistic_flag_with_non_empty_tasks`
  - `restored_conversation_with_empty_task_list_creates_in_progress_optimistic_root`

### `app/src/ai/blocklist/handoff/pipeline_tests.rs` — 18 absent

pin 18 · fork 0 · source `app/src/ai/blocklist/handoff/pipeline.rs` · fork ships source: NO

- **CLOUD** — Whole file, including the generic-sounding tests: prepare_rejects_an_empty_source_without_a_prompt and prepare_collects_completed_descendant_paths still construct prepare_handoff(...) with SnapshotUploadTarget::Local{ai_client,http} and PendingCloudLaunch -- there is no local-only entry point to prepare_handoff, even its argument-validation branch is unreachable without the cloud launch/upload machinery. Source imports crate::cloud_object::CloudObjectLookup, crate::ai::cloud_environments::CloudAmbientAgentEnvironment, crate::server::server_api::ai::{...}. Directory absent from fork.
  - `cancellation_after_materialization_stops_before_spawn`
  - `cancellation_during_spawn_cancels_the_created_task`
  - `empty_prompt_substitution_matrix_matches_gui_behavior`
  - `execute_revalidates_current_environment_catalog_before_returning_future`
  - `execute_revalidates_current_handoff_enablement_before_returning_future`
  - `execute_revalidates_current_model_before_returning_future`
  - `explicit_selection_precedence_and_restoration_are_exactly_once`
  - `fork_materialization_precedes_exactly_one_spawn`
  - `fresh_launch_skips_fork_and_materializes_before_spawn`
  - `model_selection_refreshes_cloud_compatibility_in_both_directions`
  - `prepare_accepts_a_cwd_snapshot_without_a_source_or_prompt`
  - `prepare_collects_completed_descendant_paths`
  - `prepare_falls_back_to_auto_for_an_implicit_local_model`
  - `prepare_orders_guards_cancellation_token_check_and_attachment_transfer`
  - `prepare_preserves_untransferred_source_attachments`
  - `prepare_rejects_an_empty_source_without_a_prompt`
  - `required_environment_revalidates_after_catalog_refresh`
  - `snapshot_failure_degrades_to_spawn_without_token`

### `app/src/ai/orchestration/snapshots_tests.rs` — 18 absent

pin 18 · fork 0 · source `app/src/ai/orchestration/snapshots.rs` · fork ships source: NO

- **CLOUD** — Pick a remote execution target: Warp Environments (#211) / cloud runners (#290), worker_host="warp".
  - `environment_snapshot_puts_empty_option_first`
  - `host_snapshot_dedupes_connected_and_recent_against_known_rows`
  - `host_snapshot_orders_default_warp_connected_recent`
  - `runner_snapshot_loading_reports_loading_status`
  - `runner_snapshot_puts_use_default_first_and_selects`
- **MISSING-SUBSYSTEM** — the pin's `app/src/ai/orchestration/` config-picker layer is absent (orchestration ITSELF is built here — see the correction at the top of this file). 13 of these 18 (harness_snapshot_*, oz_model_snapshot_*/non_oz_model_snapshot_*, api_key_snapshot_*) are pure plain-data transforms with no cloud symbol in the test body -- real, now-in-scope local-orchestration debt (see DECLINED.md's 2026-08-08 reversal), not cloud. Refines my own earlier file-level MISSING-SUBSYSTEM verdict with this split.
  - `api_key_snapshot_keeps_named_selection_while_loading`
  - `api_key_snapshot_lists_skip_then_names`
  - `api_key_snapshot_maps_inherit_and_unset_selection`
  - `harness_snapshot_excludes_gemini_and_selects_initial`
  - `harness_snapshot_filters_product_disabled_local_harness`
  - `harness_snapshot_keeps_cloud_opencode_selectable`
  - `harness_snapshot_marks_missing_local_cli_disabled_and_sorts_last`
  - `harness_snapshot_marks_server_disabled_entries`
  - `harness_snapshot_matches_selection_by_display_name_for_stale_cache`
  - `non_oz_model_snapshot_falls_back_to_default_for_unknown_or_empty_id`
  - `non_oz_model_snapshot_puts_default_first_and_selects_server_model`
  - `oz_model_snapshot_carries_disabled_reason`
  - `oz_model_snapshot_empty_catalog_reports_empty_status`

### `app/src/ai/agent_sdk/driver_tests.rs` — 17 absent

pin 49 · fork 32 · source `app/src/ai/agent_sdk/driver.rs` · fork ships source: yes

- **CLOUD** — managed_* (6) and well_known_* (4) call MockManagedMcpClient/ManagedSecretValue -- one literally proxies through app.warp.dev/mcp/integration-proxy/linear. json_format_input_omits_filepath_and_description_for_proto_upload_result formats an UploadArtifactResult variant absent from the fork entirely (cloud-adjacent, same family as artifact_upload).
  - `json_format_input_omits_filepath_and_description_for_proto_upload_result`
  - `managed_command_config_arg_placeholder_uses_local_secret`
  - `managed_command_config_env_placeholder_uses_local_secret`
  - `managed_command_config_missing_secret_leaves_placeholder`
  - `managed_command_config_preserves_literal_env_despite_colliding_local_secret`
  - `managed_command_config_preserves_literal_env_when_synthesizing_arg_placeholder`
  - `managed_resolution_failure_includes_uid_and_message`
  - `managed_resolver_local_uuid_does_not_call_managed_client`
  - `managed_resolver_non_local_uuid_calls_managed_client`
  - `managed_url_config_preserves_header_despite_colliding_local_secret`
  - `managed_url_config_preserves_proxy_url_and_header`
  - `well_known_resolution_failure_does_not_drop_other_specs`
  - `well_known_resolution_failure_skips_server`
  - `well_known_spec_is_skipped_when_flag_disabled`
  - `well_known_spec_resolves_via_managed_client`
- **DECLINED** — These are AgentDriver::load_global_skills; already recorded as never-ported for architectural reasons (no local multi-spec policy source) rather than raw cloud need -- treat as an existing decision, not fresh debt.
  - `overlap_repo_in_env_and_global_loads_all_skills_without_duplicates`
  - `split_loading_env_loads_all_global_loads_subset`

### `app/src/ai/blocklist/action_model/execute/run_agents_tests.rs` — 16 absent

pin 16 · fork 0 · source `app/src/ai/blocklist/action_model/execute/run_agents.rs` · fork ships source: NO

- **DECLINED** — Executor for `AIAgentActionType::RunAgents` (imports `RunAgentsAgentRunConfig`, `RunAgentsExecutionMode`, `OrchestrationConfig`). This is the model-invoked spawning DECLINED.md #325 covers end to end ("the pin's RunAgentsRequest is cloud-typed... AIAgentActionType has no spawn variant"). Reclassifying all 16 (the mechanical pass split 14 as DIVERGENT?, 1 CLOUD?, 1 DECLINED? -- but the whole executor is unreachable in the fork for the same #325 reason, not three different reasons).
  - `autonomous_mode_autoexecutes_and_does_not_deny_missing_api_key`
  - `cancel_during_plan_publication_does_not_dispatch_children`
  - `execute_denies_disapproved_plan_config`
  - `execute_denies_duplicate_launched_agent`
  - `execute_denies_never_allow_profile_setting`
  - `execute_denies_remote_non_warp_harness_without_default_auth_secret`
  - `execute_publishes_every_parent_owned_plan_before_dispatch`
  - `local_codex_run_agents_maps_to_local_harness_mode_when_flag_enabled`
  - `populate_default_auth_secret_for_autoexecute_uses_persisted_secret`
  - `should_autoexecute_duplicate_launched_agent_denial`
  - `should_autoexecute_remote_non_warp_harness_with_always_allow_even_without_default_auth_secret`
  - `should_autoexecute_remote_non_warp_harness_with_default_auth_secret`
  - `should_autoexecute_remote_warp_harness_without_default_auth_secret`
  - `should_autoexecute_when_plan_has_approved_orchestration_config`
  - `should_not_autoexecute_approved_remote_non_warp_plan_without_default_auth_secret`
  - `should_not_autoexecute_without_approved_plan_or_always_allow_profile`

### `app/src/ai/agent_management/agent_management_model_tests.rs` — 15 absent

pin 15 · fork 0 · source `app/src/ai/agent_management/agent_management_model.rs` · fork ships source: NO

- **PORTABLE-OUT-OF-AREA** — The pin's `AgentNotificationsModel` (in-app notification triggering + pending-artifact accumulation) was ported to the fork, but RELOCATED to `app/src/notifications/model.rs` -- outside app/src/ai/**, my write boundary. Confirmed by the module's own doc comment (`app/src/notifications/model.rs:10,276`): "Removed the `ActiveAgentViewsModel` subscription... This replaces the original `ActiveAgentViewsModel::is_conversation_open` check", matching DECLINED.md's ActiveAgentViewsModel row exactly. `app/src/notifications/model.rs` has ZERO test coverage (grep for `cfg(test)`/`mod tests` in that file: no hits) -- all 15 pin tests (`should_trigger_notification_*`, `artifact_event_accumulates_into_pending`, `flush_drains_pending_artifacts`, etc.) are real, portable test debt, but the port target is outside my area. Flagging for the agent/coordinator that owns `app/src/notifications/**`.
  - `add_notification_tracks_unread_activity_when_in_app_notifications_are_hidden`
  - `artifact_event_accumulates_into_pending`
  - `deletion_cleans_up_pending_artifacts`
  - `flush_drains_pending_artifacts`
  - `flush_returns_empty_vec_when_no_artifacts`
  - `in_progress_resume_clears_stale_notification_and_adds_none`
  - `multiple_artifacts_accumulated_across_turns`
  - `separate_conversations_have_independent_pending_artifacts`
  - `should_trigger_notification_returns_false_for_cancelled`
  - `should_trigger_notification_returns_false_for_in_progress`
  - `should_trigger_notification_returns_false_for_waiting_for_events`
  - `should_trigger_notification_returns_true_for_blocked`
  - `should_trigger_notification_returns_true_for_error`
  - `should_trigger_notification_returns_true_for_success`
  - `waiting_for_events_clears_stale_notification_and_adds_none`

### `app/src/ai/agent_sdk/artifact_upload_tests.rs` — 15 absent

pin 15 · fork 0 · source `app/src/ai/agent_sdk/artifact_upload.rs` · fork ships source: NO

- **CLOUD** — crate::server::server_api::ServerApi; presigned_upload::upload_file_to_target.
  - `ambient_task_id_from_conversation_metadata_requires_cloud_task_metadata`
  - `checked_graphql_size_bytes_for_upload_returns_none_for_overflow`
  - `explicit_run_id_wins_over_env_fallback`
  - `failed_conversation_resolution_falls_back_to_env_run_id`
  - `file_size_and_prefix_for_path_returns_full_contents_when_prefix_exceeds_file`
  - `file_size_and_prefix_for_path_returns_truncated_prefix`
  - `invalid_env_run_id_returns_clear_error`
  - `invalid_explicit_run_id_errors_even_if_env_fallback_exists`
  - `load_env_run_id_reads_variable`
  - `missing_args_and_missing_env_return_clear_error`
  - `missing_args_fall_back_to_env_run_id_for_request_association`
  - `normalize_artifact_filepath_preserves_shape_and_normalizes_separators`
  - `single_conversation_metadata_errors_when_no_metadata_is_returned`
  - `single_conversation_metadata_returns_the_only_metadata_record`
  - `valid_conversation_resolution_ignores_env_fallback`

### `app/src/ai/agent_sdk/mod_tests.rs` — 14 absent

pin 14 · fork 0 · source `app/src/ai/agent_sdk/mod.rs` · fork ships source: yes

- **CLOUD** — Artifacts confirmed cloud elsewhere (agent_sdk/artifact*.rs).
  - `artifact_download_requires_auth`
  - `artifact_get_requires_auth`
  - `artifact_upload_requires_auth`
- **DECLINED** — All operate on CliCommand::Run(TaskCommand::Message(...)). DECLINED.md's reversed SendMessageToAgent entry explicitly separates this from the new local mailbox: "oz run/run message was removed as genuine cloud... run_command_is_removed asserts a PERMANENT absence and must not be reinstated."
  - `run_message_send_requires_auth`
  - `run_message_send_telemetry_defaults_to_unknown_harness`
  - `run_message_send_telemetry_supports_claude_code_alias`
  - `run_message_send_telemetry_supports_opencode_harness`
  - `run_message_send_telemetry_uses_canonical_harness_from_env`
  - `run_message_watch_telemetry_defaults_to_unknown_harness`
- **DECLINED** — DECLINED.md /logout row (#338).
  - `logout_does_not_require_auth`
- **DECLINED** — DECLINED.md Account-first onboarding (#11).
  - `login_does_not_require_auth`
- **MISSING-SUBSYSTEM** — Pure local harness-reconciliation logic, no network -- mis-bucketed CLOUD? by the mechanical pass.
  - `reconcile_task_harness_adopts_task_harness_when_cli_uses_default`
  - `reconcile_task_harness_allows_matching_explicit_harness`
  - `reconcile_task_harness_rejects_explicit_mismatch`

### `app/src/ai/blocklist/action_model/recording_controller_tests.rs` — 14 absent

pin 14 · fork 0 · source `app/src/ai/blocklist/action_model/recording_controller.rs` · fork ships source: NO

- **DECLINED** — DECLINED.md, "computer_use session recording" row (#350): "this fork is not doing recording... PointerSession/PointerSink exist solely to stitch pointer events across the discrete UseComputer calls that make up one recording... UseComputerExecutor runs the actor without a recording controller or pointer sink." recording_controller.rs's begin/commit/discard/finalize-scoping tests are exactly this declined controller.
  - `begin_and_commit_are_scoped_to_the_owning_conversation`
  - `begin_and_commit_record_finish_offset_and_labels`
  - `begin_while_pending_auto_commits_prior_group`
  - `commit_after_finalization_is_noop`
  - `commit_clamps_finish_to_start`
  - `commit_without_begin_is_noop`
  - `conversation_finalization_only_matches_owner`
  - `discard_drops_pending_group_without_committing`
  - `dropped_waiter_does_not_discard_finalized_result`
  - `finalization_is_shared_and_retained_until_consumed`
  - `joining_caller_observes_actual_finalize_reason_not_claimed_one`
  - `matching_conversation_cancels_start_reservation`
  - `mismatched_claim_preserves_active_recording`
  - `pointer_only_group_commits_with_empty_labels_and_geometry`

### `app/src/ai/agent/api/impl_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/ai/agent/api/impl.rs` · fork ships source: NO

- **CLOUD** — crate::server::server_api::{AIApiError, ServerApi}; generate_multi_agent_output is the direct call into Warp's warp_multi_agent_api backend, superseded here by the fork's direct-genai BYOP path.
  - `api_keys_with_warp_credit_fallback_setting_creates_fallback_only_api_keys`
  - `api_keys_with_warp_credit_fallback_setting_preserves_existing_keys`
  - `api_keys_with_warp_credit_fallback_setting_returns_none_without_keys_or_fallback`
  - `remote_supported_tools_include_search_codebase_when_connected_and_feature_flag_is_enabled`
  - `remote_supported_tools_omit_search_codebase_when_feature_flag_is_disabled`
  - `remote_supported_tools_omit_search_codebase_when_remote_is_not_connected`
  - `supported_tools_include_orchestration_tools_when_orchestration_enabled`
  - `supported_tools_include_upload_artifact_when_feature_flag_is_enabled`
  - `supported_tools_includes_ask_user_question_when_enabled_and_feature_flag_is_enabled`
  - `supported_tools_omit_orchestration_tools_when_orchestration_disabled`
  - `supported_tools_omit_upload_artifact_when_feature_flag_is_disabled`
  - `supported_tools_omits_ask_user_question_when_disabled`
  - `supports_orchestration_v2_matches_request_orchestration_setting`

### `app/src/ai/agent_sdk/artifact_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/ai/agent_sdk/artifact.rs` · fork ships source: NO

- **CLOUD** — crate::server::server_api::{ServerApi, ServerApiProvider}, AIClient, ArtifactDownloadResponse; depends on artifact_upload.rs (confirmed cloud).
  - `download_destination_defaults_pdf_to_artifact_uid_with_extension`
  - `download_destination_defaults_screenshot_to_artifact_uid_with_extension`
  - `download_destination_defaults_to_file_artifact_filename`
  - `download_destination_uses_explicit_path`
  - `write_download_output_to_writes_pretty_output`
  - `write_get_output_to_writes_json_output`
  - `write_get_output_to_writes_ndjson_output`
  - `write_get_output_to_writes_pretty_output`
  - `write_get_output_to_writes_text_output`
  - `write_upload_output_to_writes_json_output`
  - `write_upload_output_to_writes_ndjson_output`
  - `write_upload_output_to_writes_pretty_output`
  - `write_upload_output_to_writes_text_output`

### `app/src/ai/skills/file_watchers/skill_watcher_tests.rs` — 13 absent

pin 18 · fork 5 · source `app/src/ai/skills/file_watchers/skill_watcher.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Every one of these 13 needs `parse_project_skill_contents`, `refresh_project_skills_for_repo`, or `local_project_fallback_*` -- none exist anywhere in the fork's `skill_watcher.rs` (877 lines vs pin's 1095; repo-wide grep for `parse_project_skill_contents` finds zero hits). The fork's `SkillWatcher` only does direct local-filesystem repo scanning (`read_skills_for_repos`, `scan_repository_for_skills`); the pin additionally layers a `repo_metadata`/`FileTreeState`-driven remote-aware project-skill refresh with a local-fallback path when repo metadata is unavailable. This whole refresh/fallback layer is the missing subsystem, not individually portable tests.
  - `parse_project_skill_contents_classifies_foreign_encoded_provider_path`
  - `parse_project_skill_contents_preserves_remote_paths`
  - `test_handle_repository_update_non_skill_directory_added_does_not_emit_project_event`
  - `test_local_project_fallback_directory_addition_scans_filesystem`
  - `test_local_project_fallback_initial_scan_loads_symlinked_skill_directory`
  - `test_local_project_fallback_scans_filesystem_when_repo_metadata_fails`
  - `test_local_project_fallback_update_reuses_repository_update_handler`
  - `test_refresh_project_skills_for_repo_loads_indexed_and_symlinked_skill_directories`
  - `test_refresh_project_skills_for_repo_removes_missing_project_skill_paths`
  - `test_refresh_project_skills_for_repo_uses_repo_metadata_without_fallback_watcher`
  - `test_removing_project_repo_invalidates_pending_refresh_result`
  - `test_removing_remote_project_repo_deletes_shared_cached_skill_paths`
  - `test_stale_project_skill_refresh_result_is_ignored`

### `app/src/ai/agent_events/driver_tests.rs` — 12 absent

pin 19 · fork 7 · source `app/src/ai/agent_events/driver.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. Fork's app/src/ai/agent_events/driver.rs already exists and imports its own local AgentEventStreamClient/AgentRunEvent types, decoupled from crate::server::server_api (the pin's version imports that). The driver's retry/backoff/cursor-persistence logic is transport-agnostic (matches DECLINED.md's own agent_sdk-bounded-retry false-positive note, #278). Only 2 of 12 pin tests are ported; the rest are directly portable against the existing local test harness. Not ported this pass for lack of time to trace the exact FakeAgentEventSource-equivalent test fixture.
  - `driver_does_not_count_non_auth_failures_toward_auth_give_up`
  - `driver_does_not_give_up_on_non_auth_error_when_only_auth_bounded`
  - `driver_gives_up_after_consecutive_auth_failures`
  - `driver_gives_up_after_max_retry_duration`
  - `driver_resets_auth_streak_after_non_auth_failure`
  - `driver_uses_fast_backoff_on_transient_http_error`
  - `driver_uses_slow_backoff_on_permanent_http_error`
  - `http_status_error_actionability_follows_status_classification`
  - `non_actionable_stream_statuses_do_not_report_at_threshold`
  - `server_error_status_reports_at_threshold_crossing`
  - `zero_threshold_disables_stream_error_escalation`
- **COVERED-ELSEWHERE** — Renamed sibling: fork already ports 2 tests as backoff_escalates_then_caps / failure_threshold_is_reached_at_and_above_limit.
  - `actionable_stream_status_reports_only_at_threshold_crossing`

### `app/src/ai/agent_sdk/driver/attachments_tests.rs` — 11 absent

pin 11 · fork 0 · source `app/src/ai/agent_sdk/driver/attachments.rs` · fork ships source: NO

- **CLOUD** — crate::server::server_api::{ServerApi, ai::AIClient, presigned_upload::HttpStatusError}; "Fetches task attachments via GraphQL."
  - `e2e_empty_attachment_list_returns_none_without_creating_dir`
  - `e2e_get_handoff_snapshot_attachments_failure_is_fatal`
  - `e2e_happy_path_downloads_all_and_writes_to_disk`
  - `e2e_partial_success_returns_dir_with_downloaded_subset`
  - `e2e_permanent_4xx_fails_fast_without_retries`
  - `e2e_retry_exhaustion_marks_failed`
  - `e2e_returns_none_when_oz_handoff_flag_is_disabled`
  - `e2e_transient_5xx_retried_then_succeeds`
  - `process_attachment_nonexistent_file`
  - `process_attachment_text_file`
  - `process_attachment_too_large`

### `app/src/ai/blocklist/inline_action/host_picker_tests.rs` — 11 absent

pin 11 · fork 0 · source `app/src/ai/blocklist/inline_action/host_picker.rs` · fork ships source: NO

- **CLOUD** — host_picker.rs's own doc comment: "Picker for the cloud-agent worker host slug... mirrors the Oz webapp's host selector: workspace default first..., then warp, then connected worker hosts...". This is the cloud-runner worker-host picker, part of #290's declined half, not a generic remote_server/SSH host picker.
  - `build_menu_items_adds_connected_hosts_before_recent_and_dedups_known_hosts`
  - `build_menu_items_adds_recent_after_warp`
  - `build_menu_items_custom_entry_dispatches_enter_custom_mode`
  - `build_menu_items_dedups_recent_when_it_matches_default_or_warp`
  - `build_menu_items_promotes_default_to_top`
  - `build_menu_items_warp_entry_dispatches_select_known_warp`
  - `build_menu_items_with_no_defaults_shows_warp_and_custom`
  - `menu_label_for_picks_default_badge_when_slug_matches_default`
  - `menu_label_for_returns_plain_slug_for_unknown_value`
  - `menu_label_for_returns_plain_slug_for_warp`
  - `normalize_slug_trims_whitespace_and_falls_back_to_warp_when_empty`

### `app/src/ai/active_agent_views_model_tests.rs` — 10 absent

pin 10 · fork 0 · source `app/src/ai/active_agent_views_model.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — `ActiveAgentViewsModel` does not exist anywhere in the fork as a real type -- every repo-wide hit on the name is a prose comment explaining its removal (`app/src/notifications/model.rs:10`, `app/src/ai/blocklist/history_model.rs:895`, `app/src/ai/blocklist/controller.rs:1776`, `app/src/workspace/view.rs:4630`). DECLINED.md's ActiveAgentViewsModel row says the fork substituted `BlocklistAIHistoryModel::terminal_view_id_for_conversation` for the model's `is_conversation_open`/conversation-transfer check, and names exactly ONE still-declined pin test (`clicking_old_banner_for_open_conversation_focuses_current_terminal_surface_without_transferring_blocks`) -- which is NOT among these 10. These 10 test a DIFFERENT mechanism: per-`WindowId` "last focused agent view" tracking (register/overwrite/clear per window, `ambient_session_registration_replaces_stale_terminal_for_same_task`, `last_focused_terminal_tracks_most_recent_globally`, etc.). `terminal_view_id_for_conversation` is a reverse lookup (conversation -> terminal) with no per-window state at all -- it does not cover this forward-direction "what was last focused in window X" tracking. Correction to DECLINED.md's entry: it is NOT fully superseded; only the one banner test it names is. These 10 remain real, unaddressed parity debt for a per-window focus-tracking mechanism the fork lacks.
  - `ambient_session_registration_replaces_stale_terminal_for_same_task`
  - `ambient_session_unregister_keeps_task_open_until_last_terminal_is_removed`
  - `clearing_one_window_does_not_affect_other`
  - `conversation_switch_updates_last_focused_terminal_state`
  - `focus_change_without_task_id_has_no_conversation`
  - `last_focused_terminal_tracks_most_recent_globally`
  - `overwriting_same_window_updates_state`
  - `per_window_focused_state_is_independent`
  - `remove_focused_state_for_window_cleans_up`
  - `unknown_window_returns_none`

### `app/src/ai/agent_sdk/ambient_tests.rs` — 10 absent

pin 10 · fork 0 · source `app/src/ai/agent_sdk/ambient.rs` · fork ships source: NO

- **CLOUD** — Module doc: "Commands to interact with ambient agents on Warp's platform"; crate::ServerApiProvider. Matches DECLINED.md's run_command_is_removed permanent-absence note.
  - `empty_args_yields_default_filter`
  - `every_field_maps_through`
  - `source_cli_maps_to_cli`
  - `source_interactive_maps_to_local`
  - `state_flags_map_to_filter`
  - `task_id_for_message_send_falls_back_to_oz_run_id`
  - `task_id_for_message_send_prefers_sender_run_id`
  - `task_id_from_oz_run_id_env_rejects_invalid_value`
  - `task_id_from_run_id_accepts_task_uuid`
  - `task_id_from_run_id_ignores_non_task_ids`

### `app/src/ai/artifacts/mod_tests.rs` — 10 absent

pin 11 · fork 1 · source `app/src/ai/artifacts/mod.rs` · fork ships source: yes

- **CLOUD** — Construct ArtifactDownloadResponse from crate::server::server_api::ai, or warp_graphql::ai::AIConversationArtifact directly.
  - `converts_graphql_file_artifact`
  - `default_download_filename_falls_back_to_artifact_uid_with_extension`
  - `default_download_filename_prefers_server_filename`
  - `download_success_message_includes_filename_and_directory`
  - `resolves_lightbox_image_for_screenshot_artifact`
  - `returns_failure_placeholder_for_screenshot_load_errors`
  - `skips_lightbox_update_for_non_screenshot_artifact`
- **MISSING-SUBSYSTEM** — Pure string/filename formatting, no server type involved -- mis-bucketed CLOUD? by the mechanical pass.
  - `file_button_label_falls_back_to_filepath_basename`
  - `file_button_label_falls_back_to_generic_label`
  - `file_button_label_prefers_filename`

### `app/src/ai/execution_profiles/editor/mod_tests.rs` — 10 absent

pin 18 · fork 8 · source `app/src/ai/execution_profiles/editor/mod.rs` · fork ships source: yes

- **DECLINED** — DECLINED.md #142/#347 CustomEndpoint row.
  - `custom_endpoint_fixed_context_does_not_expose_control_or_warning`
- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. has_configurable_context_window/context_window_limit_for_request/should_show_long_context_pricing_warning don't exist anywhere in the fork (only the pure math helper context_window_snap_values is ported). Feeds LLMPreferences::update_feature_model_choices with a hand-built LLMInfo -- since Phosphor's model list is already built entirely locally (DECLINED #142/#347), this is local BYOP context-window config, not a cloud model-catalog fetch.
  - `non_openai_configurable_context_ignores_gpt_flag_and_does_not_show_openai_warning`
  - `openai_configurable_context_does_not_require_direct_host_metadata`
  - `openai_configurable_context_uses_server_metadata_without_model_or_host_allowlist`
  - `openai_expanded_context_is_hidden_while_feature_flag_is_off`
  - `openai_fixed_context_metadata_does_not_expose_control_or_warning`
  - `openai_long_context_warning_clamps_stale_override_to_lowered_model_max`
  - `openai_long_context_warning_starts_above_threshold`
  - `openai_request_limit_is_clamped_when_configurable_context_is_available`
  - `openai_request_limit_remains_unset_without_a_selected_override`

### `app/src/ai/orchestration/edit_state_tests.rs` — 10 absent

pin 10 · fork 0 · source `app/src/ai/orchestration/edit_state.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — Same missing `app/src/ai/orchestration/` module family as snapshots.rs -- shared local/cloud execution-mode edit-state logic (`apply_execution_mode_change`, harness/model cascade). No fork equivalent exists.
  - `execution_mode_change_prefers_valid_fallback_over_default_model`
  - `execution_mode_change_to_cloud_prefills_default_environment`
  - `execution_mode_change_to_local_forces_oz_and_strips_cloud_fields`
  - `forcing_oz_before_local_preserves_codex_model_memory`
  - `harness_change_applies_resolved_auth_selection`
  - `harness_change_saves_and_restores_per_harness_model_memory`
  - `revalidate_drops_deleted_named_secret_and_reseeds_from_resolved`
  - `revalidate_keeps_named_secret_still_present`
  - `revalidate_leaves_explicit_inherit_alone`
  - `revalidate_resets_vanished_model_to_default`

### `app/src/ai/agent_sdk/driver/harness/claude_code_tests.rs` — 9 absent

pin 34 · fork 25 · source `app/src/ai/agent_sdk/driver/harness/claude_code.rs` · fork ships source: yes

- **CLOUD** — Calls ClaudeHarness::prepare_local_wake_command, defined in claude_code/wake_driver.rs which imports warp_graphql::ai::AgentTaskState + crate::server::server_api::{ServerApi, ai::AIClient, harness_support::ResolvePromptRequest} -- CLOUD-coupled at the signature level even though this specific test exercises local rehydration.
  - `prepare_local_wake_command_rehydrates_transcript_with_self_managed_listener`
- **MISSING-SUBSYSTEM** — 8 of 9 are pure local filesystem/env logic with no cloud symbols (claude_command_uses_resume_flag_when_resuming, both message_bridge_cleanup_*, both parent_bridge_event_cursor_*, resolve_suffix_from_resolved_env_vars, write_session_index_entry_creates_expected_entry, prime_parent_bridge_staged_for_self_managed_wake_keeps_message_in_staged) -- a local self-managed-wake message bridge the fork's claude_code.rs (25/34 tests already ported) doesn't have; MessageBridge/parent-bridge-cursor functions don't exist yet.
  - `claude_command_uses_resume_flag_when_resuming`
  - `message_bridge_cleanup_preserves_state_for_wakeable_runs`
  - `message_bridge_cleanup_removes_state_for_non_wakeable_runs`
  - `parent_bridge_event_cursor_defaults_to_zero_when_missing`
  - `parent_bridge_event_cursor_round_trips`
  - `prime_parent_bridge_staged_for_self_managed_wake_keeps_message_in_staged`
  - `resolve_suffix_from_resolved_env_vars`
  - `write_session_index_entry_creates_expected_entry`

### `app/src/ai/skills/skill_manager_tests.rs` — 9 absent

pin 25 · fork 16 · source `app/src/ai/skills/skill_manager.rs` · fork ships source: yes

- **PORTED** — Ported in this pass. All dependencies (`set_remote_home_skills`, `get_skills_for_working_directory_with_origin`, `skill_exists_for_any_provider`, `best_supported_provider`, existing `remote_test_path`/`make_remote_home_skill` helpers) already existed in the fork's skill_manager.rs/skill_manager_tests.rs; only `set_remote_home_skills`'s extra `ctx` parameter (vs. the pin's 3-arg call) needed adjusting.
  - `remote_home_provider_variants_are_available_for_provider_selection`
  - `remote_home_provider_variants_are_scoped_to_the_descriptor_host`
  - `remote_home_skill_replaces_an_overlapping_index_entry`
- **PORTED** — Already ported (pre-existing on main, verified present at skill_manager_tests.rs before this pass) -- carried forward as-is.
  - `active_skill_by_reference_distinguishes_remote_hosts_with_the_same_display_path`
  - `active_skill_by_reference_resolves_exact_remote_identity`
  - `active_skill_by_reference_with_origin_returns_typed_lookup_errors`
- **CLOUD** — Already hand-traced in the source inventory (not '?'): `SkillManager::set_cloud_environment` feeds `driver.rs::load_environment_skills`, whose `SourceRepo` comes from `cloud_object_models`. Carried forward as-is.
  - `cloud_environment_skills_always_included`
- **DIVERGENT** — Already hand-traced in the source inventory: bundled skill id `tui-migrate-setup` does not exist here; the fork ships `tui-settings` instead (TODO.md:231, bundled_tests.rs:131). Carried forward as-is.
  - `tui_migration_skill_has_tui_only_activation`
- **DIVERGENT** — Already hand-traced in the source inventory: the fork's `read_bundled_skills` takes one argument (no `resources_dir`) and documents `{{skill_dir}}` as absent (bundled.rs:503). Carried forward as-is.
  - `test_read_bundled_skills_renders_host_paths`

### `app/src/ai/agent_sdk/driver/git_credentials_tests.rs` — 8 absent

pin 8 · fork 0 · source `app/src/ai/agent_sdk/driver/git_credentials.rs` · fork ships source: NO

- **CLOUD** — Doc comment: "Git credentials management for cloud agent sandboxes"; crate::server::server_api::ai::{AIClient, GitCredential}.
  - `credential_diagnostics_reports_presence_without_values`
  - `git_credentials_file_content_includes_each_provider_host`
  - `write_gh_hosts_yml_excludes_gitlab_credentials`
  - `write_gh_hosts_yml_skips_gitlab_only_credentials`
  - `write_gh_hosts_yml_uses_gh_cli_filename`
  - `write_glab_config_excludes_github_credentials`
  - `write_glab_config_skips_github_only_credentials`
  - `write_glab_config_uses_glab_cli_filename`

### `app/src/ai/blocklist/block/view_impl/output_tests.rs` — 8 absent

pin 13 · fork 5 · source `app/src/ai/blocklist/block/view_impl/output.rs` · fork ships source: yes

- **CLOUD** — Needs `format_upload_artifact_text`/`UploadArtifactRequest` -- absent from `output.rs`. Same upload-artifact-needs-a-cloud-target cluster as artifact_upload_tests.rs/artifact_tests.rs.
  - `format_upload_artifact_text_includes_request_details`
  - `format_upload_artifact_text_includes_success_summary`
  - `format_upload_artifact_text_includes_terminal_status`
- **DECLINED** — Card text for the computer_use recording start/stop actions -- DECLINED.md #350.
  - `start_recording_card_text_includes_failure_copy`
  - `start_recording_card_text_uses_static_title_and_description_subtext`
  - `stop_recording_card_text_includes_complete_duration`
  - `stop_recording_card_text_includes_partial_duration_without_raw_reason`
- **MISSING-SUBSYSTEM** — Needs `use_computer_decoration` -- absent from `output.rs`. computer_use itself is explicitly NOT cloud (DECLINED.md false-positive list, and #349 restored the platform-neutral API), so this decoration/rendering helper for a UseComputer action card is real, portable-looking debt; not ported this pass for lack of time to trace its render-tree dependencies.
  - `use_computer_decoration_skips_screenshot_only_rows`

### `app/src/ai/blocklist/usage/rollup_tests.rs` — 8 absent

pin 8 · fork 0 · source `app/src/ai/blocklist/usage/rollup.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — rollup.rs's own doc comment: "Pure function -- no I/O, no GraphQL. Walks `BlocklistAIHistoryModel` using the shared `descendant_conversation_ids_in_spawn_order` helper." That helper ALREADY EXISTS in the fork at `app/src/ai/blocklist/orchestration_topology.rs:164` (confirmed, with its own tests). This module (per-agent credit-breakdown rollup for the footer "View details" list) does not exist in the fork, but its sole real dependency does -- unusually close to portable. Not ported in this pass because the consuming UI (footer credit rollup) and its exact `credits_spent` bookkeeping on `AIConversation` were not independently verified against the fork's persistence layer; flagging as the highest-value MISSING-SUBSYSTEM item in this sweep.
  - `excludes_zero_credit_descendants_from_breakdown`
  - `returns_none_when_only_orchestrator_has_zero_credits_with_loaded_children`
  - `returns_none_when_orchestrator_has_no_descendants`
  - `returns_six_contributors_for_show_n_more_caller`
  - `rolls_up_grandchildren_transitively`
  - `sums_orchestrator_and_loaded_descendants`
  - `ties_break_by_spawn_order_earlier_first`
  - `unloaded_descendant_id_is_silently_skipped`

### `app/src/ai/conversation_details_panel_tests.rs` — 8 absent

pin 8 · fork 0 · source `app/src/ai/conversation_details_panel.rs` · fork ships source: NO

- **CLOUD** — crate::server::server_api::ai::AmbientAgentTask, crate::cloud_object::CloudObjectLookup. Covers test_from_task_* (4), test_from_conversation_prefers_server_creator_profile, test_oz_run_url_present_for_task_and_absent_for_conversation.
  - `test_from_conversation_prefers_server_creator_profile`
  - `test_from_task_includes_linked_directory_when_run_id_matches`
  - `test_from_task_includes_linked_directory_when_server_token_matches`
  - `test_from_task_populates_executor`
  - `test_from_task_resolves_harness`
  - `test_oz_run_url_present_for_task_and_absent_for_conversation`
- **MISSING-SUBSYSTEM** — Local-only field mapping, but bundled inside a component the fork ships zero of.
  - `test_from_conversation_metadata_passes_harness_through`
  - `test_from_conversation_populates_local_conversation_fields`

### `app/src/ai/agent_sdk/api_key_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/ai/agent_sdk/api_key.rs` · fork ships source: NO

- **CLOUD** — warp_graphql::mutations::{expire_api_key::ExpireApiKeyResult, generate_api_key::GenerateApiKeyResult}, warp_graphql::queries::api_keys::ApiKeyProperties -- Warp platform API keys (account management), not BYOP LLM keys.
  - `api_key_display_includes_creation_date`
  - `resolve_api_key_identifier_errors_for_ambiguous_name_matches`
  - `resolve_api_key_identifier_errors_when_not_found`
  - `resolve_api_key_identifier_falls_back_to_name_match`
  - `resolve_api_key_identifier_prefers_uid_match`
  - `sort_api_keys_sorts_by_created_at_descending`
  - `sort_api_keys_sorts_by_name_ascending`

### `app/src/ai/agent_sdk/driver/environment_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/ai/agent_sdk/driver/environment.rs` · fork ships source: NO

- **CLOUD** — All 7 construct SourceRepo/CodeForge from crate::ai::cloud_environments -- Warp Environments (#211, declined) -- even though the merge/dedup algorithms themselves are pure.
  - `merge_repos_dedupes_case_insensitively_and_preserves_environment_order`
  - `merge_repos_keeps_distinct_repositories`
  - `merge_repos_rejects_clone_directory_collisions`
  - `merge_repos_supports_additional_only_and_empty_inputs`
  - `parallel_clone_command_runs_repos_in_background_and_waits`
  - `single_repo_name_returns_none_for_zero_or_many_repos`
  - `single_repo_name_returns_repo_when_exactly_one_repo`

### `app/src/ai/agent_sdk/driver/harness/mod_tests.rs` — 7 absent

pin 10 · fork 3 · source `app/src/ai/agent_sdk/driver/harness/mod.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass -- all 7 (auth_check_command_for_* x4, *_runtime_error_patterns_* x3) are pure per-harness trait-default lookups. The fork's own mod_test.rs header already documents this exact gap as issue #289, deliberately not stub-ported to avoid vacuous tests pending real per-harness overrides -- an existing, already-tracked decision, not a fresh finding.
  - `auth_check_command_for_gemini_is_none`
  - `auth_check_command_for_oz_is_none`
  - `auth_check_command_for_unknown_is_none`
  - `auth_check_command_for_unsupported_is_none`
  - `claude_runtime_error_patterns_returns_slice`
  - `codex_runtime_error_patterns_returns_slice`
  - `gemini_runtime_error_patterns_is_empty_by_default`

### `app/src/ai/blocklist/action_model/execute/wait_for_events_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/ai/blocklist/action_model/execute/wait_for_events.rs` · fork ships source: NO

- **CLOUD** — wait_for_events.rs imports `crate::ai::blocklist::orchestration_event_streamer::OrchestrationEventStreamer` (confirmed CLOUD above) directly; `AIAgentActionType::WaitForEvents` is matched only as an unreachable/pass-through proto variant in the fork (`task/helper.rs:136`, `conversation_yaml.rs`), never dispatched to an executor. The 6 `watchdog_timeout_*` tests are pure duration-clamping math and WOULD compile standalone, but porting them would create dead code with no caller -- the executor they belong to cannot exist without the streamer.
  - `execute_invokes_parent_registration_and_honors_child_short_circuit`
  - `watchdog_timeout_clamps_negative_value_to_default_minus_margin`
  - `watchdog_timeout_clamps_to_hard_floor_when_stamped_value_is_too_small`
  - `watchdog_timeout_constants_match_documented_values`
  - `watchdog_timeout_falls_back_to_default_minus_margin_when_unset`
  - `watchdog_timeout_preserves_large_stamped_value`
  - `watchdog_timeout_subtracts_margin_for_stamped_minute`

### `app/src/ai/skills/global_skills_tests.rs` — 7 absent

pin 11 · fork 4 · source `app/src/ai/skills/global_skills.rs` · fork ships source: yes

- **DECLINED** — File physically deleted 2026-08-10 per DECLINED.md #487 (confirmed: global_skills.rs / global_skills_tests.rs both absent from the fork).
  - `filter_skills_by_spec_matches_full_path_specs_for_remote_repos`
  - `filter_skills_by_spec_scopes_simple_remote_names_to_the_repo_host`
  - `resolve_skill_repos_collapses_duplicates_preserving_first_seen_order`
  - `resolve_skill_repos_collects_org_qualified_repos`
  - `resolve_skill_repos_returns_empty_for_empty_input`
  - `resolve_skill_repos_skips_parse_failures`
  - `resolve_skill_repos_skips_unqualified_and_repo_only_specs`

### `app/src/ai/agent_sdk/common_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/agent_sdk/common.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. parse_ambient_task_id and classify_agent_mode_base_model_id are pure local validation (UUID parsing; classify a model id against an already-resolved Vec<LLMId>, no network). Neither exists in the fork's current common.rs (which has already dropped most of the pin's cloud imports).
  - `classify_accepts_id_in_choices_even_when_list_unavailable`
  - `classify_returns_server_unavailable_error_when_list_unavailable`
  - `classify_returns_unknown_id_error_when_list_available_and_id_genuinely_invalid`
  - `parse_ambient_task_id_accepts_valid_ids`
  - `parse_ambient_task_id_preserves_error_prefix`
  - `update_feature_model_choices_clears_unavailable_flag_after_failed_fetch`

### `app/src/ai/agent_sdk/driver/cloud_provider_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/agent_sdk/driver/cloud_provider.rs` · fork ships source: NO

- **CLOUD** — crate::ai::cloud_environments::ProvidersConfig -- AWS/GCP setup for a cloud Environment.
  - `aws_provider_env_vars_before_setup`
  - `collect_provider_env_vars_merges_all_providers`
  - `extract_cloud_providers_creates_aws_provider`
  - `extract_cloud_providers_creates_gcp_provider`
  - `extract_cloud_providers_empty_when_no_providers`
  - `gcp_provider_env_vars`

### `app/src/ai/agent_sdk/runner_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/agent_sdk/runner.rs` · fork ships source: NO

- **CLOUD** — Module absent from fork. Source imports warp_graphql::mutations::upsert_runner, warp_graphql::queries::get_runners, crate::server::server_api::ServerApiProvider -- compute-runner-instance CLI, same family as #290 (RunAgents/cloud-runner orchestration).
  - `confirm_delete_refuses_non_interactive_without_force`
  - `merge_instance_shape_errors_on_partial_shape_without_existing`
  - `merge_instance_shape_updates_dimensions_independently`
  - `resolve_arch_auto_maps_to_os_default`
  - `resolve_arch_explicit_is_preserved_regardless_of_os`
  - `resolve_updated_name_renames_only_with_uid`

### `app/src/ai/mcp/builtin_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/mcp/builtin.rs` · fork ships source: NO

- **CLOUD** — builtin.rs's own doc comment: "Built-in Warp-hosted MCP servers... attached automatically for logged-in users... authenticated with the user's existing session credentials (warp-server accepts both session ID tokens and API keys)"; imports `crate::auth::credentials::Credentials` (Warp account session credentials). Requires a Warp account and Warp's hosted MCP-server infrastructure.
  - `bearer_token_rejects_a_firebase_token_about_to_expire`
  - `bearer_token_rejects_session_cookie_auth`
  - `bearer_token_uses_a_valid_firebase_id_token`
  - `bearer_token_uses_api_keys`
  - `factory_installation_resolves_to_a_preauthenticated_http_server`
  - `factory_mcp_url_joins_server_roots_with_and_without_trailing_slash`

### `app/src/ai/orchestration/config_state_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/orchestration/config_state.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — Same missing `app/src/ai/orchestration/` module -- `OrchestrationConfigState`/`AuthSecretSelection` plain-data state shared by the (nonexistent-in-fork) orchestration config UI.
  - `local_round_trip_preserves_remote_computer_use`
  - `resolve_from_config_preserves_local_claude`
  - `resolve_from_config_sanitizes_disabled_local_codex`
  - `runner_id_round_trips_through_config`
  - `toggle_to_local_preserves_claude`
  - `toggle_to_local_sanitizes_disabled_codex`

### `app/src/ai/ambient_agents/task_tests.rs` — 5 absent

pin 11 · fork 6 · source `app/src/ai/ambient_agents/task.rs` · fork ships source: yes

- **CLOUD** — ambient_agent_task_deserializes_github_webhook_source needs AgentSource::GitHubWebhook (fork's AgentSource enum has no such variant -- all remaining variants are cloud trigger sources) and blocks_cloud_followups() (absent). task_status_error_code_deserializes_* (3) need TaskStatusErrorCode, which doesn't exist -- GraphQL/PublicAPI casing of server error codes.
  - `ambient_agent_task_deserializes_github_webhook_source`
  - `task_status_error_code_deserializes_graphql_casing`
  - `task_status_error_code_deserializes_public_api_casing`
  - `task_status_error_code_deserializes_unknown_codes`
- **MISSING-SUBSYSTEM** — Pure ISO8601-duration deserialization on AmbientAgentTask::run_time(), which exists in the fork -- missing test only, not cloud.
  - `ambient_agent_task_deserializes_run_time_iso8601`

### `app/src/ai/artifact_download_tests.rs` — 5 absent

pin 7 · fork 2 · source `app/src/ai/artifact_download.rs` · fork ships source: yes

- **CLOUD** — Require ArtifactDownloadResponse from crate::server::server_api::ai.
  - `default_download_filename_omits_extension_when_content_type_unknown`
  - `default_download_filename_prefers_server_filename`
  - `default_download_filename_uses_content_type_extension_when_filename_missing`
  - `download_destination_uses_explicit_path`
- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD?: pure content-type-to-extension mapping. Actually needs ArtifactDownloadResponse's content_type field to exist as an input type -- borderline; treating conservatively as missing rather than cloud since the mapping logic itself is pure.
  - `extension_for_content_type_recognizes_image_jpg_alias`

### `app/src/ai/blocklist/action_model/execute/stop_recording_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/ai/blocklist/action_model/execute/stop_recording.rs` · fork ships source: NO

- **DECLINED** — stop_recording.rs imports recording_controller::{RecordingController, StopRecordingControllerError} and recording_finalize::finalize_recording_by_id -- both confirmed DECLINED #350 elsewhere in this sweep (recording_controller_tests.rs, recording_finalize_tests.rs). The StopRecording action executor is part of the same declined computer_use-recording subsystem.
  - `cancelled_result_reports_run_cancelled_from_actual_reason`
  - `error_result_reports_actual_reason_not_claimed_agent_stopped`
  - `happy_path_agent_stopped_still_reports_agent_stopped`
  - `run_ended_joined_by_stop_action_reports_run_ended`
  - `success_result_reports_actual_limit_reached_reason`

### `app/src/ai/blocklist/action_model/execute/upload_artifact_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/ai/blocklist/action_model/execute/upload_artifact.rs` · fork ships source: NO

- **CLOUD** — upload_artifact.rs imports crate::server::server_api::ServerApiProvider and agent_sdk::artifact_upload::{FileArtifactUploadRequest, FileArtifactUploader} -- the latter confirmed CLOUD via agent_sdk/artifact_upload_tests.rs elsewhere in this sweep. Same upload-needs-a-cloud-target cluster as the format_upload_artifact_text_* / converts_upload_artifact_tool_call_to_action / convert_conversation upload-artifact tests found throughout app/src/ai/**.
  - `execute_returns_error_when_conversation_has_not_synced_to_server`
  - `format_upload_artifact_error_keeps_single_layer_errors`
  - `format_upload_artifact_error_preserves_full_error_chain`
  - `resolve_path_uses_active_session_working_directory_for_relative_paths`
  - `should_autoexecute_honors_file_read_permissions_for_resolved_path`

### `app/src/ai/orchestration/validation_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/ai/orchestration/validation.rs` · fork ships source: NO

- **CLOUD** — accept_allowed_for_cloud_harness_with_named_or_inherited_auth, accept_blocked_for_cloud_harness_with_unset_auth_secret, accept_blocked_for_opencode_cloud, accept_allowed_for_oz_local_and_cloud all exercise the cloud-harness branch of should_show_auth_secret_picker/harness_is_selectable (#290).
  - `accept_allowed_for_cloud_harness_with_named_or_inherited_auth`
  - `accept_allowed_for_oz_local_and_cloud`
  - `accept_blocked_for_cloud_harness_with_unset_auth_secret`
  - `accept_blocked_for_opencode_cloud`
- **MISSING-SUBSYSTEM** — Local-only branch of the same missing orchestration/validation.rs module -- real local-orchestration debt.
  - `accept_blocked_for_product_disabled_local_codex`

### `app/src/ai/agent/task_store_tests.rs` — 4 absent

pin 32 · fork 28 · source `app/src/ai/agent/task_store.rs` · fork ships source: yes

- **PORTED** — (already hand-traced in source inventory)
  - `test_prune_unreachable_subtasks_keeps_reachable_subtask`
  - `test_prune_unreachable_subtasks_noop_when_no_subtasks`
  - `test_prune_unreachable_subtasks_removes_nested_orphans`
  - `test_prune_unreachable_subtasks_removes_orphan_and_its_exchanges`

### `app/src/ai/agent_events/message_hydrator_tests.rs` — 4 absent

pin 5 · fork 1 · source `app/src/ai/agent_events/message_hydrator.rs` · fork ships source: yes

- **CLOUD** — STUB-COVERAGE RISK, not just cloud debt. The fork's message_hydrator.rs is a literal no-op stub with its own doc comment: "Zap's local build no longer fetches message bodies from the cloud mailbox or sends delivered receipts. This type is kept for its no-op-compatible call surface." hydrate_event_for_recipient always returns None. Porting these against MockAIClient::expect_read_agent_message (crate::server::server_api::ai::AIClient) would produce exactly the gutted-stub-with-passing-tests case script/check_stub_coverage exists to catch. Correctly excluded.
  - `hydrator_reads_new_message_for_matching_run`
  - `read_message_with_timeout_does_not_retry_permanent_http_failures`
  - `read_message_with_timeout_retries_transient_failures_until_success`
  - `read_message_with_timeout_times_out_after_retrying_transient_failures`

### `app/src/ai/blocklist/action_model/recording_telemetry_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/ai/blocklist/action_model/recording_telemetry.rs` · fork ships source: NO

- **DECLINED** — recording_telemetry.rs's own doc comment: "Telemetry events for the computer-use video recording lifecycle." Directly covered by DECLINED.md #350 (computer_use session recording is declined; recording-adjacent telemetry is part of the same declined subsystem).
  - `contains_no_user_generated_content`
  - `started_payload_shape`
  - `stopped_error_payload_allows_missing_metadata`
  - `stopped_success_payload_shape`

### `app/src/ai/blocklist/agent_view/zero_state_block_tests.rs` — 4 absent

pin 15 · fork 11 · source `app/src/ai/blocklist/agent_view/zero_state_block.rs` · fork ships source: yes

- **DECLINED** — DECLINED.md, "Oz updates" zero-state section row (#321): "ChangelogModel.oz_updates / AISettings::should_show_oz_updates_in_zero_state drive a Warp-branded content feed in the zero state. Not a capability gap -- branded content this fork does not carry."
  - `oz_updates_section_does_not_render_when_feature_flag_is_disabled`
  - `oz_updates_section_does_not_render_when_setting_is_disabled`
  - `oz_updates_section_does_not_render_without_updates`
  - `oz_updates_section_renders_when_all_conditions_are_true`

### `app/src/ai/connected_self_hosted_workers_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/ai/connected_self_hosted_workers.rs` · fork ships source: NO

- **CLOUD** — crate::server::server_api::{ServerApiProvider, ai::ConnectedSelfHostedWorker}, AuthStateProvider, UserWorkspaces -- fetches a Warp team's registered self-hosted workers from Warp's server (despite the "self hosted" name, this is the cloud-team roster, not a local machine).
  - `clear_worker_cache_is_noop_when_empty`
  - `clear_worker_cache_removes_cached_hosts`
  - `worker_hosts_excluding_filters_excluded_host`
  - `worker_hosts_excluding_sorts_dedups_and_filters_empty_and_warp_hosts`

### `app/src/ai/orchestration/remote_child_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/ai/orchestration/remote_child.rs` · fork ships source: NO

- **CLOUD** — remote_child.rs imports `crate::server::server_api::ai::{AgentConfigSnapshot, SpawnAgentRequest}` and `crate::server::server_api::{AIApiError, ClientError, CloudAgentCapacityError}` -- this module prepares and classifies startup errors for a child agent launched on Warp's cloud servers. Unambiguously the cloud-runner half (#290).
  - `capacity_quota_and_fallback_errors_keep_their_semantics`
  - `github_auth_error_is_a_shared_blocker_with_cloud_callback_url`
  - `orchestration_harness_defaults_to_oz_and_parses_known_harnesses`
  - `prepared_remote_request_matches_gui_wire_semantics`

### `app/src/ai/skills/skill_utils_tests.rs` — 4 absent

pin 5 · fork 1 · source `app/src/ai/skills/skill_utils.rs` · fork ships source: yes

- **PORTED** — (already hand-traced in source inventory)
  - `skill_path_from_unix_encoded_remote_location`
  - `skill_path_from_windows_encoded_remote_location`
- **DIVERGENT** — WOULD COMPILE AND FAIL. The fork's dedup key is `(name, dir)` with a priority tiebreak (the P0-3 prompt-cache fix); the pin keys on `(dir, content)`. `test_unique_skills_name_dedup_same_name_different_providers` asserts the fork's behaviour. Never port this one.
  - `test_unique_skills_does_not_dedupe_different_content`
- **DIVERGENT** — duplicate of `test_unique_skills_keeps_same_provider_skills_from_different_dirs`
  - `test_unique_skills_does_not_dedupe_different_dirs`

### `app/src/ai/agent/api/convert_conversation_tests.rs` — 3 absent

pin 15 · fork 12 · source `app/src/ai/agent/api/convert_conversation.rs` · fork ships source: yes

- **CLOUD** — All 3 need `convert_conversation_data_to_ai_conversation`, which does not exist in the fork's `convert_conversation.rs` (grep: zero hits). It restores a conversation from server-fetched `api::ConversationData` (with `ambient_agent_task_id`, `server_token`, `RestorationMode`) -- reconstructing a conversation that was previously running on Warp's servers. Same cluster as this file's already-CLOUD? majority.
  - `test_convert_conversation_data_to_ai_conversation_sets_restored_run_id`
  - `test_convert_tool_call_result_to_input_upload_artifact_missing_result_is_error`
  - `test_convert_tool_call_result_to_input_upload_artifact_success`

### `app/src/ai/agent_sdk/config_file_tests.rs` — 3 absent

pin 15 · fork 12 · source `app/src/ai/agent_sdk/config_file.rs` · fork ships source: yes

- **CLOUD** — The fork's `mcp_specs_from_mcp_servers` (`config_file.rs:103`) REJECTS a non-UUID `warp_id` (`uuid::Uuid::parse_str(warp_id).map_err(...)`). The pin's version instead forwards it as `MCPSpec::WellKnown(id)` -- and the pin test's own comment says why: "The server owns the set of recognized ids: the client forwards any non-UUID warp_id for resolution instead of rejecting it." Needs server-side well-known-MCP-id resolution the fork has no client for, plus a `FeatureFlag::WellKnownMcpIds` that doesn't exist.
  - `any_non_uuid_warp_id_becomes_well_known_spec`
  - `empty_warp_id_is_rejected`
  - `well_known_warp_id_converts_to_well_known_spec`

### `app/src/ai/agent_sdk/driver/cache_setup_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/agent_sdk/driver/cache_setup.rs` · fork ships source: NO

- **CLOUD** — Module absent. Imports cloud_object_models::{CodeForge,SourceRepo} and warp_isolation_platform::IsolationPlatformType::Namespace (a cloud sandbox isolation type). build_export_command (shell-escaping) is pure but has no caller outside this cloud-runner-cache feature.
  - `export_commands_use_active_shell_syntax_and_escaping`
  - `gate_matrix_requires_namespace_and_nonempty_root`
  - `source_repo_maps_to_canonical_identity_and_checkout`

### `app/src/ai/agent_sdk/driver/cloud_provider/gcp_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/agent_sdk/driver/cloud_provider/gcp.rs` · fork ships source: NO

- **CLOUD** — Same cloud_provider::gcp module as driver/cloud_provider_tests.rs -- AWS/GCP Environment provisioning.
  - `best_effort_command_completes_within_timeout`
  - `best_effort_command_is_killed_on_timeout`
  - `best_effort_command_missing_binary_is_not_found`

### `app/src/ai/agent_sdk/driver/harness/codex_tests.rs` — 3 absent

pin 38 · fork 35 · source `app/src/ai/agent_sdk/driver/harness/codex.rs` · fork ships source: yes

- **CLOUD** — All 3 (fetch_resume_payload_maps_404_to_resume_state_missing, _maps_other_errors_to_load_failed, _returns_codex_variant_on_success) mock crate::server::server_api::harness_support::MockHarnessSupportClient. Fork's own codex.rs header already documents fetch_resume_payload as needing cloud HarnessSupportClient.
  - `fetch_resume_payload_maps_404_to_resume_state_missing`
  - `fetch_resume_payload_maps_other_errors_to_load_failed`
  - `fetch_resume_payload_returns_codex_variant_on_success`

### `app/src/ai/agent_sdk/retry_tests.rs` — 3 absent

pin 11 · fork 8 · source `app/src/ai/agent_sdk/retry.rs` · fork ships source: NO

- **CLOUD** — retry.rs is a 15-line re-export shim: `pub(crate) use crate::server::retry_strategies::with_bounded_retry;` plus, test-only, `is_transient_graphql_or_http_error`. The 3 absent tests (`graphql_status_classifier_*`) test the GraphQL-specific classifier specifically. The fork has no `app/src/ai/agent_sdk/retry.rs` at all, and the GraphQL classifier it would need to re-export doesn't apply to a GraphQL-less client.
  - `graphql_status_classifier_fails_fast_on_permanent_statuses`
  - `graphql_status_classifier_fails_fast_without_typed_transport_error`
  - `graphql_status_classifier_retries_transient_statuses`

### `app/src/ai/blocklist/action_model/recording_finalize_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/blocklist/action_model/recording_finalize.rs` · fork ships source: NO

- **DECLINED** — This is action_model/recording_finalize.rs (SCREEN recording block finalize) -- a different subsystem from computer_use's #350. Matches DECLINED.md's #367 row exactly ("Nothing to remove: the subsystem was never ported"). Imports crate::server::server_api::ServerApiProvider and crate::ai::agent_sdk::artifact_upload directly -- the upload leg #367 calls out as the cloud-touching piece.
  - `agent_discard_finalization_skips_upload`
  - `cancellation_finalization_skips_upload_even_without_actions`
  - `empty_actions_finalization_is_an_error_without_upload`

### `app/src/ai/blocklist/controller_tests.rs` — 3 absent

pin 6 · fork 3 · source `app/src/ai/blocklist/controller.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass, all 3. cancelling_conversation_aborts_pending_auto_resume, mock_response_stream_updates_history_through_controller, optimistic_cli_subagent_completion_with_in_flight_stream_reports_success all operate on local conversation-controller/mock-response-stream mechanics (BlocklistAIHistoryModel, ResponseStreamId::new_for_test()) with no server/cloud symbol referenced in their bodies.
  - `cancelling_conversation_aborts_pending_auto_resume`
  - `mock_response_stream_updates_history_through_controller`
  - `optimistic_cli_subagent_completion_with_in_flight_stream_reports_success`

### `app/src/ai/blocklist/queued_query_tests.rs` — 3 absent

pin 42 · fork 39 · source `app/src/ai/blocklist/queued_query.rs` · fork ships source: yes

- **DIVERGENT** — Already hand-traced in source inventory: already ported as `clear_conversations_in_terminal_view_drops_every_listed_conversation`, matching the fork's method name. Carried forward as-is.
  - `clear_conversations_for_terminal_surface_drops_every_listed_conversation`
- **COVERED-ELSEWHERE** — Fork's `locked_head_rejects_user_mutations_and_autofire` (`queued_query_tests.rs:86`) is functionally identical -- same append/reorder/peek_autofire assertions -- just against the fork's renamed `PendingLrcAutoQueue` origin instead of the pin's `initial_cloud_mode_query`. Fork's `remove_pending_lrc_rows_removes_only_pending_rows_and_emits_removed` + `remove_pending_lrc_rows_no_ops_when_no_pending_rows` (lines 997, 1027) cover the second test's semantics via the renamed `remove_pending_lrc_rows` (plural) API.
  - `initial_cloud_mode_head_rejects_user_mutations_and_autofire`
  - `remove_initial_cloud_mode_row_only_removes_the_locked_head`

### `app/src/ai/cloud_environments/catalog_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/cloud_environments/catalog.rs` · fork ships source: NO

- **DECLINED** — DECLINED.md, "Warp Environments" row (#211): "Cloud-backed. 75 pinned tests are out of scope, not parity debt." cloud_environments/catalog.rs is the Warp Environments catalog by name.
  - `default_resolution_preserves_each_gui_consumer_name_tie_breaker`
  - `environment_creation_refreshes_after_cloud_model_inserts_the_object`
  - `environment_timestamp_updates_refresh_recency_order`

### `app/src/ai/document/ai_document_model_tests.rs` — 3 absent

pin 15 · fork 12 · source `app/src/ai/document/ai_document_model.rs` · fork ships source: yes

- **CLOUD** — cloud_model_sync_event_reconciles_stale_document_client_id, publish_refreshes_pending_saving_document_content use SyncId::ClientId/pending_document_queue.
  - `cloud_model_sync_event_reconciles_stale_document_client_id`
  - `publish_refreshes_pending_saving_document_content`
- **MISSING-SUBSYSTEM** — Pure markdown round-trip test; create_document/get_document_content already exist in the fork's cloud-stripped ai_document_model.rs -- missing test only.
  - `test_plan_markdown_content_preserves_copyable_structure`

### `app/src/ai/execution_profiles/config_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/execution_profiles/config.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. All 3 (context_window_limit_schema_has_description, file_collection_round_trips_multiple_profiles, file_collection_rejects_invalid_values_as_a_unit) are pure local schema/serialization tests for ExecutionProfilesConfig/ExecutionProfileId. The one cloud-adjacent import (crate::server::ids::ServerId) feeds only a one-time migration helper not exercised by these 3. Neither type exists anywhere in the fork -- a genuine missing local file-backed-config subsystem.
  - `context_window_limit_schema_has_description`
  - `file_collection_rejects_invalid_values_as_a_unit`
  - `file_collection_round_trips_multiple_profiles`

### `app/src/ai/mcp/file_based_manager_tests.rs` — 3 absent

pin 12 · fork 9 · source `app/src/ai/mcp/file_based_manager.rs` · fork ships source: yes

- **CLOUD** — Both need `handle_cloud_environment_scan_complete` -- absent from the fork's `file_based_manager.rs` (grep: zero hits). Cloud-environment MCP auto-detection, same family as `cloud_environments/catalog_tests.rs` (#211).
  - `test_auto_started_cloud_scan_uuids_are_in_wait_set`
  - `test_project_scoped_cloud_scan_has_detected_servers_but_empty_wait_set`
- **DIVERGENT** — Already hand-traced in source inventory: the fork's `FileBasedMCPManagerEvent` has no `ServersChanged` variant. Carried forward as-is.
  - `servers_changed_only_emits_for_effective_source_set_changes`

### `app/src/ai/skills/file_watchers/utils_tests.rs` — 3 absent

pin 23 · fork 20 · source `app/src/ai/skills/file_watchers/utils.rs` · fork ships source: yes

- **PORTED** — (already hand-traced in source inventory)
  - `find_skill_files_in_tree_empty_repo`
  - `find_skill_files_in_tree_finds_root_skills`
  - `find_skill_files_in_tree_finds_subdirectory_skills`

### `app/src/ai/agent/conversation_yaml_tests.rs` — 2 absent

pin 8 · fork 6 · source `app/src/ai/agent/conversation_yaml.rs` · fork ships source: yes

- **CLOUD** — Same upload-artifact-needs-a-cloud-target cluster confirmed above.
  - `upload_file_artifact_tool_call_result_serializes_only_supported_success_fields`
- **DECLINED** — Already correctly bucketed by mechanical pass: RunAgents / #325.
  - `run_agents_result_serializes_agent_ids`

### `app/src/ai/agent_sdk/admin_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/ai/agent_sdk/admin.rs` · fork ships source: yes

- **DECLINED** — Both (multiple_teams_include_workspace_and_repeat_pretty_team_labels, single_team_omits_admin_visible_non_member_teams) are multi-team/org listing, matching DECLINED.md's #445 cloud-teams entry. Fork's admin.rs ships only whoami.
  - `multiple_teams_include_workspace_and_repeat_pretty_team_labels`
  - `single_team_omits_admin_visible_non_member_teams`

### `app/src/ai/agent_sdk/mcp_config_tests.rs` — 2 absent

pin 18 · fork 16 · source `app/src/ai/agent_sdk/mcp_config.rs` · fork ships source: yes

- **CLOUD** — Same `MCPSpec::WellKnown`/well-known-id server-resolution cluster as config_file_tests.rs above.
  - `well_known_spec_is_coerced_to_warp_id`
  - `well_known_warp_id_passes_validation`

### `app/src/ai/blocklist/action_model/execute/ask_user_question_tests.rs` — 2 absent

pin 7 · fork 5 · source `app/src/ai/blocklist/action_model/execute/ask_user_question.rs` · fork ships source: yes

- **DECLINED** — ask_user_question auto-approve divergence (#373), DECLINED.md
  - `execute_returns_sync_skipped_question_ids_when_autoapprove_is_enabled`
  - `should_autoexecute_returns_true_when_autoapprove_is_enabled_and_profile_allows_override`

### `app/src/ai/blocklist/action_model/execute/read_skill_tests.rs` — 2 absent

pin 7 · fork 5 · source `app/src/ai/blocklist/action_model/execute/read_skill.rs` · fork ships source: yes

- **DIVERGENT** — CODE DEFECT, not just a test gap. The pin's `ReadSkillExecutor` holds a `ModelHandle<ActiveSession>` and resolves `SessionContext::from_session(...).skill_path_origin()` before calling `SkillManager::active_skill_by_reference_with_origin` -- so a remote (SSH) session reads the REMOTE host's bundled-skill catalog. The fork's `ReadSkillExecutor::new()` takes NO session parameter and calls the origin-agnostic `active_skill_by_reference` unconditionally (`read_skill.rs:26-45`; call site `action_model/execute.rs:302`). The fork ALREADY HAS everything the fix needs -- `active_skill_by_reference_with_origin` (`skill_manager.rs:435`), `SessionContext::skill_path_origin` (`blocklist/controller.rs:158`), `ActiveSession` -- this is a "ported but never wired" class defect (matches HANDOFF.md's named recurring-defect class), not a missing feature. NOT fixed in this pass: the constructor-signature change touches 9 existing passing tests in the same file with no compiler to verify against, and `AGENTS.md` requires an issue before a fix lands. Reported here as the top defect finding; recommend filing an issue and fixing `ReadSkillExecutor::new`/`execute` plus its 9 call sites together in a follow-up with build access.
  - `disconnected_remote_session_does_not_fall_back_to_client_global_bundled_skill`
  - `remote_session_reads_remote_bundled_skill_catalog`

### `app/src/ai/blocklist/action_model/execute/send_message_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/ai/blocklist/action_model/execute/send_message.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. Both (sender_run_id_and_task_id_for_send_falls_back_to_ambient_task_id, _prefers_conversation_task_id) are pure local state resolution, no cloud symbols. Fork's send_message.rs already uses warp_cli::agent_mailbox (the reversed local mailbox, DECLINED.md's SendMessageToAgent row), but the sender_run_id_and_task_id_for_send helper itself doesn't exist yet -- genuine missing local debt, now buildable.
  - `sender_run_id_and_task_id_for_send_falls_back_to_ambient_task_id`
  - `sender_run_id_and_task_id_for_send_prefers_conversation_task_id`

### `app/src/ai/blocklist/input_model_tests.rs` — 2 absent

pin 14 · fork 12 · source `app/src/ai/blocklist/input_model.rs` · fork ships source: yes

- **DIVERGENT** — named in `input_model_tests.rs:25-33`: needs `BlocklistAIInputModel` to subscribe to `ConversationSelectionEvent`, which the GUI has no implementation for
  - `conversation_events_with_inert_policy_leave_config_unchanged`
- **DIVERGENT** — same comment, named verbatim
  - `conversation_events_apply_policy_updates`

### `app/src/ai/blocklist/permissions_tests.rs` — 2 absent

pin 25 · fork 23 · source `app/src/ai/blocklist/permissions.rs` · fork ships source: yes

- **DECLINED** — test_get_org_execute_commands_denylist, test_merged_denylist_deduplication both test the org/workspace denylist that DECLINED.md #445 says is permanently inert (UserWorkspaces::current_team() always returns None).
  - `test_get_org_execute_commands_denylist`
  - `test_merged_denylist_deduplication`

### `app/src/ai/blocklist/usage/conversation_usage_view_tests.rs` — 2 absent

pin 3 · fork 1 · source `app/src/ai/blocklist/usage/conversation_usage_view.rs` · fork ships source: yes

- **DIVERGENT** — CODE DEFECT. `app/src/ai/blocklist/usage/conversation_usage_view.rs:502`: `fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {}` -- a literal no-op. The struct also has no `details_expanded`/`show_all_clicked` fields at all. The pin's `ConversationUsageView` toggles an expand/collapse state on "View details"/"Show N more" clicks; the fork's `TypedActionView` impl is wired (so the specific bug the PIN TEST FILE's own doc-comment describes -- "view created via add_view instead of add_typed_action_view" -- is already fixed) but the handler itself discards every action, so those affordances still do nothing. This is real, currently-invisible-to-any-test dead UI in the fork. NOT fixed in this pass -- needs new state fields plus render-logic changes to `render_unified_layout`, which I could not verify without a compiler. Reported as a defect finding.
  - `show_all_agent_rows_is_independent_of_details_expanded`
  - `toggle_details_expanded_flips_state_and_resets_show_all_on_collapse`

### `app/src/ai/agent/api/convert_from_tests.rs` — 1 absent

pin 5 · fork 4 · source `app/src/ai/agent/api/convert_from.rs` · fork ships source: yes

- **CLOUD** — `converts_upload_artifact_tool_call_to_action` needs `extract_upload_artifact_action`/`UploadFileArtifact` handling in `convert_from.rs` -- absent (only unreachable exhaustive-match arms exist elsewhere, e.g. `convert_conversation.rs:1462: ToolType::UploadFileArtifact(_) => return None`). Same upload-artifact cluster confirmed CLOUD via `agent_sdk/artifact_upload_tests.rs` and `agent_sdk/artifact_tests.rs` (needs a GraphQL upload target / cloud task metadata).
  - `converts_upload_artifact_tool_call_to_action`

### `app/src/ai/agent_sdk/driver/harness/claude_code/wake_driver_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/agent_sdk/driver/harness/claude_code/wake_driver.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. local_wake_task_state_ready_allows_success_and_stale_in_progress is a pure AmbientAgentTaskState predicate (is_local_wake_task_state_ready); the enum already exists identically in the fork's task.rs:361. Sits in a module (wake_driver.rs) with cloud imports elsewhere, but this specific test touches none of them.
  - `local_wake_task_state_ready_allows_success_and_stale_in_progress`

### `app/src/ai/agent_sdk/driver/terminal_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/agent_sdk/driver/terminal.rs` · fork ships source: yes

- **CLOUD** — extend_shared_session_retention_emits_event_for_active_sharer exercises session_sharing_protocol::sharer::SessionRetentionReason / SharedSessionStatus::ActiveSharer -- agent session sharing (oz agent run --share) needs Warp's backend to host the share.
  - `extend_shared_session_retention_emits_event_for_active_sharer`

### `app/src/ai/blocklist/action_model/execute/read_documents_tests.rs` — 1 absent

pin 2 · fork 1 · source `app/src/ai/blocklist/action_model/execute/read_documents.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — The fork's `ReadDocumentsExecutor::execute` (`read_documents.rs`) reads only synchronously from the local `AIDocumentModel` and reports any missing document as a hard error. The pin additionally attempts to lazily hydrate a missing plan document for a remote-child conversation (walking `parent_agent_id`) before failing. The fork already has the building blocks this would need (`AIAgentConversation::set_parent_agent_id`, `BlocklistAIHistoryModel::start_new_conversation`), so this is buildable, but the hydration path itself was never written -- a genuine, if small, feature gap in the now-in-scope local-orchestration family.
  - `execute_lazily_hydrates_missing_plan_for_remote_child_without_local_parent`

### `app/src/ai/blocklist/agent_view/conversation_selection_tests.rs` — 1 absent

pin 5 · fork 4 · source `app/src/ai/blocklist/agent_view/conversation_selection.rs` · fork ships source: yes

- **DIVERGENT** — CODE DEFECT. `app/src/ai/conversation_entry.rs:90-94` defines `AgentConversationListEntryState::{Selected, OpenElsewhere, Available, Unavailable}` -- the `Unavailable` variant exists -- but the fork's `classify_gui_list_entry` (`blocklist/agent_view/conversation_selection.rs:99`) only ever returns the first three; it never returns `Unavailable` and takes 4 params vs. the pin's 5 (missing a predicate closure the pin uses to decide unavailability). Single call site in the same file (`classify_entry` impl), so the blast radius of a fix is small, but I could not determine what should make a GUI entry "unavailable" (a disconnected remote host? an unattachable session?) without deeper tracing than time allowed, and did not want to guess at business logic for an unverifiable fix. Reported as a defect finding, not fixed.
  - `gui_list_policy_classifies_unavailable_entry`

### `app/src/ai/blocklist/context_model_tests.rs` — 1 absent

pin 15 · fork 14 · source `app/src/ai/blocklist/context_model.rs` · fork ships source: yes

- **DECLINED** — DECLINED.md, "has_locking_attachment" row (#318): "DECIDED 2026-08-07: keep the fork's behaviour... The pin's has_locking_attachment_is_false_with_only_pending_block_id is permanently not ported; the fork's has_locking_attachment_is_true_with_pending_block_id is the authority."
  - `has_locking_attachment_is_false_with_only_pending_block_id`

### `app/src/ai/blocklist/handoff/touched_repos_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/handoff/touched_repos.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. find_git_root_walks_up_to_dot_git is pure filesystem git-root-walk, no cloud symbols. The containing handoff/ module (which does import cloud_environments/cloud_object elsewhere) doesn't exist in the fork at all, so this standalone-portable local utility is simply missing.
  - `find_git_root_walks_up_to_dot_git`

### `app/src/ai/blocklist/inline_action/ask_user_question_view_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/inline_action/ask_user_question_view.rs` · fork ships source: yes

- **COVERED-ELSEWHERE** — Fork's `view_state_shows_other_input` (`ask_user_question_view_tests.rs:457-495`, same file) exercises the identical state machine and assertions (open other input -> `show_other_input: true`, then `SaveOtherText` + `NavigateNext` -> `show_other_input: false` again for the next question) that this pin test checks, just via `AskUserQuestionAction::OpenOtherInput` instead of the pin's `EnterCustomAnswerEditing` (a rename, not a missing feature).
  - `view_state_shows_other_input_only_for_the_current_question`

### `app/src/ai/blocklist/inline_action/create_environment_modal_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/inline_action/create_environment_modal.rs` · fork ships source: NO

- **DECLINED** — create_environment_modal.rs imports `crate::settings_view::handoff_environment_creation_modal::{HandoffEnvironmentCreationModal, ...}` -- creates a Warp Environment (cloud). Covered by DECLINED.md #211 (Warp Environments).
  - `test_create_environment_modal_uses_orchestration_form_configuration`

### `app/src/ai/blocklist/inline_action/orchestration_controls_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/inline_action/orchestration_controls.rs` · fork ships source: NO

- **CLOUD** — runner_controls_require_both_feature_flag_and_experiment_arm directly asserts on FeatureFlag::CloudAgentRunners + ServerExperiment::MacosRunnersExperiment/ServerExperiments -- DECLINED.md's #290 row names orchestration_controls explicitly.
  - `runner_controls_require_both_feature_flag_and_experiment_arm`

### `app/src/ai/get_relevant_files/remote_search/native_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/get_relevant_files/remote_search/native.rs` · fork ships source: NO

- **MISSING-SUBSYSTEM** — Mis-bucketed CLOUD? by the mechanical pass. file_contents_from_response_keeps_only_whole_text_files operates only on remote_server::proto::{ReadFileContextResponse, FileContextProto} -- Phosphor's own SSH remote-host daemon protocol, explicitly listed in DECLINED.md's false-positive list ("not Warp's cloud backend, despite the name"). app/src/ai/get_relevant_files/ doesn't exist at all in the fork -- missing subsystem, not cloud.
  - `file_contents_from_response_keeps_only_whole_text_files`

### `app/src/ai/mcp/file_mcp_watcher_tests.rs` — 1 absent

pin 4 · fork 3 · source `app/src/ai/mcp/file_mcp_watcher.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Needs `FileMCPWatcher::parse_abort_handles`/`abort_config_parse` -- the fork's `FileMCPWatcher` struct (`file_mcp_watcher.rs:123-131`) has no such field or method (only `file_mcp_tx`, `home_provider_watchers`, `project_repo_watchers`). In-flight MCP-config-parse cancellation was never ported.
  - `abort_config_parse_cancels_and_removes_inflight_task`

### `app/src/ai/skills/bundled_tests.rs` — 1 absent

pin 2 · fork 1 · source `app/src/ai/skills/bundled.rs` · fork ships source: yes

- **MISSING-SUBSYSTEM** — Needs `display_optional_path` -- not present in the fork's `bundled.rs` (grep: zero hits).
  - `unavailable_bundled_context_path_renders_as_empty_string`

---

## What was ported

Three tests, appended to the existing `app/src/ai/skills/skill_manager_tests.rs`
(fork ships this module; 22 of 25 pin tests were already present or already
adjudicated):

- `remote_home_provider_variants_are_available_for_provider_selection`
- `remote_home_provider_variants_are_scoped_to_the_descriptor_host`
- `remote_home_skill_replaces_an_overlapping_index_entry`

All three needed only an adjustment to `set_remote_home_skills`'s call
signature (the fork's version takes an extra `ctx: &mut ModelContext<Self>`
the pin's 3-arg call doesn't); every other symbol they use
(`get_skills_for_working_directory_with_origin`, `skill_exists_for_any_provider`,
`best_supported_provider`, the `remote_test_path`/`make_remote_home_skill`
test helpers) already existed verbatim in the fork.

`rustfmt --check --config-path .rustfmt.toml --edition 2024` on the changed
file reports the same pre-existing violations the file had before this
change (verified via `git stash`) and nothing new. `script/check_cloud_boundary`
passes (271 allowlisted import sites, unchanged).

## Ranked: what I am least sure compiles

Most likely to have a mistake first, most specific expression named:

1. **The `ctx` parameter threading in the two multi-call tests.**
   `remote_home_provider_variants_are_scoped_to_the_descriptor_host` calls
   `manager.set_remote_home_skills(...)` twice inside one
   `handle.update(&mut app, |manager, ctx| { ... })` closure, reusing the same
   `ctx` for both calls. I did not find a second existing fork test doing two
   `set_remote_home_skills` calls in one closure to confirm this pattern
   compiles as written (single-call precedent only, at the end of the same
   file).
2. **`home_dir.join("repo")` in `remote_home_skill_replaces_an_overlapping_index_entry`.**
   I confirmed `LocalOrRemotePath::join` exists
   (`crates/warp_util/src/local_or_remote_path.rs:130`) but did not check its
   exact return type / whether it takes `&str` vs `impl AsRef<str>` against
   this call site's usage (`home_dir.join("repo")` where `home_dir` is a
   `LocalOrRemotePath` built via `remote_test_path`).
3. **Field-mutation access on `manager.directory_skills` / `manager.skills_by_path`
   in the third test.** Both are private (`HashMap`, no `pub`) fields on
   `SkillManager`; I relied on `use super::*;` plus same-crate test-module
   placement to make this legal, matching the pin's own test, but did not
   independently re-verify visibility rules against the fork's actual
   `#[cfg(test)] #[path = ...] mod tests;` wiring for this specific file
   (only confirmed the include exists, not that private-field access from it
   has ever been exercised elsewhere in this file for a *struct-literal
   field-by-field mutation* rather than a method call).
4. **Every DIVERGENT/MISSING-SUBSYSTEM/CLOUD verdict above that flags an
   existing fork *test* as covering a pin test "COVERED-ELSEWHERE."** These
   are read-only findings (no code changed), so they carry no compile risk,
   but the six COVERED-ELSEWHERE citations were made by reading fork test
   *source*, not by running them — if any of those six fork tests are
   themselves currently red for an unrelated reason, this document's citation
   would be misleading in a way a build would catch immediately and this
   review could not.

## What's left unadjudicated at full rigor

The two background research agents' 56-file corroboration pass used the same
import-reading method I used directly on the other 42 files, but
I did not independently re-verify every one of their 56 file-level claims
line-by-line the way I did for the ~35 files I read myself — I checked their
method (both were told to quote imports verbatim and to re-check current-tree
module presence rather than trust the stale inventory) and spot-checked
several, but a wrong quote from either agent would propagate uncaught. Treat
the ~50 files whose verdicts above are attributed only to a research agent
(not corroborated by my own `git show` in this session) as one notch less
certain than the rest.
