//! Stopping MCP servers when the app quits (jwp2987/phosphor#687).
//!
//! The winit loop ends in `std::process::exit`, which skips `Drop`, so rmcp's
//! `TokioChildProcess` never gets to kill a stdio server's child: a server that
//! ignores stdin EOF would outlive the app. [`McpAppExitShutdown`] is the second
//! half of the fix: `TemplatableMCPServerManager::begin_shutdown_for_app_exit`
//! starts closing every server's transport, and [`McpAppExitShutdown::finish`]
//! waits for those closes up to a deadline shared with the language-server
//! shutdown, then kills whatever stdio child is still running.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};

use instant::Instant;
use uuid::Uuid;

/// MCP servers whose app-exit shutdown has been started, awaiting [`Self::finish`].
#[must_use = "call `finish` to wait for the servers and kill any that did not stop"]
pub struct McpAppExitShutdown {
    /// Receives a server's installation uuid once its transport close has completed
    /// (for stdio, once rmcp has reaped the child).
    done: Receiver<Uuid>,
    /// Servers not yet confirmed stopped, with their stdio child's pid if they have one.
    pending: HashMap<Uuid, Option<u32>>,
}

/// What [`McpAppExitShutdown::finish`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct McpAppExitOutcome {
    /// Servers whose transport close completed before the deadline.
    pub stopped: usize,
    /// Stdio children still running at the deadline and killed.
    pub killed: usize,
    /// Servers neither confirmed stopped nor killed: HTTP/SSE servers whose close did
    /// not finish in time (there is no process of ours to kill), or a stdio child that
    /// had already exited when the kill was sent.
    pub abandoned: usize,
}

impl McpAppExitShutdown {
    pub(crate) fn new(done: Receiver<Uuid>, pending: HashMap<Uuid, Option<u32>>) -> Self {
        Self { done, pending }
    }

    /// How many servers are awaiting confirmation that they stopped.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Blocks until every server has stopped or `deadline` passes, whichever is first,
    /// then SIGKILLs (on Windows, terminates) every stdio child that has not stopped.
    ///
    /// Completions that already arrived are always collected, even when `deadline`
    /// has passed, so a child rmcp has already reaped is never signalled by pid.
    pub fn finish(self, deadline: Instant) -> McpAppExitOutcome {
        self.finish_with(deadline, kill_child_process)
    }

    pub(crate) fn finish_with(
        mut self,
        deadline: Instant,
        mut kill: impl FnMut(u32) -> bool,
    ) -> McpAppExitOutcome {
        let mut outcome = McpAppExitOutcome::default();
        while !self.pending.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let received = if remaining.is_zero() {
                match self.done.try_recv() {
                    Ok(uuid) => Some(uuid),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
                }
            } else {
                match self.done.recv_timeout(remaining) {
                    Ok(uuid) => Some(uuid),
                    Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
                }
            };
            let Some(uuid) = received else {
                break;
            };
            if self.pending.remove(&uuid).is_some() {
                outcome.stopped += 1;
            }
        }

        for (uuid, child_pid) in self.pending {
            match child_pid {
                Some(pid) if kill(pid) => {
                    log::info!(
                        "MCP server {uuid} (pid {pid}) did not stop in time for app exit; killed it"
                    );
                    outcome.killed += 1;
                }
                _ => {
                    log::warn!("MCP server {uuid} did not finish shutting down before app exit");
                    outcome.abandoned += 1;
                }
            }
        }
        outcome
    }
}

/// Forcibly kills the process `pid`, returning whether the signal was delivered.
///
/// Only called for a stdio MCP server's direct child. rmcp spawns it with a plain
/// `tokio::process::Command` (no process group), so this kills that process alone;
/// anything it spawned itself is left to notice stdin/stdout closing.
pub(crate) fn kill_child_process(pid: u32) -> bool {
    // pid 0 would signal our own process group.
    if pid == 0 {
        return false;
    }
    kill_child_process_impl(pid)
}

#[cfg(unix)]
fn kill_child_process_impl(pid: u32) -> bool {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    match kill(Pid::from_raw(raw), Signal::SIGKILL) {
        Ok(()) => true,
        Err(nix::errno::Errno::ESRCH) => false,
        Err(err) => {
            log::warn!("Failed to kill MCP server process {pid}: {err}");
            false
        }
    }
}

#[cfg(windows)]
fn kill_child_process_impl(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: plain Win32 calls on a handle we open and close here.
    unsafe {
        match OpenProcess(PROCESS_TERMINATE, false, pid) {
            Ok(handle) => {
                let terminated = TerminateProcess(handle, 1).is_ok();
                let _ = CloseHandle(handle);
                terminated
            }
            Err(_) => false,
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn kill_child_process_impl(_pid: u32) -> bool {
    false
}

#[cfg(test)]
#[path = "app_exit_tests.rs"]
mod tests;
