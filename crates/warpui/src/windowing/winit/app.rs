use futures_util::future::LocalBoxFuture;
use std::mem::ManuallyDrop;

use crate::{
    clipboard::ClipboardContent,
    integration::TestDriver,
    keymap,
    platform::{self, TerminationMode},
    AppContext, AssetProvider, WindowId,
};
use derivative::Derivative;

use super::window::{IntegrationTestWindowManager, WindowManager};
use crate::notification::RequestPermissionsOutcome;

use crate::platform::NotificationInfo;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::sync::OnceLock;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub static WINDOWING_SYSTEM: OnceLock<WindowingSystem> = OnceLock::new();

pub type RequestPermissionsCallback =
    Box<dyn FnOnce(RequestPermissionsOutcome, &mut AppContext) + Send + Sync>;

#[derive(Derivative)]
#[derivative(Debug)]
pub enum CustomEvent {
    /// Open a window with the given window ID and options.
    OpenWindow {
        window_id: crate::WindowId,
        window_options: platform::WindowOptions,
    },
    /// Run the wrapped task on the main thread.
    RunTask(ManuallyDrop<async_task::Runnable>),
    /// Exit the event loop, terminating the application.
    Terminate(TerminationMode),
    /// Exit the event loop because a termination signal (`SIGTERM`/`SIGHUP`) was
    /// received, carrying which one. Handled like `Terminate(ForceTerminate)`, but
    /// kept distinct so `LoopExiting` can tell a signal-initiated quit apart from one
    /// a key binding, menu, or dialog also requested around the same moment: only a
    /// quit that reaches `LoopExiting` because of *this* event may re-raise the
    /// signal afterward (jwp2987/phosphor#726). Never posted by anything other than
    /// the termination-signal handler thread installed in `App::run`, which only
    /// exists on Unix (`#[cfg(unix)]`): not constructed on Windows or wasm.
    #[cfg_attr(not(unix), allow(dead_code))]
    TerminateFromSignal(i32),
    /// Close the specified window.
    CloseWindow {
        window_id: crate::WindowId,
        termination_mode: TerminationMode,
    },
    /// A global hotkey was pressed. Global hotkeys are not yet supported on wasm.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    GlobalShortcutTriggered(keymap::Keystroke),
    /// The active window changed.
    ///
    /// We use this to trigger [`platform::AppCallbacks::on_active_window_changed`] instead of
    /// winit's [`winit::event::WindowEvent::Focused`]. This is because winit's `Focused` event
    /// actually fires twice when focus is transferred between 2 of Zap's own windows. But, we
    /// only want to fire `on_active_window_changed` once for that focus change. So, we coalesce
    /// multiple `Focused` events into a single `ActiveWindowChanged` event on the next tick of the
    /// [`winit::event_loop::EventLoop`].
    ActiveWindowChanged,
    /// Update the UI App using the given closure.
    UpdateUIApp(#[derivative(Debug = "ignore")] Box<dyn FnOnce(&mut AppContext) + Send + Sync>),
    RequestUserAttention {
        window_id: WindowId,
    },
    StopRequestingUserAttention {
        window_id: WindowId,
    },
    #[allow(dead_code)]
    Clipboard(ClipboardEvent),
    SetCursorShape(platform::Cursor),
    ActiveCursorPositionUpdated,
    #[cfg_attr(not(any(target_os = "linux", target_os = "freebsd")), allow(dead_code))]
    AboutToSleep,
    #[cfg_attr(not(any(target_os = "linux", target_os = "freebsd")), allow(dead_code))]
    ResumedFromSleep,
    /// The application is connected to the internet.
    #[cfg_attr(any(target_os = "macos"), allow(dead_code))]
    InternetConnected,
    /// The application is disconnected from the internet.
    #[cfg_attr(any(target_os = "macos"), allow(dead_code))]
    InternetDisconnected,
    /// The system theme (light/dark) changed.
    /// TODO(CORE-2274): theming on Windows
    #[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
    SystemThemeChanged,
    /// Send a platform-native notification.
    SendNotification {
        window_id: WindowId,
        notification_info: NotificationInfo,
    },
    /// Focus the native window that triggered a notification.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    FocusWindow {
        window_id: WindowId,
    },
    RequestNotificationPermissions(#[derivative(Debug = "ignore")] RequestPermissionsCallback),
    /// Fire a debounced drag-and-drop files event.
    DragAndDropFilesDebounced {
        window_id: winit::window::WindowId,
    },
    /// Input received from the soft keyboard on mobile WASM.
    #[cfg(target_family = "wasm")]
    SoftKeyboardInput(crate::platform::wasm::SoftKeyboardInput),
    /// The visual viewport was resized (typically due to soft keyboard appearing/disappearing).
    #[cfg(target_family = "wasm")]
    VisualViewportResized {
        width: f32,
        height: f32,
    },
    /// Momentum scrolling animation frame.
    MomentumScroll {
        window_id: winit::window::WindowId,
    },
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ClipboardEvent {
    Paste(ClipboardContent),
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[derive(Debug, PartialEq)]
pub enum WindowingSystem {
    X11,
    Wayland,
}

pub struct App {
    callbacks: platform::app::AppCallbacks,
    assets: Box<dyn AssetProvider>,
    is_integration_test: bool,
    window_class: Option<String>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    force_x11: bool,
}

impl App {
    pub(crate) fn new(
        callbacks: platform::app::AppCallbacks,
        assets: Box<dyn AssetProvider>,
        test_driver: Option<&TestDriver>,
    ) -> Self {
        Self {
            callbacks,
            assets,
            is_integration_test: test_driver.is_some(),
            window_class: None,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            force_x11: false,
        }
    }

    // Dead code is allowed on wasm and Windows as the window class is only set for Linux
    // platforms.
    #[cfg_attr(any(target_family = "wasm", target_os = "windows"), allow(dead_code))]
    pub(crate) fn set_window_class(&mut self, window_class: String) {
        self.window_class = Some(window_class);
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(crate) fn force_x11(&mut self, force_x11: bool) {
        self.force_x11 = force_x11;
    }

    pub(crate) fn run(
        self,
        init_fn: impl FnOnce(&mut AppContext, LocalBoxFuture<'static, crate::App>) + 'static,
    ) {
        let App {
            callbacks,
            assets,
            is_integration_test,
            window_class,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            force_x11,
        } = self;

        let mut event_loop_builder = winit::event_loop::EventLoop::with_user_event();

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if force_x11 {
            winit::platform::x11::EventLoopBuilderExtX11::with_x11(&mut event_loop_builder);
        }

        let event_loop = event_loop_builder
            .build()
            .expect("should be able to create event loop");

        // Initialize the wgpu instance with the event loop's display handle.
        crate::rendering::wgpu::init_wgpu_instance(Box::new(event_loop.owned_display_handle()));

        // Perform some platform-specific initialization.
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
                super::linux::maybe_register_xlib_error_hook(&event_loop);
                super::linux::ensure_cursor_theme();
            } else if #[cfg(target_family = "wasm")] {
                crate::platform::wasm::add_paste_listener(event_loop.create_proxy());
                if callbacks.on_internet_reachability_changed.is_some() {
                    crate::platform::wasm::add_network_connection_listener(event_loop.create_proxy());
                }
                crate::platform::wasm::add_system_theme_listener(event_loop.create_proxy());
                crate::platform::wasm::setup_visual_viewport_resize_listener(event_loop.create_proxy());
            }
        }

        // Set the current thread as the main thread (the one that hosts the
        // application event loop).
        super::delegate::mark_current_thread_as_main();

        // SIGTERM / SIGHUP (kill, logout, `systemctl --user stop`, the launching
        // terminal closing) run the normal graceful quit instead of killing the
        // process outright (jwp2987/phosphor#685). The handler thread only posts
        // a non-cancellable `Terminate` through the proxy; the quit itself runs
        // on this loop via `LoopExiting` -> `app_will_terminate`. Integration
        // tests keep default dispositions: their driver owns signal handling.
        #[cfg(unix)]
        if !is_integration_test {
            use crate::platform::termination_signals;

            let proxy = event_loop.create_proxy();
            let result = termination_signals::install(
                termination_signals::GUI_TERMINATION_SIGNALS,
                move |signal| {
                    proxy
                        .send_event(CustomEvent::TerminateFromSignal(signal))
                        .is_ok()
                },
            );
            if let Err(err) = result {
                log::warn!("Failed to set up termination signal handling: {err}");
            }
        }
        // TODO(#773, Windows follow-up to #685): `WM_QUERYENDSESSION` /
        // `WM_ENDSESSION` (logoff, shutdown) still end the process without
        // `app_will_terminate`. The headless/TUI side of #773 is fixed (see
        // `termination_signals::console`: `SetConsoleCtrlHandler` for
        // `CTRL_CLOSE_EVENT` / `CTRL_LOGOFF_EVENT` / `CTRL_SHUTDOWN_EVENT`), but
        // this GUI path is not, deliberately -- the pinned winit fork
        // (`https://github.com/jwp2987/winit.git`, rev
        // `05e8c04da47960d8a627b73cf729d38aac91f80d`) has no `WM_ENDSESSION`
        // support to opt into (checked: no `ENDSESSION` in its source at all),
        // so unlike `CTRL_CLOSE_EVENT` this cannot reuse an existing seam.
        //
        // The viable mechanism is `SetWindowSubclass`/`DefSubclassProc`
        // (`windows::Win32::UI::Shell`, already enabled for this crate) on the
        // HWND winit's `WindowExtWindows::window_handle_any_thread` exposes:
        // unlike `SetConsoleCtrlHandler`, which runs on its own OS thread,
        // `WM_ENDSESSION` is *sent* synchronously on the same thread that pumps
        // winit's event loop, nested inside that thread's own
        // `GetMessage`/`DispatchMessage` call -- so the real work (hide windows,
        // `app_will_terminate`, exit) has to run right there in the subclass
        // callback, bounded by its own deadline watchdog, rather than merely
        // posting a `CustomEvent` and returning: the OS may not grant this
        // process another loop iteration before killing it. That callback would
        // need `&mut EventLoop` (`self.callbacks`, `self.ui_app`) to actually run
        // `app_will_terminate`, and nothing today threads a reference to it out
        // to where `SetWindowSubclass` could reach -- doing so blind, without a
        // Windows build to exercise a nested-message-pump case (e.g. a live
        // resize/move loop) where `WM_ENDSESSION` could arrive while this
        // thread's winit state is already borrowed elsewhere, is exactly the
        // kind of change issue #773's own "Scope note" says needs a real
        // Windows build/test before being trusted. Left for whoever has one.

        let ui_app = Self::construct_ui_app(assets, is_integration_test, &event_loop);
        let inner_event_loop = super::EventLoop::new(
            ui_app,
            callbacks,
            init_fn,
            window_class,
            event_loop.create_proxy(),
        );

        // Prevent dropping of our internal event loop state structure during
        // panic unwinds.
        //
        // We've seen crashes where a panic unwind leads to the dropping of the
        // event loop, which ultimately causes a segfault in graphics driver
        // code.  Given the fact that we terminate the app via `exit(0)` and
        // not by returning from the event loop, we don't ever need to drop the
        // event loop, even during a panic unwind.
        let mut inner_event_loop = std::mem::ManuallyDrop::new(inner_event_loop);

        // Temporarily allow use of the deprecated run() method until winit
        // 0.30 is here for good, at which point we'll migrate to the new
        // trait-based APIs.
        #[allow(deprecated)]
        event_loop
            .run(move |evt, window_target| {
                inner_event_loop.handle_event(evt, window_target);
            })
            .expect("Unable to run winit event loop");
    }

    fn construct_ui_app(
        assets: Box<dyn AssetProvider>,
        is_integration_test: bool,
        event_loop: &winit::event_loop::EventLoop<CustomEvent>,
    ) -> crate::App {
        let platform_delegate: Box<dyn platform::Delegate> = if is_integration_test {
            let delegate = super::delegate::IntegrationTestDelegate::new(event_loop.create_proxy())
                .expect("should not fail to create platform delegate");
            Box::new(delegate)
        } else {
            let mut delegate = super::delegate::AppDelegate::new(event_loop.create_proxy())
                .expect("should not fail to create platform delegate");
            delegate.use_platform_clipboard();
            Box::new(delegate)
        };

        let display_handle = event_loop.owned_display_handle();
        let window_manager: Box<dyn platform::WindowManager> = if is_integration_test {
            Box::new(IntegrationTestWindowManager::new(
                event_loop.create_proxy(),
                display_handle,
            ))
        } else {
            Box::new(WindowManager::new(
                event_loop.create_proxy(),
                display_handle,
            ))
        };

        crate::App::new(
            platform_delegate,
            window_manager,
            Box::new(super::fonts::FontDB::new()),
            assets,
        )
        .expect("should not fail to construct application")
    }
}
