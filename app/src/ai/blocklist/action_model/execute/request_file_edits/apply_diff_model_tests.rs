//! The remote read of the diff application keeps the file's own line endings (round 10, lab job
//! 17885063245: `lab_fam_agent_edit_endings` on the no-helper arm).

use super::{as_read_file_context_normalises, with_the_files_own_line_endings};

const CRLF: &str = "L01 alpha\r\nL02 beta\r\nL03 gamma\r\n";
const MIXED: &str = "L01 alpha\r\nL02 beta\nL03 gamma\r\nL04 delta";

fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[test]
fn a_crlf_file_is_read_with_its_crlf_endings() {
    assert_eq!(
        with_the_files_own_line_endings(lf(CRLF), Some(CRLF.as_bytes().to_vec())),
        CRLF
    );
}

#[test]
fn a_mixed_ending_file_keeps_each_lines_own_ending_and_its_missing_final_newline() {
    assert_eq!(
        with_the_files_own_line_endings(lf(MIXED), Some(MIXED.as_bytes().to_vec())),
        MIXED
    );
}

#[test]
fn an_lf_file_is_the_normalised_text() {
    let text = "L01 alpha\nL02 beta\n".to_owned();
    assert_eq!(
        with_the_files_own_line_endings(text.clone(), Some(text.clone().into_bytes())),
        text
    );
}

#[test]
fn bytes_of_a_different_text_are_never_used() {
    // The file changed between the two reads: the edit is computed against what the model
    // read, and the write guard then refuses it.
    let normalised = lf(CRLF);
    let changed = "L01 alpha\r\nL02 CHANGED\r\nL03 gamma\r\n";
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), Some(changed.as_bytes().to_vec())),
        normalised
    );
    // A read that was cut short.
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), Some(CRLF.as_bytes()[..12].to_vec())),
        normalised
    );
}

#[test]
fn a_failed_or_non_utf8_raw_read_keeps_the_normalised_text() {
    let normalised = lf(CRLF);
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), None),
        normalised
    );
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), Some(vec![0xff, b'\r', b'\n'])),
        normalised
    );
}

#[test]
fn the_normaliser_matches_read_file_context() {
    // One `\r` in front of a `\n` (or at the very end) goes; any other `\r` stays.
    for (raw, want) in [
        ("a\r\nb\r\n", "a\nb\n"),
        ("a\r\nb", "a\nb"),
        ("a\r\r\nb\n", "a\r\nb\n"),
        ("a\rb\n", "a\rb\n"),
        ("a\nb\r", "a\nb"),
        ("", ""),
        ("\r\n", "\n"),
    ] {
        assert_eq!(as_read_file_context_normalises(raw), want, "{raw:?}");
    }
    // A lone `\r` in the middle of a line is not a line ending for `ReadFileContext`, so such
    // a file still round-trips byte for byte.
    let odd = "a\rb\r\nc\r\n";
    assert_eq!(
        with_the_files_own_line_endings(
            as_read_file_context_normalises(odd),
            Some(odd.as_bytes().to_vec())
        ),
        odd
    );
}
