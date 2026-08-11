# Infra block — pin-test sweep

Oracle pin: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. **Not** `warp/master`.

Area: `app/src/remote_server/**` · `app/src/persistence/**` · `crates/repo_metadata/**` ·
`crates/warp_core/**` · `crates/remote_server/**` · `crates/managed_secrets/**` ·
`crates/warpui_core/**` · `crates/warp_multi_agent_client/**` · `crates/graphql/**` ·
`crates/languages/**`.

## Method

Absence was **re-measured directly** against the current tree rather than trusted from
`SCOPE-REST.md` (dated 2026-08-06, four days stale at time of writing and predating
several relevant merges). For every pin test file under this area, every
`#[test]`/`#[tokio::test]`/`#[gpui::test]`/`#[test_case]`/`#[rstest]` function name was
extracted and checked for presence **anywhere in the fork tree by name**, not by path —
per `ORACLE.md`'s rule 1, since the fork renames `*_tests.rs` → `*_test.rs` and
`a/b/c_tests.rs` → `a/b_c_tests.rs`. This directly caught several cases `SCOPE-REST.md`'s
per-*file* classification missed (a file can be netted as "A, N absent" while every one
of those N is actually a renamed duplicate elsewhere — see `repositories_tests.rs` and
`crates/languages/src/lib_tests.rs` below).

**Total measured absent: 81** (the brief's per-subarea breakdown also sums to 81, not 80;
treat 80 as an approximation). One test (`crates/warp_multi_agent_client/src/lib_tests.rs`)
was stated as 7 in the brief; the crate has exactly 6 pin tests total (verified via
`git ls-tree`), so 7 was itself an overcount somewhere upstream of this sweep.

## Verdict counts

| verdict | count |
|---|---:|
| PORTABLE — ported | 14 |
| PORTABLE — identified, **not** ported (risk) | 1 |
| MISSING-SUBSYSTEM (feature gap, D) | 24 |
| DECLINED (matches an existing or proposed `DECLINED.md` decision) | 13 |
| DIVERGENT (new, undocumented-in-`DECLINED.md` behavior difference) | 2 |
| COVERED-ELSEWHERE (renamed duplicate, false-positive absence) | 8 |
| CLOUD | 18 |
| CLOUD/MISSING-SUBSYSTEM 3-per file additionally verified unblocked | — |
| **Total** | **80** (81 measured) |

---

## `app/src/remote_server/**` (23 measured absent)

### `diff_state_proto_tests.rs`

| test | verdict | evidence |
|---|---|---|
| `pr_info_round_trips_through_proto` | COVERED-ELSEWHERE | Fork's `pr_info_round_trips` (same file) asserts the identical round-trip through `pr_info_to_proto`/`proto_to_pr_info` (free functions instead of the pin's `From`/`TryFrom` impls). Same behavior, different name and calling convention. |
| `diff_size_round_trips_through_proto` | **PORTABLE — PORTED** | Ported under its pin name, adapted to the fork's free-function API (`diff_size_to_proto`/`proto_to_diff_size` instead of `From`/`TryFrom`). See "Code defect" below — this port also depends on a fix landed in this PR. |

