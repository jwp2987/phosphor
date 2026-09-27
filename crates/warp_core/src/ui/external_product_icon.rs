use crate::ui::theme::Fill;
use warpui::elements::Icon as WarpUiIcon;

#[derive(Clone, Copy)]
pub enum ExternalProductIcon {
    Heroku,
    Notion,
    Linear,
    Figma,
    Github,
    Slack,
}

impl ExternalProductIcon {
    // `25f079350`: a title resolves to a product's icon when it starts with that
    // product's name, case-insensitively -- not only on an exact match. A
    // decorated title like "Sentry (OAuth)" (a locally-named or team-shared MCP
    // server) previously fell through to `None` and rendered initials instead of
    // the logo. Table-driven per upstream: adding a product is a one-line
    // addition here. Scoped to the `starts_with` change only -- the fork lacks
    // Composio/Resend/Sentry/YouDotCom's icon assets (out-of-range commits), so
    // those four are not added.
    const PREFIXES: &'static [(&'static str, Self)] = &[
        ("heroku", Self::Heroku),
        ("notion", Self::Notion),
        ("linear", Self::Linear),
        ("figma", Self::Figma),
        ("github", Self::Github),
        ("slack", Self::Slack),
    ];

    pub fn from_string(s: &str) -> Option<Self> {
        let s_lower = s.to_ascii_lowercase();
        Self::PREFIXES
            .iter()
            .find(|(prefix, _)| s_lower.starts_with(prefix))
            .map(|(_, icon)| *icon)
    }

    pub fn get_path(&self) -> &'static str {
        match self {
            Self::Heroku => "bundled/svg/heroku.svg",
            Self::Notion => "bundled/svg/notion.svg",
            Self::Linear => "bundled/svg/linear.svg",
            Self::Figma => "bundled/svg/figma.svg",
            Self::Github => "bundled/svg/github.svg",
            Self::Slack => "bundled/svg/slack-logo.svg",
        }
    }

    pub fn to_warpui_icon(&self, color: Fill) -> WarpUiIcon {
        let path = self.get_path();
        WarpUiIcon::new(path, color.into_solid())
    }
}

#[cfg(test)]
#[path = "external_product_icon_tests.rs"]
mod tests;
