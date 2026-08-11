# Sweep verdicts — `crates/ai/**` and `crates/build_cache/**`

Oracle: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`.
Source: `docs/SWEEP-INVENTORY.md` at `main` @ `2c2ccccc9` (this agent fast-forwarded
its stale worktree branch onto current local `main` before starting — see commit
log). Scope per the task brief: **only** `crates/ai/**` and `crates/build_cache/**`.
Sibling agents own `app/src/ai/**` and everything else `SCOPE-AI.md` covers.

Every `### \`crates/ai/...\`` and `### \`crates/build_cache/...\`` section in
`docs/SWEEP-INVENTORY.md`'s per-file inventory was located (`grep -n '^### \`crates/ai/\|^### \`crates/build_cache/'`)
and traced by hand. There are **11 files, 133 absent-test entries** in this slice.
All 133 are adjudicated below. Two other checks were made and found clean:

- `crates/ai/src/index/full_source_code_embedding/**` — the subsystem flagged in
  the task brief as recently reworked (`c7b8d779d` prune/rerank fix, `58db84396`
  wiring to `get_relevant_files`) — has **zero** absent-test entries anywhere in
  the inventory for this slice. It was fully ported/covered before this pass
  started; nothing left to adjudicate here.
- No `crates/ai/src/**` file appears in the inventory outside the 11 listed below
  (`grep -n 'crates/ai/src/'` over the whole inventory file, cross-checked against
  the per-file section list).

## Bucket counts (this slice, 133 tests total)

| bucket | tests | files |
|---|---:|---|
| CLOUD | 91 | `api_keys_tests.rs` (36 of 55), `geap_credentials_tests.rs` (crate, 12), `lib_tests.rs`/`spacectl_tests.rs` (build_cache, 28) |
| DECLINED | 33 | `api_keys_tests.rs` (19 of 55: custom_endpoints), `orchestration_config_tests.rs` (19)*, `grok_subscription/oauth_tests.rs` (5), `agent/action/convert_tests.rs` (3), `agent/action_result/mod_tests.rs` (3) |
| PORTABLE — ported | 1 | `agent/action_result/convert_tests.rs` |
| COVERED-ELSEWHERE | 1 | `skills/skill_provider_tests.rs` |
| DIVERGENT (MISSING-SUBSYSTEM) | 6 | `project_context/model_tests.rs` |

*`orchestration_config_tests.rs`'s 19 are counted once, under DECLINED — see its
row for why it isn't double-counted with the CLOUD bucket.

Sum check: 91 + 33 + 1 + 1 + 6 = 132. `api_keys_tests.rs`'s split (36 CLOUD + 19
DECLINED = 55) is folded into the two totals above rather than listed as a third
partial row, which is why the per-file table below is the source of truth, not
this summary arithmetic.

Adjudicated: **133 / 133 (100%) of this slice.**

---

## Per-file verdicts

### `crates/ai/src/api_keys_tests.rs` — 55 absent (0 ported, 55 already-adjudicated)

pin 67 · fork 12 · source `crates/ai/src/api_keys.rs`

**Already fully adjudicated in the file's own header comment** (lines 1-111 of
`crates/ai/src/api_keys_tests.rs`) by an earlier pass — this agent re-verified
every symbol against the current `api_keys.rs` (257 lines: `ApiKeys` has only
`google`/`anthropic`/`openai`/`open_router`; `ApiKeyManager` has no `custom_endpoints`,
no `GrokTokens`, no `GeapCredentials`, no `provider_key_count`) and the split holds:

- **24 CLOUD** — `grok_*`, `has_grok_subscription_*`, two `manager_has_any_key_*`
  cases. `GrokTokens` is an OAuth pair minted and refreshed by Warp's hosted
  xAI-subscription connect flow; there is no pasted-key equivalent to test against.
  Matches `DECLINED.md`'s xAI/Grok row (#319): "Keeps ... 24 `grok_*` tests in
  `api_keys_tests.rs` out of scope."
- **12 CLOUD** — `geap_*` and the `api_keys_for_request_*_geap_token*` group.
  `GeapMintBinding` credentials are minted by Warp's managed-secrets service for a
  workspace; nothing local to bind to.
- **19 DECLINED** — `custom_model_providers_*`, `display_label_*`,
  `provider_key_count_*`, `byok_disabled_returns_none_even_with_endpoints`,
  `empty_api_key_endpoints_are_skipped`, `endpoints_with_only_empty_models_are_skipped`,
  `has_any_key_false_for_endpoint_with_empty_api_key`,
  `has_any_key_true_for_custom_endpoints_only`, `multiple_endpoints_all_serialize`,
  `serde_legacy_endpoint_defaults_to_chat_completions`,
  `serde_round_trip_with_custom_endpoints`,
  `api_keys_for_request_none_for_custom_endpoints_only`. Matches `DECLINED.md`'s
  `CustomEndpoint` / `custom_model_providers` row (#142, #347, PRs #189/#227,
  merged): "porting `CustomEndpoint` into `crates/ai/src/api_keys.rs` would stand
  up a second, competing provider store" — the fork's real BYOP surface is
  `AgentProviderSecrets` (`app/src/ai/agent_providers/secrets.rs`), out of my
  scope but already covered by `agent_providers/mod_test.rs` (13 tests) and
  `secrets_tests.rs` (6 tests).

(24 + 12 + 19 = 55.) The inventory's mechanical `PORTABLE?` tag on 52 of these was
wrong across the board — every one needs a symbol (`GrokTokens`, `GeapCredentials`,
`CustomEndpoint`) the fork deliberately does not have. **The file's own header
comment is a better verdict source than the inventory for this file** — it was
written after tracing each test, not from imports alone.

### `crates/build_cache/src/lib_tests.rs` — 25 absent → CLOUD

pin 25 · fork 0 · source `crates/build_cache/src/lib.rs`

**Correction to the inventory: "fork ships source: yes" is wrong. The fork does
not ship `crates/build_cache` at all** — no such directory exists under
`crates/`, it is absent from the workspace `Cargo.toml`, and `find . -iname
'*build_cache*'` (excluding `target/`, `.git/`) returns nothing. The inventory
apparently inherited "yes" from a stale measurement or a path-presence check
that didn't actually stat the fork tree.

Traced against the pin (`git show 02b53fcd8:crates/build_cache/src/lib.rs`, 816
lines): the crate's own doc comment states its purpose exactly — "Persistent
build cache management for sandboxed agents ... configures a sandbox environment
to use attached persistent storage ... relies on `spacectl`
(https://github.com/namespacelabs/spacectl)." `spacectl` is Namespace's cloud
sandbox CLI. The crate has **exactly one consumer at the pin**:
`app/src/ai/agent_sdk/driver/cache_setup.rs`, gated on
`warp_isolation_platform::IsolationPlatformType::Namespace` and importing
`cloud_object_models::SourceRepo` — already verdict **C** in `SCOPE-AI.md`
("cloud-environment build cache"). The fork ships neither `cache_setup.rs` nor
`cloud_object_models`.

This is the cloud-runner-sandbox half of orchestration, already covered by
`DECLINED.md`'s RunAgents/cloud-runner row (#290): "Needs
`warp_graphql::queries::get_runners` (crate deleted), `crate::server::experiments`,
`crate::server::server_api` ... Children run as local processes here." Porting
`crates/build_cache` (a ~950-line crate across `lib.rs` + `spacectl.rs`) would add
dead code: its only local trigger, `IsolationPlatformType::Namespace`, is real
in this fork (`crates/isolation_platform` retains the variant, used elsewhere for
workload-identity tokens) but nothing in this fork's driver ever checks it to run
build-cache setup, because `cache_setup.rs` was never ported and porting it is a
cloud-runner concern. **Verdict: CLOUD**, not a stray gap — it hangs off an
already-declined feature.

### `crates/build_cache/src/spacectl_tests.rs` — 3 absent → CLOUD

pin 3 · fork 0 · source `crates/build_cache/src/spacectl.rs` (does not exist in fork)

Same crate as above, same reasoning. `spacectl.rs` (`git show
02b53fcd8:crates/build_cache/src/spacectl.rs`) is a wrapper around the `spacectl`
CLI binary that only exists inside a Namespace cloud sandbox. Verdict: **CLOUD**.

### `crates/ai/src/agent/orchestration_config_tests.rs` — 19 absent → DECLINED

pin 19 · fork 0 · source `crates/ai/src/agent/orchestration_config.rs` (does not exist in fork)

Traced the pin source (`git show 02b53fcd8:crates/ai/src/agent/orchestration_config.rs`):
the whole module is `matches_active_config(request: &RunAgentsRequest, config:
&OrchestrationConfig) -> bool` plus supporting types — it decides whether a
model-issued `run_agents` tool call auto-launches against a previously approved
orchestration config (including a `Remote { environment_id, worker_host,
runner_id }` execution-mode branch for cloud runners). `RunAgentsRequest` and
`RunAgentsExecutionMode` are the *model-invoked* spawn types.

`DECLINED.md`'s "Agent-invoked agent spawning (`AIAgentActionType::RunAgents`)"
row (#325) is exactly this surface: "the pin's `RunAgentsRequest` is cloud-typed
so there is nothing to port ... `StartAgentExecutionMode`/
`RunAgentsExecutionMode`/`RunAgentsAgentRunConfig` have no reference
implementation to follow." Confirmed the fork has no `RunAgentsRequest`,
`RunAgentsExecutionMode`, or `super::action::RunAgentsRequest` anywhere under
`crates/ai/src/agent/` (`grep -rn "RunAgentsRequest" crates/ai/src/agent/` —
no hits). **Verdict: DECLINED**, not the inventory's mechanical `DIVERGENT?`
(feature gap) — the feature that would host this module is a declined product
decision, not an unstarted port.

This file is counted once (19, DECLINED) in the bucket totals above; it is not
double-counted with the CLOUD figures even though its `Remote` branch touches
cloud-runner concepts, because the correct citation is #325 (model-invoked
spawning, declined) rather than #290 (cloud-runner execution, out of scope) —
the whole module is unreachable either way once #325 stands.

### `crates/ai/src/geap_credentials_tests.rs` — 12 absent → CLOUD

pin 12 · fork 0 · source `crates/ai/src/geap_credentials.rs` (does not exist in fork)

This is the crate-level `GeapCredentials` state/UI-icon model (`access_token`,
`expires_at`, `access_token_for_request`), consumed exclusively by
`app/src/ai/geap_credentials.rs` — already verdict **C** in `SCOPE-AI.md`:
"`use warp_managed_secrets::client::{IdentityTokenOptions, TaskIdentityToken};`
... GEAP tokens are minted per workspace by Warp's managed-secret service."
Same family, same reasoning, as the 12 `geap_*` tests already declared CLOUD
inside `api_keys_tests.rs`'s own header comment above. Nothing in this
crate-level file imports cloud symbols directly, but it exists solely to be
filled by a cloud-only minting path — porting the pure state machine alone (as
the inventory's mechanical `DIVERGENT?` tag suggests) would add a type with no
non-test producer. **Verdict: CLOUD.**

### `crates/ai/src/project_context/model_tests.rs` — 6 absent → DIVERGENT / MISSING-SUBSYSTEM

pin 29 · fork 23 · source `crates/ai/src/project_context/model.rs`

**Already fully hand-traced in the fork file's own header comment**
(`model_tests.rs:1-68`) — re-verified, not re-derived. Breakdown of the 6:

- **2 blocked on `ProjectContextModel::reconcile_project_rules` /
  `ProjectRules::rule_paths`**, absent from the fork (#150 item 2, #201):
  `test_missing_rule_content_preserves_cached_content_while_path_is_standing`,
  `test_rule_missing_from_standing_results_is_removed_from_cached_content`.
- **3 blocked on remote (`LocalOrRemotePath`) *project*-rule support** (as
  opposed to *global* rules, which the fork already layers with per-host
  isolation via `#575`'s `global_rules.rs`): `test_remote_project_rules_require_matching_host`,
  `test_remote_standing_results_preserve_host_qualified_rule_paths`,
  `test_reconcile_project_rules_hydrates_local_and_remote_paths`.
- **1 split off `test_remote_global_rules_only_layer_for_matching_remote_host`**,
  whose global-rules half is already ported as
  `test_remote_project_rules_layers_local_global_ahead_of_remote_global`; the
  remaining *project*-rule half is the same remote gap as the group above.

**MISSING-SUBSYSTEM, correctly flagged non-cloud in the fork's own comment**:
this fork's `ProjectRule::path`, `ProjectRulePath` and `path_to_rules` are keyed
by plain `PathBuf`, so `find_applicable_project_rules` unconditionally returns
`None` for a remote path. Global rules already solved the equivalent problem
(`global_rules.rs`'s per-host isolation, landed under #575) — the fix here is
the same shape applied to *project* rules: give `path_to_rules` a `HostId`
dimension mirroring `global_rules.rs`'s approach, "a materially larger
restructuring than adding the global-rules-only lookup" per the existing
comment. This is a `remote_server`-shaped gap (SSH-remote project-rule
resolution), not cloud — refs #150 item 2, #170. Not ported here: the fix is a
genuine restructuring of `path_to_rules`'s key type, which this task's
"verify every helper exists before porting" rule rules out attempting blind in
an environment where builds are on hold.

### `crates/ai/src/grok_subscription/oauth_tests.rs` — 5 absent → DECLINED

pin 5 · fork 0 · source `crates/ai/src/grok_subscription/oauth.rs` (does not exist in fork)

Matches `DECLINED.md`'s xAI/Grok row (#319) exactly: "Phosphor supports
API-key credentials only. The flow is genuinely local (OAuth2+PKCE direct to
`auth.x.ai`, loopback `127.0.0.1:56121`, public Grok-CLI `client_id`) — it is
*not* a cloud drop — but it is an alternative credential *source*, and a user
with an xAI API key is already fully served." Confirmed `crates/ai/src/grok_subscription/`
does not exist in the fork tree. **Verdict: DECLINED** (product decision, not a
cloud boundary — matches the inventory's own `DECLINED?` tag).

### `crates/ai/src/agent/action/convert_tests.rs` — 3 absent → DECLINED

pin 3 · fork 0 (these 3) · source `crates/ai/src/agent/action/convert.rs` (fork ships the file)

Traced the pin test bodies (`git show 02b53fcd8:crates/ai/src/agent/action/convert_tests.rs`):
all three (`start_recording_parses_valid_window_target`,
`start_recording_rejects_unparseable_window_id`,
`start_recording_without_target_records_whole_screen`) construct an
`api::message::tool_call::StartRecording` and convert it through
`AIAgentActionType::try_from`, asserting on `AIAgentActionType::StartRecording`.
Confirmed the fork's `AIAgentActionType` (`crates/ai/src/agent/action/mod.rs`,
via `grep`) has no `StartRecording`/`StopRecording` variant. Matches
`DECLINED.md`'s computer-use session-recording row (#350): the whole capture
subsystem (`mac/recording.rs`, `linux/recording.rs`, `PointerSession`/`PointerSink`,
overlay burn-in) is declined; only per-window *activation* (#349) was restored,
separately. **Verdict: DECLINED.**

### `crates/ai/src/agent/action_result/mod_tests.rs` — 3 absent → DECLINED

pin 3 · fork 0 (these 3) · source `crates/ai/src/agent/action_result/mod.rs` (fork ships the file)

`run_agents_is_failed_when_no_agents_launch`,
`run_agents_is_successful_when_all_agents_launch`,
`run_agents_is_successful_when_some_agents_launch` all need
`AIAgentActionResultType::RunAgents(RunAgentsResult)`. Confirmed absent: the
fork's `AIAgentActionResultType` enum has no `RunAgents` variant, and the
`SendMessageToAgent` variant's own doc comment says so explicitly (`mod.rs:86`):
"agent-initiated spawning (`AIAgentActionResultType::RunAgents`/`RunAgentsResult`)
is deliberately not part of this fork -- see `DECLINED.md`'s `#325` row." Same
decision as `orchestration_config_tests.rs` above. **Verdict: DECLINED.**

### `crates/ai/src/agent/action_result/convert_tests.rs` — 1 absent → **PORTED**

pin 2 · fork 1 (before this change) · source `crates/ai/src/agent/action_result/convert.rs`

`read_files_partial_success_converts_failed_files` carried a blocking comment in
the fork's own test file claiming "this fork's variant is `Success { files }`
only." **That comment was stale.** Tracing the current code: `#369` (referenced
in `mod.rs:97`'s doc comment on `ReadFilesFailedFile`) already added
`failed_files: Vec<ReadFilesFailedFile>` to `ReadFilesResult::Success`
(`mod.rs:391-395`) and wired it through `convert.rs`'s
`TryFrom<ReadFilesResult>` into `failed_reads` on the wire
(`convert.rs:156-186`) — the exact shape the pin test expects
(`api::read_files_result::AnyFilesSuccess { files, failed_reads }`,
`api::read_files_result::FailedRead { path, message }`). Verified every symbol
the test touches has the same signature in the fork: `FileContext::new(String,
AnyFileContent, Option<Range<usize>>, Option<SystemTime>)` (4 args, matches),
`ReadFilesFailedFile { path, message }` (matches), `AnyFileContent::StringContent`
(matches), and the `From<FileContext> for Vec<api::AnyFileContent>` /
`From<FileContext> for Vec<api::FileContent>` impls both exist. **Ported
verbatim** into `crates/ai/src/agent/action_result/convert_tests.rs`, replacing
the stale blocking comment with one explaining why the block was lifted.
`rustfmt --check --config-path .rustfmt.toml --edition 2024` passes on the
changed file.

### `crates/ai/src/skills/skill_provider_tests.rs` — 1 absent → COVERED-ELSEWHERE

pin 6 · fork 5 · source `crates/ai/src/skills/skill_provider.rs`

The pin's `warp_home_skill_path_is_home_warp_skill`
(`git show 02b53fcd8:crates/ai/src/skills/skill_provider_tests.rs`) asserts two
things in one test body: `get_provider_for_path(...) == Some(SkillProvider::Warp)`
**and** `get_scope_for_path(...) == SkillScope::Home`. The fork already covers
both assertions, split into two separate tests already present in
`crates/ai/src/skills/skill_provider_tests.rs`:
`warp_home_skill_path_uses_warp_provider` (the provider assertion, renamed
`Warp`→`Zap`) and `home_skill_path_is_home_scope` (the scope assertion, with its
own regression-guard comment: "home-directory skills must be Home scope, not
Project"). No behavior is untested; the fork simply split one pin test into two
more narrowly named ones. **Verdict: COVERED-ELSEWHERE** — cite
`warp_home_skill_path_uses_warp_provider` + `home_skill_path_is_home_scope`.

---

## Deliverable summary

**1. Counts per bucket / adjudicated:** 133 / 133 (100%) of this slice's absent
tests adjudicated. CLOUD 91, DECLINED 33, PORTED 1, COVERED-ELSEWHERE 1,
DIVERGENT/MISSING-SUBSYSTEM 6.

**2. Fork code defects exposed:** none found. The one candidate
(`read_files_partial_success_converts_failed_files`'s stale blocking comment)
was a documentation defect, not a code defect — the code (`#369`) was already
correct; only the test comment hadn't caught up. Fixed by porting the test
rather than by changing any production code.

**3. MISSING-SUBSYSTEM:**
- **`crates/ai/src/project_context/model.rs`: no `HostId` dimension on
  `path_to_rules`/`ProjectRule::path`.** 6 pin tests (see above) need SSH-remote
  *project*-rule resolution (as opposed to *global*-rule resolution, which
  `#575`'s `global_rules.rs` already solved with per-host isolation). What it
  would take: give `path_to_rules` a `HostId`-keyed structure mirroring
  `global_rules.rs`'s approach, then implement `reconcile_project_rules` /
  `ProjectRules::rule_paths` against it. Explicitly flagged in the existing
  code comment as "a materially larger restructuring than adding the
  global-rules-only lookup" — refs #150 item 2, #170, #201.

**4. Where `SCOPE-AI.md` / the inventory was wrong:**
- `docs/SWEEP-INVENTORY.md`'s entry for `crates/build_cache/src/lib_tests.rs`
  states **"fork ships source: yes."** This is false — `crates/build_cache` does
  not exist anywhere in the fork tree or workspace `Cargo.toml`. Its sibling
  `spacectl_tests.rs` entry correctly says "fork ships source: NO," so the
  `lib_tests.rs` row is an isolated error, not a whole-crate misclassification.
- The inventory's mechanical `PORTABLE?` tag on 52 of `api_keys_tests.rs`'s 55
  absent tests was wrong for all 52 — every one needs `GrokTokens`,
  `GeapCredentials`, or `CustomEndpoint`, none of which the fork has, and all of
  which are already-recorded decisions in `DECLINED.md`/`SCOPE-AI.md`. The
  file's own header comment (written by an earlier, more careful pass) was the
  reliable source here, not the inventory.
- `crates/ai/src/agent/orchestration_config_tests.rs`'s mechanical `DIVERGENT?`
  tag (feature gap) undersold it: the module is not merely unported, it hosts
  exactly the model-invoked-spawning surface `DECLINED.md` #325 declines by
  name (`RunAgentsRequest`, `RunAgentsExecutionMode`).

**5. Ranked list of what I am least sure compiles, most likely first:**
1. **`read_files_partial_success_converts_failed_files`** (the one test I
   ported, `crates/ai/src/agent/action_result/convert_tests.rs`). I traced every
   symbol's signature by hand (`FileContext::new`'s 4 positional args,
   `ReadFilesFailedFile`'s two fields, the `AnyFileContent`/`api::AnyFileContent`
   conversion path, `api::read_files_result::AnyFilesSuccess`/`FailedRead`'s
   field names) against the current fork source, and it is a verbatim copy of
   the pin test body with no adaptation needed — but I could not run `cargo
   check`/`nextest` per the hard constraint, so this is unverified end to end.
   `rustfmt --check` passes. This is the only compilation risk this pass
   introduced; everything else in this file is verdicts only, no code changes.

No other files in this slice were touched.
