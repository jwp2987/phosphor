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
//! shutdown has completed, [`exit_after_signal_shutdown_for`] re-raises the signal
//! with its default disposition, so the parent sees the conventional "terminated
//! by SIGTERM" status rather than a clean exit -- but only for a request each
//! main-loop variant has itself attributed to that signal (jwp2987/phosphor#726,
//! jwp2987/phosphor#791), never merely because some signal was received at some
//! point during the process's life.

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
    /// Posts a non-cancellable terminate request, carrying which signal asked for
    /// it, to the main loop. Returns `false` if the main loop is gone and the
    /// request could not be delivered.
    ///
    /// A loop that can also reach its exit path for a reason other than this
    /// specific request (winit: a key binding or menu quit can race a concurrent
    /// signal) should record `signal` itself, scoped to *this* request, rather than
    /// relying solely on [`Self::record_initiating_signal`]'s process-wide latch --
    /// see [`exit_after_signal_shutdown_for`] (jwp2987/phosphor#726).
    fn request_terminate(&self, signal: i32) -> bool;
    /// Arms a watchdog that exits the process with `exit_code` after `deadline`.
    fn arm_deadline(&self, deadline: Duration, exit_code: i32);
    /// Exits the process immediately.
    fn exit(&self, exit_code: i32);
    /// Remembers that `signal` started the shutdown, for the latch-reading
    /// [`exit_after_signal_shutdown`]. Kept as a fallback entry point: winit,
    /// macOS, and the headless loop now all track their own request's
    /// attribution instead and call [`exit_after_signal_shutdown_for`] directly
    /// (jwp2987/phosphor#726, jwp2987/phosphor#791), so no current caller actually
    /// reads this latch, but a future main-loop variant that cannot yet
    /// distinguish its own request's signal still can.
    fn record_initiating_signal(&self, signal: i32);
}

/// The conventional exit status for a process ended by `signal`.
pub(crate) fn exit_code_for_signal(signal: i32) -> i32 {
    128 + signal
}

/// Maps a Windows console control event -- as delivered to a
/// `SetConsoleCtrlHandler` callback -- to the synthetic "signal" number that
/// drives the same [`ShutdownState`] machine `SIGTERM`/`SIGHUP` already use, or
/// `None` for an event this module does not own (jwp2987/phosphor#773, the
/// Windows follow-up to #685).
///
/// `CTRL_C_EVENT` (0) and `CTRL_BREAK_EVENT` (1) are excluded on purpose:
/// Windows calls every registered console control handler for every event
/// type regardless of install order, and the headless loop's own `ctrlc`-based
/// handler already owns Ctrl-C/Ctrl-Break (mapped to `SIGINT`, installed
/// separately in `headless::event_loop::setup_signal_handler`); this handler
/// must return `None` for them so it never races that one.
///
/// Deliberately free of the `windows` crate -- the numbers are
/// `windows::Win32::System::Console`'s own `CTRL_CLOSE_EVENT` (2),
/// `CTRL_LOGOFF_EVENT` (5) and `CTRL_SHUTDOWN_EVENT` (6), duplicated here as
/// plain `u32`s -- so this mapping, unlike the rest of the console-handler
/// path, builds and is unit-tested on every platform, not only Windows.
///
/// Its only non-test caller is [`console::console_ctrl_handler`], which is
/// `#[cfg(windows)]`; a non-Windows build that does not compile `#[cfg(test)]`
/// code (e.g. `cargo check` without `--tests`) sees no caller at all, so this
/// needs its own `dead_code` opt-out there rather than relying on tests to
/// keep it "used".
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn ctrl_event_shutdown_reason(ctrl_type: u32) -> Option<i32> {
    const CTRL_CLOSE_EVENT: u32 = 2;
    const CTRL_LOGOFF_EVENT: u32 = 5;
    const CTRL_SHUTDOWN_EVENT: u32 = 6;
    match ctrl_type {
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => Some(ctrl_type as i32),
        _ => None,
    }
}

