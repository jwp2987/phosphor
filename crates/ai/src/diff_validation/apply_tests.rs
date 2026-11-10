//! #1925: an applied edit changes the edited lines and nothing else. The matrix is line ending
//! (LF, CRLF) x final newline (yes, no) x where the edit lands (first line, middle, last line,
//! append, whole file, empty file), plus files that mix endings (#1956).

use std::ops::Range;

use super::*;

const BASE: [&str; 4] = ["one", "two", "three", "four"];

fn delta(range: Range<usize>, insertion: &str) -> DiffDelta {
    DiffDelta {
        replacement_line_range: range,
        insertion: insertion.to_owned(),
    }
}

/// `lines` joined by `eol`, the last one ended with `eol` only when `final_newline`.
fn file(lines: &[&str], eol: &str, final_newline: bool) -> String {
    let mut text = lines.join(eol);
    if final_newline && !lines.is_empty() {
        text.push_str(eol);
    }
    text
}

/// Runs `check(eol, final_newline)` for LF and CRLF, with and without a final newline.
fn for_each_shape(check: impl Fn(&str, bool)) {
    for eol in ["\n", "\r\n"] {
        for final_newline in [true, false] {
            check(eol, final_newline);
        }
    }
}

fn applied(content: &str, deltas: &[DiffDelta]) -> String {
    apply_deltas_to_text(content, deltas).expect("the deltas are in range")
}

#[test]
fn an_edit_of_the_first_line_keeps_every_ending() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(1..2, "ONE")]),
            file(&["ONE", "two", "three", "four"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

#[test]
fn an_edit_of_a_middle_line_keeps_every_ending() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(2..3, "TWO")]),
            file(&["one", "TWO", "three", "four"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

/// The reported defect: replacing the last line of a file with no final newline added one
/// (217 -> 218 bytes for LF, 226 -> 228 for CRLF).
#[test]
fn an_edit_of_the_last_line_keeps_the_final_newline_state() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(4..5, "FOUR")]),
            file(&["one", "two", "three", "FOUR"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

/// The shape of the app case `agent_edit_endings`: several hunks, the last on the last line,
/// given out of order.
#[test]
fn a_multi_hunk_edit_ending_on_the_last_line_keeps_every_ending() {
    for_each_shape(|eol, fin| {
        let deltas = [delta(4..5, "FOUR"), delta(1..2, "ONE"), delta(2..3, "TWO")];
        assert_eq!(
            applied(&file(&BASE, eol, fin), &deltas),
            file(&["ONE", "TWO", "three", "FOUR"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

#[test]
fn a_line_break_inside_the_insertion_takes_the_file_ending() {
    for_each_shape(|eol, fin| {
        let expected = file(&["one", "TWO", "TWO-B", "three", "four"], eol, fin);
        // The model's text is LF, or CRLF with its own trailing break: same lines either way.
        for insertion in ["TWO\nTWO-B", "TWO\r\nTWO-B\r\n"] {
            assert_eq!(
                applied(&file(&BASE, eol, fin), &[delta(2..3, insertion)]),
                expected,
                "eol {eol:?}, final newline {fin}, insertion {insertion:?}"
            );
        }
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(4..5, "FOUR\nFIVE")]),
            file(&["one", "two", "three", "FOUR", "FIVE"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

#[test]
fn a_deleted_line_takes_only_its_own_bytes() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(2..3, "")]),
            file(&["one", "three", "four"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
        // Deleting the last line leaves line three as it was, which is terminated.
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(4..5, "")]),
            file(&["one", "two", "three"], eol, true),
            "eol {eol:?}, final newline {fin}"
        );
        assert_eq!(applied(&file(&BASE, eol, fin), &[delta(1..5, "")]), "");
    });
}

#[test]
fn an_inserted_line_takes_the_file_ending() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(3..3, "NEW")]),
            file(&["one", "two", "NEW", "three", "four"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
        let prepended = file(&["NEW", "one", "two", "three", "four"], eol, fin);
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(0..0, "NEW")]),
            prepended
        );
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(1..1, "NEW")]),
            prepended
        );
    });
}

