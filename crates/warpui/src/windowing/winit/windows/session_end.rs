//! `WM_QUERYENDSESSION` / `WM_ENDSESSION` handling for Warp's own windows
//! (jwp2987/phosphor#773, the GUI follow-up to #685's console-only fix --
//! see `crate::platform::termination_signals`'s module doc for the console
//! side of this story).
//!
//! # Why a window subclass
//!
//! The pinned winit fork (`https://github.com/jwp2987/winit.git`, rev
//! `05e8c04da47960d8a627b73cf729d38aac91f80d`) has no `WM_ENDSESSION` support
//! to opt into, so there is no existing winit event to reuse the way the
//! console path reuses `SetConsoleCtrlHandler`. [`install`] attaches a
//! [`SetWindowSubclass`] callback to each window's HWND as it is created
//! (`windowing::winit::event_loop`'s `CustomEvent::OpenWindow` handler, right
//! after `Window::open_window` succeeds), which intercepts
//! `WM_QUERYENDSESSION`/`WM_ENDSESSION` before winit's own window procedure
//! ever sees them.
//!
//! # Why the callback stays minimal (the option chosen, and why)
//!
//! `WM_ENDSESSION` is *sent* synchronously on the same thread that pumps
//! winit's event loop, nested inside whatever `GetMessage`/`DispatchMessage`
//! call delivered it -- which can itself already be nested (e.g. Windows runs
//! a modal message loop of its own for `WM_ENTERSIZEMOVE` during a live
//! resize/move). Three shapes were considered for what the callback actually
//! does with that:
//!
//! (a) Post a [`CustomEvent`] and then pump messages from *inside* the
//!     subclass callback (`PeekMessage`/`DispatchMessage`) until a "done" flag
//!     is set or the deadline passes, so `app_will_terminate` runs before the
//!     callback returns. Rejected: this calls back into winit's own message
//!     dispatch while a frame higher up the same thread's stack may already be
//!     inside it (the live-resize/move case above), which can re-enter
//!     [`crate::windowing::winit::event_loop::EventLoop::handle_event`] and
//!     anything it has already borrowed (`RefCell`s on `Window`/`Inner`,
//!     `AppContext` state) while that outer call is still on the stack --
//!     exactly the kind of case this module's own issue (#773) says needs a
//!     real Windows build to exercise before being trusted, not a blind
//!     implementation.
//! (b) Stash a `Box<dyn FnOnce()>` shutdown closure (capturing what
//!     `app_will_terminate` needs via `Rc<RefCell<..>>` handles) in a static at
//!     startup, and call it directly from the subclass callback. Rejected for
//!     the same reason as (a) -- it still runs the real shutdown work
//!     synchronously inside a message that can itself be nested -- plus it
//!     requires threading owned handles to live `AppContext`/`EventLoop` state
//!     out to a `'static` the subclass callback can reach, which nothing
//!     today does and which would have to be invented and reasoned about
//!     without a Windows build to test the borrow hazards it creates.
//! (c) **Chosen.** Keep the callback itself to: call
//!     [`ShutdownBlockReasonCreate`] (which asks Windows to hold off tearing
//!     the process down while this app cleans up, showing the user a "this
//!     app is preventing shutdown" screen rather than silently killing it --
//!     unlike the `CTRL_CLOSE_EVENT` console path, which truly does get ~5s
//!     before the OS kills it, there is no hard timer here once this is
//!     called), post the *same* [`CustomEvent::TerminateFromSignal`] the
//!     Unix `SIGTERM`/`SIGHUP` path already posts, and return. The graceful
//!     shutdown then runs on its normal, already-tested turn through the
//!     event loop (`LoopExiting` -> `app_will_terminate`) instead of inside
//!     this callback, so this module never touches `AppContext`/`EventLoop`
//!     state and never nests a message pump. [`SHUTDOWN_DEADLINE`] is still
//!     armed as a safety net in case `app_will_terminate` itself wedges, so a
//!     hung flush does not leave the user staring at Windows's "preventing
//!     shutdown" screen forever.
//!
//! This also means [`ShutdownBlockReasonDestroy`] is only ever called from the
//! `Cancelled` branch, not after a successful shutdown: a successful shutdown
//! ends with `process::exit`/[`hard_exit`][termination_signals::hard_exit]
//! from the normal `LoopExiting` path (same place the Unix signal path ends),
//! which tears down the block reason along with the rest of the process, so
//! there is nothing left to explicitly release.
//!
//! # What still needs a real Windows machine (jwp2987/phosphor#773)
//!
//! This module only builds and is exercised today by CI's Windows
//! `cargo check`, which proves it type-checks against the pinned `windows`
//! crate (the `SetWindowSubclass`/`DefSubclassProc`/`ShutdownBlockReasonCreate`
//! signatures, the `HSTRING` conversion, the `SUBCLASSPROC` signature matching
//! [`session_end_subclass_proc`]'s). It proves nothing about runtime behavior.
//! Needs an actual Windows machine to verify:
//! - That `WM_QUERYENDSESSION`/`WM_ENDSESSION` actually arrive at this
//!   callback ahead of winit's own window procedure, for a real shutdown,
//!   logoff, and "close all programs" from the Shutdown Event Tracker.
//! - That a multi-window session end (every window gets `WM_ENDSESSION`) in
//!   fact posts exactly one [`CustomEvent::TerminateFromSignal`], via the
//!   `SESSION_ENDING` latch below -- this was reasoned through, not observed.
//! - That `ShutdownBlockReasonCreate`'s UI behaves as documented (holds off
//!   the kill with a visible "preventing shutdown" message) rather than the
//!   OS still force-ending the process on its own schedule in some Windows
//!   version/configuration.
//! - That the deadline watchdog's [`hard_exit`][termination_signals::hard_exit]
//!   actually fires and is not itself blocked by whatever state Windows is in
//!   during a real session end.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    ENDSESSION_LOGOFF, WM_ENDSESSION, WM_QUERYENDSESSION,
};
use windows::core::HSTRING;
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window as WinitWindow;

