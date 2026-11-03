use sum_tree::SumTree;

use crate::content::text::{BufferBlockStyle, BufferText, BufferTextStyle, MarkerDir};
use string_offset::CharOffset;

use super::{BufferCursor, BufferSumTree};

/// Helper function to count the number of Text fragments in a SumTree
fn count_text_fragments(tree: &SumTree<BufferText>) -> usize {
    let mut cursor = tree.cursor::<(), ()>();
    cursor.descend_to_first_item(tree, |_| true);
    let mut count = 0;
    while let Some(item) = cursor.item() {
        if matches!(item, BufferText::Text { .. }) {
            count += 1;
        }
        cursor.next();
    }
    count
}

#[test]
fn test_plain_text_before_markers() {
    let mut tree: SumTree<BufferText> = SumTree::new();
    tree.push(BufferText::BlockMarker {
        marker_type: BufferBlockStyle::PlainText,
    });
    tree.append_str("This is some text");
    tree.push(BufferText::Newline);
    tree.append_str("New line veryyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy long text");
    assert_eq!(
        tree.debug(),
        "<text>This is some text\\nNew line veryyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy long text"
    );

    let cursor = tree.cursor::<CharOffset, CharOffset>();
    let mut text_cursor = BufferCursor::new(cursor);
    text_cursor.seek_to_offset_before_markers(CharOffset::from(3));
    let new_content = text_cursor.slice_to_offset_before_markers(CharOffset::from(6));
    assert_eq!(new_content.debug(), "is ");

    let new_content = text_cursor.slice_to_offset_before_markers(CharOffset::from(20));
    assert_eq!(new_content.debug(), "is some text\\nN");

    let new_content = text_cursor.slice_to_offset_before_markers(CharOffset::from(40));
    assert_eq!(new_content.debug(), "ew line veryyyyyyyyy");
}

#[test]
fn test_plain_text_after_markers() {
    let mut tree: SumTree<BufferText> = SumTree::new();
    tree.push(BufferText::BlockMarker {
        marker_type: BufferBlockStyle::PlainText,
    });
    tree.append_str("This is some text");
    tree.push(BufferText::Newline);
    tree.append_str("New line veryyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy long text");
    assert_eq!(
        tree.debug(),
        "<text>This is some text\\nNew line veryyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy long text"
    );

    let cursor = tree.cursor::<CharOffset, CharOffset>();
    let mut text_cursor = BufferCursor::new(cursor);
    text_cursor.seek_to_offset_after_markers(CharOffset::from(3));
    let new_content = text_cursor.slice_to_offset_after_markers(CharOffset::from(6));
    assert_eq!(new_content.debug(), "is ");

    let new_content = text_cursor.slice_to_offset_after_markers(CharOffset::from(20));
    assert_eq!(new_content.debug(), "is some text\\nN");

    let new_content = text_cursor.slice_to_offset_after_markers(CharOffset::from(40));
    assert_eq!(new_content.debug(), "ew line veryyyyyyyyy");
}

#[test]
fn test_append_str() {
    let mut tree: SumTree<BufferText> = SumTree::new();
    tree.append_str("Som");
    tree.append_str("ething");
    tree.append_str(" long stringggggggggggggg");
    assert_eq!(tree.debug(), "Something long stringggggggggggggg");
}

