//! Turns process termination signals into a graceful, non-cancellable app quit
//! (jwp2987/phosphor#685).
//!
//! Without this, `SIGTERM` / `SIGHUP` take their default disposition and kill the
//! process outright, skipping every step of `app_will_terminate` (notebook flush,
//! persistence-writer drain and WAL checkpoint, language-server and pty teardown).
//!
//! # Signal safety
//!
//! No app work ever runs in signal context. On Unix, [`signal_hook::iterator::Signals`]
//! installs a handler that only writes to a self-pipe; a dedicated thread reads the
//! pipe and calls [`handle_signal`], which does nothing but post a
//! `TerminationMode::ForceTerminate` request to the main loop through a thread-safe
//! handle supplied by the platform (a winit `EventLoopProxy`, the headless loop's
//! channel, or the main dispatch queue on macOS). The main loop then runs the same
//! shutdown path as any other non-cancellable quit.
//!
//! # Deadline and escalation
//!
//! The first signal also arms a watchdog thread that exits the process after
//! [`SHUTDOWN_DEADLINE`], so a wedged flush cannot make the app ignore `SIGTERM`.
//!
//! A further signal only cuts the graceful shutdown short when it is a repeat of a
//! signal already received, at least [`ESCALATION_MIN_INTERVAL`] after the first
//! one, and not `SIGHUP`. Closing the launching terminal delivers `SIGHUP` twice in
//! quick succession (the shell forwards it to its jobs, then the kernel sends it again
//! when the session leader exits), and logind sends `SIGTERM` then `SIGHUP`; neither
//! is anyone insisting, and escalating on them would skip `app_will_terminate`.
//! (jwp2987/phosphor#685 review.)
//!
//! # Exiting
//!
//! The watchdog and an escalated exit end the process with [`hard_exit`]
//! (`_exit(2)` / `TerminateProcess`), not `std::process::exit`: the main thread is
//! mid-shutdown, and running `atexit` handlers and C++ static destructors (GPU
//! drivers, SQLite) concurrently with it risks a crash. Once a signal-initiated
//! shutdown has completed, [`exit_after_signal_shutdown`] re-raises the signal
//! with its default disposition, so the parent sees the conventional "terminated
//! by SIGTERM" status rather than a clean exit.

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use instant::Instant;

/// How long a signal-initiated graceful shutdown may take before the process
/// exits regardless.
pub(crate) const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);

/// How long after the first delivery of a signal a repeat of it must arrive to be
/// taken as "exit now" rather than as a duplicate delivery.
pub(crate) const ESCALATION_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// `SIGHUP`'s number (1 on every Unix; also used for the conventional exit status).
#[cfg(unix)]
pub(crate) const SIGHUP: i32 = libc::SIGHUP;
#[cfg(not(unix))]
pub(crate) const SIGHUP: i32 = 1;

/// What the handler thread should do in response to one delivered signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignalResponse {
    /// The first termination signal: ask the main loop to shut down gracefully.
    BeginGracefulShutdown,
    /// A duplicate or different signal during the shutdown: let it finish.
    Ignore,
    /// The same signal again, long enough after the first: the sender is
    /// insisting, so exit now.
    ExitImmediately,
}

/// Remembers which signals have arrived, and when, since a signal-initiated
/// shutdown began.
#[derive(Debug, Default)]
pub(crate) struct ShutdownState {
    /// Each distinct signal received, with the time it was first received. Non-empty
    /// once a shutdown has been requested.
    first_received: Vec<(i32, Instant)>,
}

impl ShutdownState {
    pub(crate) fn on_signal(&mut self, signal: i32, now: Instant) -> SignalResponse {
        if self.first_received.is_empty() {
            self.first_received.push((signal, now));
            return SignalResponse::BeginGracefulShutdown;
        }
        if signal == SIGHUP {
            return SignalResponse::Ignore;
        }
        match self.first_received.iter().find(|(seen, _)| *seen == signal) {
            Some(&(_, first))
                if now.saturating_duration_since(first) >= ESCALATION_MIN_INTERVAL =>
            {
                SignalResponse::ExitImmediately
            }
            Some(_) => SignalResponse::Ignore,
            None => {
                self.first_received.push((signal, now));
                SignalResponse::Ignore
            }
        }
    }
}

