//! Tests of the edge-driven ssh phase (#1424, step 7): each edge moves the phase the way the
//! legacy derivation (the connect gate's `drops_keys`, `ssh_prompt_has_arrived`, the swallow
//! window) did, and nothing but an edge moves it.

use super::*;

fn machine() -> SshPhaseMachine {
    SshPhaseMachine::default()
}

fn armed() -> SshPhaseMachine {
    let mut m = machine();
    m.apply(SshEdge::Submitted { ssh: true });
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
fn submitting_ssh_holds_through_the_silent_connect() {
    // Nothing printed yet: the gate drops keys, the phase holds (legacy `ssh_connecting_silently`).
    let m = armed();
    assert!(m.armed());
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn a_full_screen_program_never_holds() {
    let m = armed();
    assert_eq!(m.phase(true), SshPhase::Idle);
}

#[test]
fn a_banner_opens_the_gate_and_the_phase_is_idle_until_a_key_is_held() {
    let mut m = armed();
    m.apply(observed(|o| o.visible_text = true));
    // Visible text, terminated: not a prompt, and the silent stretch is over.
    assert_eq!(m.phase(false), SshPhase::Idle);
    m.apply(SshEdge::HoldStarted);
    // Once anything is held every later key is held too (#1292).
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::HoldEnded);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn an_unterminated_line_is_a_prompt_with_the_gen_it_became_one() {
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.unterminated_since = Some(7);
    }));
    assert_eq!(m.phase(false), SshPhase::PromptUp { prompt_gen: 7 });
    // The answer was sent and the line ended: not a prompt any more.
    m.apply(observed(|o| o.visible_text = true));
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn visible_text_is_sticky_even_if_a_later_observation_does_not_repeat_it() {
    let mut m = armed();
    m.apply(observed(|o| o.visible_text = true));
    m.apply(observed(|_| {}));
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_managed_remote_is_never_a_password_prompt() {
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.hook_seen = true;
        o.unterminated_since = Some(3);
    }));
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn the_announced_session_holds_through_its_bootstrap() {
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.pending_session = true;
    }));
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
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.pending_session = true;
    }));
    assert_eq!(m.phase(false), SshPhase::Holding);
    m.apply(SshEdge::BootstrapFailed);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn the_swallow_give_up_ends_the_swallow() {
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.pending_session = true;
    }));
    m.apply(SshEdge::SwallowGiveUp);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_new_announcement_swallows_again() {
    let mut m = armed();
    m.apply(observed(|o| o.pending_session = true));
    m.apply(SshEdge::BootstrapFailed);
    m.apply(observed(|_| {}));
    m.apply(observed(|o| o.pending_session = true));
    assert_eq!(m.phase(false), SshPhase::Holding);
}

#[test]
fn bootstrapped_holds_until_the_first_prompt() {
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.pending_session = true;
    }));
    // Bootstrapped: the session is no longer pending. The remote bootstrap is finishing; keys
    // written now became remote typeahead the shell handed back in pieces (#1085).
    m.apply(observed(|o| o.visible_text = true));
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
    let mut m = armed();
    m.apply(observed(|o| {
        o.visible_text = true;
        o.hook_seen = true;
    }));
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
    let mut m = armed();
    m.apply(observed(|o| o.visible_text = true));
    m.apply(SshEdge::BlockCompleted { clean: true });
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn preexec_arms_a_window_nothing_armed() {
    let mut m = machine();
    m.apply(SshEdge::ConnectStart);
    assert!(m.armed());
    assert_eq!(m.phase(false), SshPhase::Holding);
    // A second start changes nothing.
    m.apply(observed(|o| o.visible_text = true));
    m.apply(SshEdge::ConnectStart);
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn a_submit_ends_the_previous_window_and_its_hold() {
    let mut m = armed();
    m.apply(SshEdge::HoldStarted);
    m.apply(observed(|o| {
        o.visible_text = true;
        o.unterminated_since = Some(4);
    }));
    m.apply(SshEdge::Submitted { ssh: false });
    assert!(!m.armed());
    assert!(!m.holding());
    assert_eq!(m.phase(false), SshPhase::Idle);
}

#[test]
fn every_give_up_is_named_and_only_the_timers_are_give_ups() {
    for how in [
        SshRelease::FirstPrompt,
        SshRelease::Failed,
        SshRelease::OtherCommand,
        SshRelease::ShellPrompt,
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
    let mut m = armed();
    m.apply(observed(|o| o.visible_text = true));
    let before = m.phase(false);
    // Reading the phase any number of times, with any alt-screen flag, changes nothing.
    let _ = m.phase(true);
    let _ = m.phase(false);
    assert_eq!(m.phase(false), before);
}
