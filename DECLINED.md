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

## Divergences where the fork deliberately differs

| what | issue | note |
|---|---|---|
| **`has_locking_attachment`** | #318 | **DECIDED 2026-08-07: keep the fork's behaviour.** The oracle narrows locking to image/file attachments; the fork also locks on pending block ids and inline `@` context refs. Attaching a block is an explicit "use this as agent context" gesture, and the pin's narrower rule lets the classifier flip the input back to shell mode afterwards — the bug the fork's own comment at `input.rs:12924` describes. The pin's `has_locking_attachment_is_false_with_only_pending_block_id` is **permanently not ported**; the fork's `has_locking_attachment_is_true_with_pending_block_id` is the authority. |
| **`ask_user_question` auto-approve** | #373 | **DECIDED 2026-08-07: keep the fork's behaviour.** Under the pin, a conversation in auto-approve mode makes `can_ask_user_question` return `false`; `should_autoexecute` is its negation, so `execute` returns `SkippedByAutoApprove { question_ids }`. The question is **silently discarded, not auto-answered** — the model asks, nothing reaches the user, and the agent proceeds having never got an answer. The fork's divergence in `permissions.rs` makes auto-approve (ctrl+shift+i) auto-pass *execution* tools only, so a question always surfaces. The pin's `should_autoexecute_returns_true_when_autoapprove_is_enabled_and_profile_allows_override` and `execute_returns_sync_skipped_question_ids_when_autoapprove_is_enabled` are **permanently not ported**; the fork's two inverted tests are the authority. An earlier "mirror Warp" call was reversed once the swallow was traced — PR #489 implemented it and was closed unmerged. |
| **TUI/GUI shared app id** | — | The fork deliberately shares one app id (and therefore one keychain namespace and config) between GUI and TUI; the pin separates them. Two pin tests assert the separation and are intentionally not ported. |
| **Privacy toggle defaults** | — | Warp defaults telemetry/crash-reporting **on** (opt-out, commercial product). This fork defaults them **off** — leaving them on would show "ON" while nothing goes out. |
| **`CustomEndpoint` / `custom_model_providers` on `ApiKeyManager`** | #142, #347 | **DECIDED (PR #189, PR #227 — both merged 2026-08-07): superseded, not ported.** The pin's `ApiKeyManager` has a fixed four-provider list plus a `custom_endpoints: Vec<CustomEndpoint>` field, and `custom_model_providers_for_request` serializes those into a `CustomModelProviders` wire payload that tells Warp's *server* how to proxy to a user's endpoint on their behalf. This fork calls custom endpoints directly via genai (`chat_stream::build_client`) with no server proxy, and the pinned `warp_multi_agent_api` rev has no `CustomModelProviders` message to serialize into at all. The fork's actual BYOP surface for this is `AgentProviderSecrets` (`app/src/ai/agent_providers/secrets.rs`) — arbitrary user-defined providers each with their own `base_url`, a strict superset of the pin's fixed-four-plus-custom-endpoints design, with its own coverage (`app/src/ai/agent_providers/mod_test.rs`, 13 tests; `secrets_tests.rs`, 6 tests). Porting `CustomEndpoint` into `crates/ai/src/api_keys.rs` would stand up a second, competing provider store. Keeps 16 `api_keys_tests.rs` pin tests permanently unported — see that file's header comment for the per-test list. **#347 (`app/src/ai/llms.rs`), assessed 2026-08-07: also superseded, not a live gap.** #347 has two clusters. Cluster 2 (`CustomEndpoint`/`CustomEndpointModel`/`build_custom_llm_infos`, 8 tests, plus 5 more that depend on it) is this same declined surface viewed from `llms.rs` instead of `api_keys.rs` — the pin's `build_custom_llm_infos` takes `ai::api_keys::ApiKeys::custom_endpoints` as its direct input, so porting it would resurrect the exact competing provider store this row already declines. Cluster 1 (`DisableReason::should_clear_preference`, `is_usable_llm`, `usable_default_llm_info`, `should_show_host_icon_for_model`, 7 tests) exists in the pin to reconcile a *cloud*-fetched model list — subject to org-admin/quota/upgrade disables and AWS Bedrock / Gemini Enterprise host routing — against a BYOK override. Phosphor's model list is built entirely locally from `AgentProviderSecrets` (`LLMPreferences::new`'s own comment: "BYOP-only mode ... no longer consumes the upstream cloud model list at all"), so `DisableReason::AdminDisabled`/`OutOfRequests`/`ProviderOutage`/`RequiresUpgrade` are never constructed by any live code path (repo-wide grep finds only dead rendering-only branches in `execution_profiles/model_menu_items.rs` and `terminal/input/models/data_source.rs` that would fire if they ever were) and `LLMInfo::host_configs` is always the empty map. The fork's disable handling already works, by a different mechanism: `build_byop_llm_infos` (`app/src/ai/agent_providers/mod.rs`) omits disabled models from `choices` outright, and `LLMPreferences::on_server_update` already clears any profile preference pointing at a model id that dropped out of `choices` — no `should_clear_preference` distinction is needed because there is no disabled-but-still-BYOK-usable case to distinguish. `is_usable_llm`/`usable_default_llm_info` also cannot be ported standalone: the pin's own implementation falls back into `custom_llm_choices`, i.e. cluster 2. |

