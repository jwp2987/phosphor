//! Property-style tests of parked line chunks through the render state (#1340 P5).
//!
//! A random document is held three ways: a plain `Vec<String>` model, a render state whose
//! dehydrated runs are parked into chunks, and an "eager" render state holding the same lines as
//! individual paragraphs (the P4 shape). Random edits (replace-N-with-M across chunk boundaries,
//! deletes, large pastes, whole-document replacement as a line-ending change produces), hydration
//! passes, unparking, eviction and re-parking are applied, and after every step:
//!
//! * the rendered text equals the plain model,
//! * the chunked tree's summary (length, height, width, lines) equals the eager tree's exactly,
//! * per-line metrics equal a fresh eager layout of the same text,
//! * y geometry (`y_offset_at_line`, `character_bounds`, soft-wrap rows) equals the eager tree's,
//! * every chunk satisfies its invariants.

use std::collections::HashMap;
use std::sync::Arc;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use string_offset::CharOffset;
use sum_tree::{Item, SeekBias, SumTree};
use vec1::vec1;
use warpui::units::IntoPixels;

use super::{ParagraphEdit, YRange, y_only_geometry};
use crate::content::edit::LaidOutRenderDelta;
use crate::render::model::parked::tests::dehydrated_line;
use crate::render::model::parked::{
    MAX_PARKED_CHARS, MAX_PARKED_LINES, MIN_REPARK_RUN, PARK_MIN_RUN, ParkedLines, Parker,
};
use crate::render::model::test_utils::{TEST_STYLES, layout};
use crate::render::model::{
    BlockItem, LayoutSummary, LineCount, ParagraphBlock, ParagraphMetrics, RenderLineLocation,
    RenderState, WidthSetting,
};

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

fn random_text(rng: &mut StdRng) -> String {
    const PIECES: &[&str] = &["a", "b", "zz", "0", " ", "\t", "é", "日", "{", "}", "👩‍👩‍👧‍👦"];
    match rng.gen_range(0..20) {
        0..=2 => String::new(),
        3 => "x".repeat(rng.gen_range(250..400)),
        _ => (0..rng.gen_range(1..40))
            .map(|_| PIECES[rng.gen_range(0..PIECES.len())])
            .collect(),
    }
}

/// One line as a block item: usually dehydrated, sometimes hydrated (non-empty lines only).
fn item_for(text: &str, rng: &mut StdRng) -> BlockItem {
    let mut paragraph = dehydrated_line(text, rng.gen_range(0..4));
    if !text.is_empty() && rng.gen_range(0..20) == 0 {
        paragraph.hydrate(Arc::new(layout(text, &TEST_STYLES, f32::MAX)));
    }
    BlockItem::Paragraph(paragraph)
}

fn items_for(lines: &[String], rng: &mut StdRng) -> Vec<BlockItem> {
    lines.iter().map(|line| item_for(line, rng)).collect()
}

/// The grouping `EditDelta::layout_delta` applies to a large dehydrated delta.
fn parked(items: Vec<BlockItem>) -> Vec<BlockItem> {
    let mut parker = Parker::new(PARK_MIN_RUN);
    for item in items {
        parker.push(item);
    }
    parker.finish()
}

fn state_with(items: Vec<BlockItem>) -> RenderState {
    let mut state =
        RenderState::new_for_test(TEST_STYLES, 400.0.into_pixels(), 100.0.into_pixels())
            .with_width_setting(WidthSetting::InfiniteWidth);
    let mut tree = SumTree::new();
    tree.extend(items);
    state.set_content(tree);
    state
}

/// Content offset of the start of line `line` (the tree offset: 0 for the first line).
fn offset_of_line(lines: &[String], line: usize) -> usize {
    lines[..line]
        .iter()
        .map(|text| text.chars().count() + 1)
        .sum()
}

/// Applies the delta a buffer edit replacing lines `first..last` with `new_items` produces.
fn apply_edit(
    state: &RenderState,
    lines_before: &[String],
    first: usize,
    last: usize,
    new_items: Vec<BlockItem>,
) {
    let start = offset_of_line(lines_before, first);
    let end = offset_of_line(lines_before, last);
    let trailing = match RenderState::final_trailing_newline_cursor(&state.styles) {
        BlockItem::TrailingNewLine(cursor) => cursor,
        other => panic!("expected a trailing newline, got {other:?}"),
    };
    state.layout_pending_edit(
        LaidOutRenderDelta {
            // Buffer offsets are 1-based.
            old_offset: CharOffset::from(start + 1)..CharOffset::from(end + 1),
            laid_out_line: new_items,
            trailing_newline: Some(trailing),
        },
        None,
    );
}

