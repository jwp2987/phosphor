# Sweep verdicts — the tail block

Oracle: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. Never `warp/master`.

Scope per the task brief: the small per-file remainder left after the six-area sweep
(`docs/SWEEP-SUMMARY.md`) — `app/src/util/**`, `app/src/tracing/**`, `app/src/uri/**`,
`crates/http_client/**`, `app/src/root_view_tests.rs`, `app/src/code_review/**`,
`app/src/tui/**`, `app/src/notebooks/**`, `app/src/ui_components/**`,
`crates/warp_server_auth/**`, `app/src/ai/**` stragglers, `app/src/local_control/**`,
`app/src/lib_tests.rs`, `app/src/drive/**`, `crates/mcp/**`, `crates/warpui/**`,
`app/src/tab_configs/**`, `app/src/bin/**`. Sibling agents own `crates/computer_use`,
the cloud block, and the `remote_server`/persistence/core block.

## Method — and why the numbers below differ from the task brief's estimate

`SCOPE-REST.md` (repo root) is the mechanical per-file inventory the task brief's
per-area counts were drawn from. It is **stale**: this fork ports fast, and several
of these areas (`app/src/local_control`, `crates/mcp`, `app/src/tab_configs`,
`app/src/util`) have had most of their gap closed since that inventory was written.
Trusting its counts would both overstate some areas (the file-level "missing" number
counts renamed tests as absent) and understate nothing — but it can't be trusted
either way without re-deriving.

For every file in scope: pulled the pin's test-function names with
`git show 02b53fcd8:<path>`, matched them **by name against the whole fork tree**
(not by path — this fork renames `*_tests.rs` → `*_test.rs` and flattens
`a/b/c_tests.rs` → `a/b_c_tests.rs`), then read both the pin source and the current
fork source for every name with no match. `docs/sweep-verdict-ledger.tsv` was checked
for every candidate before writing a verdict — the ledger has **zero** rows for any
path in this block except `app/src/ai/**` (see the stragglers section), confirming
this genuinely is the un-adjudicated tail.

Work was split three ways and re-verified by hand afterward against pin source,
current fork source, and `DECLINED.md`:
- `app/src/util/**`, `app/src/tracing/**`, `app/src/uri/**`, `crates/http_client/**`
- `app/src/code_review/**`, `app/src/root_view_tests.rs`, `app/src/tui/**`,
  `app/src/notebooks/**`, `app/src/ui_components/**`
- `crates/warp_server_auth/**`, `app/src/local_control/**`, `app/src/lib_tests.rs`,
  `app/src/drive/**`, `crates/mcp/**`, `crates/warpui/**`, `app/src/tab_configs/**`,
  `app/src/bin/**`, `app/src/ai/**` stragglers

## Bucket counts (84 tests adjudicated; see "stragglers" note below for the rest of
## the ~93 estimate)

| bucket | tests | files |
|---|---:|---|
| CLOUD | 53 | `uri_tests.rs` (9), `http_client/{iap,lib}_tests.rs` + inline `origin_tests` (10), `tracing/cloud_agent_auth_tests.rs` (13), `root_view_tests.rs` (6), `tui/mod_tests.rs` (6), `notebooks/notebook_tests.rs` (3), `warp_server_auth` (5), `drive/sharing/qr_code_tests.rs` (1) |
| DIVERGENT | 14 | `util/file/external_editor/linux_tests.rs` (10), `code_review/diff_state/remote_tests.rs` (4) |
| DECLINED | 6 | `util/bindings_tests.rs` (1), `notebooks/notebook_tests.rs` (1), `lib_tests.rs` (2), `drive/index_tests.rs` (1), `bin/generate_settings_schema_tests.rs` (1) |
| COVERED-ELSEWHERE | 4 | `code_review/diff_state/mod_tests.rs` (1), `local_control` (2), `tab_configs` (1) |
| MISSING-SUBSYSTEM | 6 | `ui_components/json_tree_tests.rs` (5), `crates/warpui/tests/headless_main_thread.rs` (1) |
| PORTABLE — identified, **not ported** (see rationale) | 1 | `crates/mcp/src/oauth_tests.rs::loopback_oauth_completes_dcr_and_code_exchange` |
| **PORTED this sweep** | **0** | — |

Sum check: 53 + 14 + 6 + 4 + 6 + 1 = 84.