use crate::platform::termination_signals::{
    self, ProcessHooks, SHUTDOWN_DEADLINE, SessionEndAction, ShutdownHooks, exit_code_for_signal,
    session_end_action,
};
use crate::windowing::winit::app::CustomEvent;

/// A constant, process-private subclass ID for [`SetWindowSubclass`]. Only
/// needs to be distinct from any other subclass this process installs on the
/// *same* HWND with the *same* callback pointer -- nothing else in this crate
/// subclasses a window, so any value works; `773` is the tracking issue.
const SUBCLASS_ID: usize = 773;

/// Set once any window's `WM_ENDSESSION(wParam = TRUE)` has posted a
/// [`CustomEvent::TerminateFromSignal`] for the real shutdown attempt in
/// progress. Process-wide rather than per-window: Windows sends
/// `WM_ENDSESSION` to every top-level window this process owns for the one
/// real event, and only the first should start a shutdown
/// (jwp2987/phosphor#773). Cleared by a `Cancelled` result (another
/// application vetoed the session end), so a later, successful attempt is not
/// mistaken for a continuation of the cancelled one.
static SESSION_ENDING: AtomicBool = AtomicBool::new(false);

/// How to post a terminate request to the main loop, and how to arm the
/// deadline watchdog / hard-exit if it is not received -- set once, from the
/// first window this process opens. Every later window's subclass reuses it:
/// there is exactly one event loop (and one process-wide shutdown) regardless
/// of how many windows exist when a session actually ends.
static HOOKS: OnceLock<ProcessHooks<Box<dyn Fn(i32) -> bool + Send + Sync>>> = OnceLock::new();

