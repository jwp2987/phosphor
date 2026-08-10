# Declined parity

Deliberate decisions **not** to match the pinned oracle. These are choices, not
debt, and not oversights.

Read this before filing a parity issue or porting a subsystem. Several entries
exist because an agent found a gap, filed it as debt, and the gap turned out to
be a decision already made. `SCOPE-*.md` says what is *absent*; this file says
what is absent **on purpose**.

The oracle is pinned — see `ORACLE.md`. Every entry below cites the issue where
the decision was argued, so the reasoning is recoverable rather than folklore.

## How to use this file

- **Before porting anything**, check whether it is listed here.
- **If you disagree with a decision**, reopen the cited issue and argue there.
  Do not silently port around it — the cloud-boundary guard
  (`script/check_cloud_boundary`) will usually stop you, but not always.
- **If you make a new declined-parity decision**, add it here in the same PR.
  A decision that lives only in a closed issue will be re-litigated by the next
  agent that trips over the gap.

---

## Cloud — out of scope by definition

Phosphor drops Warp's cloud backend. These are not gaps.

| what | issue | note |
|---|---|---|
| **Warp Environments** | #211 | Cloud-backed. 75 pinned tests are out of scope, not parity debt. |
| **RunAgents / cloud-runner orchestration** | #290 | `host_picker`, `run_agents_card_view`, `orchestration_controls`. Needs `warp_graphql::queries::get_runners` (crate deleted), `crate::server::experiments`, `crate::server::server_api`; tests assert on `FeatureFlag::CloudAgentRunners`. **Caveat:** a few `run_agents_card_view` cases exercise a *Local* execution-mode variant that may be a legitimate non-cloud feature — tracked under #11, not declined. |
| **Cloud teams / org policy** | #445 | `UserWorkspaces::current_team()` returns `None` unconditionally — a deliberate BYOP decision, already documented in an `#[ignore]` at `app/src/cloud_object/model/model_test.rs`. Consequence: the org/workspace command denylist is inert. Whether a **local** workspace policy layer is wanted is still open on #445. |
| **Account-first onboarding, billing, paid tiers** | #11 | `account_class`, `is_paid`, `has_team`, upgrade flows. No BYOP equivalent. |
| **`/logout` slash command** | #338 | `crate::tui::log_out_tui` (its dispatch target) is a documented no-op: "BYOP has no account to log out of." Registering `/logout` would add a row to the `/` menu that does nothing when selected — the dispatch code existing does not make this a wiring gap, unlike `/exit`/`/mcp`/`/view-logs`/`/auto-approve`/`/natural-language-detection`/`/clear` in the same issue. |
| **`/voice` slash command** | #11 | `VoiceInputLifecycle` — KEEP-DROPPED, maintainer 2026-08-02: the voice transcription backend (Wispr) is cloud and dropped. |
| **`ActiveAgentViewsModel`** | #418 | Deleted with the cloud management view it was the state source for — see the module doc at `app/src/notifications/model.rs`. The fork substituted a working equivalent: `is_conversation_open` became a `BlocklistAIHistoryModel::conversation()` check for "is the conversation still in memory" (`model.rs:275`). **This is invisible to the import test**, because the symbol is simply absent rather than importing anything cloudy, so it keeps resurfacing as apparent debt. **Corrected 2026-08-08:** only one of #418's three pinned "conversation transfer" tests actually needs it — `clicking_old_banner_for_open_conversation_focuses_current_terminal_surface_without_transferring_blocks`, which calls `ActiveAgentViewsModel::register_agent_view_controller`/`terminal_view_id_for_conversation` directly. That one test is permanently out of scope; **deleted**, not ignored. `RestoreConversationEntryBehavior` is unrelated to `ActiveAgentViewsModel` and does **not** require it — it was ported in #418 (a `pub enum` on `TerminalView` plus a threaded parameter on `restore_conversation_after_view_creation`/`restore_conversations_from_block_params`), and the other two conversation-transfer tests (`appended_exchange_renders_in_current_terminal_surface_after_conversation_transfer`, `restoring_conversation_to_new_pane_transfers_blocks_from_previous_terminal_surface`) use it and both **pass** as of the #418 branch (verified via `nextest`, 2026-08-08). The previous wording of this entry ("all are permanently out of scope") overstated the blocker for those two — verify before citing this entry again. |

## Provider credentials — API keys only

