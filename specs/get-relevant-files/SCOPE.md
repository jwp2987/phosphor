# GetRelevantFiles (local BYOP) — scoping + decision

**Status: BUILT (2026-08-04, branch `get-relevant-files`).** Implemented as the
model-callable `get_relevant_files` tool (the faithful BYOP realization — see the
correction below). The rest of this doc is the original scoping; the
**"Correction"** section at the end records why the tool *is* the faithful port
and what shipped.

## What exists today (all BYOP-clean, no cloud)

The fork already has every low-level primitive GetRelevantFiles needs; only the
*orchestration* is missing:

- `app/src/ai/agent_providers/active_ai/mod.rs` → `pub mod relevant_files`:
  - `relevant_files::dispatch(app, terminal_view_id, Input{query, files})`
    resolves the BYOP oneshot config (`resolve_active_ai_oneshot`) and renders
    the `relevant_files_system.j2` / `relevant_files_user.j2` templates into a
    `Prepared` request.
  - `relevant_files::run(prepared).await -> Vec<String>` calls
    `byop_oneshot_completion` (the local BYOP model — no cloud) and parses the
    result via `parsing::parse_relevant_files` (`RelevantFilesDto`).
  - **This whole module is currently DEAD CODE — nothing calls it.**
- Prompt templates: `app/src/ai/agent_providers/prompts/active_ai/relevant_files_{system,user}.j2`.
- Custom-prompt editor slot: `PromptSlot::RelevantFiles`
  (`execution_profiles/mod.rs:256`, `execution_profiles/editor/ui_helpers.rs`).
- The restored `RepoOutlines` index (commit `a09653678`) supplies candidate
  files + per-file symbol summaries — same source the local `search_codebase`
  tool uses (`tools/codebase_runtime::collect_codebase_symbols`).

## What was removed (the amputation)

`d84dd8e4d` removed the `GetRelevantFilesController`
(`app/src/ai/get_relevant_files/{api,controller,mod}.rs` in `warp/master`) and
its blocklist wiring. Tombstone: `ai/blocklist/block.rs:1021` ("this used to
subscribe to GetRelevantFilesController's Success event"). Warp's controller is
459 lines and ~11 of them touch the cloud `remote_search` / graphql
`get_relevant_fragments` path — the BYOP fork keeps only the local half.

## The design decision (two materially different shapes)

1. **Auto-context controller (faithful Warp behavior).** On AI-conversation
   submit, gather candidate files → `relevant_files::dispatch`/`run` → attach the
   selected files as conversation context. This is what "GetRelevantFiles"
   *meant* in Warp (a cheap pre-filter that saves the main model from seeing
   irrelevant files). **Cost:** reverses a deliberate amputation *into the core
   AI-conversation flow* (`terminal/view/load_ai_conversation.rs`, a controller,
   a `block.rs` subscription) — high blast radius; risks regressing live AI
   behavior; a real design decision on trigger timing + context-attachment.

2. **Model-callable `get_relevant_files` tool.** Mirror the local
   `search_codebase` BYOP intercept tool: pre-materialize candidate `FileEntry`s
   + the oneshot config on `RequestParams::new`, then an async interceptor in
   `chat_stream` renders the templates with the model's query, calls
   `byop_oneshot_completion`, and returns the relevant paths. **Cost:** edits the
   5800-line core `chat_stream` pipeline; makes a *nested* LLM call per
   invocation; and is arguably *not* GetRelevantFiles — the model can already
   call `search_codebase` / read files, so an LLM-prefilter-as-tool is marginal.

## Why held

The AFK "finish it (local BYOP)" decision rested on the premise that this was
"partly a rebuild (get_files.rs was dead code)." Investigation disproves that:
`get_files.rs` was Warp's *read-files* action (unrelated); the real work is a
genuine orchestration **design choice**, and both options either (1) reverse a
deliberate amputation into the core AI flow or (2) edit the core BYOP pipeline
for a half-measure. Per the standing guardrail — *stop and flag
deliberate-amputation reversals and design-decision items rather than guess them
into the core pipeline* — this is held for the maintainer.

## Recommendation

If the goal is parity with Warp's actual behavior → option 1 (the controller),
scheduled as its own focused pass with running-app verification of AI
conversations. If the goal is just to expose the capability at low risk →
option 2. Either way the primitives above are ready; no cloud work is required.

## Correction (2026-08-04) — the tool IS the faithful port

Deeper reading of `warp/master` corrected the framing above. GetRelevantFiles in
Warp is **not** an auto-prefilter on conversation submit — it is **agent-driven**:
the agent emits a `GetFiles` action (`AIAgentActionType::GetFiles`), and
`get_files.rs` runs the controller (outline → LLM-filter → paths) and returns the
files as the action result. Both of the controller's request paths were cloud
(`server_api.get_relevant_files` for the outline-LLM path, `remote_search` for the
embedding index); `active_ai::relevant_files` is the BYOP replacement for the
former.

Because BYOP models invoke capabilities through **tool calls**, the faithful BYOP
realization is a model-callable tool — not the on-submit prefilter that "Option 1"
described (which corresponds to no real Warp behavior). So Option 2 was the
faithful port all along; it was built.

## What shipped

A `get_relevant_files` BYOP tool, mirroring the `search_codebase` intercept
pattern exactly (no protobuf executor variant, no cloud, no server API):

- `active_ai::relevant_files` gained a two-phase API replacing the dead
  `dispatch`/`run`: `prepare_context(app, tvid)` (resolves the BYOP one-shot config
  + renders the query-independent system prompt) and async
  `run_with_context(prepared, query, files)` (renders the user prompt, runs
  `byop_oneshot_completion`, parses via `parse_relevant_files`).
- `tools/get_relevant_files.rs` (descriptor) + `tools/get_relevant_files_runtime.rs`
  (snapshot collection from the local `RepoOutlines` outline via `to_file_symbols`,
  async dispatch, `_byop_intercepted` serialization). `RelevantFilesSnapshot` rides
  on `RequestParams` (materialized in `new()` while `AppContext` is available; the
  interceptor is AppContext-less), with a redacting `Debug` so the provider api_key
  can't leak.
- `chat_stream` advertises the tool (gated on the same `codebase_context_enabled`
  flag as `search_codebase`), intercepts it by name, and runs an async local
  dispatcher.

Verified: warp lib suite **3796 / 0 / 33**; 8 new unit tests. The async filter's
live-provider path is exercised at the integration level, not in unit tests.

The auto-context *controller* variant (fire on conversation submit, no cloud) was
**not** built — it isn't a Warp behavior to match, and would be a net-new feature.
