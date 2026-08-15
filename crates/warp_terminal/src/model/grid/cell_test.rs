// The code in this file is adapted from the alacritty_terminal crate under the
// Apache license; see: crates/warp_terminal/src/model/LICENSE-ALACRITTY.

use super::{Cell, LineLength};

use crate::model::{
    char_or_str::CharOrStr,
    grid::{
        cell::{Flags, MAX_GRAPHEME_BYTES},
        row::Row,
    },
};

#[test]
fn verify_cell_size() {
    // If this test fails, then something has changed about Cell that alters its memory layout and
    // causes it to be a different size than expected. Verify carefully if that is expected before
    // updating the constant value.
    const EXPECTED_CELL_SIZE_IN_BYTES: usize = 24;

    assert_eq!(std::mem::size_of::<Cell>(), EXPECTED_CELL_SIZE_IN_BYTES);
}

#[test]
fn line_length_works() {
    let mut row = Row::new(10);
    row[5].c = 'a';

    assert_eq!(row.line_length(), 6);
}

#[test]
fn line_length_works_with_wrapline() {
    let mut row = Row::new(10);
    row[9].flags.insert(super::Flags::WRAPLINE);

    assert_eq!(row.line_length(), 10);
}

#[test]
fn line_length_works_with_empty_line() {
    let mut row = Row::new(1);
    row.shrink(0);
    assert_eq!(row.line_length(), 0);
}

#[test]
fn test_contains_cell_decorations() {
    assert!(Flags::UNDERLINE.intersects(Flags::CELL_DECORATIONS));
    assert!(Flags::STRIKEOUT.intersects(Flags::CELL_DECORATIONS));
    assert!(Flags::DOUBLE_UNDERLINE.intersects(Flags::CELL_DECORATIONS));
}

#[test]
fn push_zerowidth_caps_accumulated_grapheme() {
    // A ZWJ (U+200D) is three bytes in UTF-8.  Push enough of them to go
    // well past `MAX_GRAPHEME_BYTES`, and verify that the accumulated
    // content stops growing at the cap.
    let mut cell = Cell {
        c: 'e',
        ..Cell::default()
    };
    let zwj = '\u{200D}';
    let zwj_bytes = zwj.len_utf8();
    let pushes = (MAX_GRAPHEME_BYTES * 10) / zwj_bytes;
    for _ in 0..pushes {
        cell.push_zerowidth(zwj, /* log_long_grapheme_warnings */ true);
    }

    let CharOrStr::Str(content) = cell.raw_content() else {
        panic!("cell should have accumulated zero-width content as a string");
    };
    // The stored content is "base char + N zero-width chars".  The total
    // length in bytes must fit within the cap.
    assert!(
        content.len() <= MAX_GRAPHEME_BYTES,
        "expected stored content length {} to be <= cap {}",
        content.len(),
        MAX_GRAPHEME_BYTES,
    );
    // We also want the cap to actually be approached: the stored content
    // should contain many zero-width characters and not have been truncated
    // early.
    let zero_width_bytes = content.len() - 'e'.len_utf8();
    let zero_width_count = zero_width_bytes / zwj_bytes;
    assert!(
        zero_width_count >= 80,
        "expected at least 80 zero-width chars to fit, got {zero_width_count}",
    );
    assert!(content.starts_with('e'));
    assert!(content[1..].chars().all(|c| c == zwj));
}

#[test]
fn push_zerowidth_seeds_base_char_on_first_push() {
    // Before any zero-width char is pushed, the cell's raw content is just
    // the base char.  After the first push, the content becomes a string
    // consisting of the base char plus the pushed zero-width char.
    let mut cell = Cell {
        c: 'x',
        ..Cell::default()
    };
    assert_eq!(cell.raw_content(), CharOrStr::Char('x'));

    cell.push_zerowidth('\u{0301}', /* log_long_grapheme_warnings */ true);
    assert_eq!(cell.raw_content(), CharOrStr::Str("x\u{0301}"));
}

// Tests for `keycap_sequence_width`. Written 2026-08-15 while investigating a report that a
// keycap emoji (`1️⃣`, i.e. U+0031 U+FE0F U+20E3) rendered as a tofu box overdrawing the
// character after it in the GUI. These are NOT tests of a fix to `unicode_width` itself --
// reading the vendored `unicode-width-0.1.14` source (the version this workspace resolves to;
// see `Cargo.lock`) and its own test suite showed it already resolves this exact grapheme to
// width 2 via `UnicodeWidthStr::width`. The actual bug was `Block::set_shell_host` (in `app`)
// applying the Zsh bracketed-paste workaround to `output_grid`, which has nothing to do with
// Zsh's line editor. These tests exist to pin down `keycap_sequence_width`'s own behavior as an
// explicit, crate-independent cross-check, per its doc comment.

#[test]
fn keycap_sequence_width_recognizes_fully_qualified_keycap() {
    // All twelve legal keycap bases per Unicode's emoji-data.txt: 0-9, '#', '*'.
    for base in "0123456789#*".chars() {
        let grapheme = format!("{base}\u{FE0F}\u{20E3}");
        assert_eq!(
            super::keycap_sequence_width(&grapheme),
            Some(2),
            "expected {grapheme:?} to be recognized as a keycap sequence"
        );
    }
}

#[test]
fn keycap_sequence_width_rejects_unqualified_keycap() {
    // Without the VS16, this is the "unqualified" keycap form -- not a defined emoji-presentation
    // sequence per UTR #51. Leave it to `unicode_width`'s own answer instead of asserting one here.
    assert_eq!(super::keycap_sequence_width("1\u{20E3}"), None);
}

#[test]
fn keycap_sequence_width_rejects_non_keycap_base() {
    // 'a' is not one of the twelve Unicode keycap bases, so this is not a keycap sequence
    // however structurally similar it looks to one.
    assert_eq!(super::keycap_sequence_width("a\u{FE0F}\u{20E3}"), None);
}

#[test]
fn keycap_sequence_width_rejects_partial_sequence() {
    // Just the digit + VS16, with no combining enclosing keycap yet -- e.g. mid-accumulation,
    // before the third codepoint of a real keycap grapheme has arrived at the cell.
    assert_eq!(super::keycap_sequence_width("1\u{FE0F}"), None);
}

#[test]
fn keycap_sequence_width_rejects_trailing_content() {
    // Anything after the keycap sequence means this is not (just) a keycap grapheme.
    assert_eq!(super::keycap_sequence_width("1\u{FE0F}\u{20E3}a"), None);
}