| what | issue | note |
|---|---|---|
| **xAI / Grok subscription OAuth** | #319 | Phosphor supports **API-key credentials only**. The flow is genuinely local (OAuth2+PKCE direct to `auth.x.ai`, loopback `127.0.0.1:56121`, public Grok-CLI `client_id`) — it is *not* a cloud drop — but it is an alternative credential *source*, and a user with an xAI API key is already fully served. Keeps 5 `grok_subscription` tests and 24 `grok_*` tests in `api_keys_tests.rs` out of scope. Self-contained if ever revisited (~492 lines). |

## Telemetry and crash reporting — deliberately asymmetric

This one has been mis-diagnosed more than once. Read carefully before "fixing"
it.

| what | state | why |
|---|---|---|
| **Telemetry** | Channel physically removed. `ChannelState::is_telemetry_available()` is hard-coded `false`, so `AppAnalyticsWidget::should_render` returns `false` and **the toggle is never shown**. `should_collect_ai_ugc_telemetry()` returns `false`, ignoring the setting. | Nothing is sent. The `IsTelemetryEnabled` setting and its widget are retained so the control reappears automatically if a telemetry channel is ever wired up. |
| **Crash reporting** | Toggle **is** shown and **is** functional. | `is_crash_reporting_available()` is `false` (nothing is ever *uploaded*), but the setting is not a no-op: `crash_reporting::init` subscribes to it, and enabling it installs the panic hook in `init_local_crash_reporting` that writes a full backtrace to the **local** log. |

**Do not remove either toggle.** Removing the crash-reporting one hides a
control that still does something — the exact defect `privacy_page.rs` was
ported to fix. Removing the telemetry one is a no-op, since it already never
renders. See #165, closed after this was established for the third time.

Warp gates the crash-reporting widget on
`ChannelState::is_crash_reporting_available()`; this fork gates it on
`FeatureFlag::CrashReporting` instead, precisely so a locally-useful control is
not hidden by a cloud-availability check.

## Voice input — recording exists, transcription is cloud and disabled

| what | issue | note |
|---|---|---|
| **Voice-input UI (TUI composer, statusline item, `/voice`)** | #389 | Not a gap — the transcription backend the UI would drive is cloud, and this fork has already turned it off. |
| **Voice input language preference** | #352 | **DECIDED 2026-08-08.** `VOICE_INPUT_LANGUAGES` and `voice_input_language` configure the transcription language for a backend that cannot run here — `VoiceTranscriber::disabled()` since `9d92598c4`, because the BYOP genai protocol cannot carry audio. A setting with no reachable effect. Note `crates/voice_input` **does** exist and does real `cpal` audio capture, which is why this keeps looking live: the capture works, the transcriber it feeds does not. |

`crates/voice_input` (audio capture: `cpal` input stream, resampling, WAV
encoding) is real and already used by the GUI editor
(`app/src/editor/view/voice.rs`). What consumes its output is not: Warp's
`Transcriber` trait (`app/src/voice/transcriber.rs`) is implemented by
`ServerVoiceTranscriber`, which calls `server_api.transcribe` to send audio to
Warp's cloud Wispr speech-to-text — there is no local/BYOP transcription
engine. This fork already made that call: `app/src/lib.rs` constructs
`VoiceTranscriber::disabled()` instead of injecting `ServerVoiceTranscriber`
(commit `9d92598c4`, "Phase 4-1 默认 VoiceTranscriber 改 disabled 跳过云端 Wispr
STT" — default `VoiceTranscriber` changed to disabled, skipping cloud Wispr
STT), and `TranscribeError::Disabled` documents why: "the BYOP genai protocol
can't carry audio." So today, pressing the GUI's mic button *records* audio
successfully and then always fails to transcribe it.

Porting the TUI's voice composer state machine, border animation, and
statusline item (the `crates/warp_tui/src/voice_input.rs` half of #389) would
add a control with the same property in the surface that has none of it
today: it would toggle, animate, and show a hint, and every real use would end
in the same transcription failure the GUI already has. That is worse than no
control — see AGENTS.md's guidance on shipping UI for a backend the fork
doesn't have. Declined until a local/BYOP transcription path exists (e.g. a
local Whisper-class model), at which point both the GUI button and the TUI
composer should be wired to it together.

The read-only menu surface #389 also asks for (`?` shortcuts, `/status`) is
unrelated to voice and is not declined — see the issue for its state.

## Divergences where the fork deliberately differs

