# Outcome: `ai/agent_events/driver_tests.rs` — 11 pin tests

Task package: port the 11 named pin tests onto `AgentEventDriverConfig`'s
give-up/backoff-classification fields (`auth_error_give_up_failures`,
`max_retry_duration`, `permanent_error_backoff_steps`) plus
`HttpStatusError::is_actionable()` and `agent_event_failure_should_log_error()`.

Base branch: `working` at `32257ed2a` (fast-forwarded from an older point on
this worktree; `working` has since moved further via other agents' commits —
not chased, out of scope for this package).

## Starting state found

`working` already carried a rescued, uncommitted-review WIP
(`9b8307ec4`, "wip(agent_events): rescued from the shared checkout") that had
**already implemented all 11 tests plus the config/driver changes they need**,
and a follow-up commit (`5f7c96883`) that moved `HttpStatusError` from
`app/src/server/retry_strategies.rs` to `app/src/util/retry_strategies.rs` to
clear a `check_cloud_boundary` failure. Both were unbuilt and unreviewed. This
package's work was: verify that WIP against the pin line-by-line, fix a real
defect the move left behind, reformat, and re-run the guards.

## Defect found and fixed (not part of the original 11, but blocking)

`5f7c96883` moved `app/src/server/retry_strategies.rs` →
`app/src/util/retry_strategies.rs` but **left `retry_strategies_test.rs`
behind in `app/src/server/`**. `util/retry_strategies.rs:147` declares
`#[path = "retry_strategies_test.rs"]`, a path relative to its own directory
(`util/`), so the module would have failed to compile (`file not found`) the
moment anyone tried to build this crate. Fixed with `git mv
app/src/server/retry_strategies_test.rs app/src/util/retry_strategies_test.rs`
— the test file has zero cloud dependencies (confirmed by reading it: `std`,
`anyhow`, `futures::executor::block_on`, and `super::*`), so the move is a
pure relocation, no content change beyond rustfmt (see below).

## Verification method

Since the build freeze forbids `cargo`/`rustc`, verification was source
comparison against the pin, not compilation:

- Pulled `app/src/ai/agent_events/driver.rs`, `driver_tests.rs`, and
  `app/src/server/retry_strategies.rs` from `02b53fcd8` via `git show`.
- Diffed the fork's driver/test logic against the pin's line-by-line — see
  per-test evidence below.
- Ran `script/check_cloud_boundary` and `script/check_stub_coverage` (pure
  shell, permitted).
- Ran `rustfmt --check` per touched file (checking `mod.rs` in isolation
  pulls in a sibling module via rustfmt's mod-tree resolution — see Notes).

## Per-test outcome

All 11 — **PORTED, written, unverified (cannot compile under the build
freeze)**. The fork's `run_agent_event_driver`, `AgentEventDriverConfig`,
`agent_event_give_up_reason`, `classify_failure_and_give_up_reason`,
`handle_http_error`, and `agent_event_failure_should_log_error` are
line-for-line equivalent to the pin's (same field names except
`run_ids: Vec<String>` in place of the pin's `filter: AgentEventFilter` — see
Adaptation below), and every test body matches the pin's byte-for-byte modulo
that substitution and the import paths.

| test | outcome | evidence |
|---|---|---|
| `driver_gives_up_after_consecutive_auth_failures` | PORTED | Pin lines 478-509; fork matches exactly (3× 401 → `Err`, `handled_sequences` empty). Exercises `agent_event_give_up_reason`'s `auth_error_give_up_failures` branch, present in fork `driver.rs:468-474`. |
| `driver_does_not_count_non_auth_failures_toward_auth_give_up` | PORTED | Pin lines 551-590; fork matches exactly (500, 500, 401 → still reconnects, only 1 consecutive auth failure). Exercises the reset in `classify_failure_and_give_up_reason` (`driver.rs:393-397`: non-auth error resets `consecutive_auth_failures` to 0). |
| `driver_does_not_give_up_on_non_auth_error_when_only_auth_bounded` | PORTED | Pin lines 511-549; fork matches exactly (3× 500 with `auth_error_give_up_failures: Some(3)` set → driver keeps retrying and succeeds, because 500 never increments the auth streak). |
| `driver_resets_auth_streak_after_non_auth_failure` | PORTED | Pin lines 592-628; fork matches exactly (401, 500, 401, 401, 401 → gives up only after the fresh run of 3 consecutive 401s following the 500 reset; comment notes the fake source panics if over-polled, proving the reset behavior). |
| `driver_gives_up_after_max_retry_duration` | PORTED | Pin lines 630-656; fork matches exactly (`max_retry_duration: Some(Duration::from_secs(0))` gives up on the first failure). Exercises the `agent_event_give_up_reason`'s `max_retry_duration` branch (`driver.rs:476-483`), seeded via `retry_window_started_at.get_or_insert(now)`. |
| `driver_uses_fast_backoff_on_transient_http_error` | PORTED | Pin lines 658-710; fork matches exactly. 500 is transient (`is_transient_http_error` → `matches!(status, 408\|429\|500..=599)`), so `handle_http_error` selects `config.reconnect_backoff_steps` (`ZERO_BACKOFF_STEPS` = 0s) over the deliberately-huge `permanent_error_backoff_steps: &[9999]`. |
| `driver_uses_slow_backoff_on_permanent_http_error` | PORTED | Pin lines 423-476; fork matches exactly. 404 is not transient, so `handle_http_error` selects `config.permanent_error_backoff_steps` (0s) over the huge `reconnect_backoff_steps: &[9999]`. |
| `http_status_error_actionability_follows_status_classification` | PORTED | Pin lines 416-421; fork matches exactly. `HttpStatusError::is_actionable()` in `app/src/util/retry_strategies.rs:62-66` is `!matches!(self.status, 408 \| 429)` — confirmed identical to the pin's definition, which the fork's `retry_strategies.rs` had to re-host because the pin's lives in `crates/warp_server_client/src/public_api.rs` (a cloud crate) at `HttpStatusError::is_actionable`. Verified byte-identical logic by reading `crates/warp_server_client/src/public_api.rs` at the pin. |
| `non_actionable_stream_statuses_do_not_report_at_threshold` | PORTED | Pin lines 399-408; fork matches exactly (408/429 never log even at threshold, because `is_actionable()` is false for both). |
| `server_error_status_reports_at_threshold_crossing` | PORTED | Pin lines 410-414; fork matches exactly (500 logs at `failures == threshold`). |
| `zero_threshold_disables_stream_error_escalation` | PORTED | Pin lines 393-397; fork matches exactly (`threshold: 0` short-circuits `agent_event_failure_should_log_error` via `threshold > 0 && ...`). |

