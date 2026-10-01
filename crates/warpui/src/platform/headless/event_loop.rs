use std::mem::ManuallyDrop;
use std::sync::mpsc::{Receiver, Sender};

use crate::{
    AppContext, WindowId,
    platform::{
        self, TerminationMode,
        app::{
            AppCallbackDispatcher, ApproveTerminateResult, TerminationResult, approve_termination,
        },
    },
};

/// Application events handled on the headless platform's main thread.
pub(super) enum AppEvent {
    /// Run the wrapped task on the main thread.
    RunTask(ManuallyDrop<async_task::Runnable>),
    /// Run a synchronous callback on the main thread.
    RunCallback(Box<dyn FnOnce(&mut AppContext) + Send + Sync>),
    /// Close a window.
    CloseWindow(WindowId),
    /// Active window changed.
    ActiveWindowChanged(Option<WindowId>),
    /// Exit the event loop, terminating the application. Not itself attributed to
    /// a termination signal -- see [`AppEvent::TerminateFromSignal`] for the
    /// request that is (jwp2987/phosphor#791).
    Terminate(TerminationMode),
    /// Exit the event loop because a termination signal (`SIGINT`/`SIGTERM`/
    /// `SIGHUP`) was received, carrying which one. Handled exactly like
    /// `Terminate(ForceTerminate)`, but kept distinct so `run` can report, to its
    /// caller, whether THIS exit was actually caused by a signal: the TUI's own
    /// exit actions send a plain `Terminate(ForceTerminate)` that can race a
    /// concurrent, unrelated signal, and must not inherit its re-raise
    /// (jwp2987/phosphor#726, ported here from the winit loop's
    /// `CustomEvent::TerminateFromSignal`).
    TerminateFromSignal(i32),
}

/// Whether a termination request should proceed, given its mode and -- if it is
/// itself signal-initiated -- the signal that asked for it. Pulled out of `run`'s
/// match arms purely so this request-scoping is unit-testable without
/// constructing a full headless `App`/`AppCallbackDispatcher`: a
/// [`AppEvent::TerminateFromSignal`] request's `Some(signal)` must flow through
/// only when its OWN request is approved, and an ordinary [`AppEvent::Terminate`]
/// must never pick one up, even though both use
/// [`TerminationMode::ForceTerminate`] (jwp2987/phosphor#726, jwp2987/phosphor#791).
///
/// Returns `None` if the request was declined (a `Cancellable` quit the
/// confirmation turned down) and the loop must keep running; `Some(signal)` if it
/// proceeds, with the attribution `run` should hold until shutdown completes and
/// then pass to [`crate::platform::termination_signals::exit_after_signal_shutdown_for`].
fn terminate_decision(
    mode: TerminationMode,
    signal: Option<i32>,
    should_terminate_app: impl FnOnce() -> ApproveTerminateResult,
) -> Option<Option<i32>> {
    approve_termination(mode, should_terminate_app).then_some(signal)
}

/// Run a simple, blocking event loop that processes AppEvent messages until
/// termination. Returns the app's termination result and, if the request that
/// actually broke the loop was itself signal-initiated, the signal responsible --
/// the caller (`headless::app::App::run`) passes it to
/// [`crate::platform::termination_signals::exit_after_signal_shutdown_for`]
/// instead of the process-wide latch `exit_after_signal_shutdown` reads, so an
/// ordinary (non-signal) exit that merely raced a real SIGTERM/SIGHUP never
/// re-raises it (jwp2987/phosphor#726, jwp2987/phosphor#791).
pub(super) fn run(
    mut ui_app: crate::App,
    callbacks: &mut AppCallbackDispatcher,
    init_fn: platform::app::AppInitCallbackFn,
    receiver: Receiver<AppEvent>,
    sender: Sender<AppEvent>,
) -> (TerminationResult, Option<i32>) {
    // Turn Ctrl-C / SIGTERM / SIGHUP into a graceful, non-cancellable quit.
    setup_signal_handler(sender);

    // First, initialize the app.
    callbacks.initialize_app(init_fn);

    // Set only when the request that breaks the loop below is itself
    // signal-initiated; see this function's doc comment.
    let mut terminating_signal = None;

    // Then, process events until termination.
    for event in receiver.iter() {
        match event {
            AppEvent::RunCallback(callback) => ui_app.update(callback),
            AppEvent::RunTask(task) => {
                // Poll a task on the main thread.
                let task = ManuallyDrop::into_inner(task);
                task.run();
            }
            AppEvent::Terminate(termination_mode) => {
                if let Some(signal) =
                    terminate_decision(termination_mode, None, || callbacks.should_terminate_app())
                {
                    terminating_signal = signal;
                    break;
                }
            }
            AppEvent::TerminateFromSignal(signal) => {
                if let Some(signal) =
                    terminate_decision(TerminationMode::ForceTerminate, Some(signal), || {
                        callbacks.should_terminate_app()
                    })
                {
                    terminating_signal = signal;
                    break;
                }
            }
            AppEvent::CloseWindow(window_id) => {
                // Notify the app that a window is closing. The app will then remove the window
                // from WindowManager.
                callbacks.window_will_close(window_id);
            }
            AppEvent::ActiveWindowChanged(window_id) => {
                callbacks.active_window_changed(window_id);
            }
        }
    }

    // Drop the receiver so a signal that arrives during shutdown cannot queue a
    // terminate nobody will read. (The first signal armed a deadline, and an
    // insistent repeat exits immediately; see `termination_signals`.)
    drop(receiver);

    callbacks.app_will_terminate();

    // A Windows console control handler thread may be blocked waiting for this
    // (jwp2987/phosphor#773); a no-op everywhere else, including on Windows
    // when nothing is blocked.
    #[cfg(windows)]
    platform::termination_signals::console::notify_shutdown_complete();

    (
        ui_app.termination_result().unwrap_or(Ok(())),
        terminating_signal,
    )
}

