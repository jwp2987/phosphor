# SCOPE-AI — authoritative scope classification for `app/src/ai` + `crates/ai`

Oracle: **`02b53fcd8`** (Warp `2026.07.29.09.05` stable), the pin recorded in `ORACLE.md`.
Fork side: `origin/main` @ `4f33fcf9c`. Slice: every `*_tests.rs` file under `app/src/ai/`
and `crates/ai/` that exists at the pin. Refs #2.

## Method

Test functions are counted by attribute, not by filename: a function counts when it
carries `#[test]`, `#[tokio::test]`, `#[gpui::test]`, `#[test_case(..)]` or `#[rstest]`.
Running that counter over both trees reproduces `ORACLE.md` exactly —
`app/ai` 1847 vs 1229 and `crates/ai` 352 vs 142 — so the numbers below are on the same
footing as the published gap.

Coverage is then matched **by test-function name across the whole fork slice**, never by
path. A pin test counts as present if a function of that name exists anywhere under the
fork's `app/src/ai` or `crates/ai`, which is what makes the fork's `*_tests.rs` →
`*_test.rs` rename and its `a/b/c_tests.rs` → `a/b_c_tests.rs` flattening invisible to the
result. Scope calls (C and D) are made from the **pin source** file's import list, quoted
in the evidence column.

## The headline correction

The published gap for this slice is **828**. That is a *net* figure and it understates the
work:

```
Warp tests at the pin in this slice         2,199
  ... of which present in the fork by name     688
  ... genuinely absent                       1,511

Fork tests in this slice                   1,371
  ... carrying a pin test name                697
  ... fork-original (no pin counterpart)      674
      (688 vs 697 is the same overlap counted
       from each side: a few pin names are
       realised by more than one fork test)

net gap  2,199 - 1,371                        828
```

674 fork-original tests mask 1,511 absent Warp tests. **401 of those 674 sit under
`app/src/ai/agent_providers/`** — the BYOP agent loop, chat streaming, prompt rendering and
tool plumbing that Warp has no counterpart for. The rest cluster in `byop_compaction`,
`byop_readiness`, `usage_cost` and `blocklist/block/cli`. Those are real coverage, but they
do not close a single Warp test.

So: **828 is the right burndown number for "are we level with Warp"; 1,511 is the right
number for "how many tests must be written".**

## Totals

| verdict | files | Warp tests at pin | missing (file-level) | missing (per-test adjusted) |
|---|---:|---:|---:|---:|
| **A** — test debt | 62 | 1029 | 647 | 571 |
| **B** — covered | 37 | 306 | 0 | 0 |
| **C** — out of scope | 47 | 509 | 509 | 528 |
| **D** — feature gap | 38 | 355 | 355 | 412 |
| **total** | 184 | 2199 | 1511 | 1511 |

**The real target for this slice is 571 tests of straight debt (A), plus 412 that need a
feature ported first (D). 528 are legitimately out of scope (C).**

