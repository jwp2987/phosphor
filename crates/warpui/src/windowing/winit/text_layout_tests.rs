use super::*;
use crate::fonts::{collect_glyph_indices, init_fonts, Properties};
use crate::platform::FontDB as _;
use crate::{
    elements::DEFAULT_UI_LINE_HEIGHT_RATIO,
    text_layout::{TextStyle, DEFAULT_TOP_BOTTOM_RATIO},
};
use anyhow::Result;

const FONT_SIZE: f32 = 16.;
const FRAME_WIDTH: f32 = 80.;
const FRAME_HEIGHT: f32 = f32::MAX;

#[test]
fn test_fixed_width_tab_size_affects_tab_width() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let tabbed = "\tX";
    let spaced = "        X";

    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: Some(8),
    };

    let tabbed_line = font_db.text_layout_system().layout_line(
        tabbed,
        line_style,
        &[(
            0..tabbed.chars().count(),
            StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
        )],
        f32::MAX,
        crate::text_layout::ClipConfig::default(),
    );
    let spaced_line = font_db.text_layout_system().layout_line(
        spaced,
        line_style,
        &[(
            0..spaced.chars().count(),
            StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
        )],
        f32::MAX,
        crate::text_layout::ClipConfig::default(),
    );

    let error = (tabbed_line.width - spaced_line.width).abs();
    assert!(
        error < 1.0,
        "expected tab width ~= 8 spaces; got tabbed {}, spaced {} (error {})",
        tabbed_line.width,
        spaced_line.width,
        error
    );

    Ok(())
}

