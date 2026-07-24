//! App-side support for the headless `warp_tui` front-end (Zap / BYOP build).
//!
//! Upstream Warp's `tui` module drives a device-authorization login against the
//! Warp cloud before the TUI becomes usable. Zap is BYOP: there is no account to
//! log into — the user configures a provider (API key / endpoint) directly — so
//! the login model here is a trivial always-`LoggedIn` stand-in. The
//! [`TuiLoginModel`]/[`TuiLoginPhase`]/[`TuiLoginEvent`] shapes are kept intact
//! so `warp_tui` (which renders a login placeholder for the non-`LoggedIn`
//! phases) compiles and behaves correctly — those phases simply never occur.

mod mcp;

pub use mcp::{
    TuiMcpAction, TuiMcpConfigState, TuiMcpManager, TuiMcpManagerEvent, TuiMcpServerId,
    TuiMcpServerSnapshot, TuiMcpServerStatus, TuiMcpSnapshot, TuiMcpTransport,
};
use warpui::{AppContext, Entity, SingletonEntity};

use crate::TuiMountFn;

/// Login state of the headless TUI, observed by the `warp_tui` root view to
/// decide whether to show the login placeholder or the input UI.
///
/// In the BYOP build only [`TuiLoginPhase::LoggedIn`] ever occurs; the other
/// variants are retained for source compatibility with the `warp_tui`
/// front-end (which was ported from upstream Warp and renders them).
pub enum TuiLoginPhase {
    /// Waiting for the user to finish a device-authorization login. Never
    /// reached in the BYOP build (no cloud login).
    AwaitingLogin {
        verification_uri: Option<String>,
        user_code: Option<String>,
    },
    /// Login failed; the placeholder shows the message. Never reached in BYOP.
    Failed { message: String },
    /// Ready — the input UI can be shown. The only phase used by BYOP.
    LoggedIn,
}

/// Events emitted by [`TuiLoginModel`].
pub enum TuiLoginEvent {
    /// Authentication completed and the TUI can create its terminal session.
    LoggedIn,
    /// The current user logged out and the TUI should return to authentication.
    /// Not emitted in the BYOP build.
    LoggedOut,
}

/// Singleton holding the TUI's [`TuiLoginPhase`], read by the `warp_tui` root
/// view. In BYOP it is constructed already `LoggedIn` and never changes.
pub struct TuiLoginModel {
    phase: TuiLoginPhase,
}

impl TuiLoginModel {
    /// The current login phase (always [`TuiLoginPhase::LoggedIn`] in BYOP).
    pub fn phase(&self) -> &TuiLoginPhase {
        &self.phase
    }
}

impl Entity for TuiLoginModel {
    type Event = TuiLoginEvent;
}

impl SingletonEntity for TuiLoginModel {}

/// Entry point invoked from `run_internal` for [`crate::LaunchMode::Tui`], after
/// `initialize_app`. Registers the always-`LoggedIn` [`TuiLoginModel`] singleton
/// (BYOP performs no authentication, so the TUI is ready immediately) and then
/// runs `mount`, which builds the root TUI view and starts the TUI driver.
#[cfg(feature = "tui")]
pub(crate) fn init(mount: TuiMountFn, ctx: &mut AppContext) {
    ctx.add_singleton_model(|_| TuiLoginModel {
        phase: TuiLoginPhase::LoggedIn,
    });
    ctx.add_singleton_model(TuiMcpManager::new);
    // Mount the TUI now that the login model exists; the root view goes straight
    // to the input UI since the phase is already `LoggedIn`.
    mount(ctx);
}

/// Logs out the current TUI user. BYOP has no account to log out of, so this is
/// a no-op kept for source compatibility with the `warp_tui` front-end.
pub fn log_out_tui(_ctx: &mut AppContext) {}