The two columns differ because four files are mixed; the per-test split is in
[Mixed files](#mixed-files) below.

## Verdicts

One row per pin test file. `pin` = Warp test functions in that file at the pin; `fork` =
how many of those names exist in the fork slice; `miss` = the rest.

| file | verdict | pin | fork | miss | evidence |
|---|:--:|---:|---:|---:|---|
| `app/src/ai/active_agent_views_model_tests.rs` | D | 10 | 0 | 10 | absent: `app/src/ai/active_agent_views_model.rs`. Imports are entirely in-tree (`blocklist`, `agent_conversations_model`, `terminal::model::session::active_session`); no cloud import. |
| `app/src/ai/agent/api/convert_conversation_tests.rs` | A | 15 | 0 | 15 | fork ships `app/src/ai/agent/api/convert_conversation.rs` |
| `app/src/ai/agent/api/convert_from_tests.rs` | A | 5 | 0 | 5 | fork ships `app/src/ai/agent/api/convert_from.rs` |
| `app/src/ai/agent/api/convert_to_tests.rs` | A | 6 | 2 | 4 | fork ships `app/src/ai/agent/api/convert_to.rs` |
| `app/src/ai/agent/api/impl_tests.rs` | C | 13 | 0 | 13 | src `agent/api/impl.rs`: `use crate::server::server_api::{AIApiError, ServerApi};` — sends the conversation to Warp's hosted LLM API. Fork's BYOP realisation is `app/src/ai/agent_providers/`. |
| `app/src/ai/agent/conversation_tests.rs` | A | 36 | 7 | 29 | fork ships `app/src/ai/agent/conversation.rs` |
| `app/src/ai/agent/conversation_yaml_tests.rs` | A | 8 | 6 | 2 | fork ships `app/src/ai/agent/conversation_yaml.rs` |
| `app/src/ai/agent/linearization_tests.rs` | B | 17 | 17 | 0 | all 17 test names present under `app/src/ai/agent/linearization.rs` |
| `app/src/ai/agent/mod_tests.rs` | A | 13 | 10 | 3 | fork ships `app/src/ai/agent/mod.rs` |
| `app/src/ai/agent/suggestions_tests.rs` | B | 1 | 1 | 0 | the 1 test name is present under `app/src/ai/agent/suggestion_test.rs` |
| `app/src/ai/agent/task_store_tests.rs` | A | 32 | 22 | 10 | fork ships `app/src/ai/agent/task_store.rs` |
| `app/src/ai/agent/task_tests.rs` | B | 18 | 18 | 0 | all 18 test names present under `app/src/ai/agent/task.rs` |
| `app/src/ai/agent/util_tests.rs` | B | 14 | 14 | 0 | all 14 test names present under `app/src/ai/agent/util.rs` |
| `app/src/ai/agent_conversations_model_tests.rs` | A | 59 | 11 | 48 | Fork ships `agent_conversations_model.rs` (1135 lines vs 2056). 2 of the 48 touch `crate::cloud_object::{Owner, Revision, ServerMetadata}` / `crate::server::ids::ServerId`; the other 46 are plain debt. |
| `app/src/ai/agent_events/driver_tests.rs` | A | 19 | 7 | 12 | fork ships `app/src/ai/agent_events/driver.rs` |
| `app/src/ai/agent_events/message_hydrator_tests.rs` | A | 5 | 1 | 4 | fork ships `app/src/ai/agent_events/message_hydrator.rs` |
| `app/src/ai/agent_management/agent_management_model_tests.rs` | D | 15 | 0 | 15 | absent: `app/src/ai/agent_management/agent_management_model.rs`. Imports: `crate::terminal::cli_agent_sessions`, `crate::workspace::{Workspace, WorkspaceRegistry}` (window/pane registry, not team workspaces). |
| `app/src/ai/agent_management/notifications/item_tests.rs` | D | 4 | 0 | 4 | absent: `app/src/ai/agent_management/notifications/item.rs`. Imports: `enum_iterator`, `instant`, `uuid`, `warpui::EntityId`, `crate::terminal::CLIAgent` — no cloud import. |
| `app/src/ai/agent_sdk/admin_tests.rs` | A | 2 | 0 | 2 | fork ships `app/src/ai/agent_sdk/admin.rs` |
| `app/src/ai/agent_sdk/agent_management_tests.rs` | C | 23 | 0 | 23 | src `agent_sdk/agent_management.rs`: `use crate::server::server_api::ServerApiProvider;` + `use crate::server::server_api::ai::{...};` |
| `app/src/ai/agent_sdk/ambient_tests.rs` | C | 10 | 0 | 10 | src `agent_sdk/ambient.rs`: `use crate::cloud_object::model::persistence::CloudModel;` + `use crate::server::ids::{ServerId, SyncId};` + `use crate::workspaces::user_workspaces::UserWorkspaces;` |
| `app/src/ai/agent_sdk/api_key_tests.rs` | C | 7 | 0 | 7 | src `agent_sdk/api_key.rs`: `use warp_graphql::mutations::expire_api_key::ExpireApiKeyResult;` + `use warp_graphql::queries::api_keys::ApiKeyProperties;` — Warp *account* API keys, not BYOP provider keys. |
| `app/src/ai/agent_sdk/artifact_tests.rs` | C | 13 | 0 | 13 | src `agent_sdk/artifact.rs`: `use crate::server::server_api::ai::{AIClient, ArtifactDownloadResponse};` |
| `app/src/ai/agent_sdk/artifact_upload_tests.rs` | C | 15 | 0 | 15 | src `agent_sdk/artifact_upload.rs`: `use crate::server::server_api::presigned_upload::upload_file_to_target;` + `use crate::server::server_api::harness_support::FileUploadBody;` |
| `app/src/ai/agent_sdk/common_tests.rs` | A | 6 | 0 | 6 | fork ships `app/src/ai/agent_sdk/common.rs` |
| `app/src/ai/agent_sdk/config_file_tests.rs` | A | 15 | 11 | 4 | fork ships `app/src/ai/agent_sdk/config_file.rs` |
| `app/src/ai/agent_sdk/driver/attachments_tests.rs` | C | 11 | 0 | 11 | src `driver/attachments.rs`: `use crate::server::server_api::ai::AIClient;` + `use crate::server::server_api::presigned_upload::HttpStatusError;` |
| `app/src/ai/agent_sdk/driver/cache_setup_tests.rs` | C | 3 | 0 | 3 | src `driver/cache_setup.rs`: `use cloud_object_models::SourceRepo;` + `use warp_isolation_platform::IsolationPlatformType;` — cloud-environment build cache. |
| `app/src/ai/agent_sdk/driver/cloud_provider/gcp_tests.rs` | C | 3 | 0 | 3 | src `driver/cloud_provider/gcp.rs`: `use warp_managed_secrets::{GcpCredentials, GcpFederationConfig};` + `use crate::ai::cloud_environments::GcpProviderConfig;` |
| `app/src/ai/agent_sdk/driver/cloud_provider_tests.rs` | C | 6 | 0 | 6 | src `driver/cloud_provider.rs`: `use crate::ai::cloud_environments::ProvidersConfig;` |
| `app/src/ai/agent_sdk/driver/environment_tests.rs` | C | 7 | 0 | 7 | src `driver/environment.rs`: `use crate::ai::cloud_environments::{CodeForge, SourceRepo};` + `use ai::index::full_source_code_embedding::manager::{...};` |
| `app/src/ai/agent_sdk/driver/error_classification_tests.rs` | C | 22 | 0 | 22 | src `driver/error_classification.rs`: `use warp_graphql::ai::{AgentTaskState, PlatformErrorCode};` + `use crate::server::server_api::ai::TaskStatusUpdate;` |
| `app/src/ai/agent_sdk/driver/git_credentials_tests.rs` | C | 8 | 0 | 8 | src `driver/git_credentials.rs`: `use crate::server::server_api::ai::{AIClient, GitCredential};` — git creds minted by warp-server for cloud runners. |
| `app/src/ai/agent_sdk/driver/harness/claude_code/wake_driver_tests.rs` | C | 1 | 0 | 1 | src `harness/claude_code/wake_driver.rs`: `use crate::server::server_api::harness_support::ResolvePromptRequest;` + `use warp_graphql::ai::AgentTaskState;` |
| `app/src/ai/agent_sdk/driver/harness/claude_code_tests.rs` | A | 34 | 17 | 17 | fork ships `app/src/ai/agent_sdk/driver/harness/claude_code.rs` |
| `app/src/ai/agent_sdk/driver/harness/claude_transcript_tests.rs` | D | 10 | 0 | 10 | absent: `agent_sdk/driver/harness/claude_transcript.rs`. Imports: `std::fs`, `serde_json`, `super::json_utils::entries_to_jsonl` — parses local Claude Code JSONL transcripts. |
| `app/src/ai/agent_sdk/driver/harness/codex_tests.rs` | D | 38 | 0 | 38 | absent: `agent_sdk/driver/harness/codex.rs`. Fork ships `harness/claude_code.rs` and `harness/gemini.rs` but no Codex harness; driving the local `codex` CLI is not a cloud feature (7 of the 38 additionally touch `server_api::harness_support` upload and would need adapting). |
| `app/src/ai/agent_sdk/driver/harness/codex_transcript_tests.rs` | D | 9 | 0 | 9 | absent: `agent_sdk/driver/harness/codex_transcript.rs`. Imports: `std::fs`, `serde_json`, `chrono`, `uuid` only. |
| `app/src/ai/agent_sdk/driver/harness/gemini_tests.rs` | B | 9 | 9 | 0 | all 9 test names present under `app/src/ai/agent_sdk/driver/harness/gemini.rs` |
| `app/src/ai/agent_sdk/driver/harness/mod_tests.rs` | A | 10 | 3 | 7 | fork ships `app/src/ai/agent_sdk/driver/harness/mod.rs` |
| `app/src/ai/agent_sdk/driver/harness_output_monitor_tests.rs` | D | 8 | 0 | 8 | absent: `agent_sdk/driver/harness_output_monitor.rs`. Imports: `regex::escape`, `crate::terminal::model::find::RegexDFAs`, `crate::terminal::cli_agent_sessions::CLIAgentSessionStatus`. |
| `app/src/ai/agent_sdk/driver/snapshot_tests.rs` | C | 35 | 0 | 35 | src `driver/snapshot.rs`: `use crate::server::server_api::ai::{...};` + `use crate::server::server_api::harness_support::{...};` — uploads workspace snapshots to warp-server. |
| `app/src/ai/agent_sdk/driver/terminal_tests.rs` | A | 1 | 0 | 1 | fork ships `app/src/ai/agent_sdk/driver/terminal.rs` |
| `app/src/ai/agent_sdk/driver_tests.rs` | A | 49 | 0 | 49 | Fork ships `agent_sdk/driver.rs` (1583 lines vs 4042 at the pin). 18 of the 49 depend on `warp_graphql::mutations::create_managed_mcp_client_config` / `server_api::managed_mcp::MockManagedMcpClient` / `warp_managed_secrets::ManagedSecretValue`; the fork has a BYOP secret-injection equivalent (`agent_providers/secrets.rs`), so these are portable with adaptation rather than out of scope. |
| `app/src/ai/agent_sdk/mcp_config_tests.rs` | A | 18 | 15 | 3 | fork ships `app/src/ai/agent_sdk/mcp_config.rs` |
| `app/src/ai/agent_sdk/mod_tests.rs` | A | 14 | 0 | 14 | fork ships `app/src/ai/agent_sdk/mod.rs` |
| `app/src/ai/agent_sdk/output_tests.rs` | B | 14 | 14 | 0 | all 14 test names present under `app/src/ai/agent_sdk/output.rs` |
| `app/src/ai/agent_sdk/retry_tests.rs` | D | 11 | 0 | 11 | absent: `agent_sdk/retry.rs`. Generic bounded-retry helper; the source file has no `use` statements at all beyond `mod tests;`. |
| `app/src/ai/agent_sdk/runner_tests.rs` | C | 6 | 0 | 6 | src `agent_sdk/runner.rs`: `use warp_graphql::mutations::upsert_runner::{...};` + `use warp_graphql::queries::get_runners::{...};` |
| `app/src/ai/ambient_agents/spawn_tests.rs` | C | 20 | 0 | 20 | src `ambient_agents/spawn.rs` (pin, 338 lines): `use crate::server::server_api::ai::{...};` + `use crate::server::retry_strategies::with_bounded_retry;`. Fork ships a 74-line type-only stub with none of the spawn/poll logic the tests drive. |
| `app/src/ai/ambient_agents/task_tests.rs` | A | 11 | 0 | 11 | fork ships `app/src/ai/ambient_agents/task.rs` |
| `app/src/ai/artifact_download_tests.rs` | A | 7 | 2 | 5 | fork ships `app/src/ai/artifact_download.rs` |
| `app/src/ai/artifacts/mod_tests.rs` | A | 11 | 1 | 10 | 5 of the 10 missing use `crate::server::server_api::ai::{ArtifactDownloadCommonFields, FileArtifactResponseData, ScreenshotArtifactResponseData}` (C); the other 5 are plain debt on the retained `artifacts/mod.rs`. |
| `app/src/ai/aws_credentials_tests.rs` | B | 5 | 5 | 0 | all 5 test names present under `app/src/ai/aws_credentials.rs` |
| `app/src/ai/block_context_tests.rs` | B | 3 | 3 | 0 | all 3 test names present under `app/src/ai/block_context.rs` |
| `app/src/ai/blocklist/action_model/execute/ask_user_question_tests.rs` | A | 7 | 5 | 2 | fork ships `app/src/ai/blocklist/action_model/execute/ask_user_question.rs` |
| `app/src/ai/blocklist/action_model/execute/call_mcp_tool_tests.rs` | A | 17 | 2 | 15 | fork ships `app/src/ai/blocklist/action_model/execute/call_mcp_tool.rs` |
| `app/src/ai/blocklist/action_model/execute/file_glob_tests.rs` | B | 3 | 3 | 0 | all 3 test names present under `app/src/ai/blocklist/action_model/execute/file_glob.rs` |
| `app/src/ai/blocklist/action_model/execute/grep_tests.rs` | A | 4 | 1 | 3 | fork ships `app/src/ai/blocklist/action_model/execute/grep.rs` |
| `app/src/ai/blocklist/action_model/execute/read_documents_tests.rs` | A | 2 | 1 | 1 | fork ships `app/src/ai/blocklist/action_model/execute/read_documents.rs` |
| `app/src/ai/blocklist/action_model/execute/read_skill_tests.rs` | A | 7 | 2 | 5 | fork ships `app/src/ai/blocklist/action_model/execute/read_skill.rs` |
| `app/src/ai/blocklist/action_model/execute/request_file_edits/diff_application_tests.rs` | A | 28 | 25 | 3 | fork ships `app/src/ai/blocklist/action_model/execute/request_file_edits/diff_application.rs` |
| `app/src/ai/blocklist/action_model/execute/request_file_edits_tests.rs` | A | 5 | 4 | 1 | fork ships `app/src/ai/blocklist/action_model/execute/request_file_edits.rs` |
| `app/src/ai/blocklist/action_model/execute/run_agents_tests.rs` | D | 16 | 0 | 16 | absent: `blocklist/action_model/execute/run_agents.rs`. Imports `crate::ai::local_harness_setup::local_harness_product_disabled_message` and `ai::agent::action::RunAgentsExecutionMode` — orchestration has a local-harness mode; the fork ships no run_agents tool at all. |
| `app/src/ai/blocklist/action_model/execute/send_message_tests.rs` | C | 2 | 0 | 2 | src `execute/send_message.rs`: `use crate::server::server_api::ai::{SendAgentMessageRequest, SendAgentMessageResponse};` + `use crate::server::server_api::ServerApiProvider;` |
| `app/src/ai/blocklist/action_model/execute/shell_command_tests.rs` | A | 1 | 0 | 1 | fork ships `app/src/ai/blocklist/action_model/execute/shell_command.rs` |
| `app/src/ai/blocklist/action_model/execute/stop_recording_tests.rs` | D | 5 | 0 | 5 | absent: `blocklist/action_model/execute/stop_recording.rs`. Imports: `ai::agent::action_result::{RecordingStopped, StopRecordingResult}`, `warpui` only. |
| `app/src/ai/blocklist/action_model/execute/upload_artifact_tests.rs` | C | 5 | 0 | 5 | src `execute/upload_artifact.rs`: `use crate::{ai::{agent_sdk::artifact_upload::{FileArtifactUploadRequest, FileArtifactUploader}, ...}, server::server_api::ServerApiProvider};` |
| `app/src/ai/blocklist/action_model/execute/wait_for_events_tests.rs` | D | 7 | 0 | 7 | absent: `blocklist/action_model/execute/wait_for_events.rs`. Imports are all in-tree orchestration types; no cloud import. |
| `app/src/ai/blocklist/action_model/execute_tests.rs` | A | 22 | 8 | 14 | fork ships `app/src/ai/blocklist/action_model/execute.rs` |
| `app/src/ai/blocklist/action_model/preprocess_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `app/src/ai/blocklist/action_model/preprocess.rs` |
| `app/src/ai/blocklist/action_model/recording_controller_tests.rs` | D | 14 | 0 | 14 | absent: `blocklist/action_model/recording_controller.rs`. Imports: `std::mem`, `instant::Instant`, `thiserror`, `ai::agent::action_result::StopRecordingResult`. |
| `app/src/ai/blocklist/action_model/recording_finalize_tests.rs` | C | 3 | 0 | 3 | src `action_model/recording_finalize.rs`: `use crate::ai::agent_sdk::artifact_upload::{FileArtifactUploadRequest, FileArtifactUploader};` + `use crate::server::server_api::ServerApiProvider;` |
| `app/src/ai/blocklist/action_model/recording_telemetry_tests.rs` | D | 4 | 0 | 4 | absent: `blocklist/action_model/recording_telemetry.rs`. Imports: `warp_core::telemetry::{EnablementState, TelemetryEvent, TelemetryEventDesc}`, `crate::features::FeatureFlag`. |
| `app/src/ai/blocklist/action_model_tests.rs` | B | 3 | 3 | 0 | all 3 test names present under `app/src/ai/blocklist/action_model.rs` |
| `app/src/ai/blocklist/agent_view/conversation_selection_tests.rs` | D | 5 | 0 | 5 | absent: `blocklist/agent_view/conversation_selection.rs`. Imports are all in-tree (`active_agent_views_model`, `agent_conversations_model`, `workspace::RestoreConversationLayout`). |
| `app/src/ai/blocklist/agent_view/orchestration_avatar_tests.rs` | D | 1 | 0 | 1 | absent: `blocklist/agent_view/orchestration_avatar.rs`. Imports: `warpui::elements::Element`, `orchestration_pill_bar`, `crate::appearance::Appearance`. |
| `app/src/ai/blocklist/agent_view/orchestration_pill_bar_model_tests.rs` | D | 3 | 0 | 3 | absent: `blocklist/agent_view/orchestration_pill_bar_model.rs`. Imports: `warpui`, `AIConversationId`, `BlocklistAIHistoryModel` only. |
| `app/src/ai/blocklist/agent_view/orchestration_pill_bar_tests.rs` | D | 4 | 0 | 4 | absent: `blocklist/agent_view/orchestration_pill_bar.rs`. Pure view code over `BlocklistAIHistoryModel` / `orchestration_topology`; no cloud import. |
| `app/src/ai/blocklist/agent_view/zero_state_block_tests.rs` | A | 15 | 11 | 4 | fork ships `app/src/ai/blocklist/agent_view/zero_state_block.rs` |
| `app/src/ai/blocklist/block/cli_controller_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `app/src/ai/blocklist/block/cli_controller.rs` |
| `app/src/ai/blocklist/block/find_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `app/src/ai/blocklist/block/find.rs` |
| `app/src/ai/blocklist/block/number_shortcut_buttons_tests.rs` | B | 8 | 8 | 0 | all 8 test names present under `app/src/ai/blocklist/block/number_shortcut_buttons.rs` |
| `app/src/ai/blocklist/block/secret_redaction_tests.rs` | B | 19 | 19 | 0 | all 19 test names present under `app/src/ai/blocklist/block/secret_redaction_test.rs`; fork adds 2 more |
| `app/src/ai/blocklist/block/view_impl/common_tests.rs` | A | 15 | 13 | 2 | fork ships `app/src/ai/blocklist/block/view_impl/common.rs` |
| `app/src/ai/blocklist/block/view_impl/orchestration_tests.rs` | D | 8 | 0 | 8 | absent: `blocklist/block/view_impl/orchestration.rs`. Pure view code; no cloud import. |
| `app/src/ai/blocklist/block/view_impl/output_tests.rs` | A | 13 | 0 | 13 | Fork ships `block/view_impl/output.rs` with no tests at all. |
| `app/src/ai/blocklist/block_tests.rs` | A | 35 | 4 | 31 | fork ships `app/src/ai/blocklist/block.rs` |
| `app/src/ai/blocklist/context_model_tests.rs` | A | 15 | 7 | 8 | fork ships `app/src/ai/blocklist/context_model.rs` |
| `app/src/ai/blocklist/controller/response_stream_tests.rs` | A | 7 | 0 | 7 | fork ships `app/src/ai/blocklist/controller/response_stream.rs` |
| `app/src/ai/blocklist/controller_tests.rs` | A | 6 | 0 | 6 | Fork ships `blocklist/controller.rs` (4193 lines, larger than the pin's 3589) with no tests at all. |
| `app/src/ai/blocklist/diff_storage_tests.rs` | B | 5 | 5 | 0 | all 5 test names present under `app/src/ai/blocklist/diff_storage.rs` |
| `app/src/ai/blocklist/handoff/pipeline_tests.rs` | C | 18 | 0 | 18 | src `blocklist/handoff/pipeline.rs`: `use crate::cloud_object::CloudObjectLookup as _;` + `use crate::server::ids::{ServerId, SyncId};` + `use crate::ai::cloud_environments::CloudAmbientAgentEnvironment;` — handing a local conversation off to a Warp cloud agent. |
| `app/src/ai/blocklist/handoff/touched_repos_tests.rs` | C | 1 | 0 | 1 | src `blocklist/handoff/touched_repos.rs`: `use crate::cloud_object::CloudObjectLookup as _;` + `use crate::server::ids::SyncId;` + `use crate::ai::cloud_environments::{...};` |
| `app/src/ai/blocklist/history_model_tests.rs` | A | 71 | 32 | 39 | fork ships `app/src/ai/blocklist/history_model.rs` |
| `app/src/ai/blocklist/inline_action/ask_user_question_view_tests.rs` | A | 1 | 0 | 1 | fork ships `app/src/ai/blocklist/inline_action/ask_user_question_view.rs` |
| `app/src/ai/blocklist/inline_action/create_environment_modal_tests.rs` | C | 1 | 0 | 1 | src `inline_action/create_environment_modal.rs`: `use crate::settings_view::handoff_environment_creation_modal::{...};` — creates a Warp *cloud* ambient-agent environment. |
| `app/src/ai/blocklist/inline_action/host_picker_tests.rs` | D | 11 | 0 | 11 | absent: `blocklist/inline_action/host_picker.rs`. Imports: `warpui::elements`, `crate::editor`, `crate::menu`, `crate::view_components::dropdown` — plain UI. |
| `app/src/ai/blocklist/inline_action/malformed_line_heuristics_tests.rs` | B | 4 | 4 | 0 | all 4 test names present under `app/src/ai/blocklist/inline_action/malformed_line_heuristics_test.rs` |
| `app/src/ai/blocklist/inline_action/orchestration_controls_tests.rs` | D | 1 | 0 | 1 | absent: `blocklist/inline_action/orchestration_controls.rs`. Orchestration UI over `harness_availability` + `execution_profiles::model_menu_items`; the one cloud touch is `crate::server::experiments::ServerExperiments`. |
| `app/src/ai/blocklist/inline_action/requested_command_attribution_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `app/src/ai/blocklist/inline_action/requested_command_attribution_test.rs` |
| `app/src/ai/blocklist/inline_action/requested_command_tests.rs` | B | 12 | 12 | 0 | all 12 test names present under `app/src/ai/blocklist/inline_action/requested_command_test.rs` |
| `app/src/ai/blocklist/inline_action/run_agents_card_view_tests.rs` | D | 32 | 0 | 32 | absent: `blocklist/inline_action/run_agents_card_view.rs`. The run_agents orchestration card; cloud host selection (`warp_graphql::queries::get_runners`, `connected_self_hosted_workers`) is one branch of an otherwise local view. |
| `app/src/ai/blocklist/input_model_tests.rs` | A | 14 | 0 | 14 | fork ships `app/src/ai/blocklist/input_model.rs` |
| `app/src/ai/blocklist/local_agent_task_sync_model_tests.rs` | C | 36 | 0 | 36 | src `blocklist/local_agent_task_sync_model.rs`: `use warp_graphql::ai::{AgentTaskState, PlatformErrorCode};` + `use crate::server::server_api::ai::{AIClient, TaskStatusUpdate};` — pushes local agent task state up to warp-server. |
| `app/src/ai/blocklist/orchestration_event_streamer_tests.rs` | C | 55 | 0 | 55 | src `blocklist/orchestration_event_streamer.rs`: `use crate::server::server_api::ai::{AIClient, AgentRunEvent, TaskListFilter};` + `use crate::server::retry_strategies::is_transient_http_error;`. The test file itself is built on `use crate::server::server_api::ai::MockAIClient;` — every test drives the cloud SSE run-event stream. |
| `app/src/ai/blocklist/orchestration_events_tests.rs` | D | 10 | 0 | 10 | absent: `blocklist/orchestration_events.rs`. Imports: `warp_multi_agent_api as api`, `warpui`, `super::history_model` — no `server::`, no `cloud_object`. |
| `app/src/ai/blocklist/orchestration_topology_tests.rs` | D | 26 | 0 | 26 | absent: `blocklist/orchestration_topology.rs`. Whole import list: `std::collections::HashSet`, `crate::ai::agent::conversation::{AIConversation, AIConversationId, ConversationStatus}`, `crate::ai::blocklist::BlocklistAIHistoryModel`. Pure in-memory parent/child graph. |
| `app/src/ai/blocklist/permissions_tests.rs` | A | 25 | 22 | 3 | fork ships `app/src/ai/blocklist/permissions.rs` |
| `app/src/ai/blocklist/persistence_tests.rs` | B | 1 | 1 | 0 | the 1 test name is present under `app/src/ai/blocklist/persistence_test.rs` |
| `app/src/ai/blocklist/queued_query_tests.rs` | A | 42 | 39 | 3 | fork ships `app/src/ai/blocklist/queued_query.rs` |
| `app/src/ai/blocklist/usage/conversation_usage_view_tests.rs` | A | 3 | 0 | 3 | fork ships `app/src/ai/blocklist/usage/conversation_usage_view.rs` |
| `app/src/ai/blocklist/usage/mod_tests.rs` | A | 4 | 0 | 4 | fork ships `app/src/ai/blocklist/usage/mod.rs` |
| `app/src/ai/blocklist/usage/rollup_tests.rs` | D | 8 | 0 | 8 | absent: `blocklist/usage/rollup.rs`. Whole import list: `AIConversation`, `AIConversationId`, `BlocklistAIHistoryModel`, `orchestration_topology::descendant_conversation_ids_in_spawn_order`. |
| `app/src/ai/cloud_environments/catalog_tests.rs` | C | 3 | 0 | 3 | src `cloud_environments/catalog.rs`: `use crate::cloud_object::model::persistence::{CloudModel, CloudModelEvent};` + `use crate::server::cloud_objects::update_manager::UpdateManager;` + `use crate::server::ids::SyncId;` |
| `app/src/ai/codebase_auto_indexing_tests.rs` | C | 3 | 0 | 3 | Gate for `full_source_code_embedding`, whose only non-mock backend is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); enablement reads `UserWorkspaces::is_codebase_context_enabled` (workspace/team policy). |
| `app/src/ai/connected_self_hosted_workers_tests.rs` | C | 4 | 0 | 4 | src `connected_self_hosted_workers.rs`: `use crate::server::server_api::ai::ConnectedSelfHostedWorker;` + `use crate::server::server_api::ServerApiProvider;` + `use crate::auth::auth_manager::{AuthManager, AuthManagerEvent};` |
| `app/src/ai/conversation_details_panel_tests.rs` | C | 8 | 0 | 8 | src `conversation_details_panel.rs`: `use crate::cloud_object::CloudObjectLookup as _;` + `use crate::server::ids::{ServerId, SyncId};` + `use crate::server::server_api::ai::AmbientAgentTask;` + `use crate::workspaces::user_profiles::{UserProfileWithUID, UserProfiles};` |
| `app/src/ai/conversation_export_tests.rs` | B | 4 | 4 | 0 | all 4 test names present under `app/src/ai/conversation_export_test.rs` |
| `app/src/ai/document/ai_document_model_tests.rs` | A | 15 | 12 | 3 | fork ships `app/src/ai/document/ai_document_model.rs` |
| `app/src/ai/execution_profiles/config_tests.rs` | D | 3 | 0 | 3 | absent: `execution_profiles/config.rs`. Serde config types; the fork already ships `execution_profiles/profiles.rs` which itself uses `crate::server::ids::SyncId`, so that import is not a scope boundary. |
| `app/src/ai/execution_profiles/editor/mod_tests.rs` | A | 18 | 8 | 10 | fork ships `app/src/ai/execution_profiles/editor/mod.rs` |
| `app/src/ai/execution_profiles/profiles_tests.rs` | A | 25 | 2 | 23 | fork ships `app/src/ai/execution_profiles/profiles.rs` |
| `app/src/ai/geap_credentials_tests.rs` | C | 21 | 0 | 21 | src `geap_credentials.rs`: `use warp_managed_secrets::client::{IdentityTokenOptions, TaskIdentityToken};` + `use crate::auth::AuthStateProvider;` + `use crate::workspaces::user_workspaces::{UserWorkspaces, UserWorkspacesEvent};` — GEAP tokens are minted per workspace by Warp's managed-secret service. |
| `app/src/ai/get_relevant_files/remote_search/native_tests.rs` | C | 1 | 0 | 1 | src `get_relevant_files/remote_search/native.rs`: `use ::ai::index::full_source_code_embedding::store_client::StoreClient;` + `use crate::server::server_api::{ServerApi, ServerApiProvider};` |
| `app/src/ai/llms_tests.rs` | A | 27 | 3 | 24 | Fork ships `llms.rs` (1345 lines vs 2008). ~12 of the 24 target Warp's BYOK custom-endpoint surface (`custom_llm_infos`, alias display, endpoint purge) — the fork has 0 hits for `custom_endpoints`/`CustomEndpoint`, so those are effectively D within a shipped file. |
| `app/src/ai/local_harness_setup_tests.rs` | D | 5 | 0 | 5 | absent: `local_harness_setup.rs`. Whole import list: `warp_cli::agent::Harness`, `crate::features::FeatureFlag`, `crate::util::path::resolve_executable` — detects locally installed agent CLIs. |
| `app/src/ai/mcp/builtin_tests.rs` | C | 6 | 0 | 6 | src `mcp/builtin.rs` doc comment: "Built-in **Warp-hosted** MCP servers ... authenticated with the user's existing session credentials (warp-server accepts both session ID tokens and API keys)"; `use crate::auth::credentials::Credentials;` + `ChannelState::server_root_url()`. |
| `app/src/ai/mcp/file_based_manager_tests.rs` | A | 12 | 8 | 4 | fork ships `app/src/ai/mcp/file_based_manager.rs` |
| `app/src/ai/mcp/file_mcp_watcher_tests.rs` | A | 4 | 2 | 2 | fork ships `app/src/ai/mcp/file_mcp_watcher.rs` |
| `app/src/ai/mcp/mod_tests.rs` | B | 34 | 34 | 0 | all 34 test names present under `app/src/ai/mcp/mod_test.rs`; fork adds 4 more |
| `app/src/ai/mcp/parsing_tests.rs` | B | 6 | 6 | 0 | all 6 test names present under `app/src/ai/mcp/parsing.rs` |
| `app/src/ai/orchestration/config_state_tests.rs` | D | 6 | 0 | 6 | absent: `app/src/ai/orchestration/` (entire directory). `config_state.rs` imports `crate::ai::local_harness_setup::local_harness_product_disabled_message` — orchestration has a local-harness mode. |
| `app/src/ai/orchestration/edit_state_tests.rs` | D | 10 | 0 | 10 | absent: `app/src/ai/orchestration/`. `edit_state.rs` imports `crate::ai::harness_availability::{AuthSecretFetchState, HarnessAvailabilityModel}`; no cloud import. |
| `app/src/ai/orchestration/remote_child_tests.rs` | C | 4 | 0 | 4 | src `orchestration/remote_child.rs`: `use crate::server::server_api::ai::{AgentConfigSnapshot, SpawnAgentRequest};` + `use crate::server::server_api::{AIApiError, ClientError, CloudAgentCapacityError};` |
| `app/src/ai/orchestration/snapshots_tests.rs` | D | 18 | 0 | 18 | absent: `app/src/ai/orchestration/`. `snapshots.rs` is mostly local (`local_harness_setup`, `harness_availability`, `auth_secret_types`); `cloud_object::CloudObjectLookup` covers only the cloud host option. |
| `app/src/ai/orchestration/validation_tests.rs` | D | 5 | 0 | 5 | absent: `app/src/ai/orchestration/`. `validation.rs` mirrors `snapshots.rs`: local harness validation with one cloud host branch. |
| `app/src/ai/predict/generate_ai_input_suggestions_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `app/src/ai/predict/generate_ai_input_suggestions_test.rs` |
| `app/src/ai/predict/next_command_model_tests.rs` | B | 8 | 8 | 0 | all 8 test names present under `app/src/ai/predict/next_command_model_test.rs` |
| `app/src/ai/remote_agent_context_tests.rs` | D | 4 | 0 | 4 | absent: `remote_agent_context.rs`. Imports: `remote_server::manager::RemoteServerManager`, `remote_server::proto`, `warp_util::remote_path::RemotePath` — SSH remote, not cloud. |
| `app/src/ai/remote_context_files_tests.rs` | D | 4 | 0 | 4 | absent: `remote_context_files.rs`. Imports: `remote_server::proto`, `crate::remote_server::manager::RemoteServerManager` — SSH remote. |
| `app/src/ai/request_usage_model_tests.rs` | C | 30 | 0 | 30 | src `request_usage_model.rs` (pin): `pub use warp_graphql::billing::BonusGrantType;` + `use crate::server::server_api::ai::AIClient;` + `use crate::pricing::PricingInfoModel;` + `use crate::workspaces::user_workspaces::UserWorkspaces;`. Fork ships a 260-line no-op stub (`fn refresh_request_usage_async(&mut self, _ctx: &mut ModelContext<Self>) {}`); every test asserts Warp cloud request-limit / credit / billing policy. |
| `app/src/ai/restored_conversations_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `app/src/ai/restored_conversations_test.rs` |
| `app/src/ai/skills/bundled_tests.rs` | D | 2 | 0 | 2 | absent: `skills/bundled.rs`. Imports: `ai::skills::parse_bundled_skill`, `crate::keyboard::keybinding_file_path`, `crate::settings::user_preferences_toml_file_path` — skills shipped inside the binary. |
| `app/src/ai/skills/file_watchers/skill_watcher_tests.rs` | A | 18 | 5 | 13 | fork ships `app/src/ai/skills/file_watchers/skill_watcher.rs` |
| `app/src/ai/skills/file_watchers/utils_tests.rs` | A | 23 | 17 | 6 | fork ships `app/src/ai/skills/file_watchers/utils.rs` |
| `app/src/ai/skills/global_skills_tests.rs` | D | 11 | 0 | 11 | absent: `skills/global_skills.rs`. Imports: `ai::skills::{ParsedSkill, provider_rank}`, `warp_cli::skill::SkillSpec`; the only cloud-namespaced import is the plain data type `crate::ai::cloud_environments::GithubRepo`. |
| `app/src/ai/skills/remote_tests.rs` | D | 1 | 0 | 1 | absent: `skills/remote.rs`. Imports: `remote_server::proto::{BundledSkillMetadata, RemoteSkillProto, remote_skill_proto}` — SSH remote skills. |
| `app/src/ai/skills/resolve_skill_spec_tests.rs` | B | 7 | 7 | 0 | all 7 test names present under `app/src/ai/skills/resolve_skill_spec.rs` |
| `app/src/ai/skills/skill_manager_tests.rs` | A | 25 | 9 | 16 | fork ships `app/src/ai/skills/skill_manager.rs` |
| `app/src/ai/skills/skill_utils_tests.rs` | A | 5 | 1 | 4 | fork ships `app/src/ai/skills/skill_utils.rs` |
| `crates/ai/src/agent/action/convert_tests.rs` | A | 3 | 0 | 3 | fork ships `crates/ai/src/agent/action/convert.rs` |
| `crates/ai/src/agent/action/review_comments_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `crates/ai/src/agent/action/review_comments.rs` |
| `crates/ai/src/agent/action_result/convert_tests.rs` | A | 2 | 1 | 1 | fork ships `crates/ai/src/agent/action_result/convert.rs` |
| `crates/ai/src/agent/action_result/mod_tests.rs` | A | 3 | 0 | 3 | fork ships `crates/ai/src/agent/action_result/mod.rs` |
| `crates/ai/src/agent/ask_user_question_session_tests.rs` | B | 19 | 19 | 0 | all 19 test names present under `crates/ai/src/agent/ask_user_question_session.rs`; fork adds 9 more |
| `crates/ai/src/agent/document_action_presentation_tests.rs` | B | 4 | 4 | 0 | all 4 test names present under `crates/ai/src/agent/document_action_presentation.rs` |
| `crates/ai/src/agent/orchestration_config_tests.rs` | D | 19 | 0 | 19 | absent: `crates/ai/src/agent/orchestration_config.rs`. Whole import list: `warp_multi_agent_api as api`, `super::action::RunAgentsRequest`. Pure data type. |
| `crates/ai/src/api_keys_tests.rs` | A | 67 | 10 | 57 | Fork ships `crates/ai/src/api_keys.rs` but at 229 lines vs 776 at the pin. Of the 57 missing: 24 target the Grok subscription token path, 19 the BYOK custom-endpoint path (`custom_model_providers`, `provider_key_count`, `display_label_*`), 2 the absent `llm_provider.rs`, and 12 the GEAP path. **45 are D (feature gap)** — the fork has neither `custom_endpoints` (0 hits repo-wide) nor Grok nor `llm_provider.rs`; **12 GEAP tests are C** (workspace-brokered, see `geap_credentials`). 0 are plain test debt. |
| `crates/ai/src/diff_validation/mod_tests.rs` | B | 31 | 31 | 0 | all 31 test names present under `crates/ai/src/diff_validation/mod_test.rs` |
| `crates/ai/src/geap_credentials_tests.rs` | C | 12 | 0 | 12 | Token/state model for the workspace-brokered GEAP credential above; unusable without the `warp_managed_secrets` minting path in `app/src/ai/geap_credentials.rs`. |
| `crates/ai/src/gfm_table_tests.rs` | B | 4 | 4 | 0 | all 4 test names present under `crates/ai/src/gfm_table.rs` |
| `crates/ai/src/grok_subscription/oauth_tests.rs` | D | 5 | 0 | 5 | absent: `crates/ai/src/grok_subscription/`. `oauth.rs` imports only `std::net`, `base64`, `sha2`, `rand`, `serde` — a loopback PKCE flow against xAI, i.e. BYOP auth, no Warp server. |
| `crates/ai/src/index/file_outline/native_tests.rs` | B | 2 | 2 | 0 | all 2 test names present under `crates/ai/src/index/file_outline/native.rs` |
| `crates/ai/src/index/full_source_code_embedding/changed_files_tests.rs` | C | 10 | 0 | 10 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/chunker/naive_tests.rs` | C | 10 | 0 | 10 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/chunker/semantic_tests.rs` | C | 1 | 0 | 1 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/codebase_index_tests.rs` | C | 35 | 0 | 35 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/manager_tests.rs` | C | 13 | 0 | 13 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/merkle_tree/hash_tests.rs` | C | 3 | 0 | 3 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/merkle_tree/node_tests.rs` | C | 4 | 0 | 4 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/merkle_tree/serialized_tree_tests.rs` | C | 2 | 0 | 2 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/merkle_tree/tree_tests.rs` | C | 1 | 0 | 1 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/search_shaping_tests.rs` | C | 4 | 0 | 4 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/snapshot_tests.rs` | C | 5 | 0 | 5 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/index/full_source_code_embedding/sync_client_tests.rs` | C | 5 | 0 | 5 | The only non-mock backend for this module is `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`); `full_source_code_embedding/mod.rs` imports `warp_graphql::queries::rerank_fragments::FragmentLocationInput`. Codebase embedding is a Warp cloud vector store. |
| `crates/ai/src/llm_provider_tests.rs` | D | 2 | 0 | 2 | absent: `crates/ai/src/llm_provider.rs`. Whole import list: `serde`, `warp_core::ui::icons::Icon`, `warp_errors::report_error`, `crate::api_keys::ApiKeys`. This is the BYOP provider enum. |
| `crates/ai/src/paths_tests.rs` | B | 8 | 8 | 0 | all 8 test names present under `crates/ai/src/paths.rs` |
| `crates/ai/src/project_context/model_tests.rs` | A | 29 | 11 | 18 | fork ships `crates/ai/src/project_context/model.rs` |
| `crates/ai/src/skills/conversion_tests.rs` | A | 12 | 0 | 12 | Fork ships `crates/ai/src/skills/conversion.rs` with no tests at all. |
| `crates/ai/src/skills/parse_skill_tests.rs` | B | 12 | 12 | 0 | all 12 test names present under `crates/ai/src/skills/parse_skill_test.rs` |
| `crates/ai/src/skills/parser_tests.rs` | B | 11 | 11 | 0 | all 11 test names present under `crates/ai/src/skills/parser_test.rs` |
| `crates/ai/src/skills/read_skills_tests.rs` | B | 6 | 6 | 0 | all 6 test names present under `crates/ai/src/skills/read_skills_test.rs` |
| `crates/ai/src/skills/skill_provider_tests.rs` | A | 6 | 2 | 4 | fork ships `crates/ai/src/skills/skill_provider.rs` |

