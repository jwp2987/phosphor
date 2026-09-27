use super::ExternalProductIcon;

/// `25f079350`: an exact match still resolves -- the prefix match must not
/// regress the plain, undecorated title.
#[test]
fn exact_title_resolves() {
    assert!(matches!(
        ExternalProductIcon::from_string("GitHub"),
        Some(ExternalProductIcon::Github)
    ));
    assert!(matches!(
        ExternalProductIcon::from_string("github"),
        Some(ExternalProductIcon::Github)
    ));
}

/// The bug this ports a fix for: a decorated title like "Sentry (OAuth)" used
/// to fall through to `None` (initials in the UI) because `from_string` only
/// matched the product name exactly. Since the fork doesn't carry the Sentry
/// icon asset, this is exercised on a product the fork does have.
#[test]
fn decorated_title_resolves_by_prefix() {
    assert!(matches!(
        ExternalProductIcon::from_string("GitHub (OAuth)"),
        Some(ExternalProductIcon::Github)
    ));
    assert!(matches!(
        ExternalProductIcon::from_string("Slack (workspace-shared)"),
        Some(ExternalProductIcon::Slack)
    ));
}

/// The match is case-insensitive on the whole title, not just the prefix.
#[test]
fn match_is_case_insensitive() {
    assert!(matches!(
        ExternalProductIcon::from_string("HEROKU (Team)"),
        Some(ExternalProductIcon::Heroku)
    ));
}

/// A title that merely contains a product name later in the string (not as a
/// prefix) must not match -- this is a prefix match, not a substring match.
#[test]
fn substring_that_is_not_a_prefix_does_not_match() {
    assert!(ExternalProductIcon::from_string("My GitHub Proxy").is_none());
}

/// No configured product name is a prefix of an unrelated title.
#[test]
fn unrecognized_title_resolves_to_none() {
    assert!(ExternalProductIcon::from_string("Totally Unrelated MCP Server").is_none());
}
