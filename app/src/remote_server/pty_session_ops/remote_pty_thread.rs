//! The dedicated OS thread + mio poll loop that owns one remote session's
//! real pty, for `pty_session_ops::LocalTtyPtySessionOperations`.
//!
//! Mirrors the shape `local_tty::event_loop::EventLoop` uses to drive a local
//! terminal's pty I/O on its own thread (`terminal_manager.rs`), but forwards
//! raw bytes and the exit outcome into an `async_channel` instead of an ANSI
//! parser writing into a `TerminalModel`: there is no grid to parse into on
//! the daemon side (`SessionType::Remote`, item 6 of "Scoping session
//! ownership" in `docs/design/moth-parliament.md`, is out of scope here), so
//! this is bytes in, bytes out.
//!
//! Unix-only: registering the pty's raw fd with mio goes through
//! `mio::unix::SourceFd` inside `local_tty::unix::Pty`, which has no Windows
//! equivalent in this crate -- see `pty_session_ops`'s
//! `#[cfg(all(feature = "local_tty", not(unix)))]` stub.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use mio::{Events, Interest, Poll, Token, Waker};
use remote_server::RemotePtySessionId;
use remote_server::session_store::SessionExitStatus;
use warpui::AppContext;

use crate::terminal::SizeInfo;
use crate::terminal::local_tty::shell::{
    BASH_SHELL_PATH, DirectShellStarter, FISH_SHELL_PATH, ShellStarter, ZSH_SHELL_PATH,
    supported_shell_path_and_type,
};
use crate::terminal::local_tty::{ChildEvent, EventedPty, EventedReadWrite, Pty, PtyOptions};

use super::{PtySessionEvent, PtySessionOpError, PtySpawnSpec, session_exit_status_from_process};

/// A command sent from `LocalTtyPtySessionOperations`'s `&self` methods into
/// the thread that owns the live `Pty`.
enum PtyCommand {
    WriteStdin(Vec<u8>),
    Resize { rows: u32, cols: u32 },
}

/// The mio-pollable side of the command channel: a plain queue behind a
/// `Mutex`, woken by a `mio::Waker`. `local_tty::mio_channel` (what
/// `EventLoop` uses for the same purpose) is private to the `local_tty`
/// module and unreachable from here, so this is the minimal equivalent built
/// directly on public mio.
struct CommandChannel {
    queue: Mutex<VecDeque<PtyCommand>>,
    waker: Waker,
}

impl CommandChannel {
    fn push(&self, command: PtyCommand) {
        self.queue.lock().unwrap().push_back(command);
        // A failed wake means the poll loop -- and its thread -- is already
        // gone. The command is simply never seen, which is fine: nothing is
        // waiting on it, and the session is already torn down.
        let _ = self.waker.wake();
    }
}

/// Token for `CommandChannel`'s waker in this thread's own, private
/// `mio::Poll`. Must differ from `Pty`'s hard-coded `PTY_TOKEN`(1) /
/// `SIGNALS_TOKEN`(2) (`local_tty::unix::Pty::new`); nothing else contends
/// for tokens on this poll instance.
const COMMAND_TOKEN: Token = Token(3);

/// Bytes read per `read()` call before a chunk is forwarded. Deliberately
/// smaller than local_tty's own 256 KiB `READ_BUFFER_SIZE`: that size exists
/// to amortize a locked `TerminalModel` update, which nothing here holds.
/// Forwarding smaller chunks sooner keeps output latency down for a remote
/// client; it is unrelated to the design doc's 256 KiB *retention* bound
/// (`SessionStore`'s ring buffer), which bounds how much is kept, not how
/// much moves per read.
const READ_CHUNK_SIZE: usize = 8192;

/// One remote session's live pty: the dedicated thread that owns it, and the
/// channel used to reach that thread.
pub(super) struct LiveSession {
    commands: Arc<CommandChannel>,
    pid: u32,
    thread: JoinHandle<()>,
}

