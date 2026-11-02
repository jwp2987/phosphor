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

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use string_offset::CharOffset;
use warpui::text_layout::StyleAndFont;
use warpui::units::{IntoPixels, Pixels};

use super::{
    BlockSpacing, LineCount, LineSource, OffsetMap, Paragraph, ParagraphMetrics, ParagraphStyles,
};

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

// ---------------------------------------------------------------------------------------------
// The chunk
// ---------------------------------------------------------------------------------------------

/// `N` consecutive dehydrated single-line plain-text paragraphs, stored compactly. Cheap to
/// clone (the data is behind an `Arc`, as sum-tree nodes clone their items freely).
#[derive(Clone)]
pub struct ParkedLines(Arc<ParkedData>);

#[derive(Clone)]
struct ParkedData {
    /// Paragraph style shared by every line.
    style: ParagraphStyles,
    /// Spacing shared by every line.
    spacing: BlockSpacing,
    /// Minimum paragraph height shared by every line.
    minimum_height: Option<Pixels>,
    /// Metrics height of every line (one text line of `font_size * line_height_ratio`).
    line_height: Pixels,
    first_line_height: f32,
    /// The text of all lines, concatenated without separators.
    text: Box<str>,
    /// End byte offset in `text` of each line.
    text_ends: Box<[u32]>,
    /// Cumulative content length (chars including each line's newline) at the end of each line.
    content_ends: Box<[u32]>,
    /// Cached (estimated or measured) text width of each line.
    widths: Box<[f32]>,
    /// Distinct styles used by the runs of this chunk.
    styles: Box<[StyleAndFont]>,
    /// Cumulative number of runs at the end of each line.
    run_line_ends: Box<[u32]>,
    /// End of each run, in chars from the start of its line. Runs tile their line.
    run_char_ends: Box<[u32]>,
    /// Index into `styles` of each run.
    run_styles: Box<[u16]>,
    /// Max over lines of `width + x_axis_offset` (cached; see [`ParkedLines::max_item_width`]).
    max_item_width: Pixels,
}

impl fmt::Debug for ParkedLines {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ParkedLines({} lines, {} chars)",
            self.len(),
            self.content_length()
        )
    }
}

impl ParkedData {
    fn recompute_max_item_width(&mut self) {
        let x_axis_offset = self.spacing.x_axis_offset();
        // Computed per line with the same `Pixels` arithmetic as `BlockItem::width` so the chunk
        // width equals the max of the per-line item widths exactly.
        self.max_item_width = self
            .widths
            .iter()
            .map(|width| width.into_pixels() + x_axis_offset)
            .fold(Pixels::zero(), Pixels::max);
    }
}

fn prev_end(ends: &[u32], index: usize) -> usize {
    if index == 0 {
        0
    } else {
        ends[index - 1] as usize
    }
}

