use serde::{Deserialize, Serialize};
use warp_util::path::ShellFamily;

use crate::terminal::warpify::settings::WarpifySettings;

/// The different possible outcomes of detecting an interactive SSH session.
/// Also the payload for the [`crate::server::telemetry::TelemetryEvent::SshInteractiveSessionDetected`] event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SshInteractiveSessionDetected {
    #[serde(rename = "feature_disabled")]
    FeatureDisabled,
    #[serde(rename = "host_denylisted")]
    HostDenylisted,
    #[serde(rename = "warpify_prompt")]
    ShouldPromptWarpification {
        #[serde(skip)]
        command: String,
        #[serde(skip)]
        host: Option<String>,
    },
}

/// Determines whether a host could be warpified.
///
/// The tmux-wrapper prompt was the only thing that ever produced
/// `ShouldPromptWarpification` here; that option was removed (the no-helper ssh path is
/// primary), so a plain ssh session is never offered tmux warpification. Control-mode
/// support for users who run tmux themselves does not go through this function.
pub fn evaluate_warpify_ssh_host(
    _command: &str,
    _ssh_host: Option<&str>,
    _shell_family: ShellFamily,
    _warpify_settings: &WarpifySettings,
) -> SshInteractiveSessionDetected {
    SshInteractiveSessionDetected::FeatureDisabled
}
