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
//! (c) **Chosen.** Keep the callback itself to: answer `WM_QUERYENDSESSION` `TRUE` at once,
//!     and on the first `WM_ENDSESSION(TRUE)` register a shutdown block reason, arm
//!     [`SHUTDOWN_DEADLINE`]'s watchdog, post the *same* [`CustomEvent::TerminateFromSignal`] the
//!     Unix `SIGTERM`/`SIGHUP` path already posts, and return. The graceful shutdown then runs
//!     on its normal, already-tested turn through the event loop (`LoopExiting` ->
//!     `app_will_terminate`) instead of inside this callback, so this module never touches
//!     `AppContext`/`EventLoop` state and never nests a message pump.
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
//! - That Windows lets `app_will_terminate` finish after this callback has returned from
//!   `WM_ENDSESSION`: the docs say the session "can end any time after all applications have
//!   returned from processing this message". The `graceful shutdown completed in` log line
//!   (or its absence) answers this (jwp2987/phosphor#1075).
//! - That the deadline watchdog's [`hard_exit`][termination_signals::hard_exit]
//!   actually fires and is not itself blocked by whatever state Windows is in
//!   during a real session end.

use std::sync::OnceLock;

use instant::Instant;
use parking_lot::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_QUERYENDSESSION};
use windows::core::HSTRING;
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window as WinitWindow;

use crate::platform::termination_signals::{
    self, SESSION_END_LOG_PREFIX, SessionEndEffect, SessionEndInput, SessionEndState,
    describe_end_session_flags, session_end_step, spawn_deadline_watchdog,
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
    drive(SessionEndInput::ShutdownCompleted, None);
    log::logger().flush();
}

/// The watchdog's exit path; runs on the watchdog thread.
fn on_deadline_elapsed(exit_code: i32) {
    drive(SessionEndInput::DeadlineElapsed, None);
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
fn drive(input: SessionEndInput, hwnd: Option<HWND>) -> LRESULT {
    let mut reply = LRESULT(0);
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
                    let delivered = REQUEST_TERMINATE
                        .get()
                        .is_some_and(|request_terminate| request_terminate(signal));
                    if delivered {
                        log::info!(
                            "{SESSION_END_LOG_PREFIX} graceful shutdown begun \
                             (TerminateFromSignal({signal}) posted)"
                        );
                    } else {
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
    reply
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
        SessionEndInput::QueryEndSession { lparam: flags }
    } else if msg == WM_ENDSESSION {
        let session_ending = wparam.0 != 0;
        log::info!(
            "{SESSION_END_LOG_PREFIX} WM_ENDSESSION({}) received (lParam {})",
            if session_ending { "TRUE" } else { "FALSE" },
            describe_end_session_flags(flags)
        );
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
    let reply = drive(input, Some(hwnd));
    log::logger().flush();
    reply
}
