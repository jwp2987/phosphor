//! Client-side readiness checks for using a harness in *local* orchestration
//! (i.e. spawning a local CLI process — `claude`, `codex`, `opencode` — as a
//! child agent instead of routing through Zap's own MAA infrastructure).
//!
//! Ported from the pin (`02b53fcd8:app/src/ai/local_harness_setup.rs`) as
//! part of #381. Of the pin's three call sites, only one exists on this
//! branch: `app/src/pane_group/pane/local_harness_launch.rs`'s `Harness::Codex`
//! arm, wired in #323 via [`local_harness_setup_state`]. The other two are
//! permanently unavailable, not merely unbuilt — traced 2026-08-10 while
//! scoping the pin's orchestration config-picker cluster
//! (`OrchestrationConfigState`/`AuthSecretSelection`):
//! `app/src/ai/orchestration/{config_state,edit_state,providers,remote_child,
//! snapshots,validation}.rs` is the confirmation-card/plan-card picker for
//! *agent-invoked* `run_agents` (model-proposed spawns), which `DECLINED.md`'s
//! #325 row already declined outright — `RunAgentsExecutionMode`/
//! `OrchestrationConfig`, the very types `OrchestrationConfigState` is built
//! from, do not exist anywhere in this fork's `crates/ai`. Even setting that
//! aside, the layer's harness/model/auth-secret catalogs resolve through
//! `HarnessAvailabilityModel`, itself backed by `warp_managed_secrets::
//! ManagedSecretManager` fetched via `crate::server::server_api::
//! ServerApiProvider`; this fork wires that manager to
//! `local_managed_secrets::DisabledManagedSecretsClient`, whose own doc
//! comment says "every cloud managed-secret action is unreachable". And
//! `remote_child.rs` spawns the child on Warp's server outright
//! (`server_api::ai::SpawnAgentRequest`, credits, GitHub-auth remediation).
//! So this call site cannot become a non-cloud caller for
//! [`LocalHarnessSetupState::is_selectable`]/[`local_harness_is_product_enabled`]
//! by porting it — a previous version of this comment said "#310/#304 is
//! their real caller, not #323", but #310/#304 (topology + pill bar) are
//! done and correctly did not build this cloud layer. The third call site,
//! `app/src/ai/blocklist/action_model/execute/run_agents.rs`, doesn't exist
//! on this branch either (same declined family). Both functions stay
//! `#[allow(dead_code)]` until a genuinely local harness-picker UI is
//! designed for `/orchestrate` from scratch — that command currently
//! hardcodes `ORCHESTRATE_DEFAULT_HARNESS` and has no harness-selection
//! surface at all (see `pane_group/pane/mod.rs`'s `prepare_tui_child_agent_launch`
//! doc comment) — which is new feature work, not a port.

use warp_cli::agent::Harness;

use crate::features::FeatureFlag;
#[cfg(not(target_family = "wasm"))]
use crate::util::path::resolve_executable;

/// Tooltip shown when a local harness is product-enabled but its CLI is missing.
pub(crate) const LOCAL_HARNESS_INSTALLATION_REQUIRED_TOOLTIP: &str =
    "Install Claude Code to use this local harness.";
pub(crate) const LOCAL_CODEX_HARNESS_INSTALLATION_REQUIRED_TOOLTIP: &str =
    "Install Codex to use this local harness.";
pub(crate) const LOCAL_CODEX_HARNESS_DISABLED_MESSAGE: &str =
    "Local Codex child agents are temporarily disabled.";

/// Client-side readiness for using a harness in local orchestration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalHarnessSetupState {
    /// The harness is product-enabled and its required local CLI is installed.
    Ready,
    /// The harness is intentionally unavailable in the product.
    ProductDisabled { message: &'static str },
    /// The harness is product-enabled but the required local CLI is missing.
    MissingHarness { tooltip: &'static str },
}

impl LocalHarnessSetupState {
    /// Returns whether the harness can be selected in local orchestration controls.
    ///
    /// No caller on this branch: this is a harness-*picker* helper (grey out
    /// an unselectable option), and the only wired-up caller of this module,
    /// `local_harness_launch.rs`'s launch-time gate, needs the actual message
    /// or tooltip text rather than a bare bool, so it pattern-matches
    /// `LocalHarnessSetupState` directly instead. The pin's real caller
    /// (`app/src/ai/orchestration/validation.rs`'s `harness_is_selectable`) is
    /// cloud, not merely unbuilt -- see the module doc comment above.
    #[allow(dead_code)]
    pub(crate) fn is_selectable(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Returns the product-level disabled reason for a local harness.
pub(crate) fn local_harness_product_disabled_message(harness: Harness) -> Option<&'static str> {
    match harness {
        Harness::Codex if !local_codex_harness_is_enabled() => {
            Some(LOCAL_CODEX_HARNESS_DISABLED_MESSAGE)
        }
        Harness::Oz | Harness::Claude | Harness::OpenCode | Harness::Gemini | Harness::Unknown => {
            None
        }
        Harness::Codex => None,
    }
}

fn local_codex_harness_is_enabled() -> bool {
    FeatureFlag::LocalClaudeCodexChildHarnesses.is_enabled()
}

/// Returns whether a local harness is exposed by product policy.
///
/// No caller on this branch, for the same reason as
/// [`LocalHarnessSetupState::is_selectable`]: it's the coarse bool a picker
/// UI would use to filter its harness list, and #323's launch-time gate needs
/// the message text, not just the bool, so it goes through
/// [`local_harness_setup_state`] directly instead. The pin's real caller
/// (`app/src/ai/orchestration/validation.rs`'s `harness_is_selectable`) is
/// cloud, not merely unbuilt -- see the module doc comment above.
#[allow(dead_code)]
pub(crate) fn local_harness_is_product_enabled(harness: Harness) -> bool {
    local_harness_product_disabled_message(harness).is_none()
}

/// Returns the current local setup state for a harness.
pub(crate) fn local_harness_setup_state(harness: Harness) -> LocalHarnessSetupState {
    local_harness_setup_state_with_cli_resolver(harness, local_cli_is_installed)
}

fn local_harness_setup_state_with_cli_resolver(
    harness: Harness,
    cli_is_installed: impl Fn(&str) -> bool,
) -> LocalHarnessSetupState {
    if let Some(message) = local_harness_product_disabled_message(harness) {
        return LocalHarnessSetupState::ProductDisabled { message };
    }

    match harness {
        Harness::Claude if !cli_is_installed("claude") => LocalHarnessSetupState::MissingHarness {
            tooltip: LOCAL_HARNESS_INSTALLATION_REQUIRED_TOOLTIP,
        },
        Harness::Codex if !cli_is_installed("codex") => LocalHarnessSetupState::MissingHarness {
            tooltip: LOCAL_CODEX_HARNESS_INSTALLATION_REQUIRED_TOOLTIP,
        },
        Harness::Oz
        | Harness::Claude
        | Harness::OpenCode
        | Harness::Gemini
        | Harness::Codex
        | Harness::Unknown => LocalHarnessSetupState::Ready,
    }
}

fn local_cli_is_installed(command: &str) -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        resolve_executable(command).is_some()
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = command;
        false
    }
}

#[cfg(test)]
#[path = "local_harness_setup_tests.rs"]
mod tests;