impl ParkedLines {
    /// Number of lines.
    pub fn len(&self) -> usize {
        self.0.text_ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total content length (chars, newlines included).
    pub fn content_length(&self) -> CharOffset {
        CharOffset::from(self.0.content_ends.last().copied().unwrap_or(0) as usize)
    }

    pub fn lines(&self) -> LineCount {
        LineCount::from(self.len())
    }

    pub fn spacing(&self) -> BlockSpacing {
        self.0.spacing
    }

    pub fn style(&self) -> &ParagraphStyles {
        &self.0.style
    }

    pub fn minimum_height(&self) -> Option<Pixels> {
        self.0.minimum_height
    }

    pub fn first_line_height(&self) -> f32 {
        self.0.first_line_height
    }

    /// Height of one line's content, as `BlockItem::content_height` computes it for a paragraph.
    pub fn line_content_height(&self) -> Pixels {
        let mut height = self.0.line_height;
        if let Some(minimum_height) = self.0.minimum_height {
            height = height.max(minimum_height);
        }
        height
    }

    /// Height of one line including spacing: what `BlockItem::height` gives the per-line item.
    pub fn stride(&self) -> Pixels {
        self.line_content_height() + self.0.spacing.y_axis_offset()
    }

    /// The summary height: `N * stride` in `f64`, exactly the sum of the per-line summaries
    /// (`stride` as an `f32` widened to `f64`, which any partial sum represents exactly).
    pub fn summary_height(&self) -> f64 {
        self.len() as f64 * f64::from(self.stride().as_f32())
    }

    /// Total height including spacing, as a `Pixels` (not exact for huge chunks; summaries use
    /// [`Self::summary_height`]).
    pub fn total_height(&self) -> Pixels {
        (self.summary_height() as f32).into_pixels()
    }

    /// The width the chunk contributes to the tree summary: the widest line plus spacing.
    pub fn max_item_width(&self) -> Pixels {
        self.0.max_item_width
    }

    /// Text of line `index` (no newline).
    pub fn line_text(&self, index: usize) -> &str {
        let start = prev_end(&self.0.text_ends, index);
        &self.0.text[start..self.0.text_ends[index] as usize]
    }

    /// Content offset, relative to the chunk start, where line `index` starts.
    pub fn line_start(&self, index: usize) -> usize {
        prev_end(&self.0.content_ends, index)
    }

    /// Content offset, relative to the chunk start, just past line `index` (its newline).
    pub fn line_end(&self, index: usize) -> usize {
        self.0.content_ends[index] as usize
    }

    /// Index of the line containing the content offset `relative` (clamped to the last line).
    pub fn line_at_offset(&self, relative: usize) -> usize {
        let index = self
            .0
            .content_ends
            .partition_point(|end| (*end as usize) <= relative);
        index.min(self.len().saturating_sub(1))
    }

    /// The cached width of line `index`.
    pub fn line_width(&self, index: usize) -> f32 {
        self.0.widths[index]
    }

    /// The `(width, line)` of the `count` widest lines, widest first.
    pub fn widest_lines(&self, count: usize) -> Vec<(f32, usize)> {
        let mut lines: Vec<(f32, usize)> = self
            .0
            .widths
            .iter()
            .copied()
            .enumerate()
            .map(|(index, width)| (width, index))
            .collect();
        lines.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        lines.truncate(count);
        lines
    }

    /// Replaces the cached width of line `index` (an exact measurement of a widest line).
    pub fn set_line_width(&mut self, index: usize, width: f32) {
        let data = Arc::make_mut(&mut self.0);
        data.widths[index] = width;
        data.recompute_max_item_width();
    }

    /// The style runs of line `index`, exactly as the paragraph's `LineSource` had them.
    pub fn line_runs(&self, index: usize) -> Vec<(Range<usize>, StyleAndFont)> {
        let first = prev_end(&self.0.run_line_ends, index);
        let last = self.0.run_line_ends[index] as usize;
        let mut start = 0usize;
        (first..last)
            .map(|run| {
                let end = self.0.run_char_ends[run] as usize;
                let range = start..end;
                start = end;
                (range, self.0.styles[self.0.run_styles[run] as usize])
            })
            .collect()
    }

    /// What it takes to lay line `index` out (the `LineSource` its paragraph had).
    pub fn line_source(&self, index: usize) -> LineSource {
        LineSource::new(
            self.line_text(index),
            self.0.style,
            self.0.spacing,
            &self.line_runs(index),
        )
    }

    /// The metrics the dehydrated paragraph of line `index` carried.
    pub fn line_metrics(&self, index: usize) -> ParagraphMetrics {
        let text_is_empty = self.line_text(index).is_empty();
        ParagraphMetrics {
            height: self.0.line_height,
            width: self.0.widths[index].into_pixels(),
            lines: 1,
            is_empty: text_is_empty,
            first_line_height: self.0.first_line_height,
            // Empty lines are exactly zero wide; everything else is an estimate (or a measurement
            // that [`Paragraph::hydrate`] will replace when the line is shaped).
            width_exact: text_is_empty,
        }
    }

    /// The dehydrated paragraph line `index` stands for.
    pub fn paragraph(&self, index: usize) -> Paragraph {
        let content_length = self.line_end(index) - self.line_start(index);
        Paragraph::dehydrated(
            self.line_source(index),
            self.line_metrics(index),
            OffsetMap::direct(content_length),
            content_length.into(),
            vec![],
            self.0.spacing,
            self.0.minimum_height,
        )
    }

    /// Expands the chunk into the dehydrated paragraphs it stands for.
    pub fn to_paragraphs(&self) -> Vec<Paragraph> {
        (0..self.len()).map(|index| self.paragraph(index)).collect()
    }

    /// A chunk of the lines `lines` of this one.
    pub fn slice(&self, lines: Range<usize>) -> ParkedLines {
        debug_assert!(lines.start < lines.end && lines.end <= self.len());
        let data = &*self.0;
        let text_base = prev_end(&data.text_ends, lines.start);
        let text_end = data.text_ends[lines.end - 1] as usize;
        let content_base = prev_end(&data.content_ends, lines.start);
        let run_base = prev_end(&data.run_line_ends, lines.start);
        let run_end = data.run_line_ends[lines.end - 1] as usize;
        let mut sliced = ParkedData {
            style: data.style,
            spacing: data.spacing,
            minimum_height: data.minimum_height,
            line_height: data.line_height,
            first_line_height: data.first_line_height,
            text: data.text[text_base..text_end].into(),
            text_ends: data.text_ends[lines.clone()]
                .iter()
                .map(|end| end - text_base as u32)
                .collect(),
            content_ends: data.content_ends[lines.clone()]
                .iter()
                .map(|end| end - content_base as u32)
                .collect(),
            widths: data.widths[lines.clone()].into(),
            styles: data.styles.clone(),
            run_line_ends: data.run_line_ends[lines.clone()]
                .iter()
                .map(|end| end - run_base as u32)
                .collect(),
            run_char_ends: data.run_char_ends[run_base..run_end].into(),
            run_styles: data.run_styles[run_base..run_end].into(),
            max_item_width: Pixels::zero(),
        };
        sliced.recompute_max_item_width();
        ParkedLines(Arc::new(sliced))
    }

    /// Splits the chunk before each line index in `cuts` (strictly increasing, each in
    /// `1..len`). The pieces together stand for exactly the lines of this chunk.
    pub fn split_at_lines(&self, cuts: &[usize]) -> Vec<ParkedLines> {
        let mut pieces = Vec::with_capacity(cuts.len() + 1);
        let mut start = 0;
        for &cut in cuts {
            debug_assert!(
                cut > start && cut < self.len(),
                "cut {cut} of {}",
                self.len()
            );
            pieces.push(self.slice(start..cut));
            start = cut;
        }
        pieces.push(self.slice(start..self.len()));
        pieces
    }

    /// Whether `other` can be appended to this chunk (same shared metadata and still within the
    /// size limits).
    pub fn can_merge(&self, other: &ParkedLines) -> bool {
        let (a, b) = (&*self.0, &*other.0);
        a.style == b.style
            && a.spacing == b.spacing
            && a.minimum_height == b.minimum_height
            && a.line_height == b.line_height
            && a.first_line_height == b.first_line_height
            && self.len() + other.len() <= MAX_PARKED_LINES
            && (self.content_length().as_usize() + other.content_length().as_usize())
                <= MAX_PARKED_CHARS + MAX_PARKED_LINES
    }

    /// `self` followed by `other`. Callers check [`Self::can_merge`] first.
    pub fn merged(&self, other: &ParkedLines) -> ParkedLines {
        debug_assert!(self.can_merge(other));
        let (a, b) = (&*self.0, &*other.0);

        // Merge the style tables; remap `b`'s ids into the combined table.
        let mut styles: Vec<StyleAndFont> = a.styles.to_vec();
        let remap: Vec<u16> = b
            .styles
            .iter()
            .map(
                |style| match styles.iter().position(|existing| existing == style) {
                    Some(index) => index as u16,
                    None => {
                        styles.push(*style);
                        (styles.len() - 1) as u16
                    }
                },
            )
            .collect();

        let text_shift = a.text.len() as u32;
        let content_shift = a.content_ends.last().copied().unwrap_or(0);
        let run_shift = a.run_line_ends.last().copied().unwrap_or(0);
        let mut merged = ParkedData {
            style: a.style,
            spacing: a.spacing,
            minimum_height: a.minimum_height,
            line_height: a.line_height,
            first_line_height: a.first_line_height,
            text: [&*a.text, &*b.text].concat().into_boxed_str(),
            text_ends: a
                .text_ends
                .iter()
                .copied()
                .chain(b.text_ends.iter().map(|end| end + text_shift))
                .collect(),
            content_ends: a
                .content_ends
                .iter()
                .copied()
                .chain(b.content_ends.iter().map(|end| end + content_shift))
                .collect(),
            widths: a.widths.iter().chain(b.widths.iter()).copied().collect(),
            styles: styles.into_boxed_slice(),
            run_line_ends: a
                .run_line_ends
                .iter()
                .copied()
                .chain(b.run_line_ends.iter().map(|end| end + run_shift))
                .collect(),
            run_char_ends: a
                .run_char_ends
                .iter()
                .chain(b.run_char_ends.iter())
                .copied()
                .collect(),
            run_styles: a
                .run_styles
                .iter()
                .copied()
                .chain(b.run_styles.iter().map(|id| remap[*id as usize]))
                .collect(),
            max_item_width: Pixels::zero(),
        };
        merged.recompute_max_item_width();
        ParkedLines(Arc::new(merged))
    }
}

// ---------------------------------------------------------------------------------------------
// Building chunks
// ---------------------------------------------------------------------------------------------

/// A paragraph that can be parked, with the pieces the builder reads.
struct Parkable<'a> {
    source: &'a LineSource,
    metrics: ParagraphMetrics,
    chars: usize,
    minimum_height: Option<Pixels>,
}

