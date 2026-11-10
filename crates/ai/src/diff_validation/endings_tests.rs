//! #1956: a text written over a file that mixes line endings keeps each line's own ending.

use super::*;

fn re_ended(original: &str, text: &str) -> String {
    text_with_line_endings_of(original, text.to_owned())
}

#[test]
fn raw_lines_split_at_every_kind_of_line_break_and_lose_nothing() {
    assert_eq!(
        raw_lines("a\r\nb\nc\rd"),
        vec![("a", "\r\n"), ("b", "\n"), ("c", "\r"), ("d", "")]
    );
    assert_eq!(raw_lines("a\n\n"), vec![("a", "\n"), ("", "\n")]);
    assert_eq!(raw_lines(""), Vec::<RawLine<'_>>::new());
    for text in ["a\r\nb\nc\rd", "\r\n\r\n", "x", "é\r\nü\n"] {
        let joined: String = raw_lines(text)
            .iter()
            .flat_map(|&(content, ending)| [content, ending])
            .collect();
        assert_eq!(joined, text);
    }
}

#[test]
fn mixed_means_more_than_one_kind_of_line_break() {
    for text in ["a\r\nb\n", "a\nb\r\n", "a\rb\n", "a\r\nb\r", "a\r\n\n"] {
        assert!(has_mixed_line_endings(text), "{text:?}");
    }
    for text in [
        "",
        "a",
        "a\n",
        "a\nb\n",
        "a\r\nb\r\n",
        "a\r\nb",
        "a\rb\r",
        "\n\n",
    ] {
        assert!(!has_mixed_line_endings(text), "{text:?}");
    }
}

/// One line ending in the file: the caller's text is the exact result and comes back untouched,
/// whatever line ending it uses itself.
#[test]
fn a_file_with_one_line_ending_is_not_touched() {
    for (original, text) in [
        ("a\r\nb\r\n", "a\r\nX\r\n"),
        ("a\r\nb\r\n", "a\nX\n"),
        ("a\nb\n", "a\r\nX\r\n"),
        ("a\nb", "a\nb\nc"),
        ("solo", "solo\nmore\n"),
        ("", "new\n"),
    ] {
        assert_eq!(re_ended(original, text), text, "{original:?}");
    }
}

/// The editor wrote the majority ending on every line; each line gets its own back.
#[test]
fn unchanged_lines_keep_their_own_ending() {
    let original = "one\r\ntwo\nthree\r\nfour\r\n";
    // Nothing changed: the file comes back byte for byte, from either serialisation.
    assert_eq!(
        re_ended(original, "one\r\ntwo\r\nthree\r\nfour\r\n"),
        original
    );
    assert_eq!(re_ended(original, "one\ntwo\nthree\nfour\n"), original);
    // One line changed: it keeps the ending of the line it replaces.
    assert_eq!(
        re_ended(original, "one\r\nTWO\r\nthree\r\nfour\r\n"),
        "one\r\nTWO\nthree\r\nfour\r\n"
    );
    assert_eq!(
        re_ended(original, "ONE\r\ntwo\r\nthree\r\nfour\r\n"),
        "ONE\r\ntwo\nthree\r\nfour\r\n"
    );
    assert_eq!(
        re_ended(original, "one\r\ntwo\r\nthree\r\nFOUR\r\n"),
        "one\r\ntwo\nthree\r\nFOUR\r\n"
    );
}

#[test]
fn inserted_and_deleted_lines_leave_the_other_lines_alone() {
    let original = "a\r\nb\nc\r\n";
    // An inserted line takes the ending of the line before it, or after it at the top.
    assert_eq!(re_ended(original, "a\nX\nb\nc\n"), "a\r\nX\r\nb\nc\r\n");
    assert_eq!(re_ended(original, "a\nb\nX\nc\n"), "a\r\nb\nX\nc\r\n");
    assert_eq!(re_ended(original, "X\na\nb\nc\n"), "X\r\na\r\nb\nc\r\n");
    assert_eq!(re_ended(original, "a\nb\nc\nX\n"), "a\r\nb\nc\r\nX\r\n");
    // A deleted line takes its ending with it.
    assert_eq!(re_ended(original, "a\nc\n"), "a\r\nc\r\n");
    assert_eq!(re_ended(original, "b\nc\n"), "b\nc\r\n");
    assert_eq!(re_ended(original, "a\nb\n"), "a\r\nb\n");
}

#[test]
fn a_changed_run_takes_the_endings_of_the_lines_it_replaces() {
    // Two lines replaced by two: position by position.
    assert_eq!(
        re_ended("a\r\nb\nc\r\nd\n", "a\nB\nC\nd\n"),
        "a\r\nB\nC\r\nd\n"
    );
    // One line replaced by three: the surplus takes the last replaced line's ending.
    assert_eq!(
        re_ended("a\r\nb\nc\r\n", "a\nB1\nB2\nB3\nc\n"),
        "a\r\nB1\nB2\nB3\nc\r\n"
    );
    // Nothing in common: positions as far as they go.
    assert_eq!(re_ended("a\r\nb\n", "X\nY\nZ\n"), "X\r\nY\nZ\n");
}

/// Lines that repeat (`}`) are matched in order, so a deleted first line does not hand its
/// ending to a later line with the same text.
#[test]
fn repeated_lines_are_matched_in_order() {
    assert_eq!(re_ended("}\r\na\n}\n", "a\n}\n"), "a\n}\n");
    assert_eq!(re_ended("}\r\na\n}\n", "}\na\n"), "}\r\na\n");
    assert_eq!(
        re_ended("x\r\n}\n\r\ny\r\n}\r\n", "x\n}\n\ny\nNEW\n}\n"),
        "x\r\n}\n\r\ny\r\nNEW\r\n}\r\n"
    );
}

/// Whether the text ends in a line break is the text's own choice.
#[test]
fn the_final_newline_state_is_the_texts() {
    assert_eq!(re_ended("a\r\nb\n", "a\nb"), "a\r\nb");
    assert_eq!(re_ended("a\r\nb\n", "a\nB"), "a\r\nB");
    // The original's unterminated last line gains a line break: the line before lends its own.
    assert_eq!(re_ended("a\r\nb\nc", "a\nb\nc\n"), "a\r\nb\nc\n");
    assert_eq!(re_ended("a\r\nb\nc", "a\nb\nc\nd"), "a\r\nb\nc\nd");
    assert_eq!(re_ended("a\r\nb\nc", "a\nb\nc"), "a\r\nb\nc");
    assert_eq!(re_ended("a\r\nb\n", ""), "");
}

/// A lone `\r` line ending in front of an empty LF line would read back as one CRLF line break:
/// the result would not be the same text, so the caller's text is written as it is.
#[test]
fn a_result_that_would_change_the_text_is_never_returned() {
    let text = "a\n\nc\n";
    assert_eq!(re_ended("a\rb\n\nc\n", text), text);
}

#[test]
fn only_line_endings_ever_differ_from_the_text() {
    let originals = [
        "one\r\ntwo\nthree\r\nfour\r\n",
        "a\r\nb\nc",
        "a\rb\nc\r\n",
        "\r\n\n\r\n",
        "}\r\na\n}\n",
    ];
    let texts = [
        "",
        "one\ntwo\nthree\nfour\n",
        "one\r\nthree\r\nfive",
        "\n\n\n",
        "a\n}\na\n}\n",
        "x",
        "é\nü\n",
    ];
    for original in originals {
        for text in texts {
            assert_eq!(
                lf_normalized(&re_ended(original, text)),
                lf_normalized(text),
                "{original:?} / {text:?}"
            );
        }
    }
}

/// A changed middle too large for the exact table takes the greedy match: every unchanged line
/// still keeps its own ending here, and the two changed lines take the ones they replace.
#[test]
fn a_large_changed_middle_still_keeps_each_line_ending() {
    const LINES: usize = 1100;
    assert!(LINES * LINES > MAX_ALIGNMENT_CELLS);
    let ending = |index: usize| if index % 3 == 0 { "\n" } else { "\r\n" };
    let content = |index: usize, edited: bool| match (edited, index) {
        (true, 0) => "FIRST".to_owned(),
        (true, index) if index == LINES - 1 => "LAST".to_owned(),
        (_, index) => format!("line {index}"),
    };
    let mut original = String::new();
    let mut buffer_text = String::new();
    let mut expected = String::new();
    for index in 0..LINES {
        original.push_str(&content(index, false));
        original.push_str(ending(index));
        buffer_text.push_str(&content(index, true));
        buffer_text.push_str("\r\n");
        expected.push_str(&content(index, true));
        expected.push_str(ending(index));
    }
    assert_eq!(text_with_line_endings_of(&original, buffer_text), expected);
}
