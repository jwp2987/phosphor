//! Windows console close / logoff / shutdown handling for the headless / TUI
//! build (jwp2987/phosphor#685 follow-up).
//!
//! `crate::windowing::winit::windows::end_session` is the GUI analogue for
//! logoff/shutdown (`WM_QUERYENDSESSION`/`WM_ENDSESSION`); this module covers
//! the same OS events for a build with no window at all, where they instead
//! arrive as console control events: `CTRL_CLOSE_EVENT` (the console window
//! was closed) and `CTRL_LOGOFF_EVENT`/`CTRL_SHUTDOWN_EVENT`.
//!
//! `event_loop.rs`'s own `setup_signal_handler` already registers a `ctrlc`
//! handler for Ctrl-C / Ctrl-Break (`CTRL_C_EVENT`/`CTRL_BREAK_EVENT`); this
//! installs a second, independent `SetConsoleCtrlHandler` callback for the
//! other three events. Windows calls every registered handler, most recently
//! installed first, until one returns `TRUE`, so the two coexist without
//! interfering with each other.
//!
//! # Why this handler blocks, unlike the `ctrlc` one above
//!
//! A console control handler runs on its own OS-created thread, separate
//! from the app's main thread -- never in the headless event loop itself.
//! Per `SetConsoleCtrlHandler`'s documented contract, once every registered
//! handler for `CTRL_CLOSE_EVENT` (or a logoff/shutdown event) has returned,
//! Windows gives the process only a few seconds before terminating it
//! outright. The `ctrlc` handler above can return immediately because
//! nothing then kills the process. Here, returning immediately would race
//! `app_will_terminate` (LSP/MCP shutdown, terminal-server teardown, the
//! persistence flush) against that OS deadline with no guarantee it even
//! starts in time. So this handler blocks -- on its own thread, not the main
//! loop's -- on a [`ShutdownGate`] until either [`headless::event_loop::run`]
//! signals that shutdown has completed, or
//! [`termination_signals::SHUTDOWN_DEADLINE`] elapses, whichever comes
//! first. Either way it is then safe to return: on the deadline path, the
//! watchdog thread armed below hard-exits on its own if `app_will_terminate`
//! is still wedged, exactly like the signal path in `termination_signals`.
//!
//! The wait/notify coordination itself (`ShutdownGate`) is platform-independent
//! and covered by tests that run on every host; only this file's use of the
//! real `SetConsoleCtrlHandler`/`AppEvent` plumbing is Windows-only and
//! unverified without a Windows build.

use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{FALSE, TRUE};
use windows_core::BOOL;

use crate::platform::termination_signals::{self, ShutdownGate};
use crate::platform::TerminationMode;

use super::event_loop::AppEvent;

/// The headless loop's event sender, recorded by [`install`] so the console
/// control handler (which gets no user data of its own) can reach it.
static SENDER: OnceLock<Mutex<Sender<AppEvent>>> = OnceLock::new();

/// Signalled by [`signal_shutdown_complete`] once `app_will_terminate` has
/// finished, so a handler thread blocked in
/// [`request_graceful_shutdown_and_wait`] can return promptly instead of
/// sleeping out the whole deadline.
static SHUTDOWN_GATE: ShutdownGate = ShutdownGate::new();

/// Installs a `SetConsoleCtrlHandler` callback for `CTRL_CLOSE_EVENT`,
/// `CTRL_LOGOFF_EVENT` and `CTRL_SHUTDOWN_EVENT`, in addition to (not instead
/// of) the `ctrlc`-based Ctrl-C handler `event_loop.rs` already installs.
pub(super) fn install(sender: Sender<AppEvent>) -> windows::core::Result<()> {
    // `set` only fails if called twice; `setup_signal_handler` calls this
    // once, so a failure here would mean an unexpected second call, and
    // keeping the existing sender is the safe choice either way.
    let _ = SENDER.set(Mutex::new(sender));

    // SAFETY: `console_ctrl_handler` only touches the process-global statics
    // above and does no app work itself (see the module docs); it is valid
    // for the rest of the process's life, which is exactly the lifetime
    // `SetConsoleCtrlHandler` expects of a handler passed `add == true`.
    unsafe {
        windows::Win32::System::Console::SetConsoleCtrlHandler(Some(console_ctrl_handler), true)
    }
}

/// The handler Windows calls, on its own thread, for every console control
/// event. Requests a graceful shutdown and blocks for the close/logoff/
/// shutdown events; returns `FALSE` for everything else so Ctrl-C/Ctrl-Break
/// keep going through the ordinary `ctrlc` handler.
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    use windows::Win32::System::Console::{
        CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    };

    match ctrl_type {
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            request_graceful_shutdown_and_wait();
            TRUE
        }
        _ => FALSE,
    }
}

/// Posts a non-cancellable terminate request to the headless loop, arms the
/// same deadline watchdog the signal path uses, and blocks until either the
/// loop confirms shutdown has completed or the deadline elapses.
fn request_graceful_shutdown_and_wait() {
    log::info!(
        "Received a console close/logoff/shutdown event; shutting down gracefully (deadline {}s)",
        termination_signals::SHUTDOWN_DEADLINE.as_secs()
    );

    // If `app_will_terminate` hangs, exit anyway rather than let Windows
    // decide this handler (and therefore the process) is unresponsive.
    let spawned = std::thread::Builder::new()
        .name("shutdown-deadline".to_string())
        .spawn(|| {
            termination_signals::run_deadline_watchdog(
                termination_signals::SHUTDOWN_DEADLINE,
                0,
                std::thread::sleep,
                termination_signals::hard_exit,
            )
        });
    if let Err(err) = spawned {
        log::warn!("Failed to spawn the shutdown deadline watchdog: {err}");
    }

    let Some(sender) = SENDER.get() else {
        log::warn!("Console close/logoff/shutdown event before the sender was recorded");
        return;
    };
    let sent = match sender.lock() {
        Ok(sender) => sender
            .send(AppEvent::Terminate(TerminationMode::ForceTerminate))
            .is_ok(),
        Err(_) => false,
    };
    if !sent {
        log::warn!("Main loop is gone; exiting without waiting for it");
        return;
    }

    SHUTDOWN_GATE.wait(termination_signals::SHUTDOWN_DEADLINE);
}

/// Called by `headless::event_loop::run` right after `app_will_terminate`
/// finishes, so a handler thread blocked in
/// [`request_graceful_shutdown_and_wait`] can return promptly. A no-op if no
/// such handler is waiting (the ordinary, non-console-close exit path).
pub(super) fn signal_shutdown_complete() {
    SHUTDOWN_GATE.signal();
}