/// Whether `paragraph` can be stored in a [`ParkedLines`] and later expanded back into an
/// identical dehydrated paragraph. Anything unusual (hydrated, wrapped, URLs, placeholders,
/// non-tiling runs, over-long lines) stays an ordinary paragraph.
fn parkable(paragraph: &Paragraph) -> Option<Parkable<'_>> {
    if paragraph.is_hydrated() || !paragraph.detected_url.is_empty() {
        return None;
    }
    let source = paragraph.line_source()?;
    let text = source.text();
    let metrics = paragraph.metrics;
    if metrics.lines != 1 || metrics.is_empty != text.is_empty() {
        return None;
    }
    let chars = text.chars().count();
    if chars > MAX_PARKED_CHARS
        || paragraph.content_length.as_usize() != chars + 1
        || !paragraph.offsets.is_direct(chars + 1)
        || paragraph.spacing != source.spacing
    {
        return None;
    }
    // The runs must tile `0..chars` so `(end, style)` pairs rebuild them exactly.
    let mut end = 0usize;
    for (range, _) in source.style_runs.iter() {
        if range.start != end || range.end < range.start {
            return None;
        }
        end = range.end;
    }
    if end != chars {
        return None;
    }
    Some(Parkable {
        source,
        metrics,
        chars,
        minimum_height: paragraph.minimum_height,
    })
}

