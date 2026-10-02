use std::cell::{Cell, RefCell};
use std::time::Duration;

use instant::Instant;

use super::*;

/// Records every side effect instead of acting on the process.
#[derive(Default)]
struct FakeHooks {
    main_loop_gone: bool,
    terminate_requests: Cell<usize>,
    terminate_request_signals: RefCell<Vec<i32>>,
    deadlines: RefCell<Vec<(Duration, i32)>>,
    exits: RefCell<Vec<i32>>,
    initiating_signals: RefCell<Vec<i32>>,
}

impl ShutdownHooks for FakeHooks {
    fn request_terminate(&self, signal: i32) -> bool {
        self.terminate_requests
            .set(self.terminate_requests.get() + 1);
        self.terminate_request_signals.borrow_mut().push(signal);
        !self.main_loop_gone
    }

    fn arm_deadline(&self, deadline: Duration, exit_code: i32) {
        self.deadlines.borrow_mut().push((deadline, exit_code));
    }

    fn exit(&self, exit_code: i32) {
        self.exits.borrow_mut().push(exit_code);
    }

    fn record_initiating_signal(&self, signal: i32) {
        self.initiating_signals.borrow_mut().push(signal);
    }
}

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

#[test]
fn first_signal_requests_graceful_shutdown_and_arms_deadline() {
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();

    handle_signal(&mut state, SIGTERM, Instant::now(), &hooks);

    assert_eq!(hooks.terminate_requests.get(), 1);
    assert_eq!(*hooks.terminate_request_signals.borrow(), vec![SIGTERM]);
    assert_eq!(*hooks.deadlines.borrow(), vec![(SHUTDOWN_DEADLINE, 143)]);
    assert_eq!(*hooks.initiating_signals.borrow(), vec![SIGTERM]);
    assert!(
        hooks.exits.borrow().is_empty(),
        "the first signal must leave the exit to the graceful shutdown"
    );
}

#[test]
fn two_sighups_from_a_closing_terminal_stay_graceful() {
    // The shell forwards SIGHUP to its jobs, then the kernel sends it again when
    // the session leader exits. Neither may cut `app_will_terminate` short, however
    // far apart they arrive.
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();
    let start = Instant::now();

    handle_signal(&mut state, SIGHUP, start, &hooks);
    handle_signal(&mut state, SIGHUP, start + Duration::from_millis(5), &hooks);
    handle_signal(&mut state, SIGHUP, start + Duration::from_secs(3), &hooks);

    assert_eq!(hooks.terminate_requests.get(), 1);
    assert!(hooks.exits.borrow().is_empty(), "SIGHUP never escalates");
}

#[test]
fn a_different_signal_during_shutdown_does_not_escalate() {
    // logind sends SIGTERM then SIGHUP.
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();
    let start = Instant::now();

    handle_signal(&mut state, SIGTERM, start, &hooks);
    handle_signal(&mut state, SIGHUP, start + Duration::from_secs(2), &hooks);
    handle_signal(&mut state, SIGINT, start + Duration::from_secs(2), &hooks);

    assert_eq!(hooks.terminate_requests.get(), 1);
    assert!(hooks.exits.borrow().is_empty());
}

#[test]
fn a_quick_repeat_of_the_same_signal_is_a_duplicate() {
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();
    let start = Instant::now();

    handle_signal(&mut state, SIGTERM, start, &hooks);
    handle_signal(
        &mut state,
        SIGTERM,
        start + Duration::from_millis(200),
        &hooks,
    );

    assert!(hooks.exits.borrow().is_empty());
}

#[test]
fn sigterm_repeated_after_the_interval_exits_immediately() {
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();
    let start = Instant::now();

    handle_signal(&mut state, SIGTERM, start, &hooks);
    handle_signal(&mut state, SIGTERM, start + ESCALATION_MIN_INTERVAL, &hooks);

    assert_eq!(
        hooks.terminate_requests.get(),
        1,
        "a repeat must not queue another terminate"
    );
    assert_eq!(hooks.deadlines.borrow().len(), 1);
    assert_eq!(*hooks.exits.borrow(), vec![143]);
}

