use float_cmp::assert_approx_eq;

use crate::scene::ZIndex;
use crate::App;

use super::*;

#[test]
fn test_laid_out_text_height() {
    App::test((), |mut app| async move {
        app.update(|_ctx| {
            let text_frame = TextFrame::mock("foo\nbar\nbaz");
            let line_count = text_frame.lines().len();
            let laid_out_text = LaidOutText::Frame(Arc::new(text_frame));
            let height = laid_out_text.height();
            let expected = 13. * 1.2 * line_count as f32;
            assert_approx_eq!(f32, height, expected);
        });
    });
}

/// We calculate height of a line by multiplying the line's font size by line
/// height ratio. This test ensures that the height of a laid out line respects
/// this calculation.
#[test]
fn test_laid_out_line_height() {
    App::test((), |mut app| async move {
        app.update(|_ctx| {
            let line = Line::mock_from_str("foo");
            let laid_out_line = LaidOutText::Line(Arc::new(line));
            let height = laid_out_line.height();

            // 13 and 1.2 are the default font size and line height ratios, respectively.
            let expected = 13. * 1.2;
            assert_approx_eq!(f32, height, expected);
        });
    });
}

#[test]
fn test_single_line_char_hit_testing_respects_y_bounds() {
    App::test((), |mut app| async move {
        app.update(|_ctx| {
            let mut line = Line::mock_from_str("foo");
            line.width = 30.;
            line.runs[0].width = 30.;
            let line = Arc::new(line);
            let line_height = line.height();
            let mut text = Text::new_inline("foo", crate::fonts::FamilyId(0), 13.);
            text.laid_out_text = LaidOutText::Line(Arc::clone(&line));
            text.origin = Some(Point::from_vec2f(vec2f(10., 20.), ZIndex::new(0)));

            assert!(text.get_char_index(&vec2f(10., 19.9)).is_none());
            assert!(text
                .get_char_index(&vec2f(10., 20. + line_height + 0.1))
                .is_none());
            assert!(text
                .get_char_index(&vec2f(10., 20. + line_height / 2.))
                .is_some());
        });
    });
}

#[test]
fn test_merge_non_overlapping_ranges() {
    let highlight = Highlight::new();

    let range1 = HighlightedRange {
        highlight,
        highlight_indices: vec![1, 2, 3],
    };
    let range2 = HighlightedRange {
        highlight,
        highlight_indices: vec![5, 6, 7],
    };

    let result = HighlightedRange::merge_overlapping_ranges(vec![range1.clone(), range2.clone()]);

    assert_eq!(result, vec![range1, range2]);
}

#[test]
fn test_merge_contiguous_ranges() {
    let highlight = Highlight::new();

    let range1 = HighlightedRange {
        highlight,
        highlight_indices: vec![1, 2, 3],
    };
    let range2 = HighlightedRange {
        highlight,
        highlight_indices: vec![4, 5, 6],
    };

    let result = HighlightedRange::merge_overlapping_ranges(vec![range1.clone(), range2.clone()]);

    assert_eq!(
        result,
        vec![HighlightedRange {
            highlight,
            highlight_indices: vec![1, 2, 3, 4, 5, 6],
        }]
    );
}

#[test]
fn test_merge_overlapping_ranges() {
    let highlight = Highlight::new();

    let range1 = HighlightedRange {
        highlight,
        highlight_indices: vec![1, 2, 3],
    };
    let range2 = HighlightedRange {
        highlight,
        highlight_indices: vec![3, 4, 5],
    };

    let result = HighlightedRange::merge_overlapping_ranges(vec![range1.clone(), range2.clone()]);

    assert_eq!(
        result,
        vec![HighlightedRange {
            highlight,
            highlight_indices: vec![1, 2, 3, 4, 5],
        }]
    );
}

#[test]
fn test_merge_single_range() {
    let highlight = Highlight::new();

    let range = HighlightedRange {
        highlight,
        highlight_indices: vec![1, 2, 3],
    };

    let result = HighlightedRange::merge_overlapping_ranges(vec![range.clone()]);

    assert_eq!(result, vec![range]);
}

#[test]
fn test_merge_empty_ranges() {
    let result = HighlightedRange::merge_overlapping_ranges(vec![]);
    assert!(result.is_empty());
}

const TEST_GLYPH_ADVANCE: f32 = 10.0;

