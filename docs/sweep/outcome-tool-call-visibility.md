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

**Why `Text` instead of the block-level `RenderableAction` + red-X-icon widget used
for other failed actions** (found at `app/src/ai/blocklist/block/view_impl/output.rs`,
e.g. the "Failed to read files" case around line 456, and the generic `action_icon`
function around line 3036): that pattern is driven by a real `AIAgentAction` with an
id registered in `action_model`/`AIAgentActionResult` — exactly the pipeline our
carrier deliberately never enters (per the existing `NoClientRepresentation` comment,
entering it would reject the whole conversation update). Wiring a new visible variant
into `AIAgentOutputMessageType` would touch at least 4 more exhaustively-matched
`match` sites across `mod.rs`, `agent_sdk/driver/output.rs`, and
`orchestration_events.rs` that were found by grepping for the enum's last variants
— editing all of them correctly without being able to run `cargo check` was judged
too risky for a build-freeze change. Reusing the already-fully-wired `Text` variant
(and the existing `AIAgentOutputMessage::text` constructor) gets the rejection
visibly in front of the user with no new enum surface and no risk of an unhandled
match arm. **Left for the maintainer**: if pixel parity with the red-X action-card
style is wanted, that requires adding a new `AIAgentOutputMessageType` variant and
updating every exhaustive match on it — doable, but needs `cargo check` to do safely.

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
- `app/src/ai/agent/api/convert_from_tests.rs` (existing file, new tests): the
  `invalid_arguments` marker converts to a visible `Text` message containing the
  tool name, the detail string, and the word "rejected"; the unrelated
  `{"status":"cancelled",...}` `result: None` payload still converts to
  `NoClientRepresentation` (no false positive); and non-JSON `server_message_data`
  doesn't panic the conversion and also stays invisible.

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
- Pixel-parity block rendering (red-X `RenderableAction` card) for the rejected
  tool call, instead of the current plain `Text` message — needs `cargo check` to
  wire safely given how widely `AIAgentOutputMessageType` is matched.
- No compilation or test run was performed (build freeze). All of the above is
  "written, unverified."