#[test]
fn test_layout_text_first_line_indent_small() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "Let's lay out s𐍈me Roboto text.";
    //          0123456789012345678901234567890
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.encode_utf16().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    // First, lay out the text with no head indent.
    let no_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        None,
    );

    // The text should contain multiple lines.
    // The first line has about the same amount of content as the others,
    // since there's no head indent.
    assert_eq!(no_indent_frame.lines().len(), 4);
    assert_eq!(
        collect_glyph_indices(&no_indent_frame),
        vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],      // 9 is whitespace.
            vec![10, 11, 12, 13, 14, 15, 16, 17], // 18 is whitespace.
            vec![19, 20, 21, 22, 23, 24],         // 25 is whitespace.
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(&no_indent_frame, 0., FRAME_WIDTH));
    assert!(all_lines_bounded(&no_indent_frame, FRAME_WIDTH));

    // Lay out the text with a 5px head indent.
    let small_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(5.),
    );

    // The first line has about the same amount of content as the others,
    // since the head indent is small.
    assert_eq!(small_indent_frame.lines().len(), 4);
    assert_eq!(
        collect_glyph_indices(&small_indent_frame),
        vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            vec![10, 11, 12, 13, 14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(&small_indent_frame, 5., FRAME_WIDTH));
    assert!(all_lines_bounded(&small_indent_frame, FRAME_WIDTH));

    // Lay out the text with a 40px head indent,
    // which is half the width of the frame.
    let half_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH / 2.),
    );

    // The text contains an additional line to accommodate the indent.
    assert_eq!(half_indent_frame.lines().len(), 5);
    assert_eq!(
        collect_glyph_indices(&half_indent_frame),
        vec![
            vec![0, 1, 2, 3, 4],          // Fewer glyphs fit on this line. 5 is whitespace.
            vec![6, 7, 8, 9, 10, 11, 12], // 13 is whitespace.
            vec![14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(
        &half_indent_frame,
        FRAME_WIDTH / 2.,
        FRAME_WIDTH,
    ));
    assert!(all_lines_bounded(&half_indent_frame, FRAME_WIDTH));

    Ok(())
}

#[test]
fn test_layout_text_first_line_indent_medium() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "Let's lay out s𐍈me Roboto text.";
    //          0123456789012345678901234567890
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.encode_utf16().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    // First, lay out the text with no head indent.
    let no_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(0.),
    );

    // The text should contain multiple lines.
    // The first line has about the same amount of content as the others,
    // since there's no head indent.
    assert_eq!(no_indent_frame.lines().len(), 4);
    assert_eq!(
        collect_glyph_indices(&no_indent_frame),
        vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            vec![10, 11, 12, 13, 14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(&no_indent_frame, 0., FRAME_WIDTH));
    assert!(all_lines_bounded(&no_indent_frame, FRAME_WIDTH));

    // Lay out the text with a head indent that's 15px smaller than
    // the width of the frame.
    let overflow_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH - 20.),
    );

    // The first line should have some glyphs on it, but not the whole
    // first word.
    assert_eq!(overflow_indent_frame.lines().len(), 5);
    assert_eq!(
        collect_glyph_indices(&overflow_indent_frame),
        vec![
            vec![0, 1], // Only a few glyphs fit.
            vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            vec![14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(
        &overflow_indent_frame,
        FRAME_WIDTH - 20.,
        FRAME_WIDTH,
    ));
    assert!(all_lines_bounded(&overflow_indent_frame, FRAME_WIDTH));

    Ok(())
}

#[test]
fn test_layout_text_first_line_indent_large() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "Let's lay out s𐍈me Roboto text.";
    //          0123456789012345678901234567890
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.encode_utf16().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    // First, lay out the text with no head indent.
    let no_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(0.),
    );

    // The text should contain multiple lines.
    // The first line has about the same amount of content as the others,
    // since there's no head indent.
    assert_eq!(no_indent_frame.lines().len(), 4);
    assert_eq!(
        collect_glyph_indices(&no_indent_frame),
        vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            vec![10, 11, 12, 13, 14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(&no_indent_frame, 0., FRAME_WIDTH));
    assert!(all_lines_bounded(&no_indent_frame, FRAME_WIDTH));

    // Lay out the text with a head indent that's 5px bigger than the width of the frame.
    let overflow_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH + 5.),
    );

    // The first line is left entirely blank since no glyphs fit on it.
    assert_eq!(
        collect_glyph_indices(&overflow_indent_frame),
        vec![
            vec![], // No glyphs fit on this line.
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            vec![10, 11, 12, 13, 14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(
        &overflow_indent_frame,
        FRAME_WIDTH + 5.,
        FRAME_WIDTH,
    ));
    assert!(all_lines_bounded(&overflow_indent_frame, FRAME_WIDTH));

    // Lay out the text with a 79px head indent,
    // which spans almost the entire width of the frame.
    let big_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH - 0.1),
    );

    // The first line is left entirely blank since no glyphs fit on it.
    assert_eq!(big_indent_frame.lines().len(), 5);
    assert_eq!(
        collect_glyph_indices(&big_indent_frame),
        vec![
            vec![], // No glyphs fit on this line.
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            vec![10, 11, 12, 13, 14, 15, 16, 17],
            vec![19, 20, 21, 22, 23, 24],
            vec![26, 27, 28, 29, 30],
        ]
    );
    assert!(first_line_bounded(
        &big_indent_frame,
        FRAME_WIDTH - 0.1,
        FRAME_WIDTH,
    ));
    assert!(all_lines_bounded(&big_indent_frame, FRAME_WIDTH));

    Ok(())
}

// TODO(PLAT-779): check all line bounds once bidirectional wrapping is fixed in cosmic-text.
// See https://github.com/pop-os/cosmic-text/issues/252.
#[test]
fn test_layout_text_first_line_indent_small_bidirectional() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "brekkie, إفطار, lunch (غداء) and dinner - عشاء";
    //          0123456783210945678901265437890123456789015432
    // RTL spans:       |-----|       |----|             |----|
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.encode_utf16().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    // First, lay out the text with no head indent.
    let no_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        None,
    );

    // The text should contain multiple lines.
    // The first line has about the same amount of content as the others,
    // since there's no head indent.
    assert_eq!(no_indent_frame.lines().len(), 4);
    assert!(first_line_bounded(&no_indent_frame, 0., FRAME_WIDTH));
    // assert!(all_lines_bounded(&no_indent_frame, FRAME_WIDTH));

    // Lay out the text with a 5px head indent.
    let small_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(5.),
    );

    // The first line has about the same amount of content as the others,
    // since the head indent is small.
    assert_eq!(small_indent_frame.lines().len(), 4);
    assert!(first_line_bounded(&small_indent_frame, 5., FRAME_WIDTH));
    // assert!(all_lines_bounded(&small_indent_frame, FRAME_WIDTH));

    // Lay out the text with a 40px head indent,
    // which is half the width of the frame.
    let half_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH / 2.),
    );

    // The text contains an additional line to accommodate the indent.
    assert_eq!(half_indent_frame.lines().len(), 5);
    assert!(first_line_bounded(
        &half_indent_frame,
        FRAME_WIDTH / 2.,
        FRAME_WIDTH,
    ));
    // assert!(all_lines_bounded(&half_indent_frame, FRAME_WIDTH));

    Ok(())
}

