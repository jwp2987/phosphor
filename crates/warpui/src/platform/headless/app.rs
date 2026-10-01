use futures::future::LocalBoxFuture;

use crate::platform::app::TerminationResult;
use crate::platform::test::FontDB as TestFontDB;
use crate::{
    AppContext, AssetProvider,
    integration::TestDriver,
    platform::{self},
};

use super::delegate::{self, AppDelegate};
use super::event_loop::{self, AppEvent};
use super::windowing::WindowManager;
use std::sync::mpsc;

pub struct App {
    callbacks: platform::app::AppCallbacks,
    assets: Box<dyn AssetProvider>,
}

impl App {
    pub(in crate::platform) fn new(
        callbacks: platform::app::AppCallbacks,
        assets: Box<dyn AssetProvider>,
        test_driver: Option<&TestDriver>,
    ) -> Self {
        // Other platforms use the test_driver parameter to enable an alternative platform delegate implementation
        // in integration tests - that doesn't apply here.
        let _ = test_driver;
        Self { callbacks, assets }
    }

    pub(in crate::platform) fn run(
        self,
        init_fn: impl FnOnce(&mut AppContext, LocalBoxFuture<'static, crate::App>) + 'static,
    ) -> TerminationResult {
        let App { callbacks, assets } = self;

        let (sender, receiver) = mpsc::channel::<AppEvent>();

        // Mark this thread as the main thread for DispatchDelegate checks.
        delegate::mark_current_thread_as_main();

        let platform_delegate = Box::new(AppDelegate::new(sender.clone()));
        let window_manager = Box::new(WindowManager::new(sender.clone()));
        // Reuse the testing FontDB implementation, as no font features are needed in headless mode.
        let font_db: Box<dyn platform::FontDB> = Box::new(TestFontDB::new());

        let ui_app = crate::App::new(platform_delegate, window_manager, font_db, assets)
            .expect("should not fail to construct application");

        let mut callbacks =
            warpui_core::platform::app::AppCallbackDispatcher::new(callbacks, ui_app.clone());

        // Run the event loop until the app terminates. `terminating_signal` is
        // `Some` only if the request that actually broke the loop was itself
        // signal-initiated (`AppEvent::TerminateFromSignal`), never merely because
        // some signal was received at some point during the process's life
        // (jwp2987/phosphor#726, jwp2987/phosphor#791).
        let (result, terminating_signal) =
            event_loop::run(ui_app, &mut callbacks, Box::new(init_fn), receiver, sender);

        // A signal-initiated quit ends the way the signal would have
        // (jwp2987/phosphor#685), matching the winit and macOS loops, but scoped to
        // *this* exit's own request via `terminating_signal` rather than the
        // process-wide latch `exit_after_signal_shutdown` reads -- an ordinary
        // (non-signal) exit that merely raced a real SIGTERM/SIGHUP must not
        // inherit its re-raise. Otherwise this returns and `result` is reported as
        // usual. `event_loop::run` itself does not make this call (see its doc
        // comment): it returns to this, its caller, before the exit status is
        // decided (jwp2987/phosphor#717).
        #[cfg(not(target_family = "wasm"))]
        platform::termination_signals::exit_after_signal_shutdown_for(terminating_signal);
        #[cfg(target_family = "wasm")]
        let _ = terminating_signal;

        result
    }
}
