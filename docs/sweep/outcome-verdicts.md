# Sweep verdicts — the "verdict itself is in question" package

Oracle: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. Never `warp/master`.

Scope per the task brief: 8 tests across 4 files where the *verdict* needed
adjudicating before any porting decision, not a straightforward port. Each
was investigated against pin source (`git show 02b53fcd8:<path>`), current
fork source, `DECLINED.md`, `TODO.md`, and fork commit history.

Branch: `sweep/verdict-package-8`, from local `main` @ `17025cd66f`.

## Summary

| test | file | verdict |
|---|---|---|
| `context_window_limit_schema_has_description` | `app/src/ai/execution_profiles/config_tests.rs` | RE-ADJUDICATED → DIVERGENT |
| `file_collection_rejects_invalid_values_as_a_unit` | same | RE-ADJUDICATED → DIVERGENT |
| `file_collection_round_trips_multiple_profiles` | same | RE-ADJUDICATED → DIVERGENT |
| `test_from_conversation_populates_local_conversation_fields` | `app/src/ai/conversation_details_panel_tests.rs` | RE-ADJUDICATED → CLOUD (collateral of a documented cloud-removal commit) |
| `test_from_conversation_metadata_passes_harness_through` | same | RE-ADJUDICATED → CLOUD (same) |
| `sharer_rejects_dcs_hook_with_unregistered_session_id` | `app/src/terminal/model/terminal_model_tests.rs` (fork: `terminal_model_test.rs`) | STILL-BLOCKED on #419 (confirmed NOT landed, despite #532's closure) |
| `viewer_processes_dcs_hook_with_unregistered_session_id` | same | STILL-BLOCKED (same) |
| `prepare_local_harness_child_launch_rejects_disabled_codex_before_shell_validation` | `app/src/pane_group/pane/local_harness_launch_tests.rs` | PORTED (real ordering bug, fixed) |

---

## A. `execution_profiles/config_tests.rs` — DIVERGENT (confirmed)

**The hypothesis in the brief is correct.** `ExecutionProfilesConfig`,
`ExecutionProfileFile`, and `ExecutionProfileId::parse` do not exist anywhere
in the fork tree (`grep -rn "ExecutionProfilesConfig\|ExecutionProfileFile"
app/src crates` — zero hits; `app/src/ai/execution_profiles/config.rs` and
`config_tests.rs` do not exist — `ls app/src/ai/execution_profiles/` shows
only `editor/`, `mod.rs`, `model_menu_items.rs`, `profiles.rs`,
`profiles_tests.rs`).

**Evidence the fork's persistence model is a genuine, load-bearing
architectural choice, not an unbuilt feature:**

