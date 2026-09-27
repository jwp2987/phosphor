//! Windows `WM_QUERYENDSESSION` / `WM_ENDSESSION` handling for the GUI build
//! (jwp2987/phosphor#685, jwp2987/phosphor#773).
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
//! # Every window is subclassed, not just the first
//!
//! An earlier version of this file installed the subclass once, on the
//! first window opened, on the assumption that any one top-level window
//! receiving `WM_ENDSESSION` is enough. That's true for the message itself,
//! but not for the *subclass*: if that first window is later closed while
//! others stay open, `WM_ENDSESSION` handling silently disappears for the
//! rest of the process's life, along with the previous window procedure it
//! was chaining to. So every window gets subclassed as it opens
//! ([`install`]), each one's own previous `WNDPROC` is tracked separately in
//! [`PREV_WNDPROCS`] (keyed by `HWND`, since each window has its own), and
//! the entry is removed on `WM_NCDESTROY` ([`call_or_remove_prev_wndproc`]).
//! [`SHUTDOWN_STARTED`] still ensures the shutdown body itself runs at most
//! once, however many subclassed windows end up receiving `WM_ENDSESSION`.
//!
//! # Safety / re-entrancy
//!
//! Reaching back into the running [`EventLoop`] through [`EVENT_LOOP`] (a raw
//! pointer recorded once, from the main thread, at `NewEvents(StartCause::Init)`
//! -- see `EventLoop::handle_event`) would be unsound if some other frame on
//! this thread's stack could still hold the `&mut EventLoop` that call
//! received. That *can* happen: `WM_ENDSESSION`, like any sent message, can
//! be delivered from inside a nested message pump this same thread is
//! already running underneath some other code -- `DefWindowProc`'s modal
//! move/size loop, menu tracking, a native dialog, drag-and-drop, OLE/
//! clipboard negotiation -- any of which can sit several frames below a live
//! `handle_event` call. There is no static proof otherwise, so this checks
//! [`EVENT_HANDLER_DEPTH`] at runtime: the pointer is dereferenced only when
//! it reads 0, meaning no `handle_event` call (and therefore no `&mut
//! EventLoop` borrow) is anywhere on this thread's stack right now.
//!
//! When the depth is *not* 0, this does not touch the `EventLoop` at all.
//! The safe minimum reachable without it is: log what happened, and end the
//! process the same bounded way the deadline watchdog would (skipping
//! `app_will_terminate` -- LSP/MCP shutdown, terminal-server teardown, the
//! persistence flush all get skipped in this case, same as an ordinary
//! `SIGKILL` would). This is a deliberately narrow, rare fallback: it needs
//! `WM_ENDSESSION` to land inside one of the nested pumps named above, at the
//! exact moment a logoff/shutdown is requested. See jwp2987/phosphor#773 for
//! whether this needs to be tightened further once it can be exercised on a
//! real Windows build.
//!
//! [`clear_event_loop`] is called from `EventLoop::handle_event`'s
//! `Event::LoopExiting` arm, so the pointer cannot outlive the `EventLoop` it
//! names, however that exit was reached.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, WM_ENDSESSION, WM_NCDESTROY,
    WM_QUERYENDSESSION, WNDPROC,
};

use crate::platform::termination_signals;
use crate::windowing::winit::event_loop::{EventLoop, EVENT_HANDLER_DEPTH};

/// Whether the shutdown body has already started (and therefore, in
/// practice, that the process is already on its way out). Guards against
/// running it twice when more than one subclassed window gets `WM_ENDSESSION`.
static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

/// Each currently-subclassed window's own previous `WNDPROC` (the one
/// [`install`] replaced), keyed by `hwnd.0 as isize`, so
/// [`call_or_remove_prev_wndproc`] forwards a message to the *right*
/// procedure for whichever window it arrived on. An entry is removed on
/// `WM_NCDESTROY`.
static PREV_WNDPROCS: Mutex<Option<HashMap<isize, isize>>> = Mutex::new(None);

/// A raw pointer to the running [`EventLoop`], set once by
/// [`set_event_loop`] and cleared by [`clear_event_loop`]. See the module
/// docs for why dereferencing it is guarded by [`EVENT_HANDLER_DEPTH`].
static EVENT_LOOP: AtomicPtr<EventLoop> = AtomicPtr::new(std::ptr::null_mut());

/// Records the running event loop so a later `WM_ENDSESSION` can reach back
/// into it. Must be called from the main thread, before installing the
/// subclass below (i.e. before any window opens); `EventLoop::handle_event`
/// does this once, on `NewEvents(StartCause::Init)`.
pub(in crate::windowing::winit) fn set_event_loop(event_loop: *mut EventLoop) {
    EVENT_LOOP.store(event_loop, Ordering::Release);
}

/// Clears the recorded event loop pointer so it can never be dereferenced
/// once the `EventLoop` it names is no longer valid. `EventLoop::handle_event`
/// calls this from its `Event::LoopExiting` arm, before running the shutdown
/// body, so it's cleared however the exit was reached.
pub(in crate::windowing::winit) fn clear_event_loop() {
    EVENT_LOOP.store(std::ptr::null_mut(), Ordering::Release);
}

