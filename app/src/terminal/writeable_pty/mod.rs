mod bootstrap_file;
pub mod command_history;
pub(crate) mod command_pipe;
mod long_command_file;
mod message;
mod posix_long_command;
pub mod pty_controller;
pub mod remote_server_controller;
pub mod terminal_manager_util;
pub mod write_queue;
// The `TerminalSurface` trait + `PtyIntent` vocabulary + `TerminalSurfaceInit`/
// `TerminalSurfaceResult` live here as the surface abstraction seam that lets a
// single `TerminalManager` drive both the GUI `TerminalView` and the headless
// TUI surface.
pub mod terminal_surface;

pub use message::Message;
pub use pty_controller::{PtyController, PtyControllerEvent};
pub use terminal_surface::{
    PtyIntent, PtyIntentEvent, TerminalSurface, TerminalSurfaceInit, TerminalSurfaceResult,
};
