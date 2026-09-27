use regex::Regex;

use super::*;

fn rule(pattern: &str, color: AnsiColorIdentifier) -> HostFooterColorRule {
    HostFooterColorRule {
        pattern: Regex::new(pattern).expect("valid test regex"),
        color,
        name: None,
    }
}

fn named_rule(pattern: &str, color: AnsiColorIdentifier, name: &str) -> HostFooterColorRule {
    HostFooterColorRule {
        pattern: Regex::new(pattern).expect("valid test regex"),
        color,
        name: Some(name.to_string()),
    }
}

// --- resolve_host precedence ---------------------------------------------------

/// Source 1 (shell-integration hostname) wins whenever the session is genuinely
/// remote, even if source 2 (a pending SSH target) disagrees -- which should never
/// happen in practice, but proves the precedence rather than an accidental
/// last-write-wins.
///
/// Breaks if the precedence is flipped (checking `pending_ssh_target` before
/// `session_type`) or if the function returns the pending target instead of the
/// session hostname here.
#[test]
fn remote_bootstrapped_hostname_wins_over_pending_ssh_target() {
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let resolved = resolve_host(
        &session_type,
        "prod-db-1",
        Some(Some("unrelated-host".to_string())),
    );
    assert_eq!(resolved, ResolvedHost::Named("prod-db-1".to_string()));
}

/// A `Local` session with no interactive-SSH-shaped command in flight resolves to
/// `Local`, not `Unknown` and not a stale hostname.
///
/// Breaks if `None` (source 2's "nothing in flight" case) is mapped to `Unknown` or
/// `Named` instead of `Local`.
#[test]
fn local_session_with_no_pending_ssh_command_is_local() {
    let resolved = resolve_host(&SessionType::Local, "my-laptop", None);
    assert_eq!(resolved, ResolvedHost::Local);
}

/// A `Local` session running a typed `ssh host` command falls back to source 2's
/// resolved host.
///
/// Breaks if the fallback to `pending_ssh_target` is removed, or if it returns
/// `Local`/`Unknown` instead of the parsed host.
#[test]
fn local_session_falls_back_to_pending_ssh_host() {
    let resolved = resolve_host(
        &SessionType::Local,
        "my-laptop",
        Some(Some("prod-web-3".to_string())),
    );
    assert_eq!(resolved, ResolvedHost::Named("prod-web-3".to_string()));
}

/// The exact defect this module exists to prevent: a `Local` session running an
/// SSH-shaped command whose host could not be parsed (`gcloud compute ssh`, an SSH
/// alias, etc.) must resolve to `Unknown`, never silently to `Local`.
///
/// Breaks if `Some(None)` is mapped to `ResolvedHost::Local` (or to `Named("")`)
/// instead of `ResolvedHost::Unknown`.
#[test]
fn local_session_with_unresolved_ssh_command_is_unknown_not_local() {
    let resolved = resolve_host(&SessionType::Local, "my-laptop", Some(None));
    assert_eq!(resolved, ResolvedHost::Unknown);
    assert_ne!(resolved, ResolvedHost::Local);
}

/// Defensive edge case: even if `session_type` claims `WarpifiedRemote`, an empty
/// hostname carries no information to match against and must not be treated as a
/// legitimate (empty-string) host.
///
/// Breaks if an empty `session_hostname` is wrapped in `ResolvedHost::Named(String::new())`
/// instead of `ResolvedHost::Unknown`.
#[test]
fn remote_session_with_empty_hostname_is_unknown() {
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let resolved = resolve_host(&session_type, "", None);
    assert_eq!(resolved, ResolvedHost::Unknown);
}

// --- matching_color: first-match-wins and defaults ------------------------------

/// Rules are tried in list order and the first match wins, even when a later rule
/// would also match.
///
/// Breaks if `matching_color` picks the last match, the most specific pattern, or
/// otherwise ignores list order (e.g. searching in reverse).
#[test]
fn first_matching_rule_wins() {
    let rules = vec![
        rule("^prod-", AnsiColorIdentifier::Red),
        rule("^prod-db", AnsiColorIdentifier::Yellow),
    ];

    let host = ResolvedHost::Named("prod-db-1".to_string());
    assert_eq!(
        matching_color(&host, &rules),
        Some(AnsiColorIdentifier::Red)
    );
}

