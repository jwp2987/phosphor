# Outcome: tool-call parse failures must not be silent

Task package: two related fixes to one root cause class — a purely human-facing,
`required` field in a BYOP tool's parser can sink an entire tool call before any
file/action logic runs, and a rejected tool call had no trace in the UI at all.
Triggering incident: asking the agent to create `hi.txt` did nothing — no file, no
error, no prompt — because `apply_file_diffs`'s `summary` field was missing from the
model's payload and `serde_json::from_str` died on it before any file logic ran.

Base: reset to `origin/main` at `206799556` (feat(usage-suite): run real-shell
scenarios by DEFAULT). Build freeze in effect (no `cargo`/`rustc`/`nextest` — the
maintainer owns builds). Everything below is source review plus `rustfmt --check`
and the pure-shell guard scripts, not compilation or test execution. **Written,
unverified.**

## Fix 1 — `summary`/`action_summary`/`task_summary` must not be able to kill a call

Confirmed the premise directly: `app/src/ai/agent_providers/tools/edit.rs`'s `Args`
struct had `summary: String` with no `#[serde(default)]`, so any payload missing it
failed `serde_json::from_str` inside `from_args`, before the file-diff logic, the
permission check, or the diff view ever ran. Also confirmed the maintainer's
correction was right: the operative field name is `content` (schema `:74`, parser
`:38`), not `contents` — left untouched.

**What changed** (`edit.rs`):

- `Args::summary` is now `Option<String>` with `#[serde(default)]`. `from_args`
  treats a present-but-blank string (`"   "`) the same as absent, via
  `.filter(|s| !s.trim().is_empty())` — a model that sends `""` gets the same
  fallback treatment as one that omits the field entirely.
- Added `fallback_summary(operations: &[Operation]) -> String`, which derives a
  one-line description from the operation list: `"Create hi.txt"` /
  `"Edit README.md"` / `"Delete old.txt"` for a single op, or
  `"Create 2 files, edit 1 file, delete 1 file"`-style summaries for a batch.
- `parameters()`'s advertised JSON Schema still lists `summary` in `required`
  (comment added explaining why: guidance for well-behaved models, while the parser
  stays forgiving of bad ones — the schema and the parser are allowed to diverge on
  purpose).

**Audit of the other tools** in `app/src/ai/agent_providers/tools/` for the same
shape (a required, purely human-facing field whose absence kills a real operation):

| file | field | same shape? | action |
|---|---|---|---|
| `computer.rs` `UseComputerArgs` | `action_summary: String` | **yes** — "shown to the user describing what this batch does"; a missing value previously killed the entire mouse/keyboard action batch | fixed, same treatment as `summary` |
| `computer.rs` `RequestComputerUseArgs` | `task_summary: String` | **yes** — "shown to the user... so they can decide whether to approve it"; a missing value previously killed the whole computer-control permission request | fixed — no operation list to derive a fallback from, so it falls back to a fixed sentence (`FALLBACK_TASK_SUMMARY`) instead |
| `markers.rs` `TransferArgs::reason` | already `#[serde(default)]` | no — already forgiving | none needed |
| `suggest.rs` `PromptArgs::label` | already `#[serde(default)]` | no — already forgiving | none needed |
| `ask.rs` `QuestionArg` (`recommended_index`/`multi_select`/`supports_other`) | already `#[serde(default)]`; `question`/`options` are operational, not decorative | no | none needed |
| `documents.rs` `NewDoc::title` | required, no default | borderline — `title` is the document's display name in Drive, closer to an identifying property (like a filename) than a pure approval-flow decoration such as `summary` | **left as-is**; flagged for the maintainer to decide if this should get the same treatment |
| `files.rs`, `search.rs`, `shell.rs`, `long_shell.rs`, `skill.rs`, `mcp.rs` | no purely-decorative required field found | no | none needed |

`computer.rs` changes mirror `edit.rs` exactly: `action_summary`/`task_summary`
became `Option<String>` in the parser, the schema keeps them `required`, and both
`from_args` functions synthesize a fallback (`fallback_action_summary` derives from
the action kinds; `request_computer_use_from_args` falls back to a fixed sentence
since there is no per-call operation list to summarize before the model is even
allowed to act).

## Fix 2 — a rejected tool call must be visible

Traced the existing silent path: when BYOP's `from_args` fails,
`chat_stream::parse_incoming_tool_call`'s `Err` arm (already, pre-existing) emits a
carrier `ToolCall(tool: None)` plus a synthetic `ToolCallResult(result: None)` whose
`server_message_data` holds
`{"error":"invalid_arguments","detail":...,"tool":...,"received_args":...,"hint":...}`.
That JSON already reaches the model (so it can retry — `controller.rs`'s
`needs_byop_local_resume` already schedules an auto-resume off this exact marker).
But on the client-conversion side
(`app/src/ai/agent/api/convert_from.rs`), **both** messages mapped to
`MaybeAIAgentOutputMessage::NoClientRepresentation`:

