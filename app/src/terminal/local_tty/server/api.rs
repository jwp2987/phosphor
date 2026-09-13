use serde::{Deserialize, Serialize};

use crate::terminal::local_tty::{PtyOptions, PtySpawnResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum Result<T> {
    Ok(T),
    Err(String),
}

impl<T> From<anyhow::Result<T>> for Result<T> {
    fn from(value: anyhow::Result<T>) -> Self {
        match value {
            Ok(val) => self::Result::Ok(val),
            Err(err) => self::Result::Err(err.to_string()),
        }
    }
}

/// A child process's real exit status, made serializable so it can cross the
/// terminal-server socket.
///
/// `std::process::ExitStatus` has no public constructor other than the
/// platform-specific raw-status decoding `ExitStatusExt` provides, and isn't
/// itself `Serialize`/`Deserialize`. Rather than pre-extracting a code/signal
/// pair (which would have to reimplement -- and could drift from -- however
/// `ExitStatus::code()` decides that split), this carries the raw `wait(2)`
/// status word. `ExitStatusExt::into_raw`/`from_raw` are exact inverses, so
/// the receiving side reconstructs precisely what `Child::try_wait` observed
/// on the server, including `.code()` returning `None` if and only if the
/// child was terminated by a signal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) struct ChildExitStatus {
    raw_status: i32,
}

impl ChildExitStatus {
    /// Captures `status` in its serializable form.
    pub(super) fn from_std(status: std::process::ExitStatus) -> Self {
        use std::os::unix::process::ExitStatusExt;
        Self {
            raw_status: status.into_raw(),
        }
    }

    /// Reconstructs the `ExitStatus` this was captured from.
    pub(super) fn into_std(self) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(self.raw_status)
    }
}

/// The API for communication between the terminal client and server.  This is
/// organized into request/response pairs for the API "methods".
///
/// ### Future work
/// * We may want to structure this slightly differently to group
/// messages sent by the client or sent by the server, simplifying logic that
/// exists on each side for message parsing.  (We currently have error-checking
/// logic to ensure that, for example, the server doesn't receive a message that
/// should only be sent server->client; it would be preferable if we didn't need
/// to ever perform that check.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum Message {
    /// A message sent from client -> server requesting that the server spawns
    /// a new pty using the provided options.
    SpawnShellRequest { options: PtyOptions },
    /// The response for a `SpawnShellRequest`, with the result of the spawn
    /// operation.  Should only be sent from server -> client.
    SpawnShellResponse {
        spawn_result: Result<PtySpawnResult>,
    },
    /// A message sent from client -> server requesting that the server kill the
    /// child process with the provided process ID.
    KillChildRequest { pid: u32 },
    /// The response for a `KillChildRequest`, returning the string message from
    /// an error that occurred during the operation, if any.  Should only be
    /// sent from server -> client.
    KillChildResponse { error_msg: Option<String> },
    /// A message sent from server -> client requesting that a log message be
    /// written to the host application's log.  This has no matching response
    /// message - these requests are fire-and-forget from the server to the
    /// host application.
    WriteLogRequest {
        level: log::Level,
        target: String,
        message: String,
    },
    /// A message sent from server -> client notifying the client that one or
    /// more child processes have terminated, along with each one's real exit
    /// status.  This has no matching response message - these requests are
    /// fire-and-forget from the server to the host application.
    ChildrenTerminatedRequest {
        children: Vec<(u32, ChildExitStatus)>,
    },
}