/// Builds a single-line `Text` element positioned at origin (0, 0) whose glyphs each advance by
/// [`TEST_GLYPH_ADVANCE`], so `expand_selection` results can be checked by x position. Mirrors
/// the equivalent helper in `formatted_text_element_tests.rs`.
fn positioned_text(text: &str) -> Text {
    let text_frame = Arc::new(TextFrame::mock_with_positions(text, TEST_GLYPH_ADVANCE));
    let mut element = Text::new_inline(text.to_string(), FamilyId(0), 13.0);
    element.origin = Some(Point::new(0.0, 0.0, ZIndex::new(0)));
    element.size = Some(vec2f(text_frame.max_width(), text_frame.height()));
    element.laid_out_text = LaidOutText::Frame(text_frame);
    element
}

/// Drives `expand_selection` for a semantic selection whose tail is over the glyph at
/// `glyph_index`, returning the resulting x position. The element is laid out so x == glyph index
/// times [`TEST_GLYPH_ADVANCE`], so the caller can assert which character boundary it snapped to.
fn semantic_target_x(element: &Text, glyph_index: usize, direction: SelectionDirection) -> f32 {
    // Aim near the middle of the target glyph so the point snaps to it.
    let point = vec2f(glyph_index as f32 * TEST_GLYPH_ADVANCE + 2.0, 5.0);
    element
        .expand_selection(
            point,
            direction,
            SelectionType::Semantic,
            &WordBoundariesPolicy::Default,
        )
        .expect("expand_selection should return a point")
        .x()
}

/// Regression test for #213 (same defect class as #163, already fixed for
/// `FormattedTextElement` in commit 6852eb048): `Text::expand_selection` for `Semantic` selection
/// must stop at the end of a punctuation run and exclude trailing whitespace, matching
/// `FormattedTextElement` and the oracle pin (`crates/warpui_core/src/elements/gui/text.rs` at
/// `02b53fcd8`), rather than expanding all the way to the end of the string.
#[test]
fn expand_selection_excludes_trailing_whitespace_after_punctuation() {
    // "alpha, bravo": a0 l1 p2 h3 a4 ,5 <space>6 b7 r8 a9 v10 o11
    let element = positioned_text("alpha, bravo");

    // Forward drag with the tail on the comma ends just after the comma (x == 6 * advance),
    // NOT including the following space (which would be x == 7 * advance).
    assert_eq!(
        semantic_target_x(&element, 5, SelectionDirection::Forward),
        6.0 * TEST_GLYPH_ADVANCE,
    );
    // Forward drag with the tail on a word char selects the whole word "alpha" (end x == 5 * advance).
    assert_eq!(
        semantic_target_x(&element, 1, SelectionDirection::Forward),
        5.0 * TEST_GLYPH_ADVANCE,
    );
    // Backward drag with the tail on a word char selects from the start of "bravo" (x == 7 * advance).
    assert_eq!(
        semantic_target_x(&element, 8, SelectionDirection::Backward),
        7.0 * TEST_GLYPH_ADVANCE,
    );
}

/// Regression test for #213 (same defect class as #163): semantic selection expansion must stop
/// at the end of a punctuation run rather than continuing past it to the end of the string.
#[test]
fn expand_selection_stops_at_end_of_punctuation_run() {
    // "foo... bar": f0 o1 o2 .3 .4 .5 <space>6 b7 a8 r9
    let element = positioned_text("foo... bar");

    // Tail on the last dot ends at x == 6 * advance ("foo..."), excluding the trailing space
    // (which would be x == 7 * advance) and never reaching "bar".
    assert_eq!(
        semantic_target_x(&element, 5, SelectionDirection::Forward),
        6.0 * TEST_GLYPH_ADVANCE,
    );
}

#[test]
fn test_merge_adjacent_non_contiguous_ranges() {
    let highlight1 = Highlight::new();
    let highlight2 = Highlight::new();

    let range1 = HighlightedRange {
        highlight: highlight1,
        highlight_indices: vec![1, 2],
    };
    let range2 = HighlightedRange {
        highlight: highlight2,
        highlight_indices: vec![4, 5],
    };

    let result = HighlightedRange::merge_overlapping_ranges(vec![range1.clone(), range2.clone()]);

    assert_eq!(result, vec![range1, range2]);
}
