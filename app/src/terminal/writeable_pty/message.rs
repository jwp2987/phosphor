use crate::terminal::SizeInfo;
use std::borrow::Cow;

/// Messages that may be sent to the `EventLoop`.
///
/// Two of these mean "stop", and they are deliberately two variants rather than
/// one variant carrying a flag. The transports that consume this enum
/// (`local_tty`, `remote_tty`, `remote_server_tty`) all match it exhaustively
/// with no wildcard arm, so a *variant* forces every one of them to state an
/// answer before it will compile; a new *field* on [`Message::Shutdown`] would
/// let a transport keep its existing arm and silently inherit the wrong one.
/// At the call sites `Message::Kill` also reads as the act it is, where
/// `Message::Shutdown { kill: true }` reads as a parameter to a different act.
#[derive(Debug)]
pub enum Message {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    /// Tear down *this client's* side of the pty: stop the `EventLoop` and let
    /// go of the session. Says nothing about the process on the far end.
    ///
    /// This is the indiscriminate teardown -- it is what
    /// `local_tty::TerminalManager`'s `Drop` sends, so it fires on tab close,
    /// window close and app quit alike, and a caller sending it is usually not
    /// choosing to end anything, merely going away.
    ///
    /// Note the qualifier: **`local_tty`'s**. It is the only producer, and it
    /// sends on `local_tty`'s own `mio_channel`, so no `Shutdown` reaches the
    /// other two transports today -- neither has a `Drop` impl. Their arms below
    /// describe what they *would* do, not what they currently see. An earlier
    /// version of this comment said "`TerminalManager`'s `Drop`" unqualified,
    /// which read as though all three received it.
    ///
    /// What that means depends on who owns the process, and the three
    /// transports differ because the underlying facts differ:
    ///
    /// * `local_tty` -- the shell is this app's own child and there is nobody
    ///   left to own it, so the loop's teardown reaps it. Here `Shutdown` and
    ///   [`Message::Kill`] converge.
    /// * `remote_server_tty` -- the pty belongs to a daemon on another host and
    ///   survives this client by design, so `Shutdown` *detaches*. See
    ///   "Shutdown is two intents wearing one name" in
    ///   `docs/design/moth-parliament.md`.
    /// * `remote_tty` -- not implemented; the websocket protocol carries no
    ///   teardown request. See that module's event loop.
    Shutdown,

    /// End the process on the far end of the pty, then stop the `EventLoop`.
    ///
    /// The deliberate act, sent by a caller that has decided this shell must
    /// not outlive the request -- the autoupdate relaunch path and the
    /// close-a-tab-with-a-running-command path, both of which reach here via
    /// `PtyIntent::ShutdownPty`. Unlike [`Message::Shutdown`] this is a
    /// statement about the *process*, so a transport that can leave a session
    /// running must not treat it as a detach.
    Kill,

    /// Indicates that the child process has exited.
    ///
    /// Only used on Windows, as we need to pass this information to the
    /// event loop via the channel (and cannot use the child event token).
    #[cfg_attr(not(windows), allow(dead_code))]
    ChildExited,

    /// Instruction to resize the PTY.
    Resize(SizeInfo),
}