| what | issue | note |
|---|---|---|
| **`has_locking_attachment`** | #318 | **DECIDED 2026-08-07: keep the fork's behaviour.** The oracle narrows locking to image/file attachments; the fork also locks on pending block ids and inline `@` context refs. Attaching a block is an explicit "use this as agent context" gesture, and the pin's narrower rule lets the classifier flip the input back to shell mode afterwards — the bug the fork's own comment at `input.rs:12924` describes. The pin's `has_locking_attachment_is_false_with_only_pending_block_id` is **permanently not ported**; the fork's `has_locking_attachment_is_true_with_pending_block_id` is the authority. |
| **`ask_user_question` auto-approve** | #373 | **DECIDED 2026-08-07: keep the fork's behaviour.** Under the pin, a conversation in auto-approve mode makes `can_ask_user_question` return `false`; `should_autoexecute` is its negation, so `execute` returns `SkippedByAutoApprove { question_ids }`. The question is **silently discarded, not auto-answered** — the model asks, nothing reaches the user, and the agent proceeds having never got an answer. The fork's divergence in `permissions.rs` makes auto-approve (ctrl+shift+i) auto-pass *execution* tools only, so a question always surfaces. The pin's `should_autoexecute_returns_true_when_autoapprove_is_enabled_and_profile_allows_override` and `execute_returns_sync_skipped_question_ids_when_autoapprove_is_enabled` are **permanently not ported**; the fork's two inverted tests are the authority. An earlier "mirror Warp" call was reversed once the swallow was traced — PR #489 implemented it and was closed unmerged. |
| **TUI/GUI shared app id** | — | The fork deliberately shares one app id (and therefore one keychain namespace and config) between GUI and TUI; the pin separates them. Two pin tests assert the separation and are intentionally not ported. |
| **Privacy toggle defaults** | — | Warp defaults telemetry/crash-reporting **on** (opt-out, commercial product). This fork defaults them **off** — leaving them on would show "ON" while nothing goes out. |
| **`CustomEndpoint` / `custom_model_providers` on `ApiKeyManager`** | #142, #347 | **DECIDED (PR #189, PR #227 — both merged 2026-08-07): superseded, not ported.** The pin's `ApiKeyManager` has a fixed four-provider list plus a `custom_endpoints: Vec<CustomEndpoint>` field, and `custom_model_providers_for_request` serializes those into a `CustomModelProviders` wire payload that tells Warp's *server* how to proxy to a user's endpoint on their behalf. This fork calls custom endpoints directly via genai (`chat_stream::build_client`) with no server proxy, and the pinned `warp_multi_agent_api` rev has no `CustomModelProviders` message to serialize into at all. The fork's actual BYOP surface for this is `AgentProviderSecrets` (`app/src/ai/agent_providers/secrets.rs`) — arbitrary user-defined providers each with their own `base_url`, a strict superset of the pin's fixed-four-plus-custom-endpoints design, with its own coverage (`app/src/ai/agent_providers/mod_test.rs`, 13 tests; `secrets_tests.rs`, 6 tests). Porting `CustomEndpoint` into `crates/ai/src/api_keys.rs` would stand up a second, competing provider store. Keeps 16 `api_keys_tests.rs` pin tests permanently unported — see that file's header comment for the per-test list. **#347 (`app/src/ai/llms.rs`), assessed 2026-08-07: also superseded, not a live gap.** #347 has two clusters. Cluster 2 (`CustomEndpoint`/`CustomEndpointModel`/`build_custom_llm_infos`, 8 tests, plus 5 more that depend on it) is this same declined surface viewed from `llms.rs` instead of `api_keys.rs` — the pin's `build_custom_llm_infos` takes `ai::api_keys::ApiKeys::custom_endpoints` as its direct input, so porting it would resurrect the exact competing provider store this row already declines. Cluster 1 (`DisableReason::should_clear_preference`, `is_usable_llm`, `usable_default_llm_info`, `should_show_host_icon_for_model`, 7 tests) exists in the pin to reconcile a *cloud*-fetched model list — subject to org-admin/quota/upgrade disables and AWS Bedrock / Gemini Enterprise host routing — against a BYOK override. Phosphor's model list is built entirely locally from `AgentProviderSecrets` (`LLMPreferences::new`'s own comment: "BYOP-only mode ... no longer consumes the upstream cloud model list at all"), so `DisableReason::AdminDisabled`/`OutOfRequests`/`ProviderOutage`/`RequiresUpgrade` are never constructed by any live code path (repo-wide grep finds only dead rendering-only branches in `execution_profiles/model_menu_items.rs` and `terminal/input/models/data_source.rs` that would fire if they ever were) and `LLMInfo::host_configs` is always the empty map. The fork's disable handling already works, by a different mechanism: `build_byop_llm_infos` (`app/src/ai/agent_providers/mod.rs`) omits disabled models from `choices` outright, and `LLMPreferences::on_server_update` already clears any profile preference pointing at a model id that dropped out of `choices` — no `should_clear_preference` distinction is needed because there is no disabled-but-still-BYOK-usable case to distinguish. `is_usable_llm`/`usable_default_llm_info` also cannot be ported standalone: the pin's own implementation falls back into `custom_llm_choices`, i.e. cluster 2. |
| **`FEATURE_INTROS` content (feature-intro popover)** | #404 | The pin's `OneTimeModalModel` grew a reusable "feature intro" popover mechanism (`app/src/workspace/view/feature_intro_modal/`) — a data-driven registry (`FEATURE_INTROS: &[FeatureIntro]`) plus a non-blocking popover view, model wiring, and per-id "seen" tracking on `AISettings`. The mechanism itself is generic and non-cloud, and is ported. Its only registered entry (`FeatureIntroId::CustomModelRouter`) is not: it promotes a Warp-hosted custom-model-router feature this fork does not have, using Warp-branded marketing copy ("Build a custom model router for the Warp Agent..."), the same category of content declined for the "Oz updates" zero-state section (#321). `FEATURE_INTROS` ships empty in this fork; `FeatureIntroId`'s only variant is a `#[cfg(test)]` fixture. Populate it again once there is a real, non-cloud Phosphor feature worth announcing this way — that is a content decision, not a mechanism gap. |