#[test]
fn a_signal_first_seen_mid_shutdown_escalates_only_on_its_own_repeat() {
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();
    let start = Instant::now();

    handle_signal(&mut state, SIGHUP, start, &hooks);
    handle_signal(&mut state, SIGINT, start + Duration::from_secs(2), &hooks);
    assert!(hooks.exits.borrow().is_empty());
    handle_signal(&mut state, SIGINT, start + Duration::from_secs(3), &hooks);

    assert_eq!(*hooks.exits.borrow(), vec![130]);
}

#[test]
fn exits_when_main_loop_cannot_receive_the_request() {
    let hooks = FakeHooks {
        main_loop_gone: true,
        ..Default::default()
    };
    let mut state = ShutdownState::default();

    handle_signal(&mut state, SIGTERM, Instant::now(), &hooks);

    assert_eq!(*hooks.exits.borrow(), vec![143]);
}

#[test]
fn escalation_interval_is_at_least_a_second() {
    assert!(ESCALATION_MIN_INTERVAL >= Duration::from_secs(1));
    assert!(ESCALATION_MIN_INTERVAL < SHUTDOWN_DEADLINE);
}

#[test]
fn deadline_watchdog_waits_the_deadline_then_exits() {
    let slept = Cell::new(None);
    let exited = Cell::new(None);

    run_deadline_watchdog(
        Duration::from_secs(5),
        143,
        |duration| {
            assert!(exited.get().is_none(), "must not exit before the deadline");
            slept.set(Some(duration));
        },
        |code| exited.set(Some(code)),
    );

    assert_eq!(slept.get(), Some(Duration::from_secs(5)));
    assert_eq!(exited.get(), Some(143));
}

#[test]
fn process_hooks_end_the_process_with_hard_exit() {
    // The watchdog and an escalated exit fire while the main thread is
    // mid-shutdown; `std::process::exit` would run atexit handlers and static
    // destructors concurrently with it.
    let hooks = ProcessHooks::new(|_signal| true);
    assert!(std::ptr::fn_addr_eq(hooks.exit, hard_exit as fn(i32)));
}

#[test]
fn the_armed_watchdog_exits_through_the_hooks_exit_seam() {
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};

    static EXITS: OnceLock<Mutex<Option<mpsc::Sender<i32>>>> = OnceLock::new();
    fn record_exit(code: i32) {
        if let Some(sender) = EXITS.get().and_then(|exits| exits.lock().unwrap().clone()) {
            let _ = sender.send(code);
        }
    }

    let (sender, receiver) = mpsc::channel();
    *EXITS.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(sender);
    let hooks = ProcessHooks {
        request_terminate: |_signal| true,
        exit: record_exit,
    };

    hooks.arm_deadline(Duration::from_millis(10), 143);

    assert_eq!(receiver.recv_timeout(Duration::from_secs(10)), Ok(143));
}

#[test]
fn a_completed_signal_shutdown_ends_as_the_signal_would() {
    assert_eq!(final_exit(None), FinalExit::Status(0));
    if cfg!(unix) {
        assert_eq!(final_exit(Some(SIGTERM)), FinalExit::Reraise(SIGTERM));
    } else {
        assert_eq!(final_exit(Some(SIGINT)), FinalExit::Status(130));
    }
}

