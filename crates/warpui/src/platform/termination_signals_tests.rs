use std::cell::{Cell, RefCell};
use std::time::Duration;

use instant::Instant;

use super::*;

/// Records every side effect instead of acting on the process.
#[derive(Default)]
struct FakeHooks {
    main_loop_gone: bool,
    terminate_requests: Cell<usize>,
    deadlines: RefCell<Vec<(Duration, i32)>>,
    exits: RefCell<Vec<i32>>,
    initiating_signals: RefCell<Vec<i32>>,
}

impl ShutdownHooks for FakeHooks {
    fn request_terminate(&self) -> bool {
        self.terminate_requests
            .set(self.terminate_requests.get() + 1);
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
    let hooks = ProcessHooks::new(|| true);
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
        request_terminate: || true,
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
