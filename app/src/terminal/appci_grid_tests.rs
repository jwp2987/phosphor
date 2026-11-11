use warpui::color::ColorU;

use super::*;
use crate::terminal::model::ansi::NamedColor;

fn cell(c: char) -> Cell {
    let mut cell = Cell::default();
    cell.c = c;
    cell
}

#[test]
fn tokens_are_letters_and_digits_of_a_bounded_length() {
    assert!(valid_token("gridAB12"));
    assert!(!valid_token("short"));
    assert!(!valid_token("has_underscore1"));
    assert!(!valid_token("../../etc/passwd"));
    assert!(!valid_token(&"a".repeat(65)));
    assert_eq!(
        parse_tokens("gridAB12\n\n  gridCD34  \nbad token\ngridAB12\nx\n"),
        vec!["gridAB12".to_owned(), "gridCD34".to_owned()]
    );
}

#[test]
fn colours_are_named_as_the_escape_sequence_gave_them() {
    assert_eq!(color_name(&Color::Named(NamedColor::Red)), "named:red");
    assert_eq!(
        color_name(&Color::Named(NamedColor::BrightBlue)),
        "named:brightblue"
    );
    assert_eq!(
        color_name(&Color::Named(NamedColor::Foreground)),
        "named:foreground"
    );
    assert_eq!(
        color_name(&Color::Named(NamedColor::Background)),
        "named:background"
    );
    assert_eq!(color_name(&Color::Indexed(196)), "idx:196");
    assert_eq!(
        color_name(&Color::Spec(ColorU::new(255, 136, 0, 255))),
        "rgb:ff8800"
    );
}

#[test]
fn attribute_letters_have_a_fixed_order() {
    assert_eq!(attr_letters(Flags::empty()), "");
    assert_eq!(attr_letters(Flags::BOLD), "b");
    assert_eq!(attr_letters(Flags::INVERSE | Flags::BOLD), "br");
    assert_eq!(
        attr_letters(
            Flags::HIDDEN
                | Flags::STRIKEOUT
                | Flags::DOUBLE_UNDERLINE
                | Flags::UNDERLINE
                | Flags::ITALIC
                | Flags::DIM
                | Flags::BOLD
                | Flags::INVERSE
        ),
        "bdiuUsrh"
    );
    assert_eq!(attr_letters(Flags::WIDE_CHAR), "W");
    assert_eq!(attr_letters(Flags::WIDE_CHAR_SPACER), "S");
    assert_eq!(attr_letters(Flags::LEADING_WIDE_CHAR_SPACER), "L");
    // Bookkeeping flags are not attributes of what is painted.
    assert_eq!(attr_letters(Flags::WRAPLINE | Flags::HAS_CURSOR), "");
}

#[test]
fn cursor_shapes_have_names() {
    assert_eq!(shape_name(CursorShape::Block), "block");
    assert_eq!(shape_name(CursorShape::Underline), "underline");
    assert_eq!(shape_name(CursorShape::Beam), "beam");
    assert_eq!(shape_name(CursorShape::HollowBlock), "hollow_block");
    assert_eq!(shape_name(CursorShape::Hidden), "hidden");
}

/// `a`, a blank, a red bold `b`, a wide character and its spacer, `e` with a combining acute, a
/// blank on a coloured background, then never-written cells.
fn sample_row() -> Vec<Cell> {
    let mut row = vec![Cell::default(); 12];
    row[0] = cell('a');
    row[1] = cell(' ');
    row[2] = cell('b');
    row[2].fg = Color::Named(NamedColor::Red);
    row[2].flags = Flags::BOLD;
    row[3] = cell('\u{4e16}');
    row[3].flags = Flags::WIDE_CHAR;
    row[4] = cell(' ');
    row[4].flags = Flags::WIDE_CHAR_SPACER;
    row[5] = cell('e');
    row[5].push_zerowidth('\u{301}', false);
    row[6] = cell(' ');
    row[6].bg = Color::Indexed(21);
    row
}

