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
//! # What the callback does (the options considered, and why)
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
//! (c) Keep the callback itself to: answer `WM_QUERYENDSESSION` `TRUE` at once, and on the first
//!     `WM_ENDSESSION(TRUE)` register a shutdown block reason, arm [`SHUTDOWN_DEADLINE`]'s
//!     watchdog, post the *same* [`CustomEvent::TerminateFromSignal`] the Unix `SIGTERM`/`SIGHUP`
//!     path already posts, and return. Superseded (jwp2987/phosphor#1075): Windows may end the
//!     session as soon as every window has returned from `WM_ENDSESSION`, so the graceful
//!     shutdown, which then ran on a later turn of the loop, raced the OS killing the process.
//! (d) **Chosen.** (c), but the callback does not return until the shutdown is over:
//!     [`run_shutdown_before_returning`] pumps *only* winit's wake-up message (a `PeekMessageW`
//!     filter, so no input, paint or timer message is dispatched re-entrantly), which delivers
//!     the posted `TerminateFromSignal` to the main loop's handler. That handler checks
//!     [`take_inline_shutdown_request`] and runs the exit sequence (`app_will_terminate`) itself
//!     rather than waiting for `LoopExiting`, which cannot come until this callback returns, and
//!     the exit sequence ends the process. Winit hands an event to the app only when its handler
//!     is not already running (otherwise it buffers it), so the nested dispatch cannot re-enter a
//!     handler that is mid-borrow; in that case the wait ends at the deadline instead. Nothing is
//!     destroyed at `WM_QUERYENDSESSION`, which is cancellable.
//!
//! When the block reason is registered, and why only then, is
//! [`termination_signals::session_end_step`]'s doc (jwp2987/phosphor#1072): it lives from
//! `WM_ENDSESSION(TRUE)` until [`notify_shutdown_complete`] (destroyed) or the deadline
//! (released by process exit), and the terminate request is only posted once the watchdog is
//! known to be running -- if it cannot be spawned the process exits at once instead of running
//! an unbounded shutdown.
//!
//! Every branch logs at info level with [`SESSION_END_LOG_PREFIX`], so a real logoff/restart
//! shows which path it took.
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
//!   process-wide [`SessionEndState`] below -- this was reasoned through, not observed.
//! - That the nested wake-up dispatch in [`run_shutdown_before_returning`] really reaches the
//!   main loop's `TerminateFromSignal` handler from inside `WM_ENDSESSION`, and that
//!   `app_will_terminate` finishes (`shutdown finished`/`graceful shutdown completed in` log
//!   lines; the `hard_exit` status 133/134 ends the process from inside the callback)
//!   (jwp2987/phosphor#1075).
//! - That a cancelled restart/sign-out (`WM_ENDSESSION(FALSE)`) leaves Phosphor running with
//!   its shell panes kept open (`crate::platform::session_end_gate`, jwp2987/phosphor#1103).
//! - That the deadline watchdog's [`hard_exit`][termination_signals::hard_exit]
//!   actually fires and is not itself blocked by whatever state Windows is in
//!   during a real session end.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use instant::Instant;
use parking_lot::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, RegisterWindowMessageA, WM_ENDSESSION,
    WM_QUERYENDSESSION,
};
use windows::core::{HSTRING, s};
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window as WinitWindow;

use crate::platform::session_end_gate::{self, PumpOutcome, pump_until};
use crate::platform::termination_signals::{
    self, SESSION_END_LOG_PREFIX, SHUTDOWN_DEADLINE, SessionEndEffect, SessionEndInput,
    SessionEndState, describe_end_session_flags, session_end_step, spawn_deadline_watchdog,
};
use crate::windowing::winit::app::CustomEvent;

/// A constant, process-private subclass ID for [`SetWindowSubclass`]. Only
/// needs to be distinct from any other subclass this process installs on the
/// *same* HWND with the *same* callback pointer -- nothing else in this crate
/// subclasses a window, so any value works; `773` is the tracking issue.
const SUBCLASS_ID: usize = 773;

const BLOCK_REASON: &str = "Saving your session…";

struct Runtime {
    state: SessionEndState,
    /// The HWND (as an integer, since `HWND` is not `Send`) the block reason is registered on.
    block_reason_hwnd: Option<isize>,
    /// When the terminate request was posted, for the completion log line.
    shutdown_started: Option<Instant>,
}