/// A host that matches no configured rule yields the default (`None`), not an
/// arbitrary rule's color.
///
/// Breaks if `matching_color` falls back to `rules.first()` (or any other rule)
/// instead of `None` when nothing actually matches.
#[test]
fn non_matching_host_yields_default() {
    let rules = vec![
        rule("^prod-", AnsiColorIdentifier::Red),
        rule("^staging-", AnsiColorIdentifier::Yellow),
    ];

    let host = ResolvedHost::Named("my-laptop".to_string());
    assert_eq!(matching_color(&host, &rules), None);
}

/// `Local` and `Unknown` can never match any rule, even a catch-all pattern -- this
/// is the load-bearing safety property from the module doc: there is no string to
/// match, so nothing must be substituted in its place (e.g. an empty string, which
/// `.*` or `^$` would happily match).
///
/// Breaks if `Local`/`Unknown` are matched against rules using a placeholder string
/// (such as `""`) instead of short-circuiting to `None` before any rule is
/// consulted.
#[test]
fn unknown_and_local_never_match_any_rule() {
    let catch_all = vec![rule(".*", AnsiColorIdentifier::Red)];

    assert_eq!(matching_color(&ResolvedHost::Local, &catch_all), None);
    assert_eq!(matching_color(&ResolvedHost::Unknown, &catch_all), None);
}

/// `resolve_footer_bar_color` composes `resolve_host` and `matching_color`
/// end-to-end: a remote session's hostname reaches rule matching.
///
/// Breaks if the composition drops either step, e.g. always returning `None` or
/// ignoring `session_hostname`.
#[test]
fn resolve_footer_bar_color_end_to_end() {
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let rules = vec![rule("^prod-", AnsiColorIdentifier::Red)];

    let color = resolve_footer_bar_color(
        &session_type,
        "prod-db-1",
        None,
        &rules,
        AnsiColorIdentifier::Yellow,
    );
    assert_eq!(color, Some(AnsiColorIdentifier::Red));
}

// --- resolve_footer_bar_color: the unknown-host color -----------------------------

/// The safety property the coordinator called out explicitly: an unidentifiable
/// remote host must render the configured caution color, never the same `None`
/// (default) a genuinely local shell gets.
///
/// Breaks if this reverts to returning `None` for `ResolvedHost::Unknown` (i.e. if
/// `resolve_footer_bar_color` goes back to being a bare `matching_color` call with
/// no special case for `Unknown`).
#[test]
fn unknown_host_yields_the_configured_unknown_color() {
    let color = resolve_footer_bar_color(
        &SessionType::Local,
        "my-laptop",
        Some(None),
        &[],
        AnsiColorIdentifier::Yellow,
    );
    assert_eq!(color, Some(AnsiColorIdentifier::Yellow));
}

/// An ordinary local shell (no SSH-shaped command in flight) must still yield the
/// default (`None`), not the unknown-host color -- the unknown color is for "we
/// don't know", not for "definitely local".
///
/// Breaks if `Local` starts also returning `unknown_host_color` (e.g. by
/// simplifying the `Local` and `Unknown` arms to share a branch).
#[test]
fn local_host_still_yields_default_not_unknown_color() {
    let color = resolve_footer_bar_color(
        &SessionType::Local,
        "my-laptop",
        None,
        &[],
        AnsiColorIdentifier::Yellow,
    );
    assert_eq!(color, None);
}

/// The unknown-host color is independent of the configured rules: even when a rule
/// is present that would match some other string, an `Unknown` host must still
/// yield `unknown_host_color`, never a rule's color.
///
/// Breaks if `Unknown` is routed through `matching_color` (e.g. by matching it
/// against a placeholder string), which would let an unrelated rule's color leak
/// through instead of the dedicated unknown-host color.
#[test]
fn unknown_host_color_is_independent_of_rules() {
    let rules = vec![rule(".*", AnsiColorIdentifier::Red)];

    let color = resolve_footer_bar_color(
        &SessionType::Local,
        "my-laptop",
        Some(None),
        &rules,
        AnsiColorIdentifier::Yellow,
    );
    assert_eq!(color, Some(AnsiColorIdentifier::Yellow));
}

// --- resolve_footer_bar_tooltip: explains what resolve_footer_bar_color painted --

/// #700: a colored footer bar with no explanation is the whole bug. When a rule
/// matches, the tooltip must name the resolved host so hovering the bar answers
/// "why is this colored" without a trip to Settings.
#[test]
fn matched_host_tooltip_names_the_host() {
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let rules = vec![rule("^prod-", AnsiColorIdentifier::Red)];

    let tooltip = resolve_footer_bar_tooltip(&session_type, "prod-db-1", None, &rules);
    assert_eq!(tooltip.as_deref(), Some("prod-db-1"));
}

