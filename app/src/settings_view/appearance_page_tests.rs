use regex::Regex;

use super::{
    fallback_font_dropdown_should_include_font, host_footer_color_rules_contains_duplicate,
    is_valid_host_footer_color_rule_pattern, FontType,
};
use crate::settings::{MonospaceFallbackFontName, DEFAULT_MONOSPACE_FONT_NAME};
use crate::workspace::tab_settings::HostFooterColorRule;
use settings::Setting as _;
use warp_core::ui::theme::AnsiColorIdentifier;

fn rule(pattern: &str, color: AnsiColorIdentifier) -> HostFooterColorRule {
    HostFooterColorRule {
        pattern: Regex::new(pattern).expect("valid test regex"),
        color,
        name: None,
    }
}

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

/// #699: exercises the actual predicate `commit_host_footer_color_rule` rejects a
/// duplicate with, not just the `HostFooterColorRule::eq` it's built on
/// (`tab_settings_tests::host_footer_color_rule_eq_compares_pattern_only` already
/// covers that). Constructing the full `AppearanceSettingsPageView` to drive
/// `commit_host_footer_color_rule` itself isn't attempted here: its `new` (~740
/// lines) wires up dozens of singleton settings models and typed action views well
/// beyond what this one rejection check touches -- the same tradeoff
/// `features_page_tests.rs` documents for `FeaturesPageView`.
#[test]
fn host_footer_color_rules_contains_duplicate_matches_pattern_only() {
    let existing = [rule("^prod-", AnsiColorIdentifier::Red)];

    // Same pattern, different color: still a duplicate, since a second rule with an
    // already-configured pattern can never match (first-match-wins) regardless of
    // what it's configured to do.
    assert!(host_footer_color_rules_contains_duplicate(
        &existing,
        &rule("^prod-", AnsiColorIdentifier::Blue)
    ));

    // Different pattern: not a duplicate.
    assert!(!host_footer_color_rules_contains_duplicate(
        &existing,
        &rule("^staging-", AnsiColorIdentifier::Red)
    ));

    // Empty rule list: nothing to duplicate.
    assert!(!host_footer_color_rules_contains_duplicate(
        &[],
        &rule("^prod-", AnsiColorIdentifier::Red)
    ));
}