<a name="mixed-files"></a>
## Mixed files

Four files carry a single verdict in the table but do not deserve one. Per the
`server/server_api/ai_tests.rs` precedent, they are classified per test:

| file | file verdict | per-test split |
|---|:--:|---|
| `crates/ai/src/api_keys_tests.rs` | A | **0 A, 12 C, 45 D.** The fork ships `api_keys.rs` at 229 lines vs 776. 24 missing tests drive the Grok subscription token path, 19 the BYOK custom-endpoint path (`custom_model_providers`, `provider_key_count`, `display_label_*`), 2 the absent `llm_provider.rs` — all D, since the fork has zero repo-wide hits for `custom_endpoints`/`CustomEndpoint` and no `grok_subscription`. The 12 `geap_*` tests are C. |
| `app/src/ai/llms_tests.rs` | A | **12 A, 12 D.** Half the missing set targets Warp's BYOK custom-endpoint surface (`custom_llm_infos_built_from_endpoints`, alias display, endpoint purge) which the fork does not implement. |
| `app/src/ai/artifacts/mod_tests.rs` | A | **5 A, 5 C.** Five use `crate::server::server_api::ai::{ArtifactDownloadCommonFields, FileArtifactResponseData, ScreenshotArtifactResponseData}`. |
| `app/src/ai/agent_conversations_model_tests.rs` | A | **46 A, 2 C.** Two touch `crate::cloud_object::{Owner, Revision, ServerMetadata}` and `crate::server::ids::ServerId`. |

