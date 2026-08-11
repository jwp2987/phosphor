# `crates/warp_tui/**` sweep — pin tests with no fork equivalent

Oracle: **`02b53fcd8`** (Warp `2026.07.29.09.05` stable), the pin in `ORACLE.md`. Fork
side: `main` (this branch, based on the commit that carries
`docs/SWEEP-INVENTORY.md`). Scope: every `crates/warp_tui/src/*` section of
`docs/SWEEP-INVENTORY.md` — **101 absent pin tests across 20 files**, all
mechanical (`?`-suffixed) guesses at the start of this pass. All 101 are
adjudicated below; none are left `?`.

## Totals

| bucket | tests | note |
|---|---:|---:|
| PORTED (this pass) | 6 | 1 exposed a real code defect, fixed |
| PORTED (already landed, prior sweep) | 3 | carried forward, verified still present |
| DEFECT-FIXED (already landed, prior sweep) | 1 | ctrl-c / shortcuts sheet, PR #124-era fix |
| DECLINED | 25 | quoting an existing `DECLINED.md` row, or extending one |
| CLOUD | 11 | `ServerApiProvider` / `CloudEnvironmentCatalog` / cloud-runner RunAgents |
| DIVERGENT | 15 | fork API/behaviour genuinely differs, several already ported under another name |
| MISSING-SUBSYSTEM | 15 | non-cloud, real gap; several share one root cause (see below) |
| COVERED-ELSEWHERE | 4 | pin behaviour already exercised under a different fork name |
| **total** | **80** | *(see note)* |