**Headline: zero ports.** Every candidate was traced to a genuine cloud dependency,
an already-documented deliberate divergence, an existing fork test under a different
name, or a real non-cloud gap too large/risky to port blind (a JSON-tree MCP-result
renderer subsystem; a macOS-only GCD dispatch-queue test needing new dev-dependencies
and a new `[[test]]` target). This is a re-checked result, not a shortcut — see the
per-area sections below, especially the utility/URI/tracing/http_client areas the task
brief specifically flagged as likely to be over-classified CLOUD. They were rechecked
and the CLOUD verdicts hold: every one names a specific absent symbol or an import that
only exists to talk to Warp's server.

---

## `app/src/util/**` (test-by-test)

pin footprint across 11 test files; **1 DECLINED, 10 DIVERGENT, 0 else absent** — the
rest (`openable_file_type_tests.rs`, `mac_tests.rs`, `mod_test.rs`, `git_tests.rs`,
`image_tests.rs`, `link_detection_test.rs`, `path_test.rs`, `time_format_tests.rs`,
inline `mod.rs` tests) are **fully covered**, 1:1 by name, several with fork-added
extras.

- **DECLINED (1):** `test_orchestration_cycle_bindings_are_editable`
  (`util/bindings_tests.rs`) — exact match for `DECLINED.md`'s `#376`/`#410` row
  (orchestration "pill bar" child-agent cycle bindings; cloud-adjacent UI absent here).
- **DIVERGENT (10):** `util/file/external_editor/linux_tests.rs`. The pin exercises a
  hand-rolled `tokenize_exec` directly; the fork replaced it with the `shell_words`
  crate (`linux.rs:90`), so the 9 pure `tokenize_exec` unit tests
  (`test_tokenize_simple`, `test_tokenize_quoted_argument`,
  `test_tokenize_escape_sequences_in_quotes`, `test_tokenize_multiple_whitespace`,
  `test_mixed_quoted_and_unquoted_in_single_token`, `test_tokenize_empty_string`,
  `test_tokenize_quoted_empty_string_produces_token`,
  `test_tokenize_unrecognized_escape_in_quotes_keeps_backslash`,
  `test_tokenize_unterminated_quote_errors`) have no function left to test. Already
  documented in-source at `linux_tests.rs:397-403`. The 10th,
  `test_unterminated_quote_errors`, is behavioral (goes through
  `build_default_command`) but the pin's `DesktopExecError::UnterminatedQuote` variant
  no longer exists — `linux.rs:90` maps every `shell_words::split` failure to
  `DesktopExecError::MalformedFieldCode` (verified: the enum at `linux.rs:567-579` has
  only `IoError`/`DecodeError`/`NoExec`/`MalformedFieldCode`). **Verified this is not a
  user-visible regression**: `DesktopExecError` is a private `enum` with zero callers
  outside `linux.rs` that branch on its variants (`grep` confirms), so both failure
  modes already surface as "the external editor didn't open" either way — collapsing
  the variant changed an internal error-message string, not behavior. Worth a
  `DECLINED.md` row anyway (see proposal below) since it's currently only documented in
  a source comment, not the ledger.

## `app/src/tracing.rs` (not a directory — 1 file)

- **CLOUD (13):** all of `tracing/cloud_agent_auth_tests.rs`. Confirmed: fork's
  `app/src/tracing.rs` is a 10-line stub (`subscriber::set_global_default(NoSubscriber::new())`).
  Pin module doc: signs OTLP exports with a cloud task identity token via
  `warp_managed_secrets::client::{IdentityTokenOptions, ManagedSecretsClient, TaskIdentityToken}`,
  audience `"warp-cloud-agent-otel"`. Absent tests:
  `authorization_overwrites_supplied_header`, `authorized_request_debug_redacts_token`,
  `client_with_expiry`, `debug_output_redacts_token`, `expected_run_id_is_required`,
  `expired_token_is_refused_and_supplied_header_is_removed`, `jwt_with_payload`,
  `malformed_refreshed_tokens_are_rejected`, `refreshed_token_run_id_exactly_matches`,
  `refreshed_token_run_id_is_required`, `refreshed_token_run_id_must_be_a_string`,
  `refreshed_token_run_id_must_match`, `rejected_refreshed_token_preserves_previous_token`.

## `app/src/uri/**`