fn flatten(state: &RenderState) -> Vec<String> {
    let content = state.content();
    let mut lines = Vec::new();
    for item in content.block_items() {
        match item {
            BlockItem::Paragraph(paragraph) => {
                lines.push(paragraph.line_source().expect("source").text().to_owned())
            }
            BlockItem::ParkedLines(chunk) => {
                lines.extend((0..chunk.len()).map(|index| chunk.line_text(index).to_owned()))
            }
            BlockItem::TrailingNewLine(_) | BlockItem::TemporaryBlock { .. } => {}
            other => panic!("unexpected item {other:?}"),
        }
    }
    lines
}

fn chunk_count(state: &RenderState) -> usize {
    state
        .content()
        .block_items()
        .filter(|item| matches!(item, BlockItem::ParkedLines(_)))
        .count()
}

fn item_count(state: &RenderState) -> usize {
    state.content().block_items().count()
}

// ---------------------------------------------------------------------------------------------
// Invariants
// ---------------------------------------------------------------------------------------------

fn check_chunk(chunk: &ParkedLines) {
    assert!(
        chunk.len() >= 1 && chunk.len() <= MAX_PARKED_LINES,
        "{chunk:?}"
    );
    assert!(
        chunk.content_length().as_usize() <= MAX_PARKED_CHARS + MAX_PARKED_LINES,
        "{chunk:?}"
    );
    let mut previous_end = 0;
    for index in 0..chunk.len() {
        let chars = chunk.line_text(index).chars().count();
        assert_eq!(chunk.line_start(index), previous_end);
        assert_eq!(chunk.line_end(index) - chunk.line_start(index), chars + 1);
        previous_end = chunk.line_end(index);
        let mut run_end = 0;
        for (range, _) in chunk.line_runs(index) {
            assert_eq!(range.start, run_end, "runs tile line {index}");
            run_end = range.end;
        }
        assert_eq!(run_end, chars, "runs cover line {index}");
    }
    assert_eq!(previous_end, chunk.content_length().as_usize());

    // The chunk's tree summary is the sum of the summaries of the paragraphs it stands for.
    let mut expected = LayoutSummary::default();
    for paragraph in chunk.to_paragraphs() {
        expected += &BlockItem::Paragraph(paragraph).summary();
    }
    expected.item_count = 1;
    assert_eq!(BlockItem::ParkedLines(chunk.clone()).summary(), expected);
}

fn stride() -> f64 {
    f64::from(
        BlockItem::Paragraph(dehydrated_line("x", 0))
            .height()
            .as_f32(),
    )
}

fn check(chunked: &RenderState, eager: &RenderState, lines: &[String]) {
    // Text.
    assert_eq!(flatten(chunked), lines, "chunked text");
    assert_eq!(flatten(eager), lines, "eager text");

    // Summaries: the chunked tree has exactly the eager tree's dimensions.
    let (c, e) = (
        chunked.content.borrow().summary(),
        eager.content.borrow().summary(),
    );
    let expected_length: usize = lines.iter().map(|l| l.chars().count() + 1).sum::<usize>() + 1;
    assert_eq!(c.content_length.as_usize(), expected_length);
    assert_eq!(c.content_length, e.content_length);
    assert_eq!(c.height, e.height, "height drift");
    assert_eq!(c.width, e.width, "width drift");
    assert_eq!(c.lines, e.lines);
    assert_eq!(c.lines.as_usize(), lines.len() + 1);
    assert!(c.height >= lines.len() as f64 * stride());

    // Chunk invariants and per-line metrics against a fresh eager layout.
    let content = chunked.content();
    for item in content.block_items() {
        match item {
            BlockItem::ParkedLines(chunk) => {
                check_chunk(chunk);
                for index in 0..chunk.len() {
                    check_metrics(chunk.line_text(index), &chunk.line_metrics(index));
                }
            }
            BlockItem::Paragraph(paragraph) => check_metrics(
                paragraph.line_source().expect("source").text(),
                &paragraph.metrics(),
            ),
            _ => {}
        }
    }
    drop(content);
}