/// What a Windows GUI window's `WM_QUERYENDSESSION` / `WM_ENDSESSION` subclass
/// callback should do for one such message (jwp2987/phosphor#773, the GUI
/// follow-up to the console path above, which has no equivalent window to
/// subclass -- a console app has no HWND).
///
/// A winit app can own several top-level windows, and Windows sends
/// `WM_ENDSESSION` to *every one of them* for a single real shutdown attempt --
/// so `already_shutting_down` must be a process-wide latch the caller threads
/// through each window's callback, not per-window state, or a two-window app
/// would post two terminate requests for the one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionEndAction {
    /// `WM_QUERYENDSESSION`: tell Windows this process may need a moment to
    /// clean up (`ShutdownBlockReasonCreate`) and allow the session to end --
    /// this fork never vetoes a shutdown/logoff/close-all-programs from here.
    BlockAndAllow,
    /// The first `WM_ENDSESSION` with `wParam` true that any window has seen
    /// since the latch was last clear: start the graceful shutdown, carrying
    /// which of the two reasons [`ctrl_event_shutdown_reason`]'s synthetic
    /// signal space `WM_ENDSESSION`'s `lParam` can distinguish.
    BeginGracefulShutdown { reason_signal: i32 },
    /// A later `WM_ENDSESSION` true while that shutdown is already in flight --
    /// another window's copy of the same one real event, not a second attempt.
    AlreadyShuttingDown,
    /// `WM_ENDSESSION` with `wParam` false: a *different* application vetoed
    /// the session end, so this one was cancelled. The caller should release
    /// its shutdown block and clear the latch, so a later, successful attempt
    /// still starts a shutdown instead of being mistaken for a continuation of
    /// the cancelled one.
    Cancelled,
}

