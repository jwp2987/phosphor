# Cloud-block sweep adjudication

Oracle: pin `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. Source
list: `docs/SWEEP-INVENTORY.md`'s per-file sections for this area (the file-level
counts there are trustworthy; the *bucket* on each test, where it ends in `?`,
is a mechanical guess this document replaces with a hand-traced verdict).

Scope: `app/src/server/**` (excluding `app/src/server/experiments/**`, which
ships and passes in the fork and is not part of the absent-test workload) ·
`crates/warp_server_client/**` · `app/src/workspaces/**` ·
`crates/cloud_object_models/**` · `app/src/cloud_object/**` ·
`app/src/auth/**`. 27 pin test files, **285 absent tests** (the assignment's
"283" was a rounded estimate; 285 is the exact count of pin test names in
these 27 files that have no same-named function anywhere in the fork tree).

## Method

This block was assigned as "expected mostly CLOUD" but flagged explicitly as
not to be rubber-stamped, for two documented reasons: false positives named
like cloud that are entirely local, and a real name collision inside
`app/src/workspaces/**` between Warp's cloud team-workspace concept and this
fork's local `UserWorkspaces`.

For every file: read the pin source's `use` imports at `02b53fcd8`
(`git show 02b53fcd8:<path>`), check whether the fork ships a file of that
name or the same directory at all (`find` / `git ls-tree` on the fork tree),
and where the fork does ship something under that name, read enough of it to
tell whether it is the same concept (has the same fields/methods the pin test
needs) or a same-named stub/facade. Where a test's own body was ambiguous
from the source-file's imports alone, the pin test body itself was read.

Every `CLOUD` and `DECLINED` verdict below is evidence-backed by an import
quote or a fork source quote — not by directory name.

## Bucket totals — all 285 absent tests adjudicated

| bucket | count | meaning |
|---|---:|---|
| CLOUD | 195 | needs `warp_graphql` / `warp_server_client` / `warp_server_auth` / `crate::server::` / cloud-object-sync plumbing that the fork does not ship |
| DECLINED | 87 | covered by an existing `DECLINED.md` decision (67), or by a decision recorded only in fork source comments that `DECLINED.md` should also carry (20 — see "New DECLINED rows to propose") |
| MISSING-SUBSYSTEM | 3 | real, non-cloud parity debt — see the headline finding below |
| **total** | **285** | |

**Zero PORTABLE, zero DIVERGENT, zero COVERED-ELSEWHERE.** For this
particular 27-file slice the mechanical CLOUD?/DECLINED? split was directionally
right at the file level — every file's *dominant* import is genuinely
`warp_graphql`, `warp_server_client`, `warp_server_auth`, or `crate::server::`
plumbing the fork does not ship, and none of the excluded areas
(`computer_use`, `remote_server`, `local_control`, Grok OAuth) appear here at
all. The value this pass adds is (a) correcting one mis-bucketed test inside
an otherwise-correct file, (b) resolving the `app/src/workspaces` name
collision explicitly rather than assuming, and (c) the one file that
*wasn't* cloud despite living inside the block that looked entirely cloud.

## The headline finding — NOT cloud

### `crates/warp_server_client/src/network_logging_tests.rs` — 3 tests — MISSING-SUBSYSTEM, not CLOUD

`crates/warp_server_client/src/network_logging.rs` (pin, 164 lines) implements
`NetworkLogModel`: a bounded in-memory ring buffer of the last 50 HTTP
request/response pairs, populated by hooking `http_client::Client`'s
`set_before_request_fn` / `set_after_response_fn`, formatted as
timestamped `Debug` strings and joined for display in a "network log" console
pane. **Its imports are `std::fmt`, `bounded_vec_deque::BoundedVecDeque`,
`chrono`, `warp_errors::report_error`, `warpui_core::{Entity, ModelContext,
SingletonEntity}` — zero cloud symbols.** It hooks a generic `http_client`,
not a Warp-specific one; nothing in the module cares what server the client
talks to.

It landed in this 27-file block only because it physically lives inside the
`crates/warp_server_client` directory, which was deleted wholesale when the
cloud backend was dropped — collateral damage, not a targeted decision. That
this was collateral rather than reasoned is corroborated by the fork's own
`app/src/settings_view/privacy_page.rs:13-15`:

> `NetworkLogWidget` — the network-log console (`pane_group/pane/network_log_pane.rs`,
> `server/network_log_view.rs`, `warp_server_client::network_logging`) was
> removed from this fork, so the "View network logging" link has no
> destination.

That comment records the *fact* of removal, not a *reason* tied to cloud —
unlike the same file's adjacent bullets for `CloudConversationStorageWidget`
and `DataManagementWidget`, which do give a cloud reason ("this fork removed
cloud conversation storage", "part of the Warp cloud account, which this fork
does not have"). `NetworkLogWidget`'s bullet gives no such reason because
there isn't one for the model itself — only for the three files that used to
carry it.

**The wiring this needs already exists and is unused.** Verified in this
fork: `crates/http_client/src/lib.rs:175,179` define
`set_before_request_fn` / `set_after_response_fn` on `http_client::Client`,
and a repo-wide grep for callers of either finds none outside their own
definitions — dead hook points sized exactly for `NetworkLogModel::install_on_clients`.
`reqwest` (the pin's `NetworkLogItem` formats `reqwest::Request`/`Response`
via `Debug`) and `async-channel` (used for the request/response delivery
channel) are both already workspace dependencies. Only `bounded_vec_deque`
would need adding.

**Not ported this pass.** The 3 tests (`empty_snapshot_is_empty_string`,
`push_beyond_capacity_drops_oldest`, `snapshot_joins_items_with_newlines`)
only exercise the pure `NetworkLogModel`/`NetworkLogItem` data structure, but
porting just that with no consumer would be dead code with no home crate
(`crates/warp_server_client` doesn't exist to receive it, and this pass has no
build access to verify a new crate or a relocation compiles). The real value
— a working debug console for outgoing HTTP, which would show BYOP LLM
provider traffic too, not just Warp's — needs the pane/settings-page wiring
`privacy_page.rs` already describes as absent. Flagging as **MISSING-SUBSYSTEM**
rather than porting: this is genuine non-cloud parity debt, self-contained,
and the next agent with build access has everything needed to scope it in
one place (this section) instead of re-deriving it from three files.

## Per-file adjudication

Ordered by absent-test count, matching `docs/SWEEP-INVENTORY.md`.

### `app/src/server/cloud_objects/update_manager_tests.rs` — 72 absent

pin 73 · fork 1 · source `app/src/server/cloud_objects/update_manager.rs` · fork ships source: NO (`app/src/server/cloud_objects/` does not exist in the fork tree at all — the fork's `app/src/server/` contains only `datetime_ext.rs`, `experiments/`, `ids.rs`, `retry_strategies.rs`, `telemetry.rs`)

- **CLOUD** — all 72. This is Warp Drive's real-time object-sync engine:
  polling/RTC baton-grab, folder-move/trash/untrash conflict resolution,
  action-history overwrite, tier-limit enforcement — every test constructs a
  mock cloud-object server and asserts on sync-queue state after a simulated
  server round trip. No file of this name or purpose exists in the fork.

### `app/src/server/server_api/ai_tests.rs` — 30 absent

pin 45 · fork 15 · source `app/src/server/server_api/ai.rs` · fork ships source: NO (`app/src/server/server_api/` does not exist in the fork at all)

- **CLOUD** — all 30. Pin source imports `warp_graphql::mutations::{create_agent_task, create_file_artifact_upload_target, delete_ai_conversation, generate_code_embeddings, ...}`, `ai::index::full_source_code_embedding` (server-side codebase embeddings). Every absent test is either a GraphQL request/response (de)serializer or a URL builder for a Warp agent-task REST/GraphQL endpoint (`build_fork_conversation_url`, `build_list_agent_runs_url`, spawn-agent request shaping). The 15 fork-side name matches are coincidental hits elsewhere in the tree (e.g. generic `test_deserialize_*` names), not coverage of this file.

### `app/src/workspaces/user_workspaces_tests.rs` — 29 absent

pin 29 · fork 0 · source `app/src/workspaces/user_workspaces.rs` · fork ships source: **yes — and this is the collision the task asked me to resolve, not assume.**

The fork's `user_workspaces.rs` (922 lines) is the *same* Warp team-workspace
concept as the pin, not a distinct local `UserWorkspaces` — it still holds
`Vec<Workspace>`, `Team`, invite/domain-restriction events, and imports
`crate::server::ids::ServerId`. It is not a rewrite; it is the pin's own
struct with the team-RPC code paths cut. The fork's own trailing comment
(`user_workspaces.rs:918-920`) says so directly:

> Zap (localization, Phase 5): `user_workspaces_tests.rs` targeted entirely
> the team RPC path (`MockTeamClient` / `mockall::Sequence`); after
> localization these paths are unreachable, so the entire file was
> physically removed.

- **DECLINED** — all 29. Already covered by `DECLINED.md`'s existing "Cloud
  teams / org policy" row (#445): *"`UserWorkspaces::current_team()` returns
  `None` unconditionally — a deliberate BYOP decision... Consequence: the
  org/workspace command denylist is inert."* Every one of these 29 tests
  (AWS Bedrock/Gemini Enterprise admin-enforcement, team billing metadata,
  domain-scoped codebase-context policy, per-window team assignment) is
  exactly that: a team/org admin-policy behaviour that requires a `Team` this
  fork's `current_team()` never returns. No new row needed — this is the
  clearest possible instance of an existing decision.

### `crates/warp_server_client/src/iap_tests.rs` — 23 absent

pin 23 · fork 0 · source `crates/warp_server_client/src/iap.rs` · fork ships source: NO (`crates/warp_server_client` does not exist anywhere in the fork — no directory, not a workspace member)

- **CLOUD** — all 23, **correcting one mechanical mis-bucket**. `iap.rs`'s
  own header comments identify it as GCP Workload Identity Federation
  (STS token exchange + IAM `generateIdToken`) used to mint an OIDC identity
  token so a sandboxed Warp cloud runner can authenticate to `warp-server`'s
  IAP-gated endpoints — infrastructure for Warp's hosted agent runners, with
  no BYOP equivalent (there is no sandboxed cloud runner to bootstrap).
  `docs/SWEEP-INVENTORY.md` bucketed 22 of these `DIVERGENT?` (feature gap)
  and mis-bucketed one, `generate_id_token_request_uses_camel_case_include_email`,
  as `DECLINED?` citing #389 ("Status-menu org/email fields") — a false match
  on the word "email". Read, that test asserts `serde_json` field-casing
  (`includeEmail`) on `GenerateIdTokenRequest`, the exact same GCP
  `generateIdToken` request struct the other 22 tests exercise; it has
  nothing to do with the TUI status menu. All 23 are one CLOUD verdict.

### `crates/cloud_object_models/src/cloud_environment_tests.rs` — 20 absent

pin 20 · fork 0 · source `crates/cloud_object_models/src/cloud_environment.rs` · fork ships source: NO (`crates/cloud_object_models` does not exist in the fork)

- **DECLINED** — all 20. `cloud_environment.rs` imports
  `cloud_objects::cloud_object::{GenericCloudObject, GenericServerObject, ...}`
  and defines `CodeForge`/environment provider (AWS/GCP) serde shapes — this
  is Warp Environments (cloud dev-container/runner config), already covered
  by `DECLINED.md`'s existing row: *"Warp Environments | #211 | Cloud-backed.
  75 pinned tests are out of scope, not parity debt."*

### `app/src/server/telemetry/secret_redaction_tests.rs` — 18 absent

pin 18 · fork 0 · source `app/src/server/telemetry/secret_redaction.rs` · fork ships source: NO (fork's `app/src/server/telemetry.rs` is a single file, not a `telemetry/` directory; no `secret_redaction.rs` exists there)

- **DECLINED** — all 18. This is a *different* module from the fork's
  `app/src/ai/blocklist/block/secret_redaction.rs` (which the fork does
  ship, fully tested) — the pin's own doc comment on this file
  distinguishes them explicitly: *"Unlike the AI-side secret redaction...
  which is gated on the user's safe-mode setting and used for visual
  obfuscation in the terminal, the redaction in this module is
  unconditional... a defence-in-depth measure for data leaving the device"* —
  i.e. it exists solely to scrub outbound **telemetry** payloads. Its only
  caller in the pin is `telemetry_ext.rs`. `DECLINED.md`'s existing
  "Telemetry" row applies directly: *"Channel physically removed...
  Nothing is sent."* A redaction pass over data that is never sent has
  nothing to protect.

### `app/src/server/sync_queue_tests.rs` — 13 absent

pin 13 · fork 0 · source `app/src/server/sync_queue.rs` · fork ships source: NO

- **CLOUD** — all 13. Imports `warp_graphql::scalars::time::ServerTimestamp`
  and `super::server_api::auth::UserAuthenticationError`; implements the
  outbound queue that batches local object mutations for the cloud sync
  engine (dependency ordering, retry-after-transient-failure). No local
  destination for these operations exists.

### `app/src/cloud_object/model/model_tests.rs` — 12 absent

pin 27 · fork 15 · source `app/src/cloud_object/model/model.rs` · fork ships source: NO exact match, but the fork's `app/src/cloud_object/model/model_test.rs` (999 lines, singular filename — the fork's standard `_tests.rs` → `_test.rs` rename) covers the other 15 tests in this pin file, confirming the file is genuinely MIXED, not absent.

- **CLOUD** — all 12 remaining. Read against the pin source: every one of
  these calls `base_mock_cloud_object_server_api()` / a mocked
  `expect_fetch_changed_objects()`, or `update_object_after_server_creation`
  (assigning a server-issued ID after a create round-trip), or constructs
  `Owner::Team`. The fork's equivalent, `ObjectStoreModel`
  (`app/src/cloud_object/model/persistence.rs`), has no
  `update_object_after_server_creation` and no `fetch_changed_objects` (both
  zero hits repo-wide) — it keeps `time_of_next_force_refresh` bookkeeping
  but not the server round-trip that would fill it. The 15 tests the fork
  *does* cover (local CRUD, breadcrumbs, trash cascade, active-object-uid
  bookkeeping) are exactly the local-state half of this file; these 12 are
  the server-sync half.

### `app/src/server/server_api/presigned_upload_tests.rs` — 9 absent

pin 9 · fork 0 · source `app/src/server/server_api/presigned_upload.rs` · fork ships source: NO

- **CLOUD** — all 9. `pub use warp_server_client::HttpStatusError;` at the
  top of the file, and every test builds a multipart upload against a Warp
  presigned-URL upload target (`UploadTarget`, `FileUploadBody`). The CRC32c
  hashing itself is generic, but it exists only to checksum uploads to Warp
  cloud storage, which this fork has no destination for.

### `app/src/auth/auth_manager_tests.rs` — 7 absent

pin 7 · fork 0 · source `app/src/auth/auth_manager.rs` · fork ships source: NO — but `app/src/auth/mod.rs` (which the fork does ship) documents exactly why, at its own top-of-file:

> The 167 call sites of `crate::auth::AuthStateProvider::as_ref(ctx).get()`
> keep compiling with zero changes, and at runtime they always get a local
> placeholder state of "logged in, unlimited Free Tier"... See the README
> for the physical-deletion list: 21 UI / RPC / token-persistence /
> web-handoff files — login_slide / paste_auth_token_modal / web_handoff,
> etc. — were removed together with the external account system.

- **DECLINED** — all 7 (see "New DECLINED rows to propose" below; no
  existing row covers the external-account-system removal specifically).
  Confirmed via the pin source: `auth_manager.rs` imports
  `crate::ServerApiProvider`, `crate::auth::auth_view_modal::AuthRedirectPayload`,
  `crate::server::server_api::auth::UserAuthenticationError`, and the 7 tests
  cover the OAuth device-code flow, redirect-state validation, and
  refresh-token persistence — the exact machinery `auth/mod.rs` says was
  physically deleted.

### `crates/warp_server_client/src/public_api_tests.rs` — 7 absent

pin 7 · fork 0 · source `crates/warp_server_client/src/public_api.rs` · fork ships source: NO

- **CLOUD** — all 7. `public_api.rs` builds on `crate::base_client::BaseClient`
  (see `base_client_tests.rs` below) and adds IAP-challenge-observation
  events and bearer-auth GET requests against Warp's public API. Whole crate
  absent.

### `app/src/server/server_api/harness_support_tests.rs` — 6 absent

pin 7 · fork 1 · source `app/src/server/server_api/harness_support.rs` · fork ships source: NO

- **CLOUD** — all 6. Imports `super::ServerApi`,
  `crate::ai::agent_sdk::retry::with_bounded_retry`,
  `crate::ai::ambient_agents::AmbientAgentTaskId` — file/screenshot artifact
  upload-target plumbing and agent-run shutdown reporting to Warp's server.
  The 1 fork-side name coincidence is elsewhere in the tree.

### `app/src/workspaces/workspace_tests.rs` — 6 absent

pin 6 · fork 0 · source `app/src/workspaces/workspace.rs` · fork ships source: yes, but partially — the fork's `Workspace` keeps `billing_metadata: BillingMetadata` but drops the usage-visibility-tier machinery these 6 tests exercise (`resolve_usage_visibility`, `UsageVisibilityPolicy`, `UsageVisibilityGranularity`, `MaxPriorCycles`, `FtueAccountClass` — all zero hits in the fork's `workspace.rs`).

- **DECLINED** — all 6. This is billing-tier / account-class policy
  (per-cycle usage-visibility granularity by admin tier, FTUE account-class
  telemetry labels for `Paid`/`FreeIcp`/`FreeStandard`) — covered by
  `DECLINED.md`'s existing "Account-first onboarding, billing, paid tiers"
  row (#11): *"`account_class`, `is_paid`, `has_team`, upgrade flows. No BYOP
  equivalent."*

### `crates/warp_server_client/src/graphql_helpers_tests.rs` — 6 absent

pin 6 · fork 0 · source `crates/warp_server_client/src/graphql_helpers.rs` · fork ships source: NO

- **CLOUD** — all 6. `send_graphql_request` wraps `warp_graphql::client::{GraphQLError, Operation}` over a `BaseClient`; tests cover auth-rejection/refresh-disabled/refresh-enabled request shaping. Whole crate absent.

### `crates/warp_server_client/src/base_client_tests.rs` — 5 absent

pin 5 · fork 0 · source `crates/warp_server_client/src/base_client.rs` · fork ships source: NO

- **CLOUD** — all 5. Imports `warp_graphql::client::RequestOptions`,
  `warp_server_auth::auth_state::AuthState`, and implements ambient ("connected
  self-hosted worker") header injection plus IAP-proxy-auth headers for
  GraphQL requests. Whole crate absent.

### `app/src/server/telemetry_ext_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/server/telemetry_ext.rs` · fork ships source: NO

- **DECLINED** — all 3. `telemetry_ext.rs` imports
  `super::telemetry::secret_redaction::redact_secrets_in_value` (see
  `secret_redaction_tests.rs` above) and `RudderBatchMessage`/`RudderTrack` —
  it is the UGC-redaction pass applied immediately before a telemetry batch
  is sent over the (physically removed) telemetry channel. Same
  `DECLINED.md` "Telemetry" row applies.

### `app/src/workspaces/gql_convert_tests.rs` — 3 absent

pin 3 · fork 0 · source `app/src/workspaces/gql_convert.rs` · fork ships source: NO

- **CLOUD** — all 3. File is nothing but `warp_graphql::{billing, workspace,
  user}` type conversions (team ordering for the workspace picker, straight
  off a GraphQL response). Not present at all in the fork (distinct from
  `user_workspaces.rs`, which the fork does keep a localized copy of).

### `crates/warp_server_client/src/auth/session_tests.rs` — 3 absent

pin 3 · fork 0 · source `crates/warp_server_client/src/auth/session.rs` · fork ships source: NO

- **CLOUD** — all 3. Imports `oauth2::TokenResponse`,
  `warp_server_auth::auth_state::AuthState`,
  `warp_server_auth::credentials::{Credentials, FirebaseToken, LoginToken, RefreshToken}` —
  Firebase-token-backed session refresh. Whole crate absent.

### `crates/warp_server_client/src/network_logging_tests.rs` — 3 absent

**MISSING-SUBSYSTEM — see the headline finding above.** Not repeated here.

### `app/src/server/server_api/auth_tests.rs` — 2 absent

pin 2 · fork 0 · source `app/src/server/server_api/auth.rs` · fork ships source: NO

- **CLOUD** — both. `server_api/auth.rs` is a thin re-export of
  `warp_server_client::auth::{AuthClient, FetchUserResult, ...}`.
  `test_firebase_token_urls` asserts Firebase Identity Toolkit URL
  construction; `access_token_skip_login_rejects_bearer_token` needs
  `ServerApi` under the `skip_login` cargo feature. The mechanical inventory
  guessed `DECLINED?` (#11, "Account-first onboarding") for the second test;
  read, it is not an onboarding/billing test at all — it is a daemon-mode
  auth-rejection assertion gated on `ServerApi`, which is cloud, full stop.
  Correcting to CLOUD.

### `crates/cloud_object_models/src/scheduled_ambient_agent_tests.rs` — 2 absent

pin 2 · fork 0 · source `crates/cloud_object_models/src/scheduled_ambient_agent.rs` · fork ships source: NO

- **CLOUD** — both. `AgentConfigSnapshot`/`SourceRepo` round-trip tests for
  Warp's scheduled (cron-like) cloud agent runs; imports
  `crate::cloud_environment::SourceRepo` (Warp Environments, #211) and
  `cloud_objects::cloud_object::{GenericCloudObject, GenericServerObject}`.
  Adjacent to #211 rather than a literal restatement of it (a different pin
  file), so kept as its own CLOUD verdict rather than folded into that row.

### `app/src/auth/login_slide_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/auth/login_slide.rs` · fork ships source: NO — same physical-deletion note as `auth_manager_tests.rs` above.

- **DECLINED** — the one test (`account_first_copy_matches_product_spec`)
  asserts literal Warp marketing copy ("Access AI, run cloud agents,
  collaborate with teammates, and sync settings across devices... Use a work
  email to find teammates") for the account-creation onboarding slide. Same
  proposed new row as `auth_manager_tests.rs`.

### `app/src/auth/mod_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/auth/mod.rs` · fork ships source: yes (see above — the localized facade)

- **DECLINED** — `web_logout_url_uses_configured_server_root`. Already
  covered by `DECLINED.md`'s existing "`/logout` slash command" row (#338):
  *"`crate::tui::log_out_tui`... is a documented no-op: 'BYOP has no account
  to log out of.'"* This test asserts the URL a logout button would open;
  the fork's `AuthManager` has no `web_logout_url` at all (`log_out` just
  calls `reset_local_defaults()` — no URL involved).

### `app/src/server/telemetry/events_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/server/telemetry/events.rs` · fork ships source: NO (fork keeps a single `telemetry.rs`, not this directory)

- **DECLINED** — `telemetry_events_have_nonempty_name_and_description`
  iterates `warp_core::telemetry::all_events()` / `TelemetryEventDesc`,
  asserting every registered telemetry event has a non-empty user-facing
  name/description (feeds Warp's public "exhaustive telemetry table" docs
  page). Neither `all_events()` nor `TelemetryEventDesc` exist anywhere in
  the fork — confirmed by the fork's own header comment on
  `app/src/server/telemetry.rs:1-2`: *"the telemetry send layer and context
  provider have been removed. Only the `TelemetryEvent` enum and its helper
  types are kept here, serving as a type shell."* Same "Telemetry" DECLINED row.

### `app/src/server/telemetry/mod_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/server/telemetry/mod.rs` · fork ships source: NO

- **DECLINED** — `test_persist_events_doesnt_include_ugc_events` needs
  `TelemetryApi::new()` and `warpui::telemetry::record_event(...)` disk
  persistence, asserting UGC-flagged events are filtered before the on-disk
  telemetry log is written. `TelemetryApi` does not exist in the fork. Same
  "Telemetry" DECLINED row — there is no send/persist layer left to filter.

### `app/src/workspaces/update_manager_tests.rs` — 1 absent

pin 1 · fork 0 · source `app/src/workspaces/update_manager.rs` · fork ships source: NO

- **CLOUD** — `test_leaving_team_removes_objects`. Source imports
  `super::team_tester::{TeamTesterStatus, TeamTesterStatusEvent}` and
  `super::user_workspaces::CreateTeamResponse` — team-membership-change
  polling against the cloud workspace metadata endpoint. Distinct file from
  `user_workspaces.rs`; not covered by the #445 DECLINED row (that row is
  about `current_team()` always being `None`, not about a background
  membership-change poller, which the fork does not ship at all).

### `crates/warp_server_client/src/auth/mod_tests.rs` — 1 absent

pin 1 · fork 1 (false positive — see below) · source `crates/warp_server_client/src/auth/mod.rs` · fork ships source: NO

- **CLOUD** — `unknown_settings_results_preserve_operation_context`. The
  inventory's "fork ships source: yes" / "fork 1" here is a basename-matching
  false positive: it matched `app/src/auth/mod.rs` (a completely different
  file, already covered above), not this one. `crates/warp_server_client`
  does not exist in the fork. Read against the pin: the test asserts
  `AuthClientImpl::on_settings_updated` error-message pass-through against
  `warp_graphql::mutations::update_user_settings::UpdateUserSettingsResult` —
  telemetry/crash-reporting/cloud-conversation-storage *cloud sync-settings*
  RPC, unrelated to the fork's local `auth/mod.rs` facade.

## New DECLINED rows to propose

Two decisions found in this pass are real, already implemented, and
documented in fork source comments, but have **no corresponding `DECLINED.md`
row** — I did not add one myself (per instructions, another agent owns that
file), but the ledger owner should add one covering:

**External account system (OAuth device-code login, browser handoff, SSO
link, paste-auth-token) — physically removed.** Affects
`app/src/auth/auth_manager_tests.rs` (7) and `app/src/auth/login_slide_tests.rs` (1),
8 tests total. Evidence: `app/src/auth/mod.rs`'s own top-of-file doc comment
(quoted in the `auth_manager_tests.rs` section above) states 21 UI/RPC/
token-persistence/web-handoff files were physically deleted "together with
the external account system," and that `AuthStateProvider` now always
returns a local placeholder "logged in, unlimited Free Tier" user. Proposed
row, in `DECLINED.md`'s existing style:

> | **External account system (OAuth login, browser handoff, SSO link)** | — | `app/src/auth/mod.rs` documents 21 physically-deleted files (`login_slide`, `paste_auth_token_modal`, `web_handoff`, `auth_view_modal`, etc.) "removed together with the external account system." `AuthStateProvider` now always returns a local placeholder user; `AuthManager::log_out` resets to that placeholder instead of ending a cloud session. No BYOP equivalent — there is no external account to log into. |

This is distinct from the already-declined `/logout` row (#338, which is
about the *slash command* being a no-op) and from #11 (billing/paid tiers) —
it is the login/auth-flow UI and RPC layer specifically.

## Ranked list — least sure it compiles

Nothing was ported this pass (all evidence-gathering, zero code changes), so
there is no compile risk from this sweep's own edits. Ranking here is instead
"if a future agent acts on this document's flags, in order of how much
verification they'd still need":

1. **The MISSING-SUBSYSTEM `NetworkLogModel` port** (headline finding) — highest
   uncertainty of anything here. Verified: `set_before_request_fn`/
   `set_after_response_fn` exist and are unused; `reqwest` and `async-channel`
   are workspace deps. NOT verified: whether `bounded_vec_deque` needs adding
   fresh or already exists transitively; whether `http_client::Client`'s
   hook-fn types (`RequestHookFn`/`ResponseHookFn`) match the pin's closure
   signatures exactly; where the pane/settings-page half would live if ever
   built (no build access was used to check any of this).
2. **The two proposed `DECLINED.md` rows** — text is evidence-backed but
   unreviewed by the ledger owner; if the wording is rejected the affected
   8 auth tests fall back to unadjudicated rather than wrongly-adjudicated.
3. Everything else in this document is a **pure read** (imports, `find`,
   `git show` at the pin) with no code written, so there is nothing else here
   that "compiles" in the first place.
