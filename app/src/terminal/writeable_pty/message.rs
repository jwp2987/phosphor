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

    /// Drops submitted user commands (`CommandInput`) that are queued for the PTY but not yet
    /// written, including the unwritten remainder of one in progress (#1046). Sent just before a
    /// ^C so the interrupt is not stuck behind a big multi-line paste. Everything else queued
    /// (bootstrap, typeahead, replies, agent writes) is kept.
    DiscardQueuedInput,

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