/// The side effects of handling a signal, separated from the decision logic so
/// tests can drive [`handle_signal`] without delivering real signals, spawning
/// watchdogs, or exiting the test process.
pub(crate) trait ShutdownHooks {
    /// Posts a non-cancellable terminate request to the main loop. Returns
    /// `false` if the main loop is gone and the request could not be delivered.
    fn request_terminate(&self) -> bool;
    /// Arms a watchdog that exits the process with `exit_code` after `deadline`.
    fn arm_deadline(&self, deadline: Duration, exit_code: i32);
    /// Exits the process immediately.
    fn exit(&self, exit_code: i32);
    /// Remembers that `signal` started the shutdown, for
    /// [`exit_after_signal_shutdown`].
    fn record_initiating_signal(&self, signal: i32);
}

/// The conventional exit status for a process ended by `signal`.
pub(crate) fn exit_code_for_signal(signal: i32) -> i32 {
    128 + signal
}

/// Handles one delivered termination signal. Runs on the signal-handling
/// thread, never in signal context.
pub(crate) fn handle_signal(
    state: &mut ShutdownState,
    signal: i32,
    now: Instant,
    hooks: &impl ShutdownHooks,
) {
    let exit_code = exit_code_for_signal(signal);
    match state.on_signal(signal, now) {
        SignalResponse::BeginGracefulShutdown => {
            log::info!(
                "Received termination signal {signal}; shutting down gracefully (deadline {}s)",
                SHUTDOWN_DEADLINE.as_secs()
            );
            hooks.record_initiating_signal(signal);
            hooks.arm_deadline(SHUTDOWN_DEADLINE, exit_code);
            if !hooks.request_terminate() {
                log::warn!("Main loop is gone; exiting without a graceful shutdown");
                hooks.exit(exit_code);
            }
        }
        SignalResponse::Ignore => {
            log::info!(
                "Received termination signal {signal} during shutdown; letting the graceful \
                 shutdown finish"
            );
        }
        SignalResponse::ExitImmediately => {
            log::warn!("Received termination signal {signal} again during shutdown; exiting now");
            hooks.exit(exit_code);
        }
    }
}

/// Waits out `deadline` with `sleep`, then ends the process with `exit`. The
/// real watchdog passes `std::thread::sleep` and [`hard_exit`]; tests inject
/// fakes.
pub(crate) fn run_deadline_watchdog(
    deadline: Duration,
    exit_code: i32,
    sleep: impl FnOnce(Duration),
    exit: impl FnOnce(i32),
) {
    sleep(deadline);
    log::error!(
        "Graceful shutdown did not finish within {}s; exiting",
        deadline.as_secs()
    );
    exit(exit_code);
}

/// Ends the process now with `exit_code`, without running `atexit` handlers or
/// static destructors: `_exit(2)` on Unix, `TerminateProcess` on Windows.
///
/// Used where the main thread may be mid-shutdown (the deadline watchdog, an
/// escalated exit), where `std::process::exit` would run those destructors
/// concurrently with it. The logger is flushed first; that only takes the logger's
/// own lock, which the main thread never holds for long.
#[allow(unreachable_code)]
pub(crate) fn hard_exit(exit_code: i32) {
    log::logger().flush();
    #[cfg(unix)]
    // SAFETY: `_exit` is async-signal-safe and never returns.
    unsafe {
        libc::_exit(exit_code)
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        // SAFETY: terminating our own process through its pseudo-handle.
        let _ = unsafe { TerminateProcess(GetCurrentProcess(), exit_code as u32) };
    }
    std::process::exit(exit_code)
}

/// The signal that started the current shutdown, or 0.
static INITIATING_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// How the process should end once a shutdown has run to completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalExit {
    /// Exit normally with this status.
    Status(i32),
    /// Die of this signal, as the parent expects of a process sent it.
    Reraise(i32),
}

/// How to end the process after a completed shutdown, given the signal (if any)
/// that started it.
pub(crate) fn final_exit(initiating_signal: Option<i32>) -> FinalExit {
    match initiating_signal {
        Some(signal) if cfg!(unix) => FinalExit::Reraise(signal),
        Some(signal) => FinalExit::Status(exit_code_for_signal(signal)),
        None => FinalExit::Status(0),
    }
}

fn initiating_signal() -> Option<i32> {
    match INITIATING_SIGNAL.load(Ordering::Acquire) {
        0 => None,
        signal => Some(signal),
    }
}

