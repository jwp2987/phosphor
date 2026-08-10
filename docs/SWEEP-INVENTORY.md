# Sweep inventory — pin tests with no fork equivalent

Oracle: **`02b53fcd8`** (Warp `2026.07.29.09.05` stable), the pin in `ORACLE.md`.
Fork side: `main` @ `2b072ec61`. Produced for the **#2 sweep** (`TODO.md`).

This file is the *derived* answer to "which pinned tests are missing, and which of
them are worth porting". It is mechanical where it can be and hand-traced where it
must be. **Read the caveats before quoting a number.**

## Method

1. Test functions are counted by attribute, not filename: `#[test]`, `#[tokio::test]`,
   `#[gpui::test]`, `#[warpui::test]`, `#[rstest]`, `#[test_case(..)]`. Running that
   counter over the pin reproduces `ORACLE.md` exactly — **854 test-bearing files,
   10,120 test functions** (`ORACLE.md` says 854 / 10,123).
2. A pin test counts as present when a function of that name exists **anywhere** in
   the fork tree, so the fork's `*_tests.rs` → `*_test.rs` rename and its
   `a/b/c_tests.rs` → `a/b_c_tests.rs` flattening are invisible to the result.
3. Each absent test is bucketed from the pin source's imports, the test body's own
   symbols, whether the fork ships the module at all, and `DECLINED.md`.

Fork side at this commit: **842 test-bearing files, 9,716 test functions** (up from
7,884 when `ORACLE.md` was written on 2026-08-06 — roughly 1,800 tests have landed
since, and the absent count has fallen from 3,902 to 2,357).

## Totals

**2357 pinned tests have no fork test of the same name.**

| bucket | tests | of which hand-traced |
|---|---:|---:|
| PORTED | 17 | 17 |
| PORTABLE? | 370 | 0 |
| CLOUD? | 1219 | 0 |
| DECLINED? | 340 | 0 |
| DIVERGENT? | 345 | 0 |
| DIVERGENT | 63 | 63 |
| CLOUD | 1 | 1 |
| DECLINED | 2 | 2 |
| **total** | **2357** | **83** |

A bucket name ending in `?` is the **mechanical** verdict: derived from imports and
module presence, not from reading the test. A bucket without `?` was traced by hand
in this pass and is quotable. `PORTED` means this sweep ported it.

## Two caveats that change the numbers

**1. Name matching over-reports, badly.** Roughly a quarter of the "missing" tests
are not missing in any useful sense. Three distinct causes, all seen in this pass:

- *Renamed with the code.* The pin's `detect_possible_local_git_repo` is this fork's
  `detect_possible_git_repo`, and its three tests were renamed to match. Name-diff
  calls all three absent.
- *Replaced by a documented analogue.* `pty_controller_tests.rs` carries four
  current-API tests, each with a comment naming the orphaned pin test it replaces.
- *Same filename, different module.* The pin has both
  `server/telemetry/secret_redaction.rs` and `ai/blocklist/block/secret_redaction.rs`;
  the fork ships only the second. A basename match reports the dropped telemetry one
  as present, and its 18 tests as portable debt. They are neither.

A cheap detector for the second case: **566 of the 2357 absent test names
already appear verbatim somewhere in the fork tree's prose** — a provenance comment,
a port-audit header, or a `DECLINED.md` row. Grep the fork for a pin test's name
before concluding anything about it. (`grep -w` is *not* enough to decide presence,
for exactly this reason: use `grep -E 'fn <name>\b'` for presence and the plain name
for adjudication.)

**2. `SCOPE-*.md`'s verdict A is overstated, and this pass found the mechanism.**
`SCOPE-*.md` collapses a MIXED file to its majority bucket, so a verdict-A file can
be mostly unportable. But the larger error runs the other way: verdict A only asks
whether the fork *ships a file of that name*. It does not ask whether that file is
the same module (`diff_state_tracker.rs`), whether the API under test still exists
(`pty_controller.rs`), or whether the fork deliberately inverted the behaviour
(`unique_skills`). Every one of those reads as straight debt and is not.

## Findings that are not test debt

Four gaps surfaced by doing this that are **feature** gaps, non-cloud, and
user-visible. None is tracked as such today.

1. **MCP tool results render as a JSON text blob, not a tree.** The pin normalises a
   `CallMCPToolResult` through `mcp_result_to_renderable` into `McpRenderable::{Tree,
   Error, Cancelled}` and renders `Tree` with the collapsible `json_tree` component.
   This fork has neither symbol and falls back to
   `serde_json::to_string_pretty(result)` (`requested_command.rs:1494`). The fork's
   own `json_tree_tests.rs:303` notes the five pin tests are unported *because the
   call-site code is missing* — that is the gap, not the tests.
2. **No `/index` slash command — indexing can only happen automatically.** The pin
   registers `INDEX` (`/index`, "Index this codebase", GUI-only) in
   `static_commands/commands.rs`, gated on
   `REPOSITORY | CODEBASE_CONTEXT | AI_ENABLED`. The indexing subsystem itself was
   restored here on the D2 track and its gate exists
   (`UserWorkspaces::is_codebase_context_enabled`, consulted by
   `ai/codebase_auto_indexing.rs`), but the fork's `Availability` bitflags have no
   `CODEBASE_CONTEXT` variant (bit 6 is unused) and `commands.rs` has no `INDEX`. So
   a repository is indexed when the auto-indexing gates say so and there is no way
   for a user to ask for it. Three pin tests hang off this
   (`codebase_context_requirement_{satisfied_when_enabled,not_satisfied_when_disabled}`,
   `index_command_requires_repo_and_codebase_context`); they are the symptom, not the gap.
3. **TUI selection cannot trim trailing whitespace or select a styled word.** The two
   unported `read_only_menu` tests need `TuiViewportedList::with_trimmed_selection_line_ends`
   and `TuiSelectable::with_semantic_selection_by_style` in `warpui_core`. Today a
   drag-select in a read-only menu copies the row's padding, and a double-click does
   not select a value that spans two styles.
4. **`languages::language_by_filename` has no `StandardizedPath` overload.** The pin
   splits local (`&Path`) from standardized (`&StandardizedPath`) resolution; the fork
   kept only the local one. Remote files therefore resolve their language through a
   host-local `Path`. Benign for POSIX remotes on a POSIX client; unverified for a
   Windows remote.

## Per-file inventory

Ordered by number of absent tests. `pin` / `fork` are test counts for that pin file
(`fork` = how many of its test names exist somewhere in the fork).

### `crates/warp_cli/src/lib_tests.rs` — 100 absent

pin 126 · fork 26 · source `crates/warp_cli/src/lib.rs` · fork ships source: yes