#[test]
fn test_append_str_merges_with_existing_fragment() {
    // Test the bug fix: is_first should be true when appending to allow merging
    // with the last text fragment if it has space remaining
    let mut tree: SumTree<BufferText> = SumTree::new();

    // Add initial content that creates a text fragment with remaining capacity
    tree.append_str("Initial");

    // Count fragments before second append
    let text_fragments_before = count_text_fragments(&tree);

    // Append more text - this should merge with the existing fragment if possible
    tree.append_str(" text");

    // Count fragments after second append
    let text_fragments_after = count_text_fragments(&tree);

    // The result should be a single merged fragment, not separate ones
    assert_eq!(tree.debug(), "Initial text");
    assert_eq!(
        text_fragments_before, 1,
        "Should have 1 fragment before second append"
    );
    assert_eq!(
        text_fragments_after, 1,
        "Should still have 1 fragment after merging"
    );

    // Verify the internal structure by checking we can iterate correctly
    let cursor = tree.cursor::<CharOffset, CharOffset>();
    let mut buffer_cursor = BufferCursor::new(cursor);
    assert_eq!(buffer_cursor.char_at(CharOffset::from(0)), Some('I'));
    assert_eq!(buffer_cursor.char_at(CharOffset::from(7)), Some(' '));
    assert_eq!(buffer_cursor.char_at(CharOffset::from(8)), Some('t'));
    assert_eq!(buffer_cursor.char_at(CharOffset::from(11)), Some('t'));
}

#[test]
fn test_append_str_creates_new_fragment_when_full() {
    use crate::content::text::TEXT_FRAGMENT_SIZE;

    let mut tree: SumTree<BufferText> = SumTree::new();

    // Create a text fragment that's at the TEXT_FRAGMENT_SIZE limit
    let large_text = "a".repeat(TEXT_FRAGMENT_SIZE);
    tree.append_str(&large_text);

    let fragments_before = count_text_fragments(&tree);

    // Append additional text - this should create a new fragment since the first is full
    tree.append_str("extra");

    let fragments_after = count_text_fragments(&tree);

    // Should create a new fragment since the first one is at capacity
    let expected = format!("{large_text}extra");
    assert_eq!(tree.debug(), expected);
    assert_eq!(fragments_before, 1, "Should have 1 fragment before append");
    assert_eq!(
        fragments_after, 2,
        "Should have 2 fragments after append when first is full"
    );
}

#[test]
fn test_styled_text_before_markers() {
    let mut tree: SumTree<BufferText> = SumTree::new();
    tree.push(BufferText::BlockMarker {
        marker_type: BufferBlockStyle::PlainText,
    });
    tree.append_str("Plain text");
    tree.push(BufferText::Marker {
        marker_type: BufferTextStyle::bold(),
        dir: MarkerDir::Start,
    });
    tree.push(BufferText::Marker {
        marker_type: BufferTextStyle::Italic,
        dir: MarkerDir::Start,
    });
    tree.append_str("BI");
    tree.push(BufferText::Marker {
        marker_type: BufferTextStyle::bold(),
        dir: MarkerDir::End,
    });
    tree.append_str("Just Italic");
    tree.push(BufferText::Marker {
        marker_type: BufferTextStyle::Italic,
        dir: MarkerDir::End,
    });
    tree.append_str("Plain text");
    assert_eq!(
        tree.debug(),
        "<text>Plain text<b_s><i_s>BI<b_e>Just Italic<i_e>Plain text"
    );

    let cursor = tree.cursor::<CharOffset, CharOffset>();
    let mut text_cursor = BufferCursor::new(cursor);
    text_cursor.seek_to_offset_before_markers(CharOffset::from(11));
    let new_content = text_cursor.slice_to_offset_after_markers(CharOffset::from(13));
    assert_eq!(new_content.debug(), "<b_s><i_s>BI<b_e>");

    let new_content = text_cursor.slice_to_offset_after_markers(CharOffset::from(17));
    assert_eq!(new_content.debug(), "Just");

    let new_content = text_cursor.slice_to_offset_before_markers(CharOffset::from(24));
    assert_eq!(new_content.debug(), " Italic");

    let new_content = text_cursor.suffix();
    assert_eq!(new_content.debug(), "<i_e>Plain text");
}