// jwp2987/phosphor#726: `exit_after_signal_shutdown` (used by macOS and, before this
// fix, winit) reads the process-wide `INITIATING_SIGNAL` latch, which
// `record_initiating_signal` sets the moment *any* termination signal is first
// received -- regardless of whether that signal is what actually drove *this*
// particular quit to completion. A key binding or menu quit (`TerminationMode::
// Cancellable`/`ForceTerminate` from something other than the signal thread) can
// reach the very same `LoopExiting` handler while a concurrent, unrelated SIGTERM/
// SIGHUP is also being processed (e.g. a desktop session ending at the same moment
// the user quits by hand); the latch cannot tell the two apart, so the key-initiated
// quit would incorrectly re-raise a signal it had nothing to do with. winit now
// tracks, per its own termination request, whether *that* request came from the
// signal thread (`CustomEvent::TerminateFromSignal`) and calls
// `exit_after_signal_shutdown_for` with that explicit, request-scoped value instead
// of trusting the latch.
#[test]
fn a_request_not_attributed_to_a_signal_never_reraises_even_if_one_was_received() {
    // Simulate the latch already being poisoned by an unrelated signal (as it would
    // be after any earlier `record_initiating_signal` call in this process).
    INITIATING_SIGNAL.store(SIGTERM, Ordering::Release);

    // A caller (winit's `LoopExiting`, for a key/menu-initiated quit) that knows its
    // own request was not the signal's must pass that explicitly, rather than calling
    // the latch-reading `exit_after_signal_shutdown`.
    assert_eq!(final_exit(None), FinalExit::Status(0));

    // The pure decision function agreeing is not enough by itself: the entry point
    // winit actually calls must also treat `None` as the ordinary, non-reraising
    // no-op. If it instead consulted the (still-poisoned) latch, this call would
    // reset SIGTERM's disposition and raise it on this very test process.
    exit_after_signal_shutdown_for(None);

    // Restore the latch so this test cannot affect any test that runs after it in
    // the same process.
    INITIATING_SIGNAL.store(0, Ordering::Release);
}

#[test]
fn deadline_is_bounded() {
    // The watchdog is what stops a wedged flush from making the app ignore
    // SIGTERM; keep it short enough that session managers don't SIGKILL first.
    assert!(SHUTDOWN_DEADLINE <= Duration::from_secs(5));
    assert!(SHUTDOWN_DEADLINE > Duration::ZERO);
}

#[test]
fn exit_code_follows_the_shell_convention() {
    assert_eq!(exit_code_for_signal(2), 130);
    assert_eq!(exit_code_for_signal(SIGTERM), 143);
}

#[cfg(unix)]
#[test]
fn signal_sets_cover_sigterm_and_sighup() {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    assert!(GUI_TERMINATION_SIGNALS.contains(&SIGTERM));
    assert!(GUI_TERMINATION_SIGNALS.contains(&SIGHUP));
    assert!(HEADLESS_TERMINATION_SIGNALS.contains(&SIGTERM));
    assert!(HEADLESS_TERMINATION_SIGNALS.contains(&SIGHUP));
    assert!(
        HEADLESS_TERMINATION_SIGNALS.contains(&SIGINT),
        "the headless loop has always handled Ctrl-C"
    );
}

// jwp2987/phosphor#773 (Windows follow-up to #685): `ctrl_event_shutdown_reason`
// is the one part of the console-close/logoff/shutdown path that touches no OS
// API, so -- unlike `termination_signals::console`, which is `#[cfg(windows)]`
// -- it builds and runs on every platform this suite does.
mod ctrl_event_shutdown_reason {
    use super::ctrl_event_shutdown_reason;

    // `windows::Win32::System::Console`'s own constants, duplicated here as the
    // function under test duplicates them, so neither needs the `windows` crate.
    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;
    const CTRL_CLOSE_EVENT: u32 = 2;
    const CTRL_LOGOFF_EVENT: u32 = 5;
    const CTRL_SHUTDOWN_EVENT: u32 = 6;

    #[test]
    fn close_logoff_and_shutdown_map_to_their_own_event_code() {
        assert_eq!(ctrl_event_shutdown_reason(CTRL_CLOSE_EVENT), Some(2));
        assert_eq!(ctrl_event_shutdown_reason(CTRL_LOGOFF_EVENT), Some(5));
        assert_eq!(ctrl_event_shutdown_reason(CTRL_SHUTDOWN_EVENT), Some(6));
    }

    #[test]
    fn ctrl_c_and_ctrl_break_are_not_claimed() {
        // The headless loop's own `ctrlc` handler, installed separately, already
        // owns these (mapped to `SIGINT`); claiming them here too would race it.
        assert_eq!(ctrl_event_shutdown_reason(CTRL_C_EVENT), None);
        assert_eq!(ctrl_event_shutdown_reason(CTRL_BREAK_EVENT), None);
    }