## Files that are not what their name says

Three pin paths match `*_tests.rs` but hold no tests, and every filename-based estimate has
counted them:

| path at pin | what it actually is |
|---|---|
| `app/src/ai/blocklist/inline_action/suggested_unit_tests.rs` | **Not a test file.** 429 lines of production view code for the "suggested unit tests" AI feature (`SuggestedUnitTestsView`); it contains two `#[derive]` attributes and no test attribute at all. |
| `app/src/ai/metadata_project_rules_tests.rs` | Empty file (0 bytes) at the pin. |
| `app/src/ai/persisted_workspace_tests.rs` | Empty file (0 bytes) at the pin. |

That is 187 `*_tests.rs` paths at the pin but only **184 real test files**.

## What C actually rests on

Every C above is justified from the pin *source* file, not from its path. The recurring
proofs:

- **`crate::server::server_api::*`** — the client for warp-server. Present in the whole
  `agent_sdk` cloud-task surface (`ambient`, `artifact`, `artifact_upload`, `runner`,
  `agent_management`, `driver/{attachments,git_credentials,snapshot}`) and in
  `blocklist/{local_agent_task_sync_model,orchestration_event_streamer}`.
- **`warp_graphql::*`** — `mutations::expire_api_key`, `mutations::upsert_runner`,
  `queries::rerank_fragments`, `billing::BonusGrantType`, `ai::{AgentTaskState, PlatformErrorCode}`.
