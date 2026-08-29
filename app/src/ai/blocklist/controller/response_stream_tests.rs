#[cfg(not(target_family = "wasm"))]
use std::time::Duration;

use super::{
    FailReason, MAX_RECOVERY_ATTEMPTS, RecoveryAction, RecoveryBudget, ResponseStream,
    ResponseStreamId, backoff_after_attempts, lrc_tag_in_lacks_confirmation_ui, recovery_action,
};
// `agent_sdk` (and so the driver's recovery deadline) is native-only here.
#[cfg(not(target_family = "wasm"))]
use crate::ai::agent_sdk::driver::AUTO_RESUME_TIMEOUT;

// Argument order: has_received_client_actions, is_recoverable, recovery, is_online.

/// A budget with every recovery attempt spent.
fn exhausted() -> RecoveryBudget {
    let mut recovery = RecoveryBudget::fresh();
    for _ in 0..MAX_RECOVERY_ATTEMPTS {
        recovery = recovery.next_attempt();
    }
    recovery
}

/// Ported from the pin's `pre_action_failures_retry` (upstream 4111d08f9).
#[test]
fn pre_action_failures_retry() {
    assert_eq!(
        recovery_action(false, true, RecoveryBudget::fresh(), true),
        RecoveryAction::Retry
    );
    // Resume eligibility is irrelevant pre-actions.
    assert_eq!(
        recovery_action(false, true, RecoveryBudget::fresh().without_resume(), true),
        RecoveryAction::Retry
    );
}

/// Ported from the pin's `pre_action_failures_wait_for_connectivity_when_offline`
/// (upstream 4111d08f9).
#[test]
fn pre_action_failures_wait_for_connectivity_when_offline() {
    assert_eq!(
        recovery_action(false, true, RecoveryBudget::fresh(), false),
        RecoveryAction::RetryWhenOnline
    );
}

/// Ported from the pin's `pre_action_budget_exhaustion_is_terminal` (upstream 4111d08f9).
#[test]
fn pre_action_budget_exhaustion_is_terminal() {
    // The turn has already spent MAX_RECOVERY_ATTEMPTS attempts; stop.
    assert_eq!(
        recovery_action(false, true, exhausted(), true),
        RecoveryAction::Fail(FailReason::BudgetExhausted)
    );
    assert_eq!(
        recovery_action(false, true, exhausted(), false),
        RecoveryAction::Fail(FailReason::BudgetExhausted)
    );
}

/// Ported from the pin's `non_recoverable_pre_action_failure_is_terminal`
/// (upstream 4111d08f9).
#[test]
fn non_recoverable_pre_action_failure_is_terminal() {
    assert_eq!(
        recovery_action(false, false, RecoveryBudget::fresh(), true),
        RecoveryAction::Fail(FailReason::NotRecoverable)
    );
}

/// Ported from the pin's `post_action_recoverable_failures_resume` (upstream 4111d08f9).
#[test]
fn post_action_recoverable_failures_resume() {
    assert_eq!(
        recovery_action(true, true, RecoveryBudget::fresh(), true),
        RecoveryAction::Resume
    );
    // Offline doesn't change the decision; the resume spawn waits for connectivity.
    assert_eq!(
        recovery_action(true, true, RecoveryBudget::fresh(), false),
        RecoveryAction::Resume
    );
}

/// Ported from the pin's `post_action_failures_without_resume_eligibility_are_terminal`
/// (upstream 4111d08f9).
#[test]
fn post_action_failures_without_resume_eligibility_are_terminal() {
    // Passive background requests may not resume, and a post-action failure has no other
    // recovery available.
    assert_eq!(
        recovery_action(true, true, RecoveryBudget::fresh().without_resume(), true),
        RecoveryAction::Fail(FailReason::ResumeNotAllowed)
    );
}

/// Ported from the pin's `ineligibility_is_reported_ahead_of_an_exhausted_budget`
/// (upstream 4111d08f9).
#[test]
fn ineligibility_is_reported_ahead_of_an_exhausted_budget() {
    // A passive request that spent its budget on pre-action retries and then fails after
    // actions is blocked by both constraints. The reason logged should be the one that would
    // still block it if the budget were full, since that is what a reader needs to know.
    let exhausted_passive = exhausted().without_resume();
    assert_eq!(
        recovery_action(true, true, exhausted_passive, true),
        RecoveryAction::Fail(FailReason::ResumeNotAllowed)
    );
    // Pre-action, the budget is the only thing standing in the way, so it is the reason.
    assert_eq!(
        recovery_action(false, true, exhausted_passive, true),
        RecoveryAction::Fail(FailReason::BudgetExhausted)
    );
}

/// Ported from the pin's `a_scheduled_resume_inherits_a_charged_budget`
/// (upstream 4111d08f9).
#[test]
fn a_scheduled_resume_inherits_a_charged_budget() {
    // The boundary the controller consumes: whatever budget a failed request was running
    // with, the resume it schedules must run with that budget charged one attempt, and with
    // resume eligibility carried over unchanged. Getting this wrong in either direction is
    // the bug REMOTE-2269 was about -- a fresh budget would restart recovery from scratch,
    // and a lost `resume_allowed` would silently re-enable resumes for a passive request.
    let stream = ResponseStream::new_for_test(ResponseStreamId::new_for_test());
    let before = RecoveryBudget::fresh().without_resume();
    let after = stream.recovery.next_attempt();

    assert_eq!(stream.recovery, before, "test fixture drifted");
    assert_eq!(after.attempts_used(), before.attempts_used() + 1);
    // Eligibility is preserved, so the resumed request is still bound by the same rule.
    assert_eq!(
        recovery_action(true, true, after, true),
        RecoveryAction::Fail(FailReason::ResumeNotAllowed)
    );
}

