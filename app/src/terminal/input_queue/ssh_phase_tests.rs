//! Tests of the edge-driven ssh phase (#1424, step 7): each edge moves the phase the way the
//! connect gate's `drops_keys` and the swallow window do, and nothing but an edge moves it. The
//! pre-ownership hold ends at [`SshEdge::TtyOwned`] (#1611, #1612, #1621), never at anything read
//! from the screen; the tests of the old prompt latch (a banner opening the gate, an unterminated
//! line being a prompt, a managed remote never being a password prompt) were removed with it,
//! which is the owner's decision applied, not a weakened test.

use super::*;

fn machine() -> SshPhaseMachine {
    SshPhaseMachine::default()
}

fn armed() -> SshPhaseMachine {
    let mut m = machine();
    m.apply(SshEdge::Submitted { ssh: true });
    m
}

/// An armed window whose ssh owns the tty (the common state after the 50 ms settle).
fn owned() -> SshPhaseMachine {
    let mut m = armed();
    m.apply(SshEdge::TtyOwned);
    m
}

fn observed(f: impl FnOnce(&mut SshObservation)) -> SshEdge {
    let mut o = SshObservation::default();
    f(&mut o);
    SshEdge::Observed(o)
}

#[test]
fn an_idle_machine_is_idle_and_does_nothing_on_events() {
    let m = machine();
    assert_eq!(m.phase(false), SshPhase::Idle);
    assert!(!m.is_active());
}

#[test]
fn a_non_ssh_command_leaves_the_machine_idle() {
    let mut m = machine();
    m.apply(SshEdge::Submitted { ssh: false });
    assert_eq!(m.phase(false), SshPhase::Idle);
    assert!(!m.armed());
}

