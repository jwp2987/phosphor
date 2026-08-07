# Authoritative scope classification - remaining areas

Oracle pin: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. **Not** `warp/master`.

**Slice.** Every test file at the pin *except* those under `app/src/ai/`, `crates/ai/`,
`app/src/terminal/`, `crates/warp_terminal/`, `crates/warp_tui/` (covered by two sibling audits).

## Method

Every number below is a **test-function count**, produced by extracting the name of every
function carrying a test attribute (`#[test]`, `#[tokio::test]`, `#[gpui::test]`,
`#[test_case]`, `#[rstest]`, ...) from every `.rs` file at each revision, then matching
**name against name across the whole fork tree**. Nothing is matched by path.

This is the fix for the two errors that have burned this project before:

- *"468 missing test files"* came from path matching, which counts the fork's
  `*_tests.rs` -> `*_test.rs` rename and its `a/b/c_tests.rs` -> `a/b_c_tests.rs`
  flattening as absent. A file whose tests all resurface elsewhere in the fork under
  the same names is **B**, wherever it now lives.
- *"path is scope"* fails in both directions. `app/src/server/server_api/ai_tests.rs`
  reads as pure cloud and is 33% retained; `app/src/settings_view/custom_inference_modal_tests.rs`
  reads as a settings page and is a BYOP feature gap. Every C verdict below quotes the
  `use` lines of the **source** file at the pin, not the test file, and every C was
  checked for a local/BYOP equivalent first.

Extractor calibration: it counts **10,112** tests at the pin and **7,873** on fork `main`,
against `ORACLE.md`'s 10,123 / 7,884 - off by exactly 11 on both sides, so the delta it
reports is exact.

## Verdicts

| | meaning |
|---|---|
| **A - TEST DEBT** | Fork ships the source. Tests genuinely missing. Port them. |
| **B - ALREADY COVERED** | Every test-function name is present in the fork, under a renamed / moved / flattened path. Nothing to do. |
| **C - OUT OF SCOPE** | Cloud or otherwise dropped by design. Justified from the pin source's imports. |
| **D - FEATURE GAP** | Non-cloud, but the fork lacks the source entirely. Porting the tests requires porting the feature. |
| **MIXED** | Classified per test; the split is given in the evidence column. |

## Totals

| verdict | files | tests |
|---|---:|---:|
| A - test debt | 104 | **614** |
| B - already covered | 295 | 2778 (already present) |
| C - out of scope (cloud/dropped) | 63 | 782 |
| D - feature gap | 25 | 266 |
| MIXED (split across A/C/D above) | 6 | - |

Test files at the pin in this slice: **493**, holding **5521** tests.
Present in the fork by name: **3859**. Absent: **1662**.

### The headline number

Of the **1662** tests absent from the fork in this slice:

- **614 are real test debt** - the code is there, the tests are not.
- **782 are out of scope** - cloud sync, cloud accounts/billing/teams, cloud
  objects and Warp Drive, cloud ambient agents and their environments, cloud telemetry
  upload, cloud-backed codebase embeddings, and the cloud half of the `oz` CLI.
- **266 are feature gaps** - non-cloud functionality the fork simply does not have.
  Porting these tests means porting the feature first; they should not be counted as
  test debt and they should not be counted as out of scope either.

So the portable target for this slice is **614**, not 1662.

### D is the finding worth acting on

266 tests sit behind 25 non-cloud features the fork dropped. The largest:

| feature | tests | missing source |
|---|---:|---|
| Local control server (`oz` <-> running app IPC) | 53 | `app/src/local_control/**` (15 files), `crates/warp_cli/src/local_control/**`, `app/src/settings/local_control.rs` |
| JSON tree UI widget | 34 | `app/src/ui_components/json_tree.rs` |
| Jupyter `.ipynb` parsing | 24 | `crates/ipynb_parser/src/lib.rs` |
| LSP support | 22 | `crates/lsp/**` + `app/src/code/language_server_extension.rs`, `language_server_shutdown_manager.rs`, `lsp_logs.rs`, `lsp_telemetry.rs` |
| MCP runtime + OAuth (shared crate) | 23 | `crates/mcp/src/runtime.rs`, `crates/mcp/src/oauth.rs` |
| BYOP custom-endpoint editor | 19 | `app/src/settings_view/custom_inference_modal.rs` |
| Remote diff-state tracker | 19 | `app/src/remote_server/diff_state_tracker.rs` |
| Onboarding AI-setup routing | 12 | `crates/onboarding/src/slides/ai_setup_slide.rs`, `ai_access_slide.rs` |
| Host-scoped daemon response unwrapping | 11 | `crates/remote_server/src/host_response.rs` |
| Local git/GitHub repo status models | 7 | `app/src/code_review/{github_repo_model,git_repo_model}/local.rs` |

`crates/local_control` is worth singling out: the fork **ships the crate** (14 files) but
`git grep local_control origin/main -- app/src crates/warp_cli` returns nothing. It is ported
and unwired - see the defect note at the end.

### Corrections to prior assumptions

- **`crates/computer_use` is not a dropped feature.** The fork ships 28 of the pin's 45
  files - all the keyboard, mouse, screenshot and platform backends. What it drops is
  `overlay.rs`, `recording*.rs`, `mock.rs`, `mac/activation.rs`, `mac/post.rs`,
  `mac/window.rs`, `linux/x11/seat.rs` and `linux/x11/windows.rs`. Of its 69 tests, 58 are
  C (agent screen recording, which fed cloud artifacts) and **11 are D** (`PointerSession`
  input state machine, macOS window activation).
- **`app/src/code_review/diff_state/remote_tests.rs` is not a feature gap.** The fork
  flattened `diff_state/remote.rs` to `diff_state_remote.rs` and kept a
  `diff_state_remote_tests.rs` - but wrote 8 tests of its own, sharing zero names with the
  pin's 26. Retained source, missing tests: A.
- **`app/src/settings_view/custom_inference_modal.rs` is not cloud.** It imports
  `::ai::api_keys::{CustomEndpoint, CustomEndpointSchema}` - BYOP. The fork substituted its
  own `agent_providers_widget.rs`, so this is D, not C.

## Gap by area

| area | tests at pin | missing | A | C | D |
|---|---:|---:|---:|---:|---:|
| `app/src/settings_view` | 228 | 166 | 13 | 134 | 19 |
| `app/src/server` | 177 | 155 | 1 | 154 | 0 |
| `crates/warp_cli` | 216 | 143 | 7 | 113 | 23 |
| `app/src/remote_server` | 100 | 94 | 29 | 46 | 19 |
| `app/src/pane_group` | 106 | 76 | 40 | 36 | 0 |
| `crates/computer_use` | 69 | 69 | 0 | 58 | 11 |
| `crates/repo_metadata` | 129 | 55 | 55 | 0 | 0 |
| `crates/remote_server` | 99 | 54 | 38 | 5 | 11 |
| `app/src/code_review` | 126 | 51 | 44 | 0 | 7 |
| `app/src/settings` | 110 | 48 | 14 | 17 | 17 |
| `crates/warp_server_client` | 48 | 48 | 0 | 48 | 0 |
| `app/src/workspace` | 186 | 46 | 29 | 17 | 0 |
| `app/src/ui_components` | 43 | 43 | 3 | 0 | 40 |
| `app/src/workspaces` | 39 | 39 | 0 | 39 | 0 |
| `app/src/code` | 179 | 37 | 37 | 0 | 0 |
| `crates/warpui_core` | 540 | 35 | 30 | 0 | 5 |
| `app/src/persistence` | 39 | 30 | 30 | 0 | 0 |
| `app/src/search` | 195 | 30 | 30 | 0 | 0 |
| `app/src/themes` | 34 | 30 | 30 | 0 | 0 |
| `app/src/local_control` | 28 | 28 | 0 | 0 | 28 |
| `crates/build_cache` | 28 | 28 | 0 | 28 | 0 |
| `crates/onboarding` | 26 | 26 | 6 | 8 | 12 |
| `app/src/notebooks` | 120 | 24 | 24 | 0 | 0 |
| `crates/ipynb_parser` | 24 | 24 | 0 | 0 | 24 |
| `crates/mcp` | 23 | 23 | 0 | 0 | 23 |
| `app/src/util` | 139 | 22 | 22 | 0 | 0 |
| `crates/cloud_object_models` | 29 | 22 | 0 | 22 | 0 |
| `crates/lsp` | 22 | 22 | 0 | 0 | 22 |
| `app/src/uri` | 66 | 18 | 18 | 0 | 0 |
| `crates/editor` | 477 | 16 | 16 | 0 | 0 |
| `app/src (top level)` | 71 | 14 | 14 | 0 | 0 |
| `app/src/cloud_object` | 33 | 12 | 0 | 12 | 0 |
| `app/src/tracing` | 11 | 11 | 0 | 11 | 0 |
| `crates/vim` | 68 | 11 | 11 | 0 | 0 |
| `app/src/view_components` | 12 | 10 | 10 | 0 | 0 |
| `crates/warp_core` | 54 | 10 | 10 | 0 | 0 |
| `app/src/auth` | 9 | 9 | 0 | 9 | 0 |
| `crates/http_client` | 8 | 8 | 5 | 3 | 0 |
| `crates/persistence` | 21 | 8 | 8 | 0 | 0 |
| `crates/warpui` | 92 | 8 | 8 | 0 | 0 |
| `crates/managed_secrets` | 32 | 7 | 1 | 6 | 0 |
| `app/src/tui` | 6 | 6 | 6 | 0 | 0 |
| `crates/warp_multi_agent_client` | 6 | 6 | 0 | 6 | 0 |
| `crates/asset_cache` | 6 | 5 | 5 | 0 | 0 |
| `crates/warp_server_auth` | 5 | 5 | 0 | 5 | 0 |
| `crates/voice_input` | 4 | 4 | 4 | 0 | 0 |
| `crates/warpui_extras` | 44 | 4 | 1 | 0 | 3 |
| `app/src/launch_configs` | 9 | 3 | 3 | 0 | 0 |
| `crates/graphql` | 3 | 3 | 0 | 3 | 0 |
| `crates/languages` | 6 | 3 | 3 | 0 | 0 |
| `app/src/context_chips` | 92 | 2 | 2 | 0 | 0 |
| `app/src/drive` | 27 | 2 | 0 | 2 | 0 |
| `crates/input_classifier` | 20 | 2 | 2 | 0 | 0 |
| `crates/warp_errors` | 2 | 2 | 0 | 0 | 2 |
| `app/src/bin` | 1 | 1 | 1 | 0 | 0 |
| `app/src/completer` | 11 | 1 | 1 | 0 | 0 |
| `app/src/tab_configs` | 65 | 1 | 1 | 0 | 0 |
| `crates/warp_completer` | 181 | 1 | 1 | 0 | 0 |
| `crates/warp_features` | 2 | 1 | 1 | 0 | 0 |

## Per-file classification

One row per test file at the pin in this slice. `missing` is `tests at pin` minus the
number of that file's test-function names found anywhere in the fork.

### Files with missing tests

