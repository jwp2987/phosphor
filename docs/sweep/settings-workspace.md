# Sweep verdicts — `settings_view/`, `settings/`, `workspace/`, `pane_group/`, `search/`

Area of `docs/SWEEP-INVENTORY.md` (fork side `main` @ `2b072ec61`, oracle pin
`02b53fcd8` / Warp `2026.07.29.09.05` stable, per `ORACLE.md`) covering:

- `app/src/settings_view/**`
- `app/src/settings/**`
- `app/src/workspace/**`
- `app/src/pane_group/**`
- `app/src/search/**`

`app/src/workspaces/` (plural — cloud user-workspace objects) is **out of
scope**: it is a different module than `app/src/workspace/` and not one of
the five listed directories.

## Totals

32 files, **288** absent pinned tests, all adjudicated.

| bucket | tests | notes |
|---|---:|---:|
| CLOUD | 233 | needs `warp_graphql`, `server_api`, `cloud_object_models`, `warp_server_client`, or an already-declined cloud subsystem |
| DECLINED | 27 | covered by an existing `DECLINED.md` row |
| COVERED-ELSEWHERE | 9 | renamed/replaced fork test, cited by name |
| DIVERGENT | 8 | fork's API/behavior genuinely differs, by design |
| PORTABLE (ported) | 2 | ported this pass, see below |
| MISSING-SUBSYSTEM | 6 | non-cloud, needs a subsystem the fork doesn't wire up yet |
| PORTABLE (not yet ported) | 3 | confirmed portable, deferred — see §"Left for the next pass" |

Two tests ported this pass (`prepare_local_claude_child_merges_anthropic_model_env_var`,
`prepare_local_claude_child_no_anthropic_model_when_empty` in
`app/src/pane_group/pane/local_harness_launch_tests.rs`). Everything else is
adjudicated but not code-touched.

---

## Per-file verdicts

