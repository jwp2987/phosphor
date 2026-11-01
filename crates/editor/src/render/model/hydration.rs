//! Lazy paragraph layout (#1340 P3/P4).
//!
//! A large plain-text delta (opening a file) is laid out *dehydrated*: every paragraph keeps its
//! [`LineSource`] and cached [`ParagraphMetrics`] but no glyphs or carets. The element layout hook
//! ([`RenderState::update_hydration`]) then hydrates what is on or near the screen plus the pinned
//! set (selections), and evicts hydrated paragraphs that fall far outside it.
//!
//! Invariants this module relies on:
//! * The editor is unwrapped (`WidthSetting::InfiniteWidth`), so every plain-text paragraph is
//!   exactly one line of `font_size * line_height_ratio` pixels: height is known without shaping.
//! * Width is an estimate (`cells * monospace advance`) until the paragraph is shaped; the widest
//!   few lines of a delta are shaped at layout time so the content width is exact where it matters,
//!   and hydrating reports width changes so sizing is invalidated.
//! * Anything that reads glyph or caret data (`Positioned<Paragraph>::lines()` consumers) must run
//!   on hydrated paragraphs. Reaching one dehydrated bumps [`dehydrated_line_queries`] and logs.

use std::sync::atomic::{AtomicU64, Ordering};

use unicode_width::UnicodeWidthChar;
use warpui::units::{IntoPixels, Pixels};

use super::{ParagraphMetrics, ParagraphStyles};

/// Paragraphs hydrated per frame outside the visible rows, on top of everything visible.
pub const HYDRATION_BUDGET_PER_FRAME: usize = 256;
/// Hydrate this many viewport heights above and below the visible rows.
pub const HYDRATE_MARGIN_SCREENS: f32 = 2.0;
/// Evict hydrated paragraphs farther than this many viewport heights from the visible rows.
pub const EVICT_MARGIN_SCREENS: f32 = 4.0;
/// Rows around each selection endpoint that are always kept hydrated.
pub const PINNED_ROWS_AROUND_SELECTION: usize = 3;
/// A full eviction sweep runs after this many hydrations.
pub const EVICTION_SWEEP_EVERY: usize = 1500;
/// Floor for the viewport height used to size the hydration windows before the first layout.
pub const MIN_WINDOW_HEIGHT_PX: f32 = 900.0;

static DEHYDRATED_LINE_QUERIES: AtomicU64 = AtomicU64::new(0);

/// How many times glyph/caret geometry was requested from a dehydrated paragraph. Non-zero means
/// some consumer ran before its lines were hydrated.
pub fn dehydrated_line_queries() -> u64 {
    DEHYDRATED_LINE_QUERIES.load(Ordering::Relaxed)
}

/// Records a glyph/caret query against a dehydrated paragraph: counts it, warns (first one and
/// then every 1024th) and, in strict mode (`PHOSPHOR_EDITOR_LAZY_STRICT=1`, always on in unit
/// tests), `debug_assert!`s so the offending path is found.
pub(super) fn note_dehydrated_line_query() {
    let count = DEHYDRATED_LINE_QUERIES.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count % 1024 == 0 {
        log::warn!("editor: glyph geometry queried on a dehydrated paragraph (#{count})");
    }
    debug_assert!(
        !strict_mode(),
        "glyph geometry queried on a dehydrated paragraph"
    );
}

fn strict_mode() -> bool {
    static STRICT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    cfg!(test)
        || *STRICT.get_or_init(|| {
            std::env::var("PHOSPHOR_EDITOR_LAZY_STRICT").is_ok_and(|v| !v.is_empty() && v != "0")
        })
}

/// Characters that make a text layout produce more than one line even without wrapping.
pub(crate) fn has_line_break(text: &str) -> bool {
    const BREAKS: &[char] = &[
        '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
    ];
    text.contains(BREAKS)
}

