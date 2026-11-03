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

/// #1278: `mode` is case-insensitive; old code fell through to Line for "RAW" (wrapping the
/// input in ^A ^K ... CR) and for any unknown value.
#[test]
fn write_mode_is_case_insensitive_and_unknown_modes_error() {
    use api::message::tool_call::write_to_long_running_shell_command::mode::Mode as InnerMode;
    let parse = |mode: &str| {
        write_from_args(&format!(
            r#"{{"command_id":"c","input":"y","mode":"{mode}"}}"#
        ))
    };
    let inner = |mode: &str| match parse(mode).unwrap() {
        api::message::tool_call::Tool::WriteToLongRunningShellCommand(w) => {
            w.mode.unwrap().mode.unwrap()
        }
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(inner("RAW"), InnerMode::Raw(())));
    assert!(matches!(inner("Block"), InnerMode::Block(())));
    assert!(matches!(inner("line"), InnerMode::Line(())));
    assert!(parse("typo").is_err());
}
