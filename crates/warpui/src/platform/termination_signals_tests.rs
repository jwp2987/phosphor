use std::cell::{Cell, RefCell};
use std::time::Duration;

use super::*;

/// Records every side effect instead of acting on the process.
#[derive(Default)]
struct FakeHooks {
    main_loop_gone: bool,
    terminate_requests: Cell<usize>,
    deadlines: RefCell<Vec<(Duration, i32)>>,
    exits: RefCell<Vec<i32>>,
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
}

const SIGTERM: i32 = 15;
const SIGHUP: i32 = 1;

#[test]
fn first_signal_requests_graceful_shutdown_and_arms_deadline() {
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();

    handle_signal(&mut state, SIGTERM, &hooks);

    assert_eq!(hooks.terminate_requests.get(), 1);
    assert_eq!(*hooks.deadlines.borrow(), vec![(SHUTDOWN_DEADLINE, 143)]);
    assert!(
        hooks.exits.borrow().is_empty(),
        "the first signal must leave the exit to the graceful shutdown"
    );
}

#[test]
fn second_signal_during_shutdown_exits_immediately() {
    let hooks = FakeHooks::default();
    let mut state = ShutdownState::default();

    handle_signal(&mut state, SIGTERM, &hooks);
    handle_signal(&mut state, SIGHUP, &hooks);

    assert_eq!(
        hooks.terminate_requests.get(),
        1,
        "a second signal must not queue another terminate"
    );
    assert_eq!(hooks.deadlines.borrow().len(), 1);
    assert_eq!(*hooks.exits.borrow(), vec![129]);
}

#[test]
fn exits_when_main_loop_cannot_receive_the_request() {
    let hooks = FakeHooks {
        main_loop_gone: true,
        ..Default::default()
    };
    let mut state = ShutdownState::default();

    handle_signal(&mut state, SIGTERM, &hooks);

    assert_eq!(*hooks.exits.borrow(), vec![143]);
}

#[test]
fn shutdown_state_escalates_only_after_the_first_signal() {
    let mut state = ShutdownState::default();
    assert_eq!(state.on_signal(), SignalResponse::BeginGracefulShutdown);
    assert_eq!(state.on_signal(), SignalResponse::ExitImmediately);
    assert_eq!(state.on_signal(), SignalResponse::ExitImmediately);
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
