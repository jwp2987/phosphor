# Outcome: shared local child-launch executor + `orchestration_model_tests.rs`

Task: give the TUI a shared local child-launch executor (mirroring the pin's
`StartAgentExecutor` architecture, GUI+TUI both consuming it, not a
TUI-private mechanism), and port `orchestration_model_tests.rs`'s last two
`MISSING-SUBSYSTEM` tests: `failed_launch_cleanup_preserves_other_sessions`
and `local_harness_children_fail_cleanly`.

Branch: current worktree branch (fast-forwarded onto `working` @ `3d83ef816`
before starting; no other work landed on top).

**Build freeze in force. Nothing here has been compiled or run** — every
claim below is source-level review, not a green test. `rustfmt --check` and
the two shell guards were run and are clean (see bottom).

## What was built

### 1. `StartAgentExecutor` — `app/src/ai/blocklist/action_model/execute/start_agent.rs` (new)

A frontend-neutral, shared entity, following the exact template the task
pointed at (`ShellCommandExecutor`/`CLISubagentController` in
`app/src/tui_export.rs:60-71`): defined in the `app` crate under
`ai/blocklist/action_model/execute/`, wired through `execute.rs` →
`action_model.rs` → `tui_export.rs`, so both the GUI and TUI *could* consume
it (only the TUI actually does, in this round — see "Not finished" below).

Ported from the pin's `app/src/ai/blocklist/action_model/execute/start_agent.rs`
(`02b53fcd8`): `StartAgentOutcome`, `StartAgentRequestId`, `StartAgentRequest`,
`StartAgentExecutor` (`dispatch`), `StartAgentExecutorEvent`
(`CreateAgent`/`CleanupFailedChildLaunch`), and
`should_cleanup_failed_child_launch` (ported verbatim — this fork's
`ConversationStatus` has the identical variant set).

**`StartAgentExecutionMode`** (`crates/ai/src/agent/action/mod.rs`) is ported
LOCAL-only: the pin's `Remote` variant is omitted outright, not stubbed —
`is_remote_child` is permanently false in this fork (`DECLINED.md` #290 /
the "Orchestration config-picker layer" row), so there is no code path that
could ever construct one.

### 2. The seam: why resolution is a direct call, not a history-event broadcast

