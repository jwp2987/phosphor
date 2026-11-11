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

/// Whether a typed `ssh` can be taken by the tmux ssh wrapper (the "warpify this host?" prompt
/// and the tmux install). It cannot: that option was removed on 2026-11-08 (`DECLINED.md`, "SSH
/// tmux wrapper option -- REMOVED"), and [`evaluate_warpify_ssh_host`] below answers
/// `FeatureDisabled` for every host. The wrapper that runs for a typed `ssh` is therefore always
/// the legacy (ControlMaster) one, whatever `FeatureFlag::SSHTmuxWrapper` says: that flag stays
/// on in every default build because it also gates tmux control-mode handling for people who
/// run tmux themselves.
///
/// The ssh integration scenarios are gated on this, not on the flag (#2128): gated on the flag
/// being off, the scenarios of the wrapper this fork ships never ran. If the tmux option ever
/// comes back, this and the function below change together.
pub const TMUX_SSH_WRAPPER_TAKES_SSH: bool = false;

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
