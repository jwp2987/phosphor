//! Stopping MCP servers when the app quits (jwp2987/phosphor#687).
//!
//! The winit loop ends in `std::process::exit`, which skips `Drop`, so rmcp's
//! `TokioChildProcess` never gets to kill a stdio server's child: a server that
//! ignores stdin EOF would outlive the app. [`McpAppExitShutdown`] is the second
//! half of the fix: `TemplatableMCPServerManager::begin_shutdown_for_app_exit`
//! starts closing every server's transport, and [`McpAppExitShutdown::finish`]
//! waits for those closes up to a deadline shared with the language-server
//! shutdown, then kills whatever stdio child is still running, through a
//! [`ChildKillHandle`] opened when it was spawned.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};

use instant::Instant;
use parking_lot::Mutex;
use uuid::Uuid;

/// MCP servers whose app-exit shutdown has been started, awaiting [`Self::finish`].
#[must_use = "call `finish` to wait for the servers and kill any that did not stop"]
pub struct McpAppExitShutdown {
    /// Receives a server's installation uuid once its transport close has completed
    /// (for stdio, once rmcp has reaped the child).
    done: Receiver<Uuid>,
    /// Servers not yet confirmed stopped, with their stdio child's kill handle (an
    /// empty slot for HTTP/SSE servers).
    pending: HashMap<Uuid, Arc<ChildProcessSlot>>,
}

/// What [`McpAppExitShutdown::finish`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct McpAppExitOutcome {
    /// Servers whose transport close completed before the deadline.
    pub stopped: usize,
    /// Stdio children still running at the deadline and killed.
    pub killed: usize,
    /// Servers neither confirmed stopped nor killed: HTTP/SSE servers whose close did
    /// not finish in time (there is no process of ours to kill), stdio servers whose
    /// child already exited (its handle was released) or could not be signalled.
    pub abandoned: usize,
}

impl McpAppExitShutdown {
    pub(crate) fn new(done: Receiver<Uuid>, pending: HashMap<Uuid, Arc<ChildProcessSlot>>) -> Self {
        Self { done, pending }
    }

    /// How many servers are awaiting confirmation that they stopped.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Blocks until every server has stopped or `deadline` passes, whichever is first,
    /// then kills every stdio child that has not stopped, through the handle opened
    /// when it was spawned (never by a bare pid).
    ///
    /// Completions that already arrived are always collected, even when `deadline`
    /// has passed, so a server known to have stopped is left alone.
    pub fn finish(self, deadline: Instant) -> McpAppExitOutcome {
        self.finish_with(deadline, |_, child| child.kill())
    }

    /// [`Self::finish`] with the kill step injected: `kill` returns `None` when there
    /// is no process to kill, else whether the kill was delivered.
    pub(crate) fn finish_with(
        mut self,
        deadline: Instant,
        mut kill: impl FnMut(Uuid, &ChildProcessSlot) -> Option<bool>,
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

        for (uuid, child) in self.pending {
            match kill(uuid, &child) {
                Some(true) => {
                    log::info!("MCP server {uuid} did not stop in time for app exit; killed it");
                    outcome.killed += 1;
                }
                Some(false) | None => {
                    log::warn!("MCP server {uuid} did not finish shutting down before app exit");
                    outcome.abandoned += 1;
                }
            }
        }
        outcome
    }
}

/// Holds a stdio MCP server child's [`ChildKillHandle`] from spawn until the child is
/// known to be gone.
///
/// Filled right after the spawn, while the child cannot have been reaped; emptied when
/// the server's service loop closes its transport (rmcp has reaped the child by then),
/// when the handshake fails, or when the handle is used. HTTP/SSE servers keep an
/// empty slot.
#[derive(Default)]
pub(crate) struct ChildProcessSlot(Mutex<Option<ChildKillHandle>>);

impl ChildProcessSlot {
    pub(crate) fn fill(&self, handle: ChildKillHandle) {
        *self.0.lock() = Some(handle);
    }