fn check_metrics(text: &str, cached: &ParagraphMetrics) {
    let fresh = if text.is_empty() {
        ParagraphMetrics::from_frame(&warpui::text_layout::TextFrame::empty(
            TEST_STYLES.base_text.font_size,
            TEST_STYLES.base_text.line_height_ratio,
        ))
    } else {
        ParagraphMetrics::from_frame(&layout(text, &TEST_STYLES, f32::MAX))
    };
    assert_eq!(cached.height, fresh.height, "height of {text:?}");
    assert_eq!(cached.lines, fresh.lines, "lines of {text:?}");
    assert_eq!(cached.is_empty, fresh.is_empty, "emptiness of {text:?}");
    assert_eq!(
        cached.first_line_height, fresh.first_line_height,
        "first line of {text:?}"
    );
}

/// y geometry of a few random lines agrees between the chunked and eager trees.
fn check_geometry(chunked: &RenderState, eager: &RenderState, lines: &[String], rng: &mut StdRng) {
    for _ in 0..6 {
        let line = rng.gen_range(0..lines.len());
        let line_count = LineCount::from(line);
        assert_eq!(
            chunked.content().y_offset_at_line(line_count),
            eager.content().y_offset_at_line(line_count),
            "y_offset_at_line({line})"
        );
        assert_eq!(
            chunked.vertical_offset_at_render_location(RenderLineLocation::Current(line_count)),
            eager.vertical_offset_at_render_location(RenderLineLocation::Current(line_count)),
            "vertical_offset_at_render_location({line})"
        );

        let start = offset_of_line(lines, line);
        let chars = lines[line].chars().count();
        for offset in [start, start + chars / 2, start + chars] {
            let offset = CharOffset::from(offset);
            let (chunked_bounds, eager_bounds) = y_only_geometry(|| {
                (
                    chunked.character_bounds(offset),
                    eager.character_bounds(offset),
                )
            });
            let (chunked_bounds, eager_bounds) = (
                chunked_bounds.expect("bounds"),
                eager_bounds.expect("bounds"),
            );
            assert!(
                (chunked_bounds.origin_y() - eager_bounds.origin_y()).abs() < 0.01,
                "character_bounds y of offset {offset} (line {line}): {chunked_bounds:?} vs {eager_bounds:?}"
            );
            let row = y_only_geometry(|| chunked.offset_to_softwrap_point(offset)).row();
            assert_eq!(row as usize, line, "softwrap row of offset {offset}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------------

fn random_edit(
    rng: &mut StdRng,
    lines: &mut Vec<String>,
    chunked: &RenderState,
    eager: &RenderState,
) {
    let count = lines.len();
    let first = rng.gen_range(0..count);
    let available = count - first;
    let span = match rng.gen_range(0..6) {
        0 => available,
        1 => 1,
        2 => rng.gen_range(1..=available.min(600)),
        _ => rng.gen_range(1..=available.min(5)),
    };
    let last = first + span;
    let mut new_count = match rng.gen_range(0..6) {
        0 => 0,
        1 => rng.gen_range(PARK_MIN_RUN..700),
        2 => 1,
        _ => rng.gen_range(1..6),
    };
    if new_count == 0 && span == count {
        // Keep the document non-empty (an empty buffer is one empty line).
        new_count = 1;
    }
    let new_lines: Vec<String> = (0..new_count).map(|_| random_text(rng)).collect();
    let items = items_for(&new_lines, rng);
    // Large deltas park their dehydrated runs, as `layout_delta` does.
    let chunked_items = if new_count >= PARK_MIN_RUN {
        parked(items.clone())
    } else {
        items.clone()
    };
    apply_edit(chunked, lines, first, last, chunked_items);
    apply_edit(eager, lines, first, last, items);
    lines.splice(first..last, new_lines);
}

fn random_y_range(rng: &mut StdRng, lines: &[String]) -> YRange {
    let height = (lines.len() as f64 * stride()) as f32;
    let start = rng.gen_range(0.0..height.max(1.0));
    YRange {
        start,
        end: start + rng.gen_range(1.0..4000.0),
    }
}

/// Hydrates one line in `state` the way the hydration pass does (expanding its chunk first).
fn hydrate_line(state: &RenderState, lines: &[String], line: usize) {
    if lines[line].is_empty() {
        return;
    }
    state.unpark_offsets(&[CharOffset::from(offset_of_line(lines, line))]);
    let index = {
        let content = state.content.borrow();
        let mut cursor = content.cursor::<LineCount, LayoutSummary>();
        cursor.seek(&LineCount::from(line), SeekBias::Right);
        match cursor.item() {
            Some(BlockItem::Paragraph(paragraph)) if !paragraph.is_hydrated() => {
                Some(cursor.start().item_count)
            }
            _ => None,
        }
    };
    if let Some(index) = index {
        let frame = Arc::new(layout(&lines[line], &TEST_STYLES, f32::MAX));
        state.apply_paragraph_edits(vec![(index, ParagraphEdit::Hydrate(frame))]);
    }
}

fn run_random_session(seed: u64, operations: usize) {
    let mut rng = StdRng::seed_from_u64(seed);
    let initial_count = rng.gen_range(1..800);
    let mut lines: Vec<String> = (0..initial_count).map(|_| random_text(&mut rng)).collect();
    let items = items_for(&lines, &mut rng);
    let chunked = state_with(parked(items.clone()));
    let eager = state_with(items);
    check(&chunked, &eager, &lines);
    if chunked.parking_enabled() && initial_count >= 400 {
        assert!(chunk_count(&chunked) > 0, "a 400+ line document is parked");
    }

    for _ in 0..operations {
        match rng.gen_range(0..12) {
            0..=6 => random_edit(&mut rng, &mut lines, &chunked, &eager),
            7 => {
                let range = random_y_range(&mut rng, &lines);
                chunked.unpark_ranges(&[range]);
            }
            8 => {
                let offsets: Vec<CharOffset> = (0..3)
                    .map(|_| {
                        let line = rng.gen_range(0..lines.len());
                        CharOffset::from(offset_of_line(&lines, line))
                    })
                    .collect();
                chunked.unpark_offsets(&offsets);
            }
            9 | 10 => {
                let line = rng.gen_range(0..lines.len());
                hydrate_line(&chunked, &lines, line);
                hydrate_line(&eager, &lines, line);
            }
            _ => {
                let keep = random_y_range(&mut rng, &lines);
                chunked.evict_outside(&keep, &[]);
                eager.evict_outside(&keep, &[]);
                if chunked.parking_enabled() {
                    chunked.repark_outside(&keep, &[]);
                }
            }
        }
        check(&chunked, &eager, &lines);
        check_geometry(&chunked, &eager, &lines, &mut rng);
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[test]
fn random_edit_sessions_match_the_plain_model_and_the_eager_tree() {
    for seed in 0..60 {
        run_random_session(seed, 40);
    }
}

#[test]
fn a_long_session_with_big_pastes_keeps_every_invariant() {
    for seed in 1000..1006 {
        run_random_session(seed, 250);
    }
}

fn numbered_lines(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("line {index}")).collect()
}

fn fixture(count: usize) -> (RenderState, RenderState, Vec<String>) {
    let lines = numbered_lines(count);
    let items: Vec<BlockItem> = lines
        .iter()
        .enumerate()
        .map(|(index, text)| BlockItem::Paragraph(dehydrated_line(text, (index % 3) as u8)))
        .collect();
    let chunked = state_with(parked(items.clone()));
    let eager = state_with(items);
    (chunked, eager, lines)
}

#[test]
fn a_big_dehydrated_document_is_a_handful_of_tree_items() {
    let (chunked, eager, lines) = fixture(5000);
    if !chunked.parking_enabled() {
        return;
    }
    check(&chunked, &eager, &lines);
    // 5000 lines at <= 256 per chunk, plus the trailing newline.
    assert!(
        item_count(&chunked) <= 5000 / 128 + 1,
        "{}",
        item_count(&chunked)
    );
    assert_eq!(item_count(&eager), 5001);
}

#[test]
fn an_edit_inside_one_chunk_splits_it_at_both_ends() {
    let (chunked, eager, mut lines) = fixture(1000);
    if !chunked.parking_enabled() {
        return;
    }
    let before = chunk_count(&chunked);
    // Replace lines 100..103 (inside the first 256-line chunk) with two lines.
    let new_lines = vec!["new a".to_owned(), "new b".to_owned()];
    let items: Vec<BlockItem> = new_lines
        .iter()
        .map(|text| BlockItem::Paragraph(dehydrated_line(text, 1)))
        .collect();
    apply_edit(&chunked, &lines, 100, 103, items.clone());
    apply_edit(&eager, &lines, 100, 103, items);
    lines.splice(100..103, new_lines);
    check(&chunked, &eager, &lines);
    // The chunk that contained the edit became a head chunk and a tail chunk around the two
    // new paragraphs: one more chunk than before.
    assert_eq!(chunk_count(&chunked), before + 1);
}

#[test]
fn deleting_every_line_but_one_leaves_a_consistent_tree() {
    let (chunked, eager, mut lines) = fixture(700);
    let item = BlockItem::Paragraph(dehydrated_line("only", 0));
    apply_edit(&chunked, &lines, 0, 700, vec![item.clone()]);
    apply_edit(&eager, &lines, 0, 700, vec![item]);
    lines = vec!["only".to_owned()];
    check(&chunked, &eager, &lines);
}

#[test]
fn unparking_and_reparking_round_trip() {
    let (chunked, eager, lines) = fixture(3000);
    if !chunked.parking_enabled() {
        return;
    }
    let chunks = chunk_count(&chunked);
    assert!(chunks > 0);

    // Expand everything.
    let all = YRange {
        start: 0.,
        end: f32::MAX,
    };
    assert_eq!(chunked.unpark_ranges(&[all]), 3000);
    assert_eq!(chunk_count(&chunked), 0);
    assert_eq!(item_count(&chunked), item_count(&eager));
    check(&chunked, &eager, &lines);

    // Re-park everything outside a small keep window.
    let keep = YRange {
        start: 0.,
        end: 2000.,
    };
    chunked.repark_outside(&keep, &[]);
    assert!(chunk_count(&chunked) > 0);
    assert!(item_count(&chunked) < 200, "{}", item_count(&chunked));
    check(&chunked, &eager, &lines);
    // The keep window itself stayed expanded.
    let content = chunked.content();
    let first = content.block_items().next().expect("items");
    assert!(matches!(first, BlockItem::Paragraph(_)));
}

#[test]
fn a_temporary_block_inside_a_chunk_lands_on_its_line() {
    let (chunked, eager, lines) = fixture(1000);
    let temporary = || {
        let mut blocks = HashMap::new();
        blocks.insert(
            LineCount::from(100),
            vec![BlockItem::TemporaryBlock {
                paragraph_block: ParagraphBlock::new(vec1![dehydrated_line("removed", 0)]),
                text_decoration: vec![],
                decoration: None,
            }],
        );
        blocks
    };
    chunked.reset_temporary_block(temporary());
    eager.reset_temporary_block(temporary());
    check(&chunked, &eager, &lines);

    for state in [&chunked, &eager] {
        let content = state.content();
        let mut seen_lines = 0;
        let mut at = None;
        for item in content.block_items() {
            if matches!(item, BlockItem::TemporaryBlock { .. }) {
                at = Some(seen_lines);
            } else if !matches!(item, BlockItem::TrailingNewLine(_)) {
                seen_lines += item.lines().as_usize();
            }
        }
        assert_eq!(at, Some(100), "the temporary block sits before line 100");
    }
}

#[test]
fn splitting_at_a_non_line_boundary_expands_the_chunk() {
    let lines = numbered_lines(40);
    let items: Vec<BlockItem> = lines
        .iter()
        .map(|text| BlockItem::Paragraph(dehydrated_line(text, 0)))
        .collect();
    let state = state_with(parked(items));
    if !state.parking_enabled() {
        return;
    }
    // Offset 3 is in the middle of "line 0".
    state.split_parked_chunks_at(&[CharOffset::from(3)]);
    assert_eq!(flatten(&state), lines);
    let content = state.content();
    assert!(
        matches!(content.block_items().next(), Some(BlockItem::Paragraph(_))),
        "the chunk was expanded into paragraphs"
    );
}

#[test]
fn min_repark_run_keeps_short_runs_as_paragraphs() {
    let lines = numbered_lines(MIN_REPARK_RUN - 1);
    let items: Vec<BlockItem> = lines
        .iter()
        .map(|text| BlockItem::Paragraph(dehydrated_line(text, 0)))
        .collect();
    let state = state_with(items);
    if !state.parking_enabled() {
        return;
    }
    state.repark_outside(
        &YRange {
            start: 1.0e9,
            end: 1.1e9,
        },
        &[],
    );
    assert_eq!(chunk_count(&state), 0, "too short to be worth a chunk");
}