`agent_event_failure_should_log_error` itself
(`app/src/ai/agent_events/driver.rs:543-549`) is byte-identical to the pin's
(`threshold > 0 && failures == threshold && err.is_actionable()`).

## Adaptation from the pin (disclosed, not a simplification of behavior)

The pin's `AgentEventDriverConfig.filter` is an `AgentEventFilter` enum with
two variants: `RunIds(Vec<String>)` (used by the local/child-run case) and
`AncestorRunId { ancestor_run_id, include_self }` (used only by the
shared-session viewer's pill bar and the cloud-agent message bridge —
`ServerApiAgentEventSource::open_stream`'s `AncestorRunId` arm calls
`self.server_api.stream_agent_events_for_ancestor(...)`, and
`AgentEventDriverConfig::bounded_run_ids` — the pin constructor that actually
sets `auth_error_give_up_failures`/`max_retry_duration` in production — is
doc-commented "Only called by the (native-only) cloud-agent message bridge").
The fork keeps the pre-existing (pre-dating this task) `run_ids: Vec<String>`
field instead of porting `AgentEventFilter`, since `AncestorRunId` has no
non-cloud caller in this fork. This is a scope decision already baked into
the fork before this package started, not something introduced here — it does
not change any of the 11 tests' semantics, since all 11 construct
`AgentEventDriverConfig` directly with `run_ids: vec![...]` and never touch
`AncestorRunId`. Flagging per the "port only the local half, say so" instruction.

Similarly, the pin's `bounded_run_ids()` constructor and its
`DEFAULT_AUTH_ERROR_GIVE_UP_FAILURES`/`DEFAULT_AGENT_EVENT_MAX_RETRY_DURATION`
constants (both `#[cfg_attr(target_family = "wasm", allow(dead_code))]` and
only reachable from the cloud-agent message bridge) were **not** ported — the
11 tests don't need them, they construct `AgentEventDriverConfig` literals
with `Some(3)` / `Some(Duration::from_secs(0))` directly. Only the generic
driver mechanism (the fields + `agent_event_give_up_reason` logic) was
ported, which is real, local, BYOP-path behavior with no cloud dependency.

One pin test, `actionable_stream_status_reports_only_at_threshold_crossing`
(pin `driver_tests.rs:380-391`), was **not** in this package's list of 11 and
was left unported — noted for completeness, not a gap in this package's scope.

## Guards run (pure shell, permitted under the build freeze)

- `script/check_cloud_boundary` → `ok (270 allowlisted import sites)`
- `script/check_stub_coverage` → `ok (no test targets a gutted stub)`
- `rustfmt --check` on each touched file individually → clean:
  `app/src/ai/agent_events/driver.rs`, `app/src/ai/agent_events/driver_tests.rs`,
  `app/src/ai/agent_events/mod.rs`, `app/src/util/retry_strategies.rs`,
  `app/src/util/retry_strategies_test.rs`.

**Note on `rustfmt --check`:** running it against `app/src/ai/agent_events/mod.rs`
alone reports a diff, but that diff is entirely inside
`app/src/ai/agent_events/message_hydrator_tests.rs` — rustfmt resolves the
`mod message_hydrator_tests;` declaration and reformats the sibling file too.
That file was not touched by this package (pre-existing drift, unrelated to
agent-events driver work) and was deliberately left alone; an earlier
attempt to batch-format all touched files together caught this and it was
reverted before committing. `mod.rs`'s own content has no diff.

## Not done / not verified

- **Cannot compile.** The build freeze forbids `cargo`/`rustc` in any form.
  Every claim above is source comparison against the pin, not a passing test
  run. Written, unverified.
- **`message_hydrator_tests.rs` formatting drift** exists on `working` but is
  out of this package's scope; left alone.
- **`working` has moved past this package's base** (`32257ed2a` → at least
  `989087c3d` as of this writeup) via other concurrent agents. Not rebased
  onto — the commit that lands from this branch will need normal merge
  handling, not a fast-forward, if `working`'s tip has touched the same files.
