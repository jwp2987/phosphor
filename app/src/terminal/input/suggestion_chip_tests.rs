//! #2138: the pure part of the prompt-suggestion chip. Design: `docs/design/r21-chip.md`.

use super::*;

const CELL: f32 = 8.;
const EDITOR: f32 = 800.; // 100 cells

fn room(used_cols: usize) -> ChipRoom {
    ChipRoom {
        editor_width_px: EDITOR,
        cell_width_px: CELL,
        used_cols,
        right_inset_px: 0.,
    }
}

#[test]
fn the_switch_is_on_unless_it_says_off() {
    assert!(suggestion_chip_enabled_from(None));
    assert!(suggestion_chip_enabled_from(Some("")));
    assert!(suggestion_chip_enabled_from(Some("1")));
    assert!(suggestion_chip_enabled_from(Some("on")));
    assert!(suggestion_chip_enabled_from(Some("anything")));
    for off in ["0", "off", "OFF", " false ", "No"] {
        assert!(!suggestion_chip_enabled_from(Some(off)), "{off:?}");
    }
}

#[test]
fn the_chip_is_the_placement_unless_the_switch_is_off_or_the_line_is_unknown() {
    assert_eq!(suggestion_placement(true, true), SuggestionPlacement::Chip);
    assert_eq!(
        suggestion_placement(false, true),
        SuggestionPlacement::Banner
    );
    assert_eq!(
        suggestion_placement(true, false),
        SuggestionPlacement::Banner
    );
    assert_eq!(
        suggestion_placement(false, false),
        SuggestionPlacement::Banner
    );
    assert_eq!(SuggestionPlacement::Chip.log_word(), "chip");
    assert_eq!(SuggestionPlacement::Banner.log_word(), "banner");
}

#[test]
fn a_layout_with_nothing_else_on_the_line_is_the_default() {
    let line = ChipLine::default();
    assert_eq!(line.prefix_cols, 0);
    assert_eq!(line.right_inset_px, 0.);
    assert_eq!(line.y_offset_px, 0.);
    assert!(line.first_line_known);
}

#[test]
fn an_empty_line_gives_the_chip_half_the_editor() {
    assert_eq!(chip_max_width(room(0)), Some(EDITOR * CHIP_MAX_FRACTION));
}

#[test]
fn typing_towards_the_chip_makes_it_narrower() {
    // 60 cells typed: 100 - 60 - 2 (gap) = 38 cells are free, under the half-width cap.
    assert_eq!(chip_max_width(room(60)), Some(38. * CELL));
    // One more character, one cell less.
    assert_eq!(chip_max_width(room(61)), Some(37. * CELL));
    // A short command leaves more than half: the cap holds.
    assert_eq!(chip_max_width(room(10)), Some(EDITOR * CHIP_MAX_FRACTION));
}

#[test]
fn the_chip_is_never_drawn_over_text() {
    for used_cols in 0..=200 {
        if let Some(width) = chip_max_width(room(used_cols)) {
            let text_end = used_cols as f32 * CELL;
            let chip_start = EDITOR - width;
            assert!(
                chip_start >= text_end + CHIP_GAP_CELLS * CELL,
                "{used_cols} columns: the chip starts at {chip_start}, the text ends at {text_end}"
            );
            assert!(width >= CHIP_MIN_CELLS * CELL);
            assert!(width <= EDITOR * CHIP_MAX_FRACTION);
        }
    }
}

#[test]
fn under_the_minimum_the_chip_is_not_drawn() {
    // 100 - 82 - 2 = 16 cells: the minimum, still drawn.
    assert_eq!(chip_max_width(room(82)), Some(CHIP_MIN_CELLS * CELL));
    // 15 cells: not drawn.
    assert_eq!(chip_max_width(room(83)), None);
    // A line longer than the editor (it wraps).
    assert_eq!(chip_max_width(room(150)), None);
    assert_eq!(chip_max_width(room(usize::MAX)), None);
}

#[test]
fn a_narrow_pane_has_no_chip() {
    // 30 cells: half is 15, under the minimum, even with an empty line.
    assert_eq!(
        chip_max_width(ChipRoom {
            editor_width_px: 30. * CELL,
            cell_width_px: CELL,
            used_cols: 0,
            right_inset_px: 0.,
        }),
        None
    );
}

#[test]
fn a_right_prompt_takes_its_room_from_the_chip() {
    // 100 - 20 (right prompt) - 50 - 2 = 28 cells.
    assert_eq!(
        chip_max_width(ChipRoom {
            right_inset_px: 20. * CELL,
            ..room(50)
        }),
        Some(28. * CELL)
    );
    // A negative inset is no inset.
    assert_eq!(
        chip_max_width(ChipRoom {
            right_inset_px: -40.,
            ..room(60)
        }),
        Some(38. * CELL)
    );
}

#[test]
fn sizes_that_are_not_known_yet_draw_no_chip() {
    for (editor_width_px, cell_width_px) in
        [(0., CELL), (EDITOR, 0.), (-1., CELL), (f32::NAN, CELL)]
    {
        assert_eq!(
            chip_max_width(ChipRoom {
                editor_width_px,
                cell_width_px,
                used_cols: 0,
                right_inset_px: 0.,
            }),
            None
        );
    }
}

#[test]
fn the_keystroke_needs_a_wide_chip() {
    assert!(chip_shows_keystroke(CHIP_KEYSTROKE_MIN_CELLS * CELL, CELL));
    assert!(!chip_shows_keystroke(
        CHIP_KEYSTROKE_MIN_CELLS * CELL - 1.,
        CELL
    ));
}

#[test]
fn columns_are_display_columns_of_the_first_line() {
    assert_eq!(first_line_cols(""), 0);
    assert_eq!(first_line_cols("ls -la"), 6);
    assert_eq!(first_line_cols("echo one\necho two two two"), 8);
    assert_eq!(first_line_cols("a\r\nbcd"), 1);
    // Wide characters take two cells, a tab is counted at its widest.
    assert_eq!(first_line_cols("日本"), 4);
    assert_eq!(first_line_cols("a\tb"), 10);
}

#[test]
fn used_columns_are_prefix_buffer_and_ghost_text() {
    assert_eq!(used_cols(0, "", None), 0);
    assert_eq!(used_cols(12, "git st", None), 18);
    assert_eq!(used_cols(12, "git st", Some("atus")), 22);
    // Ghost text follows a single-line buffer only.
    assert_eq!(used_cols(0, "one\ntwo", Some("ghost")), 3);
    assert_eq!(used_cols(usize::MAX, "x", None), usize::MAX);
}

#[test]
fn a_font_with_no_usable_m_width_sizes_the_chip_in_the_grid_cell() {
    assert_eq!(chip_cell_width(8., 7.), 8.);
    assert_eq!(chip_cell_width(0., 7.), 7.);
    assert_eq!(chip_cell_width(f32::NAN, 7.), 7.);
    assert_eq!(chip_cell_width(-1., 7.), 7.);
}
