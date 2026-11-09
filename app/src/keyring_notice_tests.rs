use warpui_extras::secure_storage::availability::Availability;

use super::NoticeGate;

/// #1805: one outage is one notice, however many times it is reported (the current state at
/// startup and the transition event that produced it, or repeated failures).
#[test]
fn one_outage_shows_one_notice() {
    let mut gate = NoticeGate::default();
    assert!(!gate.take(), "nothing to show while the keyring is fine");

    gate.observe(Availability::Unavailable);
    gate.observe(Availability::Unavailable);
    assert!(gate.take());
    assert!(!gate.take(), "shown once");

    // The same outage reported again (the queued transition event after the startup read).
    gate.observe(Availability::Unavailable);
    assert!(!gate.take(), "a repeat of the shown outage is not news");
}

/// The notice waits for a window: an outage seen before any window exists is still shown once
/// one does.
#[test]
fn an_outage_before_any_window_is_shown_later() {
    let mut gate = NoticeGate::default();
    gate.observe(Availability::Unavailable);
    // No `take` yet (no window); more failures in the meantime change nothing.
    gate.observe(Availability::Unavailable);
    assert!(gate.take());
    assert!(!gate.take());
}

/// After a user retry (or the keyring coming back) a new outage is news again.
#[test]
fn a_new_outage_after_a_retry_is_shown_again() {
    let mut gate = NoticeGate::default();
    gate.observe(Availability::Unavailable);
    assert!(gate.take());

    gate.observe(Availability::Unknown);
    assert!(!gate.take());
    gate.observe(Availability::Unavailable);
    assert!(gate.take(), "the retry failed: tell the user");

    gate.observe(Availability::Available);
    assert!(!gate.take(), "nothing pending once the keyring answers");
}