impl LiveSession {
    /// Spawns a real pty for `spec` and a dedicated OS thread that drives its
    /// I/O, forwarding output/exit through `events_tx`.
    ///
    /// `ctx` is only needed to spawn the pty itself (`Pty::new` looks up the
    /// `PtySpawner` singleton through it); nothing past that point touches
    /// the entity system, which is exactly why every other method here is
    /// `&self` with no ctx.
    pub(super) fn spawn(
        id: RemotePtySessionId,
        spec: &PtySpawnSpec,
        ctx: &mut AppContext,
        events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>,
    ) -> Result<Self, PtySessionOpError> {
        let shell_starter = resolve_shell_starter(spec.shell.as_deref())?;
        Self::spawn_with_shell_starter(id, shell_starter, spec, ctx, events_tx)
    }

    /// The shared spawn path behind [`Self::spawn`], parameterized on an
    /// already-resolved `ShellStarter` rather than resolving one itself.
    ///
    /// Split out so `pty_session_ops_tests.rs` can spawn a trivial, hermetic
    /// command (`DirectShellStarter::new_for_test`) instead of a real
    /// interactive shell -- production shell resolution
    /// (`resolve_shell_starter`) only recognizes real shells (bash/zsh/fish),
    /// which is the wrong shape for a fast test.
    pub(super) fn spawn_with_shell_starter(
        id: RemotePtySessionId,
        shell_starter: ShellStarter,
        spec: &PtySpawnSpec,
        ctx: &mut AppContext,
        events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>,
    ) -> Result<Self, PtySessionOpError> {
        let options = PtyOptions {
            size: SizeInfo::new_without_font_metrics(spec.rows as usize, spec.cols as usize),
            window_id: None,
            shell_starter,
            start_dir: Some(PathBuf::from(spec.cwd.clone())),
            env_vars: spec
                .environment_variables
                .iter()
                .map(|(key, value)| (OsString::from(key.clone()), OsString::from(value.clone())))
                .collect(),
            // These all gate Warp's *own* shell-integration bootstrap (the
            // ControlMaster SSH wrapper, shell-debug mode, PS1 honoring, the
            // Node-version prompt chip). A remote session is a plain
            // byte-passthrough pty with no client-side shell integration yet
            // (`SessionType::Remote`, item 6, is out of scope), so none of
            // it applies.
            enable_ssh_wrapper: false,
            reuse_ssh_control_master: false,
            shell_debug_mode: false,
            honor_ps1: false,
            node_version_chip_enabled: false,
            close_fds: true,
        };

        // This fork's crash reporter is a no-op either way (`crash_reporting::
        // suspend_crash_reporting_for_child_spawn`/`resume_crash_reporting_after_child_spawn`
        // both just log and return), so there is nothing to suspend/resume here
        // regardless of which value is passed.
        let mut pty = Pty::new(options, false, ctx)
            .map_err(|err| PtySessionOpError::new(format!("failed to spawn pty: {err:#}")))?;
        let pid = pty.get_pid();

        let poll = Poll::new()
            .map_err(|err| PtySessionOpError::new(format!("failed to create mio poll: {err}")))?;
        let waker = Waker::new(poll.registry(), COMMAND_TOKEN)
            .map_err(|err| PtySessionOpError::new(format!("failed to create mio waker: {err}")))?;
        pty.register(&poll, Interest::READABLE | Interest::WRITABLE)
            .map_err(|err| PtySessionOpError::new(format!("failed to register pty: {err}")))?;

        let commands = Arc::new(CommandChannel {
            queue: Mutex::new(VecDeque::new()),
            waker,
        });
        let thread_commands = commands.clone();

        let thread = std::thread::Builder::new()
            .name("remote-pty".to_string())
            .spawn(move || run_session_loop(id, pty, poll, thread_commands, events_tx))
            .map_err(|err| {
                PtySessionOpError::new(format!("failed to spawn pty reader thread: {err}"))
            })?;

        Ok(Self {
            commands,
            pid,
            thread,
        })
    }

