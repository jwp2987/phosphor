use super::{
    fallback_font_dropdown_should_include_font, is_valid_host_footer_color_rule_pattern, FontType,
};
use crate::settings::{MonospaceFallbackFontName, DEFAULT_MONOSPACE_FONT_NAME};
use settings::Setting as _;

#[test]
fn fallback_font_dropdown_includes_default_monospace_font() {
    assert_eq!(MonospaceFallbackFontName::default_value(), "");
    assert!(fallback_font_dropdown_should_include_font(
        DEFAULT_MONOSPACE_FONT_NAME,
        FontType::Monospace,
        FontType::Monospace,
        "",
    ));
}

#[test]
fn host_footer_color_rule_pattern_accepts_valid_regex() {
    assert!(is_valid_host_footer_color_rule_pattern("prod-.*"));
    // Leading/trailing whitespace around an otherwise-valid pattern is trimmed, not rejected.
    assert!(is_valid_host_footer_color_rule_pattern("  prod-.*  "));
}

#[test]
fn host_footer_color_rule_pattern_rejects_invalid_regex() {
    // An unclosed group is not a valid regex. `HostFooterColorRule::pattern` is a `Regex`, so
    // this must be rejected here -- there is no later validation step that would catch it.
    assert!(!is_valid_host_footer_color_rule_pattern("prod-(unclosed"));
}

#[test]
fn host_footer_color_rule_pattern_rejects_empty() {
    assert!(!is_valid_host_footer_color_rule_pattern(""));
    // Whitespace-only input trims down to empty, and an empty pattern is meaningless as a
    // host-matching rule.
    assert!(!is_valid_host_footer_color_rule_pattern("   "));
}
