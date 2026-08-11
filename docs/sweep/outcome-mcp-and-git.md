# Outcome: MCP structured result rendering (item 1) / Zap #329 remainder (item 2)

Task package: two items queued together — (1) MCP tool results render as a raw
JSON blob, (2) hunk-level staging + branch create/switch for Zap #329's
remainder. **Item 2 was reassigned to the coordinator mid-task** (to avoid two
agents colliding on `app/src/code_review/diff_state.rs`,
`app/src/code_review/git_dialog/*`, `app/src/util/git.rs`) and is **not
started** in this worktree — see "Item 2" below.

Base branch: `working`, reset to `390453a94`. Build freeze in effect (no
`cargo`/`rustc`/`nextest`/`script/precheck` in any form) — every claim below is
source comparison against the pin (`02b53fcd8`, cross-checked against the
vendored `rmcp` crate at `~/.cargo/git/checkouts/rmcp-aaacf7b4731e81c8/c0f65dc`
for exact field/method signatures) plus `rustfmt --check` and the two CI
shell-guard scripts, not compilation or test execution.

**Item 1: written, unverified. Item 2: reassigned, not started.**

## Item 1 — MCP tool results render as a raw JSON blob

Premise confirmed real: `app/src/ai/blocklist/inline_action/requested_command.rs`
(pre-change, line ~1494) rendered a finished MCP tool call by calling
`serde_json::to_string_pretty(result)` directly on the whole
`rmcp::model::CallToolResult` wire struct — exposing its `content`/`is_error`/
`meta`/`structured_content` wrapper fields to the user instead of the tool's
actual output.

**Both pin sites read, as instructed, before changing anything:**

- The pin's *structured* renderer, `mcp_result_to_renderable` (pin
  `requested_command.rs:~203`), is only reachable from the
  `FeatureFlag::McpJsonTreeView`-gated branch of the render function
  (pin line ~1668, `if FeatureFlag::McpJsonTreeView.is_enabled() { ... }`).
- The pin's *own* `else` fallback (pin line ~1884) is the exact same raw
  `serde_json::to_string_pretty(result)` call the fork has — i.e. Warp's own
  release-channel users see the same raw blob, because `McpJsonTreeView` is a
  `DOGFOOD_FLAGS`-only flag (`crates/warp_features/src/lib.rs:1006` at the
  pin), never in `PREVIEW_FLAGS` or `RELEASE_FLAGS`.

So the "bug" is real relative to what Warp's *dogfood* build can do, not a
divergence from Warp's public release behavior. Given that, and given the task
brief's explicit instruction to route the render site through the normalizer
either way, this fix ports the normalization unconditionally (no feature flag)
rather than reproducing the pin's dogfood-only gating — every fork user gets
the fixed rendering, not just an internal build.

### What was ported

`app/src/ai/blocklist/inline_action/requested_command.rs`:

- `pub(crate) enum McpRenderable { Tree(serde_json::Value), Error(String),
  Cancelled }` and `pub(crate) fn mcp_result_to_renderable(result:
  &CallMCPToolResult) -> McpRenderable`, ported verbatim from the pin
  (`structured_content` preferred, else joined `RawContent::Text` parts parsed
  as JSON, else wrapped as a JSON string). Verified against the vendored
  `rmcp` source that `CallToolResult::structured_content: Option<Value>`,
  `content: Vec<Content>`, `Content = Annotated<RawContent>` with a public
  `.raw` field, and `RawContent::Text(RawTextContent { text, .. })` all match
  the pin's usage exactly — this fork's `rmcp` git rev
  (`c0f65dc441af7d714b9c453ac5e7ef641451abe3`, `Cargo.toml:439`) is the same
  fork Warp uses, so no adaptation was needed.
- The render site (previously a bare `match result { CallMCPToolResult::... }`
  producing raw text) now calls `mcp_result_to_renderable` and matches on
  `McpRenderable`.

**The `JsonTreeView` check, and what it turned into:** `app/src/ui_components/json_tree.rs`
already existed in this fork — a complete, tested, but *completely unused*
(zero call sites outside its own test file) interactive JSON tree widget
(`render_json_tree`, `JsonTreeState`, `JsonTreeColors`, `PathSegment`). Its
own test file, `app/src/ui_components/json_tree_tests.rs`, had a note (now
removed) saying the pin's 5 `mcp_result_to_renderable` tests were *deliberately
not ported* because doing so "requires also porting the MCP request/response
tree wiring in `requested_command.rs`" — exactly this task. So: yes,
`McpRenderable::Tree` now feeds the existing widget instead of building a new
one. Scope was bounded relative to the pin's full feature, documented below.