/// #1954: appended text is a new line. After a last line with no line ending it used to be
/// joined onto that line (`four` + `five` -> `fourfive\n`).
#[test]
fn an_append_starts_a_new_line_and_keeps_the_final_newline_state() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(5..5, "five")]),
            file(&["one", "two", "three", "four", "five"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(5..5, "five\nsix")]),
            file(&["one", "two", "three", "four", "five", "six"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
        // An edit of the last line together with an append after it.
        assert_eq!(
            applied(
                &file(&BASE, eol, fin),
                &[delta(4..5, "FOUR"), delta(5..5, "five")]
            ),
            file(&["one", "two", "three", "FOUR", "five"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

#[test]
fn a_whole_file_replace_keeps_the_ending_and_the_final_newline_state() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(1..5, "a\nb")]),
            file(&["a", "b"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

/// An insertion that ends in a line break says so: at the end of a file it ends the last line.
#[test]
fn a_line_break_at_the_end_of_the_insertion_is_taken_at_its_word() {
    for_each_shape(|eol, fin| {
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(4..5, "FOUR\n")]),
            file(&["one", "two", "three", "FOUR"], eol, true),
            "eol {eol:?}, final newline {fin}"
        );
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(1..5, "a\nb\n")]),
            file(&["a", "b"], eol, true),
            "eol {eol:?}, final newline {fin}"
        );
        // Mid-file it is the line's own ending, not an extra blank line.
        assert_eq!(
            applied(&file(&BASE, eol, fin), &[delta(2..3, "TWO\n")]),
            file(&["one", "TWO", "three", "four"], eol, fin),
            "eol {eol:?}, final newline {fin}"
        );
    });
}

#[test]
fn an_empty_file_takes_the_insertion_literally() {
    assert_eq!(applied("", &[delta(0..0, "a\r\nb")]), "a\r\nb");
    assert_eq!(applied("", &[delta(0..0, "a\nb\n")]), "a\nb\n");
    assert_eq!(applied("", &[delta(0..0, "")]), "");
}

#[test]
fn a_one_line_file_keeps_its_final_newline_state() {
    assert_eq!(applied("solo", &[delta(1..2, "SOLO")]), "SOLO");
    assert_eq!(applied("solo\n", &[delta(1..2, "SOLO")]), "SOLO\n");
    assert_eq!(applied("solo\r\n", &[delta(1..2, "SOLO")]), "SOLO\r\n");
    // With no line ending anywhere in the file, an appended line is separated by LF.
    assert_eq!(applied("solo", &[delta(2..2, "more")]), "solo\nmore");
    assert_eq!(
        applied("solo\r\n", &[delta(2..2, "more")]),
        "solo\r\nmore\r\n"
    );
}

/// #1956: in a file that mixes endings every untouched line keeps its own, and an edited line
/// keeps the ending of the line it replaces.
#[test]
fn a_file_with_mixed_endings_keeps_each_line_ending() {
    let mixed = "one\r\ntwo\nthree\r\nfour";
    assert_eq!(
        applied(mixed, &[delta(2..3, "TWO"), delta(4..5, "FOUR")]),
        "one\r\nTWO\nthree\r\nFOUR"
    );
    // An inserted line takes the ending of the line before it.
    assert_eq!(
        applied(mixed, &[delta(2..2, "NEW")]),
        "one\r\nNEW\r\ntwo\nthree\r\nfour"
    );
    assert_eq!(
        applied(mixed, &[delta(5..5, "five")]),
        "one\r\ntwo\nthree\r\nfour\r\nfive"
    );
}

