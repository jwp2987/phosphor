# Build triage — the ~70 unverified commits of 2026-08-10/11

**Purpose.** Every agent that landed code in this batch was forbidden from
running `cargo`, and each was required to hand back a ranked list of what it
doubted would compile. This file collates all of them, grouped by file, so the
first build is worked through in a planned order rather than reactively.

**Status: RESOLVED — the batch was built on 2026-08-10.** The predictions below
are kept verbatim, with a verdict added to each. See "Result" immediately below
for the scoring; the per-tier verdicts are inline.

---

## Result — what the first build actually found

`cargo check --workspace --all-targets --features warp/gui`, from a warm cache:

| | |
|---|---:|
| Predictions in this file | 16 |
| Predictions that fired | **0** |
| Distinct compile errors | **5** |
| Errors any prediction anticipated | **0** |
| Files correctly named as risky (wrong reason) | 2 |

**Every prediction missed, and every error was unpredicted.** The two are not
the same failure: the predictions were not merely low-yield, they had no overlap
with reality at all. All 16 predicted shapes — the prost naming, every
borrow-checker region, every type-resolution and deref-coercion question —
compiled exactly as the agents reasoned they would.

What actually broke was a single class the file never names:

> **Calling a fork API through the pin's signature.** Four of five errors are a
> test calling a function whose arity, receiver type, or member kind differs
> here from `02b53fcd8`. Not one agent listed "the signature I copied might not
> be this fork's signature" as a doubt.

The fifth is the same shape one step removed: a test asking for `{:?}` on a type
that had never needed `Debug` because nothing had ever printed it.

**Direction of the error, and why the miss rate is not the lesson.** Agents were
uncertain about the code they *wrote* and confident about the API they *called*.
They had it backwards. Hand-tracing the hard thing worked — 16 for 16 — and the
unexamined assumption was the cheap call site nobody thought to check. Next
round, the ranked-doubt list is worth less than a mechanical diff of every
pin-derived call site against this fork's signatures; the second is a grep, and
it would have found four of these five before the build.

The counterweight in "Known-good" below is therefore right on its own terms and
still misleading: 5 errors across ~80 unbuilt commits is a low rate, but they
clustered entirely in the one place nobody was watching.

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

> **MISSED — the inference was exactly right.** The generated enum is
> `Unspecified / NotEnabled / InvalidRepoPath / IndexNotFound / IndexSyncing /
> IndexFailed / RetrievalFailed`, matching all 30-odd call sites across the three
> files. Reasoning-by-analogy from `FragmentMetadataLookupErrorCode` was sound:
> prost strips the SCREAMING_SNAKE enum-name prefix, and the proto had spelled
> every variant with the full prefix. The highest-ranked risk in the batch cost
> nothing.

### `crates/remote_server/src/manager.rs`
**Prediction: `HostRequestHandle` gaining `#[derive(Clone)]`.** Depends on
`ModelSpawner<M>`'s manual `impl<M> Clone` (verified unconditional) and
`warp_core::HostId: Clone` (verified derived). Believed fine, unverified.

### `app/src/ai/blocklist/usage/conversation_usage_view.rs`
**Prediction: `Text::new(..).with_color(..).soft_wrap(false).with_clip(ClipConfig::ellipsis()).finish()`.**
Method names verified individually elsewhere; *this exact chain order* was not.

> **MISSED (both tier-1 remainders).** `HostRequestHandle`'s `#[derive(Clone)]`
> and the builder chain both compiled untouched.

---

## Tier 2 — borrow-checker shapes

These are the densest regions written blind. All were traced by hand against an
existing identical pattern, which is why they are tier 2 and not tier 1 — but
NLL surprises are exactly what hand-tracing misses.

> **ALL SIX MISSED. Zero borrow-checker errors in the entire batch.** Including
> `force_reload_server_local`, nominated as the densest borrow region written
> blind. Hand-tracing a borrow region against an existing identical pattern is,
> on this evidence, a reliable substitute for the compiler — it did not fail
> once in six attempts at the hardest cases the agents could nominate.

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

> **ALL FIVE MISSED.** The `LLMId` re-export was the same type, the
> `AgentDriverError` variant was public enough, `child_conversations_of`'s
> lifetimes elided, `Arc<Vec<PathBuf>>` did coerce transitively to `&[PathBuf]`
> without `.as_slice()`, and the `FileMCPWatcher::new` restructure — the largest
> structural change to non-test code in the batch — compiled as written.

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

