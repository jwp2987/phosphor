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
//! **How a session comes to exist.** [`spawn_remote_session`] mints a
//! `RemotePtySessionId`, sends `SpawnSession`, and -- only once the daemon has
//! answered -- builds the [`ConnectedRemotePtySession`] witness that
//! [`TerminalManager::create_model`] demands. The witness refuses to exist for a
//! host with no connected client, which is the invariant the event loop records
//! and cannot check for itself. (`EventLoop` is intentionally not re-exported,
//! so it is not linkable from here; see the note on the `pub use` below.)
//!
//! **Where this stops.** Three things, none of them hidden:
//!
//! - **Nothing here opens a tab.** [`spawn_remote_session`] hands back a
//!   witness; building a pane needs `TerminalViewResources`, a size and a
//!   `WindowId`, which only the pane layer has.
//! - **Reattach-on-open is a separate increment.** Nothing enumerates a host's
//!   existing sessions on connect or re-adopts one, and nothing subscribes to
//!   `SessionReconnected` to recover a session that outlived a failed attach.
//! - **The output a session emits before its `EventLoop` subscribes is not
//!   replayed.** The daemon pushes output as soon as the pty produces it, and
//!   the subscription is only established inside `create_model`, one network
//!   round trip later -- so a shell's first prompt is typically pushed to a
//!   manager with no subscriber for it and dropped from the live stream. It is
//!   *not* lost on the far side: nothing acknowledges live pushes
//!   (`RemoteServerClient::acknowledge_session_output` is sent only from
//!   `reattach_session`), so those bytes are still in `SessionStore`'s ring
//!   buffer. Recovering them means a `reattach_session` and a way to prime the
//!   loop with its payload, which is the reattach increment's shape.

mod event_loop;
mod session_spawn;
mod terminal_manager;

// `EventLoop` is deliberately NOT re-exported, matching `remote_tty::mod`, which
// exports only its `TerminalManager` for the same reason. It was exported here
// at first, and that quietly falsified this module's central claim: with a `pub`
// `EventLoop::start` reachable from outside, anyone could build a loop without a
// `ConnectedRemotePtySession` and the invariant was back to being a comment.
// Keeping the type module-private is what makes the witness the only door.
pub use session_spawn::{
    DEFAULT_SPAWN_COLS, DEFAULT_SPAWN_ROWS, OrphanCleanup, RemoteSessionSpawnFailure,
    RemoteSessionSpawnRequest, spawn_remote_session,
};
pub use terminal_manager::{ConnectedRemotePtySession, TerminalManager};