// TODO(PLAT-779): check all line bounds once bidirectional wrapping is fixed in cosmic-text.
// See https://github.com/pop-os/cosmic-text/issues/252.
#[test]
fn test_layout_text_first_line_indent_medium_bidirectional() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "brekkie, إفطار, lunch (غداء) and dinner - عشاء";
    //          0123456783210945678901265437890123456789015432
    // RTL spans:       |-----|       |----|             |----|
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.encode_utf16().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    // First, lay out the text with no head indent.
    let no_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        None,
    );

    // The text should contain multiple lines.
    // The first line has about the same amount of content as the others,
    // since there's no head indent.
    assert_eq!(no_indent_frame.lines().len(), 4);
    assert!(first_line_bounded(&no_indent_frame, 0., FRAME_WIDTH));
    // assert!(all_lines_bounded(&no_indent_frame, FRAME_WIDTH));

    // Lay out the text with a head indent that's 15px smaller than
    // the width of the frame.
    let overflow_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH - 20.),
    );

    // The first line should have some glyphs on it, but not the whole
    // first word.
    assert_eq!(overflow_indent_frame.lines().len(), 5);
    assert!(first_line_bounded(
        &overflow_indent_frame,
        FRAME_WIDTH - 20.,
        FRAME_WIDTH,
    ));
    // assert!(all_lines_bounded(&overflow_indent_frame, FRAME_WIDTH));

    Ok(())
}

// TODO(PLAT-779): check all line bounds once bidirectional wrapping is fixed in cosmic-text.
// See https://github.com/pop-os/cosmic-text/issues/252.
#[test]
fn test_layout_text_first_line_indent_large_bidirectional() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "brekkie, إفطار, lunch (غداء) and dinner - عشاء";
    //          0123456783210945678901265437890123456789015432
    // RTL spans:       |-----|       |----|             |----|
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.encode_utf16().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    // First, lay out the text with no head indent.
    let no_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(0.),
    );

    // The text should contain multiple lines.
    // The first line has about the same amount of content as the others,
    // since there's no head indent.
    assert_eq!(no_indent_frame.lines().len(), 4);
    assert!(first_line_bounded(&no_indent_frame, 0., FRAME_WIDTH));
    // assert!(all_lines_bounded(&no_indent_frame, FRAME_WIDTH));

    // Lay out the text with a head indent that's 5px bigger than the width of the frame.
    let overflow_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH + 5.),
    );

    // The first line is left entirely blank since no glyphs fit on it.
    assert_eq!(overflow_indent_frame.lines().len(), 5);
    assert!(collect_glyph_indices(&overflow_indent_frame)
        .first()
        .unwrap()
        .is_empty(),);
    assert!(first_line_bounded(
        &overflow_indent_frame,
        FRAME_WIDTH + 5.,
        FRAME_WIDTH,
    ));
    // assert!(all_lines_bounded(&overflow_indent_frame, FRAME_WIDTH));

    // Lay out the text with a 79px head indent,
    // which spans almost the entire width of the frame.
    let big_indent_frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        Some(FRAME_WIDTH - 0.1),
    );

    // The first line is left entirely blank since no glyphs fit on it.
    assert_eq!(big_indent_frame.lines().len(), 5);
    assert!(collect_glyph_indices(&big_indent_frame)
        .first()
        .unwrap()
        .is_empty(),);
    assert!(first_line_bounded(
        &big_indent_frame,
        FRAME_WIDTH - 0.1,
        FRAME_WIDTH,
    ));
    // assert!(all_lines_bounded(&big_indent_frame, FRAME_WIDTH));

    Ok(())
}