> **The only tier that scored, and only by accident.** Two of the seven files
> below did contain errors, but neither for the reason given:
>
> - `orchestration_model_tests.rs` — flagged for being novel. It broke on two
>   pin-vs-fork signature mismatches: `add_test_semantic_selection` takes
>   `&mut AppContext`, not `&mut App`, and `start_new_conversation` takes four
>   arguments here where the pin passes five.
> - `terminal_session_view_tests.rs` — flagged for `simulate_long_running_block`
>   producing a taggable block. That part was fine; the file broke because
>   `session_state` is a *field* holding a `TuiTerminalSessionStateModel`, not a
>   method, so resolving it goes through `.resolve(ctx)`.
>
> The remaining five compiled, including the two whose risk was semantic rather
> than syntactic. `utils_tests.rs`'s assumed second parameter to
> `FileMetadata::new` is `ignored: bool`, and `false` — "not gitignored", so the
> skill file is discoverable — is what those tests want. A compiler could not
> have caught that one; it was checked by reading the struct.
>
> Note the blast-radius model held: both failures were confined to one crate's
> test target and neither touched shipping code.

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

> **Checked by reading, not by running — still outstanding.** The singleton
> graph was audited against this batch's two additions and both order
> constraints hold, each with the reason recorded at the registration site:
> `NotificationsModel` is registered after `BlocklistAIHistoryModel` and
> `CLIAgentSessionsModel`, and `CodebaseRetrievalController` after
> `CodebaseIndexManager`, which it subscribes to during construction on
> `local_fs`.
>
> That is an argument, not evidence. **Nobody has launched the app since the
> freeze lifted.** A green suite does not discharge this item — the startup
> crash it describes passed 6,000 tests. Manual launch remains the open task.

---

## Round 2 — the genai 0.7 bump (2026-08-11)

A second batch of unverified code landed on top of the batch above: the
vendored `lib/rust-genai` was re-ported `0.6.0-beta.18` → `0.7.0-beta.18` and
merged before the first batch had finished verifying. Four app-side
adaptations were applied by hand and never compiled.

**The prediction carried into this round was that it would produce more of the
same class — fork APIs called through the pin's signature. It did not.**

| | |
|---|---:|
| Hand-applied adaptations, uncompiled | 4 |
| That were correct as written | **4** |
| Additional compile errors found | **0** |
| Test failures found | 2 |
| Test failures attributable to the genai bump | **0** |

`cargo check --workspace --all-targets --features warp/gui` returned 0 errors
on the first attempt. All four adaptations — the `=0.7.0-beta.18` pin,
`ReasoningEffort::None` → `Zero` in the settings conversion and its test, and
`ChatStreamEvent::Heartbeat` added to the oneshot match — were right. The two
further 0.7 changes flagged as risk (`Tool::custom_format`, opt-in prompt
caching) touched nothing: the app builds its genai `Tool` through
`Tool::new(..)`, which fills the new field, and it never constructs the struct
literally.

One near-miss worth recording, because it went the opposite way to Round 1's
lesson. The other exhaustive `ChatStreamEvent` match — the real streaming path
in `chat_stream.rs`, far more load-bearing than the oneshot one — was *not*
updated and did not need to be: it ends in `_ => {}`. The compiler would never
have raised it, and a new variant it silently swallows is exactly the shape
that a catch-all hides. Here ignoring a keepalive is correct, so the catch-all
happened to be right. It would not have announced itself if it were wrong.

### What actually broke: a rename applied to one of its two call sites

Both failures came from a different unverified commit in the same window
(`a4ebf6876`, the SSH-install fix), and neither was a compile error:

- `download_tarball_url` still formatted a literal `zap-` prefix after the
  commit introduced `RELEASE_ASSET_PREFIX` and threaded it through the install
  script. The constant and the literal type-check identically, so only a test
  comparing the built URL could see the disagreement — and that path,
  `ssh_transport.rs`, is the one a real user goes through.
- The install-script test asserted the old asset name, i.e. the behaviour the
  commit deliberately changed.

**The generalised lesson, now twice-confirmed in different forms.** Round 1:
the danger was not the code agents wrote but the API they assumed. Round 2: the
adaptations to a genuinely-changed API were all correct, and the defect was a
rename that reached the template but not the function beside it. Both rounds
failed at the *unexamined second call site*, not at the hard change. The
compiler caught Round 1's version of that and could not catch Round 2's.

Rounds 1 and 2 together: **20 predictions, 0 fired.** Predicting which written
code will fail to compile has now returned nothing twice. Enumerating every
call site of a symbol being renamed would have caught the only real defect in
this round, in seconds.