The pin links a pending `StartAgentExecutor` request to its child
conversation by watching two `BlocklistAIHistoryEvent` variants —
`NewConversationRequestComplete` and `ConversationServerTokenAssigned` —
both fired only when a conversation is server-created. **Neither variant
exists in this fork's `BlocklistAIHistoryEvent`** (verified by reading the
full enum, `app/src/ai/blocklist/history_model.rs:2543-2693`): there is no
server, so there is nothing to assign a token or complete a server-side
request. Growing that already-large, heavily-matched enum with a
fork-only event just to restore the pin's decoupled broadcast would have
meant hand-auditing every exhaustive `match` on it across the crate (the
exact risk `HANDOFF.md` flags: "a non-exhaustive match after adding an enum
variant breaks every exhaustive match").

Instead, resolution is a **direct call**: `StartAgentExecutor::resolve_error`
is a plain method, and whichever materializer decided a request's outcome
(here, `TuiOrchestrationModel::fail_child_request`) calls it directly on the
`ModelHandle<StartAgentExecutor>` it already holds — it received that same
handle alongside the `CreateAgent` event it's handling. This is safe only
because every resolution in this fork's current scope happens synchronously
within the same dispatch; there is no cross-cutting consumer (like the pin's
`OrchestrationEventStreamer`) that needs the decoupled broadcast. Documented
in `start_agent.rs`'s module doc in detail, including the exact grep that
established the two pin events are absent.

**A second, independently-discovered seam:** the pin's
`prepare_local_oz_child_launch` — despite being the "local" half of
`StartAgentExecutionMode` — creates the child's server task via
`ServerApiProvider::as_ref(ctx).get_ai_client().create_agent_task(..)`
(`crates/ai/src/agent/action/mod.rs:28-56` at the pin). That is cloud
regardless of the "local" name. The task brief listed `prepare_local_oz_child_launch`
as in-scope; tracing its actual body during this round found the cloud
coupling, so **it was not ported** — building a genuinely local replacement
(materializing a real TUI session, likely reusing
`app/src/pane_group/pane/local_harness_launch.rs`'s already-local
`prepare_local_harness_child_launch` machinery) is future work, called out
explicitly in both `start_agent.rs`'s and `orchestration_model.rs`'s module
docs. This also means `StartAgentExecutionMode::Local { harness_type: None,
.. }` (the "native embedded local child" mode) has no success path yet
either — it resolves as a clean failure with its own message, same shape as
the named-harness case the two goal tests exercise.

### 3. TUI wiring — `crates/warp_tui/src/orchestration_model.rs`

Added `dispatch_create_agent`, `fail_child_request`, `cleanup_failed_child`
(all present in the pin, all missing here before this round), and an
`event_consumers_by_session` field (always empty in this scoped port — see
below). Module doc comment rewritten: it previously stated
`StartAgentExecutor` "does not exist anywhere in this fork, not even for the
GUI" — that claim is now stale and has been corrected in place, along with a
restatement of exactly what is still cut (native local-Oz materialization,
everything remote) and why.

**Not ported from the pin's `orchestration_model.rs`:** `begin_local_oz_child_launch`,
`register_local_oz_child_session`, `begin_remote_child_launch`,
`register_remote_child_session`, `finish_remote_child_launch`,
`handle_streamer_event`, `register_event_consumer`, `handle_session_removed`,
and the `TuiOrchestrationEvent` enum that carries their requests. None of
these are reachable from the two goal tests (both exercise only the
`harness_type: Some(_)` failure path), and all of them either need the
cloud-coupled `prepare_local_oz_child_launch` or are the declined remote
path. `event_consumers_by_session` is kept as a struct field (read by
`assert_failed_launch_cleaned_up`'s canary assertion) but nothing writes to
it in this port, since `register_event_consumer` is one of the omitted
pieces — documented in place on the field.

## Per-test outcome — written, unverified

Both goal tests are written into the existing
`crates/warp_tui/src/orchestration_model_tests.rs` (this file already existed
on `working`, containing one previously-ported test,
`snapshot_is_shared_across_tree_and_filters_conversations_without_sessions` —
its fixture helpers, `orchestration_fixture`/`add_dispatching_session`, are
reused as-is, not duplicated):

- **`local_harness_children_fail_cleanly`** — written. Dispatches
  `StartAgentExecutionMode::Local { harness_type: Some("claude"), model_id: None }`
  through a `StartAgentExecutor` relayed into `dispatch_create_agent`, asserts
  the outcome is `Error` containing "aren't supported in Warp Agent CLI yet",
  and asserts the failed child's conversation, `event_consumers_by_session`,
  and session count are all cleaned up (matching the pin's assertions
  exactly, adapted to this fork's `BlocklistAIHistoryModel` API — see below).
- **`failed_launch_cleanup_preserves_other_sessions`** — written. Same
  shape, with a second, unrelated foreground session present throughout, to
  assert the failed launch on the background session's executor doesn't
  touch the other session's conversation tree or session count.

Also fixed a bug caught during self-review before finalizing: the first draft
of `add_relayed_executor` only relayed `StartAgentExecutorEvent::CreateAgent`
and silently dropped `CleanupFailedChildLaunch`, which would have left the
failed child's ephemeral conversation never deleted — breaking exactly the
cleanup assertion these tests exist to check. Fixed to relay both variants
into `dispatch_create_agent` / `cleanup_failed_child`, matching the pin's own
`add_relayed_executor`.

**API adaptations from the pin's test file, all traced against this fork's
real `BlocklistAIHistoryModel`/`AIConversation` (not assumed):**
`status_error_message()`/`status()` exist with identical signatures;
`update_conversation_status_with_error_message` replaces the pin's
`update_conversation_status`-plus-separate-error-message-field shape;
`terminal_view_id_for_conversation` replaces
`terminal_surface_id_for_conversation` (same behavior, different name —
confirmed by reading its body); `child_conversation_ids_of` and
`delete_conversation` match the pin's signatures exactly.

**I did not run these tests and cannot claim they pass.** Say "written,
unverified" for both — not "should pass," not "done."

## Explicit list of what was NOT finished

- **GUI wiring.** `StartAgentExecutor` is not instantiated anywhere in
  `app/src/pane_group/pane/terminal_pane.rs` or `app/src/terminal/view.rs`.
  The type is shared/reusable (not TUI-only), but only the TUI actually
  dispatches through it in this round.
- **Native local-Oz child success path** (`harness_type: None` resolving as
  `Started` instead of a clean failure). Blocked on a local (non-cloud)
  replacement for `prepare_local_oz_child_launch`; not attempted.
- **Session materialization for named-harness children.** The pin's own
  comment says the TUI doesn't support this ("would be odd in the TUI"), so
  this is not a gap versus the pin — flagging it only so it isn't mistaken
  for an oversight.
- **`snapshot_is_shared_across_tree_and_filters_conversations_without_sessions`**
  already existed on `working` before this round; not touched, not
  re-verified beyond confirming it doesn't exercise any of the new code.
- The pin's other two `orchestration_model_tests.rs` tests
  (`github_auth_blocker_keeps_the_remote_session_and_actionable_url`,
  `remote_child_session_is_navigable_and_projects_lifecycle`) are remote —
  correctly not ported; this fork's `StartAgentExecutionMode` has no
  `Remote` variant to dispatch them through.

## Guards run (pure shell, per the task's tooling allowance)

- `rustfmt --check` on every file touched: clean. (Two adjacent files —
  `crates/ai/src/agent/action/convert.rs` and the pre-existing `use` block
  in `crates/ai/src/agent/action/mod.rs` and `app/src/tui_export.rs` —
  report unrelated formatting drift from before this round; confirmed via
  `git diff` that none of it is inside my changes, so left alone per the
  brief's "revert cosmetic reordering of untouched code, don't touch what
  you didn't edit.")
- `script/check_cloud_boundary`: **ok** (270 allowlisted import sites,
  unchanged).
- `script/check_stub_coverage`: **ok**.
- `script/check_declined_collisions` / `script/check_sweep_ledger`: also run
  (not required by the brief, cheap): both **ok**.

## Files touched

- `crates/ai/src/agent/action/mod.rs` — `StartAgentExecutionMode` (new).
- `app/src/ai/blocklist/action_model/execute/start_agent.rs` — new file.
- `app/src/ai/blocklist/action_model/execute.rs` — registers the new module,
  re-exports its public types.
- `app/src/ai/blocklist/action_model.rs` — re-exports up one level.
- `app/src/tui_export.rs` — re-exports `StartAgentExecutionMode` and the
  `start_agent` types to `warp_tui`.
- `crates/warp_tui/src/orchestration_model.rs` — module doc corrected;
  `dispatch_create_agent`, `fail_child_request`, `cleanup_failed_child`,
  `event_consumers_by_session` added.
- `crates/warp_tui/src/orchestration_model_tests.rs` — module doc updated;
  `add_relayed_executor`, `dispatch_and_recv`, `assert_error_containing`,
  `assert_failed_launch_cleaned_up`, and the two goal tests added.