- **CLOUD (9):** `uri_tests.rs`. All construct `Action::CreateEnvironment{..}` /
  `Action::CloudAgentSetup` / `Action::AutoHandoffToCloud{..}` / `Action::FocusCloudMode`
  / `Action::NewCloudAgentConversation`; confirmed none of these `Action` variants
  exist in the fork's `uri/mod.rs` (`grep` empty). Same family as `DECLINED.md`'s
  Warp Environments (`#211`) and account-first (`#11`) rows. Tests:
  `test_action_auto_handoff_to_cloud_parse_alias_path`,
  `test_action_auto_handoff_to_cloud_parse_default_trigger`,
  `test_action_auto_handoff_to_cloud_parse_sleep_trigger`,
  `test_action_cloud_agent_setup_parse`, `test_action_create_environment_parse`,
  `test_action_create_environment_parse_no_repos`, `test_action_focus_cloud_mode_parse`,
  `test_action_new_cloud_agent_conversation_parse`,
  `test_app_web_link_rewrites_to_new_cloud_agent_conversation`.
  `docker_tests.rs` is fully covered (fork adds `uri_validation_test.rs` besides).

## `crates/http_client/**`

- **CLOUD (10):**
  - `iap_tests.rs` (4): `challenge_status_without_iap_header_is_not_a_challenge`,
    `challenge_statuses_with_iap_header_are_challenges`, `headers_with_iap`,
    `non_challenge_status_with_iap_header_is_not_a_challenge`. `iap.rs` (absent from
    fork) is Google Identity-Aware Proxy support for Warp's enterprise cloud endpoint.
  - `lib_tests.rs` (4): `injects_trace_link_header_when_span_active`,
    `omits_trace_link_header_when_no_span`,
    `request_carries_trace_link_header_on_warp_header_path`, `with_active_span`.
    Confirmed fork's `crates/http_client/Cargo.toml` has **no** `opentelemetry` /
    `tracing-opentelemetry` dependency at all. Pin comment (`lib.rs:355-357`): "so the
    server can attach a span link back to this client (**cloud-agent**) span."
  - inline `mod origin_tests` in `lib.rs` (2): `server_and_rtc_origins_match`,
    `third_party_origin_does_not_match`. Test `is_warp_server_origin`, which doesn't
    exist in the fork (confirmed: no hits repo-wide); its only purpose is
    distinguishing Warp's own server/RTC origin from third parties, and `rtc_http_url`
    (real-time-collab, cloud) is likewise absent.
  - `lib.rs` inline tests (2 pin tests, not counted above): fully covered.

---

## `app/src/code_review/**`

pin footprint across 12 test files (126 tests); **5 absent, 1 COVERED-ELSEWHERE + 4
DIVERGENT**. Everything else — `comments/{batch,diff_hunk_parser}_tests.rs`,
`find_model_tests.rs`, `hidden_lines_tests.rs`, `code_review_view_tests.rs`,
`comments/comment_tests.rs`, `github_repo_model`/`git_repo_model` local tests,
`diff_state/local_tests.rs` — is either fully covered or (per `SCOPE-REST.md`,
re-verified) a `D` feature-gap already outside this block's numbers because the source
was never forked (not "absent tests", absent feature — not actionable as a test port).

- **COVERED-ELSEWHERE (1):** `new_for_test_creates_local_variant`
  (`diff_state/mod_tests.rs`) — renamed to `new_creates_local_variant` in the fork's
  `code_review/diff_state_tests.rs:558`, same assertion (`DiffStateModel::new(None, ctx)`
  replaces a dedicated `new_for_test` constructor).
- **DIVERGENT (4), all in `diff_state/remote_tests.rs`**, all already documented
  in-source:
  - `get_committed_branch_files_response_emits_domain_files` — the fork moved this RPC
    handling into `git_dialog/pr.rs::spawn_load_remote_file_changes` (an inline
    `ctx.spawn` callback instead of a dedicated model event); the pure proto→domain
    conversion is separately unit-tested as `file_change_entry_round_trips`
    (`app/src/remote_server/diff_state_proto_tests.rs:204`).
  - `apply_snapshot_loaded_preserves_content_at_base_in_event` and
    `apply_file_delta_preserves_content_at_base_in_event` — fork's `FileDiff`/
    `GitDiffData` carry no base-content field; content lives only in the
    non-persisted `FileDiffAndContent` wrapper.
  - `apply_snapshot_loaded_without_diffs_becomes_error` — documented at
    `diff_state_remote_tests.rs:409-413`: fork's `DiffState::Loaded` always carries
    `GitDiffData`, so the pin's "loaded but empty" invalid state can't be constructed.