**Rendering behavior after this change**, for a finished MCP tool call:
- `McpRenderable::Tree(value)` — request text (unchanged, same as before) above
  an interactive, collapsible JSON tree for the response (new). Expand/collapse
  state is stored per-view in a new `mcp_result_tree_state: JsonTreeState`
  field, so clicks persist across re-renders. Right-click on a row copies that
  node's JSON to the clipboard directly (see "simplified vs. pin" below).
- `McpRenderable::Error(e)` / `McpRenderable::Cancelled` — unchanged flat text,
  now built from the normalized variant instead of the raw match.
- No finished result yet — unchanged (`command_text` when expanded, else
  `mcp_clean_tool_name()`).

New plumbing added to support the tree (all in the same file):
- Three new `RequestedCommandViewAction` variants:
  `ToggleMcpResponseJsonNode { path, depth }`, `ToggleMcpResponseJsonString
  { path }`, `CopyMcpResponseJson(String)`, each with a `handle_action` arm
  that mutates `mcp_result_tree_state` + `ctx.notify()`, or writes to the
  clipboard via `ctx.clipboard().write(ClipboardContent::plain_text(..))` —
  the same idiom already used elsewhere in this file
  (`maybe_copy_on_select`) and across the codebase (`ai_assistant/panel.rs`,
  `editor/view/mod.rs`, etc.).
- One new struct field, `mcp_result_tree_state: JsonTreeState` (derives
  `Default`, initialized as such in `new()`).
- A small helper, `mcp_response_text_element`, factored out of the original
  inline `Text::new(...)` construction so the three non-tree cases (Error,
  Cancelled, no-result-yet) keep the exact same text styling as before.

### Simplified vs. the pin — documented, not silent

The pin's full `McpJsonTreeView` feature is larger than what was ported here.
Differences, each a deliberate scope cut to stay inside what could be
type-checked by hand under the build freeze:

1. **No separate Request tree.** The pin streams MCP call args into a
   `McpRequest { args }` field (`update_mcp_request`, called from wherever the
   tool-call action starts streaming) and renders a second, independent JSON
   tree for the request. This fork has no such streaming hook already wired to
   `RequestedCommandView`, and adding one is a separate, unscoped change to the
   action-streaming path. The existing `command_text` text display for the
   request is kept as-is (this is not a regression — it's the same text the
   fork already showed).
2. **No feature flag.** The pin gates this behind `FeatureFlag::McpJsonTreeView`
   (`DOGFOOD_FLAGS` only). This port makes it unconditional. Rationale: (a) the
   bug being fixed — raw SDK-struct leakage — is fixed either way by
   `mcp_result_to_renderable`, independent of tree-vs-text; (b) gating a
   strictly-better rendering behind a flag that defaults off for every real
   fork user would leave the reported problem *visibly* unfixed for anyone not
   manually flipping a dev flag, which does not match the spirit of "do this
   first." If a future maintainer wants pin-identical gating, add
   `FeatureFlag::McpJsonTreeView` to `crates/warp_features/src/lib.rs`
   (enum + `DOGFOOD_FLAGS`) and wrap the `Some(McpRenderable::Tree(value))` arm
   with `if FeatureFlag::McpJsonTreeView.is_enabled() { .. } else { ..text.. }`.
3. **No right-click context menu.** The pin's `on_copy_json` dispatches
   `ShowMcpContextMenu { json_text, anchor_id }`, which opens a `Menu<..>`
   view positioned at the clicked row, with a "Copy JSON" item the user then
   clicks. Building that menu (a new `ViewHandle<Menu<..>>` field, its own
   `ctx.add_typed_action_view`/`subscribe_to_view` wiring in `new()`, an
   overlay `Dismiss`-wrapped child in the render tree, anchor-position
   plumbing) is a materially larger, independently-verifiable chunk of new
   state that could not be checked without compiling. Right-click now copies
   directly to the clipboard instead — same end result (JSON on the
   clipboard), one fewer click, no confirmation menu. This is a scoped,
   documented simplification of an interaction detail, not the bug being
   fixed.
