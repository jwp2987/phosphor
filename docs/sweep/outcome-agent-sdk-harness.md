# Outcome: `agent_sdk/driver/harness` (claude_code_tests + wake_driver_tests + mod_tests)

Package from `TODO.md`'s 2026-08-11 six-agent round: `outcome-agent-sdk-harness`, 10
tests across `claude_code_tests.rs` (5), `claude_code/wake_driver_tests.rs` (1), and
`mod_test.rs` (4, named `mod_tests.rs` in the assignment — the fork's actual file is
singular, `mod_test.rs`, declared via `#[path = "mod_test.rs"]` in `mod.rs:430`).

Branch: `sweep/agent-sdk-harness-2026-08-11`, based on local `main` `17025cd66`.

**Everything in this package had already been independently re-verified once
before** (round 5, 2026-08-07, and a round-4-correction on 2026-08-11 visible in
the ledger's `prepare_local_wake_command_...` row). This round re-derived each
verdict from the current oracle/fork source rather than trusting the prior
write-up, per `CLAUDE.local.md`'s "verify before acting." One correction came out
of that re-derivation: `wake_driver_tests.rs`'s test was carried on the ledger as
`MISSING-SUBSYSTEM` (2026-08-10) even though its sibling test in the same file was
already corrected to `CLOUD` the next day (2026-08-11) for the same root cause —
see below.

## A. `claude_code_tests.rs` — 5/5 PORTED

All five needed real, previously-missing local functionality. None of it required
`ServerApi`/cloud plumbing; each new piece is documented in place with a "ported
from the pin" note pointing back here.

| test | outcome | evidence |
|---|---|---|
| `claude_command_uses_resume_flag_when_resuming` | **PORTED** | `claude_command` (`claude_code.rs`) gained the pin's `resuming: bool` parameter, selecting `--resume` vs `--session-id`. The CLI-flag-selection logic is real and self-contained. **Caveat, disclosed in the new doc comment:** this fork's sole caller (`ClaudeHarnessRunner::new`) always passes `false` — there is no resume-payload plumbing anywhere in this fork that could ever pass `true` (the pin's `fetch_resume_payload`/`ResumePayload`/`ClaudeResumeInfo` machinery is cloud-only, fetching the prior transcript via `HarnessSupportClient`; already absent here per `codex.rs`'s module doc for the equivalent Codex cut). The ported function and test are correct and real; the flag is simply unreached in production until a local resume path exists. |
| `message_bridge_cleanup_preserves_state_for_wakeable_runs` | **PORTED** | Required building three new, non-cloud pieces, all wired into real call sites (not just test-only scaffolding): (1) `harness::HarnessCleanupDisposition` (`mod.rs`) — `DropResumptionState` / `PreserveResumptionStateIfSupported`, computed in `AgentDriver::run_harness` (`driver.rs`) from local signals only: final-save success, no mid-run runtime failure, and a clean exit code. (2) `parent_bridge::MessageBridgeCleanupDisposition` (`RemoveState`/`PreserveState`) threaded into `MessageBridge::cleanup`. (3) `ClaudeHarnessRunner::should_preserve_parent_bridge`, which additionally checks the CLI agent session isn't still `InProgress`/`Blocked` (this fork's `CLIAgentSessionStatus` has no `Failed` variant, unlike the pin's — noted in the adaptation comment). |
| `message_bridge_cleanup_removes_state_for_non_wakeable_runs` | **PORTED** | Same wiring as above; this is the `RemoveState` branch. |
| `parent_bridge_event_cursor_defaults_to_zero_when_missing` | **PORTED** | `read_parent_bridge_event_cursor`/`write_parent_bridge_event_cursor` ported to `parent_bridge.rs`, pure local disk I/O as the pre-existing round-5 comment predicted. Given a real caller, not left dead: `MessageBridgeEventConsumer::persist_cursor` (an existing no-op-by-default trait hook, `agent_events/driver.rs:164`) now writes the cursor on each processed event, and `run_parent_bridge_forever` reads it back as the stream's starting `since_sequence` instead of hardcoding `0` — so a bridge-process restart resumes instead of replaying history. |
| `parent_bridge_event_cursor_round_trips` | **PORTED** | Same functions as above. |

**What this closes vs. what it doesn't:** the pin's `MessageBridgeCleanupDisposition`
and cursor machinery also feed a cloud dormant-wake reader
(`ClaudeHarness::wake_dormant_session`) that this fork does not have and this round
did not add (see part B) — so "preserve" currently just means "leave the directory
on disk," with no reader consuming it yet. That is still real, tested behavior
(a local-only unfinished half of a larger pin feature), not a stub — same
distinction the pre-existing note in this file already drew for
`runtime_error_patterns`.

**Files touched:** `app/src/ai/agent_sdk/driver.rs`,
`app/src/ai/agent_sdk/driver/harness/mod.rs`,
`app/src/ai/agent_sdk/driver/harness/claude_code.rs`,
`app/src/ai/agent_sdk/driver/harness/claude_code/parent_bridge.rs`,
`app/src/ai/agent_sdk/driver/harness/claude_code_tests.rs`.

Still blocked in the same file (not in this package's 5, listed for completeness
since the file's own header comment enumerates all 9 remaining oracle tests):
`write_session_index_entry_creates_expected_entry` (needs `claude_transcript`
wired into this file, #289),
`prepare_local_wake_command_rehydrates_transcript_with_self_managed_listener` and
`prime_parent_bridge_staged_for_self_managed_wake_keeps_message_in_staged` (cloud,
see part B), and `resolve_suffix_from_resolved_env_vars` (signature divergence,
not missing capability).

## B. `claude_code/wake_driver_tests.rs` — 1 test — RE-ADJUDICATED, not ported

| test | outcome |
|---|---|
| `local_wake_task_state_ready_allows_success_and_stale_in_progress` | **RE-ADJUDICATED: CLOUD** (was `MISSING-SUBSYSTEM` on the ledger, dated 2026-08-10) |

**Assignment's framing needed correction.** The assignment described this as
"`wake_driver.rs` is absent... `is_local_wake_task_state_ready` absent" and asked
to verify. That much is true (confirmed: no `wake_driver.rs` under
`app/src/ai/agent_sdk/driver/harness/claude_code/`, no matches for "wake" anywhere
in this fork's `agent_sdk` tree). But the deeper question — *why* is it absent, and
does that make it portable or declined — needed tracing into the oracle, which the
assignment didn't do.

At the pin, `is_local_wake_task_state_ready` is a private, pure function (`state:
AmbientAgentTaskState -> bool`) with exactly one caller:
`ClaudeHarness::wake_dormant_session`, the entry point for "wake a dormant/completed
Claude session when a lead-agent message arrives." That caller:
- calls `server_api.get_ambient_agent_task(&task_id)` to fetch the task's
  server-tracked state — this fork has no `ServerApi` at all (confirmed: `grep -rln
  "struct ServerApi\b" app/src` returns nothing);
- calls `server_api.resolve_prompt_for_task` and `server_api.fetch_transcript_for_task`
  to rehydrate the session from the server;
- calls `server_api.update_agent_task(...)` to reopen the task on the server.

This fork's own `AmbientAgentTaskState` (`app/src/ai/ambient_agents/task.rs:361`)
happens to have identical variants to the pin's, but it is populated as a local
UI/conversation-state snapshot, not from any server task-polling mechanism — there
is no `get_ambient_agent_task`-equivalent anywhere in this fork (`grep -rn "fn
get_ambient_agent_task"` finds nothing; the sole near-miss,
`BlocklistController::get_ambient_agent_task_id`, returns an id, not a
server-fetched task). So the type match is coincidental, not evidence the feature
is buildable without `ServerApi`.

This is exactly the shape the already-existing (2026-08-11) sibling ledger entry
for `prepare_local_wake_command_rehydrates_transcript_with_self_managed_listener`
already established for two other tests in the same file: "cloud, not merely
'local wake absent' as round 4 characterized them ... need the dropped
`ServerApi`/`AIClient` cloud plumbing regardless of the 'local' naming." The same
reasoning applies to `is_local_wake_task_state_ready` — it is a fragment of that
same cloud-only `wake_dormant_session` flow, with no caller and no purpose outside
it. Porting the pure function alone (adding it with no caller, just to pass a
test) is the same "manufactured debt" pattern this repo's conventions warn against
for `auth_check_command` in part C: a test that passes while asserting nothing
about a feature this fork actually has.

**Proposed `DECLINED.md` row** (not added — coordinator reconciles `DECLINED.md`
centrally, per the same reasoning `TODO.md` gives for not touching the ledger
directly):

```
| **Claude Code dormant-session wake** | #252/#289 | `claude_code/wake_driver.rs` doesn't exist here: `ClaudeHarness::wake_dormant_session` needs `ServerApi::{get_ambient_agent_task, resolve_prompt_for_task, fetch_transcript_for_task, update_agent_task}` (dropped, no local equivalent) to detect and resume a completed session when a lead-agent message arrives. `is_local_wake_task_state_ready` (`wake_driver_tests.rs`) is a pure function but has no caller or purpose outside that flow -- this fork's `AmbientAgentTaskState` has matching variants by coincidence (it's a local UI-state snapshot type, not server-polled), not because the feature is buildable without `ServerApi`. <!-- markers: path:app/src/ai/agent_sdk/driver/harness/claude_code/wake_driver.rs sym:wake_dormant_session sym:is_local_wake_task_state_ready test:local_wake_task_state_ready_allows_success_and_stale_in_progress --> |
```

**Ledger correction needed** (not applied — outcome doc only, per instructions):
`docs/sweep-verdict-ledger.tsv:264` (`local_wake_task_state_ready_...`) should move
from `MISSING-SUBSYSTEM` to `CLOUD`, matching row 270's 2026-08-11 correction for
its sibling in the same file.

## C. `mod_test.rs` — 4 tests — RE-ADJUDICATED, verdict confirmed (no change)

| test | outcome |
|---|---|
| `auth_check_command_for_gemini_is_none` | **RE-ADJUDICATED: verdict confirmed, MISSING-SUBSYSTEM (deferred under #289)** |
| `auth_check_command_for_oz_is_none` | same |
| `auth_check_command_for_unknown_is_none` | same |
| `auth_check_command_for_unsupported_is_none` | same |

Independently re-verified rather than trusted: `ThirdPartyHarness` (`mod.rs`) has
no `auth_check_command`/`auth_check_command_for` method or default impl anywhere
in this fork (confirmed by reading the full trait definition, `mod.rs:48-115`),
and no caller exists that would need one. The fork's own `mod_test.rs:1-16` header
comment and `codex.rs:37-38`'s module doc both already state the #289 deferral
and give the same reason the assignment did: at the pin this is a trait method
with a trivial `None` default, so porting the default alone would produce a test
that passes while asserting nothing about the feature.

This matches the existing ledger rows
(`docs/sweep-verdict-ledger.tsv:277-280`, `RE-VERIFIED 2026-08-10`) and
`TODO.md`'s own framing of this file as "verdict-first, not port-first." No code
change and no verdict change — the deferral holds. Not touched:
`mod_test.rs`, `docs/sweep-verdict-ledger.tsv`, `TODO.md`.

## Summary

| bucket | count |
|---|---:|
| PORTED | 5 |
| RE-ADJUDICATED (verdict changed: MISSING-SUBSYSTEM -> CLOUD) | 1 |
| RE-ADJUDICATED (verdict confirmed unchanged: MISSING-SUBSYSTEM, #289 deferral) | 4 |

## What remains open

- The `--resume` flag on `claude_command` is real but currently unreachable in
  production (fork has no resume-payload plumbing to ever pass `resuming: true`).
  Wiring an actual local resume path is out of this package's scope.
- `MessageBridgeCleanupDisposition::PreserveState` has no reader yet (no
  dormant-wake feature exists to consume the preserved directory) — see part B.
  Building the reader is a much larger, cloud-blocked feature.
- Part B's ledger correction and proposed `DECLINED.md` row are written here for
  the coordinator to apply; this branch does not touch either file.
- `write_session_index_entry_creates_expected_entry` (needs `claude_transcript`
  wiring, #289) and the two genuinely-cloud wake tests remain blocked in
  `claude_code_tests.rs`, as already documented in that file before this round.
- Not run: `cargo`/`nextest`/`script/precheck` (forbidden for this agent). Verified
  instead with `rustfmt --check --config-path .rustfmt.toml` on all 5 touched
  files (parses cleanly, matching `script/precheck`'s own parse-only rustfmt gate
  — the repo is not fully rustfmt-clean project-wide, so a full formatting diff
  was not chased into unrelated pre-existing drift in these same files, e.g.
  import-ordering in `codex.rs`/`gemini.rs` and `assert!` wrapping elsewhere in
  `claude_code_tests.rs` that this change never touched).
