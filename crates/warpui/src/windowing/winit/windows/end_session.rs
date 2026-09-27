//! Windows `WM_QUERYENDSESSION` / `WM_ENDSESSION` handling for the GUI build
//! (jwp2987/phosphor#685 follow-up).
//!
//! `crate::platform::headless::console_close` is the headless/TUI analogue,
//! using `SetConsoleCtrlHandler` for the same underlying OS events (there is
//! no window to send `WM_ENDSESSION` to in that build).
//!
//! # Why this can't reuse the ordinary `CustomEvent::Terminate` path
//!
//! `WM_QUERYENDSESSION`/`WM_ENDSESSION` are *sent*, not posted: Windows
//! delivers them to a window's procedure synchronously, from inside the same
//! thread's own call to `PeekMessageW`/`GetMessageW` (a sent, cross-thread
//! message is fully dispatched *within* that call, before it returns any
//! message to its caller -- it never becomes a `MSG` that reaches winit's own
//! `DispatchMessageW`, so a `winit::platform::windows::EventLoopBuilderExtWindows::with_msg_hook`
//! callback would never see it either).
//!
//! Posting `CustomEvent::Terminate` and waiting for the ordinary
//! `Event::LoopExiting` path to run it would require *returning* control to
//! winit's own dispatch loop first, and then waiting for that loop to notice
//! the exit request on a later iteration -- which for this fork of winit only
//! happens once `EventLoop::run`'s own outer loop (`wait_for_messages` /
//! `dispatch_peeked_messages`) regains control, i.e. after this handler
//! returns. That is also the last moment before Windows may terminate the
//! process outright, per `WM_ENDSESSION`'s documented contract. So
//! `WM_ENDSESSION` with `wParam != 0` (the session really is ending) runs the
//! same non-cancellable shutdown body `Event::LoopExiting` runs
//! (`EventLoop::run_shutdown_body`), directly on this thread, before
//! returning -- see that method for the rest of the ordering.
//!
//! # Safety / re-entrancy
//!
//! The subclass is installed once, on the first window opened
//! (`INSTALLED`), and the shutdown body runs at most once
//! (`SHUTDOWN_STARTED`): every top-level window would otherwise get its own
//! `WM_ENDSESSION`, but any one of them is enough to shut the whole app down.
//!
//! Reaching back into the running [`EventLoop`] through [`EVENT_LOOP`] (a raw
//! pointer recorded once, from the main thread, at `NewEvents(StartCause::Init)`
//! -- see `EventLoop::handle_event`) is safe because `WM_ENDSESSION` cannot
//! arrive while `EventLoop::handle_event` is itself running: both execute on
//! the main thread, and, as above, a sent message is fully handled inside the
//! `PeekMessageW`/`GetMessageW` call that retrieves the *next posted*
//! message -- never while a previously retrieved one is still being
//! dispatched. So there is never a live `&mut EventLoop` borrow anywhere else
//! when this one-shot call happens. The pointer itself stays valid because
//! `windowing::winit::app::App::run` never drops the `EventLoop` it points
//! to, and boxes its closure once with winit (see the comment on
//! `ManuallyDrop` there), so its heap address never moves either.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, WM_ENDSESSION, WM_QUERYENDSESSION, WNDPROC,
};

use crate::platform::termination_signals;
use crate::windowing::winit::event_loop::EventLoop;

/// Whether the subclass has been installed on some window already.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Whether the shutdown body has already started (and therefore, in
/// practice, that the process is already on its way out).
static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

/// The window procedure winit installed before we subclassed it, so
/// unhandled messages still reach it: the raw value `SetWindowLongPtrW`
/// returned in `install`, reconstructed as a `WNDPROC` in
/// `call_prev_wndproc`. Zero means "not installed yet" (matching
/// `SetWindowLongPtrW`'s own failure return and the fact that a real
/// `WNDPROC` is never null).
static PREV_WNDPROC: AtomicIsize = AtomicIsize::new(0);

/// A raw pointer to the running [`EventLoop`], set once by
/// [`set_event_loop`]. See the module docs for why this is safe.
static EVENT_LOOP: AtomicPtr<EventLoop> = AtomicPtr::new(std::ptr::null_mut());

/// Records the running event loop so a later `WM_ENDSESSION` can reach back
/// into it. Must be called from the main thread, before installing the
/// subclass below (i.e. before any window opens); `EventLoop::handle_event`
/// does this once, on `NewEvents(StartCause::Init)`.
pub(in crate::windowing::winit) fn set_event_loop(event_loop: *mut EventLoop) {
    EVENT_LOOP.store(event_loop, Ordering::Release);
}