4. **No scroll/height cap.** The pin wraps the tree in a `NewScrollable` with a
   `MAX_EDITOR_HEIGHT` cap so a huge response can't push the rest of the block
   list off-screen. This port reuses the existing unconstrained `Container`
   wrapper (same as the pre-existing text rendering had). A very large nested
   JSON response can grow the block taller than before was possible with flat
   text truncated by nothing in particular either way — flagged as a follow-up,
   not a regression (the flat-text path had no cap either).

### Tests ported

The pin's 5 `mcp_result_to_renderable` unit tests, added to
`app/src/ui_components/json_tree_tests.rs` (the pin's own location for them,
despite the function living in a different file — matched for consistency):
`mcp_result_success_with_structured_content_returns_tree`,
`mcp_result_success_with_json_text_content_returns_parsed_tree`,
`mcp_result_success_with_non_json_text_returns_string_tree`,
`mcp_result_error_returns_error_variant`,
`mcp_result_cancelled_returns_cancelled_variant`. Import ordering was adjusted
to satisfy this repo's `rustfmt` (uppercase-first ASCII ordering differs from
the pin's own file in a couple of spots); assertions are otherwise verbatim.
The stale comment explaining why these tests were *not* present was removed.

No new tests were written for the tree-widget wiring itself
(`ToggleMcpResponseJsonNode`/`ToggleMcpResponseJsonString`/
`CopyMcpResponseJson` handling, or the `Flex::column` composition in the
render body) because exercising them requires the running UI framework
(`ViewContext`/element tree), which this file's existing test coverage
explicitly does not attempt for `json_tree.rs` either (see that test file's
own header comment: "They do not exercise the element-construction layer").

### Verification performed (no `cargo`, per the build freeze)

- `rustfmt --check` on both touched files: all diff hunks remaining are
  pre-existing import-ordering drift and one pre-existing long-line
  (`log::warn!` at line ~754) that predate this change and are outside it —
  confirmed by re-running `rustfmt --check` before and after each edit and
  diffing the hunk count/content. Every hunk touching lines this change added
  is clean.
- `script/check_cloud_boundary` — ok (270 allowlisted import sites, unchanged).
- `script/check_stub_coverage` — ok (no test targets a gutted stub).
- `script/check_declined_collisions` / `script/check_sweep_ledger` — ok,
  unaffected (this change touches neither `DECLINED.md` nor the ledger).
- Every `rmcp` type/field/method used (`CallToolResult::structured`,
  `::success`, `.structured_content`, `.content`, `Content::text`,
  `Annotated::raw`, `RawContent::Text`, `RawTextContent::text`) was checked
  against the actual vendored source at this workspace's pinned `rmcp` rev,
  not assumed from the pin's usage.
- Every new `warpui`/internal API used (`ViewContext::clipboard`,
  `EventContext` closure signatures for `ToggleFn`/`ToggleStringFn`/
  `CopyJsonFn`, `Flex::add_child`, `Container::with_padding_bottom`,
  `SelectableArea::new`'s `Box<dyn Element>` child type) was cross-checked
  against an existing call site elsewhere in this file or crate, not assumed.
- **Not verified:** actual compilation, test execution, or a running render.
  "Written, unverified" per the build freeze.

### Explicit unfinished list — Item 1

- No `FeatureFlag::McpJsonTreeView` (pin-parity gating) — see simplification 2
  above for the exact wiring if a future pass wants it.
- No Request-side JSON tree (only the Response side is a tree) — see
  simplification 1.
- No right-click context menu; right-click copies directly instead — see
  simplification 3.
- No scroll/max-height cap on the tree — see simplification 4.
- No UI-framework-level test for the new tree wiring (toggle/copy actions,
  the `Flex::column` composition) — only the pure-logic
  `mcp_result_to_renderable` tests were ported, consistent with this file's
  existing test-scope convention.

## Item 2 — Zap #329 remainder (hunk staging, branch create/switch)

**Reassigned to the coordinator mid-task, not started in this worktree.** The
coordinator is doing this directly to avoid two agents editing
`app/src/code_review/diff_state.rs`, `app/src/code_review/git_dialog/*`, and
`app/src/util/git.rs` concurrently. No investigation, code, or design work was
done on this item here — see the coordinator's own output for its status.