    /// Drops the handle: the child is gone, or no longer ours to kill.
    pub(crate) fn release(&self) {
        self.0.lock().take();
    }

    #[cfg(test)]
    pub(crate) fn is_filled(&self) -> bool {
        self.0.lock().is_some()
    }

    /// Kills the child if the slot still holds its handle, consuming the handle.
    /// Returns `None` when the slot is empty, else whether the kill was delivered.
    pub(crate) fn kill(&self) -> Option<bool> {
        let handle = self.0.lock().take()?;
        Some(handle.kill())
    }
}

/// A way to kill one specific child process that cannot hit an unrelated process
/// that later reuses its pid.
///
/// - Linux: a pidfd (`pidfd_open`, killed with `pidfd_send_signal`). A pidfd refers
///   to the process itself, so once it has been reaped the kill fails with `ESRCH`.
///   Kernels before 5.3 have no pidfds; then no handle is opened and the child is
///   never force-killed (it still gets stdin EOF).
/// - macOS: no pidfds, so the child's start time is recorded, and the kill is only
///   sent if the pid still names a process with that start time.
/// - Windows: a process handle opened at spawn. Holding it keeps the process object,
///   and so its pid, from being reused. No process-group/Job-Object equivalent of
///   `kill_group` is implemented yet (jwp2987/phosphor#707); a stdio server's
///   grandchild is not killed on Windows.
/// - Anything else: no handle.
///
/// On Unix, `kill` also best-effort signals the child's whole process group (see
/// [`ChildKillHandle::kill`]): the spawn site puts the direct child in a new group
/// of its own, so `pgid == pid` and no separate id needs tracking.
pub(crate) struct ChildKillHandle {
    pid: u32,
    inner: imp::Handle,
}

impl ChildKillHandle {
    /// Opens a handle to `pid`. Call only while the child is guaranteed unreaped
    /// (right after spawning it, before anything waits on it), or it may name
    /// another process.
    pub(crate) fn open(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        let inner = imp::open(pid)?;
        Some(Self { pid, inner })
    }

    /// Forcibly kills the child (`SIGKILL` / `TerminateProcess`), returning whether
    /// the kill was delivered. A child that has already exited is left alone.
    ///
    /// The direct child is spawned into its own new process group (`pgid == pid`,
    /// jwp2987/phosphor#707), so on Unix this also best-effort signals the whole
    /// group (`kill(-pid, SIGKILL)`) after the pidfd-guarded (or, on macOS,
    /// start-time-guarded) single-process kill: that part still cannot hit a
    /// reused pid, but the group signal is a plain numeric `pgid` syscall with no
    /// pidfd-equivalent, so it carries a narrower, accepted residual race (the
    /// group would have to fully empty out and that exact number be reissued as an
    /// unrelated process's own new group, between the two syscalls) -- the same
    /// kind of residual this module already documents for macOS and pre-5.3
    /// kernels. This is what lets the process-group survive a leftover
    /// grandchild (e.g. under `npx`/`uvx`/a shell wrapper) that ignores stdin EOF:
    /// without it, only the direct child died and the grandchild outlived the app.
    pub(crate) fn kill(&self) -> bool {
        let killed = imp::kill(&self.inner, self.pid);
        imp::kill_group(self.pid);
        killed
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};

    pub(super) struct Handle(OwnedFd);