- The pin's `config.rs` centers on `ExecutionProfileId(String)` — an
  ASCII-key identity meant to be a **TOML map key** in `settings.toml**, with
  a `from_legacy_server_id(ServerId)` constructor explicitly for **migrating
  profiles down from Warp's old cloud object store** into the new local file.
  `ExecutionProfileFile` is a `schemars`-derived struct whose JSON Schema
  backs a settings-file editor UI.
- The fork's `profiles.rs` instead centers on `ClientProfileId(usize)` — a
  process-local atomic counter, explicitly *not* meant to survive past one
  process — and persists profiles as `AIExecutionProfileObject =
  GenericStoredObject<GenericStringObjectId, AIExecutionProfileObjectModel>`
  (`mod.rs:602-604`), the fork's general **object-store** abstraction
  (`crate::cloud_object::model::{generic_string_model,persistence}`,
  `UpdateManager`, `SyncId`/`ClientId`).
- This object-store abstraction is not specific to execution profiles — it is
  the same mechanism used across dozens of other fork subsystems (agent
  history, blocklist, MCP config, etc. — see
  `script/cloud_boundary_allowlist.txt`'s ~40+ entries for
  `cloud_object::model::{generic_string_model,persistence}`). It is a
  deliberate, repo-wide persistence architecture, allowlisted and actively
  used, not a stub or an unbuilt feature.
- Porting the pin's `ExecutionProfilesConfig`/`ExecutionProfileFile` verbatim
  would stand up a **second, competing persistence mechanism** for data the
  fork already fully manages through `AIExecutionProfileObject` — the same
  shape of problem `DECLINED.md`'s `CustomEndpoint`/`custom_model_providers`
  row (#142, #347) describes for a different subsystem ("would stand up a
  second, competing provider store").

**Verdict: DIVERGENT.** The fork's execution-profile persistence is BYOP's
own object-store model, not Warp's settings.toml file-collection model. Since
`ExecutionProfilesConfig`/`ExecutionProfileFile`/`ExecutionProfileId::parse`
do not exist, none of the 3 pin tests can be ported without first re-adopting
Warp's file-collection design — a product decision, not a test port.

**Ready-to-paste `DECLINED.md` row** (for the "Divergences where the fork
deliberately differs" section):

```
| **Execution-profile persistence — object store, not a settings.toml file collection** | — | **DIVERGENT, confirmed 2026-08-11 (sweep package 8).** The pin persists `AIExecutionProfile`s via `ExecutionProfilesConfig`/`ExecutionProfileFile` (`app/src/ai/execution_profiles/config.rs`) — a `schemars`-derived struct keyed by a TOML-safe `ExecutionProfileId(String)`, with `ExecutionProfileId::from_legacy_server_id` explicitly for migrating profiles down from Warp's old cloud object store. This fork persists the same data as `AIExecutionProfileObject = GenericStoredObject<GenericStringObjectId, AIExecutionProfileObjectModel>` (`app/src/ai/execution_profiles/mod.rs:602-604`), keyed by a process-local `ClientProfileId(usize)` (`profiles.rs:34`) — the fork's general object-store abstraction (`cloud_object::model::{generic_string_model,persistence}`), the same mechanism used across dozens of other subsystems, not specific to this feature. `ExecutionProfilesConfig`, `ExecutionProfileFile`, and `ExecutionProfileId::parse` do not exist here and porting them would stand up a second, competing persistence store for data already fully managed through `AIExecutionProfileObject` — the same shape of problem as the `CustomEndpoint` row above. Permanently unported: `context_window_limit_schema_has_description`, `file_collection_round_trips_multiple_profiles`, `file_collection_rejects_invalid_values_as_a_unit` (`app/src/ai/execution_profiles/config_tests.rs`, pin only). <!-- markers: sym:ExecutionProfilesConfig sym:ExecutionProfileFile keep:AIExecutionProfileObject -->
```

---

## B. `conversation_details_panel_tests.rs` — RE-ADJUDICATED to CLOUD, wasm break: decline, doc-only

**Finding that changes the picture from the brief:** `conversation_details_panel.rs`
and `conversation_details_panel_tests.rs` were not simply "never ported" — they
were **deliberately deleted** by commit `002ce4671`
("feat(cloud-removal): drop agent-management UI + active-views +
task-status-sync", Phase 2c), the *same commit* that deleted
`active_agent_views_model.rs` — the subject of the existing `DECLINED.md`
`ActiveAgentViewsModel` row (#418). That commit's own message (translated):
"BYOP doesn't need cloud task UI / active-view tracking / status sync,"
deleting the whole `agent_management/` directory, `conversation_details_panel.rs`
(2006 lines) + tests (274 lines), `active_agent_views_model.rs` + tests, and
`blocklist/task_status_sync_model.rs` + tests together, and cleaning up 24
caller files — **except** two residual `#[cfg(target_family = "wasm")]`
imports in `app/src/workspace/view.rs:179` and `wasm_view.rs:11` that the
cleanup missed.

**Why this is CLOUD, not MISSING-SUBSYSTEM, even though the two assigned
tests target the "local conversation" code path:**

