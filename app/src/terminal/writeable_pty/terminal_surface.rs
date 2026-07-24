//! The [`TerminalSurface`] trait and the [`PtyIntent`] vocabulary that let a
//! single `TerminalManager` drive different terminal frontends (GUI views and
//! the headless TUI) without knowing the concrete UI type.
//!
//! Ported from upstream Warp (`writeable_pty/terminal_surface.rs` +
//! `local_tty/terminal_manager.rs`) as the **first, additive step** of the
//! terminal-manager surface abstraction: these types are defined here but the
//! local `TerminalManager` is not yet generic over the surface — that
//! genericization is a follow-up. Nothing implements [`TerminalSurface`] yet, so
//! this step changes no runtime behavior.

use std::borrow::Cow;
use std::sync::Arc;

use async_broadcast::InactiveReceiver;
use async_channel::Sender;
use parking_lot::FairMutex;
#[cfg(unix)]
use warpui::AppContext;
use warpui::{Entity, ModelHandle, ViewContext, ViewHandle};

use crate::ai::agent::AIAgentPtyWriteMode;
#[cfg(unix)]
use crate::terminal::event::AfterBlockCompletedEvent;
use crate::terminal::model::completions::ShellCompletion;
use crate::terminal::model::session::Sessions;
use crate::terminal::model::terminal_model::TerminalModel;
#[cfg(unix)]
use crate::terminal::model::terminal_model::BlockIndex;
use crate::terminal::model_events::ModelEventDispatcher;
use crate::terminal::view::ExecuteCommandEvent;
use crate::terminal::{ShellLaunchData, SizeInfo, SizeUpdate};

/// A normalized request from a terminal UI surface to the PTY controller.
///
/// This is the narrow vocabulary that `TerminalManager` uses to drive the PTY
/// without knowing the concrete UI implementation. It only contains actions
/// meaningful to the PTY/session boundary: process control, byte writes,
/// resizing, command execution, and native shell completions.
pub enum PtyIntent {
    CtrlD,
    #[cfg(not(target_family = "wasm"))]
    Interrupt,
    ShutdownPty,
    WriteBytes(Cow<'static, [u8]>),
    WriteAgentInput {
        bytes: Cow<'static, [u8]>,
        mode: AIAgentPtyWriteMode,
    },
    Resize(SizeUpdate),
    ExecuteCommand(ExecuteCommandEvent),
    RunNativeShellCompletions {
        buffer_text: String,
        results_tx: Sender<Vec<ShellCompletion>>,
    },
}

/// Event types that can be projected into an [`Option<PtyIntent>`].
pub trait PtyIntentEvent {
    /// Projects this event into a PTY/session intent, or `None` if it is not a
    /// PTY-driving event.
    fn pty_intent(&self) -> Option<PtyIntent>;
}

/// A terminal frontend surface driven by `TerminalManager`.
///
/// Each surface defines how its own event type collapses into a PTY/session
/// intent. This is bounded by [`Entity`] instead of [`View`](warpui::View) so
/// the same manager can drive both GUI views and TUI views.
pub trait TerminalSurface: Entity + 'static
where
    <Self as Entity>::Event: PtyIntentEvent,
{
    /// Whether the local manager should start polling termios for a password
    /// prompt after the given block starts.
    #[cfg(unix)]
    fn should_start_password_prompt_polling(&self, _command: &str, _ctx: &AppContext) -> bool {
        false
    }

    /// Whether the local manager should stop password-prompt polling for this
    /// completed block.
    #[cfg(unix)]
    fn should_stop_password_prompt_polling(&self, _completed: &AfterBlockCompletedEvent) -> bool {
        false
    }

    /// Called once the shell starter has been determined and the PTY event loop
    /// has started, so the surface can react to shell launch metadata.
    #[cfg(feature = "local_tty")]
    fn on_shell_determined(&mut self, _ctx: &mut ViewContext<Self>) {}

    /// Called when the active shell launch data is updated (e.g. shell
    /// indicator metadata).
    fn on_active_shell_launch_data_updated(
        &mut self,
        _shell_launch_data: Option<ShellLaunchData>,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    /// Called when the PTY fails to spawn so the surface can surface the error.
    #[cfg(feature = "local_tty")]
    fn on_pty_spawn_failed(&mut self, error: anyhow::Error, ctx: &mut ViewContext<Self>);

    /// Called when termios indicates a likely password prompt is blocking the
    /// active block.
    #[cfg(unix)]
    fn on_possible_password_prompt(
        &mut self,
        _block_index: Option<BlockIndex>,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    /// Called when the block the poller was tracking completes.
    #[cfg(unix)]
    fn on_polled_block_completed(
        &mut self,
        _completed: &AfterBlockCompletedEvent,
        _ctx: &mut ViewContext<Self>,
    ) {
    }
}

/// Shared inputs needed to construct a terminal surface for a local PTY.
///
/// (Upstream Warp defines this in `local_tty/terminal_manager.rs`; kept here
/// alongside the trait until the local manager is genericized.)
pub struct TerminalSurfaceInit {
    pub wakeups_rx: async_channel::Receiver<()>,
    pub model_events: ModelHandle<ModelEventDispatcher>,
    pub model: Arc<FairMutex<TerminalModel>>,
    pub sessions: ModelHandle<Sessions>,
    pub size_info: SizeInfo,
    pub colors: crate::terminal::color::List,
    pub inactive_pty_reads_rx: InactiveReceiver<Arc<Vec<u8>>>,
}

/// A newly constructed terminal surface and its manager post-wiring callback.
pub struct TerminalSurfaceResult<S, PostWire> {
    pub surface: ViewHandle<S>,
    pub post_wire: PostWire,
}