static RUNTIME: Mutex<Runtime> = Mutex::new(Runtime {
    state: SessionEndState::Idle,
    block_reason_hwnd: None,
    shutdown_started: None,
});

/// Set when `WM_ENDSESSION(TRUE)` posts its terminate request and consumed by the main loop's
/// `TerminateFromSignal` handler ([`take_inline_shutdown_request`]): that handler then runs the
/// exit sequence (`app_will_terminate`) right there instead of waiting for winit's `LoopExiting`,
/// which only comes once the `WM_ENDSESSION` callback has returned (jwp2987/phosphor#1075).
static INLINE_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether the `TerminateFromSignal` being handled came from `WM_ENDSESSION(TRUE)` and so must
/// run the exit sequence inline. Main thread.
pub(crate) fn take_inline_shutdown_request() -> bool {
    INLINE_SHUTDOWN_REQUESTED.swap(false, Ordering::AcqRel)
}

/// Posts a terminate request to the main loop -- set once, from the first window this process
/// opens. There is exactly one event loop regardless of how many windows exist.
static REQUEST_TERMINATE: OnceLock<Box<dyn Fn(i32) -> bool + Send + Sync>> = OnceLock::new();

/// Attaches the `WM_QUERYENDSESSION`/`WM_ENDSESSION` subclass to `window`'s
/// HWND. Called once per window, right after it is created
/// (`windowing::winit::event_loop`'s `CustomEvent::OpenWindow` handler).
pub(crate) fn install(window: &WinitWindow, proxy: EventLoopProxy<CustomEvent>) {
    let Ok(RawWindowHandle::Win32(handle)) = window.window_handle().map(|handle| handle.as_raw())
    else {
        log::warn!(
            "{SESSION_END_LOG_PREFIX} could not get this window's HWND; it will not react to \
             session end (jwp2987/phosphor#773)"
        );
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as _);

    REQUEST_TERMINATE.get_or_init(|| {
        Box::new(move |signal: i32| {
            proxy
                .send_event(CustomEvent::TerminateFromSignal(signal))
                .is_ok()
        }) as Box<dyn Fn(i32) -> bool + Send + Sync>
    });

    // SAFETY: `session_end_subclass_proc` only ever touches the process-wide
    // statics above (initialized just before this call, never torn down) and
    // the `hwnd`/`msg`/`wparam`/`lparam` Windows passes it, which is exactly
    // the contract `SetWindowSubclass`/`SUBCLASSPROC` document.
    let installed =
        unsafe { SetWindowSubclass(hwnd, Some(session_end_subclass_proc), SUBCLASS_ID, 0) };
    if !installed.as_bool() {
        log::warn!(
            "{SESSION_END_LOG_PREFIX} failed to install the WM_ENDSESSION subclass for this \
             window (jwp2987/phosphor#773); it will not react to session end"
        );
    }
}

/// Called on the main thread right after `app_will_terminate` returns, for every quit: releases
/// the block reason of a session-end shutdown, and logs how long it took.
pub(crate) fn notify_shutdown_complete() {
    let started = RUNTIME.lock().shutdown_started.take();
    match started {
        Some(started) => log::info!(
            "{SESSION_END_LOG_PREFIX} graceful shutdown completed in {}ms",
            started.elapsed().as_millis()
        ),
        None => log::info!(
            "{SESSION_END_LOG_PREFIX} app_will_terminate finished; no WM_ENDSESSION shutdown \
             was in progress"
        ),
    }
    let _ = drive(SessionEndInput::ShutdownCompleted, None);
    log::logger().flush();
}

/// The watchdog's exit path; runs on the watchdog thread.
fn on_deadline_elapsed(exit_code: i32) {
    let _ = drive(SessionEndInput::DeadlineElapsed, None);
    // `session_end_step` always asks for a hard exit once the watchdog was armed; this only
    // guards a state the machine says is unreachable.
    log::error!(
        "{SESSION_END_LOG_PREFIX} deadline elapsed with no session end in progress; exiting"
    );
    termination_signals::hard_exit(exit_code);
}