- **`crate::cloud_object::*` / `CloudModel`** — `cloud_environments/catalog.rs`,
  `agent_sdk/ambient.rs`, `blocklist/handoff/*`.
- **`impl StoreClient for ServerApi`** (`app/src/server/server_api/ai.rs:3332`) — the only
  non-mock backend for `crates/ai/src/index/full_source_code_embedding/**`. The merkle-tree
  and chunker submodules are pure local algorithms, but they exist only to feed that cloud
  vector store, so the module is C as a unit (93 tests).
- **`warp_managed_secrets::client::{IdentityTokenOptions, TaskIdentityToken}`** plus
  `UserWorkspaces` — the GEAP credential path (33 tests across app and crate).

Two calls deliberately went the other way, because path suggested cloud and imports did not:

- **`app/src/ai/orchestration/**` and the whole run_agents/orchestration UI are D, not C.**
  `orchestration/config_state.rs` imports
  `crate::ai::local_harness_setup::local_harness_product_disabled_message`, and
  `blocklist/orchestration_topology.rs` imports nothing but `AIConversation`,
  `AIConversationId`, `ConversationStatus` and `BlocklistAIHistoryModel`. Warp orchestration
  has a local-harness mode; the fork ships none of it. Only `orchestration/remote_child.rs`
  is C (`SpawnAgentRequest`, `CloudAgentCapacityError`).