- **DECLINED?** — /logout slash command (#338)
  - `logout_parses`  _(named in fork prose)_
- **DECLINED?** — Account-first onboarding (#11)
  - `login_parses`  _(named in fork prose)_
- **PORTABLE?**
  - `agent_create_accepts_prompt`  _(named in fork prose)_
  - `agent_run_accepts_skill_and_task_id`  _(named in fork prose; near-name fork test `agent_run_accepts_prompt_and_skill`)_
  - `agent_run_accepts_skip_initial_turn_with_task_id_and_idle_on_complete`  _(named in fork prose)_
  - `agent_run_accepts_snapshot_flags`  _(named in fork prose)_
  - `agent_run_accepts_task_id_only`  _(named in fork prose)_
  - `agent_run_accepts_task_id_with_conversation_for_worker_followups`  _(named in fork prose)_
  - `agent_run_cloud_accepts_agent_flag`  _(named in fork prose)_
  - `agent_run_cloud_accepts_claude_auth_secret_with_harness`  _(named in fork prose)_
  - `agent_run_cloud_accepts_computer_use_flag`  _(named in fork prose; near-name fork test `agent_run_accepts_computer_use_flag`)_
  - `agent_run_cloud_accepts_model`  _(named in fork prose; near-name fork test `agent_run_accepts_model`)_
  - `agent_run_cloud_accepts_no_computer_use_flag`  _(named in fork prose; near-name fork test `agent_run_accepts_no_computer_use_flag`)_
  - `agent_run_cloud_accepts_run_ambient_alias`  _(named in fork prose)_
  - `agent_run_cloud_accepts_snapshot_flags`  _(named in fork prose)_
  - `agent_run_cloud_claude_auth_secret_without_harness_parses`  _(named in fork prose)_
  - `agent_run_cloud_defaults_to_no_computer_use_override`  _(named in fork prose; near-name fork test `agent_run_defaults_to_no_computer_use_override`)_
  - `agent_run_cloud_rejects_both_computer_use_flags`  _(named in fork prose; near-name fork test `agent_run_rejects_both_computer_use_flags`)_
  - `agent_run_rejects_file_and_task_id`  _(named in fork prose)_
  - `agent_run_rejects_prompt_and_task_id`  _(named in fork prose; near-name fork test `agent_run_rejects_prompt_and_saved_prompt`)_
  - `agent_run_rejects_saved_prompt_and_task_id`  _(named in fork prose; near-name fork test `agent_run_rejects_prompt_and_saved_prompt`)_
  - `agent_run_rejects_skip_initial_turn_without_idle_on_complete`  _(named in fork prose)_
  - `agent_run_rejects_skip_initial_turn_without_task_id`  _(named in fork prose)_
  - `agent_run_rejects_without_prompt_or_task_id`  _(named in fork prose; near-name fork test `agent_run_rejects_without_prompt_or_skill`)_
  - `agent_update_accepts_prompt_replacement`  _(named in fork prose)_
  - `agent_update_accepts_remove_prompt`  _(named in fork prose)_
  - `agent_update_leaves_prompt_unset_when_neither_flag_passed`  _(named in fork prose)_
  - `agent_update_rejects_conflicting_remove_flags`  _(named in fork prose)_
  - `agent_update_rejects_prompt_and_remove_prompt`  _(named in fork prose)_
  - `agent_update_rejects_remove_all_secret_deltas`  _(named in fork prose)_
  - `artifact_download_parses_artifact_id_and_out`  _(named in fork prose)_
  - `artifact_get_parses_artifact_uid`  _(named in fork prose)_
  - `artifact_help_hides_upload_but_keeps_download_visible`  _(named in fork prose)_
  - `artifact_upload_accepts_conversation_id_and_description`  _(named in fork prose)_
  - `artifact_upload_accepts_missing_association_target_for_env_fallback`  _(named in fork prose)_
  - `artifact_upload_accepts_run_id`  _(named in fork prose)_
  - `artifact_upload_accepts_run_id_and_description`  _(named in fork prose)_
  - `artifact_upload_rejects_both_association_targets`  _(named in fork prose)_
  - `environment_create_accepts_description`  _(named in fork prose)_
  - `environment_create_description_max_length`  _(named in fork prose)_
  - `environment_image_list_parses`  _(named in fork prose)_
  - `environment_update_accepts_description`  _(named in fork prose)_
  - `environment_update_accepts_remove_description`  _(named in fork prose)_
  - `finish_task_accepts_status_failure`  _(named in fork prose)_
  - `finish_task_accepts_status_success`  _(named in fork prose)_
  - `finish_task_rejects_invalid_status`  _(named in fork prose)_
  - `finish_task_rejects_missing_status`  _(named in fork prose)_
  - `hidden_server_overrides_parse_from_env`  _(named in fork prose)_
  - `integration_create_accepts_file`  _(named in fork prose)_
  - `integration_create_accepts_mcp_json`  _(named in fork prose)_
  - `integration_create_accepts_model`  _(named in fork prose)_
  - `integration_update_accepts_file`  _(named in fork prose)_
  - `integration_update_accepts_mcp_json_and_remove_mcp`  _(named in fork prose)_
  - `integration_update_accepts_model`  _(named in fork prose)_
  - `legacy_memory_store_memory_commands_are_rejected`  _(named in fork prose)_
  - `memory_create_parses`  _(named in fork prose)_
  - `memory_delete_parses`  _(named in fork prose)_
  - `memory_list_parses`  _(named in fork prose)_
  - `memory_store_get_parses`  _(named in fork prose)_
  - `memory_store_get_store_alias_parses`  _(named in fork prose)_
  - `memory_store_list_parses`  _(named in fork prose)_
  - `memory_store_update_parses`  _(named in fork prose)_
  - `memory_store_update_store_alias_parses`  _(named in fork prose)_
  - `memory_stores_alias_parses`  _(named in fork prose)_
  - `memory_update_parses`  _(named in fork prose)_
  - `memory_versions_parses`  _(named in fork prose)_
  - `raw_command_keeps_message_visible_before_runtime_help_customization`  _(named in fork prose)_
  - `report_external_reference_missing_reference_type_fails`  _(named in fork prose)_
  - `report_external_reference_missing_url_fails`  _(named in fork prose)_
  - `report_external_reference_optional_title_parses`  _(named in fork prose)_
  - `report_external_reference_required_args_parse`  _(named in fork prose)_
  - `report_shutdown_abnormal_parses`  _(named in fork prose)_
  - `report_shutdown_clean_parses`  _(named in fork prose)_
  - `run_cloud_accepts_claude_auth_secret`  _(named in fork prose)_
  - `run_cloud_accepts_codex_auth_secret`  _(named in fork prose)_
  - `run_cloud_help_lists_harness_and_auth_secret_flags`  _(named in fork prose)_
  - `run_cloud_rejects_claude_auth_secret_without_claude_harness`  _(named in fork prose)_
  - `run_cloud_rejects_codex_auth_secret_without_codex_harness`  _(named in fork prose)_
  - `run_message_delivered_alias_parses`  _(named in fork prose)_
  - `run_message_list_parses_filters`  _(named in fork prose)_
  - `run_message_list_rejects_non_positive_limit`  _(named in fork prose; near-name fork test `agent_message_list_rejects_non_positive_limit`)_
  - `run_message_mark_delivered_parses`  _(named in fork prose)_
  - `run_message_read_parses`  _(named in fork prose)_
  - `run_message_send_parses`  _(named in fork prose; near-name fork test `agent_message_send_parses`)_
  - `run_message_watch_parses`  _(named in fork prose)_
  - `schedule_create_accepts_file`  _(named in fork prose)_
  - `schedule_create_accepts_mcp_json`  _(named in fork prose)_
  - `schedule_create_accepts_personal_scope`  _(named in fork prose)_
  - `schedule_create_accepts_team_scope`  _(named in fork prose)_
  - `schedule_create_rejects_multiple_scopes`  _(named in fork prose)_
  - `schedule_resume_alias_parses_as_unpause`  _(named in fork prose)_
  - `schedule_update_accepts_file`  _(named in fork prose)_
  - `schedule_update_accepts_mcp_json_and_remove_mcp`  _(named in fork prose)_
  - `secret_create_codex_api_key_accepts_base_url_and_value_file`  _(named in fork prose)_
  - `secret_create_codex_api_key_parses_minimal`  _(named in fork prose)_
  - `secret_create_codex_api_key_requires_name`  _(named in fork prose)_
- **PORTED** — ported as `agent_run_accepts_file_short_flag`
  - `agent_run_cloud_accepts_file_short_flag`  _(named in fork prose)_
- **PORTED** — ported as `agent_run_accepts_harness_flag`
  - `agent_run_cloud_accepts_harness_flag`  _(named in fork prose)_
- **PORTED** — ported as `agent_run_accepts_mcp`
  - `agent_run_cloud_accepts_mcp`  _(named in fork prose)_
- **PORTED** — ported as `agent_run_defaults_harness_to_oz`
  - `agent_run_cloud_defaults_harness_to_oz`  _(named in fork prose)_

### `app/src/server/cloud_objects/update_manager_tests.rs` — 72 absent

pin 73 · fork 1 · source `app/src/server/cloud_objects/update_manager.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_accepts_new_metadata_with_force_refresh`
  - `test_add_guest_failure`
  - `test_add_guest_success`
  - `test_bulk_create_generic_string_objects`
  - `test_create_object_online_failure`
  - `test_create_object_online_success`
  - `test_create_object_online_user_facing_error`
  - `test_create_object_online_with_client_folder_id_fails`
  - `test_create_object_online_with_folder_id`
  - `test_create_sets_editor`
  - `test_delete_single_object`
  - `test_delete_single_object_with_actions`
  - `test_duplicate_workflow_not_pending_no_overwrite`
  - `test_empty_trash`
  - `test_fetch_single_cloud_object_not_pending_no_overwrite`
  - `test_fetch_single_cloud_object_pending_no_overwrite`
  - `test_fetch_single_cloud_object_pending_with_overwrite`
  - `test_leave_shared_object`
  - `test_metadata_after_non_optimistic_grab_baton_failure`
  - `test_metadata_after_non_optimistic_grab_baton_success`
  - `test_metadata_after_optimistic_grab_baton_failure`
  - `test_metadata_after_optimistic_grab_baton_success`
  - `test_metadata_after_trash_item_failure`
  - `test_metadata_after_trash_item_success`
  - `test_metadata_after_untrash_item_and_move_to_root`
  - `test_metadata_after_untrash_item_failure`
  - `test_metadata_after_untrash_item_success`
  - `test_metadata_update_with_polling_no_pending`
  - `test_metadata_update_with_rtc_no_pending`
  - `test_move_cloud_environment_personal_to_team_success`
  - `test_move_object_folder_to_folder_failure`
  - `test_move_object_folder_to_folder_success`
  - `test_move_object_folder_to_root_failure`
  - `test_move_object_folder_to_root_success`
  - `test_move_object_from_folder_to_folder_over_rtc`
  - `test_move_object_from_folder_to_root_over_rtc`
  - `test_move_object_from_root_to_folder_over_rtc`
  - `test_move_object_personal_to_team_failure`
  - `test_move_object_personal_to_team_success`
  - `test_move_object_root_to_folder_failure`
  - `test_move_object_root_to_folder_success`
  - `test_move_workflow_with_enums_personal_to_team_failure`
  - `test_move_workflow_with_enums_personal_to_team_success`
  - `test_object_action_histories_with_initial_load`
  - `test_overwrite_object_action_history_ignores_pending_local_actions`
  - `test_overwrite_object_action_history_no_actions_on_client`
  - `test_overwrite_object_action_history_reject`
  - `test_pending_conflict_correctly_clears_after_edits`
  - `test_pending_conflict_correctly_stays_after_edits`
  - `test_pending_metadata_update_with_polling`
  - `test_pending_metadata_update_with_rtc`
  - `test_pending_newer_conflict_remains_out_of_order`
  - `test_pending_self_conflict_clears_out_of_order`
  - `test_permissions_update_existing_object`
  - `test_permissions_update_grants_access`
  - `test_record_object_action`
  - `test_replace_object_with_conflicts`
  - `test_report_initial_load`
  - `test_sync_state_after_creation_fails_due_to_limit`
  - `test_sync_state_after_creation_failure_item_not_in_sync_queue`
  - `test_sync_state_after_creation_item_in_flight`
  - `test_sync_state_after_creation_item_not_in_sync_queue_folder`
  - `test_sync_state_after_creation_item_not_in_sync_queue_generic_object`
  - `test_sync_state_after_creation_item_not_in_sync_queue_notebook`
  - `test_sync_state_after_creation_item_not_in_sync_queue_workflow`
  - `test_sync_state_after_object_with_dependencies_created`
  - `test_sync_state_after_update_failure_item_in_sync_queue`
  - `test_sync_state_after_update_item_in_sync_queue`
  - `test_sync_state_after_update_item_not_in_sync_queue`
  - `test_sync_state_after_update_item_not_in_sync_queue_generic_string_object`
  - `test_trash_object_over_rtc`
  - `test_untrash_object_over_rtc`

### `app/src/ai/blocklist/orchestration_event_streamer_tests.rs` — 55 absent

pin 55 · fork 0 · source `app/src/ai/blocklist/orchestration_event_streamer.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

### `crates/ai/src/api_keys_tests.rs` — 55 absent

pin 67 · fork 12 · source `crates/ai/src/api_keys.rs` · fork ships source: yes

- **DECLINED?** — CustomEndpoint / custom_model_providers (#142, #347)
  - `api_keys_for_request_none_for_custom_endpoints_only`  _(named in fork prose)_
  - `has_any_key_true_for_custom_endpoints_only`  _(named in fork prose; near-name fork test `has_any_key_true_for_openai_only`)_
  - `serde_round_trip_with_custom_endpoints`  _(named in fork prose)_
- **PORTABLE?**
  - `api_keys_for_request_includes_expired_geap_token`
  - `api_keys_for_request_includes_expired_grok_token`
  - `api_keys_for_request_includes_geap_token_when_gate_and_binding_match`
  - `api_keys_for_request_includes_grok_token`
  - `api_keys_for_request_omits_geap_token_during_first_mint`
  - `api_keys_for_request_omits_geap_token_for_non_loaded_states`
  - `api_keys_for_request_omits_geap_token_on_binding_mismatch`
  - `api_keys_for_request_omits_geap_token_when_previous_binding_mismatches`
  - `api_keys_for_request_omits_geap_token_without_gate`
  - `api_keys_for_request_omits_grok_token_when_byo_disabled`  _(near-name fork test `api_keys_for_request_omits_keys_when_byo_disabled`)_
  - `api_keys_for_request_serves_previous_geap_token_while_refreshing`
  - `byok_disabled_returns_none_even_with_endpoints`  _(named in fork prose)_
  - `custom_model_providers_none_when_byo_disabled`  _(named in fork prose)_
  - `custom_model_providers_none_when_empty`  _(named in fork prose)_
  - `custom_model_providers_populates_single_endpoint`  _(named in fork prose)_
  - `custom_model_providers_preserves_configured_schema`  _(named in fork prose)_
  - `display_label_falls_back_to_name_when_alias_is_whitespace`
  - `display_label_falls_back_to_name_when_alias_missing`
  - `display_label_uses_alias_when_present`
  - `empty_api_key_endpoints_are_skipped`  _(named in fork prose)_
  - `endpoints_with_only_empty_models_are_skipped`  _(named in fork prose)_
  - `geap_access_token_blank_is_none`
  - `geap_access_token_near_expiry_still_sent`
  - `geap_access_token_present_without_expiry`
  - `geap_needs_refresh_lead_time_boundaries`
  - `grok_access_token_blank_is_none`
  - `grok_access_token_far_future_is_some`
  - `grok_access_token_near_expiry_still_sent`
  - `grok_access_token_present_without_expiry`
  - `grok_expired_refresh_token_ignores_in_flight_refresh`
  - `grok_expired_refresh_token_none_when_byo_disabled`
  - `grok_expired_refresh_token_none_when_near_expiry_but_valid`
  - `grok_expired_refresh_token_none_when_no_expiry`
  - `grok_expired_refresh_token_none_when_no_refresh_token`
  - `grok_expired_refresh_token_none_when_no_tokens`
  - `grok_expired_refresh_token_returns_token_when_expired`
  - `grok_is_expired_semantics`
  - `grok_needs_refresh_within_lead_time`
  - `has_any_key_false_for_endpoint_with_empty_api_key`  _(named in fork prose)_
  - `has_grok_subscription_false_when_not_connected`
  - `has_grok_subscription_false_when_token_blank`
  - `has_grok_subscription_true_for_expired_token`
  - `has_grok_subscription_true_when_connected`
  - `manager_has_any_key_false_for_blank_grok_and_no_keys`
  - `manager_has_any_key_false_when_no_keys_and_no_grok`
  - `manager_has_any_key_true_for_connected_grok_without_pasted_key`
  - `manager_has_any_key_true_for_pasted_key_without_grok`
  - `multiple_endpoints_all_serialize`  _(named in fork prose)_
  - `provider_key_count_counts_each_provider_key`
  - `provider_key_count_ignores_blank_keys_and_endpoints`
  - `provider_key_count_zero_when_empty`
  - `serde_legacy_endpoint_defaults_to_chat_completions`  _(named in fork prose)_

### `app/src/settings_view/update_environment_form_tests.rs` — 43 absent

pin 43 · fork 0 · source `app/src/settings_view/update_environment_form.rs` · fork ships source: NO

- **DECLINED?** — Warp Environments (#211)
  - `test_authed_repo_input_allows_arbitrary_repo`
  - `test_build_auth_url_with_next_cloud_setup_source`
  - `test_build_auth_url_with_next_focus_cloud_mode`
  - `test_build_auth_url_with_next_overrides_existing`
  - `test_build_auth_url_with_next_uses_scheme_param`
  - `test_can_suggest_image_for_create_does_not_require_repos_modified`
  - `test_can_suggest_image_for_edit_requires_repos_modified`
  - `test_create_environment_form_with_team_can_toggle_share_with_team_and_renders_warning_when_disabled`
  - `test_create_environment_form_without_team_does_not_render_checkbox_and_defaults_disabled`
  - `test_edit_mode_allows_saving_environment_without_docker_image`
  - `test_edit_mode_initializes_form_state_from_initial_values`
  - `test_empty_docker_image_produces_none_base_image`
  - `test_environment_form_copy_orchestration_modal_overrides_settings_defaults`
  - `test_environment_form_values_default`
  - `test_is_valid_requires_only_name`
  - `test_orchestration_modal_form_configuration_renders_footer_actions_without_team_controls`
  - `test_parse_docker_hub_url_bare_owner_repo`
  - `test_parse_docker_hub_url_empty_or_whitespace`
  - `test_parse_docker_hub_url_explicit_docker_io`
  - `test_parse_docker_hub_url_explicit_index_docker_io`
  - `test_parse_docker_hub_url_official_image`
  - `test_parse_docker_hub_url_official_image_explicit_library_prefix`
  - `test_parse_docker_hub_url_other_registry_returns_none`
  - `test_parse_docker_hub_url_trims_whitespace`
  - `test_parse_docker_hub_url_with_digest`
  - `test_parse_docker_hub_url_with_tag`
  - `test_parse_repo_input_github_url`
  - `test_parse_repo_input_owner_repo`
  - `test_parse_repo_inputs_invalid_returns_empty`
  - `test_parse_repo_inputs_multiple_entries`
  - `test_render_docker_image_field_shows_custom_image_warning`
  - `test_render_docker_image_field_shows_generating_state`
  - `test_render_docker_image_field_shows_github_auth_required_message`
  - `test_render_docker_image_field_shows_suggest_image_button_on_create`
  - `test_render_docker_image_field_shows_suggest_image_button_on_edit`
  - `test_render_repos_field_auth_required`
  - `test_render_repos_field_authed_state`
  - `test_render_repos_field_error_state`
  - `test_render_repos_field_loading_state`
  - `test_render_repos_field_with_selected_repos`
  - `test_repos_field_error_state_allows_manual_repo_entry`
  - `test_selected_repos_as_remote_repo_args_formats_owner_repo_strings`
  - `test_submit_button_disabled_until_required_fields_present`

### `app/src/ai/agent_conversations_model_tests.rs` — 41 absent

pin 59 · fork 18 · source `app/src/ai/agent_conversations_model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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
  - `test_get_entries_includes_local_only_entry`  _(named in fork prose)_
  - `test_get_entries_includes_task_only_entry`
  - `test_get_entries_keeps_unrelated_task_and_conversation_entries`
  - `test_get_entries_keeps_unrelated_tasks_and_conversations`  _(near-name fork test `test_get_tasks_and_conversations_keeps_unrelated_tasks_and_conversations`)_
  - `test_get_entries_merges_task_and_local_conversation_by_run_id`
  - `test_get_entries_merges_task_and_local_conversation_by_server_token`
  - `test_get_entries_prefers_task_when_server_token_matches`  _(near-name fork test `test_get_tasks_and_conversations_prefers_task_when_server_token_matches`)_
  - `test_get_entries_prefers_task_when_task_id_matches_conversation_run_id`  _(near-name fork test `test_get_tasks_and_conversations_prefers_task_when_task_id_matches_conversation_run_id`)_
  - `test_get_or_async_fetch_task_data_returns_cached_task_without_fetching`  _(near-name fork test `test_get_or_async_fetch_task_data_returns_cached_task`)_
  - `test_get_or_async_fetch_task_data_skips_when_in_flight`
  - `test_get_or_async_fetch_task_data_skips_when_permanently_failed`
  - `test_get_or_async_fetch_task_data_skips_within_transient_cooldown`
  - `test_has_items_ignores_child_agent_tasks`
  - `test_resolve_copy_link_prefers_active_session_link`
  - `test_resolve_copy_link_returns_none_for_local_only_unsynced_conversation`
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

### `crates/computer_use/src/overlay_tests.rs` — 41 absent

pin 41 · fork 0 · source `crates/computer_use/src/overlay.rs` · fork ships source: NO

- **DECLINED?** — computer_use session recording (#350)
  - `adjacent_margin_windows_merge`
  - `bottom_center_pill_style_and_dimensions`
  - `build_action_segments_uses_finish_offsets_and_drops_blocked_gaps`
  - `build_keep_segments_empty_when_no_entries`
  - `circle_path_is_origin_centered`
  - `click_animation_fits_within_retained_post_action_margin`
  - `click_ring_and_drag_circles_center_via_an7`
  - `drag_emits_trail_anchor_held_and_no_ring`
  - `duplicate_starts_merge_into_one_segment`
  - `empty_entries_produce_no_dialogue`
  - `entries_are_ordered_by_timecode`
  - `entries_beyond_source_duration_produce_no_segment`
  - `equal_frame_start_finish_enforces_one_frame_minimum`
  - `finish_after_source_end_clamps_to_source_duration`
  - `instantaneous_action_pill_lingers_past_finish`
  - `is_meaningful_action_group_false_for_wait_only_or_empty`
  - `is_meaningful_action_group_true_for_real_interactions`
  - `labels_in_a_group_share_timing_and_position`
  - `maps_all_scroll_directions_without_distance`
  - `maps_semantic_labels_in_action_order`
  - `mixed_group_renders_pill_and_pointer_without_leaking_text`
  - `multi_click_emits_one_ring_per_completed_click`
  - `multi_segment_drag_trail_has_a_quad_per_nonzero_segment`
  - `one_group_produces_one_segment`
  - `out_of_bounds_point_is_clamped_into_frame`
  - `out_of_order_groups_are_sorted_by_source_start`
  - `overlapping_margin_windows_merge`
  - `overlay_remaps_pill_timings_through_cut_segments`
  - `press_held_at_end_renders_held_indicator_without_ring`
  - `redacts_printable_keys_and_omits_pointer_actions`
  - `remap_source_interval_clamps_and_omits_across_removed_gaps`
  - `right_and_middle_clicks_render_rings`
  - `second_press_while_held_closes_prior_gesture_deterministically`
  - `single_click_emits_one_expanding_ring`
  - `source_duration_shorter_than_margin_clamps_window`
  - `split_call_drag_renders_one_trail_like_a_canonical_drag`
  - `split_call_drag_with_moves_across_two_entries_renders_one_trail`
  - `start_at_zero_clamps_margin_to_source_start`
  - `unmatched_release_and_stray_move_render_nothing`
  - `unmatched_release_for_a_different_button_does_not_close_a_drag`
  - `wait_only_group_renders_no_dialogue`

### `app/src/ai/blocklist/local_agent_task_sync_model_tests.rs` — 36 absent

pin 36 · fork 0 · source `app/src/ai/blocklist/local_agent_task_sync_model.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

### `app/src/terminal/view/shared_session/view_impl_tests.rs` — 36 absent

pin 41 · fork 5 · source `app/src/terminal/view/shared_session/view_impl.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `passive_suggestions_suppressed_for_shared_ambient_viewer`  _(named in fork prose)_
  - `test_ambient_session_join_auto_opens_details_panel`  _(named in fork prose)_
  - `test_begin_viewing_ambient_session_creates_and_wires_model_for_link_join_viewer`  _(named in fork prose)_
  - `test_begin_viewing_ambient_session_emits_view_model_created_event_once`  _(named in fork prose)_
  - `test_begin_viewing_ambient_session_reuses_existing_model_for_cloud_pane`  _(named in fork prose)_
  - `test_child_shared_session_link_keeps_default_conversation_details_auto_open`  _(named in fork prose)_
  - `test_cloud_cloud_handoff_session_join_keeps_closed_details_panel_hidden`  _(named in fork prose)_
  - `test_cloud_cloud_handoff_session_join_respects_details_panel_closed_after_followup_input`  _(named in fork prose)_
  - `test_continue_in_cloud_tombstone_routes_third_party_followup_to_new_cloud_vm`  _(named in fork prose)_
  - `test_conversation_details_auto_open_policy_defaults_to_open_for_ambient_shared_session`  _(named in fork prose)_
  - `test_deep_linked_ambient_continuation_refreshes_when_task_data_arrives`  _(named in fork prose)_
  - `test_local_to_cloud_handoff_session_join_keeps_details_panel_hidden`  _(named in fork prose)_
  - `test_non_owned_tombstone_is_removed_for_followup_and_reinserted_after_completion`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_enables_followup_for_owned_task_without_metadata`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_enables_followup_input_for_editable_non_owner_finished_view`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_enables_followup_input_without_tombstone_for_owned_task`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_inserts_tombstone_when_handoff_enabled`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_inserts_tombstone_without_handoff`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_keeps_live_owned_session_on_session_sharing_path`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_refreshes_open_details_panel_to_terminal_status`  _(named in fork prose)_
  - `test_on_ambient_agent_execution_ended_shows_tombstone_for_github_action_ambient_session`  _(named in fork prose)_
  - `test_on_session_share_ended_clears_frozen_followup_input_for_owned_ambient_session`  _(named in fork prose)_
  - `test_on_session_share_ended_does_not_insert_tombstone_for_ambient_session_under_cloud_mode_setup_v2`  _(named in fork prose; near-name fork test `test_on_session_share_ended_does_not_insert_tombstone_for_non_ambient_session_under_ambient_agent_setup_v2`)_
  - `test_on_session_share_ended_does_not_insert_tombstone_for_non_ambient_session_under_cloud_mode_setup_v2`  _(named in fork prose; near-name fork test `test_on_session_share_ended_does_not_insert_tombstone_for_non_ambient_session_under_ambient_agent_setup_v2`)_
  - `test_on_session_share_ended_does_not_insert_tombstone_for_owned_ambient_session_without_handoff`  _(named in fork prose)_
  - `test_on_session_share_ended_enables_followup_input_without_tombstone_for_owned_ambient_session`  _(named in fork prose)_
  - `test_on_session_share_ended_hides_input_for_no_cta_tombstone`  _(named in fork prose)_
  - `test_on_session_share_ended_shows_tombstone_for_github_action_ambient_session`  _(named in fork prose)_
  - `test_on_session_share_ended_skips_cloud_continuation_for_user_share_with_task_id`  _(named in fork prose)_
  - `test_restored_ambient_view_resolves_cta_from_view_model_task_id`  _(named in fork prose)_
  - `test_restored_owned_tombstone_hides_input_until_continue`  _(named in fork prose)_
  - `test_restored_oz_edit_access_non_owner_finished_view_uses_followup_input_without_tombstone`  _(named in fork prose)_
  - `test_shared_followup_on_existing_conversation_converts_user_query_input`  _(named in fork prose)_
  - `test_suppressed_conversation_details_auto_open_consumes_initial_open_but_manual_toggle_works`  _(named in fork prose)_
  - `test_try_submit_pending_cloud_followup_allows_repeat_submission_for_owned_task`  _(named in fork prose)_
  - `test_try_submit_pending_cloud_followup_rejects_task_source_that_blocks_followups`  _(named in fork prose)_

### `app/src/ai/agent_sdk/driver/snapshot_tests.rs` — 35 absent

pin 35 · fork 0 · source `app/src/ai/agent_sdk/driver/snapshot.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

### `app/src/terminal/shared_session/viewer/orchestration_viewer_model_tests.rs` — 34 absent

pin 34 · fork 0 · source `app/src/terminal/shared_session/viewer/orchestration_viewer_model.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `b1_populates_agent_id_to_conversation_id_for_new_child`  _(named in fork prose)_
  - `b2_backfills_parent_agent_id_on_orchestrator_token_assigned`  _(named in fork prose)_
  - `b2_does_not_overwrite_existing_parent_agent_id`  _(named in fork prose)_
  - `b2_ignores_token_assigned_for_unrelated_conversation`  _(named in fork prose)_
  - `child_spawned_with_malformed_run_id_is_dropped`  _(named in fork prose)_
  - `child_status_changed_does_not_refetch_when_already_materialized`  _(named in fork prose)_
  - `child_status_changed_refetches_metadata_while_session_id_is_pending`  _(named in fork prose)_
  - `child_status_changed_updates_existing_placeholder_via_local_map`  _(named in fork prose)_
  - `child_status_changed_with_unknown_run_id_is_silently_dropped`  _(named in fork prose)_
  - `handle_streamer_event_filters_on_parent_task_id`  _(named in fork prose)_
  - `maps_blocked_to_blocked`  _(named in fork prose)_
  - `maps_cancelled_to_cancelled`  _(named in fork prose)_
  - `maps_failed_and_error_to_error`  _(named in fork prose)_
  - `maps_succeeded_to_success`  _(named in fork prose)_
  - `maps_working_states_to_in_progress`  _(named in fork prose)_
  - `materialization_gate_flips_on_session_id_transition`  _(named in fork prose)_
  - `materialization_requested_only_once_per_child`  _(named in fork prose)_
  - `pending_session_id_poll_dispatches_per_pending_child`  _(named in fork prose)_
  - `pending_session_id_poll_does_not_schedule_when_no_children_pending`  _(named in fork prose)_
  - `pending_session_id_poll_schedules_while_session_id_is_none`  _(named in fork prose)_
  - `registers_child_agent_name_does_not_set_fallback_for_whitespace_only_title`  _(named in fork prose)_
  - `registers_child_agent_name_falls_back_to_title_when_snapshot_name_is_missing`  _(named in fork prose)_
  - `registers_child_agent_name_from_snapshot_name`  _(named in fork prose)_
  - `registers_child_agent_name_trims_whitespace`  _(named in fork prose)_
  - `registers_child_agent_name_uses_literal_agent_when_both_are_empty`  _(named in fork prose)_
  - `registers_multiple_children`  _(named in fork prose)_
  - `registers_new_child_conversation`  _(named in fork prose)_
  - `skips_child_when_no_active_parent_conversation`  _(named in fork prose)_
  - `skips_parent_task_id_as_child`  _(named in fork prose)_
  - `streamer_consumer_is_registered_when_constructed`  _(named in fork prose)_
  - `unknown_state_maps_to_error`  _(named in fork prose)_
  - `updates_status_on_state_change`  _(named in fork prose)_
  - `viewer_model_does_not_register_when_active_conversation_is_a_child_placeholder`  _(named in fork prose)_
  - `viewer_model_retries_consumer_registration_on_set_active_conversation`  _(named in fork prose)_

### `app/src/pane_group/mod_tests.rs` — 33 absent

pin 48 · fork 15 · source `app/src/pane_group/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `create_shared_session_viewer_with_cloud_mode_populates_ambient_agent_view_model`
  - `create_shared_session_viewer_without_cloud_mode_does_not_populate_ambient_agent_view_model`
  - `decide_remote_child_hydration_active_unattachable_with_token_loads_transcript`
  - `decide_remote_child_hydration_active_unattachable_without_token_falls_back_non_terminal`
  - `decide_remote_child_hydration_attachable_live_session_chooses_live_attach`
  - `decide_remote_child_hydration_empty_token_falls_back`
  - `decide_remote_child_hydration_inactive_with_token_loads_transcript`
  - `decide_remote_child_hydration_inactive_without_token_falls_back`
  - `test_add_pane_restores_hidden_child_when_parent_is_already_fullscreen`
  - `test_ambient_transcript_restore_creates_cloud_mode_pane_when_handoff_enabled`
  - `test_ambient_transcript_restore_uses_generic_viewer_when_handoff_disabled`
  - `test_close_pane_clears_transitively_shared_child_entry_on_non_undo_branch`
  - `test_create_missing_child_agent_panes_restores_remote_child_from_history_model`
  - `test_ensure_hidden_child_agent_pane_materializes_missing_child_pane`
  - `test_ensure_hidden_child_agent_pane_materializes_restored_remote_child_linked_by_parent_agent_id`
  - `test_ensure_hidden_child_agent_pane_skips_child_owned_by_another_pane_group`
  - `test_entering_parent_agent_view_lazily_restores_hidden_child_pane`
  - `test_entering_parent_agent_view_skips_child_owned_by_another_pane`
  - `test_entering_parent_agent_view_skips_child_owned_by_another_pane_group`
  - `test_entering_remote_parent_agent_view_lazily_restores_local_hidden_child_pane`
  - `test_entering_remote_parent_agent_view_lazily_restores_remote_hidden_child_pane`
  - `test_hidden_child_creation_applies_ambient_task_id_to_controller`
  - `test_insert_hidden_ambient_child_agent_pane_suppresses_details_auto_open`
  - `test_pane_group_restore_loop_keeps_orchestration_topology_and_materializes_child_pane`
  - `test_reattach_panes_restores_hidden_child_when_parent_is_already_fullscreen`
  - `test_replace_pane_restores_hidden_child_when_replacement_is_already_fullscreen`
  - `test_restore_closed_pane_restores_hidden_child_when_parent_is_already_fullscreen`
  - `test_restored_hidden_child_pane_reapplies_ambient_task_id_to_controller`
  - `test_restored_remote_hidden_child_pane_enters_existing_ambient_session`
  - `test_restored_remote_hidden_child_pane_fallback_when_task_data_unavailable`
  - `test_start_shared_session_from_modal`  _(named in fork prose)_
  - `test_stop_shared_session`  _(named in fork prose)_
  - `test_swapping_to_child_agent_from_maximized_pane_keeps_maximized_state`

### `app/src/ai/blocklist/inline_action/run_agents_card_view_tests.rs` — 32 absent

pin 32 · fork 0 · source `app/src/ai/blocklist/inline_action/run_agents_card_view.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

### `app/src/settings_view/environments_page_tests.rs` — 32 absent

pin 32 · fork 0 · source `app/src/settings_view/environments_page.rs` · fork ships source: NO

- **DECLINED?** — Warp Environments (#211)
  - `test_agent_assisted_modal_confirm_dispatches_root_view_action_and_hides_modal`
  - `test_agent_assisted_modal_open_and_cancel_renders_and_hides`
  - `test_environment_matches_search_query_empty_query_matches_all`
  - `test_environment_matches_search_query_env_id_substring`
  - `test_environment_matches_search_query_is_case_insensitive`
  - `test_environment_matches_search_query_name_description_image_repos`
  - `test_environment_setup_mode_selector_renders_options`
  - `test_environments_page_default_is_list`
  - `test_environments_page_edit_variant`
  - `test_environments_page_widget_search_terms`
  - `test_github_repo_display`
  - `test_github_repo_equality`
  - `test_github_repo_new`
  - `test_render_empty_state_github_card_error_state_shows_retry`
  - `test_render_empty_state_github_card_loading_state`
  - `test_render_empty_state_github_card_unauthed_state_shows_authorize`
  - `test_render_empty_state_shows_github_remote_and_local_rows`
  - `test_render_environment_card_with_all_features`
  - `test_render_environment_card_with_empty_setup_commands`
  - `test_render_environment_card_with_github_repos`
  - `test_render_environment_card_with_last_used_never`
  - `test_render_environment_card_with_last_used_timestamp`
  - `test_render_environment_card_with_minimal_config`
  - `test_render_environment_card_with_setup_commands`
  - `test_render_environments_list_with_multiple_environments`
  - `test_render_environments_list_with_single_environment`
  - `test_render_list_page_with_environments_shows_list`
  - `test_render_list_page_with_no_environments_shows_empty_state`
  - `test_render_list_page_with_only_personal_environments_shows_personal_header`
  - `test_render_list_page_with_personal_and_team_environments_shows_section_headers`
  - `test_set_github_auth_redirect_target_updates_form`
  - `test_toolbar_renders_search_editor_view`

### `app/src/ai/request_usage_model_tests.rs` — 30 absent

pin 30 · fork 0 · source `app/src/ai/request_usage_model.rs` · fork ships source: yes

- **DECLINED?** — source is a 260-line no-op stub (ORACLE.md)
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
  - `test_request_limit_info`  _(named in fork prose)_
  - `test_request_limit_info_is_unlimited_true`
  - `test_request_limit_info_past_refresh_time`
  - `test_request_limit_info_with_limit`

### `app/src/server/server_api/ai_tests.rs` — 30 absent

pin 45 · fork 15 · source `app/src/server/server_api/ai.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `ambient_agent_headers_for_task_overrides_existing_cloud_agent_header`
  - `build_fork_conversation_url_escapes_path_param`
  - `build_fork_conversation_url_routes_to_conversation_fork`
  - `build_list_agent_runs_url_all_fields`
  - `build_list_agent_runs_url_empty_filter`
  - `build_list_agent_runs_url_repeats_state_filter`
  - `build_list_agent_runs_url_routes_to_runs_not_tasks`
  - `build_list_agent_runs_url_skips_unknown_state`
  - `build_run_followup_url_routes_to_run_followups`
  - `connected_self_hosted_workers_path_uses_public_api_route`
  - `deserialize_connected_self_hosted_workers_response`
  - `deserialize_fork_conversation_response`
  - `serialize_run_followup_request`
  - `spawn_agent_request_omits_prompt_when_none`
  - `spawn_agent_request_serializes_agent_uid_as_agent_identity_uid`
  - `test_deserialize_agent_message_headers`
  - `test_deserialize_agent_run_events_with_optional_fields`
  - `test_deserialize_file_artifact_download_response`
  - `test_deserialize_list_tasks_all_tasks_invalid_returns_empty`
  - `test_deserialize_list_tasks_all_tasks_valid`
  - `test_deserialize_list_tasks_corrupted_json_in_middle`
  - `test_deserialize_list_tasks_empty_tasks_array`
  - `test_deserialize_list_tasks_error_and_blocked_states`
  - `test_deserialize_list_tasks_invalid_state_enum`
  - `test_deserialize_list_tasks_response_empty_artifacts`
  - `test_deserialize_list_tasks_response_missing_artifacts_field`
  - `test_deserialize_list_tasks_response_with_artifacts`
  - `test_deserialize_list_tasks_skips_invalid_task`
  - `test_deserialize_read_agent_message_response_with_timestamps`
  - `test_deserialize_screenshot_artifact_download_response`

### `app/src/workspaces/user_workspaces_tests.rs` — 29 absent

pin 29 · fork 0 · source `app/src/workspaces/user_workspaces.rs` · fork ships source: yes

- **DECLINED?** — Cloud teams / org policy (#445)
  - `test_agent_attribution_default_with_no_workspace`  _(named in fork prose)_
  - `test_agent_attribution_forced_off_by_team`  _(named in fork prose)_
  - `test_agent_attribution_forced_on_by_team`  _(named in fork prose)_
  - `test_agent_attribution_respects_user_setting`  _(named in fork prose)_
  - `test_aws_bedrock_credentials_default_off_when_admin_respects_user_setting`
  - `test_aws_bedrock_credentials_enforced_by_admin`  _(named in fork prose)_
  - `test_aws_bedrock_credentials_respect_user_setting`
  - `test_codebase_context_disabled_by_workspace`
  - `test_codebase_context_enabled_by_team_and_user`
  - `test_codebase_context_enabled_by_team_disabled_by_user`
  - `test_codebase_context_enabled_with_no_workspace`
  - `test_codebase_context_respect_user_setting`
  - `test_current_workspace_billing_metadata_uses_selected_teamless_workspace`
  - `test_gemini_enterprise_credentials_default_off_when_admin_respects_user_setting`
  - `test_gemini_enterprise_credentials_disabled_when_host_absent`
  - `test_gemini_enterprise_credentials_disabled_when_host_disabled`
  - `test_gemini_enterprise_credentials_disabled_when_logged_out`
  - `test_gemini_enterprise_credentials_enforced_by_admin`
  - `test_gemini_enterprise_credentials_respect_user_setting_honors_member_toggle`
  - `test_gemini_enterprise_host_settings_carries_federation_config`
  - `test_joining_team_moves_objects`  _(named in fork prose)_
  - `test_leaving_team_moves_objects`
  - `test_loading_all_spaces_after_switching_from_offline`
  - `test_spaces_for_window_orders_selected_team_shared_and_personal`
  - `test_unassigned_window_is_initialized_after_workspace_metadata_loads`
  - `test_window_team_assignment_falls_back_when_team_is_removed`
  - `test_window_team_assignment_inherits_from_source_or_default_team`
  - `test_window_team_assignment_is_immutable`
  - `test_window_team_assignment_reconciles_when_current_workspace_changes`

### `app/src/ai/blocklist/history_model_tests.rs` — 27 absent

pin 71 · fork 44 · source `app/src/ai/blocklist/history_model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `hydrate_remote_child_placeholder_with_cloud_transcript_preserves_placeholder_identity`
  - `prompt_history_candidates_seeds_from_snapshot_then_appends_session_prompts`  _(named in fork prose; near-name fork test `prompt_history_candidates_seeds_from_snapshot`)_
  - `start_new_child_conversation_persists_harness_metadata`
  - `test_assign_run_id_for_conversation_persists_updated_conversation_state`
  - `test_find_by_token_after_mark_conversations_historical_for_terminal_surface`  _(near-name fork test `test_find_by_token_after_mark_conversations_historical_for_terminal_view`)_
  - `test_find_by_token_after_merge_cloud_metadata`
  - `test_fork_conversation_title_override_replaces_prefix`
  - `test_fork_then_bind_handoff_token_persists_to_restored_conversation`
  - `test_fork_then_bind_handoff_token_resolves_to_forked_conversation`
  - `test_fork_then_bind_handoff_token_updates_cached_metadata_and_emits_refresh_events`
  - `test_initialize_historical_conversations_eagerly_hydrates_orchestration_children`
  - `test_initialize_historical_conversations_resolves_parent_agent_id_children_via_seeded_run_ids`
  - `test_initialize_output_for_response_stream_persists_updated_conversation_state`
  - `test_mark_conversation_as_remote_child_persists_updated_conversation_state`
  - `test_merge_cloud_conversation_metadata`
  - `test_merge_cloud_metadata_refreshes_stale_restored_conversation_metadata`
  - `test_merge_cloud_metadata_removes_stale_duplicate_metadata_ids_for_token`
  - `test_merge_cloud_metadata_reuses_restored_conversation_id_for_token`
  - `test_merge_cloud_metadata_updates_already_restored_conversations`
  - `test_optimistic_root_restore_round_trip_yields_in_progress_optimistic_root`
  - `test_optimistic_root_upgrade_then_persist_emits_event_with_single_server_task_row`
  - `test_persist_with_optimistic_root_emits_event_with_no_task_rows`
  - `test_reserved_canonical_conversation_id_reused_by_later_metadata_merge`
  - `test_restore_conversations_indexes_child_by_parent_agent_id`
  - `test_start_new_child_conversation_persists_child_metadata_for_restore`
  - `test_truncate_from_exchange_to_empty_persist_event_has_empty_updated_tasks`
  - `test_two_restart_cycles_keep_exactly_one_server_root_task_row`

### `crates/warp_tui/src/terminal_session_view_tests.rs` — 27 absent

pin 90 · fork 63 · source `crates/warp_tui/src/terminal_session_view.rs` · fork ships source: yes

- **DECLINED?** — Status-menu org/email fields (#389)
  - `status_email_fallback_chain_covers_username_and_signed_in_arms`  _(named in fork prose)_
- **DECLINED?** — Voice input (#389, #352)
  - `configured_voice_item_renders_idle_listening_and_transcribing_states`  _(named in fork prose)_
  - `footer_falls_back_to_replacing_voice_hints_when_voice_item_is_disabled`  _(named in fork prose)_
  - `listening_voice_input_animates_the_input_border`  _(named in fork prose)_
  - `voice_accepts_exact_and_whitespace_only_arguments`  _(named in fork prose)_
  - `voice_click_is_interactive_only_within_the_segment_bounds`  _(named in fork prose)_
  - `voice_input_uses_ctrl_s_only_when_the_composer_owns_input`  _(named in fork prose)_
  - `voice_slash_command_rejects_arguments_before_prompt_fallback`  _(named in fork prose)_
  - `voice_toggle_stops_listening_and_ignores_transcribing`  _(named in fork prose)_
- **PORTABLE?**
  - `blocked_terminal_use_action_acceptance_uses_ctrl_enter_without_rebinding_submit`  _(named in fork prose)_
  - `figma_statusline_metadata_formats_are_stable`  _(named in fork prose)_
  - `grok_oauth_block_exclusively_owns_input_until_escape`  _(named in fork prose)_
  - `manual_attach_and_detach_switch_running_command_input_ownership`  _(named in fork prose)_
  - `nld_reset_only_unlocks_after_agent_control_and_not_on_user_edit`  _(named in fork prose)_
  - `provider_api_key_shell_command_uses_shared_tui_launcher`  _(named in fork prose)_
  - `response_summary_visibility_is_independent_from_the_footer_usage_mode`  _(named in fork prose; near-name fork test `response_summary_visibility_is_independent_from_the_footer_usage_entry`)_
  - `resume_shell_commands_use_shared_tui_launcher`  _(named in fork prose)_
  - `running_command_completion_clears_transient_attachment_lock`  _(named in fork prose)_
  - `shell_mode_reserves_tab_even_when_attachments_render`  _(named in fork prose)_
  - `status_slash_command_opens_dedicated_status_menu_via_shared_structure`  _(named in fork prose)_
  - `tagged_in_alt_screen_keeps_output_and_composer_visible`  _(named in fork prose; near-name fork test `agent_controlled_alt_screen_keeps_output_and_composer_visible`)_
  - `terminal_use_interrupt_closes_shortcuts_before_taking_control`  _(named in fork prose)_
  - `tui_cli_shell_command_uses_channel_entry_points`  _(named in fork prose)_
  - `user_controlled_alt_screen_keeps_full_session_input_on_the_pty`  _(named in fork prose)_
  - `user_info_updates_only_require_an_open_status_menu_repaint`  _(named in fork prose)_
  - `visible_startup_script_shows_no_running_command_hint`  _(named in fork prose; near-name fork test `visible_startup_script_shows_no_interrupt_hint`)_
  - `zero_state_running_command_hint_shows_attachment`  _(named in fork prose)_

### `crates/build_cache/src/lib_tests.rs` — 25 absent

pin 25 · fork 0 · source `crates/build_cache/src/lib.rs` · fork ships source: yes

- **PORTABLE?**
  - `cache_setup_error_variants_have_expected_is_actionable_classification`
  - `destructive_execution_uses_resolved_modes_without_redetection`
  - `empty_mode_union_produces_no_executable_plan`
  - `failure_categories_are_preserved`
  - `global_modes_are_union_of_successful_repo_detections`
  - `hit_miss_aggregation_retains_zero_mount_modes`
  - `invalid_env_names_are_rejected_individually`
  - `json_parse_failure_is_classified_and_does_not_abort_later_repos`
  - `permission_denied_cache_directory_uses_noninteractive_sudo_mkdir_and_chown`
  - `plan_adds_supplied_apt_global_mode`
  - `plan_adds_supplied_brew_global_mode`
  - `plan_orders_repository_keys_and_places_single_global_last`
  - `plan_sorts_and_deduplicates_configuration_modes`
  - `plan_uses_only_relative_repo_and_shared_cache_directories`
  - `process_runner_classifies_spawn_nonzero_and_timeout`
  - `queued_executor_can_return_each_failure_category`
  - `repo_cache_key_distinguishes_forge_owner_and_repo`
  - `repo_cache_key_is_stable_for_canonical_identity`
  - `repo_env_conflict_resolves_by_key_order`
  - `repo_failure_continues_and_global_still_executes`
  - `scratch_directories_are_unique_0700_outside_repo_and_retained`
  - `shared_failure_keeps_canonical_repo_env_overlay`
  - `shared_success_replaces_complete_repo_env_overlay`
  - `spacectl_calls_are_bounded_by_two_repos_plus_one_global`
  - `timeout_returns_bounded_when_descendant_keeps_stdout_open`

### `app/src/ai/agent_sdk/agent_management_tests.rs` — 23 absent

pin 23 · fork 0 · source `app/src/ai/agent_sdk/agent_management.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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
- **DECLINED?** — Account-first onboarding (#11)
  - `pre_login_edit_materializes_the_pending_collection`

### `app/src/ai/llms_tests.rs` — 23 absent

pin 27 · fork 4 · source `app/src/ai/llms.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `active_models_use_default_when_usable`
  - `custom_llm_display_name_falls_back_to_name_when_alias_missing`
  - `custom_llm_display_name_uses_alias_when_present`
  - `custom_llm_infos_built_from_endpoints`  _(named in fork prose)_
  - `custom_llm_infos_skip_endpoints_with_empty_api_key`
  - `custom_llm_infos_skip_models_without_config_key`
  - `explicit_child_model_pin_preserves_gui_behavior_and_only_emits_for_effective_changes`
  - `host_icon_visibility_requires_enabled_credentials_and_model_host`
  - `is_cloud_runnable_oz_model_id_classifies_ids`
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
- **DECLINED?** — CustomEndpoint / custom_model_providers (#142, #347)
  - `active_models_fall_back_to_usable_choice_or_custom_endpoint_when_default_disabled`
  - `custom_endpoint_usage_display_label_resolves_alias_name_and_generic_fallback`
  - `reconcile_preserves_custom_endpoint_models_not_configured_locally`

### `app/src/terminal/view/shared_session/cloud_conversation_continuation_tests.rs` — 23 absent

pin 23 · fork 0 · source `app/src/terminal/view/shared_session/cloud_conversation_continuation.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `active_task_execution_returns_error`  _(named in fork prose)_
  - `environment_setup_failure_with_conversation_shows_continue_cta`  _(named in fork prose)_
  - `environment_setup_failure_without_conversation_shows_tombstone_without_cta`  _(named in fork prose)_
  - `github_action_source_shows_tombstone_without_cta`  _(named in fork prose)_
  - `missing_metadata_returns_error`  _(named in fork prose)_
  - `missing_task_returns_error`  _(named in fork prose)_
  - `owned_oz_task_without_metadata_shows_inline_followup_input`  _(named in fork prose)_
  - `owned_third_party_task_without_metadata_shows_continue_in_cloud_tombstone`  _(named in fork prose)_
  - `oz_conversation_with_edit_access_shows_inline_followup_input`  _(named in fork prose)_
  - `oz_conversation_with_view_access_shows_continue_locally_tombstone`  _(named in fork prose)_
  - `routing_is_live_remote_vm_for_active_execution_without_attached_viewer`  _(named in fork prose)_
  - `routing_is_live_remote_vm_for_active_viewer`  _(named in fork prose)_
  - `routing_is_local_for_active_sharer_local_orchestration_child`  _(named in fork prose)_
  - `routing_is_local_for_non_cloud_pane`  _(named in fork prose)_
  - `routing_is_new_cloud_vm_for_owned_oz_disconnected_pane`  _(named in fork prose)_
  - `routing_is_read_only_for_non_owner_disconnected_pane`  _(named in fork prose)_
  - `routing_omits_task_id_for_non_ambient_shared_session_viewer`  _(named in fork prose)_
  - `third_party_conversation_created_by_current_user_shows_continue_in_cloud_tombstone`  _(named in fork prose)_
  - `third_party_conversation_owned_by_current_team_shows_continue_in_cloud_tombstone`  _(named in fork prose)_
  - `third_party_conversation_shared_with_current_team_as_editor_shows_continue_in_cloud_tombstone`  _(named in fork prose)_
  - `third_party_conversation_with_edit_access_shows_continue_in_cloud_tombstone`  _(named in fork prose)_
  - `third_party_conversation_with_view_access_shows_tombstone_without_cta`  _(named in fork prose)_
  - `unknown_access_returns_error`  _(named in fork prose)_

### `crates/warp_server_client/src/iap_tests.rs` — 23 absent

pin 23 · fork 0 · source `crates/warp_server_client/src/iap.rs` · fork ships source: NO

- **DECLINED?** — Status-menu org/email fields (#389)
  - `generate_id_token_request_uses_camel_case_include_email`
- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `fetch_iap_token_via_wif_errors_on_iam_failure`
  - `fetch_iap_token_via_wif_errors_on_sts_failure`
  - `fetch_iap_token_via_wif_returns_token_on_success`
  - `generate_id_token_response_parses_token`
  - `get_cached_failed_uses_valid_previous_token`
  - `get_cached_loaded_expired_is_none`
  - `get_cached_loaded_valid_returns_token`
  - `get_cached_refreshing_drops_expired_previous_token`
  - `get_cached_refreshing_uses_valid_previous_token`
  - `get_expires_at_future_exp_is_ok`
  - `get_expires_at_missing_exp_errs`
  - `get_expires_at_past_exp_errs`
  - `parse_aud_from_jwt_missing_aud_is_none`
  - `parse_aud_from_jwt_reads_first_array_aud`
  - `parse_aud_from_jwt_reads_string_aud`
  - `parse_exp_from_jwt_invalid_base64_is_none`
  - `parse_exp_from_jwt_missing_exp_is_none`
  - `parse_exp_from_jwt_not_a_jwt_is_none`
  - `parse_exp_from_jwt_reads_exp_claim`
  - `resolve_wif_identity_token_mints_when_injected_expired`
  - `resolve_wif_identity_token_prefers_valid_injected_jwt`
  - `sts_response_parses_and_ignores_extra_fields`

### `app/src/ai/agent_sdk/driver/error_classification_tests.rs` — 22 absent

pin 22 · fork 0 · source `app/src/ai/agent_sdk/driver/error_classification.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

### `app/src/terminal/input_tests.rs` — 22 absent

pin 148 · fork 126 · source `app/src/terminal/input.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `attach_ambient_view_model_builds_composer_selectors_for_fresh_cloud_pane_in_view_pending`  _(named in fork prose)_
  - `attach_ambient_view_model_skips_composer_selectors_for_actual_shared_session_viewer`  _(named in fork prose)_
  - `cloud_mode_host_selector_shown_when_connected_workers_present`  _(named in fork prose)_
  - `empty_buffer_enter_skips_locked_initial_cloud_mode_head`  _(named in fork prose)_
  - `maybe_route_ai_query_to_remote_target_blocks_read_only_viewer`  _(named in fork prose)_
  - `maybe_route_ai_query_to_remote_target_forwards_executor_viewer_prompt`  _(named in fork prose)_
  - `maybe_route_ai_query_to_remote_target_proceeds_for_empty_buffer`  _(named in fork prose)_
  - `maybe_route_ai_query_to_remote_target_proceeds_for_local_pane`  _(named in fork prose)_
  - `send_now_event_submits_through_active_pane_and_preserves_draft`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_activates_in_powershell_when_nld_disabled`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_activates_when_handoff_flags_enabled`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_escape_exits_mode_preserving_prompt_text`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_exits_on_backspace_at_beginning_of_buffer`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_ignores_terminal_input_mode_toggle`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_keeps_shell_prefix_as_query_text`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_normal_deletion_does_not_exit`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_remains_text_in_powershell_with_nld_enabled`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_remains_text_when_handoff_flag_disabled`  _(named in fork prose)_
  - `test_cloud_handoff_prefix_vim_escape_exits_insert_before_handoff_mode`  _(named in fork prose)_
  - `test_source_less_locked_config_clears_decision_source`  _(named in fork prose)_
  - `test_terminal_prefix_sets_shell_prefix_decision_source`  _(named in fork prose)_
  - `zero_state_hint_text_only_registers_active_slash_command_placeholders`  _(named in fork prose)_

### `app/src/terminal/view_tests.rs` — 22 absent

pin 141 · fork 119 · source `app/src/terminal/view.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `active_cli_agent_ignores_warp_tui_when_hoa_code_review_disabled`  _(named in fork prose; near-name fork test `active_cli_agent_ignores_phosphor_tui_when_hoa_code_review_disabled`)_
  - `active_cli_agent_recognizes_detected_warp_tui_session`  _(named in fork prose; near-name fork test `active_cli_agent_recognizes_detected_phosphor_tui_session`)_
  - `clicking_old_banner_for_open_conversation_focuses_current_terminal_surface_without_transferring_blocks`  _(named in fork prose)_
  - `cloud_mode_dispatched_agent_inserts_queued_user_query`  _(named in fork prose)_
  - `cloud_mode_failed_keeps_queued_query_above_tombstone_and_hides_input`  _(named in fork prose)_
  - `cloud_mode_followup_dispatched_inserts_queued_user_query`  _(named in fork prose)_
  - `cloud_mode_followup_input_uses_explicit_submit_event_even_when_view_pending`  _(named in fork prose)_
  - `cloud_mode_setup_v2_suppresses_sharer_input_updates_while_followup_setup_commands_run`  _(named in fork prose)_
  - `cloud_mode_v1_agent_prefixed_query_spawns_cloud_agent`  _(named in fork prose)_
  - `cloud_mode_v2_agent_prefixed_query_spawns_cloud_agent`  _(named in fork prose)_
  - `escape_does_not_exit_root_cloud_agent_view_with_long_running_command`  _(named in fork prose; near-name fork test `escape_does_not_exit_local_agent_view_with_long_running_command`)_
  - `escape_pops_nested_cloud_agent_view_with_long_running_command`  _(named in fork prose)_
  - `fresh_cloud_mode_setup_enters_agent_view_when_view_pending`  _(named in fork prose)_
  - `pending_cloud_followup_without_ambient_model_restores_prompt`  _(named in fork prose)_
  - `pending_cloud_mode_query_clears_when_streaming_exchange_becomes_renderable`  _(named in fork prose)_
  - `pending_cloud_mode_query_waits_for_renderable_user_query_exchange`  _(named in fork prose)_
  - `root_cloud_mode_pane_sets_root_cloud_mode_context_key`  _(named in fork prose)_
  - `send_review_comments_to_warp_tui_writes_prompt_to_pty`  _(named in fork prose; near-name fork test `send_review_comments_to_phosphor_tui_writes_prompt_to_pty`)_
  - `set_input_mode_agent_does_not_enter_local_agent_from_root_cloud_mode_pane`  _(named in fork prose; near-name fork test `set_input_mode_agent_does_not_enter_local_agent_from_root_ambient_agent_pane`)_
  - `shared_third_party_viewer_sync_enters_agent_view_and_retags_existing_block`  _(named in fork prose)_
  - `shared_third_party_viewer_syncs_from_cli_agent_state_without_ambient_model`  _(named in fork prose)_
  - `shared_third_party_viewer_syncs_from_viewer_harness_updated_when_harness_unchanged`  _(named in fork prose)_

### `app/src/ai/geap_credentials_tests.rs` — 21 absent

pin 21 · fork 0 · source `app/src/ai/geap_credentials.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `compose_child_prompt_concatenates_when_both_non_empty`
  - `compose_child_prompt_returns_empty_when_both_empty`
  - `compose_child_prompt_treats_whitespace_only_base_as_empty`
  - `compose_child_prompt_uses_base_only_when_per_agent_empty`
  - `compose_child_prompt_uses_per_agent_only_when_base_empty`
  - `local_arm_allows_claude`
  - `local_arm_ignores_auth_secret_name`
  - `local_arm_rejects_agent_identity_uid`
  - `local_arm_rejects_disabled_codex`
  - `received_message_collapsible_id_prefixes_row_ids`
  - `recording_artifact_view_url_requires_task_id`
  - `recording_artifact_view_url_uses_configured_oz_origin`
  - `remote_arm_filters_whitespace_auth_secret_name_to_none`
  - `remote_arm_propagates_agent_identity_uid`
  - `remote_arm_propagates_claude_auth_secret_into_mode`
  - `remote_arm_propagates_skills_into_skill_references`
  - `remote_arm_rejects_opencode`
  - `remote_arm_with_empty_skills_propagates_empty_vec`
  - `user_avatar_info_prefers_conversation_creator_profile`
  - `user_avatar_info_uses_cached_profile_for_creator_uid`

### `crates/cloud_object_models/src/cloud_environment_tests.rs` — 20 absent

pin 20 · fork 0 · source `crates/cloud_object_models/src/cloud_environment.rs` · fork ships source: NO

- **DECLINED?** — Warp Environments (#211)
  - `deserialize_environment_without_docker_image`
  - `deserialize_gitlab_environment_uses_authoritative_source_repos`
  - `deserialize_legacy_environment_without_providers`
  - `deserialize_legacy_environment_without_secrets`
  - `deserialize_with_aws_provider`
  - `deserialize_with_both_providers`
  - `deserialize_with_empty_secrets`
  - `deserialize_with_gcp_provider`
  - `deserialize_with_gcp_provider_service_account`
  - `deserialize_with_specific_secrets`
  - `legacy_environment_serialization_omits_provider_neutral_fields`
  - `present_empty_source_repos_override_legacy_mirror`
  - `roundtrip_serde_with_providers`
  - `roundtrip_serde_with_secrets`
  - `roundtrip_serde_without_docker_image`
  - `serialize_environment_without_docker_image_omits_field`
  - `serialize_with_empty_secrets_includes_field`
  - `serialize_with_providers_includes_field`
  - `serialize_with_providers_none_omits_field`
  - `serialize_with_secrets_none_omits_field`

### `crates/warp_tui/src/orchestration_block_tests.rs` — 20 absent

pin 20 · fork 0 · source `crates/warp_tui/src/orchestration_block.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `accepting_dispatches_once_and_releases_focus`  _(named in fork prose)_
  - `blocked_accept_invalidates_card_layout`  _(named in fork prose)_
  - `build_request_carries_card_fields_and_edited_run_wide_state`  _(named in fork prose)_
  - `build_request_omits_the_auth_secret_when_the_picker_is_not_applicable`  _(named in fork prose)_
  - `cloud_managed_credential_harness_inserts_the_api_key_page`  _(named in fork prose)_
  - `cloud_oz_uses_five_pages_without_the_api_key_page`  _(named in fork prose)_
  - `confirming_a_search_result_returns_focus_to_the_acceptance_card`  _(named in fork prose)_
  - `edit_state_carries_the_request_auth_secret`  _(named in fork prose)_
  - `edit_state_is_overridden_by_an_approved_config`  _(named in fork prose)_
  - `environment_and_model_pages_are_searchable`  _(named in fork prose)_
  - `environment_selector_is_searchable`  _(named in fork prose)_
  - `failed_arrow_confirmation_does_not_change_later_enter_navigation`  _(named in fork prose)_
  - `focusing_a_configuring_card_delegates_to_the_selector`  _(named in fork prose)_
  - `local_collapses_the_page_sequence_to_two_pages`  _(named in fork prose)_
  - `local_request_with_implicit_oz_harness_preserves_explicit_model`  _(named in fork prose)_
  - `model_selector_arrows_navigate_after_search_takes_focus`  _(named in fork prose)_
  - `opening_configuration_only_invalidates_layout`  _(named in fork prose)_
  - `selector_actions_commit_edits_and_follow_the_dynamic_page_sequence`  _(named in fork prose)_
  - `selector_layout_invalidations_are_forwarded`  _(named in fork prose)_
  - `unapproved_local_request_forces_oz_harness`  _(named in fork prose)_

### `app/src/settings_view/custom_inference_modal_tests.rs` — 19 absent

pin 19 · fork 0 · source `app/src/settings_view/custom_inference_modal.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `action_row_remains_fixed_when_form_scrolls`
  - `add_model_scrolls_only_after_form_is_full`
  - `endpoint_form_valid_accepts_complete_valid_form`
  - `endpoint_form_valid_rejects_invalid_current_url`
  - `endpoint_form_valid_requires_non_empty_url`
  - `focus_editor_scrolls_whole_form_to_field`
  - `modal_resizes_with_window_and_added_models`
  - `modal_with_many_models_lays_out`
  - `model_row_inputs_align_and_controls_fit_gutter`
  - `prefill_resets_form_scroll_position`
  - `selecting_schema_is_reflected_in_saved_schema`
  - `validate_url_accepts_https_with_host`
  - `validate_url_allows_empty_string`
  - `validate_url_allows_whitespace_only`
  - `validate_url_rejects_empty_host`
  - `validate_url_rejects_ftp_and_other_schemes`
  - `validate_url_rejects_http`
  - `validate_url_rejects_localhost_and_private_ips`
  - `validate_url_rejects_malformed_strings`

### `app/src/terminal/view/queued_prompts_tests.rs` — 19 absent

pin 39 · fork 20 · source `app/src/terminal/view/queued_prompts.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `cloud_setup_cleanup_events_remove_the_locked_queue_row`  _(named in fork prose)_
  - `cloud_setup_enter_does_not_queue_followup_for_third_party_harness`  _(named in fork prose)_
  - `cloud_setup_enter_queues_followup_input_when_v2_is_enabled`  _(named in fork prose)_
  - `cloud_setup_enter_queues_followup_while_setup_commands_run`  _(named in fork prose)_
  - `cloud_setup_enter_remains_blocked_when_v2_is_disabled`  _(named in fork prose)_
  - `copying_locked_initial_cloud_mode_prompt_copies_full_prompt_to_clipboard`  _(named in fork prose)_
  - `dispatched_cloud_followup_uses_locked_queue_row_when_v2_is_enabled`  _(named in fork prose)_
  - `dispatched_cloud_prompt_uses_locked_queue_row_when_v2_is_enabled`  _(named in fork prose)_
  - `enqueue_followup_prompt_appends_compact_and_row_when_v2_is_enabled`  _(named in fork prose)_
  - `enqueue_followup_prompt_appends_fork_and_compact_row_when_v2_is_enabled`  _(named in fork prose)_
  - `enqueue_followup_prompt_falls_back_to_pending_block_when_v2_is_disabled`  _(named in fork prose)_
  - `enqueue_followup_prompt_uses_supplied_conversation_id_when_v2_is_enabled`  _(named in fork prose)_
  - `enter_hint_hidden_during_inline_edit_and_for_locked_head`  _(named in fork prose)_
  - `failed_event_keeps_locked_queue_row_under_cloud_mode_setup_v2`  _(named in fork prose)_
  - `failed_event_removes_locked_queue_row_without_cloud_mode_setup_v2`  _(named in fork prose)_
  - `promptless_setup_complete_auto_sends_queued_prompt_to_viewer`  _(named in fork prose)_
  - `promptless_setup_complete_with_initial_prompt_does_not_drain_queue`  _(named in fork prose)_
  - `send_now_disabled_for_all_rows_while_initial_cloud_mode_row_is_present`  _(named in fork prose)_
  - `terminal_cloud_status_transition_drains_once_through_cloud_followup_input_event`  _(named in fork prose)_

### `crates/ai/src/agent/orchestration_config_tests.rs` — 19 absent

pin 19 · fork 0 · source `crates/ai/src/agent/orchestration_config.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `computer_use_not_in_match_check`
  - `different_harness_mismatches`
  - `different_model_mismatches`
  - `empty_harness_inherits_and_matches`
  - `empty_model_inherits_and_matches`
  - `exact_match_local`
  - `exact_match_remote`
  - `execution_mode_variant_mismatch`
  - `proto_round_trip_config_local`
  - `proto_round_trip_config_remote`
  - `proto_round_trip_config_remote_with_runner`
  - `proto_round_trip_status`
  - `remote_different_environment_mismatches`
  - `remote_different_runner_mismatches`
  - `remote_empty_env_inherits_and_matches`
  - `remote_empty_runner_inherits_and_matches`
  - `remote_matching_runner_matches`
  - `status_default_is_none`
  - `status_predicates`

### `app/src/ai/agent/conversation_tests.rs` — 18 absent

pin 36 · fork 18 · source `app/src/ai/agent/conversation.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `cli_agent_transcript_vehicle_is_excluded_from_navigation`
  - `fetched_memories_dedupes_keeping_first_position_and_latest_data`
  - `fetched_memories_is_empty_when_no_message_has_memories`
  - `fetched_memories_preserves_order_across_and_within_messages`
  - `reassign_exchange_ids_keeps_exchange_lookup_consistent`
  - `recording_span_clears_when_stop_errors`
  - `recording_span_closes_on_matching_stop_result`
  - `recording_span_ignores_failed_start`
  - `recording_span_ignores_mismatched_stop_id`
  - `recording_span_stays_open_without_stop_result`
  - `restored_conversation_ignores_legacy_root_task_is_optimistic_flag_with_empty_tasks`
  - `restored_conversation_ignores_legacy_root_task_is_optimistic_flag_with_non_empty_tasks`
  - `restored_conversation_with_empty_task_list_creates_in_progress_optimistic_root`
  - `usage_totals_reads_gui_credits_and_accumulates_provider_cost`
- **DECLINED?** — CustomEndpoint / custom_model_providers (#142, #347)
  - `footer_model_token_usage_keeps_custom_endpoint_usage_distinct_from_same_labeled_models`
  - `footer_model_token_usage_preserves_unresolved_custom_endpoint_usage_with_fallback_label`
  - `update_cost_and_usage_resolves_custom_endpoint_alias_for_footer_usage`
  - `update_cost_and_usage_uses_fallback_label_for_unknown_custom_endpoint`

### `app/src/ai/blocklist/handoff/pipeline_tests.rs` — 18 absent

pin 18 · fork 0 · source `app/src/ai/blocklist/handoff/pipeline.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `api_key_snapshot_keeps_named_selection_while_loading`
  - `api_key_snapshot_lists_skip_then_names`
  - `api_key_snapshot_maps_inherit_and_unset_selection`
  - `environment_snapshot_puts_empty_option_first`
  - `harness_snapshot_excludes_gemini_and_selects_initial`
  - `harness_snapshot_filters_product_disabled_local_harness`
  - `harness_snapshot_keeps_cloud_opencode_selectable`
  - `harness_snapshot_marks_missing_local_cli_disabled_and_sorts_last`
  - `harness_snapshot_marks_server_disabled_entries`
  - `harness_snapshot_matches_selection_by_display_name_for_stale_cache`
  - `host_snapshot_dedupes_connected_and_recent_against_known_rows`
  - `host_snapshot_orders_default_warp_connected_recent`
  - `non_oz_model_snapshot_falls_back_to_default_for_unknown_or_empty_id`
  - `non_oz_model_snapshot_puts_default_first_and_selects_server_model`
  - `oz_model_snapshot_carries_disabled_reason`
  - `oz_model_snapshot_empty_catalog_reports_empty_status`
  - `runner_snapshot_loading_reports_loading_status`
  - `runner_snapshot_puts_use_default_first_and_selects`

### `app/src/server/telemetry/secret_redaction_tests.rs` — 18 absent

pin 18 · fork 0 · source `app/src/server/telemetry/secret_redaction.rs` · fork ships source: NO

- **DECLINED?** — Telemetry channel physically removed
  - `compose_patterns_dedups_enterprise_pattern_that_matches_a_user_pattern`
  - `compose_patterns_dedups_user_pattern_that_matches_a_default`
  - `compose_patterns_includes_defaults_when_user_and_enterprise_are_empty`
  - `compose_patterns_layers_user_and_enterprise_on_top_of_defaults`
  - `redact_secrets_in_string_redacts_multiple_independent_secrets`
  - `redact_secrets_in_string_redacts_single_secret_in_middle`
  - `redact_secrets_in_string_redacts_string_that_is_entirely_a_secret`
  - `redact_secrets_in_string_with_no_match_is_noop`
  - `redact_secrets_in_value_leaves_non_string_scalars_untouched`
  - `redact_secrets_in_value_recurses_into_nested_structures`
  - `redact_secrets_in_value_redacts_strings_in_arrays`
  - `redact_secrets_in_value_redacts_strings_in_objects`
  - `replace_byte_ranges_with_asterisks_handles_fully_contained_range`
  - `replace_byte_ranges_with_asterisks_handles_unsorted_ranges`
  - `replace_byte_ranges_with_asterisks_merges_adjacent_ranges`
  - `replace_byte_ranges_with_asterisks_merges_overlapping_ranges`
  - `replace_byte_ranges_with_asterisks_replaces_independent_ranges`
  - `replace_byte_ranges_with_asterisks_with_empty_ranges_is_noop`

### `app/src/ai/agent_sdk/driver_tests.rs` — 17 absent

pin 49 · fork 32 · source `app/src/ai/agent_sdk/driver.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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
  - `overlap_repo_in_env_and_global_loads_all_skills_without_duplicates`
  - `split_loading_env_loads_all_global_loads_subset`
  - `well_known_resolution_failure_does_not_drop_other_specs`
  - `well_known_resolution_failure_skips_server`
  - `well_known_spec_is_skipped_when_flag_disabled`
  - `well_known_spec_resolves_via_managed_client`

### `app/src/remote_server/diff_state_tracker_tests.rs` — 17 absent

pin 19 · fork 2 · source `app/src/remote_server/diff_state_tracker.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** Name collision, not a gap. The pin's `diff_state_tracker.rs` is a `RemoteDiffStateManager` entity holding a `LocalDiffStateModel` per `(repo, mode)`; this fork's file of the same name is an unrelated per-repository git watch (#577), and the subscription lifecycle the pin tests lives inline on `ServerModel` (`diff_state_subscribers` + `diff_state_keys_by_conn`, #324). The equivalent assertions are already ported to `app/src/remote_server/server_model_tests.rs`, which says so in a comment at line 351. All 17 permanently unported.

- **DIVERGENT**
  - `add_and_drain_pending_responses`
  - `different_modes_are_different_keys`
  - `different_repos_are_different_keys`
  - `drain_pending_responses_returns_empty_for_unknown_key`
  - `get_model_returns_none_when_empty`
  - `has_pending_responses_false_when_empty`
  - `insert_and_get_model`
  - `multiple_pending_responses_for_same_key`
  - `remove_connection_clears_pending_responses`
  - `remove_connection_keeps_models_with_other_subscribers`
  - `remove_connection_unsubscribes_from_all_keys`
  - `remove_model_clears_pending_and_subscriptions`
  - `subscribe_registers_connection`  _(named in fork prose)_
  - `subscribed_connections_returns_empty_for_unknown_key`
  - `unsubscribe_clears_pending_responses_for_that_connection`
  - `unsubscribe_last_connection_removes_model`
  - `unsubscribe_one_of_two_keeps_model`

### `app/src/settings/cloud_preferences_syncer_tests.rs` — 17 absent

pin 17 · fork 0 · source `app/src/settings/cloud_preferences_syncer.rs` · fork ships source: NO

- **DECLINED?** — cloud preference sync
  - `test_cloud_pref_not_synced_when_current_value_not_syncable`
  - `test_cloud_preferences_setting_enabling_setting_syncs_prefs`
  - `test_cloud_preferences_setting_initial_load_skipped_when_setting_is_off`
  - `test_ensure_no_duplicate_cloud_prefs`
  - `test_file_missing_with_stored_hash_lets_cloud_win`
  - `test_first_launch_with_no_stored_hash_lets_cloud_win`
  - `test_force_local_suppressed_when_file_is_broken`
  - `test_force_local_wins_on_startup_uploads_local_to_cloud`
  - `test_no_force_local_when_hashes_match`
  - `test_offline_ui_change_does_not_update_hash_until_sync_succeeds`
  - `test_sync_cloud_pref_to_local_on_initial_load_or_collab_update`
  - `test_sync_local_pref_to_cloud_after_initial_sync`
  - `test_sync_local_pref_to_cloud_after_initial_sync_creates_prefs_setting`
  - `test_sync_local_pref_to_cloud_doesnt_update_equal_pref`
  - `test_sync_local_pref_to_cloud_on_initial_sync_for_first_time_user`
  - `test_sync_local_pref_to_cloud_on_initial_sync_for_returning_user`
  - `test_sync_local_pref_to_cloud_updates_existing_pref`

### `app/src/terminal/shared_session/sharer/network_tests.rs` — 17 absent

pin 17 · fork 0 · source `app/src/terminal/shared_session/sharer/network.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_events_are_saved_on_send_and_removed_on_ack`  _(named in fork prose)_
  - `test_handle_non_pty_read_event_while_batching`  _(named in fork prose)_
  - `test_handle_non_pty_read_event_while_not_batching`  _(named in fork prose)_
  - `test_handle_pty_read_event_while_batching`  _(named in fork prose)_
  - `test_handle_pty_read_event_while_not_batching`  _(named in fork prose)_
  - `test_ignore_duplicate_prompt_updates`  _(named in fork prose)_
  - `test_messages_are_buffered_before_session_initialized`  _(named in fork prose)_
  - `test_messages_are_buffered_while_reconnecting`  _(named in fork prose)_
  - `test_selection_updates_throttled_and_duplicates_ignored`  _(named in fork prose)_
  - `test_send_ordered_terminal_event_message_advances_event_no`  _(named in fork prose)_
  - `test_send_ordered_terminal_event_message_max_reached`  _(named in fork prose)_
  - `test_send_pty_read_event_while_batching`  _(named in fork prose)_
  - `test_send_pty_read_event_while_not_batching`  _(named in fork prose)_
  - `test_should_retry_startup_failure_respects_attempt_budget`  _(named in fork prose)_
  - `test_startup_attempt_stale_filtering`  _(named in fork prose)_
  - `test_startup_failure_retryability`  _(named in fork prose)_
  - `test_startup_max_attempts_only_retries_ambient_agent_sources`  _(named in fork prose)_

### `crates/onboarding/src/model_tests.rs` — 17 absent

pin 17 · fork 0 · source `crates/onboarding/src/model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `account_first_path_uses_agent_ui_defaults`  _(named in fork prose)_
- **PORTABLE?**
  - `account_first_path_is_linear_and_reversible`  _(named in fork prose)_
  - `account_first_path_uses_three_step_progress`  _(named in fork prose)_
  - `agent_intent_keeps_ai_enabled_for_any_setup_choice`
  - `agent_path_routes_through_ai_setup`
  - `cancel_no_ai_from_intention_routes_to_ai_setup`
  - `confirm_no_ai_from_intention_then_back_returns_to_intention`
  - `confirm_no_ai_switches_to_terminal_path`
  - `dismiss_no_ai_closes_without_changing_path`
  - `post_auth_offer_is_unclassified_until_selected_and_does_not_switch`  _(named in fork prose)_
  - `post_auth_offer_supports_back_to_theme_and_no_direct_next`  _(named in fork prose)_
  - `progress_reports_terminal_path_uses_three_dot_variant`
  - `progress_reports_v3_positions_for_agent_path`
  - `progress_reports_v3_positions_for_third_party_path`
  - `terminal_path_skips_third_party`
  - `terminal_settings_disable_ai`
  - `third_party_choice_routes_to_third_party_slide`

### `app/src/ai/blocklist/action_model/execute/run_agents_tests.rs` — 16 absent

pin 16 · fork 0 · source `app/src/ai/blocklist/action_model/execute/run_agents.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `cancel_during_plan_publication_does_not_dispatch_children`
- **DECLINED?** — Agent-invoked agent spawning / RunAgents (#325, #290)
  - `local_codex_run_agents_maps_to_local_harness_mode_when_flag_enabled`
- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `autonomous_mode_autoexecutes_and_does_not_deny_missing_api_key`
  - `execute_denies_disapproved_plan_config`
  - `execute_denies_duplicate_launched_agent`
  - `execute_denies_never_allow_profile_setting`
  - `execute_denies_remote_non_warp_harness_without_default_auth_secret`
  - `execute_publishes_every_parent_owned_plan_before_dispatch`
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

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

### `app/src/search/slash_command_menu/static_commands/commands_tests.rs` — 15 absent

pin 27 · fork 12 · source `app/src/search/slash_command_menu/static_commands/commands.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** The whole file is written against `all_commands(settings::SettingsMode)` and a per-command `supported_surfaces: SlashCommandSurfaces` field. This fork derives surfaces from the command name instead (`StaticCommand::supports_tui`, `supports_gui` -- see the doc comment at `static_commands/mod.rs:289`) and `SettingsMode` is documented as dropped. Nothing to run these against.

- **DIVERGENT**
  - `add_api_key_command_is_tui_only_and_requires_a_provider`
  - `clear_api_key_command_is_tui_only_and_requires_a_provider`
  - `clear_command_is_active_only_outside_cloud_mode`  _(named in fork prose)_
  - `clear_command_is_registered_only_for_tui_mode`
  - `command_names_and_kinds_are_unique_per_surface`  _(named in fork prose)_
  - `command_registry_filters_explicit_surface_metadata`
  - `continue_locally_command_is_registered`
  - `gui_icon_metadata_matches_surface_support`
  - `logout_command_is_registered_only_for_tui_mode`
  - `natural_language_detection_command_is_registered_only_for_tui_mode`
  - `rename_conversation_command_is_active_conversation_scoped_and_requires_argument`
  - `statusline_command_is_always_available_only_in_tui_mode`
  - `theme_command_is_registered_only_for_tui_mode`  _(named in fork prose)_
  - `view_logs_command_is_registered_only_for_tui_mode`  _(named in fork prose)_
  - `voice_command_is_registered_only_for_tui_mode`

### `app/src/ai/agent_sdk/mod_tests.rs` — 14 absent

pin 14 · fork 0 · source `app/src/ai/agent_sdk/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `artifact_download_requires_auth`
  - `artifact_get_requires_auth`
  - `artifact_upload_requires_auth`
  - `reconcile_task_harness_adopts_task_harness_when_cli_uses_default`
  - `reconcile_task_harness_allows_matching_explicit_harness`
  - `reconcile_task_harness_rejects_explicit_mismatch`
  - `run_message_send_requires_auth`
  - `run_message_send_telemetry_defaults_to_unknown_harness`
  - `run_message_send_telemetry_supports_claude_code_alias`
  - `run_message_send_telemetry_supports_opencode_harness`
  - `run_message_send_telemetry_uses_canonical_harness_from_env`
  - `run_message_watch_telemetry_defaults_to_unknown_harness`
- **DECLINED?** — /logout slash command (#338)
  - `logout_does_not_require_auth`
- **DECLINED?** — Account-first onboarding (#11)
  - `login_does_not_require_auth`

### `app/src/ai/blocklist/action_model/recording_controller_tests.rs` — 14 absent

pin 14 · fork 0 · source `app/src/ai/blocklist/action_model/recording_controller.rs` · fork ships source: NO

- **DECLINED?** — screen recording (#367)
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

### `app/src/workspace/view_tests.rs` — 14 absent

pin 85 · fork 71 · source `app/src/workspace/view.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `copy_model_and_profile_preserves_explicit_model_over_source_profile_default`  _(named in fork prose)_
  - `test_active_tab_bar_position_id_tracks_layout`  _(named in fork prose)_
  - `test_new_session_menu_is_capped_to_window_height`  _(named in fork prose)_
  - `test_open_cloud_agent_setup_guide_action_opens_management_view_and_is_idempotent`  _(named in fork prose)_
  - `test_open_file_notebook_focuses_existing_markdown_pane`  _(named in fork prose)_
  - `test_open_vertical_tabs_panel_is_idempotent`  _(named in fork prose)_
  - `test_reward_modal_no_overlap`  _(named in fork prose)_
  - `test_reward_modal_shows_for_received_referral`  _(named in fork prose)_
  - `test_stop_sharing_all_sessions_in_tab`  _(named in fork prose)_
  - `test_stop_sharing_session`  _(named in fork prose)_
  - `test_tab_bar_traffic_light_space_regression_for_resource_center_overlap`  _(named in fork prose)_
  - `test_tab_context_menu_share_session_items`  _(named in fork prose)_
  - `test_tools_panel_preferences_activate_after_signup_and_ai_enablement`  _(named in fork prose)_
  - `test_tools_panel_warp_drive_toggle_updates_available_views`  _(named in fork prose)_

### `app/src/ai/agent/api/impl_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/ai/agent/api/impl.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **PORTABLE?**
  - `parse_project_skill_contents_classifies_foreign_encoded_provider_path`
  - `parse_project_skill_contents_preserves_remote_paths`
  - `test_handle_repository_update_non_skill_directory_added_does_not_emit_project_event`  _(near-name fork test `test_handle_repository_update_non_skill_file_added_does_not_queue_project_directory`)_
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

### `app/src/server/sync_queue_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/server/sync_queue.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_create_and_update_notebook`
  - `test_create_notebook`
  - `test_dequeue_after_transient_failure`
  - `test_generic_string_object_unique_key_failure`
  - `test_initial_queue_items_processed_properly`
  - `test_no_dequeue_after_intransient_failure`
  - `test_record_object_action`
  - `test_sync_queue_bulk_generic_string_object_update_waits_for_matching_create`
  - `test_sync_queue_dependency_failure`
  - `test_sync_queue_dependency_mixed_ids`
  - `test_sync_queue_dependency_successes`
  - `test_sync_queue_enum_dependency`
  - `test_sync_queue_generic_string_object_update_depends_on_pending_create`

### `app/src/terminal/shared_session/viewer/event_loop_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/terminal/shared_session/viewer/event_loop.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `command_execution_finished_defers_queued_command_advance_until_block_completion`  _(named in fork prose)_
  - `command_execution_started_preserves_draft_for_queued_command`  _(named in fork prose)_
  - `new_viewer_processes_old_sharer_lifecycle_stream`  _(named in fork prose)_
  - `test_append_followup_replay_marks_existing_conversations_suppressible`  _(named in fork prose)_
  - `test_append_followup_scrollback_skips_duplicates`  _(named in fork prose)_
  - `test_append_followup_scrollback_with_completed_last_block_creates_active_block`  _(named in fork prose)_
  - `test_cloud_mode_setup_phase_ended_clears_setup_state`  _(named in fork prose)_
  - `test_cloud_mode_setup_phase_ended_is_idempotent`  _(named in fork prose)_
  - `test_cloud_mode_setup_phase_ended_when_flag_already_false`  _(named in fork prose)_
  - `test_fresh_session_replay_does_not_suppress_existing_conversations`  _(named in fork prose)_
  - `test_out_of_order_buffering`  _(named in fork prose)_
  - `test_pty_bytes_buffered_before_command_execution_started`  _(named in fork prose)_
  - `test_terminal_model_is_correct`  _(named in fork prose)_

### `app/src/terminal/view/ambient_agent/model_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/terminal/view/ambient_agent/model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `duplicate_handoff_completion_is_ignored`  _(named in fork prose)_
  - `followup_github_auth_does_not_reuse_stored_initial_request`  _(named in fork prose)_
  - `github_auth_completed_retries_stored_initial_run_request`  _(named in fork prose)_
  - `github_auth_url_for_initial_run_includes_focus_cloud_mode_next`  _(named in fork prose)_
  - `handoff_cancellation_is_signalled_and_late_failure_is_ignored`  _(named in fork prose)_
  - `record_ambient_execution_ended_clears_active_session_and_enables_followup`  _(named in fork prose)_
  - `record_ambient_execution_ended_keeps_active_session_when_id_differs`  _(named in fork prose)_
  - `set_live_execution_session_marks_session_live_until_it_ends`  _(named in fork prose)_
  - `spawn_agent_omits_orchestration_handoff_for_fresh_launches`  _(named in fork prose)_
  - `spawn_config_falls_back_to_auto_only_for_non_cloud_runnable_model`  _(named in fork prose)_
  - `spawn_config_honors_pane_model_override`  _(named in fork prose)_
  - `viewed_task_config_applies_oz_model_override`  _(named in fork prose)_
  - `viewed_task_config_preserves_environment_before_cloud_model_load`  _(named in fork prose)_

### `app/src/ai/agent_events/driver_tests.rs` — 12 absent

pin 19 · fork 7 · source `app/src/ai/agent_events/driver.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `actionable_stream_status_reports_only_at_threshold_crossing`
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

### `app/src/cloud_object/model/model_tests.rs` — 12 absent

pin 27 · fork 15 · source `app/src/cloud_object/model/model.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_force_refresh_correctly_resets_timestamp`
  - `test_force_refresh_only_happens_once`
  - `test_load_cloud_objects_on_initial_load_with_empty_cache`
  - `test_loading_all_cloud_objects_after_switching_from_offline`
  - `test_update_folder_timestamp_from_child_trash`
  - `test_update_folder_timestamp_from_child_update`
  - `test_update_folder_timestamp_from_new_child`
  - `test_update_folder_timestamp_from_object_move`
  - `test_update_object_server_id_for_folder`
  - `test_update_object_server_id_for_notebook`
  - `test_update_object_server_id_for_workflow`
  - `test_update_with_deleted_objects`

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_common_tests.rs` — 12 absent

pin 12 · fork 0 · source `app/src/settings_view/billing_and_usage/billing_cycle_usage_common.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `aggregate_segments_merges_dupes_drops_zeros_and_sorts`
  - `has_non_viewer_data_returns_false_when_entries_empty`
  - `has_non_viewer_data_returns_false_when_only_viewer_user_rows`
  - `has_non_viewer_data_returns_true_for_other_user_row`
  - `has_non_viewer_data_returns_true_for_service_account_row`
  - `has_non_viewer_data_returns_true_for_team_aggregate_row`
  - `has_non_viewer_data_treats_missing_subject_uid_as_non_viewer`
  - `has_non_viewer_data_treats_missing_viewer_uid_as_non_viewer`
  - `legend_cost_types_excludes_legacy_only_buckets`
  - `legend_cost_types_excludes_zero_credit_bucket`
  - `legend_cost_types_includes_used_buckets_in_display_order`
- **DECLINED?** — Voice input (#389, #352)
  - `filter_legacy_buckets_drops_voice_and_suggested_code_diffs_in_input_order`

### `app/src/util/file/external_editor/linux_tests.rs` — 12 absent

pin 29 · fork 17 · source `app/src/util/file/external_editor/linux.rs` · fork ships source: yes

- **DIVERGENT** — duplicate: covered by the fork's `test_remaining_substitutions`
  - `test_field_code_substitutions`
- **DIVERGENT** — duplicate: the fork's `test_exec_ending_on_percent_fails` asserts the same `DesktopExecError::MalformedFieldCode` on a trailing bare `%`
  - `test_bare_percent_at_end_errors`
- **PORTABLE?**
  - `test_mixed_quoted_and_unquoted_in_single_token`
  - `test_tokenize_empty_string`
  - `test_tokenize_escape_sequences_in_quotes`
  - `test_tokenize_multiple_whitespace`
  - `test_tokenize_quoted_argument`
  - `test_tokenize_quoted_empty_string_produces_token`
  - `test_tokenize_simple`
  - `test_tokenize_unrecognized_escape_in_quotes_keeps_backslash`
  - `test_tokenize_unterminated_quote_errors`
  - `test_unterminated_quote_errors`

### `crates/ai/src/geap_credentials_tests.rs` — 12 absent

pin 12 · fork 0 · source `crates/ai/src/geap_credentials.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `admin_config_status_flags_only_non_429_4xx`
  - `exchange_token_4xx_requires_admin`
  - `exchange_token_transient_is_user_retryable`
  - `failed_state_components_match_error_copy`
  - `impersonation_4xx_requires_admin`
  - `impersonation_transient_is_user_retryable`
  - `loaded_state_shows_scheduled_refresh_instead_of_expiry`
  - `mint_identity_token_failure_is_user_retryable`
  - `state_components_use_expected_icons`
  - `state_recovery_action_for_failed_and_unconfigured`
  - `unconfigured_state_points_user_to_admin_setup`
  - `user_facing_never_leaks_raw_provider_detail`

### `crates/warp_tui/src/input/view_tests.rs` — 12 absent

pin 89 · fork 77 · source `crates/warp_tui/src/input/view.rs` · fork ships source: yes

- **DECLINED?** — Voice input (#389, #352)
  - `listening_voice_input_suppresses_shell_gutter`  _(named in fork prose)_
- **PORTABLE?**
  - `completion_can_append_a_space_at_buffer_end`  _(named in fork prose)_
  - `completion_replaces_utf8_byte_span_and_preserves_following_text`  _(named in fork prose)_
  - `enter_and_escape_stop_listening_while_escape_cancels_transcribing`  _(named in fork prose)_
  - `move_left_from_shortcuts_replaces_it_with_conversation_menu`  _(named in fork prose)_
  - `question_mark_at_empty_shell_input_toggles_shortcuts`  _(named in fork prose; near-name fork test `question_mark_at_empty_agent_input_toggles_shortcuts`)_
  - `tab_cycles_open_completion_menu_and_enter_applies_selection`  _(named in fork prose)_
  - `tab_is_consumed_by_an_existing_non_completion_menu`  _(named in fork prose)_
  - `tab_requests_completion_for_detected_shell_input`  _(named in fork prose)_
  - `tab_requests_completion_only_in_shell_mode_without_submitting`  _(named in fork prose)_
  - `typing_into_an_open_shortcuts_surface_closes_it_and_inserts`  _(named in fork prose)_
  - `up_from_shortcuts_replaces_it_with_prompt_and_command_history`  _(named in fork prose)_

### `app/src/ai/agent_sdk/driver/attachments_tests.rs` — 11 absent

pin 11 · fork 0 · source `app/src/ai/agent_sdk/driver/attachments.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
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

### `app/src/terminal/model/terminal_model_tests.rs` — 11 absent

pin 49 · fork 38 · source `app/src/terminal/model/terminal_model.rs` · fork ships source: yes

- **PORTABLE?**
  - `cloud_mode_deferred_terminal_model_starts_view_pending`  _(named in fork prose)_
  - `cloud_mode_setup_phase_ended_does_not_emit_when_not_sharing`  _(named in fork prose)_
  - `cloud_mode_setup_phase_ended_emits_when_sharing`  _(named in fork prose)_
  - `generic_shared_session_viewer_model_starts_view_pending`  _(named in fork prose)_
  - `is_cloud_agent_conversation_only_true_for_genuine_ambient_sessions`  _(named in fork prose)_
  - `precmd_with_completion_metadata_records_completion_mismatch_without_overwriting_completed_block`  _(named in fork prose)_
  - `precmd_with_completion_metadata_recovers_in_band_completion_and_reuses_cached_prompt`  _(named in fork prose)_
  - `repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored`  _(named in fork prose; near-name fork test `repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored_when_recovery_is_disabled`)_
  - `sharer_rejects_dcs_hook_with_unregistered_session_id`  _(named in fork prose)_
  - `ssh_bootstraps_if_blocklist_empty_and_reconciles_parent_return`  _(named in fork prose)_
  - `viewer_processes_dcs_hook_with_unregistered_session_id`  _(named in fork prose)_

### `app/src/tracing/cloud_agent_auth_tests.rs` — 11 absent

pin 11 · fork 0 · source `app/src/tracing/cloud_agent_auth.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `authorization_overwrites_supplied_header`
  - `authorized_request_debug_redacts_token`
  - `debug_output_redacts_token`
  - `expected_run_id_is_required`
  - `expired_token_is_refused_and_supplied_header_is_removed`
  - `malformed_refreshed_tokens_are_rejected`
  - `refreshed_token_run_id_exactly_matches`
  - `refreshed_token_run_id_is_required`
  - `refreshed_token_run_id_must_be_a_string`
  - `refreshed_token_run_id_must_match`
  - `rejected_refreshed_token_preserves_previous_token`

### `app/src/ai/active_agent_views_model_tests.rs` — 10 absent

pin 10 · fork 0 · source `app/src/ai/active_agent_views_model.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `converts_graphql_file_artifact`
  - `default_download_filename_falls_back_to_artifact_uid_with_extension`
  - `default_download_filename_prefers_server_filename`
  - `download_success_message_includes_filename_and_directory`
  - `file_button_label_falls_back_to_filepath_basename`
  - `file_button_label_falls_back_to_generic_label`
  - `file_button_label_prefers_filename`
  - `resolves_lightbox_image_for_screenshot_artifact`
  - `returns_failure_placeholder_for_screenshot_load_errors`
  - `skips_lightbox_update_for_non_screenshot_artifact`

### `app/src/ai/execution_profiles/editor/mod_tests.rs` — 10 absent

pin 18 · fork 8 · source `app/src/ai/execution_profiles/editor/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `non_openai_configurable_context_ignores_gpt_flag_and_does_not_show_openai_warning`
  - `openai_configurable_context_does_not_require_direct_host_metadata`
  - `openai_configurable_context_uses_server_metadata_without_model_or_host_allowlist`
  - `openai_expanded_context_is_hidden_while_feature_flag_is_off`
  - `openai_fixed_context_metadata_does_not_expose_control_or_warning`
  - `openai_long_context_warning_clamps_stale_override_to_lowered_model_max`
  - `openai_long_context_warning_starts_above_threshold`
  - `openai_request_limit_is_clamped_when_configurable_context_is_available`
  - `openai_request_limit_remains_unset_without_a_selected_override`
- **DECLINED?** — CustomEndpoint / custom_model_providers (#142, #347)
  - `custom_endpoint_fixed_context_does_not_expose_control_or_warning`

### `app/src/ai/orchestration/edit_state_tests.rs` — 10 absent

pin 10 · fork 0 · source `app/src/ai/orchestration/edit_state.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
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

### `app/src/pane_group/pane/local_harness_launch_tests.rs` — 10 absent

pin 18 · fork 8 · source `app/src/pane_group/pane/local_harness_launch.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `local_child_task_config_records_supported_third_party_harnesses`
  - `local_child_task_config_returns_none_for_oz_and_unknown`
  - `local_child_task_config_stamps_orchestrator_name`
  - `local_child_task_config_trims_whitespace_only_name`
  - `normalize_orchestrator_agent_name_trims_and_drops_empty`
  - `prepare_local_claude_child_merges_anthropic_model_env_var`  _(named in fork prose)_
  - `prepare_local_claude_child_no_anthropic_model_when_empty`
  - `prepare_local_codex_child_launch_rejects_without_rewriting_global_codex_state`
  - `prepare_local_codex_child_launch_succeeds_when_testing_flag_is_enabled`
  - `prepare_local_harness_child_launch_rejects_disabled_codex_before_shell_validation`

### `app/src/ai/agent_sdk/driver/harness/claude_code_tests.rs` — 9 absent

pin 34 · fork 25 · source `app/src/ai/agent_sdk/driver/harness/claude_code.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `claude_command_uses_resume_flag_when_resuming`
  - `message_bridge_cleanup_preserves_state_for_wakeable_runs`
  - `message_bridge_cleanup_removes_state_for_non_wakeable_runs`
  - `parent_bridge_event_cursor_defaults_to_zero_when_missing`
  - `parent_bridge_event_cursor_round_trips`
  - `prepare_local_wake_command_rehydrates_transcript_with_self_managed_listener`  _(named in fork prose)_
  - `prime_parent_bridge_staged_for_self_managed_wake_keeps_message_in_staged`  _(named in fork prose)_
  - `resolve_suffix_from_resolved_env_vars`  _(named in fork prose)_
  - `write_session_index_entry_creates_expected_entry`  _(named in fork prose)_

### `app/src/ai/skills/skill_manager_tests.rs` — 9 absent

pin 25 · fork 16 · source `app/src/ai/skills/skill_manager.rs` · fork ships source: yes

- **CLOUD** — `SkillManager::set_cloud_environment` feeds `driver.rs::load_environment_skills`, whose `SourceRepo` comes from `cloud_object_models`
  - `cloud_environment_skills_always_included`
- **DIVERGENT** — bundled skill id `tui-migrate-setup` does not exist here; the fork ships `tui-settings` instead (TODO.md:231, bundled_tests.rs:131)
  - `tui_migration_skill_has_tui_only_activation`  _(named in fork prose)_
- **DIVERGENT** — the fork's `read_bundled_skills` takes one argument (no `resources_dir`) and documents `{{skill_dir}}` as absent (`bundled.rs:503`)
  - `test_read_bundled_skills_renders_host_paths`
- **PORTABLE?**
  - `remote_home_provider_variants_are_available_for_provider_selection`
  - `remote_home_provider_variants_are_scoped_to_the_descriptor_host`
  - `remote_home_skill_replaces_an_overlapping_index_entry`
- **PORTED**
  - `active_skill_by_reference_distinguishes_remote_hosts_with_the_same_display_path`
  - `active_skill_by_reference_resolves_exact_remote_identity`
  - `active_skill_by_reference_with_origin_returns_typed_lookup_errors`

### `app/src/persistence/sqlite_tests.rs` — 9 absent

pin 20 · fork 11 · source `app/src/persistence/sqlite.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `app_scope_database_path_matches_app_database_path`
  - `database_path_for_current_scope_defaults_to_app_scope`
  - `remote_server_daemon_database_permissions_are_owner_only`
  - `remote_server_daemon_scope_database_path_handles_empty_identity_key`
  - `remote_server_daemon_scope_database_path_uses_identity_data_dir`
  - `sqlite_read_restores_app_state_and_codebase_metadata`
  - `sqlite_writer_reuses_codebase_index_metadata_events`
  - `tui_database_in_tui_subdirectory_round_trips_data`
  - `tui_scope_database_path_is_tui_subdirectory_of_app_database_dir`

### `app/src/server/server_api/presigned_upload_tests.rs` — 9 absent

pin 9 · fork 0 · source `app/src/server/server_api/presigned_upload.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `encode_crc32c_base64_matches_spec_example`
  - `file_upload_body_hashes_without_buffering`
  - `multipart_post_requires_content_data_field`
  - `upload_file_to_target_builds_multipart_post_and_returns_computed_crc32c`
  - `upload_file_to_target_replays_headers_sets_content_length_and_returns_checksum`
  - `upload_file_to_target_returns_status_and_body_for_failed_uploads`
  - `upload_to_target_builds_multipart_post_with_static_crc_and_data_fields`
  - `upload_to_target_replays_headers_for_byte_uploads`
  - `vec_upload_body_hashes_its_buffer`

### `app/src/settings_view/mod_tests.rs` — 9 absent

pin 52 · fork 43 · source `app/src/settings_view/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `arrow_down_across_adjacent_collapsed_umbrellas`  _(named in fork prose)_
  - `arrow_down_collapsed_umbrella_respects_search_filter`  _(named in fork prose; near-name fork test `arrow_down_wrapping_into_collapsed_umbrella_respects_search_filter`)_
  - `arrow_down_from_account_with_collapsed_agents_lands_on_first_subpage`
  - `arrow_up_from_billing_and_usage_with_collapsed_agents_lands_on_last_subpage`  _(near-name fork test `arrow_up_from_code_with_collapsed_agents_lands_on_last_subpage`)_
  - `cloud_platform_subpages_are_identified`  _(named in fork prose)_
  - `cloud_platform_subpages_map_to_their_backing_pages`  _(named in fork prose)_
  - `code_subpages_are_identified`  _(named in fork prose; near-name fork test `ai_subpages_are_identified`)_
  - `code_subpages_map_to_code_backing_page`  _(named in fork prose; near-name fork test `ai_subpages_map_to_ai_backing_page`)_
  - `search_terms_match_direct_unit_checks`  _(named in fork prose)_

### `app/src/terminal/input/slash_commands/mod_tests.rs` — 9 absent

pin 11 · fork 2 · source `app/src/terminal/input/slash_commands/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `auto_approve_is_an_exact_no_argument_command`  _(named in fork prose)_
  - `cloud_mode_v2_commands_are_active_only_in_cloud_mode_v2_context`  _(named in fork prose)_
  - `exit_command_executes_immediately_and_takes_no_argument`  _(named in fork prose)_
  - `natural_language_detection_command_is_supported_in_tui`  _(named in fork prose)_
  - `not_cloud_agent_commands_are_only_active_outside_cloud_mode`  _(named in fork prose)_
  - `slash_command_is_submitted_as_prompt_only_for_prompt_commands`  _(named in fork prose)_
  - `theme_command_inserts_input_for_its_required_argument`  _(named in fork prose)_
  - `tui_commands_have_typed_identities_and_explicit_surface_support`  _(named in fork prose)_
- **DECLINED?** — /logout slash command (#338)
  - `logout_command_executes_immediately_and_takes_no_argument`  _(named in fork prose)_

### `app/src/uri/uri_tests.rs` — 9 absent

pin 65 · fork 56 · source `app/src/uri/uri.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_action_auto_handoff_to_cloud_parse_alias_path`
  - `test_action_auto_handoff_to_cloud_parse_default_trigger`
  - `test_action_auto_handoff_to_cloud_parse_sleep_trigger`
  - `test_action_cloud_agent_setup_parse`  _(named in fork prose)_
  - `test_action_create_environment_parse`  _(named in fork prose)_
  - `test_action_create_environment_parse_no_repos`
  - `test_action_focus_cloud_mode_parse`  _(named in fork prose)_
  - `test_action_new_cloud_agent_conversation_parse`  _(named in fork prose; near-name fork test `test_action_new_agent_conversation_parse`)_
  - `test_app_web_link_rewrites_to_new_cloud_agent_conversation`  _(named in fork prose)_

### `app/src/workspace/auto_handoff_tests.rs` — 9 absent

pin 9 · fork 0 · source `app/src/workspace/auto_handoff.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `auto_handoff_skips_already_attempted_conversations`
  - `auto_handoff_skips_conversations_that_cannot_handoff_to_cloud`
  - `auto_handoff_skips_empty_conversations`
  - `auto_handoff_skips_idle_conversations`
  - `auto_handoff_skips_non_cloud_runnable_models`
  - `auto_handoff_skips_orchestrator_with_local_children`
  - `auto_handoff_skips_shared_session_viewers`
  - `auto_handoff_skips_unsynced_conversations`
  - `eligible_running_synced_conversation_is_not_skipped`

### `app/src/ai/agent_sdk/driver/git_credentials_tests.rs` — 8 absent

pin 8 · fork 0 · source `app/src/ai/agent_sdk/driver/git_credentials.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
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

- **DECLINED?** — screen recording (#367)
  - `start_recording_card_text_includes_failure_copy`
  - `start_recording_card_text_uses_static_title_and_description_subtext`
- **PORTABLE?**
  - `format_upload_artifact_text_includes_request_details`
  - `format_upload_artifact_text_includes_success_summary`
  - `format_upload_artifact_text_includes_terminal_status`
  - `stop_recording_card_text_includes_complete_duration`
  - `stop_recording_card_text_includes_partial_duration_without_raw_reason`
  - `use_computer_decoration_skips_screenshot_only_rows`

### `app/src/ai/blocklist/usage/rollup_tests.rs` — 8 absent

pin 8 · fork 0 · source `app/src/ai/blocklist/usage/rollup.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
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

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_from_conversation_metadata_passes_harness_through`
  - `test_from_conversation_populates_local_conversation_fields`
  - `test_from_conversation_prefers_server_creator_profile`
  - `test_from_task_includes_linked_directory_when_run_id_matches`
  - `test_from_task_includes_linked_directory_when_server_token_matches`
  - `test_from_task_populates_executor`
  - `test_from_task_resolves_harness`
  - `test_oz_run_url_present_for_task_and_absent_for_conversation`

### `app/src/settings_view/agent_assisted_environment_modal_tests.rs` — 8 absent

pin 8 · fork 0 · source `app/src/settings_view/agent_assisted_environment_modal.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_modal_cancel_emits_event`
  - `test_modal_confirm_only_emits_event_when_repos_selected`
  - `test_modal_default_render_is_empty`
  - `test_modal_directory_picked_adds_repo_and_confirm_emits_event`
  - `test_modal_directory_picked_dedupes_paths`
  - `test_modal_directory_picked_rejects_non_repos`
  - `test_modal_show_clears_selection`
  - `test_modal_show_renders_expected_copy_with_empty_repos_message`

### `app/src/ai/agent_sdk/api_key_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/ai/agent_sdk/api_key.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `api_key_display_includes_creation_date`
  - `resolve_api_key_identifier_errors_for_ambiguous_name_matches`
  - `resolve_api_key_identifier_errors_when_not_found`
  - `resolve_api_key_identifier_falls_back_to_name_match`
  - `resolve_api_key_identifier_prefers_uid_match`
  - `sort_api_keys_sorts_by_created_at_descending`
  - `sort_api_keys_sorts_by_name_ascending`

### `app/src/ai/agent_sdk/driver/environment_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/ai/agent_sdk/driver/environment.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `merge_repos_dedupes_case_insensitively_and_preserves_environment_order`
  - `merge_repos_keeps_distinct_repositories`
  - `merge_repos_rejects_clone_directory_collisions`
  - `merge_repos_supports_additional_only_and_empty_inputs`
  - `parallel_clone_command_runs_repos_in_background_and_waits`
  - `single_repo_name_returns_none_for_zero_or_many_repos`
  - `single_repo_name_returns_repo_when_exactly_one_repo`

### `app/src/ai/agent_sdk/driver/harness/mod_tests.rs` — 7 absent

pin 10 · fork 3 · source `app/src/ai/agent_sdk/driver/harness/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `auth_check_command_for_gemini_is_none`
  - `auth_check_command_for_oz_is_none`
  - `auth_check_command_for_unknown_is_none`
  - `auth_check_command_for_unsupported_is_none`
  - `claude_runtime_error_patterns_returns_slice`
  - `codex_runtime_error_patterns_returns_slice`
  - `gemini_runtime_error_patterns_is_empty_by_default`

### `app/src/ai/blocklist/action_model/execute/wait_for_events_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/ai/blocklist/action_model/execute/wait_for_events.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `execute_invokes_parent_registration_and_honors_child_short_circuit`
- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `watchdog_timeout_clamps_negative_value_to_default_minus_margin`
  - `watchdog_timeout_clamps_to_hard_floor_when_stamped_value_is_too_small`
  - `watchdog_timeout_constants_match_documented_values`
  - `watchdog_timeout_falls_back_to_default_minus_margin_when_unset`
  - `watchdog_timeout_preserves_large_stamped_value`
  - `watchdog_timeout_subtracts_margin_for_stamped_minute`

### `app/src/ai/skills/global_skills_tests.rs` — 7 absent

pin 11 · fork 4 · source `app/src/ai/skills/global_skills.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `filter_skills_by_spec_matches_full_path_specs_for_remote_repos`  _(named in fork prose; near-name fork test `filter_skills_by_spec_matches_full_path_specs`)_
  - `filter_skills_by_spec_scopes_simple_remote_names_to_the_repo_host`  _(named in fork prose)_
  - `resolve_skill_repos_collapses_duplicates_preserving_first_seen_order`
  - `resolve_skill_repos_collects_org_qualified_repos`
  - `resolve_skill_repos_returns_empty_for_empty_input`
  - `resolve_skill_repos_skips_parse_failures`
  - `resolve_skill_repos_skips_unqualified_and_repo_only_specs`

### `app/src/auth/auth_manager_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/auth/auth_manager.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_device_code_request_retries_then_times_out`
  - `test_duplicate_redirect_for_logged_in_user_is_silently_ignored`
  - `test_log_out_clears_pending_auth_state`
  - `test_mismatched_state_with_different_user_uid_emits_invalid_state_parameter`
  - `test_persist_skips_when_api_key_authenticated`
  - `test_persist_skips_when_refresh_token_is_empty`
  - `test_stale_state_when_logged_out_emits_invalid_state_parameter`

### `app/src/search/slash_command_menu/static_commands/mod_tests.rs` — 7 absent

pin 30 · fork 23 · source `app/src/search/slash_command_menu/static_commands/mod.rs` · fork ships source: yes

- **PORTABLE?**
  - `cloud_agent_required_command_not_satisfied_outside_cloud_agent_session`
  - `cloud_agent_required_command_satisfied_in_cloud_agent_session`
  - `cloud_mode_v2_composer_required_command_not_satisfied_outside_v2_composer`
  - `cloud_mode_v2_composer_required_command_satisfied_in_v2_composer_session`
  - `codebase_context_requirement_not_satisfied_when_disabled`
  - `codebase_context_requirement_satisfied_when_enabled`
  - `index_command_requires_repo_and_codebase_context`

### `app/src/settings_view/billing_and_usage_page_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/settings_view/billing_and_usage_page.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_case_insensitive_display_name_sorting`
  - `test_default_sorting_pins_current_user_first_then_display_name_asc`
  - `test_display_name_az_sorting_pins_current_user`
  - `test_display_name_za_sorting_pins_current_user`
  - `test_requests_usage_asc_sorting_pins_current_user_with_display_name_tie_breaker`
  - `test_requests_usage_desc_sorting_pins_current_user_with_display_name_tie_breaker`
- **DECLINED?** — Status-menu org/email fields (#389)
  - `test_display_name_az_sorting_with_emails`

### `app/src/settings_view/platform_page_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/settings_view/platform_page.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `api_key_search_matches_agent_name_only_when_enabled`
  - `api_key_search_matches_key_name_case_insensitively`
  - `api_key_search_treats_empty_query_as_match`
  - `key_column_width_is_fixed_and_narrow`
  - `name_column_max_width_never_drops_below_min_width`
  - `name_column_max_width_reserves_extra_scope_budget_when_scope_enabled`
  - `name_column_max_width_reserves_non_resizable_columns_without_scope`

### `crates/warp_server_client/src/public_api_tests.rs` — 7 absent

pin 7 · fork 0 · source `crates/warp_server_client/src/public_api.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `iap_challenge_failure_emits_event_when_observation_is_enabled`
  - `iap_challenge_failure_emits_no_event_when_observation_is_disabled`
  - `ordinary_public_api_failure_preserves_shared_status_error`
  - `public_api_get_deserializes_successful_response`
  - `public_api_get_inherits_ambient_headers`
  - `public_api_get_sends_bearer_auth`
  - `shared_status_error_actionability_ignores_retryable_client_failures`

### `app/src/ai/agent_sdk/common_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/agent_sdk/common.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `classify_accepts_id_in_choices_even_when_list_unavailable`
  - `classify_returns_server_unavailable_error_when_list_unavailable`
  - `classify_returns_unknown_id_error_when_list_available_and_id_genuinely_invalid`
  - `parse_ambient_task_id_accepts_valid_ids`
  - `parse_ambient_task_id_preserves_error_prefix`
  - `update_feature_model_choices_clears_unavailable_flag_after_failed_fetch`

### `app/src/ai/agent_sdk/driver/cloud_provider_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/agent_sdk/driver/cloud_provider.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `aws_provider_env_vars_before_setup`
  - `collect_provider_env_vars_merges_all_providers`
  - `extract_cloud_providers_creates_aws_provider`
  - `extract_cloud_providers_creates_gcp_provider`
  - `extract_cloud_providers_empty_when_no_providers`
  - `gcp_provider_env_vars`

### `app/src/ai/agent_sdk/runner_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/agent_sdk/runner.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `confirm_delete_refuses_non_interactive_without_force`
  - `merge_instance_shape_errors_on_partial_shape_without_existing`
  - `merge_instance_shape_updates_dimensions_independently`
  - `resolve_arch_auto_maps_to_os_default`
  - `resolve_arch_explicit_is_preserved_regardless_of_os`
  - `resolve_updated_name_renames_only_with_uid`

### `app/src/ai/mcp/builtin_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/mcp/builtin.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `bearer_token_rejects_a_firebase_token_about_to_expire`
  - `bearer_token_rejects_session_cookie_auth`
  - `bearer_token_uses_a_valid_firebase_id_token`
  - `bearer_token_uses_api_keys`
  - `factory_installation_resolves_to_a_preauthenticated_http_server`
  - `factory_mcp_url_joins_server_roots_with_and_without_trailing_slash`

### `app/src/ai/orchestration/config_state_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/ai/orchestration/config_state.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `local_round_trip_preserves_remote_computer_use`
  - `resolve_from_config_preserves_local_claude`
  - `resolve_from_config_sanitizes_disabled_local_codex`
  - `runner_id_round_trips_through_config`
  - `toggle_to_local_preserves_claude`
  - `toggle_to_local_sanitizes_disabled_codex`

### `app/src/root_view_tests.rs` — 6 absent

pin 9 · fork 3 · source `app/src/root_view.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `account_first_class_uses_paid_status_then_fresh_request_limit`
  - `account_first_classes_route_to_paid_or_the_expected_offer`
  - `account_first_completion_metadata_matches_terminal_outcomes`
  - `refreshing_pending_onboarding_choices_replaces_stale_settings`
- **DECLINED?** — Account-first onboarding (#11)
  - `account_first_requires_login_even_without_ai_or_drive_settings`
  - `fallback_flow_only_requires_login_for_account_backed_settings`

### `app/src/server/server_api/harness_support_tests.rs` — 6 absent

pin 7 · fork 1 · source `app/src/server/server_api/harness_support.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `external_reference_artifact_includes_optional_title_and_metadata`
  - `external_reference_artifact_round_trips`
  - `external_reference_artifact_serializes_to_expected_wire_format`
  - `report_shutdown_abnormal_serializes_with_error`
  - `report_shutdown_clean_serializes_without_error`
  - `upload_target_deserializes_null_fields_as_empty`

### `app/src/settings_view/ai_page_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/settings_view/ai_page.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `ai_globally_disabled_marks_toggle_disabled_but_not_forced`
  - `respect_user_setting_returns_user_pref_unlocked`
  - `respect_user_setting_with_user_off_returns_unchecked_unlocked`
  - `team_disable_locks_toggle_off_regardless_of_user_pref`
  - `team_enable_locks_toggle_on_regardless_of_user_pref`
  - `team_force_takes_precedence_over_global_ai_disabled`

### `app/src/terminal/writeable_pty/pty_controller_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/terminal/writeable_pty/pty_controller.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** Already adjudicated in-tree. `write_user_command`, `write_in_band_command`, `queue_async_write` and `AsyncPtyWrite` no longer exist; the fork's `pty_controller_tests.rs` carries four current-API analogues (`queue_in_band_command_*`, `write_command_*`) with per-test comments naming the pin test each replaces. All 6 permanently unported.

- **DIVERGENT**
  - `test_pty_controller_cancels_async_writes_upon_user_command`  _(named in fork prose)_
  - `test_pty_controller_updates_block_list_when_writing_in_band_command`  _(named in fork prose)_
  - `test_pty_controller_writes_in_band_command`  _(named in fork prose)_
  - `test_pty_controller_writes_in_band_command_after_input_buffer_sequence`
  - `test_pty_controller_writes_input_buffer_sequence_after_block_completed`
  - `test_pty_controller_writes_user_command`  _(named in fork prose)_

### `app/src/tui/mod_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/tui/mod.rs` · fork ships source: yes

- **DECLINED?** — Account-first onboarding (#11)
  - `emits_logged_in_event_when_login_completes`
  - `emits_logged_out_event_and_resets_login_details`
- **PORTABLE?**
  - `leaves_invalid_verification_url_unchanged`
  - `renders_device_code_request_timeout_without_id_token_prefix`
  - `stores_device_fallback_before_opening_browser`
  - `tags_tui_verification_url_without_losing_existing_query_parameters`

### `app/src/workspaces/workspace_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/workspaces/workspace.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `admin_inherits_tier_full_breakdown_unlimited`
  - `admin_inherits_tier_per_user_totals_unlimited`
  - `admin_inherits_tier_team_aggregate_granularity`
  - `ftue_account_classes_have_stable_telemetry_labels`
  - `missing_policy_returns_defaults_for_admin_and_non_admin`
  - `non_admin_collapses_granularity_but_keeps_max_prior_cycles`

### `crates/ai/src/project_context/model_tests.rs` — 6 absent

pin 29 · fork 23 · source `crates/ai/src/project_context/model.rs` · fork ships source: yes

- **PORTABLE?**
  - `test_missing_rule_content_preserves_cached_content_while_path_is_standing`  _(named in fork prose)_
  - `test_reconcile_project_rules_hydrates_local_and_remote_paths`  _(named in fork prose)_
  - `test_remote_global_rules_only_layer_for_matching_remote_host`  _(named in fork prose)_
  - `test_remote_project_rules_require_matching_host`  _(named in fork prose)_
  - `test_remote_standing_results_preserve_host_qualified_rule_paths`  _(named in fork prose)_
  - `test_rule_missing_from_standing_results_is_removed_from_cached_content`  _(named in fork prose)_

### `crates/computer_use/src/linux/recording_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/computer_use/src/linux/recording.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `build_cut_only_filtergraph_constructs_trim_setpts_concat`
  - `linux_capture_command_captures_at_1x_without_setpts`
  - `records_full_display_for_screen_target`
  - `records_window_target_via_native_x11grab_after_raise`
  - `smart_cut_retains_only_selected_frames_in_order`
  - `visibility_samples_stay_inside_window`

### `crates/computer_use/src/pointer_session_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/computer_use/src/pointer_session.rs` · fork ships source: NO

- **DECLINED?** — computer_use session recording (#350)
  - `clear_resets_active_press_so_a_later_release_is_ignored`
  - `move_without_press_records_point_but_no_release_matches`
  - `new_press_while_held_replaces_active_button`
  - `release_for_a_different_button_is_ignored`
  - `release_reuses_last_point_from_a_prior_press_and_move`
  - `release_with_no_prior_press_is_ignored`

### `crates/managed_secrets/src/gcp_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/managed_secrets/src/gcp.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `basic_config_shape`
  - `config_file_path_matches_application_credentials_env_var`
  - `no_duration_flag_when_lifetime_absent`
  - `rejects_binary_path_with_spaces`
  - `rejects_task_id_with_spaces`
  - `service_account_impersonation`

### `crates/onboarding/src/telemetry_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/onboarding/src/telemetry.rs` · fork ships source: yes

- **DECLINED?** — Telemetry channel physically removed
  - `account_first_lifecycle_payloads_include_flow_and_classification`
  - `account_first_slide_and_setting_payloads_include_flow_version`
  - `account_first_started_payload_includes_flow_metadata`
  - `offer_action_payload_includes_account_class`
  - `onboarding_action_payload_omits_absent_account_class`
  - `stable_slide_payload_does_not_include_flow_version`

### `crates/repo_metadata/src/local_model_tests.rs` — 6 absent

pin 54 · fork 48 · source `crates/repo_metadata/src/local_model.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** Feature gap, already recorded. `local_model_test.rs` lines 1594-1618 list exactly these as deliberate #236 follow-ups: `repo_watches` per-directory extra-watch tracking for a `RootWatchMode::NonRecursive` root, and `index_lazy_loaded_path` still being synchronous and always-recursive. The types exist; nothing constructs `NonRecursive` outside tests. Port the feature before the tests.

- **DIVERGENT**
  - `deleted_subdir_drops_its_tracked_watch`
  - `index_lazy_loaded_path_tracks_only_root`
  - `lazy_root_created_directory_inserted_as_placeholder`  _(named in fork prose)_
  - `recursive_repo_uses_recursive_watch_mode`
  - `remove_lazy_loaded_path_clears_tracked_watches`
  - `remove_repository_clears_extra_dir_watches`

### `crates/warp_cli/src/api_key_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/warp_cli/src/api_key.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `create_accepts_expires_in`
  - `create_accepts_no_expiration`
  - `create_accepts_rfc3339_expiration`
  - `create_rejects_multiple_expiration_decisions`
  - `create_requires_expiration_decision`
  - `delete_is_alias_for_expire`

### `crates/warp_core/src/paths_tests.rs` — 6 absent

pin 14 · fork 8 · source `crates/warp_core/src/paths.rs` · fork ships source: yes

- **PORTABLE?**
  - `test_gui_app_id_maps_oss_tui_to_oss_gui`
  - `test_gui_config_and_mcp_paths_resolve_explicit_sources`
  - `test_project_path_for_warp_app_id`  _(near-name fork test `test_project_path_for_oss_app_id`)_
  - `test_project_path_for_warp_dev_app_id`  _(near-name fork test `test_project_path_for_zap_dev_app_id`)_
  - `test_tui_mcp_config_path_is_separate_from_gui`
  - `test_tui_state_dir_is_tui_subdir_of_gui_state_base`

### `crates/warp_multi_agent_client/src/lib_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/warp_multi_agent_client/src/lib.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `decodes_quoted_base64_protobuf_response_event`
  - `detects_passive_suggestion_requests`
  - `distinguishes_base64_and_protobuf_decode_errors`
  - `native_output_stream_is_send`
  - `routes_regular_and_passive_requests_to_distinct_endpoints`
  - `selects_endpoint_specific_ambient_header_policies`

### `crates/warp_server_client/src/graphql_helpers_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/warp_server_client/src/graphql_helpers.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `external_auth_rejection_returns_credentials_rejected_without_account_event`
  - `external_user_not_in_context_returns_credentials_rejected_without_account_event`
  - `missing_request_credentials_returns_before_sending`
  - `refresh_disabled_sends_provided_bearer_token`
  - `refresh_enabled_sends_configured_request_options`
  - `refreshable_user_not_in_context_emits_account_disabled_event`

### `app/src/ai/ambient_agents/task_tests.rs` — 5 absent

pin 11 · fork 6 · source `app/src/ai/ambient_agents/task.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `ambient_agent_task_deserializes_github_webhook_source`
  - `ambient_agent_task_deserializes_run_time_iso8601`
  - `task_status_error_code_deserializes_graphql_casing`
  - `task_status_error_code_deserializes_public_api_casing`
  - `task_status_error_code_deserializes_unknown_codes`

### `app/src/ai/artifact_download_tests.rs` — 5 absent

pin 7 · fork 2 · source `app/src/ai/artifact_download.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `default_download_filename_omits_extension_when_content_type_unknown`
  - `default_download_filename_prefers_server_filename`
  - `default_download_filename_uses_content_type_extension_when_filename_missing`
  - `download_destination_uses_explicit_path`
  - `extension_for_content_type_recognizes_image_jpg_alias`

### `app/src/ai/blocklist/action_model/execute/stop_recording_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/ai/blocklist/action_model/execute/stop_recording.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `cancelled_result_reports_run_cancelled_from_actual_reason`
  - `error_result_reports_actual_reason_not_claimed_agent_stopped`
  - `happy_path_agent_stopped_still_reports_agent_stopped`
  - `run_ended_joined_by_stop_action_reports_run_ended`
  - `success_result_reports_actual_limit_reached_reason`

### `app/src/ai/blocklist/action_model/execute/upload_artifact_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/ai/blocklist/action_model/execute/upload_artifact.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `execute_returns_error_when_conversation_has_not_synced_to_server`
  - `format_upload_artifact_error_keeps_single_layer_errors`
  - `format_upload_artifact_error_preserves_full_error_chain`
  - `resolve_path_uses_active_session_working_directory_for_relative_paths`
  - `should_autoexecute_honors_file_read_permissions_for_resolved_path`

### `app/src/ai/orchestration/validation_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/ai/orchestration/validation.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `accept_allowed_for_cloud_harness_with_named_or_inherited_auth`
  - `accept_allowed_for_oz_local_and_cloud`
  - `accept_blocked_for_cloud_harness_with_unset_auth_secret`
  - `accept_blocked_for_opencode_cloud`
  - `accept_blocked_for_product_disabled_local_codex`

### `app/src/pane_group/pane/terminal_pane_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/pane_group/pane/terminal_pane.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `inherit_share_cascades_ambient_source_for_cloud_orchestrator`
  - `inherit_share_cascades_user_source_for_manually_shared_local_orchestrator`
  - `inherit_share_returns_no_when_host_ambient_share_has_no_task_id`
  - `inherit_share_returns_no_when_host_is_not_sharing`
  - `inherit_share_returns_no_when_host_user_share_has_no_task_id`

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_section_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/settings_view/billing_and_usage/billing_cycle_usage_section.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `builds_one_plain_item_per_period`
  - `selects_explicitly_selected_period`
  - `selects_most_recent_period_when_none_selected`
  - `selects_nothing_when_no_summaries`
  - `selects_nothing_when_selection_absent`

### `app/src/terminal/shared_session/share_modal/body_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/terminal/shared_session/share_modal/body.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_open_modal_from_block`  _(named in fork prose)_
  - `test_open_modal_from_block_disabled`  _(named in fork prose)_
  - `test_open_modal_from_long_running_block`  _(named in fork prose)_
  - `test_open_modal_from_non_block`  _(named in fork prose)_
  - `test_open_modal_from_non_block_disabled`  _(named in fork prose)_

### `app/src/terminal/shared_session/viewer/terminal_manager_tests.rs` — 5 absent

pin 5 · fork 0 · source `app/src/terminal/shared_session/viewer/terminal_manager.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `command_execution_request_failed_clears_queued_command_in_flight`  _(named in fork prose)_
  - `handle_viewer_session_end_ignores_stale_ambient_end`  _(named in fork prose)_
  - `on_view_detached_closed_clears_orchestration_viewer_model_slot`  _(named in fork prose)_
  - `on_view_detached_hidden_for_close_keeps_orchestration_viewer_model_alive`  _(named in fork prose)_
  - `on_view_detached_moved_keeps_orchestration_viewer_model_alive`  _(named in fork prose)_

### `app/src/terminal/view/use_agent_footer/mod_tests.rs` — 5 absent

pin 8 · fork 3 · source `app/src/terminal/view/use_agent_footer/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `cli_agent_footer_does_not_render_for_warp_tui_session`  _(named in fork prose; near-name fork test `cli_agent_footer_does_not_render_for_phosphor_tui_session`)_
  - `cli_agent_footer_renders_for_viewer_of_shared_cloud_agent_session`  _(named in fork prose; near-name fork test `cli_agent_footer_renders_for_viewer_of_shared_ambient_agent_session`)_
  - `test_rich_input_submit_strategy_for_oh_my_pi`  _(named in fork prose)_
  - `use_agent_footer_hidden_during_cloud_agent_setup_lrc`  _(named in fork prose; near-name fork test `use_agent_footer_hidden_during_ambient_agent_setup_lrc`)_
- **DECLINED?** — Voice input (#389, #352)
  - `insert_cli_agent_voice_text_hermes_multiline_uses_bracketed_paste_without_submitting`  _(named in fork prose)_

### `app/src/ui_components/json_tree_tests.rs` — 5 absent

pin 34 · fork 29 · source `app/src/ui_components/json_tree.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** Feature gap, already recorded at `json_tree_tests.rs:303`. The five `mcp_result_*` tests need `McpRenderable` / `mcp_result_to_renderable` in `inline_action/requested_command.rs`, which this fork does not have: it renders an MCP tool result as `serde_json::to_string_pretty` text (`requested_command.rs:1494`) rather than as a collapsible JSON tree. User-visible and non-cloud -- see the findings section.

- **DIVERGENT**
  - `mcp_result_cancelled_returns_cancelled_variant`
  - `mcp_result_error_returns_error_variant`
  - `mcp_result_success_with_json_text_content_returns_parsed_tree`
  - `mcp_result_success_with_non_json_text_returns_string_tree`
  - `mcp_result_success_with_structured_content_returns_tree`

### `crates/ai/src/grok_subscription/oauth_tests.rs` — 5 absent

pin 5 · fork 0 · source `crates/ai/src/grok_subscription/oauth.rs` · fork ships source: NO

- **DECLINED?** — xAI/Grok subscription OAuth (#319)
  - `authorize_url_contains_required_params`
  - `cancelling_loopback_wait_releases_listener`
  - `manual_code_exchange_captures_attempt_verifier`
  - `manual_code_exchange_rejects_blank_code`
  - `token_response_parses_minimal_and_full`

### `crates/warp_server_client/src/base_client_tests.rs` — 5 absent

pin 5 · fork 0 · source `crates/warp_server_client/src/base_client.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `ambient_policy_supports_inherit_override_and_omit`
  - `authenticated_graphql_configuration_cannot_override_base_client_owned_headers`
  - `authenticated_graphql_options_include_configured_and_ambient_headers`
  - `explicit_token_graphql_options_route_without_authenticated_headers`
  - `iap_proxy_auth_header_uses_configured_provider`

### `crates/warp_tui/src/agent_block_tests.rs` — 5 absent

pin 48 · fork 43 · source `crates/warp_tui/src/agent_block.rs` · fork ships source: yes

- **PORTABLE?**
  - `agent_block_preserves_received_messages_and_hides_lifecycle_ids`  _(named in fork prose)_
  - `agent_message_defaults_collapsed_and_expands_through_block_state`  _(named in fork prose)_
  - `failed_output_usage_notice_matches_gui_conditions`  _(named in fork prose)_
  - `hidden_only_orchestration_exchange_has_zero_height`  _(named in fork prose)_
  - `orchestration_outputs_render_without_wait_for_events_tool_row`  _(named in fork prose)_

### `crates/warp_tui/src/handoff/tests.rs` — 5 absent

pin 5 · fork 0 · source `crates/warp_tui/src/handoff/tests.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `long_running_command_rejection_preserves_the_full_local_draft`  _(named in fork prose)_
  - `no_environment_card_has_top_padding_and_ctrl_c_restores_prompt_and_images`  _(named in fork prose)_
  - `privacy_invalidation_restores_the_draft_and_removes_handoff_from_commands`  _(named in fork prose)_
  - `settings_invalidation_restores_the_draft_and_repeated_submission_keeps_one_card`  _(named in fork prose)_
  - `slash_menu_selection_inserts_handoff_for_optional_prompt_composition`  _(named in fork prose)_

### `crates/warp_tui/src/orchestration_model_tests.rs` — 5 absent

pin 5 · fork 0 · source `crates/warp_tui/src/orchestration_model.rs` · fork ships source: yes

- **PORTABLE?**
  - `failed_launch_cleanup_preserves_other_sessions`  _(named in fork prose)_
  - `github_auth_blocker_keeps_the_remote_session_and_actionable_url`  _(named in fork prose)_
  - `local_harness_children_fail_cleanly`  _(named in fork prose)_
  - `remote_child_session_is_navigable_and_projects_lifecycle`  _(named in fork prose)_
  - `snapshot_is_shared_across_tree_and_filters_conversations_without_sessions`  _(named in fork prose)_

### `app/src/ai/agent/task_store_tests.rs` — 4 absent

pin 32 · fork 28 · source `app/src/ai/agent/task_store.rs` · fork ships source: yes

- **PORTED**
  - `test_prune_unreachable_subtasks_keeps_reachable_subtask`
  - `test_prune_unreachable_subtasks_noop_when_no_subtasks`
  - `test_prune_unreachable_subtasks_removes_nested_orphans`
  - `test_prune_unreachable_subtasks_removes_orphan_and_its_exchanges`

### `app/src/ai/agent_events/message_hydrator_tests.rs` — 4 absent

pin 5 · fork 1 · source `app/src/ai/agent_events/message_hydrator.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `hydrator_reads_new_message_for_matching_run`
  - `read_message_with_timeout_does_not_retry_permanent_http_failures`
  - `read_message_with_timeout_retries_transient_failures_until_success`
  - `read_message_with_timeout_times_out_after_retrying_transient_failures`

### `app/src/ai/blocklist/action_model/recording_telemetry_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/ai/blocklist/action_model/recording_telemetry.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `contains_no_user_generated_content`
  - `started_payload_shape`
  - `stopped_error_payload_allows_missing_metadata`
  - `stopped_success_payload_shape`

### `app/src/ai/blocklist/agent_view/zero_state_block_tests.rs` — 4 absent

pin 15 · fork 11 · source `app/src/ai/blocklist/agent_view/zero_state_block.rs` · fork ships source: yes

- **DECLINED?** — "Oz updates" zero-state section (#321)
  - `oz_updates_section_does_not_render_when_feature_flag_is_disabled`
  - `oz_updates_section_does_not_render_when_setting_is_disabled`
  - `oz_updates_section_does_not_render_without_updates`
  - `oz_updates_section_renders_when_all_conditions_are_true`

### `app/src/ai/connected_self_hosted_workers_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/ai/connected_self_hosted_workers.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `clear_worker_cache_is_noop_when_empty`
  - `clear_worker_cache_removes_cached_hosts`
  - `worker_hosts_excluding_filters_excluded_host`
  - `worker_hosts_excluding_sorts_dedups_and_filters_empty_and_warp_hosts`

### `app/src/ai/orchestration/remote_child_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/ai/orchestration/remote_child.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `capacity_quota_and_fallback_errors_keep_their_semantics`
  - `github_auth_error_is_a_shared_blocker_with_cloud_callback_url`
  - `orchestration_harness_defaults_to_oz_and_parses_known_harnesses`
  - `prepared_remote_request_matches_gui_wire_semantics`

### `app/src/ai/skills/skill_utils_tests.rs` — 4 absent

pin 5 · fork 1 · source `app/src/ai/skills/skill_utils.rs` · fork ships source: yes

- **DIVERGENT** — WOULD COMPILE AND FAIL. The fork's dedup key is `(name, dir)` with a priority tiebreak (the P0-3 prompt-cache fix); the pin keys on `(dir, content)`. `test_unique_skills_name_dedup_same_name_different_providers` asserts the fork's behaviour. Never port this one.
  - `test_unique_skills_does_not_dedupe_different_content`
- **DIVERGENT** — duplicate of `test_unique_skills_keeps_same_provider_skills_from_different_dirs`
  - `test_unique_skills_does_not_dedupe_different_dirs`
- **PORTED**
  - `skill_path_from_unix_encoded_remote_location`
  - `skill_path_from_windows_encoded_remote_location`

### `app/src/code_review/diff_state/remote_tests.rs` — 4 absent

pin 26 · fork 22 · source `app/src/code_review/diff_state/remote.rs` · fork ships source: yes

- **PORTABLE?**
  - `apply_file_delta_preserves_content_at_base_in_event`  _(named in fork prose)_
  - `apply_snapshot_loaded_preserves_content_at_base_in_event`
  - `apply_snapshot_loaded_without_diffs_becomes_error`
  - `get_committed_branch_files_response_emits_domain_files`  _(named in fork prose)_

### `app/src/notebooks/notebook_tests.rs` — 4 absent

pin 12 · fork 8 · source `app/src/notebooks/notebook.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_close_with_pending_changes`
  - `test_conflicting_notebook_read_only`
  - `test_edit_telemetry`
  - `test_only_user_title_edits_synced`

### `app/src/remote_server/server_model_tests.rs` — 4 absent

pin 17 · fork 13 · source `app/src/remote_server/server_model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `diff_states_starts_empty`
  - `empty_authenticate_clears_auth_token`
  - `empty_initialize_clears_auth_context`
  - `remote_agent_context_snapshot_broadcasts_replacements_and_initializes_once`

### `app/src/settings/ai_tests.rs` — 4 absent

pin 30 · fork 26 · source `app/src/settings/ai.rs` · fork ships source: yes

- **DECLINED?** — Voice input (#389, #352)
  - `test_voice_input_languages_auto_detect_is_first_with_empty_code`
  - `test_voice_input_languages_codes_and_names_are_valid_and_unique`
  - `test_voice_input_languages_has_full_catalog`
  - `test_voice_input_languages_includes_common_languages`

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_rows_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/settings_view/billing_and_usage/billing_cycle_usage_rows.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `build_own_usage_row_cloud_filter_drops_local_entries`
  - `build_own_usage_row_drops_other_users_entries`
  - `build_own_usage_row_drops_team_subject_entries`
  - `build_own_usage_row_local_filter_drops_cloud_entries`

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_team_totals_tests.rs` — 4 absent

pin 4 · fork 0 · source `app/src/settings_view/billing_and_usage/billing_cycle_usage_team_totals.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `full_breakdown_visibility_returns_three_cards_with_partitioned_sums`
  - `own_only_visibility_yields_overall_card_only`
  - `per_user_totals_visibility_yields_overall_card_only`
  - `team_aggregate_visibility_yields_overall_card_only`

### `app/src/terminal/cli_agent_tests.rs` — 4 absent

pin 34 · fork 30 · source `app/src/terminal/cli_agent.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_warp_tui_does_not_match_other_commands`  _(named in fork prose; near-name fork test `test_phosphor_tui_does_not_match_other_commands`)_
  - `test_warp_tui_matches_binaries_and_launchers`  _(named in fork prose; near-name fork test `test_phosphor_tui_matches_binaries_and_launchers`)_
  - `test_warp_tui_matches_with_env_var_prefix`  _(named in fork prose; near-name fork test `test_phosphor_tui_matches_with_env_var_prefix`)_
  - `test_warp_tui_variant_properties`  _(named in fork prose; near-name fork test `test_phosphor_tui_variant_properties`)_

### `app/src/terminal/conversation_restoration_tests.rs` — 4 absent

pin 16 · fork 12 · source `app/src/terminal/conversation_restoration.rs` · fork ships source: yes

- **PORTABLE?**
  - `single_block_at_same_time_as_exchange`  _(named in fork prose)_
  - `sorted_blocks_exchange_equal_to_block`  _(named in fork prose)_
  - `sorted_tail_equal_timestamps_pick_first_inserted_block`  _(named in fork prose)_
  - `sorted_tail_exchange_equals_tail_block`  _(named in fork prose)_

### `crates/computer_use/src/mac/recording_tests.rs` — 4 absent

pin 4 · fork 0 · source `crates/computer_use/src/mac/recording.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `applies_setpts_filter_when_playback_speed_exceeds_one`
  - `ignores_window_target_until_window_scoped_recording_lands`
  - `limits_duration_as_an_input_option_before_i`
  - `omits_setpts_filter_when_playback_speed_is_real_time`

### `crates/computer_use/src/recording_tests.rs` — 4 absent

pin 4 · fork 0 · source `crates/computer_use/src/recording.rs` · fork ships source: NO

- **DECLINED?** — computer_use session recording (#350)
  - `observes_synthetic_recording_exit`
  - `removes_unclaimed_output_when_handle_is_dropped`
  - `removes_unclaimed_output_when_handle_is_dropped_macos`
  - `start_reports_unsupported_when_ffmpeg_absent`

### `crates/remote_server/src/client_tests.rs` — 4 absent

pin 16 · fork 12 · source `crates/remote_server/src/client/mod.rs` · fork ships source: yes

- **PORTABLE?**
  - `codebase_index_push_messages_become_client_events`
  - `get_diff_state_on_dead_connection_errors_promptly`
  - `get_diff_state_round_trips_as_session_scoped`
  - `open_buffer_round_trips_as_session_scoped`

### `crates/warp_cli/src/runner_tests.rs` — 4 absent

pin 4 · fork 0 · source `crates/warp_cli/src/runner.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `validate_os_config_accepts_matching_linux`
  - `validate_os_config_accepts_matching_macos`
  - `validate_os_config_rejects_docker_image_with_macos`
  - `validate_os_config_rejects_macos_version_with_linux`

### `crates/warp_tui/src/agent_message_tests.rs` — 4 absent

pin 4 · fork 0 · source `crates/warp_tui/src/agent_message.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `conversation_statuses_render_expected_glyphs`  _(named in fork prose)_
  - `message_preview_wraps_with_a_hanging_indent_and_falls_back_to_subject`  _(named in fork prose)_
  - `parent_sender_renders_as_orchestrator_in_child_transcript`  _(named in fork prose)_
  - `running_child_message_matches_the_design_layout_and_styles`  _(named in fork prose)_

### `crates/warpui_core/src/telemetry/event_store_tests.rs` — 4 absent

pin 4 · fork 0 · source `crates/warpui_core/src/telemetry/event_store.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_app_active_after_activity`
  - `test_app_active_after_inactivity`
  - `test_event_queue_empty`
  - `test_initialize_session`

### `app/src/ai/agent/api/convert_conversation_tests.rs` — 3 absent

pin 15 · fork 12 · source `app/src/ai/agent/api/convert_conversation.rs` · fork ships source: yes

- **PORTABLE?**
  - `test_convert_conversation_data_to_ai_conversation_sets_restored_run_id`
  - `test_convert_tool_call_result_to_input_upload_artifact_missing_result_is_error`
  - `test_convert_tool_call_result_to_input_upload_artifact_success`

### `app/src/ai/agent_sdk/config_file_tests.rs` — 3 absent

pin 15 · fork 12 · source `app/src/ai/agent_sdk/config_file.rs` · fork ships source: yes

- **PORTABLE?**
  - `any_non_uuid_warp_id_becomes_well_known_spec`
  - `empty_warp_id_is_rejected`
  - `well_known_warp_id_converts_to_well_known_spec`

### `app/src/ai/agent_sdk/driver/cache_setup_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/agent_sdk/driver/cache_setup.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `export_commands_use_active_shell_syntax_and_escaping`
  - `gate_matrix_requires_namespace_and_nonempty_root`
  - `source_repo_maps_to_canonical_identity_and_checkout`

### `app/src/ai/agent_sdk/driver/cloud_provider/gcp_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/agent_sdk/driver/cloud_provider/gcp.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `best_effort_command_completes_within_timeout`
  - `best_effort_command_is_killed_on_timeout`
  - `best_effort_command_missing_binary_is_not_found`

### `app/src/ai/agent_sdk/driver/harness/codex_tests.rs` — 3 absent

pin 38 · fork 35 · source `app/src/ai/agent_sdk/driver/harness/codex.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `fetch_resume_payload_maps_404_to_resume_state_missing`  _(named in fork prose)_
  - `fetch_resume_payload_maps_other_errors_to_load_failed`  _(named in fork prose)_
  - `fetch_resume_payload_returns_codex_variant_on_success`  _(named in fork prose)_

### `app/src/ai/agent_sdk/retry_tests.rs` — 3 absent

pin 11 · fork 8 · source `app/src/ai/agent_sdk/retry.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `graphql_status_classifier_fails_fast_on_permanent_statuses`
  - `graphql_status_classifier_fails_fast_without_typed_transport_error`
  - `graphql_status_classifier_retries_transient_statuses`

### `app/src/ai/blocklist/action_model/recording_finalize_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/blocklist/action_model/recording_finalize.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `agent_discard_finalization_skips_upload`
  - `cancellation_finalization_skips_upload_even_without_actions`
  - `empty_actions_finalization_is_an_error_without_upload`

### `app/src/ai/blocklist/controller_tests.rs` — 3 absent

pin 6 · fork 3 · source `app/src/ai/blocklist/controller.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `cancelling_conversation_aborts_pending_auto_resume`
  - `mock_response_stream_updates_history_through_controller`
  - `optimistic_cli_subagent_completion_with_in_flight_stream_reports_success`

### `app/src/ai/blocklist/queued_query_tests.rs` — 3 absent

pin 42 · fork 39 · source `app/src/ai/blocklist/queued_query.rs` · fork ships source: yes

- **PORTABLE?**
  - `clear_conversations_for_terminal_surface_drops_every_listed_conversation`  _(near-name fork test `clear_conversations_in_terminal_view_drops_every_listed_conversation`)_
  - `initial_cloud_mode_head_rejects_user_mutations_and_autofire`  _(near-name fork test `locked_head_rejects_user_mutations_and_autofire`)_
  - `remove_initial_cloud_mode_row_only_removes_the_locked_head`

### `app/src/ai/cloud_environments/catalog_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/cloud_environments/catalog.rs` · fork ships source: NO

- **DECLINED?** — Warp Environments (#211)
  - `default_resolution_preserves_each_gui_consumer_name_tie_breaker`
  - `environment_creation_refreshes_after_cloud_model_inserts_the_object`
  - `environment_timestamp_updates_refresh_recency_order`

### `app/src/ai/document/ai_document_model_tests.rs` — 3 absent

pin 15 · fork 12 · source `app/src/ai/document/ai_document_model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `cloud_model_sync_event_reconciles_stale_document_client_id`
  - `publish_refreshes_pending_saving_document_content`
  - `test_plan_markdown_content_preserves_copyable_structure`

### `app/src/ai/execution_profiles/config_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/ai/execution_profiles/config.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `context_window_limit_schema_has_description`
  - `file_collection_rejects_invalid_values_as_a_unit`
  - `file_collection_round_trips_multiple_profiles`

### `app/src/ai/mcp/file_based_manager_tests.rs` — 3 absent

pin 12 · fork 9 · source `app/src/ai/mcp/file_based_manager.rs` · fork ships source: yes

- **PORTABLE?**
  - `servers_changed_only_emits_for_effective_source_set_changes`
  - `test_auto_started_cloud_scan_uuids_are_in_wait_set`
  - `test_project_scoped_cloud_scan_has_detected_servers_but_empty_wait_set`

### `app/src/ai/skills/file_watchers/utils_tests.rs` — 3 absent

pin 23 · fork 20 · source `app/src/ai/skills/file_watchers/utils.rs` · fork ships source: yes

- **PORTED**
  - `find_skill_files_in_tree_empty_repo`  _(near-name fork test `find_skill_directories_in_tree_empty_repo`)_
  - `find_skill_files_in_tree_finds_root_skills`  _(near-name fork test `find_skill_directories_in_tree_finds_root_skills`)_
  - `find_skill_files_in_tree_finds_subdirectory_skills`  _(near-name fork test `find_skill_directories_in_tree_finds_subdirectory_skills`)_

### `app/src/server/telemetry_ext_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/server/telemetry_ext.rs` · fork ships source: NO

- **DECLINED?** — Telemetry channel physically removed
  - `to_rudder_batch_message_does_not_redact_non_ugc_named_events`
  - `to_rudder_batch_message_redacts_nested_strings_in_ugc_payload`
  - `to_rudder_batch_message_redacts_ugc_named_events`

### `app/src/settings/onboarding_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/settings/onboarding.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `account_first_settings_enable_agent_for_authenticated_users_and_apply_ui_choices`
  - `apply_onboarding_settings_gates_third_party_ai_on_account`
- **DECLINED?** — Account-first onboarding (#11)
  - `apply_onboarding_settings_preserves_existing_cloud_profile_on_existing_user_login`  _(named in fork prose; near-name fork test `apply_onboarding_settings_preserves_existing_profile_object_on_existing_user_login`)_

### `app/src/settings_view/billing_and_usage_dispatch_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/settings_view/billing_and_usage_dispatch.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `does_not_use_v2_for_legacy_paid_workspace`
  - `uses_v2_for_free_workspace`
  - `uses_v2_when_user_has_no_workspace`

### `app/src/terminal/cli_agent_sessions/listener/mod_tests.rs` — 3 absent

pin 21 · fork 18 · source `app/src/terminal/cli_agent_sessions/listener/mod.rs` · fork ships source: yes

- **PORTABLE?**
  - `codex_try_parse_ignores_structured_event_without_codex_plugin`  _(named in fork prose)_
  - `oh_my_pi_end_to_end_parsing_and_handling`  _(named in fork prose)_
  - `oh_my_pi_is_supported`  _(named in fork prose; near-name fork test `pi_is_supported`)_

### `app/src/terminal/shared_session/viewer/network_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/terminal/shared_session/viewer/network.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_send_pty_write_event_advances_event_no`  _(named in fork prose)_
  - `test_send_pty_write_event_while_batching`  _(named in fork prose)_
  - `test_send_pty_write_event_while_not_batching`  _(named in fork prose)_

### `app/src/workspaces/gql_convert_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/workspaces/gql_convert.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `order_authenticated_teams_before_non_member_teams`
  - `preserve_relative_order_within_member_groups`
  - `preserve_server_order_when_user_has_no_team_membership`

### `crates/ai/src/agent/action/convert_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/ai/src/agent/action/convert.rs` · fork ships source: yes

- **DECLINED?** — screen recording (#367)
  - `start_recording_parses_valid_window_target`
  - `start_recording_rejects_unparseable_window_id`
  - `start_recording_without_target_records_whole_screen`

### `crates/ai/src/agent/action_result/mod_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/ai/src/agent/action_result/mod.rs` · fork ships source: yes

- **DECLINED?** — Agent-invoked agent spawning / RunAgents (#325, #290)
  - `run_agents_is_failed_when_no_agents_launch`
  - `run_agents_is_successful_when_all_agents_launch`
  - `run_agents_is_successful_when_some_agents_launch`

### `crates/build_cache/src/spacectl_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/build_cache/src/spacectl.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `detect_command_has_exact_argv_cache_root_and_repo_cwd`
  - `mount_command_has_exact_explicit_modes_dry_run_false_root_and_cwd`
  - `mount_response_deserializes_spacectl_output`

### `crates/computer_use/src/recording_metadata_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/computer_use/src/recording_metadata.rs` · fork ships source: NO

- **DECLINED?** — computer_use session recording (#350)
  - `parses_ffmpeg_container_duration`
  - `probes_duration_after_timestamp_rescaling`
  - `rejects_missing_or_invalid_duration`

### `crates/graphql/src/api/ai_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/graphql/src/api/ai.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `conversion_merges_warp_and_byok_usage_for_same_model`
  - `conversion_populates_token_usage_and_tool_usage_metadata`
  - `list_ai_conversations_query_selects_token_usage_fields`

### `crates/http_client/src/iap_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/http_client/src/iap.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `challenge_status_without_iap_header_is_not_a_challenge`
  - `challenge_statuses_with_iap_header_are_challenges`
  - `non_challenge_status_with_iap_header_is_not_a_challenge`

### `crates/http_client/src/lib_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/http_client/src/lib.rs` · fork ships source: yes

- **PORTABLE?**
  - `injects_trace_link_header_when_span_active`
  - `omits_trace_link_header_when_no_span`
  - `request_carries_trace_link_header_on_warp_header_path`

### `crates/onboarding/src/slides/offer_slide_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/onboarding/src/slides/offer_slide.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `choose_how_to_start_copy_and_telemetry_names_match_spec`
  - `head_start_copy_and_telemetry_names_match_spec`
  - `offer_slide_can_render_before_classification`

### `crates/remote_server/src/setup_tests.rs` — 3 absent

pin 38 · fork 35 · source `crates/remote_server/src/setup.rs` · fork ships source: yes

- **PORTABLE?**
  - `parse_preinstall_unsupported_glibc_too_old`  _(named in fork prose)_
  - `parse_preinstall_unsupported_non_glibc`  _(named in fork prose)_
  - `parse_uname_unsupported_armv8l`  _(named in fork prose; near-name fork test `parse_uname_unsupported_arch`)_

### `crates/repo_metadata/src/repositories_tests.rs` — 3 absent

pin 4 · fork 1 · source `crates/repo_metadata/src/repositories.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** Renamed method, tests renamed with it: the pin's `detect_possible_local_git_repo` is this fork's `detect_possible_git_repo`, and all three tests exist under `test_detect_possible_git_repo_*` in `repositories_tests.rs`. Pure false positive of name matching.

- **DIVERGENT**
  - `test_detect_possible_local_git_repo_nested_repo_created_after_parent_registration`  _(near-name fork test `test_detect_possible_git_repo_nested_repo_created_after_parent_registration`)_
  - `test_detect_possible_local_git_repo_non_existent_directory`  _(near-name fork test `test_detect_possible_git_repo_non_existent_directory`)_
  - `test_detect_possible_local_git_repo_not_a_git_repo`  _(near-name fork test `test_detect_possible_git_repo_not_a_git_repo`)_

### `crates/warp_cli/src/mcp_tests.rs` — 3 absent

pin 10 · fork 7 · source `crates/warp_cli/src/mcp.rs` · fork ships source: yes

- **PORTABLE?**
  - `test_bare_identifier_treated_as_json_when_flag_disabled`
  - `test_bare_identifier_treated_as_well_known`
  - `test_parse_well_known_integration_id`

### `crates/warp_core/src/channel/state_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_core/src/channel/state.rs` · fork ships source: yes

- **PORTABLE?**
  - `unparseable_input_returns_none`
  - `ws_becomes_http_and_preserves_port`
  - `wss_becomes_https_and_strips_path`

### `crates/warp_server_auth/src/user/persistence_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_server_auth/src/user/persistence.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_deserialize_2026_03_06_persisted_user`
  - `test_serialize_persisted_user`
  - `test_windows_user_persistence`

### `crates/warp_server_client/src/auth/session_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_server_client/src/auth/session.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `api_key_exchange_defers_owner_type_until_user_properties_are_fetched`
  - `bearer_credentials_are_returned_without_session_refresh_events`
  - `unexpired_firebase_credentials_return_cached_token_without_refresh_events`

### `crates/warp_server_client/src/network_logging_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_server_client/src/network_logging.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `empty_snapshot_is_empty_string`
  - `push_beyond_capacity_drops_oldest`
  - `snapshot_joins_items_with_newlines`

### `crates/warp_tui/src/grok_oauth/tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_tui/src/grok_oauth/tests.rs` · fork ships source: NO

- **DECLINED?** — xAI/Grok subscription OAuth (#319)
  - `callback_and_manual_failures_do_not_claim_success_or_expose_raw_details`  _(named in fork prose)_
  - `fatal_card_sanitizes_the_body_and_escape_closes_the_attempt`  _(named in fork prose)_
  - `waiting_card_uses_handoff_structure_and_only_escape_footer_hint`  _(named in fork prose)_

### `crates/warp_tui/src/usage_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_tui/src/usage.rs` · fork ships source: yes

- **PORTABLE?**
  - `cost_formats_cents_as_dollars`  _(named in fork prose)_
  - `entry_text_follows_the_persisted_display_mode`  _(named in fork prose)_
  - `entry_text_matches_the_gui_credits_formatting`  _(named in fork prose)_

### `crates/warp_tui/src/voice_input_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_tui/src/voice_input.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `cancel_returns_the_model_to_idle`  _(named in fork prose)_
  - `start_does_not_replace_an_active_session`  _(named in fork prose)_
  - `stop_transitions_the_model_to_transcribing`  _(named in fork prose)_

### `app/src/ai/agent/conversation_yaml_tests.rs` — 2 absent

pin 8 · fork 6 · source `app/src/ai/agent/conversation_yaml.rs` · fork ships source: yes

- **DECLINED?** — Agent-invoked agent spawning / RunAgents (#325, #290)
  - `run_agents_result_serializes_agent_ids`
- **PORTABLE?**
  - `upload_file_artifact_tool_call_result_serializes_only_supported_success_fields`

### `app/src/ai/agent_sdk/admin_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/ai/agent_sdk/admin.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `multiple_teams_include_workspace_and_repeat_pretty_team_labels`
  - `single_team_omits_admin_visible_non_member_teams`

### `app/src/ai/agent_sdk/mcp_config_tests.rs` — 2 absent

pin 18 · fork 16 · source `app/src/ai/agent_sdk/mcp_config.rs` · fork ships source: yes

- **PORTABLE?**
  - `well_known_spec_is_coerced_to_warp_id`  _(near-name fork test `uuid_spec_is_coerced_to_warp_id`)_
  - `well_known_warp_id_passes_validation`

### `app/src/ai/blocklist/action_model/execute/ask_user_question_tests.rs` — 2 absent

pin 7 · fork 5 · source `app/src/ai/blocklist/action_model/execute/ask_user_question.rs` · fork ships source: yes

- **DECLINED** — ask_user_question auto-approve divergence (#373), DECLINED.md
  - `execute_returns_sync_skipped_question_ids_when_autoapprove_is_enabled`  _(named in fork prose)_
  - `should_autoexecute_returns_true_when_autoapprove_is_enabled_and_profile_allows_override`  _(named in fork prose; near-name fork test `should_autoexecute_returns_false_when_autoapprove_is_enabled_and_profile_always_blocks`)_

### `app/src/ai/blocklist/action_model/execute/read_skill_tests.rs` — 2 absent

pin 7 · fork 5 · source `app/src/ai/blocklist/action_model/execute/read_skill.rs` · fork ships source: yes

- **PORTABLE?**
  - `disconnected_remote_session_does_not_fall_back_to_client_global_bundled_skill`
  - `remote_session_reads_remote_bundled_skill_catalog`

### `app/src/ai/blocklist/action_model/execute/send_message_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/ai/blocklist/action_model/execute/send_message.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `sender_run_id_and_task_id_for_send_falls_back_to_ambient_task_id`
  - `sender_run_id_and_task_id_for_send_prefers_conversation_task_id`

### `app/src/ai/blocklist/input_model_tests.rs` — 2 absent

pin 14 · fork 12 · source `app/src/ai/blocklist/input_model.rs` · fork ships source: yes

- **PORTABLE?**
  - `conversation_events_apply_policy_updates`  _(named in fork prose)_
  - `conversation_events_with_inert_policy_leave_config_unchanged`  _(named in fork prose)_

### `app/src/ai/blocklist/permissions_tests.rs` — 2 absent

pin 25 · fork 23 · source `app/src/ai/blocklist/permissions.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_get_org_execute_commands_denylist`
  - `test_merged_denylist_deduplication`

### `app/src/ai/blocklist/usage/conversation_usage_view_tests.rs` — 2 absent

pin 3 · fork 1 · source `app/src/ai/blocklist/usage/conversation_usage_view.rs` · fork ships source: yes

- **PORTABLE?**
  - `show_all_agent_rows_is_independent_of_details_expanded`
  - `toggle_details_expanded_flips_state_and_resets_show_all_on_collapse`

### `app/src/lib_tests.rs` — 2 absent

pin 4 · fork 2 · source `app/src/lib.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `app_keeps_default_secure_storage_service_name`  _(named in fork prose)_
  - `tui_uses_distinct_secure_storage_service_name`  _(named in fork prose)_

### `app/src/remote_server/diff_state_proto_tests.rs` — 2 absent

pin 15 · fork 13 · source `app/src/remote_server/diff_state_proto.rs` · fork ships source: yes

- **DIVERGENT** — the fork's `DiffSize::Unrenderable` carries no `UnrenderableReason`, so the pin's four-variant round trip has no fork equivalent
  - `diff_size_round_trips_through_proto`
- **PORTED** — adapted to the fork's free conversion fns
  - `pr_info_round_trips_through_proto`  _(near-name fork test `pr_info_round_trips`)_

### `app/src/server/server_api/auth_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/server/server_api/auth.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_firebase_token_urls`
- **DECLINED?** — Account-first onboarding (#11)
  - `access_token_skip_login_rejects_bearer_token`

### `app/src/settings/tui_theme_tests.rs` — 2 absent

pin 6 · fork 4 · source `app/src/settings/tui_theme.rs` · fork ships source: yes

- **PORTABLE?**
  - `theme_schema_entry_is_tui_only`  _(named in fork prose)_
  - `theme_setting_is_tui_local_and_defaults_to_automatic_detection`  _(near-name fork test `theme_setting_is_local_and_defaults_to_automatic_detection`)_

### `app/src/settings/tui_zero_state_tests.rs` — 2 absent

pin 5 · fork 3 · source `app/src/settings/tui_zero_state.rs` · fork ships source: yes

- **PORTABLE?**
  - `zero_state_schema_entries_are_tui_only`  _(named in fork prose)_
  - `zero_state_settings_are_tui_local_file_settings`  _(near-name fork test `zero_state_settings_are_local_file_settings`)_

### `app/src/settings_view/code_page_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/settings_view/code_page.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `other_unavailable_failures_are_not_index_limit_failures`
  - `remote_index_limit_failure_is_detected_from_status_message`

### `app/src/settings_view/teams_page.rs` — 2 absent

pin 2 · fork 0 · source `app/src/settings_view/teams_page.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_owner_state_chip_text_contrasts_with_accent_overlay`
  - `test_valid_domains`

### `app/src/terminal/share_block_modal_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/terminal/share_block_modal.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `escape_html_attribute_escapes_attribute_breakout_characters`  _(named in fork prose)_
  - `escape_html_attribute_leaves_safe_text_unchanged`  _(named in fork prose)_

### `app/src/terminal/shared_session/network/heartbeat_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/terminal/shared_session/network/heartbeat.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_idle_timeout`  _(named in fork prose)_
  - `test_periodic_ping`  _(named in fork prose)_

### `app/src/terminal/view/ambient_agent/block/setup_command_text_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/terminal/view/ambient_agent/block/setup_command_text.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `setup_command_groups_have_independent_visibility`  _(named in fork prose)_
  - `setup_command_groups_track_running_group_independently`  _(named in fork prose)_

### `app/src/util/file/external_editor/mac_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/util/file/external_editor/mac.rs` · fork ships source: yes

- **PORTABLE?**
  - `is_warp_bundle_recognises_warp_channels`  _(named in fork prose; near-name fork test `is_zap_bundle_recognises_zap_channels`)_
  - `is_warp_bundle_rejects_other_apps`  _(named in fork prose; near-name fork test `is_zap_bundle_rejects_other_apps`)_

### `app/src/workspace/one_time_modal_model_tests.rs` — 2 absent

pin 5 · fork 3 · source `app/src/workspace/one_time_modal_model.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_free_ai_removal_modal_decision_matrix`
  - `wait_until_auto_handoff_sleep_modal_closed_tracks_modal_state`

### `crates/cloud_object_models/src/scheduled_ambient_agent_tests.rs` — 2 absent

pin 2 · fork 0 · source `crates/cloud_object_models/src/scheduled_ambient_agent.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `additional_source_repos_make_snapshot_non_empty`
  - `additional_source_repos_round_trip_and_is_optional`

### `crates/http_client/src/lib.rs` — 2 absent

pin 2 · fork 0 · source `crates/http_client/src/lib.rs` · fork ships source: yes

- **PORTABLE?**
  - `server_and_rtc_origins_match`
  - `third_party_origin_does_not_match`

### `crates/languages/src/lib_tests.rs` — 2 absent

pin 6 · fork 4 · source `crates/languages/src/lib.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** The pin splits `language_by_filename(&StandardizedPath)` from `language_by_local_filename(&Path)`; this fork has only `language_by_filename(&Path)`, which IS the pin's local variant, and both assertions already exist against it. Worth noting separately: the fork has no `StandardizedPath` overload, so remote-path language resolution goes through a local `Path` -- benign on POSIX, unverified for Windows remotes.

- **DIVERGENT**
  - `local_command_extension_resolves_to_shell`  _(near-name fork test `command_extension_resolves_to_shell`)_
  - `local_html_extensions_resolve_to_html`  _(near-name fork test `html_extensions_resolve_to_html`)_

### `crates/warp_server_auth/src/user_tests.rs` — 2 absent

pin 2 · fork 0 · source `crates/warp_server_auth/src/user.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_parse_user_profile`
  - `test_user_global_skills_defaults_to_empty`

### `crates/warp_tui/src/cloud_run_view_tests.rs` — 2 absent

pin 2 · fork 0 · source `crates/warp_tui/src/cloud_run_view.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `lightweight_cloud_view_renders_startup_and_blocker_without_terminal_state`  _(named in fork prose)_
  - `spawned_cloud_view_matches_figma_in_progress_and_succeeded_states`  _(named in fork prose)_

### `crates/warp_tui/src/completion_menu_tests.rs` — 2 absent

pin 2 · fork 0 · source `crates/warp_tui/src/completion_menu.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `show_does_not_replace_an_existing_inline_menu`  _(named in fork prose)_
  - `show_reuses_inline_menu_rows_and_accepts_the_selected_span`  _(named in fork prose)_

### `crates/warp_tui/src/read_only_menu_tests.rs` — 2 absent

pin 6 · fork 4 · source `crates/warp_tui/src/read_only_menu.rs` · fork ships source: yes

**DIVERGENT (hand-traced).** Already adjudicated in-tree: `read_only_menu_tests.rs:1-7` records that both need `TuiViewportedList::with_trimmed_selection_line_ends` and `TuiSelectable::with_semantic_selection_by_style`, neither of which exists in this fork's `warpui_core`. This is a real feature gap in TUI text selection (trailing-whitespace trimming and double-click word selection), not test debt.

- **DIVERGENT**
  - `double_click_selects_complete_styled_text`  _(named in fork prose)_
  - `selection_stops_at_trailing_whitespace`  _(named in fork prose)_

### `crates/warp_tui/src/slash_commands_tests.rs` — 2 absent

pin 21 · fork 19 · source `crates/warp_tui/src/slash_commands.rs` · fork ships source: yes

- **DECLINED?** — Voice input (#389, #352)
  - `slash_command_menu_renders_voice_row`  _(named in fork prose; near-name fork test `slash_command_menu_renders_view_logs_row`)_
- **PORTABLE?**
  - `slash_command_menu_renders_theme_row`  _(named in fork prose; near-name fork test `slash_command_menu_renders_view_logs_row`)_

### `app/src/ai/agent/api/convert_from_tests.rs` — 1 absent

pin 5 · fork 4 · source `app/src/ai/agent/api/convert_from.rs` · fork ships source: yes

- **PORTABLE?**
  - `converts_upload_artifact_tool_call_to_action`

### `app/src/ai/agent_sdk/driver/harness/claude_code/wake_driver_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/agent_sdk/driver/harness/claude_code/wake_driver.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `local_wake_task_state_ready_allows_success_and_stale_in_progress`

### `app/src/ai/agent_sdk/driver/terminal_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/agent_sdk/driver/terminal.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `extend_shared_session_retention_emits_event_for_active_sharer`

### `app/src/ai/blocklist/action_model/execute/read_documents_tests.rs` — 1 absent

pin 2 · fork 1 · source `app/src/ai/blocklist/action_model/execute/read_documents.rs` · fork ships source: yes

- **PORTABLE?**
  - `execute_lazily_hydrates_missing_plan_for_remote_child_without_local_parent`  _(named in fork prose)_

### `app/src/ai/blocklist/agent_view/conversation_selection_tests.rs` — 1 absent

pin 5 · fork 4 · source `app/src/ai/blocklist/agent_view/conversation_selection.rs` · fork ships source: yes

- **PORTABLE?**
  - `gui_list_policy_classifies_unavailable_entry`  _(named in fork prose; near-name fork test `gui_list_policy_classifies_available_entry`)_

### `app/src/ai/blocklist/context_model_tests.rs` — 1 absent

pin 15 · fork 14 · source `app/src/ai/blocklist/context_model.rs` · fork ships source: yes

- **DECLINED?** — has_locking_attachment divergence (#318)
  - `has_locking_attachment_is_false_with_only_pending_block_id`  _(named in fork prose; near-name fork test `has_locking_attachment_is_true_with_pending_block_id`)_

### `app/src/ai/blocklist/handoff/touched_repos_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/handoff/touched_repos.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `find_git_root_walks_up_to_dot_git`

### `app/src/ai/blocklist/inline_action/ask_user_question_view_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/inline_action/ask_user_question_view.rs` · fork ships source: yes

- **PORTABLE?**
  - `view_state_shows_other_input_only_for_the_current_question`

### `app/src/ai/blocklist/inline_action/create_environment_modal_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/inline_action/create_environment_modal.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_create_environment_modal_uses_orchestration_form_configuration`

### `app/src/ai/blocklist/inline_action/orchestration_controls_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/blocklist/inline_action/orchestration_controls.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `runner_controls_require_both_feature_flag_and_experiment_arm`

### `app/src/ai/get_relevant_files/remote_search/native_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/ai/get_relevant_files/remote_search/native.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `file_contents_from_response_keeps_only_whole_text_files`

### `app/src/ai/mcp/file_mcp_watcher_tests.rs` — 1 absent

pin 4 · fork 3 · source `app/src/ai/mcp/file_mcp_watcher.rs` · fork ships source: yes

- **PORTABLE?**
  - `abort_config_parse_cancels_and_removes_inflight_task`

### `app/src/ai/skills/bundled_tests.rs` — 1 absent

pin 2 · fork 1 · source `app/src/ai/skills/bundled.rs` · fork ships source: yes

- **PORTABLE?**
  - `unavailable_bundled_context_path_renders_as_empty_string`  _(named in fork prose)_

### `app/src/auth/login_slide_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/auth/login_slide.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `account_first_copy_matches_product_spec`

### `app/src/auth/mod_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/auth/mod.rs` · fork ships source: yes

- **DECLINED?** — /logout slash command (#338)
  - `web_logout_url_uses_configured_server_root`

### `app/src/bin/generate_settings_schema_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/bin/generate_settings_schema.rs` · fork ships source: yes

- **PORTABLE?**
  - `surface_annotation_matches_setting_schema_entry_metadata`

### `app/src/code_review/diff_state/mod_tests.rs` — 1 absent

pin 6 · fork 5 · source `app/src/code_review/diff_state/mod.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `new_for_test_creates_local_variant`

### `app/src/drive/index_tests.rs` — 1 absent

pin 4 · fork 3 · source `app/src/drive/index.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_shared_object_limit_banner_dismissal_persists_per_type`

### `app/src/drive/sharing/qr_code_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/drive/sharing/qr_code.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `qr_matrix_for_url_returns_square_matrix_with_dark_modules`

### `app/src/local_control/handlers/app_state_tests.rs` — 1 absent

pin 2 · fork 1 · source `app/src/local_control/handlers/app_state.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `unavailable_surface_open_returns_structured_error`  _(named in fork prose)_

### `app/src/local_control/handlers/metadata_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/local_control/handlers/metadata.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `agent_management_surface_reports_feature_flag_unavailable`  _(named in fork prose)_

### `app/src/persistence/block_list_tests.rs` — 1 absent

pin 7 · fork 6 · source `app/src/persistence/block_list.rs` · fork ships source: yes

- **PORTABLE?**
  - `process_ai_queries_for_nld_history_match_filters_empty_and_whitespace_inputs_oldest_first`

### `app/src/server/telemetry/events_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/server/telemetry/events.rs` · fork ships source: NO

- **DECLINED?** — Telemetry channel physically removed
  - `telemetry_events_have_nonempty_name_and_description`

### `app/src/server/telemetry/mod_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/server/telemetry/mod.rs` · fork ships source: NO

- **DECLINED?** — Telemetry channel physically removed
  - `test_persist_events_doesnt_include_ugc_events`

### `app/src/settings_view/admin_actions_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/settings_view/admin_actions.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_admin_panel_link_generation`

### `app/src/settings_view/custom_router_view_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/settings_view/custom_router_view.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `error_card_lays_out_under_unbounded_vertical_constraint_without_panicking`

### `app/src/settings_view/platform/create_api_key_modal_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/settings_view/platform/create_api_key_modal.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_agent_dropdown_is_searchable`

### `app/src/tab_configs/session_config_tests.rs` — 1 absent

pin 38 · fork 37 · source `app/src/tab_configs/session_config.rs` · fork ships source: yes

- **PORTABLE?**
  - `snapshot_cloud_pane_gets_cloud_type`  _(named in fork prose; near-name fork test `snapshot_cloud_pane_gets_agent_type`)_

### `app/src/terminal/input/handoff_compose_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/terminal/input/handoff_compose.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `preserves_explicit_environment_selection`  _(named in fork prose)_

### `app/src/terminal/model/lifecycle/mod_tests.rs` — 1 absent

pin 7 · fork 6 · source `app/src/terminal/model/lifecycle/mod.rs` · fork ships source: yes

- **PORTABLE?**
  - `lifecycle_telemetry_payload_is_allowlisted_and_non_ugc`  _(named in fork prose)_

### `app/src/terminal/warpify/settings_tests.rs` — 1 absent

pin 7 · fork 6 · source `app/src/terminal/warpify/settings.rs` · fork ships source: yes

- **PORTABLE?**
  - `test_deprecated_ssh_wrapper_migration_triggers_are_not_synced`  _(named in fork prose)_

### `app/src/util/bindings_tests.rs` — 1 absent

pin 4 · fork 3 · source `app/src/util/bindings.rs` · fork ships source: yes

- **PORTABLE?**
  - `test_orchestration_cycle_bindings_are_editable`  _(named in fork prose)_

### `app/src/workspace/view/vertical_tabs_tests.rs` — 1 absent

pin 60 · fork 59 · source `app/src/workspace/view/vertical_tabs.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `summary_pane_kind_icons_distinguish_ambient_claude_from_local_claude`

### `app/src/workspaces/update_manager_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/workspaces/update_manager.rs` · fork ships source: NO

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `test_leaving_team_removes_objects`

### `crates/ai/src/agent/action_result/convert_tests.rs` — 1 absent

pin 2 · fork 1 · source `crates/ai/src/agent/action_result/convert.rs` · fork ships source: yes

- **PORTABLE?**
  - `read_files_partial_success_converts_failed_files`  _(named in fork prose)_

### `crates/ai/src/skills/skill_provider_tests.rs` — 1 absent

pin 6 · fork 5 · source `crates/ai/src/skills/skill_provider.rs` · fork ships source: yes

- **PORTABLE?**
  - `warp_home_skill_path_is_home_warp_skill`  _(near-name fork test `home_skill_path_is_home_scope`)_

### `crates/mcp/src/oauth_tests.rs` — 1 absent

pin 8 · fork 7 · source `crates/mcp/src/oauth.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `loopback_oauth_completes_dcr_and_code_exchange`

### `crates/remote_server/src/manager_tests.rs` — 1 absent

pin 4 · fork 3 · source `crates/remote_server/src/manager.rs` · fork ships source: yes

- **PORTABLE?**
  - `remote_agent_context_snapshot_is_a_host_scoped_manager_event`

### `crates/warp_server_client/src/auth/mod_tests.rs` — 1 absent

pin 1 · fork 0 · source `crates/warp_server_client/src/auth/mod.rs` · fork ships source: yes

- **CLOUD?** — cloud symbols in the pin source imports or the test body
  - `unknown_settings_results_preserve_operation_context`

### `crates/warp_tui/src/handoff/model_tests.rs` — 1 absent

pin 1 · fork 0 · source `crates/warp_tui/src/handoff/model.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `missing_token_after_eager_cancellation_restores_only_trimmed_argument`  _(named in fork prose)_

### `crates/warp_tui/src/tool_call_labels_tests.rs` — 1 absent

pin 6 · fork 5 · source `crates/warp_tui/src/tool_call_labels.rs` · fork ships source: yes

- **DECLINED?** — Agent-invoked agent spawning / RunAgents (#325, #290)
  - `all_failed_run_agents_uses_failure_glyph`  _(named in fork prose)_

### `crates/warp_tui/src/tui_builder_tests.rs` — 1 absent

pin 3 · fork 2 · source `crates/warp_tui/src/tui_builder.rs` · fork ships source: yes

- **DECLINED?** — Voice input (#389, #352)
  - `voice_input_border_pulses_between_cyan_overlay_2_and_lilac_600`  _(named in fork prose)_

### `crates/warp_tui/src/tui_shell_command_view_tests.rs` — 1 absent

pin 11 · fork 10 · source `crates/warp_tui/src/tui_shell_command_view.rs` · fork ships source: yes

- **PORTABLE?**
  - `escape_while_editing_exits_editor_without_cancelling`  _(named in fork prose)_

### `crates/warp_tui/src/zero_state_animation_tests.rs` — 1 absent

pin 26 · fork 25 · source `crates/warp_tui/src/zero_state_animation.rs` · fork ships source: yes

- **PORTABLE?**
  - `logo_mask_preserves_the_offset_warp_faces`  _(named in fork prose)_

### `crates/warp_tui/src/zero_state_tests.rs` — 1 absent

pin 6 · fork 5 · source `crates/warp_tui/src/zero_state.rs` · fork ships source: yes

- **DECLINED?** — Account-first onboarding (#11)
  - `login_line_shows_signed_in_account_email`  _(named in fork prose)_

### `crates/warpui/src/browser_tests.rs` — 1 absent

pin 5 · fork 4 · source `crates/warpui/src/browser.rs` · fork ships source: yes

- **PORTABLE?**
  - `safe_browser_open_url_accepts_warp_channel_urls`  _(near-name fork test `safe_browser_open_url_accepts_app_channel_urls`)_

### `crates/warpui_core/src/app_focus_telemetry_tests.rs` — 1 absent

pin 1 · fork 0 · source `crates/warpui_core/src/app_focus_telemetry.rs` · fork ships source: NO

- **DIVERGENT?** — the fork does not ship the pin's source module (feature gap)
  - `test_daily_app_focus_duration_increase`

---

## How to continue this

For any file above with a `PORTABLE?` group:

1. `grep -rn '<test_name>' <fork>` — if a comment already adjudicates it, you are done;
   record the verdict here and move on. About a quarter of the list resolves this way.
2. Open the pin test and list every symbol it touches. For each, confirm the fork has
   it **with the same signature** — a same-named function with an extra `ctx` argument
   or a different return type is a rewrite, not a port.
3. Compare the pin's implementation of the function under test with the fork's. A
   deliberate behavioural inversion (see `unique_skills`) produces a test that compiles
   and then fails, which is worse than not porting it. Those belong in `DECLINED.md`.
4. Prefer tests that guard behaviour a user would notice over tests of internals, and
   skip tests the fork already covers under another name.

Regenerating the mechanical half is cheap: extract test-function names from
`git archive 02b53fcd8 app crates` and from the fork tree, diff by name, then bucket
from the pin source's imports plus fork module presence. No build is required.
