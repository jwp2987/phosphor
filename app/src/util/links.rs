use crate::channel::ChannelState;

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;

/// The GitHub repository this fork is published from. Every user-visible link
/// that points at the project — the issue tracker, the feedback form, the user
/// manual — is derived from these two constants, so they cannot drift apart.
/// `autoupdate::github` builds its Releases-API URL from the same pair.
pub const REPO_OWNER: &str = "jwp2987";
pub const REPO_NAME: &str = "phosphor";

/// Branch the in-repo documentation links resolve against.
const DOCS_BRANCH: &str = "main";

/// Sections of the in-repo user manual (`docs/manual/`) that UI links point at.
/// This fork has no documentation site; the manual is the documentation.
pub const MANUAL_SHELL_INTEGRATION: &str = "08-shell-integration-and-appearance.md";
pub const MANUAL_TROUBLESHOOTING: &str = "09-troubleshooting-and-limitations.md";
pub const MANUAL_MCP: &str = "06-mcp-skills-and-rules.md";

// Upstream Warp's Slack workspace and privacy policy do not apply to this fork,
// and it has neither of its own yet. These stay empty on purpose, and every
// control that would open one is hidden while its URL is empty (see
// `app_menus::link_menu_item`, `ResourceCenterFooterItem::url`,
// `workspace::add_overflow_menu_items_as_editable_binding` and
// `settings_view::privacy_page`) rather than presenting a link that opens
// nothing. Filling either constant in is all that is needed to bring the
// controls back.
pub const SLACK_URL: &str = "";
pub const PRIVACY_POLICY_URL: &str = "";

/// `https://github.com/<owner>/<repo>` — the root every other project link
/// hangs off.
pub fn repo_url() -> String {
    format!("https://github.com/{REPO_OWNER}/{REPO_NAME}")
}

/// The issue tracker users can actually file against.
pub fn github_issues_url() -> String {
    format!("{}/issues", repo_url())
}

/// The user manual's directory listing, used as this fork's "Documentation".
pub fn user_docs_url() -> String {
    format!("{}/tree/{DOCS_BRANCH}/docs/manual", repo_url())
}

/// A single section of the user manual, rendered on GitHub. `section` is one of
/// the `MANUAL_*` file names above.
pub fn manual_url(section: &str) -> String {
    format!("{}/blob/{DOCS_BRANCH}/docs/manual/{section}", repo_url())
}

/// A specific issue form, by its `.github/ISSUE_TEMPLATE/` file name. The
/// template declares its own labels, so nothing else needs pinning here.
pub fn issue_form_url(template: &str) -> String {
    format!("{}/issues/new?template={template}", repo_url())
}

/// The issue-form chooser, for "file an issue" controls that should not presume
/// which template the user wants.
pub fn new_issue_url() -> String {
    format!("{}/issues/new/choose", repo_url())
}

/// The latest release, for "download it yourself" paths — the About page's
/// manual-download link and the autoupdate-failure banner. `autoupdate::github`
/// polls the API form of this same release off `REPO_OWNER`/`REPO_NAME`.
pub fn latest_release_url() -> String {
    format!("{}/releases/latest", repo_url())
}

/// Name of the version query parameter on the feedback form. Matches the `id:`
/// of the version field in `.github/ISSUE_TEMPLATE/*.yml`.
pub const APP_VERSION_QUERY_PARAM: &str = "phosphor-version";

/// The "Send feedback" destination: this repo's issue-form chooser, pre-filling
/// the version fields.
///
/// The query-parameter names are not free-form — GitHub matches them against the
/// `id:` of a field in `.github/ISSUE_TEMPLATE/*.yml` to pre-fill it, so
/// renaming one here without renaming the template field silently stops the
/// pre-fill.
pub fn feedback_form_url() -> String {
    let mut url = url::Url::parse(&new_issue_url()).expect("Should not fail to parse");
    if let Some(version) = ChannelState::app_version() {
        url.query_pairs_mut()
            .append_pair(APP_VERSION_QUERY_PARAM, version);
    }
    url.query_pairs_mut()
        .append_pair("os-version", &os_info::get().version().to_string());
    url.to_string()
}
