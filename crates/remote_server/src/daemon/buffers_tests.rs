use super::*;

fn edit(start: u64, end: u64, text: &str) -> TextEdit {
    TextEdit {
        start_offset: start,
        end_offset: end,
        text: text.to_owned(),
    }
}

#[test]
fn edits_are_one_indexed_character_offsets_applied_in_sequence() {
    let mut text = "héllo".to_owned();
    // Replace "é" (char 2), then insert after the (now longer) text's 3rd character.
    apply_edits(&mut text, &[edit(2, 3, "ee"), edit(4, 4, "-")]);
    assert_eq!(text, "hee-llo");
    // Offsets are clamped to the end, an end before its start inserts at the start.
    let mut text = "ab".to_owned();
    apply_edits(&mut text, &[edit(50, 60, "Z"), edit(2, 1, "Y")]);
    assert_eq!(text, "aYbZ");
    // Offset 0 (invalid, 1-indexed) is treated as the start.
    let mut text = "ab".to_owned();
    apply_edits(&mut text, &[edit(0, 0, "S")]);
    assert_eq!(text, "Sab");
}

#[test]
fn diff_edit_covers_only_the_differing_middle_in_characters() {
    assert_eq!(diff_edit("same", "same"), None);
    let e = diff_edit("日本語テキスト", "日本X語テキスト").unwrap();
    assert_eq!((e.start_offset, e.end_offset, e.text.as_str()), (3, 3, "X"));
    let e = diff_edit("abc", "").unwrap();
    assert_eq!((e.start_offset, e.end_offset, e.text.as_str()), (1, 4, ""));
    let e = diff_edit("", "abc").unwrap();
    assert_eq!(
        (e.start_offset, e.end_offset, e.text.as_str()),
        (1, 1, "abc")
    );
    // Overlapping repeats: the suffix never reuses the prefix.
    let e = diff_edit("aa", "aaa").unwrap();
    let mut text = "aa".to_owned();
    apply_edits(&mut text, &[e]);
    assert_eq!(text, "aaa");
}

#[test]
fn diff_edit_applied_to_the_old_text_gives_the_new_text() {
    let cases = [
        ("", ""),
        ("hello world", "hello brave new world"),
        ("a\nb\nc\n", "a\nc\n"),
        ("ünï", "ünïcode"),
        ("xyz", "abc"),
    ];
    for (old, new) in cases {
        let mut text = old.to_owned();
        if let Some(e) = diff_edit(old, new) {
            apply_edits(&mut text, &[e]);
        }
        assert_eq!(text, new, "{old:?} -> {new:?}");
    }
}