#[test]
fn test_char_at() {
    let mut tree: SumTree<BufferText> = SumTree::new();
    tree.append_str("Line");
    tree.push(BufferText::Marker {
        marker_type: BufferTextStyle::bold(),
        dir: MarkerDir::Start,
    });
    tree.append_str("String");
    tree.push(BufferText::Marker {
        marker_type: BufferTextStyle::bold(),
        dir: MarkerDir::End,
    });
    tree.push(BufferText::Newline);
    tree.append_str("Next");
    assert_eq!(tree.debug(), "Line<b_s>String<b_e>\\nNext");

    let cursor = tree.cursor::<CharOffset, CharOffset>();
    let mut text_cursor = BufferCursor::new(cursor);
    assert_eq!(text_cursor.char_at(CharOffset::from(1)), Some('i'));
    assert_eq!(text_cursor.char_at(CharOffset::from(3)), Some('e'));
    assert_eq!(text_cursor.char_at(CharOffset::from(4)), Some('S'));
    assert_eq!(text_cursor.char_at(CharOffset::from(10)), Some('\n'));
}

/// The items the pre-#1607 `append_str` collected into a Vec (fragments after the first-line fill),
/// kept here as the reference the streaming version must reproduce item for item.
fn reference_fragments(first_rest: &str, s: &str) -> Vec<BufferText> {
    use crate::content::text::TEXT_FRAGMENT_SIZE;
    use arrayvec::ArrayString;

    let trailing_newline = s.ends_with('\n');
    let mut out = Vec::new();
    let mut lines = s.lines().peekable();
    let mut is_first = true;
    while let Some(line) = lines.next() {
        let mut text = if is_first { first_rest } else { line };
        is_first = false;
        while !text.is_empty() {
            let mut split_ix = text.len().min(TEXT_FRAGMENT_SIZE);
            while !text.is_char_boundary(split_ix) {
                split_ix -= 1;
            }
            let (chunk, remainder) = text.split_at(split_ix);
            out.push(BufferText::Text {
                char_count: chunk.chars().count() as u8,
                fragment: ArrayString::from(chunk).unwrap(),
            });
            text = remainder;
        }
        if lines.peek().is_some() || trailing_newline {
            out.push(BufferText::Newline);
        }
    }
    out
}

#[test]
fn append_str_streams_the_same_items_as_the_collected_vec() {
    use crate::content::text::TEXT_FRAGMENT_SIZE;

    let long = "x".repeat(TEXT_FRAGMENT_SIZE * 2 + 5);
    let wide = "\u{00e9}\u{4e2d}\u{1f600}".repeat(TEXT_FRAGMENT_SIZE / 3 + 7);
    let inputs = [
        String::new(),
        "\n".to_string(),
        "\n\n\n".to_string(),
        "one line".to_string(),
        "one line\n".to_string(),
        "a\nb\n\nc".to_string(),
        "a\r\nb\r\n".to_string(),
        format!("{long}\nshort\n{long}"),
        format!("{wide}\n{wide}\n"),
        (0..500)
            .map(|i| format!("line {i}: alpha beta\n"))
            .collect(),
    ];
    for input in &inputs {
        // Into an empty tree: no first-line fill, so the reference is the whole input.
        let mut tree: SumTree<BufferText> = SumTree::new();
        tree.append_str(input);
        let first = input.lines().next().unwrap_or("");
        assert_eq!(
            tree.items(),
            reference_fragments(first, input),
            "input {input:?}"
        );

        // Behind a marker (what a buffer looks like): still no fill, the marker is not text.
        let mut tree: SumTree<BufferText> = SumTree::new();
        let marker = BufferText::BlockMarker {
            marker_type: BufferBlockStyle::PlainText,
        };
        tree.push(marker.clone());
        tree.append_str(input);
        let mut expected = vec![marker];
        expected.extend(reference_fragments(first, input));
        assert_eq!(tree.items(), expected, "input after marker {input:?}");

        // Behind a short text fragment: the first line tops it up, the rest streams.
        let mut fresh: SumTree<BufferText> = SumTree::new();
        fresh.append_str(input);
        let mut tree: SumTree<BufferText> = SumTree::new();
        tree.append_str("head");
        tree.append_str(input);
        assert_eq!(
            tree.debug(),
            format!("head{}", fresh.debug()),
            "input after text {input:?}"
        );
    }
}