/// Feeds `input` (and whatever outcomes its effects produce) through
/// [`session_end_step`], performing each effect. `hwnd` is the window whose message is being
/// handled, if any. Returns the reply for that message.
fn drive(input: SessionEndInput, hwnd: Option<HWND>) -> (LRESULT, bool) {
    let mut reply = LRESULT(0);
    let mut shutdown_posted = false;
    let mut next_input = Some(input);
    while let Some(input) = next_input.take() {
        let effects = {
            let mut runtime = RUNTIME.lock();
            let step = session_end_step(runtime.state, input);
            log::info!(
                "{SESSION_END_LOG_PREFIX} {input:?}: {:?} -> {:?}, effects {:?}",
                runtime.state,
                step.next,
                step.effects
            );
            runtime.state = step.next;
            step.effects
        };
        for effect in effects {
            match effect {
                SessionEndEffect::AllowSessionEnd => {
                    log::info!(
                        "{SESSION_END_LOG_PREFIX} allowing the session end; no block reason \
                         registered"
                    );
                    reply = LRESULT(TRUE.0 as isize);
                }
                SessionEndEffect::CreateBlockReason => create_block_reason(hwnd),
                SessionEndEffect::DestroyBlockReason => destroy_block_reason(hwnd),
                SessionEndEffect::ArmDeadline {
                    deadline,
                    exit_code,
                } => match spawn_deadline_watchdog(deadline, exit_code, on_deadline_elapsed) {
                    Ok(()) => {
                        log::info!(
                            "{SESSION_END_LOG_PREFIX} deadline armed ({}s, exit code {exit_code})",
                            deadline.as_secs()
                        );
                        next_input = Some(SessionEndInput::DeadlineArmed);
                    }
                    Err(err) => {
                        log::error!(
                            "{SESSION_END_LOG_PREFIX} failed to spawn the deadline watchdog \
                             ({err}); exiting now rather than running an unbounded shutdown"
                        );
                        next_input = Some(SessionEndInput::DeadlineUnavailable);
                    }
                },
                SessionEndEffect::RequestTerminate { signal } => {
                    RUNTIME.lock().shutdown_started = Some(Instant::now());
                    // Set before posting so the main loop cannot see the request without it.
                    INLINE_SHUTDOWN_REQUESTED.store(true, Ordering::Release);
                    let delivered = REQUEST_TERMINATE
                        .get()
                        .is_some_and(|request_terminate| request_terminate(signal));
                    if delivered {
                        shutdown_posted = true;
                        log::info!(
                            "{SESSION_END_LOG_PREFIX} graceful shutdown begun \
                             (TerminateFromSignal({signal}) posted)"
                        );
                    } else {
                        INLINE_SHUTDOWN_REQUESTED.store(false, Ordering::Release);
                        log::warn!(
                            "{SESSION_END_LOG_PREFIX} main loop is gone; exiting without a \
                             graceful shutdown"
                        );
                        next_input = Some(SessionEndInput::TerminateUndeliverable);
                    }
                }
                SessionEndEffect::HardExit { exit_code } => {
                    log::warn!("{SESSION_END_LOG_PREFIX} hard_exit({exit_code}) after {input:?}");
                    termination_signals::hard_exit(exit_code);
                }
            }
        }
    }
    (reply, shutdown_posted)
}

fn create_block_reason(hwnd: Option<HWND>) {
    let Some(hwnd) = hwnd else {
        return;
    };
    // SAFETY: `hwnd` is a valid top-level window owned by this process, and this runs on the
    // thread that created it (its subclass proc), as `ShutdownBlockReasonCreate` requires.
    match unsafe { ShutdownBlockReasonCreate(hwnd, &HSTRING::from(BLOCK_REASON)) } {
        Ok(()) => {
            RUNTIME.lock().block_reason_hwnd = Some(hwnd.0 as isize);
            log::info!("{SESSION_END_LOG_PREFIX} block reason created");
        }
        Err(err) => {
            log::warn!("{SESSION_END_LOG_PREFIX} ShutdownBlockReasonCreate failed: {err}");
        }
    }
}

