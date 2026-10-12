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
