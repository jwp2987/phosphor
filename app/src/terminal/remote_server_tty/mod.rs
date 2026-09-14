//! The client half of a daemon-owned pty session: the transport between this
//! app's `TerminalModel` and a session whose pty lives on a remote host, owned
//! by `crates/remote_server`'s daemon.
//!
//! **Not to be confused with its neighbour `remote_tty`**, which the name
//! deliberately does not shorten to. That module speaks a websocket protocol to
//! Warp's `ssh-proxy-server` on `127.0.0.1:3030` and is a different remote, a
//! different transport, and a different lifetime model: its sessions do not
//! outlive a disconnect. This one speaks the `remote_server` session RPCs
//! (`SpawnSession`, `WriteSessionStdin`, `ResizeSession`, `SignalSession`,
//! `ReattachSession`) over the SSH channel the user already authenticated, to a
//! daemon that keeps the pty when this client goes away.
//!
//! Item 6 of "Scoping session ownership" in `docs/design/moth-parliament.md` --
//! "the client seam: a `SessionType::Remote` whose pty lives on the far side,
//! where the terminal model expects a local handle". This module is the
//! transport half of that seam, plus the [`TerminalManager`] that owns one.
//!
//! **Where this stops.** Nothing creates a session yet. [`TerminalManager`]
//! attaches to a session that already exists, and it can only be reached through
//! a [`ConnectedRemotePtySession`] witness, which refuses to exist for a host
//! with no connected client -- the invariant the event loop records and cannot
//! check for itself. (`EventLoop` is intentionally not re-exported, so it is not
//! linkable from here; see the note on the `pub use` below.) The session-creation path that calls `spawn_session`
//! and mints a `RemotePtySessionId`, and reattach-on-open, are separate
//! increments.

mod event_loop;
mod terminal_manager;

// `EventLoop` is deliberately NOT re-exported, matching `remote_tty::mod`, which
// exports only its `TerminalManager` for the same reason. It was exported here
// at first, and that quietly falsified this module's central claim: with a `pub`
// `EventLoop::start` reachable from outside, anyone could build a loop without a
// `ConnectedRemotePtySession` and the invariant was back to being a comment.
// Keeping the type module-private is what makes the witness the only door.
pub use terminal_manager::{ConnectedRemotePtySession, TerminalManager};