    pub(super) fn write_stdin(&self, data: Vec<u8>) {
        self.commands.push(PtyCommand::WriteStdin(data));
    }

    pub(super) fn resize(&self, rows: u32, cols: u32) {
        self.commands.push(PtyCommand::Resize { rows, cols });
    }

    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    /// Reclaims this session's thread. The only caller,
    /// `LocalTtyPtySessionOperations::kill`, is only reached after
    /// `ServerModel` has recorded this session's exit -- which in turn only
    /// happens once this thread has already sent that exit event and
    /// returned -- so the join below is expected to return immediately, not
    /// block on a still-running thread.
    pub(super) fn join(self) {
        if let Err(panic) = self.thread.join() {
            log::error!("remote pty reader thread panicked: {panic:?}");
        }
    }
}

/// Resolves `spec_shell` (the client-requested shell path, if any) to a
/// `ShellStarter`, falling back to `$SHELL` and then the same bash/zsh/fish
/// candidates `local_tty::shell::ShellStarter::compute_fallback_shell` tries.
///
/// Reimplemented rather than called, because that function -- and the
/// passwd-lookup it also falls back to -- is private to `local_tty`, and
/// `ShellStarter::init` (the public entry point) needs an `AvailableShells`
/// model and a settings service, neither of which the daemon has. This is
/// deliberately a smaller, self-contained subset of that resolution, not a
/// port of it.
fn resolve_shell_starter(spec_shell: Option<&str>) -> Result<ShellStarter, PtySessionOpError> {
    if let Some(shell) = spec_shell {
        let (path, shell_type) = supported_shell_path_and_type(shell)
            .ok_or_else(|| PtySessionOpError::new(format!("unsupported shell: {shell}")))?;
        return Ok(ShellStarter::Direct(
            DirectShellStarter::for_explicit_shell(path, shell_type),
        ));
    }

    let mut candidates = Vec::new();
    if let Ok(env_shell) = std::env::var("SHELL") {
        candidates.push(env_shell);
    }
    candidates.extend(
        [ZSH_SHELL_PATH, BASH_SHELL_PATH, FISH_SHELL_PATH]
            .into_iter()
            .map(str::to_owned),
    );

    for candidate in &candidates {
        if let Some((path, shell_type)) = supported_shell_path_and_type(candidate) {
            return Ok(ShellStarter::Direct(
                DirectShellStarter::for_explicit_shell(path, shell_type),
            ));
        }
    }

    Err(PtySessionOpError::new(
        "no supported shell found on this host ($SHELL, /bin/zsh, /bin/bash, /bin/fish all \
         missing or unsupported)",
    ))
}