### `app/src/settings_view/update_environment_form_tests.rs` — 43 absent
**DECLINED** — Warp Environments (`DECLINED.md` #211: "Cloud-backed. 75 pinned
tests are out of scope, not parity debt."). Confirmed: pin source imports
`::ai::api_keys::{CustomEndpoint, CustomEndpointSchema}` and
`warp_graphql`-backed environment creation; fork does not ship
`update_environment_form.rs` at all.

### `app/src/pane_group/mod_tests.rs` — 33 absent
Already fully adjudicated **in-file** by an earlier pass — see the module
header at `app/src/pane_group/mod_tests.rs:1-35`. Re-verified, not re-derived:
- **CLOUD** (~26): `decide_remote_child_hydration_*`, `create_shared_session_viewer_*`,
  `test_ambient_transcript_restore_*_cloud_mode*`,
  `test_entering_remote_parent_agent_view_*`, `test_insert_hidden_ambient_child_agent_pane_*`,
  `test_close_pane_clears_transitively_shared_child_entry_*` — cloud/remote
  orchestration or the removed ambient-agent-UI subsystem.
- **DIVERGENT** (~5): `test_add_pane_restores_hidden_child_when_parent_is_already_fullscreen`,
  `test_ensure_hidden_child_agent_pane_*` (×3), `test_hidden_child_creation_applies_ambient_task_id_to_controller`
  — depend on a *lazy* hidden-child-agent-pane restoration mechanism
  (`restore_missing_child_agent_panes_for_parent`) this fork doesn't have; it
  only restores child panes **eagerly**, once, at
  `PaneGroup::new_internal`/`reattach_panes` (`create_missing_child_agent_panes`).
- **DECLINED** (2): `test_start_shared_session_from_modal`, `test_stop_shared_session`
  — `TerminalView::attempt_to_share_session` is a declared no-op ("the Shared
  Session network entry point has been cut"); testing it would violate
  `script/check_stub_coverage`.
- **Needs a second look, not re-verified this pass**: `test_reattach_panes_restores_hidden_child_when_parent_is_already_fullscreen`,
  `test_restore_closed_pane_restores_hidden_child_when_parent_is_already_fullscreen`,
  `test_replace_pane_restores_hidden_child_when_replacement_is_already_fullscreen`,
  `test_pane_group_restore_loop_keeps_orchestration_topology_and_materializes_child_pane`,
  `test_swapping_to_child_agent_from_maximized_pane_keeps_maximized_state` — these
  name the **eager** restore path (`reattach_panes`) the fork *does* have, per
  the header's own text, so they may be genuinely portable rather than
  DIVERGENT. Flagged, not traced further — see "Left for the next pass."

### `app/src/settings_view/environments_page_tests.rs` — 32 absent
**DECLINED** — Warp Environments (#211). Confirmed: pin source imports
`warp_graphql::scalars::time::ServerTimestamp`; fork does not ship
`environments_page.rs`.

### `app/src/settings_view/custom_inference_modal_tests.rs` — 19 absent
**DECLINED** — `CustomEndpoint`/`custom_model_providers` on `ApiKeyManager`
(`DECLINED.md` #142, #347: "superseded, not ported... The fork's actual BYOP
surface for this is `AgentProviderSecrets`"). Confirmed: pin source imports
`::ai::api_keys::{CustomEndpoint, CustomEndpointSchema}` directly — this
modal is the UI for the exact mechanism that row declines. The inventory's
mechanical `DIVERGENT?` guess is corrected to DECLINED with citation.

### `app/src/settings/cloud_preferences_syncer_tests.rs` — 17 absent
**CLOUD.** Confirmed: pin source imports `cloud_object_models::JsonSerializer`,
`crate::cloud_object::model::persistence::CloudModel`,
`crate::cloud_object::model::generic_string_model::GenericStringObjectId` —
the whole file is the client for syncing settings to Warp's cloud object
store. **Not yet in `DECLINED.md` under this name** — proposed new row below.

### `app/src/search/slash_command_menu/static_commands/commands_tests.rs` — 15 absent
**DIVERGENT**, already hand-traced by the inventory itself (`docs/SWEEP-INVENTORY.md:1801-1823`).
Confirmed: fork's `static_commands/mod.rs:290-330` documents deriving surfaces
from `StaticCommand::supports_tui`/`supports_gui` instead of the pin's
`all_commands(settings::SettingsMode)` + `SlashCommandSurfaces`; `SettingsMode`
is elsewhere documented dropped (`DECLINED.md`, `SettingSurfaces` row). Nothing
to run these tests against — no new work.

### `app/src/workspace/view_tests.rs` — 14 absent
Mixed, per `SCOPE-REST.md:220` (verified, not just quoted):
- **CLOUD** (7): `test_reward_modal_no_overlap`, `test_reward_modal_shows_for_received_referral`
  (cloud referrals), `test_stop_sharing_session`, `test_stop_sharing_all_sessions_in_tab`,
  `test_tab_context_menu_share_session_items` (cloud session sharing),
  `test_open_cloud_agent_setup_guide_action_opens_management_view_and_is_idempotent`,
  `test_tools_panel_warp_drive_toggle_updates_available_views` (this is the
  cloud "Warp Drive" toggle predating the Library rename, not the
  `warp_drive.*` settings/`SettingsSection::ZapDrive` retained-by-decision
  surface — different symbol, still cloud),
  `test_tools_panel_preferences_activate_after_signup_and_ai_enablement` (cloud signup).
- **PORTABLE, confirmed by name but not ported** (7 — "genuine debt" per
  `SCOPE-REST.md`, re-verified here): `test_tab_bar_traffic_light_space_regression_for_resource_center_overlap`,
  `copy_model_and_profile_preserves_explicit_model_over_source_profile_default`,
  `test_open_file_notebook_focuses_existing_markdown_pane`,
  `test_open_vertical_tabs_panel_is_idempotent`, `test_active_tab_bar_position_id_tracks_layout`,
  `test_new_session_menu_is_capped_to_window_height`. (`test_tab_mru_order`,
  the 7th `SCOPE-REST.md` names, is **already ported** — not in the current
  absent list, which is why this file shows 14 absent, not 15.) See "Left for
  the next pass" for why these weren't ported this round.

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_common_tests.rs` — 12 absent
**CLOUD** (11): pin source is billing-credit aggregation over
`warp_graphql::billing`. **DECLINED** (1): `filter_legacy_buckets_drops_voice_and_suggested_code_diffs_in_input_order`
— Voice input (`DECLINED.md` #389/#352).

### `app/src/pane_group/pane/local_harness_launch_tests.rs` — 10 absent
Mixed, hand-traced against the fork's actual `local_harness_launch.rs`:
- **PORTED** (2): `prepare_local_claude_child_merges_anthropic_model_env_var`,
  `prepare_local_claude_child_no_anthropic_model_when_empty`. Adapted (dropped
  the pin's `MockAIClient`/`ai_client` argument — this fork's
  `prepare_local_harness_child_launch` no longer creates a cloud agent task,
  see below — and the exact mocked `run_id`, since the fork generates one
  locally via `Uuid::new_v4`). The `ANTHROPIC_MODEL` merge logic itself
  (`harness_model_env_vars`, `app/src/ai/agent_sdk/driver/harness/mod.rs:289`)
  is unchanged from the pin and was already exercised nowhere at the
  `local_harness_launch` call-site level.
- **DIVERGENT-by-decision, not yet in `DECLINED.md`** (5):
  `local_child_task_config_records_supported_third_party_harnesses`,
  `local_child_task_config_stamps_orchestrator_name`,
  `local_child_task_config_trims_whitespace_only_name`,
  `local_child_task_config_returns_none_for_oz_and_unknown`,
  `normalize_orchestrator_agent_name_trims_and_drops_empty`. The pin's
  `local_child_task_config(harness, name)` feeds a cloud `create_agent_task`
  mutation. The fork's version (`local_harness_launch.rs:153-163`) takes only
  `harness` (no `name`/orchestrator-name param) and its one call site
  discards the result outright: `let _ = local_child_task_config(harness);`
  with the comment "launching a local harness child task no longer goes
  through the cloud `create_agent_task` mutation; a UUID v4 is generated
  locally as the task_id instead. The `local_child_task_config(harness)`
  argument is no longer used." This is a deliberate divergence already
  implemented in code but not recorded in `DECLINED.md` — proposed new row
  below.
- **MISSING-SUBSYSTEM** (3): `prepare_local_codex_child_launch_rejects_without_rewriting_global_codex_state`,
  `prepare_local_codex_child_launch_succeeds_when_testing_flag_is_enabled`,
  `prepare_local_harness_child_launch_rejects_disabled_codex_before_shell_validation`.
  These need `local_harness_setup::local_harness_product_disabled_message`
  wired into `prepare_local_harness_child_launch`'s `Harness::Codex` arm.
  `app/src/ai/local_harness_setup.rs` is already ported and tested
  standalone, but its own module doc says explicitly: "**Not yet wired to a
  caller on this branch**... `app/src/pane_group/pane/local_harness_launch.rs`
  (whose Codex arm explicitly defers 'disabled-product-message gating' to
  #323)... Wiring any of those in is out of scope here." Confirmed
  `local_harness_launch.rs`'s `Harness::Codex` match arm has no gating check
  today. Not attempted this pass — it's a real wiring gap, but a prior agent
  already scoped it to #323 and it's more than a mechanical test port
  (changes a `match` arm's control flow in an already-large async function).

### `app/src/settings_view/mod_tests.rs` — 9 absent
**CLOUD**, all 9. Already fully adjudicated in-file — header at
`app/src/settings_view/mod_tests.rs:8-18`: "Warp's `is_code_subpage()` /
`is_cloud_platform_subpage()` and the `CodeIndexing`/`CloudEnvironments`/
`OzCloudAPIKeys`/`Account`/`Teams`/`BillingAndUsage` `SettingsSection`
variants do not exist in this fork... Tests that depended on those are
adapted below... or dropped where the underlying feature is gone." Confirmed
the renamed survivors exist: `code_subpages_are_identified` →
`ai_subpages_are_identified` (line 21), `code_subpages_map_to_code_backing_page`
→ `ai_subpages_map_to_ai_backing_page` (line 52) — both **COVERED-ELSEWHERE**,
counted under CLOUD's complement since the underlying umbrella they replace
(`Code`) is retained but `CloudPlatform`/`Account`/`Teams`/`BillingAndUsage`
are not.

### `app/src/workspace/auto_handoff_tests.rs` — 9 absent
**CLOUD**, all 9. Confirmed: `auto_handoff.rs` imports
`crate::ai::ambient_agents::telemetry::CloudAgentTelemetryEvent` and
`AutoCloudHandoffTrigger`; every test name is `auto_handoff_skips_*_to_cloud`
or exercises the cloud-handoff eligibility gate directly. Not yet a
`DECLINED.md` row under this name — proposed below (folds into the
auto-cloud-handoff family together with the `one_time_modal_model_tests.rs`
sleep-modal test).

### `app/src/settings_view/agent_assisted_environment_modal_tests.rs` — 8 absent
**DECLINED** — Warp Environments (#211). This modal is the repo-auto-detection
step of Environment creation (imports `ai::index::full_source_code_embedding`
and `git2::Repository` to *suggest* repos for a new cloud Environment, not a
freestanding local feature); fork does not ship
`agent_assisted_environment_modal.rs`.

### `app/src/search/slash_command_menu/static_commands/mod_tests.rs` — 7 absent
Fork's version of this file is `mod_test.rs` (already exists, 23 tests,
singular `_test.rs`, not `_tests.rs` — the fork-wide rename the inventory's
own caveats warn about). Hand-traced against fork's `Availability: u8`
bitflags (`static_commands/mod.rs:18-35`):
- **CLOUD** (4): `cloud_agent_required_command_satisfied_in_cloud_agent_session`,
  `cloud_agent_required_command_not_satisfied_outside_cloud_agent_session`,
  `cloud_mode_v2_composer_required_command_satisfied_in_v2_composer_session`,
  `cloud_mode_v2_composer_required_command_not_satisfied_outside_v2_composer`
  — need `Availability::CLOUD_AGENT`/`NOT_CLOUD_AGENT`/`CLOUD_MODE_V2_COMPOSER`
  bits, none of which exist (or should exist) in a BYOP fork.
- **MISSING-SUBSYSTEM** (3): `codebase_context_requirement_satisfied_when_enabled`,
  `codebase_context_requirement_not_satisfied_when_disabled`,
  `index_command_requires_repo_and_codebase_context` — need
  `Availability::CODEBASE_CONTEXT` (fork's `u8` bitflags has exactly one
  unused bit left, so it fits) plus an `/index` slash command; grepped
  `commands.rs` for "index"/"codebase" and found neither. Codebase indexing
  itself exists in the fork (`ai::index::full_source_code_embedding`,
  `code_page.rs`) but has no slash-command surface. Real feature work, not a
  mechanical test port — the inventory's mechanical `PORTABLE?` tag for this
  whole file was wrong for these 3 and right in spirit for none of the 7 (the
  other 4 are cloud, see above).

### `app/src/settings_view/billing_and_usage_page_tests.rs` — 7 absent
**CLOUD** (6): confirmed via `warp_graphql::billing::AddonCreditsOption` import.
**DECLINED** (1): `test_display_name_az_sorting_with_emails` — Status-menu
`org`/`email` fields (`DECLINED.md` #389: "Both fields were removed from
`TuiStatusInfo` outright").

### `app/src/settings_view/platform_page_tests.rs` — 7 absent
**CLOUD**, all 7. Confirmed: imports `warp_graphql::object_permissions::OwnerType`
and `warp_graphql::queries::api_keys::ApiKeyProperties`; this is the cloud API
platform page (dropped with the account/Teams surface).

### `app/src/settings_view/ai_page_tests.rs` — 6 absent
**CLOUD**, all 6. Confirmed: `team_disable_locks_toggle_off_regardless_of_user_pref`
and its siblings call `derive_agent_attribution_toggle_state(&AdminEnablementSetting::…)`
— org-admin policy overriding a user's AI toggle. Cloud teams/org policy
(`DECLINED.md` #445: "`UserWorkspaces::current_team()` returns `None`
unconditionally... the org/workspace command denylist is inert").

### `app/src/pane_group/pane/terminal_pane_tests.rs` — 5 absent
**CLOUD**, all 5. Confirmed: `terminal_pane.rs` imports
`session_sharing_protocol::sharer::SessionSourceType`; all 5 test names are
`inherit_share_*`, and one explicitly names `cloud_orchestrator`.

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_section_tests.rs` — 5 absent
**CLOUD**, all 5. Same billing-credit family as `billing_cycle_usage_common_tests.rs`.

### `app/src/settings/ai_tests.rs` — 4 absent
**DECLINED**, all 4 — Voice input language preference (`DECLINED.md` #389/#352:
"`VOICE_INPUT_LANGUAGES` and `voice_input_language` configure the
transcription language for a backend that cannot run here").

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_rows_tests.rs` — 4 absent
**CLOUD**, all 4. Same billing-credit family.

### `app/src/settings_view/billing_and_usage/billing_cycle_usage_team_totals_tests.rs` — 4 absent
**CLOUD**, all 4. Same billing-credit family (team aggregation specifically).

### `app/src/settings/onboarding_tests.rs` — 3 absent
**CLOUD** (2): `account_first_settings_enable_agent_for_authenticated_users_and_apply_ui_choices`,
`apply_onboarding_settings_gates_third_party_ai_on_account`. **DECLINED** (1):
`apply_onboarding_settings_preserves_existing_cloud_profile_on_existing_user_login`
— Account-first onboarding (`DECLINED.md` #11), near-name fork test
`apply_onboarding_settings_preserves_existing_profile_object_on_existing_user_login`
already covers the non-cloud remainder of that behavior.

### `app/src/settings_view/billing_and_usage_dispatch_tests.rs` — 3 absent
**CLOUD**, all 3. Confirmed: `billing_and_usage_dispatch.rs` imports
`crate::workspaces::user_workspaces::UserWorkspaces` and dispatches between
legacy/v2 cloud billing pages.

### `app/src/settings/tui_theme_tests.rs` — 2 absent
**COVERED-ELSEWHERE**, both. `app/src/settings/tui_theme_tests.rs:1-6`
(the fork's own file, same name) documents: `theme_schema_entry_is_tui_only`
asserted on `SettingSurfaces`/`SettingsMode` (dropped, see `DECLINED.md`) and
was replaced by `theme_schema_entry_is_registered_and_public` (line 70);
`theme_setting_is_tui_local_and_defaults_to_automatic_detection` was renamed
to `theme_setting_is_local_and_defaults_to_automatic_detection` (line 60).
Both pin tests are already ported under new names — no action needed.

### `app/src/settings/tui_zero_state_tests.rs` — 2 absent
**COVERED-ELSEWHERE**, both. Same pattern, same header
(`app/src/settings/tui_zero_state_tests.rs:1-6`): `zero_state_schema_entries_are_tui_only`
→ `zero_state_schema_entries_are_registered_and_public` (line 118);
`zero_state_settings_are_tui_local_file_settings` →
`zero_state_settings_are_local_file_settings` (line 89).

### `app/src/settings_view/code_page_tests.rs` — 2 absent
**MISSING-SUBSYSTEM**, both — `remote_index_limit_failure_is_detected_from_status_message`,
`other_unavailable_failures_are_not_index_limit_failures`. These classify a
"maximum number of codebase indexes has been reached" status message from
`remote_server::codebase_index_proto::RemoteCodebaseIndexStatus`.
`remote_server` is **not cloud** (`DECLINED.md`'s "not declined" list:
"Phosphor's SSH remote-host daemon, entirely local"), and
`RemoteCodebaseIndexStatus` genuinely exists in the fork's
`crates/remote_server/src/codebase_index_proto.rs` — but `code_page.rs`
itself has no `remote_codebase_index_limit_reached`-equivalent classifier;
grepped for "Remote"/"index limit" in `code_page.rs` and found neither.
`RemoteCodebaseIndexStatus` **is** consumed in this fork, at
`app/src/remote_server/codebase_index_model.rs` — outside my listed
directories, so whether that module already has equivalent limit-detection
coverage under a different name is **not verified here**; worth checking
before treating this as fresh debt.

### `app/src/settings_view/teams_page.rs` — 2 absent
**CLOUD**, both. Confirmed: `teams_page.rs` (pin) imports
`warp_core::features::FeatureFlag` plus team/org types throughout; fork does
not ship it — `SettingsSection::Teams` is gone with the account surface (per
this sweep's own brief).

### `app/src/workspace/one_time_modal_model_tests.rs` — 2 absent
**CLOUD**, both. `test_free_ai_removal_modal_decision_matrix` keys off
`CustomerType`, `has_zero_base_credits`, `workspaces_fetched` — paid-tier
billing (`DECLINED.md` #11). `wait_until_auto_handoff_sleep_modal_closed_tracks_modal_state`
is the auto-cloud-handoff sleep modal — same family as
`workspace/auto_handoff_tests.rs` above.

### `app/src/settings_view/admin_actions_tests.rs` — 1 absent
**CLOUD.** Confirmed: `admin_actions.rs` imports `crate::channel::ChannelState`
and `crate::server::ids::ServerId`; `admin_panel_link_for_team` builds a URL
against `ChannelState::server_root_url()`.

### `app/src/settings_view/custom_router_view_tests.rs` — 1 absent
**DECLINED** — `FEATURE_INTROS` content / `FeatureIntroId::CustomModelRouter`
(`DECLINED.md`, "Divergences" section: "promotes a Warp-hosted
custom-model-router feature this fork does not have"). Confirmed:
`custom_router_view.rs` imports `crate::ai::custom_model_routers::{CustomModelRouter, CustomModelRouting}`
— this view *is* that declined feature's UI; fork does not ship it.

### `app/src/settings_view/platform/create_api_key_modal_tests.rs` — 1 absent
**CLOUD.** Confirmed: imports `warp_server_client::auth::AgentIdentity`.

### `app/src/workspace/view/vertical_tabs_tests.rs` — 1 absent
**DIVERGENT-by-decision.** `summary_pane_kind_icons_distinguish_ambient_claude_from_local_claude`
asserts a pin-only `is_ambient` field on `SummaryPaneKind::CLIAgent` that
distinguishes a *cloud-hosted* Claude session (`claude_cloud.svg`) from a
local one. Fork's `SummaryPaneKind::CLIAgent { agent: CLIAgent }`
(`vertical_tabs.rs:763`) carries no such field — only `OzAgent { is_ambient }`
does. This matches the "children run as local processes; `is_remote_child`
will be permanently false" reasoning already recorded for local orchestration
in `DECLINED.md` (the reversed multi-agent-orchestration row) — third-party
CLI children have no cloud-hosted variant to distinguish here, so there is
nothing to give a second icon to. Not a gap.

---

## New `DECLINED.md` rows this sweep found evidence for

Proposed, not written (another agent owns `DECLINED.md`; per instructions,
recorded here for that agent to add):

1. **Cloud preference sync** (`app/src/settings/cloud_preferences_syncer.rs`,
   17 tests) — settings syncing to Warp's cloud object store via
   `cloud_object_models::JsonSerializer`/`CloudModel`. Same shape as the
   already-declined billing/teams rows; currently has no row of its own.
2. **`local_child_task_config` no longer creates a cloud agent task**
   (`app/src/pane_group/pane/local_harness_launch.rs`, 5 tests) — the fork's
   `local_child_task_config(harness)` (single-arg, no orchestrator-name
   param) discards its own result; task ids are generated locally via
   `Uuid::new_v4` instead of the pin's cloud `create_agent_task` mutation.
   Already implemented and commented in code, not yet recorded as a decision.
3. **Auto-handoff-to-cloud** (`app/src/workspace/auto_handoff.rs` +
   `one_time_modal_model.rs`'s sleep modal, 10 tests total) — offering to move
   a local conversation to a Warp-cloud-hosted agent. Non-cloud substrate
   (`AutoCloudHandoffTrigger` enum, skip-reason logic) exists in-tree, but
   every actual trigger path needs `CloudAgentTelemetryEvent`/cloud handoff.
   Adjacent to, but not the same as, the existing RunAgents/#290 row.

## Where `SCOPE-REST.md` / the inventory was wrong or imprecise

- `SCOPE-REST.md:220`'s `app/src/workspace/view_tests.rs` breakdown is
  **accurate** on re-verification (7 cloud / 7 non-cloud, one of the latter
  already ported) — no correction needed, just confirmed.
- The inventory's mechanical `PORTABLE?` tag on
  `app/src/search/slash_command_menu/static_commands/mod_tests.rs` (7 tests)
  was **half wrong**: 4 of the 7 need cloud-only `Availability` bits
  (`CLOUD_AGENT`, `CLOUD_MODE_V2_COMPOSER`) and should never be ported; only
  3 are genuinely non-cloud, and those need a missing subsystem
  (`/index` command + `CODEBASE_CONTEXT` bit), not a mechanical port.
- The inventory's mechanical `DIVERGENT?` tag on
  `app/src/settings_view/custom_inference_modal_tests.rs` (19 tests) should
  be **DECLINED** — it's the exact `CustomEndpoint` mechanism `DECLINED.md`
  #142/#347 already retired in favor of `AgentProviderSecrets`, not an
  independent feature gap.
- `app/src/pane_group/mod_tests.rs` and `app/src/settings_view/mod_tests.rs`
  were already fully adjudicated **in-file** by an earlier pass (see their
  module-doc headers) before this sweep started; the inventory's per-test
  `CLOUD?`/mechanical tags for those two files add nothing beyond what the
  files already say, and in `pane_group/mod_tests.rs`'s case may be *less*
  precise than the file's own eager-vs-lazy-restore distinction (see the
  "needs a second look" sub-list above).

## Left for the next pass

Confirmed **PORTABLE**, not ported this round, in priority order:

1. **`app/src/workspace/view_tests.rs`** — `test_open_file_notebook_focuses_existing_markdown_pane`,
   `test_open_vertical_tabs_panel_is_idempotent`, `test_active_tab_bar_position_id_tracks_layout`,
   `test_new_session_menu_is_capped_to_window_height`, `copy_model_and_profile_preserves_explicit_model_over_source_profile_default`.
   All 5 target functions/state confirmed present in `view.rs`. Deferred
   because `view_test.rs` is a ~4,700-line file with its own elaborate
   `App::test`/`mock_workspace` GPUI harness, and none of these tests could
   be verified to compile without building (hard constraint: no `cargo`).
   `copy_model_and_profile_preserves_explicit_model_over_source_profile_default`
   specifically needs `LLMInfo::new_for_test`, `AvailableLLMs::new`, and
   `AIExecutionProfilesModel::create_profile` exercised together — highest
   value (it's a real regression guard, and `copy_model_and_profile_to_terminal_view`
   at `view.rs:12536` looks like it already has the fixed, non-buggy
   behavior) but also the most fixture-heavy.
2. **`test_tab_bar_traffic_light_space_regression_for_resource_center_overlap`**
   (same file) — the pin's `should_reserve_traffic_light_space_in_tab_bar(side)`
   is a one-line pure predicate (`side == TrafficLightSide::Right`) in the
   pin, but this fork inlines the equivalent check at three call sites in
   `view.rs` (~17990, ~18016, ~19105) instead of extracting it. Porting the
   test as-is would mean adding a new standalone function nothing calls —
   dead code exercised by nothing real. Correctly porting it means extracting
   the existing inline checks into one function first, a small refactor
   across three call sites I did not attempt without being able to compile.
3. **`app/src/pane_group/mod_tests.rs`** — the five "needs a second look"
   tests above (`test_reattach_panes_restores_hidden_child_when_parent_is_already_fullscreen`
   and siblings) that may exercise the fork's *eager* restore path rather
   than the missing *lazy* one. Not re-traced against `create_missing_child_agent_panes`
   this pass.

## Ranked by "least sure it compiles"

Only two tests were actually written this pass, both in
`app/src/pane_group/pane/local_harness_launch_tests.rs`:

1. **`prepare_local_claude_child_merges_anthropic_model_env_var`** — most
   risk. Chains `ClaudeHarness::validate()` (PATH lookup via
   `resolve_executable`), `prepare_claude_environment_config` (writes
   `$HOME/.claude.json` + `$HOME/.claude/settings.json`, dirs-crate home
   resolution), and `plugin_manager_for(CLIAgent::Claude).install()` (invokes
   the faked `claude` binary with `plugin marketplace add`/`plugin install`,
   verified to be a harmless instant no-op against the fake script but not
   verified against a real build). Modeled closely on the pin's own fixture
   (`write_fake_cli`, `EnvVarGuard`) and on this fork's own
   `claude_code_tests.rs::prepare_claude_environment_config_without_config_dir_uses_home_global_config`,
   which exercises the same `HOME`-env-var-plus-`tempfile` pattern
   successfully elsewhere in the tree — but the full call chain through
   `prepare_local_harness_child_launch` was not exercised end-to-end before
   this pass.
2. **`prepare_local_claude_child_no_anthropic_model_when_empty`** — same
   fixture and call chain as #1, so the same risk profile, but the assertion
   surface is narrower (one `contains_key` check instead of several), so a
   fixture mistake is more likely to surface as a wrong-value assertion
   failure than a panic.

Everything else in this file (`local_claude_child_prompt_includes_oz_cli_messaging_instructions`
through `compose_child_agent_prompt_is_a_verbatim_passthrough`) was already
in the fork before this pass and untouched.

`rustfmt --check --config-path .rustfmt.toml --edition 2024` passes clean on
the one changed file. `script/check_cloud_boundary`,
`script/check_stub_coverage`, and `script/check_settings_registry` do not
apply — no settings were registered or asserted, and no cloud-boundary
imports were added (the two ported tests remove the pin's `MockAIClient`/
`crate::server::server_api::ai` dependency rather than adding one).