## `app/src/root_view_tests.rs`

- **CLOUD (6):** `account_first_class_uses_paid_status_then_fresh_request_limit`,
  `account_first_requires_login_even_without_ai_or_drive_settings`,
  `fallback_flow_only_requires_login_for_account_backed_settings`,
  `account_first_classes_route_to_paid_or_the_expected_offer`,
  `account_first_completion_metadata_matches_terminal_outcomes`,
  `refreshing_pending_onboarding_choices_replaces_stale_settings`. All reference
  `RootView::account_first_class` / `FtueAccountClass`, confirmed absent from
  `root_view.rs` (no hits). Exact match for `DECLINED.md`'s "Account-first onboarding,
  billing, paid tiers" row (`#11`).

## `app/src/tui/**`

- **CLOUD (6):** `mod_tests.rs`:
  `tags_tui_verification_url_without_losing_existing_query_parameters`,
  `leaves_invalid_verification_url_unchanged`,
  `stores_device_fallback_before_opening_browser`,
  `renders_device_code_request_timeout_without_id_token_prefix`,
  `emits_logged_in_event_when_login_completes`,
  `emits_logged_out_event_and_resets_login_details` — all test the device-authorization
  OAuth flow (`TuiLoginModel`, `TuiLoginPhase::AwaitingLogin`, `tui_verification_url`).
  Confirmed: `app/src/tui/mod.rs`'s own doc comment (lines 1-8) says "Zap is BYOP: there
  is no account to log into ... the login model here is a trivial always-`LoggedIn`
  stand-in." **Not currently in `DECLINED.md`** — proposed row below.

## `app/src/notebooks/**`

pin footprint across 14 test files (120 tests); **4 absent** (1 DECLINED, 3 CLOUD). The
rest — `editor/model_tests.rs`, `editor/view_tests.rs`, `notebook_tests.rs` (remainder),
`file/mod_tests.rs`, `link_tests.rs`, `context_menu_tests.rs`, `manager_tests.rs`,
`notebook/details_bar_tests.rs` — fully covered.

- **DECLINED (1):** `test_edit_telemetry` — relies on `warpui::telemetry::flush_events()`
  capturing an emitted event. Confirmed: `crates/warp_core/src/telemetry.rs`'s
  `send_telemetry_from_ctx!` macro wraps the event expression in `if false { ... }` —
  it type-checks but never evaluates or sends. Exact match for `DECLINED.md`'s
  Telemetry row ("Nothing is sent").
- **CLOUD (3):** `test_close_with_pending_changes`, `test_only_user_title_edits_synced`,
  `test_conflicting_notebook_read_only` — all exercise `SyncQueue`/`CloudModel`, which
  are entirely absent from `notebook.rs` (confirmed: zero hits; comment at
  `notebook_tests.rs:556` marks the removal "Zap (Wave 4)"). The fork replaced
  cloud-synced notebooks with a local `ObjectStoreModel`. **Not currently in
  `DECLINED.md`** — proposed row below.

## `app/src/ui_components/**`

- **MISSING-SUBSYSTEM (5):** `json_tree_tests.rs`:
  `mcp_result_success_with_structured_content_returns_tree`,
  `mcp_result_success_with_json_text_content_returns_parsed_tree`,
  `mcp_result_success_with_non_json_text_returns_string_tree`,
  `mcp_result_error_returns_error_variant`, `mcp_result_cancelled_returns_cancelled_variant`.
  `McpRenderable`/`mcp_result_to_renderable` genuinely don't exist (pre-confirmed by the
  task brief and re-checked: the only tree-wide hits are a comment in
  `json_tree_tests.rs:303` explaining the scope decision). The generic `json_tree.rs`
  widget itself is present and fully tested elsewhere in the same file; only the
  MCP-result-specific adapter is missing. Not ported: building the adapter is new
  feature work, not a test port, and out of this block's conservative scope.

---

## `crates/warp_server_auth/**`

- **CLOUD (5):** `user_tests.rs::test_parse_user_profile`,
  `user_tests.rs::test_user_global_skills_defaults_to_empty`,
  `user/persistence_tests.rs::test_deserialize_2026_03_06_persisted_user`,
  `user/persistence_tests.rs::test_serialize_persisted_user`,
  `user/persistence_tests.rs::test_windows_user_persistence`. Crate entirely absent
  from the fork. Verified per the task brief's instruction to check dependencies
  directly: `user.rs` imports `warp_graphql::queries::get_user::FirebaseProfile`;
  the crate's `Cargo.toml` depends on `warp_graphql`. This is the one area in the tail
  block that is genuinely, unambiguously cloud — confirmed, not assumed.