/// Combining marks must be placed with the shaper's GPOS offsets, not with the pen position.
///
/// [`cosmic_text::LayoutGlyph`] reports the shaped placement of a glyph in `x_offset` / `y_offset`
/// (em units, to be scaled by `font_size`), separately from `x` / `y`, which are the pen position
/// after advance accumulation. Ignoring the offsets draws every zero-advance combining mark at the
/// pen instead of on its base.
///
/// This is invisible with a single mark — most fonts draw mark glyphs extending left of their
/// origin, so a mark placed at the pen happens to land over its base — and only becomes visible
/// once two marks stack, since both then get identical coordinates.
#[test]
fn test_combining_marks_are_placed_with_gpos_offsets() -> Result<()> {
    use std::path::PathBuf;

    let (mut font_db, _roboto) = init_fonts();

    // The bundled Roboto-Regular doesn't cover COMBINING DIAERESIS, and carries no mark anchors
    // for COMBINING ACUTE ACCENT either, so it can't exercise GPOS mark positioning. RobotoFlex,
    // also bundled, has both marks plus mark-to-base and mark-to-mark anchors.
    let font_path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "app",
        "assets",
        "bundled",
        "fonts",
        "roboto",
        "RobotoFlex-Semibold.ttf",
    ]
    .iter()
    .collect();
    let roboto_flex = font_db
        .load_from_bytes(
            "RobotoFlex",
            vec![std::fs::read(font_path).expect("should be able to read the bundled RobotoFlex")],
        )
        .expect("should be able to load RobotoFlex for test");

    // `b` has no precomposed form with either mark, so the shaper cannot compose them away and
    // emits three glyphs: the base, then two stacked zero-advance marks.
    let text = "b\u{0301}\u{0308}"; // b + COMBINING ACUTE ACCENT + COMBINING DIAERESIS

    let line = font_db.text_layout_system().layout_line(
        text,
        LineStyle {
            font_size: FONT_SIZE,
            line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
            baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
            fixed_width_tab_size: None,
        },
        &[(
            0..text.chars().count(),
            StyleAndFont::new(roboto_flex, Properties::default(), TextStyle::new()),
        )],
        f32::MAX,
        crate::text_layout::ClipConfig::default(),
    );

    let glyphs = line
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .collect_vec();
    assert_eq!(
        glyphs.len(),
        3,
        "expected a base glyph followed by two combining marks, got {glyphs:?}"
    );

    let base = glyphs[0];
    let (acute, diaeresis) = (glyphs[1], glyphs[2]);

    assert!(base.width > 0., "the base glyph should advance the pen");
    assert_eq!(
        (
            base.position_along_baseline.x(),
            base.position_along_baseline.y()
        ),
        (0., 0.),
        "the base glyph starts at the origin"
    );

    for mark in [acute, diaeresis] {
        assert_eq!(mark.width, 0., "combining marks have no advance");
        // Without the GPOS offsets a mark keeps the pen position it inherited from the base's
        // advance, which places it after the base instead of over it.
        assert!(
            mark.position_along_baseline.x() < base.width,
            "mark should be pulled back over its base by its GPOS x offset, got x = {} with a \
             base advance of {}",
            mark.position_along_baseline.x(),
            base.width
        );
        // Screen coordinates grow downwards, so a mark rendered above the baseline has a negative
        // y. Without the GPOS offsets every mark stays on the baseline.
        assert!(
            mark.position_along_baseline.y() < 0.,
            "mark should be raised above the baseline by its GPOS y offset, got y = {}",
            mark.position_along_baseline.y()
        );
    }

    // The point of the fix: stacked marks must not collapse onto one another. The diaeresis is
    // attached to the acute by the font's mark-to-mark anchors, so it sits strictly higher.
    assert!(
        diaeresis.position_along_baseline.y() < acute.position_along_baseline.y(),
        "the second mark should stack above the first, but they are at y = {} and y = {}",
        diaeresis.position_along_baseline.y(),
        acute.position_along_baseline.y()
    );

    Ok(())
}

/// Checks that the head indent and first line's width don't exceed the frame's width.
fn first_line_bounded(frame: &TextFrame, first_line_indent: f32, frame_width: f32) -> bool {
    let first_line_width = frame.lines().first().unwrap().width;
    first_line_width + first_line_indent.min(frame_width) <= frame_width
}

