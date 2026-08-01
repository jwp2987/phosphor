# TUI render performance — deferred HIGH findings

Status: **Finding A fixed; Finding B still open.** Captured from the
2026-07-26 parallel performance audit. One sibling finding was already fixed
at the time of the audit (code-block re-clones, commit `448b7b9c`); the two
below were deferred because a correct fix touches independently-changing
state and needs verification in a running TUI, not just a compile.

**Finding A — DONE.** Fixed independently on 2026-07-26 in commit
`e77659f72` ("perf(tui): skip re-cloning unchanged action views each streamed
chunk"), following the proposed approach below almost exactly: added
`TuiPlanView::matches`/`TuiShellCommandView::matches` (the latter's model-
derived state included via a `resolve_presentation` comparison) and wired
them into `sync_action_views` (`crates/warp_tui/src/agent_block.rs:531-548`).
This status line went stale after that fix landed — corrected 2026-08-01
during an upstream-commit review that also checked whether Warp's own
2026-07 render-perf series (`eda008544`/`08ad6e8ab`/`a95e6e541`/`b462e0132`,
a 4-part "TUI: bound clipped viewport painting" / "reuse retained text
measurements" / "clip inline terminal block painting" / "reduce zero-state
animation CPU" stack) fixed either finding. Verdict: no overlap — those four
commits fix different code paths (paint-time viewport clipping, plain
`TuiText` measurement caching, terminal-block output clipping, zero-state
animation), not `agent_block.rs`/`tui_plan_view.rs` (Finding A, already
fixed here independently) or `editor_element.rs`/`char_cell_display.rs`
(Finding B, still open below). Two of the four (`eda008544`'s clip-aware
`TuiPaintSurface` and `08ad6e8ab`'s width-keyed measurement cache) are
low-friction ports if wanted as reusable infrastructure — this fork's
`crates/warpui_core/src/elements/tui/{buffer.rs,clipped.rs,text.rs}` are
untouched since the original TUI import and match upstream's pre-fix shape —
but neither substitutes for Finding B's own fix, which lives one layer up in
the `editor` crate that none of those four commits touch.

These are real per-streamed-chunk / per-frame costs in the ported `warp_tui`
rendering path. They cannot regress the GUI (warp_tui is a non-default workspace
member), but they degrade the TUI during active agent turns.

---

## Finding A — `sync_action_views` re-clones all actions every chunk

**Location:** `crates/warp_tui/src/agent_block.rs:498-702`
**Trigger:** `on_updated_output` (fires once per network chunk during streaming).

Every chunk, the reconciler rescans the whole message list and clones **every**
plan / shell / generic action into per-type `Vec`s before dispatching to the
retained views — O(chunks × Σ action size) over a long tool-heavy exchange.

### Analysis (why it wasn't a drop-in fix like code blocks)

- **Shell (`shell_command_actions`) — safe but low value.**
  `TuiShellCommandView::update_action` (`tui_shell_command_view.rs:208-222`) is a
  pure function of `(action, output_streaming)`: it sets the command-editor text,
  stores `self.action` / `self.output_streaming`, and calls `invalidate_layout`.
  Live command output is reactive from `terminal_model`, not snapshotted here.
  So skipping the update when `(action, output_streaming)` are unchanged is
  correct. But shell action payloads are just the command string (small), so the
  win is marginal.

- **Plan (`plan_actions`) — the real cost, NOT safe to skip naively.**
  `CreateDocuments` / `EditDocuments` carry the large document payloads. But
  `TuiPlanView::sync_action` (`tui_plan_view.rs:90-104`) unconditionally calls
  `sync_documents` → `resolve_presentation(ctx)`, which re-derives per-document
  state from `action_model`. That model state (e.g. per-document approval /
  status) can change **independently of the action**, so skipping on
  action-equality alone would drop model-driven updates and freeze the plan UI.

### Proposed approach

1. Add a cheap accessor to each view mirroring the code-block fix:
   - `TuiShellCommandView::matches(&self, action: &AIAgentAction, streaming: bool) -> bool`
     = `self.action == action && self.output_streaming == streaming`.
   - `TuiPlanView::matches(...)` must also incorporate the **model-derived**
     document state its render depends on. Options:
     a. Have `resolve_presentation` produce a cheap fingerprint (hash of the
        resolved `DocumentActionPresentation` + per-doc statuses) and store it on
        the view; `matches` compares `(action, streaming, fingerprint)`. This
        keeps correctness because the fingerprint captures the model-derived part.
     b. Or drive plan re-sync from an `action_model` subscription (event-based)
        instead of the per-chunk full rescan, so it only re-resolves when the
        model actually changes.
   `AIAgentAction` already derives `Eq`/`PartialEq` (`app/src/ai/agent/mod.rs:899`).
2. In `sync_action_views`, during the message scan, compare the borrowed `&action`
   against the retained view via `matches` and skip the `action.clone()` +
   push for unchanged entries (same borrow shape as the code-block fix, which
   compiled fine). Still record the id in `action_ids` / active set so retain
   logic is unaffected.
3. Leave `ask_question` / `file_edit` / `generic` paths as-is (they already have
   `needs_init` / `contains_key` / `is_blocked` gating and don't clone
   unconditionally).

### Acceptance
- No visible change to plan / shell / generic rendering across a streamed,
  tool-heavy exchange (verified by the snapshot tests below).
- A streaming test asserts an unchanged action does not trigger a view rebuild /
  layout invalidation, and that a model-only change (e.g. doc approval) still
  updates the plan view.

---

## Finding B — full-document rebuild on every layout pass, not viewport-gated

**Location:** `crates/warp_tui/src/editor_element.rs:351-401` (`build`) +
`crates/editor/src/render/model/char_cell_display.rs:257-334` (`display_rows` /
`display_lattice`).
**Trigger:** `layout()` calls `build()` unconditionally; any animated element in
the tree (e.g. the shimmer "thinking" indicator, ~10 Hz) makes the presenter
(`presenter/tui.rs:190` `arrange`) re-run `layout()` over the whole retained
tree — so a visible large code block / diff is fully re-scanned ~10×/second.

### Analysis (why `build()` can't just be memoized)

`build()` has essential per-layout **side effects** that must run every pass:
- `render_state.try_layout_pending_edits(app)` — flushes queued buffer edits into
  the char-cell index; without it, typed text never renders.
- `char_cell.set_terminal_width(content_width)` and
  `follow_cursor` / `clamp_scroll_offset` — scroll/width state.

So a naive "cache `build()` output and skip" breaks editing and scrolling. The
expensive part is specifically the projection: `text.chars().collect()` (line
382) and `char_cell.display_lattice(&hidden)` (line 401), which walks **every**
logical line even when `self.viewport_rows` is `Some` — and the code-block / diff
views never set `viewport_rows`, so nothing is windowed.

### Proposed approach

1. **Separate side effects from the projection.** Keep the edit-flush / scroll /
   width mutations running every `build()`. Cache only the pure projection
   output (the `display_lattice` rows + derived spans) keyed on
   `(text_version, content_width, hidden_ranges, scroll_offset, syntax_version,
   cursor, selection)`. Reuse the cache when the key is unchanged; recompute when
   any component changes. `text_version` should come from a monotonic buffer
   revision, not a full text compare.
2. **Viewport-window `display_lattice`.** Make the projection honor
   `viewport_rows` (build only the visible row window plus a small overscan)
   instead of projecting the whole buffer, and have the code-block / diff views
   pass a viewport. This is the deeper win for very large blocks; it lives in
   shared `crates/editor` code, so it must not regress the GUI editor — gate or
   verify against the GUI render path.
3. Consider whether animated elements should force a **paint-only** pass rather
   than a full `layout()` on unchanged siblings (see `elements/tui/animated.rs`
   doc comment) — a presenter-level fix that would help beyond the editor.

### Acceptance
- Editing (typed text appears), scrolling, cursor, and selection are unchanged in
  the running TUI.
- Cell-exact rendering tests still pass for code blocks, diffs, and the input box.
- A benchmark / instrumentation shows `display_lattice` work is bounded by the
  viewport (not total buffer) for windowed views, and unchanged siblings are not
  re-projected under a 10 Hz shimmer.

---

## Test strategy (shared)

`warp_tui` already has a strong harness: 525 lib tests including **cell-exact**
rendering assertions (e.g. `fenced_markdown_code_block_applies_syntax_colors_to_exact_cells`,
`renders_read_only_code_with_language_and_wrapping`) and an interactive PTY
harness. Plan:

1. **Correctness (snapshot):** add streaming tests that push a sequence of
   `on_updated_output` updates (growing text, a finished tool call, a
   model-only status change) and assert the rendered cells are identical
   before/after the optimization.
2. **No-redundant-work (behavioral):** assert that an unchanged update does not
   rebuild a child view / invalidate layout (e.g. via a counter or an emitted
   event), proving the skip fires.
3. **Manual:** run `cargo run -p warp_tui` (or `zap-tui-oss`) against a BYOP
   endpoint and eyeball a long streamed response with a large code block + an
   active shimmer, before/after.

Pre-existing suite caveat: `transcript_clear_event_removes_only_named_conversations`
and `agent_hint_tracks_transcript_emptiness_without_input_invalidation` fail on
the current baseline (test-isolation flakiness in the ported suite) — fix or
quarantine separately so they don't mask regressions here.

## Sequencing
Finding A is done (see status above). Remaining: Finding B's cache (step 1,
self-contained in `editor_element.rs`, biggest per-frame win under the
shimmer) first, then Finding B's viewport-windowing (shared `crates/editor`,
highest blast radius — needs GUI-safety verification), last. Consider
porting upstream's `eda008544`/`08ad6e8ab` (see status note above) as
groundwork before or alongside step 1 — they're independently useful and
low-risk, though they don't fix Finding B on their own.
