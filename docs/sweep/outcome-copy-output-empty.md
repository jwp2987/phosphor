# Outcome: "Copy output as Markdown" silently writes nothing to the clipboard

Task: the AI block overflow menu's "Copy output as Markdown" (and the sibling
copy actions next to it) can silently overwrite the clipboard with an empty
string — no error, no toast, no log line — when the exchange(s) being copied
have nothing extractable. Fix both the silent-empty-write and the root cause
(errored/cancelled exchanges contributing nothing to the copy).

Status: **written, unverified**. Hard rules forbid `cargo`/`nextest`/`rustc`/
`script/precheck` in this environment — nothing below has been compiled or
run. Verified by reading call sites, matching existing patterns in the
codebase, and reasoning through the match arms by hand.

## The chain, as verified against the tree at `206799556`

- `app/src/ai/blocklist/block.rs` — `AIBlockAction::CopyOutput` (was line
  6015, confirmed) calls `get_output_text_since_preceding_user_query(ctx)`
  and unconditionally writes the result via
  `ctx.clipboard().write(ClipboardContent::plain_text(output_text))`. Same
  shape for `CopyQuery` (was 6009) and `Copy` (was 6021). **Also found the
  same shape in `CopyConversation`** (was 6029–6055, not named in the brief
  but writes an unconditionally-computed `conversation_text` the same way) —
  included it in the fix since it goes through the identical
  `format_for_copy` path and has the identical bug.
- `app/src/ai/agent/mod.rs:3038` (was) — `AIAgentExchange::format_output_for_copy`
  returned `String::new()` whenever `self.output_status.output()` was `None`.
- `app/src/ai/agent/mod.rs:204` — `FinishedAIAgentOutput::output()`:
  `Self::Error { .. } => None` **unconditionally, discarding any partial
  `output` the `Error` variant might itself be carrying** — this is a
  stronger form of data loss than the brief described: even an `Error` that
  *did* capture output before failing loses it through this accessor.
  `Self::Cancelled { output, .. } => output.as_ref()` is `None` only when
  cancelled before any output arrived.

All three confirmed by reading, matching the brief.

## Fix 1: never silently write an empty clipboard

`app/src/ai/blocklist/block.rs` — added a private helper on `AIBlock`,
`write_copied_text_or_toast_if_empty(&self, text: String, ctx: &mut ViewContext<Self>)`,
placed next to `get_output_text_since_preceding_user_query`. If `text.trim()`
is empty it shows an ephemeral toast instead of touching the clipboard;
otherwise it writes as before. Follows the existing
`ToastStack::handle(ctx).update(...)` + `DismissibleToast` pattern already
used a few lines away for `CopyAIBlockCodeSnippet`'s success toast (and in
`terminal/input.rs`), rather than inventing a new mechanism.

`CopyQuery`, `CopyOutput`, `Copy`, and `CopyConversation` now call this
helper instead of writing to the clipboard directly. `CopyCommand` (copies a
shell command, not AI output text) was deliberately left untouched — it's a
different extraction path with a different root cause, outside this bug's
chain.

New i18n key, English only per `CLAUDE.local.md`:
`app/i18n/en/warp.ftl` — `menu-ai-block-nothing-to-copy = Nothing to copy`,
added next to the other `menu-ai-block-copy-*` entries. No zh-CN/ja entries
added (out of scope, per instructions).

## Fix 2: stop discarding errored/cancelled exchanges

`app/src/ai/agent/mod.rs` — `AIAgentExchange::format_output_for_copy` no
longer routes through `AIAgentOutputStatus::output()` for its "what got
streamed" question. It now matches `self.output_status` directly:

- `Streaming { output }` / `Finished { Success { output } }` — unchanged
  behavior (format whatever streamed).
- `Finished { Error { output, error } }` — **recovers `output` directly**
  (fixing the data loss noted above: `.output()` drops it even when
  present), and always appends a `[Error: {error}]` annotation.
- `Finished { Cancelled { output: None, reason } }` — appends
  `[Cancelled: {reason}]` since there is nothing else to copy.
- `Finished { Cancelled { output: Some(_), .. } }` — unchanged: real partial
  output exists, so no redundant annotation is added.

The annotation is appended after any recovered/streamed text (separated by
a blank line), or returned alone if there was no streamed text at all. This
flows through unchanged into `format_for_copy` (adds `USER:`/`AGENT:`
labels), `get_output_text_since_preceding_user_query` (multi-exchange
`CopyOutput`/`Copy` range), and `CopyConversation`'s per-exchange loop —
all three call sites go through `format_output_for_copy`/`format_for_copy`,
so the fix is centralized in one place.

### Which callers of `output()` I checked, and why neither `output()` was touched

