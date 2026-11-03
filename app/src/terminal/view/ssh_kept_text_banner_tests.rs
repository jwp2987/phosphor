use super::*;

#[test]
fn preview_shows_short_text_whole_and_line_breaks_as_a_mark() {
    assert_eq!(kept_text_preview("echo hi"), "echo hi");
    assert_eq!(kept_text_preview("ls\npwd"), "ls⏎pwd");
    assert_eq!(kept_text_preview("a\tb"), "a b");
}

#[test]
fn preview_truncates_long_text_to_the_cap_with_an_ellipsis() {
    let long = "x".repeat(KEPT_TEXT_PREVIEW_CHARS + 10);
    let preview = kept_text_preview(&long);
    assert_eq!(preview.chars().count(), KEPT_TEXT_PREVIEW_CHARS);
    assert!(preview.ends_with('…'));
    let exact = "y".repeat(KEPT_TEXT_PREVIEW_CHARS);
    assert_eq!(
        kept_text_preview(&exact),
        exact,
        "at the cap nothing is cut"
    );
}

#[test]
fn typed_bytes_send_each_line_break_as_enter_and_add_none() {
    assert_eq!(kept_text_bytes("echo hi"), b"echo hi".to_vec());
    assert_eq!(kept_text_bytes("ls\npwd"), b"ls\rpwd".to_vec());
    assert_eq!(kept_text_bytes("ls\r\npwd"), b"ls\rpwd".to_vec());
}
