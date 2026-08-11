# Build triage — the ~70 unverified commits of 2026-08-10/11

**Purpose.** Every agent that landed code in this batch was forbidden from
running `cargo`, and each was required to hand back a ranked list of what it
doubted would compile. This file collates all of them, grouped by file, so the
first build is worked through in a planned order rather than reactively.

**Status: PREDICTIONS, not errors.** Nothing here has been compiled. When the
build runs, mark each line hit/missed — the miss rate is itself worth knowing,
because it tells us how much to trust unbuilt agent output next time.

**How to use it.** Build once, capture the full error list, then match errors
against the sections below. Anything matching a prediction has a suggested fix
already. Anything *not* matching is the interesting case — it means the agents'
self-assessment missed a class of problem.

---

## Tier 1 — most likely, and highest blast radius

### `crates/remote_server/proto/remote_server.proto` + 3 consumers
**Prediction: prost enum-variant naming for `RemoteCodebaseSearchErrorCode`.**
The agent inferred prost's common-prefix-stripping rule from *usage patterns
elsewhere in the tree* (`FragmentMetadataLookupErrorCode`,
`CodebaseIndexStatusState`) rather than from generated code — it could not build,
so it never saw the generated names. If the inference is wrong, **every
`RemoteCodebaseSearchErrorCode::Variant` reference across three files needs
renaming.**
*Fix:* read the generated `out/remote_server.rs` after the first build; rename
mechanically.
*Why tier 1:* single root cause, many call sites, and the only prediction in the
batch that was explicitly reasoned-by-analogy rather than read.

### `crates/remote_server/src/manager.rs`
**Prediction: `HostRequestHandle` gaining `#[derive(Clone)]`.** Depends on
`ModelSpawner<M>`'s manual `impl<M> Clone` (verified unconditional) and
`warp_core::HostId: Clone` (verified derived). Believed fine, unverified.

### `app/src/ai/blocklist/usage/conversation_usage_view.rs`
**Prediction: `Text::new(..).with_color(..).soft_wrap(false).with_clip(ClipConfig::ellipsis()).finish()`.**
Method names verified individually elsewhere; *this exact chain order* was not.

---

## Tier 2 — borrow-checker shapes

These are the densest regions written blind. All were traced by hand against an
existing identical pattern, which is why they are tier 2 and not tier 1 — but
NLL surprises are exactly what hand-tracing misses.

| file | shape |
|---|---|
| `app/src/code/global_buffer_model.rs` | `force_reload_server_local` — `state`/`sync_clock` immutable borrows must end before `ctx.spawn`; inside the callback `state` is used before *and* after `buffer.update(ctx, ..)`. Densest borrow region in the batch. |
| `crates/warp_tui/src/terminal_session_view.rs` | `render_footer`'s new `terminal_model.lock()..is_agent_tagged_in()` branch, with `builder`/`muted` already borrowed above. |
| `app/src/remote_server/server_model.rs` | `handle_search_remote_codebase` — `msg.repo_path` borrowed, then `msg.query` moved into a closure that also borrows `&repo_path`. |
| `app/src/ai/skills/file_watchers/skill_watcher.rs` | `RepoMetadataModel::as_ref(ctx)` then passing `ctx` again to a `&AppContext` parameter — overlapping immutable reborrows through `ModelContext: Deref`. |
| `crates/ai/src/index/.../local_store_client.rs` | `ranked_paths_for` receiver changed `&self` → `&mut self`; `retrieval_requests.remove()` then `self.ranked_paths_for(..)` in sequence. |
| `app/src/ai/blocklist/agent_view/conversation_selection.rs` | `matches!` with an `if action_id == ..` guard while `&self.autonomy_setting_speedbump` is borrowed. |

---

## Tier 3 — type resolution and signatures

