mod bootstrap_file;
mod long_command_file;
pub(crate) mod command_pipe;
pub mod command_history;
mod message;
mod posix_long_command;
pub mod write_queue;
pub mod pty_controller;
pub mod remote_server_controller;
pub mod terminal_manager_util;
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
