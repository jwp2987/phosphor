//! The local user's display name and avatar (#1806).
//!
//! There is no account: the name comes from the OS user (`$USER`, `$USERNAME`, `$LOGNAME`), the
//! avatar defaults to a fun emoji picked from that name, and both can be changed in settings
//! (`appearance.profile.display_name` / `appearance.profile.avatar`). The avatar setting is an
//! emoji (or any short text), or a path to a local image file; nothing is ever fetched from the
//! network. All resolution here is pure so it can be unit tested without an app.

use std::path::{Path, PathBuf};

use settings::Setting as _;
use warpui::{AppContext, SingletonEntity};

use crate::settings::InputSettings;

/// Shown when neither a configured name nor an OS user name exists.
pub const FALLBACK_DISPLAY_NAME: &str = "You";

/// The default avatars: a few phosphor-flavoured glyphs and friendly creatures. The pick is a
/// stable function of the display name, so a user keeps theirs until they change it.
pub const DEFAULT_AVATARS: [&str; 8] = [
    "\u{1F7E2}", // green circle (a phosphor dot)
    "\u{2728}",  // sparkles
    "\u{1F52E}", // crystal ball
    "\u{1F98E}", // lizard
    "\u{1F438}", // frog
    "\u{1F33F}", // herb
    "\u{1FA90}", // ringed planet
    "\u{1F980}", // crab
];

/// Longest emoji/text avatar kept, in characters (a ZWJ emoji sequence needs a handful).
const MAX_AVATAR_CHARS: usize = 8;

const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalAvatar {
    /// An emoji (or short text) drawn on the avatar disc.
    Emoji(String),
    /// A local image file.
    Image(PathBuf),
}

/// The OS user name from the given environment values, first non-blank wins.
pub fn os_user_name(
    user: Option<&str>,
    username: Option<&str>,
    logname: Option<&str>,
) -> Option<String> {
    [user, username, logname]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|name| !name.is_empty())
        .map(str::to_owned)
}

/// The OS user name of this process.
pub fn current_os_user_name() -> Option<String> {
    os_user_name(
        std::env::var("USER").ok().as_deref(),
        std::env::var("USERNAME").ok().as_deref(),
        std::env::var("LOGNAME").ok().as_deref(),
    )
}

/// The display name: the configured one if non-blank, else the OS user, else a neutral word.
pub fn resolve_display_name(configured: &str, os_user: Option<&str>) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        return configured.to_owned();
    }
    os_user
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(FALLBACK_DISPLAY_NAME)
        .to_owned()
}

/// The default avatar for a display name: stable across runs and builds (FNV-1a, not `Hash`).
pub fn default_avatar_for(display_name: &str) -> &'static str {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in display_name.trim().to_lowercase().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    DEFAULT_AVATARS[hash as usize % DEFAULT_AVATARS.len()]
}

fn looks_like_path(value: &str) -> bool {
    if value.starts_with('/') || value.starts_with('~') || value.starts_with("./") {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') {
        return true;
    }
    Path::new(value)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Resolves the avatar setting. Empty means the default for `display_name`; a path to an
/// existing file is an image; a path to a missing file falls back to the default (never an
/// error, never a network fetch); anything else is an emoji, cut to a few characters.
pub fn resolve_avatar(
    configured: &str,
    display_name: &str,
    home: Option<&Path>,
    file_exists: impl Fn(&Path) -> bool,
) -> LocalAvatar {
    let configured = configured.trim();
    let default = || LocalAvatar::Emoji(default_avatar_for(display_name).to_owned());
    if configured.is_empty() {
        return default();
    }
    if looks_like_path(configured) {
        let path = match (configured.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(configured),
        };
        return if file_exists(&path) {
            LocalAvatar::Image(path)
        } else {
            default()
        };
    }
    LocalAvatar::Emoji(configured.chars().take(MAX_AVATAR_CHARS).collect())
}

/// The local user's display name, from settings and the OS.
pub fn current_display_name(app: &AppContext) -> String {
    let configured = InputSettings::as_ref(app)
        .profile_display_name
        .value()
        .clone();
    resolve_display_name(&configured, current_os_user_name().as_deref())
}

/// The local user's avatar, from settings.
pub fn current_avatar(app: &AppContext) -> LocalAvatar {
    let configured = InputSettings::as_ref(app).profile_avatar.value().clone();
    resolve_avatar(
        &configured,
        &current_display_name(app),
        dirs::home_dir().as_deref(),
        |path| path.is_file(),
    )
}

#[cfg(test)]
#[path = "local_identity_tests.rs"]
mod tests;