## Retired features (decision pending on some)

| what | issue | note |
|---|---|---|
| ~~**Codebase indexing / `RepoOutlines`**~~ | #11 | **ROW WITHDRAWN 2026-08-08 — it was already false when last edited.** It said "deliberately removed... keep-dropped-vs-restore decision pending", but BOTH halves were restored days earlier: `RepoOutlines` in `a09653678` (2026-08-03) and `SearchCodebase` in `257174c30` (2026-08-04), four to five days before this row's last edit (`afbaedb3`, 2026-08-08). Nothing here is declined. Kept as a struck-through row rather than deleted, because the same claim is echoed in older notes and issues that readers may still hit. |
| **Screen recording** | #367 | **Declined.** Not cloud — the pin's capture is local `ffmpeg`, and only `upload_artifact` touched Warp's servers — but Phosphor is not shipping it. 26 pinned tests are out of scope. **Nothing to remove:** the subsystem was never ported, so there is no `recording_controller` / capture / finalize code here. What remains are `Tool::StartRecording` / `StopRecording` variants on the shared `warp_multi_agent_api` types, handled as unreachable no-ops — those must stay for exhaustive matching against the external crate. |
| **`SettingSurfaces` / `SettingsMode`** | — | Explicitly documented as dropped in `app/src/settings/ai.rs` and `tui_autoupdate.rs`. |
| **warp.dev Drive link resolution** | #267 | **DECIDED 2026-08-07: keep as dead code.** The fork still resolves `warp.dev/drive/...` links into `ZapDriveObjectArgs`. Warp Drive is the cloud product and nothing here can service such a link, but the resolution path is retained deliberately rather than ripped out. Its two tests and its two `cloud_boundary_allowlist.txt` entries (`uri_test.rs` → `cloud_object::ObjectType`, `server::ids::ServerId`) stay — **do not "clean up" the allowlist by removing them.** |
| **`>` vs `>=` restore-order tie-break** | #174 | **DECIDED 2026-08-07: declined.** `conversation_restoration` breaks ties differently from the pin. The fork's choice was previously justified only in a source comment, with three pin tests marked "intentionally NOT ported" — that is now a recorded decision rather than an undocumented divergence. Those three tests stay unported permanently. |
| **Orchestration persistence fields** | #376 | **DECIDED 2026-08-07: declined, cloud.** `AgentConversationData` lacks `pinned`, `is_remote_child`, `orchestration_harness_type`, `root_task_is_optimistic`. They serve the multi-agent "pill bar" UI whose ~15-20 consuming files are entirely absent here. Same cloud family as the RunAgents row above. **Also covers #410's second regression:** the pin's `terminal:cycle_next_orchestration_child_agent` / `terminal:cycle_previous_orchestration_child_agent` editable bindings (`app/src/terminal/view/init.rs`) and their pin test `test_orchestration_cycle_bindings_are_editable` exist only to navigate the same "pill bar" child-agent list (`AgentViewController::adjacent_orchestration_conversation_id`, `Event::RevealChildAgent`) — this decision answers #410's open "needs a scope call" question, so that half stays unported rather than being re-filed as fresh debt. |
| **"Oz updates" zero-state section** | #321 | **DECIDED 2026-08-07: declined.** `ChangelogModel.oz_updates` / `AISettings::should_show_oz_updates_in_zero_state` drive a Warp-branded content feed in the zero state. Not a capability gap — branded content this fork does not carry. |
| ~~**remote_server bundled skills/resources**~~ | #440 | **REVERSED 2026-08-09 — this row was wrong and the feature SHIPPED.** The row claimed `BUNDLED_RESOURCES_DIR_NAME`, `remote_server_bundled_resources_dir` and `remote_server_removal_command` were absent and that Phosphor was not gaining remote-installed bundled skills. All three now exist in `crates/remote_server/src/setup.rs` (lines 462/471/434) with 5 tests, landed by `8dd2b9185` (Rust side) and `56ae8c7d5` (the install-script half that made the directory actually get populated). The release pipeline already tarred a `resources/` tree into the CLI asset the whole time — only the installer template was missing. **Kept as a struck-through row rather than deleted** because a future sweep trusting the original text would misclassify legitimate main-side work as declined; that is exactly how it was caught, by the `integration/round2` salvage audit. |
| ~~**Multi-agent orchestration, entirely**~~ | #304, #309, #310, #325, #329 | **REVERSED 2026-08-08 late — LOCAL orchestration is back in scope; issues reopened.** The original decline (below) was made on the *product* question and was explicitly correct that the code is non-cloud. The product answer changed: the fork already ships the substrate (`local_harness_launch.rs`, `OZ_RUN_ID`/`OZ_PARENT_RUN_ID` parent-child identity) and carries unwired, already-tested local scaffolding dead in the tree (`children_by_parent`, `ChildAgentStatusCard`). ~72 of ~305 orchestration-adjacent pin tests are import-clean of cloud. **Still declined: the cloud-runner half** — #290 RunAgents (children executing on Warp's servers) and credit rollup, which has no BYOP equivalent. Children run as local processes here; `is_remote_child` will be permanently false. Restoring persisted state needs a NEW forward migration — `crates/persistence/migrations/2026-03-23-180000_remove_orchestration_persistence` deleted it deliberately. Original text follows. ~~**DECIDED 2026-08-08: Phosphor is not doing multi-agent orchestration.** Decided on the *product* question, deliberately not on cloud coupling — that axis produced contradictory answers three times. The cloud rows above (#290 RunAgents, #376 persistence fields) cover only the cloud-runner half; the pin's `orchestration_topology.rs` imports **nothing** cloud-bound (`HashSet`, `ai::agent::conversation`, `BlocklistAIHistoryModel`) and `SCOPE-AI.md` correctly rates it verdict **D, non-cloud**. An agent rightly overruled an earlier claim that it was moot under #290. It is declined anyway, because the feature is not wanted. Corroborating: `crates/persistence/migrations/2026-03-23-180000_remove_orchestration_persistence` removed orchestration storage deliberately. Out of scope: `orchestration_topology.rs` (26 tests), `orchestration_events.rs` (10), `orchestration_event_streamer.rs`, `local_agent_task_sync_model.rs`, the orchestrator/child-agent view, credit rollup and `/orchestrate`, run-agents child prompt composition, collapsible defaults. Those 26 topology tests are good local logic — status precedence, pre-order flattening, adjacent-child navigation — and are worth revisiting *only* if orchestration is ever wanted.~~ |
| **SSH tmux wrapper — kept, deprecation not ported** | #322 | **DECIDED 2026-08-08: keep the tmux wrapper permanently.** The pin deprecates it in favour of the remote-server SSH extension; Phosphor does not, because it should warpify whatever host it is SSH'ing into. **CORRECTION 2026-08-08: the technical justification below is out of date — the DECISION still stands, its stated reason does not.** This row claimed `crates/remote_server/src/setup.rs` enumerates exactly two conditions returning `Unsupported` (`UnsupportedReason::GlibcTooOld`, `NonGlibc`), making the fallback population precisely Alpine/musl and old-glibc hosts. Commit `6f6bcdcdd` (2026-08-06) changed `parse_status()` so **neither condition returns `Unsupported` any more** — and that landed two days BEFORE this row was written. The keep-the-wrapper decision rests on the product principle (Phosphor should warpify whatever host it SSHes into) and that is unaffected; but anyone reasoning from the "exactly two conditions" claim will reach a wrong conclusion about who the fallback actually serves. Re-derive the real fallback population before relying on it. `WarpifySettings::use_ssh_tmux_wrapper` and its 6 call sites stay (`settings_view/warpify_page.rs` ×4, `terminal/local_tty/terminal_manager.rs`, `terminal/ssh/ssh_detection.rs`), as does the gate on `is_compatible_subshell_command`. Any pinned tests asserting the deprecation are permanently unported. |
| **computer_use session recording** | #350 | **DECIDED 2026-08-08: declined — this fork is not doing recording.** Distinct from #367 (terminal screen capture) but the same call. The whole subsystem is video capture of computer-use sessions: `mac/recording.rs`, `linux/recording.rs`, `recording_metadata.rs`, and `overlay::build_overlay_ass` which burns click/drag annotations into the video. `PointerSession`/`PointerSink` sound independent but exist solely to stitch pointer events across the discrete `UseComputer` calls that make up one recording, so a drag split across several calls renders as one trail — not separable. Out of scope: `pointer_session_tests.rs` (6), `overlay_tests.rs`, `recording_metadata_tests.rs`, `recording_tests.rs`, `mac/recording_tests.rs`, `linux/recording_tests.rs`. **#349 is NOT covered** — computer_use per-window *activation* (`mac/activation.rs`, `window.rs`, `post.rs`, `linux/x11/seat.rs`) is unrelated to recording and remains a real gap; it was closed on 2026-08-10. **The client-side dispatch path is NOT covered either** — the executors, `crates/ai` action/action_result variants, protobuf conversions, persistence and block rendering removed by `9765692e1` (an inherited upstream Zap commit, not a decision of this fork) were restored from the pin. Recording is the only declined part of computer use: `PointerSink`/`PointerSession`, `RecordingConfig::target` and the overlay burn-in stay out, so `UseComputerExecutor` runs the actor without a recording controller or pointer sink. |
| **Dead code for declined subsystems** | #550, #551 | **DECIDED 2026-08-08.** Surfaced by the #207 dead-code audit, which reports candidates without judging them; each still needs a call on wire-it-up versus always-dead. These two are always-dead. `warpui_core::integration::capture_recorder` is an orphaned duplicate of `video_recorder` with no callers, for the declined recording feature. `WarpDriveSpace::new` is never constructed and has no factory call site — Warp Drive is dropped cloud (see the #267 row, which keeps link resolution as deliberate dead code). Neither is a wiring gap. |
| **Status-menu `org` / `email` fields** | #389 | **DECIDED 2026-08-09: dropped, not deferred.** Warp's status menu shows the signed-in account's organisation and email. Phosphor has no cloud account, no organisation and no sign-in email, so there is no truthful value to render. Both fields were removed from `TuiStatusInfo` outright rather than blanked or filled with a BYOP substitute (commit `c87c49820`), and no empty rows are left where they used to be. **Permanently unported pin tests:** `status_email_fallback_chain_*`, `status_slash_command_opens_dedicated_status_menu_via_shared_structure` (asserts literal `"Org"`/`"Email"` rows this fork no longer renders), and `user_info_updates_only_require_*`, plus anything needing `resolve_status_email` / `STATUS_SIGNED_IN`. Porting them would be red; weakening their assertions to match is what AGENTS §5.6 forbids. **Recorded here 2026-08-09** because the decision previously existed only in a commit message, which is where the #576 port sweep found it — the rest of `/status` and the read-only menu surface ARE ported and wired, so this is a narrow field-level drop, not a declined feature. |
| **Agent-invoked agent spawning (`AIAgentActionType::RunAgents`)** | #325 | **DECLINED 2026-08-09 (maintainer) — was 'deferred' earlier the same day, now a decline.** Phosphor's answer to orchestration is the USER-invoked route: `/orchestrate` (`ad06c852c`), shipped on both GUI and TUI, spawning local children wired into the #304 pill bar, tab bar and transcript rendering, with swap/stop/kill and a working child-to-parent mailbox. **What is declined is letting the MODEL decide to spawn agents.** Reasons: the pin's `RunAgentsRequest` is cloud-typed so there is nothing to port — it would be from-scratch API design that permanently diverges from the pin; `AIAgentActionType` has 35 variants and no spawn variant, with **68 files** matching that enum; and `StartAgentExecutionMode`/`RunAgentsExecutionMode`/`RunAgentsAgentRunConfig` have no reference implementation to follow, so their shape would be embedded across those 68 files before anyone knew it was right. A model that can spawn agents also multiplies token spend, which the pin governs cloud-side and this fork has no equivalent for. **Not covered by this decline:** the proto side is already handled — `Tool::RunAgents(_)` and `ToolCallResultType::RunAgentsResult(_)` are recognised and routed (`conversation_yaml.rs`, `convert_conversation.rs`, `task/helper.rs:135` maps it to "orchestrate"); and `SendMessageToAgent`, once part of this family, was split out, landed with a local executor, and its own row above is marked reversed. Revisit only if user-invoked orchestration proves insufficient in practice. |
| ~~**`SendMessageToAgent` executor (the cloud half)**~~ | #325 | **REVERSED 2026-08-09 — the executor now EXISTS and is local.** This row said the executor could not be ported because the pin's version posts through `ServerApiProvider`/`SendAgentMessageRequest` to Warp's GraphQL backend. True of the pin, but a local equivalent was built instead: a filesystem mailbox (`crates/warp_cli/src/agent_mailbox.rs`) under `state_dir()/oz/agent-mailbox/<run_id>/`, keyed on the existing `OZ_RUN_ID` identity, with `oz agent message send|list` added as a NEW subcommand under the existing `agent` surface. `SendMessageToAgentExecutor` writes through it in-process; `convert_from.rs` now constructs the action from `Tool::SendMessageToAgent` instead of hitting a catch-all, and `convert_conversation.rs` reconstructs the result on history restore. **The variant is no longer inert.** Two related findings recorded here so they are not re-derived: `oz run`/`run message` was removed as genuine cloud (the pin's `task.rs` filters by `creator`/`environment`/`schedule`/`execution_location`, server-assigned ids from Warp's hosted task registry — there is no local registry to be a client of), so `run_command_is_removed` asserts a PERMANENT absence and must not be reinstated; and `local_control`/`warpctrl` was evaluated and rejected as the transport — its `ActionKind` catalog has no message concept, and it is gated behind `Settings > Scripting` with a live GUI, which a headless spawned child cannot rely on. |