    #[test]
    fn an_unrecognized_event_code_is_not_claimed() {
        assert_eq!(ctrl_event_shutdown_reason(99), None);
    }
}

// jwp2987/phosphor#773: `session_end_action` is the pure decision behind the
// `WM_QUERYENDSESSION`/`WM_ENDSESSION` window subclass, so -- like
// `ctrl_event_shutdown_reason` above -- it builds and runs on every platform,
// not only Windows.
mod session_end_action {
    use super::{SessionEndAction, session_end_action};

    const QUERY_END_SESSION: bool = true;
    const END_SESSION: bool = false;

    #[test]
    fn query_end_session_always_blocks_and_allows() {
        // Never veto a shutdown/logoff, and this is independent of whether a
        // shutdown is already underway or what a later `WM_ENDSESSION` will say.
        assert_eq!(
            session_end_action(QUERY_END_SESSION, false, false, false),
            SessionEndAction::BlockAndAllow
        );
        assert_eq!(
            session_end_action(QUERY_END_SESSION, true, true, true),
            SessionEndAction::BlockAndAllow
        );
    }

    #[test]
    fn first_endsession_true_begins_a_graceful_shutdown() {
        assert_eq!(
            session_end_action(END_SESSION, true, false, false),
            SessionEndAction::BeginGracefulShutdown { reason_signal: 6 },
        );
    }

    #[test]
    fn logoff_maps_to_the_console_paths_logoff_reason() {
        assert_eq!(
            session_end_action(END_SESSION, true, true, false),
            SessionEndAction::BeginGracefulShutdown { reason_signal: 5 },
        );
    }

    #[test]
    fn a_second_windows_endsession_true_is_ignored_once_already_shutting_down() {
        // Windows sends `WM_ENDSESSION` to every top-level window for the one
        // real shutdown attempt; only the first should start anything.
        assert_eq!(
            session_end_action(END_SESSION, true, false, true),
            SessionEndAction::AlreadyShuttingDown
        );
        assert_eq!(
            session_end_action(END_SESSION, true, true, true),
            SessionEndAction::AlreadyShuttingDown
        );
    }

    #[test]
    fn endsession_false_is_a_cancellation_regardless_of_the_latch() {
        // Some other application vetoed the session end.
        assert_eq!(
            session_end_action(END_SESSION, false, false, false),
            SessionEndAction::Cancelled
        );
        assert_eq!(
            session_end_action(END_SESSION, false, false, true),
            SessionEndAction::Cancelled
        );
    }
}

#[test]
fn a_repeated_console_close_event_escalates_like_sigterm() {
    // CTRL_CLOSE_EVENT (2) is not SIGHUP, so a repeat after the escalation
    // interval must still exit immediately, exactly as a repeated SIGTERM does.
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();
    let start = Instant::now();
    const CTRL_CLOSE_EVENT: i32 = 2;

    handle_signal(&mut state, CTRL_CLOSE_EVENT, start, &hooks);
    handle_signal(
        &mut state,
        CTRL_CLOSE_EVENT,
        start + ESCALATION_MIN_INTERVAL,
        &hooks,
    );

    assert_eq!(hooks.terminate_requests.get(), 1);
    assert_eq!(
        *hooks.exits.borrow(),
        vec![exit_code_for_signal(CTRL_CLOSE_EVENT)]
    );
}

mod approve_termination {
    use std::cell::Cell;

    use crate::platform::TerminationMode;
    use crate::platform::app::{ApproveTerminateResult, approve_termination};

    #[test]
    fn force_terminate_skips_the_quit_confirmation() {
        // A signal-initiated quit is ForceTerminate; nobody can answer the
        // "Quit Phosphor? processes are running" dialog.
        let asked = Cell::new(false);
        let approved = approve_termination(TerminationMode::ForceTerminate, || {
            asked.set(true);
            ApproveTerminateResult::Cancel
        });
        assert!(approved);
        assert!(
            !asked.get(),
            "ForceTerminate must not consult the confirmation"
        );
    }