/// If a termination signal started this shutdown, ends the process the way that
/// signal would have (default disposition, re-raised) now that `app_will_terminate`
/// has finished; otherwise returns so the caller exits as usual.
///
/// Called by platform loops right after `app_will_terminate` where the process is
/// about to exit anyway (winit, macOS). Not by the headless loop itself
/// (`headless::event_loop::run`), which still has to return control to its caller
/// to restore the terminal (the TUI) or otherwise wind down first; that caller
/// (`headless::app::App::run`) calls this once `event_loop::run` returns
/// (jwp2987/phosphor#717).
pub(crate) fn exit_after_signal_shutdown() {
    match final_exit(initiating_signal()) {
        FinalExit::Status(0) => {}
        FinalExit::Status(code) => hard_exit(code),
        FinalExit::Reraise(signal) => {
            log::info!("Graceful shutdown after signal {signal} finished; re-raising it");
            log::logger().flush();
            #[cfg(unix)]
            // SAFETY: restoring the default disposition and raising the signal on
            // ourselves; if that somehow returns, `hard_exit` ends the process.
            unsafe {
                libc::signal(signal, libc::SIG_DFL);
                libc::raise(signal);
            }
            hard_exit(exit_code_for_signal(signal));
        }
    }
}

/// [`ShutdownHooks`] that act on the real process.
pub(crate) struct ProcessHooks<F> {
    request_terminate: F,
    /// How the watchdog and an escalated exit end the process: [`hard_exit`]
    /// outside tests.
    exit: fn(i32),
}

impl<F: Fn() -> bool> ProcessHooks<F> {
    pub(crate) fn new(request_terminate: F) -> Self {
        Self {
            request_terminate,
            exit: hard_exit,
        }
    }
}

impl<F: Fn() -> bool> ShutdownHooks for ProcessHooks<F> {
    fn request_terminate(&self) -> bool {
        (self.request_terminate)()
    }

    fn arm_deadline(&self, deadline: Duration, exit_code: i32) {
        let exit = self.exit;
        let spawned = std::thread::Builder::new()
            .name("shutdown-deadline".to_string())
            .spawn(move || run_deadline_watchdog(deadline, exit_code, std::thread::sleep, exit));
        if let Err(err) = spawned {
            log::warn!("Failed to spawn the shutdown deadline watchdog: {err}");
        }
    }

    fn exit(&self, exit_code: i32) {
        (self.exit)(exit_code);
    }

    fn record_initiating_signal(&self, signal: i32) {
        INITIATING_SIGNAL.store(signal, Ordering::Release);
    }
}

/// Signals that end a GUI app: `SIGTERM` (kill, logout, `systemctl stop`) and
/// `SIGHUP` (the launching terminal or session went away). `SIGINT` is left at
/// its default for GUI builds.
#[cfg(unix)]
pub(crate) const GUI_TERMINATION_SIGNALS: &[i32] =
    &[signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP];

/// Signals that end a headless / TUI app: the GUI set plus `SIGINT`, which the
/// headless loop has always handled gracefully.
#[cfg(unix)]
pub(crate) const HEADLESS_TERMINATION_SIGNALS: &[i32] = &[
    signal_hook::consts::SIGINT,
    signal_hook::consts::SIGTERM,
    signal_hook::consts::SIGHUP,
];

/// Returns the subset of `signals` whose disposition was not inherited as
/// `SIG_IGN`. `nohup` and job-control shells launch processes with `SIGHUP` /
/// `SIGINT` ignored on purpose; installing a handler would silently undo that.
#[cfg(unix)]
fn signals_not_ignored(signals: &[i32]) -> Vec<i32> {
    signals
        .iter()
        .copied()
        .filter(|&signal| !is_ignored(signal))
        .collect()
}

#[cfg(unix)]
fn is_ignored(signal: i32) -> bool {
    let mut current = std::mem::MaybeUninit::<libc::sigaction>::zeroed();
    // SAFETY: a null `act` only queries the current disposition into `current`.
    let rc = unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) };
    // SAFETY: `sigaction` fully initialised `current` when it returned 0.
    rc == 0 && unsafe { current.assume_init() }.sa_sigaction == libc::SIG_IGN
}

/// Installs handlers for `signals` that request a graceful, non-cancellable
/// shutdown through `request_terminate`, which must be safe to call from a
/// background thread and must only enqueue work for the main loop.
#[cfg(unix)]
pub(crate) fn install(
    signals: &[i32],
    request_terminate: impl Fn() -> bool + Send + 'static,
) -> std::io::Result<()> {
    let signals = signals_not_ignored(signals);
    if signals.is_empty() {
        return Ok(());
    }
    let mut delivered = signal_hook::iterator::Signals::new(&signals)?;
    std::thread::Builder::new()
        .name("termination-signals".to_string())
        .spawn(move || {
            let hooks = ProcessHooks::new(request_terminate);
            let mut state = ShutdownState::default();
            for signal in delivered.forever() {
                handle_signal(&mut state, signal, Instant::now(), &hooks);
            }
        })?;
    Ok(())
}

#[cfg(test)]
#[path = "termination_signals_tests.rs"]
mod tests;
