use warpui::units::IntoLines as _;

use super::*;
use crate::terminal::SizeUpdateReason;

fn default_size() -> SizeInfo {
    SizeInfo::new_without_font_metrics(10, 7)
}

fn new_alt_screen(size: SizeInfo) -> AltScreen {
    AltScreen::new(
        size,
        1_000, /* max_scroll_limit */
        ChannelEventListener::new_for_test(),
        ObfuscateSecrets::No,
        KeyModelCell::default(),
    )
}

#[test]
fn test_alt_screen_resize_clears_selection() {
    let original_size = default_size();
    let mut screen = new_alt_screen(original_size);

    screen.start_selection(Point::new(2, 6), SelectionType::Simple, Side::Left);
    screen.update_selection(Point::new(4, 4), Side::Right);
    let semantic_selection = SemanticSelection::mock(false, "");

    assert_eq!(
        screen.selection_range(&semantic_selection),
        Some(ExpandedSelectionRange::Regular {
            start: Point::new(2, 6),
            end: Point::new(4, 4),
            reversed: false,
        })
    );

    // Resizing with same number of rows / cols should not clear selections.
    let size_update = SizeUpdate {
        update_reason: SizeUpdateReason::Refresh,
        last_size: original_size,
        new_size: original_size,
        new_gap_height: None,
        natural_rows: original_size.rows(),
        natural_cols: original_size.columns(),
    };
    screen.resize(&size_update);
    assert_eq!(
        screen.selection_range(&semantic_selection),
        Some(ExpandedSelectionRange::Regular {
            start: Point::new(2, 6),
            end: Point::new(4, 4),
            reversed: false,
        })
    );

    // Resizing with diff size should clear selections.
    let new_size =
        default_size().with_rows_and_columns(original_size.rows() + 1, original_size.columns() + 1);
    let size_update = SizeUpdate {
        update_reason: SizeUpdateReason::Refresh,
        last_size: original_size,
        new_size,
        new_gap_height: None,
        natural_rows: new_size.rows(),
        natural_cols: new_size.columns(),
    };
    screen.resize(&size_update);
    assert!(screen.selection_range(&semantic_selection).is_none());
}

#[test]
fn test_alt_screen_single_cell_selection() {
    let mut screen = new_alt_screen(default_size());

    screen.start_selection(Point::new(2, 6), SelectionType::Simple, Side::Left);
    screen.update_selection(Point::new(2, 7), Side::Left);
    let semantic_selection = SemanticSelection::mock(false, "");

    assert_eq!(
        screen.selection_range(&semantic_selection),
        Some(ExpandedSelectionRange::Regular {
            start: Point::new(2, 6),
            end: Point::new(2, 6),
            reversed: false,
        })
    );
}

#[test]
fn test_accumulate_lines_to_scroll() {
    let mut screen = new_alt_screen(default_size());

    assert_eq!(screen.accumulate_lines_to_scroll(-0.9.into_lines()), 0);
    assert_lines_approx_eq!(screen.pending_lines_to_scroll(), -0.9);

    assert_eq!(screen.accumulate_lines_to_scroll(-3.2.into_lines()), -4);
    assert_lines_approx_eq!(screen.pending_lines_to_scroll(), -0.1);

    assert_eq!(screen.accumulate_lines_to_scroll(5.0.into_lines()), 4);
    assert_lines_approx_eq!(screen.pending_lines_to_scroll(), 0.9);
}

/// `AltScreenElement::paint` paints from a clone of the grid handler taken under the model lock,
/// so everything `render_grid` reads has to survive the clone unchanged.
#[test]
fn a_cloned_grid_handler_carries_the_same_renderable_state() {
    use crate::terminal::model::grid::Dimensions as _;

    let mut screen = new_alt_screen(default_size());
    for c in "hello".chars() {
        ansi::Handler::input(&mut screen, c);
    }
    ansi::Handler::linefeed(&mut screen);
    ansi::Handler::carriage_return(&mut screen);
    for c in "world".chars() {
        ansi::Handler::input(&mut screen, c);
    }

    let original = screen.grid_handler();
    let clone = original.clone();
    assert_eq!(clone.columns(), original.columns());
    assert_eq!(clone.visible_rows(), original.visible_rows());
    assert_eq!(clone.cursor_point(), original.cursor_point());
    assert_eq!(clone.cursor_render_point(), original.cursor_render_point());
    assert_eq!(
        clone.is_cursor_on_wide_char(),
        original.is_cursor_on_wide_char()
    );
    assert_eq!(clone.marked_text(), original.marked_text());
    assert_eq!(clone.has_secrets(), original.has_secrets());
    for row in 0..original.visible_rows() {
        assert_eq!(clone.row(row), original.row(row), "row {row}");
    }
    assert_eq!(clone.row(1).expect("row 1 exists")[0].c, 'w');
}

/// The parser drops an alt-screen selection the scroll region carries out, with no event: the key
/// path's copy is told at that moment, so a Ctrl-C in the same slice does not copy nothing (#1411).
#[test]
fn a_selection_scrolled_out_by_the_parser_leaves_the_published_copy() {
    let copy = KeyModelCell::default();
    let size = default_size();
    let mut screen = AltScreen::new(
        size,
        1_000, /* max_scroll_limit */
        ChannelEventListener::new_for_test(),
        ObfuscateSecrets::No,
        copy.clone(),
    );
    screen.start_selection(Point::new(0, 1), SelectionType::Simple, Side::Left);
    screen.update_selection(Point::new(0, 3), Side::Right);
    assert!(screen.selection().is_some());
    assert!(copy.load_exact_modes().has_alt_screen_selection);

    // Output on the bottom row scrolls the screen one line per linefeed; the selection on the top
    // row leaves the scroll region with the first and is dropped.
    ansi::Handler::goto(&mut screen, VisibleRow(size.rows() - 1), 0);
    for _ in 0..size.rows() {
        ansi::Handler::linefeed(&mut screen);
    }
    assert!(
        screen.selection().is_none(),
        "the parser dropped the selection"
    );
    assert!(
        !copy.load_exact_modes().has_alt_screen_selection,
        "and the copy says so, with no event"
    );
}