- At the pin, the file is genuinely mixed: `from_task` (ambient/cloud agent
  task details — imports `AmbientAgentTask`, `CloudObjectLookup`, `ServerId`,
  `SyncId`) sits alongside `from_conversation`/`from_conversation_metadata`
  (local conversation details). The two tests in this package
  (`test_from_conversation_populates_local_conversation_fields`,
  `test_from_conversation_metadata_passes_harness_through`) are indeed on the
  local side of that split.
- But the **only current or historical consumer of this code in the fork is
  the wasm build** — the browser-based viewer for a session **published to
  Warp Drive** (`wasm_view.rs` wires it behind `build_open_in_warp_button`,
  `ZapDriveObjectSettings`, `parse_current_url`/`browser_url_handler` — i.e.
  "Open in Warp" from a page rendering a `warp.dev/...` shared-block URL).
  Phosphor has no cloud backend to publish such a URL to, so this consumer
  has no reachable use in a BYOP deployment.
- There is no native (non-wasm) GUI/TUI caller of `conversation_details_panel`
  anywhere in the fork, before or after the deletion — the entire feature
  (info side panel on a conversation) was bundled with, and removed as part
  of, the cloud agent-management UI at Phase 2c.
- Restoring just the local-only slice for these 2 tests would produce dead
  code with no caller (the `#[cfg(target_family = "wasm")]` sites are
  themselves recommended for removal below), which is exactly the
  "ported but never wired" defect class `HANDOFF.md` flags as a recurring
  cost.

