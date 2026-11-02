//! Parked line chunks: P5 of the code-editor memory rebuild (#1340).
//!
//! # Problem
//!
//! After P0-P4 a 7 MB file (85k-200k lines) opens with its paragraphs *dehydrated* (no glyphs, no
//! carets), but every line is still one `BlockItem::Paragraph` in the content sum-tree, and each
//! of those owns: the item itself (~250 B incl. tree summary), a boxed [`LineSource`] (an `Arc<str>`
//! copy of the line text, an `Arc<[(Range, StyleAndFont)]>` copy of its syntax-colour runs at
//! ~56 B per run, a `ParagraphStyles` and a `BlockSpacing`), an `OffsetMap` heap vector and a
//! `Vec<ParsedUrl>`. That is 500+ B of bookkeeping per line plus ~56 B per syntax token: tens of
//! MB for plain text and ~100 MB for Rust, on top of the buffer itself.
//!
//! # Design
//!
//! A [`ParkedLines`] is ONE sum-tree item (`BlockItem::ParkedLines`) standing for `N` consecutive
//! dehydrated single-line plain-text paragraphs (N <= [`MAX_PARKED_LINES`], at most
//! [`MAX_PARKED_CHARS`] chars). It stores, in flat arrays shared by the whole chunk:
//!
//! * the style / spacing / minimum height / line height that every line has in common,
//! * the concatenated line text plus a `u32` end per line,
//! * the cumulative content length per line (`chars + 1`, the newline),
//! * the cached estimated-or-measured width per line (`f32`),
//! * the syntax-colour runs as `(u32 end, u16 style id)` against a per-chunk style table
//!   (~6 B per run instead of ~56 B; runs must tile the line, otherwise the line is not parkable).
//!
//! About 16 B per line plus 6 B per token plus the text, versus 500+ B and 56 B per token. A
//! 200k-line file is ~800 sum-tree items rather than 200k.
//!
//! The chunk is exact, not an approximation: every line of a chunk is by construction a
//! single-line paragraph of height `line_height`, so [`ParkedLines::summary_height`] is
//! `N * stride` (`stride` computed with the same expression as `BlockItem::height`; all partial
//! sums are exactly representable in `f64`, so the chunked tree has bit-identical heights to the
//! per-line tree: no drift), `lines` is `N`, content length the sum, and width the max of
//! `width_i + x_axis_offset`.
//!
//! ## Where the text lives (and why it is not the buffer, yet)
//!
//! The brief asked for dehydrated paragraphs to *reference* the main buffer instead of owning
//! their text. The chunk keeps its own compact copy for now (one `Box<str>` per chunk, ~1x the
//! file size, ~7 MB for the 7 MB target) and exposes it only through [`ParkedLines::line_text`] /
//! [`ParkedLines::line_source`], so a buffer-backed reader can replace it later without touching
//! any consumer. Reasons not to do it blind in this change:
//!
//! * the render tree lags the buffer by design (edits reach it through an async layout channel
//!   and are queued until the element layout hook), so a char offset read from the buffer at
//!   hydration time may not belong to the tree's version: wrong text on screen, the one failure
//!   this work must not introduce. A correct reference needs a `BufferSnapshot` taken per delta
//!   and kept in step with the tree, and
//! * the syntax colours are style *markers* in the buffer; rebuilding a line's style runs from a
//!   snapshot means making `StyledBufferBlocks` (which needs `&Buffer`) snapshot-capable.
//!
//! The dominant per-line costs (item, `LineSource` boxes, runs) are gone either way.
//!
//! ## How "ranges stay valid" without storing ranges
//!
//! Nothing in a chunk is an absolute offset. A chunk's position is its position in the tree, and
//! the tree is always spliced by the same block-aligned `EditDelta`s that P0-P4 received (insert,
//! delete, replace across lines, undo/redo, replace-all, multi-cursor edits, external reloads and
//! line-ending changes all arrive as `EditDelta { old_offset, new_lines }`). The only new rule is:
//!
//! > **Before a splice, any chunk that strictly contains a splice boundary is split at that
//! > (line-aligned) boundary** ([`split_parked_at`]); chunks entirely inside the replaced range
//! > are simply dropped by the splice, exactly like the per-line items they replace.
//!
//! The same split runs before the two other places that cut the tree at positions: temporary
//! (diff-hunk) block insertion by line number, and hidden-range de-duplication. A boundary that
//! is not a line boundary of the chunk cannot be produced by a block-aligned delta; if it ever
//! happens the chunk is expanded into ordinary per-line paragraphs instead (the P4 shape), which
//! is always correct. External reloads and `release_layout` replace the whole tree.
//!
//! ## Split / merge rules
//!
//! * Build: [`ParkedBuilder`] greedily groups consecutive parkable paragraphs into chunks of at
//!   most [`MAX_PARKED_LINES`] lines / [`MAX_PARKED_CHARS`] chars, closing a chunk when a line has
//!   different shared metadata. A line is parkable only if it is dehydrated, single-line, has a
//!   direct offset map of exactly `chars + 1`, no detected URLs and tiling style runs. Everything
//!   else (a line with a URL, a placeholder, a hydrated line, a line that is too long) stays an
//!   ordinary `BlockItem::Paragraph`; chunks never make anything *less* correct, only smaller.
//! * Split: [`ParkedLines::split_at_lines`] at splice boundaries (above). Pieces are chunks again.
//! * Merge: the eviction sweep re-parks runs of dehydrated paragraphs outside the keep window
//!   ([`RenderState::repark_outside`]) and merges adjacent small chunks whose combined size fits.
//! * Unpark: [`ParkedLines::to_paragraphs`] materialises a chunk into the exact dehydrated
//!   `Paragraph`s it stood for (same text, runs, metrics, offsets).
//!
//! ## Hydration and eviction (P3/P4) on top of chunks
//!
//! `update_hydration` first *unparks* every chunk overlapping the visible rows, the pinned rows
//! around selections and the hydration margin (cheap: no shaping), then runs the unchanged
//! hydrate path on the resulting per-line paragraphs. So the viewport window, the selections and
//! every `hydrate_around_offsets` target (go-to-line, find-next, LSP reveal, autoscroll, IME
//! composition) are always ordinary paragraphs. The eviction sweep evicts far hydrated
//! paragraphs (unchanged) and then re-parks far dehydrated ones. Cached metrics map one-to-one:
//! `height`/`lines`/`first_line_height` are chunk constants, `width` is the per-line array,
//! `is_empty` is `text.is_empty()`.
//!
//! ## Consumers that can still see a chunk (far from the viewport)
//!
//! They get the same y-exact, x-approximate answers a dehydrated paragraph already gave in P3
//! (and report it through `hydration::report_dehydrated_lines`): `character_bounds`,
//! `offset_to_softwrap_point`, `softwrap_point_to_offset`, hit testing, `y_offset_at_line`,
//! `vertical_offset_at_render_location`, find / go-to-line / selections (after hydrating their
//! targets), diff hunks (`reset_temporary_block` splits first), soft wrap (the code editor is
//! unwrapped; wrapped editors never park), folding / hidden lines (parking is off while hidden
//! ranges exist in a delta, and boundaries are split before de-duplication), IME (pinned rows are
//! unparked and hydrated).
//!
//! # Kill switch
//!
//! `PHOSPHOR_EDITOR_P5=0` restores the P4 representation: nothing is ever parked, every line is
//! an individual paragraph. `PHOSPHOR_EDITOR_EAGER_LAYOUT=1` still disables all lazy layout.

use std::sync::OnceLock;

/// Maximum lines per parked chunk.
pub const MAX_PARKED_LINES: usize = 256;
/// Maximum characters (excluding newlines) per parked chunk, which also bounds the `u32` offsets.
pub const MAX_PARKED_CHARS: usize = 64 * 1024;
/// Shortest run of dehydrated paragraphs the eviction sweep bothers to re-park.
pub const MIN_REPARK_RUN: usize = 32;

/// Whether chunking is enabled (`PHOSPHOR_EDITOR_P5=0` turns it off). Read once.
pub fn p5_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("PHOSPHOR_EDITOR_P5").map_or(true, |v| v.trim() != "0"))
}
