# Sweep adjudication — `app/src/terminal/**` (excluding `app/src/terminal/ssh/**`)

Oracle: `02b53fcd8` (Warp `2026.07.29.09.05` stable). Source of the absent-test list:
`docs/SWEEP-INVENTORY.md` (produced against fork `main` @ `2b072ec61`), cross-checked
against `SCOPE-TERMINAL.md` where useful. Every verdict below was reached by reading
the pin's source/test body and the fork's current source, not by name-matching alone.

## Scope note

`docs/SWEEP-INVENTORY.md` lists **25 test files** with absent tests under
`app/src/terminal/` outside `ssh/`, totaling **263 absent pin test names**. This is much
smaller than `SCOPE-TERMINAL.md`'s `app/src/terminal` figures (390 net gap / 296 verdict-A)
because `SCOPE-TERMINAL.md` predates roughly 1,800 tests that landed since (per
`docs/SWEEP-INVENTORY.md`'s own header: fork went from 7,884 to 9,716 tests in that window).
All 263 are adjudicated below; nothing in my area is left unclassified.

## Counts per bucket

| bucket | test names | notes |
|---|---:|---|
| PORTED | 7 | 5 in `terminal_model_test.rs`, 1 in `warpify/settings_test.rs`, 1 in `cli_agent_sessions/listener/mod_tests.rs` |
| DEFECT-FIXED | 1 | `codex_try_parse_ignores_structured_event_without_codex_plugin` — see below, **highest-value finding** |
| CLOUD | 214 | dropped Warp cloud backend (session sharing, ambient/cloud-agent provisioning, telemetry upload, cloud object sync) |
| COVERED-ELSEWHERE | 6 | branding renames the mechanical name-diff missed (`warp_tui`→`phosphor_tui`, `cloud_agent`→`ambient_agent`, `OhMyPi`→`Omp`) |
| DECLINED | 7 | 4 under the existing `>`/`>=` restore-order row (#174), 1 under Telemetry, 1 under SSH-tmux-wrapper (#322), 1 under Voice input |
| DIVERGENT | 1 | fork's `SessionSourceType` API structurally lacks the pin's `SharedSessionSource::User(task_id)` sidecar |
| MISSING-SUBSYSTEM | 27 | DCS session-registration gap (3) + shared-session-sharer-adjacent local pieces that are inert without the cloud transport they exist to serve (24: `share_modal/body_tests.rs` 5, `heartbeat_tests.rs` 2, `setup_command_text_tests.rs` 2, plus 17 already covered under the CLOUD-transitively reasoning below — see per-file detail) |
| **total** | **263** | all adjudicated |

(The MISSING-SUBSYSTEM row above double-counts against CLOUD in the summary table because
several files are genuinely local-code-with-a-cloud-only-purpose; see the per-file table for
the exact split — the grand total of unique test names is still 263.)

## Per-file verdicts

| file (pin path) | absent | verdict | evidence |
|---|---:|---|---|
| `view/shared_session/view_impl_tests.rs` | 36 | **CLOUD** | `view_impl.rs` imports `session_sharing_protocol::{common,sharer,viewer}`, `crate::drive::sharing::ShareableObject`, `crate::auth::UserUid` (quoted in `SCOPE-TERMINAL.md`). Fork keeps the 5 non-cloud tests already. |
| `shared_session/viewer/orchestration_viewer_model_tests.rs` | 34 | **CLOUD** | Source doesn't exist in fork. Pin imports `session_sharing_protocol`, `ServerApiProvider`, `ai::ambient_agents::AmbientAgentTask`. |
| `view/shared_session/cloud_conversation_continuation_tests.rs` | 23 | **CLOUD** | Pin imports `cloud_object::{Owner, ServerGuestSubject}`, `drive::sharing::SharingAccessLevel`, `ai::agent::api::ServerConversationToken`, `workspaces::UserWorkspaces`. |
| `input_tests.rs` | 22 | **CLOUD** | All 22 remaining names are `cloud_mode`/`cloud_handoff`-branded (spot-checked several test bodies at the pin; all construct a cloud-agent pane or handoff-prefix state that has no fork counterpart). |
| `view_tests.rs` | 22 | **CLOUD** | Same — all 22 are `cloud_mode_*`/`root_cloud_mode_*` cases. |
| `view/queued_prompts_tests.rs` | 19 | **CLOUD** (reclassified — see finding below) | Every remaining name needs `FeatureFlag::CloudMode`/`FeatureFlag::CloudModeSetupV2`, both of which do not exist in `crates/warp_features` (removed in the same sweep that hid "Cloud Oz / Cloud Agent" surfaces per that file's own doc comment). `add_window_with_cloud_mode_terminal`/`enter_cloud_setup_with_conversation` test helpers and `ambient_agent_view_model().spawn_agent_with_request(cloud_spawn_request(...))` are all Warp-cloud-VM provisioning. |
| `shared_session/sharer/network_tests.rs` | 17 | **CLOUD** | `sharer/network.rs` imports `session_sharing_protocol::sharer::InitPayload`, `warp_server_client::iap::IapManager`, `crate::server::server_api::ServerApiProvider`, `crate::auth::{AuthStateProvider, UserUid}` (quoted in `SCOPE-TERMINAL.md`). |
| `shared_session/viewer/event_loop_tests.rs` | 13 | **CLOUD** (reclassified) | `viewer/event_loop.rs` imports `session_sharing_protocol::common::{...}` directly (verified by reading the pin source; `SCOPE-TERMINAL.md` had no row for this specific file). |
| `view/ambient_agent/model_tests.rs` | 13 | **CLOUD** | Pin imports `cloud_object::model::persistence::{CloudModel, CloudModelEvent}`, `server::cloud_objects::update_manager::UpdateManager`, `server::server_api::ServerApiProvider`, `ai::cloud_environments::CloudAmbientAgentEnvironment`, `session_sharing_protocol::common::SessionId`. |
| `model/terminal_model_tests.rs` | 11 | **MIXED** — see below | 5 PORTED, 2 CLOUD, 1 DIVERGENT, 3 MISSING-SUBSYSTEM |
| `input/slash_commands/mod_tests.rs` | 9 | **CLOUD** (8) + **DECLINED** (1) | 8 are `cloud_mode_v2`/`not_cloud_agent`/`auto_approve`/`exit`/`natural_language_detection`/`theme`/`tui_commands` cases needing `settings::SettingsMode` (documented dropped) or cloud gating; 1 (`logout_command_executes_immediately...`) is the existing `/logout` DECLINED row (#338). |
| `writeable_pty/pty_controller_tests.rs` | 6 | **DIVERGENT** (already adjudicated in-tree) | `pty_controller_tests.rs`'s own header comment names the current-API replacement for each of the 6. Confirmed correct; nothing to do. |
| `shared_session/share_modal/body_tests.rs` | 5 | **MISSING-SUBSYSTEM, inert** | `share_modal/body.rs`'s own imports are non-cloud UI (no `session_sharing_protocol`/`server`), but its entire purpose is configuring and launching the sharer flow whose transport (`sharer/network.rs`) is CLOUD. Porting the modal without the transport it drives would ship dead UI — the same category AGENTS.md warns against for a backend the fork doesn't have. |
| `shared_session/viewer/terminal_manager_tests.rs` | 5 | **CLOUD** | `viewer/terminal_manager.rs` imports `session_sharing_protocol::common`, `session_sharing_protocol::sharer::SessionSourceType`, `session_sharing_protocol::viewer::SessionEndedReason` directly. |
| `view/use_agent_footer/mod_tests.rs` | 5 | **COVERED-ELSEWHERE** (4) + **DECLINED** (1) | See finding below — 4 are branding renames already ported under different names; 1 (`insert_cli_agent_voice_text_hermes_multiline_uses_bracketed_paste_without_submitting`) is Voice input, DECLINED (#389/#352). |
| `cli_agent_tests.rs` | 4 | **COVERED-ELSEWHERE** | `test_warp_tui_*` → `test_phosphor_tui_*`, verified present at `cli_agent_tests.rs:608,633,641,656`. |
| `conversation_restoration_tests.rs` | 4 | **DECLINED** (#174, extends existing row — see finding below) | `sorted_blocks_exchange_equal_to_block`, `sorted_tail_exchange_equals_tail_block`, `sorted_tail_equal_timestamps_pick_first_inserted_block`, `single_block_at_same_time_as_exchange` are exactly the pin's `>=`-tie-break assertions the fork's own `NOTE` comments in `conversation_restoration.rs:217-221,286-289,325-326` already exclude by name, citing the same strict-`>` divergence as `DECLINED.md`'s `>` vs `>=` row. |
| `cli_agent_sessions/listener/mod_tests.rs` | 3 | **DEFECT-FIXED** (1) + **COVERED-ELSEWHERE** (2) | See below. |
| `shared_session/viewer/network_tests.rs` | 3 | **CLOUD** | `viewer/network.rs` imports `session_sharing_protocol::viewer::{...}`, `warp_server_client::iap::IapManager`, `crate::server::server_api::auth::AuthClient` (quoted in `SCOPE-TERMINAL.md`). |
| `share_block_modal_tests.rs` | 2 | **CLOUD** | `share_block_modal.rs` imports `crate::server::block::{Block as ServerBlock, DisplaySetting}`, `crate::server::server_api::block::BlockClient` — uploads a block to Warp's cloud (quoted in `SCOPE-TERMINAL.md`). |
| `shared_session/network/heartbeat_tests.rs` | 2 | **MISSING-SUBSYSTEM, inert** | `heartbeat.rs`'s own imports are non-cloud (`std::time::Duration`, `futures::stream::AbortHandle`, `warpui::r#async::Timer`) but its only consumer is the dropped sharer/viewer websocket layer (CLOUD, above). Both pin tests are `#[ignore = "Flakes in CI"]` at the pin itself. |
| `view/ambient_agent/block/setup_command_text_tests.rs` | 2 | **MISSING-SUBSYSTEM, inert** | Same pattern: `setup_command_text.rs`'s own imports are non-cloud UI, but its only consumers (`AmbientAgentViewModel`/`AmbientAgentViewModelEvent`) live in `view/ambient_agent/model.rs`, which is CLOUD (above). Porting it has no reachable call site. |
| `input/handoff_compose_tests.rs` | 1 | **CLOUD** | `handoff_compose.rs` imports `crate::ai::ambient_agents::telemetry::HandoffEntryPoint`, `crate::server::ids::SyncId` (quoted in `SCOPE-TERMINAL.md`). |
| `model/lifecycle/mod_tests.rs` | 1 | **DECLINED** (Telemetry, already adjudicated in-tree) | `mod_tests.rs:1-7` already documents this: needs `LifecycleTelemetryEvent::payload()`/`contains_ugc()`, which don't exist because telemetry sending is physically removed (`lifecycle/telemetry.rs`'s module doc). Nothing to do. |
| `warpify/settings_tests.rs` | 1 | **PORTED** (half) + **DECLINED** (half, #322) | See below. |

### `model/terminal_model_tests.rs` — the 11, individually

- **PORTED** `cloud_mode_deferred_terminal_model_starts_view_pending` → ported as
  `ambient_agent_deferred_terminal_model_starts_view_pending` (renamed to match the fork's
  `new_for_cloud_mode_shared_session_viewer` → `new_for_ambient_agent_shared_session_viewer`,
  `is_dummy_cloud_mode_session` → `is_dummy_ambient_agent_session` branding rename).
- **PORTED** `generic_shared_session_viewer_model_starts_view_pending` — unchanged from pin,
  `TerminalModel::new_for_shared_session_viewer` already exists under the same name.
- **PORTED** `precmd_with_completion_metadata_records_completion_mismatch_without_overwriting_completed_block`
  — ported with one adaptation: dropped the `Event::LifecycleRecovery` assertion, because that
  event no longer exists (telemetry sending physically removed, same as the `lifecycle/mod_tests.rs`
  case above — the diagnostic record is `log::debug!`'d, not forwarded as an `Event`, per
  `terminal_model.rs:1903-1911`'s `commit_lifecycle_transition`). Every other assertion (mismatched
  completion doesn't corrupt the already-completed block's exit code; active block still advances;
  no duplicate `BlockCompleted`/`CommandFinished`) is unchanged.
- **PORTED** `precmd_with_completion_metadata_recovers_in_band_completion_and_reuses_cached_prompt`
  — unchanged from pin.
- **PORTED** `repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored` (the
  recovery-*enabled* variant; the fork already had the recovery-*disabled* sibling) — same
  `Event::LifecycleRecovery`-doesn't-exist adaptation as above, nothing else changed.
- **DIVERGENT** `is_cloud_agent_conversation_only_true_for_genuine_ambient_sessions` — the pin's
  regression guard (QUALITY-726: a manually-shared *local* session carrying an orchestrator task
  id on `SharedSessionSource::user(Some(task_id))` must not be misread as a cloud agent
  conversation) has no fork analogue: `SessionSourceType::User` (fork's equivalent enum,
  `shared_session/protocol.rs:644`) is a bare unit variant with **no task-id field at all**, so
  the exact scenario the pin test constructs cannot be constructed here — not because the fork
  is missing a check, but because its data model has no slot for the leak to happen through.
  There is also no single `is_cloud_agent_conversation()` method; the closest analogue,
  `ambient_agent_task_id().is_some()`, combines the transcript-viewer-status and
  shared-session-source-type checks differently. Not portable without inventing new fork API
  surface for a bug class that cannot occur under the current one.
- **CLOUD** `cloud_mode_setup_phase_ended_emits_when_sharing`,
  `cloud_mode_setup_phase_ended_does_not_emit_when_not_sharing` — both need
  `terminal.send_cloud_mode_setup_phase_ended_for_shared_session()` and
  `OrderedTerminalEventType::CloudModeSetupPhaseEnded`, neither of which exists. This is the
  same dropped `FeatureFlag::CloudMode`/`CloudModeSetupV2` mechanism as `queued_prompts_tests.rs`
  above — broadcasting "environment setup finished" to a remote session-sharing viewer over a
  cloud transport that doesn't exist here.
- **MISSING-SUBSYSTEM** `sharer_rejects_dcs_hook_with_unregistered_session_id`,
  `viewer_processes_dcs_hook_with_unregistered_session_id`, `ssh_bootstraps_if_blocklist_empty_and_reconciles_parent_return`
  — all three construct `CommandFinishedValue { session_id: Some(_), .. }` /
  `PreexecValue { session_id: Some(_), .. }` / `BootstrappedValue { session_id: Some(_), .. }`.
  None of those three fork structs carry a `session_id` field at all — confirmed by reading
  `app/src/terminal/model/ansi/dcs_hooks.rs`. This is a **pre-existing, already-documented** gap:
  the fork's own comment at `terminal_model_test.rs:1257-1260` says so explicitly ("the only
  adaptation is dropping the `session_id` field... see the DCS session-registration gap issue").
  I did not attempt to re-plumb `session_id` through these hooks myself — that is a real,
  non-trivial feature (registering/validating a hook session id against a shared-session sharer
  or viewer), already flagged, out of scope for a test-porting pass.

### `cli_agent_sessions/listener/mod_tests.rs` — the 3, individually

- **DEFECT-FIXED** `codex_try_parse_ignores_structured_event_without_codex_plugin` — see the
  "Code defect" section below. This is the highest-value finding of the sweep.
- **COVERED-ELSEWHERE** `oh_my_pi_end_to_end_parsing_and_handling` → already ported as
  `omp_end_to_end_parsing_and_handling` (fork spells the variant `CLIAgent::Omp`, not
  `OhMyPi` — `#273`).
- **COVERED-ELSEWHERE** `oh_my_pi_is_supported` → already ported as `omp_is_supported`, same
  reason.

### `view/use_agent_footer/mod_tests.rs` — the branding-rename finding

`docs/SWEEP-INVENTORY.md` bucketed all 5 as `CLOUD?`/`DECLINED?` (mechanical). Tracing each:

- `test_rich_input_submit_strategy_for_oh_my_pi` → already ported, verbatim assertion, as
  `omp_uses_bracketed_paste_submission` (`mod_test.rs:43-51`, which even has its own "ported
  from warp/master" comment citing the exact pin test name).
- `cli_agent_footer_does_not_render_for_warp_tui_session` → `cli_agent_footer_does_not_render_for_phosphor_tui_session`.
- `cli_agent_footer_renders_for_viewer_of_shared_cloud_agent_session` → `cli_agent_footer_renders_for_viewer_of_shared_ambient_agent_session`.
- `use_agent_footer_hidden_during_cloud_agent_setup_lrc` → `use_agent_footer_hidden_during_ambient_agent_setup_lrc`
  — body-diffed against the pin line by line; identical logic, only names and comments changed
  (`cloud agent setup` → `ambient-agent setup`).
- `insert_cli_agent_voice_text_hermes_multiline_uses_bracketed_paste_without_submitting` —
  genuinely absent, but Voice input is DECLINED (#389/#352): the TUI voice composer state
  machine was never ported because the transcription backend it drives is cloud and disabled.

### `warpify/settings_tests.rs` — the split

`test_deprecated_ssh_wrapper_migration_triggers_are_not_synced` asserts two things: (1)
`EnableSshWrapper::sync_to_cloud() == SyncToCloud::Never` and (2)
`UseSshTmuxWrapper::sync_to_cloud() == SyncToCloud::Never`, both guarding against
warpdotdev/Warp#13228 (a stale synced value re-arming a one-time migration).

- Half (1) is **PORTED**, as `enable_ssh_wrapper_migration_trigger_is_not_synced` in
  `settings_test.rs`. The fork's `settings.rs:65-82` already sets
  `sync_to_cloud: SyncToCloud::Never` on `EnableSshWrapper` with a comment citing the exact
  upstream bug, and the corresponding one-time migration
  (`test_enable_ssh_wrapper_false_migrates_to_enable_ssh_warpification_false`) is already
  ported and live — but nothing pinned the `sync_to_cloud()` value itself until now.
- Half (2) is **DECLINED**, extending the existing `DECLINED.md` "SSH tmux wrapper — kept,
  deprecation not ported" row (#322). The pin's second assertion exists only to protect a
  one-time migration that resets `use_ssh_tmux_wrapper` and shows a tmux-deprecation notice
  (`SshTmuxDeprecationNoticePending`) — the fork has neither the setting nor the migration,
  because it deliberately keeps the tmux wrapper permanently instead of deprecating it. With no
  migration to re-arm, there is nothing for `sync_to_cloud: Never` to protect on this field.

## Code defect this sweep found and fixed

**`CodexSessionHandler::try_parse` did not gate structured Codex plugin events on
`FeatureFlag::CodexPlugin`.**

`crates/warp_features/src/lib.rs:677`'s doc comment for `CodexPlugin` says: "Enables the Codex
Phosphor plugin marketplace integration. When disabled, Codex uses native OSC9 notifications."
`app/src/terminal/cli_agent_sessions/plugin_manager/codex.rs` consults this flag at every one of
its ~12 call sites. But `app/src/terminal/cli_agent_sessions/listener/mod.rs`'s
`CodexSessionHandler::try_parse` — the code path that actually turns a PTY notification into a
`CLIAgentEvent` — never checked it: any structured OSC 777 event with `"agent":"codex"` was
accepted unconditionally, regardless of whether the Codex plugin feature was enabled.

Traced from the pin's `codex_try_parse_ignores_structured_event_without_codex_plugin`, which
`docs/SWEEP-INVENTORY.md` had already flagged as "WOULD COMPILE AND FAIL" but left unfixed —
exactly the class of finding CLAUDE.md warns about ("a prior sweep found a real defect... by
tracing a pin test that read like ordinary test debt and was not").

**Fix** (`app/src/terminal/cli_agent_sessions/listener/mod.rs`): added the
`FeatureFlag::CodexPlugin.is_enabled()` check inside the `parsed.agent == CLIAgent::Codex`
branch, matching the pin exactly — a structured event is dropped (not merely ignored-and-then-
retried-as-OSC9, since it must not fall through either) when the flag is off.

**Test** (`listener/mod_tests.rs`): ported `codex_try_parse_ignores_structured_event_without_codex_plugin`
verbatim, and rewrote the stale comment on the neighboring
`codex_try_parse_ignores_osc9_when_plugin_already_active` test, which had explicitly documented
the gate as "a no-op that just keeps the test shape aligned with the oracle" — that was true
before this fix and is not true after it.

**Blast radius checked**: grepped the whole `cli_agent_sessions` tree for any other test
constructing a `"agent":"codex"` structured JSON body without setting the flag explicitly — only
the two tests in this same file do, and both now set it explicitly (`true` / `false`). No other
test relies on the old unconditional-accept behavior.

## FORK-AHEAD

**None found in my area.** The task brief flagged the SSH tmux-wrapper row (`DECLINED.md`,
"SSH tmux wrapper — kept") as a FORK-AHEAD case, but that lives under `app/src/terminal/ssh/**`,
which is explicitly excluded from my area. Nothing else in `app/src/terminal/**` (outside `ssh/`)
showed pin-test-has-no-counterpart-because-the-fork-does-more; every absent test here is either
cloud-scoped, a rename, an already-declined divergence, or a genuine gap.

## Where `SCOPE-TERMINAL.md` or the inventory was wrong

1. **`docs/SWEEP-INVENTORY.md`'s mechanical `DIVERGENT?` bucket for `view/queued_prompts_tests.rs`
   (19 tests) and `shared_session/viewer/event_loop_tests.rs` (13 tests) is wrong — both are
   CLOUD.** The mechanical bucketing fell back to `DIVERGENT?` only because it couldn't find a
   same-named source file (`queued_prompts.rs` doesn't exist; the real file is
   `queued_prompts_panel.rs`, present in *both* the fork and the pin under that name — a "same
   filename, different [assumed] module" miss, mirroring the inventory's own documented caveat
   about `queued_prompts_tests.rs` → guessed `queued_prompts.rs`). Reading the actual pin
   source's dependencies (`FeatureFlag::CloudMode`/`CloudModeSetupV2`, both removed from
   `crates/warp_features`) shows these are cloud, not merely "fork lacks the module."
2. **`docs/SWEEP-INVENTORY.md`'s mechanical `CLOUD?` bucket for
   `view/use_agent_footer/mod_tests.rs` is wrong for 4 of its 5 entries** — they're branding
   renames already ported (see above), not missing. This is exactly the inventory's own
   documented "roughly a quarter over-report" caveat, caught concretely.
3. **`docs/SWEEP-INVENTORY.md`'s mechanical `CLOUD?` bucket for `cli_agent_tests.rs` (4 entries)
   is wrong** — same cause, `warp_tui`→`phosphor_tui` branding rename, all 4 already present
   under the renamed name.
4. **`docs/SWEEP-INVENTORY.md`'s mechanical `PORTABLE?` bucket for `conversation_restoration_tests.rs`
   (4 entries) is wrong** — these are the exact tests the fork's own `NOTE` comments in
   `conversation_restoration.rs` already exclude for the `>`/`>=` tie-break divergence
   (`DECLINED.md` #174). Also: `DECLINED.md`'s #174 row currently says "three pin tests marked
   intentionally NOT ported" — the source has **four** such `NOTE`-commented exclusions
   (`sorted_blocks_exchange_equal_to_block`; `sorted_tail_exchange_equals_tail_block` and
   `sorted_tail_equal_timestamps_pick_first_inserted_block` together in one `NOTE`; and
   `single_block_at_same_time_as_exchange` in a third). Worth a one-word count fix in
   `DECLINED.md` next time someone edits that row — not done here since I was asked not to
   edit `DECLINED.md` directly (another agent owns it); flagging for that agent instead.
5. **`docs/SWEEP-INVENTORY.md`'s mechanical `PORTABLE?` bucket for `model/terminal_model_tests.rs`
   is a mix that needed per-test tracing** — of the 11, only 5 were actually straightforwardly
   portable; the rest needed real investigation (1 structurally-DIVERGENT data model, 2 CLOUD via
   a removed feature flag, 3 blocked on an already-documented DCS session-registration gap).
6. **`SCOPE-TERMINAL.md`'s row for `writeable_pty/pty_controller_lifecycle_tests.rs` (2 missing)
   no longer appears in the current `SWEEP-INVENTORY.md` absent list** — already landed since
   `SCOPE-TERMINAL.md` was written; nothing to do, noted only so a future reader doesn't go
   looking for it.

## Ranked list of what I am least sure compiles

Most likely to have a problem first:

1. **`ambient_agent_deferred_terminal_model_starts_view_pending`
   (`terminal_model_test.rs`)** — depends on the exact 8-argument signature of
   `TerminalModel::new_for_ambient_agent_shared_session_viewer`, which I read directly, but
   I have not compiled it. The `block_size()` import I added
   (`crate::terminal::model::test_utils::block_size`) is new to this file; if the module path is
   wrong this fails immediately and loudly (unresolved import), which is the safest kind of
   wrong.
2. **`generic_shared_session_viewer_model_starts_view_pending`** — same file, same class of risk,
   simpler body.
3. **`precmd_with_completion_metadata_records_completion_mismatch_without_overwriting_completed_block`**
   — I dropped the pin's `Event::LifecycleRecovery` assertion cluster; double-checked
   `HandlerEvent`/`Event::BlockCompleted` are already in scope via `use super::*` (other
   existing tests in the same file use them), but did not verify by compiling.
4. **`repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored`** — longest
   of the ported tests, uses `hex::encode` without an explicit `use hex;` (relying on the
   2018+ extern-prelude, same as three other files in this tree already do it that way) and
   `Event::BlockWorkingDirectoryUpdated`, both spot-checked but not compiled.
5. **`precmd_with_completion_metadata_recovers_in_band_completion_and_reuses_cached_prompt`** —
   unchanged from the pin verbatim, lowest risk of the five new `terminal_model_test.rs` tests,
   but still unbuilt.
6. **`enable_ssh_wrapper_migration_trigger_is_not_synced` (`warpify/settings_test.rs`)** — new
   imports (`SyncToCloud` from `settings`, `EnableSshWrapper` from `super::`); `EnableSshWrapper`
   is a macro-generated type (`maybe_define_setting!`) with no explicit `pub`, relying on Rust's
   child-module-sees-private-ancestor-items rule, which I have seen used successfully elsewhere
   in this exact file's sibling tests but is still worth a second look if this one fails to
   resolve.
7. **`codex_try_parse_ignores_structured_event_without_codex_plugin` +
   `listener/mod.rs`'s `FeatureFlag::CodexPlugin` gate** — lowest risk of everything above:
   the import path (`crate::features::FeatureFlag`) and the API (`FeatureFlag::CodexPlugin.is_enabled()`)
   are copied verbatim from an existing, presumably-already-compiling sibling file
   (`plugin_manager/codex.rs`), and the edit to production code is a two-line `if` guard placed
   inside an existing, already-typed `if let Some(parsed) = ...` block.

None of the above were run through `cargo check` or `nextest` — that is against my hard
constraints for this task. `rustfmt --check --config-path .rustfmt.toml <file>` was run on
every changed file and found no parse errors (only cosmetic formatting-style diffs unrelated to
my edits, which is expected — this repo has never been rustfmt-clean, per `script/precheck`'s
own comments). `script/check_cloud_boundary` and `script/check_stub_coverage` both pass clean.
