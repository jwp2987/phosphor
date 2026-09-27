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
//! # Deadline
//!
//! The first signal also arms a watchdog thread that exits the process after
//! [`SHUTDOWN_DEADLINE`], so a wedged flush cannot make the app ignore `SIGTERM`.
//! A further signal while the shutdown is in flight exits immediately.

use std::time::Duration;

/// How long a signal-initiated graceful shutdown may take before the process
/// exits regardless.
pub(crate) const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);

/// What the handler thread should do in response to one delivered signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignalResponse {
    /// The first termination signal: ask the main loop to shut down gracefully.
    BeginGracefulShutdown,
    /// A signal arrived while a graceful shutdown was already underway: the
    /// sender is insisting, so exit now.
    ExitImmediately,
}

/// Remembers whether a signal-initiated shutdown is already in flight.
#[derive(Debug, Default)]
pub(crate) struct ShutdownState {
    shutdown_requested: bool,
}

impl ShutdownState {
    pub(crate) fn on_signal(&mut self) -> SignalResponse {
        if std::mem::replace(&mut self.shutdown_requested, true) {
            SignalResponse::ExitImmediately
        } else {
            SignalResponse::BeginGracefulShutdown
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
}

/// The conventional exit status for a process ended by `signal`.
pub(crate) fn exit_code_for_signal(signal: i32) -> i32 {
    128 + signal
}

/// Handles one delivered termination signal. Runs on the signal-handling
/// thread, never in signal context.
pub(crate) fn handle_signal(state: &mut ShutdownState, signal: i32, hooks: &impl ShutdownHooks) {
    let exit_code = exit_code_for_signal(signal);
    match state.on_signal() {
        SignalResponse::BeginGracefulShutdown => {
            log::info!(
                "Received termination signal {signal}; shutting down gracefully (deadline {}s)",
                SHUTDOWN_DEADLINE.as_secs()
            );
            hooks.arm_deadline(SHUTDOWN_DEADLINE, exit_code);
            if !hooks.request_terminate() {
                log::warn!("Main loop is gone; exiting without a graceful shutdown");
                hooks.exit(exit_code);
            }
        }
        SignalResponse::ExitImmediately => {
            log::warn!("Received termination signal {signal} during shutdown; exiting now");
            hooks.exit(exit_code);
        }
    }
}

/// Waits out `deadline` with `sleep`, then ends the process with `exit`. The
/// real watchdog passes `std::thread::sleep` and `std::process::exit`; tests
/// inject fakes.
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

/// [`ShutdownHooks`] that act on the real process.
pub(crate) struct ProcessHooks<F> {
    request_terminate: F,
}

impl<F: Fn() -> bool> ProcessHooks<F> {
    pub(crate) fn new(request_terminate: F) -> Self {
        Self { request_terminate }
    }
}

impl<F: Fn() -> bool> ShutdownHooks for ProcessHooks<F> {
    fn request_terminate(&self) -> bool {
        (self.request_terminate)()
    }

    fn arm_deadline(&self, deadline: Duration, exit_code: i32) {
        let spawned = std::thread::Builder::new()
            .name("shutdown-deadline".to_string())
            .spawn(move || {
                run_deadline_watchdog(deadline, exit_code, std::thread::sleep, |code| {
                    std::process::exit(code)
                })
            });
        if let Err(err) = spawned {
            log::warn!("Failed to spawn the shutdown deadline watchdog: {err}");
        }
    }

    fn exit(&self, exit_code: i32) {
        std::process::exit(exit_code);
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
                handle_signal(&mut state, signal, &hooks);
            }
        })?;
    Ok(())
}

#[cfg(test)]
#[path = "termination_signals_tests.rs"]
mod tests;
