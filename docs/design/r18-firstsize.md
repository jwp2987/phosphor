# Round 18: r18-firstsize: two `test_first_size_hold_*` unit tests fail (#2028)

Branch `fix/r18-firstsize`, from `integ/r14` `42ef67839`. Nothing here was compiled or run with
cargo; Rust is `rustfmt --check` only. "Worked, unproven" until a run.

## Verdict: TEST fault (both tests). No app change.

Round 14 (`build/r14a`, pipeline 3052606915), Linux and macOS, `warp_editor`:

- `render::tests::test_first_size_hold_lays_out_at_the_first_width_and_not_before` panics at
  `crates/editor/src/render/mod_tests.rs:504`, "the held edit must be laid out by the first size"
  (`render.blocks() > 1`).
- `render::tests::test_without_the_first_size_hold_an_edit_is_laid_out_on_arrival` (the control)
  panics at `:551`, `assertion failed: render.blocks() > 1`.

Evidence, from the stderr of the two failing tests in `test-workspace-rest.txt` (the macOS trace
is the same):

- Hold test: every assertion before `:504` passed (holding after the edit, holding after the
  0 x 0 size). Then the app logs
  `rich text: first size 400x300 released 1 held layout edit(s)` and the layout trace shows
  `Initial blocks: Trailing Newline (1 characters, ...)` and
  `Resulting blocks: Paragraph (28 characters, 1 lines, 32.00px tall)`. The hold queued the edit,
  released it at the first width and laid it out. The app did what #2028 designed.
- Control test: the same layout trace runs on arrival (no first-size line), with the same
  resulting block. The edit was laid out before any size, as the control expects.

Cause: the oracle. `RenderState::blocks()` (`crates/editor/src/render/model/mod.rs:2508`) counts
block items. An empty document has one block, the trailing newline. Laying out one typed
paragraph REPLACES that block with a `Paragraph` block (as `test_simple_edit` in the same file
shows: one paragraph, no trailing newline). So the count is 1 before and 1 after, and
`blocks() > 1` cannot be true. For the same reason the hold test's `blocks() == 1` "nothing laid
out yet" checks could not fail: they were true with or without the hold.

## Fix (tests only)

Both tests now judge the described content (`TestState::assert_rendered`, the oracle every other
test in the file uses) instead of the block count:

- nothing laid out: `Trailing Newline (1 characters, 1 lines, 32.00px tall)`;
- laid out: `Paragraph (28 characters, 1 lines, 32.00px tall)` (the exact strings of the trace).

Both tests can fail: a hold that lays out early shows `Paragraph` where the hold test expects
`Trailing Newline` (after the edit and after the zero size); a hold that does not release shows
`Trailing Newline` at the first width; a control that does not lay out on arrival shows
`Trailing Newline`. Added to the hold test: no `LayoutUpdated` and no `NeedsResize` after the
zero size. Nothing was removed except the block counts; this is not a weakening, the old checks
were either impossible or vacuous.

Rejected: changing `blocks()`; typing two paragraphs so the count rises (the count would still
say nothing about WHICH blocks exist, and the described content is the stronger oracle).

Risk: the assertions after the old `:504` (version published, viewport width 400, `LayoutUpdated`
exactly once, no `NeedsResize`, then the one-shot part: a later width asks for one resize and a
later edit is laid out on arrival) have never run. Read against the model they hold, but the next
run is their first.
