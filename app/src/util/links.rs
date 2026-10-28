use crate::channel::ChannelState;

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;

/// The GitLab project this fork is published from. Every user-visible link
/// that points at the project — the issue tracker, the feedback form, the user
/// manual — is derived from these two constants, so they cannot drift apart.
/// So does the release feed in `autoupdate::gitlab` (dormant: the `autoupdate`
/// feature is off in shipped builds, see `script/check_no_autoupdate`).
pub const REPO_OWNER: &str = "winters-phosphor";
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

/// `https://gitlab.com/<owner>/<repo>` — the root every other project link
/// hangs off. GitLab puts everything except the repo root under `/-/`.
pub fn repo_url() -> String {
    format!("https://gitlab.com/{REPO_OWNER}/{REPO_NAME}")
}

/// The issue tracker users can actually file against.
pub fn issues_url() -> String {
    format!("{}/-/issues", repo_url())
}

/// The user manual's directory listing, used as this fork's "Documentation".
pub fn user_docs_url() -> String {
    format!("{}/-/tree/{DOCS_BRANCH}/docs/manual", repo_url())
}

/// A single section of the user manual, rendered on GitLab. `section` is one of
/// the `MANUAL_*` file names above.
pub fn manual_url(section: &str) -> String {
    format!("{}/-/blob/{DOCS_BRANCH}/docs/manual/{section}", repo_url())
}

/// This fork's contribution guide (`CONTRIBUTING.md`, at the repo root), rendered on GitLab.
pub fn contributing_url() -> String {
    format!("{}/-/blob/{DOCS_BRANCH}/CONTRIBUTING.md", repo_url())
}

/// A specific issue template, by its `.gitlab/issue_templates/` file name (a trailing `.yml`
/// or `.md` is ignored). The template applies its own labels, so nothing else needs pinning
/// here. The query string is always present, so callers may append `&key=value` pairs.
pub fn issue_form_url(template: &str) -> String {
    let name = template
        .strip_suffix(".yml")
        .or_else(|| template.strip_suffix(".md"))
        .unwrap_or(template);
    format!("{}/-/issues/new?issuable_template={name}", repo_url())
}

/// The new-issue page, for "file an issue" controls that should not presume
/// which template the user wants. GitLab has no separate chooser: the form
/// offers the templates in a dropdown.
pub fn new_issue_url() -> String {
    format!("{}/-/issues/new", repo_url())
}

/// The latest release, for "download it yourself" paths — the About page's
/// manual-download link and the autoupdate-failure banner.
pub fn latest_release_url() -> String {
    format!("{}/-/releases/permalink/latest", repo_url())
}

/// The "Send feedback" destination: this repo's new-issue page, with the version
/// and OS pre-filled into the description.
///
/// GitLab has no per-field pre-fill (GitHub matched query parameters against issue-form
/// field ids), only `issue[description]`, so the details go in as text lines.
pub fn feedback_form_url() -> String {
    let mut description = String::new();
    if let Some(version) = ChannelState::app_version() {
        description.push_str(&format!("Phosphor version: {version}\n"));
    }
    description.push_str(&format!("OS version: {}\n", os_info::get().version()));
    let mut url = url::Url::parse(&new_issue_url()).expect("Should not fail to parse");
    url.query_pairs_mut()
        .append_pair("issue[description]", &description);
    url.to_string()
}

/// Whether `url` is a web or mail link: `http`, `https` or `mailto`, and nothing else.
///
/// For `open_url` call sites whose URL comes from data rather than from a constant -- a PR URL
/// from `gh`, an MCP server's `authorization_url`, a toast link, a hyperlink in a prompt alert.
/// Those links are only ever meant to reach a browser or mail client; a `file:` URL, a bare
/// path, or another app's scheme reaching them would be handed to whatever handler claims it
/// (#681), so they are refused rather than passed through.
pub fn is_web_or_mail_link(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https" | "mailto"))
}

/// Open `url` only if it [`is_web_or_mail_link`]; log and do nothing otherwise.
pub fn open_web_or_mail_link(url: &str, ctx: &warpui::AppContext) {
    if is_web_or_mail_link(url) {
        ctx.open_url(url);
    } else {
        log::warn!("Refusing to open a non-web link: {url:?}");
    }
}