#[test]
fn test_layout_text_chinese_soft_wrap_caret_indices() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "人生是一场无法回头的旅行走过的路遇见过的人经历过的风雨都成了生命里最珍贵的风景";
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let style_runs = [(
        0..text.chars().count(),
        StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
    )];

    let frame = font_db.text_layout_system().layout_text(
        text,
        line_style,
        &style_runs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        Default::default(),
        None,
    );

    let glyph_indices = collect_glyph_indices(&frame);
    assert!(
        glyph_indices.len() > 1,
        "expected soft wrap: {glyph_indices:?}"
    );

    let flattened = glyph_indices.iter().flatten().copied().collect::<Vec<_>>();
    assert_eq!(flattened, (0..text.chars().count()).collect::<Vec<_>>());

    for (line, glyphs) in frame.lines().iter().zip(glyph_indices.iter()) {
        let Some(first_glyph) = glyphs.first() else {
            continue;
        };
        let Some(last_glyph) = glyphs.last() else {
            continue;
        };
        assert_eq!(line.first_index(), *first_glyph);
        assert_eq!(line.last_index(), *last_glyph);
        assert_eq!(line.end_index(), *last_glyph + 1);
    }

    Ok(())
}

#[test]
fn test_softwrap_caret_positions_are_contiguous() -> Result<()> {
    let (font_db, font_family) = init_fonts();

    // A single paragraph (no newlines) long enough to soft-wrap at 200px.
    let text = "The quick brown fox jumps over the lazy dog and then keeps running onward";
    let frame = font_db.text_layout_system().layout_text(
        text,
        LineStyle {
            font_size: FONT_SIZE,
            line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
            baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
            fixed_width_tab_size: None,
        },
        &[(
            0..text.chars().count(),
            StyleAndFont::new(font_family, Properties::default(), TextStyle::new()),
        )],
        200.,
        f32::MAX,
        TextAlignment::Left,
        None,
    );

    // Should wrap onto multiple lines.
    assert!(
        frame.lines().len() >= 2,
        "Expected at least 2 lines but got {}",
        frame.lines().len()
    );

    // Collect all caret position start_offsets across all lines.
    let all_caret_starts: Vec<usize> = frame
        .lines()
        .iter()
        .flat_map(|line| line.caret_positions.iter().map(|c| c.start_offset))
        .collect();

    // The caret positions should be monotonically non-decreasing across all lines.
    // Before the fix, the second/third wrapped line's carets would reset to 0.
    for window in all_caret_starts.windows(2) {
        assert!(
            window[0] <= window[1],
            "Caret positions are not monotonically non-decreasing: {} > {} (all: {:?})",
            window[0],
            window[1],
            all_caret_starts
        );
    }

    // The first caret should start at 0 and the last should correspond to near the end of the text.
    assert_eq!(
        *all_caret_starts.first().unwrap(),
        0,
        "First caret should start at 0"
    );
    let last_caret = frame
        .lines()
        .last()
        .unwrap()
        .caret_positions
        .last()
        .unwrap();
    assert!(
        last_caret.last_offset > 0,
        "Last caret offset should be > 0"
    );

    // Each wrapped line's first caret should pick up where the previous line left off.
    for i in 1..frame.lines().len() {
        let prev_line = &frame.lines()[i - 1];
        let curr_line = &frame.lines()[i];
        if let (Some(prev_last), Some(curr_first)) = (
            prev_line.caret_positions.last(),
            curr_line.caret_positions.first(),
        ) {
            assert!(
                curr_first.start_offset > prev_last.start_offset,
                "Line {}'s first caret ({}) should be after line {}'s last caret ({})",
                i,
                curr_first.start_offset,
                i - 1,
                prev_last.start_offset
            );
        }
    }

    Ok(())
}

#[test]
fn test_softwrap_caret_positions_multi_paragraph() -> Result<()> {
    let (font_db, font_family) = init_fonts();

    // Two paragraphs, each long enough to soft-wrap.
    let text = "The quick brown fox jumps over the lazy dog repeatedly\nAnother paragraph that \
        also wraps around when narrow";
    let frame = font_db.text_layout_system().layout_text(
        text,
        LineStyle {
            font_size: FONT_SIZE,
            line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
            baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
            fixed_width_tab_size: None,
        },
        &[(
            0..text.chars().count(),
            StyleAndFont::new(font_family, Properties::default(), TextStyle::new()),
        )],
        200.,
        f32::MAX,
        TextAlignment::Left,
        None,
    );

    // Should have multiple lines from wrapping.
    assert!(
        frame.lines().len() >= 3,
        "Expected at least 3 lines but got {}",
        frame.lines().len()
    );

    // Caret positions should be monotonically non-decreasing across ALL lines (including across
    // the paragraph boundary).
    let all_caret_starts: Vec<usize> = frame
        .lines()
        .iter()
        .flat_map(|line| line.caret_positions.iter().map(|c| c.start_offset))
        .collect();

    for window in all_caret_starts.windows(2) {
        assert!(
            window[0] <= window[1],
            "Caret positions are not monotonically non-decreasing across paragraphs: {} > {} (all: {:?})",
            window[0],
            window[1],
            all_caret_starts
        );
    }

    Ok(())
}

