use warpui_extras::secure_storage::availability::Availability;

use super::NoticeGate;

/// #1805: one outage is one notice, however many times it is reported (the current state at
/// startup and the transition event that produced it, or repeated failures).
#[test]
fn one_outage_shows_one_notice() {
    let mut gate = NoticeGate::default();
    assert!(!gate.due(), "nothing to show while the keyring is fine");

    gate.observe(Availability::Unavailable);
    gate.observe(Availability::Unavailable);
    assert!(gate.due());
    assert!(gate.confirm_shown());
    assert!(!gate.due(), "shown once");
    assert!(
        !gate.confirm_shown(),
        "a second workspace showing it is not a second notice"
    );

    // The same outage reported again (the queued transition event after the startup read).
    gate.observe(Availability::Unavailable);
    assert!(!gate.due(), "a repeat of the shown outage is not news");
}

/// #1805 review: a request is not a notice. Until a workspace confirms it displayed the toast
/// (an outage seen before any window, or a window with no workspace), the notice stays due.
#[test]
fn the_notice_stays_due_until_a_workspace_displayed_it() {
    let mut gate = NoticeGate::default();
    gate.observe(Availability::Unavailable);
    // Asked a window to show it; nothing confirmed yet.
    assert!(gate.due());
    gate.observe(Availability::Unavailable);
    assert!(gate.due(), "still owed to the user");
    assert!(gate.confirm_shown());
    assert!(!gate.due());
}

/// After a user retry (or the keyring coming back) a new outage is news again, and a notice on
/// screen is reported stale so it can be dismissed at once.
#[test]
fn recovery_dismisses_the_notice_and_a_new_outage_is_shown_again() {
    let mut gate = NoticeGate::default();
    gate.observe(Availability::Unavailable);
    assert!(gate.confirm_shown());

    assert!(
        gate.observe(Availability::Available),
        "a shown notice is stale once the keyring answers"
    );
    assert!(!gate.due());
    assert!(
        !gate.observe(Availability::Available),
        "nothing on screen to dismiss twice"
    );

    gate.observe(Availability::Unavailable);
    assert!(gate.due(), "a later outage is news again");
    assert!(gate.confirm_shown());
    assert!(
        gate.observe(Availability::Unknown),
        "a retry also retires it"
    );

    // An outage that was never shown has nothing to dismiss, and a late confirmation of it
    // (the toast landed after the keyring came back) is ignored.
    gate.observe(Availability::Unavailable);
    assert!(!gate.observe(Availability::Available));
    assert!(!gate.confirm_shown());
}