/// Metrics of an unwrapped single-line paragraph without shaping it. Height, line count and
/// emptiness are exact; width is `cells * advance` (an upper bound for tabs) and flagged inexact.
///
/// Callers must not use this for text containing line breaks ([`has_line_break`]).
pub(crate) fn estimate_paragraph_metrics(
    text: &str,
    style: &ParagraphStyles,
    advance: f32,
) -> ParagraphMetrics {
    let line_height = (style.font_size * style.line_height_ratio).into_pixels();
    if text.is_empty() {
        return ParagraphMetrics {
            height: line_height,
            width: Pixels::zero(),
            lines: 1,
            is_empty: true,
            first_line_height: line_height.as_f32(),
            width_exact: true,
        };
    }
    let tab_cells = usize::from(style.fixed_width_tab_size.unwrap_or(4).max(1));
    let cells: usize = text
        .chars()
        .map(|c| {
            if c == '\t' {
                tab_cells
            } else {
                c.width().unwrap_or(0)
            }
        })
        .sum();
    ParagraphMetrics {
        height: line_height,
        width: (cells as f32 * advance).into_pixels(),
        lines: 1,
        is_empty: false,
        first_line_height: line_height.as_f32(),
        width_exact: false,
    }
}

// ---------------------------------------------------------------------------------------------
// Hydration manager
// ---------------------------------------------------------------------------------------------

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

use ordered_float::OrderedFloat;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use string_offset::CharOffset;
use sum_tree::{SeekBias, SumTree};
use warpui::AppContext;
use warpui::text_layout::TextFrame;

use super::{BlockItem, Height, LayoutMode, LayoutSummary, LineSource, RenderState, WidthSetting};
use crate::render::layout::eager_layout_forced;

/// Per-editor hydration bookkeeping.
#[derive(Default)]
pub(super) struct HydrationState {
    /// Hydrations since the last eviction sweep.
    hydrated_since_sweep: Cell<usize>,
    /// Content size changed because a hydrated paragraph measured differently from its
    /// estimate; the next model update must refresh viewport content sizing.
    sizing_dirty: Cell<bool>,
    /// Hydration stopped at the per-frame budget with work left; another frame is needed.
    more_pending: Cell<bool>,
}

impl HydrationState {
    /// Takes the "content size changed" flag.
    pub(super) fn take_sizing_dirty(&self) -> bool {
        self.sizing_dirty.replace(false)
    }

    /// The font or size changed, so every parked width estimate is stale.
    pub(super) fn mark_restyled(&self) {
        self.sizing_dirty.set(true);
    }

    /// Takes the "more hydration pending" flag.
    pub(super) fn take_more_pending(&self) -> bool {
        self.more_pending.replace(false)
    }
}

/// Index of a block item in the content tree (the item-count dimension).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ItemIndex(usize);

impl<'a> sum_tree::Dimension<'a, LayoutSummary> for ItemIndex {
    fn add_summary(&mut self, summary: &'a LayoutSummary) {
        self.0 += summary.item_count;
    }
}

enum ParagraphEdit {
    Hydrate(Arc<TextFrame>),
    Evict,
}

/// A closed y-range of content space, in pixels.
#[derive(Debug, Clone, Copy)]
struct YRange {
    start: f32,
    end: f32,
}