/// When the matching rule has a user-given name, the tooltip surfaces it alongside
/// the host so a rule like "prod fleet" is recognizable, not just its raw pattern.
#[test]
fn matched_host_tooltip_includes_the_rule_name_when_set() {
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let rules = vec![named_rule("^prod-", AnsiColorIdentifier::Red, "Production")];

    let tooltip = resolve_footer_bar_tooltip(&session_type, "prod-db-1", None, &rules);
    assert_eq!(tooltip.as_deref(), Some("prod-db-1 (Production)"));
}

/// No tooltip for a plain local session -- `resolve_footer_bar_color` paints
/// nothing for it either, so there is nothing to explain.
#[test]
fn local_session_has_no_tooltip() {
    let tooltip = resolve_footer_bar_tooltip(&SessionType::Local, "my-laptop", None, &[]);
    assert_eq!(tooltip, None);
}

/// No tooltip for a named host that no rule matches -- the bar is uncolored
/// (`None`, the default), matching `resolve_footer_bar_color`'s own behavior for
/// this case (see `non_matching_host_yields_default` above).
#[test]
fn non_matching_named_host_has_no_tooltip() {
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let rules = vec![rule("^staging-", AnsiColorIdentifier::Red)];

    let tooltip = resolve_footer_bar_tooltip(&session_type, "prod-db-1", None, &rules);
    assert_eq!(tooltip, None);
}

/// An unknown host gets an explanatory tooltip too, independent of `rules` --
/// mirrors `unknown_host_color_is_independent_of_rules` for the color.
///
/// #700: this text used to be a hardcoded English literal; it now goes through
/// `crate::t!`, so this asserts against the same fluent key's resolved value --
/// initializing i18n first, per `crate::i18n::init`'s doc, so a comparison against
/// the raw key (the "not yet initialized" fallback) can't pass by accident.
#[test]
fn unknown_host_tooltip_is_independent_of_rules() {
    crate::i18n::init(Some("en"));
    let rules = vec![rule(".*", AnsiColorIdentifier::Red)];

    let tooltip = resolve_footer_bar_tooltip(&SessionType::Local, "my-laptop", Some(None), &rules);
    assert_eq!(
        tooltip.as_deref(),
        Some(crate::t!("terminal-host-footer-bar-unknown-host").as_str())
    );
    assert_ne!(
        tooltip.as_deref(),
        Some("terminal-host-footer-bar-unknown-host")
    );
}

// --- resolve_footer_bar_color_and_tooltip: single-pass resolution (#700) --------

/// The combined resolver must agree with the two individual ones for a matched
/// named host -- proving the single-pass refactor didn't change behavior, only
/// how many times the host is resolved and the rules are matched.
#[test]
fn combined_resolver_agrees_with_individual_calls_for_matched_host() {
    crate::i18n::init(Some("en"));
    let session_type = SessionType::WarpifiedRemote { host_id: None };
    let rules = vec![named_rule("^prod-", AnsiColorIdentifier::Red, "Production")];

    let (color, tooltip) = resolve_footer_bar_color_and_tooltip(
        &session_type,
        "prod-db-1",
        None,
        &rules,
        AnsiColorIdentifier::Yellow,
    );

    assert_eq!(color, Some(AnsiColorIdentifier::Red));
    assert_eq!(tooltip.as_deref(), Some("prod-db-1 (Production)"));
}

/// Same agreement check for the unknown-host case: the combined resolver must
/// still yield the configured unknown-host color and the localized tooltip,
/// independent of `rules`.
#[test]
fn combined_resolver_agrees_with_individual_calls_for_unknown_host() {
    crate::i18n::init(Some("en"));
    let rules = vec![rule(".*", AnsiColorIdentifier::Red)];

    let (color, tooltip) = resolve_footer_bar_color_and_tooltip(
        &SessionType::Local,
        "my-laptop",
        Some(None),
        &rules,
        AnsiColorIdentifier::Yellow,
    );

    assert_eq!(color, Some(AnsiColorIdentifier::Yellow));
    assert_eq!(
        tooltip.as_deref(),
        Some(crate::t!("terminal-host-footer-bar-unknown-host").as_str())
    );
}

/// And for a plain local session: no color, no tooltip -- there is nothing to
/// paint or explain.
#[test]
fn combined_resolver_agrees_with_individual_calls_for_local_session() {
    let (color, tooltip) = resolve_footer_bar_color_and_tooltip(
        &SessionType::Local,
        "my-laptop",
        None,
        &[],
        AnsiColorIdentifier::Yellow,
    );

    assert_eq!(color, None);
    assert_eq!(tooltip, None);
}