**Code defect found and fixed (most important finding of this sweep):** the fork's
`DiffSize::Unrenderable` had been collapsed from the pin's `Unrenderable(UnrenderableReason)`
(two variants, `DiffTooLarge` / `FileTooLarge`, each with distinct `Display` text: "Diff is
too large to render" vs "File is too large to render") to a bare unit variant. This is
the exact AGENTS.md §5.10 "collapsing a data-carrying variant into a unit variant" trap.
Consequence, traced end to end: `app/src/code_review/code_review_view.rs`'s
`render_file_content` always showed **"Diff is too large to render"**, even when the real
reason (only reachable over a remote/SSH diff subscription — `diff_state_proto.rs`'s
`file_diff_to_proto`, gated on `MAX_DIFF_SIZE`) was that the **base file content was too
large to ship over the wire**, not that the diff itself was unrenderable. A user reviewing
a large file over SSH would see a message that describes the wrong problem. The existing
fork test `diff_size_round_trips_and_maps_file_too_large` had a comment
*documenting the lossy behavior as if it were accepted* ("The fork lacks the
file-too-large distinction; it collapses to Unrenderable") rather than justifying it —
exactly the anti-pattern AGENTS.md §5.10 calls out. Not tracked in `DECLINED.md` or
`TODO.md`.

**Fixed** (not just flagged, since the fix is small, self-contained, and every call site
was enumerated by repo-wide grep before touching the enum shape — see the "least sure
compiles" ranking for why this is still flagged as risk):
- `app/src/code_review/diff_size_limits.rs`: restored `UnrenderableReason` enum +
  `Display` impl, verbatim from the pin.
- `app/src/remote_server/diff_state_proto.rs`: `diff_size_to_proto`/`proto_to_diff_size`
  now round-trip both reasons (the wire proto already had
  `DIFF_SIZE_UNRENDERABLE_FILE_TOO_LARGE` — only the Rust domain type had regressed).
- `app/src/code_review/code_review_view.rs`: `render_file_content` now matches
  `DiffSize::Unrenderable(reason)` and renders `reason.to_string()`, matching pin exactly.
- Updated `diff_size_round_trips_and_maps_file_too_large`'s stale comment/assertion to
  reflect the restored, non-lossy behavior, and ported `diff_size_round_trips_through_proto`
  as a second, faithful-to-pin round-trip test.

### `diff_state_tracker_tests.rs` (17 tests) + `server_model_tests.rs::diff_states_starts_empty` (1 test)

**MISSING-SUBSYSTEM.** All 18 verified as genuinely absent — **not** a renaming trap.
`app/src/remote_server/diff_state_tracker.rs` exists in the fork (126 lines) but is a
**different, fork-original subsystem with the same filename**: a per-repository git
*watch* (`DiffStateWatch`, `DiffStateTrackerSubscriber`, `#577`) that pushes granular
file-delta notifications. It shares nothing with the pin's `diff_state_tracker.rs`, whose
`RemoteDiffStateManager` (`DiffModelKey`, `PendingDiffStateResponse`, `SubscribeOutcome`)
manages per-(repo,mode) subscriptions as its own entity. **This is exactly the "same
filename ≠ same code" trap `CLAUDE.md` warns about** — trust the content diff, not the
path.

The pin's `RemoteDiffStateManager` responsibilities are inlined instead into
`ServerModel` itself (`server_model.rs`'s own `DiffModelKey`/`PendingDiffStateResponse`
structs and `diff_state_subscribers`/`diff_state_keys_by_conn`/`diff_state_in_flight`/
`diff_state_pending_responses` fields), a decision the fork's own code already documents
and tracks: `server_model.rs:245` ("`DiffModelKey` (`app/src/remote_server/diff_state_tracker.rs`,
issue #324)") and `server_model_tests.rs`'s own comment block ("the pin's equivalent tests
... exercise a separate `RemoteDiffStateManager` entity ... which this change deliberately
does not port (see the #324 module doc note)"). Matches `HANDOFF.md`'s #324 finding
verbatim ("the fork's inline `diff_state_subscriptions` is a subset of the pin's
`RemoteDiffStateManager`, missing model sharing, pending-response queueing and abort").

Not ported: the pin's 17 tests construct and call methods on a standalone
`RemoteDiffStateManager` that has no fork equivalent to construct in isolation (the
fork's bookkeeping is coupled into `ServerModel`, which needs a full daemon harness).
`diff_states_starts_empty` (`server_model_tests.rs`) is the same bucket — it reads
`model.diff_states.read(...)`, a `ModelHandle<RemoteDiffStateManager>` field that does
not exist. **Verify absence before re-deriving:** `grep -rn "RemoteDiffStateManager"`
returns zero real definitions, only these tracking comments.

### `server_model_tests.rs` (4 tests)

| test | verdict | evidence |
|---|---|---|
| `remote_agent_context_snapshot_broadcasts_replacements_and_initializes_once` | **PORTABLE — PORTED** | The whole feature exists: `remote_agent_context_snapshot`, `send_remote_agent_context_snapshot_to_connection`, `broadcast_remote_agent_context_snapshot` (all `#[cfg(feature = "local_fs")]`). Ported, adapted to the fork's synchronous `test_model()` (no `warpui::App::test` wrapper needed — neither method takes a `ModelContext`). Added `test_bundled_skill_proto` helper matching the pin's. |
| `empty_initialize_clears_auth_context` | DIVERGENT (new finding, untracked) | See below. |
| `empty_authenticate_clears_auth_token` | DIVERGENT (new finding, untracked) | See below. |
| `diff_states_starts_empty` | MISSING-SUBSYSTEM | Same `RemoteDiffStateManager` bucket above. |

**DIVERGENT finding, not yet in `DECLINED.md` — flagging for maintainer sign-off, not
fixing blind.** `server_model.rs`'s `auth_token: Option<String>` field carries a doc
comment stating it is **"intentionally retained across proxy connection teardown and
cleared only by daemon process exit"** — both `handle_initialize` (`if !msg.auth_token.is_empty()
{ self.auth_token = Some(...) }`) and `handle_authenticate` (`if msg.auth_token.is_empty()
{ log::warn!(...); return; }`) treat an **empty** token as a no-op rather than a clear.
The pin's `empty_initialize_clears_auth_context`/`empty_authenticate_clears_auth_token`
assert the opposite: re-Initialize/Authenticate with an empty token clears the stored
credential (a "sign out" signal). This fork has no `AuthState`/`user_id`/`user_email`
concept at all (correctly dropped — no cloud account, matching the "Status-menu org/email
fields" DECLINED.md row's pattern), so those parts of the pin test are legitimately out of
scope; but the `auth_token`-sticky behavior itself is a genuine, plausible-but-unreviewed
security-relevant design choice (arguably hardening against a client silently clearing a
daemon's shared bearer credential by omission; arguably also a footgun if the intent
really was "empty token signs out"). **Not fixed** — this is exactly the class of change
AGENTS.md §5.10 requires explicit maintainer sign-off + a tracking issue for, and this
task's constraints forbid filing one. Proposed `DECLINED.md` text below.

---

## `app/src/persistence/**` (10 measured absent)

### `block_list_tests.rs`

| test | verdict | evidence |
|---|---|---|
| `process_ai_queries_for_nld_history_match_filters_empty_and_whitespace_inputs_oldest_first` | MISSING-SUBSYSTEM (already tracked) | `process_ai_queries_for_nld_history_match` does not exist in `block_list.rs` — only `process_ai_queries_for_uparrow_prompt`. **Already tracked, not a new finding**: `app/src/ai/blocklist/history_model.rs:2240-2244`'s doc comment states the SQLite-backed `nld_prompts` read "is tracked separately (superseded by #336/#337/#331)", and `FeatureFlag::NldPromptHistoryMatch` is off by default (matching the pin), so "in production this currently has no observable effect either way." Not ported — porting the bare function without the `FeatureFlag`-gated `sqlite.rs` read-path wiring would be exactly the "ported but never wired" class `TODO.md` calls out. |

### `sqlite_tests.rs`

| test | verdict | evidence |
|---|---|---|
| `app_scope_database_path_matches_app_database_path` | DECLINED | See below. |
| `tui_scope_database_path_is_tui_subdirectory_of_app_database_dir` | DECLINED | See below. |
| `database_path_for_current_scope_defaults_to_app_scope` | DECLINED | See below. |
| `remote_server_daemon_scope_database_path_uses_identity_data_dir` | DECLINED | See below. |
| `remote_server_daemon_scope_database_path_handles_empty_identity_key` | DECLINED | See below. |
| `remote_server_daemon_database_permissions_are_owner_only` | DECLINED | See below. |
| `sqlite_read_restores_app_state_and_codebase_metadata` | **PORTABLE — PORTED** | `read_sqlite_data(conn, current_user_id)` takes no scope parameter and always returns everything (fork has no scope split at all — see DECLINED row). Ported with the `PersistedDataScope::Full` argument dropped; everything else (`AppState`, `save_codebase_index_metadata`, `codebase_indices`) matches. |
| `tui_database_in_tui_subdirectory_round_trips_data` | DECLINED | Mirrors `init_db(&PersistenceScope::Tui)` explicitly in its own doc comment — same scope-split gap. |
| `sqlite_writer_reuses_codebase_index_metadata_events` | **PORTABLE — PORTED** | Does not touch `PersistenceScope` at all — pure writer round-trip (`start_writer`, `ModelEvent::UpsertCodebaseIndexMetadata`/`DeleteCodebaseIndexMetadata`, `get_all_codebase_index_metadata`). All symbols present with matching shapes; ported near-verbatim (only the metadata type name changed: pin's inline construction vs this fork's `ai::workspace::WorkspaceMetadata`, added as a `test_workspace_metadata` helper). |

**DECLINED — proposed addendum to `DECLINED.md`'s "TUI/GUI shared app id" row.**
`PersistenceScope` (`App`/`Tui`/`RemoteServerDaemon { identity_key }`) and
`database_file_path_for_scope`/`PersistedDataScope` do not exist anywhere in the fork —
confirmed by repo-wide grep. `app/src/persistence/sqlite.rs` has exactly one
unscoped `database_file_path()` and `read_sqlite_data(conn, user_id)` (no scope param):
one shared SQLite database for GUI, TUI, and (implicitly) any remote-server daemon
process, consistent with the existing row's "one app id... between GUI and TUI." The
existing row says "**Two** pin tests assert the separation and are intentionally not
ported" — this sweep found **6** (the ones listed above), all in the same family, plus
3 in `crates/warp_core/src/paths_tests.rs` below (10 total across both files). The "two"
count in the current row is an undercount; propose updating it to cite all 9 by name (the
10th, `test_gui_config_and_mcp_paths_resolve_explicit_sources`, is arguably a distinct
sub-case — GUI-vs-daemon config dir naming, not TUI/GUI — worth the DECLINED.md owner's
own judgment on whether to fold it in or list separately). Also worth noting for that
owner: `RemoteServerDaemon`-scoped isolation is a **security-relevant** gap, not just a
GUI/TUI one — `remote_server_daemon_database_permissions_are_owner_only` asserts the
daemon's per-identity database gets `0600`-only permissions, which the fork's single
shared database does not distinguish by identity at all.

---

## `crates/repo_metadata/**` (9 measured absent)

### `local_model_test.rs` (renamed from `local_model_tests.rs`; 6 tests)

| test | verdict | evidence |
|---|---|---|
| `index_lazy_loaded_path_tracks_only_root` | **PORTABLE — PORTED** | Verbatim port. Every dependency (`repo_watches`, `RootWatchMode::{Recursive,NonRecursive}`, `LocalRepoMetadataModel::new_for_test`, `index_lazy_loaded_path`, `is_lazy_loaded_path`, `await_build_tasks_for_repo`) exists with matching shapes — this file already carries 48/54 pin tests. |
| `recursive_repo_uses_recursive_watch_mode` | **PORTABLE — PORTED** | Same; also uses `add_repository_internal`, `empty_repo_state` (both present). |
| `remove_repository_clears_extra_dir_watches` | **PORTABLE — PORTED** | Same; also `remove_repository`, `repository_state`, `load_directory`. |
| `remove_lazy_loaded_path_clears_tracked_watches` | **PORTABLE — PORTED** | Same; also `remove_lazy_loaded_path`. |
| `deleted_subdir_drops_its_tracked_watch` | **PORTABLE — PORTED** | Same; also `handle_watcher_event`, `BulkFilesystemWatcherEvent`, `RepositoryMetadataEvent::FileTreeEntryUpdated`, `to_local_path`. |
| `lazy_root_created_directory_inserted_as_placeholder` | **PORTABLE — identified, NOT ported (risk)** | See below. |

**Not ported, flagged as the sweep's highest-risk-if-wrong finding.** The pin's
`compute_file_tree_mutations` takes an extra `lazy_load: bool` parameter (5 args) that
this fork's version lacks (4 args) — the fork always computes a full
`FileTreeMutation::AddDirectorySubtree` and defers the lazy/eager decision entirely to
`apply_file_tree_mutations`'s `lazy_load` flag, which **skips** a mutation only when
`!is_parent_loaded_in_entry(root_entry, &std_dir)`. In this test's exact scenario the new
directory's parent **is** the (loaded) root, so that guard would not fire — reading the
code, the fork would materialize the **full subtree** where the pin inserts an **unloaded
placeholder**. This traces as a plausible real laziness regression in the incremental
watcher path (a newly-created directory under an already-expanded lazy root might get
eagerly walked instead of deferred), but confirming that requires running the test
against a build, which this sweep cannot do. Left unported per the task's "leave a row
PORTABLE with a reason rather than porting on a guess" instruction. **Ranked #1 in "least
sure compiles / most needs follow-up" below** — not because the code I wrote is uncertain
(I wrote none for this one), but because the underlying behavior needs a build-verified
answer before anyone acts on it.

### `repositories_tests.rs` (3 tests)

| test | verdict | evidence |
|---|---|---|
| `test_detect_possible_local_git_repo_non_existent_directory` | COVERED-ELSEWHERE | Fork's `test_detect_possible_git_repo_non_existent_directory` (same file) is byte-identical apart from `detect_possible_local_git_repo` → `detect_possible_git_repo` (function renamed, "local" dropped as redundant) and the matching test-name drop. Diffed body-for-body to confirm. |
| `test_detect_possible_local_git_repo_not_a_git_repo` | COVERED-ELSEWHERE | Same — `test_detect_possible_git_repo_not_a_git_repo`. |
| `test_detect_possible_local_git_repo_nested_repo_created_after_parent_registration` | COVERED-ELSEWHERE | Same — `test_detect_possible_git_repo_nested_repo_created_after_parent_registration`. |

This is a straight repeat of the "renamed function, renamed test, same body" trap —
`SCOPE-REST.md`'s file-level "A, 3 absent" verdict for this file is wrong for all 3.

---

## `crates/warp_core/**` (9 measured absent)

### `channel/state_tests.rs` (3 tests: `wss_becomes_https_and_strips_path`, `ws_becomes_http_and_preserves_port`, `unparseable_input_returns_none`)

**CLOUD — already documented, no action needed.** `derive_http_origin_from_ws_url`
(the function under test) does not exist. `channel/state.rs:268` carries an explicit
comment: *"Zap Wave 5-5: `derive_http_origin_from_ws_url` was physically removed together
with `rtc_http_url()`."* — `rtc_http_url` is Warp's realtime-collaboration HTTP origin,
unambiguously cloud. `state_tests.rs` repeats the same note. Verified this removal is
intentional and already recorded, just not in `DECLINED.md` (it lives only in the removal
commit's trail via these two comments) — low-value to add given how self-documenting the
in-code note already is, but flagging for the `DECLINED.md` owner in case they want it
consolidated there too.

### `paths_tests.rs` (6 tests)

| test | verdict | evidence |
|---|---|---|
| `test_gui_app_id_maps_oss_tui_to_oss_gui` | DECLINED | `gui_app_id_for_channel` does not exist. Same "TUI/GUI shared app id" family as the persistence-scope tests above. |
| `test_gui_config_and_mcp_paths_resolve_explicit_sources` | DECLINED | `gui_config_local_dir` does not exist. Same family. |
| `test_tui_mcp_config_path_is_separate_from_gui` | DECLINED | `tui_mcp_config_file_path`/`tui_config_local_dir` do not exist. Same family. |
| `test_tui_state_dir_is_tui_subdir_of_gui_state_base` | DECLINED | `tui_state_dir` does not exist. Same family. |
| `test_project_path_for_warp_app_id` | COVERED-ELSEWHERE | `project_dirs_for_app_id` (the function under test) exists and IS tested — by the fork's own `test_project_path_for_oss_app_id` (`AppId::new("dev", "zap", "Zap")`). The pin's literal `"Warp"`/`AppId::new("dev","warp","Warp")` input would not exercise any code path the fork's own branding tests miss: the function's only special case is `application_name() == "Zap"` / `starts_with("Zap")` (Linux-only dashing), which a `"Warp"` input cannot hit — it would just fall through to the generic `directories::ProjectDirs::from(...)` passthrough, adding no coverage. |
| `test_project_path_for_warp_dev_app_id` | COVERED-ELSEWHERE | Same — fork's `test_project_path_for_zap_dev_app_id` (`AppId::new("dev", "zap", "ZapDev")`) already covers the interesting (`starts_with("Zap")`) branch this input would also hit if renamed to the fork's own branding. |

---

## `crates/remote_server/**` (8 measured absent)

### `client_tests.rs` (4 tests)

All four **PORTABLE — PORTED**, and all four confirm the brief's hint that recent
remote-codebase-search / protocol work unblocked pin tests:

| test | evidence |
|---|---|
| `codebase_index_push_messages_become_client_events` | **Unblocked by the D2 local/BYOP codebase-indexing port.** `CodebaseIndexStatus(esSnapshot)`/`CodebaseIndexStatusUpdated` proto messages, `ClientEvent::CodebaseIndexStatusesSnapshotReceived`/`CodebaseIndexStatusUpdated`, and `crates/remote_server/src/codebase_index_proto.rs`'s domain conversions all now exist and match the pin's shape (verified `codebase_index_proto.rs`'s own doc comment: "Ported from the pin ... unchanged except for `EmbeddingProviderConfig`"). Adapted only for `RemoteServerClient::new`'s 3-tuple return (fork has no separate failure channel) and the fork's domain-type conversion step (`RemoteCodebaseIndexStatus`, same `repo_path` field name). |
| `get_diff_state_round_trips_as_session_scoped` | **Unblocked by issue #509's `SessionScopedRequest`/`HostScopedRequest` envelope split** (confirmed landed — `HANDOFF.md`'s "#509 ported the envelope only" note). `session_scoped_request::Message::GetDiffState`, `client.get_diff_state`, `unwrap_session_scoped` (added, matching the pin's helper) all line up. Adapted: fork's `get_diff_state` takes a `GetDiffState` request struct rather than the pin's separate `(repo_path, mode)` args; `DiffMode` had to be constructed via its actual generated shape (`proto::DiffMode { mode: Some(proto::diff_mode::Mode::Head(DiffModeHead {})) }`) rather than the pin's app-side `encode_diff_mode` helper, which `crates/remote_server` cannot reach (`app` is not a dependency of this crate). |
| `open_buffer_round_trips_as_session_scoped` | Same envelope unblock. Adapted: fork's `OpenBuffer` proto message has no `force_reload` field at all (dropped from the wire, not just the client) — the pin's `assert!(!req.force_reload)` has nothing to assert, so it was dropped rather than invented. |
| `get_diff_state_on_dead_connection_errors_promptly` | Same envelope unblock + the `GetDiffState`-struct adaptation above. |

### `manager_tests.rs` (1 test)

| test | verdict | evidence |
|---|---|---|
| `remote_agent_context_snapshot_is_a_host_scoped_manager_event` | **PORTABLE — PORTED** | `RemoteServerManagerEvent::RemoteAgentContextSnapshot { host_id, snapshot }` and `.session_id()` both exist with matching shapes; the file already has a sibling test (`remote_agent_context_snapshot_revisions_are_deduplicated_per_host`) exercising the same feature. Verbatim port. |

### `setup_tests.rs` (3 tests)

| test | verdict | evidence |
|---|---|---|
| `parse_uname_unsupported_armv8l` | DECLINED (documented in-code, not yet in `DECLINED.md`) | `parse_uname_output` deliberately maps `"armv8l"` to `RemoteArch::Aarch64` rather than rejecting it. **Already has an in-code rationale** at `setup_tests.rs:102-105`: "this fork's `parse_uname_output` deliberately treats `armv8l` as `RemoteArch::Aarch64` (32-bit userland on aarch64 hardware, e.g. Raspberry Pi OS)" — and the fork's own `parse_uname_linux_armv8l` test asserts the opposite of the pin's `parse_uname_unsupported_armv8l`. Meets AGENTS.md §5.10's "code comment must state why" bar but is not recorded in `DECLINED.md`. Worth a second look by whoever owns that file: shipping an aarch64 (64-bit) binary to a host whose `uname -m` reports `armv8l` is only safe if that host can actually execute 64-bit ELF binaries, which is not guaranteed by the `armv8l` string alone — I did not verify this claim further, flagging rather than second-guessing an already-made call. |
| `parse_preinstall_unsupported_glibc_too_old` | DECLINED | `parse_status()` maps `reason=glibc_too_old` to `Supported`, not `Unsupported`, with an explicit, well-reasoned comment: remote-server now ships as a static musl binary with no host-libc dependency, so this legacy libc-gate reason is obsolete but may still arrive from a stale cached preinstall script. **This confirms and updates `DECLINED.md`'s "SSH tmux wrapper" row's own note that its glibc-related justification "is out of date" — the current code is exactly the state that row asked someone to re-derive.** |
| `parse_preinstall_unsupported_non_glibc` | DECLINED | Same `parse_status()` change, `reason=non_glibc` arm. |

---

## `crates/managed_secrets/**` (6 measured absent — `gcp_tests.rs`)

**CLOUD, matches `SCOPE-REST.md`'s existing verdict, re-confirmed.**
`crates/managed_secrets/src/gcp.rs` does not exist in the fork at all (confirmed by
`find`). Per this task's brief, `managed_secrets` is wired to `DisabledManagedSecretsClient`
(`app/src/lib.rs:1543`); GCP Workload Identity Federation credentials specifically serve
cloud ambient agents (`TaskIdentityToken` — GCP-issued task identity tokens for agents
running on Warp's infrastructure), which this fork does not have a local/BYOP equivalent
for. All 6 tests (`basic_config_shape`, `rejects_binary_path_with_spaces`,
`rejects_task_id_with_spaces`, `service_account_impersonation`,
`config_file_path_matches_application_credentials_env_var`,
`no_duration_flag_when_lifetime_absent`) are out of scope together.

---

## `crates/warpui_core/**` (5 measured absent)

| test | verdict | evidence |
|---|---|---|
| `app_focus_telemetry_tests.rs::test_daily_app_focus_duration_increase` | MISSING-SUBSYSTEM | `crates/warpui_core/src/app_focus_telemetry.rs` does not exist (confirmed absent, `find` returns nothing). Non-cloud (per-user focus-duration accounting) but the source was never ported. |
| `telemetry/event_store_tests.rs` (4 tests: `test_initialize_session`, `test_event_queue_empty`, `test_app_active_after_inactivity`, `test_app_active_after_activity`) | MISSING-SUBSYSTEM | `crates/warpui_core/src/telemetry/event_store.rs` does not exist (confirmed absent). Non-cloud (in-memory bounded ring buffer, per `SCOPE-REST.md`'s prior evidence: imports only `bounded_vec_deque::BoundedVecDeque` and `crate::time::get_current_time`) but never ported. Not attempted — building a new module from scratch is feature work, not test porting, and out of this task's scope. |

---

## `crates/warp_multi_agent_client/**` (6 measured absent) and `crates/graphql/**` (3 measured absent)

**CLOUD — the crates do not exist in the fork's workspace at all.** Confirmed via
`ls crates/` — neither `graphql` nor `warp_multi_agent_client` is present (unlike the
other 8 areas, which all ship *some* fork source). Per this task's brief, these are
"much more likely to be genuinely cloud — check their dependencies rather than their
names," and `SCOPE-REST.md`'s prior per-file verdicts already say so (`crates/graphql/src/api/ai_tests.rs`:
"Warp cloud GraphQL schema crate"; `crates/warp_multi_agent_client/src/lib_tests.rs`:
"Cloud MAA (multi-agent API) client... `use warp_server_client::base_client::{AmbientHeaderPolicy, BaseClient}`").
Re-confirmed rather than re-derived from scratch, since the crate's total absence is
dispositive on its own.

---

## `crates/languages/**` (2 measured absent — `lib_tests.rs`)

| test | verdict | evidence |
|---|---|---|
| `local_html_extensions_resolve_to_html` | COVERED-ELSEWHERE | Fork's `html_extensions_resolve_to_html` (same file) is byte-identical apart from `language_by_local_filename` → `language_by_filename` (renamed, "local" dropped) and the matching test-name drop. Diffed body-for-body. |
| `local_command_extension_resolves_to_shell` | COVERED-ELSEWHERE | Same — fork's `command_extension_resolves_to_shell`. |

Third instance of the same renamed-function/renamed-test trap this sweep hit in
`crates/repo_metadata`.

---

## What was ported (14 tests, across 6 files)

1. `app/src/remote_server/diff_state_proto_tests.rs`: `diff_size_round_trips_through_proto`
   (+ fixed `diff_size_round_trips_and_maps_file_too_large`'s now-stale lossy-behavior
   assertion, as part of the `UnrenderableReason` defect fix).
2. `app/src/remote_server/server_model_tests.rs`:
   `remote_agent_context_snapshot_broadcasts_replacements_and_initializes_once`.
3. `app/src/persistence/sqlite_tests.rs`: `sqlite_read_restores_app_state_and_codebase_metadata`,
   `sqlite_writer_reuses_codebase_index_metadata_events`.
4. `crates/repo_metadata/src/local_model_test.rs`: `index_lazy_loaded_path_tracks_only_root`,
   `recursive_repo_uses_recursive_watch_mode`, `remove_repository_clears_extra_dir_watches`,
   `remove_lazy_loaded_path_clears_tracked_watches`, `deleted_subdir_drops_its_tracked_watch`.
5. `crates/remote_server/src/client_tests.rs`: `codebase_index_push_messages_become_client_events`,
   `get_diff_state_round_trips_as_session_scoped`, `open_buffer_round_trips_as_session_scoped`,
   `get_diff_state_on_dead_connection_errors_promptly`.
6. `crates/remote_server/src/manager_tests.rs`:
   `remote_agent_context_snapshot_is_a_host_scoped_manager_event`.

## Code defects found

1. **`DiffSize::Unrenderable` lost its reason payload** (`app/src/code_review/diff_size_limits.rs`,
   `app/src/remote_server/diff_state_proto.rs`, `app/src/code_review/code_review_view.rs`).
   Real user-facing bug: a remote/SSH diff whose base content was too large to transmit
   showed "Diff is too large to render" instead of "File is too large to render." **Fixed**
   in this PR — restored `UnrenderableReason` (both variants; the wire proto already
   supported both), fixed all 3 call sites (enumerated by repo-wide grep before touching
   the enum shape), and fixed the fork test whose comment had documented the lossy
   behavior as accepted rather than justifying it.
2. **Stale doc comment**, `app/src/persistence/mod.rs`'s `ModelEvent::UpsertCodebaseIndexMetadata`:
   said "this fork has no codebase indexing," which predates the D2 local/BYOP embedding
   index (`app/src/remote_server/codebase_index_model.rs`, 1129 lines). Fixed the comment
   to clarify the `workspace_metadata` table this variant writes is unrelated
   "recently-navigated-workspace" bookkeeping, distinct from the (now-real) embedding
   index's own storage.

## Did the recent remote-codebase-search work unblock pin tests?

**Yes, substantially — 5 of the 14 ported tests and 1 not-yet-ported-but-newly-portable
test file are direct consequences of two separate landings:**

- **D2 (local/BYOP codebase indexing)** unblocked `codebase_index_push_messages_become_client_events`
  (`crates/remote_server/src/client_tests.rs`) outright — the whole
  `CodebaseIndexStatus(es)*` proto/event surface it needs now exists and matches the pin.
  It also means `app/src/remote_server/codebase_index_model_tests.rs` and
  `codebase_index_status_tests.rs` — previously `SCOPE-REST.md`'s C (cloud) verdict at
  46 combined tests — are **now 39/39 and 7/7 present** respectively, a verdict flip from
  cloud to fully covered that this sweep re-confirmed but did not need to act on further.
- **Issue #509's `SessionScopedRequest`/`HostScopedRequest` envelope restructuring**
  unblocked all 3 of `get_diff_state_round_trips_as_session_scoped`,
  `open_buffer_round_trips_as_session_scoped`, and
  `get_diff_state_on_dead_connection_errors_promptly`.

Neither landing touched the `RemoteDiffStateManager` gap (still `#324`, still 18 tests
absent) or the `remote_server_daemon`/TUI persistence-scope gap — those remain real,
separately-tracked debt.

## Proposed `DECLINED.md` text (not applied — another agent owns that file)

**Addendum to the existing "TUI/GUI shared app id" row:**

> Also covers `app/src/persistence/sqlite_tests.rs`'s `app_scope_database_path_matches_app_database_path`,
> `tui_scope_database_path_is_tui_subdirectory_of_app_database_dir`,
> `database_path_for_current_scope_defaults_to_app_scope`,
> `remote_server_daemon_scope_database_path_uses_identity_data_dir`,
> `remote_server_daemon_scope_database_path_handles_empty_identity_key`,
> `remote_server_daemon_database_permissions_are_owner_only`,
> `tui_database_in_tui_subdirectory_round_trips_data`, and
> `crates/warp_core/src/paths_tests.rs`'s `test_gui_app_id_maps_oss_tui_to_oss_gui`,
> `test_gui_config_and_mcp_paths_resolve_explicit_sources`,
> `test_tui_mcp_config_path_is_separate_from_gui`,
> `test_tui_state_dir_is_tui_subdir_of_gui_state_base` — the pin's `PersistenceScope`
> (`App`/`Tui`/`RemoteServerDaemon`) and `PersistedDataScope` do not exist in the fork at
> all; there is exactly one shared, unscoped SQLite database. Corrects the row's "two pin
> tests" undercount to ten. **Flagging, not deciding:** `remote_server_daemon_scope_*`
> is a security-relevant sub-case (per-identity `0600` database isolation for the SSH
> daemon), not purely a GUI/TUI cosmetic split — worth the owner's explicit judgment on
> whether it deserves separate tracking from the GUI/TUI half.

**New row, "SSH remote-server host detection — `armv8l` treated as supported":**

> `crates/remote_server/src/setup.rs`'s `parse_uname_output` maps `"armv8l"` to
> `RemoteArch::Aarch64` rather than rejecting it as the pin does, reasoning (in-code,
> `setup_tests.rs:102-105`) that `armv8l` is commonly a 32-bit userland report on
> 64-bit-capable ARM hardware (e.g. 32-bit Raspberry Pi OS). Pin's
> `parse_uname_unsupported_armv8l` is permanently not ported; fork's
> `parse_uname_linux_armv8l` is the authority. **Not independently verified by this
> sweep** that shipping a 64-bit binary to every host reporting `armv8l` is safe —
> recorded as already-decided-in-code, flagged for a second look.

**Correction to the "SSH tmux wrapper — kept, deprecation not ported" row's stale
technical note:** that row already says its glibc-based justification "is out of date...
re-derive the real fallback population before relying on it." This sweep did: as of the
current tree, `parse_status()` maps both `reason=glibc_too_old` and `reason=non_glibc` to
`PreinstallStatus::Supported` (remote-server ships as a static musl binary now, so no host
libc dependency exists to gate on) — meaning `parse_preinstall_unsupported_glibc_too_old`
and `parse_preinstall_unsupported_non_glibc` are permanently not portable, not just
currently-unmeasured. Worth folding this confirmation into that row directly.

**New row, untracked (needs an actual maintainer decision, not just documentation) —
"remote-server daemon auth token is not cleared by an empty Initialize/Authenticate":**

> `app/src/remote_server/server_model.rs`'s `auth_token: Option<String>` is, per its own
> doc comment, "intentionally retained across proxy connection teardown and cleared only
> by daemon process exit" — an empty token on `Initialize` or `Authenticate` is a no-op,
> not a clear. The pin's `empty_initialize_clears_auth_context` and
> `empty_authenticate_clears_auth_token` assert the opposite (empty token = sign-out
> signal, credential cleared). This is plausible as deliberate hardening for a
> long-lived daemon credential, but it is **not recorded as a decision anywhere** — the
> comment states *what* the fork does, not that a person decided it should diverge from
> the pin. Needs actual maintainer sign-off per AGENTS.md §5.10, not just a documentation
> pass.

---

## Ranked: least sure this compiles

Ordered by risk, most uncertain first. **No `cargo`/`nextest` was run anywhere in this
task** (hard constraint) — `script/precheck --fast` (rustfmt + both boundary guards) is
green on every change, which only confirms syntax and cloud-boundary/stub-coverage
compliance, not type-correctness or borrow-checking.

1. **`crates/remote_server/src/client_tests.rs`'s three `SessionScopedRequest`-envelope
   ports** (`get_diff_state_round_trips_as_session_scoped`,
   `open_buffer_round_trips_as_session_scoped`,
   `get_diff_state_on_dead_connection_errors_promptly`). The riskiest single line is the
   hand-constructed `proto::DiffMode { mode: Some(proto::diff_mode::Mode::Head(DiffModeHead {})) }`
   — I inferred the generated submodule/variant names (`diff_mode::Mode::Head`) from
   prost-build's standard naming convention, corroborated by four other oneofs already
   used elsewhere in this same file (`session_scoped_request::Message`,
   `host_scoped_request::Message`, `git_push_response::Result`,
   `git_create_pr_response::Result`), but never saw the actual generated code.
2. **`crates/remote_server/src/client_tests.rs`'s `codebase_index_push_messages_become_client_events`**
   — depends on `RemoteCodebaseIndexStatus`'s conversion functions not silently dropping
   a `NotEnabled`-state status from the snapshot Vec (traced through
   `codebase_index_proto.rs`'s `proto_to_state` match, which does handle `NotEnabled`
   explicitly, but the full D2 module is 1129 lines and I read only the conversion path).
3. **The `UnrenderableReason` restoration** (3 files). Confident on exhaustiveness (grepped
   every `DiffSize::` construction/match site in the repo before and after), less confident
   that `code_review_view.rs`'s surrounding function still borrow-checks cleanly around the
   changed `if let` — I read ~15 lines of context, not the whole function.
4. **`crates/repo_metadata/src/local_model_test.rs`'s 5 ports** — lowest risk of the
   batch (every helper/field verified present with matching signatures by direct grep, and
   the file already carries 48 passing pin-derived tests in the identical style), but it's
   also the largest single diff (282 lines) or on this ranking, so a stray borrow or
   lifetime issue in the closures is the most likely single point of failure by sheer
   surface area.
5. **`app/src/persistence/sqlite_tests.rs`'s two ports** — `ai::workspace::WorkspaceMetadata`'s
   `Clone`/field set was verified directly, `start_writer`/`WriterHandles` verified by
   reading the struct def, but this is the first test in the file to use `start_writer`
   directly rather than going through a higher-level fixture, so an unnoticed setup
   requirement (e.g. an feature flag needed for the writer thread) is possible.
6. **`app/src/remote_server/server_model_tests.rs`'s port** and
   **`crates/remote_server/src/manager_tests.rs`'s port** — lowest risk: both are near-mechanical
   copies against symbols verified present with matching signatures, in files whose
   existing sibling tests use the identical pattern one function away.