impl YRange {
    fn overlaps(&self, other: &YRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

thread_local! {
    static Y_ONLY_GEOMETRY: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` declaring that it only reads y-axis geometry (row tops, heights), which a dehydrated
/// paragraph answers exactly, so dehydrated line access inside it is not reported.
pub(super) fn y_only_geometry<T>(f: impl FnOnce() -> T) -> T {
    let previous = Y_ONLY_GEOMETRY.with(|flag| flag.replace(true));
    let result = f();
    Y_ONLY_GEOMETRY.with(|flag| flag.set(previous));
    result
}

/// Reports a dehydrated glyph/caret query unless it is inside [`y_only_geometry`].
pub(super) fn report_dehydrated_lines() {
    if !Y_ONLY_GEOMETRY.with(Cell::get) {
        note_dehydrated_line_query();
    }
}

impl RenderState {
    /// Whether this editor lays paragraphs out lazily: only the unwrapped pixel-mode editor
    /// (the code editor), and not when `PHOSPHOR_EDITOR_EAGER_LAYOUT=1`.
    pub(super) fn lazy_paragraphs_enabled(&self) -> bool {
        matches!(self.width_setting, WidthSetting::InfiniteWidth)
            && matches!(self.layout_mode, LayoutMode::Pixels)
            && !eager_layout_forced()
    }

    /// A relayout-requiring style change (font family, size, line height...) arrived: the
    /// paragraphs this editor parked dehydrated carry width estimates measured with the old font,
    /// and a pending full relayout re-estimates them. Invalidates content sizing now so the next
    /// element update refreshes it, whatever order the relayout and the hydration pass land in.
    /// A no-op for editors that do not park paragraphs.
    pub(super) fn note_restyled_for_parked_paragraphs(&self) {
        if self.lazy_paragraphs_enabled() {
            self.hydration.mark_restyled();
        }
    }

    /// Takes (and clears) the flags the hydration pass raised, so the model can refresh content
    /// sizing and schedule a follow-up frame. Returns `(sizing_dirty, more_pending)`.
    pub(super) fn take_hydration_flags(&self) -> (bool, bool) {
        (
            self.hydration.take_sizing_dirty(),
            self.hydration.take_more_pending(),
        )
    }

    /// The element layout hook: hydrates the visible rows, the pinned rows around selections and
    /// a margin of [`HYDRATE_MARGIN_SCREENS`] screens (up to [`HYDRATION_BUDGET_PER_FRAME`] per
    /// frame beyond the visible rows), then evicts hydrated paragraphs far from all of them.
    ///
    /// Must run after pending edits are flushed and before blocks are collected for layout.
    pub fn update_hydration(&self, app: &AppContext) {
        if !self.lazy_paragraphs_enabled() {
            return;
        }
        let window = self.viewport.height().as_f32().max(MIN_WINDOW_HEIGHT_PX);
        let top = self.viewport.scroll_top().as_f32();
        let visible = YRange {
            start: top,
            end: top + window,
        };
        let margin = YRange {
            start: (top - HYDRATE_MARGIN_SCREENS * window).max(0.),
            end: top + (1. + HYDRATE_MARGIN_SCREENS) * window,
        };
        let pinned = self.pinned_ranges(&self.selection_offsets());

        // Visible rows and pinned rows are mandatory; the margin is budgeted.
        let mut wanted: BTreeMap<usize, ()> = BTreeMap::new();
        {
            let content = self.content.borrow();
            for range in std::iter::once(&visible).chain(pinned.iter()) {
                for index in dehydrated_in(&content, range, usize::MAX) {
                    wanted.insert(index, ());
                }
            }
            let mut budget = HYDRATION_BUDGET_PER_FRAME;
            for index in dehydrated_in(&content, &margin, budget + wanted.len() + 1) {
                if wanted.contains_key(&index) {
                    continue;
                }
                if budget == 0 {
                    self.hydration.more_pending.set(true);
                    break;
                }
                budget -= 1;
                wanted.insert(index, ());
            }
        }
        if !wanted.is_empty() {
            self.hydrate_items(wanted.into_keys().collect(), app);
        }

        if self.hydration.hydrated_since_sweep.get() >= EVICTION_SWEEP_EVERY {
            let keep_window = YRange {
                start: (top - EVICT_MARGIN_SCREENS * window).max(0.),
                end: top + (1. + EVICT_MARGIN_SCREENS) * window,
            };
            self.evict_outside(&keep_window, &pinned);
            self.hydration.hydrated_since_sweep.set(0);
        }
    }

    /// Hydrates the rows around `offsets` (plus the selections), immediately. Called before any
    /// query that needs glyph or caret geometry at a position that may not be on screen:
    /// autoscroll, go-to-line, find-next, LSP reveal, selection changes.
    pub fn hydrate_around_offsets(&self, offsets: &[CharOffset], app: &AppContext) {
        if !self.lazy_paragraphs_enabled() {
            return;
        }
        let mut all = self.selection_offsets();
        all.extend_from_slice(offsets);
        let ranges = self.pinned_ranges(&all);
        let wanted: Vec<usize> = {
            let content = self.content.borrow();
            let mut set: BTreeMap<usize, ()> = BTreeMap::new();
            for range in &ranges {
                for index in dehydrated_in(&content, range, usize::MAX) {
                    set.insert(index, ());
                }
            }
            set.into_keys().collect()
        };
        if !wanted.is_empty() {
            self.hydrate_items(wanted, app);
        }
    }

    /// Hydrates the rows around the current selections.
    pub fn hydrate_pinned(&self, app: &AppContext) {
        self.hydrate_around_offsets(&[], app);
    }

    /// Hydrates every paragraph overlapping `range`. A deliberately unbounded escape hatch for
    /// whole-document operations that need glyph geometry (not used by the editor itself).
    pub fn hydrate_all(&self, app: &AppContext) {
        if !self.lazy_paragraphs_enabled() {
            return;
        }
        let wanted: Vec<usize> = {
            let content = self.content.borrow();
            let all = YRange {
                start: 0.,
                end: f32::MAX,
            };
            dehydrated_in(&content, &all, usize::MAX)
        };
        if !wanted.is_empty() {
            self.hydrate_items(wanted, app);
        }
    }

    fn selection_offsets(&self) -> Vec<CharOffset> {
        self.selections()
            .iter()
            .flat_map(|selection| [selection.start(), selection.end(), selection.head])
            .collect()
    }

    /// The y-ranges kept hydrated around each of `offsets`.
    fn pinned_ranges(&self, offsets: &[CharOffset]) -> Vec<YRange> {
        let content = self.content.borrow();
        let row = self.styles.base_line_height().as_f32().max(1.);
        let pad = PINNED_ROWS_AROUND_SELECTION as f32 * row;
        let mut ranges = Vec::with_capacity(offsets.len());
        for offset in offsets {
            let mut cursor = content.cursor::<CharOffset, LayoutSummary>();
            if cursor.seek(offset, SeekBias::Right) {
                let y = cursor.start().height as f32;
                ranges.push(YRange {
                    start: (y - pad).max(0.),
                    end: y + row + pad,
                });
            }
        }
        ranges
    }

    /// Shapes the paragraphs at `indices` (in parallel) and installs the frames.
    fn hydrate_items(&self, indices: Vec<usize>, app: &AppContext) {
        let sources: Vec<(usize, LineSource)> = {
            let content = self.content.borrow();
            let mut sources = Vec::with_capacity(indices.len());
            let mut cursor = content.cursor::<ItemIndex, ()>();
            for index in indices {
                cursor.seek(&ItemIndex(index), SeekBias::Right);
                if let Some(BlockItem::Paragraph(paragraph)) = cursor.item() {
                    if !paragraph.is_hydrated() {
                        if let Some(source) = paragraph.line_source() {
                            sources.push((index, source.clone()));
                        }
                    }
                }
            }
            sources
        };
        if sources.is_empty() {
            return;
        }
        let layout = self.layout_context(app);
        let frames: Vec<(usize, ParagraphEdit)> = sources
            .into_par_iter()
            .map(|(index, source)| (index, ParagraphEdit::Hydrate(source.layout(&layout))))
            .collect();
        self.hydration
            .hydrated_since_sweep
            .set(self.hydration.hydrated_since_sweep.get() + frames.len());
        self.apply_paragraph_edits(frames);
    }

    /// Evicts hydrated paragraphs outside `keep` and not inside any pinned range.
    fn evict_outside(&self, keep: &YRange, pinned: &[YRange]) {
        let evictions: Vec<(usize, ParagraphEdit)> = {
            let content = self.content.borrow();
            let mut out = Vec::new();
            let mut cursor = content.cursor::<ItemIndex, LayoutSummary>();
            cursor.descend_to_first_item(&content, |_| true);
            while let Some(item) = cursor.item() {
                if let BlockItem::Paragraph(paragraph) = item {
                    if paragraph.is_hydrated() && paragraph.line_source().is_some() {
                        let start = cursor.start().height as f32;
                        let row = YRange {
                            start,
                            end: start + paragraph.height().as_f32(),
                        };
                        if !row.overlaps(keep) && !pinned.iter().any(|range| row.overlaps(range)) {
                            out.push((cursor.start().item_count, ParagraphEdit::Evict));
                        }
                    }
                }
                cursor.next();
            }
            out
        };
        if !evictions.is_empty() {
            self.apply_paragraph_edits(evictions);
        }
    }

    /// Rebuilds the content tree with `edits` (sorted ascending, unique item indices) applied to
    /// their paragraphs. Marks sizing dirty when a hydrated paragraph's measured size differs
    /// from its cached metrics.
    fn apply_paragraph_edits(&self, mut edits: Vec<(usize, ParagraphEdit)>) {
        edits.sort_by_key(|(index, _)| *index);
        let mut size_changed = false;
        let new_tree = {
            let content = self.content.borrow();
            let mut new_tree = SumTree::new();
            let mut cursor = content.cursor::<ItemIndex, ()>();
            for (index, edit) in edits {
                new_tree.push_tree(cursor.slice(&ItemIndex(index), SeekBias::Right));
                let Some(item) = cursor.item() else {
                    break;
                };
                match (item, edit) {
                    (BlockItem::Paragraph(paragraph), ParagraphEdit::Hydrate(frame)) => {
                        let mut paragraph = paragraph.clone();
                        size_changed |= paragraph.hydrate(frame);
                        new_tree.push(BlockItem::Paragraph(paragraph));
                    }
                    (BlockItem::Paragraph(paragraph), ParagraphEdit::Evict) => {
                        let mut paragraph = paragraph.clone();
                        paragraph.evict();
                        new_tree.push(BlockItem::Paragraph(paragraph));
                    }
                    (other, _) => new_tree.push(other.clone()),
                }
                cursor.next();
            }
            new_tree.push_tree(cursor.suffix());
            new_tree
        };
        *self.content.borrow_mut() = new_tree;
        if size_changed {
            self.hydration.sizing_dirty.set(true);
        }
    }
}

/// Indices of dehydrated, hydratable paragraphs overlapping `range`, at most `limit` of them.
fn dehydrated_in(content: &SumTree<BlockItem>, range: &YRange, limit: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut cursor = content.cursor::<Height, LayoutSummary>();
    cursor.seek(
        &Height(OrderedFloat(f64::from(range.start))),
        SeekBias::Right,
    );
    while let Some(item) = cursor.item() {
        let start = cursor.start().height as f32;
        if start >= range.end || out.len() >= limit {
            break;
        }
        if let BlockItem::Paragraph(paragraph) = item {
            if !paragraph.is_hydrated() && paragraph.line_source().is_some() {
                out.push(cursor.start().item_count);
            }
        }
        cursor.next();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::model::StyleUpdateAction;
    use crate::render::model::test_utils::{TEST_STYLES, layout};
    use crate::render::model::{BlockSpacing, OffsetMap, Paragraph};

    fn dehydrated_paragraph(text: &str) -> Paragraph {
        let style = TEST_STYLES.base_text;
        let length = text.chars().count() + 1;
        Paragraph::dehydrated(
            LineSource::new(text, style, BlockSpacing::default(), &[]),
            estimate_paragraph_metrics(text, &style, style.font_size),
            OffsetMap::direct(length),
            length.into(),
            vec![],
            BlockSpacing::default(),
            None,
        )
    }

    fn state_with_rows(rows: usize) -> RenderState {
        let mut state =
            RenderState::new_for_test(TEST_STYLES, 400.0.into_pixels(), 100.0.into_pixels())
                .with_width_setting(WidthSetting::InfiniteWidth);
        let mut content = SumTree::new();
        for row in 0..rows {
            content.push(BlockItem::Paragraph(dehydrated_paragraph(&format!(
                "row {row}"
            ))));
        }
        state.set_content(content);
        state
    }

    fn is_hydrated(state: &RenderState, index: usize) -> bool {
        let content = state.content.borrow();
        let mut cursor = content.cursor::<ItemIndex, ()>();
        cursor.seek(&ItemIndex(index), SeekBias::Right);
        match cursor.item() {
            Some(BlockItem::Paragraph(paragraph)) => paragraph.is_hydrated(),
            _ => panic!("item {index} is not a paragraph"),
        }
    }

    /// Height-equality property: for unwrapped single-line text, the estimate and a real
    /// layout agree exactly on height, line count, first-line height and emptiness.
    #[test]
    fn estimate_matches_layout_for_unwrapped_text() {
        let style = TEST_STYLES.base_text;
        let long = "x".repeat(5_000);
        let texts = [
            "",
            "plain ascii text",
            "\tleading tab and\tinner",
            "日本語 中文 한국어",
            "emoji 👩‍👩‍👧‍👦 ✌🏽",
            long.as_str(),
        ];
        for text in texts {
            let frame = layout(text, &TEST_STYLES, f32::MAX);
            let measured = ParagraphMetrics::from_frame(&frame);
            let estimated = estimate_paragraph_metrics(text, &style, style.font_size);
            assert_eq!(estimated.height, measured.height, "height of {text:?}");
            assert_eq!(estimated.lines, measured.lines, "lines of {text:?}");
            assert_eq!(
                estimated.first_line_height, measured.first_line_height,
                "first line of {text:?}"
            );
            if !text.is_empty() {
                assert!(!estimated.is_empty, "{text:?}");
                assert!(!estimated.width_exact, "non-empty widths are estimates");
            }
        }
        // Empty text is exact and matches the real empty frame, which has no runs.
        let empty = ParagraphMetrics::from_frame(&warpui::text_layout::TextFrame::empty(
            style.font_size,
            style.line_height_ratio,
        ));
        assert_eq!(estimate_paragraph_metrics("", &style, 7.), empty);
    }

    #[test]
    fn estimate_width_for_ascii_matches_cells_times_advance() {
        let style = TEST_STYLES.base_text;
        let metrics = estimate_paragraph_metrics("abcd", &style, 8.);
        assert_eq!(metrics.width.as_f32(), 32.);
        // Wide characters count two cells, tabs count the tab size.
        let wide = estimate_paragraph_metrics("日", &style, 8.);
        assert_eq!(wide.width.as_f32(), 16.);
        let tab = estimate_paragraph_metrics("\t", &style, 8.);
        assert_eq!(
            tab.width.as_f32(),
            f32::from(style.fixed_width_tab_size.unwrap_or(4)) * 8.
        );
    }

    #[test]
    fn line_breaks_are_never_estimated() {
        for text in ["a\nb", "a\rb", "a\u{2028}b", "a\u{85}b"] {
            assert!(has_line_break(text), "{text:?}");
        }
        assert!(!has_line_break("plain\ttext"));
    }

    #[test]
    fn dehydrated_in_returns_only_rows_overlapping_the_range() {
        let state = state_with_rows(40);
        let row = TEST_STYLES.base_text.font_size * TEST_STYLES.base_text.line_height_ratio;
        let content = state.content.borrow();
        let range = YRange {
            start: 0.,
            end: row * 3.,
        };
        assert_eq!(dehydrated_in(&content, &range, usize::MAX), vec![0, 1, 2]);
        let range = YRange {
            start: row * 10.5,
            end: row * 12.5,
        };
        assert_eq!(
            dehydrated_in(&content, &range, usize::MAX),
            vec![10, 11, 12]
        );
        assert_eq!(dehydrated_in(&content, &range, 2), vec![10, 11]);
    }

    #[test]
    fn hydrating_and_evicting_keeps_sizing_and_neighbours() {
        let state = state_with_rows(40);
        let height_before = state.height();
        let (_, _) = state.take_hydration_flags();

        let frames = [3usize, 7, 20]
            .into_iter()
            .map(|index| {
                (
                    index,
                    ParagraphEdit::Hydrate(Arc::new(layout(
                        &format!("row {index}"),
                        &TEST_STYLES,
                        f32::MAX,
                    ))),
                )
            })
            .collect();
        state.apply_paragraph_edits(frames);
        for index in 0..40 {
            assert_eq!(
                is_hydrated(&state, index),
                matches!(index, 3 | 7 | 20),
                "row {index}"
            );
        }
        // The exact layout has the estimated height, so nothing moves.
        assert_eq!(state.height(), height_before);

        // Evict everything outside a window around the top; pinned row 20 survives.
        let row = TEST_STYLES.base_text.font_size * TEST_STYLES.base_text.line_height_ratio;
        let keep = YRange {
            start: 0.,
            end: row * 4.,
        };
        let pinned = [YRange {
            start: row * 20.,
            end: row * 21.,
        }];
        state.evict_outside(&keep, &pinned);
        for index in 0..40 {
            assert_eq!(
                is_hydrated(&state, index),
                matches!(index, 3 | 20),
                "row {index}"
            );
        }
        assert_eq!(state.height(), height_before);
    }

    #[test]
    fn hydrate_reports_size_changes_for_sizing_invalidation() {
        let state = state_with_rows(4);
        // A frame much wider than the estimate flips the sizing flag.
        let wide = Arc::new(layout(&"w".repeat(500), &TEST_STYLES, f32::MAX));
        state.apply_paragraph_edits(vec![(1, ParagraphEdit::Hydrate(wide))]);
        let (sizing_dirty, more_pending) = state.take_hydration_flags();
        assert!(sizing_dirty);
        assert!(!more_pending);
        // The flag is consumed.
        assert_eq!(state.take_hydration_flags(), (false, false));
    }

    /// A font-size change makes every parked width estimate stale: sizing is invalidated at the
    /// style change itself, and a change that needs no relayout leaves it alone.
    #[test]
    fn a_font_size_change_invalidates_sizing_for_parked_lines() {
        let mut state = state_with_rows(40);
        assert_eq!(state.take_hydration_flags(), (false, false));

        let mut repaint_only = TEST_STYLES;
        repaint_only.cursor_width += 1.;
        let action = state.update_styles(repaint_only);
        assert!(matches!(action, StyleUpdateAction::Repaint), "{action:?}");
        assert_eq!(state.take_hydration_flags(), (false, false));

        let mut bigger = state.styles().clone();
        bigger.base_text.font_size += 2.;
        let action = state.update_styles(bigger);
        assert!(matches!(action, StyleUpdateAction::Relayout), "{action:?}");
        assert_eq!(state.take_hydration_flags(), (true, false));
        // Consumed.
        assert_eq!(state.take_hydration_flags(), (false, false));
    }

    #[test]
    fn y_only_geometry_suppresses_dehydrated_line_reports() {
        let before = dehydrated_line_queries();
        y_only_geometry(|| report_dehydrated_lines());
        assert_eq!(dehydrated_line_queries(), before);
    }

    #[test]
    fn lazy_paragraphs_require_an_unwrapped_pixel_editor() {
        let wrapped =
            RenderState::new_for_test(TEST_STYLES, 100.0.into_pixels(), 100.0.into_pixels());
        assert!(!wrapped.lazy_paragraphs_enabled());
        let unwrapped = state_with_rows(1);
        assert_eq!(unwrapped.lazy_paragraphs_enabled(), !eager_layout_forced());
    }
}