#[test]
fn a_row_lists_only_the_cells_that_paint_something() {
    let cells = snap_cells(&sample_row());
    let cols: Vec<usize> = cells.iter().map(|c| c.col).collect();
    assert_eq!(cols, vec![0, 2, 3, 4, 5, 6]);
    assert_eq!(cells[0].text, "a");
    assert_eq!(cells[0].fg, "named:foreground");
    assert_eq!(cells[0].bg, "named:background");
    assert_eq!(cells[0].attrs, "");
    assert_eq!(
        (
            cells[1].text.as_str(),
            cells[1].fg.as_str(),
            cells[1].attrs.as_str()
        ),
        ("b", "named:red", "b")
    );
    assert_eq!(
        (cells[2].text.as_str(), cells[2].attrs.as_str()),
        ("\u{4e16}", "W")
    );
    assert_eq!((cells[3].text.as_str(), cells[3].attrs.as_str()), ("", "S"));
    assert_eq!(cells[4].text, "e\u{301}");
    assert_eq!(
        (cells[5].text.as_str(), cells[5].bg.as_str()),
        (" ", "idx:21")
    );
    // The wide character takes two columns of the grid and one position of the text.
    assert_eq!(row_text(&cells), "a b\u{4e16}e\u{301}");
}

#[test]
fn a_blank_cell_with_a_drawn_attribute_is_listed() {
    let mut row = vec![Cell::default(); 3];
    row[1].flags = Flags::UNDERLINE;
    row[2].flags = Flags::INVERSE;
    let cells = snap_cells(&row);
    assert_eq!(cells.len(), 2);
    assert_eq!((cells[0].col, cells[0].attrs.as_str()), (1, "u"));
    assert_eq!((cells[1].col, cells[1].attrs.as_str()), (2, "r"));
    assert!(snap_cells(&vec![Cell::default(); 4]).is_empty());
}

#[test]
fn the_snapshot_file_is_one_json_object_in_format_1() {
    let rows = vec![
        RowSnap {
            row: 4,
            cells: snap_cells(&sample_row()),
        },
        RowSnap {
            row: 5,
            cells: Vec::new(),
        },
    ];
    let cursor = CursorSnap {
        row: 5,
        col: 2,
        shape: CursorShape::Beam,
    };
    let text = format_snapshot("gridAB12", 12, &rows, Some(cursor));
    assert!(!text.contains('\n'));
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(value["v"], 1);
    assert_eq!(value["token"], "gridAB12");
    assert_eq!(value["cols"], 12);
    assert_eq!(
        value["cursor"],
        serde_json::json!({"row": 5, "col": 2, "shape": "beam"})
    );
    assert_eq!(value["rows"].as_array().expect("rows").len(), 2);
    assert_eq!(value["rows"][0]["row"], 4);
    assert_eq!(value["rows"][0]["text"], "a b\u{4e16}e\u{301}");
    assert_eq!(
        value["rows"][0]["cells"][1],
        serde_json::json!([2, "b", "named:red", "named:background", "b"])
    );
    assert_eq!(
        value["rows"][0]["cells"][3],
        serde_json::json!([4, "", "named:foreground", "named:background", "S"])
    );
    assert_eq!(value["rows"][1]["text"], "");
    assert_eq!(value["rows"][1]["cells"], serde_json::json!([]));

    let no_cursor = format_snapshot("gridAB12", 12, &rows, None);
    let value: serde_json::Value = serde_json::from_str(&no_cursor).expect("valid JSON");
    assert!(value["cursor"].is_null());
}

#[test]
fn only_a_grid_that_shows_a_listed_token_is_reported() {
    let mut row = vec![Cell::default(); 20];
    for (col, c) in "x gridAB12 y".chars().enumerate() {
        row[col] = cell(c);
    }
    let rows = vec![RowSnap {
        row: 0,
        cells: snap_cells(&row),
    }];
    let tokens = vec!["gridZZ99".to_owned(), "gridAB12".to_owned()];
    assert_eq!(tokens_shown(&rows, &tokens), vec!["gridAB12"]);
    assert!(tokens_shown(&rows, &["gridZZ99".to_owned()]).is_empty());
    assert!(tokens_shown(&[], &tokens).is_empty());
}