/// Accumulates parkable paragraphs into one [`ParkedLines`].
#[derive(Default)]
pub struct ParkedBuilder {
    shared: Option<Shared>,
    text: String,
    text_ends: Vec<u32>,
    content_ends: Vec<u32>,
    widths: Vec<f32>,
    styles: Vec<StyleAndFont>,
    style_ids: HashMap<StyleAndFont, u16>,
    run_line_ends: Vec<u32>,
    run_char_ends: Vec<u32>,
    run_styles: Vec<u16>,
    content_length: usize,
}

struct Shared {
    style: ParagraphStyles,
    spacing: BlockSpacing,
    minimum_height: Option<Pixels>,
    line_height: Pixels,
    first_line_height: f32,
}

impl ParkedBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.text_ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text_ends.is_empty()
    }

    /// Whether `paragraph` is parkable at all (independent of the current chunk).
    pub fn is_parkable(paragraph: &Paragraph) -> bool {
        parkable(paragraph).is_some()
    }

    /// Appends `paragraph` if it is parkable and fits this chunk (same shared metadata, room
    /// left). Returns whether it was appended; on `false` nothing changed.
    pub fn try_push(&mut self, paragraph: &Paragraph) -> bool {
        let Some(line) = parkable(paragraph) else {
            return false;
        };
        let source = line.source;
        if let Some(shared) = &self.shared {
            if shared.style != source.paragraph_style
                || shared.spacing != source.spacing
                || shared.minimum_height != line.minimum_height
                || shared.line_height != line.metrics.height
                || shared.first_line_height != line.metrics.first_line_height
                || self.len() >= MAX_PARKED_LINES
                || self.content_length + line.chars + 1 > MAX_PARKED_CHARS + MAX_PARKED_LINES
            {
                return false;
            }
        }
        // Style ids must fit a u16: refuse a line that would overflow the table.
        let new_styles = source
            .style_runs
            .iter()
            .filter(|(_, style)| !self.style_ids.contains_key(style))
            .count();
        if self.styles.len() + new_styles >= usize::from(u16::MAX) {
            return false;
        }
        if self.shared.is_none() {
            self.shared = Some(Shared {
                style: source.paragraph_style,
                spacing: source.spacing,
                minimum_height: line.minimum_height,
                line_height: line.metrics.height,
                first_line_height: line.metrics.first_line_height,
            });
        }

        self.text.push_str(source.text());
        self.text_ends.push(self.text.len() as u32);
        self.content_length += line.chars + 1;
        self.content_ends.push(self.content_length as u32);
        self.widths.push(line.metrics.width.as_f32());
        for (range, style) in source.style_runs.iter() {
            let id = match self.style_ids.get(style) {
                Some(id) => *id,
                None => {
                    let id = self.styles.len() as u16;
                    self.styles.push(*style);
                    self.style_ids.insert(*style, id);
                    id
                }
            };
            self.run_char_ends.push(range.end as u32);
            self.run_styles.push(id);
        }
        self.run_line_ends.push(self.run_char_ends.len() as u32);
        true
    }

    /// The chunk, or `None` if nothing was pushed.
    pub fn finish(self) -> Option<ParkedLines> {
        let shared = self.shared?;
        let mut data = ParkedData {
            style: shared.style,
            spacing: shared.spacing,
            minimum_height: shared.minimum_height,
            line_height: shared.line_height,
            first_line_height: shared.first_line_height,
            text: self.text.into_boxed_str(),
            text_ends: self.text_ends.into_boxed_slice(),
            content_ends: self.content_ends.into_boxed_slice(),
            widths: self.widths.into_boxed_slice(),
            styles: self.styles.into_boxed_slice(),
            run_line_ends: self.run_line_ends.into_boxed_slice(),
            run_char_ends: self.run_char_ends.into_boxed_slice(),
            run_styles: self.run_styles.into_boxed_slice(),
            max_item_width: Pixels::zero(),
        };
        data.recompute_max_item_width();
        Some(ParkedLines(Arc::new(data)))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::render::model::hydration::estimate_paragraph_metrics;
    use crate::render::model::test_utils::{TEST_STYLES, layout};
    use crate::render::model::{BlockItem, PARAGRAPH_MIN_HEIGHT, TEXT_SPACING};
    use warpui::color::ColorU;
    use warpui::fonts::{FamilyId, Properties};
    use warpui::text_layout::TextStyle;

    pub(crate) fn style(shade: u8) -> StyleAndFont {
        StyleAndFont::new(
            FamilyId(0),
            Properties::default(),
            TextStyle::new().with_syntax_color(ColorU::new(shade, shade, shade, 255)),
        )
    }

    /// A dehydrated plain-text paragraph with two runs (split at the middle) like a lazily laid
    /// out code line.
    pub(crate) fn dehydrated_line(text: &str, shade: u8) -> Paragraph {
        let chars = text.chars().count();
        let mid = chars / 2;
        let runs = if chars == 0 {
            vec![]
        } else {
            vec![
                (0..mid, style(shade)),
                (mid..chars, style(shade.wrapping_add(1))),
            ]
        };
        let paragraph_style = TEST_STYLES.base_text;
        Paragraph::dehydrated(
            LineSource::new(text, paragraph_style, TEXT_SPACING, &runs),
            estimate_paragraph_metrics(text, &paragraph_style, paragraph_style.font_size),
            OffsetMap::direct(chars + 1),
            (chars + 1).into(),
            vec![],
            TEXT_SPACING,
            Some(PARAGRAPH_MIN_HEIGHT),
        )
    }

    fn park(paragraphs: &[Paragraph]) -> ParkedLines {
        let mut builder = ParkedBuilder::new();
        for paragraph in paragraphs {
            assert!(builder.try_push(paragraph), "{:?}", paragraph.line_source());
        }
        builder.finish().expect("non-empty")
    }

    fn assert_same(expected: &Paragraph, actual: &Paragraph) {
        let (e, a) = (
            expected.line_source().unwrap(),
            actual.line_source().unwrap(),
        );
        assert_eq!(e.text(), a.text());
        assert_eq!(e.style_runs.as_ref(), a.style_runs.as_ref());
        assert_eq!(e.paragraph_style(), a.paragraph_style());
        assert_eq!(e.spacing(), a.spacing());
        assert_eq!(expected.content_length, actual.content_length);
        assert_eq!(expected.spacing, actual.spacing);
        assert_eq!(expected.minimum_height, actual.minimum_height);
        assert_eq!(expected.metrics.height, actual.metrics.height);
        assert_eq!(expected.metrics.width, actual.metrics.width);
        assert_eq!(expected.metrics.lines, actual.metrics.lines);
        assert_eq!(expected.metrics.is_empty, actual.metrics.is_empty);
        assert_eq!(
            expected.metrics.first_line_height,
            actual.metrics.first_line_height
        );
        assert!(actual.offsets.is_direct(actual.content_length.as_usize()));
        assert!(!actual.is_hydrated());
    }

    fn texts() -> Vec<String> {
        vec![
            "fn main() {".into(),
            String::new(),
            "    let x = 1; // 日本語 and 👩‍👩‍👧‍👦".into(),
            "\tindented\twith tabs".into(),
            "x".repeat(300),
            "}".into(),
            String::new(),
            "last".into(),
        ]
    }

    #[test]
    fn a_chunk_expands_into_the_exact_paragraphs_it_stood_for() {
        let originals: Vec<Paragraph> = texts()
            .iter()
            .enumerate()
            .map(|(i, t)| dehydrated_line(t, i as u8))
            .collect();
        let parked = park(&originals);
        assert_eq!(parked.len(), originals.len());
        let expanded = parked.to_paragraphs();
        for (expected, actual) in originals.iter().zip(&expanded) {
            assert_same(expected, actual);
        }
        // Per-line accessors agree with the expansion.
        let mut offset = 0;
        for (index, text) in texts().iter().enumerate() {
            assert_eq!(parked.line_text(index), text);
            assert_eq!(parked.line_start(index), offset);
            offset += text.chars().count() + 1;
            assert_eq!(parked.line_end(index), offset);
            assert_eq!(parked.line_at_offset(parked.line_start(index)), index);
            assert_eq!(parked.line_at_offset(parked.line_end(index) - 1), index);
        }
        assert_eq!(parked.content_length().as_usize(), offset);
        // Past the end clamps to the last line.
        assert_eq!(parked.line_at_offset(offset + 10), originals.len() - 1);
    }

    /// The chunk's summary is bit-identical to the sum of the per-line summaries: no height,
    /// width, line or length drift between the chunked and per-line trees.
    #[test]
    fn summary_equals_the_sum_of_per_line_summaries() {
        use sum_tree::Item;
        let originals: Vec<Paragraph> = (0..MAX_PARKED_LINES)
            .map(|i| dehydrated_line(&format!("line {i} {}", "w".repeat(i % 37)), i as u8))
            .collect();
        let parked = park(&originals);
        let mut expected = crate::render::model::LayoutSummary::default();
        for paragraph in &originals {
            expected += &BlockItem::Paragraph(paragraph.clone()).summary();
        }
        // The chunk is one tree item standing for all of them.
        expected.item_count = 1;
        let actual = BlockItem::ParkedLines(parked).summary();
        assert_eq!(actual, expected);
    }

    #[test]
    fn splitting_and_merging_round_trip() {
        let originals: Vec<Paragraph> = texts()
            .iter()
            .enumerate()
            .map(|(i, t)| dehydrated_line(t, (i % 3) as u8))
            .collect();
        let parked = park(&originals);
        let pieces = parked.split_at_lines(&[1, 4, 5]);
        assert_eq!(
            pieces.iter().map(ParkedLines::len).collect::<Vec<_>>(),
            vec![1, 3, 1, 3]
        );
        let expanded: Vec<Paragraph> = pieces.iter().flat_map(|p| p.to_paragraphs()).collect();
        assert_eq!(expanded.len(), originals.len());
        for (expected, actual) in originals.iter().zip(&expanded) {
            assert_same(expected, actual);
        }
        // Pieces keep exact per-chunk summaries.
        let total: usize = pieces.iter().map(|p| p.content_length().as_usize()).sum();
        assert_eq!(total, parked.content_length().as_usize());

        let mut merged = pieces[0].clone();
        for piece in &pieces[1..] {
            assert!(merged.can_merge(piece));
            merged = merged.merged(piece);
        }
        for (expected, actual) in originals.iter().zip(merged.to_paragraphs().iter()) {
            assert_same(expected, actual);
        }
        assert_eq!(merged.max_item_width(), parked.max_item_width());
    }

    #[test]
    fn unusual_paragraphs_are_not_parkable() {
        let good = dehydrated_line("hello world", 1);
        assert!(ParkedBuilder::is_parkable(&good));

        // Hydrated.
        let mut hydrated = good.clone();
        hydrated.hydrate(std::sync::Arc::new(layout(
            "hello world",
            &TEST_STYLES,
            f32::MAX,
        )));
        assert!(!ParkedBuilder::is_parkable(&hydrated));
        // ...but evicting it makes it parkable again.
        assert!(hydrated.evict());
        assert!(ParkedBuilder::is_parkable(&hydrated));

        // Content length that is not `chars + 1` (a placeholder or unterminated last line).
        let mut odd = good.clone();
        odd.content_length = CharOffset::from(5);
        assert!(!ParkedBuilder::is_parkable(&odd));

        // Runs that do not tile the text.
        let paragraph_style = TEST_STYLES.base_text;
        let gap = Paragraph::dehydrated(
            LineSource::new("abcd", paragraph_style, TEXT_SPACING, &[(1..4, style(1))]),
            estimate_paragraph_metrics("abcd", &paragraph_style, 8.),
            OffsetMap::direct(5),
            5.into(),
            vec![],
            TEXT_SPACING,
            Some(PARAGRAPH_MIN_HEIGHT),
        );
        assert!(!ParkedBuilder::is_parkable(&gap));

        // An over-long line stays a normal paragraph.
        let long = dehydrated_line(&"y".repeat(MAX_PARKED_CHARS + 1), 1);
        assert!(!ParkedBuilder::is_parkable(&long));
    }

    #[test]
    fn a_chunk_closes_at_the_line_limit_and_on_different_metadata() {
        let mut builder = ParkedBuilder::new();
        for i in 0..MAX_PARKED_LINES {
            assert!(builder.try_push(&dehydrated_line("x", i as u8)));
        }
        assert!(!builder.try_push(&dehydrated_line("x", 0)), "line limit");

        let mut builder = ParkedBuilder::new();
        assert!(builder.try_push(&dehydrated_line("x", 0)));
        let mut other = dehydrated_line("y", 0);
        other.minimum_height = None;
        assert!(!builder.try_push(&other), "different minimum height");
    }

    #[test]
    fn setting_a_measured_width_updates_the_chunk_width() {
        let originals = vec![dehydrated_line("ab", 0), dehydrated_line("abcd", 0)];
        let mut parked = park(&originals);
        let before = parked.max_item_width();
        let widest = parked.widest_lines(1);
        assert_eq!(widest[0].1, 1);
        parked.set_line_width(0, 1000.);
        assert!(parked.max_item_width() > before);
        assert_eq!(parked.line_width(0), 1000.);
        assert_eq!(parked.paragraph(0).metrics.width.as_f32(), 1000.);
    }

    #[test]
    fn empty_lines_are_exact_and_empty() {
        let parked = park(&[dehydrated_line("", 0)]);
        let paragraph = parked.paragraph(0);
        assert!(paragraph.metrics.is_empty);
        assert!(paragraph.metrics.width_exact);
        assert_eq!(parked.line_runs(0), vec![]);
        assert_eq!(parked.line_text(0), "");
    }
}
