# Authoritative scope classification: terminal + warp_tui test gap

Slice: every test-bearing file under `app/src/terminal/`, `crates/warp_terminal/`,
and `crates/warp_tui/` **as it exists at the pinned oracle**.

| | |
|---|---|
| Oracle pin | `02b53fcd8` — Warp `2026.07.29.09.05` stable (see `ORACLE.md`) |
| Fork side | `origin/main` @ `4f33fcf9c` |
| Method | test-**function**-name set comparison, file by file. No path matching. |

## Method

1. Every `#[test]` / `#[tokio::test]` / `#[gpui::test]` / `#[rstest]` / `#[test_case]`
   function name was extracted from every `.rs` file in the three subtrees, on both
   sides. Totals reproduce `ORACLE.md` to within 0.1% (pin 10,112 vs 10,123 repo-wide;
   per-area `app/terminal` 1535/1145, `warp_tui` 745/608, `warp_terminal` 112/111).
2. For each pin test file, fork files were accepted as its counterpart when the
   normalised stem matched (`x_tests.rs` <-> `x_test.rs` <-> inline `mod tests` in `x.rs`
   <-> flattened `a/b_c_tests.rs`) **or** when >=3 test names overlapped. A single
   coincidental name match against an unrelated file was rejected — this is what
   separates a real rename from a name collision.
3. Rejected coincidences actually found (and excluded): `selection_cursor_tests.rs::test_cursor`
   vs `grapheme_cursor_tests.rs::test_cursor`; `codex_tests.rs` vs `claude_tests.rs`/`gemini_tests.rs`;
   `in_band_command_executor_tests.rs` vs `tmux_executor_tests.rs`; `state_tests.rs` vs `input_hints_tests.rs`.
4. Verdict C was assigned only after reading the **source** file's `use` lines at the pin.
   Every C row below quotes them.

## Headline: the "~530" figure is a net, not the workload

```
pin tests in slice                     2392
fork tests in slice                    1864
net gap (the ORACLE.md number)          528   <- 390 + 137 + 1

pin tests with NO counterpart in fork    729   <- actual missing test functions
fork tests with NO counterpart at pin    201   <- fork-original / divergent
729 - 201 = 528
```

**729 test functions are missing, not ~530.** The 528 figure nets 201 fork-original
tests against them. Those 201 do not cover the Warp behaviour the 729 cover, so they
cannot be subtracted from the porting workload.

Of the 729:

| verdict | files | tests missing | meaning |
|---|---:|---:|---|
| **A** | 46 | 420 | TEST DEBT — fork ships the source, tests never ported |
| **B** | 101 | 0 | ALREADY COVERED — renamed/moved/inlined in the fork |
| **C** | 19 | 195 | OUT OF SCOPE — cloud / dropped feature |
| **D** | 11 | 114 | FEATURE GAP — non-cloud, fork lacks the source |
| **total** | 177 | 729 | |

**Real, actionable terminal test debt is 420 tests across 46 files** (verdict A).
A further 114 (verdict D) need source ported first. 195 (verdict C) should never be ported.

Per area:

| area | pin | fork | net gap | A | B files | C | D |
|---|---:|---:|---:|---:|---:|---:|---:|
| `app/terminal` | 1535 | 1145 | 390 | 296 | 55 | 152 | 43 |
| `crates/warp_tui` | 745 | 608 | 137 | 122 | 36 | 43 | 71 |
| `crates/warp_terminal` | 112 | 111 | 1 | 2 | 10 | 0 | 0 |

## Verdict table

`tests at pin` / `in fork` = test functions in the pin file, and how many of those exact
functions exist anywhere in the fork's counterpart. `missing` = the difference.