| path | verdict | pin | fork | missing | evidence |
|---|---|---:|---:|---:|---|
| `crates/warp_cli/src/lib_tests.rs` | MIXED | 126 | 19 | 107 | Fork's warp_cli drops api_key/artifact/environment/federate/harness_support/integration/local_control/memory_store/runner/schedule/secret/task modules and the RunCloud/Get/Create/Update/Delete/Skills AgentCommand variants. 103 missing tests parse dropped cloud subcommands (`agent run-cloud` 13, cloud `agent run` worker flags --task-id/--snapshot/--skip-initial-turn/--conversation 11, `run_cloud_*` 5, agent create/update 6, harness-support finish-task/report-* 10, artifact 8, run message 8, schedule 6, memory/memory-store 12, integration 4, my-server mcp-json 4, environment 5, secret 3, api-key 2, login/logout 3, whoami server overrides 1, run-ambient 1). 4 are non-cloud gaps: 2 x `harness_parse_*_accepts_codex` (fork's Harness enum drops Codex) and 2 x `agent_run_*bedrock_role_region*` (fork keeps --bedrock-inference-role but not --bedrock-role-region). |
| `app/src/server/cloud_objects/update_manager_tests.rs` | C | 73 | 1 | 72 | Cloud object update manager. update_manager.rs: `pub use cloud_object_client::GetCloudObjectResponse; use warp_graphql::mcp_gallery_template::MCPGalleryTemplate; use warp_graphql::object_permissions::AccessLevel; use crate::ai::cloud_environments::{AmbientAgentEnvironment, CloudAmbientAgentEnvironmentModel}; use crate::cloud_object::model::persistence::{CloudModel, CloudModelEvent, UpdateSource};`. 1 of 73 tests has a fork equivalent by name. |
| `app/src/pane_group/mod_tests.rs` | MIXED | 48 | 2 | 46 | app/src/pane_group/mod.rs is retained, but the fork dropped app/src/pane_group/ambient_pane_restoration.rs and the whole child_agent/ subtree. 36 of the 46 missing tests target that dropped cloud/ambient surface (hidden child-agent panes, ambient task ids, cloud-mode transcript restore, decide_remote_child_hydration_*, and the shared-session-over-cloud helpers). The other 10 are genuine debt on retained pane-group behaviour: test_pane_focus_on_close, test_active_session_id_reset_on_last_pane_close, test_add_pane_aborts_cleanly_when_pre_attach_returns_false, test_group_without_terminals, test_update_session_visibility, test_initial_widths_are_computed_correctly, test_navigation_skips_hidden_closed_panes, test_terminal_pane_headers, test_pane_focus_does_not_have_an_infinite_event_loop, test_focused_pane_is_synchronized_with_application_focus. |
| `app/src/settings_view/update_environment_form_tests.rs` | C | 43 | 0 | 43 | Cloud ambient-agent environments. update_environment_form.rs: `use crate::ai::cloud_environments::{AmbientAgentEnvironment, GithubRepo}; use crate::ai::ambient_agents::telemetry::CloudAgentTelemetryEvent; use crate::server::server_api::ServerApiProvider; use crate::server::ids::SyncId;` (already recorded on #211). |
| `crates/repo_metadata/src/local_model_tests.rs` | A | 54 | 12 | 42 | Fork ships the source (crates/repo_metadata/src/local_model.rs); 42 of 54 test-function names have no fork equivalent. |
| `crates/computer_use/src/overlay_tests.rs` | C | 41 | 0 | 41 | overlay.rs doc comment: `//! Action overlay model and .ass subtitle generation for burned-in recording annotations.` Its only consumers at the pin are app/src/ai/blocklist/action_model/recording_controller.rs and use_computer.rs; the fork drops the recording controller and the cloud artifact-upload path the recordings feed. Reclassify to D if local agent screen-recording is ever wanted. |
| `app/src/remote_server/codebase_index_model_tests.rs` | C | 39 | 0 | 39 | Cloud-backed codebase embedding index. codebase_index_model.rs: `use ai::index::full_source_code_embedding::NodeHash; use crate::ai::codebase_auto_indexing::{...};` - the fork's crates/ai/src/index contains only file_outline/ and locations.rs (no full_source_code_embedding), and app/src/ai/codebase_auto_indexing/ is absent. Embeddings are produced by the cloud `generate_code_embeddings` GraphQL mutation. |
| `app/src/ui_components/json_tree_tests.rs` | D | 34 | 0 | 34 | Non-cloud generic widget. json_tree.rs imports only `pathfinder_color::ColorU`, `warp_core::ui::{icons::Icon, theme::WarpTheme}`, `warpui::elements`, `crate::appearance::Appearance`. Missing source: app/src/ui_components/json_tree.rs. |
| `app/src/settings_view/environments_page_tests.rs` | C | 32 | 0 | 32 | Cloud ambient-agent environments. environments_page.rs: `use crate::ai::cloud_environments::{self, CloudAmbientAgentEnvironment}; use crate::cloud_object::model::persistence::{CloudModel, CloudModelEvent}; use crate::drive::CloudObjectTypeAndId; use crate::server::cloud_objects::update_manager::{...};` (already recorded on #211). |
| `app/src/server/server_api/ai_tests.rs` | MIXED | 45 | 15 | 30 | Calibration case. server_api/ai.rs is a cloud GraphQL surface (`use warp_graphql::mutations::generate_dialogue::{...}`, `use warp_graphql::queries::get_relevant_fragments::{...}`, `use cynic::{MutationBuilder, QueryBuilder}`), but 15 of its 45 tests already have fork equivalents by name and cover retained non-cloud helpers. The remaining 30 all sit on cloud mutations/queries (merkle-tree sync, code embeddings, agent tasks, request limits, bonus grants). |
| `app/src/themes/theme_tests.rs` | A | 31 | 1 | 30 | Fork ships the source (app/src/themes/theme.rs); 30 of 31 test-function names have no fork equivalent. |
| `app/src/workspaces/user_workspaces_tests.rs` | C | 29 | 0 | 29 | Cloud team workspaces. All 29 assertions exercise team-scoped cloud policy: `test_aws_bedrock_credentials_enforced_by_admin`, `test_gemini_enterprise_credentials_*`, `test_window_team_assignment_*`, `test_joining_team_moves_objects`, `test_codebase_context_enabled_by_team_*`. user_workspaces.rs survives in the fork only as a stub for the retained call sites; team_tester.rs, update_manager.rs and gql_convert.rs are all absent. |
| `app/src/code_review/diff_state/remote_tests.rs` | A | 26 | 0 | 26 | NOT a feature gap - the fork flattened app/src/code_review/diff_state/remote.rs to app/src/code_review/diff_state_remote.rs and kept diff_state_remote_tests.rs, but wrote 8 tests of its own with different names, so none of the pin's 26 match. Retained non-cloud source (`use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent}; use crate::remote_server::diff_state_proto::{try_decode_file_delta, try_decode_snapshot};`). |
| `crates/build_cache/src/lib_tests.rs` | C | 25 | 0 | 25 | Crate absent from fork. lib.rs doc: `//! Persistent build cache management for sandboxed agents.` and `use spacectl::{MountResponse, run_spacectl_mount};` (namespacelabs). Sole consumer at pin is app/src/ai/agent_sdk/driver/cache_setup.rs (cloud sandbox driver), also absent from fork. |
| `app/src/code/editor/view/vim_handler_tests.rs` | A | 65 | 41 | 24 | Fork ships the source (app/src/code/editor/view/vim_handler.rs); 24 of 65 test-function names have no fork equivalent. |
| `crates/ipynb_parser/src/lib_tests.rs` | D | 24 | 0 | 24 | Non-cloud. lib.rs imports only `std::collections::BTreeMap`, `markdown_parser`, `serde::Deserialize`. Jupyter .ipynb parsing; fork keeps the .ipynb file-type constant in warp_util/file_type.rs but has no parser. Missing source: crates/ipynb_parser/src/lib.rs. |
| `app/src/local_control/mod_tests.rs` | D | 23 | 0 | 23 | Non-cloud. mod.rs imports `::local_control::auth::{CredentialRequest, ScopedCredential}`, `axum::{Router, Json, routing::post}`, `warp_core::channel::ChannelState` - a localhost HTTP control server, no cloud client anywhere. Fork has crates/local_control but the entire app/src/local_control/ tree (15 files) is absent and unreferenced. Missing sources: app/src/local_control/{mod,bridge,handlers,permissions,resolver}.rs. |
| `crates/remote_server/src/setup_tests.rs` | A | 38 | 15 | 23 | Fork ships the source (crates/remote_server/src/setup.rs); 23 of 38 test-function names have no fork equivalent. |
| `crates/warp_server_client/src/iap_tests.rs` | C | 23 | 0 | 23 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `crates/lsp/src/config_tests.rs` | D | 22 | 0 | 22 | Non-cloud. config.rs imports only `lsp_types`, `command::r#async::Command`, `crate::supported_servers::LSPServerType` - zero cloud deps. Fork lacks the whole crate AND the app integration (app/src/code/language_server_extension.rs, language_server_shutdown_manager.rs, lsp_logs.rs, lsp_telemetry.rs all absent). Missing source: crates/lsp/src/config.rs. |
| `app/src/search/slash_command_menu/static_commands/commands_tests.rs` | A | 27 | 6 | 21 | Fork ships the source (app/src/search/slash_command_menu/static_commands/commands.rs); 21 of 27 test-function names have no fork equivalent. |
| `crates/cloud_object_models/src/cloud_environment_tests.rs` | C | 20 | 0 | 20 | Cloud object model crate; absent from fork. cloud_environment.rs: `use cloud_objects::cloud_object::{...}; use cloud_objects::ids::GenericStringObjectId;`. |
| `app/src/remote_server/diff_state_tracker_tests.rs` | D | 19 | 0 | 19 | Non-cloud. diff_state_tracker.rs imports `warp_util::standardized_path::StandardizedPath`, `super::{protocol::RequestId, server_model::ConnectionId}`, `crate::code_review::diff_state::{...}` - pure local/SSH diff-state fan-out bookkeeping. Fork ships diff-state over SSH but has no tracker module. Missing source: app/src/remote_server/diff_state_tracker.rs. |
| `app/src/settings_view/custom_inference_modal_tests.rs` | D | 19 | 0 | 19 | NOT cloud - this is the BYOP custom-endpoint editor. custom_inference_modal.rs: `use ::ai::api_keys::{CustomEndpoint, CustomEndpointSchema};` plus editor/modal/dropdown UI only. The fork substituted its own app/src/settings_view/agent_providers_widget.rs, so none of the 19 assertions have a fork equivalent. Missing source: app/src/settings_view/custom_inference_modal.rs. |
| `crates/warp_cli/src/local_control_tests.rs` | D | 19 | 0 | 19 | Non-cloud local IPC control surface. local_control/mod.rs imports only clap/clap_complete + `crate::agent::OutputFormat`. Fork ships crates/local_control (protocol/client) but nothing references it (`git grep local_control origin/main -- app/src crates/warp_cli` is empty), so the CLI + app surface is unported. Missing source: crates/warp_cli/src/local_control/mod.rs. |
| `app/src/server/telemetry/secret_redaction_tests.rs` | C | 18 | 0 | 18 | Cloud telemetry payload redaction. secret_redaction.rs exists only to scrub values on their way into Rudderstack events (`use serde_json::Value; use warp_errors::report_error;`, consumed from app/src/server/telemetry_ext.rs `use super::telemetry::secret_redaction::redact_secrets_in_value;`). The fork keeps app/src/server/telemetry.rs but not the upload path, and has no redact_secrets_in_value. |
| `app/src/uri/uri_tests.rs` | A | 65 | 47 | 18 | Fork retains the module; 18 of 65 names have no fork equivalent. |
| `app/src/settings/cloud_preferences_syncer_tests.rs` | C | 17 | 0 | 17 | Cloud settings sync. cloud_preferences_syncer.rs: `use crate::cloud_object::model::persistence::CloudModel; use crate::cloud_object::{CloudObjectEventEntrypoint, GenericStringObjectFormat, JsonObjectType}; use crate::drive::CloudObjectTypeAndId; use crate::server::cloud_objects::update_manager::{...}; use crate::server::sync_queue::{SyncQueue, SyncQueueEvent};`. |
| `crates/onboarding/src/model_tests.rs` | MIXED | 17 | 0 | 17 | crates/onboarding/src/model.rs is retained but the fork drops slides/ai_access_slide.rs, slides/ai_setup_slide.rs, slides/offer_slide.rs, slides/upgrade_auth_prompt.rs and components/feature_optout_dialog.rs. 5 tests target the cloud account-first / post-auth-offer path (account_first_path_is_linear_and_reversible, account_first_path_uses_three_step_progress, account_first_path_uses_agent_ui_defaults, post_auth_offer_is_unclassified_until_selected_and_does_not_switch, post_auth_offer_supports_back_to_theme_and_no_direct_next); the other 12 are non-cloud onboarding routing on slides the fork still ships. |
| `app/src/remote_server/diff_state_proto_tests.rs` | A | 15 | 0 | 15 | Fork ships the source (app/src/remote_server/diff_state_proto.rs); 15 of 15 test-function names have no fork equivalent. |
| `app/src/workspace/view_tests.rs` | MIXED | 85 | 70 | 15 | app/src/workspace/view.rs is retained and 70/85 tests are covered. 8 of the 15 missing are cloud: test_reward_modal_no_overlap, test_reward_modal_shows_for_received_referral (cloud referrals), test_stop_sharing_session, test_stop_sharing_all_sessions_in_tab, test_tab_context_menu_share_session_items (cloud session sharing), test_open_cloud_agent_setup_guide_action_opens_management_view_and_is_idempotent, test_tools_panel_warp_drive_toggle_updates_available_views (Warp Drive), test_tools_panel_preferences_activate_after_signup_and_ai_enablement (cloud signup). The other 7 are genuine debt on retained workspace-view code: test_tab_bar_traffic_light_space_regression_for_resource_center_overlap, copy_model_and_profile_preserves_explicit_model_over_source_profile_default, test_open_file_notebook_focuses_existing_markdown_pane, test_open_vertical_tabs_panel_is_idempotent, test_active_tab_bar_position_id_tracks_layout, test_new_session_menu_is_capped_to_window_height, test_tab_mru_order. |
| `crates/mcp/src/runtime_tests.rs` | D | 15 | 0 | 15 | Non-cloud MCP runtime. runtime_tests.rs tests `query_tools_for` / `query_resources_for` capability negotiation over `rmcp`; oauth.rs imports `oauth2`, `rmcp::transport::auth`, `warpui_extras::secure_storage` - no cloud client. Fork's MCP lives in app/src/ai/mcp but these names exist nowhere in the fork. Missing sources: crates/mcp/src/runtime.rs, crates/mcp/src/oauth.rs. |
| `app/src/remote_server/server_model_tests.rs` | A | 17 | 3 | 14 | Fork ships the source (app/src/remote_server/server_model.rs); 14 of 17 test-function names have no fork equivalent. |
| `app/src/server/sync_queue_tests.rs` | C | 13 | 0 | 13 | Cloud object sync queue. sync_queue.rs: `pub use cloud_objects::cloud_object::SerializedModel; use warp_graphql::scalars::time::ServerTimestamp; use super::graphql::GraphQLError; use crate::cloud_object::{...}; use crate::drive::CloudObjectTypeAndId;`. |
| `app/src/cloud_object/model/model_tests.rs` | C | 27 | 15 | 12 | Cloud object model. The fork keeps a reduced app/src/cloud_object/model/ (15/27 tests covered by model_test.rs); the 12 missing ones exercise CloudModel sync/persistence semantics that the fork's stub drops. |
| `app/src/pane_group/pane/local_harness_launch_tests.rs` | A | 18 | 6 | 12 | Fork ships the source (app/src/pane_group/pane/local_harness_launch.rs); 12 of 18 test-function names have no fork equivalent. |
| `app/src/persistence/agent_tests.rs` | A | 12 | 0 | 12 | Fork ships the source (app/src/persistence/agent.rs); 12 of 12 test-function names have no fork equivalent. |
| `app/src/settings_view/billing_and_usage/billing_cycle_usage_common_tests.rs` | C | 12 | 0 | 12 | Cloud billing-cycle usage rendering; imports `crate::settings_view::billing_and_usage_page_v2::{...}` and `crate::workspaces::workspace::{...}` (cloud team workspace), both absent from the fork. |
| `app/src/util/file/external_editor/linux_tests.rs` | A | 29 | 17 | 12 | Fork ships the source (app/src/util/file/external_editor/linux.rs); 12 of 29 test-function names have no fork equivalent. |
| `app/src/pane_group/working_directories_tests.rs` | A | 13 | 2 | 11 | Fork ships the source (app/src/pane_group/working_directories.rs); 11 of 13 test-function names have no fork equivalent. |
| `app/src/persistence/sqlite_tests.rs` | A | 20 | 9 | 11 | Fork ships the source (app/src/persistence/sqlite.rs); 11 of 20 test-function names have no fork equivalent. |
| `app/src/tracing/cloud_agent_auth_tests.rs` | C | 11 | 0 | 11 | Cloud agent OTLP auth. cloud_agent_auth.rs: `use warp_managed_secrets::client::{IdentityTokenOptions, ManagedSecretsClient, TaskIdentityToken}; use opentelemetry_http::{HttpClient, HttpError, Request, Response};` - signs OTLP exports with a cloud task identity token. |
| `crates/remote_server/src/host_response_tests.rs` | D | 11 | 0 | 11 | Non-cloud. host_response.rs: `use crate::proto::{ServerMessage, server_message};` - helpers that unwrap nested per-operation errors from host-scoped daemon responses. Missing source: crates/remote_server/src/host_response.rs. |
| `crates/vim/src/vim_tests.rs` | A | 32 | 21 | 11 | Fork ships the source (crates/vim/src/vim.rs); 11 of 32 test-function names have no fork equivalent. |
| `app/src/view_components/dismissible_toast_tests.rs` | A | 10 | 0 | 10 | Fork ships the source (app/src/view_components/dismissible_toast.rs); 10 of 10 test-function names have no fork equivalent. |
| `crates/warpui_core/src/presenter/tui_tests.rs` | A | 10 | 0 | 10 | Fork ships the source (crates/warpui_core/src/presenter/tui.rs); 10 of 10 test-function names have no fork equivalent. |
| `app/src/server/server_api/presigned_upload_tests.rs` | C | 9 | 0 | 9 | Cloud artifact upload. presigned_upload.rs: `pub use warp_server_client::HttpStatusError; use super::ai::FileArtifactUploadTargetInfo; use super::harness_support::{UploadFieldValue, UploadTarget};`. |
| `app/src/settings_view/mod_tests.rs` | MIXED | 52 | 43 | 9 | app/src/settings_view/mod.rs is retained and 43/52 tests are covered. 4 of the 9 missing are cloud (`cloud_platform_subpages_are_identified`, `cloud_platform_subpages_map_to_their_backing_pages`, and the two collapsed-umbrella walks that traverse the absent Account/Billing pages); 5 are genuine debt on retained nav code (`code_subpages_are_identified`, `code_subpages_map_to_code_backing_page`, `arrow_down_collapsed_umbrella_respects_search_filter`, `search_terms_match_direct_unit_checks`, `arrow_down_across_adjacent_collapsed_umbrellas`). |
| `app/src/workspace/auto_handoff_tests.rs` | C | 9 | 0 | 9 | Cloud handoff. auto_handoff.rs: `use crate::ai::ambient_agents::telemetry::CloudAgentTelemetryEvent; use crate::ai::active_agent_views_model::{ActiveAgentViewsModel, ConversationOrTaskId}; use crate::ai::blocklist::orchestration_topology::has_local_orchestrated_children;` - hands a local conversation off to a cloud ambient agent. Source absent from fork. |
| `app/src/notebooks/editor/model_tests.rs` | A | 70 | 62 | 8 | Fork ships the source (app/src/notebooks/editor/model.rs); 8 of 70 test-function names have no fork equivalent. |
| `app/src/settings_view/agent_assisted_environment_modal_tests.rs` | C | 8 | 0 | 8 | Cloud environment creation flow. Imports `ai::index::full_source_code_embedding::manager::CodebaseIndexManager` (cloud embedding index) and is reached only from environments_page.rs (`use super::agent_assisted_environment_modal::{...}`). |
| `crates/editor/src/content/edit_tests.rs` | A | 24 | 16 | 8 | Fork ships the source (crates/editor/src/content/edit.rs); 8 of 24 test-function names have no fork equivalent. |
| `crates/mcp/src/oauth_tests.rs` | D | 8 | 0 | 8 | Non-cloud MCP runtime. runtime_tests.rs tests `query_tools_for` / `query_resources_for` capability negotiation over `rmcp`; oauth.rs imports `oauth2`, `rmcp::transport::auth`, `warpui_extras::secure_storage` - no cloud client. Fork's MCP lives in app/src/ai/mcp but these names exist nowhere in the fork. Missing sources: crates/mcp/src/runtime.rs, crates/mcp/src/oauth.rs. |
| `crates/persistence/src/model_tests.rs` | A | 21 | 13 | 8 | Fork ships the source (crates/persistence/src/model.rs); 8 of 21 test-function names have no fork equivalent. |
| `crates/remote_server/src/client_tests.rs` | A | 16 | 8 | 8 | Fork ships the source (crates/remote_server/src/client/mod.rs); 8 of 16 test-function names have no fork equivalent. |
| `app/src/auth/auth_manager_tests.rs` | C | 7 | 0 | 7 | Cloud account auth. auth_manager.rs: `use warp_server_auth::user::persistence::PersistedUser; use warp_graphql::mutations::create_anonymous_user::{...}; use crate::server::cloud_objects::update_manager::UpdateManager; use crate::server::server_api::auth::{...}; use crate::workspaces::team_tester::TeamTesterStatus;`. |
| `app/src/persistence/block_list_tests.rs` | A | 7 | 0 | 7 | Fork ships the source (app/src/persistence/block_list.rs); 7 of 7 test-function names have no fork equivalent. |
| `app/src/remote_server/codebase_index_status_tests.rs` | C | 7 | 0 | 7 | Same cloud embedding index. codebase_index_status.rs: `use ::ai::index::full_source_code_embedding::SyncProgress; use ::ai::index::full_source_code_embedding::manager::{...};`. |
| `app/src/search/slash_command_menu/static_commands/mod_tests.rs` | A | 30 | 23 | 7 | Fork ships the source (app/src/search/slash_command_menu/static_commands/mod.rs); 7 of 30 test-function names have no fork equivalent. |
| `app/src/settings/ai_tests.rs` | A | 30 | 23 | 7 | Fork ships the source (app/src/settings/ai.rs); 7 of 30 test-function names have no fork equivalent. |
| `app/src/settings_view/billing_and_usage_page_tests.rs` | C | 7 | 0 | 7 | Cloud billing. billing_and_usage_page.rs: `use warp_graphql::billing::AddonCreditsOption; use crate::pricing::{PricingInfoModel, PricingInfoModelEvent}; use crate::auth::auth_manager::LoginGatedFeature; use crate::server::ids::ServerId;`. |
| `app/src/settings_view/platform_page_tests.rs` | C | 7 | 0 | 7 | Warp platform (cloud) page. platform_page.rs: `use warp_graphql::object_permissions::OwnerType; use warp_graphql::queries::api_keys::ApiKeyProperties as GqlApiKeyProperties; use crate::server::ids::ApiKeyUid;`. |
| `crates/warp_core/src/paths_tests.rs` | A | 14 | 7 | 7 | Fork ships the source (crates/warp_core/src/paths.rs); 7 of 14 test-function names have no fork equivalent. |
| `crates/warp_server_client/src/public_api_tests.rs` | C | 7 | 0 | 7 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `app/src/code/buffer_location_tests.rs` | A | 18 | 12 | 6 | Fork ships the source (app/src/code/buffer_location.rs); 6 of 18 test-function names have no fork equivalent. |
| `app/src/code_review/diff_state/mod_tests.rs` | A | 6 | 0 | 6 | Same flattening; source retained as app/src/code_review/diff_state.rs. |
| `app/src/root_view_tests.rs` | A | 9 | 3 | 6 | Fork ships the source (app/src/root_view.rs); 6 of 9 test-function names have no fork equivalent. |
| `app/src/server/server_api/harness_support_tests.rs` | C | 7 | 1 | 6 | Cloud harness/task support API: `use crate::ai::ambient_agents::AmbientAgentTaskId; use crate::ai::artifacts::Artifact; pub use super::presigned_upload::{FileUploadBody, UploadBody};`. |
| `app/src/settings/local_control_tests.rs` | D | 6 | 0 | 6 | Non-cloud settings group for the local control server. local_control.rs: `use settings::macros::define_settings_group; use warpui_extras::secure_storage;` - no cloud imports. Missing source: app/src/settings/local_control.rs. |
| `app/src/settings/tui_theme_tests.rs` | D | 6 | 0 | 6 | Non-cloud. tui_theme.rs: `use settings::macros::define_settings_group; use warp_core::ui::theme::{ColorScheme, WarpTheme}; use warpui_core::runtime::BackgroundLuminance;`. Fork ships warp_tui but not this settings group. Missing source: app/src/settings/tui_theme.rs. |
| `app/src/settings_view/ai_page_tests.rs` | A | 6 | 0 | 6 | app/src/settings_view/ai_page.rs is present in the fork; 6 of 6 assertions absent. |
| `app/src/tui/mod_tests.rs` | A | 6 | 0 | 6 | Fork ships the source (app/src/tui/mod.rs); 6 of 6 test-function names have no fork equivalent. |
| `app/src/ui_components/agent_icon_tests.rs` | D | 6 | 0 | 6 | Non-cloud. agent_icon.rs: `use warp_cli::agent::Harness; use crate::terminal::CLIAgent; use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;` - local CLI-agent status iconography. Missing source: app/src/ui_components/agent_icon.rs. |
| `app/src/workspace/tab_settings_tests.rs` | A | 10 | 4 | 6 | Fork ships the source (app/src/workspace/tab_settings.rs); 6 of 10 test-function names have no fork equivalent. |
| `app/src/workspaces/workspace_tests.rs` | C | 6 | 0 | 6 | Cloud team workspace model; the fork keeps workspace.rs but drops the team/billing metadata these tests assert on. |
| `crates/computer_use/src/linux/recording_tests.rs` | C | 6 | 0 | 6 | Agent screen recording. Fork retains the computer-use input/screenshot primitives but drops recording.rs, recording_metadata.rs, mock.rs and overlay.rs together with app/src/ai/blocklist/action_model/recording_controller.rs, which is what uploaded recordings as cloud artifacts. |
| `crates/computer_use/src/pointer_session_tests.rs` | D | 6 | 0 | 6 | Non-cloud. Tests `PointerSession` press/move/release state machine (`use crate::{MouseButton, PointerEventKind, PointerSession, Vector2I};`) - pure local input bookkeeping with no recording or cloud dependency. `PointerSession` does not exist anywhere in the fork. Missing source: the PointerSession type in crates/computer_use/src/lib.rs. |
| `crates/managed_secrets/src/gcp_tests.rs` | C | 6 | 0 | 6 | Cloud agent identity. gcp.rs: `use crate::client::TaskIdentityToken;` - GCP-issued task identity tokens for cloud ambient agents. |
| `crates/onboarding/src/telemetry_tests.rs` | A | 6 | 0 | 6 | crates/onboarding/src/telemetry.rs is retained in the fork (telemetry_provider.rs is not); 6 of 6 assertions missing. |
| `crates/repo_metadata/src/repository_tests.rs` | A | 6 | 0 | 6 | Fork ships the source (crates/repo_metadata/src/repository.rs); 6 of 6 test-function names have no fork equivalent. |
| `crates/warp_cli/src/api_key_tests.rs` | C | 6 | 0 | 6 | Warp platform API keys (cloud account credentials). api_key.rs: `use crate::json_filter::JsonOutput; use crate::date_time::parse_rfc3339;` driving `oz api-key` against the cloud platform; fork drops api_key.rs and date_time.rs. |
| `crates/warp_multi_agent_client/src/lib_tests.rs` | C | 6 | 0 | 6 | Cloud MAA (multi-agent API) client; absent from fork. lib.rs: `use warp_server_client::base_client::{AmbientHeaderPolicy, BaseClient};`. |
| `crates/warp_server_client/src/graphql_helpers_tests.rs` | C | 6 | 0 | 6 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `app/src/code_review/comments/comment_tests.rs` | A | 15 | 10 | 5 | Fork ships the source (app/src/code_review/comments/comment.rs); 5 of 15 test-function names have no fork equivalent. |
| `app/src/code_review/github_repo_model/local_tests.rs` | D | 5 | 0 | 5 | Non-cloud. github_repo_model/local.rs imports `crate::code_review::git_repo_model::{GitRepoStatusEvent, GitRepoStatusModel}`, `crate::terminal::session_settings::GithubPrPromptChipDefaultValidation`, `crate::util::git::{...}` - local `gh`/git PR detection, no cloud client. Fork replaced the github_repo_model/ and git_repo_model/ trees with app/src/code_review/git_status_update.rs and none of these names survive. Missing source: app/src/code_review/github_repo_model/local.rs. |
| `app/src/notebooks/editor/view_tests.rs` | A | 14 | 9 | 5 | Fork ships the source (app/src/notebooks/editor/view.rs); 5 of 14 test-function names have no fork equivalent. |
| `app/src/pane_group/pane/terminal_pane_tests.rs` | A | 5 | 0 | 5 | Fork ships the source (app/src/pane_group/pane/terminal_pane.rs); 5 of 5 test-function names have no fork equivalent. |
| `app/src/settings/tui_zero_state_tests.rs` | D | 5 | 0 | 5 | Non-cloud. tui_zero_state.rs: `use settings::macros::define_settings_group; use settings::{SupportedPlatforms, SyncToCloud};` + serde only. Missing source: app/src/settings/tui_zero_state.rs. |
| `app/src/settings_view/billing_and_usage/billing_cycle_usage_section_tests.rs` | C | 5 | 0 | 5 | Cloud billing-cycle usage rendering; imports `crate::settings_view::billing_and_usage_page_v2::{...}` and `crate::workspaces::workspace::{...}` (cloud team workspace), both absent from the fork. |
| `app/src/workspace/one_time_modal_model_tests.rs` | A | 5 | 0 | 5 | Fork ships the source (app/src/workspace/one_time_modal_model.rs); 5 of 5 test-function names have no fork equivalent. |
| `app/src/workspace/view/global_search/model_tests.rs` | A | 5 | 0 | 5 | Fork ships the source (app/src/workspace/view/global_search/model.rs); 5 of 5 test-function names have no fork equivalent. |
| `app/src/workspace/view/vertical_tabs_tests.rs` | A | 60 | 55 | 5 | Fork ships the source (app/src/workspace/view/vertical_tabs.rs); 5 of 60 test-function names have no fork equivalent. |
| `crates/asset_cache/src/lib_tests.rs` | A | 6 | 1 | 5 | Fork ships the source (crates/asset_cache/src/lib.rs); 5 of 6 test-function names have no fork equivalent. |
| `crates/computer_use/src/mac/activation_tests.rs` | D | 5 | 0 | 5 | Non-cloud macOS window activation for computer use. activation.rs: `use objc2_app_kit::{NSEvent, NSEventModifierFlags, ...}; use objc2_core_graphics::{...}; use super::window::{self, WindowInfo};`. Fork drops mac/activation.rs, mac/post.rs, mac/window.rs, linux/x11/seat.rs and linux/x11/windows.rs. Missing source: crates/computer_use/src/mac/activation.rs. |
| `crates/remote_server/src/codebase_index_proto_tests.rs` | C | 5 | 0 | 5 | Proto surface for the same cloud embedding index; only meaningful with app/src/remote_server/codebase_index_model.rs, which is absent. |
| `crates/warp_server_client/src/base_client_tests.rs` | C | 5 | 0 | 5 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `crates/warpui/src/windowing/winit/event_loop/key_events_tests.rs` | A | 6 | 1 | 5 | Fork ships the source (crates/warpui/src/windowing/winit/event_loop/key_events.rs); 5 of 6 test-function names have no fork equivalent. |
| `crates/warpui_core/src/elements/animation_tests.rs` | A | 5 | 0 | 5 | Fork ships the source (crates/warpui_core/src/elements/animation.rs); 5 of 5 test-function names have no fork equivalent. |
| `app/src/code_review/diff_state/local_tests.rs` | A | 24 | 20 | 4 | Fork flattened diff_state/local.rs into app/src/code_review/diff_state.rs with diff_state_tests.rs (25 tests); 20/24 covered, 4 genuinely missing. |
| `app/src/lib_tests.rs` | A | 4 | 0 | 4 | Fork ships the source (app/src/lib.rs); 4 of 4 test-function names have no fork equivalent. |
| `app/src/notebooks/notebook_tests.rs` | A | 12 | 8 | 4 | Fork ships the source (app/src/notebooks/notebook.rs); 4 of 12 test-function names have no fork equivalent. |
| `app/src/settings_view/billing_and_usage/billing_cycle_usage_rows_tests.rs` | C | 4 | 0 | 4 | Cloud billing-cycle usage rendering; imports `crate::settings_view::billing_and_usage_page_v2::{...}` and `crate::workspaces::workspace::{...}` (cloud team workspace), both absent from the fork. |
| `app/src/settings_view/billing_and_usage/billing_cycle_usage_team_totals_tests.rs` | C | 4 | 0 | 4 | Cloud billing-cycle usage rendering; imports `crate::settings_view::billing_and_usage_page_v2::{...}` and `crate::workspaces::workspace::{...}` (cloud team workspace), both absent from the fork. |
| `app/src/tab_tests.rs` | A | 4 | 0 | 4 | Fork ships the source (app/src/tab.rs); 4 of 4 test-function names have no fork equivalent. |
| `app/src/util/openable_file_type_tests.rs` | A | 27 | 23 | 4 | Fork ships the source (app/src/util/openable_file_type.rs); 4 of 27 test-function names have no fork equivalent. |
| `crates/computer_use/src/mac/recording_tests.rs` | C | 4 | 0 | 4 | Agent screen recording. Fork retains the computer-use input/screenshot primitives but drops recording.rs, recording_metadata.rs, mock.rs and overlay.rs together with app/src/ai/blocklist/action_model/recording_controller.rs, which is what uploaded recordings as cloud artifacts. |
| `crates/computer_use/src/recording_tests.rs` | C | 4 | 0 | 4 | Agent screen recording. Fork retains the computer-use input/screenshot primitives but drops recording.rs, recording_metadata.rs, mock.rs and overlay.rs together with app/src/ai/blocklist/action_model/recording_controller.rs, which is what uploaded recordings as cloud artifacts. |
| `crates/editor/src/content/markdown_tests.rs` | A | 15 | 11 | 4 | Fork ships the source (crates/editor/src/content/markdown.rs); 4 of 15 test-function names have no fork equivalent. |
| `crates/remote_server/src/manager_tests.rs` | A | 4 | 0 | 4 | Fork ships the source (crates/remote_server/src/manager.rs); 4 of 4 test-function names have no fork equivalent. |
| `crates/voice_input/src/lib_tests.rs` | A | 4 | 0 | 4 | Fork ships the source (crates/voice_input/src/lib.rs); 4 of 4 test-function names have no fork equivalent. |
| `crates/warp_cli/src/mcp_tests.rs` | A | 10 | 6 | 4 | crates/warp_cli/src/mcp.rs is present in the fork; 4 of 10 tests missing. |
| `crates/warp_cli/src/runner_tests.rs` | C | 4 | 0 | 4 | Cloud runner registration. runner.rs: `use crate::scope::ObjectScope;` - ObjectScope is the cloud personal/team object scope. Fork drops runner.rs. |
| `crates/warpui_core/src/elements/shimmer_math_tests.rs` | A | 4 | 0 | 4 | Fork ships the source (crates/warpui_core/src/elements/shimmer_math.rs); 4 of 4 test-function names have no fork equivalent. |
| `crates/warpui_core/src/image_cache_tests.rs` | A | 18 | 14 | 4 | Fork ships the source (crates/warpui_core/src/image_cache.rs); 4 of 18 test-function names have no fork equivalent. |
| `crates/warpui_core/src/telemetry/event_store_tests.rs` | D | 4 | 0 | 4 | Non-cloud. event_store.rs imports only `bounded_vec_deque::BoundedVecDeque` and `crate::time::get_current_time` - an in-memory bounded ring buffer. Missing source: crates/warpui_core/src/telemetry/event_store.rs. |
| `app/src/code/editor/model_tests.rs` | A | 22 | 19 | 3 | Fork ships the source (app/src/code/editor/model.rs); 3 of 22 test-function names have no fork equivalent. |
| `app/src/code/file_tree/view/view_tests.rs` | A | 18 | 15 | 3 | Fork retains the module; 3 of 18 names have no fork equivalent. |
| `app/src/code_review/code_review_view_tests.rs` | A | 18 | 15 | 3 | Fork ships the source (app/src/code_review/code_review_view.rs); 3 of 18 test-function names have no fork equivalent. |
| `app/src/launch_configs/launch_config_tests.rs` | A | 9 | 6 | 3 | Fork ships the source (app/src/launch_configs/launch_config.rs); 3 of 9 test-function names have no fork equivalent. |
| `app/src/notebooks/file/mod_tests.rs` | A | 8 | 5 | 3 | Fork ships the source (app/src/notebooks/file/mod.rs); 3 of 8 test-function names have no fork equivalent. |
| `app/src/notebooks/link_tests.rs` | A | 10 | 7 | 3 | Fork ships the source (app/src/notebooks/link.rs); 3 of 10 test-function names have no fork equivalent. |
| `app/src/server/telemetry_ext_tests.rs` | C | 3 | 0 | 3 | Cloud telemetry (Rudderstack) payload assembly: `use super::telemetry::rudder_message::{...}; use warpui::telemetry::EventPayload;`. |
| `app/src/settings/onboarding_tests.rs` | A | 3 | 0 | 3 | Fork ships the source (app/src/settings/onboarding.rs); 3 of 3 test-function names have no fork equivalent. |
| `app/src/settings/theme_tests.rs` | A | 6 | 3 | 3 | Fork ships the source (app/src/settings/theme.rs); 3 of 6 test-function names have no fork equivalent. |
| `app/src/settings_view/billing_and_usage_dispatch_tests.rs` | C | 3 | 0 | 3 | Cloud billing page dispatch. `use super::billing_and_usage_page::{BillingAndUsagePageEvent, BillingAndUsagePageView}; use crate::auth::{AuthManager, AuthStateProvider}; use crate::workspaces::user_workspaces::UserWorkspaces;`. |
| `app/src/ui_components/icon_with_status_tests.rs` | A | 3 | 0 | 3 | app/src/ui_components/icon_with_status.rs is present in the fork; the sibling test file was dropped. |
| `app/src/workspaces/gql_convert_tests.rs` | C | 3 | 0 | 3 | Cloud GraphQL conversion. gql_convert.rs: `use warp_graphql::billing::{...}; use warp_graphql::queries::get_workspaces_metadata_for_user::User as GqlUser; use warp_graphql::subscriptions::get_warp_drive_updates::WarpDriveUpdate; use crate::server::cloud_objects::listener::ObjectUpdateMessage;`. |
| `crates/build_cache/src/spacectl_tests.rs` | C | 3 | 0 | 3 | Crate absent from fork. lib.rs doc: `//! Persistent build cache management for sandboxed agents.` and `use spacectl::{MountResponse, run_spacectl_mount};` (namespacelabs). Sole consumer at pin is app/src/ai/agent_sdk/driver/cache_setup.rs (cloud sandbox driver), also absent from fork. |
| `crates/computer_use/src/recording_metadata_tests.rs` | C | 3 | 0 | 3 | Agent screen recording. Fork retains the computer-use input/screenshot primitives but drops recording.rs, recording_metadata.rs, mock.rs and overlay.rs together with app/src/ai/blocklist/action_model/recording_controller.rs, which is what uploaded recordings as cloud artifacts. |
| `crates/graphql/src/api/ai_tests.rs` | C | 3 | 0 | 3 | Warp cloud GraphQL schema crate; absent from fork. api/ai.rs: `use crate::queries::get_conversation_usage::{TokenUsage, ToolUsageMetadata, convert_token_usage}; use crate::schema;`. |
| `crates/http_client/src/iap_tests.rs` | C | 3 | 0 | 3 | Google Identity-Aware Proxy support for the Warp enterprise cloud endpoint; crates/http_client/src/iap.rs is absent from the fork (the rest of the crate is retained). |
| `crates/http_client/src/lib_tests.rs` | A | 3 | 0 | 3 | crates/http_client/src/lib.rs is present in the fork. |
| `crates/languages/src/lib_tests.rs` | A | 6 | 3 | 3 | Fork ships the source (crates/languages/src/lib.rs); 3 of 6 test-function names have no fork equivalent. |
| `crates/onboarding/src/slides/offer_slide_tests.rs` | C | 3 | 0 | 3 | Paid-plan upsell slide. offer_slide.rs: `use super::upgrade_auth_prompt::render_upgrade_auth_prompt_bar;` - both files absent from the fork. |
| `crates/repo_metadata/src/repositories_tests.rs` | A | 4 | 1 | 3 | Fork ships the source (crates/repo_metadata/src/repositories.rs); 3 of 4 test-function names have no fork equivalent. |
| `crates/repo_metadata/src/watcher_tests.rs` | A | 10 | 7 | 3 | Fork ships the source (crates/repo_metadata/src/watcher.rs); 3 of 10 test-function names have no fork equivalent. |
| `crates/warp_cli/src/agent_tests.rs` | A | 3 | 0 | 3 | crates/warp_cli/src/agent.rs present in fork; 3 of 3 file-local tests missing (fork trimmed agent_tests.rs entirely). |
| `crates/warp_core/src/channel/state_tests.rs` | A | 3 | 0 | 3 | Fork ships the source (crates/warp_core/src/channel/state.rs); 3 of 3 test-function names have no fork equivalent. |
| `crates/warp_server_auth/src/user/persistence_tests.rs` | C | 3 | 0 | 3 | Cloud account/auth crate; absent from fork. user/persistence.rs: `use warp_graphql::scalars::time::ServerTimestamp;` + `super::{AnonymousUserType, FirebaseAuthTokens, PersonalObjectLimits, UserMetadata}`. |
| `crates/warp_server_client/src/auth/session_tests.rs` | C | 3 | 0 | 3 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `crates/warp_server_client/src/network_logging_tests.rs` | C | 3 | 0 | 3 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `crates/warpui_extras/src/secure_storage/unavailable_tests.rs` | D | 3 | 0 | 3 | Non-cloud. unavailable.rs is a 23-line no-keyring fallback (`use super::Error;`). Missing source: crates/warpui_extras/src/secure_storage/unavailable.rs. |
| `app/src/code_review/git_repo_model/local_tests.rs` | D | 2 | 0 | 2 | Non-cloud. git_repo_model/local.rs: `use repo_metadata::repository::{RepositorySubscriber, SubscriberId}; use crate::code_review::diff_state::diff_metadata_against_head; use crate::util::git::{detect_current_branch_display, detect_main_branch};`. Missing source: app/src/code_review/git_repo_model/local.rs. |
| `app/src/local_control/handlers/app_state_tests.rs` | D | 2 | 0 | 2 | Same feature gap. app_state.rs: `use ::local_control::protocol::{...}; use crate::local_control::LocalControlBridge;`. Missing source: app/src/local_control/handlers/app_state.rs. |
| `app/src/local_control/handlers/layout_tests.rs` | D | 2 | 0 | 2 | Same feature gap. layout.rs: `use ::local_control::protocol::{TabCreateParams, TabType, TargetSelector};`. Missing source: app/src/local_control/handlers/layout.rs. |
| `app/src/pane_group/tree_tests.rs` | A | 20 | 18 | 2 | Fork ships the source (app/src/pane_group/tree.rs); 2 of 20 test-function names have no fork equivalent. |
| `app/src/search/files/model_tests.rs` | A | 31 | 29 | 2 | Fork ships the source (app/src/search/files/model.rs); 2 of 31 test-function names have no fork equivalent. |
| `app/src/server/server_api/auth_tests.rs` | C | 2 | 0 | 2 | Cloud auth client re-export: `pub use warp_server_client::auth::MockAuthClient;`. |
| `app/src/settings_view/code_page_tests.rs` | A | 2 | 0 | 2 | app/src/settings_view/code_page.rs is present in the fork; 2 of 2 assertions absent. |
| `app/src/settings_view/teams_page.rs` | C | 2 | 0 | 2 | Cloud teams. teams_page.rs: `use warp_graphql::object_permissions::OwnerType`-adjacent team plumbing, `use super::transfer_ownership_confirmation_modal::{...}`, `use crate::ai::AIRequestUsageModel;`, `use warp_core::features::FeatureFlag;`. |
| `app/src/util/bindings_tests.rs` | A | 4 | 2 | 2 | Fork ships the source (app/src/util/bindings.rs); 2 of 4 test-function names have no fork equivalent. |
| `app/src/util/file/external_editor/mac_tests.rs` | A | 2 | 0 | 2 | Fork ships the source (app/src/util/file/external_editor/mac.rs); 2 of 2 test-function names have no fork equivalent. |
| `crates/cloud_object_models/src/scheduled_ambient_agent_tests.rs` | C | 2 | 0 | 2 | Cloud object model crate; absent from fork. cloud_environment.rs: `use cloud_objects::cloud_object::{...}; use cloud_objects::ids::GenericStringObjectId;`. |
| `crates/editor/src/content/find_tests.rs` | A | 12 | 10 | 2 | Fork ships the source (crates/editor/src/content/find.rs); 2 of 12 test-function names have no fork equivalent. |
| `crates/editor/src/render/model/mod_tests.rs` | A | 66 | 64 | 2 | Fork ships the source (crates/editor/src/render/model/mod.rs); 2 of 66 test-function names have no fork equivalent. |
| `crates/http_client/src/lib.rs` | A | 2 | 0 | 2 | Inline tests in a retained source file (crates/http_client/src/lib.rs). |
| `crates/remote_server/src/repo_metadata_proto_tests.rs` | A | 2 | 0 | 2 | Fork ships the source (crates/remote_server/src/repo_metadata_proto.rs); 2 of 2 test-function names have no fork equivalent. |
| `crates/warp_errors/src/errors_tests.rs` | D | 2 | 0 | 2 | Crate absent from fork; `report_error` was folded into crates/warp_core/src/errors.rs, but the once-per-run / log-mode behaviour these 2 tests cover has no fork equivalent by name. Missing source: crates/warp_errors/src/registration.rs. |
| `crates/warp_server_auth/src/user_tests.rs` | C | 2 | 0 | 2 | Cloud account/auth crate; absent from fork. user/persistence.rs: `use warp_graphql::scalars::time::ServerTimestamp;` + `super::{AnonymousUserType, FirebaseAuthTokens, PersonalObjectLimits, UserMetadata}`. |
| `crates/warpui/src/windowing/winit/text_layout_tests.rs` | A | 9 | 7 | 2 | Fork retains the module; 2 of 9 names have no fork equivalent. |
| `crates/warpui_core/src/core/transfer_view_tests.rs` | A | 14 | 12 | 2 | Fork retains the module; 2 of 14 names have no fork equivalent. |
| `crates/warpui_core/tests/tui_integration.rs` | A | 2 | 0 | 2 | Integration test file exists at both revisions in spirit; 2 assertions have no fork equivalent by name and the crate is fully retained. |
| `app/src/auth/login_slide_tests.rs` | C | 1 | 0 | 1 | Cloud account login onboarding slide. |
| `app/src/auth/mod_tests.rs` | C | 1 | 0 | 1 | Cloud account auth module. |
| `app/src/bin/generate_settings_schema_tests.rs` | A | 1 | 0 | 1 | Fork ships the source (app/src/bin/generate_settings_schema.rs); 1 of 1 test-function names have no fork equivalent. |
| `app/src/code/file_tree/view/editing_tests.rs` | A | 3 | 2 | 1 | Fork ships the source (app/src/code/file_tree/view/editing.rs); 1 of 3 test-function names have no fork equivalent. |
| `app/src/completer/test.rs` | A | 11 | 10 | 1 | Fork ships the source (app/src/completer/test.rs); 1 of 11 test-function names have no fork equivalent. |
| `app/src/context_chips/current_prompt_tests.rs` | A | 11 | 10 | 1 | Fork ships the source (app/src/context_chips/current_prompt.rs); 1 of 11 test-function names have no fork equivalent. |
| `app/src/context_chips/logging_tests.rs` | A | 4 | 3 | 1 | Fork ships the source (app/src/context_chips/logging.rs); 1 of 4 test-function names have no fork equivalent. |
| `app/src/drive/index_tests.rs` | C | 4 | 3 | 1 | Warp Drive (cloud) object index. |
| `app/src/drive/sharing/qr_code_tests.rs` | C | 1 | 0 | 1 | Warp Drive (cloud) object sharing. |
| `app/src/local_control/handlers/metadata_tests.rs` | D | 1 | 0 | 1 | Same feature gap. Missing source: app/src/local_control/handlers/metadata.rs. |
| `app/src/notebooks/context_menu_tests.rs` | A | 4 | 3 | 1 | Fork ships the source (app/src/notebooks/context_menu.rs); 1 of 4 test-function names have no fork equivalent. |
| `app/src/server/telemetry/events_tests.rs` | C | 1 | 0 | 1 | Cloud telemetry event definitions. |
| `app/src/server/telemetry/mod_tests.rs` | A | 1 | 0 | 1 | app/src/server/telemetry.rs is retained in the fork (flattened from the pin's telemetry/ dir); 1 of 1 assertion missing. |
| `app/src/settings/init_tests.rs` | A | 11 | 10 | 1 | Fork ships the source (app/src/settings/init.rs); 1 of 11 test-function names have no fork equivalent. |
| `app/src/settings_view/admin_actions_tests.rs` | C | 1 | 0 | 1 | Team-admin cloud actions; consumed by billing_and_usage_page.rs / teams_page.rs (`use super::admin_actions::AdminActions;`), both cloud-only pages absent from the fork. |
| `app/src/settings_view/custom_router_view_tests.rs` | C | 1 | 0 | 1 | Cloud model-router configuration surface; page absent from fork alongside the rest of the cloud settings pages. |
| `app/src/settings_view/platform/create_api_key_modal_tests.rs` | C | 1 | 0 | 1 | Cloud platform API-key creation modal; child of platform_page.rs. |
| `app/src/tab_configs/session_config_tests.rs` | A | 38 | 37 | 1 | Fork ships the source (app/src/tab_configs/session_config.rs); 1 of 38 test-function names have no fork equivalent. |
| `app/src/util/link_detection_tests.rs` | A | 14 | 13 | 1 | Fork ships the source (app/src/util/link_detection.rs); 1 of 14 test-function names have no fork equivalent. |
| `app/src/util/time_format_tests.rs` | A | 4 | 3 | 1 | Fork ships the source (app/src/util/time_format.rs); 1 of 4 test-function names have no fork equivalent. |
| `app/src/workspace/cli_install_tests.rs` | A | 1 | 0 | 1 | Fork ships the source (app/src/workspace/cli_install.rs); 1 of 1 test-function names have no fork equivalent. |
| `app/src/workspaces/update_manager_tests.rs` | C | 1 | 0 | 1 | Cloud team object update manager (`TeamUpdateManager`); source absent from fork with the rest of app/src/server/cloud_objects/. |
| `crates/input_classifier/src/heuristic_classifier/mod_tests.rs` | A | 2 | 1 | 1 | Fork ships the source (crates/input_classifier/src/heuristic_classifier/mod.rs); 1 of 2 test-function names have no fork equivalent. |
| `crates/input_classifier/src/onnx/mod_tests.rs` | A | 1 | 0 | 1 | Fork ships the source (crates/input_classifier/src/onnx/mod.rs); 1 of 1 test-function names have no fork equivalent. |
| `crates/managed_secrets/src/secret_value_tests.rs` | A | 22 | 21 | 1 | Fork ships the source (crates/managed_secrets/src/secret_value.rs); 1 of 22 test-function names have no fork equivalent. |
| `crates/remote_server/src/protocol_tests.rs` | A | 14 | 13 | 1 | Fork ships the source (crates/remote_server/src/protocol.rs); 1 of 14 test-function names have no fork equivalent. |
| `crates/repo_metadata/src/remote_model_tests.rs` | A | 1 | 0 | 1 | Fork ships the source (crates/repo_metadata/src/remote_model.rs); 1 of 1 test-function names have no fork equivalent. |
| `crates/warp_completer/src/parsers/simple/parser_tests.rs` | A | 9 | 8 | 1 | Fork ships the source (crates/warp_completer/src/parsers/simple/parser.rs); 1 of 9 test-function names have no fork equivalent. |
| `crates/warp_features/src/features_tests.rs` | A | 2 | 1 | 1 | Fork retains the module; 1 of 2 names have no fork equivalent. |
| `crates/warp_server_client/src/auth/mod_tests.rs` | C | 1 | 0 | 1 | Cloud backend client; crate absent from fork. base_client.rs: `use warp_server_auth::auth_state::AuthState; use warp_server_auth::credentials::AuthToken;`; graphql_helpers.rs: `use warp_graphql::client::{GraphQLError, Operation};`; iap.rs: `use warp_core::channel::IapConfig` (Google Identity-Aware Proxy, enterprise cloud). |
| `crates/warpui/src/browser_tests.rs` | A | 5 | 4 | 1 | Fork ships the source (crates/warpui/src/browser.rs); 1 of 5 test-function names have no fork equivalent. |
| `crates/warpui_core/src/app_focus_telemetry_tests.rs` | D | 1 | 0 | 1 | Non-cloud focus-duration accounting inside warpui_core. Missing source: crates/warpui_core/src/app_focus_telemetry.rs. |
| `crates/warpui_core/src/elements/tui/viewported_list_tests.rs` | A | 32 | 31 | 1 | Fork ships the source (crates/warpui_core/src/elements/tui/viewported_list.rs); 1 of 32 test-function names have no fork equivalent. |
| `crates/warpui_core/src/text/mod_tests.rs` | A | 5 | 4 | 1 | Fork ships the source (crates/warpui_core/src/text/mod.rs); 1 of 5 test-function names have no fork equivalent. |
| `crates/warpui_core/src/text/word_boundaries_tests.rs` | A | 3 | 2 | 1 | Fork ships the source (crates/warpui_core/src/text/word_boundaries.rs); 1 of 3 test-function names have no fork equivalent. |
| `crates/warpui_extras/src/secure_storage/linux_tests.rs` | A | 5 | 4 | 1 | Fork ships the source (crates/warpui_extras/src/secure_storage/linux.rs); 1 of 5 test-function names have no fork equivalent. |

### Files fully covered (verdict B)

295 files, 2778 tests - every test-function name is present in the fork.
Listed for completeness; these are the renames and flattenings that the old path-matching
count reported as missing.

<details>
<summary>Expand</summary>

| path | verdict | pin | fork | missing |
|---|---|---:|---:|---:|
| `app/src/ai_assistant/transcript_tests.rs` | B | 3 | 3 | 0 |
| `app/src/ai_assistant/utils_tests.rs` | B | 8 | 8 | 0 |
| `app/src/app_id_tests.rs` | B | 2 | 2 | 0 |
| `app/src/app_state_tests.rs` | B | 3 | 3 | 0 |
| `app/src/autoupdate/linux_tests.rs` | B | 1 | 1 | 0 |
| `app/src/autoupdate/mod_tests.rs` | B | 11 | 11 | 0 |
| `app/src/autoupdate/windows_tests.rs` | B | 9 | 9 | 0 |
| `app/src/cloud_object/model/actions_tests.rs` | B | 6 | 6 | 0 |
| `app/src/code/editor/comment_editor_tests.rs` | B | 2 | 2 | 0 |
| `app/src/code/editor/diff_tests.rs` | B | 8 | 8 | 0 |
| `app/src/code/editor/element_tests.rs` | B | 6 | 6 | 0 |
| `app/src/code/editor/embedded_comment_tests.rs` | B | 8 | 8 | 0 |
| `app/src/code/editor/view/view_tests.rs` | B | 1 | 1 | 0 |
| `app/src/code/file_tree/snapshot_tests.rs` | B | 24 | 24 | 0 |
| `app/src/code/global_buffer_model_tests.rs` | B | 4 | 4 | 0 |
| `app/src/code_review/comments/batch_tests.rs` | B | 4 | 4 | 0 |
| `app/src/code_review/comments/diff_hunk_parser_tests.rs` | B | 11 | 11 | 0 |
| `app/src/code_review/find_model_tests.rs` | B | 4 | 4 | 0 |
| `app/src/code_review/hidden_lines_tests.rs` | B | 11 | 11 | 0 |
| `app/src/context_chips/builtins_tests.rs` | B | 3 | 3 | 0 |
| `app/src/context_chips/directory_fetcher_tests.rs` | B | 3 | 3 | 0 |
| `app/src/context_chips/display_chip_tests.rs` | B | 51 | 51 | 0 |
| `app/src/context_chips/display_menu_tests.rs` | B | 4 | 4 | 0 |
| `app/src/context_chips/git_branch_on_click_tests.rs` | B | 8 | 8 | 0 |
| `app/src/context_chips/mod_tests.rs` | B | 3 | 3 | 0 |
| `app/src/context_chips/prompt_tests.rs` | B | 3 | 3 | 0 |
| `app/src/context_chips/renderer_tests.rs` | B | 2 | 2 | 0 |
| `app/src/drive/export_tests.rs` | B | 8 | 8 | 0 |
| `app/src/drive/import/import_tests.rs` | B | 1 | 1 | 0 |
| `app/src/drive/import/node_tests.rs` | B | 2 | 2 | 0 |
| `app/src/drive/panel_tests.rs` | B | 1 | 1 | 0 |
| `app/src/drive/workflows/arguments_tests.rs` | B | 3 | 3 | 0 |
| `app/src/drive/workflows/modal_tests.rs` | B | 7 | 7 | 0 |
| `app/src/editor/soft_wrap_tests.rs` | B | 5 | 5 | 0 |
| `app/src/editor/view/element_tests.rs` | B | 3 | 3 | 0 |
| `app/src/editor/view/figma_utils/is_figma_png_tests.rs` | B | 9 | 9 | 0 |
| `app/src/editor/view/marked_text_tests.rs` | B | 5 | 5 | 0 |
| `app/src/editor/view/mod_tests.rs` | B | 74 | 74 | 0 |
| `app/src/editor/view/model/buffer/deferred_ops_tests.rs` | B | 2 | 2 | 0 |
| `app/src/editor/view/model/buffer/mod_tests.rs` | B | 60 | 60 | 0 |
| `app/src/editor/view/model/buffer/subword_boundaries_tests.rs` | B | 3 | 3 | 0 |
| `app/src/editor/view/model/buffer/text_tests.rs` | B | 2 | 2 | 0 |
| `app/src/editor/view/model/buffer/undo_tests.rs` | B | 11 | 11 | 0 |
| `app/src/editor/view/model/display_map/fold_map_tests.rs` | B | 6 | 6 | 0 |
| `app/src/editor/view/model/display_map/mod_tests.rs` | B | 4 | 4 | 0 |
| `app/src/editor/view/model/mod_tests.rs` | B | 14 | 14 | 0 |
| `app/src/editor/view/vim_handler_tests.rs` | B | 125 | 125 | 0 |
| `app/src/env_vars/view/env_var_collection_tests.rs` | B | 3 | 3 | 0 |
| `app/src/experiments/mod_tests.rs` | B | 5 | 5 | 0 |
| `app/src/experiments/validation_tests.rs` | B | 5 | 5 | 0 |
| `app/src/input_suggestions_tests.rs` | B | 10 | 10 | 0 |
| `app/src/keyboard_tests.rs` | B | 7 | 7 | 0 |
| `app/src/login_item/windows_tests.rs` | B | 5 | 5 | 0 |
| `app/src/menu_tests.rs` | B | 6 | 6 | 0 |
| `app/src/notebooks/manager_tests.rs` | B | 1 | 1 | 0 |
| `app/src/notebooks/notebook/details_bar_tests.rs` | B | 1 | 1 | 0 |
| `app/src/pane_group/pane/view/header/mod_tests.rs` | B | 2 | 2 | 0 |
| `app/src/prefix_tests.rs` | B | 6 | 6 | 0 |
| `app/src/preview_config_migration_tests.rs` | B | 6 | 6 | 0 |
| `app/src/remote_server/ripgrep_search_tests.rs` | B | 2 | 2 | 0 |
| `app/src/remote_server/ssh_transport_tests.rs` | B | 1 | 1 | 0 |
| `app/src/safe_triangle_tests.rs` | B | 10 | 10 | 0 |
| `app/src/search/ai_context_menu/blocks/data_source_tests.rs` | B | 5 | 5 | 0 |
| `app/src/search/ai_context_menu/code/data_source_tests.rs` | B | 12 | 12 | 0 |
| `app/src/search/ai_context_menu/diffset/search_item_tests.rs` | B | 1 | 1 | 0 |
| `app/src/search/ai_context_menu/files/data_source_tests.rs` | B | 43 | 43 | 0 |
| `app/src/search/ai_context_menu/notebooks/data_source_tests.rs` | B | 5 | 5 | 0 |
| `app/src/search/ai_context_menu/rules/data_source_tests.rs` | B | 2 | 2 | 0 |
| `app/src/search/command_palette/conversations/search_tests.rs` | B | 1 | 1 | 0 |
| `app/src/search/command_palette/data_sources_tests.rs` | B | 4 | 4 | 0 |
| `app/src/search/command_palette/files/data_source_tests.rs` | B | 1 | 1 | 0 |
| `app/src/search/command_palette/navigation/search_tests.rs` | B | 8 | 8 | 0 |
| `app/src/search/command_palette/selected_items_tests.rs` | B | 2 | 2 | 0 |
| `app/src/search/command_search/searcher_tests.rs` | B | 10 | 10 | 0 |
| `app/src/search/command_search/view_tests.rs` | B | 1 | 1 | 0 |
| `app/src/search/notebook_embedding/workflows/workflows_data_source_tests.rs` | B | 2 | 2 | 0 |
| `app/src/search/slash_command_menu/fuzzy_match_tests.rs` | B | 6 | 6 | 0 |
| `app/src/search/workflows/fuzzy_match_tests.rs` | B | 4 | 4 | 0 |
| `app/src/server/experiments/model_tests.rs` | B | 2 | 2 | 0 |
| `app/src/server/ids_tests.rs` | B | 3 | 3 | 0 |
| `app/src/settings/import/alacritty_parser_tests.rs` | B | 7 | 7 | 0 |
| `app/src/settings/import/iterm_parser_tests.rs` | B | 18 | 18 | 0 |
| `app/src/settings/schema_validation_tests.rs` | B | 1 | 1 | 0 |
| `app/src/settings_view/directory_color_add_picker_tests.rs` | B | 7 | 7 | 0 |
| `app/src/settings_view/mcp_servers/edit_page_tests.rs` | B | 5 | 5 | 0 |
| `app/src/settings_view/mcp_servers_page_tests.rs` | B | 2 | 2 | 0 |
| `app/src/settings_view/settings_file_footer_tests.rs` | B | 5 | 5 | 0 |
| `app/src/system/info_tests.rs` | B | 1 | 1 | 0 |
| `app/src/tab_configs/params_modal_tests.rs` | B | 3 | 3 | 0 |
| `app/src/tab_configs/tab_config_tests.rs` | B | 24 | 24 | 0 |
| `app/src/themes/theme_creator_tests.rs` | B | 3 | 3 | 0 |
| `app/src/tips/tip_view_tests.rs` | B | 1 | 1 | 0 |
| `app/src/uri/docker_tests.rs` | B | 1 | 1 | 0 |
| `app/src/user_config/mod_tests.rs` | B | 13 | 13 | 0 |
| `app/src/user_config/util_tests.rs` | B | 2 | 2 | 0 |
| `app/src/util/file/external_editor/mod_tests.rs` | B | 27 | 27 | 0 |
| `app/src/util/git_tests.rs` | B | 21 | 21 | 0 |
| `app/src/util/image_tests.rs` | B | 6 | 6 | 0 |
| `app/src/util/mod.rs` | B | 2 | 2 | 0 |
| `app/src/util/path_tests.rs` | B | 3 | 3 | 0 |
| `app/src/view_components/compact_dropdown_tests.rs` | B | 1 | 1 | 0 |
| `app/src/view_components/find_tests.rs` | B | 1 | 1 | 0 |
| `app/src/warp_managed_paths_watcher_tests.rs` | B | 4 | 4 | 0 |
| `app/src/workflows/aliases_tests.rs` | B | 3 | 3 | 0 |
| `app/src/workflows/categories_tests.rs` | B | 1 | 1 | 0 |
| `app/src/workflows/command_parser_tests.rs` | B | 11 | 11 | 0 |
| `app/src/workflows/local_workflows_tests.rs` | B | 2 | 2 | 0 |
| `app/src/workspace/action_tests.rs` | B | 7 | 7 | 0 |
| `app/src/workspace/cross_window_tab_drag_tests.rs` | B | 4 | 4 | 0 |
| `crates/channel_versions/src/channel_versions_tests.rs` | B | 5 | 5 | 0 |
| `crates/channel_versions/src/overrides_tests.rs` | B | 5 | 5 | 0 |
| `crates/cloud_object_models/src/mcp_tests.rs` | B | 4 | 4 | 0 |
| `crates/cloud_object_models/src/workflow_tests.rs` | B | 3 | 3 | 0 |
| `crates/cloud_object_persistence/src/encoded_permissions_tests.rs` | B | 2 | 2 | 0 |
| `crates/cloud_object_persistence/src/objects_tests.rs` | B | 2 | 2 | 0 |
| `crates/command/src/wsl_tests.rs` | B | 11 | 11 | 0 |
| `crates/editor/src/content/anchor_tests.rs` | B | 8 | 8 | 0 |
| `crates/editor/src/content/buffer_tests.rs` | B | 193 | 193 | 0 |
| `crates/editor/src/content/core_tests.rs` | B | 1 | 1 | 0 |
| `crates/editor/src/content/cursor_tests.rs` | B | 7 | 7 | 0 |
| `crates/editor/src/content/mermaid_diagram_tests.rs` | B | 3 | 3 | 0 |
| `crates/editor/src/content/outline_tests.rs` | B | 5 | 5 | 0 |
| `crates/editor/src/content/segmentation_tests.rs` | B | 6 | 6 | 0 |
| `crates/editor/src/content/text_tests.rs` | B | 9 | 9 | 0 |
| `crates/editor/src/content/undo_tests.rs` | B | 7 | 7 | 0 |
| `crates/editor/src/content/validation_tests.rs` | B | 2 | 2 | 0 |
| `crates/editor/src/multiline_tests.rs` | B | 21 | 21 | 0 |
| `crates/editor/src/render/element/table_tests.rs` | B | 22 | 22 | 0 |
| `crates/editor/src/render/mod_tests.rs` | B | 10 | 10 | 0 |
| `crates/editor/src/render/model/char_cell_display_tests.rs` | B | 16 | 16 | 0 |
| `crates/editor/src/render/model/location_tests.rs` | B | 11 | 11 | 0 |
| `crates/editor/src/render/model/offset_map_tests.rs` | B | 3 | 3 | 0 |
| `crates/editor/src/render/model/table_offset_map_tests.rs` | B | 10 | 10 | 0 |
| `crates/editor/src/render/model/viewport_tests.rs` | B | 5 | 5 | 0 |
| `crates/editor/src/search_tests.rs` | B | 4 | 4 | 0 |
| `crates/editor/src/selection_tests.rs` | B | 17 | 17 | 0 |
| `crates/fuzzy_match/src/fuzzy_tests.rs` | B | 33 | 33 | 0 |
| `crates/handlebars/src/lib_tests.rs` | B | 6 | 6 | 0 |
| `crates/handlebars/src/parser_tests.rs` | B | 2 | 2 | 0 |
| `crates/input_classifier/src/parser_tests.rs` | B | 1 | 1 | 0 |
| `crates/input_classifier/src/util_tests.rs` | B | 16 | 16 | 0 |
| `crates/isolation_platform/src/namespace_tests.rs` | B | 5 | 5 | 0 |
| `crates/jsonrpc/src/service_tests.rs` | B | 2 | 2 | 0 |
| `crates/local_control/src/auth_tests.rs` | B | 9 | 9 | 0 |
| `crates/local_control/src/client_tests.rs` | B | 2 | 2 | 0 |
| `crates/local_control/src/discovery_tests.rs` | B | 15 | 15 | 0 |
| `crates/local_control/src/protocol_tests.rs` | B | 11 | 11 | 0 |
| `crates/local_control/src/selection_tests.rs` | B | 3 | 3 | 0 |
| `crates/managed_secrets/src/envelope_tests.rs` | B | 4 | 4 | 0 |
| `crates/managed_secrets_wasm/src/lib_tests.rs` | B | 5 | 5 | 0 |
| `crates/markdown_parser/src/html_parser_tests.rs` | B | 14 | 14 | 0 |
| `crates/markdown_parser/src/markdown_parser_tests.rs` | B | 134 | 134 | 0 |
| `crates/remote_server/src/setup/glibc_tests.rs` | B | 9 | 9 | 0 |
| `crates/repo_metadata/src/entry_tests.rs` | B | 29 | 29 | 0 |
| `crates/repo_metadata/src/file_tree_store_tests.rs` | B | 6 | 6 | 0 |
| `crates/repo_metadata/src/file_tree_update_tests.rs` | B | 15 | 15 | 0 |
| `crates/repo_metadata/src/standing_queries_tests.rs` | B | 4 | 4 | 0 |
| `crates/settings/src/macros_tests.rs` | B | 40 | 40 | 0 |
| `crates/settings/src/mod_tests.rs` | B | 11 | 11 | 0 |
| `crates/settings/src/schema_tests.rs` | B | 14 | 14 | 0 |
| `crates/settings/src/toml_path_tests.rs` | B | 6 | 6 | 0 |
| `crates/settings_value/src/lib_tests.rs` | B | 7 | 7 | 0 |
| `crates/settings_value/tests/derive_tests.rs` | B | 8 | 8 | 0 |
| `crates/simple_logger/src/lib_tests.rs` | B | 14 | 14 | 0 |
| `crates/simple_logger/src/manager_tests.rs` | B | 6 | 6 | 0 |
| `crates/string-offset/src/lib_tests.rs` | B | 5 | 5 | 0 |
| `crates/sum_tree/src/lib_tests.rs` | B | 5 | 5 | 0 |
| `crates/syntax_tree/src/queries/indent_query_tests.rs` | B | 7 | 7 | 0 |
| `crates/vim/src/matching_brackets_tests.rs` | B | 1 | 1 | 0 |
| `crates/vim/src/paragraph_iterator_tests.rs` | B | 6 | 6 | 0 |
| `crates/vim/src/text_objects/block_tests.rs` | B | 2 | 2 | 0 |
| `crates/vim/src/text_objects/paragraph_tests.rs` | B | 12 | 12 | 0 |
| `crates/vim/src/text_objects/quote_tests.rs` | B | 2 | 2 | 0 |
| `crates/vim/src/text_objects/word_tests.rs` | B | 4 | 4 | 0 |
| `crates/vim/src/word_iterator_tests.rs` | B | 9 | 9 | 0 |
| `crates/warp_cli/src/json_filter_tests.rs` | B | 8 | 8 | 0 |
| `crates/warp_cli/src/share_tests.rs` | B | 13 | 13 | 0 |
| `crates/warp_cli/src/skill_tests.rs` | B | 18 | 18 | 0 |
| `crates/warp_cli/src/task_tests.rs` | B | 9 | 9 | 0 |
| `crates/warp_completer/src/completer/describe_tests.rs` | B | 15 | 15 | 0 |
| `crates/warp_completer/src/completer/engine/path_tests.rs` | B | 18 | 18 | 0 |
| `crates/warp_completer/src/completer/engine/test.rs` | B | 13 | 13 | 0 |
| `crates/warp_completer/src/completer/matchers_tests.rs` | B | 4 | 4 | 0 |
| `crates/warp_completer/src/completer/suggest/alias_tests.rs` | B | 6 | 6 | 0 |
| `crates/warp_completer/src/completer/suggest/priority/priority_tests.rs` | B | 4 | 4 | 0 |
| `crates/warp_completer/src/completer/suggest/test.rs` | B | 64 | 64 | 0 |
| `crates/warp_completer/src/completer/tests.rs` | B | 2 | 2 | 0 |
| `crates/warp_completer/src/meta_tests.rs` | B | 1 | 1 | 0 |
| `crates/warp_completer/src/parsers/simple/lexer_tests.rs` | B | 7 | 7 | 0 |
| `crates/warp_completer/src/parsers/test.rs` | B | 8 | 8 | 0 |
| `crates/warp_completer/src/signatures/legacy/registry_tests.rs` | B | 12 | 12 | 0 |
| `crates/warp_completer/src/signatures/v2/lookup_tests.rs` | B | 15 | 15 | 0 |
| `crates/warp_completer/src/signatures/v2/registry_tests.rs` | B | 1 | 1 | 0 |
| `crates/warp_completer/src/signatures/v2/signatures_tests.rs` | B | 2 | 2 | 0 |
| `crates/warp_core/src/app_id_tests.rs` | B | 2 | 2 | 0 |
| `crates/warp_core/src/interval_timer_tests.rs` | B | 1 | 1 | 0 |
| `crates/warp_core/src/semantic_selection/mod_tests.rs` | B | 6 | 6 | 0 |
| `crates/warp_core/src/sync_queue_tests.rs` | B | 8 | 8 | 0 |
| `crates/warp_core/src/ui/color/color_tests.rs` | B | 3 | 3 | 0 |
| `crates/warp_core/src/ui/color/contrast_tests.rs` | B | 6 | 6 | 0 |
| `crates/warp_core/src/ui/theme/theme_tests.rs` | B | 11 | 11 | 0 |
| `crates/warp_files/src/lib_tests.rs` | B | 5 | 5 | 0 |
| `crates/warp_files/src/text_file_reader_tests.rs` | B | 25 | 25 | 0 |
| `crates/warp_logging/src/native_tests.rs` | B | 17 | 17 | 0 |
| `crates/warp_logging/src/rotation_tests.rs` | B | 7 | 7 | 0 |
| `crates/warp_search_core/src/inline_menu_tests.rs` | B | 9 | 9 | 0 |
| `crates/warp_search_core/src/mixer_tests.rs` | B | 6 | 6 | 0 |
| `crates/warp_search_core/src/searcher_tests.rs` | B | 5 | 5 | 0 |
| `crates/warp_util/src/content_version_tests.rs` | B | 6 | 6 | 0 |
| `crates/warp_util/src/file_type_tests.rs` | B | 6 | 6 | 0 |
| `crates/warp_util/src/local_or_remote_path_tests.rs` | B | 8 | 8 | 0 |
| `crates/warp_util/src/on_cancel_tests.rs` | B | 2 | 2 | 0 |
| `crates/warp_util/src/path_tests.rs` | B | 26 | 26 | 0 |
| `crates/warp_util/src/standardized_path_tests.rs` | B | 23 | 23 | 0 |
| `crates/warp_util/src/sync_tests.rs` | B | 3 | 3 | 0 |
| `crates/warp_util/src/worktree_names_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui/src/fonts/text_layout_tests.rs` | B | 20 | 20 | 0 |
| `crates/warpui/src/platform/mac/clipboard_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui/src/platform/mac/fonts_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui/src/platform/mac/menus_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui/src/platform/mac/rendering/metal/frame_capture_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui/src/platform/mac/text_layout_tests.rs` | B | 10 | 10 | 0 |
| `crates/warpui/src/platform/wasm/mobile_detection/user_agent_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui/src/platform/wasm/soft_keyboard_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui/src/rendering/wgpu/resources_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui/src/windowing/winit/event_loop/drag_drop_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui/src/windowing/winit/fonts/str_index_map_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui/src/windowing/winit/linux/clipboard_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui/src/windowing/winit/linux/cursor_theme_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui/src/windowing/winit/windows/clipboard_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui_core/src/assets/asset_cache_tests.rs` | B | 4 | 4 | 0 |
| `crates/warpui_core/src/clipboard_utils_tests.rs` | B | 10 | 10 | 0 |
| `crates/warpui_core/src/core/autotracking/autotracking_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui_core/src/core/mod_tests.rs` | B | 41 | 41 | 0 |
| `crates/warpui_core/src/core/model/context_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/core/ref_count_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/core/tui_view_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui_core/src/core/view/context_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui_core/src/elements/gui/clipped_scrollable_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/elements/gui/clipped_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/elements/gui/container_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/elements/gui/event_handler_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui_core/src/elements/gui/flex/mod_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui_core/src/elements/gui/flex/wrap_tests.rs` | B | 18 | 18 | 0 |
| `crates/warpui_core/src/elements/gui/formatted_text_element_tests.rs` | B | 10 | 10 | 0 |
| `crates/warpui_core/src/elements/gui/hoverable_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui_core/src/elements/gui/image_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui_core/src/elements/gui/list_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui_core/src/elements/gui/new_scrollable/scrollable_tests.rs` | B | 10 | 10 | 0 |
| `crates/warpui_core/src/elements/gui/new_scrollable/util_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/elements/gui/scrollable_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/elements/gui/size_constraint_switch_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui_core/src/elements/gui/stack/mod_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui_core/src/elements/gui/stack/offset_positioning_tests.rs` | B | 11 | 11 | 0 |
| `crates/warpui_core/src/elements/gui/table/mod_tests.rs` | B | 26 | 26 | 0 |
| `crates/warpui_core/src/elements/gui/text_tests.rs` | B | 9 | 9 | 0 |
| `crates/warpui_core/src/elements/gui/uniform_list_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/elements/gui/viewported_list_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui_core/src/elements/tui/animated_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/elements/tui/buffer_tests.rs` | B | 8 | 8 | 0 |
| `crates/warpui_core/src/elements/tui/child_view_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui_core/src/elements/tui/clipped_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui_core/src/elements/tui/collapsible_tests.rs` | B | 6 | 6 | 0 |
| `crates/warpui_core/src/elements/tui/constrained_box_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui_core/src/elements/tui/container_tests.rs` | B | 9 | 9 | 0 |
| `crates/warpui_core/src/elements/tui/event_handler_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui_core/src/elements/tui/flex_tests.rs` | B | 17 | 17 | 0 |
| `crates/warpui_core/src/elements/tui/geometry_tests.rs` | B | 8 | 8 | 0 |
| `crates/warpui_core/src/elements/tui/hoverable_tests.rs` | B | 7 | 7 | 0 |
| `crates/warpui_core/src/elements/tui/scene_tests.rs` | B | 9 | 9 | 0 |
| `crates/warpui_core/src/elements/tui/selectable/state_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui_core/src/elements/tui/shimmering_text_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui_core/src/elements/tui/size_constraint_switch_tests.rs` | B | 3 | 3 | 0 |
| `crates/warpui_core/src/elements/tui/stack_tests.rs` | B | 12 | 12 | 0 |
| `crates/warpui_core/src/elements/tui/text_helpers_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/elements/tui/text_tests.rs` | B | 13 | 13 | 0 |
| `crates/warpui_core/src/fonts_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/keymap/context_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/keymap/matcher_tests.rs` | B | 5 | 5 | 0 |
| `crates/warpui_core/src/keymap_tests.rs` | B | 20 | 20 | 0 |
| `crates/warpui_core/src/platform/file_picker_tests.rs` | B | 2 | 2 | 0 |
| `crates/warpui_core/src/presenter_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/rendering/texture_cache_tests.rs` | B | 4 | 4 | 0 |
| `crates/warpui_core/src/runtime/event_conversion_tests.rs` | B | 22 | 22 | 0 |
| `crates/warpui_core/src/runtime/mod_tests.rs` | B | 11 | 11 | 0 |
| `crates/warpui_core/src/runtime/renderer_tests.rs` | B | 9 | 9 | 0 |
| `crates/warpui_core/src/runtime/terminal_probe_tests.rs` | B | 10 | 10 | 0 |
| `crates/warpui_core/src/scene_tests.rs` | B | 4 | 4 | 0 |
| `crates/warpui_core/src/text_layout_tests.rs` | B | 12 | 12 | 0 |
| `crates/warpui_core/src/ui_components/components_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_core/src/util_tests.rs` | B | 4 | 4 | 0 |
| `crates/warpui_extras/src/secure_storage/windows_tests.rs` | B | 1 | 1 | 0 |
| `crates/warpui_extras/src/user_preferences/toml_backed_tests.rs` | B | 35 | 35 | 0 |
| `crates/websocket/src/proxy_tests.rs` | B | 23 | 23 | 0 |
| `crates/websocket/src/sink_map_err_tests.rs` | B | 1 | 1 | 0 |

</details>

## Defect found during this audit

`crates/local_control` is ported into the fork (14 source files) but nothing references it.
The consumers - `app/src/local_control/**` and `crates/warp_cli/src/local_control/**` - were
never ported, so the crate is dead code and the local control surface (`oz` driving a running
app instance: open files, create tabs, query app state, open settings surfaces) does not exist.
Filed as a separate issue.

## Reproducing

The extraction and classification scripts are not committed (they are one-shot analysis).
The method is: enumerate test-attribute-bearing functions per file at `02b53fcd8` and at
`origin/main`, build a global name -> file index over the fork, and diff per pin file.
Every C and D verdict was then confirmed by reading the pin **source** file's imports.

Refs #2.
