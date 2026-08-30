//! Regression tests for issue #632 — "Send feedback" opened `zerx-lab/warp`,
//! the dead ancestor whose issue tracker is disabled, so every report was lost.

use super::*;

/// The whole point of `REPO_OWNER`/`REPO_NAME` is that no link hardcodes a
/// second copy of the repository. If someone reintroduces a literal URL, one of
/// these stops agreeing with `repo_url()`.
#[test]
fn every_project_link_is_derived_from_the_repo_constants() {
    assert_eq!(
        repo_url(),
        format!("https://github.com/{REPO_OWNER}/{REPO_NAME}")
    );

    for url in [
        github_issues_url(),
        user_docs_url(),
        manual_url(MANUAL_TROUBLESHOOTING),
        feedback_form_url(),
    ] {
        assert!(
            url.starts_with(&repo_url()),
            "{url} does not hang off {}",
            repo_url()
        );
    }
}

#[test]
fn feedback_form_opens_this_repos_issue_chooser() {
    let url = feedback_form_url();
    assert!(
        url.starts_with(&format!(
            "https://github.com/{REPO_OWNER}/{REPO_NAME}/issues/new/choose"
        )),
        "feedback form does not open this repo's issue chooser: {url}"
    );
}

/// The ancestor's tracker is disabled, so a link to it loses the report
/// silently. Nothing user-facing in this module may point there.
#[test]
fn no_link_points_at_the_dead_ancestor() {
    for url in [
        repo_url(),
        github_issues_url(),
        user_docs_url(),
        manual_url(MANUAL_SHELL_INTEGRATION),
        feedback_form_url(),
    ] {
        assert!(
            !url.contains("zerx-lab"),
            "still points at the ancestor: {url}"
        );
        assert!(
            !url.contains("warp.dev"),
            "still points at Warp's site: {url}"
        );
    }
}

/// GitHub pre-fills an issue-form field by matching a query parameter against
/// the field's `id:`. Renaming either side alone silently breaks the pre-fill,
/// so the templates are checked against the constant the URL is built from.
#[test]
fn version_query_parameter_matches_the_issue_form_field_id() {
    const TEMPLATES: &[(&str, &str)] = &[
        (
            "01_bug_report.yml",
            include_str!("../../../.github/ISSUE_TEMPLATE/01_bug_report.yml"),
        ),
        (
            "03_ssh_tmux.yml",
            include_str!("../../../.github/ISSUE_TEMPLATE/03_ssh_tmux.yml"),
        ),
        (
            "04_ssh_legacy.yml",
            include_str!("../../../.github/ISSUE_TEMPLATE/04_ssh_legacy.yml"),
        ),
    ];

    for (name, contents) in TEMPLATES {
        assert!(
            contents.contains(&format!("id: \"{APP_VERSION_QUERY_PARAM}\"")),
            "{name} has no field with id \"{APP_VERSION_QUERY_PARAM}\", so the version pre-fill is dead"
        );
        assert!(
            !contents.contains("zap-version"),
            "{name} still carries the pre-rename field id"
        );
    }

    let url = url::Url::parse(&feedback_form_url()).expect("feedback URL should parse");
    let params: Vec<_> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
    assert!(
        params.contains(&"os-version".to_owned()),
        "os-version pre-fill was dropped: {params:?}"
    );
}

/// `include_str!` fails to compile if a manual section a UI link points at is
/// renamed or deleted — the failure a URL string on its own cannot catch.
macro_rules! assert_manual_section_exists {
    ($constant:expr, $file:literal) => {{
        assert_eq!(
            $constant, $file,
            "manual section constant disagrees with the file checked here"
        );
        assert!(
            !include_str!(concat!("../../../docs/manual/", $file)).is_empty(),
            concat!($file, " is empty")
        );
        assert!(manual_url($constant).ends_with(concat!("/docs/manual/", $file)));
    }};
}

#[test]
fn manual_sections_referenced_by_ui_links_exist() {
    assert_manual_section_exists!(MANUAL_MCP, "06-mcp-skills-and-rules.md");
    assert_manual_section_exists!(
        MANUAL_SHELL_INTEGRATION,
        "08-shell-integration-and-appearance.md"
    );
    assert_manual_section_exists!(
        MANUAL_TROUBLESHOOTING,
        "09-troubleshooting-and-limitations.md"
    );
    assert!(user_docs_url().ends_with("/docs/manual"));
}
