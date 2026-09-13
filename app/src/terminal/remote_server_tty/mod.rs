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
//! transport half of that seam. The `TerminalManager` that wraps it and the
//! session-creation path that spawns one are separate increments; see
//! [`event_loop`]'s doc comment for exactly where this stops.

mod event_loop;

pub use event_loop::EventLoop;