| path (at pin) | verdict | at pin | in fork | missing | evidence |
|---|---|---:|---:|---:|---|
| `app/src/terminal/alias_tests.rs` | B | 5 | 5 | 0 | same path; all names present |
| `app/src/terminal/available_shells_tests.rs` | B | 9 | 9 | 0 | renamed -> `app/src/terminal/available_shells_test.rs`; all names present |
| `app/src/terminal/bootstrap_tests.rs` | B | 4 | 4 | 0 | renamed -> `app/src/terminal/bootstrap_test.rs`; all names present |
| `app/src/terminal/cli_agent_sessions/listener/mod_tests.rs` | A | 21 | 16 | 5 | counterpart `app/src/terminal/cli_agent_sessions/listener/mod.rs`; 5 pin test fn(s) absent |
| `app/src/terminal/cli_agent_sessions/mod_tests.rs` | A | 39 | 31 | 8 | counterpart `app/src/terminal/cli_agent_sessions/mod_tests.rs`; 8 pin test fn(s) absent |
| `app/src/terminal/cli_agent_sessions/plugin_manager/claude_tests.rs` | A | 20 | 9 | 11 | counterpart `app/src/terminal/cli_agent_sessions/plugin_manager/claude_tests.rs`; 11 pin test fn(s) absent. Fork `claude.rs` is 247 lines vs 372 at pin — some of the 11 need source work too. |
| `app/src/terminal/cli_agent_sessions/plugin_manager/codex_tests.rs` | D | 39 | 0 | 39 | Non-cloud, but fork lacks the implementation: fork `plugin_manager/codex.rs` is 61 lines (install-instruction text only) vs 492 at pin. Missing source behaviour = the Codex plugin auto-install/update/config manager (`use crate::terminal::model::session::LocalCommandExecutor;`, `MARKETPLACE_REPO = "warpdotdev/codex-warp"`). Fork ships 3 fork-original tests in its own `codex_tests.rs`; 0 of the 39 pin names survive. |
| `app/src/terminal/cli_agent_sessions/plugin_manager/gemini_tests.rs` | B | 12 | 12 | 0 | same path; all names present |
| `app/src/terminal/cli_agent_sessions/plugin_manager/mod_tests.rs` | B | 13 | 13 | 0 | same path; all names present |
| `app/src/terminal/cli_agent_sessions/plugin_manager/opencode_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `app/src/terminal/cli_agent_tests.rs` | A | 34 | 28 | 6 | counterpart `app/src/terminal/cli_agent_tests.rs`; 6 pin test fn(s) absent |
| `app/src/terminal/conversation_restoration_tests.rs` | A | 16 | 10 | 6 | counterpart `app/src/terminal/conversation_restoration.rs`; 6 pin test fn(s) absent |
| `app/src/terminal/find/model/alt_screen_tests.rs` | B | 3 | 3 | 0 | renamed -> `app/src/terminal/find/model/alt_screen_test.rs`; all names present |
| `app/src/terminal/find/model/async_find_tests.rs` | B | 21 | 21 | 0 | same path; all names present |
| `app/src/terminal/find/model/block_list_tests.rs` | B | 7 | 7 | 0 | renamed -> `app/src/terminal/find/model/block_list_test.rs`; all names present |
| `app/src/terminal/focus_env_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/grid_renderer/box_drawing_tests.rs` | B | 10 | 10 | 0 | same path; all names present |
| `app/src/terminal/grid_renderer_tests.rs` | B | 5 | 5 | 0 | renamed -> `app/src/terminal/grid_renderer_test.rs`; all names present |
| `app/src/terminal/history/up_arrow_tests.rs` | B | 6 | 6 | 0 | same path; all names present |
| `app/src/terminal/history_tests.rs` | B | 14 | 14 | 0 | same path; all names present |
| `app/src/terminal/input/decorations_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/input/handoff_compose_tests.rs` | C | 1 | 0 | 1 | src `handoff_compose.rs`: `use crate::ai::ambient_agents::telemetry::HandoffEntryPoint;` `use crate::server::ids::SyncId;` — cloud-handoff telemetry + server ids. Fork drops `ai/ambient_agents/telemetry.rs` and `app/src/server`. |
| `app/src/terminal/input/inline_history/data_source_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/input/inline_menu/model_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/input/message_bar/mod_tests.rs` | B | 8 | 8 | 0 | same path; all names present |
| `app/src/terminal/input/slash_command_model_tests.rs` | A | 17 | 13 | 4 | counterpart `app/src/terminal/input/slash_command_model_tests.rs`; 4 pin test fn(s) absent |
| `app/src/terminal/input/slash_commands/data_source/mod_tests.rs` | B | 6 | 6 | 0 | renamed -> `app/src/terminal/input/slash_commands/data_source/mod_test.rs`; all names present |
| `app/src/terminal/input/slash_commands/data_source/saved_prompts_tests.rs` | B | 9 | 9 | 0 | same path; all names present |
| `app/src/terminal/input/slash_commands/mod_tests.rs` | A | 11 | 1 | 10 | counterpart `app/src/terminal/input/slash_commands/mod.rs`; 10 pin test fn(s) absent. 2 of the 10 are cloud-mode gating (`not_cloud_agent_commands_*`, `cloud_mode_v2_commands_*`). |
| `app/src/terminal/input_tests.rs` | A | 148 | 88 | 60 | counterpart `app/src/terminal/input_test.rs`; 60 pin test fn(s) absent |
| `app/src/terminal/local_shell/mod_tests.rs` | B | 6 | 6 | 0 | same path; all names present |
| `app/src/terminal/local_tty/shell_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `app/src/terminal/local_tty/unix.rs` | A | 3 | 0 | 3 | no counterpart test file; 3 pin test fn(s) absent. Inline `#[test]` fns in the source file (not the `#[path]`-included `unix_tests.rs`, which the fork does have): `parse_passwd_line_extracts_matching_uid`, `parse_passwd_line_matches_glibc_edge_cases`, `resolve_current_user_returns_running_user`. |
| `app/src/terminal/local_tty/unix_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `app/src/terminal/local_tty/windows/child_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/local_tty/windows/environment_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `app/src/terminal/meta_shortcuts_tests.rs` | B | 1 | 1 | 0 | renamed -> `app/src/terminal/meta_shortcuts.rs`; all names present |
| `app/src/terminal/model/alt_screen_tests.rs` | B | 3 | 3 | 0 | renamed -> `app/src/terminal/model/alt_screen_test.rs`; all names present |
| `app/src/terminal/model/ansi/ansi_c_decoder_tests.rs` | B | 11 | 11 | 0 | renamed -> `app/src/terminal/model/ansi/ansi_c_decoder_test.rs`; all names present |
| `app/src/terminal/model/ansi/mod_tests.rs` | A | 70 | 48 | 22 | counterpart `app/src/terminal/model/ansi/mod_test.rs`; 22 pin test fn(s) absent |
| `app/src/terminal/model/block/serialized_block_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `app/src/terminal/model/block_tests.rs` | A | 43 | 35 | 8 | counterpart `app/src/terminal/model/block_test.rs`; 8 pin test fn(s) absent |
| `app/src/terminal/model/blockgrid_tests.rs` | A | 9 | 8 | 1 | counterpart `app/src/terminal/model/blockgrid_test.rs`; 1 pin test fn(s) absent |
| `app/src/terminal/model/blocks/selection_tests.rs` | B | 18 | 18 | 0 | same path; all names present |
| `app/src/terminal/model/blocks_tests.rs` | A | 45 | 34 | 11 | counterpart `app/src/terminal/model/blocks_test.rs`; 11 pin test fn(s) absent |
| `app/src/terminal/model/early_output_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `app/src/terminal/model/find_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `app/src/terminal/model/grid/displayed_output_tests.rs` | B | 32 | 32 | 0 | renamed -> `app/src/terminal/model/grid/displayed_output_test.rs`; all names present |
| `app/src/terminal/model/grid/filtering_tests.rs` | B | 68 | 68 | 0 | same path; all names present |
| `app/src/terminal/model/grid/grapheme_cursor_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/model/grid/grid_handler_tests.rs` | A | 93 | 82 | 11 | counterpart `app/src/terminal/model/grid/grid_handler_test.rs`; 11 pin test fn(s) absent |
| `app/src/terminal/model/grid/grid_tests.rs` | B | 25 | 25 | 0 | renamed -> `app/src/terminal/model/grid/grid_test.rs`; all names present |
| `app/src/terminal/model/grid/secrets_tests.rs` | A | 10 | 8 | 2 | counterpart `app/src/terminal/model/grid/secrets_tests.rs`; 2 pin test fn(s) absent |
| `app/src/terminal/model/grid/selection_cursor_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/model/grid/storage_tests.rs` | B | 22 | 22 | 0 | renamed -> `app/src/terminal/model/grid/storage_test.rs`; all names present |
| `app/src/terminal/model/grid/tests.rs` | A | 34 | 33 | 1 | counterpart `app/src/terminal/model/grid/tests.rs`; 1 pin test fn(s) absent |
| `app/src/terminal/model/iterm_image_tests.rs` | B | 3 | 3 | 0 | renamed -> `app/src/terminal/model/iterm_image_test.rs`; all names present |
| `app/src/terminal/model/lifecycle/mod_tests.rs` | A | 7 | 6 | 1 | counterpart `app/src/terminal/model/lifecycle/mod_tests.rs`; 1 pin test fn(s) absent |
| `app/src/terminal/model/secrets_tests.rs` | B | 13 | 13 | 0 | renamed -> `app/src/terminal/model/secrets_test.rs`; all names present |
| `app/src/terminal/model/selection_tests.rs` | B | 8 | 8 | 0 | renamed -> `app/src/terminal/model/selection_test.rs`; all names present |
| `app/src/terminal/model/session/command_executor/in_band_command_executor_tests.rs` | B | 9 | 9 | 0 | same path; all names present |
| `app/src/terminal/model/session/command_executor/shared_tests.rs` | B | 7 | 7 | 0 | same path; all names present |
| `app/src/terminal/model/session_tests.rs` | A | 7 | 3 | 4 | counterpart `app/src/terminal/model/session_test.rs`; 4 pin test fn(s) absent |
| `app/src/terminal/model/terminal_model_tests.rs` | A | 49 | 26 | 23 | counterpart `app/src/terminal/model/terminal_model_test.rs`; 23 pin test fn(s) absent |
| `app/src/terminal/package_installers_tests.rs` | B | 4 | 4 | 0 | renamed -> `app/src/terminal/package_installers_test.rs`; all names present |
| `app/src/terminal/settings_tests.rs` | B | 6 | 6 | 0 | same path; all names present |
| `app/src/terminal/share_block_modal_tests.rs` | C | 2 | 0 | 2 | src `share_block_modal.rs`: `use crate::server::block::{Block as ServerBlock, DisplaySetting};` `use crate::server::server_api::block::BlockClient;` — uploads a block to Warp cloud. |
| `app/src/terminal/shared_session/mod_tests.rs` | A | 13 | 12 | 1 | counterpart `app/src/terminal/shared_session/mod_test.rs`; 1 pin test fn(s) absent |
| `app/src/terminal/shared_session/network/heartbeat_tests.rs` | D | 2 | 0 | 2 | Missing source `app/src/terminal/shared_session/network/heartbeat.rs`. Its own imports are non-cloud (`use std::time::Duration; use futures::stream::AbortHandle; use warpui::r#async::Timer; use warpui::{Entity, ModelContext};`) — a generic ping timer. Its only consumer is the dropped session-sharing websocket layer, and both pin tests are `#[ignore = "Flakes in CI"]`. |
| `app/src/terminal/shared_session/presence_manager_tests.rs` | A | 5 | 3 | 2 | counterpart `app/src/terminal/shared_session/presence_manager_test.rs`; 2 pin test fn(s) absent |
| `app/src/terminal/shared_session/selections_tests.rs` | B | 4 | 4 | 0 | renamed -> `app/src/terminal/shared_session/selections_test.rs`; all names present |
| `app/src/terminal/shared_session/share_modal/body_tests.rs` | C | 5 | 0 | 5 | Start-sharing UI for the cloud session-sharing feature; sibling transport `sharer/network.rs` is `use session_sharing_protocol::sharer::InitPayload;` + `use warp_server_client::iap::IapManager;`. Fork drops `shared_session/{share_modal,sharer,network}/`. |
| `app/src/terminal/shared_session/sharer/network_tests.rs` | C | 17 | 0 | 17 | src `sharer/network.rs`: `use session_sharing_protocol::sharer::InitPayload;` `use warp_server_client::iap::IapManager;` `use crate::server::server_api::ServerApiProvider;` `use crate::auth::{AuthStateProvider, UserUid};` |
| `app/src/terminal/shared_session/viewer/event_loop_tests.rs` | C | 13 | 0 | 13 | src `viewer/event_loop.rs`: `use session_sharing_protocol::common::{...};` — decodes the cloud session-sharing wire protocol. |
| `app/src/terminal/shared_session/viewer/mod_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `app/src/terminal/shared_session/viewer/network_tests.rs` | C | 3 | 0 | 3 | src `viewer/network.rs`: `use session_sharing_protocol::viewer::{...};` `use warp_server_client::iap::IapManager;` `use crate::server::server_api::auth::AuthClient;` |
| `app/src/terminal/shared_session/viewer/orchestration_viewer_model_tests.rs` | C | 34 | 0 | 34 | src `viewer/orchestration_viewer_model.rs`: `use session_sharing_protocol::common::SessionId;` `use crate::server::server_api::ServerApiProvider;` `use crate::ai::ambient_agents::{AmbientAgentTask, ...};` |
| `app/src/terminal/shared_session/viewer/terminal_manager_tests.rs` | C | 5 | 0 | 5 | src `viewer/terminal_manager.rs`: `use session_sharing_protocol::sharer::SessionSourceType;` `use session_sharing_protocol::viewer::SessionEndedReason;` |
| `app/src/terminal/size_update_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `app/src/terminal/ssh/util_tests.rs` | B | 5 | 5 | 0 | renamed -> `app/src/terminal/ssh/util.rs`; all names present |
| `app/src/terminal/view/ambient_agent/block/setup_command_text_tests.rs` | D | 2 | 0 | 2 | Missing source `app/src/terminal/view/ambient_agent/block/setup_command_text.rs` (and sibling `block/setup_command.rs`). Imports are non-cloud UI: `use warp_core::ui::Icon;` `use crate::ai::blocklist::inline_action::inline_action_icons;` `use crate::terminal::view::ambient_agent::{AmbientAgentViewModel, AmbientAgentViewModelEvent};`. Fork keeps `view/ambient_agent/` and `block/{entry,query}.rs`. |
| `app/src/terminal/view/ambient_agent/model_tests.rs` | C | 13 | 0 | 13 | src `view/ambient_agent/model.rs`: `use crate::cloud_object::model::persistence::{CloudModel, CloudModelEvent};` `use crate::server::cloud_objects::update_manager::UpdateManager;` `use crate::server::server_api::ServerApiProvider;` `use crate::ai::cloud_environments::CloudAmbientAgentEnvironment;` `use session_sharing_protocol::common::SessionId;`. All 13 names are cloud (`github_auth_url_*`, `viewed_task_config_preserves_environment_before_cloud_model_load`, handoff). |
| `app/src/terminal/view/blocklist_filter_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `app/src/terminal/view/link_detection_tests.rs` | B | 5 | 5 | 0 | same path; all names present |
| `app/src/terminal/view/open_in_warp_tests.rs` | B | 7 | 7 | 0 | same path; all names present |
| `app/src/terminal/view/queued_prompts_tests.rs` | A | 39 | 11 | 28 | counterpart `app/src/terminal/view/queued_prompts_tests.rs`; 28 pin test fn(s) absent. Source is `view/queued_prompts_panel.rs` (1367 pin / 1242 fork lines) — present in the fork. |
| `app/src/terminal/view/shared_session/cloud_conversation_continuation_tests.rs` | C | 23 | 0 | 23 | src: `use crate::cloud_object::{Owner, ServerGuestSubject};` `use crate::drive::sharing::SharingAccessLevel;` `use crate::ai::agent::api::ServerConversationToken;` `use crate::workspaces::user_workspaces::UserWorkspaces;` |
| `app/src/terminal/view/shared_session/conversation_ended_tombstone_view_tests.rs` | A | 7 | 6 | 1 | counterpart `app/src/terminal/view/shared_session/conversation_ended_tombstone_view_tests.rs`; 1 pin test fn(s) absent |
| `app/src/terminal/view/shared_session/view_impl_tests.rs` | C | 41 | 5 | 36 | src `view/shared_session/view_impl.rs`: `use session_sharing_protocol::{common,sharer,viewer}::…;` `use crate::drive::sharing::ShareableObject;` `use crate::auth::UserUid;`. All 36 missing names are ambient/cloud-session/handoff (`test_begin_viewing_ambient_session_*`, `test_*_cloud_handoff_session_join_*`, `test_continue_in_cloud_tombstone_*`). Fork keeps the 5 non-cloud ones. |
| `app/src/terminal/view/use_agent_footer/mod_tests.rs` | A | 8 | 2 | 6 | counterpart `app/src/terminal/view/use_agent_footer/mod_test.rs`; 6 pin test fn(s) absent. 3 of the 6 are cloud (`*_cloud_agent_setup_lrc`, `*_shared_cloud_agent_session`); 3 are portable (oh-my-pi / hermes rich-input submit strategy). |
| `app/src/terminal/view_tests.rs` | A | 141 | 86 | 55 | counterpart `app/src/terminal/view_test.rs`; 55 pin test fn(s) absent |
| `app/src/terminal/warpify/settings_tests.rs` | A | 7 | 3 | 4 | counterpart `app/src/terminal/warpify/settings_test.rs`; 4 pin test fn(s) absent |
| `app/src/terminal/writeable_pty/pty_controller_command_bytes_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `app/src/terminal/writeable_pty/pty_controller_lifecycle_tests.rs` | A | 2 | 0 | 2 | no counterpart test file; 2 pin test fn(s) absent. Fork ships `writeable_pty/pty_controller.rs`; only the lifecycle test file was never ported. |
| `app/src/terminal/writeable_pty/pty_controller_tests.rs` | B | 6 | 6 | 0 | same path; all names present |
| `app/src/terminal/writeable_pty/remote_server_controller_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/ansi/control_sequence_parameters.rs` | B | 14 | 14 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/escape_sequences_tests.rs` | A | 20 | 18 | 2 | counterpart `crates/warp_terminal/src/model/escape_sequences_test.rs`; 2 pin test fn(s) absent |
| `crates/warp_terminal/src/model/grid/cell_tests.rs` | B | 7 | 7 | 0 | renamed -> `crates/warp_terminal/src/model/grid/cell_test.rs`; all names present |
| `crates/warp_terminal/src/model/grid/flat_storage/attribute_map_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/grid/flat_storage/content_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/grid/flat_storage/index_tests.rs` | B | 16 | 16 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/grid/flat_storage/mod_tests.rs` | B | 11 | 11 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/grid/hyperlink_registry_tests.rs` | B | 6 | 6 | 0 | same path; all names present |
| `crates/warp_terminal/src/model/indexing_tests.rs` | B | 18 | 18 | 0 | same path; all names present |
| `crates/warp_terminal/src/shell/mod_tests.rs` | B | 12 | 12 | 0 | same path; all names present |
| `crates/warp_terminal/src/shell/unescape.rs` | B | 4 | 4 | 0 | same path; all names present |
| `crates/warp_tui/src/agent_block_tests.rs` | A | 48 | 42 | 6 | counterpart `crates/warp_tui/src/agent_block_tests.rs`; 6 pin test fn(s) absent |
| `crates/warp_tui/src/agent_message_tests.rs` | C | 4 | 0 | 4 | src `agent_message.rs`: `use warp::tui_export::{… OrchestrationParticipantKind, orchestrator_agent_id_for_conversation, resolve_orchestration_participant};` — renders orchestrated (cloud) child-agent transcripts. |
| `crates/warp_tui/src/alt_screen_view_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `crates/warp_tui/src/attachment_bar/image_processing_tests.rs` | B | 11 | 11 | 0 | same path; all names present |
| `crates/warp_tui/src/attachment_bar/model_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_tui/src/attachment_bar/view_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `crates/warp_tui/src/autoupdate_tests.rs` | B | 15 | 15 | 0 | same path; all names present |
| `crates/warp_tui/src/clipboard_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `crates/warp_tui/src/cloud_run_view_tests.rs` | C | 2 | 0 | 2 | src `cloud_run_view.rs`: `use crate::cloud_run::{TuiCloudRunStartup, TuiCloudRunState};` `use crate::orchestration_model::{…};` |
| `crates/warp_tui/src/completion_menu_tests.rs` | D | 2 | 0 | 2 | Missing source `crates/warp_tui/src/completion_menu.rs` (`use warp_completer::completer::{EngineFileType, MatchedSuggestion};` — local completion engine, non-cloud). Fork ships a divergent `completions_menu.rs` with no overlapping test names. |  **[FALSE POSITIVE — verified 2026-08-10: the fork's `completions_menu.rs` IS this component, renamed. Same list state, same try_open/close gating, same inline-menu wiring, same completer engine. Zero test-name overlap was renaming, not absence. Do not port; -2 from the debt count.]**
| `crates/warp_tui/src/conversation_menu_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `crates/warp_tui/src/conversation_selection_tests.rs` | B | 8 | 8 | 0 | same path; all names present |
| `crates/warp_tui/src/editor_element_tests.rs` | B | 24 | 24 | 0 | same path; all names present |
| `crates/warp_tui/src/editor_view_tests.rs` | A | 14 | 13 | 1 | counterpart `crates/warp_tui/src/editor_view_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/exit_confirmation_tests.rs` | B | 5 | 5 | 0 | same path; all names present |
| `crates/warp_tui/src/grok_oauth/tests.rs` | D | 3 | 0 | 3 | Missing sources `crates/warp_tui/src/grok_oauth/{mod,session}.rs` and `crates/ai/src/grok_subscription/`. Non-cloud: pin `grok_subscription/oauth.rs` is a local PKCE loopback flow (`use std::net::{Shutdown, TcpListener, TcpStream}; use sha2::{Digest, Sha256}; use base64::engine::general_purpose::URL_SAFE_NO_PAD;`) with no Warp-server import. |
| `crates/warp_tui/src/handoff/model_tests.rs` | C | 1 | 0 | 1 | src `handoff/model.rs`: `use warp::tui_export::{… CloudEnvironmentCatalog, PendingCloudLaunch, SnapshotUploadTarget, ServerApiProvider, UserWorkspaces …};` — hands a local TUI session off to a Warp cloud agent. |
| `crates/warp_tui/src/handoff/tests.rs` | C | 5 | 0 | 5 | Same module as above; `handoff/mod.rs` is `mod block; mod model;`, and `model.rs` imports `CloudEnvironmentCatalog`, `PendingCloudLaunch`, `SnapshotUploadTarget`, `ServerApiProvider`. |
| `crates/warp_tui/src/inline_menu_tests.rs` | B | 24 | 24 | 0 | same path; all names present |
| `crates/warp_tui/src/input/view_tests.rs` | A | 89 | 67 | 22 | counterpart `crates/warp_tui/src/input/view_tests.rs`; 22 pin test fn(s) absent |
| `crates/warp_tui/src/input_mode_policy_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `crates/warp_tui/src/input_suggestions_mode_tests.rs` | B | 3 | 3 | 0 | same path; all names present |
| `crates/warp_tui/src/keybindings_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `crates/warp_tui/src/link_tests.rs` | A | 2 | 1 | 1 | counterpart `crates/warp_tui/src/link_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/model_menu_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_tui/src/option_selector_tests.rs` | A | 39 | 38 | 1 | counterpart `crates/warp_tui/src/option_selector_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/orchestrated_agent_identity_styling_tests.rs` | D | 9 | 0 | 9 | Missing source `crates/warp_tui/src/orchestrated_agent_identity_styling.rs`. Imports are pure theming — `use pathfinder_color::ColorU; use warp_core::ui::theme::{Fill as ThemeFill, TerminalColors}; use warpui_core::elements::{Fill as CoreFill, tui::TuiStyle};` — no cloud. Caveat: its only consumers (`orchestration_block.rs`, `agent_message.rs`) are verdict C, so porting it has no user-visible effect until orchestration is ported. |
| `crates/warp_tui/src/orchestration_block_tests.rs` | C | 20 | 0 | 20 | src `orchestration_block.rs`: `use warp::tui_export::{… ORCHESTRATION_WARP_WORKER_HOST, AuthSecretSelection, RunAgentsExecutor, resolve_default_environment_id, resolve_default_host_slug …};` — configures cloud worker hosts / auth secrets. |
| `crates/warp_tui/src/orchestration_model_tests.rs` | C | 5 | 0 | 5 | src `orchestration_model.rs`: `use warp::tui_export::{… CloudAgentStartupIssue, PreparedRemoteChildLaunch, ServerApiProvider, classify_cloud_agent_startup_error, oz_run_url, prepare_remote_child_launch …};` `use crate::cloud_run::TuiCloudRunState;` |
| `crates/warp_tui/src/platform_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_tui/src/prompt_and_command_history_menu_tests.rs` | D | 12 | 0 | 12 | Missing source `crates/warp_tui/src/prompt_and_command_history_menu.rs` (`use warp::editor::{CodeEditorModel, CodeEditorModelEvent}; use warp_editor::model::CoreEditorModel;` — non-cloud). Fork ships a divergent `prompt_history_menu.rs`; 0 of the 12 pin names overlap. |
| `crates/warp_tui/src/read_only_menu_tests.rs` | D | 6 | 0 | 6 | Missing source `crates/warp_tui/src/read_only_menu.rs`. Imports are pure UI — `use warpui_core::AppContext; use warpui_core::elements::CrossAxisAlignment; use warpui_core::elements::tui::{…}; use crate::tui_builder::TuiUiBuilder;`. |
| `crates/warp_tui/src/resume_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `crates/warp_tui/src/root_view_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `crates/warp_tui/src/session_registry_tests.rs` | B | 1 | 1 | 0 | same path; all names present |
| `crates/warp_tui/src/session_tests.rs` | A | 9 | 3 | 6 | counterpart `crates/warp_tui/src/session_tests.rs`; 6 pin test fn(s) absent |
| `crates/warp_tui/src/slash_commands_tests.rs` | A | 21 | 17 | 4 | counterpart `crates/warp_tui/src/slash_commands_tests.rs`; 4 pin test fn(s) absent |
| `crates/warp_tui/src/statusline_config_view_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_tui/src/tab_bar_tests.rs` | B | 12 | 12 | 0 | same path; all names present |
| `crates/warp_tui/src/terminal_block_tests.rs` | B | 9 | 9 | 0 | same path; all names present |
| `crates/warp_tui/src/terminal_content_element_tests.rs` | B | 10 | 10 | 0 | same path; all names present |
| `crates/warp_tui/src/terminal_session_view/completions_tests.rs` | D | 3 | 0 | 3 | Missing source `crates/warp_tui/src/terminal_session_view/completions.rs` (`use warp_completer::completer::{…}; use warp_core::SessionId;` — local shell completion, non-cloud). |
| `crates/warp_tui/src/terminal_session_view/input_detection_tests.rs` | B | 9 | 9 | 0 | same path; all names present |
| `crates/warp_tui/src/terminal_session_view/state_tests.rs` | D | 10 | 0 | 10 | Missing source `crates/warp_tui/src/terminal_session_view/state.rs` (`use warp::tui_export::{BlocklistAIInputModel, CLISubagentController, TerminalModel};` — non-cloud input-ownership state machine). Fork keeps `terminal_session_view.rs` monolithic with no equivalent extracted state type; 9/10 names absent (the 10th collides with an unrelated `input_hints_tests.rs` name and was rejected). |
| `crates/warp_tui/src/terminal_session_view_tests.rs` | A | 90 | 33 | 57 | counterpart `crates/warp_tui/src/terminal_session_view_tests.rs`; 57 pin test fn(s) absent. Largest single A row. >=20 of the 57 depend on sources classified C or D here (voice_* x5, orchestration_* x7, grok_oauth x1, prompt/command-history x3, status menu x4, cost slash command x1) and cannot land until those sources do. |
| `crates/warp_tui/src/terminal_use_tests.rs` | A | 12 | 11 | 1 | counterpart `crates/warp_tui/src/terminal_use_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/tool_call_labels_tests.rs` | A | 6 | 5 | 1 | counterpart `crates/warp_tui/src/tool_call_labels_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/transcript_view_tests.rs` | B | 8 | 8 | 0 | same path; all names present |
| `crates/warp_tui/src/transient_hint_tests.rs` | A | 4 | 3 | 1 | counterpart `crates/warp_tui/src/transient_hint_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/tui_ask_question_view_tests.rs` | A | 13 | 9 | 4 | counterpart `crates/warp_tui/src/tui_ask_question_view_tests.rs`; 4 pin test fn(s) absent |
| `crates/warp_tui/src/tui_block_list_viewport_source_tests.rs` | B | 19 | 19 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_builder_tests.rs` | A | 3 | 2 | 1 | counterpart `crates/warp_tui/src/tui_builder_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/src/tui_cli_subagent_view_tests.rs` | A | 9 | 4 | 5 | counterpart `crates/warp_tui/src/tui_cli_subagent_view_tests.rs`; 5 pin test fn(s) absent |
| `crates/warp_tui/src/tui_code_block_view_tests.rs` | B | 5 | 5 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_column_layout_tests.rs` | B | 5 | 5 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_diff_storage_tests.rs` | B | 9 | 9 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_file_edits_view_tests.rs` | A | 9 | 5 | 4 | counterpart `crates/warp_tui/src/tui_file_edits_view_tests.rs`; 4 pin test fn(s) absent |
| `crates/warp_tui/src/tui_generic_tool_call_view_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_markdown_tests.rs` | B | 15 | 15 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_permission_prompt_tests.rs` | A | 8 | 4 | 4 | counterpart `crates/warp_tui/src/tui_permission_prompt_tests.rs`; 4 pin test fn(s) absent |
| `crates/warp_tui/src/tui_plan_view_tests.rs` | B | 7 | 7 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_review_comments_tests.rs` | B | 2 | 2 | 0 | same path; all names present |
| `crates/warp_tui/src/tui_shell_command_view_tests.rs` | A | 11 | 9 | 2 | counterpart `crates/warp_tui/src/tui_shell_command_view_tests.rs`; 2 pin test fn(s) absent |
| `crates/warp_tui/src/ui_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `crates/warp_tui/src/usage_tests.rs` | C | 3 | 0 | 3 | src `usage.rs` at pin: `use warp::tui_export::{ConversationUsageTotals, format_credits};` — Warp cloud credit/cost billing. Fork replaced it with a BYOP context-window display (fork `usage.rs` doc comment: "BYOP replacement for Warp's cloud credits/cost usage entry"). Missing names are `cost_formats_cents_as_dollars`, `entry_text_matches_the_gui_credits_formatting`, `entry_text_follows_the_persisted_display_mode`. |
| `crates/warp_tui/src/voice_input_tests.rs` | C | 3 | 0 | 3 | src `voice_input.rs`: `use warp::tui_export::{AIRequestUsageModel, StartListeningError, TranscribeError, UserWorkspaces, VoiceInput, VoiceInputToggledFrom, VoiceSessionResult, VoiceTranscriber};` — `VoiceTranscriber` is `app/src/voice/transcriber.rs`, whose pin import is `use crate::server::server_api::TranscribeError;` (Warp server transcription). Fork ships a permanently-`Disabled` transcriber stub. |
| `crates/warp_tui/src/warping_indicator_tests.rs` | B | 4 | 4 | 0 | same path; all names present |
| `crates/warp_tui/src/zero_state_animation_tests.rs` | D | 26 | 0 | 26 | Fork `zero_state_animation.rs` is a fork-original starfield ("Starfield animation for the TUI zero state") with 13 of its own tests; pin's is a rotating wireframe Warp mark plus a missing source `crates/warp_tui/src/zero_state_animation_config.rs`. 0/26 pin names overlap. Non-cloud (`use warpui_core::elements::tui::{…}` only). |
| `crates/warp_tui/src/zero_state_tests.rs` | A | 6 | 5 | 1 | counterpart `crates/warp_tui/src/zero_state_tests.rs`; 1 pin test fn(s) absent |
| `crates/warp_tui/tests/worker_dispatch.rs` | B | 1 | 1 | 0 | same path; all names present |

## Appendix: every missing test function, by file

This is the burndown list. Verify any row above by grepping these names on both sides.

### `app/src/terminal/cli_agent_sessions/listener/mod_tests.rs` — A, 5 missing

- `codex_try_parse_ignores_osc9_when_plugin_already_active`
- `codex_try_parse_ignores_structured_event_without_codex_plugin`
- `codex_try_parse_ignores_other_structured_agents`
- `oh_my_pi_is_supported`
- `oh_my_pi_end_to_end_parsing_and_handling`

### `app/src/terminal/cli_agent_sessions/mod_tests.rs` — A, 8 missing

- `parse_droid_stop_notification`
- `codex_session_not_rich_until_rich_notification`
- `non_codex_session_rich_after_rich_notification`
- `stop_clears_permission_scoped_state`
- `permission_replied_clears_permission_scoped_state`
- `prompt_submit_clears_permission_scoped_state`
- `tool_complete_clears_permission_scoped_state`
- `permission_request_still_populates_summary_and_tool_fields`

### `app/src/terminal/cli_agent_sessions/plugin_manager/claude_tests.rs` — A, 11 missing

- `local_marketplace_override_detects_directory_source`
- `local_marketplace_override_ignores_repo_source`
- `local_marketplace_override_via_trait_uses_claude_config_dir`
- `installed_platform_plugin_version_returns_version_when_present`
- `platform_plugin_installed_when_platform_plugin_present`
- `platform_plugin_needs_update_via_trait_when_version_below_minimum`
- `platform_plugin_does_not_need_update_via_trait_when_current`
- `platform_plugin_needs_update_via_trait_when_installed_without_version`
- `platform_plugin_not_installed_when_only_notification_plugin_present`
- `is_installed_via_trait_with_claude_config_dir_env`
- `not_installed_via_trait_when_claude_config_dir_empty`

### `app/src/terminal/cli_agent_sessions/plugin_manager/codex_tests.rs` — D, 39 missing

- `can_auto_install_is_true`
- `can_auto_install_is_false_without_codex_plugin`
- `install_instructions_are_native_without_codex_plugin`
- `supports_update`
- `does_not_support_update_without_codex_plugin`
- `minimum_version`
- `minimum_version_is_zero_without_codex_plugin`
- `install_instructions_has_marketplace_and_plugin_add_steps`
- `update_instructions_has_marketplace_and_plugin_add_steps`
- `update_instructions_are_empty_without_codex_plugin`
- `installed_when_plugin_enabled_in_config`
- `not_installed_when_plugin_disabled_in_config`
- `not_installed_when_only_marketplace_present`
- `platform_plugin_installed_when_enabled_in_config`
- `platform_plugin_not_installed_when_disabled_in_config`
- `not_installed_when_config_missing`
- `not_installed_when_config_invalid`
- `installed_version_reads_cache_manifest_version`
- `installed_platform_plugin_version_reads_cache_manifest_version`
- `installed_version_picks_latest_cached`
- `installed_version_returns_none_when_cache_missing`
- `installed_version_returns_none_when_cache_manifest_has_no_version`
- `platform_plugin_version_is_current_when_cache_current`
- `platform_plugin_version_is_not_current_when_cache_outdated`
- `needs_update_true_when_enabled_and_version_outdated`
- `needs_update_false_when_enabled_and_version_current`
- `needs_update_false_when_not_enabled`
- `needs_update_true_when_enabled_without_cached_version`
- `platform_plugin_needs_update_true_when_enabled_and_outdated`
- `platform_plugin_needs_update_false_when_current`
- `is_not_installed_via_trait_without_codex_plugin`
- `is_installed_via_trait_with_codex_home_env`
- `is_platform_plugin_installed_via_trait_with_codex_home_env`
- `is_platform_plugin_not_installed_via_trait_without_codex_plugin`
- `needs_update_via_trait_with_codex_home_env`
- `does_not_need_update_via_trait_when_version_current`
- `does_not_need_update_without_codex_plugin`
- `does_not_need_update_when_not_enabled`
- `does_not_need_update_for_non_git_marketplace_override`

### `app/src/terminal/cli_agent_tests.rs` — A, 6 missing

- `test_detect_vibe_acp_binary`
- `test_oh_my_pi_supports_bash_mode`
- `test_warp_tui_matches_binaries_and_launchers`
- `test_warp_tui_matches_with_env_var_prefix`
- `test_warp_tui_does_not_match_other_commands`
- `test_warp_tui_variant_properties`

### `app/src/terminal/conversation_restoration_tests.rs` — A, 6 missing

- `sorted_blocks_exchange_equal_to_block`
- `sorted_tail_exchange_equals_tail_block`
- `sorted_tail_equal_timestamps_pick_first_inserted_block`
- `single_block_at_same_time_as_exchange`
- `conversation_tracks_initial_and_latest_working_directory`
- `forked_startup_working_directory_uses_latest_directory`

### `app/src/terminal/input/handoff_compose_tests.rs` — C, 1 missing

- `preserves_explicit_environment_selection`

### `app/src/terminal/input/slash_command_model_tests.rs` — A, 4 missing

- `test_parse_input_requires_slash_at_start`
- `test_non_ai_commands_remain_active_when_ai_is_disabled`
- `repository_gated_command_drops_when_leaving_repository`
- `repository_gated_command_stays_within_repository`

### `app/src/terminal/input/slash_commands/mod_tests.rs` — A, 10 missing

- `slash_command_is_submitted_as_prompt_only_for_prompt_commands`
- `auto_approve_is_an_exact_no_argument_command`
- `theme_command_inserts_input_for_its_required_argument`
- `tui_commands_have_typed_identities_and_explicit_surface_support`
- `model_command_is_supported_in_tui_without_becoming_a_prompt_command`
- `exit_command_executes_immediately_and_takes_no_argument`
- `logout_command_executes_immediately_and_takes_no_argument`
- `not_cloud_agent_commands_are_only_active_outside_cloud_mode`
- `cloud_mode_v2_commands_are_active_only_in_cloud_mode_v2_context`
- `natural_language_detection_command_is_supported_in_tui`

### `app/src/terminal/input_tests.rs` — A, 60 missing

- `renders_git_checkout_prompt_chip_command_as_single_shell_argument`
- `renders_nvm_use_prompt_chip_command_as_single_shell_argument`
- `renders_change_directory_prompt_chip_command_as_single_shell_argument`
- `renders_echo_prompt_chip_command_as_single_shell_argument`
- `renders_fixed_prompt_chip_command_without_interpolation`
- `zero_state_hint_text_only_registers_active_slash_command_placeholders`
- `maybe_route_ai_query_to_remote_target_proceeds_for_local_pane`
- `maybe_route_ai_query_to_remote_target_proceeds_for_empty_buffer`
- `maybe_route_ai_query_to_remote_target_blocks_read_only_viewer`
- `maybe_route_ai_query_to_remote_target_forwards_executor_viewer_prompt`
- `attach_ambient_view_model_builds_composer_selectors_for_fresh_cloud_pane_in_view_pending`
- `attach_ambient_view_model_skips_composer_selectors_for_actual_shared_session_viewer`
- `cloud_mode_host_selector_shown_when_connected_workers_present`
- `send_now_event_submits_through_active_pane_and_preserves_draft`
- `send_now_command_event_executes_command_and_arms_in_flight`
- `queued_command_completion_preserves_draft`
- `row_deleted_event_preserves_existing_draft`
- `empty_buffer_enter_sends_top_queued_prompt_then_next_on_repeat`
- `empty_buffer_enter_executes_top_queued_command`
- `enter_with_nonempty_buffer_does_not_send_queued_row`
- `empty_buffer_enter_skips_locked_initial_cloud_mode_head`
- `prompt_submission_auto_queues_during_agent_requested_lrc`
- `lrc_queued_prompts_wait_while_subagent_is_active`
- `prompt_submission_during_lrc_with_non_lrc_queue_head_uses_generic_origin`
- `prompt_submission_during_lrc_with_lrc_queue_head_uses_lrc_origin`
- `prompt_submission_does_not_auto_queue_for_user_tagged_lrc`
- `prompt_submission_is_not_queued_during_lrc_when_set_to_send_immediately`
- `prompt_submission_during_lrc_with_queue_default_uses_generic_origin`
- `lrc_queued_prompts_fire_from_queue_head_when_command_finishes`
- `ghost_text_shows_queue_hint_during_agent_requested_lrc`
- `shell_submission_queues_as_command_row_when_gated_under_v2`
- `shell_submission_is_not_queued_when_v2_disabled`
- `slash_fork_bypasses_prompt_queue_while_in_progress`
- `slash_compact_still_queues_while_in_progress`
- `test_open_slash_command_does_not_autofill_single_file_completion`
- `question_mark_does_not_toggle_shortcuts_while_editing_queued_prompt`
- `test_classic_tab_completions_close_after_user_backspace`
- `test_classic_tab_completions_keep_menu_open_while_cycling`
- `test_cloud_handoff_prefix_remains_text_when_handoff_flag_disabled`
- `test_cloud_handoff_prefix_activates_when_handoff_flags_enabled`
- `test_cloud_handoff_prefix_normal_deletion_does_not_exit`
- `test_cloud_handoff_prefix_exits_on_backspace_at_beginning_of_buffer`
- `test_cloud_handoff_prefix_keeps_shell_prefix_as_query_text`
- `test_cloud_handoff_prefix_escape_exits_mode_preserving_prompt_text`
- `test_cloud_handoff_prefix_remains_text_in_powershell_with_nld_enabled`
- `test_cloud_handoff_prefix_activates_in_powershell_when_nld_disabled`
- `test_cloud_handoff_prefix_vim_escape_exits_insert_before_handoff_mode`
- `test_cloud_handoff_prefix_ignores_terminal_input_mode_toggle`
- `test_terminal_prefix_sets_shell_prefix_decision_source`
- `test_source_less_locked_config_clears_decision_source`
- `enter_submits_when_submit_on_ctrl_enter_is_false`
- `ctrl_enter_emits_ctrl_enter_event_when_submit_on_ctrl_enter_is_false`
- `enter_inserts_newline_when_submit_on_ctrl_enter_is_true`
- `ctrl_enter_submits_when_submit_on_ctrl_enter_is_true`
- `ctrl_enter_with_selection_preserves_selection_in_submit_when_setting_is_true`
- `editor_keymap_context_excludes_ctrl_enter_enters_agent_view_when_rich_input_is_open`
- `enter_accepts_inline_menu_item_when_submit_on_ctrl_enter_is_true`
- `ctrl_enter_inserts_newline_when_submit_on_ctrl_enter_is_false`
- `unfreeze_agent_input_does_not_clear_buffer`
- `ctrl_enter_inserts_newline_in_normal_input_after_rich_input_closes`

### `app/src/terminal/local_tty/unix.rs` — A, 3 missing

- `parse_passwd_line_extracts_matching_uid`
- `parse_passwd_line_matches_glibc_edge_cases`
- `resolve_current_user_returns_running_user`

### `app/src/terminal/model/ansi/mod_tests.rs` — A, 22 missing

- `parse_dcs_unregistered_session_id_rejected`
- `parse_dcs_unregistered_session_id_allowed_when_validation_disabled`
- `parse_dcs_init_shell_7bit_st`
- `parse_osc7_local_hostname`
- `parse_osc7_with_st_terminator`
- `parse_osc7_percent_encoded`
- `parse_osc7_path_with_unescaped_semicolons_preserved`
- `parse_osc7_empty_host_ignored`
- `parse_osc7_localhost_host_ignored`
- `parse_osc7_uppercase_localhost_host_ignored`
- `parse_osc7_non_local_host_ignored`
- `parse_osc7_non_file_scheme_ignored`
- `parse_osc7_missing_path_ignored`
- `parse_osc7_malformed_percent_escape_ignored`
- `parse_osc7_truncated_percent_at_end_ignored`
- `parse_osc7_truncated_percent_with_one_hex_digit_ignored`
- `parse_osc7_empty_payload_ignored`
- `parse_osc7_windows_drive_letter_normalized`
- `parse_osc7_windows_drive_letter_root`
- `parse_osc7_windows_drive_letter_percent_encoded`
- `parse_osc7_posix_path_not_mangled_non_windows`
- `parse_osc7_non_drive_slash_letter_untouched`

### `app/src/terminal/model/block_tests.rs` — A, 8 missing

- `test_image_completion_before_execution_routes_to_output_grid`
- `test_image_completion_drops_in_warp_input_stage`
- `test_set_current_working_directory_updates_pwd_and_emits_cwd_event`
- `test_elapsed_duration_rounds_down_to_whole_seconds`
- `test_elapsed_duration_requires_executing_state`
- `test_elapsed_duration_for_background_block_is_not_live`
- `test_command_and_output_to_string_includes_ps1_prompt_command_rprompt_and_output`
- `test_command_and_output_to_string_excludes_warp_prompt`

### `app/src/terminal/model/blockgrid_tests.rs` — A, 1 missing

- `test_non_moving_kitty_image_keeps_finished_grid_visible`

### `app/src/terminal/model/blocks_tests.rs` — A, 11 missing

- `test_iterm_image_renders_in_script_execution_block`
- `test_invalid_iterm_image_does_not_render_in_script_execution_block`
- `test_kitty_image_renders_in_script_execution_block`
- `test_kitty_store_only_does_not_render_in_script_execution_block`
- `test_iterm_image_early_output_routes_to_background_block`
- `test_kitty_image_early_output_routes_to_background_block`
- `test_kitty_store_only_early_output_does_not_create_background_block`
- `test_zero_sized_kitty_early_output_does_not_create_background_block`
- `visible_bootstrap_block_event_fires_when_script_execution_becomes_visible`
- `unfiltered_transcript_scope_shows_restored_conversation_command_blocks`
- `test_finish_startup_commands_at_block_attaches_and_unhides_command_blocks_since_target_block`

### `app/src/terminal/model/grid/grid_handler_tests.rs` — A, 11 missing

- `test_possible_file_paths_candidate_count_is_bounded`
- `test_clear_screen_all_primary_preserves_visible_rows_in_history_by_default`
- `test_clear_screen_all_primary_with_full_grid_clear_behavior_clears_in_place`
- `test_clear_screen_all_alt_screen_clears_in_place`
- `test_resize_primary_preserves_visible_rows_in_history_by_default`
- `test_resize_primary_with_full_grid_clear_behavior_keeps_visible_rows_in_place`
- `test_resize_finished_primary_with_full_grid_clear_behavior_uses_scrollback`
- `test_full_grid_clear_resize_then_scroll_does_not_panic_on_row_iteration`
- `test_full_grid_clear_resize_narrower_then_scroll_does_not_panic`
- `test_full_grid_clear_shrink_cols_does_not_orphan_wide_char_at_boundary`
- `test_full_grid_clear_resize_then_bounds_to_string_does_not_panic`

### `app/src/terminal/model/grid/secrets_tests.rs` — A, 2 missing

- `test_secret_redacted_after_multibyte_prefix`
- `test_secret_with_word_boundaries_redacted_after_multibyte_prefix`

### `app/src/terminal/model/grid/tests.rs` — A, 1 missing

- `test_shrink_cols_reflow_preserves_split_wide_char_as_wrapped_content`

### `app/src/terminal/model/lifecycle/mod_tests.rs` — A, 1 missing

- `lifecycle_telemetry_payload_is_allowlisted_and_non_ugc`

### `app/src/terminal/model/session_tests.rs` — A, 4 missing

- `can_resolve_cwd_to_native_path_accepts_posix_path`
- `can_resolve_cwd_to_native_path_accepts_windows_drive_path`
- `can_resolve_cwd_to_native_path_rejects_unix_encoded_path_on_windows`
- `powershell_read_command_embeds_escaped_path_without_args`

### `app/src/terminal/model/terminal_model_tests.rs` — A, 23 missing

- `cloud_mode_deferred_terminal_model_starts_view_pending`
- `generic_shared_session_viewer_model_starts_view_pending`
- `is_cloud_agent_conversation_only_true_for_genuine_ambient_sessions`
- `ignores_non_inline_iterm_file_payload_without_overwriting_cwd_file`
- `ignores_multipart_non_inline_iterm_file_payload_without_overwriting_cwd_file`
- `handles_inline_iterm_image_payload`
- `ssh_bootstraps_if_blocklist_empty_and_reconciles_parent_return`
- `accepted_precmd_and_preexec_target_the_block_list_while_the_alt_screen_is_active`
- `normal_lifecycle_pipeline_emits_completion_and_prompt_side_effects_once`
- `precmd_with_completion_metadata_records_completion_mismatch_without_overwriting_completed_block`
- `precmd_with_completion_metadata_recovers_in_band_completion_and_reuses_cached_prompt`
- `empty_and_syntax_error_commands_without_preexec_complete_as_execution`
- `command_finished_recovers_unknown_started_block_with_real_exit_code`
- `recovery_advances_finished_active_block_without_republishing_completion`
- `repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored`
- `repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored_when_recovery_is_disabled`
- `repeated_and_executing_command_starts_are_safely_gated`
- `duplicate_and_colliding_completion_evidence_is_ignored`
- `terminal_exit_absorbs_later_lifecycle_inputs`
- `viewer_processes_dcs_hook_with_unregistered_session_id`
- `sharer_rejects_dcs_hook_with_unregistered_session_id`
- `cloud_mode_setup_phase_ended_emits_when_sharing`
- `cloud_mode_setup_phase_ended_does_not_emit_when_not_sharing`

### `app/src/terminal/share_block_modal_tests.rs` — C, 2 missing

- `escape_html_attribute_escapes_attribute_breakout_characters`
- `escape_html_attribute_leaves_safe_text_unchanged`

### `app/src/terminal/shared_session/mod_tests.rs` — A, 1 missing

- `shared_session_viewer_recovers_from_raw_precmd_with_completion_metadata_without_ordered_hint`

### `app/src/terminal/shared_session/network/heartbeat_tests.rs` — D, 2 missing

- `test_periodic_ping`
- `test_idle_timeout`

### `app/src/terminal/shared_session/presence_manager_tests.rs` — A, 2 missing

- `single_distinct_present_viewer_uid_filters_absent_duplicates`
- `single_distinct_present_viewer_uid_returns_none_for_zero_or_multiple_uids`

### `app/src/terminal/shared_session/share_modal/body_tests.rs` — C, 5 missing

- `test_open_modal_from_non_block`
- `test_open_modal_from_block`
- `test_open_modal_from_non_block_disabled`
- `test_open_modal_from_block_disabled`
- `test_open_modal_from_long_running_block`

### `app/src/terminal/shared_session/sharer/network_tests.rs` — C, 17 missing

- `test_startup_max_attempts_only_retries_ambient_agent_sources`
- `test_startup_failure_retryability`
- `test_should_retry_startup_failure_respects_attempt_budget`
- `test_startup_attempt_stale_filtering`
- `test_send_ordered_terminal_event_message_advances_event_no`
- `test_send_ordered_terminal_event_message_max_reached`
- `test_send_pty_read_event_while_batching`
- `test_send_pty_read_event_while_not_batching`
- `test_handle_pty_read_event_while_batching`
- `test_handle_pty_read_event_while_not_batching`
- `test_handle_non_pty_read_event_while_batching`
- `test_handle_non_pty_read_event_while_not_batching`
- `test_ignore_duplicate_prompt_updates`
- `test_selection_updates_throttled_and_duplicates_ignored`
- `test_messages_are_buffered_before_session_initialized`
- `test_messages_are_buffered_while_reconnecting`
- `test_events_are_saved_on_send_and_removed_on_ack`

### `app/src/terminal/shared_session/viewer/event_loop_tests.rs` — C, 13 missing

- `test_terminal_model_is_correct`
- `new_viewer_processes_old_sharer_lifecycle_stream`
- `test_append_followup_scrollback_skips_duplicates`
- `test_append_followup_scrollback_with_completed_last_block_creates_active_block`
- `test_append_followup_replay_marks_existing_conversations_suppressible`
- `test_fresh_session_replay_does_not_suppress_existing_conversations`
- `test_out_of_order_buffering`
- `command_execution_finished_defers_queued_command_advance_until_block_completion`
- `command_execution_started_preserves_draft_for_queued_command`
- `test_pty_bytes_buffered_before_command_execution_started`
- `test_cloud_mode_setup_phase_ended_clears_setup_state`
- `test_cloud_mode_setup_phase_ended_when_flag_already_false`
- `test_cloud_mode_setup_phase_ended_is_idempotent`

### `app/src/terminal/shared_session/viewer/network_tests.rs` — C, 3 missing

- `test_send_pty_write_event_advances_event_no`
- `test_send_pty_write_event_while_batching`
- `test_send_pty_write_event_while_not_batching`

### `app/src/terminal/shared_session/viewer/orchestration_viewer_model_tests.rs` — C, 34 missing

- `maps_working_states_to_in_progress`
- `maps_succeeded_to_success`
- `maps_failed_and_error_to_error`
- `maps_blocked_to_blocked`
- `maps_cancelled_to_cancelled`
- `unknown_state_maps_to_error`
- `registers_new_child_conversation`
- `skips_parent_task_id_as_child`
- `skips_child_when_no_active_parent_conversation`
- `updates_status_on_state_change`
- `materialization_requested_only_once_per_child`
- `materialization_gate_flips_on_session_id_transition`
- `registers_multiple_children`
- `registers_child_agent_name_from_snapshot_name`
- `registers_child_agent_name_falls_back_to_title_when_snapshot_name_is_missing`
- `registers_child_agent_name_does_not_set_fallback_for_whitespace_only_title`
- `registers_child_agent_name_uses_literal_agent_when_both_are_empty`
- `registers_child_agent_name_trims_whitespace`
- `child_status_changed_with_unknown_run_id_is_silently_dropped`
- `child_status_changed_updates_existing_placeholder_via_local_map`
- `child_status_changed_refetches_metadata_while_session_id_is_pending`
- `pending_session_id_poll_schedules_while_session_id_is_none`
- `pending_session_id_poll_does_not_schedule_when_no_children_pending`
- `pending_session_id_poll_dispatches_per_pending_child`
- `child_status_changed_does_not_refetch_when_already_materialized`
- `b1_populates_agent_id_to_conversation_id_for_new_child`
- `b2_backfills_parent_agent_id_on_orchestrator_token_assigned`
- `b2_does_not_overwrite_existing_parent_agent_id`
- `b2_ignores_token_assigned_for_unrelated_conversation`
- `handle_streamer_event_filters_on_parent_task_id`
- `child_spawned_with_malformed_run_id_is_dropped`
- `streamer_consumer_is_registered_when_constructed`
- `viewer_model_retries_consumer_registration_on_set_active_conversation`
- `viewer_model_does_not_register_when_active_conversation_is_a_child_placeholder`

### `app/src/terminal/shared_session/viewer/terminal_manager_tests.rs` — C, 5 missing

- `command_execution_request_failed_clears_queued_command_in_flight`
- `on_view_detached_closed_clears_orchestration_viewer_model_slot`
- `on_view_detached_hidden_for_close_keeps_orchestration_viewer_model_alive`
- `on_view_detached_moved_keeps_orchestration_viewer_model_alive`
- `handle_viewer_session_end_ignores_stale_ambient_end`

### `app/src/terminal/view/ambient_agent/block/setup_command_text_tests.rs` — D, 2 missing

- `setup_command_groups_have_independent_visibility`
- `setup_command_groups_track_running_group_independently`

### `app/src/terminal/view/ambient_agent/model_tests.rs` — C, 13 missing

- `record_ambient_execution_ended_clears_active_session_and_enables_followup`
- `spawn_config_falls_back_to_auto_only_for_non_cloud_runnable_model`
- `spawn_config_honors_pane_model_override`
- `spawn_agent_omits_orchestration_handoff_for_fresh_launches`
- `duplicate_handoff_completion_is_ignored`
- `handoff_cancellation_is_signalled_and_late_failure_is_ignored`
- `record_ambient_execution_ended_keeps_active_session_when_id_differs`
- `set_live_execution_session_marks_session_live_until_it_ends`
- `github_auth_url_for_initial_run_includes_focus_cloud_mode_next`
- `github_auth_completed_retries_stored_initial_run_request`
- `viewed_task_config_preserves_environment_before_cloud_model_load`
- `viewed_task_config_applies_oz_model_override`
- `followup_github_auth_does_not_reuse_stored_initial_request`

### `app/src/terminal/view/queued_prompts_tests.rs` — A, 28 missing

- `dispatched_cloud_prompt_uses_locked_queue_row_when_v2_is_enabled`
- `dispatched_cloud_followup_uses_locked_queue_row_when_v2_is_enabled`
- `cloud_setup_cleanup_events_remove_the_locked_queue_row`
- `failed_event_keeps_locked_queue_row_under_cloud_mode_setup_v2`
- `failed_event_removes_locked_queue_row_without_cloud_mode_setup_v2`
- `cloud_setup_enter_queues_followup_input_when_v2_is_enabled`
- `cloud_setup_enter_does_not_queue_followup_for_third_party_harness`
- `cloud_setup_enter_queues_followup_while_setup_commands_run`
- `cloud_setup_enter_remains_blocked_when_v2_is_disabled`
- `terminal_cloud_status_transition_drains_once_through_cloud_followup_input_event`
- `promptless_setup_complete_auto_sends_queued_prompt_to_viewer`
- `promptless_setup_complete_with_initial_prompt_does_not_drain_queue`
- `commit_edit_saves_current_editor_text_for_lrc_row`
- `lrc_finish_commits_edited_lrc_row_before_sending`
- `lrc_finish_queued_compact_and_sends_followup_after_summary`
- `enqueue_followup_prompt_appends_compact_and_row_when_v2_is_enabled`
- `enqueue_followup_prompt_appends_fork_and_compact_row_when_v2_is_enabled`
- `enqueue_followup_prompt_uses_supplied_conversation_id_when_v2_is_enabled`
- `enqueue_followup_prompt_falls_back_to_pending_block_when_v2_is_disabled`
- `send_now_action_emits_row_kind_and_leaves_rows_for_host_to_fire`
- `send_now_disabled_for_all_rows_while_initial_cloud_mode_row_is_present`
- `copying_locked_initial_cloud_mode_prompt_copies_full_prompt_to_clipboard`
- `redetermine_terminal_focus_preserves_focused_queued_prompt_editor`
- `can_send_prompt_gates_buttons_and_hint_while_nonempty_input_gates_only_the_hint`
- `enter_hint_hidden_during_inline_edit_and_for_locked_head`
- `multi_cycle_queue_keeps_each_rows_attachments_independent`
- `finish_reason_is_scoped_to_the_finished_conversation`
- `finished_receiving_output_drains_queue_when_sibling_block_masks_turn_end`

### `app/src/terminal/view/shared_session/cloud_conversation_continuation_tests.rs` — C, 23 missing

- `missing_task_returns_error`
- `github_action_source_shows_tombstone_without_cta`
- `oz_conversation_with_edit_access_shows_inline_followup_input`
- `oz_conversation_with_view_access_shows_continue_locally_tombstone`
- `third_party_conversation_with_edit_access_shows_continue_in_cloud_tombstone`
- `environment_setup_failure_without_conversation_shows_tombstone_without_cta`
- `environment_setup_failure_with_conversation_shows_continue_cta`
- `third_party_conversation_created_by_current_user_shows_continue_in_cloud_tombstone`
- `third_party_conversation_owned_by_current_team_shows_continue_in_cloud_tombstone`
- `third_party_conversation_shared_with_current_team_as_editor_shows_continue_in_cloud_tombstone`
- `third_party_conversation_with_view_access_shows_tombstone_without_cta`
- `unknown_access_returns_error`
- `missing_metadata_returns_error`
- `owned_oz_task_without_metadata_shows_inline_followup_input`
- `owned_third_party_task_without_metadata_shows_continue_in_cloud_tombstone`
- `active_task_execution_returns_error`
- `routing_is_local_for_non_cloud_pane`
- `routing_is_live_remote_vm_for_active_viewer`
- `routing_omits_task_id_for_non_ambient_shared_session_viewer`
- `routing_is_local_for_active_sharer_local_orchestration_child`
- `routing_is_new_cloud_vm_for_owned_oz_disconnected_pane`
- `routing_is_read_only_for_non_owner_disconnected_pane`
- `routing_is_live_remote_vm_for_active_execution_without_attached_viewer`

### `app/src/terminal/view/shared_session/conversation_ended_tombstone_view_tests.rs` — A, 1 missing

- `task_failure_status_message_overrides_conversation_error`

### `app/src/terminal/view/shared_session/view_impl_tests.rs` — C, 36 missing

- `test_on_ambient_agent_execution_ended_enables_followup_input_for_editable_non_owner_finished_view`
- `test_begin_viewing_ambient_session_creates_and_wires_model_for_link_join_viewer`
- `test_begin_viewing_ambient_session_emits_view_model_created_event_once`
- `test_begin_viewing_ambient_session_reuses_existing_model_for_cloud_pane`
- `test_on_session_share_ended_does_not_insert_tombstone_for_ambient_session_under_cloud_mode_setup_v2`
- `test_on_session_share_ended_skips_cloud_continuation_for_user_share_with_task_id`
- `test_conversation_details_auto_open_policy_defaults_to_open_for_ambient_shared_session`
- `test_suppressed_conversation_details_auto_open_consumes_initial_open_but_manual_toggle_works`
- `test_child_shared_session_link_keeps_default_conversation_details_auto_open`
- `test_ambient_session_join_auto_opens_details_panel`
- `test_local_to_cloud_handoff_session_join_keeps_details_panel_hidden`
- `test_cloud_cloud_handoff_session_join_keeps_closed_details_panel_hidden`
- `test_cloud_cloud_handoff_session_join_respects_details_panel_closed_after_followup_input`
- `test_restored_ambient_view_resolves_cta_from_view_model_task_id`
- `test_continue_in_cloud_tombstone_routes_third_party_followup_to_new_cloud_vm`
- `test_restored_oz_edit_access_non_owner_finished_view_uses_followup_input_without_tombstone`
- `test_on_session_share_ended_enables_followup_input_without_tombstone_for_owned_ambient_session`
- `test_on_session_share_ended_shows_tombstone_for_github_action_ambient_session`
- `test_on_session_share_ended_hides_input_for_no_cta_tombstone`
- `test_on_session_share_ended_does_not_insert_tombstone_for_owned_ambient_session_without_handoff`
- `test_on_session_share_ended_clears_frozen_followup_input_for_owned_ambient_session`
- `test_on_session_share_ended_does_not_insert_tombstone_for_non_ambient_session_under_cloud_mode_setup_v2`
- `test_on_ambient_agent_execution_ended_inserts_tombstone_when_handoff_enabled`
- `test_on_ambient_agent_execution_ended_enables_followup_for_owned_task_without_metadata`
- `test_on_ambient_agent_execution_ended_shows_tombstone_for_github_action_ambient_session`
- `test_on_ambient_agent_execution_ended_enables_followup_input_without_tombstone_for_owned_task`
- `test_restored_owned_tombstone_hides_input_until_continue`
- `test_deep_linked_ambient_continuation_refreshes_when_task_data_arrives`
- `test_on_ambient_agent_execution_ended_keeps_live_owned_session_on_session_sharing_path`
- `test_try_submit_pending_cloud_followup_allows_repeat_submission_for_owned_task`
- `test_try_submit_pending_cloud_followup_rejects_task_source_that_blocks_followups`
- `test_shared_followup_on_existing_conversation_converts_user_query_input`
- `test_non_owned_tombstone_is_removed_for_followup_and_reinserted_after_completion`
- `test_on_ambient_agent_execution_ended_refreshes_open_details_panel_to_terminal_status`
- `test_on_ambient_agent_execution_ended_inserts_tombstone_without_handoff`
- `passive_suggestions_suppressed_for_shared_ambient_viewer`

### `app/src/terminal/view/use_agent_footer/mod_tests.rs` — A, 6 missing

- `use_agent_footer_hidden_during_cloud_agent_setup_lrc`
- `cli_agent_footer_renders_for_viewer_of_shared_cloud_agent_session`
- `cli_agent_footer_does_not_render_for_warp_tui_session`
- `test_rich_input_submit_strategy_for_oh_my_pi`
- `test_rich_input_submit_strategy_for_hermes_uses_bracketed_paste`
- `insert_cli_agent_voice_text_hermes_multiline_uses_bracketed_paste_without_submitting`

### `app/src/terminal/view_tests.rs` — A, 55 missing

- `agent_view_lifecycle_updates_input_mode`
- `updated_conversation_metadata_refreshes_selected_conversation_pane_title`
- `jump_to_latest_agent_message_no_ops_when_agent_view_disabled`
- `jump_to_latest_agent_message_no_ops_without_conversations`
- `jump_to_latest_agent_message_enters_agent_view_and_records_pending_scroll`
- `jump_to_latest_agent_message_targets_latest_visible_exchange`
- `jump_to_latest_agent_message_scrolls_without_re_entering_when_already_in_view`
- `restoring_conversation_to_new_pane_transfers_blocks_from_previous_terminal_surface`
- `clicking_old_banner_for_open_conversation_focuses_current_terminal_surface_without_transferring_blocks`
- `appended_exchange_renders_in_current_terminal_surface_after_conversation_transfer`
- `escape_pops_nested_cloud_agent_view_with_long_running_command`
- `escape_does_not_exit_root_cloud_agent_view_with_long_running_command`
- `root_cloud_mode_pane_sets_root_cloud_mode_context_key`
- `set_input_mode_agent_does_not_enter_local_agent_from_root_cloud_mode_pane`
- `cloud_mode_v1_agent_prefixed_query_spawns_cloud_agent`
- `cloud_mode_v2_agent_prefixed_query_spawns_cloud_agent`
- `fresh_cloud_mode_setup_enters_agent_view_when_view_pending`
- `shared_third_party_viewer_sync_enters_agent_view_and_retags_existing_block`
- `shared_third_party_viewer_syncs_from_viewer_harness_updated_when_harness_unchanged`
- `shared_third_party_viewer_syncs_from_cli_agent_state_without_ambient_model`
- `cloud_mode_followup_input_uses_explicit_submit_event_even_when_view_pending`
- `pending_cloud_followup_without_ambient_model_restores_prompt`
- `cloud_mode_dispatched_agent_inserts_queued_user_query`
- `cloud_mode_failed_keeps_queued_query_above_tombstone_and_hides_input`
- `cmd_enter_from_terminal_without_selected_block_enters_agent_view`
- `cmd_enter_from_terminal_with_selected_block_enters_agent_view_with_context`
- `cmd_enter_from_active_non_empty_agent_view_requires_confirmation`
- `cloud_mode_followup_dispatched_inserts_queued_user_query`
- `cloud_mode_setup_v2_suppresses_sharer_input_updates_while_followup_setup_commands_run`
- `pending_cloud_mode_query_waits_for_renderable_user_query_exchange`
- `pending_cloud_mode_query_clears_when_streaming_exchange_becomes_renderable`
- `test_context_menu_includes_clear_when_block_list_non_empty`
- `test_context_menu_omits_clear_when_block_list_empty`
- `test_context_menu_omits_clear_for_text_right_click`
- `test_control_master_banner_permanent_dismissal_persists`
- `test_control_master_banner_suppressed_does_not_reopen`
- `ctrl_g_closes_cli_agent_rich_input_when_editor_is_focused`
- `ctrl_g_closes_cli_agent_rich_input_from_terminal_context`
- `ctrl_g_toggles_cli_agent_rich_input_from_terminal_context`
- `submit_cli_agent_rich_input_hermes_multiline_uses_bracketed_paste`
- `attach_path_as_context_routes_to_open_cli_agent_rich_input`
- `drag_drop_image_in_cli_agent_long_running_command_pastes_via_clipboard`
- `paste_raw_image_clipboard_in_cli_agent_sends_correct_bytes`
- `codex_status_change_does_not_auto_open_rich_input`
- `cli_session_status_updates_single_child_conversation_without_agent_view`
- `close_find_bar_clears_ai_block_find_highlights`
- `close_find_bar_preserves_options_on_async_find_path`
- `copy_selected_text_from_ai_block`
- `cmd_k_does_not_clear_buffer_when_agent_is_driving_command`
- `cmd_k_in_agent_view_clears_active_block_not_full_buffer_when_agent_driving_command`
- `cmd_k_in_agent_view_cancels_in_progress_conversation_and_starts_new_one`
- `active_cli_agent_recognizes_detected_warp_tui_session`
- `active_cli_agent_ignores_warp_tui_when_hoa_code_review_disabled`
- `active_cli_agent_ignores_non_tui_long_running_command`
- `send_review_comments_to_warp_tui_writes_prompt_to_pty`

### `app/src/terminal/warpify/settings_tests.rs` — A, 4 missing

- `test_enable_ssh_wrapper_false_migrates_to_enable_ssh_warpification_false`
- `test_deprecated_ssh_wrapper_migration_triggers_are_not_synced`
- `test_legacy_wrapper_migration_is_one_time_and_preserves_reenabled_warpification`
- `test_enable_ssh_wrapper_default_does_not_affect_enable_ssh_warpification`

### `app/src/terminal/writeable_pty/pty_controller_lifecycle_tests.rs` — A, 2 missing

- `rejected_and_coalesced_starts_do_not_mutate_controller_or_write_bytes`
- `rejected_queued_in_band_start_is_cancelled_without_writing_bytes`

### `crates/warp_terminal/src/model/escape_sequences_tests.rs` — A, 2 missing

- `test_alt_screen_scroll_to_pty_bytes`
- `test_to_pty_bytes_layers_fallbacks_over_the_encoder`

### `crates/warp_tui/src/agent_block_tests.rs` — A, 6 missing

- `failed_output_usage_notice_matches_gui_conditions`
- `orchestration_outputs_render_without_wait_for_events_tool_row`
- `hidden_only_orchestration_exchange_has_zero_height`
- `agent_block_preserves_received_messages_and_hides_lifecycle_ids`
- `agent_message_defaults_collapsed_and_expands_through_block_state`
- `streaming_conversation_summary_renders_collapsed_by_default`

### `crates/warp_tui/src/agent_message_tests.rs` — C, 4 missing

- `parent_sender_renders_as_orchestrator_in_child_transcript`
- `conversation_statuses_render_expected_glyphs`
- `running_child_message_matches_the_design_layout_and_styles`
- `message_preview_wraps_with_a_hanging_indent_and_falls_back_to_subject`

### `crates/warp_tui/src/cloud_run_view_tests.rs` — C, 2 missing

- `lightweight_cloud_view_renders_startup_and_blocker_without_terminal_state`
- `spawned_cloud_view_matches_figma_in_progress_and_succeeded_states`

### `crates/warp_tui/src/completion_menu_tests.rs` — D, 2 missing  **[FALSE POSITIVE — verified 2026-08-10: the fork's `completions_menu.rs` IS this component, renamed. Same list state, same try_open/close gating, same inline-menu wiring, same completer engine. Zero test-name overlap was renaming, not absence. Do not port; -2 from the debt count.]**

- `show_reuses_inline_menu_rows_and_accepts_the_selected_span`
- `show_does_not_replace_an_existing_inline_menu`

### `crates/warp_tui/src/editor_view_tests.rs` — A, 1 missing

- `selection_end_without_copy_on_mouse_highlight_is_not_copied`

### `crates/warp_tui/src/grok_oauth/tests.rs` — D, 3 missing

- `waiting_card_uses_handoff_structure_and_only_escape_footer_hint`
- `callback_and_manual_failures_do_not_claim_success_or_expose_raw_details`
- `fatal_card_sanitizes_the_body_and_escape_closes_the_attempt`

### `crates/warp_tui/src/handoff/model_tests.rs` — C, 1 missing

- `missing_token_after_eager_cancellation_restores_only_trimmed_argument`

### `crates/warp_tui/src/handoff/tests.rs` — C, 5 missing

- `slash_menu_selection_inserts_handoff_for_optional_prompt_composition`
- `no_environment_card_has_top_padding_and_ctrl_c_restores_prompt_and_images`
- `settings_invalidation_restores_the_draft_and_repeated_submission_keeps_one_card`
- `privacy_invalidation_restores_the_draft_and_removes_handoff_from_commands`
- `long_running_command_rejection_preserves_the_full_local_draft`

### `crates/warp_tui/src/input/view_tests.rs` — A, 22 missing

- `tab_cycles_open_completion_menu_and_enter_applies_selection`
- `tab_requests_completion_for_detected_shell_input`
- `tab_requests_completion_only_in_shell_mode_without_submitting`
- `tab_is_consumed_by_an_existing_non_completion_menu`
- `completion_replaces_utf8_byte_span_and_preserves_following_text`
- `completion_can_append_a_space_at_buffer_end`
- `enter_and_escape_stop_listening_while_escape_cancels_transcribing`
- `agent_mode_render_has_prompt_gutter`
- `listening_voice_input_suppresses_shell_gutter`
- `move_left_from_shortcuts_replaces_it_with_conversation_menu`
- `question_mark_at_empty_agent_input_toggles_shortcuts`
- `question_mark_at_empty_shell_input_toggles_shortcuts`
- `escape_closes_shortcuts_before_exiting_shell_mode`
- `typing_into_an_open_shortcuts_surface_closes_it_and_inserts`
- `up_on_first_row_opens_prompt_and_command_history_menu`
- `up_from_shortcuts_replaces_it_with_prompt_and_command_history`
- `up_on_lower_row_moves_cursor_without_opening_prompt_and_command_history`
- `shell_mode_up_opens_command_only_history`
- `escape_closes_prompt_and_command_history_and_restores_typed_buffer`
- `blur_closes_prompt_and_command_history_and_restores_text_and_input_type`
- `submit_accepts_highlighted_history_entry_with_its_kind`
- `submit_accepts_highlighted_command_history_entry`

### `crates/warp_tui/src/link_tests.rs` — A, 1 missing

- `link_row_in_stretched_banner_only_underlines_the_link_text`

### `crates/warp_tui/src/option_selector_tests.rs` — A, 1 missing

- `selected_custom_answer_number_is_not_highlighted_after_the_cursor_moves_away`

### `crates/warp_tui/src/orchestrated_agent_identity_styling_tests.rs` — D, 9 missing

- `palette_crosses_the_seven_design_glyphs_and_colors`
- `palette_uses_the_themed_design_color_roles_in_order`
- `palette_entries_are_distinct_glyph_color_pairs`
- `stable_hash_is_deterministic_and_name_sensitive`
- `assignment_is_deterministic_across_calls`
- `assignment_keeps_identities_distinct_within_one_request`
- `assignment_keeps_glyphs_and_colors_unique_until_exhausted`
- `assignment_cycles_deterministically_beyond_palette_exhaustion`
- `assignment_handles_an_empty_palette`

### `crates/warp_tui/src/orchestration_block_tests.rs` — C, 20 missing

- `environment_selector_is_searchable`
- `environment_and_model_pages_are_searchable`
- `local_collapses_the_page_sequence_to_two_pages`
- `cloud_oz_uses_five_pages_without_the_api_key_page`
- `cloud_managed_credential_harness_inserts_the_api_key_page`
- `edit_state_carries_the_request_auth_secret`
- `edit_state_is_overridden_by_an_approved_config`
- `unapproved_local_request_forces_oz_harness`
- `local_request_with_implicit_oz_harness_preserves_explicit_model`
- `build_request_carries_card_fields_and_edited_run_wide_state`
- `build_request_omits_the_auth_secret_when_the_picker_is_not_applicable`
- `selector_layout_invalidations_are_forwarded`
- `selector_actions_commit_edits_and_follow_the_dynamic_page_sequence`
- `focusing_a_configuring_card_delegates_to_the_selector`
- `opening_configuration_only_invalidates_layout`
- `model_selector_arrows_navigate_after_search_takes_focus`
- `blocked_accept_invalidates_card_layout`
- `failed_arrow_confirmation_does_not_change_later_enter_navigation`
- `confirming_a_search_result_returns_focus_to_the_acceptance_card`
- `accepting_dispatches_once_and_releases_focus`

### `crates/warp_tui/src/orchestration_model_tests.rs` — C, 5 missing

- `local_harness_children_fail_cleanly`
- `github_auth_blocker_keeps_the_remote_session_and_actionable_url`
- `snapshot_is_shared_across_tree_and_filters_conversations_without_sessions`
- `remote_child_session_is_navigable_and_projects_lifecycle`
- `failed_launch_cleanup_preserves_other_sessions`

### `crates/warp_tui/src/prompt_and_command_history_menu_tests.rs` — D, 12 missing

- `agent_mode_combines_ordered_deduped_prompts_and_commands`
- `shell_mode_excludes_prompts_and_previews_commands`
- `command_history_initialization_refreshes_an_open_menu`
- `prompt_and_command_with_same_text_remain_distinct`
- `prefix_filter_matches_any_line_without_changing_source_text`
- `selection_preview_switches_input_type_and_dismiss_restores_both`
- `accepting_selected_item_returns_its_kind`
- `accepting_without_a_match_uses_the_current_input_type`
- `empty_and_filtered_empty_states_use_history_copy`
- `command_prefix_matches_the_row_text_style`
- `multiline_history_title_handles_windows_line_endings`
- `reconciled_selection_preserves_full_row_identity`

### `crates/warp_tui/src/read_only_menu_tests.rs` — D, 6 missing

- `visual_rows_own_the_full_width_background`
- `background_fills_available_width_under_loose_constraints`
- `background_fills_available_width_through_session_style_wrapper`
- `selection_spans_section_titles_and_rows`
- `selection_stops_at_trailing_whitespace`
- `double_click_selects_complete_styled_text`

### `crates/warp_tui/src/session_tests.rs` — A, 6 missing

- `parses_provider_api_key_setup_flag`
- `parses_provider_api_key_clear_flag`
- `rejects_unknown_provider_api_key_setup_value`
- `provider_api_key_flags_are_mutually_exclusive`
- `provider_api_key_help_lists_supported_providers`
- `version_flag_prints_cli_version`

### `crates/warp_tui/src/slash_commands_tests.rs` — A, 4 missing

- `slash_command_menu_renders_voice_row`
- `slash_command_menu_renders_auto_approve_row`
- `slash_command_menu_renders_natural_language_detection_row`
- `slash_command_menu_renders_theme_row`

### `crates/warp_tui/src/terminal_session_view/completions_tests.rs` — D, 3 missing

- `common_prefix_extends_only_the_current_backend_span`
- `completion_requests_reject_every_stale_snapshot_dimension`
- `common_prefix_rejects_invalid_utf8_or_out_of_bounds_spans`

### `crates/warp_tui/src/terminal_session_view/state_tests.rs` — D, 10 missing

- `tagged_in_composer_exposes_detach_shortcut`
- `resolve_returns_error_after_terminal_model_owner_drops`
- `shell_hint_is_selected_with_additive_orchestration`
- `transcript_state_selects_the_applicable_hint_segments`
- `only_composer_interactions_produce_input_hints`
- `hierarchy_encodes_input_ownership`
- `alt_screen_can_retain_an_agent_composer`
- `shell_and_orchestration_contribute_active_shortcut_sections`
- `agent_terminal_use_and_orchestration_are_additive`
- `user_controlled_terminal_use_has_terminal_only_shortcuts`

### `crates/warp_tui/src/terminal_session_view_tests.rs` — A, 57 missing

- `figma_statusline_metadata_formats_are_stable`
- `statusline_datetime_requests_a_periodic_repaint`
- `footer_supports_arbitrary_order_and_figma_group_dividers`
- `footer_uses_pipes_between_figma_groups_and_preserves_within_group_separators`
- `shell_mode_reserves_tab_even_when_attachments_render`
- `nld_reset_only_unlocks_after_agent_control_and_not_on_user_edit`
- `voice_accepts_exact_and_whitespace_only_arguments`
- `voice_slash_command_rejects_arguments_before_prompt_fallback`
- `tui_cli_shell_command_uses_channel_entry_points`
- `provider_api_key_shell_command_uses_shared_tui_launcher`
- `grok_oauth_block_exclusively_owns_input_until_escape`
- `zero_state_reload_failure_renders_as_an_error_footer_hint`
- `theme_slash_command_accepts_direct_selection_and_rejects_invalid_values`
- `zero_state_initial_load_failure_shows_an_error_footer_hint`
- `listening_voice_input_animates_the_input_border`
- `auto_approve_slash_command_toggles_selected_conversation_off_on_off`
- `theme_slash_command_rejects_a_missing_argument`
- `cost_slash_command_rejects_an_empty_conversation_like_the_gui`
- `response_summary_visibility_is_independent_from_the_footer_usage_mode`
- `auto_approve_actions_control_visible_feedback`
- `shortcuts_surface_renders_above_the_input`
- `nld_slash_command_toggles_and_reports_its_effects`
- `status_slash_command_opens_dedicated_status_menu_via_shared_structure`
- `status_conversation_id_uses_the_selected_id_or_none`
- `user_info_updates_only_require_an_open_status_menu_repaint`
- `accepted_command_history_executes_through_the_shell_submission_path`
- `accepted_command_history_preserves_workflow_metadata`
- `accepted_prompt_history_submits_to_the_selected_ai_conversation`
- `zero_state_running_command_hint_shows_attachment`
- `manual_attach_and_detach_switch_running_command_input_ownership`
- `running_command_completion_clears_transient_attachment_lock`
- `tagged_in_alt_screen_keeps_output_and_composer_visible`
- `agent_controlled_alt_screen_keeps_output_and_composer_visible`
- `user_controlled_alt_screen_keeps_full_session_input_on_the_pty`
- `stale_user_pty_bytes_are_dropped_after_agent_takes_control_or_is_tagged_in`
- `visible_startup_script_shows_no_running_command_hint`
- `footer_falls_back_to_replacing_voice_hints_when_voice_item_is_disabled`
- `configured_voice_item_renders_idle_listening_and_transcribing_states`
- `voice_click_is_interactive_only_within_the_segment_bounds`
- `voice_toggle_stops_listening_and_ignores_transcribing`
- `clear_slash_command_clears_shell_commands_from_transcript`
- `orchestration_tab_icon_replaces_identity_only_while_active_or_blocked`
- `footer_renders_shell_mode_sections_without_model_or_usage`
- `terminal_use_interrupt_closes_shortcuts_before_taking_control`
- `running_command_attachment_bindings_are_context_scoped`
- `blocked_terminal_use_action_acceptance_uses_ctrl_enter_without_rebinding_submit`
- `voice_input_uses_ctrl_s_only_when_the_composer_owns_input`
- `alternate_screen_clears_orchestration_tab_focus_and_bindings`
- `orchestration_updates_refresh_only_the_focused_session`
- `focus_input_bindings_match_down_and_shift_down_in_tab_context_only`
- `escape_binding_targets_main_agent_in_tab_context_only`
- `orchestration_tab_navigation_bindings_remain_scoped_to_tab_context`
- `orchestration_tab_footer_advertises_down_without_shift_or_escape_hint`
- `escape_from_child_tab_switches_to_root_and_clears_tab_focus`
- `escape_with_root_selected_clears_tab_focus_without_switching`
- `status_email_fallback_chain_covers_username_and_signed_in_arms`
- `resume_shell_commands_use_shared_tui_launcher`

### `crates/warp_tui/src/terminal_use_tests.rs` — A, 1 missing

- `alternate_screen_routes_input_to_its_current_owner`

### `crates/warp_tui/src/tool_call_labels_tests.rs` — A, 1 missing

- `all_failed_run_agents_uses_failure_glyph`

### `crates/warp_tui/src/transient_hint_tests.rs` — A, 1 missing

- `error_hint_uses_error_tone`

### `crates/warp_tui/src/tui_ask_question_view_tests.rs` — A, 4 missing

- `focusing_an_active_question_delegates_to_the_selector`
- `enter_selects_options_and_other_before_shift_enter_advances_multiselect`
- `enter_keeps_single_select_auto_advance_behavior`
- `enter_does_not_submit_a_final_multiselect_question`

### `crates/warp_tui/src/tui_builder_tests.rs` — A, 1 missing

- `voice_input_border_pulses_between_cyan_overlay_2_and_lilac_600`

### `crates/warp_tui/src/tui_cli_subagent_view_tests.rs` — A, 5 missing

- `allow_executes_the_exact_displayed_action`
- `reject_cancels_only_the_exact_displayed_action`
- `write_action_presentation_shows_input_and_mode_without_internal_ids`
- `transfer_action_presentation_shows_the_agents_reason`
- `pty_input_display_names_control_bytes_and_preserves_lines`

### `crates/warp_tui/src/tui_file_edits_view_tests.rs` — A, 4 missing

- `section_states_expand_and_collapse_for_approval_lifecycle`
- `toggle_expand_all_collapses_then_expands`
- `blocked_file_edits_card_shows_expand_hint_sections_and_options`
- `e_key_dispatches_toggle_expand_all_on_blocked_card`

### `crates/warp_tui/src/tui_permission_prompt_tests.rs` — A, 4 missing

- `focusing_an_active_prompt_delegates_to_the_selector`
- `e_focuses_the_body_editor_without_interfering_with_other`
- `footer_shows_exit_editor_hint_while_body_editor_is_focused`
- `editable_prompt_uses_the_standard_footer`

### `crates/warp_tui/src/tui_shell_command_view_tests.rs` — A, 2 missing

- `escape_while_editing_exits_editor_without_cancelling`
- `second_escape_after_editor_exit_cancels_tool_call`

### `crates/warp_tui/src/usage_tests.rs` — C, 3 missing

- `cost_formats_cents_as_dollars`
- `entry_text_matches_the_gui_credits_formatting`
- `entry_text_follows_the_persisted_display_mode`

### `crates/warp_tui/src/voice_input_tests.rs` — C, 3 missing

- `start_does_not_replace_an_active_session`
- `stop_transitions_the_model_to_transcribing`
- `cancel_returns_the_model_to_idle`

### `crates/warp_tui/src/zero_state_animation_tests.rs` — D, 26 missing

- `starfield_density_scales_with_the_full_panel_area`
- `logo_mask_preserves_the_offset_warp_faces`
- `full_face_frame_is_recognizable_and_centered`
- `background_starfield_stays_low_density`
- `background_stars_move_between_frames`
- `quarter_turn_is_narrower_and_exposes_the_side`
- `half_turn_exposes_the_back_face`
- `one_revolution_returns_to_the_initial_frame`
- `logo_scales_down_while_preserving_cell_aspect`
- `animation_is_hidden_when_the_panel_is_too_small`
- `ascii_parser_normalizes_crlf_trims_borders_and_pads_ragged_rows`
- `representative_ascii_fixtures_have_distinct_dimensions`
- `ascii_parser_rejects_invalid_empty_and_oversized_input`
- `relative_ascii_paths_resolve_from_the_tui_config_directory`
- `startup_loader_reads_relative_ascii_art_and_retains_motion_settings`
- `startup_loader_falls_back_for_missing_or_invalid_art_only`
- `object_path_change_reloads_shape_without_changing_motion_settings`
- `linked_file_content_change_is_ignored_when_object_path_is_unchanged`
- `invalid_object_path_change_keeps_last_valid_shape`
- `settings_model_reloads_only_object_changes`
- `representative_ascii_shapes_rotate_through_front_side_and_back`
- `configured_period_controls_phase_and_repeats_exactly`
- `configured_depth_changes_edge_on_width`
- `custom_shapes_preserve_their_authored_cell_aspect`
- `extreme_ascii_aspect_ratios_clamp_to_a_visible_minimum`
- `custom_animation_element_paints_and_requests_another_frame`

### `crates/warp_tui/src/zero_state_tests.rs` — A, 1 missing

- `login_line_shows_signed_in_account_email`