    pub(super) fn open(pid: u32) -> Option<Handle> {
        let pid = libc::pid_t::try_from(pid).ok()?;
        let flags: libc::c_uint = 0;
        // SAFETY: `pidfd_open(2)` takes a pid and flags and returns a new fd or -1.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, flags) };
        if fd < 0 {
            log::warn!(
                "pidfd_open({pid}) failed ({}); the MCP server will not be force-killed on quit",
                std::io::Error::last_os_error()
            );
            return None;
        }
        let fd = libc::c_int::try_from(fd).ok()?;
        // SAFETY: `fd` is a freshly opened pidfd that nothing else owns.
        Some(Handle(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    pub(super) fn kill(handle: &Handle, pid: u32) -> bool {
        let flags: libc::c_uint = 0;
        // SAFETY: `pidfd_send_signal(2)` on a pidfd we own, with no siginfo.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                handle.0.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                flags,
            )
        };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::ESRCH) {
                log::warn!("Failed to kill MCP server process {pid}: {err}");
            }
            return false;
        }
        true
    }

    /// Best-effort `SIGKILL` to the whole process group (jwp2987/phosphor#707): the
    /// spawn site puts the child in a new group of its own, so `pgid == pid`. A
    /// negative pid signals the group rather than the single process. Errors
    /// (`ESRCH`: the group is already empty) are not logged -- this runs on every
    /// kill, including the common case where there was never a grandchild to reach.
    pub(super) fn kill_group(pid: u32) {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return;
        };
        // SAFETY: plain kill(2); a negative pid targets the process group `-pid`.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    pub(super) struct Handle {
        start_time: (u64, u64),
    }

    /// The start time of the process `pid` names now, if any.
    fn start_time(pid: u32) -> Option<(u64, u64)> {
        let pid = libc::c_int::try_from(pid).ok()?;
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        // SAFETY: `info` is a writable buffer of exactly `size` bytes.
        let written = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                libc::c_int::try_from(size).ok()?,
            )
        };
        if usize::try_from(written).ok()? != size {
            return None;
        }
        // SAFETY: `proc_pidinfo` filled all `size` bytes.
        let info = unsafe { info.assume_init() };
        Some((info.pbi_start_tvsec, info.pbi_start_tvusec))
    }

    pub(super) fn open(pid: u32) -> Option<Handle> {
        Some(Handle {
            start_time: start_time(pid)?,
        })
    }

    pub(super) fn kill(handle: &Handle, pid: u32) -> bool {
        // Only signal `pid` while it still names the process we spawned.
        if start_time(pid) != Some(handle.start_time) {
            return false;
        }
        let Ok(raw) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: plain kill(2).
        unsafe { libc::kill(raw, libc::SIGKILL) == 0 }
    }

    /// Best-effort `SIGKILL` to the whole process group (jwp2987/phosphor#707); see
    /// the Linux `kill_group` for the reasoning. macOS has no pidfd, but the
    /// group-kill carries the same residual race either way -- it is a plain
    /// numeric pgid syscall on every platform.
    pub(super) fn kill_group(pid: u32) {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return;
        };
        // SAFETY: plain kill(2); a negative pid targets the process group `-pid`.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    /// A process handle, stored as an integer so the struct is `Send`.
    pub(super) struct Handle(isize);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: closing the handle `open` opened; nothing else closes it.
            let _ = unsafe { CloseHandle(HANDLE(self.0 as *mut core::ffi::c_void)) };
        }
    }

    pub(super) fn open(pid: u32) -> Option<Handle> {
        // SAFETY: plain Win32 call; the returned handle is owned by `Handle`.
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }.ok()?;
        Some(Handle(handle.0 as isize))
    }

    pub(super) fn kill(handle: &Handle, _pid: u32) -> bool {
        // SAFETY: terminating the process our open handle refers to.
        unsafe { TerminateProcess(HANDLE(handle.0 as *mut core::ffi::c_void), 1) }.is_ok()
    }

    /// No Windows equivalent yet (jwp2987/phosphor#707): a Job Object would need
    /// plumbing through to this out-of-band kill path (not just to rmcp's own
    /// `Child`), which is more than a "if simple" change. A stdio server's
    /// grandchild is not killed on Windows.
    pub(super) fn kill_group(_pid: u32) {}
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    pub(super) struct Handle;

    pub(super) fn open(_pid: u32) -> Option<Handle> {
        None
    }

    pub(super) fn kill(_handle: &Handle, _pid: u32) -> bool {
        false
    }

    pub(super) fn kill_group(_pid: u32) {}
}

#[cfg(test)]
#[path = "app_exit_tests.rs"]
mod tests;