/// Sends `signal` to the process group headed by `pid`. The spawned shell is
/// its own session/process-group leader (`setsid()` + `TIOCSCTTY` in
/// `local_tty::unix::spawn_command_in_pty`'s pre_exec hook), so `-pid`
/// reaches it and any of its still-attached (non-detached) children --
/// mirroring what a terminal's own Ctrl-C/Ctrl-\ delivery targets.
///
/// `pid` must be greater than 1, and this refuses to proceed otherwise. The
/// negation is what makes that non-negotiable: `kill(-0, sig)` signals every
/// process in **the caller's own** process group -- the daemon and everything
/// it spawned -- and `kill(-1, sig)` signals every process the user is allowed
/// to signal. A stale or defaulted pid reaching here would therefore turn a
/// client's `SignalSession` into a remotely-triggered kill of the daemon
/// itself. No caller is believed to pass 0 today; the guard is here because
/// the blast radius if one ever does is the whole session, and one comparison
/// is a trivial price for removing that possibility entirely.
///
/// SAFETY: `libc::kill` is a plain syscall wrapper; passing a negated pid and
/// a signal number carries no aliasing or memory-safety requirement beyond
/// the FFI call itself. The guard above is about which processes are targeted,
/// not memory safety.
pub(super) fn send_signal_to_process_group(pid: u32, signal: libc::c_int) -> io::Result<()> {
    if pid <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to signal process group for implausible pid {pid}"),
        ));
    }
    let result = unsafe { libc::kill(-(pid as libc::pid_t), signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The pty thread's body. Mirrors `local_tty::event_loop::EventLoop::spawn`'s
/// shape (poll, drain commands, read while readable, write while writable,
/// stop on child-exit or a fatal I/O error) without the ANSI parser or
/// `TerminalModel` it drives locally.
fn run_session_loop(
    id: RemotePtySessionId,
    mut pty: Pty,
    mut poll: Poll,
    commands: Arc<CommandChannel>,
    events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>,
) {
    let mut events = Events::with_capacity(128);
    let mut write_queue: VecDeque<Vec<u8>> = VecDeque::new();
    let mut can_read = false;
    let mut can_write = false;
    let mut read_buf = [0u8; READ_CHUNK_SIZE];

    let exit_status = 'session: loop {
        if let Err(err) = poll.poll(&mut events, None) {
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            log::error!("remote pty session {id}: poll error: {err}");
            break 'session force_kill_and_report(pty);
        }

        for event in events.iter() {
            let token = event.token();
            if token == COMMAND_TOKEN {
                while let Some(command) = commands.queue.lock().unwrap().pop_front() {
                    match command {
                        PtyCommand::WriteStdin(data) => write_queue.push_back(data),
                        PtyCommand::Resize { rows, cols } => pty.on_resize(
                            &SizeInfo::new_without_font_metrics(rows as usize, cols as usize),
                        ),
                    }
                }
            } else if token == pty.child_event_token() {
                if let Some(ChildEvent::Exited) = pty.next_child_event() {
                    let status = pty
                        .take_exit_status()
                        .map(session_exit_status_from_process)
                        .unwrap_or_else(SessionExitStatus::signalled);
                    break 'session status;
                }
            } else if token == pty.read_token() || token == pty.write_token() {
                if event.is_readable() {
                    can_read = true;
                }
                if event.is_writable() {
                    can_write = true;
                }
            }
        }

        while can_read {
            match pty.reader().read(&mut read_buf) {
                Ok(0) => can_read = false,
                Ok(n) => {
                    let chunk = read_buf[..n].to_vec();
                    if events_tx
                        .try_send((id.clone(), PtySessionEvent::Output(chunk)))
                        .is_err()
                    {
                        // The receiving end (`ServerModel`) is gone -- the
                        // daemon is shutting down. Nothing left to forward
                        // to.
                        break 'session force_kill_and_report(pty);
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => can_read = false,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => {
                    log::error!("remote pty session {id}: read error: {err}");
                    break 'session force_kill_and_report(pty);
                }
            }
        }

        while can_write {
            let Some(mut chunk) = write_queue.pop_front() else {
                break;
            };
            match pty.writer().write(&chunk) {
                Ok(written) if written == chunk.len() => {}
                Ok(0) => {
                    can_write = false;
                    write_queue.push_front(chunk);
                }
                Ok(written) => {
                    chunk.drain(..written);
                    write_queue.push_front(chunk);
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    can_write = false;
                    write_queue.push_front(chunk);
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    write_queue.push_front(chunk);
                }
                Err(err) => {
                    log::error!("remote pty session {id}: write error: {err}");
                    break 'session force_kill_and_report(pty);
                }
            }
        }
    };

    let _ = events_tx.try_send((id, PtySessionEvent::Exited(exit_status)));
}

/// Force-terminates `pty` for a shutdown path that was not itself a child
/// exit (a poll/read/write error, or the model's receiver having gone away).
/// Mirrors `EventLoop::spawn`'s `if !child_exited { pty.kill() }`. The real
/// exit status can't be recovered here -- the process was killed by us, not
/// observed exiting on its own -- so this always reports `signalled()`.
fn force_kill_and_report(pty: Pty) -> SessionExitStatus {
    if let Err(err) = pty.kill() {
        log::error!("failed to force-kill a remote pty session: {err:#}");
    }
    SessionExitStatus::signalled()
}