**Verdict: CLOUD** (collateral of the #418-sibling cloud-removal decision),
not MISSING-SUBSYSTEM. This corrects the brief's framing — it is not merely
"never ported," it was actively removed with cloud-management UI, and its
only remaining reference is dead residue in an unreachable-in-BYOP build
target.

### The wasm LATENT BREAK — recommendation: (b), doc-only

Per `TODO.md`'s "LATENT BREAK" entry, the two `use` sites are the only
reason `wasm_view.rs`/`view.rs:179` fail to compile. Two options were framed:
(a) restore the panel and fix wasm, (b) declare wasm unsupported and strip
the dead paths.

**Recommendation: (b), but narrower than "declare wasm unsupported."** The
wasm target is *not* uniformly abandoned — `TODO.md`'s own "Step 6a" entry
records a *different* wasm compile break (an LSP no-op enum arm) being fixed
during unrelated restoration work, meaning engineers still keep the wasm
target's *other* cfg-gated paths compiling as a matter of hygiene even though
nothing in CI builds it. So the right scope for this decision is: the
**conversation-details-panel dependency specifically** is dead residue from a
documented cloud-removal decision and should be declined/stripped, not that
wasm as a whole is out of scope.

Restoring the panel (option a) is not "clearly right": it is 2006+274 lines,
heavily coupled to already-declined cloud surfaces
(`AmbientAgentTask`, `CloudObjectLookup`, `ServerId`/`SyncId`,
`agent_management`), reviving it would resurrect part of the exact UI
`ActiveAgentViewsModel`'s `DECLINED.md` row already covers, and its sole
purpose in this fork build (an "Open in Warp" info panel for a cloud-Drive-
published session) has no BYOP function.

**Per the task's instruction, only the doc half is implemented here** — no
`.rs` files under `app/src/workspace/` were touched. Actually stripping the
two dead imports (and whatever rendering code in `wasm_view.rs` reads
`ConversationDetailsData`) is separable follow-up work, tracked by the
existing `TODO.md` "LATENT BREAK" entry; this doc establishes the verdict
that make that follow-up "strip," not "restore."

**Ready-to-paste `DECLINED.md` addendum** (extends the existing `ActiveAgentViewsModel`
row under "Cloud — out of scope by definition," or stands alone if preferred):

```
| **`conversation_details_panel`** | #418 | **RE-ADJUDICATED 2026-08-11 (sweep package 8): CLOUD, same commit as `ActiveAgentViewsModel` above.** `app/src/ai/conversation_details_panel.rs` (+`_tests.rs`) was deleted by the same `002ce4671` "drop agent-management UI + active-views + task-status-sync" commit that removed `ActiveAgentViewsModel`, alongside the whole `agent_management/` directory and `task_status_sync_model.rs`. The pin file mixes a cloud path (`from_task`, built on `AmbientAgentTask`/`CloudObjectLookup`/`ServerId`/`SyncId`) with a local path (`from_conversation`/`from_conversation_metadata`), but the fork has no native (non-wasm) caller of either, before or after the removal — only two `#[cfg(target_family = "wasm")]` residue imports remain (`app/src/workspace/view.rs:179`, `wasm_view.rs:11`), left behind by the cleanup that removed everything else. Those wasm sites gate the browser viewer for a session **published to Warp Drive** (`build_open_in_warp_button`, `ZapDriveObjectSettings`) — a cloud-publish flow this fork has no backend for — so restoring the panel would revive dead-with-no-caller code to fix a compile break in an unreachable-in-BYOP consumer. Permanently unported: `test_from_conversation_populates_local_conversation_fields`, `test_from_conversation_metadata_passes_harness_through` (pin `conversation_details_panel_tests.rs`), and by extension the rest of that file's tests. The two residual wasm `use` sites should be stripped (not restored-around) as separate follow-up — tracked in `TODO.md`'s "LATENT BREAK" entry. <!-- markers: path:app/src/ai/conversation_details_panel.rs path:app/src/ai/conversation_details_panel_tests.rs -->
```

---

## C. `terminal_model_tests.rs` DCS-hook tests — STILL-BLOCKED (TODO.md #532 closure is wrong)

**The brief's question was whether #419's PTY-spawn wiring landed. It has
NOT**, despite `TODO.md`'s #532 entry claiming otherwise. This is the same
"TODO.md states the opposite of the code" failure class `CLAUDE.md` warns
about (#148) — a new instance, found here.

**Evidence:**

- `should_validate_dcs_hook_session_id` (`app/src/terminal/model/terminal_model.rs:2699`)
  is still hardcoded `false`, with a doc comment that is accurate and
  self-consistent: *"Validation stays off in production for now: nothing yet
  calls `register_session_id` at PTY-spawn time... Until that wiring lands,
  flipping this to `true` would reject every real lifecycle hook."*
- `register_session_id` (`terminal_model.rs:2548`) has exactly **one call
  site in the whole tree**, and it is inside `new_for_test`, gated
  `#[cfg(any(test, feature = "test-util"))]` (`terminal_model.rs:1047`). No
  production code path registers a session ID.
- The pin's PTY-spawn wiring this is blocked on lives in
  `app/src/terminal/local_tty/terminal_manager.rs`, where
  `.register_session_id(generated_session_id)` is called right after the
  shell starter produces a session ID (pin lines 621-629). **The fork's
  version of that same file has zero occurrences of `session_id` at all** —
  confirmed by direct grep, not inferred.
- `TODO.md`'s #532 entry (closed 2026-08-08) reasoned from **symbol
  presence**: `requires_registered_session`, `is_registered_session`, and
  `should_validate_dcs_hook_session_id` exist in
  `terminal_model.rs`/`ansi/{dcs_hooks,mod}.rs`, so it declared "#419 has now
  landed." That is true of the *scaffolding* (the trait methods, the
  `HashSet<SessionId>` field) but not of the *wiring* the scaffolding exists
  to support — the PTY-spawn call that would ever populate the set in a real
  session. Symbol presence without the call site that uses it is exactly the
  "ported but never wired" defect class `HANDOFF.md` names as the highest-
  cost recurring mistake.