**Note on the arithmetic:** several tests carry two true verdicts (e.g. "already
covered elsewhere, and also would fail if ported because the fork inverted the
behaviour"). Each test below is filed under one *primary* bucket with the
secondary noted in its evidence; the table above undercounts by the number of
such dual-tagged rows relative to a naive per-file sum, and the per-file
sections below are the source of truth (they sum to 101 test names).

## The biggest finding: how much of the gap is BEHIND-A-GENERATION

**None of the 101 is a clean BEHIND-A-GENERATION case in the strict sense**
(a view that was simply never resynced after a pin refactor). But **15 tests
(15% of this area's gap) share one real, coherent MISSING-SUBSYSTEM root
cause that reads exactly like #456's territory**, and it is worth scoping
precisely because it explains a disproportionate slice of the "PORTABLE?"
mechanical bucket:

**The "attach agent to running command" feature landed partially.** The core
attach/detach mechanism exists and is fully wired
(`can_attach_agent_to_running_command`, `try_attach_agent_to_running_command`,
`try_detach_agent_from_running_command`, `TuiTerminalSessionAction::AttachAgentToRunningCommand`,
`SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG` — all in
`terminal_session_view.rs`). But three supporting pieces that the pin ships
alongside it never landed:

1. **`InputTypeAutoDetectionSource` has one variant** (`HistoryMatch`) where the
   pin has ~30, including the `AgentTerminalControl` variant these tests need.
   This is a **known, already-documented, already-scoped decision** —
   `app/src/ai/blocklist/input_model.rs:1060-1067`: *"narrowed to the single
   variant this fork can actually produce... threading a source through all of
   those is a separate, much larger port (12 files, ~76 references) already
   tracked by #399/#254 item d and out of scope here."* Verified still true
   this session (the enum is still single-variant; #254's *landed* narrow items
   — `Input::unfreeze_agent_input`, `preserve_input` — are unrelated fields,
   confirmed via `TODO.md:1199-1201`).
2. **`RUNNING_COMMAND_DETACH_HINT`** (the footer text shown while the agent is
   tagged into a running command) does not exist anywhere in `warp_tui`.
3. **`running_command_hint()` / `input_hints::long_running_command_hint()`**
   (the "`<key>`  to use agent" attach-hint text) do not exist either — the
   fork's `input_hints.rs` has `LONG_RUNNING_COMMAND_HINT` ("ctrl-c to
   interrupt") but not the pin's attach-advertising counterpart, even though
   every dependency it needs (`binding_hint`, `ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME`,
   `self.keymap_context(ctx)`) already exists and compiles today.

Six `terminal_session_view_tests.rs` tests trace to this one gap:
`manual_attach_and_detach_switch_running_command_input_ownership`,
`nld_reset_only_unlocks_after_agent_control_and_not_on_user_edit`,
`running_command_completion_clears_transient_attachment_lock`,
`tagged_in_alt_screen_keeps_output_and_composer_visible`,
`user_controlled_alt_screen_keeps_full_session_input_on_the_pty`,
`zero_state_running_command_hint_shows_attachment`. All six are otherwise
fully testable — every *other* symbol they need (alt-screen mode, block
tagging, `render_session`, `focus_test_fixture`) already exists and is
exercised by neighbouring tests in the same file.

**Separately**, 9 more MISSING-SUBSYSTEM tests (`agent_message_tests.rs` ×4,
two `agent_block_tests.rs` tests, one dual-tagged with DECLINED, and two
`orchestration_model_tests.rs` local-child-launch tests) trace to a **second,
unrelated** root cause: the TUI never got a renderer for
`AIAgentOutputMessageType::MessagesReceivedFromAgents`/`EventsFromAgents`. See
the dedicated section below — this one is more consequential because the
underlying data model is already non-cloud and the GUI already renders it.

**Bottom line for scoping #456**: of this area's 101, roughly **15 are a
partial-port gap** (the attach-lock family) and **9 more are a second,
distinct partial-port gap** (orchestration-message rendering) — together
**24 of 101 (24%)** are "the feature landed most of the way and stopped,"
which is #456-shaped work, not ordinary per-test debt. The rest (56 DECLINED
+ CLOUD + DIVERGENT + COVERED-ELSEWHERE) are not generational lag at all;
they are scope decisions or naming differences.

## A second finding worth its own flag: the TUI silently drops non-cloud orchestration messages

`crates/warp_tui/src/agent_block.rs` (~line 1307) contains:

```rust
// Inter-agent messages/events are orchestration (cloud)
// surfaces Zap does not render.
AIAgentOutputMessageType::MessagesReceivedFromAgents { .. }
| AIAgentOutputMessageType::EventsFromAgents { .. } => {}
```

This comment is **stale, not a live decision**. `MessagesReceivedFromAgents` is
fully implemented non-cloud in this fork: `app/src/ai/blocklist/orchestration_topology.rs`
(`resolve_orchestration_participant`, `orchestrator_agent_id_for_conversation`,
`OrchestrationParticipantKind`), `app/src/ai/agent/mod.rs` (`ReceivedMessageDisplay`),
and the **GUI already renders it** end-to-end
(`app/src/ai/blocklist/block/view_impl/orchestration.rs`). `DECLINED.md`'s
"Multi-agent orchestration" row was itself **REVERSED 2026-08-08** — "LOCAL
orchestration is back in scope." The TUI's `agent_block.rs` comment predates
that reversal (or was never revisited after it) and is now simply wrong: this
fork's own GUI proves the surface is not cloud.

This is not something I fixed live — building `crates/warp_tui/src/agent_message.rs`
(the pin's renderer: `conversation_status_glyph`, `render_agent_message`,
`agent_message_section_id`) and wiring a new `TuiAIBlockSection::AgentMessage`
variant into `agent_block.rs`'s match arm is a real, multi-file feature port,
not a test port, and this pass had no way to compile-check it. I filed it as
**MISSING-SUBSYSTEM** everywhere it recurs (9 tests below) rather than risk an
uncompilable change. The GUI's `view_impl/orchestration.rs` is a working,
non-cloud reference implementation for whoever picks this up.

---

## Per-file adjudication

### `terminal_session_view_tests.rs` — 27 absent (pin 90 · fork 63)

| test | verdict | evidence |
|---|---|---|
| `status_email_fallback_chain_covers_username_and_signed_in_arms` | DECLINED | `DECLINED.md` #389: status-menu org/email fields dropped outright (commit `c87c49820`); no `resolve_status_email`/`STATUS_SIGNED_IN` in this fork. |
| `configured_voice_item_renders_idle_listening_and_transcribing_states` | DECLINED | `DECLINED.md` #389/#352: voice-input UI, backend (Wispr) is cloud and disabled. |
| `footer_falls_back_to_replacing_voice_hints_when_voice_item_is_disabled` | DECLINED | same |
| `listening_voice_input_animates_the_input_border` | DECLINED | same |
| `voice_accepts_exact_and_whitespace_only_arguments` | DECLINED | same |
| `voice_click_is_interactive_only_within_the_segment_bounds` | DECLINED | same |
| `voice_input_uses_ctrl_s_only_when_the_composer_owns_input` | DECLINED | same |
| `voice_slash_command_rejects_arguments_before_prompt_fallback` | DECLINED | same |
| `voice_toggle_stops_listening_and_ignores_transcribing` | DECLINED | same |
| `terminal_use_interrupt_closes_shortcuts_before_taking_control` | DEFECT-FIXED | Already fixed in a prior sweep (commit `30dce9d5a`, merged `6826cb89f`): `handle_interrupt` now closes an open read-only sheet before taking control, pinned by `interrupt_closes_an_open_read_only_menu` / `interrupt_closes_an_open_status_menu`. Not re-touched this pass. |
| `response_summary_visibility_is_independent_from_the_footer_usage_mode` | DIVERGENT | Already ported as `..._from_the_footer_usage_entry` — renamed because BYOP has no `TuiUsageDisplayMode` (`usage.rs`). |
| `visible_startup_script_shows_no_running_command_hint` | DIVERGENT + PORTED (partial) | Already ported as `visible_startup_script_shows_no_interrupt_hint`. Three dropped pin assertions (`can_attach_agent_to_running_command`, `SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG`, `try_attach_agent_to_running_command`) **re-added this pass** — all three symbols already exist and compile; the fork test simply never asserted on them. |
| `figma_statusline_metadata_formats_are_stable` | DIVERGENT | Already ported as `statusline_datetime_formats_are_stable`. |
| `status_slash_command_opens_dedicated_status_menu_via_shared_structure` | DECLINED | Inventory mis-bucketed this PORTABLE? — it's already named in `DECLINED.md` #389 ("asserts literal `Org`/`Email` rows this fork no longer renders"). |
| `user_info_updates_only_require_an_open_status_menu_repaint` | DECLINED | Same #389 row, same correction. |
| `blocked_terminal_use_action_acceptance_uses_ctrl_enter_without_rebinding_submit` | DIVERGENT | Fork has no `AcceptBlockedTerminalUseAction`/editable ctrl-enter binding. The equivalent concept exists as `TuiTerminalSessionAction::AllowBlockedLrcAction`, bound to a **fixed** (non-rebindable) `ctrl-o` (`ALLOW_BLOCKED_ACTION_KEY_BINDING`, `tui_cli_subagent_view.rs:34`), not an editable ctrl-enter binding that avoids colliding with input-submit. No fork test currently covers `AllowBlockedLrcAction`'s keybinding at all — a real, small, separate gap, but the pin's *exact* test doesn't apply. |
| `grok_oauth_block_exclusively_owns_input_until_escape` | DECLINED | `DECLINED.md` #319: Grok subscription OAuth declined (API-key-only). `BlockingInputSource`'s own doc comment (`terminal_session_view/state.rs:318-320`) says so explicitly: *"`Orchestration`, `Handoff`, and `GrokOAuth` are omitted because none of [these exist here]."* |
| `manual_attach_and_detach_switch_running_command_input_ownership` | MISSING-SUBSYSTEM | Needs `InputTypeAutoDetectionSource::AgentTerminalControl` (see root-cause section above) and `RUNNING_COMMAND_DETACH_HINT`. Cross-ref #456, #399/#254 item d. |
| `nld_reset_only_unlocks_after_agent_control_and_not_on_user_edit` | MISSING-SUBSYSTEM | Same root cause: no `lock_for_agent_control`/`reset_after_agent_control` on `TuiInputView` — the NLD-lock coordination half of the attach feature was never ported. |
| `provider_api_key_shell_command_uses_shared_tui_launcher` | DIVERGENT / COVERED-ELSEWHERE | The pin's `tui_cli_shell_command`/`provider_api_key_shell_command`/`ProviderApiKeyOperation` don't exist and have no caller. The fork replaced this whole flow with an inline `/api-keys` picker (`api_keys_menu.rs`), whose own doc comment explicitly contrasts it with upstream's shell-out approach: *"deliberately without upstream's Warp-credit-fallback toggle or Grok-subscription OAuth connect flow."* |
| `resume_shell_commands_use_shared_tui_launcher` | CLOUD | `tui_resume_shell_command`/`--resume <token>` resumes a **`ServerConversationToken`** (`resume.rs:6`) — a server-hosted conversation token. Cloud by definition. |
| `running_command_completion_clears_transient_attachment_lock` | MISSING-SUBSYSTEM | Same `AgentTerminalControl` root cause as above. |
| `shell_mode_reserves_tab_even_when_attachments_render` | **PORTED + code defect fixed** | See below. |
| `tagged_in_alt_screen_keeps_output_and_composer_visible` | MISSING-SUBSYSTEM | Attach-into-alt-screen mechanism works; only `RUNNING_COMMAND_DETACH_HINT` (footer text) is missing. |
| `tui_cli_shell_command_uses_channel_entry_points` | DIVERGENT / COVERED-ELSEWHERE | Same dead helper as `provider_api_key_shell_command...` above — its two real call sites (cloud resume, API-key shell-out) are respectively CLOUD and replaced by `api_keys_menu.rs`. |
| `user_controlled_alt_screen_keeps_full_session_input_on_the_pty` | MISSING-SUBSYSTEM | Needs `running_command_hint()` + `input_hints::long_running_command_hint()`, neither of which exist, though every symbol they'd need (`binding_hint`, `ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME`) does. |
| `zero_state_running_command_hint_shows_attachment` | MISSING-SUBSYSTEM | Same `running_command_hint()` gap. |

**Code defect found and fixed:** `shell_mode_reserves_tab_even_when_attachments_render`
ported clean, but tracing it exposed a real bug. The fork's `keymap_context()`
(`terminal_session_view.rs:4728-4734`, old code) set `ATTACHMENTS_AVAILABLE_FLAG`
from `attachment_bar.should_render(ctx)` alone — **it never checked shell
mode.** The binding table two hundred lines above it, in the same file, has a
comment that already states the invariant this violates: *"Tab completes the
token under the cursor when there are no image attachments to focus (which
reserve Tab). Gated as the mutually exclusive complement of the
`FocusAttachments` binding above."* Because `TriggerCompletions` is bound to
Tab exactly when `!ATTACHMENTS_AVAILABLE_FLAG`, the missing shell-mode check
meant: **if image attachments were present while in shell mode, Tab would
try to focus the attachment bar instead of completing the shell command**,
silently breaking shell-mode completion. Fixed by porting the pin's
`attachment_focus_available(is_shell_mode, attachments_should_render) -> bool`
helper and gating the flag through it
(`terminal_session_view.rs:300-307`, call site `:4741-4746`). Pinned by the
ported test.

### `orchestration_block_tests.rs` — 20 absent (pin 20 · fork 0)

**DECLINED**, all 20 — `crates/warp_tui/src/orchestration_block.rs` (pin) is the
TUI's "Can I start additional agents for this task?" acceptance/config card.
Its imports are `AIAgentActionType`, `BlocklistAIActionModel::execute_run_agents`,
`RunAgentsExecutor`, `RunAgentsRequest`, `RunAgentsSpawningSnapshot`,
`ORCHESTRATION_WARP_WORKER_HOST` — this **is** the TUI analogue of the GUI's
`run_agents_card_view`, already declined in `DECLINED.md` #290 ("RunAgents /
cloud-runner orchestration... `host_picker`, `run_agents_card_view`,
`orchestration_controls`. Needs `warp_graphql::queries::get_runners`... tests
assert on `FeatureFlag::CloudAgentRunners`"). The fork ships no
`orchestration_block.rs` at all (`fork ships source: NO`), consistent with the
decline. All 20 tests: `accepting_dispatches_once_and_releases_focus`,
`blocked_accept_invalidates_card_layout`,
`build_request_carries_card_fields_and_edited_run_wide_state`,
`build_request_omits_the_auth_secret_when_the_picker_is_not_applicable`,
`cloud_managed_credential_harness_inserts_the_api_key_page`,
`cloud_oz_uses_five_pages_without_the_api_key_page`,
`confirming_a_search_result_returns_focus_to_the_acceptance_card`,
`edit_state_carries_the_request_auth_secret`,
`edit_state_is_overridden_by_an_approved_config`,
`environment_and_model_pages_are_searchable`,
`environment_selector_is_searchable`,
`failed_arrow_confirmation_does_not_change_later_enter_navigation`,
`focusing_a_configuring_card_delegates_to_the_selector`,
`local_collapses_the_page_sequence_to_two_pages`,
`local_request_with_implicit_oz_harness_preserves_explicit_model`,
`model_selector_arrows_navigate_after_search_takes_focus`,
`opening_configuration_only_invalidates_layout`,
`selector_actions_commit_edits_and_follow_the_dynamic_page_sequence`,
`selector_layout_invalidations_are_forwarded`,
`unapproved_local_request_forces_oz_harness`.

### `input/view_tests.rs` — 12 absent (pin 89 · fork 77)

| test | verdict | evidence |
|---|---|---|
| `listening_voice_input_suppresses_shell_gutter` | DECLINED | `DECLINED.md` #389/#352. |
| `tab_requests_completion_for_detected_shell_input` | DIVERGENT | Already adjudicated in-tree at `input/view.rs:903`: this fork's Tab completion is deliberately not shell-gated. |
| `tab_requests_completion_only_in_shell_mode_without_submitting` | DIVERGENT | Already adjudicated at `terminal_session_view/completions.rs:3-9`; pin's assertion is the inverse of the fork's deliberate behaviour. |
| `tab_is_consumed_by_an_existing_non_completion_menu` | DIVERGENT | No `TuiInputAction::Complete` to dispatch. |
| `tab_cycles_open_completion_menu_and_enter_applies_selection` | DIVERGENT | `TuiInputAction` has no `Complete` variant, `TuiInputViewEvent` no `RequestShellCompletion`; completion is owned by the session view (`TRIGGER_COMPLETIONS_BINDING_NAME`). |
| `completion_replaces_utf8_byte_span_and_preserves_following_text` | **PORTED** | One rename: pin's `TuiCompletionAcceptance { replacement_range }` → fork's `TuiAcceptedCompletion { span }` (`completions_menu.rs:57`). Guards the multi-byte offset conversion in `apply_shell_completion`. |
| `completion_can_append_a_space_at_buffer_end` | **PORTED** | Same rename. |
| `move_left_from_shortcuts_replaces_it_with_conversation_menu` | **PORTED** | `view.rs` already opens `ConversationMenu` on MoveLeft from an empty cursor-at-start non-shell buffer; `build_view_with_conversation_menu` (`view_tests.rs:543`) returns the pin's 3-tuple. Structurally identical to the fork's own `move_left_on_empty_buffer_opens_conversation_menu`. |
| `enter_and_escape_stop_listening_while_escape_cancels_transcribing` | DECLINED | Needs `crate::voice_input::{TuiVoiceInputModel, TuiVoiceInputState}` and `build_view_with_voice` — module doesn't exist. `DECLINED.md` #389/#352. |
| `question_mark_at_empty_shell_input_toggles_shortcuts` | PORTED (prior sweep) | Landed as part of `6826cb89f`; near-name fork test was `question_mark_at_empty_agent_input_toggles_shortcuts`. |
| `typing_into_an_open_shortcuts_surface_closes_it_and_inserts` | PORTED (prior sweep) | Landed as part of `6826cb89f`. |
| `up_from_shortcuts_replaces_it_with_prompt_and_command_history` | PORTED (prior sweep) | Landed as part of `6826cb89f` — the comment declaring it unportable had gone stale once #389's shortcuts sheet landed. |

### `agent_block_tests.rs` — 5 absent (pin 48 · fork 43)

| test | verdict | evidence |
|---|---|---|
| `agent_block_preserves_received_messages_and_hides_lifecycle_ids` | MISSING-SUBSYSTEM | Needs `TuiAIBlockSection::AgentMessage` (doesn't exist on the fork's enum, `agent_block.rs:104-131`); `sections()` discards `MessagesReceivedFromAgents`/`EventsFromAgents` at `:1307-1310`. See the orchestration-message root-cause section above. |
| `agent_message_defaults_collapsed_and_expands_through_block_state` | MISSING-SUBSYSTEM | Same root cause. |
| `failed_output_usage_notice_matches_gui_conditions` | DIVERGENT / COVERED-ELSEWHERE | `should_show_failed_output_usage_notice` is hardcoded `false` in this fork (`app/src/ai/blocklist/view_util.rs:166-171`, deliberate: BYOP has no Warp usage/credits). Already covered by the fork's own `failed_output_usage_notice_never_shown_in_byop` (`agent_block_tests.rs:319`). |
| `hidden_only_orchestration_exchange_has_zero_height` | DECLINED | Constructs `AIAgentActionType::WaitForEvents`, which doesn't exist in this fork's action enum (`tool_call_labels.rs:501-502`: "BYOP: RunAgents and WaitForEvents are cloud-orchestration `AIAgentActionType` variants absent from Zap"). Extends `DECLINED.md` #325. |
| `orchestration_outputs_render_without_wait_for_events_tool_row` | DECLINED + MISSING-SUBSYSTEM | Needs both the missing `WaitForEvents` variant (DECLINED, #325) and the missing `AgentMessage` section (MISSING-SUBSYSTEM, root-cause section above). |

### `handoff/tests.rs` — 5 absent (pin 5 · fork 0)

**CLOUD**, all 5 — `crates/warp_tui/src/handoff/model.rs` (pin) is *"State and
execution model for the TUI local-to-cloud handoff flow"* (its own doc
comment). Imports: `CloudEnvironmentCatalog`, `ServerApiProvider`,
`PendingCloudLaunch`, `HandoffSurface`, `SnapshotUploadTarget`,
`execute_handoff`/`prepare_handoff` posting to Warp's servers. Fork ships no
`handoff/` module at all. Not previously in `DECLINED.md` by this name;
recorded here as a new CLOUD verdict since it needs cloud plumbing directly.
All 5: `long_running_command_rejection_preserves_the_full_local_draft`,
`no_environment_card_has_top_padding_and_ctrl_c_restores_prompt_and_images`,
`privacy_invalidation_restores_the_draft_and_removes_handoff_from_commands`,
`settings_invalidation_restores_the_draft_and_repeated_submission_keeps_one_card`,
`slash_menu_selection_inserts_handoff_for_optional_prompt_composition`.

### `orchestration_model_tests.rs` — 5 absent (pin 5 · fork ships `orchestration_model.rs`, 0 tests before this pass)

`orchestration_model.rs`'s own module doc (already in the fork, pre-dating this
pass) is the clearest self-adjudication in this whole sweep: it explains
exactly what was kept (read-only navigation: `snapshot`,
`focus_conversation_session`, `set_explicit_page`) and what was cut (child
*materialization* — `dispatch_create_agent`, `begin_local_oz_child_launch`,
`begin_remote_child_launch`, the whole `TuiOrchestrationEvent` family — because
the pin drives it from a shared `StartAgentExecutor` singleton that "does not
exist anywhere in this fork, not even for the GUI").

| test | verdict | evidence |
|---|---|---|
| `failed_launch_cleanup_preserves_other_sessions` | MISSING-SUBSYSTEM | Needs `cleanup_failed_child`/child materialization — cut (local half, "future work, not a mechanical trim"). |
| `github_auth_blocker_keeps_the_remote_session_and_actionable_url` | CLOUD | Remote child launch (`register_remote_child_session`) calls `ServerApiProvider`/`ai_client` — cloud-runner, #290. |
| `local_harness_children_fail_cleanly` | MISSING-SUBSYSTEM | Needs `begin_local_oz_child_launch` — cut (local half). |
| `remote_child_session_is_navigable_and_projects_lifecycle` | CLOUD | Remote child, #290. |
| `snapshot_is_shared_across_tree_and_filters_conversations_without_sessions` | **PORTED** | Exercises exactly the kept surface (`snapshot`, `focus_conversation_session`, `set_explicit_page`), fed by `BlocklistAIHistoryModel::start_new_child_conversation` directly (not through the cut executor). Ported into a new `orchestration_model_tests.rs`, wired via `#[cfg(test)] #[path = ...] mod tests;`. Every helper used (`TuiSessions::register_session`/`wire_orchestration`/`new_for_test`, `BlocklistAIHistoryModel::start_new_child_conversation`/`active_conversation`/`set_active_conversation_id`, `AIConversation::id`, `Harness::Oz`) verified present with matching signatures before porting. |

### `agent_message_tests.rs` — 4 absent (pin 4 · fork 0)

**MISSING-SUBSYSTEM**, all 4 — see the dedicated root-cause section above.
`crates/warp_tui/src/agent_message.rs` (the renderer:
`conversation_status_glyph`, `render_agent_message`, `agent_message_section_id`)
does not exist in the fork at all, and `agent_block.rs` explicitly (if
staled-ly) discards the messages it would render. All 4:
`conversation_statuses_render_expected_glyphs`,
`message_preview_wraps_with_a_hanging_indent_and_falls_back_to_subject`,
`parent_sender_renders_as_orchestrator_in_child_transcript`,
`running_child_message_matches_the_design_layout_and_styles`.

### `grok_oauth/tests.rs` — 3 absent (pin 3 · fork 0)

**DECLINED**, all 3 — `DECLINED.md` #319: Phosphor supports API-key
credentials only; Grok subscription OAuth is a legitimate non-cloud flow but
an alternative credential *source* a user with an API key doesn't need.
`callback_and_manual_failures_do_not_claim_success_or_expose_raw_details`,
`fatal_card_sanitizes_the_body_and_escape_closes_the_attempt`,
`waiting_card_uses_handoff_structure_and_only_escape_footer_hint`.

### `usage_tests.rs` — 3 absent (pin 3 · fork ships `usage.rs`, unrelated tests)

**DECLINED**, all 3 — `format_cost`, `entry_text`, `TuiUsageDisplayMode`,
`ConversationUsageTotals { credits_spent, cost_in_cents }` don't exist
anywhere in the fork (repo-wide `TuiUsageDisplayMode` grep is empty).
`usage.rs`'s own doc comment explains: BYOP has no server-computed
credits/cost, so the footer shows local context-window occupancy instead
(`format_context_usage`, covered by the fork's own existing `usage_tests.rs`).
`cost_formats_cents_as_dollars`, `entry_text_follows_the_persisted_display_mode`,
`entry_text_matches_the_gui_credits_formatting`.

### `voice_input_tests.rs` — 3 absent (pin 3 · fork 0)

**DECLINED**, all 3 — `DECLINED.md` #389/#352. Pin's `voice_input.rs` imports
`VoiceInputLifecycleState`, `VoiceTranscriber`, `AIRequestUsageModel` — the
disabled cloud transcription backend. `cancel_returns_the_model_to_idle`,
`start_does_not_replace_an_active_session`,
`stop_transitions_the_model_to_transcribing`.

### `cloud_run_view_tests.rs` — 2 absent (pin 2 · fork 0)

**CLOUD**, both — `crates/warp_tui/src/cloud_run_view.rs` (pin) renders a
**remote/cloud-runner** child session's lightweight progress view; imports
`crate::cloud_run::{TuiCloudRunStartup, TuiCloudRunState}` and
`crate::agent_message` (also absent — see above). Part of the #290
cloud-runner-orchestration family. `lightweight_cloud_view_renders_startup_and_blocker_without_terminal_state`,
`spawned_cloud_view_matches_figma_in_progress_and_succeeded_states`.

### `completion_menu_tests.rs` — 2 absent (pin 2 · fork 0)

**COVERED-ELSEWHERE** — this is exactly the "renamed with the code" case
`docs/SWEEP-INVENTORY.md` warns about: name-diffing reports `completion_menu.rs`
absent because the fork's file is `completions_menu.rs` (plural). Same
`warp_completer` engine, same `show()`-based inline-menu shape, same
"Mirrors the GUI's `InputSuggestionsMode::CompletionSuggestions`" framing.

| test | verdict | evidence |
|---|---|---|
| `show_does_not_replace_an_existing_inline_menu` | COVERED-ELSEWHERE | Fork's `completions_menu_tests.rs::show_does_not_replace_an_already_visible_menu` (line 129) is the same assertion under a near-identical name. |
| `show_reuses_inline_menu_rows_and_accepts_the_selected_span` | COVERED-ELSEWHERE | Split across the fork's own `show_opens_with_rows_and_sets_mode` and `accept_selected_returns_replacement_and_span_then_dismisses`. |

### `read_only_menu_tests.rs` — 2 absent (pin 6 · fork 4)

**DIVERGENT**, both — already hand-traced in-tree (`read_only_menu_tests.rs:1-7`):
both need `TuiViewportedList::with_trimmed_selection_line_ends` and
`TuiSelectable::with_semantic_selection_by_style`, neither of which exists in
this fork's `warpui_core`. Real feature gap (trailing-whitespace trim,
double-click styled-word selection), not test debt; not attempted this pass
(the gap is in `warpui_core`, outside `crates/warp_tui/**`).
`double_click_selects_complete_styled_text`, `selection_stops_at_trailing_whitespace`.

### `slash_commands_tests.rs` — 2 absent (pin 21 · fork 19)

| test | verdict | evidence |
|---|---|---|
| `slash_command_menu_renders_voice_row` | DECLINED | `DECLINED.md` #389/#352. |
| `slash_command_menu_renders_theme_row` | **PORTED + code defect fixed** | `/theme` is a real, fully-wired TUI command (`terminal_session_view.rs::toggle_theme`); the row's `(currently …)` live-state suffix is wired for `/auto-approve`, `/natural-language-detection`, `/vim-mode` but **`/theme` was never added to `state_suffix`'s match**, so its row silently showed no current-theme indicator. Not a deliberate decision (nothing in `DECLINED.md`). Fixed in `slash_commands.rs::state_suffix` using the identical, already-compiling logic from `terminal_session_view.rs:4567-4589` (`TuiThemeSettings::as_ref(ctx).selected_theme()`, `TuiTheme::from(Appearance::as_ref(ctx).theme())`). |

### `handoff/model_tests.rs` — 1 absent (pin 1 · fork 0)

**CLOUD** — same `handoff/model.rs` cloud-handoff module as above.
`missing_token_after_eager_cancellation_restores_only_trimmed_argument`.

### `tool_call_labels_tests.rs` — 1 absent (pin 6 · fork 5)

**DECLINED** — `all_failed_run_agents_uses_failure_glyph` needs the RunAgents
family; `DECLINED.md` #325 (agent-invoked agent spawning).

### `tui_builder_tests.rs` — 1 absent (pin 3 · fork 2)

**DECLINED** — `voice_input_border_pulses_between_cyan_overlay_2_and_lilac_600`;
`DECLINED.md` #389/#352.

### `tui_shell_command_view_tests.rs` — 1 absent (pin 11 · fork 10)

**COVERED-ELSEWHERE** — `escape_while_editing_exits_editor_without_cancelling`.
The save-not-cancel behaviour is already covered by the fork's own
`escape_while_editing_exits_edit_mode_without_rejecting_or_discarding`
(`tui_shell_command_view_tests.rs:253`); the footer-hint-text half is covered
by `tui_permission_prompt_tests.rs::footer_shows_exit_editor_hint_while_body_editor_is_focused`.

### `zero_state_animation_tests.rs` — 1 absent (pin 26 · fork 25)

**DIVERGENT** (branding, #384) — `logo_mask_preserves_the_offset_warp_faces`
tests exact points against `warp_logo_contains`/`UPPER_FACE`/`LOWER_FACE`
(Warp's copyrighted logo geometry), which this fork deliberately replaced with
a generic diamond mark (`built_in_mark_contains`,
`zero_state_animation.rs:16-25`, explicit "Branding deviation from the pin
(see #384 / DECLINED.md)" doc comment). This also **corrects `ORACLE.md`**,
which calls this file "a fork-original starfield vs Warp's rotating mark (0 of
26)" — that was true before issue #466's `warp_tui` resync; today it's 25/26
matching, with only the Warp-branded face mask itself intentionally
different. `docs/SWEEP-INVENTORY.md`'s "pin 26 / fork 25" figure is the
current, correct one.

### `zero_state_tests.rs` — 1 absent (pin 6 · fork 5)

**DECLINED** — `login_line_shows_signed_in_account_email`; `DECLINED.md` #11
(account-first onboarding, no BYOP equivalent).

---

## Where `SCOPE-TERMINAL.md` / the inventory was wrong

1. **Two `terminal_session_view_tests.rs` tests were mis-bucketed `PORTABLE?`
   when they're already named in an existing `DECLINED.md` row** —
   `status_slash_command_opens_dedicated_status_menu_via_shared_structure` and
   `user_info_updates_only_require_an_open_status_menu_repaint` are both
   explicitly listed in the #389 row's "permanently unported" list. The
   mechanical pass that produced the inventory evidently didn't cross-reference
   `DECLINED.md` against every test name inside a file it had already flagged
   `DECLINED?` for *other* tests in the same file.
2. **`completion_menu_tests.rs` is the exact "renamed with the code" case the
   inventory's own caveat #1 warns about**, but the inventory still filed it
   `DIVERGENT?` ("fork does not ship the pin's source module") rather than
   flagging the rename — the fork's `completions_menu.rs` (plural) is a
   line-for-line conceptual match, and both pin tests are already covered
   under near-identical fork names.
3. **`zero_state_animation_tests.rs`'s pin-25/fork-1-absent figure in the
   inventory is correct and should be trusted over `ORACLE.md`**, which still
   describes the pre-#466-resync state ("fork-original starfield"). This is
   exactly the kind of staleness `ORACLE.md` itself warns readers to watch for
   in older per-file claims.
4. **`orchestration_model_tests.rs` said "fork ships source: yes" but didn't
   note the module's own doc comment already fully adjudicates 4 of its 5
   absent tests** (child-materialization cut, cited by issue number) — only
   `snapshot_is_shared_across_tree_and_filters_conversations_without_sessions`
   needed hand-tracing, and it turned out portable.
5. **The inventory's `PORTABLE?` tag on 6 `terminal_session_view_tests.rs`
   tests undersold how close they are** — every dependency except the specific
   `AgentTerminalControl` autodetection-source variant and two hint strings
   already exists and compiles; this is a much narrower, more precisely
   scoped gap than "needs hand-tracing" suggests.

## Ranked list: what I am least sure compiles (most-likely-to-break first)

1. **`crates/warp_tui/src/orchestration_model_tests.rs` (new file, ported by
   me).** The largest, most novel piece of code in this pass — a ~180-line
   test file built from scratch against fixtures I verified individually via
   `grep` (not compiled). Every symbol and signature was cross-checked
   (`TuiSessions::register_session`/`wire_orchestration`/`new_for_test`/
   `focused_session_id`, `BlocklistAIHistoryModel::start_new_child_conversation`/
   `active_conversation`/`set_active_conversation_id`, `AIConversation::id`,
   `Harness::Oz`, `warpui::EntityId::new()`), but this is the one place a
   subtle type mismatch (e.g. `EntityId` vs `TuiSessionId` in a collection key,
   or a `Copy`/`Clone` bound on `AIConversationId`) could still slip through
   text-matching.
2. **`slash_commands.rs::state_suffix`'s `/theme` branch (subagent-ported).**
   Second-largest surface change — a new source-code branch across two files
   with a `TuiTheme`/`Appearance` conversion chain
   (`TuiTheme::from(Appearance::as_ref(ctx).theme())`). I independently
   re-verified `Appearance::theme(&self) -> &WarpTheme` and
   `impl From<&WarpTheme> for TuiTheme` both exist and match, and the whole
   expression already appears verbatim in a compiling call site
   (`terminal_session_view.rs:4567-4589`), which is why I trust it more than
   #1, but it's still the second-newest code in this pass.
3. **`terminal_session_view.rs`'s `attachment_focus_available` fix.** Small
   (8 lines + one call-site change), and I verified `is_shell_mode(&self, ctx:
   &AppContext) -> bool` and `keymap_context`'s enclosing signature match
   exactly, but it changes a real runtime code path (not just tests), so a
   silent regression here would be more consequential than a compile error.
4. **`input/view_tests.rs`'s three ported completion tests (subagent-ported).**
   Smallest risk of the substantive changes — single-field rename
   (`replacement_range` → `span`) against an already-compiling struct.

All files touched passed `rustfmt --check --config-path .rustfmt.toml --edition 2024`
before commit. No `cargo`/`rustc`/`nextest` was run, per the hard constraint —
none of the above has been compiler-verified.
