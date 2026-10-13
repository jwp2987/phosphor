use super::*;

#[test]
fn prefix_on_char_boundary_keeps_short_text_whole() {
    assert_eq!(prefix_on_char_boundary("git status", 64), "git status");
    assert_eq!(prefix_on_char_boundary("", 0), "");
    assert_eq!(prefix_on_char_boundary("abc", 3), "abc");
}

#[test]
fn prefix_on_char_boundary_backs_off_to_a_char_boundary() {
    // "é" is two bytes, so a 2-byte cap that lands inside it backs off to 1 byte.
    assert_eq!(prefix_on_char_boundary("aé", 2), "a");
    assert_eq!(prefix_on_char_boundary("aé", 3), "aé");
    // A 4-byte emoji with a cap inside it yields the empty prefix rather than panicking.
    assert_eq!(prefix_on_char_boundary("🦀rest", 3), "");
    assert_eq!(prefix_on_char_boundary("🦀rest", 5), "🦀r");
}

#[test]
fn bounded_display_text_marks_truncation_with_an_ellipsis() {
    assert_eq!(bounded_display_text("echo hi", 64), "echo hi");
    assert_eq!(bounded_display_text("abcdef", 3), "abc…");
    assert_eq!(bounded_display_text("aé", 2), "a…");
}

/// The case from #868: a single 50 MB history line must not be copied or laid out whole
/// for display.
#[test]
fn bounded_display_text_bounds_a_huge_line() {
    let huge = "x".repeat(50 * 1024 * 1024);
    let shown = bounded_display_text(&huge, 4096);
    assert_eq!(shown.len(), 4096 + "…".len());
    assert!(shown.ends_with('…'));
    assert!(huge.starts_with(shown.trim_end_matches('…')));
}