/// Releases the block reason on the window it was registered on, or else (defensively, for a
/// cancelled session end) on `hwnd`. Must run on the window's own thread.
fn destroy_block_reason(hwnd: Option<HWND>) {
    let registered = RUNTIME.lock().block_reason_hwnd.take();
    let Some(target) = registered.map(|hwnd| HWND(hwnd as _)).or(hwnd) else {
        return;
    };
    // SAFETY: `target` is a top-level window owned by this process, and this runs on the UI
    // thread that created it (its subclass proc, or `LoopExiting`).
    match unsafe { ShutdownBlockReasonDestroy(target) } {
        Ok(()) => log::info!("{SESSION_END_LOG_PREFIX} block reason destroyed"),
        // With nothing registered (the defensive cancel case) this is expected to fail.
        Err(err) if registered.is_none() => {
            log::debug!("{SESSION_END_LOG_PREFIX} no block reason to destroy: {err}");
        }
        Err(err) => {
            log::warn!("{SESSION_END_LOG_PREFIX} ShutdownBlockReasonDestroy failed: {err}");
        }
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
    let flags = lparam.0 as u32;
    let input = if msg == WM_QUERYENDSESSION {
        log::info!(
            "{SESSION_END_LOG_PREFIX} WM_QUERYENDSESSION received (lParam {})",
            describe_end_session_flags(flags)
        );
        // Answer at once and destroy nothing: the query is cancellable (another app, or the
        // user, can still veto the session end). The gate only keeps panes whose shells
        // Windows kills from closing meanwhile (jwp2987/phosphor#1103).
        session_end_gate::note_query();
        SessionEndInput::QueryEndSession { lparam: flags }
    } else if msg == WM_ENDSESSION {
        let session_ending = wparam.0 != 0;
        log::info!(
            "{SESSION_END_LOG_PREFIX} WM_ENDSESSION({}) received (lParam {})",
            if session_ending { "TRUE" } else { "FALSE" },
            describe_end_session_flags(flags)
        );
        session_end_gate::note_end_session(session_ending);
        SessionEndInput::EndSession {
            session_ending,
            lparam: flags,
        }
    } else {
        // SAFETY: chaining to the next handler in the subclass/window-proc
        // chain for a message this proc does not own, exactly as
        // `DefSubclassProc`'s contract requires.
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    };
    let (reply, shutdown_posted) = drive(input, Some(hwnd));
    log::logger().flush();
    if shutdown_posted {
        // Windows may end the session as soon as every window has returned from
        // `WM_ENDSESSION(TRUE)`, so the save must finish before this returns
        // (jwp2987/phosphor#1075). In practice it never does return: the exit sequence ends
        // the process.
        run_shutdown_before_returning();
    }
    reply
}

/// Delivers the posted `TerminateFromSignal` to the main loop from inside the
/// `WM_ENDSESSION` callback and waits (at most [`SHUTDOWN_DEADLINE`]) for the shutdown it runs.
///
/// Only winit's wake-up message is pumped (`PeekMessageW` filtered to it), not the whole queue,
/// so no other input/paint/timer message is dispatched re-entrantly. Dispatching it makes winit
/// call the app's event handler for the user event; the handler sees
/// [`take_inline_shutdown_request`] and runs the exit sequence on the spot. If this callback
/// was itself reached from inside the event handler, winit buffers that event instead and it
/// cannot run until we return; the wait then ends at the deadline (the watchdog armed for the
/// shutdown ends the process at the same time).
fn run_shutdown_before_returning() {
    // SAFETY: `RegisterWindowMessageA` is given a NUL-terminated literal. It returns the same id
    // for the same string process-wide, so this is the id winit's `EventLoopProxy::send_event`
    // posts (`Winit::WakeupMsg`).
    let wake_message = unsafe { RegisterWindowMessageA(s!("Winit::WakeupMsg")) };
    if wake_message == 0 {
        log::warn!(
            "{SESSION_END_LOG_PREFIX} could not look up winit's wake-up message; returning from \
             WM_ENDSESSION without waiting for the shutdown"
        );
        return;
    }
    let started = Instant::now();
    let outcome = pump_until(
        SHUTDOWN_DEADLINE,
        || started.elapsed(),
        || matches!(RUNTIME.lock().state, SessionEndState::Exiting { .. }),
        |_remaining| {
            let mut msg = MSG::default();
            // SAFETY: `msg` is a valid out-pointer; the filter admits only the wake-up
            // message, and dispatching it is what the thread's own message loop would do.
            while unsafe { PeekMessageW(&mut msg, None, wake_message, wake_message, PM_REMOVE) }
                .as_bool()
            {
                unsafe { DispatchMessageW(&msg) };
            }
            std::thread::sleep(Duration::from_millis(2));
        },
    );
    match outcome {
        PumpOutcome::Completed => log::info!(
            "{SESSION_END_LOG_PREFIX} shutdown finished inside WM_ENDSESSION after {}ms",
            started.elapsed().as_millis()
        ),
        PumpOutcome::TimedOut => log::warn!(
            "{SESSION_END_LOG_PREFIX} shutdown still running at the {}s deadline; returning from \
             WM_ENDSESSION",
            SHUTDOWN_DEADLINE.as_secs()
        ),
    }
    log::logger().flush();
}
