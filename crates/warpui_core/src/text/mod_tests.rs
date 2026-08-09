use super::{
    byte_offset_for_char_offset, count_chars_up_to_byte,
    point::Point,
    word_boundaries::WordBoundariesPolicy,
    {char_slice, BufferIndex, SelectionDirection, TextBuffer},
};

use super::str_to_byte_vec;
use string_offset::CharOffset;

#[test]
fn test_str_to_byte_vec() {
    assert_eq!(
        str_to_byte_vec("foo bar"),
        vec![0x66, 0x6f, 0x6f, 0x20, 0x62, 0x61, 0x72]
    );
}

/// Test the [`str`] implementation of [`TextBuffer`], which we rely on in other unit tests.
#[test]
fn test_str_buffer() -> anyhow::Result<()> {
    let buf = "Hello\nWorld!";

    assert_eq!(buf.chars_at(2.into())?.collect::<String>(), "llo\nWorld!");
    assert_eq!(buf.chars_rev_at(3.into())?.collect::<String>(), "leH");

    // For simplicity, we do not wrap newlines into new rows.
    assert_eq!(buf.to_point(7.into())?, Point::new(0, 7));
    assert!(Point::new(1, 1).to_char_offset(buf).is_err());
    assert_eq!(Point::new(0, 7).to_char_offset(buf)?, 7.into());

    Ok(())
}

#[test]
fn test_char_slice() {
    let has_nonbreaking_space = "A\u{a0}non-breaking space occupies 2 bytes in UTF-8";
    assert_eq!(char_slice(has_nonbreaking_space, 0, 3), Some("A\u{a0}n"));

    // This string has characters ['A', '❤', '\u{fe0f}', '\u{200d}', '🔥', 'b']
    assert_eq!(char_slice("A❤️‍🔥b", 4, 5), Some("🔥"));

    assert_eq!(char_slice("abc", 5, 10), None);
    assert_eq!(char_slice("abc", 2, 0), None);
    assert_eq!(char_slice("abc", 1, 4), None);

    assert_eq!(char_slice("A string", 2, 4), Some("st"));

    assert_eq!(char_slice("The end: 🫥??", 10, 12), Some("??"));

    assert_eq!(char_slice("🫥", 0, 0), Some(""));
}

#[test]
fn test_char_counts_up_to_byte() {
    let text = "abc🔥abc☄️abc😬";
    assert_eq!(count_chars_up_to_byte(text, 0.into()), Some(0.into()));
    assert_eq!(
        count_chars_up_to_byte(text, "abc🔥".len().into()),
        Some(4.into())
    );
    assert_eq!(
        count_chars_up_to_byte(text, text.len().into()),
        Some(text.chars().count().into())
    );
}

#[test]
fn test_byte_offset_for_char_offset() {
    let text = "abc🔥abc☄️abc😬";
    assert_eq!(byte_offset_for_char_offset(text, 0.into()), Some(0.into()));
    assert_eq!(
        byte_offset_for_char_offset(text, 4.into()),
        Some("abc🔥".len().into())
    );
    assert_eq!(
        byte_offset_for_char_offset(text, text.chars().count().into()),
        Some(text.len().into())
    );
    assert_eq!(
        byte_offset_for_char_offset(text, (text.chars().count() + 1).into()),
        None
    );
}

// REMOVED: test_semantic_expansion_on_a_space_does_not_select_both_flanking_words.
//
// Added by 891f5b88e (2026-07-30) as a fork-only test asserting that
// double-clicking a boundary character collapses the selection to that single
// character. Removed on a maintainer decision (2026-08-08) after it was found to
// contradict the pinned oracle's own test, `test_semantic_expansion_matches_block_list`
// in `word_boundaries_tests.rs`, ported from `02b53fcd8` by 6f2a5afcd (2026-08-07).
// The two encoded opposite behaviour; the collision went unseen for a day because
// no gate ran `warpui_core` at all (#573).
//
// The pin's behaviour wins, for a reason beyond parity: this fork's own terminal
// engine, `app/src/terminal/model/grid/grid_handler.rs::semantic_search_left`/`right`,
// never special-cases the clicked position either. So the collapsing behaviour made
// the TUI disagree with the GUI terminal, and the pinned test exists precisely to
// keep those two implementations in step. See #574.
//
// This is a deliberate reversal of a previous decision, not a test weakened to go
// green — the code was changed to match the pin and this assertion no longer
// describes intended behaviour.

/// Sanity check on the normal case: clicking a character inside a word expands
/// to the whole word.
#[test]
fn test_semantic_expansion_on_a_word_char_selects_the_whole_word() -> anyhow::Result<()> {
    let buf = "foo bar";
    let policy = WordBoundariesPolicy::Default;
    let clicked = CharOffset::from(1); // the 'o' in "foo"

    let start = buf.semantic_expansion_target(clicked, SelectionDirection::Backward, &policy)?;
    let end = buf.semantic_expansion_target(clicked, SelectionDirection::Forward, &policy)?;
    let start_offset = buf.to_offset(start)?.as_usize();
    let end_offset = buf.to_offset(end)?.as_usize();

    assert_eq!((start_offset, end_offset), (0, 3));

    Ok(())
}