## `app/src/local_control/**` + `app/src/settings/local_control.rs`

**Correction to `SCOPE-REST.md`: this area is NOT absent.** At the time that inventory
was written, `app/src/local_control/**` (15 files) didn't exist in the fork; it does
now — every file at the pin path exists in the fork at the identical path
(`git ls-tree -r 02b53fcd8 -- app/src/local_control` vs `find app/src/local_control`:
identical file lists). Per-file name diff of every test file
(`mod_tests.rs`, `handlers/{app_state,layout,metadata}_tests.rs`,
`settings/local_control_tests.rs`) found only:

- **COVERED-ELSEWHERE (2):**
  - `unavailable_surface_open_returns_structured_error` (pin,
    `handlers/app_state_tests.rs`) → fork's
    `agent_management_open_action_is_rejected_as_unsupported` (same file). Fork's
    stricter framing: the "agent management" surface (the thing the pin's generic
    "unavailable surface" test exercised) is unconditionally rejected rather than
    conditionally unavailable.
  - `agent_management_surface_reports_feature_flag_unavailable` (pin,
    `handlers/metadata_tests.rs`) → fork's
    `agent_management_surface_is_unconditionally_unavailable` (same file). Same
    semantic shift: the pin gates this behind a feature flag that can be enabled; the
    fork hardcodes it off since agent-management is a cloud dashboard feature (see the
    `DECLINED.md` `ActiveAgentViewsModel` row for the same family).

Everything else — `mod_tests.rs` (24/24), `handlers/layout_tests.rs` (2/2),
`settings/local_control_tests.rs` (10/10) — matches 1:1 by name. This confirms
`SCOPE-REST.md`'s own aside ("`crates/local_control` ... is ported and unwired — see
`#216`") is now stale in the good direction: it's wired.

## `app/src/lib_tests.rs`

- **DECLINED (2):** `tui_uses_distinct_secure_storage_service_name`,
  `app_keeps_default_secure_storage_service_name`. The fork's own `lib_tests.rs`
  already carries a header comment (lines 1-14) documenting exactly this: the fork
  deliberately gives the TUI the same `AppId`/keychain namespace as the GUI (commit
  `fcf5aaf56`) so `/model` can see the GUI's BYOP providers, reversing the pin's
  separate-identity design these two tests assert. **Exact match for `DECLINED.md`'s
  "TUI/GUI shared app id" row** — no new row needed, just confirming the citation
  holds. (The source comment says "no issue filed" for this divergence, which is a
  pre-existing gap against `AGENTS.md` §5.11, not something this sweep introduced.)

## `app/src/drive/**`

- **DECLINED (1):** `index_tests.rs::test_shared_object_limit_banner_dismissal_persists_per_type`.
  Traced: `SharedObjectLimitBannerKind`/`DismissObjectLimitBanner`/
  `is_object_limit_banner_dismissed` are confirmed absent from `drive/index.rs`
  (zero hits). The pin's mechanism reads `self.auth_state.personal_object_limits()`
  — a free/paid-tier account limit, i.e. the same account/billing family as
  `DECLINED.md`'s "Account-first onboarding, billing, paid tiers" row (`#11`).
- **CLOUD (1):** `sharing/qr_code_tests.rs::qr_matrix_for_url_returns_square_matrix_with_dark_modules`.
  `qr_code.rs` itself doesn't exist in the fork; the sibling `sharing/mod.rs` that
  would use it imports `cloud_object::{model::persistence::ObjectStoreModel, Owner}`
  directly and builds share URLs for `https://app.warp.dev/session/...` — Drive
  session-sharing, the same family as `DECLINED.md`'s Drive-Spaces/`#267` row.
  (This fork's Drive — renamed **Library** in display strings only, per this task's own
  brief — is a local object browser; sharing a session link is the collaborative,
  genuinely-cloud half, consistent with what `#267` already declines.)

## `crates/mcp/**`

Fork's MCP now lives at `app/src/ai/mcp/templatable_manager/`. Name-matched:
`runtime_tests.rs` is **fully covered** (16/16, at `.../native_tests.rs`).
`oauth_tests.rs` has 8 pin tests; 7 are ported verbatim.