- The carrier `ToolCall(tool: None)` — deliberately, per the existing comment at
  `convert_from.rs`'s `to_action`, to avoid rejecting the whole conversation update.
- The `ToolCallResult` — via a catch-all arm that also covers unrelated message
  kinds (`UserQuery`, `CodeReview`, etc.).

Net effect: the user saw nothing. No card, no toast, no sign a tool call was ever
attempted or rejected — the second report of this exact silent-failure shape (the
first being `hi.txt`).

**What changed** (`convert_from.rs`):

- Split `ToolCallResult` out of the catch-all arm into its own match arm.
- Added `invalid_arguments_display_text(server_message_data: &str) -> Option<AIAgentText>`,
  which recognizes the `{"error":"invalid_arguments",...}` marker specifically (not
  any other `result: None` payload — e.g. the unrelated `{"status":"cancelled",...}`
  shape `BlocklistAIController::byop_synthetic_cancellation_message` writes for a
  user-interrupted command keeps falling through unchanged) and turns it into a
  short markdown message: `**Tool call rejected: `<tool>`**` followed by the
  `detail` string and a note that a retry was requested.
- When the marker is present, the `ToolCallResult` now converts to a visible
  `AIAgentOutputMessageType::Text` message via the existing `AIAgentOutputMessage::text`
  constructor. Model-facing behavior is unchanged — the JSON still goes back to the
  model for the retry exactly as before; this is additive.

**Update (follow-up task, same build-freeze discipline): this styling gap is now
closed.** A rejected tool call no longer renders as a `Text` paragraph — it's a new
`AIAgentOutputMessageType::RejectedToolCall { tool: Option<String>, detail: String }`
variant (`app/src/ai/agent/mod.rs`), produced by `convert_from.rs`'s
`invalid_arguments_rejected_tool_call` (renamed from `invalid_arguments_display_text`,
same recognition logic, now returns the raw `(tool, detail)` pair instead of
pre-rendered markdown) via the new `AIAgentOutputMessage::rejected_tool_call`
constructor.

*Reuse vs. new variant, decided explicitly*: every existing "failed action" affordance
in the codebase (`AIAgentActionType`/`AIAgentActionResultType` and all their per-tool
UI copy, e.g. "Calling X MCP tool...") is tied to a *specific, successfully-parsed*
tool call with a real action id registered in `action_model` — exactly what our
carrier never has (parsing failed before any of that existed). Reusing one of those
variants (`CallMCPTool` was the closest candidate — generic name+JSON, but its UI
literally says "MCP tool", which would misdescribe a native BYOP tool) would have
meant contorting an existing variant to mean something it doesn't. `RenderableAIError`
was also rejected: it's a conversation-*terminal* failure, and a rejected tool call
isn't terminal — the model is still expected to retry. No honest reuse existed at the
data-model level, so a new variant was added — but its *rendering* reuses existing
failure-styled primitives rather than inventing new widgets: the GUI renders it via
`RenderableAction` + `inline_action_icons::red_x_icon` (the same red-X row already used
for other failed actions in `output.rs`, e.g. "Failed to read files"), and the TUI
converts it straight into `TuiAIBlockSection::Failure(FailedOutputPresentation::Message(...))`
— the exact section type and error styling `crates/warp_tui/src/agent_block.rs` already
uses for a failed exchange. A shared `rejected_tool_call_text(tool, detail) -> String`
helper (`app/src/ai/agent/mod.rs`) keeps the phrasing identical across every
renderer/serializer of the variant.

Match sites updated (7 were compile-mandatory — exhaustive matches on
`AIAgentOutputMessageType` with no wildcard arm; 2 more were edited for real behavior
even though a `_ => ()`/`_ => (false)` wildcard would have compiled without them):
`app/src/ai/agent/mod.rs` (`format_for_copy`, `Display for AIAgentOutputMessage`, plus
the new constructor), `app/src/ai/agent_sdk/driver/output.rs` (`format_output` in
`pub mod text`, `from_output_message` in `pub mod json` — the latter maps it onto the
pre-existing `JsonMessage::ToolError` shape, another honest reuse), and
`app/src/ai/blocklist/orchestration_events.rs` (no-op arm — this message never
originates from another agent). The two non-mandatory renderers touched:
`app/src/ai/blocklist/block/view_impl/output.rs` (main GUI renderer) and
`app/src/ai/blocklist/block/cli.rs` (the CLI-subagent status panel — a secondary
renderer that mirrors the main one). `app/src/tui_export.rs` re-exports
`rejected_tool_call_text` to `warp_tui`.