| file | shape |
|---|---|
| `app/src/ai/blocklist/block/view_impl/output.rs` | `attachment_caps_for_block` — `model.model_id(app)?` → `lookup_byop(app, &llm_id)?`. `LLMId` verified as a re-export (`pub use ai::LLMId`), so the types should be the same; traced, not compiled. |
| `app/src/pane_group/pane/local_harness_launch.rs` | `AgentDriverError::HarnessSetupFailed { harness, reason }` constructed outside its defining module — relies on the enum being fully `pub` (grep-verified). |
| `crates/warp_tui/src/agent_message.rs` | `history.child_conversations_of(parent_id)` — pin usage copied verbatim; lifetime elision through `.iter().position(..)` not cross-checked against this fork's borrow behaviour. |
| `crates/ai/src/index/.../vector_index.rs` | `relative_ranked_paths(&paths, ..)` where `paths: Arc<Vec<PathBuf>>` and the parameter is `&[PathBuf]` — **transitive** deref coercion. Fix if wrong: `.as_slice()`. |
| `app/src/ai/mcp/file_mcp_watcher.rs` | `FileMCPWatcher::new`'s restructure — deferred `initial_config_parses` Vec built before `Self` exists, then looped. Largest structural change to non-test code in the batch. |
| `app/src/ai/skills/file_watchers/skill_watcher_tests.rs` | `RemoteRepositoryIdentifier`/`RemotePath` dual-`HostId` bridging (`warp_core::HostId` vs `warp_util::host_id::HostId`) — two instances fixed by hand. **This fork has two non-converting HostId families; it is a known trap.** |

---

## Tier 4 — test fixtures

Lower blast radius: a broken fixture fails one test file, not a crate. But they
are numerous, and several agents flagged the same class.

- `crates/warp_tui/src/orchestration_model_tests.rs` — new ~180-line file, the most novel test code in the batch.
- `app/src/pane_group/pane/local_harness_launch_tests.rs` — fixture chains PATH lookup, home-dir config writes, and a plugin-install subprocess against a fake CLI.
- `app/src/ai/blocklist/usage/rollup_tests.rs` — `history.conversation_mut(&id).expect(..).set_credits_spent_for_test(..)`.
- `app/src/ai/skills/file_watchers/utils_tests.rs` — `FileMetadata::new(path, false)`; the second parameter's meaning was **assumed, not read**.
- `app/src/terminal/model/terminal_model_test.rs` — `block_size()` import path; `hex::encode` via extern prelude.
- `crates/warp_tui/src/terminal_session_view_tests.rs` — `simulate_long_running_block` producing a block that satisfies `is_eligible_to_tag_in_agent()`.
- `app/src/ai/agent_providers/tools/computer_tests.rs` — 2400×1500 gradient PNG with a Lanczos3 resize; slowest test added.

---

## Known-good, do not re-litigate

Verified by actually running them, not by reading:

- `lib/rust-genai` Anthropic adapter — **compiled and tested**, 17 pass including the new cache-breakpoint guard. It is a separate workspace; `cargo test --manifest-path lib/rust-genai/Cargo.toml` is now in `script/precheck`.
- The earlier six-branch batch — `cargo check --workspace --all-targets --features warp/gui` returned **0 errors** before the hold, and `nextest` returned **6,846 passing**.

That second point is the counterweight to this whole file: the last batch of
unbuilt agent work compiled clean on the first attempt. The predictions here may
well over-state the risk.

---

## Beyond the compiler

Two failures in this batch's lineage were invisible to both `cargo check` and a
green suite, and would be invisible again:

1. **Singleton registration order.** The startup crash was a singleton read 55
   lines before its registration — `initialize_app` compiles fine and 6,000 tests
   passed. Its test-harness echo took three rounds, each fix revealing the next
   missing registration.
2. **Prompt-cache breakpoint placement.** `cache_control` landing on a volatile
   screenshot is valid JSON, a valid API call, and silently more expensive.

Neither is a compile question. **Launching the app is a different act from
building it**, and this batch touches `initialize_app` (codebase-retrieval
wiring), the singleton graph, and the Anthropic request path.