- **`app/src/ai/execution_profiles/*` is A/D, not C**, even though `config.rs` imports
  `crate::server::ids::ServerId` — the fork's own shipped `execution_profiles/profiles.rs`
  imports `crate::server::ids::SyncId` and `crate::cloud_object::*` too. Those namespaces are
  not a scope boundary in this fork.

And one went to C despite reading as local:

- **`app/src/ai/request_usage_model_tests.rs` (30 tests).** The fork ships
  `request_usage_model.rs`, so a source-presence check calls this debt. It is a 260-line
  no-op stub — `fn refresh_request_usage_async(&mut self, _ctx: &mut ModelContext<Self>) {}`,
  `pub struct AIRequestUsageModel;` — while the pin version imports
  `warp_graphql::billing::BonusGrantType`, `crate::server::server_api::ai::AIClient` and
  `crate::pricing::PricingInfoModel`. Every one of the 30 asserts Warp cloud credit and
  billing policy.

## Biggest single items

| tests | file | verdict |
|---:|---|:--:|
| 57 | `crates/ai/src/api_keys_tests.rs` | A |
| 55 | `app/src/ai/blocklist/orchestration_event_streamer_tests.rs` | C |
| 49 | `app/src/ai/agent_sdk/driver_tests.rs` | A |
| 48 | `app/src/ai/agent_conversations_model_tests.rs` | A |
| 39 | `app/src/ai/blocklist/history_model_tests.rs` | A |
| 38 | `app/src/ai/agent_sdk/driver/harness/codex_tests.rs` | D |
| 36 | `app/src/ai/blocklist/local_agent_task_sync_model_tests.rs` | C |
| 35 | `app/src/ai/agent_sdk/driver/snapshot_tests.rs` | C |
| 35 | `crates/ai/src/index/full_source_code_embedding/codebase_index_tests.rs` | C |
| 32 | `app/src/ai/blocklist/inline_action/run_agents_card_view_tests.rs` | D |
| 31 | `app/src/ai/blocklist/block_tests.rs` | A |
| 30 | `app/src/ai/request_usage_model_tests.rs` | C |
| 29 | `app/src/ai/agent/conversation_tests.rs` | A |
| 26 | `app/src/ai/blocklist/orchestration_topology_tests.rs` | D |
| 24 | `app/src/ai/llms_tests.rs` | A |

