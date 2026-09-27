use super::{SURVIVED_SIGNALS, TERMINAL_SERVER_SIGNALS};

#[test]
fn terminal_server_survives_hangup_and_terminate() {
    // jwp2987/phosphor#685 review: a terminal close (SIGHUP) or session stop
    // (SIGTERM) must not kill the terminal server ahead of the host's graceful quit.
    for signal in [signal_hook::consts::SIGHUP, signal_hook::consts::SIGTERM] {
        assert!(SURVIVED_SIGNALS.contains(&signal));
        assert!(
            TERMINAL_SERVER_SIGNALS.contains(&signal),
            "a survived signal must be caught, or it takes its default action"
        );
    }
}

#[test]
fn terminal_server_still_handles_child_exits() {
    assert!(TERMINAL_SERVER_SIGNALS.contains(&signal_hook::consts::SIGCHLD));
    assert!(!SURVIVED_SIGNALS.contains(&signal_hook::consts::SIGCHLD));
}
