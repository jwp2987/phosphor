//! The out-of-band executor seam (design ruling R1).
//!
//! The shell backend runs ordinary commands on the host. It may do so ONLY over a channel that is
//! not the user's pty: an ssh ControlMaster side exec (a fresh mux client on the session's
//! master), a persistent side shell, or, in tests and the lab, a local `sh`. If a session has no
//! such channel the shell backend is `Unavailable` there. Implementors must never write to the
//! session's pty or read its output; that is the whole contract, and the reason this is a trait.
//!
//! No timers here: a timeout is the executor's own give-up net (it may fail the call), never a
//! poll or a debounce.

use std::fmt::Debug;

use futures::future::BoxFuture;

use crate::shellcmd::{MAX_OUTPUT_BYTES, ShellKind};

/// Room for the envelope's markers and a login banner on top of one capped section.
pub const MAX_EXEC_OUTPUT_BYTES: usize = MAX_OUTPUT_BYTES + 64 * 1024;

/// Which kind of channel an executor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    /// A mux client on the session's ssh ControlMaster (`ssh -S <socket> host <cmd>`).
    ControlMaster,
    /// A persistent side shell on the host (not the user's).
    SideShell,
    /// A local `sh -c` (tests, lab, contract harness).
    LocalSh,
}

/// One command to run. `command` is already wrapped for the remote login shell
/// ([`crate::shellcmd::ShellKind::wrap`]); the executor passes it as the remote command.
#[derive(Clone, Debug, Default)]
pub struct ExecRequest {
    pub command: String,
    /// Bytes for the command's stdin (empty: closed immediately).
    pub stdin: Vec<u8>,
    /// Stop reading stdout past this many bytes and report `truncated`.
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
    /// stdout was cut at `max_output_bytes`.
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExecError {
    #[error("no out-of-band channel: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
    #[error("cancelled")]
    Cancelled,
}

pub trait OutOfBandExecutor: Send + Sync + Debug {
    fn channel_kind(&self) -> ChannelKind;

    /// The family of the host's LOGIN shell, which parses `ExecRequest::command`.
    fn shell_kind(&self) -> ShellKind;

    /// Runs `request` and returns what it printed. Dropping the future cancels the command.
    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>>;
}