/// Subclasses `hwnd` to catch `WM_QUERYENDSESSION`/`WM_ENDSESSION`, unless
/// some window is already subclassed. Idempotent; safe to call for every
/// window opened.
pub(in crate::windowing::winit) fn install(hwnd: HWND) {
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }

    // SAFETY: `hwnd` is a valid top-level window handle obtained from winit,
    // called on the same (main) thread that created it -- the same
    // precondition every other `SetWindowLongPtrW` call site in this
    // codebase relies on (see `windowing/winit/windows/window_ext.rs`).
    // `end_session_wndproc` chains to whatever `SetWindowLongPtrW` returns
    // here for every message it doesn't special-case, so winit's own window
    // procedure keeps running exactly as before for all of them.
    let prev = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, end_session_wndproc as isize) };
    PREV_WNDPROC.store(prev, Ordering::Release);
}

/// The subclass procedure. Answers `WM_QUERYENDSESSION` immediately (any
/// nonzero return grants permission to end the session) and, for
/// `WM_ENDSESSION` with `wParam != 0` (the session really is ending), runs
/// the shutdown body in-line before returning -- see the module docs for why.
/// Everything else is forwarded to whatever window procedure was installed
/// before this one.
unsafe extern "system" fn end_session_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_QUERYENDSESSION => return LRESULT(1),
        WM_ENDSESSION if wparam.0 != 0 => {
            // In practice this never returns: `handle_end_session` always
            // ends the process itself. Fall through to the previous window
            // procedure only in the (unexpected) case that it somehow does,
            // so the window still behaves reasonably rather than swallowing
            // the message.
            handle_end_session();
        }
        _ => {}
    }
    call_prev_wndproc(hwnd, msg, wparam, lparam)
}

fn call_prev_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let prev = PREV_WNDPROC.load(Ordering::Acquire);
    let prev: WNDPROC = if prev == 0 {
        None
    } else {
        // SAFETY: `prev` is the value `SetWindowLongPtrW` returned for this
        // same window's class in `install`, so it is a valid `WNDPROC`
        // (matching `end_session_wndproc`'s own signature) for the window's
        // lifetime. `isize` and a function pointer are the same size, so
        // this transmute cannot fail to fit.
        Some(unsafe {
            std::mem::transmute::<
                isize,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(prev)
        })
    };
    unsafe { CallWindowProcW(prev, hwnd, msg, wparam, lparam) }
}

/// Runs the same non-cancellable shutdown body a graceful `SIGTERM`/`SIGHUP`
/// quit runs (`EventLoop::run_shutdown_body`, shared with
/// `Event::LoopExiting`), directly on this thread, then ends the process.
/// Bounded by a watchdog so a wedged flush can't leave the app looking hung
/// to Windows (or to whoever is waiting on the logoff/shutdown to finish).
fn handle_end_session() {
    if SHUTDOWN_STARTED.swap(true, Ordering::AcqRel) {
        // Already running (or ran) from an earlier delivery; nothing to add,
        // and calling `app_will_terminate` twice is not something the rest
        // of the shutdown path is written to expect.
        return;
    }

    log::info!(
        "Received WM_ENDSESSION; shutting down gracefully (deadline {}s)",
        termination_signals::SHUTDOWN_DEADLINE.as_secs()
    );

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

    let event_loop = EVENT_LOOP.load(Ordering::Acquire);
    if event_loop.is_null() {
        log::warn!("WM_ENDSESSION arrived before the event loop was recorded; exiting directly");
        termination_signals::hard_exit(0);
        // `hard_exit` always ends the process (see its own doc comment); this
        // is defense in depth against dereferencing a null pointer below if
        // it somehow didn't.
        return;
    }
    // SAFETY: see the module docs: this is a single, one-shot, main-thread
    // call, guaranteed not to overlap any other live borrow of `EventLoop`.
    unsafe { (*event_loop).run_shutdown_body() };

    // A signal-initiated quit (SIGTERM/SIGHUP -- not applicable here, but
    // shared with the ordinary `LoopExiting` path for consistency) would
    // reraise its signal; this is a plain, deliberate exit.
    termination_signals::exit_after_signal_shutdown();
    termination_signals::hard_exit(0);
}
