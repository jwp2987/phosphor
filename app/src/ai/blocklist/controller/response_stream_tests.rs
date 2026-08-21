use super::{lrc_tag_in_lacks_confirmation_ui, recovery_action, RecoveryAction};

// Argument order: has_received_client_actions, is_recoverable, has_retry_budget,
// can_attempt_resume_on_error, is_online.

#[test]
fn pre_action_failures_retry() {
    assert_eq!(
        recovery_action(false, true, true, true, true),
        RecoveryAction::RetryNow
    );
    // Resume eligibility is irrelevant pre-actions.
    assert_eq!(
        recovery_action(false, true, true, false, true),
        RecoveryAction::RetryNow
    );
}

#[test]
fn pre_action_failures_wait_for_connectivity_when_offline() {
    assert_eq!(
        recovery_action(false, true, true, true, false),
        RecoveryAction::RetryWhenOnline
    );
}

#[test]
fn pre_action_budget_exhaustion_is_terminal() {
    // The request has already been retried MAX_RETRIES times; stop.
    assert_eq!(
        recovery_action(false, true, false, true, true),
        RecoveryAction::Fail
    );
    assert_eq!(
        recovery_action(false, true, false, true, false),
        RecoveryAction::Fail
    );
}

#[test]
fn non_recoverable_pre_action_failure_is_terminal() {
    assert_eq!(
        recovery_action(false, false, true, true, true),
        RecoveryAction::Fail
    );
}

#[test]
fn post_action_recoverable_failures_resume() {
    assert_eq!(
        recovery_action(true, true, true, true, true),
        RecoveryAction::Resume
    );
    // Offline doesn't change the decision; the resume spawn waits for connectivity.
    assert_eq!(
        recovery_action(true, true, true, true, false),
        RecoveryAction::Resume
    );
    // The in-request retry budget is irrelevant once actions have executed.
    assert_eq!(
        recovery_action(true, true, false, true, true),
        RecoveryAction::Resume
    );
}

#[test]
fn post_action_failures_without_resume_eligibility_are_terminal() {
    // Resume requests themselves run with can_attempt_resume_on_error=false,
    // bounding recovery to a single resume.
    assert_eq!(
        recovery_action(true, true, true, false, true),
        RecoveryAction::Fail
    );
}

#[test]
fn non_recoverable_post_action_failure_is_terminal() {
    // A non-recoverable error (e.g. a client error) ends the conversation even
    // after actions have executed.
    assert_eq!(
        recovery_action(true, false, true, true, true),
        RecoveryAction::Fail
    );
}

/// #617: the tag-in auto-accept path used to forge `is_user_initiated=true`, which
/// discarded the execution profile's verdict -- an `Always ask` profile had its
/// `can_auto_execute=false` computed and then ignored. The bool is now an
/// `ActionInitiator`, and the tag-in path passes `AutoAcceptedTagIn`, which withholds
/// stand-in authority for `UseComputer`/`RequestComputerUse`. That override exists for
/// the deadlock where the tagged-in command holds the alt screen and the Accept button
/// is therefore never rendered, so there is no way for the user to answer.
///
/// The bug was gating it on the tag-in alone, which applied the override to ordinary
/// sessions whose Accept button was perfectly visible. Observed against a live PROD ssh
/// session with `is_alt_screen=false` in the logs.
///
/// The `(true, Some(false))` case is the whole point of this test: the old predicate was
/// `lrc_should_spawn_subagent` on its own, which differs from the correct one on that arm
/// and only that arm. Reverting the fix must turn this red -- if it stays green, the test
/// is vacuous and pins nothing.
#[test]
fn tag_in_without_alt_screen_keeps_the_confirmation_prompt() {
    // Tag-in, but the user can see and click Accept: the profile must be honoured.
    assert!(
        !lrc_tag_in_lacks_confirmation_ui(true, Some(false)),
        "a non-alt-screen tag-in has a visible Accept button and must not auto-accept (#617)"
    );

    // Tag-in with no running command at all: nothing is holding the screen.
    assert!(
        !lrc_tag_in_lacks_confirmation_ui(true, None),
        "no running command means no deadlock, so no override is warranted"
    );

    // Not a tag-in round: the auto-accept path must never apply.
    assert!(!lrc_tag_in_lacks_confirmation_ui(false, Some(true)));
    assert!(!lrc_tag_in_lacks_confirmation_ui(false, Some(false)));

    // The one case the override is for: tag-in whose command owns the alt screen, so
    // the Accept button is not rendered and a prompt would deadlock the turn.
    assert!(
        lrc_tag_in_lacks_confirmation_ui(true, Some(true)),
        "an alt-screen tag-in has no Accept button to answer; this is the deadlock case"
    );
}