*Persistence*: confirmed safe. `AIAgentOutputMessageType`/`AIAgentOutput` never derive
`Serialize`/`Deserialize` and are never round-tripped through disk — conversations
persist as the raw `api::Task` protobuf (`crates/persistence/src/agent.rs`), and
`AIAgentOutputMessageType` is rebuilt fresh on every load via the same
`to_client_output_message` conversion used for live streaming
(`app/src/ai/agent/task.rs`'s `into_exchanges` → `convert_from.rs`). A conversation
saved by one build loads fine in another regardless of which build recognizes this
variant, because the variant itself is never what's on disk.

A toast (`ToastStack`/`DismissibleToast`, per the alternative the task allowed) was
considered and not used: `BlocklistAIController::handle_response_stream_event` (the
natural call site — it already detects this exact marker for the auto-resume logic)
runs under `ModelContext<Self>`, not a `ViewContext`, and never handles a
`WindowId` anywhere in the file; every existing `ToastStack::handle(ctx)` call site
in the codebase runs under a `ViewContext` and derives its `window_id` from
`ctx.window_id()`. Wiring a window id through from a model-level controller looked
like more incidental surface area than the block-text approach for the same result.

## Tests

- `app/src/ai/agent_providers/tools/edit_tests.rs` (new file, wired via
  `#[cfg(test)] #[path = "edit_tests.rs"] mod tests;` at the bottom of `edit.rs`,
  matching `computer.rs`'s existing pattern): missing `summary` gets a derived
  fallback (single-op and mixed-batch cases), a blank `summary` is treated as
  absent, a provided `summary` is kept verbatim, and — for the "still fails loudly"
  half — a missing `operations` field and non-JSON garbage both still error out of
  `from_args`.
- `app/src/ai/agent_providers/tools/computer_tests.rs` (existing file, new section):
  the same shape of tests for `action_summary` (missing/blank/multi-action
  fallback/provided) and `task_summary` (missing/provided), plus confirming
  `actions` and genuinely malformed JSON still fail to parse.
- `app/src/ai/agent/api/convert_from_tests.rs` (existing file, tests extended/added
  in the follow-up styling task): the `invalid_arguments` marker converts to an
  `AIAgentOutputMessageType::RejectedToolCall` carrying the tool name and detail
  structurally (not just embedded in prose), and the shared `rejected_tool_call_text`
  rendering still says "rejected"; a marker missing a `tool` field produces
  `tool: None` rather than panicking; the unrelated `{"status":"cancelled",...}`
  `result: None` payload still converts to `NoClientRepresentation` (no false
  positive, re-verified against the new variant); and non-JSON `server_message_data`
  doesn't panic the conversion and also stays invisible.
- `crates/warp_tui/src/agent_block_tests.rs` (new test,
  `agent_block_renders_rejected_tool_call_as_a_failure_section`): asserts the
  structural shape (`sections()` produces exactly
  `TuiAIBlockSection::Failure(FailedOutputPresentation::Message(rejected_tool_call_text(...)))`,
  not a `RichText` paragraph) and the visible rendering (the tool name and detail
  appear in the rendered lines, and the row carries the same red error color as
  `agent_block_renders_generic_failure_after_partial_output`'s exchange-level
  failure).

## Checks run

- `rustfmt --check` on every touched/added file. Each file reports *some* diff, but
  in every case the diff is on pre-existing `use` lines I did not touch (import
  ordering — e.g. `{json, Value}` vs `{Value, json}`, or `util::...` sorting before
  vs after PascalCase imports), never on a line I added. This matches
  `AGENTS.md`'s warning that the local `rustfmt` build churns untouched files; I did
  not "fix" that drift, since doing so would touch unrelated code outside this
  task's scope. Confirmed by diffing the reported line ranges against `git diff` for
  each file — none overlap with my additions.
- `script/check_stub_coverage` — pass (`stub coverage: ok`).
- `script/check_settings_registry` — pass (`settings registry: ok, 50 groups`).
- `script/check_cloud_boundary` — pass (`cloud boundary: ok, 270 allowlisted import sites`),
  run as a bonus sanity check since it's the other pure-shell guard CLAUDE.md calls out.
- `script/check_dangling_modules` — **does not exist in this tree** (checked
  `script/` and grepped the repo for any reference to the name; nothing found).
  Not run, since it isn't there to run. Flagging this rather than fabricating a
  result — the task description asked for it but the tree doesn't have it.

## Unfinished / left for the maintainer

- `documents.rs`'s `NewDoc::title` (borderline case — see the audit table).
- Pixel-parity block rendering for the rejected tool call **landed** in the
  follow-up styling task (see "Update" above) — `RejectedToolCall` +
  `RenderableAction`/red-X icon in the GUI, `TuiAIBlockSection::Failure` in the TUI.
  Every exhaustive match on `AIAgentOutputMessageType` was found by grep (24 files
  reference the enum; 7 sites were exhaustive matches with no wildcard arm, all
  updated) and cross-checked again after the edits — no `match` sites were missed.
  Still build-freeze discipline throughout: no compilation or test run was
  performed for that task either. All of the above remains "written, unverified."