/// The pure decision behind [`SessionEndAction`]: given which message arrived
/// (`is_query_end_session` for `WM_QUERYENDSESSION`, else `WM_ENDSESSION`),
/// `WM_ENDSESSION`'s own `wParam`/`lParam` (`session_ending`, `is_logoff`), and
/// whether a shutdown this message could be part of has already been started
/// (`already_shutting_down`), decide what to do. Free of the `windows` crate --
/// like [`ctrl_event_shutdown_reason`] -- so it builds and is unit-tested on
/// every platform, not only Windows; its only non-test caller is
/// `windowing::winit::windows::session_end`'s subclass procedure.
///
/// `is_logoff` maps to [`ctrl_event_shutdown_reason`]'s `CTRL_LOGOFF_EVENT` (5),
/// otherwise to its `CTRL_SHUTDOWN_EVENT` (6) -- `WM_ENDSESSION`'s `lParam` has
/// no flag that distinguishes an actual shutdown/restart from the user picking
/// "Close all programs and shut down" in the Shutdown Event Tracker, so both
/// collapse to the one reason code already used for every non-logoff console
/// close/shutdown event. Reusing the same numbers means the GUI's
/// `WM_ENDSESSION` path and the headless/console `CTRL_*` path exit with the
/// same conventional status for the same real-world event.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn session_end_action(
    is_query_end_session: bool,
    session_ending: bool,
    is_logoff: bool,
    already_shutting_down: bool,
) -> SessionEndAction {
    const CTRL_LOGOFF_EVENT: i32 = 5;
    const CTRL_SHUTDOWN_EVENT: i32 = 6;

    if is_query_end_session {
        return SessionEndAction::BlockAndAllow;
    }
    if !session_ending {
        return SessionEndAction::Cancelled;
    }
    if already_shutting_down {
        return SessionEndAction::AlreadyShuttingDown;
    }
    SessionEndAction::BeginGracefulShutdown {
        reason_signal: if is_logoff {
            CTRL_LOGOFF_EVENT
        } else {
            CTRL_SHUTDOWN_EVENT
        },
    }
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
            if !hooks.request_terminate(signal) {
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
/// Consults [`INITIATING_SIGNAL`], which [`ProcessHooks::record_initiating_signal`]
/// sets the moment *any* termination signal is first received -- regardless of
/// whether that signal is what actually ends up driving the app to quit. A caller
/// that can end up here for a reason other than that specific signal (a key
/// binding, menu, or TUI exit action's quit can race a concurrent SIGTERM/SIGHUP
/// and still reach the same exit path) must not use this global-state version, or
/// it will re-raise a signal that had nothing to do with its own quit
/// (jwp2987/phosphor#726). Such callers should track their own request's signal,
/// if any, and call [`exit_after_signal_shutdown_for`] with it instead -- as
/// winit, macOS, and the headless loop all now do (jwp2987/phosphor#791), which
/// is why nothing in this crate currently calls this latch-reading version; it
/// is kept as a fallback entry point for a future main-loop variant that cannot
/// yet distinguish its own request's signal (jwp2987/phosphor#717 has the
/// headless-loop-specific reason a caller can't always call this immediately
/// after `app_will_terminate`: it has to return control to its caller to
/// restore the terminal first).
// Kept as a documented fallback entry point even though nothing in this crate
// currently calls it (see the doc comment above) -- every actual caller has been
// scoped to `exit_after_signal_shutdown_for` (jwp2987/phosphor#791).
#[allow(dead_code)]
pub(crate) fn exit_after_signal_shutdown() {
    exit_after_signal_shutdown_for(initiating_signal());
}

/// [`exit_after_signal_shutdown`], but told explicitly which signal (if any) is
/// responsible for the quit that just finished, instead of reading the process-wide
/// [`INITIATING_SIGNAL`] latch. Use this whenever the caller can distinguish "this
/// exact quit was that signal's" from "some signal was received at some point" --
/// the latch alone conflates the two (jwp2987/phosphor#726).
pub(crate) fn exit_after_signal_shutdown_for(initiating_signal: Option<i32>) {
    match final_exit(initiating_signal) {
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

/// Tracks which in-flight termination request, if any, is attributed to a
/// specific signal, so two quit paths racing to the same completion point
/// cannot clobber each other's attribution.
///
/// macOS is the motivating case: `AppDelegate::terminate_app` (a key binding,
/// menu item, or dialog) and the termination-signal handler both ultimately
/// call `app.terminate(None)`, and Cocoa's `applicationShouldTerminate:` runs
/// a two-phase dance (cancel -> hide -> a deferred re-invocation of
/// `terminate:`) before either one's request actually completes at
/// `applicationWillTerminate:`. Without this, whichever call's closure runs
/// last before completion wins the attribution, regardless of which request
/// is the one that is actually completing -- which is exactly how a
/// signal-initiated quit racing an ordinary one could previously exit status
/// 0 (the ordinary request's `None` overwrote the signal's `Some(sig)`), or an
/// ordinary Cmd+Q could re-raise a signal that had nothing to do with it (it
/// picked up a stale `Some(sig)` left by an earlier, unrelated signal
/// request) (jwp2987/phosphor#791).
///
/// Every accepted request gets a generation. [`TerminationAttribution::request`]
/// is first-writer-wins: if a termination sequence is already in flight, a
/// later call does not change its attribution, it only reports which request
/// (its own, or whichever got there first) actually owns the in-flight
/// generation. Only the request holding that generation may end it --
/// [`TerminationAttribution::complete`] consumes the attribution when that
/// request is the one that actually finishes, [`TerminationAttribution::cancel`]
/// clears it when that request is abandoned instead (e.g. the user declined
/// the "Quit Phosphor?" confirmation), so a later, unrelated quit attempt is
/// not told a shutdown is still in flight and silently denied its own
/// attribution.
///
/// Platform-neutral on purpose, so it is covered by tests that build and run
/// on every platform, including Linux; the mac glue that calls it
/// (`platform::mac::delegate`) stays `cfg(target_os = "macos")`.
#[derive(Debug)]
pub(crate) struct TerminationAttribution {
    state: parking_lot::Mutex<AttributionState>,
}

// Deliberately no `Default` impl: a default-constructed `next_generation` of
// 0 would collide with the sentinel 0 the mac glue's `CURRENT_GENERATION`
// static uses for "no request has run yet" -- `TerminationAttribution::new`
// always starts it at 1 instead.
#[derive(Debug)]
struct AttributionState {
    /// The generation to hand out to the next request that does not find one
    /// already in flight. Monotonically increasing; never reused, so a
    /// `complete`/`cancel` call carrying a stale generation can never match a
    /// later, unrelated request that happens to reuse a number.
    next_generation: u64,
    /// The request currently in flight, if a termination sequence has been
    /// requested and not yet completed or cancelled.
    in_flight: Option<InFlightRequest>,
}

#[derive(Debug, Clone, Copy)]
struct InFlightRequest {
    generation: u64,
    signal: Option<i32>,
}

impl TerminationAttribution {
    pub(crate) const fn new() -> Self {
        Self {
            state: parking_lot::Mutex::new(AttributionState {
                next_generation: 1,
                in_flight: None,
            }),
        }
    }

    /// Requests a termination attributed to `signal` (`None` for an ordinary
    /// key/menu/dialog quit, `Some(signal)` for the termination-signal
    /// handler's own request). Returns the generation of whichever request
    /// now owns the in-flight attribution: a fresh one if none was in flight,
    /// or the existing one, unchanged, if one already was -- first-writer-wins,
    /// so a later request can never overwrite an earlier one that is still
    /// being decided, regardless of whether either side is `None` or `Some`.
    pub(crate) fn request(&self, signal: Option<i32>) -> u64 {
        let mut state = self.state.lock();
        if let Some(in_flight) = &state.in_flight {
            return in_flight.generation;
        }
        let generation = state.next_generation;
        state.next_generation += 1;
        state.in_flight = Some(InFlightRequest { generation, signal });
        generation
    }

    /// Consumes the attribution for the request that is actually completing,
    /// identified by `generation`. Returns its signal (or `None` if it was not
    /// signal-initiated) and clears the in-flight state, if `generation` is
    /// the one currently owning it. Returns `None` without changing anything
    /// if it is not -- superseded by first-writer-wins, already completed, or
    /// never began -- so that caller must not attribute anything to this
    /// completion.
    pub(crate) fn complete(&self, generation: u64) -> Option<i32> {
        let mut state = self.state.lock();
        match &state.in_flight {
            Some(in_flight) if in_flight.generation == generation => {
                let signal = in_flight.signal;
                state.in_flight = None;
                signal
            }
            _ => None,
        }
    }

    /// Cancels the in-flight request if `generation` is the one that owns it
    /// (e.g. the user declined to quit), clearing its attribution so the next
    /// request starts fresh. A no-op if `generation` does not own it.
    pub(crate) fn cancel(&self, generation: u64) {
        let mut state = self.state.lock();
        if matches!(&state.in_flight, Some(f) if f.generation == generation) {
            state.in_flight = None;
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

impl<F: Fn(i32) -> bool> ProcessHooks<F> {
    pub(crate) fn new(request_terminate: F) -> Self {
        Self {
            request_terminate,
            exit: hard_exit,
        }
    }
}

impl<F: Fn(i32) -> bool> ShutdownHooks for ProcessHooks<F> {
    fn request_terminate(&self, signal: i32) -> bool {
        (self.request_terminate)(signal)
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
/// background thread and must only enqueue work for the main loop. It is called
/// with the specific signal that is asking for termination, so the main loop can
/// track which of its termination requests (if any) came from a signal, rather
/// than only a process-wide "some signal arrived" latch (jwp2987/phosphor#726).
#[cfg(unix)]
pub(crate) fn install(
    signals: &[i32],
    request_terminate: impl Fn(i32) -> bool + Send + 'static,
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

/// Windows-only: turns console control events (`CTRL_CLOSE_EVENT`,
/// `CTRL_LOGOFF_EVENT`, `CTRL_SHUTDOWN_EVENT`) into the same graceful,
/// non-cancellable quit that `SIGTERM`/`SIGHUP` already run on Unix
/// (jwp2987/phosphor#773, following up #685's "out of scope unless trivial").
///
/// # Why this can't just be [`install`]'s Unix shape
///
/// A Unix signal-handling thread only has to post a request and return --
/// nothing then kills the process, so the next line of `for signal in
/// delivered.forever()` is free to wait for the *next* signal. A Windows
/// console control handler is different: the OS calls it on its own thread and
/// kills the process shortly after *every* registered handler for the event
/// has *returned* (about 5s for `CTRL_CLOSE_EVENT`; similar for logoff and
/// shutdown). So [`console_ctrl_handler`] blocks that OS thread -- never the
/// main/UI thread, which it never touches -- until [`notify_shutdown_complete`]
/// reports that `app_will_terminate` has finished, or [`SHUTDOWN_DEADLINE`]
/// elapses, whichever comes first. The deadline watchdog [`handle_signal`]
/// already arms still ends the process if `app_will_terminate` itself wedges;
/// this wait is only what keeps the handler from returning -- and inviting the
/// OS's own kill -- before that has had its chance.
#[cfg(windows)]
pub(crate) mod console {
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::Duration;

    use instant::Instant;
    use windows::Win32::Foundation::{FALSE, TRUE};
    use windows::Win32::System::Console::SetConsoleCtrlHandler;
    use windows::core::BOOL;

    use super::{
        ProcessHooks, SHUTDOWN_DEADLINE, ShutdownHooks, ShutdownState, ctrl_event_shutdown_reason,
        handle_signal,
    };

    /// Which signals have been seen so far (for escalation) and how to post a
    /// terminate request to the main loop. `SetConsoleCtrlHandler` takes a bare
    /// `fn` pointer, not a closure, so [`console_ctrl_handler`] can only reach
    /// this through statics rather than captured state -- one handler is
    /// installed per process, so that is enough.
    static STATE: OnceLock<Mutex<ShutdownState>> = OnceLock::new();
    static HOOKS: OnceLock<ProcessHooks<Box<dyn Fn(i32) -> bool + Send + Sync>>> = OnceLock::new();

    /// Set once a signal-initiated shutdown's `app_will_terminate` has run, so
    /// a console control handler thread blocked in
    /// [`wait_for_shutdown_or_deadline`] can stop waiting and return. There is
    /// no need to reset it: the process exits, one way or another, once any
    /// termination shutdown completes.
    static SHUTDOWN_COMPLETE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();

    fn shutdown_complete() -> &'static (Mutex<bool>, Condvar) {
        SHUTDOWN_COMPLETE.get_or_init(|| (Mutex::new(false), Condvar::new()))
    }

    /// Called by a platform loop right after `app_will_terminate` finishes, so
    /// a console control handler thread blocked waiting for it can return
    /// instead of sitting out the full deadline. A harmless no-op if no console
    /// handler is installed, or none is currently blocked.
    pub(crate) fn notify_shutdown_complete() {
        let (lock, cvar) = shutdown_complete();
        let mut done = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *done = true;
        cvar.notify_all();
    }

    /// Blocks the calling thread until [`notify_shutdown_complete`] runs or
    /// `deadline` elapses, whichever is first.
    fn wait_for_shutdown_or_deadline(deadline: Duration) {
        let (lock, cvar) = shutdown_complete();
        let done = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = cvar.wait_timeout_while(done, deadline, |done| !*done);
    }

    /// The `SetConsoleCtrlHandler` callback. Runs on its own OS thread, never
    /// the main/UI thread, and must never touch app state directly -- only
    /// [`ShutdownHooks::request_terminate`] (a channel send) does that, exactly
    /// as the Unix signal thread's [`handle_signal`] call already does.
    ///
    /// Returns `TRUE` once the graceful shutdown has finished or the deadline
    /// has passed, telling Windows this handler dealt with the event (so it
    /// does not fall through to the next handler, or to the default
    /// disposition, which would kill the process without running
    /// `app_will_terminate` at all -- the exact bug #773 files). Returns
    /// `FALSE` immediately for an event this handler does not own
    /// (`CTRL_C_EVENT`/`CTRL_BREAK_EVENT`, which the headless loop's own
    /// `ctrlc` handler, installed separately, already covers).
    unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
        let Some(signal) = ctrl_event_shutdown_reason(ctrl_type) else {
            return FALSE;
        };
        let (Some(state), Some(hooks)) = (STATE.get(), HOOKS.get()) else {
            // `install` was never called, so this handler was never registered
            // either; unreachable in practice.
            return FALSE;
        };
        {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            handle_signal(&mut state, signal, Instant::now(), hooks);
        }
        // A duplicate/`ExitImmediately` response, or a main loop that could not
        // receive the request, already ends the process from inside
        // `handle_signal` (through `hooks.exit`, which never returns); this
        // wait only matters for the first event of a shutdown that is still in
        // progress.
        wait_for_shutdown_or_deadline(SHUTDOWN_DEADLINE);
        TRUE
    }

    /// Installs the console control handler. `request_terminate` must be safe
    /// to call from this module's own OS thread and must only enqueue work for
    /// the main loop, exactly like Unix's [`super::install`].
    pub(crate) fn install(
        request_terminate: impl Fn(i32) -> bool + Send + Sync + 'static,
    ) -> windows::core::Result<()> {
        STATE.get_or_init(|| Mutex::new(ShutdownState::default()));
        HOOKS.get_or_init(|| {
            ProcessHooks::new(Box::new(request_terminate) as Box<dyn Fn(i32) -> bool + Send + Sync>)
        });
        // SAFETY: `console_ctrl_handler` only ever touches the statics above
        // (initialized just before this call, and never torn down) plus
        // `wait_for_shutdown_or_deadline`, `handle_signal` and `hard_exit`, none
        // of which depend on anything this function sets up beyond those
        // statics.
        unsafe { SetConsoleCtrlHandler(Some(console_ctrl_handler), true) }
    }
}

#[cfg(test)]
#[path = "termination_signals_tests.rs"]
mod tests;
