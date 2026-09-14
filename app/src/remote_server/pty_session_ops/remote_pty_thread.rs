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
use warp_core::SessionId;
use warpui::AppContext;

use crate::terminal::SizeInfo;
use crate::terminal::local_tty::shell::{
    DirectShellStarter, ShellStarter, supported_shell_path_and_type,
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

/// Kills and reaps a spawned pty (`EventedPty::kill`, in `unix.rs`, which
/// drops the fd and blocks on `Child::wait`) when dropped, unless
/// [`disarm`](Self::disarm)ed first.
///
/// Guards every fallible step in [`LiveSession::spawn_with_shell_starter`]
/// between a successful `Pty::new` and the point where the dedicated session
/// thread actually takes ownership of the pty. Neither `Pty` nor
/// `DirectPtyHandle` has a `Drop` impl -- both are shared with the local
/// terminal, whose lifecycle differs, so giving them one here would change
/// shared behavior out of scope for this fix -- and `std::process::Child`
/// does not kill its child on drop. Without this guard, any of `Poll::new`,
/// `Waker::new`, `pty.register`, or `thread::Builder::spawn` failing after a
/// successful spawn would leak a live, never-`wait()`-ed child process
/// (eventually a zombie) while the caller is told the spawn failed. A guard
/// that must be explicitly disarmed -- rather than a `kill()` call repeated
/// in each fallible branch -- means a new early return added later is
/// cleaned up automatically instead of silently reintroducing the leak.
struct KillPtyOnDrop(Option<Pty>);

impl KillPtyOnDrop {
    fn new(pty: Pty) -> Self {
        Self(Some(pty))
    }

    fn pty_mut(&mut self) -> &mut Pty {
        self.0
            .as_mut()
            .expect("KillPtyOnDrop is armed until disarmed")
    }

    /// Hands the pty back, disarming the guard. Call this only once the pty's
    /// ownership has genuinely transferred to whatever manages its teardown
    /// from here on -- inside this file, that is the session thread itself,
    /// via `run_session_loop`'s `force_kill_and_report` or its normal exit
    /// path.
    fn disarm(mut self) -> Pty {
        self.0
            .take()
            .expect("KillPtyOnDrop is armed until disarmed")
    }
}

impl Drop for KillPtyOnDrop {
    fn drop(&mut self) {
        if let Some(pty) = self.0.take() {
            if let Err(err) = pty.kill() {
                log::error!(
                    "failed to kill an orphaned remote pty after an aborted session spawn: {err:#}"
                );
            }
        }
    }
}

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
        let shell_starter = resolve_shell_starter(
            spec.shell.as_deref(),
            spec.no_bootstrap,
            spec.bootstrap_session_id,
        )?;
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
        // Computed here because `shell_starter` moves into `PtyOptions` below.
        let init_script_writes = session_init_script_writes(
            &shell_starter,
            spec.no_bootstrap,
            spec.bootstrap_session_id,
        );

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
            // None of these five removes the bootstrap; `spec.no_bootstrap` is
            // the only thing that does, and it does it by emptying the shell
            // starter's args (`DirectShellStarter::without_bootstrap`). Two of
            // them describe how to *reach* a host -- the ControlMaster SSH
            // wrapper and its connection reuse -- which is meaningless for a
            // daemon already running on the host it is spawning on. The other
            // three (`shell_debug_mode`, `honor_ps1`,
            // `node_version_chip_enabled`) become `WARP_*` environment
            // variables that the injected bootstrap script reads, and are
            // `false` because the daemon has no settings service to ask for a
            // user's preference -- unlike `terminal_manager.rs`, which reads
            // each from `SessionSettings`. False is the conservative answer for
            // all three: each one only ever *adds* per-prompt behaviour. With
            // `no_bootstrap` set there is no script to read them at all.
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
        let pty = Pty::new(options, false, ctx)
            .map_err(|err| PtySessionOpError::new(format!("failed to spawn pty: {err:#}")))?;
        let pid = pty.get_pid();

        // From here on, `pty` is a live child process with nothing yet
        // `wait()`ing on it. Every remaining fallible step is guarded so an
        // early return still kills and reaps it instead of leaking it -- see
        // `KillPtyOnDrop`'s doc comment.
        let mut pty_guard = KillPtyOnDrop::new(pty);

        let poll = Poll::new()
            .map_err(|err| PtySessionOpError::new(format!("failed to create mio poll: {err}")))?;
        let waker = Waker::new(poll.registry(), COMMAND_TOKEN)
            .map_err(|err| PtySessionOpError::new(format!("failed to create mio waker: {err}")))?;
        pty_guard
            .pty_mut()
            .register(&poll, Interest::READABLE | Interest::WRITABLE)
            .map_err(|err| PtySessionOpError::new(format!("failed to register pty: {err}")))?;

        let commands = Arc::new(CommandChannel {
            queue: Mutex::new(VecDeque::new()),
            waker,
        });
        let thread_commands = commands.clone();

        let thread = std::thread::Builder::new()
            .name("remote-pty".to_string())
            .spawn(move || {
                // Disarmed only here, inside the closure the OS thread
                // actually runs: if `spawn` below fails to create the
                // thread, this closure is dropped without ever running, so
                // `pty_guard` -- still armed -- kills and reaps the pty on
                // that drop instead of leaking it.
                let pty = pty_guard.disarm();
                run_session_loop(id, pty, poll, thread_commands, events_tx)
            })
            .map_err(|err| {
                PtySessionOpError::new(format!("failed to spawn pty reader thread: {err}"))
            })?;

        let session = Self {
            commands,
            pid,
            thread,
        };

        // Queued as this session's first stdin writes, before the handle
        // reaches any caller -- so nothing a client sends can get in front of
        // the bootstrap. `TerminalManager::enqueue_init_script` has the same
        // ordering guarantee by construction: it writes into the event loop's
        // channel before the pty is even created.
        for write in init_script_writes {
            session.write_stdin(write);
        }

        Ok(session)
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
/// `ShellStarter`, falling back to the same passwd-entry-then-bash/zsh/fish
/// resolution `local_tty::shell::ShellStarter::compute_fallback_shell` uses
/// for a local session -- *not* `$SHELL`, which is a materially weaker
/// signal here: a daemon started by a service manager has no inherited
/// login shell, so `$SHELL` can be unset or stale in a way it never is for a
/// terminal spawned from an interactive login.
///
/// Calls that function directly (widened to `pub(crate)`) rather than
/// reimplementing it, so this can never again drift from what a local
/// session resolves. `ShellStarter::init` (`local_tty`'s other, and only
/// other public, entry point) isn't reusable here -- it needs an
/// `AvailableShells` model and a settings service, neither of which the
/// daemon has -- but `compute_fallback_shell` needs neither.
///
/// Then, if the request set `no_bootstrap` (`PtySpawnSpec::no_bootstrap`),
/// strips Warp's interactive bootstrap from whatever was resolved.
///
/// Stripping happens here, after resolution, rather than inside it: both
/// branches below reach `arguments_for_session_spawning_command` (directly via
/// `for_explicit_shell`, or inside `compute_fallback_shell`), so a per-branch
/// opt-out would have to be threaded through `compute_fallback_shell`'s three
/// fallback paths -- settings-driven discovery shared with every local session
/// -- to express something only this daemon asks for. Discarding the arguments
/// afterwards is the same result with none of that reach.
fn resolve_shell_starter(
    spec_shell: Option<&str>,
    no_bootstrap: bool,
    bootstrap_session_id: Option<SessionId>,
) -> Result<ShellStarter, PtySessionOpError> {
    // NB: `bootstrap_session_id` is used twice below -- once to decide whether a
    // shell needing an out-of-band script can be bootstrapped at all, and once to
    // rebind argv for the shells that carry their init there.
    let starter = if let Some(shell) = spec_shell {
        let (path, shell_type) = supported_shell_path_and_type(shell)
            .ok_or_else(|| PtySessionOpError::new(format!("unsupported shell: {shell}")))?;
        ShellStarter::Direct(DirectShellStarter::for_explicit_shell(path, shell_type))
    } else {
        ShellStarter::compute_fallback_shell()
            .map(ShellStarter::from)
            .ok_or_else(|| {
                PtySessionOpError::new(
                    "no supported shell found on this host (this user's passwd entry, /bin/zsh, \
                     /bin/bash, /bin/fish all missing or unsupported)",
                )
            })?
    };

    // Stripped for two different reasons that reach the same place.
    //
    // The first is the request: `no_bootstrap` asked for a plain shell.
    //
    // The second is capability, and it is the one that took a refutation pass to
    // see. A shell needing an out-of-band init script
    // (`ShellStarter::needs_out_of_band_init_script`) is given `--no-rcs` in
    // argv, suppressing the user's own startup files on the promise that the
    // script will arrive and complete a handshake. That handshake is only
    // answered if the *client* has registered the same session id with its
    // `TerminalModel` -- and this daemon has no way to tell it one: nothing
    // carries an id on the wire, and `SpawnSessionSuccess` is empty. So with no
    // `bootstrap_session_id` in hand, queueing the script would leave zsh with
    // neither the user's configuration nor Warp's, and with `ZLE` disabled by
    // the script's first line and re-enabled only by a body that never arrives
    // -- strictly worse than not bootstrapping at all.
    //
    // Spawning plainly instead gives the user a working shell reading their own
    // `~/.zshrc`, which is what "the daemon cannot bootstrap this shell" should
    // look like. When the client seam can supply an id, passing one here turns
    // the real bootstrap back on with no change to this rule.
    let cannot_bootstrap =
        starter.needs_out_of_band_init_script() && bootstrap_session_id.is_none();
    if !no_bootstrap && !cannot_bootstrap {
        // Bind the shell's handshake to the id the *client* registered. This has
        // to rebuild argv, not just the out-of-band script: bash, fish and
        // PowerShell carry their init -- and the id inside it -- in their
        // arguments, so supplying an id only to `init_script_stdin_writes` would
        // fix zsh and leave those three emitting hooks against an id nothing
        // knows. `DProtoHook::requires_registered_session` is true for every
        // hook the bootstrap emits, so those sessions would render as bare
        // shells with a warning per hook.
        return Ok(match (starter, bootstrap_session_id) {
            (ShellStarter::Direct(direct), Some(session_id)) => {
                ShellStarter::Direct(direct.with_bootstrap_session_id(session_id))
            }
            // No id: `cannot_bootstrap` above already sent every shell that
            // needs an out-of-band script down the plain path, so what reaches
            // here is bash/fish/PowerShell keeping the id their starter minted.
            // Their hooks will be rejected until a client supplies one -- the
            // documented state of a daemon session today, not a new defect.
            (starter, _) => starter,
        });
    }
    if cannot_bootstrap && !no_bootstrap {
        log::info!(
            "no bootstrap session id for this daemon-spawned shell; starting it plainly rather \
             than half-bootstrapped"
        );
    }

    match starter {
        ShellStarter::Direct(direct) => Ok(ShellStarter::Direct(direct.without_bootstrap())),
        // Unreachable on this backend: the module is `cfg(unix)`, and on unix
        // both branches above yield `Direct` (`compute_fallback_shell`'s unix
        // arm returns `UserDefault`/`Fallback`, and `From<ShellStarterSource>`
        // maps both to `Direct`). It fails rather than falling through because
        // the request was specifically "no bootstrap": silently spawning a
        // bootstrapped shell would hand a client that cannot read the InitShell
        // handshake a session full of escape noise, which is the exact failure
        // `no_bootstrap` exists to prevent. Refusing is recoverable and
        // legible; honouring the request in name only is neither.
        other => Err(PtySessionOpError::new(format!(
            "cannot strip the shell bootstrap from a {} starter, which this daemon was not \
             expected to resolve on unix",
            match other {
                ShellStarter::Direct(_) => "direct",
                ShellStarter::Wsl(_) => "WSL",
                ShellStarter::MSYS2(_) => "MSYS2",
                ShellStarter::DockerSandbox(_) => "Docker sandbox",
            }
        ))),
    }
}

/// The stdin writes that bootstrap this session's shell, in order, or none.
///
/// Which shells need one -- and with which session id -- is
/// [`ShellStarter::init_script_stdin_writes`], which lives beside the argument
/// builder that creates the asymmetry (zsh gets `--no-rcs` and nothing else;
/// bash, fish and PowerShell carry the same script in their own argv). This
/// adds only the one thing that is the daemon's own policy rather than the
/// shell's: `no_bootstrap`.
///
/// `no_bootstrap` suppresses the writes entirely. Such a session asked for a
/// bare shell, and `DirectShellStarter::without_bootstrap` has already emptied
/// the arguments -- so its zsh never sees `--no-rcs` either, and reads the
/// user's own `~/.zshrc` as it would under any other terminal. Writing the
/// script anyway would put the InitShell handshake into the session as raw
/// escape noise for a client that cannot read it, which is the exact failure
/// `no_bootstrap` exists to prevent.
fn session_init_script_writes(
    shell_starter: &ShellStarter,
    no_bootstrap: bool,
    bootstrap_session_id: Option<SessionId>,
) -> Vec<Vec<u8>> {
    if no_bootstrap {
        return Vec::new();
    }
    // No id means `resolve_shell_starter` already stripped the bootstrap for us,
    // so there is nothing to write and the shell is not expecting anything. The
    // check is repeated here rather than assumed because the two functions are
    // reachable independently: `spawn_with_shell_starter` is a test seam that
    // takes an already-resolved starter and never calls the resolver.
    let Some(session_id) = bootstrap_session_id else {
        return Vec::new();
    };

    shell_starter.init_script_stdin_writes(session_id)
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

/// Everything [`run_session_loop`] needs from the pty it drives.
///
/// Exists only because `take_exit_status` is an inherent method on
/// `local_tty::Pty` rather than part of `EventedPty`, so a loop written
/// against the trait alone cannot recover a real exit code. Widening
/// `EventedPty` itself would change a trait shared with the local terminal
/// (whose own event loop never reads an exit status at all -- see "Scope of
/// that fix" in `docs/design/moth-parliament.md`) to serve one caller here;
/// a local trait with a one-line impl does not.
///
/// The generic is load-bearing for tests: `run_session_loop`'s exit/output
/// ordering is a property of which mio events arrive in the *same* wakeup,
/// and provoking that with a real pty means racing a real child's write
/// against its own exit -- exactly the race
/// `pty_session_ops_tests.rs`'s real-pty test documents itself as avoiding
/// rather than winning. A fake whose readable and child-exit sources are
/// both armed before the first `poll()` makes that wakeup deterministic.
trait SessionPty: EventedPty {
    fn take_exit_status(&mut self) -> Option<std::process::ExitStatus>;
}

impl SessionPty for Pty {
    fn take_exit_status(&mut self) -> Option<std::process::ExitStatus> {
        // Inherent methods win name resolution, so this is `Pty`'s own
        // method, not an infinite recursion through the trait.
        Pty::take_exit_status(self)
    }
}

/// The pty thread's body. Mirrors `local_tty::event_loop::EventLoop::spawn`'s
/// shape (poll, drain commands, read while readable, write while writable,
/// stop on child-exit or a fatal I/O error) without the ANSI parser or
/// `TerminalModel` it drives locally.
///
/// One deliberate divergence from that shape: a child exit does *not* end the
/// loop on the spot. See `pending_exit` below.
fn run_session_loop<P: SessionPty>(
    id: RemotePtySessionId,
    mut pty: P,
    mut poll: Poll,
    commands: Arc<CommandChannel>,
    events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>,
) {
    let mut events = Events::with_capacity(128);
    let mut write_queue: VecDeque<Vec<u8>> = VecDeque::new();
    let mut can_read = false;
    let mut can_write = false;
    let mut read_buf = [0u8; READ_CHUNK_SIZE];
    // Set when the child's exit has been observed but not yet honoured: the
    // loop finishes draining readable output first, so the session's last
    // bytes are sent before its `Exited` event. See the child-event branch
    // below.
    let mut pending_exit: Option<SessionExitStatus> = None;

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
                    // Recorded, not acted on yet: one `poll()` wakeup can
                    // carry both this exit and the child's final readable
                    // output, and breaking here would discard whatever is
                    // still in the pty's buffer -- a shell that writes and
                    // immediately exits losing its last line, which is
                    // exactly the loss faithful output replay exists to
                    // prevent. The drain below runs first, so the last bytes
                    // reach `events_tx` before the `Exited` event does.
                    // (`local_tty::event_loop::EventLoop`, which this
                    // otherwise mirrors, still breaks here; its own comments
                    // acknowledge the same race.)
                    pending_exit = Some(status);
                    // Drain unconditionally rather than only when this
                    // wakeup also reported the pty readable: mio registers
                    // with edge-triggered epoll/kqueue, so readiness that
                    // was already consumed by an earlier iteration of this
                    // same wakeup is not reported again. One extra
                    // non-blocking read is a trivial price for not making
                    // the last line of a command depend on which readiness
                    // edges happened to coincide.
                    can_read = true;
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
                        // to. (The channel is unbounded, so a failed
                        // `try_send` means closed, never full.)
                        break 'session match pending_exit.take() {
                            // The child already exited on its own and its
                            // real status is in hand; force-killing an
                            // already-reaped process to report a fabricated
                            // signalled exit would throw that away.
                            Some(status) => status,
                            None => force_kill_and_report(pty),
                        };
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => can_read = false,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                // See `is_benign_pty_hangup_read_error`'s doc comment for why
                // this is not fatal. Let the loop come back around for the
                // `SIGCHLD`-driven `Exited` event above instead of
                // force-killing an already-exiting pty and fabricating a
                // signalled exit for what may be a clean one.
                //
                // `can_read = false` is what makes "come back around" true:
                // the EIO a hung-up pty master returns is *persistent*, so
                // leaving the flag set would keep this inner drain re-reading
                // the same error without ever reaching `poll()` again.
                //
                // It is load-bearing only now. An earlier version of this
                // comment said, in the past tense, that leaving the flag set
                // HAD caused a spinning thread. It had not: the classifier
                // above tested `ErrorKind::Other`, which never matches EIO
                // (it decodes to `Uncategorized`), so this arm never ran and
                // control never reached the branch that would have spun. See
                // `is_benign_pty_hangup_read_error` for the measurement, and
                // "RETRACTED 2026-09-13" in `docs/design/moth-parliament.md`
                // for why a plausible, internally-consistent story about a bug
                // that did not exist is worth recording rather than deleting.
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                Err(err) if is_benign_pty_hangup_read_error(&err) => can_read = false,
                Err(err) => {
                    log::error!("remote pty session {id}: read error: {err}");
                    // Prefers a held `pending_exit` for the same reason the
                    // receiver-gone branch above does, and the two were
                    // asymmetric until a refutation pass caught it: if the
                    // child has already exited on its own, its real status is
                    // in hand and `force_kill_and_report` would replace it with
                    // a fabricated `signalled()` while killing an
                    // already-reaped process. That asymmetry was not
                    // theoretical -- with the errno fix above it is only a
                    // guard, but on macOS the benign arm is `cfg`'d out
                    // entirely, so this is the *only* thing standing between a
                    // clean remote exit and a fabricated signalled one there.
                    break 'session pending_exit
                        .take()
                        .unwrap_or_else(|| force_kill_and_report(pty));
                }
            }
        }

        // After the drain, so the final output is already queued on
        // `events_tx` ahead of the `Exited` event sent below; before the
        // write drain, because there is no longer a process on the other end
        // of the pty to receive anything queued for it.
        if let Some(status) = pending_exit.take() {
            break 'session status;
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
fn force_kill_and_report<P: EventedPty>(pty: P) -> SessionExitStatus {
    if let Err(err) = pty.kill() {
        log::error!("failed to force-kill a remote pty session: {err:#}");
    }
    SessionExitStatus::signalled()
}

/// Whether `err`, from a pty-master `read`, is a benign side effect of the
/// slave side hanging up rather than a real I/O failure.
///
/// On Linux/FreeBSD, reading the master side of a pty commonly fails with
/// EIO once the slave side hangs up -- e.g. the shell exiting. Treating it as
/// fatal races the read against the child's own exit: `run_session_loop` would
/// force-kill an already-exiting pty and report a fabricated signalled exit,
/// discarding a real exit code that `take_exit_status` would otherwise have
/// recovered.
///
/// **Matched on the raw errno, not on `ErrorKind`, and that is a correction.**
/// This function previously tested `err.kind() == io::ErrorKind::Other`, copied
/// from `local_tty::event_loop::EventLoop::pty_read` along with a comment
/// asserting that EIO "has no dedicated `Eio` kind, so that surfaces as
/// `ErrorKind::Other`". It does not. `std`'s `decode_error_kind` has no mapping
/// for EIO, so it falls through to `ErrorKind::Uncategorized`, which is not
/// equal to `Other` -- measured on the pinned toolchain (1.92.0):
///
/// ```text
/// io::Error::from_raw_os_error(5).kind()  ==  Uncategorized
/// ...                            == Other ==  false
/// ```
///
/// So the arm never fired, and every clean exit fell through to the fatal arm
/// instead. `Uncategorized` is also explicitly unstable to match on by name,
/// which is why the errno is the right thing to test rather than a second
/// `ErrorKind` guess. `local_tty` still has the original check; it is
/// inherited upstream code, its loop never reads an exit status, and fixing it
/// is not this module's call -- but it means the two are no longer mirrors,
/// which is why this says so rather than still claiming to copy it.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn is_benign_pty_hangup_read_error(err: &io::Error) -> bool {
    err.raw_os_error() == Some(libc::EIO)
}

#[cfg(test)]
#[path = "remote_pty_thread_tests.rs"]
mod tests;