- **PORTABLE, identified but NOT ported (1):**
  `loopback_oauth_completes_dcr_and_code_exchange`. The underlying capability is real
  and reachable: fork's `make_authenticated_client` has an `is_headless: bool` branch
  (`oauth.rs:308-315`) that does bind `LoopbackOAuthReceiver` and does call
  `auth_manager.register_client(...)` — the same DCR-plus-loopback-callback mechanism
  the pin test exercises. **Why not ported**: the pin's test builds a minimal
  closure-based `AuthContext`; the fork's `AuthContext` (`oauth.rs:195-204`) has grown
  to 6 fields including `spawner: ModelSpawner<TemplatableMCPServerManager>` and
  `oauth_result_rx: async_channel::Receiver<CallbackResult>`, which means the test
  needs a real `App::test`/entity-registration harness for
  `TemplatableMCPServerManager`, not a straight paste of the pin's fixture. That is
  real test-writing work I can't safely do without a compiler to catch mistakes in the
  harness wiring, per this task's conservatism instruction. Flagging as the strongest
  actual port candidate in the whole tail block, worth a follow-up with build access.

## `crates/warpui/**`

- **MISSING-SUBSYSTEM (1):** `tests/headless_main_thread.rs::services_main_dispatch_queue`.
  macOS-only (`#[cfg(target_os = "macos")]`), a custom `libtest_mimic` single-threaded
  harness verifying `dispatch2::run_on_main` correctly services the process main thread
  from the app's headless run loop. Confirmed: `dispatch2` is not a dependency of
  `crates/warpui` anywhere, and the fork's `run_on_main_thread`
  (`platform/mac/delegate.rs:480`) is a different (non-GCD) implementation with no
  equivalent thread-affinity test. Not ported: needs a new dev-dependency
  (`dispatch2`, `libtest-mimic`) and a new `[[test]]` binary target in
  `crates/warpui/Cargo.toml`, is macOS-only so unverifiable on this Linux host even in
  principle, and risks silently asserting nothing if the harness is wired wrong. Least
  confident item in this report if anyone attempts it blind — see ranked list below.

## `app/src/tab_configs/**`

- **COVERED-ELSEWHERE (1):** `snapshot_cloud_pane_gets_cloud_type` → fork's
  `snapshot_cloud_pane_gets_agent_type` (`session_config_tests.rs:653`), same
  assertion, renamed for the fork's "agent" terminology instead of "cloud" pane type.

## `app/src/bin/**`

- **DECLINED (1):** `generate_settings_schema_tests.rs::surface_annotation_matches_setting_schema_entry_metadata`.
  Confirmed: fork ships `generate_settings_schema.rs` with no companion test file at
  all, and `surfaces_fn`/`SettingSurfaces` don't exist on the current
  `SettingSchemaEntry`. Exact match for `DECLINED.md`'s "`SettingSurfaces` /
  `SettingsMode`" row (already carries `sym:SettingSurfaces sym:SettingsMode` markers).

## `app/src/ai/**` stragglers

**Investigated, nothing left to adjudicate here.** Every absent-test candidate this
search turned up under `app/src/ai/**` —
`conversation_details_panel_tests.rs` (13 tests),
`mcp/builtin_tests.rs` (7), `agent/api/impl_tests.rs` (15), and
`get_relevant_files/remote_search/native_tests.rs` (1) — already has ledger rows under
`area=app-ai` in `docs/sweep-verdict-ledger.tsv` (8, 6, 13, and 1 rows respectively,
confirmed by direct grep). Those belong to the six-area sweep's 917-test `app/src/ai`
pass, not this tail block; re-adjudicating them here would duplicate that work, not
extend it. I could not find 4 genuinely un-adjudicated `app/src/ai` stragglers outside
that sweep's coverage with the effort this block warrants — if the task owner has a
more specific pointer for what "stragglers" means here, it needs re-deriving from a
narrower brief.

---

## What was ported this sweep

**Nothing.** Every candidate resolved to CLOUD, an existing documented divergence, an
already-covered fork test, or a real subsystem gap too large/risky to port blind. This
was rechecked, not assumed — the task brief specifically warned that a low PORTABLE
count in `util`/`tracing`/`uri`/`http_client` usually means over-assigning CLOUD, so
every CLOUD verdict above cites the specific absent symbol or the specific
cloud-server-only import, not just "reads cloud-shaped." One test
(`loopback_oauth_completes_dcr_and_code_exchange`) is a real, reachable port
candidate, documented above with the exact reason it wasn't attempted blind.

