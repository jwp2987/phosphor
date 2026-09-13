use std::fmt;

use serde::{Deserialize, Serialize};

/// Opaque identifier for a remote pty session hosted by the daemon.
///
/// Groundwork for session ownership (`docs/design/moth-parliament.md`,
/// "Scoping session ownership", CORRECTION 2026-09-12). A pty session is
/// owned by the *host*, not by the connection that spawned it -- it is
/// meant to keep running, and later be reattached, after that connection
/// goes away -- so it needs its own identity dimension.
///
/// This is deliberately a distinct type from two things already called
/// "session" in this crate, neither of which is a pty:
///   - `warp_core::SessionId`: one per client SSH *connection*. What
///     `SessionScopedRequest` means by "session".
///   - The bare `uint64 session_id` on `SessionBootstrapped` /
///     `RunCommandRequest`: the same per-connection id, used to key the
///     daemon's per-connection shell executor.
/// Reusing either name here would make "session" mean three different
/// things depending on which field you're reading.
///
/// Minted by the client, not the daemon -- unlike `HostId`, which the
/// daemon hands back in `InitializeResponse` because it names a fact about
/// the host the client can't know in advance. A session is created by the
/// client's own `SpawnSession` request, so the client can choose the id
/// upfront: that lets a UI key state on the session immediately, before the
/// round trip completes, and makes a retried `SpawnSession` idempotent
/// instead of spawning an orphan pty on every retry. Follows
/// `protocol::RequestId`'s style for the same client-minted reason.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemotePtySessionId(String);

impl RemotePtySessionId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for RemotePtySessionId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<RemotePtySessionId> for String {
    fn from(id: RemotePtySessionId) -> Self {
        id.0
    }
}

impl fmt::Display for RemotePtySessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
