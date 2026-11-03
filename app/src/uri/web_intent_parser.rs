use crate::ChannelState;
use anyhow::{anyhow, Result};
use url::Url;

#[derive(Debug)]
/// Represents an intent parsed from a web url
pub enum WebIntent {
    ConversationView(Url),
    DriveObject(Url),
    SettingsView(Url),
    Home(Url),
    Action(Url),
}

impl WebIntent {
    pub fn try_from_url(url: &Url) -> Result<Self> {
        let url_scheme = ChannelState::url_scheme();
        let segments = if url.scheme() == url_scheme {
            let mut segments = Vec::new();
            if let Some(host) = url.host_str() {
                segments.push(host);
            }
            if let Some(path_segments) = url.path_segments() {
                segments.extend(path_segments);
            }
            segments
        } else {
            {
                return Err(anyhow!("Attempting to parse invalid url: {}", url));
            }

        };

        if segments.is_empty() {
            return Err(anyhow!("Attempting to parse invalid url: {}", url));
        }

        match segments[0] {
            "app" | "home" => Ok(WebIntent::Home(Url::parse(&format!(
                "{url_scheme}://home"
            ))?)),
            "conversation" => {
                if segments.len() != 2 {
                    return Err(anyhow!("Attempting to parse invalid url: {}", url));
                }

                let conversation_id = segments[1];
                let conversation_intent =
                    Url::parse(format!("{url_scheme}://conversation/{conversation_id}").as_str())
                        .map_err(|_| anyhow!("Attempting to parse invalid url: {}", url))?;

                Ok(WebIntent::ConversationView(conversation_intent))
            }
            "drive" => {
                if segments.len() != 3 {
                    return Err(anyhow!("Attempting to parse invalid url: {}", url));
                }
                let id_and_name: Vec<&str> = segments[segments.len() - 1].split('-').collect();
                let id = id_and_name[id_and_name.len() - 1];
                let object_type = segments[segments.len() - 2];
                let mut drive_intent =
                    Url::parse(format!("{url_scheme}://drive/{object_type}").as_str())
                        .map_err(|_| anyhow!("Attempting to parse invalid url: {}", url))?;
                drive_intent.set_query(url.query());
                drive_intent.query_pairs_mut().append_pair("id", id);
                Ok(WebIntent::DriveObject(drive_intent))
            }
            "settings" => {
                if segments.len() != 2 {
                    return Err(anyhow!("Attempting to parse invalid url: {}", url));
                }
                let sub_section = segments[segments.len() - 1];
                let query_str = url.query().unwrap_or_default();
                if query_str.is_empty() {
                    return Err(anyhow!("Attempting to parse invalid url: {}", url));
                }
                let settings_intent = Url::parse(
                    format!("{url_scheme}://settings/{sub_section}?{query_str}").as_str(),
                )
                .map_err(|_| anyhow!("Attempting to parse invalid url: {}", url))?;
                Ok(WebIntent::SettingsView(settings_intent))
            }
            "action" => {
                if segments.len() != 2 {
                    return Err(anyhow!("Attempting to parse invalid url: {}", url));
                }
                let action_type = segments[1];
                const ALLOWED_ACTIONS: &[&str] = &["open-repo", "focus_ambient_agent"];
                if !ALLOWED_ACTIONS.contains(&action_type) {
                    return Err(anyhow!("Unknown action type in url: {}", action_type));
                }
                let action_intent =
                    Url::parse(format!("{url_scheme}://action/{action_type}").as_str())
                        .map_err(|_| anyhow!("Attempting to parse invalid url: {}", url))?;
                Ok(WebIntent::Action(action_intent))
            }
            _ => Err(anyhow!("Attempting to parse invalid url: {}", url)),
        }
    }

    /// Convert this web intent into the underlying native desktop URL.
    pub fn into_intent_url(self) -> Url {
        match self {
            WebIntent::ConversationView(url) => url,
            WebIntent::DriveObject(url) => url,
            WebIntent::SettingsView(url) => url,
            WebIntent::Home(url) => url,
            WebIntent::Action(url) => url,
        }
    }
}

/// Attempts to rewrite a Zap web URL into a native desktop intent URL (warp://...).
/// Returns `None` if the URL is not a recognized Zap web intent.
pub fn maybe_rewrite_web_url_to_intent(url: &Url) -> Option<Url> {
    WebIntent::try_from_url(url)
        .ok()
        .map(WebIntent::into_intent_url)
}

// Looks at the current url and converts it into an app intent.
// NOTE: This is only intended for use with target_family = "wasm"