**Not a cloud/declined feature either** — `SharedSessionStatus`
(sharer/viewer roles) is a real, extensively wired fork feature
(`app/src/terminal/shared_session/`, `app/src/terminal/view/shared_session/`,
`app/src/terminal/input.rs` keymap contexts), so this is genuine security-
relevant debt (DCS-hook session-ID spoofing across a shared session, the
class of issue `HANDOFF.md` flags under #171), not something to decline.

**Verdict: STILL-BLOCKED.** Porting these two tests, or flipping
`should_validate_dcs_hook_session_id` to `!is_viewer()`, now would regress
every real terminal session (every lifecycle hook — `Preexec`,
`CommandFinished`, etc. — would be rejected, since `registered_session_ids`
is always empty in production). The actual blocker is implementing the
PTY-spawn wiring in `local_tty/terminal_manager.rs` (and platform
equivalents), which is a real, scoped feature port, not something to
attempt inside an 8-test verdict sweep. No code changed for this item.

**Action needed (not performed here, out of scope):** `TODO.md`'s #532 entry
should be corrected — it currently asserts #419 landed and is being treated
as closed, which will mislead the next agent into either porting these tests
red or treating the security gap as resolved. Per this task's constraints,
`TODO.md` was not edited; this finding is recorded here so it is not lost.

---

## D. `local_harness_launch_tests.rs` — PORTED (real ordering bug, fixed)

**Confirmed a real divergence, not a deliberate one**, and fixed it.

- Pin (`local_harness_launch.rs:175-186`): after resolving `harness`, calls
  `local_harness_product_disabled_message(harness)` (a blanket
  product-disabled check for **any** harness) and returns early on it,
  *then* calls `validate_local_harness_shell(shell_type)?`.
- Fork (`local_harness_launch.rs`, before this fix): called
  `validate_local_harness_shell(shell_type)?` first, unconditionally for
  every harness, and only checked Codex's disabled/missing-CLI state
  (`codex_launch_precondition(local_harness_setup_state(harness))`) **inside**
  the `Harness::Codex` match arm — i.e. strictly after shell validation.
- Consequence: a user with `shell_type: None` (unsupported/undetected shell)
  trying to launch a **disabled-by-default** Codex child
  (`FeatureFlag::LocalClaudeCodexChildHarnesses` is off by default — see
  `local_harness_setup.rs`'s doc comment and `LOCAL_FLAGS`) got "Local child
  harnesses currently require a detected bash, zsh, or fish session"
  instead of the actually-operative "Local Codex child agents are
  temporarily disabled." No code comment documented this as deliberate; it
  fell out incidentally from how #323 structured the Codex arm.

**Fix** (`app/src/pane_group/pane/local_harness_launch.rs`): hoisted the
Codex precondition check to run before `validate_local_harness_shell`,
gated on `harness == Harness::Codex` (the fork's per-harness structure
differs from the pin's blanket check, so a harness-specific guard is the
minimal change that preserves the fork's #323 structure while fixing the
ordering). Removed the now-redundant call from inside the match arm and
left a comment explaining where it now runs.

**Test ported** into `local_harness_launch_tests.rs`, adapted to the fork's
`prepare_local_harness_child_launch` signature (no `ai_client` parameter,
same drop as the file's other adapted tests) and to the fork's error-message
idiom: `prepare_local_harness_child_launch` wraps
`AgentDriverError::HarnessSetupFailed`'s Display output
(`"Harness 'codex' setup failed: {reason}"`) via `.to_string()`, so the
assertion checks `.contains(LOCAL_CODEX_HARNESS_DISABLED_MESSAGE)` rather
than exact equality — the same idiom the file's existing
`codex_launch_precondition_refuses_launch_when_product_disabled` test uses.

**Verified NOT a cloud/declined path**: `FeatureFlag::LocalClaudeCodexChildHarnesses`
gates a real, actively-developed local feature (#323, #411), not anything
cloud-coupled.

**Gate status**: `rustfmt --check --config-path .rustfmt.toml` is clean on
both changed lines in `local_harness_launch.rs` (the file has two
*pre-existing* formatting diffs, verified via `git stash` against the
unmodified file — an import-order hunk and a `let harness_model_config = ...`
reflow, neither touched by this change) and fully clean on
`local_harness_launch_tests.rs`. No parse errors in either file.