---

## Not declined — common false positives

Filed as cloud or as decisions, but actually in scope. Do not add these here.

- **`crates/computer_use`** — not a dropped feature. The platform-neutral API, Linux
  X11 and macOS window targeting were restored under #349 (2026-08-10), and the
  client-side dispatch path that connects an agent tool call to
  `computer_use::create_actor()` was restored afterwards. Only session *recording*
  is declined (see the #350 row above).
- **`app/src/remote_server` / `crates/remote_server`** — Phosphor's SSH remote-host
  daemon, entirely local. Not Warp's cloud backend, despite the name.
- **`agent_sdk` bounded retry** (#278) — transport-agnostic and non-cloud; a BYOP
  Phosphor wants it.
- **Grok OAuth** — declined for product reasons (above), *not* because it is cloud.
  It never touches Warp's servers.

## The import test is necessary, not sufficient

`SCOPE-*.md` classifies a pin file by reading its import list: anything reaching
`crate::server::`, `cloud_object` or `warp_graphql` is cloud, everything else is
in scope. That is the right first filter and it is cheap. It is also blind to
two cases that came up repeatedly on 2026-08-07/08:

- **A feature the fork deliberately deleted and replaced.** `ActiveAgentViewsModel`
  imports nothing cloudy; it is simply gone, with a working substitute. To the
  import test that is indistinguishable from an unported symbol.
- **A feature that is genuinely local and simply not wanted.** The orchestration
  topology modules import only `HashSet` and two in-tree types. Verdict D,
  non-cloud, correctly — and declined anyway on the product question.

So before filing a gap: a pinned test is debt **only if the feature exists and
the test is missing**. If the feature is missing, it is a feature issue, and it
should be sized as one. If the feature was deliberately removed, it belongs in
this file. Three separate issues were mislabelled as portable test debt for
want of that distinction.