/// Subclasses `hwnd` to catch `WM_QUERYENDSESSION`/`WM_ENDSESSION`. Called for
/// every window as it opens (see the module docs for why); a no-op if this
/// exact `hwnd` is somehow already subclassed.
pub(in crate::windowing::winit) fn install(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let mut prev_wndprocs = PREV_WNDPROCS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prev_wndprocs = prev_wndprocs.get_or_insert_with(HashMap::new);
    if prev_wndprocs.contains_key(&key) {
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
    prev_wndprocs.insert(key, prev);
}

/// The subclass procedure, shared by every subclassed window. Answers
/// `WM_QUERYENDSESSION` immediately (any nonzero return grants permission to
/// end the session) and, for `WM_ENDSESSION` with `wParam != 0` (the session
/// really is ending), runs the shutdown body in-line before returning -- see
/// the module docs for why. `WM_NCDESTROY` removes this window's entry from
/// [`PREV_WNDPROCS`] before forwarding it (the window is going away; nothing
/// will be subclassed on this `hwnd` again). Everything else is forwarded to
/// whatever window procedure was installed on this same window before this
/// one.
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
        WM_NCDESTROY => return call_or_remove_prev_wndproc(hwnd, msg, wparam, lparam, true),
        _ => {}
    }
    call_or_remove_prev_wndproc(hwnd, msg, wparam, lparam, false)
}

/// Looks up `hwnd`'s own previous window procedure and forwards `msg` to it
/// (or to `DefWindowProcW` if, unexpectedly, none is recorded). `remove`
/// takes the entry out of [`PREV_WNDPROCS`] instead of merely reading it --
/// used for `WM_NCDESTROY`, the last message this window will ever forward.
fn call_or_remove_prev_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    remove: bool,
) -> LRESULT {
    let key = hwnd.0 as isize;
    let prev = {
        let mut prev_wndprocs = PREV_WNDPROCS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(map) = prev_wndprocs.as_mut() else {
            // SAFETY: nothing has ever been subclassed, so there is no
            // previous procedure to chain to; `DefWindowProcW` is the
            // documented, always-valid fallback.
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        };
        if remove {
            map.remove(&key)
        } else {
            map.get(&key).copied()
        }
    };

    let Some(prev) = prev.filter(|&prev| prev != 0) else {
        // SAFETY: no previous procedure is recorded for this window (should
        // not happen for a window `install` actually subclassed, but a
        // window procedure has to handle every message, including ones that
        // predictably can't be perfectly accounted for); `DefWindowProcW` is
        // the documented, always-valid fallback.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    // SAFETY: `prev` is the value `SetWindowLongPtrW` returned for this same
    // window in `install`, so it is a valid `WNDPROC` (matching
    // `end_session_wndproc`'s own signature) for as long as this entry
    // stays in the map, i.e. until this same `WM_NCDESTROY` removes it.
    // `isize` and a function pointer are the same size, so this transmute
    // cannot fail to fit.
    let prev: WNDPROC = Some(unsafe {
        std::mem::transmute::<isize, unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>(
            prev,
        )
    });
    unsafe { CallWindowProcW(prev, hwnd, msg, wparam, lparam) }
}

/// Runs the same non-cancellable shutdown body a graceful `SIGTERM`/`SIGHUP`
/// quit runs (`EventLoop::run_shutdown_body`, shared with
/// `Event::LoopExiting`) when it's safe to -- see the module docs -- then
/// ends the process. Bounded by a watchdog so a wedged flush can't leave the
/// app looking hung to Windows (or to whoever is waiting on the logoff/
/// shutdown to finish).
fn handle_end_session() {
    if SHUTDOWN_STARTED.swap(true, Ordering::AcqRel) {
        // Already running (or ran) from an earlier delivery; nothing to add,
        // and calling `app_will_terminate` twice is not something the rest
        // of the shutdown path is written to expect.
        return;
    }

    log::info!(
        "Received WM_ENDSESSION; shutting down gracefully (deadline {}s)",
        termination_signals::CONSOLE_LOGOFF_DEADLINE.as_secs()
    );

    let spawned = std::thread::Builder::new()
        .name("shutdown-deadline".to_string())
        .spawn(|| {
            termination_signals::run_deadline_watchdog(
                termination_signals::CONSOLE_LOGOFF_DEADLINE,
                0,
                std::thread::sleep,
                termination_signals::hard_exit,
            )
        });
    if let Err(err) = spawned {
        log::warn!("Failed to spawn the shutdown deadline watchdog: {err}");
    }

    if EVENT_HANDLER_DEPTH.get() != 0 {
        // Some other frame on this thread's stack may still hold the
        // `&mut EventLoop` this pointer names (see the module docs for how);
        // dereferencing it here would be unsound. There is no safe way to
        // run `app_will_terminate` without it, so this is the narrow,
        // documented fallback: skip straight to exiting.
        log::warn!(
            "WM_ENDSESSION arrived while the event loop was mid-dispatch (depth {}); \
             skipping app_will_terminate and exiting directly -- see jwp2987/phosphor#773",
            EVENT_HANDLER_DEPTH.get()
        );
        termination_signals::hard_exit(0);
        return;
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
    // SAFETY: `EVENT_HANDLER_DEPTH.get() == 0`, just checked above, means no
    // `EventLoop::handle_event` call (and therefore no other `&mut
    // EventLoop` borrow) is anywhere on this thread's stack. `event_loop` is
    // non-null (just checked) and, per `set_event_loop`/`clear_event_loop`'s
    // contract, either points at the live `EventLoop` or is null -- never
    // dangling.
    unsafe { (*event_loop).run_shutdown_body() };
    clear_event_loop();

    // A signal-initiated quit (SIGTERM/SIGHUP -- not applicable here, but
    // shared with the ordinary `LoopExiting` path for consistency) would
    // reraise its signal; this is a plain, deliberate exit.
    termination_signals::exit_after_signal_shutdown();
    termination_signals::hard_exit(0);
}