/// Attaches the `WM_QUERYENDSESSION`/`WM_ENDSESSION` subclass to `window`'s
/// HWND. Called once per window, right after it is created
/// (`windowing::winit::event_loop`'s `CustomEvent::OpenWindow` handler).
pub(crate) fn install(window: &WinitWindow, proxy: EventLoopProxy<CustomEvent>) {
    let Ok(RawWindowHandle::Win32(handle)) = window.window_handle().map(|handle| handle.as_raw())
    else {
        log::warn!(
            "Could not get this window's HWND; it will not block or react to session end \
             (jwp2987/phosphor#773)"
        );
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as _);

    HOOKS.get_or_init(|| {
        ProcessHooks::new(Box::new(move |signal: i32| {
            proxy
                .send_event(CustomEvent::TerminateFromSignal(signal))
                .is_ok()
        }) as Box<dyn Fn(i32) -> bool + Send + Sync>)
    });

    // SAFETY: `session_end_subclass_proc` only ever touches the process-wide
    // statics above (initialized just before this call, never torn down) and
    // the `hwnd`/`msg`/`wparam`/`lparam` Windows passes it, which is exactly
    // the contract `SetWindowSubclass`/`SUBCLASSPROC` document.
    let installed =
        unsafe { SetWindowSubclass(hwnd, Some(session_end_subclass_proc), SUBCLASS_ID, 0) };
    if !installed.as_bool() {
        log::warn!(
            "Failed to install the WM_ENDSESSION subclass for this window \
             (jwp2987/phosphor#773); it will not block or react to session end"
        );
    }
}

/// The `SetWindowSubclass` callback. Runs on the UI thread, synchronously
/// inside whatever `DispatchMessage` call delivered the message -- see the
/// module doc for why this deliberately does no more than the module doc's
/// option (c) describes.
unsafe extern "system" fn session_end_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uidsubclass: usize,
    _dwrefdata: usize,
) -> LRESULT {
    if msg != WM_QUERYENDSESSION && msg != WM_ENDSESSION {
        // SAFETY: chaining to the next handler in the subclass/window-proc
        // chain for a message this proc does not own, exactly as
        // `DefSubclassProc`'s contract requires.
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    }

    let is_query = msg == WM_QUERYENDSESSION;
    let session_ending = wparam.0 != 0;
    let is_logoff = (lparam.0 as u32) & ENDSESSION_LOGOFF != 0;
    let already_shutting_down = SESSION_ENDING.load(Ordering::Acquire);

    match session_end_action(is_query, session_ending, is_logoff, already_shutting_down) {
        SessionEndAction::BlockAndAllow => {
            // SAFETY: `hwnd` is a valid top-level window handle owned by this
            // process, as required by `ShutdownBlockReasonCreate`.
            if let Err(err) =
                unsafe { ShutdownBlockReasonCreate(hwnd, &HSTRING::from("Saving your session…")) }
            {
                log::warn!("ShutdownBlockReasonCreate failed: {err} (jwp2987/phosphor#773)");
            }
            LRESULT(TRUE.0 as isize)
        }
        SessionEndAction::BeginGracefulShutdown { reason_signal } => {
            SESSION_ENDING.store(true, Ordering::Release);
            if let Some(hooks) = HOOKS.get() {
                hooks.arm_deadline(SHUTDOWN_DEADLINE, exit_code_for_signal(reason_signal));
                if !hooks.request_terminate(reason_signal) {
                    log::warn!(
                        "Main loop is gone; exiting without a graceful shutdown \
                         (jwp2987/phosphor#773)"
                    );
                    hooks.exit(exit_code_for_signal(reason_signal));
                }
            } else {
                // `install` always initializes `HOOKS` before installing this
                // very callback, so this is unreachable in practice; fail safe
                // rather than leave the session end unhandled.
                log::error!(
                    "WM_ENDSESSION fired before any window's subclass initialized the shutdown \
                     hooks (jwp2987/phosphor#773); exiting without a graceful shutdown"
                );
                termination_signals::hard_exit(exit_code_for_signal(reason_signal));
            }
            LRESULT(0)
        }
        SessionEndAction::AlreadyShuttingDown => LRESULT(0),
        SessionEndAction::Cancelled => {
            SESSION_ENDING.store(false, Ordering::Release);
            // SAFETY: `hwnd` is the same handle this callback was invoked with.
            if let Err(err) = unsafe { ShutdownBlockReasonDestroy(hwnd) } {
                log::warn!("ShutdownBlockReasonDestroy failed: {err} (jwp2987/phosphor#773)");
            }
            LRESULT(0)
        }
    }
}
