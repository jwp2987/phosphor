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

mod shutdown_gate {
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::super::ShutdownGate;

    #[test]
    fn signal_before_wait_is_not_lost() {
        let gate = ShutdownGate::new();
        gate.signal();

        let woken = gate.wait(Duration::from_secs(5));
        assert!(woken, "a signal delivered before wait() must still count");
    }

    #[test]
    fn wait_without_a_signal_times_out() {
        let gate = ShutdownGate::new();
        let start = Instant::now();

        let woken = gate.wait(Duration::from_millis(50));

        assert!(!woken);
        assert!(
            start.elapsed() >= Duration::from_millis(50),
            "must actually wait out the deadline, not return early"
        );
    }

    #[test]
    fn signal_wakes_a_blocked_waiter_before_the_deadline() {
        let gate = Arc::new(ShutdownGate::new());
        let (ready_tx, ready_rx) = mpsc::channel::<()>();

        let waiter_gate = gate.clone();
        let waiter = thread::spawn(move || {
            // No exact synchronization point exists for "now inside wait()";
            // signal a best-effort readiness right before entering it and give
            // the main thread a moment to act on it. The real assertion is
            // that `join` below returns `true` well under the 10s deadline
            // it's racing, not the timing of this handoff.
            let _ = ready_tx.send(());
            waiter_gate.wait(Duration::from_secs(10))
        });

        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("waiter thread should have started");
        thread::sleep(Duration::from_millis(50));

        let start = Instant::now();
        gate.signal();

        let woken = waiter.join().unwrap();
        assert!(woken, "signal() must wake the waiter, not the 10s deadline");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the waiter should return promptly after signal(), not sleep out the deadline"
        );
    }
}

mod reentrancy_depth {
    use super::super::ReentrancyDepth;

    #[test]
    fn starts_at_zero() {
        let depth = ReentrancyDepth::new();
        assert_eq!(depth.get(), 0);
    }

    #[test]
    fn tracks_nesting_and_unwinds_in_order() {
        let depth = ReentrancyDepth::new();

        let outer = depth.enter();
        assert_eq!(depth.get(), 1);
        {
            let inner = depth.enter();
            assert_eq!(depth.get(), 2);
            drop(inner);
        }
        assert_eq!(
            depth.get(),
            1,
            "dropping the inner guard must not touch the outer one's count"
        );
        drop(outer);
        assert_eq!(depth.get(), 0);
    }

    #[test]
    fn a_panic_while_entered_still_decrements() {
        // This is the whole point of `enter()` returning a guard rather than
        // requiring a matched `enter`/`leave` pair: a caller that panics
        // mid-body must not leave the depth stuck above zero forever, or a
        // later, perfectly ordinary call would wrongly conclude someone else
        // still has the guarded value borrowed.
        let depth = std::sync::Arc::new(ReentrancyDepth::new());
        let depth_for_panic = depth.clone();

        let result = std::panic::catch_unwind(move || {
            let _guard = depth_for_panic.enter();
            assert_eq!(depth_for_panic.get(), 1);
            panic!("boom");
        });

        assert!(result.is_err());
        assert_eq!(
            depth.get(),
            0,
            "the guard's Drop must run during unwinding, same as normal drop"
        );
    }
}

mod console_shutdown_deadlines {
    use std::time::Duration;

    use super::super::{CONSOLE_CLOSE_DEADLINE, CONSOLE_LOGOFF_DEADLINE};

    #[test]
    fn deadlines_leave_margin_under_their_documented_os_budget() {
        // CTRL_CLOSE_EVENT's budget is ~5s with no configurable margin;
        // CTRL_LOGOFF_EVENT/CTRL_SHUTDOWN_EVENT/WM_ENDSESSION's is ~20s.
        assert!(CONSOLE_CLOSE_DEADLINE < Duration::from_secs(5));
        assert!(CONSOLE_LOGOFF_DEADLINE < Duration::from_secs(20));
        assert!(
            CONSOLE_CLOSE_DEADLINE < CONSOLE_LOGOFF_DEADLINE,
            "close's budget is far tighter than logoff/shutdown's"
        );
    }
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
