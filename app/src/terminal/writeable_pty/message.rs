use super::write_queue::{AbortScope, Pacing};
use crate::terminal::SizeInfo;
use std::borrow::Cow;

/// Messages that may be sent to the `EventLoop`.
#[derive(Debug)]
pub enum Message {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    /// Like `Input`, but the submitted buffer of a user command, which
    /// `DiscardQueuedInput` may drop while it is unwritten (#1046).
    CommandInput(Cow<'static, [u8]>),

    /// A paste (#1384 P4): like `CommandInput` (a ^C drops what is unwritten) but its own class,
    /// so `Abort(AbortScope::Paste)` can name it. Sent only when the loop runs the write queue.
    #[cfg_attr(not(unix), allow(dead_code))]
    PasteInput(Cow<'static, [u8]>),

    /// An agent's input to a running command (#1384 P4): ordered like `Input`, one whole write,
    /// and removable by `Abort(AbortScope::Agent)` alone. Sent only when the loop runs the write
    /// queue.
    #[cfg_attr(not(unix), allow(dead_code))]
    AgentInput(Cow<'static, [u8]>),

    /// Drops submitted user commands (`CommandInput`) that are queued for the PTY but not yet
    /// written, including the unwritten remainder of one in progress (#1046). Sent just before a
    /// ^C so the interrupt is not stuck behind a big multi-line paste. Everything else queued
    /// (bootstrap, typeahead, replies, agent writes) is kept.
    DiscardQueuedInput,

    /// A bootstrap script written as ONE job in paced chunks (#1384): nothing a user types lands
    /// inside it (that waits behind the whole job), a terminal-query reply may interleave only
    /// between chunks, and `Abort` / a signal byte cancels what is unwritten. The pacing is the
    /// writer's own poll deadline. Sent only to the local unix event loop.
    #[cfg_attr(not(unix), allow(dead_code))]
    SetupChunked {
        chunks: Vec<Vec<u8>>,
        pacing: Pacing,
    },

    /// Removes the queued and in-flight jobs `AbortScope` covers (#1384). A job cut inside a
    /// bracketed paste is followed by the paste-close so the shell does not swallow what comes
    /// next as pasted text.
    #[cfg_attr(not(unix), allow(dead_code))]
    Abort(AbortScope),

    /// Indicates that the `EventLoop` should be shut down.
    Shutdown,

    /// Indicates that the child process has exited.
    ///
    /// Only used on Windows, as we need to pass this information to the
    /// event loop via the channel (and cannot use the child event token).
    #[cfg_attr(not(windows), allow(dead_code))]
    ChildExited,

    /// Instruction to resize the PTY.
    Resize(SizeInfo),
}
