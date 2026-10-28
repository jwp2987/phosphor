use super::single_line_title;

#[test]
fn single_line_text_is_unchanged() {
    assert_eq!(single_line_title("ls -la"), "ls -la");
    assert_eq!(single_line_title("  vim  "), "  vim  ");
    assert_eq!(single_line_title(""), "");
}

#[test]
fn multiline_text_becomes_first_line_and_ellipsis() {
    assert_eq!(
        single_line_title("echo PL1\necho PL2\necho PL3"),
        "echo PL1\u{2026}"
    );
    assert_eq!(
        single_line_title("sleep 4\necho PL1\necho PL2"),
        "sleep 4\u{2026}"
    );
    assert_eq!(
        single_line_title("gradle one\ngradle two"),
        "gradle one\u{2026}"
    );
}

#[test]
fn multiline_text_never_glues_its_lines_together() {
    let title = single_line_title("echo PL1\necho PL2\necho PL3");
    assert!(!title.contains("PL1echo"), "lines were glued: {title:?}");
    assert!(
        !title.contains(['\n', '\r']),
        "title kept a line break: {title:?}"
    );
}

#[test]
fn leading_blank_lines_are_skipped() {
    assert_eq!(
        single_line_title("\n\n  \necho PL1\necho PL2"),
        "echo PL1\u{2026}"
    );
    assert_eq!(single_line_title("\n  make test"), "make test");
}

#[test]
fn crlf_line_breaks_are_handled() {
    assert_eq!(single_line_title("a\r\nb"), "a\u{2026}");
    assert_eq!(single_line_title("a\rb"), "a\u{2026}");
    assert_eq!(single_line_title("a\r\n"), "a");
}

#[test]
fn trailing_newline_only_gets_no_ellipsis() {
    assert_eq!(
        single_line_title("cargo nextest run\n"),
        "cargo nextest run"
    );
    assert_eq!(
        single_line_title("cargo nextest run\n\n  \n"),
        "cargo nextest run"
    );
}

#[test]
fn blank_only_text_becomes_empty() {
    assert_eq!(single_line_title("\n"), "");
    assert_eq!(single_line_title(" \r\n \n"), "");
}

#[test]
fn long_first_line_is_kept_whole_for_the_renderer_to_clip() {
    let long = "x".repeat(300);
    assert_eq!(single_line_title(&long), long.as_str());
    assert_eq!(
        single_line_title(&format!("{long}\necho next")),
        format!("{long}\u{2026}")
    );
}