## Retired features (decision pending on some)

| what | issue | note |
|---|---|---|
| **Codebase indexing / `RepoOutlines`** | #11 | Deliberately removed (`d84dd8e4d`). Blocks the code-symbol source and `SearchCodebase`. Re-porting is parity-legitimate (local, non-cloud) but is a keep-dropped-vs-restore decision. |
| **Screen recording** | #367 | **Declined.** Not cloud — the pin's capture is local `ffmpeg`, and only `upload_artifact` touched Warp's servers — but Phosphor is not shipping it. 26 pinned tests are out of scope. **Nothing to remove:** the subsystem was never ported, so there is no `recording_controller` / capture / finalize code here. What remains are `Tool::StartRecording` / `StopRecording` variants on the shared `warp_multi_agent_api` types, handled as unreachable no-ops — those must stay for exhaustive matching against the external crate. |
| **`SettingSurfaces` / `SettingsMode`** | — | Explicitly documented as dropped in `app/src/settings/ai.rs` and `tui_autoupdate.rs`. |
| **warp.dev Drive link resolution** | #267 | **DECIDED 2026-08-07: keep as dead code.** The fork still resolves `warp.dev/drive/...` links into `ZapDriveObjectArgs`. Warp Drive is the cloud product and nothing here can service such a link, but the resolution path is retained deliberately rather than ripped out. Its two tests and its two `cloud_boundary_allowlist.txt` entries (`uri_test.rs` → `cloud_object::ObjectType`, `server::ids::ServerId`) stay — **do not "clean up" the allowlist by removing them.** |
| **`>` vs `>=` restore-order tie-break** | #174 | **DECIDED 2026-08-07: declined.** `conversation_restoration` breaks ties differently from the pin. The fork's choice was previously justified only in a source comment, with three pin tests marked "intentionally NOT ported" — that is now a recorded decision rather than an undocumented divergence. Those three tests stay unported permanently. |
| **Orchestration persistence fields** | #376 | **DECIDED 2026-08-07: declined, cloud.** `AgentConversationData` lacks `pinned`, `is_remote_child`, `orchestration_harness_type`, `root_task_is_optimistic`. They serve the multi-agent "pill bar" UI whose ~15-20 consuming files are entirely absent here. Same cloud family as the RunAgents row above. |
| **"Oz updates" zero-state section** | #321 | **DECIDED 2026-08-07: declined.** `ChangelogModel.oz_updates` / `AISettings::should_show_oz_updates_in_zero_state` drive a Warp-branded content feed in the zero state. Not a capability gap — branded content this fork does not carry. |
| **remote_server bundled skills/resources** | #440 | **DECIDED 2026-08-07: declined.** `BUNDLED_RESOURCES_DIR_NAME`, `remote_server_bundled_resources_dir` and `remote_server_removal_command` are all absent from `crates/remote_server/src/setup.rs`. Phosphor's remote-server artifact does not ship a `resources/` tree and is not gaining remote-installed bundled skills. |

---

## Not declined — common false positives

Filed as cloud or as decisions, but actually in scope. Do not add these here.

- **`crates/computer_use`** — not a dropped feature; the fork ships 28 of 45 files.
- **`app/src/remote_server` / `crates/remote_server`** — Phosphor's SSH remote-host
  daemon, entirely local. Not Warp's cloud backend, despite the name.
- **`agent_sdk` bounded retry** (#278) — transport-agnostic and non-cloud; a BYOP
  Phosphor wants it.
- **Grok OAuth** — declined for product reasons (above), *not* because it is cloud.
  It never touches Warp's servers.