fn all_lines_bounded(frame: &TextFrame, frame_width: f32) -> bool {
    frame.lines().iter().fold(true, |all_bounded, line| {
        let current_bounded = line.width <= frame_width;
        all_bounded && current_bounded
    })
}

#[test]
fn test_longest_long_word_start_finds_only_long_words() {
    assert_eq!(
        TextLayoutSystem::longest_long_word_start("a few short words"),
        None
    );

    let long = "x".repeat(LONG_WORD_BYTES + 1);
    let text = format!("ab {long} cd");
    assert_eq!(TextLayoutSystem::longest_long_word_start(&text), Some(3));

    // A word that runs to the end of the text still counts.
    let text = format!("ab {long}");
    assert_eq!(TextLayoutSystem::longest_long_word_start(&text), Some(3));

    // The longest of several long words wins.
    let longer = "y".repeat(LONG_WORD_BYTES + 100);
    let text = format!("{long} {longer}");
    assert_eq!(
        TextLayoutSystem::longest_long_word_start(&text),
        Some(long.len() + 1)
    );
}

/// #864: a single word past rustybuzz's 16,384 limit used to shape to zero glyphs
/// and zero width on this stack.
#[test]
fn test_word_longer_than_shaping_limit_still_lays_out() -> Result<()> {
    let (font_db, roboto) = init_fonts();

    let text = "a".repeat(20_000);
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let line = font_db.text_layout_system().layout_line(
        &text,
        line_style,
        &[(
            0..text.chars().count(),
            StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
        )],
        f32::MAX,
        crate::text_layout::ClipConfig::default(),
    );

    assert!(
        line.width > 0.0,
        "a 20,000-char word laid out with zero width"
    );
    Ok(())
}

/// Lays out `text` with each `(char_len, bold)` span and returns the total glyph count.
fn glyph_count_with_spans(text: &str, spans: &[(usize, bool)]) -> usize {
    use crate::fonts::Weight;
    let (font_db, roboto) = init_fonts();
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    let mut start = 0;
    let runs: Vec<_> = spans
        .iter()
        .map(|(len, bold)| {
            let properties = Properties {
                weight: if *bold { Weight::Bold } else { Weight::Normal },
                ..Properties::default()
            };
            let range = start..start + len;
            start += len;
            (
                range,
                StyleAndFont::new(roboto, properties, TextStyle::new()),
            )
        })
        .collect();
    let line = font_db.text_layout_system().layout_line(
        text,
        line_style,
        &runs,
        f32::MAX,
        crate::text_layout::ClipConfig::default(),
    );
    line.runs.iter().map(|run| run.glyphs.len()).sum()
}

/// #940: a 30,000-char word keeps every glyph (#864), while a multi-megabyte word is shaped only
/// up to the cap so a huge paste cannot cost ~1 GB of layout.
#[test]
fn test_huge_word_is_bounded_but_long_word_is_complete() {
    let long = "a".repeat(30_000);
    assert_eq!(glyph_count_with_spans(&long, &[(30_000, false)]), 30_000);

    let cap = cosmic_text::MAX_SHAPED_WORD_BYTES;
    let huge = "a".repeat(cap * 3);
    let glyphs = glyph_count_with_spans(&huge, &[(cap * 3, false)]);
    assert!(
        glyphs > 0 && glyphs <= cap,
        "huge word shaped {glyphs} glyphs, cap is {cap}"
    );
}

