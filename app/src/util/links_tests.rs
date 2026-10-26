//! Regression tests for issue #632 — "Send feedback" opened `zerx-lab/warp`,
//! the dead ancestor whose issue tracker is disabled, so every report was lost.

use super::*;

/// Every link function in this module must hang off `repo_url()`.
///
/// Scope, stated precisely because the first version of this comment claimed
/// more than it delivers: this only sees functions defined HERE. A literal URL
/// pasted into some other file is invisible to it — which is exactly how the
/// five #632 call sites got there in the first place. `script/check_dead_links`
/// is the repo-wide half of the pair; neither one alone is sufficient.
#[test]
fn every_project_link_is_derived_from_the_repo_constants() {
    assert_eq!(
        repo_url(),
        format!("https://gitlab.com/{REPO_OWNER}/{REPO_NAME}")
    );

    for url in [
        issues_url(),
        user_docs_url(),
        manual_url(MANUAL_TROUBLESHOOTING),
        contributing_url(),
        feedback_form_url(),
        new_issue_url(),
        issue_form_url("01_bug_report.yml"),
    ] {
        assert!(
            url.starts_with(&repo_url()),
            "{url} does not hang off {}",
            repo_url()
        );
    }
}

#[test]
fn feedback_form_opens_this_repos_new_issue_page() {
    let url = feedback_form_url();
    assert!(
        url.starts_with(&format!(
            "https://gitlab.com/{REPO_OWNER}/{REPO_NAME}/-/issues/new"
        )),
        "feedback form does not open this repo's new-issue page: {url}"
    );
}

/// The ancestor's tracker is disabled, so a link to it loses the report
/// silently. Nothing user-facing in this module may point there — and
/// `script/check_dead_links` enforces the same rule across the whole tree,
/// which this test cannot.
#[test]
fn no_link_points_at_the_dead_ancestor() {
    for url in [
        repo_url(),
        issues_url(),
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

/// `issue_form_url` names a template file under `.gitlab/issue_templates/`; GitLab silently shows
/// an empty form for a name it cannot find. `include_str!` fails to compile if one is renamed or
/// deleted, and the stem passed to the URL is checked against the file it must resolve to.
#[test]
fn issue_templates_referenced_by_urls_exist() {
    const TEMPLATES: &[(&str, &str)] = &[
        (
            "01_bug_report",
            include_str!("../../../.gitlab/issue_templates/01_bug_report.md"),
        ),
        (
            "03_ssh_tmux",
            include_str!("../../../.gitlab/issue_templates/03_ssh_tmux.md"),
        ),
        (
            "04_ssh_legacy",
            include_str!("../../../.gitlab/issue_templates/04_ssh_legacy.md"),
        ),
    ];

    for (stem, contents) in TEMPLATES {
        assert!(!contents.is_empty(), "{stem} template is empty");
        for form in [
            format!("{stem}.yml"),
            format!("{stem}.md"),
            stem.to_string(),
        ] {
            assert!(
                issue_form_url(&form).ends_with(&format!("?issuable_template={stem}")),
                "{form} does not resolve to the {stem} template"
            );
        }
    }
}

/// The feedback form pre-fills the OS (and version, when known) through
/// `issue[description]`, the only pre-fill GitLab supports.
#[test]
fn feedback_form_prefills_the_description() {
    let url = url::Url::parse(&feedback_form_url()).expect("feedback URL should parse");
    let description = url
        .query_pairs()
        .find(|(key, _)| key == "issue[description]")
        .map(|(_, value)| value.into_owned())
        .expect("issue[description] pre-fill was dropped");
    assert!(
        description.contains("OS version:"),
        "OS pre-fill missing from: {description:?}"
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

/// #681: data-sourced links may only reach a browser or mail client.
#[test]
fn web_or_mail_links_only() {
    for url in [
        "https://github.com/jwp2987/phosphor/pull/1",
        "http://localhost:8080/callback",
        "mailto:someone@example.com",
        "HTTPS://example.com",
    ] {
        assert!(super::is_web_or_mail_link(url), "{url}");
    }
    for url in [
        "file:///tmp/Evil.app",
        "/tmp/Evil.app",
        "C:\\x\\setup.exe",
        "\\\\host\\share\\x.exe",
        "phosphor://launch/x",
        "vscode://file/tmp/x",
        "javascript:alert(1)",
        "",
    ] {
        assert!(!super::is_web_or_mail_link(url), "{url}");
    }
}