## Fork code defects found (reported per AGENTS.md §5.6/§5.10/§5.11 — not papered over)

1. **`DesktopExecError::UnterminatedQuote` was silently collapsed into
   `MalformedFieldCode`** (`app/src/util/file/external_editor/linux.rs:90`,
   `:567-579`) when `tokenize_exec` was replaced by the `shell_words` crate. Verified
   **not** user-visible today (the enum has zero external callers that branch on its
   variants), but it is exactly the "collapsing a distinct case into a shared one"
   pattern §5.10 calls out as the classic trap, and it currently has no `DECLINED.md`
   row — only a source comment. Proposed row below closes that gap; no code change
   needed unless a future caller starts branching on the variant.

2. **Out of this block's area, flagged for the owning sibling/maintainer**:
   `app/src/workspace/view.rs` and `app/src/workspace/view/wasm_view.rs` (both
   correctly `#[cfg(target_family = "wasm")]`-gated) still `use
   crate::ai::conversation_details_panel::{ConversationDetailsPanel, ...}`, but
   `app/src/ai/conversation_details_panel.rs` was deleted by `002ce4671`
   ("drop agent-management UI + active-views + task-status-sync"), which stripped 438
   lines from `view.rs` but missed these wasm-only leftovers (a struct field, one
   construction call site, plus 7 more usages in `wasm_view.rs`). This compiles today
   only because nothing in this repo's CI targets `wasm`; it will fail to build the
   instant anyone does. Not fixed here — outside this block's file scope and I have no
   way to verify a wasm build. Found during the `app/src/ai` stragglers check; passing
   it forward rather than dropping it.

## Proposed `DECLINED.md` additions (not made — another agent owns that file)

```
| **TUI device-authorization login is a `LoggedIn` stand-in** | — | Phosphor/Zap is
BYOP: `app/src/tui/mod.rs`'s own doc comment states there is no cloud account to log
into, so `TuiLoginPhase` only ever reaches `LoggedIn`; the pin's device-code flow
(`TuiLoginModel`, `tui_verification_url`, browser handoff, timeout rendering) has no
BYOP equivalent to drive it. The shapes are kept intact so `warp_tui`'s login
placeholder still compiles against them. 6 pin tests in `app/src/tui/mod_tests.rs`
permanently unported. <!-- markers: keep:TuiLoginPhase --> |

| **Notebook cloud sync (`SyncQueue`/`CloudModel`) replaced by local `ObjectStoreModel`**
| — | "Zap (Wave 4)" per `app/src/notebooks/notebook_tests.rs:556` — notebooks sync
through a local object store instead of Warp's cloud sync queue and conflict-resolution
protocol. 3 pin tests in `notebook_tests.rs` (`test_close_with_pending_changes`,
`test_only_user_title_edits_synced`, `test_conflicting_notebook_read_only`)
permanently unported; no local equivalent to port them against. |

| **`DesktopExecError::UnterminatedQuote` collapsed into `MalformedFieldCode`** | — |
`app/src/util/file/external_editor/linux.rs` replaced a hand-rolled `tokenize_exec`
with the `shell_words` crate (already documented at `linux_tests.rs:397-403`), which
maps every parse failure — including an unterminated quote — to one
`DesktopExecError::MalformedFieldCode` variant instead of the pin's two distinct
variants. Verified not user-visible (no caller branches on the variant). 10 pin tests
in `linux_tests.rs` permanently unported (9 test the removed `tokenize_exec` directly,
1 asserts the removed variant). |
```

## Ranked list — least confident to compile, if anyone acts on this report

1. **`crates/mcp/src/oauth_tests.rs::loopback_oauth_completes_dcr_and_code_exchange`**
   (PORTABLE, not ported) — needs a full `App::test`/`ModelSpawner<TemplatableMCPServerManager>`
   harness, not a fixture paste; easy to get subtly wrong in a way that compiles but
   asserts nothing.
2. **`crates/warpui/tests/headless_main_thread.rs`** (MISSING-SUBSYSTEM, not ported) —
   macOS-only, needs new `dispatch2`/`libtest-mimic` dev-deps and a new `[[test]]`
   Cargo target; fully unverifiable from this Linux host.
3. Everything else in this report is a verdict, not code — nothing else was written.
