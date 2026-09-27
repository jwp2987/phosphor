use std::mem::ManuallyDrop;
use std::sync::mpsc::{Receiver, Sender};

use crate::{
    AppContext, WindowId,
    platform::{
        self, TerminationMode,
        app::{AppCallbackDispatcher, TerminationResult, approve_termination},
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
    /// Exit the event loop, terminating the application.
    Terminate(TerminationMode),
}

/// Run a simple, blocking event loop that processes AppEvent messages until termination.
pub(super) fn run(
    mut ui_app: crate::App,
    callbacks: &mut AppCallbackDispatcher,
    init_fn: platform::app::AppInitCallbackFn,
    receiver: Receiver<AppEvent>,
    sender: Sender<AppEvent>,
) -> TerminationResult {
    // Turn Ctrl-C / SIGTERM / SIGHUP into a graceful, non-cancellable quit.
    setup_signal_handler(sender);

    // First, initialize the app.
    callbacks.initialize_app(init_fn);

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
                if approve_termination(termination_mode, || callbacks.should_terminate_app()) {
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

    ui_app.termination_result().unwrap_or(Ok(()))
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
/// keeps `ctrlc` for Ctrl-C / Ctrl-Break.
// TODO(#685): Windows `CTRL_CLOSE_EVENT` (console window closed) still kills the
// process without a graceful shutdown; `ctrlc`'s `termination` feature would add
// it, but it is workspace-wide and would also redirect SIGTERM in the
// integration-test driver's `ctrlc` handler.
#[cfg(unix)]
fn setup_signal_handler(sender: Sender<AppEvent>) {
    use platform::termination_signals;

    let result = termination_signals::install(
        termination_signals::HEADLESS_TERMINATION_SIGNALS,
        move || {
            sender
                .send(AppEvent::Terminate(TerminationMode::ForceTerminate))
                .is_ok()
        },
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

    let hooks = ProcessHooks::new(move || {
        sender
            .send(AppEvent::Terminate(TerminationMode::ForceTerminate))
            .is_ok()
    });
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
}

#[cfg(target_family = "wasm")]
fn setup_signal_handler(_sender: Sender<AppEvent>) {
    // No signal handling on WASM
}