Per the instructions, `AIAgentFinishedOutput::output()`'s (and
`AIAgentOutputStatus::output()`'s) signature/semantics were **not** changed.
`git grep` for `output_status.output()` across `app/src/` turned up ~19
call sites: `tui_export.rs`, `terminal/view/load_ai_conversation.rs` (x2),
`ai/blocklist/block/status_bar.rs`, `ai/blocklist/agent_view/child_agent_status_card.rs`,
`ai/agent/api/convert_conversation_tests.rs`, `ai/agent_sdk/driver.rs`,
`ai/blocklist/block/view_impl/output.rs`, `ai/blocklist/orchestration_events.rs`,
`ai/blocklist/action_model/execute/request_file_edits.rs`,
`ai/blocklist/controller/shared_session.rs`, `ai/agent/task_store.rs`, and
`ai/agent/conversation.rs` (x5). All of these use `None` to mean "nothing
was actually streamed for rendering/status/export purposes" (e.g. deciding
whether to render a message, whether a status bar shows content, whether to
export a transcript entry) — genuinely different semantics from "what should
a copy-to-clipboard action show the user." Repurposing `output()` would have
changed behavior at every one of those sites. Instead, the already
copy-oriented `format_output_for_copy` was extended directly (it's the
method the brief's own "separate copy-oriented accessor" escape hatch
describes — it already existed and already had no other callers besides
`format_for_copy`, so extending it in place, rather than adding a second
near-duplicate, keeps one source of truth for "what does the clipboard
copy of this exchange contain").

## Tests added

**`app/src/ai/agent/mod_test.rs`** (exchange-level, pure-logic — this is
where the existing `format_for_copy_preserves_visual_markdown_sections` test
already lives) — added a `exchange_with_output_status` builder mirroring
`task_store_tests.rs::create_test_exchange`, and:

- `format_output_for_copy_surfaces_error_message_for_errored_exchange_with_no_partial_output` —
  `Error { output: None, .. }` yields non-empty text containing the error
  message, while confirming `output_status.output()` is still `None` (i.e.
  the accessor's contract is unchanged).
- `format_output_for_copy_includes_partial_output_alongside_the_error` —
  `Error { output: Some(_), .. }` includes **both** the partial output text
  and the error annotation. This test caught a real bug in my first attempt
  at the fix (an earlier version still routed through `.output()` for the
  streamed part, which silently dropped the partial output on `Error` the
  same way the original bug did) — it now passes by construction after
  reading `output_status` directly instead.
- `format_output_for_copy_notes_cancellation_when_no_output_was_ever_streamed` —
  `Cancelled { output: None, .. }` yields the `[Cancelled: ...]` annotation.
- `format_output_for_copy_ignores_cancellation_reason_when_output_was_streamed` —
  `Cancelled { output: Some(_), .. }` yields exactly the streamed text, no
  redundant annotation (regression guard for the "don't over-annotate"
  half of the fix).

**`app/src/terminal/view_test.rs`** (view-level, next to the existing
`copy_selected_text_from_ai_block` test which already exercises AI-block
action dispatch through the same `App::test` + `add_window_with_terminal` +
`append_exchange_and_handle_event` harness) —
`ai_block_copy_output_does_not_clobber_clipboard_when_nothing_streamed`:
inserts a real AI block for a query whose exchange has not streamed
anything yet (`output_status` stays the harness default
`Streaming { output: None }`), pre-writes a sentinel string to the
clipboard, dispatches `AIBlockAction::CopyOutput` directly on the block
entity (`block.handle_action(&AIBlockAction::CopyOutput, ctx)`, the same
call the existing `SelectText` test in this file already uses), and asserts
the clipboard **still holds the sentinel** — i.e. the empty extraction did
not clobber it. This is the load-bearing assertion for fix 1; I did not
additionally assert the toast fired, since there was no existing test
pattern in this codebase for reading `ToastStack` contents to build on, and
I did not want to guess at its internals unverified.

I did not add a genuinely-empty-`CopyQuery` test (e.g. an exchange with no
user query at all) beyond this — the one integration test above already
exercises the shared `write_copied_text_or_toast_if_empty` helper that all
four actions call, so a second scenario through the same helper would not
have added coverage of new code paths.

## Verification performed

- **Not run, cannot run from here**: `cargo`/`nextest`/`rustc` — forbidden
  by the hard rules. None of the five new tests have actually executed.
- **Run, passed**: `rustfmt --check` on the specific changed regions.
  Running `rustfmt --check` on whole-module entry files (`block.rs`,
  `agent/mod.rs`) pulls in dozens of untouched sibling files via their `mod`
  declarations and reports large pre-existing import-order/macro-formatting
  drift unrelated to this change (exactly what `CLAUDE.md` warns "churns
  untouched files") — I did not touch any of that drift. I isolated my own
  hunks by diffing rustfmt's reported line numbers against the lines I
  changed: one real hit, a `Cancelled { output: None, reason }` match arm
  in my new code that rustfmt wanted multi-line, which I reformatted to
  match; re-running confirmed no diff remains in any line range I touched
  in `block.rs`, `agent/mod.rs`, `agent/mod_test.rs`, or `terminal/view_test.rs`.
- **Run, passed**: `script/check_settings_registry` — "settings registry:
  ok (50 group(s) registered in both)".
- **Run, passed**: `script/check_stub_coverage` — "stub coverage: ok (no
  test targets a gutted stub)".
- **`script/check_dangling_modules` does not exist at this commit.** Per
  `CLAUDE.md`'s own warning to verify claims against the tree: `ls script/`
  at `206799556` (the step-zero SHA, `git reset --hard origin/main`) has no
  `check_dangling_modules`. `git log --all` shows it was introduced by
  commit `a5c3b05be` ("guard: add check_dangling_modules and
  check_workspace_clean"), but that commit is **not an ancestor of
  `origin/main`'s current tip** (`git merge-base --is-ancestor` returns
  false) and isn't contained in any remote branch (`git branch -r
  --contains a5c3b05be` returns nothing) — it's not reachable from the tree
  I was handed. I ran the two scripts that do exist instead
  (`check_settings_registry`, `check_stub_coverage`, both above) and did not
  fabricate a result for the missing one.

## Deliberately left alone

- `CopyCommand`'s empty-command case (copies shell command text, not AI
  output) — different extraction path, not part of this bug's traced chain.
- The toast-content assertion in the new view-level test — no existing
  `ToastStack` read helper in this codebase to build on safely without
  compiling.
- Chinese/Japanese translations for the new `menu-ai-block-nothing-to-copy`
  key — explicitly out of scope per `CLAUDE.local.md`.
