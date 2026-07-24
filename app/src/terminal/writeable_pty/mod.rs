#[cfg(not(target_family = "wasm"))]
mod bootstrap_file;
pub mod command_history;
mod message;
pub mod pty_controller;
#[cfg(not(target_family = "wasm"))]
pub mod remote_server_controller;
pub mod terminal_manager_util;
// The `TerminalSurface` trait + `PtyIntent` vocabulary + `TerminalSurfaceInit`/
// `TerminalSurfaceResult` live here as the first, additive step of the
// terminal-manager surface abstraction. The convenience re-export is added
// when the local manager is genericized to consume them.
pub mod terminal_surface;

pub use message::Message;
pub use pty_controller::{PtyController, PtyControllerEvent};
