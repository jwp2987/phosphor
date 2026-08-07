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
| **`has_locking_attachment`** | #318 | The oracle narrows this to image/file attachments; the fork also locks on pending block ids. **Both sides ship a test asserting their own answer.** Porting the oracle's test would mean deleting the fork's and flipping user-visible input-mode behaviour. Maintainer call. |
| **`ask_user_question` auto-approve** | #373 | `permissions.rs` carries a documented "openWarp change" contradicting two pin tests, while `ask_user_question.rs` is byte-identical to the pin. Maintainer call. |
| **TUI/GUI shared app id** | — | The fork deliberately shares one app id (and therefore one keychain namespace and config) between GUI and TUI; the pin separates them. Two pin tests assert the separation and are intentionally not ported. |
| **Privacy toggle defaults** | — | Warp defaults telemetry/crash-reporting **on** (opt-out, commercial product). This fork defaults them **off** — leaving them on would show "ON" while nothing goes out. |

## Retired features (decision pending on some)

| what | issue | note |
|---|---|---|
| **Codebase indexing / `RepoOutlines`** | #11 | Deliberately removed (`d84dd8e4d`). Blocks the code-symbol source and `SearchCodebase`. Re-porting is parity-legitimate (local, non-cloud) but is a keep-dropped-vs-restore decision. |
| **Screen recording** | #367 | **Declined.** Not cloud — the pin's capture is local `ffmpeg`, and only `upload_artifact` touched Warp's servers — but Phosphor is not shipping it. 26 pinned tests are out of scope. **Nothing to remove:** the subsystem was never ported, so there is no `recording_controller` / capture / finalize code here. What remains are `Tool::StartRecording` / `StopRecording` variants on the shared `warp_multi_agent_api` types, handled as unreachable no-ops — those must stay for exhaustive matching against the external crate. |
| **`SettingSurfaces` / `SettingsMode`** | — | Explicitly documented as dropped in `app/src/settings/ai.rs` and `tui_autoupdate.rs`. |
| **warp.dev Drive link resolution** | #267 | The fork still resolves `warp.dev/drive/...` links into `ZapDriveObjectArgs`. Warp Drive is the cloud product, so this may be dead code that should go — along with its two tests and two allowlist lines. **Undecided.** |

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
