//! Environment variables that let a process running inside a terminal session
//! deep-link back to its own pane — e.g. so a long-running command can focus
//! its terminal when it finishes.
//!
//! Ported from Warp. The app already handles the consumer side of the
//! `<scheme>://session/<uuid-hex>` deeplink (see [`crate::uri`]'s
//! `UriHost::Session` → `PaneGroup::find_terminal_pane_by_session_uuid`); this
//! is the producer half that publishes that URL, plus the raw session UUID,
//! into the spawned shell's environment. The env-var names keep the `WARP_`
//! prefix the fork uses for its other shell-integration variables, while the
//! URL itself uses the fork's own [`ChannelState::url_scheme`].

use std::collections::HashMap;
use std::ffi::OsString;

use crate::ChannelState;

/// The deep-link URL a session's processes use to focus their own pane.
pub(crate) const FOCUS_URL_ENV: &str = "WARP_FOCUS_URL";
/// The hex-encoded UUID identifying the session (companion to [`FOCUS_URL_ENV`]).
pub(crate) const TERMINAL_SESSION_UUID_ENV: &str = "WARP_TERMINAL_SESSION_UUID";

/// The `<scheme>://session/<uuid-hex>` deeplink that focuses the pane owning
/// `session_uuid_hex`. Mirrors the URL `crate::uri` parses on the consumer side.
pub(crate) fn session_focus_url(session_uuid_hex: &str) -> String {
    format!(
        "{}://session/{session_uuid_hex}",
        ChannelState::url_scheme()
    )
}

/// Inserts [`TERMINAL_SESSION_UUID_ENV`] and [`FOCUS_URL_ENV`] into a session's
/// spawn environment. Called once per session at creation, with the same UUID
/// the pane is constructed with, so the published deeplink resolves back to it.
pub(crate) fn add_session_focus_env_vars(
    env_vars: &mut HashMap<OsString, OsString>,
    session_uuid: &[u8],
) {
    let session_uuid_hex = hex::encode(session_uuid);
    env_vars.insert(
        OsString::from(TERMINAL_SESSION_UUID_ENV),
        OsString::from(session_uuid_hex.clone()),
    );
    env_vars.insert(
        OsString::from(FOCUS_URL_ENV),
        OsString::from(session_focus_url(&session_uuid_hex)),
    );
}

#[cfg(test)]
#[path = "focus_env_tests.rs"]
mod tests;