#[test]
fn a_range_outside_the_file_is_an_error() {
    let content = file(&BASE, "\n", true);
    assert!(apply_deltas_to_text(&content, &[delta(6..7, "x")]).is_err());
    assert!(apply_deltas_to_text(&content, &[delta(4..9, "x")]).is_err());
    assert!(apply_deltas_to_text(&content, &[delta(3..2, "x")]).is_err());
}

#[test]
fn lf_normalized_turns_every_ending_into_lf() {
    assert_eq!(lf_normalized("a\r\nb\rc\nd"), "a\nb\nc\nd");
    assert_eq!(lf_normalized("plain\n"), "plain\n");
}

// --- `accepted_update_text`: what the GUI accept writes ---

/// A file with one line ending: the editor's text already is the exact result, and it is what
/// comes back.
#[test]
fn accepted_text_for_a_uniform_file_is_the_exact_result() {
    for_each_shape(|eol, fin| {
        let original = file(&BASE, eol, fin);
        let deltas = [delta(2..3, "TWO"), delta(4..5, "FOUR")];
        let expected = file(&["one", "TWO", "three", "FOUR"], eol, fin);
        assert_eq!(
            accepted_update_text(expected.clone(), &original, &deltas),
            expected,
            "eol {eol:?}, final newline {fin}"
        );
    });
}

/// #1956: the editor wrote the majority ending on every line; the accept writes each line's own.
#[test]
fn accepted_text_for_a_mixed_file_keeps_each_line_ending() {
    let original = "one\r\ntwo\nthree\r\nfour\r\n";
    let deltas = [delta(2..3, "TWO")];
    let buffer_text = "one\r\nTWO\r\nthree\r\nfour\r\n".to_owned();
    assert_eq!(
        accepted_update_text(buffer_text, original, &deltas),
        "one\r\nTWO\nthree\r\nfour\r\n"
    );
}

/// The user changed the proposal in the diff view: what they saw is what is written. On a file
/// that mixes line endings every line still keeps its own (#1956, owner decision 2026-11-10;
/// until then the buffer's one line ending was written over the whole file here).
#[test]
fn accepted_text_keeps_each_line_ending_when_the_user_changed_the_proposal() {
    let original = "one\r\ntwo\nthree\r\n";
    let deltas = [delta(2..3, "TWO")];
    let buffer_text = "one\r\nTWO and more\r\nthree\r\n".to_owned();
    assert_eq!(
        accepted_update_text(buffer_text, original, &deltas),
        "one\r\nTWO and more\nthree\r\n"
    );
    // The user added a line and removed one as well.
    let original = "one\r\ntwo\nthree\r\nfour\n";
    let buffer_text = "one\r\nTWO\r\nextra\r\nfour\r\n".to_owned();
    assert_eq!(
        accepted_update_text(buffer_text, original, &deltas),
        "one\r\nTWO\nextra\r\nfour\n"
    );
}

/// The same on a file with one line ending: the buffer's text, untouched.
#[test]
fn accepted_text_is_the_buffer_when_the_user_changed_the_proposal_of_a_uniform_file() {
    for_each_shape(|eol, fin| {
        let original = file(&BASE, eol, fin);
        let deltas = [delta(2..3, "TWO")];
        let buffer_text = file(&["one", "TWO and more", "three", "four"], eol, fin);
        assert_eq!(
            accepted_update_text(buffer_text.clone(), &original, &deltas),
            buffer_text,
            "eol {eol:?}, final newline {fin}"
        );
    });
}

/// Deltas that do not fit the original, or an original with no line break: the buffer decides.
#[test]
fn accepted_text_is_the_buffer_when_there_is_nothing_exact_to_prefer() {
    let buffer_text = "whatever the editor holds\n".to_owned();
    assert_eq!(
        accepted_update_text(buffer_text.clone(), "one\ntwo\n", &[delta(7..8, "x")]),
        buffer_text
    );
    let one_line = "solo\r\nmore".to_owned();
    assert_eq!(
        accepted_update_text(one_line.clone(), "solo", &[delta(2..2, "more")]),
        one_line
    );
}
