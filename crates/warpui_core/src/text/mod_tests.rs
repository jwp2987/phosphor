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

/// Regression test: double-clicking on a boundary character (e.g. the space
/// between two words) must select just that character, not both flanking
/// words. Previously `semantic_expansion_target` only looked at the
/// character *after* (forward) or *before* (backward) the clicked position,
/// never at the clicked character itself, so clicking the space in
/// "foo bar" expanded forward into "bar" (because 'b' is a word char) and
/// backward into "foo" (because 'o' is a word char), selecting the whole
/// "foo bar" string.
#[test]
fn test_semantic_expansion_on_a_space_does_not_select_both_flanking_words() -> anyhow::Result<()> {
    let buf = "foo bar";
    let policy = WordBoundariesPolicy::Default;
    let clicked = CharOffset::from(3); // the space between "foo" and "bar"

    let start = buf.semantic_expansion_target(clicked, SelectionDirection::Backward, &policy)?;
    let end = buf.semantic_expansion_target(clicked, SelectionDirection::Forward, &policy)?;
    let start_offset = buf.to_offset(start)?.as_usize();
    let end_offset = buf.to_offset(end)?.as_usize();

    assert_ne!(
        (start_offset, end_offset),
        (0, 7),
        "clicking the space must not select the entire \"foo bar\" span"
    );
    assert_eq!(
        (start_offset, end_offset),
        (3, 4),
        "clicking the space must select just the single boundary character"
    );

    Ok(())
}

/// Sanity check that the fix above didn't regress the normal case: clicking a
/// character inside a word still expands to the whole word.
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