    #[test]
    fn cancellable_terminate_still_asks() {
        let asked = Cell::new(false);
        let approved = approve_termination(TerminationMode::Cancellable, || {
            asked.set(true);
            ApproveTerminateResult::Cancel
        });
        assert!(asked.get());
        assert!(!approved, "a cancelled confirmation must block the quit");

        assert!(approve_termination(TerminationMode::Cancellable, || {
            ApproveTerminateResult::Terminate
        }));
    }
}

mod termination_attribution {
    use crate::platform::termination_signals::TerminationAttribution;

    #[test]
    fn an_ordinary_request_completes_with_no_signal() {
        let attribution = TerminationAttribution::new();
        let generation = attribution.request(None);
        assert_eq!(attribution.complete(generation), None);
    }

    #[test]
    fn a_signal_initiated_request_completes_with_its_signal() {
        let attribution = TerminationAttribution::new();
        let generation = attribution.request(Some(15));
        assert_eq!(attribution.complete(generation), Some(15));
    }

    #[test]
    fn an_ordinary_request_racing_in_after_a_signal_one_does_not_clear_it() {
        // The signal's request gets there first and is in flight; the
        // ordinary request (e.g. an unrelated Cmd+Q) racing in afterward must
        // not be able to overwrite its attribution with `None` -- that was
        // exactly the "ordinary quit overwrote Some(sig) with None and
        // completed first" bug (jwp2987/phosphor#791).
        let attribution = TerminationAttribution::new();
        let signal_generation = attribution.request(Some(15));
        let racing_generation = attribution.request(None);

        // First-writer-wins: the racing request does not get its own
        // generation, it is told which request actually owns the in-flight
        // attribution.
        assert_eq!(racing_generation, signal_generation);
        assert_eq!(attribution.complete(signal_generation), Some(15));
    }

    #[test]
    fn a_signal_request_racing_in_after_an_ordinary_one_does_not_steal_it() {
        // Symmetric case: an ordinary quit (e.g. the Quit menu item) is
        // already in flight when a real signal arrives. The signal's request
        // must not be able to attach its signal to the completion that is
        // actually the ordinary request's -- that was the "Cmd+Q can consume
        // a stale Some" bug (jwp2987/phosphor#791).
        let attribution = TerminationAttribution::new();
        let ordinary_generation = attribution.request(None);
        let racing_generation = attribution.request(Some(15));

        assert_eq!(racing_generation, ordinary_generation);
        assert_eq!(attribution.complete(ordinary_generation), None);
    }

    #[test]
    fn completing_a_stale_generation_reports_nothing_and_does_not_disturb_the_owner() {
        let attribution = TerminationAttribution::new();
        let owning_generation = attribution.request(Some(15));
        let stale_generation = owning_generation.wrapping_sub(1);

        assert_eq!(attribution.complete(stale_generation), None);
        // The real owner's attribution is untouched by the stale completion.
        assert_eq!(attribution.complete(owning_generation), Some(15));
    }

    #[test]
    fn cancelling_clears_the_attribution_for_a_fresh_request() {
        // The user declined the "Quit Phosphor?" confirmation: no completion
        // ever happens for this generation, so it must be cancelled rather
        // than left in flight forever, or every future quit attempt would be
        // told a shutdown is already in progress and lose its own
        // attribution to first-writer-wins.
        let attribution = TerminationAttribution::new();
        let declined_generation = attribution.request(Some(15));
        attribution.cancel(declined_generation);

        let next_generation = attribution.request(None);
        assert_ne!(next_generation, declined_generation);
        assert_eq!(attribution.complete(next_generation), None);
    }

    #[test]
    fn cancelling_a_non_owning_generation_is_a_no_op() {
        let attribution = TerminationAttribution::new();
        let generation = attribution.request(Some(15));
        attribution.cancel(generation.wrapping_sub(1));

        // Still in flight, still attributed to the real owner.
        assert_eq!(attribution.complete(generation), Some(15));
    }

    #[test]
    fn completing_clears_the_state_for_the_next_independent_request() {
        let attribution = TerminationAttribution::new();
        let first = attribution.request(Some(15));
        assert_eq!(attribution.complete(first), Some(15));

        let second = attribution.request(None);
        assert_ne!(second, first);
        assert_eq!(attribution.complete(second), None);
    }
}