/// Turns termination signals into a graceful, non-cancellable quit.
///
/// The first signal posts [`TerminationMode::ForceTerminate`] to this loop (the
/// same request the TUI's own exit actions send) and arms the shutdown deadline;
/// only a repeat of the same signal (not `SIGHUP`) at least a second later exits
/// immediately. No app work runs in the signal handler: it
/// only wakes a thread that sends on the loop's channel. See
/// [`platform::termination_signals`].
///
/// Unix handles `SIGINT`, `SIGTERM` and `SIGHUP` through `signal-hook`. Windows
/// keeps `ctrlc` for Ctrl-C / Ctrl-Break and, separately, a
/// `SetConsoleCtrlHandler` callback (jwp2987/phosphor#773) for console close /
/// logoff / shutdown (`CTRL_CLOSE_EVENT` / `CTRL_LOGOFF_EVENT` /
/// `CTRL_SHUTDOWN_EVENT`); `ctrlc`'s `termination` feature was not used for
/// those because it is workspace-wide and would also redirect SIGTERM in the
/// integration-test driver's own `ctrlc` handler.
#[cfg(unix)]
fn setup_signal_handler(sender: Sender<AppEvent>) {
    use platform::termination_signals;

    let result = termination_signals::install(
        termination_signals::HEADLESS_TERMINATION_SIGNALS,
        move |signal| sender.send(AppEvent::TerminateFromSignal(signal)).is_ok(),
    );
    if let Err(e) = result {
        log::warn!("Failed to set up termination signal handling: {e}");
    }
}

#[cfg(all(not(unix), not(target_family = "wasm")))]
fn setup_signal_handler(sender: Sender<AppEvent>) {
    use platform::termination_signals::{ProcessHooks, ShutdownState, handle_signal};
    use std::sync::Mutex;

    /// Ctrl-C is reported as SIGINT (2), preserving the historical exit status 130.
    const SIGINT: i32 = 2;

    // `CTRL_CLOSE_EVENT` / `CTRL_LOGOFF_EVENT` / `CTRL_SHUTDOWN_EVENT` are a
    // separate handler, installed below, so clone the sender before Ctrl-C's
    // handler consumes it (jwp2987/phosphor#773).
    #[cfg(windows)]
    let console_sender = sender.clone();

    let hooks =
        ProcessHooks::new(move |signal| sender.send(AppEvent::TerminateFromSignal(signal)).is_ok());
    let state = Mutex::new(ShutdownState::default());
    let result = ctrlc::set_handler(move || {
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        handle_signal(&mut state, SIGINT, instant::Instant::now(), &hooks);
    });
    if let Err(e) = result {
        log::warn!("Failed to set up Ctrl-C handler: {e}");
    }

    // Console close, logoff and shutdown (jwp2987/phosphor#773, the Windows
    // follow-up to #685): unlike Ctrl-C, these take their default disposition
    // (process killed) unless something else handles them, skipping
    // `app_will_terminate` entirely. See `termination_signals::console`.
    #[cfg(windows)]
    {
        // Carry the synthetic signal number through, as the Ctrl-C handler above
        // does, so console close/logoff/shutdown keep their attribution
        // (jwp2987/phosphor#791).
        let result = platform::termination_signals::console::install(move |signal| {
            console_sender
                .send(AppEvent::TerminateFromSignal(signal))
                .is_ok()
        });
        if let Err(e) = result {
            log::warn!("Failed to set up the console control handler: {e}");
        }
    }
}

#[cfg(target_family = "wasm")]
fn setup_signal_handler(_sender: Sender<AppEvent>) {
    // No signal handling on WASM
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGTERM: i32 = 15;

    #[test]
    fn a_signal_initiated_request_carries_its_own_signal_through() {
        // `AppEvent::TerminateFromSignal(SIGTERM)` is always `ForceTerminate`, so
        // the confirmation is never consulted, and the returned attribution is
        // exactly the signal that asked for it.
        let outcome = terminate_decision(TerminationMode::ForceTerminate, Some(SIGTERM), || {
            unreachable!("ForceTerminate must not consult the confirmation")
        });
        assert_eq!(outcome, Some(Some(SIGTERM)));
    }

    #[test]
    fn an_ordinary_quit_never_acquires_a_signal_attribution() {
        // A TUI exit action's own `AppEvent::Terminate(ForceTerminate)` passes
        // `None`: even though it shares `ForceTerminate` with the signal path, it
        // did not come from the signal thread, and must not re-raise a signal
        // merely because one happened to be received around the same time
        // (jwp2987/phosphor#726's exact bug, now also guarded against here).
        let outcome = terminate_decision(TerminationMode::ForceTerminate, None, || {
            unreachable!("ForceTerminate must not consult the confirmation")
        });
        assert_eq!(outcome, Some(None));
    }

    #[test]
    fn a_declined_cancellable_quit_never_proceeds_signal_or_not() {
        let outcome = terminate_decision(TerminationMode::Cancellable, None, || {
            ApproveTerminateResult::Cancel
        });
        assert_eq!(outcome, None);
    }

    #[test]
    fn an_approved_cancellable_quit_proceeds_with_no_signal() {
        let outcome = terminate_decision(TerminationMode::Cancellable, None, || {
            ApproveTerminateResult::Terminate
        });
        assert_eq!(outcome, Some(None));
    }
}