#[test]
fn submitting_ssh_holds_until_ssh_owns_the_tty() {
    // Nothing owns the tty yet: the gate drops keys, the phase holds (legacy
    // `ssh_connecting_silently`). Output of any shape changes nothing: only the ownership edge
    // ends the pre-ownership hold.
    let mut m = armed();
    assert!(m.armed());
    assert!(!m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(observed(|_| {}));
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::TtyOwned);
    assert!(m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_window_is_born_unowned_under_test_on_every_platform() {
    // On a shipped Windows build `TTY_OWNED_AT_ARM` is true (no ownership event on ConPTY, and a
    // timer never puts keys into a pty: keys go to the pty in order from the submit, as a plain
    // console gives them to ssh.exe); the tests drive the edge by hand everywhere.
    assert!(!TTY_OWNED_AT_ARM);
}

#[test]
fn ownership_before_the_window_is_not_remembered() {
    // The edge belongs to a window; an idle machine ignores it and a later window starts unowned.
    let mut m = machine();
    m.apply(SshEdge::TtyOwned);
    assert!(!m.is_active());
    m.apply(SshEdge::Submitted { ssh: true });
    assert!(!m.tty_owned());
}

#[test]
fn a_full_screen_program_never_holds() {
    let m = armed();
    assert_eq!(m.phase(true), SshPhase::Idle);
}

#[test]
fn an_owned_tty_opens_the_gate_and_the_phase_is_idle_until_a_key_is_held() {
    let mut m = owned();
    assert_eq!(m.phase(false), SshPhase::Idle);
    m.apply(SshEdge::HoldStarted);
    // Once anything is held every later key is held too (#1292).
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::HoldEnded);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn ownership_is_sticky_for_the_window() {
    let mut m = owned();
    m.apply(observed(|_| {}));
    m.apply(observed(|o| o.hook_seen = true));
    assert!(m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn the_announced_session_holds_through_its_bootstrap() {
    let mut m = owned();
    m.apply(observed(|o| o.pending_session = true));
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn the_setup_dialog_swallows_instead_of_holding() {
    let mut m = armed();
    m.apply(observed(|o| {
        o.pending_session = true;
        o.dialog_up = true;
        o.setup_in_flight = true;
    }));
    // Row H2 comes first in the table; the phase is not Holding for the dialog.
    assert_eq!(m.phase(false), SshPhase::Idle);
    // The dialog resolved, the setup runs: held.
    m.apply(observed(|o| {
        o.pending_session = true;
        o.setup_in_flight = true;
    }));
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn the_setup_state_event_holds_even_before_the_gate_is_armed() {
    // The remote-setup SessionsEvent: an in-flight setup for the pending session holds keys
    // whatever armed the window (legacy `ssh_setup_in_flight` needs no gate).
    let mut m = machine();
    m.apply(observed(|o| {
        o.pending_session = true;
        o.setup_in_flight = true;
    }));
    assert_eq!(m.phase(false), SshPhase::Holding);
    // The setup finished.
    m.apply(observed(|o| o.pending_session = true));
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn the_bootstrap_failure_ends_the_swallow() {
    let mut m = owned();
    m.apply(observed(|o| o.pending_session = true));
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::BootstrapFailed);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

/// #1808: no clock ends the bootstrap hold; the pending session stays held until an event.
#[test]
fn the_bootstrap_hold_has_no_clock() {
    let mut m = owned();
    m.apply(observed(|o| o.pending_session = true));
    m.apply(observed(|o| o.pending_session = true));
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn a_new_announcement_swallows_again() {
    let mut m = owned();
    m.apply(observed(|o| o.pending_session = true));
    m.apply(SshEdge::BootstrapFailed);
    m.apply(observed(|_| {}));
    m.apply(observed(|o| o.pending_session = true));
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn bootstrapped_holds_until_the_first_prompt() {
    let mut m = owned();
    m.apply(observed(|o| o.pending_session = true));
    // Bootstrapped: the session is no longer pending. The remote bootstrap is finishing; keys
    // written now became remote typeahead the shell handed back in pieces (#1085).
    m.apply(observed(|_| {}));
    assert!(m.bootstrap_done());
    assert_eq!(m.phase(false), SshPhase::Holding);
    // The first prompt: the window is released by the event.
    m.apply(SshEdge::Released(SshRelease::FirstPrompt));
    assert!(!m.armed());
    assert_eq!(m.last_release(), Some(SshRelease::FirstPrompt));
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_clean_block_end_on_a_managed_remote_keeps_holding_for_the_bootstrap() {
    let mut m = owned();
    m.apply(observed(|o| o.hook_seen = true));
    m.apply(SshEdge::BlockCompleted { clean: true });
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn a_failed_block_ends_the_gate_and_the_hold_stays_until_it_is_handed_over() {
    let mut m = armed();
    m.apply(SshEdge::HoldStarted);
    m.apply(SshEdge::BlockCompleted { clean: false });
    // The gate no longer drops keys; the held keys are still held until the hand-over.
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::Released(SshRelease::Failed));
    m.apply(SshEdge::HoldEnded);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_clean_block_end_with_no_managed_remote_is_a_failure_of_the_window() {
    let mut m = owned();
    m.apply(SshEdge::BlockCompleted { clean: true });
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn preexec_arms_a_window_nothing_armed() {
    let mut m = machine();
    m.apply(SshEdge::ConnectStart);
    assert!(m.armed());
    assert!(!m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Holding);
    // A second start changes nothing.
    m.apply(SshEdge::TtyOwned);
    m.apply(SshEdge::ConnectStart);
    assert!(m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_submit_ends_the_previous_window_and_its_hold() {
    let mut m = owned();
    m.apply(SshEdge::HoldStarted);
    m.apply(SshEdge::Submitted { ssh: false });
    assert!(!m.armed());
    assert!(!m.holding());
    assert!(!m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn every_give_up_is_named_and_only_the_timers_are_give_ups() {
    for how in [
        SshRelease::FirstPrompt,
        SshRelease::Failed,
        SshRelease::OtherCommand,
        SshRelease::ReplacedShellPlain,
    ] {
        assert!(!how.is_give_up(), "{} is an event", how.name());
    }
    for how in [
        SshRelease::GiveUpCeiling,
        SshRelease::GiveUpFirstPromptGrace,
        SshRelease::GiveUpPostBlockGrace,
    ] {
        assert!(how.is_give_up(), "{} is a timer", how.name());
        assert!(how.name().starts_with("give_up"));
    }
}

#[test]
fn a_release_keeps_the_session_and_the_hold_it_does_not_own() {
    let mut m = armed();
    m.apply(SshEdge::HoldStarted);
    m.apply(observed(|o| o.pending_session = true));
    m.apply(SshEdge::Released(SshRelease::GiveUpCeiling));
    assert!(!m.armed());
    // The hold is ended by its own edge, after the hand-over.
    assert!(m.holding());
    assert!(m.is_active());
}

#[test]
fn nothing_moves_the_phase_but_an_edge() {
    let m = owned();
    let before = m.phase(false);
    // Reading the phase any number of times, with any alt-screen flag, changes nothing.
    let _ = m.phase(true);
    let _ = m.phase(false);
    assert_eq!(m.phase(false), before);
}

/// #1641: under the ssh wrapper the view defers the ownership edge, so the window keeps holding;
/// the one pass-through is a secret read observed while ssh owns the tty, and it lasts only as
/// long as the observation says so. Held keys stay held whatever is observed (the view discards
/// them first, as the termios guard does).
#[test]
fn a_secret_read_under_the_wrapper_hold_passes_and_only_while_it_lasts() {
    let mut m = armed();
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(observed(|o| o.secret_pass_through = true));
    assert_eq!(m.phase(false), SshPhase::Idle, "the password reaches ssh");
    m.apply(observed(|_| {}));
    assert_eq!(
        m.phase(false),
        SshPhase::Holding,
        "held again after the read"
    );
    m.apply(SshEdge::HoldStarted);
    m.apply(observed(|o| o.secret_pass_through = true));
    assert_eq!(
        m.phase(false),
        SshPhase::Holding,
        "a live hold is never overtaken"
    );
}

/// #1885 (Windows audit finding 4): a window born owned (ConPTY has no tcgetpgrp, so ssh ownership
/// is assumed at arm time) passes keys straight to whatever reads the tty, exactly like a plain
/// terminal, and stays a pass-through when a secret read is observed (a `TtyRead` marker from a
/// local PowerShell `Read-Host` counts as the observation; a remote login script's `read -s` has no
/// marker and is the plain-terminal path). The pass-through never depends on screen text.
#[test]
fn an_owned_window_passes_keys_like_a_plain_terminal_secret_read_or_not() {
    let mut m = owned();
    assert_eq!(m.phase(false), SshPhase::Idle);
    m.apply(observed(|o| o.secret_pass_through = true));
    assert_eq!(m.phase(false), SshPhase::Idle);
    m.apply(observed(|_| {}));
    assert_eq!(m.phase(false), SshPhase::Idle);
}

/// #1641: the ownership edge overtook the wrapper's announcement; the announcement takes the tty
/// back (the master login owns it), so keys are held again until the verdict.
#[test]
fn a_reclaimed_tty_holds_again() {
    let mut m = owned();
    assert_eq!(m.phase(false), SshPhase::Idle);
    m.apply(SshEdge::TtyReclaimed);
    assert!(!m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Holding);
    // An idle machine ignores it.
    let mut idle = machine();
    idle.apply(SshEdge::TtyReclaimed);
    assert_eq!(idle.phase(false), SshPhase::Idle);
}

/// #2120: the remote command bootstraps bash and zsh (and, since the #2019 follow-up, fish and nu
/// through the staged shim) and nothing else, so only their `SSH` hook
/// announces a session. The names are compared whole, as the remote command's `case` does: a
/// login shell called `bash5` or `-bash` is started plain by it as well.
#[test]
fn only_a_shell_the_remote_command_bootstraps_announces_a_session() {
    for shell in ["bash", "zsh", "fish", "nu"] {
        assert!(ssh_remote_command_bootstraps(shell), "{shell}");
        assert!(ssh_hook_announces_session_with(shell, true), "{shell}");
    }
    for shell in [
        "sh", "dash", "ash", "ksh", "ksh93", "mksh", "busybox", "tcsh", "csh", "xonsh", "bash5",
        "-bash", "Bash", "zsh-5.9", "fish5", "Nu", "",
    ] {
        assert!(!ssh_remote_command_bootstraps(shell), "{shell:?}");
        assert!(!ssh_hook_announces_session_with(shell, true), "{shell:?}");
        // The kill switch: every hook announces a session again.
        assert!(ssh_hook_announces_session_with(shell, false), "{shell:?}");
    }
}

/// #2120, the machine's half. A window the wrapper took back (`TtyReclaimed`) and whose remote
/// printed the `SSH` hook of a shell that is not bootstrapped: the model reports no hook
/// (`hook_seen` false) and the view sends the ownership edge at that event. The window is then a
/// plain ssh's: keys pass, and a clean end of the block is "nothing more is coming", not a wait
/// for a bootstrap.
#[test]
fn a_hook_that_announces_no_session_leaves_a_plain_window() {
    let mut m = owned();
    m.apply(SshEdge::TtyReclaimed);
    assert_eq!(m.phase(false), SshPhase::Holding, "held for the wrapper");
    m.apply(observed(|_| {}));
    m.apply(SshEdge::TtyOwned);
    assert!(m.tty_owned());
    assert_eq!(m.phase(false), SshPhase::Idle, "keys go straight to ssh");
    m.apply(observed(|_| {}));
    assert_eq!(m.phase(false), SshPhase::Idle);
    m.apply(SshEdge::BlockCompleted { clean: true });
    assert_eq!(
        m.phase(false),
        SshPhase::Idle,
        "a clean end of an unmanaged session holds nothing for a bootstrap"
    );
}

/// #2120, why the hook of such a shell must not be reported as seen: with it the machine counts
/// the remote as managed, the ownership edge never comes under the wrapper, and a clean end of
/// the block waits for a bootstrap. Only a give-up ended that hold.
#[test]
fn a_hook_reported_as_seen_keeps_the_wrapper_hold_and_waits_after_the_block() {
    let mut m = owned();
    m.apply(SshEdge::TtyReclaimed);
    m.apply(observed(|o| o.hook_seen = true));
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::BlockCompleted { clean: true });
    assert_eq!(m.phase(false), SshPhase::Holding, "the post-block wait");
}
