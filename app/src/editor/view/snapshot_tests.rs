use super::*;
use warpui::color::ColorU;

fn plain() -> TextStyle {
    TextStyle::default()
}

fn underlined() -> TextStyle {
    TextStyle::default().with_error_underline_color(ColorU::black())
}

fn runs_for(
    chars: impl IntoIterator<Item = (char, TextStyle)>,
) -> (String, Vec<(Range<usize>, StyleAndFont)>) {
    line_and_style_runs(chars, FamilyId(0), Properties::default())
}

fn style_and_font(style: TextStyle) -> StyleAndFont {
    StyleAndFont::new(FamilyId(0), Properties::default(), style)
}

/// A row in a single style is one run spanning the whole row, not one run per
/// character (#813).
#[test]
fn test_uniform_style_row_yields_one_run() {
    let text = "ls -la ~/projects";
    let (line, runs) = runs_for(text.chars().map(|ch| (ch, plain())));

    assert_eq!(line, text);
    assert_eq!(
        runs,
        vec![(0..text.chars().count(), style_and_font(plain()))]
    );
}

/// A style change ends the current run and starts a new one; a return to an earlier
/// style starts another run rather than re-joining the first.
#[test]
fn test_style_change_splits_runs() {
    let chars = "ab"
        .chars()
        .map(|ch| (ch, plain()))
        .chain("cde".chars().map(|ch| (ch, underlined())))
        .chain("f".chars().map(|ch| (ch, plain())));
    let (line, runs) = runs_for(chars);

    assert_eq!(line, "abcdef");
    assert_eq!(
        runs,
        vec![
            (0..2, style_and_font(plain())),
            (2..5, style_and_font(underlined())),
            (5..6, style_and_font(plain())),
        ]
    );
}

/// Run boundaries are char indices, not byte offsets: a multi-byte character still
/// advances the run by one.
#[test]
fn test_runs_are_indexed_by_char() {
    let (line, runs) = runs_for(
        "héllo"
            .chars()
            .map(|ch| (ch, plain()))
            .chain(" wörld".chars().map(|ch| (ch, underlined()))),
    );

    assert_eq!(line, "héllo wörld");
    assert_eq!(
        runs,
        vec![
            (0..5, style_and_font(plain())),
            (5..11, style_and_font(underlined())),
        ]
    );
}

/// An empty row lays out as an empty string with no runs at all.
#[test]
fn test_empty_row_yields_no_runs() {
    let (line, runs) = runs_for(std::iter::empty::<(char, TextStyle)>());

    assert_eq!(line, "");
    assert!(runs.is_empty());
}

// #1191: a large buffer lays out only the viewport, the ends and the cursor, with margins.
#[test]
fn test_small_buffer_lays_out_every_row() {
    assert!(large_buffer_real_rows(LARGE_BUFFER_ROWS - 1, 0..30, &[5]).is_none());
    // No window: every row is real.
    assert!(row_is_laid_out(None, 12345));
}

#[test]
fn test_large_buffer_windows_viewport_cursor_and_ends() {
    let total = 13_000;
    let ranges = large_buffer_real_rows(total, 5_000..5_030, &[9_000]).unwrap();
    let real = |row: u32| row_is_laid_out(Some(&ranges), row);

    // Viewport and its margin.
    assert!(real(5_000) && real(5_029));
    assert!(real(5_000 - OFFSCREEN_ROW_MARGIN) && real(5_030 + OFFSCREEN_ROW_MARGIN - 1));
    // The cursor and its margin.
    assert!(real(9_000) && real(9_000 + OFFSCREEN_ROW_MARGIN - 1));
    // The first and last rows (the left notch row, Ctrl-Home / Ctrl-End).
    assert!(real(0) && real(total - 1));
    // Far from all of them: a placeholder, not a layout. The old code laid out all 13,000 rows.
    assert!(!real(2_000) && !real(7_000) && !real(12_000 - 200));

    let laid_out = (0..total).filter(|row| real(*row)).count();
    assert!(laid_out < 1_500, "laid out {laid_out} of {total} rows");
}

// #1191: a tall viewport (many rows) widens the margin to four viewports, so a page up or down
// from the edge of the window still lands on laid-out rows.
#[test]
fn test_large_buffer_margin_grows_with_the_viewport() {
    let total = 20_000;
    let ranges = large_buffer_real_rows(total, 10_000..10_200, &[10_100]).unwrap();
    let real = |row: u32| row_is_laid_out(Some(&ranges), row);
    // Four viewports (800 rows) either side of the 200-row window, more than the 300 minimum.
    assert!(real(10_000 - 800) && real(10_200 + 800 - 1));
    let narrow = large_buffer_real_rows(total, 10_000..10_030, &[]).unwrap();
    assert!(row_is_laid_out(
        Some(&narrow),
        10_000 - OFFSCREEN_ROW_MARGIN
    ));
    assert!(!row_is_laid_out(
        Some(&narrow),
        10_000 - OFFSCREEN_ROW_MARGIN - 1
    ));
}

// #1191: the windowing may only run when no row can wrap, since a placeholder is one line tall.
#[test]
fn test_rows_cannot_wrap_needs_a_proof_that_every_row_fits() {
    // 13 px font, 1300 px wide: 100 ASCII chars fit even at a 1 em advance, 101 are not proven.
    assert!(rows_cannot_wrap(100, true, true, 13., 1300.));
    assert!(!rows_cannot_wrap(101, true, true, 13., 1300.));
    // Non-ASCII text uses the 1.5 em bound: 66 chars are proven, 67 are not.
    assert!(rows_cannot_wrap(66, false, true, 13., 1300.));
    assert!(!rows_cannot_wrap(67, false, true, 13., 1300.));
    // A tab advances to a tab stop: no proof, whatever the length.
    assert!(!rows_cannot_wrap(1, true, false, 13., 1300.));
    // Degenerate geometry never proves anything.
    assert!(!rows_cannot_wrap(1, true, true, 0., 1300.));
    assert!(!rows_cannot_wrap(1, true, true, 13., f32::INFINITY));
    assert!(!rows_cannot_wrap(1, true, true, f32::NAN, 1300.));
}

#[test]
fn test_full_layout_flag_parse() {
    assert!(!full_layout_flag_enabled(None));
    assert!(!full_layout_flag_enabled(Some("")));
    assert!(!full_layout_flag_enabled(Some("0")));
    assert!(!full_layout_flag_enabled(Some("off")));
    assert!(full_layout_flag_enabled(Some("1")));
    assert!(full_layout_flag_enabled(Some(" TRUE ")));
    assert!(full_layout_flag_enabled(Some("On")));
    assert!(full_layout_flag_enabled(Some("yes")));
}

#[test]
fn test_rows_cannot_wrap_measured_uses_the_real_ascii_advance() {
    // 100 ASCII chars of a 13 px monospace font (~0.6 em) fit in 1300 px, but the 1 em worst
    // case proved nothing for 101+ (#1333: a 99-char row in a ~1000 px window kept the whole 1 MB
    // buffer fully laid out on every frame).
    assert!(!rows_cannot_wrap(120, true, true, 13., 1000.));
    let fits =
        |ascii, tab_free, ems| rows_cannot_wrap_measured(120, ascii, tab_free, 13., 1000., ems);
    assert!(fits(true, true, Some(0.65)));
    // The measurement can only tighten the bound, never loosen it past 1 em or for non-ASCII.
    assert!(!fits(true, true, Some(5.0)));
    assert!(!fits(false, true, Some(0.65)));
    assert!(!fits(true, false, Some(0.65)));
    assert!(!fits(true, true, Some(f32::NAN)));
}