/// Lays out `text` (one style run) wrapped at `width` and returns the frame.
fn frame_wrapped(text: &str, width: f32) -> crate::text_layout::TextFrame {
    let (font_db, roboto) = init_fonts();
    let line_style = LineStyle {
        font_size: FONT_SIZE,
        line_height_ratio: DEFAULT_UI_LINE_HEIGHT_RATIO,
        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
        fixed_width_tab_size: None,
    };
    font_db.text_layout_system().layout_text(
        text,
        line_style,
        &[(
            0..text.chars().count(),
            StyleAndFont::new(roboto, Properties::default(), TextStyle::new()),
        )],
        width,
        FRAME_HEIGHT,
        Default::default(),
        None,
    )
}

/// #940: every position derived from the glyphs of a line whose word was cut at the shaping cap
/// still maps to the TRUE end of the text: End, click past the end, Down on the last row (the
/// editor uses `last_glyph().index + 1`) and Home. Rows before the last stay contiguous.
#[test]
fn test_capped_word_line_positions_reach_true_text_end() {
    let n = cosmic_text::MAX_SHAPED_WORD_BYTES + 5_000;
    let text = "a".repeat(n);
    let frame = frame_wrapped(&text, 400.);
    let lines = frame.lines();
    assert!(lines.len() > 1);
    for pair in lines.windows(2).take(lines.len().saturating_sub(2)) {
        assert_eq!(pair[0].end_index(), pair[1].first_index(), "rows must be contiguous");
    }
    let last = lines.last().unwrap();
    assert_eq!(last.end_index(), n, "End on the last row");
    assert_eq!(last.last_glyph().unwrap().index + 1, n, "Down/clamp column");
    assert_eq!(last.caret_index_for_x_unbounded(last.width + 50.), n, "click past the end");
    let home = last.first_index();
    assert!(home > 0 && home < cosmic_text::MAX_SHAPED_WORD_BYTES, "Home {home}");
}

/// #940: text after the capped word on the same paragraph keeps true offsets.
#[test]
fn test_capped_word_followed_by_words_keeps_true_offsets() {
    let n = cosmic_text::MAX_SHAPED_WORD_BYTES + 100;
    let text = format!("{} tail", "a".repeat(n));
    let total = text.chars().count();
    let frame = frame_wrapped(&text, 400.);
    let last = frame.lines().last().unwrap();
    assert_eq!(last.end_index(), total);
    assert_eq!(last.last_glyph().unwrap().index + 1, total);
    assert_eq!(last.caret_index_for_x_unbounded(last.width + 50.), total);
}

/// #940: a multi-line text whose FIRST line is over the cap: the next line must start at its true
/// buffer offset (the newline caret was derived from the last glyph).
#[test]
fn test_capped_first_line_does_not_shift_following_lines() {
    let n = cosmic_text::MAX_SHAPED_WORD_BYTES + 300;
    let text = format!("{}\nsecond", "a".repeat(n));
    let frame = frame_wrapped(&text, 400.);
    let lines = frame.lines();
    let last = lines.last().unwrap();
    assert_eq!(last.first_index(), n + 1, "second paragraph starts after the newline");
    assert_eq!(last.end_index(), n + 1 + "second".len());
    let before = &lines[lines.len() - 2];
    assert_eq!(before.end_index(), n + 1, "first paragraph's last row ends at the newline");
}

/// #864: two words over the limit in differently styled spans, a word that crosses a style
/// boundary, and a long word starting in a non-default style all used to lose glyphs (the
/// attrs-list-defaults heuristic can only match one span).
#[test]
fn test_long_words_in_mixed_styles_keep_all_glyphs() {
    const N: usize = 20_000;

    // Two long words, regular then bold.
    let text = format!("{} {}", "a".repeat(N), "b".repeat(N));
    let count = glyph_count_with_spans(&text, &[(N + 1, false), (N, true)]);
    assert_eq!(count, 2 * N + 1, "two long words in different styles");

    // Bold first, regular second.
    let count = glyph_count_with_spans(&text, &[(N + 1, true), (N, false)]);
    assert_eq!(count, 2 * N + 1, "two long words, bold then regular");

    // One long word crossing a style boundary, with a trailing `;`.
    let text = format!("{};", "c".repeat(2 * N));
    let count = glyph_count_with_spans(&text, &[(N, false), (N, true), (1, false)]);
    assert_eq!(count, 2 * N + 1, "word crossing a style boundary");
}
