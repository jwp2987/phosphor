use super::*;

/// `<ENTER>` must expand to CR, the byte the Enter key sends. Expanding it to LF left the
/// input unsubmitted in pwsh, whose line editor reads LF as Ctrl+J.
#[test]
fn enter_tokens_expand_to_carriage_return() {
    for token in ["<ENTER>", "<Enter>", "<CR>"] {
        assert_eq!(
            expand_raw_input_tokens(&format!("y{token}")),
            b"y\r",
            "{token}"
        );
    }
}

/// `<LF>` stays a literal line feed for callers that ask for one by name.
#[test]
fn lf_token_expands_to_line_feed() {
    assert_eq!(expand_raw_input_tokens("a<LF>b"), b"a\nb");
}

#[test]
fn unknown_tokens_pass_through() {
    assert_eq!(expand_raw_input_tokens("<nope> <ESC>"), b"<nope> \x1b");
}