/// Ported from the pin's `non_recoverable_post_action_failure_is_terminal`
/// (upstream 4111d08f9).
#[test]
fn non_recoverable_post_action_failure_is_terminal() {
    // A non-recoverable error (e.g. a client error) ends the conversation even
    // after actions have executed.
    assert_eq!(
        recovery_action(true, false, RecoveryBudget::fresh(), true),
        RecoveryAction::Fail(FailReason::NotRecoverable)
    );
}

/// Ported from the pin's `resume_failures_consume_the_shared_budget` (upstream 4111d08f9).
#[test]
fn resume_failures_consume_the_shared_budget() {
    // Every resume is charged against the budget, so a conversation that keeps hitting
    // transport resets after client actions gets MAX_RECOVERY_ATTEMPTS resumes and then
    // fails -- where it used to get exactly one, because the resumed request ran with
    // resumes disabled rather than with the remaining budget.
    let mut recovery = RecoveryBudget::fresh();
    let mut actions = Vec::new();
    for _ in 0..MAX_RECOVERY_ATTEMPTS + 1 {
        let action = recovery_action(
            /*has_received_client_actions*/ true, true, recovery, true,
        );
        actions.push(action);
        if action == RecoveryAction::Resume {
            recovery = recovery.next_attempt();
        }
    }

    let (resumes, terminal) = actions.split_at(MAX_RECOVERY_ATTEMPTS);
    assert!(
        resumes
            .iter()
            .all(|action| *action == RecoveryAction::Resume)
    );
    assert_eq!(
        terminal,
        [RecoveryAction::Fail(FailReason::BudgetExhausted)]
    );
    assert_eq!(recovery.attempts_used(), MAX_RECOVERY_ATTEMPTS);
}

/// Ported from the pin's `retries_and_resumes_share_one_budget` (upstream 4111d08f9).
#[test]
fn retries_and_resumes_share_one_budget() {
    // The failure mode from REMOTE-2269: a pre-action failure retries, the retry then
    // fails after client actions have streamed (which is when the pre-action path is no
    // longer available), and recovery switches to resumes. Both kinds draw from the same
    // counter, so the chain is bounded by MAX_RECOVERY_ATTEMPTS sends in total rather than
    // by a per-kind allowance.
    let mut recovery = RecoveryBudget::fresh();

    assert_eq!(
        recovery_action(false, true, recovery, true),
        RecoveryAction::Retry
    );
    recovery = recovery.next_attempt();

    assert_eq!(
        recovery_action(true, true, recovery, true),
        RecoveryAction::Resume
    );
    recovery = recovery.next_attempt();

    assert_eq!(
        recovery_action(true, true, recovery, true),
        RecoveryAction::Resume
    );
    recovery = recovery.next_attempt();

    // Three attempts spent: the next failure is terminal whichever kind of recovery it
    // would have used, because the counter is shared and not per-kind.
    assert_eq!(
        recovery_action(true, true, recovery, true),
        RecoveryAction::Fail(FailReason::BudgetExhausted)
    );
    assert_eq!(
        recovery_action(false, true, recovery, true),
        RecoveryAction::Fail(FailReason::BudgetExhausted)
    );
}

/// Ported from the pin's `spending_an_attempt_preserves_resume_eligibility`
/// (upstream 4111d08f9).
#[test]
fn spending_an_attempt_preserves_resume_eligibility() {
    // Charging an attempt must not quietly re-enable resumes for a passive request, nor
    // disable them for a normal one.
    let passive = RecoveryBudget::fresh().without_resume().next_attempt();
    assert_eq!(
        recovery_action(true, true, passive, true),
        RecoveryAction::Fail(FailReason::ResumeNotAllowed)
    );

    let normal = RecoveryBudget::fresh().next_attempt();
    assert_eq!(
        recovery_action(true, true, normal, true),
        RecoveryAction::Resume
    );
}

/// Ported from the pin's `the_recovery_backoff_fits_inside_the_cloud_run_recovery_window`
/// (upstream 4111d08f9).
///
/// **Renamed** (`cloud_run` -> `driver_run`): the window is
/// [`AUTO_RESUME_TIMEOUT`], which upstream and this fork both define in
/// `ai::agent_sdk::driver` -- the headless CLI driver, which runs locally here. Nothing in
/// the assertion is cloud-bound; it relates two local constants, and both still exist and
/// still mean the same thing. `attempts_used` is not consulted, so this is a pure
/// arithmetic property of the schedule.
#[cfg(not(target_family = "wasm"))]
#[test]
fn the_recovery_backoff_fits_inside_the_driver_run_recovery_window() {
    // The driver re-arms AUTO_RESUME_TIMEOUT per recovery attempt, so what has to fit in
    // that window is a single attempt's wait, not the whole chain. Assert both anyway: if
    // the budget or the backoff schedule ever grows enough to approach the deadline, a run
    // would start dying on the deadline instead of on the failure it was recovering from.
    let total: Duration = (1..=MAX_RECOVERY_ATTEMPTS)
        .map(backoff_after_attempts)
        .sum();
    assert!(
        total * 2 < AUTO_RESUME_TIMEOUT,
        "total recovery backoff {total:?} is too close to AUTO_RESUME_TIMEOUT {AUTO_RESUME_TIMEOUT:?}"
    );
    for attempt in 1..=MAX_RECOVERY_ATTEMPTS {
        assert!(backoff_after_attempts(attempt) * 4 < AUTO_RESUME_TIMEOUT);
    }
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
